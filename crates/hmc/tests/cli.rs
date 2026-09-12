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

//! What a script sees when it runs `hmc`: the exit code, stdout, and stderr.
//!
//! The codes are the table in `docs/specs/cli.md`. Nothing here reaches a broker; the
//! one test that needs one lives in `publish.rs`.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use hiveme_core::config::{BrokerInit, Config, EncryptionMode};
use predicates::prelude::*;

/// A `hmc` invocation with the developer's own environment kept out of it.
///
/// `HIVEME_CONFIG` and `HIVEME_PASSWORD` are read by the config module, so a test that
/// did not clear them could pass or fail on the machine it ran on.
fn hmc() -> Command {
  let mut command = Command::cargo_bin("hmc").expect("the hmc binary is built");
  command.env_remove("HIVEME_CONFIG");
  command.env_remove("HIVEME_PASSWORD");
  command.env_remove("RUST_LOG");
  command
}

/// A scratch directory and the config path inside it, which nothing has written yet.
fn scratch() -> (tempfile::TempDir, PathBuf) {
  let directory = tempfile::tempdir().expect("a temporary directory");
  let path = directory.path().join("HiveMe.json");
  (directory, path)
}

/// Writes `config` to `path` the way the config module would.
fn write(path: &Path, config: &Config) {
  let text = serde_json::to_string_pretty(config).expect("the config serialises");
  std::fs::write(path, text).expect("the config is written");
}

/// A config that would connect, if anything were listening.
fn usable() -> Config {
  let mut config = Config::new_for_this_device();
  config.broker.url = "mqtt://127.0.0.1:9".to_owned();
  config.broker.username = "hiveme".to_owned();
  config.broker.password = "secret".to_owned();
  config.broker.connect_timeout_secs = 3;
  config
}

#[test]
fn help_is_printed_on_request_and_exits_zero() {
  hmc()
    .arg("--help")
    .assert()
    .success()
    .stdout(predicate::str::contains("Usage:").and(predicate::str::contains("--topic")));
}

#[test]
fn the_version_is_the_workspace_version() {
  hmc()
    .arg("--version")
    .assert()
    .success()
    .stdout(predicate::str::contains(hiveme_core::VERSION));
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
  hmc()
    .arg("--nope")
    .assert()
    .code(2)
    .stderr(predicate::str::contains("--nope"));
}

#[test]
fn an_empty_stdin_is_a_usage_error() {
  let (_directory, path) = scratch();
  hmc()
    .arg("--config")
    .arg(&path)
    .write_stdin("")
    .assert()
    .code(2)
    .stdout(predicate::str::is_empty())
    .stderr(predicate::str::starts_with("hmc: usage:"));
}

#[test]
fn an_absolute_topic_without_a_topic_is_a_usage_error() {
  hmc()
    .args(["-T", "hello"])
    .assert()
    .code(2)
    .stderr(predicate::str::contains("--topic"));
}

#[test]
fn a_level_outside_the_message_format_is_a_usage_error() {
  hmc().args(["-l", "critical", "boom"]).assert().code(2);
}

#[test]
fn a_quality_of_service_outside_mqtt_is_a_usage_error() {
  hmc().args(["-q", "3", "boom"]).assert().code(2);
}

#[test]
fn json_with_input_that_is_not_json_is_a_usage_error() {
  let (_directory, path) = scratch();
  write(&path, &usable());
  // The config is usable and the broker is not, so reaching exit 2 rather than 4 is
  // what proves the input is checked before anything connects.
  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--json", "not json"])
    .assert()
    .code(2)
    .stdout(predicate::str::is_empty())
    .stderr(predicate::str::starts_with("hmc: usage:").and(predicate::str::contains("--json")));
}

#[test]
fn json_conflicts_with_the_envelope_only_options() {
  for arguments in [
    vec!["--json", "--title", "CI", "{}"],
    vec!["--json", "--level", "warn", "{}"],
  ] {
    hmc().args(&arguments).assert().code(2);
  }
}

