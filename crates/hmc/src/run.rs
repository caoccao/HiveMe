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

//! One `hmc` run: read the config, build the message, publish it, leave.
//!
//! The behavior is specified in `docs/specs/cli.md`. Everything that could also be
//! useful to `hmg` lives in `hiveme-core`, so this file only decides what the command
//! line asked for.
//!
//! What this file prints is in the language of the config it read, see "Languages" in
//! `docs/specs/cli.md`. What `hiveme-core` reports, a broker refusal or a validation
//! message, stays English, as `hmg` shows it.

use std::path::Path;

use hiveme_core::config::{BrokerInit, Config, ConfigFile, InitOutcome, config_path};
use hiveme_core::i18n::{Locale, t_with};
#[cfg(test)]
use hiveme_core::message::Level;
use hiveme_core::message::{Message, build_envelope, raw_json_payload, raw_json_properties};
use hiveme_core::mqtt::{MqttClient, Qos, Role};

use crate::cli::Cli;
use crate::failure::{Failure, Result};

/// The name `hmc` puts in the `sender.app` field of every message it writes.
const APP: &str = "hmc";

/// The language of the config `hmc` is about to read, before anything has read it.
///
/// This is what the help, the version, and a usage error found before the config is
/// loaded are written in. Nothing is written: a missing or unreadable config means
/// English, and the run itself reports what is wrong with it.
pub fn configured_locale(explicit: Option<&Path>) -> (Locale, Option<ConfigFile>) {
  let file = config_path(explicit).and_then(|path| ConfigFile::load(&path)).ok();
  let locale = file
    .as_ref()
    .map(|file| Locale::resolve(&file.config().gui.language))
    .unwrap_or_default();
  (locale, file)
}

/// Publishes one message, or writes the config and stops.
///
/// `locale` is [`configured_locale`], used until the config is read; from then on the
/// language is the one in the config that was read.
pub async fn run(cli: Cli, locale: Locale, cached: Option<ConfigFile>) -> Result<()> {
  if let Some(setup) = cli.init.as_deref() {
    return init(&cli, setup);
  }

  let body = cli.body(locale)?;
  let config = load_config(&cli, locale, cached)?;
  let locale = Locale::resolve(&config.gui.language);

  // Encryption is designed in docs/specs/message.md but not implemented, and a config
  // that asks for it must not quietly get plaintext on the wire instead.
  if config.encryption.is_enabled() {
    return Err(Failure::Config(t_with(
      locale,
      "cli.encryptionUnavailable",
      &[("mode", &config.encryption.mode.to_string())],
    )));
  }

  let topic = resolve_topic(&cli, &config)?;
  let qos = match cli.qos {
    Some(value) => Qos::from_u8(value).unwrap_or_else(|| Qos::from_config(&config)),
    None => Qos::from_config(&config),
  };
  let retain = cli.retained(config.publish.retain);

  // Everything the user could have got wrong is settled before the network is touched,
  // so that a mistake such as `--json` with input that is not JSON is reported at once
  // rather than after a connection attempt that was never going to help.
  let publication = Publication::build(&cli, &config, &body.text, locale)?;

  let client = MqttClient::connect(&config, Role::Cli).await?;
  log::debug!("publishing to {topic} at qos {qos}, retain {retain}");
  let published = publication.send(&client, &topic, qos, retain).await;
  if published.is_ok() {
    println!("{}", t_with(locale, "cli.sent", &[("topic", &topic)]));
  }

  // The broker is told we are leaving whether or not the publish worked, so that it
  // releases the session at once rather than holding one of the connections a
  // Serverless cluster allows until the keep alive runs out.
  let goodbye = client.disconnect().await;
  published?;
  goodbye?;
  Ok(())
}

/// What this run puts on the topic.
#[derive(Debug, Clone, PartialEq)]
enum Publication {
  /// The HiveMe envelope of `docs/specs/message.md`.
  Envelope(Box<Message>),
  /// The user's own JSON, published unchanged because `--json` was given.
  RawJson(Vec<u8>),
}

impl Publication {
  fn build(cli: &Cli, config: &Config, body: &str, locale: Locale) -> Result<Self> {
    if cli.json {
      Ok(Self::RawJson(json_payload(body, locale)?))
    } else {
      Ok(Self::Envelope(Box::new(build_message(cli, config, body))))
    }
  }

