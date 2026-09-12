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

//! The MQTT behaviour promised by `docs/specs/hivemq-cloud.md`, against a real broker.
//!
//! A HiveMQ CE container stands in for the cloud. It is the open source build of the
//! same broker and speaks MQTT 5 on 1883, without TLS and without credentials, which
//! covers everything except the handshake. The TLS path is exercised only by
//! `a_real_cloud_cluster_accepts_a_message`, which a developer opts into with the
//! environment variables named there, because a cluster is an account rather than
//! something CI can create.
//!
//! Every test here needs Docker. Without it, or with `HIVEME_SKIP_DOCKER=1`, each test
//! says why it did nothing and passes, so that the macOS and Windows workflows stay
//! green; the Linux workflow has Docker and does run them.

use std::time::Duration;

use hiveme_core::config::Config;
use hiveme_core::message::{Level, Message, Parsed, Sender};
use hiveme_core::mqtt::{IncomingMessage, MqttClient, Qos, Role, State};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::mpsc::Receiver;

const BROKER_IMAGE: &str = "hivemq/hivemq-ce";
const BROKER_TAG: &str = "latest";
const BROKER_PORT: u16 = 1883;

/// The line HiveMQ CE prints once its MQTT listener is up.
const BROKER_READY: &str = "Started HiveMQ";

/// Set to 1 to skip every test in this file. The macOS and Windows workflows do.
const SKIP_VARIABLE: &str = "HIVEME_SKIP_DOCKER";

/// How long a test waits for a message that should already be on its way.
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a test waits for the client to work its way back to the broker.
const RECONNECT_TIMEOUT: Duration = Duration::from_secs(60);

/// Why this test did nothing, when it did nothing.
fn skip_reason() -> Option<String> {
  match std::env::var(SKIP_VARIABLE).as_deref() {
    Ok("1") => Some(format!("{SKIP_VARIABLE}=1")),
    _ => None,
  }
}

/// A runtime for one test, since each test owns its broker.
fn runtime() -> tokio::runtime::Runtime {
  tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("a tokio runtime")
}

/// Runs `body` against a freshly started broker, or explains why it did not.
fn with_broker<F, Fut>(name: &str, body: F)
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
fn with_broker_on<F, Fut>(name: &str, host_port: Option<u16>, body: F)
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
fn free_port() -> Option<u16> {
  std::net::TcpListener::bind(("127.0.0.1", 0))
    .ok()
    .and_then(|listener| listener.local_addr().ok())
    .map(|address| address.port())
}

/// A running broker and the address it answers on.
struct Broker {
  container: Option<ContainerAsync<GenericImage>>,
  host: String,
  port: u16,
  host_port: Option<u16>,
}

impl Broker {
  async fn start(host_port: Option<u16>) -> Result<Self, String> {
    let image = GenericImage::new(BROKER_IMAGE, BROKER_TAG)
      .with_exposed_port(BROKER_PORT.tcp())
      .with_wait_for(WaitFor::message_on_stdout(BROKER_READY));
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
    })
  }

  fn url(&self) -> String {
    format!("mqtt://{}:{}", self.host, self.port)
  }

  /// A config pointing at this broker, in a topic namespace of its own.
  ///
  /// Every test gets its own prefix so that a retained message or a slow delivery in
  /// one cannot reach another.
  fn config(&self, device: &str) -> Config {
    let mut config = Config::default();
    config.device.id = device.to_owned();
    config.device.name = device.to_owned();
    config.broker.url = self.url();
    config.broker.keep_alive_secs = 5;
    config.broker.connect_timeout_secs = 10;
    config.broker.reconnect.initial_delay_ms = 200;
    config.broker.reconnect.max_delay_ms = 2_000;
    config.publish.timeout_secs = 20;
    config.topics.prefix = format!("hiveme/{device}");
    config
  }

  /// Throws this broker away and puts a brand new one on the same address.
  ///
  /// A replacement, not a restart, because the point is a broker that has never heard
  /// of the client: it answers the reconnect with no session, and the client has to
  /// send its subscriptions again.
  async fn replace(&mut self) -> Result<(), String> {
    let container = self.container.take().ok_or("the broker is already gone")?;
    container.rm().await.map_err(|error| error.to_string())?;
    let replacement = Self::start(self.host_port).await?;
    self.host = replacement.host.clone();
    self.port = replacement.port;
    self.container = replacement.container;
    Ok(())
  }
}

