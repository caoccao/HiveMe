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

//! The HiveMe message format.
//!
//! Specified in `docs/specs/message.md`. The envelope is deliberately permissive:
//! a reader never rejects a payload, it degrades through the parse tiers instead.

use std::borrow::Cow;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Device;

/// The envelope version this build writes and fully understands.
pub const ENVELOPE_VERSION: u32 = 1;

/// The MQTT content type of every message HiveMe publishes.
pub const CONTENT_TYPE: &str = "application/json";

/// The MQTT 5 user property carrying the envelope version.
pub const USER_PROPERTY_VERSION: &str = "hiveme-v";

/// The MQTT 5 user property carrying the encryption algorithm.
pub const USER_PROPERTY_ENCRYPTION: &str = "hiveme-enc";

/// The default `type` of an envelope.
pub const DEFAULT_KIND: &str = "message";

/// How much of a raw JSON payload a notification body shows.
pub const RAW_JSON_PREVIEW_LIMIT: usize = 200;

fn default_kind() -> String {
  DEFAULT_KIND.to_owned()
}

/// The severity of a message.
///
/// This is an open enum: a value this build does not know is preserved as
/// [`Level::Other`] and displayed as [`Level::Info`], so that a newer writer never
/// makes a message unreadable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum Level {
  Debug,
  #[default]
  Info,
  Success,
  Warn,
  Error,
  /// A level this build does not know, normalized to lowercase.
  Other(String),
}

impl Level {
  /// The wire form of this level.
  pub fn as_str(&self) -> &str {
    match self {
      Self::Debug => "debug",
      Self::Info => "info",
      Self::Success => "success",
      Self::Warn => "warn",
      Self::Error => "error",
      Self::Other(raw) => raw,
    }
  }

  /// Reads a level from its wire form. An unrecognized value becomes [`Level::Other`].
  pub fn parse(raw: &str) -> Self {
    match raw.to_lowercase().as_str() {
      "debug" => Self::Debug,
      "info" => Self::Info,
      "success" => Self::Success,
      "warn" => Self::Warn,
      "error" => Self::Error,
      other => Self::Other(other.to_owned()),
    }
  }

  /// Whether this build knows what the level means.
  pub fn is_known(&self) -> bool {
    !matches!(self, Self::Other(_))
  }

  /// The level a reader displays. An unknown level displays as `info`.
  pub fn displayed(&self) -> Self {
    if self.is_known() { self.clone() } else { Self::Info }
  }

  /// Every level this build knows, in display order.
  pub const fn known() -> [Self; 5] {
    [Self::Debug, Self::Info, Self::Success, Self::Warn, Self::Error]
  }
}

impl std::fmt::Display for Level {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Other(value) => formatter.write_str(&value.to_lowercase()),
      _ => formatter.write_str(self.as_str()),
    }
  }
}

impl Serialize for Level {
  fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    match self {
      Self::Other(value) => serializer.serialize_str(&value.to_lowercase()),
      _ => serializer.serialize_str(self.as_str()),
    }
  }
}

impl<'de> Deserialize<'de> for Level {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    Ok(Self::parse(&String::deserialize(deserializer)?))
  }
}

impl schemars::JsonSchema for Level {
  fn schema_name() -> Cow<'static, str> {
    Cow::Borrowed("Level")
  }

  fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
      "type": "string",
      "description": "Message severity, normalized to lowercase. An open enum: a value outside the examples is preserved in lowercase and displayed as `info`.",
      "examples": ["debug", "info", "success", "warn", "error"],
      "default": "info",
    })
  }
}

/// Who produced a message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Sender {
  /// The `device.id` of the installation that produced the message.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
  /// The human readable device name.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  /// The producing application, `hmc` or `hmg` for the HiveMe tools.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub app: Option<String>,
  /// The version of the producing application.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub app_version: Option<String>,
}

