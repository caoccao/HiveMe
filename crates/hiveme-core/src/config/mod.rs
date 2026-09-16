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

//! The config shared by `hmc` and `hmg`.
//!
//! The field reference is `docs/specs/config.md`, and
//! `schemas/config.schema.json` is generated from the types below. Two habits keep
//! the two applications able to share one file:
//!
//! * every struct is `#[serde(default)]`, so a missing key takes its documented
//!   default rather than failing the load, and
//! * a write merges into the document that was read, so a key this build does not
//!   know survives a round trip.

mod file_io;
mod init;
mod migrate;
mod paths;
mod url;

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::message::Level;

pub use init::{BROKER_INIT_VERSION, BrokerInit, InitOutcome};
pub use migrate::{CONFIG_VERSION, MigrationOutcome};
pub use paths::{
  APP_NAME, CONFIG_FILE_NAME, CONFIG_PATH_VARIABLE, DATABASE_FILE_NAME, Os, PathEnv, config_dir_with, config_path,
  config_path_with, database_path,
};
pub use url::{BrokerUrl, BrokerUrlParts, DEFAULT_SCHEME, DEFAULT_WEBSOCKET_PATH, Scheme};

/// The environment variable that overrides the broker password.
pub const PASSWORD_VARIABLE: &str = "HIVEME_PASSWORD";

/// What a redacted secret is replaced with.
pub const REDACTED: &str = "<redacted>";

/// The smallest `broker.keepAliveSecs` the MQTT client accepts.
///
/// `rumqttc` asserts on anything shorter, so a config that asked for one second used to
/// take the application down with it instead of being reported. The value is validated
/// here and clamped again where the connection is built, so that neither a config the
/// user never saved through HiveMe nor a test that skips validation can reach the panic.
pub const MIN_KEEP_ALIVE_SECS: u16 = 5;

/// Generates the parts of a closed config enum that stay the same for all of them.
///
/// The reader is deliberately lenient: a value written by a newer build becomes the
/// default with a warning, rather than making the whole config unreadable. The
/// generated schema still lists the values this build knows, so that editors and the
/// generated TypeScript types stay precise.
macro_rules! config_enum {
  ($name:ident, $default:ident, { $( $variant:ident => $wire:literal ),+ $(,)? }) => {
    impl $name {
      /// The wire form of this value.
      pub fn as_str(&self) -> &'static str {
        match self { $( Self::$variant => $wire, )+ }
      }

      /// Every value this build knows.
      pub fn all() -> &'static [Self] {
        &[ $( Self::$variant, )+ ]
      }

      /// Reads the wire form, returning `None` for a value this build does not know.
      pub fn parse(raw: &str) -> Option<Self> {
        match raw { $( $wire => Some(Self::$variant), )+ _ => None }
      }
    }

    impl Default for $name {
      fn default() -> Self {
        Self::$default
      }
    }

    impl std::fmt::Display for $name {
      fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse(&raw).unwrap_or_else(|| {
          log::warn!(
            "'{raw}' is not a {} this build knows, falling back to {}",
            stringify!($name),
            Self::$default.as_str()
          );
          Self::$default
        }))
      }
    }
  };
}

/// The HiveMe config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
  /// The config schema version. The loader migrates older versions.
  pub version: u32,
  pub device: Device,
  pub broker: Broker,
  pub topics: Topics,
  pub publish: Publish,
  pub notifications: Notifications,
  pub gui: Gui,
  pub update: Update,
  pub encryption: Encryption,
  /// The HiveMQ Cloud REST API, which needs the Starter plan. Phase 6.
  pub cloud_api: Option<CloudApi>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      version: CONFIG_VERSION,
      device: Device::default(),
      broker: Broker::default(),
      topics: Topics::default(),
      publish: Publish::default(),
      notifications: Notifications::default(),
      gui: Gui::default(),
      update: Update::default(),
      encryption: Encryption::default(),
      cloud_api: None,
    }
  }
}

impl Config {
  /// A default config carrying a freshly generated identity for this installation.
  pub fn new_for_this_device() -> Self {
    Self {
      device: Device::new_for_this_device(),
      ..Self::default()
    }
  }

  /// Rejects a config the applications could not act on.
  ///
  /// Loading never validates, so that `hmg` can open an incomplete file and let the
  /// user finish it in the Settings tab. Both applications validate before connecting.
  pub fn validate(&self) -> Result<()> {
    let mut issues = self.issues();
    issues.extend(self.login_issues());
    report(issues)
  }

