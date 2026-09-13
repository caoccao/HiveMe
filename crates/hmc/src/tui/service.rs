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

//! What the terminal UI asks of the session, as `src/lib/service.ts` is for `hmg`.
//!
//! [`App`](super::app::App) talks to the backend only through [`Service`], which
//! [`Session`] implements by calling itself. The trait exists so that the screens, the
//! key map, and the quit path can be tested against a scripted session: one whose
//! shutdown never finishes, or whose update check has an answer ready.

use std::future::Future;
use std::pin::Pin;

use hiveme_core::Result;
use hiveme_core::config::Config;
use hiveme_core::session::{About, MessageRow, Session, SessionEvent, Status, UpdateCheckResult};
use tokio::sync::broadcast;

/// An operation of the session that waits on the broker.
pub type Pending<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// The session operations the terminal UI uses.
pub trait Service: Send + Sync + 'static {
  fn subscribe(&self) -> broadcast::Receiver<SessionEvent>;
  fn about(&self) -> About;
  fn config(&self) -> Config;
  fn set_config(&self, config: Config) -> Pending<'_, Config>;
  fn status(&self) -> Status;
  fn connect(&self) -> Pending<'_, Status>;
  fn disconnect(&self) -> Pending<'_, ()>;
  fn messages(&self, topic: &str, before: Option<i64>, limit: u32) -> Result<Vec<MessageRow>>;
  fn mark_read(&self, topic: &str) -> Result<()>;
  fn clear_topic(&self, topic: &str) -> Result<u64>;
  fn set_notifications_paused(&self, paused: bool) -> Status;
  fn update_result(&self) -> Option<UpdateCheckResult>;
  fn skip_version(&self, version: &str) -> Result<()>;
  fn begin_shutdown(&self);
  fn shutdown(&self) -> Pending<'_, ()>;
}

impl Service for Session {
  fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
    Session::subscribe(self)
  }

  fn about(&self) -> About {
    Session::about(self)
  }

  fn config(&self) -> Config {
    Session::config(self)
  }

  fn set_config(&self, config: Config) -> Pending<'_, Config> {
    Box::pin(Session::set_config(self, config))
  }

  fn status(&self) -> Status {
    Session::status(self)
  }

  fn connect(&self) -> Pending<'_, Status> {
    Box::pin(Session::connect(self))
  }

  fn disconnect(&self) -> Pending<'_, ()> {
    Box::pin(Session::disconnect(self))
  }

  fn messages(&self, topic: &str, before: Option<i64>, limit: u32) -> Result<Vec<MessageRow>> {
    Session::messages(self, topic, before, limit)
  }

  fn mark_read(&self, topic: &str) -> Result<()> {
    Session::mark_read(self, topic)
  }

  fn clear_topic(&self, topic: &str) -> Result<u64> {
    Session::clear_topic(self, topic)
  }

  fn set_notifications_paused(&self, paused: bool) -> Status {
    Session::set_notifications_paused(self, paused)
  }

  fn update_result(&self) -> Option<UpdateCheckResult> {
    Session::update_result(self)
  }

  fn skip_version(&self, version: &str) -> Result<()> {
    Session::skip_version(self, version)
  }

  fn begin_shutdown(&self) {
    Session::begin_shutdown(self)
  }

  fn shutdown(&self) -> Pending<'_, ()> {
    Box::pin(Session::shutdown(self))
  }
}
