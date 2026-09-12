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

//! The IPC types shared with the frontend.
//!
//! This file and `src/lib/protocol.ts` are hand synced: snake_case here with an
//! explicit `#[serde(rename)]`, camelCase there. `scripts/ts/check-spec-sync.ts` fails
//! when one moves without the other.
//!
//! Only the types the IPC invents live here. The config and the message envelope are
//! generated from the JSON schemas into `src/generated/`, because they are the shared
//! format rather than a protocol of the GUI's own.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::mqtt::Mqtt;
use crate::notification::Notifier;

/// The `status` event, and what `get_status` answers with.
pub const EVENT_STATUS: &str = "status";

/// The `message` event: one stored row, as it would come back from `get_messages`.
pub const EVENT_MESSAGE: &str = "message";

/// The `topic-added` event: a topic the tree has not shown before.
pub const EVENT_TOPIC_ADDED: &str = "topic-added";

/// The `notification-fired` event: a rule raised an OS notification.
pub const EVENT_NOTIFICATION_FIRED: &str = "notification-fired";

/// What the About tab shows.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct About {
  #[serde(rename = "appVersion")]
  pub app_version: String,
  #[serde(rename = "configPath")]
  pub config_path: String,
  #[serde(rename = "databasePath")]
  pub database_path: String,
  #[serde(rename = "deviceId")]
  pub device_id: String,
  #[serde(rename = "deviceName")]
  pub device_name: String,
  #[serde(rename = "githubUrl")]
  pub github_url: String,
}

/// What the status bar renders.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Status {
  /// `Connecting`, `Connected`, `Reconnecting`, or `Disconnected`.
  pub state: String,
  pub host: String,
  pub port: u16,
  #[serde(rename = "clientId")]
  pub client_id: String,
  pub subscriptions: usize,
  /// Reconnect attempts since the last successful connection.
  pub attempt: u32,
  #[serde(rename = "retryInMs")]
  pub retry_in_ms: Option<u64>,
  #[serde(rename = "lastError")]
  pub last_error: Option<String>,
  /// Messages received since the application started.
  #[serde(rename = "messagesReceived")]
  pub messages_received: u64,
  /// The size of the history database, for the status bar.
  #[serde(rename = "databaseBytes")]
  pub database_bytes: u64,
  /// Whether the toolbar toggle is holding notifications back.
  #[serde(rename = "notificationsPaused")]
  pub notifications_paused: bool,
  /// Why the config file could not be read, when that is why nothing is connected.
  #[serde(rename = "configError")]
  pub config_error: Option<String>,
}

impl Status {
  /// The status of a GUI that has never connected.
  pub fn disconnected() -> Self {
    Self {
      state: hiveme_core::State::Disconnected.as_str().to_owned(),
      host: String::new(),
      port: 0,
      client_id: String::new(),
      subscriptions: 0,
      attempt: 0,
      retry_in_ms: None,
      last_error: None,
      messages_received: 0,
      database_bytes: 0,
      notifications_paused: false,
      config_error: None,
    }
  }

  /// Fills in what the MQTT client knows, leaving the rest to the caller.
  pub fn from_client(status: &hiveme_core::Status) -> Self {
    Self {
      state: status.state.as_str().to_owned(),
      host: status.host.clone(),
      port: status.port,
      client_id: status.client_id.clone(),
      subscriptions: status.subscriptions,
      attempt: status.attempt,
      retry_in_ms: status.retry_in_ms,
      last_error: status.last_error.clone(),
      ..Self::disconnected()
    }
  }
}

/// One node of the topic tree.
///
/// The tree is built from the stored topics by splitting on `/`, so a node exists for
/// every segment even when no message ever arrived on it. `topic` is set only on the
/// nodes that are real topics, and only those can be selected.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TopicNode {
  /// The full path of this node, which is what the tree view uses as its item id.
  pub id: String,
  /// The last segment, which is what the node shows.
  pub label: String,
  /// The topic this node stands for, when a message has been stored on it.
  pub topic: Option<String>,
  /// Unread messages here and everywhere below.
  pub unread: u32,
  /// Stored messages here and everywhere below.
  pub messages: u32,
  pub children: Vec<TopicNode>,
}

/// One stored message, as the chat view renders it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MessageRow {
  /// The row identifier, which is also the cursor `get_messages` pages with.
  #[serde(rename = "rowId")]
  pub row_id: i64,
  pub topic: String,
  /// The envelope id, or a generated one for a payload HiveMe did not shape.
  pub id: String,
  pub ts: String,
  #[serde(rename = "receivedTs")]
  pub received_ts: String,
  #[serde(rename = "senderId")]
  pub sender_id: Option<String>,
  #[serde(rename = "senderName")]
  pub sender_name: Option<String>,
  pub app: Option<String>,
  /// `envelope`, `json`, `text`, or `bytes`.
  pub tier: String,
  pub level: Option<String>,
  pub title: Option<String>,
  pub body: String,
  /// The payload as text, or as hex for the `bytes` tier. The view parses it again to
  /// render the envelope and the JSON tree, so the two readers stay comparable.
  pub raw: String,
  /// How many bytes the payload was, which is what the `bytes` tier shows.
  #[serde(rename = "rawLength")]
  pub raw_length: u64,
  pub qos: u8,
  pub retain: bool,
  pub outgoing: bool,
}