impl Sender {
  /// The sender describing this installation, running as `app`.
  pub fn from_device(device: &Device, app: &str) -> Self {
    Self {
      id: (!device.id.is_empty()).then(|| device.id.clone()),
      name: (!device.name.is_empty()).then(|| device.name.clone()),
      app: Some(app.to_owned()),
      app_version: Some(crate::VERSION.to_owned()),
    }
  }

  /// The best available label for this sender.
  pub fn label(&self) -> &str {
    self
      .name
      .as_deref()
      .or(self.id.as_deref())
      .or(self.app.as_deref())
      .unwrap_or("")
  }
}

/// The readable content of a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
  /// The main text. May be empty when `data` carries the content.
  pub body: String,
  /// Used as the notification title when present.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  /// The severity. Defaults to `info`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub level: Option<Level>,
  /// Free form JSON for scripts.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[schemars(with = "Option<std::collections::BTreeMap<String, serde_json::Value>>")]
  pub data: Option<serde_json::Value>,
  /// A hint for `body`, such as `text/markdown`. Defaults to `text/plain`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub content_type: Option<String>,
}

impl Payload {
  /// A payload carrying only text.
  pub fn new(body: impl Into<String>) -> Self {
    Self {
      body: body.into(),
      title: None,
      level: None,
      data: None,
      content_type: None,
    }
  }

  /// The severity, applying the documented default.
  pub fn level(&self) -> Level {
    self.level.clone().unwrap_or_default()
  }
}

/// How a message was encrypted.
///
/// Designed but not implemented; see `docs/specs/message.md`. Readers already
/// recognize the shape so that an encrypted message is displayed as a placeholder
/// rather than as unreadable JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Enc {
  /// The AEAD algorithm. An open enum; `A256GCM` is the first one.
  pub alg: String,
  /// The identifier of the pre-shared key that encrypted this message.
  pub kid: String,
  /// The base64url nonce.
  pub iv: String,
}

/// The AEAD algorithm HiveMe will use first.
pub const ALGORITHM_A256GCM: &str = "A256GCM";

impl Enc {
  /// Whether this build could decrypt the message, once encryption is implemented.
  pub fn is_known_algorithm(&self) -> bool {
    self.alg == ALGORITHM_A256GCM
  }
}

/// A HiveMe message envelope.
///
/// Exactly one of `payload` and `enc` is meaningful. A reader that sees both prefers
/// `enc`, per `docs/specs/message.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Message {
  /// The envelope major version.
  pub v: u32,
  /// A unique identifier, UUID v7 for messages HiveMe writes.
  pub id: String,
  /// RFC 3339 with a timezone and millisecond precision, from the producer's clock.
  pub ts: String,
  /// An open enum; `message` unless stated otherwise.
  #[serde(rename = "type", default = "default_kind")]
  pub kind: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sender: Option<Sender>,
  /// The readable content. Absent on an encrypted message.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub payload: Option<Payload>,
  /// The encryption header. Absent on a plaintext message.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub enc: Option<Enc>,
  /// The base64url AEAD output. Present exactly when `enc` is.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ciphertext: Option<String>,
  /// The `id` of another message. Reserved for threading.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reply_to: Option<String>,
  /// Mirrors the MQTT message expiry so a reader can show staleness.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ttl_secs: Option<u32>,
}

impl Message {
  /// A plaintext message carrying `body`, stamped with a fresh identifier and time.
  pub fn new_text(sender: Sender, body: impl Into<String>) -> Self {
    Self {
      v: ENVELOPE_VERSION,
      id: uuid::Uuid::now_v7().to_string(),
      ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
      kind: default_kind(),
      sender: Some(sender),
      payload: Some(Payload::new(body)),
      enc: None,
      ciphertext: None,
      reply_to: None,
      ttl_secs: None,
    }
  }

  /// Sets the notification title.
  #[must_use]
  pub fn with_title(mut self, title: impl Into<String>) -> Self {
    if let Some(payload) = self.payload.as_mut() {
      payload.title = Some(title.into());
    }
    self
  }