  async fn send(&self, client: &MqttClient, topic: &str, qos: Qos, retain: bool) -> hiveme_core::Result<()> {
    match self {
      Self::Envelope(message) => client.publish_message(topic, message, qos, retain).await,
      Self::RawJson(payload) => {
        client
          .publish(topic, payload.clone(), qos, retain, Some(&raw_json_properties()))
          .await
      }
    }
  }
}

/// Writes the cluster from a setup string into the config, creating the file if needed.
///
/// The string comes from the Broker section of the `hmg` Settings tab, which is where a
/// user has the HiveMQ Cloud console open and can paste a URL and credentials. Copying
/// one line beats retyping three values, and the password is the one that is hardest to
/// notice a typo in.
///
/// The outcome is reported in the language of the config as it was left, which is the
/// language of the setup string when the string carries one.
fn init(cli: &Cli, setup: &str) -> Result<()> {
  let setup = BrokerInit::parse(setup).map_err(|error| Failure::Usage(error.to_string()))?;
  log::debug!("read a setup string for {}", setup.redacted().url);

  let path = config_path(cli.config.as_deref())?;
  let (file, outcome) = ConfigFile::initialize(&path, &setup)?;
  let key = match outcome {
    InitOutcome::Unchanged => "cli.configUnchanged",
    InitOutcome::Updated => "cli.configUpdated",
    InitOutcome::Created => "cli.configCreated",
  };
  let locale = Locale::resolve(&file.config().gui.language);
  println!(
    "{}",
    t_with(locale, key, &[("path", &file.path().display().to_string())])
  );
  Ok(())
}

/// Reads the config, or writes a default one and says so.
fn load_config(cli: &Cli, locale: Locale, cached: Option<ConfigFile>) -> Result<Config> {
  let path = config_path(cli.config.as_deref())?;
  let existed = path.exists();
  let file = match cached.filter(|file| file.path() == path && !file.config().device.id.trim().is_empty()) {
    Some(file) => file,
    None => ConfigFile::load_or_create(&path)?.0,
  };
  if !existed {
    return Err(Failure::Config(t_with(
      locale,
      "cli.configWritten",
      &[("path", &file.path().display().to_string())],
    )));
  }
  log::debug!("read the config from {}", file.path().display());
  Ok(file.into_config())
}

/// The topic under `hiveme` this run publishes to.
fn resolve_topic(cli: &Cli, config: &Config) -> Result<String> {
  let requested = cli.topic.as_deref().unwrap_or("");
  let topic = config.resolve_topic(requested);
  hiveme_core::topic::validate_topic(&topic).map_err(|reason| Failure::Usage(format!("--topic: {reason}")))?;
  Ok(topic)
}

/// The envelope of `docs/specs/message.md`, filled in from the command line.
fn build_message(cli: &Cli, config: &Config, body: &str) -> Message {
  build_envelope(config, APP, body, cli.title.as_deref(), cli.level.as_deref())
}