  /// Rejects a config that could not be written and read back as itself.
  ///
  /// Everything [`Self::validate`] refuses except the broker address and the
  /// credentials, because saving the settings is not connecting with them. A broker
  /// that is half filled in is what the Settings tab looks like until the user has
  /// finished with it, and refusing the save over it would throw away the rest of what
  /// they typed. The connection is opened right afterward and validates in full, so an
  /// address that does not work is still reported, from the place that found out.
  pub fn validate_for_save(&self) -> Result<()> {
    report(self.issues())
  }

  /// Everything wrong with the config that is not about reaching the broker.
  fn issues(&self) -> Vec<String> {
    let mut issues = Vec::new();

    if self.device.id.trim().is_empty() {
      issues.push("device.id is empty".to_owned());
    }

    if self.broker.keep_alive_secs < MIN_KEEP_ALIVE_SECS {
      issues.push(format!(
        "broker.keepAliveSecs is {}, and the MQTT client needs at least {MIN_KEEP_ALIVE_SECS}",
        self.broker.keep_alive_secs
      ));
    }
    if self.broker.reconnect.initial_delay_ms > self.broker.reconnect.max_delay_ms {
      issues.push("broker.reconnect.initialDelayMs is greater than maxDelayMs".to_owned());
    }

    if self.topics.subscriptions.is_empty() {
      issues.push("topics.subscriptions is empty, hmg would receive nothing".to_owned());
    }
    for (index, subscription) in self.topics.subscriptions.iter().enumerate() {
      if let Err(reason) = crate::topic::validate_filter(subscription.filter()) {
        issues.push(format!("topics.subscriptions[{index}]: {reason}"));
      }
    }

    if self.publish.qos > 2 {
      issues.push(format!("publish.qos is {}, expected 0, 1, or 2", self.publish.qos));
    }

    let mut rule_ids = std::collections::HashSet::new();
    for rule in &self.notifications.rules {
      if rule.id.trim().is_empty() {
        issues.push("a notification rule has an empty id".to_owned());
      } else if !rule_ids.insert(rule.id.as_str()) {
        issues.push(format!("two notification rules share the id '{}'", rule.id));
      }
      if let Err(reason) = crate::topic::validate_filter(&rule.topic) {
        issues.push(format!("notifications.rules['{}'].topic: {reason}", rule.id));
      }
    }

    issues.extend(self.encryption.issues());

    if let Some(cloud_api) = self.cloud_api.as_ref() {
      issues.extend(cloud_api.issues());
    }

    issues
  }

  /// What would stop the CONNECT packet being sent at all.
  fn login_issues(&self) -> Vec<String> {
    login_issues(
      &self.broker.url,
      &self.broker.username,
      !self.broker.password.is_empty() || self.broker.password_ref.is_some() || var(PASSWORD_VARIABLE).is_some(),
      "broker.",
    )
  }

  /// A copy safe to log or to show in a bug report.
  pub fn redacted(&self) -> Self {
    let mut copy = self.clone();
    if !copy.broker.password.is_empty() {
      copy.broker.password = REDACTED.to_owned();
    }
    for key in &mut copy.encryption.keys {
      if !key.secret.is_empty() {
        key.secret = REDACTED.to_owned();
      }
    }
    if let Some(cloud_api) = copy.cloud_api.as_mut()
      && !cloud_api.token.is_empty()
    {
      cloud_api.token = REDACTED.to_owned();
    }
    copy
  }

  /// The publish topic under the fixed root, ignoring leading input slashes.
  pub fn resolve_topic(&self, topic: &str) -> String {
    crate::topic::resolve_publish(crate::topic::ROOT_TOPIC, topic)
  }

  /// The absolute topic `hmc` publishes to when no topic is given.
  pub fn default_topic(&self) -> String {
    self.resolve_topic("")
  }

  /// The absolute filters `hmg` and interactive `hmc` subscribe to.
  pub fn subscription_filters(&self) -> Vec<String> {
    self
      .topics
      .subscriptions
      .iter()
      .map(|subscription| subscription.resolve(crate::topic::ROOT_TOPIC))
      .collect()
  }

