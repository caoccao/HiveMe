/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

//! Shared isolated Docker broker and notification recorder for the test suites.
use crate::config::Config;
use rumqttc::v5::mqttbytes::v5::Packet;
use rumqttc::v5::{AsyncClient, Event, MqttOptions};
use std::time::Duration;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

pub const BROKER_IMAGE: &str = "hivemq/hivemq-ce";
pub const BROKER_TAG: &str = "latest";
/// The MQTT port of HiveMQ CE, which its image exposes itself. Nothing here exposes it
/// again: two entries for one container port make Docker publish it twice, and the second
/// bind fails with `address already in use` on Docker Desktop.
pub const BROKER_PORT: u16 = 1883;

/// The line HiveMQ CE prints once its MQTT listener is up.
pub const BROKER_READY: &str = "Started HiveMQ";

/// Set to 1 to skip every test in this file. The macOS and Windows workflows do.
pub const SKIP_VARIABLE: &str = "HIVEME_SKIP_DOCKER";

/// How long a test waits for a message that should already be on its way.
pub const RECEIVE_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a test waits for the client to work its way back to the broker.
pub const RECONNECT_TIMEOUT: Duration = Duration::from_secs(60);

/// Why this test did nothing, when it did nothing.
pub fn skip_reason() -> Option<String> {
  match std::env::var(SKIP_VARIABLE).as_deref() {
    Ok("1") => Some(format!("{SKIP_VARIABLE}=1")),
    _ => None,
  }
}

/// A runtime for one test, since each test owns its broker.
pub fn runtime() -> tokio::runtime::Runtime {
  tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("a tokio runtime")
}

/// Runs `body` against a freshly started broker, or explains why it did not.
pub fn with_broker<F, Fut>(name: &str, body: F)
where
  F: FnOnce(Broker) -> Fut,
  Fut: std::future::Future<Output = ()>,
{
  with_broker_on(name, None, body);
}

/// As [`with_broker`], with the broker published on a host port a test picked.
///
/// A fixed port is what lets a test replace the broker underneath a running client,
/// which is the only way to see a reconnect land on the same address.
pub fn with_broker_on<F, Fut>(name: &str, host_port: Option<u16>, body: F)
where
  F: FnOnce(Broker) -> Fut,
  Fut: std::future::Future<Output = ()>,
{
  if let Some(reason) = skip_reason() {
    eprintln!("skipping {name}: {reason}");
    return;
  }
  runtime().block_on(async move {
    let broker = match Broker::start(host_port).await {
      Ok(broker) => broker,
      Err(reason) => {
        eprintln!("skipping {name}: the HiveMQ CE container did not start ({reason})");
        return;
      }
    };
    body(broker).await;
  });
}

/// A host port nothing is listening on, for a broker that has to keep its address.
///
/// The port is released before the container claims it, so this is a guess rather than
/// a reservation; a test that loses the race reports a container that would not start
/// rather than a failure.
pub fn free_port() -> Option<u16> {
  std::net::TcpListener::bind(("127.0.0.1", 0))
    .ok()
    .and_then(|listener| listener.local_addr().ok())
    .map(|address| address.port())
}

/// A running broker and the address it answers on.
pub struct Broker {
  pub container: Option<ContainerAsync<GenericImage>>,
  pub host: String,
  pub port: u16,
  pub host_port: Option<u16>,
  pub directory: tempfile::TempDir,
}

impl Broker {
  pub async fn start(host_port: Option<u16>) -> Result<Self, String> {
    let image = GenericImage::new(BROKER_IMAGE, BROKER_TAG).with_wait_for(WaitFor::message_on_stdout(BROKER_READY));
    let request = match host_port {
      Some(port) => image.with_mapped_port(port, BROKER_PORT.tcp()),
      None => image.into(),
    };
    let container = request
      .with_startup_timeout(Duration::from_secs(180))
      .start()
      .await
      .map_err(|error| error.to_string())?;

    let host = container
      .get_host()
      .await
      .map_err(|error| error.to_string())?
      .to_string();
    let port = container
      .get_host_port_ipv4(BROKER_PORT.tcp())
      .await
      .map_err(|error| error.to_string())?;
    Ok(Self {
      container: Some(container),
      host,
      port,
      host_port,
      directory: tempfile::tempdir().map_err(|error| error.to_string())?,
    })
  }

