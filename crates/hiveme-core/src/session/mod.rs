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

//! The backend both applications run on.
//!
//! The broker connection, the history, the notification rules, the config in memory,
//! and the update check, behind one API with an event stream. `hmg` wraps it in Tauri
//! commands and events; the terminal UI of `hmc` drives it directly. Neither
//! application holds logic of its own between the broker and the screen, so the two
//! cannot drift apart. Specified in `docs/specs/session.md`.
//!
//! The session owns no runtime. Every `async` operation runs on the caller's Tokio
//! runtime, and [`Session::start_background_work`] spawns onto the runtime it is called
//! from.

mod config;
mod history;
mod mqtt;
mod notify;
mod types;
mod update;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use tokio::sync::broadcast;

use crate::config::{BrokerInit, Config};
use crate::error::{Error, Result};
#[cfg(test)]
use crate::message::{CONTENT_TYPE, Level};
use crate::message::{Message, build_envelope, raw_json_payload, raw_json_properties};
use crate::mqtt::{Qos, Role};
use crate::storage::{NewMessage, PRUNE_INTERVAL_SECS, Store};

pub use config::{ConfigStore, needs_reconnect};
pub use history::build_tree;
#[cfg(target_os = "windows")]
pub use notify::register_toast_identity;
pub use notify::{Notifier, Toaster, TopmostNotification};
pub use types::{About, MessageRow, PublishOptions, SessionEvent, Status, TopicNode, UpdateCheckResult};
pub use update::{GITHUB_URL, RELEASES_API_URL, is_newer};

use mqtt::Mqtt;

/// How long either application waits for the broker session to end before it exits
/// anyway, so that an unreachable broker cannot hold a quit hostage.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// How many events wait for a receiver that has fallen behind before the oldest go.
const EVENT_CAPACITY: usize = 1_024;

/// Which application runs the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionApp {
  /// `hmg`.
  Gui,
  /// The terminal UI of `hmc`.
  Tui,
}

impl SessionApp {
  /// The MQTT role the application connects as.
  pub fn role(&self) -> Role {
    match self {
      Self::Gui => Role::Gui,
      Self::Tui => Role::Tui,
    }
  }

  /// The identifier `sender.app` carries for a message the application publishes.
  pub fn app_id(&self) -> &'static str {
    self.role().app()
  }
}

/// The shared backend of `hmg` and interactive `hmc`.
pub struct Session {
  app: SessionApp,
  config: Arc<ConfigStore>,
  store: Arc<Store>,
  notifier: Arc<Notifier>,
  mqtt: Mqtt,
  update: Arc<Mutex<Option<UpdateCheckResult>>>,
  events: broadcast::Sender<SessionEvent>,
}

impl Session {
  /// Loads the config, opens the history beside it, and compiles the rules.
  ///
  /// `config_path` is an explicit path such as `--config`; without one the location is
  /// resolved as `docs/specs/config.md` says. A config that cannot be read is kept as a
  /// load error and the session runs on defaults. Corrupt or incompatible history is
  /// recreated without changing the config. Other storage failures stop startup.
  pub fn open(config_path: Option<&Path>, app: SessionApp, toaster: Arc<dyn Toaster>) -> Result<Arc<Self>> {
    let path = crate::config::config_path(config_path)?;
    let config = Arc::new(ConfigStore::open(&path)?);
    let database_path = config.database_path();
    let store = match Store::open(&database_path) {
      Ok(store) => Arc::new(store),
      Err(error) => {
        log::error!(
          "the message history at {} could not be opened: {error}",
          database_path.display()
        );
        return Err(error);
      }
    };
    Ok(Self::with_parts(app, config, store, toaster))
  }

  /// Builds a session on a config and a store the caller already has, for tests.
  pub fn with_parts(
    app: SessionApp,
    config: Arc<ConfigStore>,
    store: Arc<Store>,
    toaster: Arc<dyn Toaster>,
  ) -> Arc<Self> {
    let notifier = Arc::new(Notifier::new(&config.get(), toaster));
    let events = broadcast::channel(EVENT_CAPACITY).0;
    let mqtt = Mqtt::new(app, store.clone(), notifier.clone(), config.clone(), events.clone());
    Arc::new(Self {
      app,
      config,
      store,
      notifier,
      mqtt,
      update: Arc::new(Mutex::new(None)),
      events,
    })
  }

  /// Which application this session belongs to.
  pub fn app(&self) -> SessionApp {
    self.app
  }