  /// The MQTT client identifier for `app` on this installation.
  ///
  /// `hmc` adds a random suffix so that concurrent runs, and a running `hmg`, never
  /// collide on the broker, which disconnects duplicate identifiers.
  pub fn client_id(&self, app: &str, unique: bool) -> String {
    let prefix = if self.broker.client_id_prefix.trim().is_empty() {
      "hiveme"
    } else {
      self.broker.client_id_prefix.trim()
    };
    let device: String = self.device.id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let base = format!("{prefix}-{app}-{device}");
    if unique {
      let suffix = uuid::Uuid::now_v7().simple().to_string();
      format!("{base}-{}", &suffix[suffix.len() - 8..])
    } else {
      base
    }
  }
}

/// Shared login validation for settings and portable setup strings.
fn login_issues(url: &str, username: &str, has_password: bool, prefix: &str) -> Vec<String> {
  let mut issues = Vec::new();
  match BrokerUrl::parse(url) {
    Ok(url) if url.is_hivemq_cloud() && !url.scheme.is_secure() => issues.push(format!(
      "{prefix}url uses {} but HiveMQ Cloud accepts TLS only, use mqtts or wss",
      url.scheme
    )),
    Ok(_) => {}
    Err(reason) => issues.push(format!("{prefix}url: {reason}")),
  }
  if username.trim().is_empty() {
    issues.push(format!("{prefix}username is empty"));
  }
  if !has_password {
    issues.push(format!("{prefix}password is empty"));
  }
  issues
}

/// This installation's identity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Device {
  /// A stable identifier for this installation, used as `sender.id`.
  pub id: String,
  /// The human readable name shown in the GUI.
  pub name: String,
}

impl Device {
  /// A freshly generated identity, named after the host.
  pub fn new_for_this_device() -> Self {
    Self {
      id: uuid::Uuid::now_v7().to_string(),
      name: hostname::get()
        .ok()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default(),
    }
  }
}

/// How to reach the MQTT broker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Broker {
  /// `host:8883` for HiveMQ Cloud, with or without the `mqtts://` in front of it: a URL
  /// that names no scheme is read as TLS MQTT. See `docs/specs/hivemq-cloud.md`.
  pub url: String,
  pub username: String,
  /// Stored in plain text. The file is created with mode 0600 on Unix.
  pub password: String,
  /// Where to read the password instead of the field above.
  pub password_ref: Option<SecretRef>,
  /// The first segment of the MQTT client identifier.
  pub client_id_prefix: String,
  pub keep_alive_secs: u16,
  /// `hmg` and interactive `hmc`. One-shot `hmc` uses a clean start and no session.
  pub session_expiry_secs: u32,
  pub connect_timeout_secs: u64,
  pub tls: Tls,
  pub reconnect: Reconnect,
}

impl Default for Broker {
  fn default() -> Self {
    Self {
      url: String::new(),
      username: String::new(),
      password: String::new(),
      password_ref: None,
      client_id_prefix: "hiveme".to_owned(),
      keep_alive_secs: 30,
      session_expiry_secs: 3600,
      connect_timeout_secs: 10,
      tls: Tls::default(),
      reconnect: Reconnect::default(),
    }
  }
}

impl Broker {
  /// The broker URL, parsed.
  pub fn parsed_url(&self) -> std::result::Result<BrokerUrl, String> {
    BrokerUrl::parse(&self.url)
  }

  /// The password to send in the CONNECT packet.
  ///
  /// `HIVEME_PASSWORD` wins, then `passwordRef`, then the stored password. The
  /// environment override is resolved here rather than at load time so that it is
  /// never written back into the file.
  pub fn resolved_password(&self) -> Result<String> {
    if let Some(password) = var(PASSWORD_VARIABLE) {
      return Ok(password);
    }
    match self.password_ref.as_ref() {
      Some(SecretRef::Env { name }) => var(name)
        .ok_or_else(|| Error::PasswordUnavailable(format!("broker.passwordRef names {name}, which is not set"))),
      Some(SecretRef::Keychain { .. }) => Err(Error::NotImplemented(
        "keychain storage for the broker password arrives in phase 6, use broker.password or HIVEME_PASSWORD for now"
          .to_owned(),
      )),
      None => Ok(self.password.clone()),
    }
  }
}

/// Where a secret really lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type")]
pub enum SecretRef {
  /// Read the secret from an environment variable.
  Env { name: String },
  /// Read the secret from the OS keychain. Phase 6.
  Keychain { service: String, account: String },
}

