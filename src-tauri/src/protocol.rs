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
//! The values the commands answer with are the shared session's own, defined in
//! `crates/hiveme-core/src/session/types.rs` with the same serialization and re-exported
//! here, so that `hmg` hands them to the frontend unchanged. What stays in this file is
//! what only the GUI has: the event names, the payloads of the two events that are not
//! one of those values, and the Tauri managed state.
//!
//! The config and the message envelope are generated from the JSON schemas into
//! `src/generated/`, because they are the shared format rather than a protocol of the
//! GUI's own.
//!
//! The frontend's Language enum lists its bundled translations. `gui.language`
//! remains a BCP 47 string in the shared config; no IPC enum or conversion is needed.
//! Its Level enum mirrors hiveme_core::message::Level (debug, info, success, warn,
//! error); level fields cross IPC as strings so unknown levels remain readable.

use std::sync::Arc;

use hiveme_core::session::Session;
use serde::{Deserialize, Serialize};

pub use hiveme_core::session::{About, MessageRow, PublishOptions, Status, TopicNode, UpdateCheckResult};

/// The `status` event, and what `get_status` answers with.
pub const EVENT_STATUS: &str = "status";

/// The `message` event: one stored row, as it would come back from `get_messages`.
pub const EVENT_MESSAGE: &str = "message";

/// The `topic-added` event: a topic the tree has not shown before.
pub const EVENT_TOPIC_ADDED: &str = "topic-added";

/// The `notification-fired` event: at least one notification channel succeeded.
pub const EVENT_NOTIFICATION_FIRED: &str = "notification-fired";

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

/// Everything a command handler needs, managed by Tauri.
pub struct AppState {
  /// The shared backend: config, history, broker connection, rules, update check.
  pub session: Arc<Session>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_event_payloads_keep_the_camel_case_the_frontend_reads() {
    let fired = serde_json::to_value(NotificationFiredEvent {
      rule_id: "error".to_owned(),
      message_id: "018f6b1e".to_owned(),
      topic: "hiveme".to_owned(),
    })
    .unwrap();

    assert_eq!(
      fired,
      serde_json::json!({ "ruleId": "error", "messageId": "018f6b1e", "topic": "hiveme" })
    );
  }
}

// Stored history tier is authoritative when rendering raw JSON/text.

/// The latest content of the one topmost notification window.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopmostSnapshot {
  pub revision: u64,
  #[serde(flatten)]
  pub content: hiveme_core::session::TopmostNotification,
}