/// The bytes of a `--json` publish, which are the user's own rather than an envelope.
fn json_payload(text: &str, locale: Locale) -> Result<Vec<u8>> {
  raw_json_payload(text)
    .map_err(|source| Failure::Usage(t_with(locale, "cli.notJson", &[("error", &source.to_string())])))
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::*;

  fn config() -> Config {
    let mut config = Config::default();
    config.device.id = "0f9c2d1e-1111-7000-8000-aaaabbbbcccc".to_owned();
    config.device.name = "sams-macbook".to_owned();
    config
  }

  fn cli(arguments: &[&str]) -> Cli {
    let mut all = vec!["hmc"];
    all.extend_from_slice(arguments);
    Cli::parse_from(all)
  }

  #[test]
  fn no_topic_means_the_prefix_itself() {
    let config = config();
    assert_eq!(resolve_topic(&cli(&["hello"]), &config).unwrap(), "hiveme");
  }

  #[test]
  fn a_topic_is_relative_to_the_prefix() {
    let config = config();
    assert_eq!(
      resolve_topic(&cli(&["-t", "build/nightly", "hello"]), &config).unwrap(),
      "hiveme/build/nightly"
    );
  }

  #[test]
  fn leading_slashes_do_not_bypass_the_prefix() {
    let config = config();
    for input in ["ci", "/ci", "///ci"] {
      assert_eq!(
        resolve_topic(&cli(&["-t", input, "hello"]), &config).unwrap(),
        "hiveme/ci"
      );
    }
    assert_eq!(resolve_topic(&cli(&["-t", "/", "hello"]), &config).unwrap(), "hiveme");
  }

  #[test]
  fn unknown_config_fields_cannot_change_the_root() {
    let config: Config = serde_json::from_value(serde_json::json!({
      "topics": { "prefix": "elsewhere" }
    }))
    .unwrap();
    assert_eq!(resolve_topic(&cli(&["hello"]), &config).unwrap(), "hiveme");
    assert_eq!(
      resolve_topic(&cli(&["-t", "/ci", "hello"]), &config).unwrap(),
      "hiveme/ci"
    );
  }

  #[test]
  fn a_wildcard_in_a_topic_is_the_users_mistake() {
    let config = config();
    let failure = resolve_topic(&cli(&["-t", "build/#", "hello"]), &config).unwrap_err();
    assert_eq!(failure.code(), 2);
    assert!(failure.to_string().contains("--topic"), "{failure}");
  }

  #[test]
  fn the_topic_never_determines_the_payload_level() {
    let config = config();
    for topic in ["info", "warn", "error", "build/nightly"] {
      let cli = cli(&["-t", topic, "hello"]);
      let message = build_message(&cli, &config, "hello");
      assert_eq!(message.level(), Level::Info, "{topic}");
    }
  }

  #[test]
  fn every_level_uses_the_same_default_or_custom_topic() {
    let config = config();
    for level in ["debug", "info", "success", "warn", "error"] {
      for (arguments, expected) in [
        (vec!["--level", level, "hello"], "hiveme"),
        (
          vec!["--level", level, "-t", "build/nightly", "hello"],
          "hiveme/build/nightly",
        ),
      ] {
        let cli = cli(&arguments);
        assert_eq!(resolve_topic(&cli, &config).unwrap(), expected);
        assert_eq!(build_message(&cli, &config, "hello").level(), Level::parse(level));
      }
    }
  }

  #[test]
  fn notification_rules_never_override_the_payload_level() {
    let mut config = config();
    config.notifications.rules[2].enabled = false;
    let cli = cli(&["-t", "error", "hello"]);
    assert_eq!(build_message(&cli, &config, "hello").level(), Level::Info);
  }

  #[test]
  fn the_envelope_carries_this_installation_as_its_sender() {
    let config = config();
    let cli = cli(&["--title", "CI", "the build finished"]);
    let message = build_message(&cli, &config, "the build finished");

    assert_eq!(message.v, hiveme_core::message::ENVELOPE_VERSION);
    assert_eq!(message.kind, hiveme_core::message::DEFAULT_KIND);
    assert!(!message.id.is_empty());
    assert!(message.ts.ends_with('Z'), "{}", message.ts);
    assert!(message.timestamp().is_some());
    assert!(!message.is_encrypted());

    let payload = message.payload.as_ref().unwrap();
    assert_eq!(payload.body, "the build finished");
    assert_eq!(payload.title.as_deref(), Some("CI"));

    let sender = message.sender.as_ref().unwrap();
    assert_eq!(sender.id.as_deref(), Some(config.device.id.as_str()));
    assert_eq!(sender.name.as_deref(), Some("sams-macbook"));
    assert_eq!(sender.app.as_deref(), Some(APP));
    assert_eq!(sender.app_version.as_deref(), Some(hiveme_core::VERSION));
  }

  #[test]
  fn a_message_without_a_title_has_none() {
    let config = config();
    let cli = cli(&["hello"]);
    let message = build_message(&cli, &config, "hello");
    assert!(message.payload.as_ref().unwrap().title.is_none());
  }

  #[test]
  fn every_built_message_is_a_valid_envelope() {
    let config = config();
    let cli = cli(&["hello"]);
    let message = build_message(&cli, &config, "hello");
    assert!(message.validate().is_ok());

    let bytes = message.to_bytes().unwrap();
    match hiveme_core::message::parse(&bytes) {
      hiveme_core::Parsed::Envelope(parsed) => assert_eq!(*parsed, message),
      other => panic!("a message hmc built did not parse as an envelope: {other:?}"),
    }
  }

  #[test]
  fn json_input_is_published_exactly_as_it_was_given() {
    let text = r#"{ "stage": "deploy",  "ok": true }"#;
    assert_eq!(json_payload(text, Locale::EnUs).unwrap(), text.as_bytes());
  }

  #[test]
  fn the_publication_is_settled_before_anything_connects() {
    let config = config();
    assert!(matches!(
      Publication::build(&cli(&["hello"]), &config, "hello", Locale::EnUs).unwrap(),
      Publication::Envelope(_)
    ));
    assert_eq!(
      Publication::build(&cli(&["--json", "{}"]), &config, "{}", Locale::EnUs).unwrap(),
      Publication::RawJson(b"{}".to_vec())
    );
    // The whole point of building first: this never reaches the broker.
    assert_eq!(
      Publication::build(&cli(&["--json", "not json"]), &config, "not json", Locale::EnUs)
        .unwrap_err()
        .code(),
      2
    );
  }

  #[test]
  fn json_input_that_is_not_json_is_a_usage_error() {
    let failure = json_payload("not json", Locale::EnUs).unwrap_err();
    assert_eq!(failure.code(), 2);
    assert!(
      failure
        .line()
        .starts_with("hmc: usage: --json was given input that is not JSON: "),
      "{failure}"
    );
    // The flag keeps its name in every language, and serde's reason stays English.
    let failure = json_payload("not json", Locale::It).unwrap_err();
    assert!(
      failure
        .line()
        .starts_with("hmc: usage: --json ha ricevuto un input che non è JSON: expected"),
      "{failure}"
    );
  }

  #[test]
  fn a_json_scalar_is_still_json() {
    for text in ["1", "\"text\"", "true", "null", "[1,2]"] {
      assert!(json_payload(text, Locale::EnUs).is_ok(), "{text}");
    }
  }

  #[test]
  fn the_language_before_the_config_is_read_is_the_one_inside_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    assert_eq!(
      configured_locale(Some(&path)).0,
      Locale::EnUs,
      "no config means English"
    );
    assert!(!path.exists(), "looking for the language never writes a config");

    std::fs::write(&path, r#"{ "gui": { "language": "zh-Hant" } }"#).unwrap();
    assert_eq!(configured_locale(Some(&path)).0, Locale::ZhTw);

    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(
      configured_locale(Some(&path)).0,
      Locale::EnUs,
      "an unreadable config means English"
    );
  }

  #[test]
  fn a_raw_json_publish_claims_no_envelope_version() {
    let properties = raw_json_properties();
    assert_eq!(properties.content_type, "application/json");
    assert!(properties.user_properties.is_empty());
    assert!(
      !properties
        .user_properties
        .iter()
        .any(|(name, _)| name == hiveme_core::message::USER_PROPERTY_VERSION)
    );
  }
  #[test]
  fn a_cached_config_is_reused_without_reading_it_again() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    std::fs::write(&path, serde_json::to_string(&config()).unwrap()).unwrap();
    let (locale, cached) = configured_locale(Some(&path));
    std::fs::write(&path, "broken JSON after the first read").unwrap();
    let args = cli(&["--config", path.to_str().unwrap(), "hello"]);
    assert_eq!(load_config(&args, locale, cached).unwrap(), config());
  }

  #[test]
  fn explicit_no_retain_overrides_the_config() {
    assert!(!cli(&["--no-retain", "hello"]).retained(true));
    assert!(cli(&["--retain", "hello"]).retained(false));
    assert!(cli(&["hello"]).retained(true));
    assert!(!cli(&["hello"]).retained(false));
    assert!(Cli::try_parse_from(["hmc", "--retain", "--no-retain", "hello"]).is_err());
    let payload = build_message(&cli(&["--title", "  ", "hello"]), &config(), "hello")
      .payload
      .unwrap();
    assert!(payload.title.is_none());
  }
}