/// TLS settings for the broker connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Tls {
  /// Honored only for hosts outside `hivemq.cloud`, and logs a warning when false.
  pub verify_server: bool,
  /// Extra PEM roots appended to the native trust store.
  pub ca_file: Option<PathBuf>,
}

impl Default for Tls {
  fn default() -> Self {
    Self {
      verify_server: true,
      ca_file: None,
    }
  }
}

/// Exponential backoff for `hmg` and interactive `hmc`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Reconnect {
  pub initial_delay_ms: u64,
  pub max_delay_ms: u64,
}

impl Default for Reconnect {
  fn default() -> Self {
    Self {
      initial_delay_ms: 1_000,
      max_delay_ms: 30_000,
    }
  }
}

/// Subscriptions under the fixed `hiveme` root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Topics {
  /// What `hmg` and interactive `hmc` subscribe to, relative to `hiveme`.
  pub subscriptions: Vec<Subscription>,
}

impl Default for Topics {
  fn default() -> Self {
    Self {
      subscriptions: vec![Subscription::Relative("#".to_owned())],
    }
  }
}

/// One entry of `topics.subscriptions`.
///
/// A bare string is relative to `hiveme`, unless it starts with `$`, which the MQTT
/// specification reserves for broker topics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Subscription {
  /// A filter relative to `hiveme`.
  Relative(String),
  /// A filter that says for itself whether the prefix applies.
  Explicit {
    filter: String,
    #[serde(default)]
    absolute: bool,
  },
}

impl Subscription {
  /// The filter as written.
  pub fn filter(&self) -> &str {
    match self {
      Self::Relative(filter) => filter,
      Self::Explicit { filter, .. } => filter,
    }
  }

  /// Whether the prefix is skipped.
  pub fn is_absolute(&self) -> bool {
    match self {
      Self::Relative(filter) => filter.starts_with('$'),
      Self::Explicit { filter, absolute } => *absolute || filter.starts_with('$'),
    }
  }

  /// The filter to send in the SUBSCRIBE packet.
  pub fn resolve(&self, prefix: &str) -> String {
    crate::topic::resolve(prefix, self.filter(), self.is_absolute())
  }
}

/// Publishing defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Publish {
  /// 0, 1, or 2.
  pub qos: u8,
  pub retain: bool,
  /// How long `hmc` waits for the acknowledgement.
  pub timeout_secs: u64,
}

impl Default for Publish {
  fn default() -> Self {
    Self {
      qos: 1,
      retain: false,
      timeout_secs: 10,
    }
  }
}

/// The rule based desktop notification system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Notifications {
  /// Raise OS notifications.
  pub enabled: bool,
  /// Raise a single topmost window, replacing its content with the latest match.
  pub topmost_enabled: bool,
  /// Absent means the four built-in rules. Present, even empty, replaces them.
  pub rules: Vec<Rule>,
}

impl Default for Notifications {
  fn default() -> Self {
    Self {
      enabled: true,
      topmost_enabled: false,
      rules: Rule::built_in(),
    }
  }
}

/// The default template for a notification title.
pub const DEFAULT_RULE_TITLE: &str = "{title|topic}";

/// The default template for a notification body.
pub const DEFAULT_RULE_BODY: &str = "{body}";

fn default_rule_title() -> String {
  DEFAULT_RULE_TITLE.to_owned()
}

fn default_rule_body() -> String {
  DEFAULT_RULE_BODY.to_owned()
}

fn default_true() -> bool {
  true
}

fn is_false(value: &bool) -> bool {
  !*value
}

/// One notification rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
  /// Unique within the config. Reusing a built-in id overrides that rule.
  pub id: String,
  /// An MQTT topic filter, relative to `hiveme` unless `absolute` is true.
  pub topic: String,
  #[serde(default, skip_serializing_if = "is_false")]
  pub absolute: bool,
  /// The payload level this notification rule matches. It never determines the MQTT topic.
  #[serde(default)]
  pub level: Level,
  #[serde(default = "default_true")]
  pub enabled: bool,
  /// Include this rule in OS notifications when the global channel is enabled.
  #[serde(default)]
  pub os: bool,
  /// Include this rule in topmost window notifications when the global channel is enabled.
  #[serde(default)]
  pub topmost: bool,
  #[serde(default = "default_rule_title")]
  pub title: String,
  #[serde(default = "default_rule_body")]
  pub body: String,
  /// Reserved for payload matching. Not implemented.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[serde(rename = "match")]
  #[schemars(with = "Option<std::collections::BTreeMap<String, serde_json::Value>>")]
  pub matches: Option<Value>,
}

