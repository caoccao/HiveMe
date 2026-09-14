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

//! The values the session answers with and the events it raises.
//!
//! They keep the camelCase serialization `hmg` has always sent its frontend, so that
//! `src-tauri/src/protocol.rs` re-exports them and `src/lib/protocol.ts` stays the
//! other half of the same hand-synced contract: snake_case here with an explicit
//! `#[serde(rename)]`, camelCase there. The shapes are listed in
//! `docs/specs/session.md`.
//!
//! Level fields travel as strings so that a level this build does not know is still
//! readable at the other end.

use serde::{Deserialize, Serialize};

use crate::mqtt::State;
use crate::storage::StoredMessage;

use super::SessionApp;

/// What the About tab shows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
  /// The status of a session that has never connected.
  pub fn disconnected() -> Self {
    Self {
      state: State::Disconnected.as_str().to_owned(),
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
  pub fn from_client(status: &crate::mqtt::Status) -> Self {
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
/// every segment even when no message ever arrived on it. Every nonempty path has a
/// `topic` and selects its full subtree. An empty leading segment is only a group.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TopicNode {
  /// The full path of this node, which is what a tree view uses as its item id.
  pub id: String,
  /// The last segment, which is what the node shows.
  pub label: String,
  /// The selected subtree root; None only for an empty leading segment.
  pub topic: Option<String>,
  /// Unread messages here and everywhere below.
  pub unread: u32,
  /// Stored messages here and everywhere below.
  pub messages: u32,
  pub children: Vec<TopicNode>,
}

/// One stored message, as a chat view renders it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MessageRow {
  /// The row identifier, which is also the cursor `messages` pages with.
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
  /// The payload as text, or as hex for the `bytes` tier. A view parses it again to
  /// render the envelope and the JSON tree, so the readers stay comparable.
  pub raw: String,
  /// How many bytes the payload was, which is what the `bytes` tier shows.
  #[serde(rename = "rawLength")]
  pub raw_length: u64,
  pub qos: u8,
  pub retain: bool,
  /// Whether the application reading this row is the one that published it, which is
  /// what puts a bubble on the right. See [`MessageRow::seen_by`].
  pub outgoing: bool,
}

impl MessageRow {
  /// One stored row, as `app` sees it.
  ///
  /// [`StoredMessage::outgoing`] says that this installation published the message,
  /// which is not the same question. `hmc` and `hmg` share one `HiveMe.db`, so a
  /// message either of them sends is outgoing in the database for both of them, and
  /// asking the column alone would put the other application's messages on the right.
  /// Pairing it with the producing application answers the narrower question the view
  /// is really asking, and both halves are stored, so a restart reads the same answer
  /// the live event gave.
  pub fn seen_by(stored: StoredMessage, app: SessionApp) -> Self {
    let raw_length = stored.raw.len() as u64;
    let outgoing = stored.outgoing && stored.app.as_deref() == Some(app.app_id());
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
      outgoing,
    }
  }
}

/// Lowercase hex, so that a payload that is not text still reaches the view intact.
fn hex(bytes: &[u8]) -> String {
  let mut out = String::with_capacity(bytes.len() * 2);
  for byte in bytes {
    use std::fmt::Write;
    write!(out, "{byte:02x}").expect("writing into a String cannot fail");
  }
  out
}

/// The publish overrides a composer's collapsible options panel controls.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct PublishOptions {
  /// Relative to the selected topic; leading slashes are ignored.
  #[serde(default)]
  pub topic: Option<String>,
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

/// What the update check found.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct UpdateCheckResult {
  #[serde(rename = "hasUpdate")]
  pub has_update: bool,
  #[serde(rename = "latestVersion")]
  pub latest_version: Option<String>,
}

impl UpdateCheckResult {
  /// Nothing newer, or nothing worth telling the user about.
  pub fn none() -> Self {
    Self {
      has_update: false,
      latest_version: None,
    }
  }
}

/// Something the screen of either application has to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
  /// A connection state change, a reconnect countdown, or a pause toggle.
  Status(Status),
  /// One stored row, whether it arrived or was published here.
  Message(MessageRow),
  /// The first message ever stored on a topic.
  TopicAdded { topic: String },
  /// A rule raised an OS notification.
  NotificationFired {
    rule_id: String,
    message_id: String,
    topic: String,
  },
}

