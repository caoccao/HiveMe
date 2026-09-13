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

//! The setup string that carries a cluster from `hmg` to `hmc`.
//!
//! A HiveMQ Cloud cluster is set up in the GUI, which is where a user has the console
//! open and can paste a URL and credentials into fields. `hmc` has no such place, and
//! asking a user to retype three values into a config file by hand invites a typo in
//! the one that is hardest to check.
//!
//! So `hmg` renders this type as one line of JSON, the user copies it, and
//! `hmc --init '<json>'` turns it back into a config. The same type is both ends of
//! that, and `schemas/broker-init.schema.json` is generated from it, so the two
//! applications cannot disagree about the shape.
//!
//! The string carries a password in plain text. It is a credential, and
//! `docs/specs/cli.md` says so where a user will read it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

use super::{Config, ConfigFile, REDACTED};
use crate::error::{Error, Result};

/// The version of the setup string this build writes.
///
/// A reader refuses a version it does not know rather than guessing at fields, because
/// a half understood broker is worse than a clear error.
pub const BROKER_INIT_VERSION: u32 = 1;

/// What applying a setup string did to the shared config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitOutcome {
  Unchanged,
  Updated,
  Created,
}

impl ConfigFile {
  /// Applies setup values through the shared config model and writer.
  ///
  /// Existing files receive serde defaults in memory without writing them back.
  /// Matching values leave the file untouched; updates preserve all other JSON values.
  pub fn initialize(path: &Path, setup: &BrokerInit) -> Result<(Self, InitOutcome)> {
    setup.validate()?;
    let (mut file, created) = match Self::load(path) {
      Ok(file) => (file, false),
      Err(Error::ConfigNotFound(_)) => (Self::new(path), true),
      Err(error) => return Err(error),
    };
    let mut config = file.config.clone();
    setup.apply_to(&mut config);
    if !created && config == file.config {
      return Ok((file, InitOutcome::Unchanged));
    }
    if file.is_read_only() {
      return Err(Error::ConfigMigrate {
        path: path.to_path_buf(),
        reason: "the existing config is newer and will not be overwritten".to_owned(),
      });
    }

    if created {
      config.validate()?;
      file.set_config(config);
      file.save()?;
      return Ok((file, InitOutcome::Created));
    }

    // Compare the same shared model on both sides, then patch only changed fields.
    // Untouched fields retain their original JSON, including unknown enum values
    // and extra keys inside arrays. Existing unrelated settings are not validated
    // here; both apps validate the complete config before connecting.
    let typed = |config: &Config| {
      serde_json::to_value(config).map_err(|source| Error::ConfigParse {
        path: path.to_path_buf(),
        source,
      })
    };
    let mut document = file.document.clone();
    merge_changes(&mut document, &typed(&file.config)?, typed(&config)?);
    file.write_document(document)?;
    file.set_config(config);
    Ok((file, InitOutcome::Updated))
  }
}

/// Replaces only values that changed in the typed config, leaving other JSON intact.
fn merge_changes(target: &mut Value, before: &Value, after: Value) {
  if before == &after {
    return;
  }
  match (before, after) {
    (Value::Object(before), Value::Object(after)) => {
      if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
      }
      let target = target.as_object_mut().expect("the target is an object");
      for (key, value) in after {
        let previous = before.get(&key).unwrap_or(&Value::Null);
        if previous != &value {
          merge_changes(target.entry(key).or_insert(Value::Null), previous, value);
        }
      }
    }
    (_, after) => *target = after,
  }
}

/// Everything `hmc` needs to reach the cluster `hmg` is already talking to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct BrokerInit {
  /// The format version. See [`BROKER_INIT_VERSION`].
  pub v: u32,
  /// `<cluster>.s1.eu.hivemq.cloud:8883` for a HiveMQ Cloud cluster, as the console
  /// shows it. A URL that names no scheme is read as TLS MQTT.
  pub url: String,
  pub username: String,
  /// Plain text, because the CONNECT packet needs it in plain text.
  pub password: String,
  /// The topic namespace, carried so that `hmc` publishes where `hmg` is listening.
  ///
  /// Absent means the receiving config keeps the prefix it already has, which is what
  /// a string written by a build that predates this field looks like.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub prefix: Option<String>,
}

impl Default for BrokerInit {
  fn default() -> Self {
    Self {
      v: BROKER_INIT_VERSION,
      url: String::new(),
      username: String::new(),
      password: String::new(),
      prefix: None,
    }
  }
}