impl Rule {
  /// The four rules that apply when the config does not list any.
  pub fn built_in() -> Vec<Self> {
    [Level::Info, Level::Success, Level::Warn, Level::Error]
      .into_iter()
      .map(|level| Self {
        id: level.as_str().to_owned(),
        topic: "#".to_owned(),
        absolute: false,
        level: level.clone(),
        enabled: true,
        os: false,
        topmost: false,
        title: default_rule_title(),
        body: default_rule_body(),
        matches: None,
      })
      .collect()
  }

  /// The absolute filter this rule matches against.
  pub fn resolve(&self, prefix: &str) -> String {
    crate::topic::resolve(prefix, &self.topic, self.absolute || self.topic.starts_with('$'))
  }
}

/// The settings of the user interface. `hmg` reads all of them; the terminal UI of `hmc`
/// reads all but `window` and `editor`, and publish mode reads `language`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Gui {
  pub display_mode: DisplayMode,
  pub theme: Theme,
  /// A BCP 47 tag. Both applications support de, en-US, es, fr, it, ja, zh-CN, zh-HK,
  /// and zh-TW; another tag resolves to the nearest of those, or to en-US.
  pub language: String,
  pub editor: Editor,
  pub history: History,
  pub window: Window,
}

impl Default for Gui {
  fn default() -> Self {
    Self {
      display_mode: DisplayMode::default(),
      theme: Theme::default(),
      language: "en-US".to_owned(),
      editor: Editor::default(),
      history: History::default(),
      window: Window::default(),
    }
  }
}

/// Browser text assistance in `hmg` only. Each feature is opt-in.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Editor {
  pub auto_complete: bool,
  pub auto_correct: bool,
  /// When enabled, capitalize sentences in input methods that support it.
  pub auto_capitalize: bool,
  pub spell_check: bool,
  pub writing_suggestions: bool,
}

/// Which color scheme the GUI follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
pub enum DisplayMode {
  /// Follow the operating system.
  Auto,
  Light,
  Dark,
}

config_enum!(DisplayMode, Auto, { Auto => "Auto", Light => "Light", Dark => "Dark" });

/// The GUI palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
pub enum Theme {
  Ocean,
  Aqua,
  Sky,
  Arctic,
  Glacier,
  Mist,
  Slate,
  Charcoal,
  Midnight,
  Indigo,
  Violet,
  Lavender,
  Rose,
  Blush,
  Coral,
  Sunset,
  Amber,
  Sand,
  Forest,
  Emerald,
}

config_enum!(Theme, Ocean, {
  Ocean => "Ocean", Aqua => "Aqua", Sky => "Sky", Arctic => "Arctic",
  Glacier => "Glacier", Mist => "Mist", Slate => "Slate", Charcoal => "Charcoal",
  Midnight => "Midnight", Indigo => "Indigo", Violet => "Violet", Lavender => "Lavender",
  Rose => "Rose", Blush => "Blush", Coral => "Coral", Sunset => "Sunset",
  Amber => "Amber", Sand => "Sand", Forest => "Forest", Emerald => "Emerald",
});

/// How much message history to keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct History {
  /// Older rows beyond this count are deleted per topic. 0 keeps everything.
  pub max_messages_per_topic: u32,
  /// Older rows than this are deleted. 0 disables time based pruning.
  pub retention_days: u32,
}

impl Default for History {
  fn default() -> Self {
    Self {
      max_messages_per_topic: 1_000,
      retention_days: 30,
    }
  }
}

/// The remembered main window geometry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Window {
  pub position: WindowPosition,
  pub size: WindowSize,
}

/// The remembered window position. Negative means "center the window".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowPosition {
  pub x: i32,
  pub y: i32,
}

impl Default for WindowPosition {
  fn default() -> Self {
    Self { x: -1, y: -1 }
  }
}

/// The remembered window size. The GUI clamps it to 600 by 450.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowSize {
  pub width: u32,
  pub height: u32,
}

impl Default for WindowSize {
  fn default() -> Self {
    Self {
      width: 1_200,
      height: 900,
    }
  }
}

/// The GitHub release check.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Update {
  pub check_interval: UpdateCheckInterval,
  /// Unix seconds of the last successful check.
  pub last_checked: i64,
  /// The latest version seen on GitHub.
  pub last_version: String,
  /// A version the user chose to skip.
  pub ignore_version: String,
}