  /// Sets the severity.
  #[must_use]
  pub fn with_level(mut self, level: Level) -> Self {
    if let Some(payload) = self.payload.as_mut() {
      payload.level = Some(level);
    }
    self
  }

  /// Attaches free form data.
  #[must_use]
  pub fn with_data(mut self, data: serde_json::Value) -> Self {
    if let Some(payload) = self.payload.as_mut() {
      payload.data = Some(data);
    }
    self
  }

  /// Sets the time to live, which also becomes the MQTT message expiry.
  #[must_use]
  pub fn with_ttl_secs(mut self, ttl_secs: u32) -> Self {
    self.ttl_secs = Some(ttl_secs);
    self
  }

  /// Whether the content is encrypted.
  pub fn is_encrypted(&self) -> bool {
    self.enc.is_some()
  }

  /// Whether the writer used an envelope version newer than this build understands.
  pub fn is_newer_version(&self) -> bool {
    self.v > ENVELOPE_VERSION
  }

  /// The severity to display, falling back to `info` for an encrypted or absent payload.
  pub fn level(&self) -> Level {
    self.payload.as_ref().map(Payload::level).unwrap_or_default()
  }

  /// The producer's timestamp, when it is readable.
  pub fn timestamp(&self) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(&self.ts)
      .ok()
      .map(|parsed| parsed.with_timezone(&Utc))
  }

  /// Rejects the combinations `docs/specs/message.md` forbids.
  pub fn validate(&self) -> Result<(), &'static str> {
    match (&self.payload, &self.enc, &self.ciphertext) {
      (_, Some(_), None) => Err("enc is present without ciphertext"),
      (_, None, Some(_)) => Err("ciphertext is present without enc"),
      (None, None, None) => Err("neither payload nor enc is present"),
      _ => Ok(()),
    }
  }

  /// The JSON bytes to publish.
  pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(self)
  }

  /// The MQTT 5 properties that accompany this message.
  pub fn mqtt_properties(&self) -> MessageProperties {
    let mut user_properties = vec![(USER_PROPERTY_VERSION.to_owned(), self.v.to_string())];
    if let Some(enc) = self.enc.as_ref() {
      user_properties.push((USER_PROPERTY_ENCRYPTION.to_owned(), enc.alg.clone()));
    }
    MessageProperties {
      content_type: CONTENT_TYPE,
      user_properties,
      message_expiry_interval: self.ttl_secs,
    }
  }
}

/// The MQTT 5 properties published alongside a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageProperties {
  pub content_type: &'static str,
  pub user_properties: Vec<(String, String)>,
  pub message_expiry_interval: Option<u32>,
}

/// What a reader made of an MQTT payload.
///
/// The tiers are specified in `docs/specs/message.md`. Nothing is ever discarded: a
/// payload that is not a HiveMe envelope is still displayed.
#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
  /// Tier A: a HiveMe envelope.
  Envelope(Box<Message>),
  /// Tier B: valid JSON that is not an envelope.
  RawJson(serde_json::Value),
  /// Tier C: bytes that are valid UTF-8.
  RawText(String),
  /// Tier C: bytes that are not valid UTF-8.
  RawBytes(Vec<u8>),
}

/// The name a stored row uses for each tier. Mirrors the `tier` column in
/// `docs/specs/gui.md`.
impl Parsed {
  pub fn tier(&self) -> &'static str {
    match self {
      Self::Envelope(_) => "envelope",
      Self::RawJson(_) => "json",
      Self::RawText(_) => "text",
      Self::RawBytes(_) => "bytes",
    }
  }

  /// The envelope, when the payload was one.
  pub fn envelope(&self) -> Option<&Message> {
    match self {
      Self::Envelope(message) => Some(message),
      _ => None,
    }
  }

  /// The text a notification body shows for this payload.
  pub fn notification_body(&self) -> String {
    match self {
      Self::Envelope(message) => match message.payload.as_ref() {
        Some(payload) => payload.body.clone(),
        None => match message.enc.as_ref() {
          Some(enc) => format!("encrypted (key {})", enc.kid),
          None => String::new(),
        },
      },
      Self::RawJson(value) => truncate(&value.to_string(), RAW_JSON_PREVIEW_LIMIT),
      Self::RawText(text) => text.clone(),
      Self::RawBytes(bytes) => format!("{} bytes", bytes.len()),
    }
  }
}