  /// Every event from now on. A receiver that falls behind loses the oldest ones.
  pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
    self.events.subscribe()
  }

  /// What the About tab shows.
  pub fn about(&self) -> About {
    let config = self.config.get();
    About {
      app_version: update::app_version().to_owned(),
      config_path: self.config_path().display().to_string(),
      database_path: self.database_path().display().to_string(),
      device_id: config.device.id,
      device_name: config.device.name,
      github_url: GITHUB_URL.to_owned(),
    }
  }

  /// The config as the session holds it.
  pub fn config(&self) -> Config {
    self.config.get()
  }

  /// Where the config file is.
  pub fn config_path(&self) -> PathBuf {
    self.config.path()
  }

  /// Where the history database is.
  pub fn database_path(&self) -> PathBuf {
    self.config.database_path()
  }

  /// Why the config file could not be read, when that happened.
  pub fn load_error(&self) -> Option<String> {
    self.config.load_error()
  }

  /// Validates, writes, and applies a config the user saved.
  ///
  /// The rules are recompiled either way, and the connection is remade only when
  /// something the CONNECT packet or the subscription list is built from changed.
  pub async fn set_config(&self, config: Config) -> Result<Config> {
    let before = self.config.get();
    self.config.set(config)?;
    let after = self.config.get();

    self.notifier.reload(&after);
    if needs_reconnect(&before, &after) {
      log::info!("the broker settings changed, reconnecting");
      if after.broker.url.trim().is_empty() {
        self.disconnect().await?;
      } else if let Err(error) = self.connect().await {
        // The config is saved either way. A cluster that is unreachable right now is
        // not a reason to refuse the settings the user typed.
        log::warn!("the new broker settings did not connect: {error}");
      }
    }
    Ok(after)
  }

  /// Writes a config change the user did not ask for, such as window geometry.
  pub fn set_config_quietly(&self, config: Config) -> Result<()> {
    self.config.set_quietly(config)
  }

  /// The setup string for `hmc --init`, from the saved broker settings.
  pub fn broker_init(&self) -> Result<String> {
    let init = BrokerInit::from_config(&self.config.get());
    init.validate()?;
    Ok(init.to_json())
  }

  /// The status bar snapshot.
  pub fn status(&self) -> Status {
    self.mqtt.shared().status()
  }

  /// Connects to the broker and subscribes, replacing a connection that is up.
  pub async fn connect(&self) -> Result<Status> {
    let result = self.mqtt.connect().await;
    if let Err(error) = &result {
      let mut status = Status::disconnected();
      status.last_error = Some(error.to_string());
      self.mqtt.shared().publish_status(status);
    }
    result
  }

  /// Says goodbye to the broker and ends its session.
  pub async fn disconnect(&self) -> Result<()> {
    self.mqtt.disconnect().await
  }

  /// The topic tree, with the unread counts rolled up into every parent.
  pub fn topic_tree(&self) -> Result<Vec<TopicNode>> {
    Ok(build_tree(&self.store.topics()?))
  }

  /// One page of a topic's history and all its descendants, oldest first.
  pub fn messages(&self, topic: &str, before: Option<i64>, limit: u32) -> Result<Vec<MessageRow>> {
    let rows = self.store.messages(topic, before, limit)?;
    Ok(rows.into_iter().map(|row| MessageRow::seen_by(row, self.app)).collect())
  }

  /// Clears the unread counts of a topic and all its descendants.
  pub fn mark_read(&self, topic: &str) -> Result<()> {
    self.store.mark_read(topic)
  }

  /// Marks a subtree read through a displayed message, preserving later arrivals.
  pub fn mark_read_through(&self, topic: &str, row_id: i64, msg_id: &str) -> Result<()> {
    self.store.mark_read_through(topic, row_id, msg_id)
  }

  /// Deletes the stored history of one topic.
  pub fn clear_topic(&self, topic: &str) -> Result<u64> {
    self.store.clear_topic(topic)
  }

  /// Publishes from a composer, through the same core path one-shot `hmc` uses.
  ///
  /// `topic` is the selected tree node; the optional input topic is relative to it. The
  /// row is stored once the broker has the message, so that a publish that failed never
  /// leaves a bubble claiming it was sent; the copy the broker echoes back onto this
  /// session's own subscription collapses into that row by its message id.
  ///
  /// The echo can beat the acknowledgement back, so the message is spoken for before it
  /// is sent rather than after it is stored: otherwise its own echo arrives at a session
  /// that has never heard of it and is drawn a second time, as somebody else's.
  pub async fn publish(&self, topic: &str, body: &str, options: PublishOptions) -> Result<MessageRow> {
    let resolved_topic = crate::topic::resolve_publish(topic, options.topic.as_deref().unwrap_or(""));
    let topic = resolved_topic.as_str();
    let config = self.config.get();

    // Encryption is designed in docs/specs/message.md but not implemented, and a config
    // that asks for it must not quietly get plaintext on the wire instead.
    if config.encryption.is_enabled() {
      return Err(Error::NotImplemented(format!(
        "encryption.mode is {} but encryption arrives in a later version of HiveMe; \
         set it to Off to publish in plain text",
        config.encryption.mode
      )));
    }
    crate::topic::validate_topic(topic).map_err(|reason| Error::InvalidTopic {
      topic: topic.to_owned(),
      reason,
    })?;
    if body.trim().is_empty() {
      return Err(Error::NothingToSend);
    }

    let qos = options
      .qos
      .and_then(Qos::from_u8)
      .unwrap_or_else(|| Qos::from_config(&config));
    let retain = options.retain.unwrap_or(config.publish.retain);

    let (payload, mut properties) = if options.json {
      (
        raw_json_payload(body).map_err(|source| Error::NotJson(source.to_string()))?,
        raw_json_properties(),
      )
    } else {
      let message = build_message(&config, self.app, body, &options);
      let payload = message.to_bytes().map_err(|source| Error::PublishRejected {
        topic: topic.to_owned(),
        reason: format!("the message cannot be encoded: {source}"),
      })?;
      (payload, message.mqtt_properties())
    };

    let mut row = NewMessage::from_payload(topic, payload.clone(), qos.as_u8(), retain, true);
    // A raw JSON publish carries no envelope, so the parser found no `sender.app` to
    // read and the row would not know which of the two applications sent it. It is
    // this one: say so, or the bubble that was just composed here arrives on the left.
    row.app.get_or_insert_with(|| self.app.app_id().to_owned());

    let shared = self.mqtt.shared();
    shared.remember(&row.topic, &row.msg_id);
    if options.json {
      // Keep raw JSON unchanged. An MQTT property identifies this publish so another
      // session sending the same bytes cannot be mistaken for our echo.
      properties
        .user_properties
        .push((mqtt::RAW_PUBLISH_ID.to_owned(), row.msg_id.clone()));
      shared.remember_raw(&row.topic, &row.msg_id);
    }
    if let Err(error) = self
      .mqtt
      .publish_bytes(topic, payload, qos, retain, Some(properties))
      .await
    {
      // A timeout does not cancel the accepted MQTT request; its echo can still arrive.
      if !matches!(error, Error::PublishTimeout { .. }) {
        shared.forget(&row.topic, &row.msg_id);
      }
      return Err(error);
    }

    let insertion = self.store.insert(&row)?;
    if insertion.topic_is_new {
      shared.emit(SessionEvent::TopicAdded {
        topic: topic.to_owned(),
      });
    }
    let message = MessageRow::seen_by(insertion.message, self.app);
    shared.emit(SessionEvent::Message(message.clone()));
    Ok(message)
  }

  /// Holds notifications back, or lets them through again, for this process.
  pub fn set_notifications_paused(&self, paused: bool) -> Status {
    self.notifier.set_paused(paused);
    let shared = self.mqtt.shared();
    shared.emit_status();
    shared.status()
  }

  /// What the release check found, once it has an answer.
  pub fn update_result(&self) -> Option<UpdateCheckResult> {
    self
      .update
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clone()
  }

  /// Remembers that the user does not want to hear about this version again.
  pub fn skip_version(&self, version: &str) -> Result<()> {
    let mut config = self.config.get();
    config.update.ignore_version = version.to_owned();
    self.config.set_quietly(config)
  }

  /// Starts the prune loop, the first connection, and the update check.
  ///
  /// Call it once the screen exists, from inside the Tokio runtime the session should
  /// work on.
  pub fn start_background_work(self: &Arc<Self>) {
    tokio::spawn(prune_forever(Arc::downgrade(self)));

    let config = self.config.get();
    // A config without a broker cannot connect, and saying so in the status bar is more
    // use than a failed connection attempt the user did not ask for.
    if config.broker.url.trim().is_empty() {
      log::info!("no broker is configured yet, not connecting");
    } else {
      let session = Arc::clone(self);
      tokio::spawn(async move {
        if let Err(error) = session.connect().await {
          log::warn!("the first connection did not come up: {error}");
        }
      });
    }

    update::start(self.config.clone(), self.update.clone());
  }

  /// Refuses new connections and publishes, and interrupts startup and subscription
  /// waits, so that nothing opens a connection behind a quit.
  pub fn begin_shutdown(&self) {
    self.mqtt.begin_shutdown();
  }

  /// Ends the broker connection and discards its session.
  ///
  /// The applications bound this with [`SHUTDOWN_TIMEOUT`] and exit whatever happens.
  pub async fn shutdown(&self) -> Result<()> {
    self.mqtt.shutdown().await
  }
}