/// Connects a client, without [`Config::validate`] because the container is anonymous.
async fn connect(config: &Config, role: Role) -> MqttClient {
  MqttClient::connect_unvalidated(config, role)
    .await
    .unwrap_or_else(|error| panic!("cannot connect as {role}: {error}"))
}

/// Waits for the next message, failing the test rather than hanging the suite.
async fn next_message(incoming: &mut Receiver<IncomingMessage>) -> IncomingMessage {
  tokio::time::timeout(RECEIVE_TIMEOUT, incoming.recv())
    .await
    .expect("a message arrives within the timeout")
    .expect("the incoming channel is still open")
}

/// Waits for the connection state to satisfy `wanted`.
async fn await_state(client: &MqttClient, timeout: Duration, wanted: impl Fn(State) -> bool) -> State {
  let mut status = client.status();
  tokio::time::timeout(timeout, async {
    loop {
      let state = status.borrow_and_update().state;
      if wanted(state) {
        return state;
      }
      if status.changed().await.is_err() {
        return state;
      }
    }
  })
  .await
  .unwrap_or_else(|_| client.status_now().state)
}

#[test]
fn a_published_message_comes_back_on_the_subscription() {
  with_broker(
    "a_published_message_comes_back_on_the_subscription",
    |broker| async move {
      // One device running both applications, which is the everyday case and the one
      // that goes wrong if the two share a client identifier.
      let config = broker.config("round-trip");
      let mut subscriber = connect(&config, Role::Gui).await;
      let publisher = connect(&config, Role::Cli).await;
      assert_ne!(
        subscriber.status_now().client_id,
        publisher.status_now().client_id,
        "the two applications must not collide on the broker"
      );

      let mut incoming = subscriber.take_incoming().expect("the incoming channel");
      subscriber
        .subscribe(config.subscription_filters(), Qos::AtLeastOnce)
        .await
        .expect("the broker accepts the prefix subscription");
      assert_eq!(subscriber.subscriptions(), vec!["hiveme/round-trip/#".to_owned()]);
      assert_eq!(subscriber.status_now().subscriptions, 1);

      let sender = Sender::from_device(&config.device, "hmc");
      let message = Message::new_text(sender, "the build finished")
        .with_title("CI")
        .with_level(Level::Warn)
        .with_ttl_secs(600);
      let topic = config.resolve_topic("warn", false);
      publisher
        .publish_message(&topic, &message, Qos::AtLeastOnce, false)
        .await
        .expect("the broker acknowledges the publish");

      let received = next_message(&mut incoming).await;
      assert_eq!(received.topic, "hiveme/round-trip/warn");
      assert_eq!(received.qos, Qos::AtLeastOnce);
      assert!(!received.retain);

      // The MQTT 5 properties of the envelope survive the round trip, which is what lets
      // a reader tell a HiveMe message from any other producer's before parsing it.
      assert_eq!(received.properties.content_type.as_deref(), Some("application/json"));
      assert_eq!(received.properties.payload_format_indicator, Some(1));
      assert_eq!(received.properties.hiveme_version(), Some(1));
      assert!(
        received.properties.message_expiry_interval.unwrap_or(0) <= 600,
        "the broker counts the expiry down rather than up"
      );

      match received.parse() {
        Parsed::Envelope(envelope) => {
          assert_eq!(envelope.id, message.id);
          let payload = envelope.payload.as_ref().expect("the payload");
          assert_eq!(payload.body, "the build finished");
          assert_eq!(payload.title.as_deref(), Some("CI"));
          assert_eq!(envelope.level(), Level::Warn);
          assert_eq!(envelope.sender.as_ref().unwrap().id.as_deref(), Some("round-trip"));
        }
        other => panic!("the payload did not parse as an envelope: {other:?}"),
      }

      assert_eq!(subscriber.dropped_messages(), 0);
      publisher.disconnect().await.expect("the publisher says goodbye");
      subscriber.disconnect().await.expect("the subscriber says goodbye");
      assert_eq!(publisher.status_now().state, State::Disconnected);
    },
  );
}

