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

//! The config behavior promised by `docs/specs/config.md`.

use std::path::{Path, PathBuf};

use hiveme_core::config::{
  CONFIG_VERSION, Config, ConfigFile, EncryptionMode, KeyEntry, KeyState, MigrationOutcome, PASSWORD_VARIABLE,
  REDACTED, Rule, SecretRef, Subscription, Theme,
};
use hiveme_core::error::Error;
use hiveme_core::message::Level;

fn fixture(name: &str) -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("tests/fixtures/config")
    .join(name)
}

fn load(name: &str) -> ConfigFile {
  ConfigFile::load(&fixture(name)).unwrap_or_else(|error| panic!("cannot load {name}: {error}"))
}

/// Copies a fixture into a scratch directory so a test may write to it.
fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
  let directory = tempfile::tempdir().expect("a temporary directory");
  let path = directory.path().join("HiveMe.json");
  std::fs::copy(fixture(name), &path).expect("the fixture copies");
  (directory, path)
}

/// A config that passes validation, which each test then breaks in one way.
fn valid() -> Config {
  let mut config = Config::default();
  config.device.id = "0f7a1c2e-5d4b-4a6e-9c1d-2b3e4f5a6b7c".to_owned();
  config.device.name = "sams-macbook".to_owned();
  config.broker.url = "mqtts://abc123.s1.eu.hivemq.cloud:8883".to_owned();
  config.broker.username = "hiveme-sam".to_owned();
  config.broker.password = "secret".to_owned();
  config
}

#[test]
fn the_defaults_survive_a_round_trip_through_json() {
  let config = Config::default();
  let text = serde_json::to_string_pretty(&config).unwrap();
  let parsed: Config = serde_json::from_str(&text).unwrap();
  assert_eq!(parsed, config);
}

#[test]
fn the_documented_defaults_are_the_real_defaults() {
  let config = Config::default();
  assert_eq!(config.version, CONFIG_VERSION);
  assert_eq!(config.broker.client_id_prefix, "hiveme");
  assert_eq!(config.broker.keep_alive_secs, 30);
  assert_eq!(config.broker.session_expiry_secs, 3_600);
  assert_eq!(config.broker.connect_timeout_secs, 10);
  assert!(config.broker.tls.verify_server);
  assert_eq!(config.broker.reconnect.initial_delay_ms, 1_000);
  assert_eq!(config.broker.reconnect.max_delay_ms, 30_000);
  assert_eq!(config.topics.prefix, "hiveme");
  assert_eq!(config.topics.default, "info");
  assert_eq!(
    config.topics.subscriptions,
    vec![Subscription::Relative("#".to_owned())]
  );
  assert_eq!(config.publish.qos, 1);
  assert!(!config.publish.retain);
  assert_eq!(config.publish.timeout_secs, 10);
  assert!(config.notifications.enabled);
  assert!(!config.notifications.notify_own_messages);
  assert_eq!(config.gui.theme, Theme::Ocean);
  assert_eq!(config.gui.language, "en-US");
  assert_eq!(config.gui.history.max_messages_per_topic, 1_000);
  assert_eq!(config.gui.history.retention_days, 30);
  assert_eq!(config.gui.window.size.width, 1_200);
  assert_eq!(config.gui.window.size.height, 900);
  assert_eq!(config.gui.window.position.x, -1);
  assert_eq!(config.encryption.mode, EncryptionMode::Off);
  assert!(config.cloud_api.is_none());
}

#[test]
fn the_three_built_in_rules_apply_when_none_are_listed() {
  let config = load("minimal.json").into_config();
  let ids: Vec<&str> = config.notifications.rules.iter().map(|rule| rule.id.as_str()).collect();
  assert_eq!(ids, ["info", "warn", "error"]);
  assert_eq!(config.notifications.rules[2].level, Level::Error);
}

#[test]
fn an_empty_rules_array_replaces_the_built_ins() {
  let text = r#"{ "version": 1, "notifications": { "rules": [] } }"#;
  let config = ConfigFile::from_text(Path::new("HiveMe.json"), text)
    .unwrap()
    .into_config();
  assert!(config.notifications.rules.is_empty());
}