impl From<hiveme_core::StoredMessage> for MessageRow {
  fn from(stored: hiveme_core::StoredMessage) -> Self {
    let raw_length = stored.raw.len() as u64;
    let raw = match String::from_utf8(stored.raw) {
      Ok(text) => text,
      Err(error) => hex(error.as_bytes()),
    };
    Self {
      row_id: stored.row_id,
      topic: stored.topic,
      id: stored.msg_id,
      ts: stored.ts,
      received_ts: stored.received_ts,
      sender_id: stored.sender_id,
      sender_name: stored.sender_name,
      app: stored.app,
      tier: stored.tier,
      level: stored.level,
      title: stored.title,
      body: stored.body,
      raw,
      raw_length,
      qos: stored.qos,
      retain: stored.retain,
      outgoing: stored.outgoing,
    }
  }
}

/// Lowercase hex, so that a payload that is not text still reaches the view intact.
fn hex(bytes: &[u8]) -> String {
  let mut out = String::with_capacity(bytes.len() * 2);
  for byte in bytes {
    out.push_str(&format!("{byte:02x}"));
  }
  out
}

/// The per-message overrides the composer's send menu offers.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PublishOptions {
  /// Publish the body as a raw JSON payload, with no HiveMe envelope, as `hmc --json`
  /// does.
  #[serde(default)]
  pub json: bool,
  #[serde(default)]
  pub qos: Option<u8>,
  #[serde(default)]
  pub retain: Option<bool>,
  #[serde(default)]
  pub title: Option<String>,
  #[serde(default)]
  pub level: Option<String>,
}

/// The `topic-added` event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TopicAddedEvent {
  pub topic: String,
}

/// The `notification-fired` event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationFiredEvent {
  #[serde(rename = "ruleId")]
  pub rule_id: String,
  #[serde(rename = "messageId")]
  pub message_id: String,
  pub topic: String,
}

/// What the update check found.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct UpdateCheckResult {
  #[serde(rename = "hasUpdate")]
  pub has_update: bool,
  #[serde(rename = "latestVersion")]
  pub latest_version: Option<String>,
}

/// Everything a command handler needs, managed by Tauri.
pub struct AppState {
  /// The history database, opened once at startup.
  pub store: Arc<hiveme_core::Store>,
  /// The broker connection and the task that drains it.
  pub mqtt: Arc<Mqtt>,
  /// The rule engine, the rate limiter, and the pause toggle.
  pub notifier: Arc<Notifier>,
  /// The result of the background update check, once it has one.
  pub update: Arc<Mutex<Option<UpdateCheckResult>>>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_payload_that_is_not_text_reaches_the_view_as_hex() {
    let stored = hiveme_core::StoredMessage {
      row_id: 1,
      topic: "hiveme/info".to_owned(),
      msg_id: "raw-1".to_owned(),
      ts: "2026-09-12T09:41:23.512Z".to_owned(),
      received_ts: "2026-09-12T09:41:23.512Z".to_owned(),
      sender_id: None,
      sender_name: None,
      app: None,
      tier: "bytes".to_owned(),
      level: None,
      title: None,
      body: "3 bytes".to_owned(),
      raw: vec![0xff, 0xfe, 0x00],
      qos: 0,
      retain: false,
      outgoing: false,
    };

    let row = MessageRow::from(stored);

    assert_eq!(row.raw, "fffe00");
    assert_eq!(row.raw_length, 3);
  }

  #[test]
  fn a_text_payload_reaches_the_view_unchanged() {
    let stored = hiveme_core::StoredMessage {
      row_id: 2,
      topic: "hiveme/info".to_owned(),
      msg_id: "raw-2".to_owned(),
      ts: "2026-09-12T09:41:23.512Z".to_owned(),
      received_ts: "2026-09-12T09:41:23.512Z".to_owned(),
      sender_id: None,
      sender_name: None,
      app: None,
      tier: "text".to_owned(),
      level: None,
      title: None,
      body: "hello".to_owned(),
      raw: b"hello".to_vec(),
      qos: 1,
      retain: false,
      outgoing: true,
    };

    let row = MessageRow::from(stored);

    assert_eq!(row.raw, "hello");
    assert_eq!(row.raw_length, 5);
    assert!(row.outgoing);
  }

  #[test]
  fn the_state_reaches_the_frontend_spelled_the_way_it_compares_it() {
    // The frontend reads Status.state against ConnectionState in src/lib/protocol.ts,
    // and enables the composer only on Connected, so these four spellings are a
    // contract rather than display text. The footer translates them into its own words.
    let states = [
      (hiveme_core::State::Connecting, "Connecting"),
      (hiveme_core::State::Connected, "Connected"),
      (hiveme_core::State::Reconnecting, "Reconnecting"),
      (hiveme_core::State::Disconnected, "Disconnected"),
    ];

    for (state, name) in states {
      let status = Status::from_client(&hiveme_core::Status {
        state,
        host: "broker.example".to_owned(),
        port: 8883,
        client_id: "hiveme-gui".to_owned(),
        subscriptions: 1,
        attempt: 0,
        retry_in_ms: None,
        last_error: None,
      });

      assert_eq!(status.state, name);
    }

    assert_eq!(Status::disconnected().state, "Disconnected");
  }
}