fn truncate(text: &str, limit: usize) -> String {
  if text.chars().count() <= limit {
    return text.to_owned();
  }
  let mut truncated: String = text.chars().take(limit).collect();
  truncated.push('\u{2026}');
  truncated
}

/// Reads an MQTT payload, degrading through the tiers rather than failing.
pub fn parse(bytes: &[u8]) -> Parsed {
  if let Ok(message) = serde_json::from_slice::<Message>(bytes)
    && message.validate().is_ok()
  {
    return Parsed::Envelope(Box::new(message));
  }
  match serde_json::from_slice::<serde_json::Value>(bytes) {
    Ok(value) => Parsed::RawJson(value),
    Err(_) => match std::str::from_utf8(bytes) {
      Ok(text) => Parsed::RawText(text.to_owned()),
      Err(_) => Parsed::RawBytes(bytes.to_vec()),
    },
  }
}

/// Builds the same envelope for the CLI and both interactive composers.
pub fn build_envelope(
  config: &crate::config::Config,
  app: &str,
  body: &str,
  title: Option<&str>,
  level: Option<&str>,
) -> Message {
  let mut message = Message::new_text(Sender::from_device(&config.device, app), body)
    .with_level(level.map(Level::parse).unwrap_or_default());
  if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
    message = message.with_title(title);
  }
  message
}

/// Checks raw JSON while preserving the original bytes.
pub fn raw_json_payload(text: &str) -> Result<Vec<u8>, serde_json::Error> {
  serde_json::from_str::<serde_json::Value>(text)?;
  Ok(text.as_bytes().to_vec())
}

/// Raw JSON has no HiveMe envelope version property.
pub fn raw_json_properties() -> MessageProperties {
  MessageProperties {
    content_type: CONTENT_TYPE,
    user_properties: Vec::new(),
    message_expiry_interval: None,
  }
}

/// The JSON schema of the message envelope, as `cargo xtask schema` writes it.
///
/// The derive cannot express "exactly one of `payload` and `enc`", so the constraint
/// is added here. Keeping it in the crate means the generator, the freshness test, and
/// the spec example check all see the same schema.
pub fn json_schema() -> serde_json::Value {
  let mut schema = serde_json::to_value(schemars::schema_for!(Message)).expect("the message schema serializes");
  if let Some(object) = schema.as_object_mut() {
    object.insert(
      "$id".to_owned(),
      serde_json::Value::from(format!("https://hiveme.dev/schemas/message/v{ENVELOPE_VERSION}.json")),
    );
    object.insert("title".to_owned(), serde_json::Value::from("HiveMe message"));
    object.insert(
      "description".to_owned(),
      serde_json::Value::from("A HiveMe message envelope. Generated from hiveme-core; see docs/specs/message.md."),
    );
    object.insert(
      "oneOf".to_owned(),
      serde_json::json!([
        {
          "title": "Plaintext",
          "description": "Carries a readable payload.",
          "required": ["payload"],
          "not": { "anyOf": [{ "required": ["enc"] }, { "required": ["ciphertext"] }] }
        },
        {
          "title": "Encrypted",
          "description": "Carries a ciphertext and the header needed to decrypt it.",
          "required": ["enc", "ciphertext"],
          "not": { "required": ["payload"] }
        }
      ]),
    );
  }
  schema
}
