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

//! What each IPC command does.
//!
//! Orchestration only, and barely that: every command is one call into the shared
//! session of `hiveme-core`, which owns the config, the history, the broker connection,
//! and the rules, so that `hmc` and `hmg` cannot drift apart. The commands in `lib.rs`
//! are thin wrappers around this file.

use anyhow::Result;
use hiveme_core::Config;
use hiveme_core::session::Session;

use crate::protocol::{About, MessageRow, PublishOptions, Status, TopicNode, UpdateCheckResult};

/// Deletes the stored history of a topic.
pub fn clear_topic(session: &Session, topic: &str) -> Result<u64> {
  Ok(session.clear_topic(topic)?)
}

/// Connects to the broker and subscribes.
pub async fn connect(session: &Session) -> Result<Status> {
  Ok(session.connect().await?)
}

/// Says goodbye to the broker and ends the session.
pub async fn disconnect(session: &Session) -> Result<()> {
  Ok(session.disconnect().await?)
}

/// What the About tab shows.
pub fn get_about(session: &Session) -> Result<About> {
  Ok(session.about())
}

/// The setup string for `hmc --init`, from the saved broker settings.
pub fn get_broker_init(session: &Session) -> Result<String> {
  Ok(session.broker_init()?)
}

/// The config as the backend holds it.
pub fn get_config(session: &Session) -> Result<Config> {
  Ok(session.config())
}

/// One page of a topic's history, oldest first.
pub fn get_messages(session: &Session, topic: &str, before: Option<i64>, limit: u32) -> Result<Vec<MessageRow>> {
  Ok(session.messages(topic, before, limit)?)
}

/// The status bar snapshot.
pub fn get_status(session: &Session) -> Result<Status> {
  Ok(session.status())
}

/// What the release check found, once it has an answer.
pub fn get_update_result(session: &Session) -> Option<UpdateCheckResult> {
  session.update_result()
}

/// The topic tree, with the unread counts rolled up into every parent.
pub fn list_topics(session: &Session) -> Result<Vec<TopicNode>> {
  Ok(session.topic_tree()?)
}

/// Clears the unread count of a topic.
pub fn mark_read(session: &Session, topic: &str) -> Result<()> {
  Ok(session.mark_read(topic)?)
}

/// Publishes from the composer, through the same core path `hmc` uses.
pub async fn publish(session: &Session, topic: &str, body: &str, options: PublishOptions) -> Result<MessageRow> {
  Ok(session.publish(topic, body, options).await?)
}

/// Validates, writes, and applies a config the user saved.
pub async fn set_config(session: &Session, config: Config) -> Result<Config> {
  Ok(session.set_config(config).await?)
}

/// Holds notifications back, or lets them through again, for this session.
pub fn set_notifications_paused(session: &Session, paused: bool) -> Result<Status> {
  Ok(session.set_notifications_paused(paused))
}

/// Remembers that the user does not want to hear about this version again.
pub fn skip_version(session: &Session, version: &str) -> Result<()> {
  Ok(session.skip_version(version)?)
}