#[test]
fn a_missing_version_is_read_as_the_current_one() {
  let file = load("no_version.json");
  assert_eq!(file.config().version, CONFIG_VERSION);
  assert_eq!(file.migration_outcome(), MigrationOutcome::AlreadyCurrent);
}

#[test]
fn a_version_one_document_needs_no_migration() {
  assert_eq!(
    load("minimal.json").migration_outcome(),
    MigrationOutcome::AlreadyCurrent
  );
}

#[test]
fn unknown_keys_survive_a_read_modify_write() {
  let (_directory, path) = scratch("unknown_keys.json");
  let mut file = ConfigFile::load(&path).unwrap();
  file.config_mut().gui.theme = Theme::Midnight;
  file.save().unwrap();

  let written: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  assert_eq!(written["futureTopLevel"]["kept"], true);
  assert_eq!(written["device"]["futureDeviceKey"], 1);
  assert_eq!(written["broker"]["futureBrokerKey"][0], "a");
  assert_eq!(
    written["notifications"]["futureNotificationKey"],
    serde_json::Value::Null
  );
  // The change this build made is there too.
  assert_eq!(written["gui"]["theme"], "Midnight");
  // And reloading sees both.
  assert_eq!(ConfigFile::load(&path).unwrap().config().gui.theme, Theme::Midnight);
}

#[test]
fn removing_a_rule_really_removes_it() {
  let (_directory, path) = scratch("minimal.json");
  let mut file = ConfigFile::load(&path).unwrap();
  file.config_mut().notifications.rules.retain(|rule| rule.id == "error");
  file.save().unwrap();

  let written: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  let rules = written["notifications"]["rules"].as_array().unwrap();
  assert_eq!(rules.len(), 1, "arrays are replaced, not merged element by element");
  assert_eq!(rules[0]["id"], "error");
}

#[test]
fn a_config_from_a_newer_build_is_read_but_never_rewritten() {
  let (_directory, path) = scratch("from_the_future.json");
  let mut file = ConfigFile::load(&path).unwrap();
  assert!(file.is_read_only());
  assert_eq!(file.config().device.name, "sams-macbook");

  let before = std::fs::read_to_string(&path).unwrap();
  file.config_mut().gui.theme = Theme::Coral;
  file.save().unwrap();
  assert_eq!(
    std::fs::read_to_string(&path).unwrap(),
    before,
    "the file must be left alone"
  );
}

#[test]
fn a_missing_file_is_reported_as_such() {
  let directory = tempfile::tempdir().unwrap();
  let error = ConfigFile::load(&directory.path().join("absent.json")).unwrap_err();
  assert!(matches!(error, Error::ConfigNotFound(_)), "got {error}");
  assert!(error.is_config());
}

#[test]
fn a_broken_document_names_the_file() {
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("HiveMe.json");
  std::fs::write(&path, "{ not json").unwrap();
  let error = ConfigFile::load(&path).unwrap_err();
  assert!(matches!(error, Error::ConfigParse { .. }), "got {error}");
  assert!(error.to_string().contains("HiveMe.json"));
}

#[test]
fn the_first_run_writes_a_config_with_an_identity() {
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("nested").join("HiveMe.json");

  let (file, created) = ConfigFile::load_or_create(&path).unwrap();
  assert!(created);
  assert!(path.exists(), "the directory is created along the way");
  assert!(!file.config().device.id.is_empty());
  assert!(uuid::Uuid::parse_str(&file.config().device.id).is_ok());

  let (again, created) = ConfigFile::load_or_create(&path).unwrap();
  assert!(!created, "a second run does not rewrite the file");
  assert_eq!(
    again.config().device.id,
    file.config().device.id,
    "the identity is stable"
  );
}

#[cfg(unix)]
#[test]
fn the_written_file_is_readable_only_by_its_owner() {
  use std::os::unix::fs::PermissionsExt;

  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("HiveMe.json");
  ConfigFile::load_or_create(&path).unwrap();
  let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
  assert_eq!(mode, 0o600, "the config holds a password in plain text");
}