#[test]
fn every_quality_of_service_is_acknowledged_before_the_publish_returns() {
  with_broker(
    "every_quality_of_service_is_acknowledged_before_the_publish_returns",
    |broker| async move {
      let config = broker.config("qos");
      let mut subscriber = connect(&config, Role::Gui).await;
      let mut incoming = subscriber.take_incoming().expect("the incoming channel");
      subscriber
        .subscribe(config.subscription_filters(), Qos::ExactlyOnce)
        .await
        .expect("the broker accepts the prefix subscription");

      let publisher = connect(&config, Role::Cli).await;
      let sender = Sender::from_device(&config.device, "hmc");
      let topic = config.default_topic();

      // Each of these returns only once the broker has answered: nothing for QoS 0, a
      // PUBACK for QoS 1, a PUBCOMP for QoS 2. A bug in the acknowledgement tracking
      // shows up here as a timeout rather than as a wrong value.
      for qos in [Qos::AtMostOnce, Qos::AtLeastOnce, Qos::ExactlyOnce] {
        let message = Message::new_text(sender.clone(), format!("quality of service {qos}"));
        publisher
          .publish_message(&topic, &message, qos, false)
          .await
          .unwrap_or_else(|error| panic!("a qos {qos} publish should be acknowledged: {error}"));
      }

      let mut bodies = Vec::new();
      for _ in 0..3 {
        match next_message(&mut incoming).await.parse() {
          Parsed::Envelope(envelope) => bodies.push(envelope.payload.as_ref().unwrap().body.clone()),
          other => panic!("the payload did not parse as an envelope: {other:?}"),
        }
      }
      bodies.sort();
      assert_eq!(
        bodies,
        [
          "quality of service 0".to_owned(),
          "quality of service 1".to_owned(),
          "quality of service 2".to_owned(),
        ]
      );

      assert_eq!(publisher.status_now().state, State::Connected);
      publisher.disconnect().await.expect("the publisher says goodbye");
      subscriber.disconnect().await.expect("the subscriber says goodbye");
    },
  );
}

#[test]
fn a_wildcard_subscription_collects_the_prefix_and_nothing_else() {
  with_broker(
    "a_wildcard_subscription_collects_the_prefix_and_nothing_else",
    |broker| async move {
      let config = broker.config("tree");
      let mut subscriber = connect(&config, Role::Gui).await;
      let mut incoming = subscriber.take_incoming().expect("the incoming channel");
      subscriber
        .subscribe(config.subscription_filters(), Qos::AtLeastOnce)
        .await
        .expect("the broker accepts the prefix subscription");

      let publisher = connect(&config, Role::Cli).await;
      let topics = [
        config.resolve_topic("info", false),
        config.resolve_topic("build/nightly", false),
        config.resolve_topic("a/b/c", false),
      ];
      for topic in &topics {
        publisher
          .publish(topic, topic.as_bytes().to_vec(), Qos::AtLeastOnce, false, None)
          .await
          .unwrap_or_else(|error| panic!("cannot publish to {topic}: {error}"));
      }
      // Outside the prefix, and published last, so that its absence is meaningful: the
      // three above have already arrived by the time it would have.
      publisher
        .publish("elsewhere/info", b"no".to_vec(), Qos::AtLeastOnce, false, None)
        .await
        .expect("the broker accepts a topic outside the prefix");

      let mut seen = Vec::new();
      for _ in 0..topics.len() {
        seen.push(next_message(&mut incoming).await.topic);
      }
      seen.sort();
      let mut expected = topics.to_vec();
      expected.sort();
      assert_eq!(seen, expected);

      tokio::time::sleep(Duration::from_millis(500)).await;
      assert!(
        incoming.try_recv().is_err(),
        "a topic outside the prefix reached a prefix subscription"
      );

      publisher.disconnect().await.expect("the publisher says goodbye");
      subscriber.disconnect().await.expect("the subscriber says goodbye");
    },
  );
}