#[cfg(test)]
mod tests {
  use super::*;

  fn stored(raw: Vec<u8>, tier: &str, outgoing: bool) -> StoredMessage {
    stored_from("hmc", raw, tier, outgoing)
  }

  fn stored_from(app: &str, raw: Vec<u8>, tier: &str, outgoing: bool) -> StoredMessage {
    StoredMessage {
      row_id: 1,
      topic: "hiveme/info".to_owned(),
      msg_id: "raw-1".to_owned(),
      ts: "2026-09-12T09:41:23.512Z".to_owned(),
      received_ts: "2026-09-12T09:41:23.512Z".to_owned(),
      sender_id: None,
      sender_name: None,
      app: Some(app.to_owned()),
      tier: tier.to_owned(),
      level: None,
      title: None,
      body: String::new(),
      raw,
      qos: 1,
      retain: false,
      outgoing,
    }
  }

  #[test]
  fn a_payload_that_is_not_text_reaches_the_view_as_hex() {
    let row = MessageRow::seen_by(stored(vec![0xff, 0xfe, 0x00], "bytes", false), SessionApp::Tui);

    assert_eq!(row.raw, "fffe00");
    assert_eq!(row.raw_length, 3);
  }

  #[test]
  fn a_text_payload_reaches_the_view_unchanged() {
    let row = MessageRow::seen_by(stored(b"hello".to_vec(), "text", true), SessionApp::Tui);

    assert_eq!(row.raw, "hello");
    assert_eq!(row.raw_length, 5);
    assert!(row.outgoing);
  }

  #[test]
  fn a_row_is_outgoing_only_to_the_application_that_published_it() {
    let sent_by_hmc = stored_from("hmc", b"hello".to_vec(), "text", true);

    assert!(MessageRow::seen_by(sent_by_hmc.clone(), SessionApp::Tui).outgoing);
    assert!(
      !MessageRow::seen_by(sent_by_hmc, SessionApp::Gui).outgoing,
      "hmc and hmg share one database, so hmg must not read hmc's message as its own"
    );

    let sent_by_hmg = stored_from("hmg", b"hello".to_vec(), "text", true);

    assert!(MessageRow::seen_by(sent_by_hmg.clone(), SessionApp::Gui).outgoing);
    assert!(!MessageRow::seen_by(sent_by_hmg, SessionApp::Tui).outgoing);
  }

  #[test]
  fn a_row_this_installation_never_published_is_incoming_to_both() {
    let from_elsewhere = stored_from("hmc", b"hello".to_vec(), "text", false);

    assert!(!MessageRow::seen_by(from_elsewhere.clone(), SessionApp::Tui).outgoing);
    assert!(!MessageRow::seen_by(from_elsewhere, SessionApp::Gui).outgoing);
  }

  #[test]
  fn the_state_reaches_the_frontend_spelled_the_way_it_compares_it() {
    // The GUI reads Status.state against ConnectionState in src/lib/protocol.ts, and
    // enables the composer only on Connected, so these four spellings are a contract
    // rather than display text. The footer translates them into its own words.
    let states = [
      (State::Connecting, "Connecting"),
      (State::Connected, "Connected"),
      (State::Reconnecting, "Reconnecting"),
      (State::Disconnected, "Disconnected"),
    ];

    for (state, name) in states {
      let status = Status::from_client(&crate::mqtt::Status {
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

  #[test]
  fn the_types_keep_the_camel_case_the_frontend_reads() {
    let status = serde_json::to_value(Status::disconnected()).unwrap();
    for key in [
      "clientId",
      "retryInMs",
      "lastError",
      "messagesReceived",
      "databaseBytes",
      "notificationsPaused",
      "configError",
    ] {
      assert!(status.get(key).is_some(), "Status has no {key}");
    }
    let row = serde_json::to_value(MessageRow::seen_by(stored(Vec::new(), "text", false), SessionApp::Tui)).unwrap();
    for key in ["rowId", "receivedTs", "senderId", "senderName", "rawLength"] {
      assert!(row.get(key).is_some(), "MessageRow has no {key}");
    }
  }
}