impl BrokerInit {
  /// The setup string for the cluster `config` is pointed at.
  ///
  /// This is the `hmg` end: the Settings tab renders [`BrokerInit::to_json`] of this
  /// for the user to copy.
  pub fn from_config(config: &Config) -> Self {
    Self {
      v: BROKER_INIT_VERSION,
      url: config.broker.url.clone(),
      username: config.broker.username.clone(),
      password: config.broker.password.clone(),
      prefix: Some(config.topics.prefix.clone()),
    }
  }

  /// Reads a setup string.
  ///
  /// Whitespace and a single pair of wrapping quotes are tolerated, because the string
  /// arrives through a clipboard and a shell, and both are good at adding them.
  pub fn parse(text: &str) -> Result<Self> {
    let trimmed = unwrap_quotes(text.trim());
    if trimmed.is_empty() {
      return Err(Error::BrokerInit("it is empty".to_owned()));
    }
    let init: Self = serde_json::from_str(trimmed)
      .map_err(|source| Error::BrokerInit(format!("it is not the expected JSON: {source}")))?;
    if init.v > BROKER_INIT_VERSION {
      return Err(Error::BrokerInit(format!(
        "it is version {} and this build of HiveMe reads version {BROKER_INIT_VERSION}; \
         update HiveMe, or copy the string again from a matching version of hmg",
        init.v
      )));
    }
    init.validate()?;
    Ok(init)
  }

  /// One line of JSON, which is what a user has to copy.
  pub fn to_json(&self) -> String {
    // Compact rather than pretty: it crosses a clipboard and a shell argument, where a
    // newline is a hazard and indentation is noise.
    serde_json::to_string(self).expect("a broker init string serializes")
  }

  /// Rejects a string the applications could not act on.
  pub fn validate(&self) -> Result<()> {
    let mut issues = Vec::new();
    match super::BrokerUrl::parse(&self.url) {
      Ok(url) => {
        if url.is_hivemq_cloud() && !url.scheme.is_secure() {
          issues.push(format!(
            "url uses {} but HiveMQ Cloud accepts TLS only, use mqtts",
            url.scheme
          ));
        }
      }
      Err(reason) => issues.push(format!("url: {reason}")),
    }
    if self.username.trim().is_empty() {
      issues.push("username is empty".to_owned());
    }
    if self.password.is_empty() {
      issues.push("password is empty".to_owned());
    }
    if let Some(prefix) = self.prefix.as_deref()
      && let Err(reason) = crate::topic::validate_prefix(prefix)
    {
      issues.push(format!("prefix: {reason}"));
    }
    if issues.is_empty() {
      Ok(())
    } else {
      Err(Error::BrokerInit(issues.join("; ")))
    }
  }

  /// Writes this cluster into `config`, leaving everything else alone.
  ///
  /// Only the fields the string carries are touched, so a `hmc` that has been running
  /// for a while keeps its device identity, its rules, and anything a newer build put
  /// in the file.
  pub fn apply_to(&self, config: &mut Config) {
    config.broker.url = self.url.clone();
    config.broker.username = self.username.clone();
    config.broker.password = self.password.clone();
    if let Some(prefix) = self.prefix.as_deref() {
      config.topics.prefix = prefix.to_owned();
    }
  }

  /// A copy safe to log or to put in a bug report.
  pub fn redacted(&self) -> Self {
    Self {
      password: if self.password.is_empty() {
        String::new()
      } else {
        REDACTED.to_owned()
      },
      ..self.clone()
    }
  }
}

/// Strips one pair of matching quotes, which a shell or a paste often leaves behind.
fn unwrap_quotes(text: &str) -> &str {
  for quote in ['"', '\''] {
    if let Some(inner) = text.strip_prefix(quote).and_then(|rest| rest.strip_suffix(quote)) {
      // Only when the result still looks like the JSON object it should be, so that a
      // legitimately quoted string is not mangled.
      if inner.trim_start().starts_with('{') {
        return inner;
      }
    }
  }
  text
}

/// The JSON schema of the setup string, written to `schemas/broker-init.schema.json`.
pub fn json_schema() -> serde_json::Value {
  let mut schema = serde_json::to_value(schemars::schema_for!(BrokerInit)).expect("the schema serializes");
  if let Some(object) = schema.as_object_mut() {
    object.insert(
      "$id".to_owned(),
      serde_json::Value::String("https://github.com/caoccao/HiveMe/schemas/broker-init.schema.json".to_owned()),
    );
    object.insert(
      "title".to_owned(),
      serde_json::Value::String("HiveMe broker setup string".to_owned()),
    );
  }
  schema
}

#[cfg(test)]
mod tests {
  use super::*;

  fn cloud() -> BrokerInit {
    BrokerInit {
      v: BROKER_INIT_VERSION,
      url: "mqtts://abc123.s1.eu.hivemq.cloud:8883".to_owned(),
      username: "hiveme-sam".to_owned(),
      password: "s3cret".to_owned(),
      prefix: Some("hiveme".to_owned()),
    }
  }

