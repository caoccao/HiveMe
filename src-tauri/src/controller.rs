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
//! Orchestration only: the config format, the message envelope, the topic rules, the
//! MQTT protocol, and the history all belong to `hiveme-core`, so that `hmc` and `hmg`
//! cannot drift apart. The commands in `lib.rs` are thin wrappers around this file.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Result, anyhow};
use hiveme_core::config::{BrokerInit, Config};
use hiveme_core::message::{CONTENT_TYPE, Level, Message, MessageProperties, Sender};
use hiveme_core::storage::TopicRow;
use hiveme_core::{NewMessage, Qos, Store};
use tauri::{AppHandle, Emitter, Manager};

use crate::config;
use crate::constants::{APP_ID, GITHUB_URL};
use crate::protocol::{
  About, AppState, EVENT_MESSAGE, EVENT_TOPIC_ADDED, MessageRow, PublishOptions, Status, TopicAddedEvent, TopicNode,
  UpdateCheckResult,
};
use crate::update;

/// What the About tab shows.
pub fn get_about() -> Result<About> {
  let config = config::get_config();
  Ok(About {
    app_version: update::app_version().to_owned(),
    config_path: config::path().display().to_string(),
    database_path: config::database_path().display().to_string(),
    device_id: config.device.id,
    device_name: config.device.name,
    github_url: GITHUB_URL.to_owned(),
  })
}

/// The config as the backend holds it.
pub fn get_config() -> Result<Config> {
  Ok(config::get_config())
}

/// Validates, writes, and applies a config the user saved.
///
/// The rules are recompiled either way, and the connection is remade only when
/// something the CONNECT packet or the subscription list is built from changed.
pub async fn set_config(app: &AppHandle, config: Config) -> Result<Config> {
  let before = config::get_config();
  config::set_config(config)?;
  let after = config::get_config();

  if let Some(state) = app.try_state::<AppState>() {
    state.notifier.reload(&after);
  }
  if config::needs_reconnect(&before, &after) {
    log::info!("the broker settings changed, reconnecting");
    if after.broker.url.trim().is_empty() {
      disconnect(app).await?;
    } else if let Err(error) = connect(app).await {
      // The config is saved either way. A cluster that is unreachable right now is
      // not a reason to refuse the settings the user typed.
      log::warn!("the new broker settings did not connect: {error}");
      return Err(error);
    }
  }
  Ok(after)
}

/// The status bar snapshot.
pub fn get_status(app: &AppHandle) -> Result<Status> {
  let state = app
    .try_state::<AppState>()
    .ok_or_else(|| anyhow!("the application is still starting"))?;
  Ok(state.mqtt.status())
}

/// The setup string for `hmc --init`, from the saved broker settings.
pub fn get_broker_init() -> Result<String> {
  let config = config::get_config();
  let init = BrokerInit::from_config(&config);
  init.validate().map_err(|error| anyhow!(error.to_string()))?;
  Ok(init.to_json())
}

/// Connects to the broker and subscribes.
pub async fn connect(app: &AppHandle) -> Result<Status> {
  let mqtt = {
    let state = app
      .try_state::<AppState>()
      .ok_or_else(|| anyhow!("the application is still starting"))?;
    state.mqtt.clone()
  };
  mqtt.connect(app).await
}

/// Says goodbye to the broker and stops the session.
pub async fn disconnect(app: &AppHandle) -> Result<()> {
  let mqtt = {
    let state = app
      .try_state::<AppState>()
      .ok_or_else(|| anyhow!("the application is still starting"))?;
    state.mqtt.clone()
  };
  mqtt.disconnect(app).await
}

/// The topic tree, with the unread counts rolled up into every parent.
pub fn list_topics(store: &Store) -> Result<Vec<TopicNode>> {
  let topics = store.topics().map_err(|error| anyhow!(error.to_string()))?;
  Ok(build_tree(&topics))
}

