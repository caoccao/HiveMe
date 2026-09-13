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

//! The session's events, as the frontend hears them.
//!
//! [`hiveme_core::session`] owns the broker connection, the history, and the rules, and
//! raises what happened on a channel. This task is the whole of what `hmg` adds: every
//! event is emitted under the name `docs/specs/gui.md` gives it.

use hiveme_core::session::SessionEvent;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

use crate::protocol::{
  EVENT_MESSAGE, EVENT_NOTIFICATION_FIRED, EVENT_STATUS, EVENT_TOPIC_ADDED, NotificationFiredEvent, TopicAddedEvent,
};

/// Emits every session event until the session is gone.
///
/// A burst that outruns the frontend loses the oldest events rather than stalling the
/// broker connection. The frontend refreshes the tree and the status after every
/// message it hears, so the next event brings it back in line.
pub async fn forward(app: AppHandle, mut events: Receiver<SessionEvent>) {
  loop {
    match events.recv().await {
      Ok(event) => emit(&app, event),
      Err(RecvError::Lagged(skipped)) => log::warn!("the frontend fell behind, {skipped} event(s) were dropped"),
      Err(RecvError::Closed) => return,
    }
  }
}

fn emit(app: &AppHandle, event: SessionEvent) {
  let _ = match event {
    SessionEvent::Status(status) => app.emit(EVENT_STATUS, status),
    SessionEvent::Message(row) => app.emit(EVENT_MESSAGE, row),
    SessionEvent::TopicAdded { topic } => app.emit(EVENT_TOPIC_ADDED, TopicAddedEvent { topic }),
    SessionEvent::NotificationFired {
      rule_id,
      message_id,
      topic,
    } => app.emit(
      EVENT_NOTIFICATION_FIRED,
      NotificationFiredEvent {
        rule_id,
        message_id,
        topic,
      },
    ),
  };
}