  #[test]
  fn initialization_creates_the_complete_shared_config_with_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested/HiveMe.json");
    let setup = cloud();
    let (created, outcome) = ConfigFile::initialize(&path, &setup).unwrap();
    assert_eq!(outcome, InitOutcome::Created);

    let (gui, written) = ConfigFile::load_or_create(&path).unwrap();
    assert!(!written);
    assert_eq!(created.config(), gui.config());
    let mut expected = Config::new_for_this_device();
    expected.device = gui.config().device.clone();
    setup.apply_to(&mut expected);
    assert_eq!(gui.config(), &expected);
    assert!(!expected.device.id.is_empty());
    let document: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(document, serde_json::to_value(expected).unwrap());
  }

  #[test]
  fn matching_setup_leaves_sparse_existing_config_and_metadata_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    let original = r#"{ "broker": { "url": "mqtts://abc123.s1.eu.hivemq.cloud:8883", "username": "hiveme-sam", "password": "s3cret" }, "gui": { "theme": "FutureTheme" } }"#;
    std::fs::write(&path, original).unwrap();
    // Missing version, device, and topic fields must not cause an automatic write.
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let (_, outcome) = ConfigFile::initialize(&path, &cloud()).unwrap();
    assert_eq!(outcome, InitOutcome::Unchanged);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), modified);
  }

  #[test]
  fn initialization_changes_only_supplied_values_that_differ() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    let mut document = serde_json::json!({
      "broker": { "url": "mqtts://abc123.s1.eu.hivemq.cloud:8883", "username": "hiveme-sam", "password": "old", "extra": 42 },
      "device": { "id": "kept", "name": "custom name", "extra": "kept" },
      "gui": { "theme": "FutureTheme", "displayMode": "Dark", "language": "ja" },
      "notifications": { "rules": [{ "id": "custom", "topic": "#", "extra": "kept" }] },
      "publish": { "qos": 2 },
      "future": { "kept": true }
    });
    std::fs::write(&path, serde_json::to_string(&document).unwrap()).unwrap();
    let mut setup = cloud();
    setup.prefix = Some("team".to_owned());
    let (file, outcome) = ConfigFile::initialize(&path, &setup).unwrap();
    assert_eq!(outcome, InitOutcome::Updated);
    document["broker"]["password"] = Value::from("s3cret");
    document["topics"] = serde_json::json!({ "prefix": "team" });
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
      written, document,
      "unrelated values and omitted defaults must stay intact"
    );
    assert_eq!(file.config().topics.prefix, "team");
    assert_eq!(file.config().publish.qos, 2);

    // Older setup strings omit the prefix, so a different password must not reset it.
    setup.prefix = None;
    setup.password = "changed again".to_owned();
    let (file, outcome) = ConfigFile::initialize(&path, &setup).unwrap();
    assert_eq!(outcome, InitOutcome::Updated);
    assert_eq!(file.config().topics.prefix, "team");
    document["broker"]["password"] = Value::from("changed again");
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(written, document);
  }

  #[test]
  fn invalid_setup_does_not_create_or_modify_a_config() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    let mut setup = cloud();
    setup.password.clear();
    assert!(ConfigFile::initialize(&path, &setup).is_err());
    assert!(!path.exists());
    let original = r#"{ "version": 1, "gui": { "language": "ja" } }"#;
    std::fs::write(&path, original).unwrap();
    assert!(ConfigFile::initialize(&path, &setup).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
  }

  #[test]
  fn unreadable_and_newer_configs_are_not_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    for original in ["{ broken", r#"{ "version": 99 }"#] {
      std::fs::write(&path, original).unwrap();
      assert!(ConfigFile::initialize(&path, &cloud()).is_err());
      assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }
    let (mut gui, _) = ConfigFile::load_or_create(&directory.path().join("gui.json")).unwrap();
    cloud().apply_to(gui.config_mut());
    let mut document = serde_json::to_value(gui.config()).unwrap();
    document["version"] = Value::from(99);
    let original = serde_json::to_string(&document).unwrap();
    std::fs::write(&path, &original).unwrap();
    let (_, outcome) = ConfigFile::initialize(&path, &cloud()).unwrap();
    assert_eq!(outcome, InitOutcome::Unchanged);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
  }

  #[test]
  fn the_string_round_trips_through_json() {
    let init = cloud();
    assert_eq!(BrokerInit::parse(&init.to_json()).unwrap(), init);
  }

  #[test]
  fn the_string_is_one_line_so_it_can_be_copied() {
    assert!(!cloud().to_json().contains('\n'));
  }

  #[test]
  fn hmg_produces_what_hmc_reads() {
    let mut config = Config::default();
    config.broker.url = "mqtts://abc123.s1.eu.hivemq.cloud:8883".to_owned();
    config.broker.username = "hiveme-sam".to_owned();
    config.broker.password = "s3cret".to_owned();
    config.topics.prefix = "team".to_owned();

    let init = BrokerInit::from_config(&config);
    let parsed = BrokerInit::parse(&init.to_json()).unwrap();

    let mut fresh = Config::new_for_this_device();
    parsed.apply_to(&mut fresh);
    assert_eq!(fresh.broker.url, config.broker.url);
    assert_eq!(fresh.broker.username, config.broker.username);
    assert_eq!(fresh.broker.password, config.broker.password);
    assert_eq!(fresh.topics.prefix, "team");
  }

  #[test]
  fn applying_a_string_leaves_the_rest_of_the_config_alone() {
    let mut config = Config::new_for_this_device();
    let device = config.device.clone();
    config.publish.qos = 2;
    config.gui.theme = super::super::Theme::Indigo;
    config.notifications.rules.truncate(1);

    cloud().apply_to(&mut config);
    assert_eq!(config.device, device, "the device identity must survive an init");
    assert_eq!(config.publish.qos, 2);
    assert_eq!(config.gui.theme, super::super::Theme::Indigo);
    assert_eq!(config.notifications.rules.len(), 1);
  }

  #[test]
  fn a_string_without_a_prefix_keeps_the_one_the_config_has() {
    let mut config = Config::default();
    config.topics.prefix = "team".to_owned();
    let init = BrokerInit {
      prefix: None,
      ..cloud()
    };
    init.apply_to(&mut config);
    assert_eq!(config.topics.prefix, "team");
  }

  #[test]
  fn the_shell_and_the_clipboard_are_forgiven() {
    let init = cloud();
    let json = init.to_json();
    for text in [
      format!("  {json}  "),
      format!("'{json}'"),
      format!("\"{json}\""),
      format!("\n{json}\n"),
    ] {
      assert_eq!(BrokerInit::parse(&text).unwrap(), init, "{text}");
    }
  }

  #[test]
  fn a_string_that_is_not_json_says_so() {
    let error = BrokerInit::parse("not json").unwrap_err();
    assert!(error.to_string().contains("not the expected JSON"), "{error}");
    assert!(BrokerInit::parse("").unwrap_err().to_string().contains("empty"));
  }

  #[test]
  fn a_string_from_a_newer_build_is_refused_rather_than_guessed_at() {
    let json = r#"{"v":2,"url":"mqtts://a.s1.eu.hivemq.cloud:8883","username":"u","password":"p"}"#;
    let error = BrokerInit::parse(json).unwrap_err();
    assert!(error.to_string().contains("version 2"), "{error}");
  }

  #[test]
  fn every_missing_piece_is_reported_at_once() {
    let json = r#"{"v":1,"url":"","username":"","password":""}"#;
    let error = BrokerInit::parse(json).unwrap_err().to_string();
    assert!(error.contains("url"), "{error}");
    assert!(error.contains("username is empty"), "{error}");
    assert!(error.contains("password is empty"), "{error}");
  }

  #[test]
  fn a_cloud_cluster_over_plain_mqtt_is_refused() {
    let init = BrokerInit {
      url: "mqtt://abc123.s1.eu.hivemq.cloud:1883".to_owned(),
      ..cloud()
    };
    let error = init.validate().unwrap_err().to_string();
    assert!(error.contains("TLS only"), "{error}");
  }

  #[test]
  fn a_local_broker_over_plain_mqtt_is_allowed() {
    let init = BrokerInit {
      url: "mqtt://localhost:1883".to_owned(),
      ..cloud()
    };
    assert!(init.validate().is_ok());
  }

  #[test]
  fn a_prefix_with_a_wildcard_is_refused() {
    let init = BrokerInit {
      prefix: Some("team/#".to_owned()),
      ..cloud()
    };
    assert!(init.validate().unwrap_err().to_string().contains("prefix"));
  }

  #[test]
  fn the_password_is_hidden_when_the_string_is_logged() {
    let redacted = cloud().redacted();
    assert_eq!(redacted.password, REDACTED);
    assert_eq!(redacted.url, cloud().url);
    assert!(!redacted.to_json().contains("s3cret"));
  }

  #[test]
  fn the_schema_describes_the_setup_string() {
    let schema = json_schema();
    assert!(schema.get("$id").is_some());
    assert_eq!(schema["type"], "object");
    let properties = schema["properties"].as_object().expect("properties");
    for field in ["v", "url", "username", "password", "prefix"] {
      assert!(properties.contains_key(field), "{field} is missing from the schema");
    }
  }
}