/// One page of a topic's history, oldest first.
pub fn get_messages(store: &Store, topic: &str, before: Option<i64>, limit: u32) -> Result<Vec<MessageRow>> {
  let rows = store
    .messages(topic, before, limit)
    .map_err(|error| anyhow!(error.to_string()))?;
  Ok(rows.into_iter().map(MessageRow::from).collect())
}

/// Clears the unread count of a topic.
pub fn mark_read(store: &Store, topic: &str) -> Result<()> {
  store.mark_read(topic).map_err(|error| anyhow!(error.to_string()))
}

/// Deletes the stored history of a topic.
pub fn clear_topic(store: &Store, topic: &str) -> Result<u64> {
  store.clear_topic(topic).map_err(|error| anyhow!(error.to_string()))
}

/// Publishes from the composer, through the same core path `hmc` uses.
///
/// The topic is absolute, because it is the tree node the user selected. The row is
/// stored once the broker has the message, so that a publish that failed never leaves
/// a bubble claiming it was sent; the copy the broker echoes back onto the GUI's own
/// subscription collapses into that row by its message id.
pub async fn publish(app: &AppHandle, topic: &str, body: &str, options: PublishOptions) -> Result<MessageRow> {
  let config = config::get_config();

  // Encryption is designed in docs/specs/message.md but not implemented, and a config
  // that asks for it must not quietly get plaintext on the wire instead.
  if config.encryption.is_enabled() {
    return Err(anyhow!(
      "encryption.mode is {} but encryption arrives in a later version of HiveMe; \
       set it to Off to publish in plain text",
      config.encryption.mode
    ));
  }
  hiveme_core::topic::validate_topic(topic).map_err(|reason| anyhow!("{topic}: {reason}"))?;
  if body.trim().is_empty() {
    return Err(anyhow!("there is nothing to send"));
  }

  let qos = options
    .qos
    .and_then(Qos::from_u8)
    .unwrap_or_else(|| Qos::from_config(&config));
  let retain = options.retain.unwrap_or(config.publish.retain);

  let (mqtt, store, level) = {
    let state = app
      .try_state::<AppState>()
      .ok_or_else(|| anyhow!("the application is still starting"))?;
    // A rule is consulted whether or not it is enabled, because `enabled` governs
    // notifications rather than what a topic means.
    let level = match options.level.as_deref() {
      Some(level) => Level::parse(level),
      None => state.notifier.level_for_topic(topic),
    };
    (state.mqtt.clone(), state.store.clone(), level)
  };

  let payload = if options.json {
    serde_json::from_str::<serde_json::Value>(body)
      .map_err(|source| anyhow!("send as raw JSON was given input that is not JSON: {source}"))?;
    let payload = body.as_bytes().to_vec();
    mqtt
      .publish_bytes(topic, payload.clone(), qos, retain, Some(raw_json_properties()))
      .await?;
    payload
  } else {
    let message = build_message(&config, body, &options, level);
    let payload = message
      .to_bytes()
      .map_err(|source| anyhow!("the message cannot be encoded: {source}"))?;
    mqtt.publish_message(topic, &message, qos, retain).await?;
    payload
  };

  let row = NewMessage::from_payload(topic, payload, qos.as_u8(), retain, true);
  let insertion = store.insert(&row).map_err(|error| anyhow!(error.to_string()))?;
  if insertion.topic_is_new {
    let _ = app.emit(
      EVENT_TOPIC_ADDED,
      TopicAddedEvent {
        topic: topic.to_owned(),
      },
    );
  }
  let message = MessageRow::from(insertion.message);
  let _ = app.emit(EVENT_MESSAGE, message.clone());
  Ok(message)
}

/// Holds notifications back, or lets them through again, for this session.
pub fn set_notifications_paused(app: &AppHandle, paused: bool) -> Result<Status> {
  let state = app
    .try_state::<AppState>()
    .ok_or_else(|| anyhow!("the application is still starting"))?;
  state.notifier.set_paused(paused);
  Ok(state.mqtt.status())
}