#[test]
fn a_config_without_an_identity_is_given_one() {
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("HiveMe.json");
  std::fs::write(&path, r#"{ "version": 1, "broker": { "url": "mqtt://localhost" } }"#).unwrap();

  let (file, written) = ConfigFile::load_or_create(&path).unwrap();
  assert!(written);
  assert!(!file.config().device.id.is_empty());
  // The key that was already there is untouched.
  assert_eq!(file.config().broker.url, "mqtt://localhost");
}

#[test]
fn a_valid_config_passes_validation() {
  valid().validate().expect("the baseline config should be valid");
}

/// One way to break a valid config, and the text the report should mention.
struct BrokenCase {
  what: &'static str,
  break_it: Box<dyn Fn(&mut Config)>,
  expected: &'static str,
}

fn case(what: &'static str, break_it: impl Fn(&mut Config) + 'static, expected: &'static str) -> BrokenCase {
  BrokenCase {
    what,
    break_it: Box::new(break_it),
    expected,
  }
}

#[test]
fn a_broker_url_copied_from_the_console_needs_no_scheme_added_to_it() {
  // The console shows a cluster as a host and a port, and both applications read one
  // that way as TLS MQTT, so the string that is copied is the string that is pasted,
  // into the config file, into a setup string, or into the Settings form of hmg.
  let mut config = valid();
  config.broker.url = "abc.s1.eu.hivemq.cloud:8883".to_owned();

  config.validate().expect("a URL with no scheme is a URL");

  let url = hiveme_core::config::BrokerUrl::parse(&config.broker.url).unwrap();
  assert_eq!(url.scheme, hiveme_core::config::Scheme::Mqtts);
  assert_eq!(url.port, 8883);
}

#[test]
fn every_validation_rule_rejects_its_own_mistake() {
  let cases: Vec<BrokenCase> = vec![
    case("an empty device id", |c: &mut Config| c.device.id.clear(), "device.id"),
    case(
      "a broker URL with a port that is not one",
      |c: &mut Config| c.broker.url = "abc.hivemq.cloud:not-a-port".to_owned(),
      "broker.url",
    ),
    case(
      "plain MQTT against HiveMQ Cloud",
      |c: &mut Config| c.broker.url = "mqtt://abc.s1.eu.hivemq.cloud:1883".to_owned(),
      "TLS only",
    ),
    case(
      "an empty username",
      |c: &mut Config| c.broker.username.clear(),
      "broker.username",
    ),
    case(
      "no password anywhere",
      |c: &mut Config| c.broker.password.clear(),
      "broker.password",
    ),
    case(
      "a backoff that shrinks",
      |c: &mut Config| c.broker.reconnect.initial_delay_ms = c.broker.reconnect.max_delay_ms + 1,
      "initialDelayMs",
    ),
    case(
      "a prefix with a leading slash",
      |c: &mut Config| c.topics.prefix = "/hiveme".to_owned(),
      "topics.prefix",
    ),
    case(
      "a wildcard in the default topic",
      |c: &mut Config| c.topics.default = "info/#".to_owned(),
      "topics.default",
    ),
    case(
      "no subscriptions",
      |c: &mut Config| c.topics.subscriptions.clear(),
      "topics.subscriptions",
    ),
    case(
      "a malformed subscription filter",
      |c: &mut Config| c.topics.subscriptions = vec![Subscription::Relative("a/#/b".to_owned())],
      "topics.subscriptions",
    ),
    case("a QoS above 2", |c: &mut Config| c.publish.qos = 3, "publish.qos"),
    case(
      "two rules with one id",
      |c: &mut Config| {
        let first = c.notifications.rules[0].clone();
        c.notifications.rules.push(first);
      },
      "share the id",
    ),
    case(
      "a rule with an empty id",
      |c: &mut Config| c.notifications.rules[0].id.clear(),
      "empty id",
    ),
    case(
      "a malformed rule filter",
      |c: &mut Config| c.notifications.rules[0].topic = "in+fo".to_owned(),
      "rules['info'].topic",
    ),
    case(
      "encryption on with no active key",
      |c: &mut Config| c.encryption.mode = EncryptionMode::Required,
      "Active",
    ),
    case(
      "encryption on with two active keys",
      |c: &mut Config| {
        c.encryption.mode = EncryptionMode::Opportunistic;
        c.encryption.keys = vec![key("k1", KeyState::Active), key("k2", KeyState::Active)];
      },
      "Active",
    ),
    case(
      "two keys with one kid",
      |c: &mut Config| c.encryption.keys = vec![key("k1", KeyState::Active), key("k1", KeyState::Retired)],
      "share the kid",
    ),
    case(
      "a REST base URL that is not HTTPS",
      |c: &mut Config| {
        c.cloud_api = Some(hiveme_core::config::CloudApi {
          base_url: "http://api.example.com".to_owned(),
          ..hiveme_core::config::CloudApi::default()
        });
      },
      "cloudApi.baseUrl",
    ),
    case(
      "an organization id of the wrong length",
      |c: &mut Config| {
        c.cloud_api = Some(hiveme_core::config::CloudApi {
          org_id: "toolong".to_owned(),
          ..hiveme_core::config::CloudApi::default()
        });
      },
      "cloudApi.orgId",
    ),
  ];

  for BrokenCase {
    what,
    break_it,
    expected,
  } in cases
  {
    let mut config = valid();
    break_it(&mut config);
    let error = config.validate().unwrap_err();
    let text = error.to_string();
    assert!(text.contains(expected), "{what}: '{text}' should mention '{expected}'");
  }
}

fn key(kid: &str, state: KeyState) -> KeyEntry {
  KeyEntry {
    kid: kid.to_owned(),
    alg: "A256GCM".to_owned(),
    secret: "c2VjcmV0".to_owned(),
    created_at: "2026-09-12".to_owned(),
    state,
  }
}

#[test]
fn every_problem_is_reported_at_once() {
  let error = load("invalid.json").config().validate().unwrap_err();
  let Error::ConfigInvalid(issues) = &error else {
    panic!("expected a validation error, got {error}");
  };
  assert!(
    issues.len() >= 10,
    "a user should see every problem at once, got {issues:#?}"
  );
  let text = error.to_string();
  for expected in [
    "device.id",
    "broker.url",
    "topics.prefix",
    "publish.qos",
    "share the id",
  ] {
    assert!(text.contains(expected), "'{text}' should mention '{expected}'");
  }
}

#[test]
fn an_encryption_key_is_only_required_when_encryption_is_on() {
  let mut config = valid();
  config.encryption.keys = vec![key("k1", KeyState::Retired)];
  config.validate().expect("keys may sit unused while the mode is Off");
}

#[test]
fn the_password_comes_from_the_environment_first() {
  temporarily_set(PASSWORD_VARIABLE, Some("from-the-environment"), || {
    let config = valid();
    assert_eq!(config.broker.resolved_password().unwrap(), "from-the-environment");
  });
}

#[test]
fn the_password_can_name_its_own_variable() {
  let mut config = valid();
  config.broker.password_ref = Some(SecretRef::Env {
    name: "HIVEME_TEST_PASSWORD_REF".to_owned(),
  });
  temporarily_set("HIVEME_TEST_PASSWORD_REF", Some("from-the-named-variable"), || {
    assert_eq!(config.broker.resolved_password().unwrap(), "from-the-named-variable");
  });
  temporarily_set("HIVEME_TEST_PASSWORD_REF", None, || {
    assert!(matches!(
      config.broker.resolved_password(),
      Err(Error::PasswordUnavailable(_))
    ));
  });
}

#[test]
fn the_keychain_is_honest_about_not_being_implemented() {
  let mut config = valid();
  config.broker.password_ref = Some(SecretRef::Keychain {
    service: "HiveMe".to_owned(),
    account: "hiveme-sam".to_owned(),
  });
  let error = config.broker.resolved_password().unwrap_err();
  assert!(matches!(error, Error::NotImplemented(_)), "got {error}");
  assert!(error.to_string().contains("phase 6"));
}

#[test]
fn a_redacted_config_hides_every_secret() {
  // Distinctive sentinels, so that finding one in the output cannot be a false alarm
  // from a key name such as "secret".
  let mut config = valid();
  config.broker.password = "sentinel-broker-password".to_owned();
  config.encryption.keys = vec![KeyEntry {
    secret: "sentinel-encryption-secret".to_owned(),
    ..key("k1", KeyState::Active)
  }];
  config.cloud_api = Some(hiveme_core::config::CloudApi {
    token: "sentinel-rest-token".to_owned(),
    ..hiveme_core::config::CloudApi::default()
  });

  let redacted = config.redacted();
  assert_eq!(redacted.broker.password, REDACTED);
  assert_eq!(redacted.encryption.keys[0].secret, REDACTED);
  assert_eq!(redacted.cloud_api.as_ref().unwrap().token, REDACTED);

  let text = serde_json::to_string(&redacted).unwrap();
  for secret in [
    "sentinel-broker-password",
    "sentinel-encryption-secret",
    "sentinel-rest-token",
  ] {
    assert!(!text.contains(secret), "'{secret}' leaked into {text}");
  }
  // The original is untouched.
  assert_eq!(config.broker.password, "sentinel-broker-password");
}

#[test]
fn topics_resolve_against_the_prefix() {
  let config = valid();
  assert_eq!(config.default_topic(), "hiveme/info");
  assert_eq!(config.resolve_topic("build/done", false), "hiveme/build/done");
  assert_eq!(config.resolve_topic("$SYS/uptime", true), "$SYS/uptime");
  assert_eq!(config.subscription_filters(), vec!["hiveme/#".to_owned()]);
}

#[test]
fn a_dollar_subscription_skips_the_prefix() {
  let mut config = valid();
  config.topics.subscriptions = vec![
    Subscription::Relative("#".to_owned()),
    Subscription::Relative("$SYS/#".to_owned()),
    Subscription::Explicit {
      filter: "other/#".to_owned(),
      absolute: true,
    },
  ];
  assert_eq!(config.subscription_filters(), vec!["hiveme/#", "$SYS/#", "other/#"]);
}

#[test]
fn the_two_applications_get_different_client_identifiers() {
  let config = valid();
  let gui = config.client_id("hmg", false);
  let cli = config.client_id("hmc", true);
  assert_eq!(gui, "hiveme-hmg-0f7a1c2e");
  assert!(cli.starts_with("hiveme-hmc-0f7a1c2e-"), "{cli}");
  assert_ne!(cli, config.client_id("hmc", true), "each run gets its own identifier");
  assert_eq!(gui, config.client_id("hmg", false), "the GUI identifier is stable");
}

#[test]
fn an_unknown_enum_value_falls_back_instead_of_breaking_the_file() {
  let text = r#"{ "version": 1, "gui": { "theme": "Sunrise", "displayMode": "Neon" } }"#;
  let config = ConfigFile::from_text(Path::new("HiveMe.json"), text)
    .unwrap()
    .into_config();
  assert_eq!(config.gui.theme, Theme::Ocean);
  assert_eq!(config.gui.display_mode, hiveme_core::config::DisplayMode::Auto);
}

#[test]
fn a_rule_written_the_short_way_takes_the_documented_defaults() {
  let text = r#"{ "version": 1, "notifications": { "rules": [ { "id": "x", "topic": "x" } ] } }"#;
  let config = ConfigFile::from_text(Path::new("HiveMe.json"), text)
    .unwrap()
    .into_config();
  let rule: &Rule = &config.notifications.rules[0];
  assert!(rule.enabled);
  assert!(!rule.absolute);
  assert_eq!(rule.level, Level::Info);
  assert_eq!(rule.title, hiveme_core::config::DEFAULT_RULE_TITLE);
  assert_eq!(rule.body, hiveme_core::config::DEFAULT_RULE_BODY);
  assert!(rule.matches.is_none());
}

/// Sets an environment variable for the duration of `body`.
///
/// The tests that need this are serialized by a mutex, because the environment is
/// shared by every test in the binary.
fn temporarily_set(name: &str, value: Option<&str>, body: impl FnOnce()) {
  static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
  let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
  let previous = std::env::var(name).ok();
  // Safety: the mutex keeps the tests in this binary from racing on the environment.
  unsafe {
    match value {
      Some(value) => std::env::set_var(name, value),
      None => std::env::remove_var(name),
    }
  }
  body();
  unsafe {
    match previous {
      Some(previous) => std::env::set_var(name, previous),
      None => std::env::remove_var(name),
    }
  }
}