#[test]
fn a_missing_config_is_written_and_reported() {
  let (_directory, path) = scratch();
  assert!(!path.exists());

  hmc()
    .arg("--config")
    .arg(&path)
    .arg("hello")
    .assert()
    .code(3)
    .stdout(predicate::str::is_empty())
    .stderr(
      predicate::str::starts_with("hmc: config:")
        .and(predicate::str::contains(path.display().to_string()))
        .and(predicate::str::contains("broker")),
    );

  // The file is there for the user to fill in, and it already carries an identity so
  // that two installations never publish as the same device.
  assert!(path.exists(), "the default config should have been written");
  let written: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  assert!(!written.device.id.trim().is_empty());
  assert_eq!(written.version, hiveme_core::config::CONFIG_VERSION);
  assert!(written.broker.url.is_empty());
}

#[test]
fn a_config_that_exists_but_is_not_filled_in_reports_what_is_missing() {
  let (_directory, path) = scratch();
  write(&path, &Config::new_for_this_device());

  hmc().arg("--config").arg(&path).arg("hello").assert().code(3).stderr(
    predicate::str::starts_with("hmc: config:")
      .and(predicate::str::contains("broker.url"))
      .and(predicate::str::contains("broker.username"))
      // The second run must not claim it wrote the file again.
      .and(predicate::str::contains("a default one was written").not()),
  );
}

#[test]
fn a_config_that_is_not_json_is_a_config_error() {
  let (_directory, path) = scratch();
  std::fs::write(&path, "{ this is not json").unwrap();
  hmc()
    .arg("--config")
    .arg(&path)
    .arg("hello")
    .assert()
    .code(3)
    .stderr(predicate::str::starts_with("hmc: config:"));
}

#[test]
fn the_config_path_can_come_from_the_environment() {
  let (_directory, path) = scratch();
  hmc()
    .env("HIVEME_CONFIG", &path)
    .arg("hello")
    .assert()
    .code(3)
    .stderr(predicate::str::contains(path.display().to_string()));
  assert!(path.exists(), "HIVEME_CONFIG should name the file that was written");
}

#[test]
fn encryption_that_is_not_implemented_is_refused_before_connecting() {
  let (_directory, path) = scratch();
  let mut config = usable();
  config.encryption.mode = EncryptionMode::Required;
  config.encryption.keys = Vec::new();
  write(&path, &config);

  hmc()
    .arg("--config")
    .arg(&path)
    .arg("hello")
    .assert()
    .code(3)
    .stderr(predicate::str::starts_with("hmc: config:").and(predicate::str::contains("encryption")));
}

/// The setup string the `hmg` Settings tab would show for a real cluster.
fn setup_string() -> String {
  let mut config = Config::default();
  config.broker.url = "mqtts://abc123.s1.eu.hivemq.cloud:8883".to_owned();
  config.broker.username = "hiveme-sam".to_owned();
  config.broker.password = "s3cret".to_owned();
  config.topics.prefix = "team".to_owned();
  BrokerInit::from_config(&config).to_json()
}

#[test]
fn init_writes_a_config_from_the_setup_string() {
  let (_directory, path) = scratch();
  assert!(!path.exists());

  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--init", &setup_string()])
    .assert()
    .success()
    // The path is the one thing hmc puts on stdout, so a user knows what to edit next.
    .stdout(predicate::str::contains(path.display().to_string()));

  let written: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  assert_eq!(written.broker.url, "mqtts://abc123.s1.eu.hivemq.cloud:8883");
  assert_eq!(written.broker.username, "hiveme-sam");
  assert_eq!(written.broker.password, "s3cret");
  assert_eq!(written.topics.prefix, "team");
  assert!(
    !written.device.id.trim().is_empty(),
    "the config still needs an identity"
  );
}

#[test]
fn a_second_init_keeps_the_identity_and_the_settings_around_it() {
  let (_directory, path) = scratch();
  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--init", &setup_string()])
    .assert()
    .success();
  let first: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

  // A setting the user changed between the two runs, which must survive the second.
  let mut edited = first.clone();
  edited.publish.qos = 2;
  write(&path, &edited);

  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--init", &setup_string()])
    .assert()
    .success();

  let second: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  assert_eq!(second.device.id, first.device.id, "the device identity must not change");
  assert_eq!(second.publish.qos, 2, "an unrelated setting must survive");
}