/// What the release check found, once it has an answer.
pub fn get_update_result(app: &AppHandle) -> Option<UpdateCheckResult> {
  app
    .try_state::<AppState>()
    .and_then(|state| state.update.lock().unwrap().clone())
}

/// Remembers that the user does not want to hear about this version again.
pub fn skip_version(version: String) -> Result<()> {
  let mut config = config::get_config();
  config.update.ignore_version = version;
  config::set_config_quietly(config)
}

/// Shows the config file in the file manager.
pub fn open_config_file(app: &AppHandle) -> Result<()> {
  use tauri_plugin_opener::OpenerExt;

  let path = config::path();
  if path.exists() {
    app
      .opener()
      .reveal_item_in_dir(&path)
      .map_err(|error| anyhow!(error.to_string()))
  } else {
    app
      .opener()
      .open_path(config::directory().display().to_string(), None::<&str>)
      .map_err(|error| anyhow!(error.to_string()))
  }
}

/// Prunes the history at startup and every ten minutes after that.
pub async fn prune_forever(app: AppHandle) {
  let interval = Duration::from_secs(hiveme_core::storage::PRUNE_INTERVAL_SECS);
  loop {
    let store = {
      let Some(state) = app.try_state::<AppState>() else {
        return;
      };
      state.store.clone()
    };
    let history = config::get_config().gui.history;
    match store.prune(&history) {
      Ok(pruned) if pruned.total() > 0 => log::info!(
        "pruned {} message(s): {} over the per-topic cap, {} past the retention window",
        pruned.total(),
        pruned.by_count,
        pruned.by_age
      ),
      Ok(_) => log::debug!("nothing to prune"),
      Err(error) => log::warn!("the history could not be pruned: {error}"),
    }
    tokio::time::sleep(interval).await;
  }
}

/// The envelope of `docs/specs/message.md`, filled in from the composer.
fn build_message(config: &Config, body: &str, options: &PublishOptions, level: Level) -> Message {
  let sender = Sender::from_device(&config.device, APP_ID);
  let mut message = Message::new_text(sender, body).with_level(level);
  if let Some(title) = options
    .title
    .as_deref()
    .map(str::trim)
    .filter(|title| !title.is_empty())
  {
    message = message.with_title(title);
  }
  message
}

/// The MQTT 5 properties of a raw JSON publish.
///
/// The content type still says JSON, because it is, but there is no `hiveme-v`
/// property: the payload is not a HiveMe envelope and a reader must not be told it is.
fn raw_json_properties() -> MessageProperties {
  MessageProperties {
    content_type: CONTENT_TYPE,
    user_properties: Vec::new(),
    message_expiry_interval: None,
  }
}

