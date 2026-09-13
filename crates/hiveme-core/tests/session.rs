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

//! The shared session of `docs/specs/session.md`, against a real broker.
//!
//! A HiveMQ CE container stands in for the cloud, as in `tests/mqtt.rs`. Its allow-all
//! extension accepts any credentials, so the config carries a username and a password
//! anyway: the session validates what it saves and connects with.
//!
//! Every test here needs Docker. Without it, or with `HIVEME_SKIP_DOCKER=1`, each test
//! says why it did nothing and passes, so that the macOS and Windows workflows stay
//! green; the Linux workflow has Docker and does run them.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hiveme_core::config::{Config, ConfigFile, Theme};
use hiveme_core::message::{Level, Message, Sender};
use hiveme_core::mqtt::{IncomingMessage, MqttClient, Qos, Role};
use hiveme_core::session::{PublishOptions, SHUTDOWN_TIMEOUT, Session, SessionApp, SessionEvent, Toaster};
use rumqttc::v5::mqttbytes::v5::Packet;
use rumqttc::v5::{AsyncClient, Event, MqttOptions};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::broadcast::Receiver;

const BROKER_IMAGE: &str = "hivemq/hivemq-ce";
const BROKER_TAG: &str = "latest";
const BROKER_PORT: u16 = 1883;

/// The line HiveMQ CE prints once its MQTT listener is up.
const BROKER_READY: &str = "Started HiveMQ";

/// Set to 1 to skip every test in this file. The macOS and Windows workflows do.
const SKIP_VARIABLE: &str = "HIVEME_SKIP_DOCKER";

/// How long a test waits for something that should already be on its way.
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(15);

/// Runs `body` against a freshly started broker, or explains why it did not.
fn with_broker<F, Fut>(name: &str, body: F)
where
  F: FnOnce(Broker) -> Fut,
  Fut: std::future::Future<Output = ()>,
{
  if let Ok("1") = std::env::var(SKIP_VARIABLE).as_deref() {
    eprintln!("skipping {name}: {SKIP_VARIABLE}=1");
    return;
  }
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("a tokio runtime");
  runtime.block_on(async move {
    let broker = match Broker::start().await {
      Ok(broker) => broker,
      Err(reason) => {
        eprintln!("skipping {name}: the HiveMQ CE container did not start ({reason})");
        return;
      }
    };
    body(broker).await;
  });
}

/// A running broker and the address it answers on.
struct Broker {
  _container: ContainerAsync<GenericImage>,
  host: String,
  port: u16,
  directory: tempfile::TempDir,
}

impl Broker {
  async fn start() -> Result<Self, String> {
    let container = GenericImage::new(BROKER_IMAGE, BROKER_TAG)
      .with_exposed_port(BROKER_PORT.tcp())
      .with_wait_for(WaitFor::message_on_stdout(BROKER_READY))
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
      _container: container,
      host,
      port,
      directory: tempfile::tempdir().map_err(|error| error.to_string())?,
    })
  }

  /// A config pointing at this test's broker, for a device of its own.
  fn config(&self, device: &str) -> Config {
    let mut config = Config::default();
    config.device.id = device.to_owned();
    config.device.name = device.to_owned();
    config.broker.url = format!("mqtt://{}:{}", self.host, self.port);
    config.broker.username = "hiveme".to_owned();
    config.broker.password = "test".to_owned();
    config.broker.keep_alive_secs = 5;
    config.broker.connect_timeout_secs = 10;
    config.broker.reconnect.initial_delay_ms = 200;
    config.broker.reconnect.max_delay_ms = 2_000;
    config.publish.timeout_secs = 20;
    config
  }

  /// A session of `app` on a config file of its own, written before it opens.
  fn session(&self, app: SessionApp, device: &str, toaster: Arc<Recorder>) -> Arc<Session> {
    let path = self.directory.path().join(device).join("HiveMe.json");
    write_config(&path, self.config(device));
    Session::open(Some(&path), app, toaster).expect("the session opens")
  }

  /// A plain client, as another installation would connect.
  async fn client(&self, device: &str, role: Role) -> MqttClient {
    MqttClient::connect(&self.config(device), role)
      .await
      .unwrap_or_else(|error| panic!("cannot connect as {role}: {error}"))
  }
}

fn write_config(path: &Path, config: Config) {
  let (mut file, _) = ConfigFile::load_or_create(path).expect("the config file is created");
  file.set_config(config);
  file.save().expect("the config file is written");
}

/// A toaster that records what it was asked to show.
#[derive(Default)]
struct Recorder {
  shown: Mutex<Vec<(String, String)>>,
}

impl Recorder {
  fn shown(&self) -> Vec<(String, String)> {
    self.shown.lock().unwrap().clone()
  }
}

impl Toaster for Recorder {
  fn show(&self, title: &str, body: &str) -> Result<(), String> {
    self.shown.lock().unwrap().push((title.to_owned(), body.to_owned()));
    Ok(())
  }
}

