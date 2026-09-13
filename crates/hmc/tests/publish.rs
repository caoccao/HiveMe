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

//! `hmc` against a real broker: what a subscriber actually receives.
//!
//! The unit tests prove that the envelope is built correctly and `cli.rs` proves the
//! exit codes, so what is left is the part neither can see, which is that the bytes
//! arrive and still parse at the other end.
//!
//! A HiveMQ CE container stands in for the cloud. Its allow-all extension accepts any
//! credentials, so the config below can carry the username and password that
//! `Config::validate` requires. Without Docker, or with `HIVEME_SKIP_DOCKER=1`, each
//! test says why it did nothing and passes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use assert_cmd::Command;
use hiveme_core::config::{BrokerInit, Config};
use hiveme_core::message::{Level, Parsed};
use hiveme_core::mqtt::{IncomingMessage, MqttClient, Qos, Role};
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

/// How long a test waits for a message `hmc` has already been told was acknowledged.
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(15);

/// Why this test did nothing, when it did nothing.
fn skip_reason() -> Option<String> {
  match std::env::var(SKIP_VARIABLE).as_deref() {
    Ok("1") => Some(format!("{SKIP_VARIABLE}=1")),
    _ => None,
  }
}

/// Runs `body` against a freshly started broker, or explains why it did not.
fn with_broker<F, Fut>(name: &str, body: F)
where
  F: FnOnce(Fixture) -> Fut,
  Fut: std::future::Future<Output = ()>,
{
  if let Some(reason) = skip_reason() {
    eprintln!("skipping {name}: {reason}");
    return;
  }
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("a tokio runtime");
  runtime.block_on(async move {
    let fixture = match Fixture::start().await {
      Ok(fixture) => fixture,
      Err(reason) => {
        eprintln!("skipping {name}: the HiveMQ CE container did not start ({reason})");
        return;
      }
    };
    body(fixture).await;
  });
}

/// A running broker and a config file that points `hmc` at it.
///
/// Every test starts its own, so the topics below are the ones a real installation
/// uses rather than a namespace invented to keep tests apart.
struct Fixture {
  #[allow(dead_code, reason = "held so the container outlives the test that uses it")]
  container: ContainerAsync<GenericImage>,
  #[allow(dead_code, reason = "held so the directory outlives the config file inside it")]
  directory: tempfile::TempDir,
  config_path: PathBuf,
  config: Config,
}

impl Fixture {
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

    let mut config = Config::new_for_this_device();
    config.device.name = "test-runner".to_owned();
    config.broker.url = format!("mqtt://{host}:{port}");
    // Any credentials do: the broker accepts all of them, and Config::validate insists
    // on something being there.
    config.broker.username = "hiveme".to_owned();
    config.broker.password = "test".to_owned();
    config.broker.connect_timeout_secs = 20;
    config.publish.timeout_secs = 20;
    config.validate().map_err(|error| error.to_string())?;

    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let config_path = directory.path().join("HiveMe.json");
    std::fs::write(
      &config_path,
      serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    Ok(Self {
      container,
      directory,
      config_path,
      config,
    })
  }

  /// A subscriber already listening to everything under the test's prefix.
  async fn subscriber(&self) -> (MqttClient, Receiver<IncomingMessage>) {
    let mut client = MqttClient::connect(&self.config, Role::Gui)
      .await
      .expect("the subscriber connects");
    let incoming = client.take_incoming().expect("the incoming channel");
    client
      .subscribe(self.config.subscription_filters(), Qos::AtLeastOnce)
      .await
      .expect("the broker accepts the prefix subscription");
    (client, incoming)
  }

  /// Runs `hmc` against this broker, off the async runtime's threads.
  async fn hmc(&self, arguments: Vec<&'static str>, stdin: Option<&'static str>) -> std::process::Output {
    let path = self.config_path.clone();
    tokio::task::spawn_blocking(move || run_hmc(&path, arguments, stdin))
      .await
      .expect("the hmc process runs")
  }
}