/// How often to look for a new release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
pub enum UpdateCheckInterval {
  Daily,
  Weekly,
  Monthly,
}

config_enum!(UpdateCheckInterval, Weekly, { Daily => "Daily", Weekly => "Weekly", Monthly => "Monthly" });

/// Message encryption. Designed in `docs/specs/message.md`, implemented in phase 6.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Encryption {
  pub mode: EncryptionMode,
  pub keys: Vec<KeyEntry>,
}

impl Encryption {
  /// Whether the applications should encrypt what they publish.
  pub fn is_enabled(&self) -> bool {
    self.mode != EncryptionMode::Off
  }

  /// The key used for sending.
  pub fn active_key(&self) -> Option<&KeyEntry> {
    self.keys.iter().find(|key| key.state == KeyState::Active)
  }

  /// The key that decrypts a message stamped with `kid`.
  pub fn key(&self, kid: &str) -> Option<&KeyEntry> {
    self.keys.iter().find(|key| key.kid == kid)
  }

  fn issues(&self) -> Vec<String> {
    let mut issues = Vec::new();
    let mut kids = std::collections::HashSet::new();
    for key in &self.keys {
      if key.kid.trim().is_empty() {
        issues.push("an encryption key has an empty kid".to_owned());
      } else if !kids.insert(key.kid.as_str()) {
        issues.push(format!("two encryption keys share the kid '{}'", key.kid));
      }
    }
    if self.is_enabled() {
      let active = self.keys.iter().filter(|key| key.state == KeyState::Active).count();
      if active != 1 {
        issues.push(format!(
          "encryption.mode is {} but {active} keys are Active, expected exactly 1",
          self.mode
        ));
      }
    }
    issues
  }
}

/// Whether messages are encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
pub enum EncryptionMode {
  /// Send plaintext, accept both.
  Off,
  /// Send encrypted, accept both.
  Opportunistic,
  /// Send encrypted, flag incoming plaintext in the UI.
  Required,
}

config_enum!(EncryptionMode, Off, {
  Off => "Off", Opportunistic => "Opportunistic", Required => "Required",
});

/// One pre-shared encryption key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct KeyEntry {
  /// The identifier written into `enc.kid`.
  pub kid: String,
  /// The AEAD algorithm, `A256GCM` today.
  pub alg: String,
  /// 32 random bytes, base64url encoded.
  pub secret: String,
  /// The date the key was created, as `YYYY-MM-DD`.
  pub created_at: String,
  pub state: KeyState,
}

/// Whether a key is still used for sending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
pub enum KeyState {
  /// Used for sending, and for decrypting.
  Active,
  /// Used for decrypting old messages only.
  Retired,
}

config_enum!(KeyState, Active, { Active => "Active", Retired => "Retired" });

/// The HiveMQ Cloud REST API. Needs the Starter plan; phase 6.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct CloudApi {
  /// From the API Access tab, for example `https://api.a01.euc1.aws.hivemq.cloud`.
  pub base_url: String,
  /// Six alphanumeric characters.
  pub org_id: String,
  /// The cluster UUID.
  pub cluster_id: String,
  /// The bearer token, shown once at creation time.
  pub token: String,
  /// Where to read the token instead of the field above.
  pub token_ref: Option<SecretRef>,
}

impl CloudApi {
  fn issues(&self) -> Vec<String> {
    let mut issues = Vec::new();
    if !self.base_url.is_empty() && !self.base_url.starts_with("https://") {
      issues.push("cloudApi.baseUrl must start with https://".to_owned());
    }
    if !self.org_id.is_empty() && (self.org_id.len() != 6 || !self.org_id.chars().all(|c| c.is_ascii_alphanumeric())) {
      issues.push("cloudApi.orgId must be six alphanumeric characters".to_owned());
    }
    issues
  }
}

/// Turns a list of complaints into the one error the applications show.
fn report(issues: Vec<String>) -> Result<()> {
  if issues.is_empty() {
    Ok(())
  } else {
    Err(Error::ConfigInvalid(issues))
  }
}

fn var(name: &str) -> Option<String> {
  match std::env::var(name) {
    Ok(value) if !value.is_empty() => Some(value),
    _ => None,
  }
}

/// A config together with the file it came from.
///
/// Holds the document as it was read so that a write preserves keys this build does
/// not know, which is what lets an older and a newer HiveMe share one file.
#[derive(Debug, Clone)]
pub struct ConfigFile {
  path: PathBuf,
  config: Config,
  document: Value,
  outcome: MigrationOutcome,
}