#[test]
fn init_keeps_keys_a_newer_build_wrote() {
  let (_directory, path) = scratch();
  // A config with a key this build knows nothing about, as a newer HiveMe would leave.
  let mut document: serde_json::Value = serde_json::to_value(Config::new_for_this_device()).unwrap();
  document["somethingNewer"] = serde_json::json!({"kept": true});
  std::fs::write(&path, serde_json::to_string_pretty(&document).unwrap()).unwrap();

  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--init", &setup_string()])
    .assert()
    .success();

  let reread: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  assert_eq!(reread["somethingNewer"], serde_json::json!({"kept": true}));
  assert_eq!(reread["broker"]["username"], "hiveme-sam");
}

#[test]
fn a_setup_string_that_is_not_json_is_a_usage_error() {
  let (_directory, path) = scratch();
  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--init", "not json"])
    .assert()
    .code(2)
    .stdout(predicate::str::is_empty())
    .stderr(predicate::str::starts_with("hmc: usage:").and(predicate::str::contains("setup string")));
  assert!(!path.exists(), "a bad setup string must not leave a config behind");
}

#[test]
fn a_setup_string_missing_its_credentials_says_which() {
  let (_directory, path) = scratch();
  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--init", r#"{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883"}"#])
    .assert()
    .code(2)
    .stderr(predicate::str::contains("username is empty").and(predicate::str::contains("password is empty")));
}

#[test]
fn a_setup_string_from_a_newer_hmg_is_refused() {
  let (_directory, path) = scratch();
  hmc()
    .arg("--config")
    .arg(&path)
    .args([
      "--init",
      r#"{"v":99,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"u","password":"p"}"#,
    ])
    .assert()
    .code(2)
    .stderr(predicate::str::contains("version 99"));
}

#[test]
fn a_setup_string_survives_the_quotes_a_shell_leaves_on_it() {
  let (_directory, path) = scratch();
  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--init", &format!("  '{}'  ", setup_string())])
    .assert()
    .success();
  let written: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  assert_eq!(written.broker.username, "hiveme-sam");
}

#[test]
fn init_does_not_publish() {
  let (_directory, path) = scratch();
  let mut config = usable();
  config.broker.connect_timeout_secs = 3;
  write(&path, &config);
  // The broker is dead, so an init that tried to connect would exit 4 rather than 0.
  hmc()
    .arg("--config")
    .arg(&path)
    .args(["--init", &setup_string()])
    .timeout(std::time::Duration::from_secs(30))
    .assert()
    .success();
}

#[test]
fn a_wildcard_in_the_topic_is_a_usage_error() {
  let (_directory, path) = scratch();
  write(&path, &usable());
  hmc()
    .arg("--config")
    .arg(&path)
    .args(["-t", "build/#", "hello"])
    .assert()
    .code(2)
    .stderr(predicate::str::starts_with("hmc: usage:").and(predicate::str::contains("--topic")));
}

#[test]
fn a_broker_nothing_answers_on_is_a_connection_error() {
  let (_directory, path) = scratch();
  write(&path, &usable());
  hmc()
    .arg("--config")
    .arg(&path)
    .arg("hello")
    .timeout(std::time::Duration::from_secs(60))
    .assert()
    .code(4)
    .stdout(predicate::str::is_empty())
    // The failure is the last line, after any warning the connection attempt raised.
    .stderr(
      predicate::str::contains(
        "
hmc: connection:",
      )
      .or(predicate::str::starts_with("hmc: connection:")),
    );
}

#[test]
fn an_unencrypted_broker_warns_even_without_verbose() {
  let (_directory, path) = scratch();
  write(&path, &usable());
  // A successful run over TLS prints nothing, but a password about to cross the
  // network in the clear is worth a line whether or not anyone asked for one.
  hmc()
    .arg("--config")
    .arg(&path)
    .arg("hello")
    .timeout(std::time::Duration::from_secs(60))
    .assert()
    .code(4)
    .stderr(predicate::str::contains("not encrypted"));
}

#[test]
fn verbose_explains_what_it_is_doing() {
  let (_directory, path) = scratch();
  write(&path, &usable());
  hmc()
    .arg("--config")
    .arg(&path)
    .args(["-v", "hello"])
    .timeout(std::time::Duration::from_secs(60))
    .assert()
    .code(4)
    .stdout(predicate::str::is_empty())
    .stderr(predicate::str::contains("connecting to").and(predicate::str::contains(path.display().to_string())));
}