  pub fn url(&self) -> String {
    format!("mqtt://{}:{}", self.host, self.port)
  }

  /// A config pointing at this test’s isolated broker.
  pub fn config(&self, device: &str) -> Config {
    let mut config = Config::default();
    config.device.id = device.to_owned();
    config.device.name = device.to_owned();
    config.broker.url = self.url();
    config.broker.username = "hiveme".to_owned();
    config.broker.password = "test".to_owned();
    config.broker.keep_alive_secs = 5;
    config.broker.connect_timeout_secs = 10;
    config.broker.reconnect.initial_delay_ms = 200;
    config.broker.reconnect.max_delay_ms = 2_000;
    config.publish.timeout_secs = 20;
    config
  }

  /// Throws this broker away and puts a brand new one on the same address.
  ///
  /// A replacement, not a restart, because the point is a broker that has never heard
  /// of the client: it answers the reconnect with no session, and the client has to
  /// send its subscriptions again.
  pub async fn replace(&mut self) -> Result<(), String> {
    let container = self.container.take().ok_or("the broker is already gone")?;
    container.rm().await.map_err(|error| error.to_string())?;
    let replacement = Self::start(self.host_port).await?;
    self.host = replacement.host.clone();
    self.port = replacement.port;
    self.container = replacement.container;
    Ok(())
  }
}

impl Broker {
  pub fn session(
    &self,
    app: crate::session::SessionApp,
    device: &str,
    toaster: std::sync::Arc<Recorder>,
  ) -> std::sync::Arc<crate::session::Session> {
    let path = self.directory.path().join(device).join("HiveMe.json");
    let (mut file, _) = crate::config::ConfigFile::load_or_create(&path).unwrap();
    file.save_config(self.config(device)).unwrap();
    crate::session::Session::open(Some(&path), app, toaster).unwrap()
  }
  pub async fn client(&self, device: &str, role: crate::mqtt::Role) -> crate::mqtt::MqttClient {
    crate::mqtt::MqttClient::connect(&self.config(device), role)
      .await
      .unwrap()
  }
}

#[derive(Default)]
pub struct Recorder {
  pub shown: std::sync::Mutex<Vec<(String, String)>>,
  pub fail: std::sync::atomic::AtomicBool,
}
impl crate::session::Toaster for Recorder {
  fn show(&self, title: &str, body: &str) -> Result<(), String> {
    if self.fail.load(std::sync::atomic::Ordering::Relaxed) {
      return Err("no notification daemon".to_owned());
    }
    self
      .shown
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .push((title.to_owned(), body.to_owned()));
    Ok(())
  }
}
pub async fn session_present(host: &str, port: u16, client_id: &str) -> bool {
  let mut options = MqttOptions::new(client_id, host, port);
  options.set_clean_start(false);
  options.set_session_expiry_interval(Some(3_600));
  let (client, mut eventloop) = AsyncClient::new(options, 1);
  let present = tokio::time::timeout(RECEIVE_TIMEOUT, async {
    let mut present = None;
    loop {
      match eventloop.poll().await.expect("the session probe connects") {
        Event::Incoming(Packet::ConnAck(ack)) => {
          present = Some(ack.session_present);
          client.disconnect().await.expect("the probe says goodbye");
        }
        Event::Outgoing(rumqttc::Outgoing::Disconnect) => return present.expect("the CONNACK arrived"),
        _ => {}
      }
    }
  })
  .await
  .expect("the session probe finishes");
  // The probe itself asked for a session; a clean one with no expiry discards it again.
  let mut options = MqttOptions::new(client_id, host, port);
  options.set_clean_start(true);
  options.set_session_expiry_interval(Some(0));
  let (client, mut eventloop) = AsyncClient::new(options, 1);
  tokio::time::timeout(RECEIVE_TIMEOUT, async {
    loop {
      match eventloop.poll().await.expect("the cleanup probe connects") {
        Event::Incoming(Packet::ConnAck(_)) => client.disconnect().await.expect("the probe says goodbye"),
        Event::Outgoing(rumqttc::Outgoing::Disconnect) => return,
        _ => {}
      }
    }
  })
  .await
  .expect("the cleanup probe finishes");
  present
}

impl Recorder {
  pub fn shown(&self) -> Vec<(String, String)> {
    self
      .shown
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clone()
  }
}