impl ConfigFile {
  /// A complete default config in memory, using the same defaults for both applications.
  fn new(path: &Path) -> Self {
    Self {
      path: path.to_path_buf(),
      config: Config::new_for_this_device(),
      document: Value::Object(serde_json::Map::new()),
      outcome: MigrationOutcome::AlreadyCurrent,
    }
  }

  /// Reads the config at `path`.
  pub fn load(path: &Path) -> Result<Self> {
    let text = match file_io::retry(|| std::fs::read_to_string(path)) {
      Ok(text) => text,
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
        return Err(Error::ConfigNotFound(path.to_path_buf()));
      }
      Err(source) => {
        return Err(Error::ConfigRead {
          path: path.to_path_buf(),
          source,
        });
      }
    };
    Self::from_text(path, &text)
  }

  /// Reads a config document that is already in memory.
  pub fn from_text(path: &Path, text: &str) -> Result<Self> {
    let mut document: Value = serde_json::from_str(text).map_err(|source| Error::ConfigParse {
      path: path.to_path_buf(),
      source,
    })?;
    let outcome = migrate::migrate(&mut document, path)?;
    let config: Config = serde_json::from_value(document.clone()).map_err(|source| Error::ConfigParse {
      path: path.to_path_buf(),
      source,
    })?;
    Ok(Self {
      path: path.to_path_buf(),
      config,
      document,
      outcome,
    })
  }

  /// Reads the config at `path`, creating a default one when the file is missing.
  ///
  /// Also fills in a missing `device.id`, so that a hand written config still gets an
  /// identity. The boolean says whether the file was written.
  pub fn load_or_create(path: &Path) -> Result<(Self, bool)> {
    match Self::load(path) {
      Ok(mut file) => {
        if file.config.device.id.trim().is_empty() && !file.is_read_only() {
          file.config.device = Device::new_for_this_device();
          file.save()?;
          return Ok((file, true));
        }
        if file.config.device.id.trim().is_empty() && file.is_read_only() {
          log::warn!(
            "{} has no device.id; identity repair was skipped because its version is newer than this build",
            path.display()
          );
        }
        Ok((file, false))
      }
      Err(Error::ConfigNotFound(_)) => {
        let mut file = Self::new(path);
        file.save()?;
        Ok((file, true))
      }
      Err(error) => Err(error),
    }
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn config(&self) -> &Config {
    &self.config
  }

  pub fn config_mut(&mut self) -> &mut Config {
    &mut self.config
  }

  pub fn into_config(self) -> Config {
    self.config
  }

  pub fn set_config(&mut self, config: Config) {
    self.config = config;
  }

  /// What the loader had to do to the document.
  pub fn migration_outcome(&self) -> MigrationOutcome {
    self.outcome
  }

  /// Whether the file came from a newer build and must not be rewritten.
  pub fn is_read_only(&self) -> bool {
    self.outcome.is_read_only()
  }

  /// The history database beside this config.
  pub fn database_path(&self) -> PathBuf {
    database_path(&self.path)
  }

  /// Writes the config back, keeping any keys this build does not know.
  pub fn save(&mut self) -> Result<()> {
    self.save_config(self.config.clone())
  }

  /// Writes `config`, and holds it only once it is on disk.
  ///
  /// A write that fails leaves this value as it was, so that a save the file system
  /// refused cannot leave the screen showing settings the next start will not find. The
  /// one case that keeps a config it did not write is a file from a newer build, which
  /// is deliberately never written and whose settings the user can still see.
  pub fn save_config(&mut self, config: Config) -> Result<()> {
    if self.is_read_only() {
      log::warn!(
        "not writing {}: it was written by a newer version of HiveMe",
        self.path.display()
      );
      self.config = config;
      return Ok(());
    }
    let typed = serde_json::to_value(&config).map_err(|source| Error::ConfigParse {
      path: self.path.clone(),
      source,
    })?;
    let mut document = self.document.clone();
    merge(&mut document, typed);
    self.write_document(document)?;
    self.config = config;
    Ok(())
  }

  /// Persists through the shared atomic writer before replacing the in-memory document.
  fn write_document(&mut self, document: Value) -> Result<()> {
    let mut text = serde_json::to_string_pretty(&document).map_err(|source| Error::ConfigParse {
      path: self.path.clone(),
      source,
    })?;
    text.push('\n');
    write_atomically(&self.path, &text)?;
    self.document = document;
    Ok(())
  }
}