fn run_hmc(config: &Path, arguments: Vec<&str>, stdin: Option<&str>) -> std::process::Output {
  let mut command = Command::cargo_bin("hmc").expect("the hmc binary is built");
  command.env_remove("HIVEME_CONFIG");
  command.env_remove("HIVEME_PASSWORD");
  command.env_remove("RUST_LOG");
  command.arg("--config").arg(config);
  command.args(arguments);
  if let Some(text) = stdin {
    command.write_stdin(text);
  }
  command.timeout(Duration::from_secs(60)).output().expect("hmc runs")
}

/// Waits for the next message, failing the test rather than hanging the suite.
async fn next_message(incoming: &mut Receiver<IncomingMessage>) -> IncomingMessage {
  tokio::time::timeout(RECEIVE_TIMEOUT, incoming.recv())
    .await
    .expect("a message arrives within the timeout")
    .expect("the incoming channel is still open")
}

/// Fails with the process output when `hmc` did not exit 0.
fn assert_published(output: &std::process::Output, topic: &str) {
  assert!(
    output.status.success(),
    "hmc exited {:?}\nstdout: {}\nstderr: {}",
    output.status.code(),
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  assert_eq!(
    String::from_utf8_lossy(&output.stdout),
    format!("Message sent to {topic}.\n")
  );
}

#[test]
fn a_message_argument_reaches_a_subscriber_as_an_envelope() {
  with_broker(
    "a_message_argument_reaches_a_subscriber_as_an_envelope",
    |fixture| async move {
      let (subscriber, mut incoming) = fixture.subscriber().await;

      let output = fixture
        .hmc(vec!["--title", "CI", "--level", "warn", "the build finished"], None)
        .await;
      assert_published(&output, "hiveme");

      let received = next_message(&mut incoming).await;
      assert_eq!(received.topic, "hiveme");
      assert_eq!(received.properties.content_type.as_deref(), Some("application/json"));
      assert_eq!(received.properties.hiveme_version(), Some(1));
      assert!(!received.retain);

      match received.parse() {
        Parsed::Envelope(envelope) => {
          let payload = envelope.payload.as_ref().expect("the payload");
          assert_eq!(payload.body, "the build finished");
          assert_eq!(payload.title.as_deref(), Some("CI"));
          // The explicit payload level does not change the default MQTT topic.
          assert_eq!(envelope.level(), Level::Warn);

          let sender = envelope.sender.as_ref().expect("the sender");
          assert_eq!(sender.id.as_deref(), Some(fixture.config.device.id.as_str()));
          assert_eq!(sender.name.as_deref(), Some("test-runner"));
          assert_eq!(sender.app.as_deref(), Some("hmc"));
          assert_eq!(sender.app_version.as_deref(), Some(hiveme_core::VERSION));
          assert!(envelope.validate().is_ok());
        }
        other => panic!("the payload did not parse as an envelope: {other:?}"),
      }

      subscriber.disconnect().await.expect("the subscriber says goodbye");
    },
  );
}

#[test]
fn a_body_piped_in_reaches_a_subscriber() {
  with_broker("a_body_piped_in_reaches_a_subscriber", |fixture| async move {
    let (subscriber, mut incoming) = fixture.subscriber().await;

    // `echo hi | hmc`, newline and all.
    let output = fixture.hmc(Vec::new(), Some("the build finished\n")).await;
    assert_published(&output, "hiveme");

    let received = next_message(&mut incoming).await;
    // No --topic, so this went to the configured default.
    assert_eq!(received.topic, "hiveme");
    match received.parse() {
      Parsed::Envelope(envelope) => {
        assert_eq!(envelope.payload.as_ref().unwrap().body, "the build finished");
        assert_eq!(envelope.level(), Level::Info);
      }
      other => panic!("the payload did not parse as an envelope: {other:?}"),
    }

    subscriber.disconnect().await.expect("the subscriber says goodbye");
  });
}

#[test]
fn a_json_publish_arrives_unchanged_and_claims_no_envelope() {
  with_broker(
    "a_json_publish_arrives_unchanged_and_claims_no_envelope",
    |fixture| async move {
      let (subscriber, mut incoming) = fixture.subscriber().await;

      let payload = r#"{"stage":"deploy","ok":true}"#;
      let output = fixture
        .hmc(vec!["--json", r#"{"stage":"deploy","ok":true}"#], None)
        .await;
      assert_published(&output, "hiveme");

      let received = next_message(&mut incoming).await;
      assert_eq!(received.payload, payload.as_bytes());
      assert_eq!(received.properties.content_type.as_deref(), Some("application/json"));
      // Not an envelope, and it must not pretend to be one.
      assert_eq!(received.properties.hiveme_version(), None);
      assert!(matches!(received.parse(), Parsed::RawJson(_)));

      subscriber.disconnect().await.expect("the subscriber says goodbye");
    },
  );
}

#[test]
fn an_absolute_topic_leaves_the_prefix_behind() {
  with_broker("an_absolute_topic_leaves_the_prefix_behind", |fixture| async move {
    let mut subscriber = MqttClient::connect(&fixture.config, Role::Gui)
      .await
      .expect("the subscriber connects");
    let mut incoming = subscriber.take_incoming().expect("the incoming channel");
    subscriber
      .subscribe(["elsewhere/#"], Qos::AtLeastOnce)
      .await
      .expect("the broker accepts the subscription");

    let output = fixture.hmc(vec!["-T", "-t", "elsewhere/status", "up"], None).await;
    assert_published(&output, "elsewhere/status");

    let received = next_message(&mut incoming).await;
    assert_eq!(received.topic, "elsewhere/status");

    subscriber.disconnect().await.expect("the subscriber says goodbye");
  });
}

#[test]
fn a_retained_message_waits_for_the_next_subscriber() {
  with_broker(
    "a_retained_message_waits_for_the_next_subscriber",
    |fixture| async move {
      let output = fixture.hmc(vec!["--retain", "the last word"], None).await;
      assert_published(&output, "hiveme");

      // Subscribing only now: the message can only arrive because the broker kept it.
      let (subscriber, mut incoming) = fixture.subscriber().await;
      let received = next_message(&mut incoming).await;
      assert!(received.retain);
      assert_eq!(received.topic, "hiveme");

      subscriber.disconnect().await.expect("the subscriber says goodbye");
    },
  );
}

#[test]
fn every_quality_of_service_is_acknowledged_before_hmc_exits() {
  with_broker(
    "every_quality_of_service_is_acknowledged_before_hmc_exits",
    |fixture| async move {
      let (subscriber, mut incoming) = fixture.subscriber().await;

      // hmc returning 0 at QoS 1 and 2 means the broker acknowledged, so a bug in the
      // acknowledgement path shows up here as a timeout exit code rather than as a lost
      // message.
      for qos in ["0", "1", "2"] {
        let output = fixture.hmc(vec!["-q", qos, "hello"], None).await;
        assert_published(&output, "hiveme");
      }

      for _ in 0..3 {
        assert_eq!(next_message(&mut incoming).await.topic, "hiveme");
      }

      subscriber.disconnect().await.expect("the subscriber says goodbye");
    },
  );
}

#[test]
fn two_runs_at_once_do_not_disconnect_each_other() {
  with_broker("two_runs_at_once_do_not_disconnect_each_other", |fixture| async move {
    let (subscriber, mut incoming) = fixture.subscriber().await;

    // Every run takes its own client identifier, so the broker has no reason to drop
    // one of them. Sharing one would make this flake or lose a message.
    let (first, second) = tokio::join!(
      fixture.hmc(vec!["-t", "info", "first"], None),
      fixture.hmc(vec!["-t", "info", "second"], None)
    );
    assert_published(&first, "hiveme/info");
    assert_published(&second, "hiveme/info");

    let mut bodies = Vec::new();
    for _ in 0..2 {
      match next_message(&mut incoming).await.parse() {
        Parsed::Envelope(envelope) => bodies.push(envelope.payload.as_ref().unwrap().body.clone()),
        other => panic!("the payload did not parse as an envelope: {other:?}"),
      }
    }
    bodies.sort();
    assert_eq!(bodies, ["first".to_owned(), "second".to_owned()]);

    subscriber.disconnect().await.expect("the subscriber says goodbye");
  });
}

/// The setup string of a real cluster, when the developer has left one for the tests.
///
/// `.config/broker.json` is gitignored and holds exactly what the `hmg` Settings tab
/// shows, so this test feeds `hmc --init` the same string a user would paste.
fn cloud_setup() -> Option<BrokerInit> {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.config/broker.json");
  let text = std::fs::read_to_string(&path).ok()?;
  match BrokerInit::parse(&text) {
    Ok(setup) => Some(setup),
    Err(error) => {
      eprintln!("{} is not a usable setup string: {error}", path.display());
      None
    }
  }
}

/// `hmc --init` then `hmc` against a real HiveMQ Cloud cluster.
///
/// This is the walkthrough of `docs/specs/cli.md` end to end, over TLS, and the only
/// test that proves the setup string a user copies out of `hmg` is enough to publish.
/// It runs when `.config/broker.json` is there and is skipped otherwise, because a
/// cluster is an account rather than something CI can create.
#[test]
fn a_real_cloud_cluster_accepts_a_message_from_hmc() {
  let Some(setup) = cloud_setup() else {
    eprintln!(
      "skipping a_real_cloud_cluster_accepts_a_message_from_hmc: put the setup string from the \
       hmg Settings tab in .config/broker.json to run it"
    );
    return;
  };

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("a tokio runtime");
  runtime.block_on(async move {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_path = directory.path().join("HiveMe.json");

    // Exactly what a user does: paste the string, get a config.
    let json = setup.to_json();
    let output = tokio::task::spawn_blocking({
      let config_path = config_path.clone();
      let json = json.clone();
      move || {
        let mut command = Command::cargo_bin("hmc").expect("the hmc binary is built");
        command.env_remove("HIVEME_CONFIG");
        command.env_remove("HIVEME_PASSWORD");
        command.arg("--config").arg(&config_path).args(["--init", &json]);
        command.timeout(Duration::from_secs(60)).output().expect("hmc runs")
      }
    })
    .await
    .expect("the hmc process runs");
    assert!(
      output.status.success(),
      "hmc --init exited {:?}: {}",
      output.status.code(),
      String::from_utf8_lossy(&output.stderr)
    );

    // A namespace of its own, so the test cannot disturb a real installation sharing
    // the cluster. This is the one thing a user would not do.
    let mut config: Config = serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    config.topics.prefix = format!("hiveme-test/{}", &config.device.id[..8]);
    std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    let mut subscriber = MqttClient::connect(&config, Role::Gui)
      .await
      .expect("the cluster accepts the subscriber");
    let mut incoming = subscriber.take_incoming().expect("the incoming channel");
    subscriber
      .subscribe(config.subscription_filters(), Qos::AtLeastOnce)
      .await
      .expect("the credential may subscribe under the test prefix");

    let output = tokio::task::spawn_blocking({
      let config_path = config_path.clone();
      move || run_hmc(&config_path, vec!["hello from hmc"], None)
    })
    .await
    .expect("the hmc process runs");
    assert_published(&output, &config.default_topic());

    let received = next_message(&mut incoming).await;
    assert_eq!(received.topic, config.default_topic());
    match received.parse() {
      Parsed::Envelope(envelope) => {
        assert_eq!(envelope.payload.as_ref().unwrap().body, "hello from hmc");
        assert_eq!(envelope.sender.as_ref().unwrap().app.as_deref(), Some("hmc"));
      }
      other => panic!("the payload did not parse as an envelope: {other:?}"),
    }

    subscriber.disconnect().await.expect("the subscriber says goodbye");
  });
}