impl std::fmt::Debug for Session {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("Session")
      .field("app", &self.app)
      .field("config", &self.config)
      .field("store", &self.store)
      .field("mqtt", &self.mqtt)
      .finish_non_exhaustive()
  }
}

/// Prunes the history at startup and every ten minutes after that, for as long as the
/// session exists.
async fn prune_forever(session: Weak<Session>) {
  let interval = Duration::from_secs(PRUNE_INTERVAL_SECS);
  loop {
    let Some(session) = session.upgrade() else {
      return;
    };
    let _ = tokio::task::spawn_blocking(move || {
      history::prune(&session.store, &session.config.get().gui.history);
    })
    .await;
    tokio::time::sleep(interval).await;
  }
}

/// The envelope of `docs/specs/message.md`, filled in from a composer.
fn build_message(config: &Config, app: SessionApp, body: &str, options: &PublishOptions) -> Message {
  build_envelope(
    config,
    app.app_id(),
    body,
    options.title.as_deref(),
    options.level.as_deref(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  struct Silent;

  impl Toaster for Silent {
    fn show(&self, _title: &str, _body: &str) -> std::result::Result<(), String> {
      Ok(())
    }
  }

  fn session(app: SessionApp) -> (tempfile::TempDir, Arc<Session>) {
    let directory = tempfile::tempdir().unwrap();
    let session = Session::open(Some(&directory.path().join("HiveMe.json")), app, Arc::new(Silent)).unwrap();
    (directory, session)
  }

  #[test]
  fn both_apps_start_with_unusable_history_and_preserve_the_config() {
    for app in [SessionApp::Gui, SessionApp::Tui] {
      for corrupt in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("HiveMe.json");
        let config = ConfigStore::open(&path).unwrap();
        let mut settings = config.get();
        settings.device.name = "preserve my settings".to_owned();
        config.set(settings.clone()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let database = config.database_path();
        if corrupt {
          std::fs::write(&database, b"not a database").unwrap();
        } else {
          rusqlite::Connection::open(&database)
            .unwrap()
            .execute_batch("CREATE TABLE topics(id INTEGER PRIMARY KEY, topic TEXT, unread INTEGER);")
            .unwrap();
        }
        let session = Session::open(Some(&path), app, Arc::new(Silent)).expect("the application can start");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(session.config().device, settings.device);
        assert!(session.store.topics().unwrap().is_empty());
        session
          .store
          .insert(&NewMessage::from_payload(
            "hiveme",
            b"new history".to_vec(),
            1,
            false,
            false,
          ))
          .unwrap();
        assert_eq!(
          session.store.messages("hiveme", None, 10).unwrap()[0].body,
          "new history"
        );
      }
    }
  }

  #[test]
  fn each_application_connects_as_its_own_role_and_names_itself_in_what_it_sends() {
    assert_eq!(SessionApp::Gui.role(), Role::Gui);
    assert_eq!(SessionApp::Tui.role(), Role::Tui);
    assert_eq!(SessionApp::Gui.app_id(), "hmg");
    assert_eq!(SessionApp::Tui.app_id(), "hmc");
  }

  #[test]
  fn a_composed_message_says_which_application_sent_it_and_carries_its_level() {
    let config = Config::default();
    let options = PublishOptions {
      level: Some("error".to_owned()),
      ..PublishOptions::default()
    };

    let message = build_message(&config, SessionApp::Gui, "Disk full", &options);
    assert_eq!(message.level(), Level::Error);
    assert_eq!(
      message.sender.as_ref().and_then(|sender| sender.app.clone()).as_deref(),
      Some("hmg")
    );

    let message = build_message(&config, SessionApp::Tui, "Disk full", &options);
    assert_eq!(
      message.sender.as_ref().and_then(|sender| sender.app.clone()).as_deref(),
      Some("hmc")
    );
  }

  #[test]
  fn a_blank_title_is_no_title() {
    let config = Config::default();

    let message = build_message(
      &config,
      SessionApp::Gui,
      "body",
      &PublishOptions {
        title: Some("   ".to_owned()),
        ..PublishOptions::default()
      },
    );

    assert_eq!(message.level(), Level::Info);
    assert_eq!(message.payload.and_then(|payload| payload.title), None);
  }

  #[test]
  fn a_raw_json_publish_never_claims_to_be_an_envelope() {
    let properties = raw_json_properties();

    assert_eq!(properties.content_type, CONTENT_TYPE);
    assert!(properties.user_properties.is_empty());
  }

  #[test]
  fn opening_writes_a_default_config_and_the_history_beside_it() {
    let (directory, session) = session(SessionApp::Tui);

    assert!(directory.path().join("HiveMe.json").is_file());
    assert!(directory.path().join("HiveMe.db").is_file());
    assert_eq!(session.about().config_path, session.config_path().display().to_string());
    assert_eq!(session.about().github_url, GITHUB_URL);
    assert_eq!(session.load_error(), None);
    assert_eq!(session.status().state, "Disconnected");
    assert!(session.topic_tree().unwrap().is_empty());
  }

  #[tokio::test]
  async fn publishing_refuses_what_it_cannot_send_before_touching_the_network() {
    let (_directory, session) = session(SessionApp::Gui);

    let empty = session.publish("hiveme", "  ", PublishOptions::default()).await;
    assert_eq!(empty.unwrap_err().to_string(), "there is nothing to send");

    let wildcard = session.publish("hiveme/#", "hi", PublishOptions::default()).await;
    assert!(matches!(wildcard, Err(Error::InvalidTopic { .. })), "{wildcard:?}");

    let not_json = session
      .publish(
        "hiveme",
        "{ nope",
        PublishOptions {
          json: true,
          ..PublishOptions::default()
        },
      )
      .await
      .unwrap_err();
    assert!(
      not_json
        .to_string()
        .starts_with("send as raw JSON was given input that is not JSON: "),
      "{not_json}"
    );

    let offline = session.publish("hiveme", "hi", PublishOptions::default()).await;
    assert_eq!(offline.unwrap_err().to_string(), "not connected to the broker");
    assert_eq!(
      session.messages("hiveme", None, 0).unwrap(),
      Vec::new(),
      "nothing was stored"
    );
  }

  #[tokio::test]
  async fn the_pause_toggle_answers_and_raises_the_status() {
    let (_directory, session) = session(SessionApp::Tui);
    let mut events = session.subscribe();

    let status = session.set_notifications_paused(true);

    assert!(status.notifications_paused);
    assert_eq!(events.recv().await.unwrap(), SessionEvent::Status(status));
  }

  #[test]
  fn skipping_a_version_is_remembered_in_the_config() {
    let (directory, session) = session(SessionApp::Gui);

    session.skip_version("0.2.0").unwrap();

    assert_eq!(session.config().update.ignore_version, "0.2.0");
    let saved = std::fs::read_to_string(directory.path().join("HiveMe.json")).unwrap();
    assert!(saved.contains("\"ignoreVersion\": \"0.2.0\""), "{saved}");
  }

  #[test]
  fn the_setup_string_is_refused_until_the_broker_is_usable() {
    let (_directory, session) = session(SessionApp::Gui);
    assert!(session.broker_init().is_err());
  }
  #[tokio::test]
  async fn a_saved_config_succeeds_even_when_its_connection_is_invalid() {
    let (_directory, session) = session(SessionApp::Gui);
    let mut config = session.config();
    config.broker.url = "mqtt://localhost".to_owned();
    config.broker.username.clear();
    let saved = session.set_config(config.clone()).await.unwrap();
    assert_eq!(saved, config);
    assert_eq!(
      crate::config::ConfigFile::load(&session.config_path())
        .unwrap()
        .config(),
      &config
    );
    assert!(session.status().last_error.is_some());
  }

  #[test]
  fn a_poisoned_update_result_is_recoverable() {
    let (_directory, session) = session(SessionApp::Gui);
    let update = session.update.clone();
    let _ = std::thread::spawn(move || {
      let _guard = update.lock().unwrap();
      panic!("test poison");
    })
    .join();
    assert!(session.update_result().is_none());
  }
}