/// Waits for the first event `wanted` accepts, failing the test rather than hanging.
async fn next_event<T>(events: &mut Receiver<SessionEvent>, mut wanted: impl FnMut(SessionEvent) -> Option<T>) -> T {
  tokio::time::timeout(RECEIVE_TIMEOUT, async {
    loop {
      let event = events.recv().await.expect("the session is still raising events");
      if let Some(found) = wanted(event) {
        return found;
      }
    }
  })
  .await
  .expect("the event arrives within the timeout")
}

/// Every event that arrives within `window`.
async fn events_within(events: &mut Receiver<SessionEvent>, window: Duration) -> Vec<SessionEvent> {
  let mut seen = Vec::new();
  let deadline = tokio::time::Instant::now() + window;
  while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, events.recv()).await {
    seen.push(event);
  }
  seen
}

async fn next_message(incoming: &mut tokio::sync::mpsc::Receiver<IncomingMessage>) -> IncomingMessage {
  tokio::time::timeout(RECEIVE_TIMEOUT, incoming.recv())
    .await
    .expect("a message arrives within the timeout")
    .expect("the incoming channel is still open")
}

/// Asks the broker whether `client_id` still has a session.
async fn session_present(broker: &Broker, client_id: &str) -> bool {
  let mut options = MqttOptions::new(client_id, &broker.host, broker.port);
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
  let mut options = MqttOptions::new(client_id, &broker.host, broker.port);
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

#[test]
fn connecting_subscribes_and_raises_the_status() {
  with_broker("connecting_subscribes_and_raises_the_status", |broker| async move {
    let session = broker.session(SessionApp::Tui, "status", Arc::default());
    let mut events = session.subscribe();

    let status = session.connect().await.expect("the session connects");

    assert_eq!(status.state, "Connected");
    assert_eq!(status.subscriptions, 1);
    assert!(
      status.client_id.starts_with("hiveme-hmc-status-"),
      "{}",
      status.client_id
    );
    let raised = next_event(&mut events, |event| match event {
      SessionEvent::Status(status) if status.state == "Connected" => Some(status),
      _ => None,
    })
    .await;
    assert_eq!(raised.subscriptions, 1);
    assert_eq!(session.status().state, "Connected");

    session.shutdown().await.expect("the session ends");
  });
}

#[test]
fn a_published_envelope_is_stored_raised_once_and_its_echo_is_recognized() {
  with_broker(
    "a_published_envelope_is_stored_raised_once_and_its_echo_is_recognized",
    |broker| async move {
      let session = broker.session(SessionApp::Gui, "composer", Arc::default());
      session.connect().await.expect("the session connects");
      let mut events = session.subscribe();

      let row = session
        .publish(
          "hiveme",
          "Deployed to staging",
          PublishOptions {
            topic: Some("/deploy".to_owned()),
            title: Some("Deploy".to_owned()),
            level: Some("success".to_owned()),
            ..PublishOptions::default()
          },
        )
        .await
        .expect("the broker acknowledges the publish");

      assert_eq!(row.topic, "hiveme/deploy");
      assert!(row.outgoing);
      assert_eq!(row.app.as_deref(), Some("hmg"));
      assert_eq!(row.level.as_deref(), Some("success"));
      assert_eq!(row.title.as_deref(), Some("Deploy"));

      // The echo comes back on the session's own subscription. It collapses into the
      // row that is there and raises nothing, so the only row event is the publish.
      let seen = events_within(&mut events, Duration::from_secs(2)).await;
      let rows: Vec<_> = seen
        .iter()
        .filter_map(|event| match event {
          SessionEvent::Message(message) => Some(message.row_id),
          _ => None,
        })
        .collect();
      assert_eq!(rows, vec![row.row_id], "{seen:?}");
      assert!(seen.contains(&SessionEvent::TopicAdded {
        topic: "hiveme/deploy".to_owned()
      }));

      let stored = session.messages("hiveme", None, 0).expect("the history is readable");
      assert_eq!(stored.len(), 1);
      assert_eq!(stored[0].id, row.id);
      let tree = session.topic_tree().expect("the tree is readable");
      assert_eq!(tree[0].unread, 0, "what this installation sent is never unread");
      assert!(session.status().messages_received >= 1, "the echo was received");

      session.shutdown().await.expect("the session ends");
    },
  );
}

#[test]
fn a_raw_json_publish_claims_no_envelope() {
  with_broker("a_raw_json_publish_claims_no_envelope", |broker| async move {
    let mut subscriber = broker.client("raw-reader", Role::Gui).await;
    let mut incoming = subscriber.take_incoming().expect("the incoming channel");
    subscriber
      .subscribe(["hiveme/#"], Qos::AtLeastOnce)
      .await
      .expect("the broker accepts the subscription");

    let session = broker.session(SessionApp::Tui, "raw-writer", Arc::default());
    session.connect().await.expect("the session connects");
    let body = r#"{"build":482,"ok":true}"#;
    let row = session
      .publish(
        "hiveme/ci",
        body,
        PublishOptions {
          json: true,
          ..PublishOptions::default()
        },
      )
      .await
      .expect("the broker acknowledges the publish");
    assert_eq!(row.tier, "json");
    assert_eq!(row.raw, body);

    let received = next_message(&mut incoming).await;
    assert_eq!(received.topic, "hiveme/ci");
    assert_eq!(received.payload, body.as_bytes());
    assert_eq!(received.properties.content_type.as_deref(), Some("application/json"));
    assert_eq!(received.properties.hiveme_version(), None);

    session.shutdown().await.expect("the session ends");
    subscriber.disconnect().await.expect("the subscriber says goodbye");
  });
}

#[test]
fn a_saved_theme_keeps_the_connection_and_a_saved_password_replaces_it() {
  with_broker(
    "a_saved_theme_keeps_the_connection_and_a_saved_password_replaces_it",
    |broker| async move {
      // The terminal UI takes a fresh client identifier for every connection, so a
      // changed identifier is a replaced connection.
      let session = broker.session(SessionApp::Tui, "settings", Arc::default());
      let first = session.connect().await.expect("the session connects").client_id;

      let mut config = session.config();
      config.gui.theme = Theme::Rose;
      let saved = session.set_config(config).await.expect("the theme is saved");
      assert_eq!(saved.gui.theme, Theme::Rose);
      assert_eq!(session.status().client_id, first, "a theme keeps the connection");

      let mut config = session.config();
      config.broker.password = "changed".to_owned();
      session.set_config(config).await.expect("the password is saved");
      let status = session.status();
      assert_eq!(status.state, "Connected");
      assert_ne!(status.client_id, first, "a password replaces the connection");

      session.shutdown().await.expect("the session ends");
    },
  );
}

#[test]
fn the_pause_toggle_keeps_the_toaster_quiet_and_the_message_arriving() {
  with_broker(
    "the_pause_toggle_keeps_the_toaster_quiet_and_the_message_arriving",
    |broker| async move {
      let recorder = Arc::new(Recorder::default());
      let session = broker.session(SessionApp::Gui, "watcher", recorder.clone());
      session.connect().await.expect("the session connects");
      let mut events = session.subscribe();
      let publisher = broker.client("elsewhere", Role::Cli).await;
      let config = broker.config("elsewhere");
      let error = |body: &str| {
        Message::new_text(Sender::from_device(&config.device, "hmc"), body)
          .with_title("Disk")
          .with_level(Level::Error)
      };

      session.set_notifications_paused(true);
      publisher
        .publish_message("hiveme/disk", &error("Disk full"), Qos::AtLeastOnce, false)
        .await
        .expect("the broker acknowledges the publish");
      let paused = next_event(&mut events, |event| match event {
        SessionEvent::Message(row) => Some(row),
        _ => None,
      })
      .await;
      assert_eq!(paused.body, "Disk full", "a paused session still stores and shows");
      assert!(!paused.outgoing);
      assert!(recorder.shown().is_empty(), "a paused session raises no toast");

      session.set_notifications_paused(false);
      let message = error("Disk still full");
      publisher
        .publish_message("hiveme/disk", &message, Qos::AtLeastOnce, false)
        .await
        .expect("the broker acknowledges the publish");
      let fired = next_event(&mut events, |event| match event {
        SessionEvent::NotificationFired {
          rule_id,
          message_id,
          topic,
        } => Some((rule_id, message_id, topic)),
        _ => None,
      })
      .await;
      assert_eq!(
        fired,
        ("error".to_owned(), message.id.clone(), "hiveme/disk".to_owned())
      );
      assert_eq!(
        recorder.shown(),
        vec![("Disk".to_owned(), "Disk still full".to_owned())]
      );

      publisher.disconnect().await.expect("the publisher says goodbye");
      session.shutdown().await.expect("the session ends");
    },
  );
}

#[test]
fn shutdown_ends_the_broker_session_within_the_bound() {
  with_broker(
    "shutdown_ends_the_broker_session_within_the_bound",
    |broker| async move {
      let session = broker.session(SessionApp::Tui, "quitter", Arc::default());
      let client_id = session.connect().await.expect("the session connects").client_id;

      let started = Instant::now();
      session.begin_shutdown();
      tokio::time::timeout(SHUTDOWN_TIMEOUT, session.shutdown())
        .await
        .expect("shutdown finishes within its bound")
        .expect("the session ends cleanly");
      assert!(started.elapsed() < SHUTDOWN_TIMEOUT);

      assert!(
        !session_present(&broker, &client_id).await,
        "quitting discards the broker session"
      );
      let refused = session.connect().await.unwrap_err();
      assert_eq!(refused.to_string(), "the application is quitting");
      let refused = session.publish("hiveme", "late", PublishOptions::default()).await;
      assert!(refused.is_err(), "nothing is published behind a quit");
    },
  );
}