/// Copies `source` over `target`, descending into objects and replacing everything else.
///
/// An array is written out as it now stands, so that removing a notification rule
/// really removes it rather than leaving the old element behind. An element that is
/// still there is merged into the element that carried the same identity, so that a key
/// this build does not know survives inside a rule exactly as it survives at the top
/// level: saving a theme must not quietly delete the settings of another version.
fn merge(target: &mut Value, source: Value) {
  match (target, source) {
    (Value::Object(target), Value::Object(source)) => {
      for (key, value) in source {
        merge(target.entry(key).or_insert(Value::Null), value);
      }
    }
    (Value::Array(target), Value::Array(source)) => {
      let mut previous = std::mem::take(target);
      for element in source {
        target.push(
          match identity(&element).and_then(|id| take_by_identity(&mut previous, id)) {
            Some(mut kept) => {
              merge(&mut kept, element);
              kept
            }
            None => element,
          },
        );
      }
    }
    (target, source) => *target = source,
  }
}

/// What names an element of a config array across a write: the `id` of a notification
/// rule, the `kid` of an encryption key. An element without one is replaced whole,
/// because nothing says which of the old elements it used to be.
fn identity(value: &Value) -> Option<&str> {
  let object = value.as_object()?;
  ["id", "kid"].into_iter().find_map(|key| object.get(key)?.as_str())
}

/// Takes the element that carried `id`, so that two elements never merge into one.
fn take_by_identity(elements: &mut Vec<Value>, id: &str) -> Option<Value> {
  let index = elements.iter().position(|element| identity(element) == Some(id))?;
  Some(elements.remove(index))
}

/// Writes through a temporary file so that a crash cannot leave a half written config.
///
/// The temporary file is named for this write and no other. `hmc` and `hmg` share the
/// directory and save whenever a window moves or a setting changes, and two writers on
/// one temporary name do not take turns: they truncate and interleave into a document
/// neither of them meant to write, which the rename then publishes as the config.
fn write_atomically(path: &Path, text: &str) -> Result<()> {
  let directory = path.parent().unwrap_or_else(|| Path::new("."));
  if !directory.as_os_str().is_empty() {
    std::fs::create_dir_all(directory).map_err(|source| Error::ConfigWrite {
      path: directory.to_path_buf(),
      source,
    })?;
  }
  let file_name = path.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
  let temporary = directory.join(format!(
    ".{}.{}.tmp",
    file_name.to_string_lossy(),
    uuid::Uuid::now_v7().simple()
  ));

  let mut options = std::fs::OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  let write = |path: &Path| -> std::io::Result<()> {
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
  };
  write(&temporary).map_err(|source| Error::ConfigWrite {
    path: temporary.clone(),
    source,
  })?;
  file_io::retry(|| std::fs::rename(&temporary, path)).map_err(|source| {
    // The rename is what makes the write visible, so a failed one leaves a file nobody
    // will ever read again.
    let _ = std::fs::remove_file(&temporary);
    Error::ConfigWrite {
      path: path.to_path_buf(),
      source,
    }
  })?;
  #[cfg(unix)]
  std::fs::File::open(if directory.as_os_str().is_empty() {
    Path::new(".")
  } else {
    directory
  })
  .and_then(|directory| directory.sync_all())
  .map_err(|source| Error::ConfigWrite {
    path: directory.to_path_buf(),
    source,
  })?;
  Ok(())
}

/// The JSON schema of the setup string, written to `schemas/broker-init.schema.json`.
pub fn broker_init_json_schema() -> Value {
  init::json_schema()
}

/// The JSON schema of the config, as `cargo xtask schema` writes it.
pub fn json_schema() -> Value {
  let mut schema = serde_json::to_value(schemars::schema_for!(Config)).expect("the config schema serializes");
  if let Some(object) = schema.as_object_mut() {
    object.insert(
      "$id".to_owned(),
      Value::from(format!("https://hiveme.dev/schemas/config/v{CONFIG_VERSION}.json")),
    );
    object.insert("title".to_owned(), Value::from("HiveMe config"));
    object.insert(
      "description".to_owned(),
      Value::from("The config shared by hmc and hmg. Generated from hiveme-core; see docs/specs/config.md."),
    );
  }
  schema
}