#[test]
fn a_payload_that_is_not_a_hiveme_envelope_is_still_delivered() {
  with_broker(
    "a_payload_that_is_not_a_hiveme_envelope_is_still_delivered",
    |broker| async move {
      let config = broker.config("lenient");
      let mut subscriber = connect(&config, Role::Gui).await;
      let mut incoming = subscriber.take_incoming().expect("the incoming channel");
      subscriber
        .subscribe(config.subscription_filters(), Qos::AtLeastOnce)
        .await
        .expect("the broker accepts the prefix subscription");

      let publisher = connect(&config, Role::Cli).await;
      let topic = config.default_topic();
      // Tiers B and C of docs/specs/message.md, as a user's own script would send them.
      publisher
        .publish(&topic, br#"{"any":"json"}"#.to_vec(), Qos::AtLeastOnce, false, None)
        .await
        .expect("the broker accepts the JSON payload");
      publisher
        .publish(&topic, b"just text".to_vec(), Qos::AtLeastOnce, false, None)
        .await
        .expect("the broker accepts the text payload");

      let first = next_message(&mut incoming).await;
      assert!(matches!(first.parse(), Parsed::RawJson(_)), "{first:?}");
      assert_eq!(first.properties.hiveme_version(), None);
      let second = next_message(&mut incoming).await;
      assert!(matches!(second.parse(), Parsed::RawText(_)), "{second:?}");

      publisher.disconnect().await.expect("the publisher says goodbye");
      subscriber.disconnect().await.expect("the subscriber says goodbye");
    },
  );
}

#[test]
fn a_retained_message_greets_a_later_subscriber() {
  with_broker("a_retained_message_greets_a_later_subscriber", |broker| async move {
    let config = broker.config("retained");
    let publisher = connect(&config, Role::Cli).await;
    let topic = config.default_topic();
    let sender = Sender::from_device(&config.device, "hmc");
    let message = Message::new_text(sender, "the last word");
    publisher
      .publish_message(&topic, &message, Qos::AtLeastOnce, true)
      .await
      .expect("the broker acknowledges the retained publish");
    publisher.disconnect().await.expect("the publisher says goodbye");

    let mut subscriber = connect(&config, Role::Gui).await;
    let mut incoming = subscriber.take_incoming().expect("the incoming channel");
    subscriber
      .subscribe(config.subscription_filters(), Qos::AtLeastOnce)
      .await
      .expect("the broker accepts the prefix subscription");

    let received = next_message(&mut incoming).await;
    assert!(received.retain, "a message stored before the subscription is retained");
    assert_eq!(received.topic, topic);
    subscriber.disconnect().await.expect("the subscriber says goodbye");
  });
}

#[test]
fn the_gui_reconnects_and_subscribes_again_when_the_broker_has_forgotten_it() {
  let Some(port) = free_port() else {
    eprintln!("skipping the_gui_reconnects_and_subscribes_again_when_the_broker_has_forgotten_it: no free port");
    return;
  };
  with_broker_on(
    "the_gui_reconnects_and_subscribes_again_when_the_broker_has_forgotten_it",
    Some(port),
    |mut broker| async move {
      let config = broker.config("survivor");
      let mut subscriber = connect(&config, Role::Gui).await;
      let mut incoming = subscriber.take_incoming().expect("the incoming channel");
      subscriber
        .subscribe(config.subscription_filters(), Qos::AtLeastOnce)
        .await
        .expect("the broker accepts the prefix subscription");

      // A brand new broker on the same address. It has no session for this client, so
      // the subscription only survives if the client sends it again by itself.
      broker.replace().await.expect("the broker is replaced");
      assert_eq!(broker.url(), config.broker.url, "the replacement kept the address");

      let dropped = await_state(&subscriber, RECONNECT_TIMEOUT, |state| state != State::Connected).await;
      assert_ne!(dropped, State::Connected, "the client should notice the broker leaving");
      assert_ne!(
        dropped,
        State::Disconnected,
        "a Gui client keeps trying rather than giving up"
      );

      let back = await_state(&subscriber, RECONNECT_TIMEOUT, |state| state == State::Connected).await;
      assert_eq!(back, State::Connected, "the client should find its way back");
      // The reconnect counter is cleared by the connection that succeeded.
      assert_eq!(subscriber.status_now().attempt, 0);

      let publisher = connect(&config, Role::Cli).await;
      let sender = Sender::from_device(&config.device, "hmc");
      let message = Message::new_text(sender, "still here");
      let topic = config.default_topic();
      publisher
        .publish_message(&topic, &message, Qos::AtLeastOnce, false)
        .await
        .expect("the replacement broker acknowledges the publish");

      let received = next_message(&mut incoming).await;
      assert_eq!(received.topic, topic);
      assert_eq!(subscriber.subscriptions(), config.subscription_filters());

      publisher.disconnect().await.expect("the publisher says goodbye");
      subscriber.disconnect().await.expect("the subscriber says goodbye");
    },
  );
}

#[test]
fn a_topic_the_broker_would_refuse_is_refused_before_it_is_sent() {
  with_broker(
    "a_topic_the_broker_would_refuse_is_refused_before_it_is_sent",
    |broker| async move {
      let client = connect(&broker.config("refusals"), Role::Gui).await;

      let error = client.subscribe(["hiveme/#/more"], Qos::AtLeastOnce).await.unwrap_err();
      assert!(error.to_string().contains("hiveme/#/more"), "{error}");
      assert!(client.subscriptions().is_empty());

      let error = client
        .publish("hiveme/+", b"x".to_vec(), Qos::AtLeastOnce, false, None)
        .await
        .unwrap_err();
      assert!(error.to_string().contains("hiveme/+"), "{error}");

      // Neither refusal cost the connection, so the caller can correct itself and
      // carry on.
      assert_eq!(client.status_now().state, State::Connected);
      client.disconnect().await.expect("the client says goodbye");
    },
  );
}

#[test]
fn a_connection_to_a_port_nothing_answers_on_fails_rather_than_hangs() {
  runtime().block_on(async {
    let mut config = Config::default();
    config.device.id = "nowhere".to_owned();
    // Port 9 on the loopback interface is the discard service, which nothing runs.
    config.broker.url = "mqtt://127.0.0.1:9".to_owned();
    config.broker.connect_timeout_secs = 3;

    let started = std::time::Instant::now();
    let error = MqttClient::connect_unvalidated(&config, Role::Cli)
      .await
      .expect_err("there is nothing to connect to");
    assert!(error.is_connection(), "{error}");
    assert!(
      started.elapsed() < Duration::from_secs(30),
      "the connect attempt should give up near broker.connectTimeoutSecs"
    );
  });
}

/// The only test that exercises TLS against a public certificate chain.
///
/// It runs when `HIVEME_TEST_BROKER_URL`, `HIVEME_TEST_USERNAME`, and
/// `HIVEME_TEST_PASSWORD` name a real cluster, and is skipped otherwise. The container
/// the other tests use has no TLS, so without this the handshake is covered only by the
/// unit tests of the rustls configuration.
#[test]
fn a_real_cloud_cluster_accepts_a_message() {
  let url = std::env::var("HIVEME_TEST_BROKER_URL").ok();
  let username = std::env::var("HIVEME_TEST_USERNAME").ok();
  let password = std::env::var("HIVEME_TEST_PASSWORD").ok();
  let (Some(url), Some(username), Some(password)) = (url, username, password) else {
    eprintln!(
      "skipping a_real_cloud_cluster_accepts_a_message: set HIVEME_TEST_BROKER_URL, \
       HIVEME_TEST_USERNAME, and HIVEME_TEST_PASSWORD to run it"
    );
    return;
  };

  runtime().block_on(async move {
    let mut config = Config::new_for_this_device();
    config.broker.url = url;
    config.broker.username = username;
    config.broker.password = password;
    // A namespace of its own, so a test run cannot disturb the messages a real
    // installation keeps on the same cluster.
    config.topics.prefix = format!("hiveme-test/{}", &config.device.id[..8]);
    config.validate().expect("the test config is usable");

    let mut subscriber = MqttClient::connect(&config, Role::Gui)
      .await
      .expect("the cluster accepts the subscriber");
    let mut incoming = subscriber.take_incoming().expect("the incoming channel");
    subscriber
      .subscribe(config.subscription_filters(), Qos::AtLeastOnce)
      .await
      .expect("the credential may subscribe under the test prefix");

    let publisher = MqttClient::connect(&config, Role::Cli)
      .await
      .expect("the cluster accepts the publisher");
    let sender = Sender::from_device(&config.device, "hmc");
    let message = Message::new_text(sender, "hello from the integration test");
    let topic = config.default_topic();
    publisher
      .publish_message(&topic, &message, Qos::AtLeastOnce, false)
      .await
      .expect("the cluster acknowledges the publish");

    let received = next_message(&mut incoming).await;
    assert_eq!(received.topic, topic);
    match received.parse() {
      Parsed::Envelope(envelope) => assert_eq!(envelope.id, message.id),
      other => panic!("the payload did not parse as an envelope: {other:?}"),
    }

    publisher.disconnect().await.expect("the publisher says goodbye");
    subscriber.disconnect().await.expect("the subscriber says goodbye");
  });
}