/// Builds the topic tree by splitting every stored topic on `/`.
///
/// A node exists for every segment, so `hiveme/build/ci` puts `build` in the tree even
/// though nothing was ever published to it; such a node has no `topic` and cannot be
/// selected. Counts are rolled up, which is what makes a collapsed branch show that
/// something below it is unread.
fn build_tree(topics: &[TopicRow]) -> Vec<TopicNode> {
  #[derive(Default)]
  struct Node {
    children: BTreeMap<String, Node>,
    unread: u32,
    messages: u32,
    is_topic: bool,
  }

  fn insert(node: &mut Node, segments: &[&str], row: &TopicRow) {
    node.unread = node.unread.saturating_add(row.unread);
    node.messages = node.messages.saturating_add(row.messages);
    match segments.split_first() {
      None => node.is_topic = true,
      Some((head, rest)) => insert(node.children.entry((*head).to_owned()).or_default(), rest, row),
    }
  }

  fn convert(path: &str, label: &str, node: &Node) -> TopicNode {
    TopicNode {
      id: path.to_owned(),
      label: label.to_owned(),
      topic: node.is_topic.then(|| path.to_owned()),
      unread: node.unread,
      messages: node.messages,
      children: node
        .children
        .iter()
        .map(|(segment, child)| convert(&format!("{path}/{segment}"), segment, child))
        .collect(),
    }
  }

  let mut root = Node::default();
  for row in topics {
    let segments: Vec<&str> = row.topic.split('/').collect();
    insert(&mut root, &segments, row);
  }
  root
    .children
    .iter()
    .map(|(segment, child)| convert(segment, segment, child))
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn row(topic: &str, unread: u32, messages: u32) -> TopicRow {
    TopicRow {
      topic: topic.to_owned(),
      first_seen_ts: "2026-09-12T09:41:23.512Z".to_owned(),
      last_seen_ts: "2026-09-12T09:41:23.512Z".to_owned(),
      unread,
      messages,
    }
  }

  #[test]
  fn the_tree_follows_the_topic_hierarchy() {
    let tree = build_tree(&[row("hiveme/info", 2, 5), row("hiveme/build/ci", 1, 3)]);

    assert_eq!(tree.len(), 1);
    let root = &tree[0];
    assert_eq!(root.id, "hiveme");
    assert_eq!(root.label, "hiveme");
    assert_eq!(root.topic, None, "nothing was published to the prefix itself");
    assert_eq!(root.unread, 3, "unread rolls up into the parent");
    assert_eq!(root.messages, 8);

    let labels: Vec<&str> = root.children.iter().map(|child| child.label.as_str()).collect();
    assert_eq!(labels, ["build", "info"], "children are in alphabetical order");

    let build = &root.children[0];
    assert_eq!(build.id, "hiveme/build");
    assert_eq!(build.topic, None, "an intermediate segment is not selectable");
    assert_eq!(build.children[0].id, "hiveme/build/ci");
    assert_eq!(build.children[0].topic.as_deref(), Some("hiveme/build/ci"));
  }

  #[test]
  fn a_topic_that_is_also_a_parent_is_selectable_and_still_has_children() {
    let tree = build_tree(&[row("hiveme", 1, 1), row("hiveme/info", 0, 2)]);

    let root = &tree[0];
    assert_eq!(root.topic.as_deref(), Some("hiveme"));
    assert_eq!(root.messages, 3);
    assert_eq!(root.children.len(), 1);
  }

  #[test]
  fn a_topic_outside_the_prefix_is_its_own_root() {
    let tree = build_tree(&[row("hiveme/info", 0, 1), row("$SYS/broker/uptime", 0, 1)]);

    let roots: Vec<&str> = tree.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(roots, ["$SYS", "hiveme"]);
  }

  #[test]
  fn an_empty_store_has_an_empty_tree() {
    assert!(build_tree(&[]).is_empty());
  }

  #[test]
  fn a_composed_message_says_it_came_from_the_gui_and_carries_the_level_it_was_given() {
    let config = Config::default();

    let message = build_message(&config, "Disk full", &PublishOptions::default(), Level::Error);

    assert_eq!(message.level(), Level::Error);
    assert_eq!(
      message.sender.as_ref().and_then(|sender| sender.app.clone()).as_deref(),
      Some("hmg")
    );
  }

  #[test]
  fn a_blank_title_is_no_title() {
    let config = Config::default();

    let message = build_message(
      &config,
      "body",
      &PublishOptions {
        title: Some("   ".to_owned()),
        ..PublishOptions::default()
      },
      Level::Info,
    );

    assert_eq!(message.payload.and_then(|payload| payload.title), None);
  }

  #[test]
  fn a_raw_json_publish_never_claims_to_be_an_envelope() {
    let properties = raw_json_properties();

    assert_eq!(properties.content_type, CONTENT_TYPE);
    assert!(properties.user_properties.is_empty());
  }
}
