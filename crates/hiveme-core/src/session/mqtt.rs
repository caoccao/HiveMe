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

//! The broker connection of the session.
//!
//! [`crate::mqtt`] owns the protocol: TLS, the client identifier, reconnection with
//! backoff, and sending the subscriptions again when the broker has forgotten the
//! session. This module is the bridge from that client to the rest of the session:
//! every message that arrives is stored, turned into an event, and offered to the
//! notification rules.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{Mutex as AsyncMutex, broadcast, mpsc, watch};
use tokio::task::JoinHandle;

use crate::error::{Error, Result};
use crate::message::MessageProperties;
use crate::mqtt::{IncomingMessage, IncomingProperties, MqttClient, Qos, Role, State};
use crate::storage::{NewMessage, Store};

use super::SessionApp;
use super::config::ConfigStore;
use super::notify::Notifier;
use super::types::{MessageRow, SessionEvent, Status};

/// How many messages a session remembers having stored or sent.
const REMEMBERED_MESSAGES: usize = 10_000;

/// Identifies a raw publish without changing its payload or comparing its bytes.
pub(super) const RAW_PUBLISH_ID: &str = "hiveme-publish-id";

/// The messages this session stored or sent, oldest first and bounded.
///
/// A row that is already in the database is either one this process put there, whose
/// echo or second delivery raises nothing again, or one that another process on the same
/// `HiveMe.db` stored a moment earlier, which this process has not shown yet. The
/// database cannot tell the two apart, so the session remembers.
#[derive(Debug, Default)]
pub(super) struct Remembered {
  keys: HashSet<(String, String)>,
  order: VecDeque<(String, String)>,
}

impl Remembered {
  fn contains(&self, topic: &str, msg_id: &str) -> bool {
    self.keys.contains(&(topic.to_owned(), msg_id.to_owned()))
  }

  /// Remembers a message by topic and id, and says whether it was new to this session.
  pub(super) fn insert(&mut self, topic: &str, msg_id: &str) -> bool {
    let key = (topic.to_owned(), msg_id.to_owned());
    if self.keys.contains(&key) {
      return false;
    }
    if self.order.len() == REMEMBERED_MESSAGES
      && let Some(oldest) = self.order.pop_front()
    {
      self.keys.remove(&oldest);
    }
    self.keys.insert(key.clone());
    self.order.push_back(key);
    true
  }

  /// Forgets a message that was spoken for by a publish the broker never took.
  pub(super) fn forget(&mut self, topic: &str, msg_id: &str) {
    let key = (topic.to_owned(), msg_id.to_owned());
    if self.keys.remove(&key) {
      self.order.retain(|held| held != &key);
    }
  }
}

/// One live connection and the tasks that serve it.
struct Connection {
  client: Arc<MqttClient>,
  tasks: Vec<JoinHandle<()>>,
}

impl Drop for Connection {
  fn drop(&mut self) {
    for task in &self.tasks {
      task.abort();
    }
    // Also stops the client when a shutdown future is canceled at its deadline,
    // even if a publish still holds another Arc to it.
    if self.client.status_now().state != State::Disconnected {
      self.client.abort();
    }
  }
}

/// What the connection tasks share with the session.
pub(super) struct Shared {
  /// Which of the two applications is reading, which is what decides the side a bubble
  /// is drawn on. See [`MessageRow::seen_by`].
  app: SessionApp,
  /// The last status the screen was told, so `status` answers the same thing whether or
  /// not a connection exists.
  status: Mutex<Status>,
  received: AtomicU64,
  store: Arc<Store>,
  notifier: Arc<Notifier>,
  config: Arc<ConfigStore>,
  events: broadcast::Sender<SessionEvent>,
  remembered: Mutex<Remembered>,
  /// Only this session's raw publish IDs, retained to recognize repeated echoes too.
  raw_publishes: Mutex<Remembered>,
}

impl Shared {
  /// The status bar's view of the world.
  pub(super) fn status(&self) -> Status {
    let mut status = self.status.lock().unwrap().clone();
    status.messages_received = self.received.load(Ordering::Relaxed);
    status.database_bytes = self.store.size_bytes();
    status.notifications_paused = self.notifier.is_paused();
    status.config_error = self.config.load_error();
    status
  }

  /// Remembers a status and raises it.
  ///
  /// A status that names no error keeps the last one, so that the reason a connection
  /// dropped, or the filter a credential may not subscribe to, stays readable in the
  /// status bar after the connection has recovered.
  pub(super) fn publish_status(&self, status: Status) {
    {
      let mut held = self.status.lock().unwrap();
      let previous = held.last_error.take();
      *held = status;
      if held.last_error.is_none() {
        held.last_error = previous;
      }
    }
    self.emit_status();
  }

  /// Raises the status as it stands.
  pub(super) fn emit_status(&self) {
    self.emit(SessionEvent::Status(self.status()));
  }

  /// Remembers a message this session stored or sent, and says whether it was new to it.
  pub(super) fn remember(&self, topic: &str, msg_id: &str) -> bool {
    self.remembered.lock().unwrap().insert(topic, msg_id)
  }

  /// Remembers the ID attached to this session's raw publish as an MQTT property.
  pub(super) fn remember_raw(&self, topic: &str, msg_id: &str) {
    self.raw_publishes.lock().unwrap().insert(topic, msg_id);
  }

  /// Recognizes an own-session raw echo, never another publish with identical bytes.
  pub(super) fn raw_publish_id(&self, topic: &str, properties: &IncomingProperties) -> Option<String> {
    let id = properties.user_property(RAW_PUBLISH_ID)?;
    self
      .raw_publishes
      .lock()
      .unwrap()
      .contains(topic, id)
      .then(|| id.to_owned())
  }

  /// Takes back what was remembered for a publish the broker never took.
  pub(super) fn forget(&self, topic: &str, msg_id: &str) {
    self.remembered.lock().unwrap().forget(topic, msg_id);
    self.raw_publishes.lock().unwrap().forget(topic, msg_id);
  }

  /// A send with no receiver is not an error: nothing is on screen yet.
  pub(super) fn emit(&self, event: SessionEvent) {
    let _ = self.events.send(event);
  }
}

/// The broker connection, and everything that happens to what comes out of it.
pub(super) struct Mqtt {
  role: Role,
  connection: AsyncMutex<Option<Connection>>,
  /// Serializes connection replacement, manual disconnect, and application shutdown.
  lifecycle: AsyncMutex<()>,
  quitting: watch::Sender<bool>,
  shared: Arc<Shared>,
}

impl Mqtt {
  pub(super) fn new(
    app: SessionApp,
    store: Arc<Store>,
    notifier: Arc<Notifier>,
    config: Arc<ConfigStore>,
    events: broadcast::Sender<SessionEvent>,
  ) -> Self {
    Self {
      role: app.role(),
      connection: AsyncMutex::new(None),
      lifecycle: AsyncMutex::new(()),
      quitting: watch::channel(false).0,
      shared: Arc::new(Shared {
        app,
        status: Mutex::new(Status::disconnected()),
        received: AtomicU64::new(0),
        store,
        notifier,
        config,
        events,
        remembered: Mutex::new(Remembered::default()),
        raw_publishes: Mutex::new(Remembered::default()),
      }),
    }
  }

  pub(super) fn shared(&self) -> &Shared {
    &self.shared
  }

  /// Connects with the saved config and subscribes to every configured filter.
  ///
  /// Replaces a connection that is already up, which is what a saved change to the
  /// broker or the subscriptions needs.
  pub(super) async fn connect(&self) -> Result<Status> {
    let _lifecycle = self.lifecycle.lock().await;
    self.ensure_running()?;
    let config = self.shared.config.get();
    self.stop_connection().await;
    self.ensure_running()?;

    let mut client = MqttClient::start(&config, self.role)?;
    self.shared.publish_status(Status::from_client(&client.status_now()));
    let incoming = client
      .take_incoming()
      .ok_or_else(|| Error::ClientStopped("the incoming stream was already taken".to_owned()))?;
    let status_channel = client.status();
    let client = Arc::new(client);
    *self.connection.lock().await = Some(Connection {
      client: client.clone(),
      tasks: Vec::new(),
    });
    if let Err(error) = self.while_running(client.wait_until_connected()).await {
      // Shutdown owns the still-connecting client when canceled; on an ordinary
      // connection failure there is no active connection to keep.
      if !*self.quitting.borrow() {
        self.connection.lock().await.take();
      }
      return Err(error);
    }

    let filters = config.subscription_filters();
    let qos = Qos::from_config(&config);
    let mut subscribe_error = None;
    if let Err(error) = self.while_running(client.subscribe(filters.clone(), qos)).await {
      // A credential without permission for one filter is a configuration problem the
      // user has to see, but the rest of the session still works, so the connection
      // stays up and the reason goes to the status bar.
      log::warn!("the broker refused a subscription: {error}");
      subscribe_error = Some(error.to_string());
    }

    self.ensure_running()?;
    let mut status = Status::from_client(&client.status_now());
    status.last_error = subscribe_error;
    self.shared.publish_status(status);

    let tasks = vec![
      tokio::spawn(pump(self.shared.clone(), incoming)),
      tokio::spawn(watch(self.shared.clone(), status_channel)),
    ];
    self
      .connection
      .lock()
      .await
      .as_mut()
      .expect("the lifecycle lock keeps the connection alive")
      .tasks = tasks;
    Ok(self.shared.status())
  }

  /// Sends DISCONNECT, stops the tasks, and says so.
  pub(super) async fn disconnect(&self) -> Result<()> {
    let _lifecycle = self.lifecycle.lock().await;
    self.ensure_running()?;
    self.stop_connection().await;
    self.shared.publish_status(Status::disconnected());
    Ok(())
  }

  /// Prevents startup, settings changes, or queued operations from opening a connection.
  pub(super) fn begin_shutdown(&self) {
    self.quitting.send_replace(true);
  }

  /// Ends the broker session before the application exits.
  pub(super) async fn shutdown(&self) -> Result<()> {
    self.begin_shutdown();
    let _lifecycle = self.lifecycle.lock().await;
    let connection = self.connection.lock().await.take();
    if let Some(connection) = connection {
      connection.client.end_session().await?;
    }
    Ok(())
  }

  /// Cancels only the wait, keeping the client available for graceful shutdown.
  async fn while_running<T>(&self, operation: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    let mut quitting = self.quitting.subscribe();
    tokio::select! {
      biased;
      _ = quitting.wait_for(|value| *value) => Err(Error::Quitting),
      result = operation => result,
    }
  }

  pub(super) fn ensure_running(&self) -> Result<()> {
    if *self.quitting.borrow() {
      return Err(Error::Quitting);
    }
    Ok(())
  }

  /// The client of the live connection.
  async fn client(&self) -> Result<Arc<MqttClient>> {
    self.ensure_running()?;
    self
      .connection
      .lock()
      .await
      .as_ref()
      .map(|connection| connection.client.clone())
      .ok_or(Error::NotConnected)
  }

  /// Publishes an encoded message through the same core path one-shot `hmc` uses.
  pub(super) async fn publish_bytes(
    &self,
    topic: &str,
    payload: Vec<u8>,
    qos: Qos,
    retain: bool,
    properties: Option<MessageProperties>,
  ) -> Result<()> {
    self
      .client()
      .await?
      .publish(topic, payload, qos, retain, properties.as_ref())
      .await
  }

  /// Ends the current connection, if there is one.
  async fn stop_connection(&self) {
    let connection = self.connection.lock().await.take();
    if let Some(connection) = connection {
      if let Err(error) = connection.client.end_session().await {
        log::debug!("the broker connection did not close cleanly: {error}");
      }
      drop(connection);
    }
  }
}

impl std::fmt::Debug for Mqtt {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("Mqtt")
      .field("role", &self.role)
      .field("status", &self.shared.status.lock().unwrap().state)
      .finish()
  }
}

/// Stores everything the broker delivers, and turns it into events and notifications.
///
/// One blocking worker preserves arrival order without blocking Tokio. A separate
/// ordered worker for each notification channel keeps a slow OS daemon or permission
/// dialog from stalling storage or replacement of the topmost message.
async fn pump(shared: Arc<Shared>, mut incoming: mpsc::UnboundedReceiver<IncomingMessage>) {
  use super::notify::{Channel, NotificationPermit};
  type NotificationJob = (
    String,
    crate::message::Parsed,
    String,
    Arc<AtomicBool>,
    NotificationPermit,
  );
  let mut queues = Vec::new();
  let mut workers = Vec::new();
  for channel in [Channel::Os, Channel::Topmost] {
    let (send, mut receive) = mpsc::unbounded_channel::<NotificationJob>();
    queues.push(send);
    let notify_shared = shared.clone();
    workers.push(tokio::task::spawn_blocking(move || {
      while let Some((topic, parsed, message_id, fired, permit)) = receive.blocking_recv() {
        if let Some(rule_id) = notify_shared.notifier.notify_channel(&topic, &parsed, channel, permit)
          && !fired.swap(true, Ordering::Relaxed)
        {
          notify_shared.emit(SessionEvent::NotificationFired {
            rule_id,
            message_id,
            topic,
          });
        }
      }
    }));
  }
  let _ = tokio::task::spawn_blocking(move || {
    let mut warned = false;
    while let Some(message) = incoming.blocking_recv() {
      let backed_up = incoming.len() >= crate::mqtt::INCOMING_HIGH_WATER;
      if backed_up && !warned {
        log::warn!("the incoming backlog exceeds 1,024 messages; preserving queued messages");
      }
      warned = backed_up;
      // Snapshot before publishing any events: an observer may resume notifications
      // as soon as it sees this message, while the toast worker is still busy.
      let notification_permit = shared.notifier.permit();
      let parsed = message.parse();
      let raw_id = if matches!(parsed, crate::message::Parsed::Envelope(_)) {
        None
      } else {
        shared.raw_publish_id(&message.topic, &message.properties)
      };
      let mut row = NewMessage::from_parsed(
        message.topic.clone(),
        message.payload,
        &parsed,
        message.qos.as_u8(),
        message.retain,
        false,
      );
      // A payload without an envelope was given a generated id a moment ago, which the
      // echo of this session's own raw publish would not share with the row it belongs
      // to. Its MQTT publish ID identifies the echo independently of payload bytes.
      if !matches!(parsed, crate::message::Parsed::Envelope(_))
        && let Some(msg_id) = raw_id
      {
        row.msg_id = msg_id;
        row.app = Some(shared.app.app_id().to_owned());
        row.outgoing = true;
      }
      let insertion = match shared.store.insert(&row) {
        Ok(insertion) => insertion,
        Err(error) => {
          log::error!("a message on {} could not be stored: {error}", message.topic);
          continue;
        }
      };
      shared.received.fetch_add(1, Ordering::Relaxed);
      if insertion.topic_is_new {
        shared.emit(SessionEvent::TopicAdded {
          topic: message.topic.clone(),
        });
      }
      let new_here = shared.remember(&row.topic, &row.msg_id);
      if !new_here {
        // Known to this session already: the echo of its own publish, whose bubble the
        // composer has on screen, or the broker delivering a message a second time.
        // A database row can belong to another session, including a previous one. It
        // must not prevent a first delivery here from running the notification rules.
        continue;
      }
      shared.emit(SessionEvent::Message(MessageRow::seen_by(
        insertion.message,
        shared.app,
      )));
      if let Some(permit) = notification_permit {
        let fired = Arc::new(AtomicBool::new(false));
        for queue in &queues {
          let _ = queue.send((
            message.topic.clone(),
            parsed.clone(),
            row.msg_id.clone(),
            fired.clone(),
            permit,
          ));
        }
      }
    }
    log::debug!("the incoming message stream ended");
  })
  .await;
  for worker in workers {
    let _ = worker.await;
  }
}

/// Forwards every connection state change to the status bar.
async fn watch(shared: Arc<Shared>, mut channel: watch::Receiver<crate::mqtt::Status>) {
  while channel.changed().await.is_ok() {
    let status = channel.borrow_and_update().clone();
    shared.publish_status(Status::from_client(&status));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  struct Silent;

  impl super::super::notify::Toaster for Silent {
    fn show(&self, _title: &str, _body: &str) -> std::result::Result<(), String> {
      Ok(())
    }
  }

  #[test]
  fn a_session_remembers_the_newest_messages_it_stored() {
    let mut remembered = Remembered::default();
    assert!(remembered.insert("hiveme", "first"));
    assert!(!remembered.insert("hiveme", "first"), "the echo is recognized");
    assert!(remembered.insert("hiveme/other", "first"), "a topic is part of the key");
    for index in 0..REMEMBERED_MESSAGES {
      remembered.insert("hiveme/flood", &index.to_string());
    }
    assert_eq!(remembered.order.len(), REMEMBERED_MESSAGES);
    assert_eq!(remembered.keys.len(), REMEMBERED_MESSAGES);
    assert!(remembered.insert("hiveme", "first"), "the oldest were forgotten");
    assert!(!remembered.insert("hiveme/flood", &(REMEMBERED_MESSAGES - 1).to_string()));
  }

  #[tokio::test]
  async fn quitting_interrupts_network_waits_and_rejects_new_work() {
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let mqtt = Arc::new(Mqtt::new(
      SessionApp::Gui,
      Arc::new(Store::open(&config.database_path()).unwrap()),
      Arc::new(Notifier::new(&config.get(), Arc::new(Silent))),
      config,
      broadcast::channel(8).0,
    ));
    let waiting = tokio::spawn({
      let mqtt = mqtt.clone();
      async move { mqtt.while_running(std::future::pending::<Result<()>>()).await }
    });
    tokio::task::yield_now().await;
    mqtt.begin_shutdown();
    let error = tokio::time::timeout(Duration::from_secs(1), waiting)
      .await
      .expect("quitting interrupts a pending network operation")
      .unwrap()
      .unwrap_err();
    assert_eq!(error.to_string(), "the application is quitting");
    assert!(mqtt.while_running(async { Ok(()) }).await.is_err());
    assert!(mqtt.ensure_running().is_err());
    assert!(
      mqtt
        .publish_bytes("hiveme", vec![], Qos::AtMostOnce, false, None)
        .await
        .is_err()
    );
    mqtt.shutdown().await.unwrap();
    mqtt.shutdown().await.unwrap();
  }

  #[tokio::test]
  async fn raw_echo_identity_requires_this_sessions_publish_id_and_topic() {
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let mqtt = Mqtt::new(
      SessionApp::Tui,
      Arc::new(Store::in_memory().unwrap()),
      Arc::new(Notifier::new(&config.get(), Arc::new(Silent))),
      config,
      broadcast::channel(8).0,
    );
    let shared = mqtt.shared();
    shared.remember_raw("hiveme", "our-publish");
    let properties = |id: &str| IncomingProperties {
      user_properties: vec![(RAW_PUBLISH_ID.to_owned(), id.to_owned())],
      ..Default::default()
    };
    assert_eq!(shared.raw_publish_id("hiveme", &IncomingProperties::default()), None);
    assert_eq!(shared.raw_publish_id("hiveme", &properties("another-publish")), None);
    assert_eq!(shared.raw_publish_id("hiveme/other", &properties("our-publish")), None);
    for _ in 0..2 {
      assert_eq!(
        shared.raw_publish_id("hiveme", &properties("our-publish")),
        Some("our-publish".to_owned()),
        "repeated own echoes keep their identity"
      );
    }
    shared.forget("hiveme", "our-publish");
    assert_eq!(shared.raw_publish_id("hiveme", &properties("our-publish")), None);
  }

  #[tokio::test]
  async fn publishing_without_a_connection_says_so() {
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let mqtt = Mqtt::new(
      SessionApp::Tui,
      Arc::new(Store::in_memory().unwrap()),
      Arc::new(Notifier::new(&config.get(), Arc::new(Silent))),
      config,
      broadcast::channel(8).0,
    );

    let error = mqtt
      .publish_bytes("hiveme", b"{}".to_vec(), Qos::AtMostOnce, false, None)
      .await
      .unwrap_err();

    assert_eq!(error.to_string(), "not connected to the broker");
    assert!(error.is_connection());
  }
  #[tokio::test]
  async fn connecting_is_visible_while_the_broker_has_not_answered() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let mut settings = config.get();
    settings.broker.url = format!("mqtt://{}", listener.local_addr().unwrap());
    settings.broker.username = "test".to_owned();
    settings.broker.password = "test".to_owned();
    config.set(settings).unwrap();
    let (events, mut receiver) = broadcast::channel(8);
    let mqtt = Arc::new(Mqtt::new(
      SessionApp::Gui,
      Arc::new(Store::in_memory().unwrap()),
      Arc::new(Notifier::new(&config.get(), Arc::new(Silent))),
      config,
      events,
    ));
    let connect = tokio::spawn({
      let mqtt = mqtt.clone();
      async move { mqtt.connect().await }
    });
    let status = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
      .await
      .unwrap()
      .unwrap();
    assert!(matches!(status, SessionEvent::Status(status) if status.state == "Connecting"));
    assert!(!connect.is_finished(), "the server has not sent CONNACK");
    mqtt.begin_shutdown();
    assert!(connect.await.unwrap().is_err());
    // A stalled peer need not delay the rest of this test's teardown.
    drop(mqtt.connection.lock().await.take());
  }

  #[tokio::test]
  async fn topmost_replacement_does_not_wait_for_os_delivery_and_each_message_fires_once() {
    struct SlowOs {
      started: tokio::sync::mpsc::UnboundedSender<()>,
      release: Mutex<std::sync::mpsc::Receiver<()>>,
      topmost: tokio::sync::mpsc::UnboundedSender<String>,
    }
    impl super::super::notify::Toaster for SlowOs {
      fn show(&self, _: &str, _: &str) -> std::result::Result<(), String> {
        let _ = self.started.send(());
        let _ = self.release.lock().unwrap().recv_timeout(Duration::from_secs(3));
        Ok(())
      }
      fn show_topmost(&self, content: &super::super::TopmostNotification) -> std::result::Result<(), String> {
        self.topmost.send(content.body.clone()).map_err(|e| e.to_string())
      }
    }
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let mut settings = config.get();
    settings.notifications.topmost_enabled = true;
    for rule in &mut settings.notifications.rules {
      rule.os = true;
      rule.topmost = true;
    }
    let (started, mut os_started) = mpsc::unbounded_channel();
    let (topmost, mut shown) = mpsc::unbounded_channel();
    let (release, wait) = std::sync::mpsc::channel();
    let toaster = Arc::new(SlowOs {
      started,
      release: Mutex::new(wait),
      topmost,
    });
    let (events, mut received) = broadcast::channel(16);
    let mqtt = Mqtt::new(
      SessionApp::Gui,
      Arc::new(Store::in_memory().unwrap()),
      Arc::new(Notifier::new(&settings, toaster)),
      config,
      events,
    );
    let (send, incoming) = mpsc::unbounded_channel();
    let task = tokio::spawn(pump(mqtt.shared.clone(), incoming));
    let message = |body: &str, level| IncomingMessage {
      topic: "hiveme".into(),
      payload: crate::message::Message::new_text(crate::message::Sender::default(), body)
        .with_level(level)
        .to_bytes()
        .unwrap(),
      qos: Qos::AtLeastOnce,
      retain: false,
      properties: Default::default(),
    };
    send.send(message("first", crate::Level::Error)).unwrap();
    tokio::time::timeout(Duration::from_secs(1), os_started.recv())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(
      tokio::time::timeout(Duration::from_secs(1), shown.recv())
        .await
        .unwrap()
        .unwrap(),
      "first"
    );
    send.send(message("latest", crate::Level::Warn)).unwrap();
    assert_eq!(
      tokio::time::timeout(Duration::from_secs(1), shown.recv())
        .await
        .expect("the latest topmost notification must replace the first while OS delivery waits")
        .unwrap(),
      "latest"
    );
    assert_eq!(mqtt.shared.store.messages("hiveme", None, 10).unwrap().len(), 2);
    drop(release);
    drop(send);
    tokio::time::timeout(Duration::from_secs(1), task)
      .await
      .unwrap()
      .unwrap();
    let mut fired = Vec::new();
    while let Ok(event) = received.try_recv() {
      if let SessionEvent::NotificationFired { message_id, .. } = event {
        fired.push(message_id);
      }
    }
    assert_eq!(fired.len(), 2);
    assert_ne!(
      fired[0], fired[1],
      "both successful channels emit only one event per message"
    );
  }

  #[tokio::test]
  async fn pause_discards_both_worker_backlogs_and_messages_received_while_paused() {
    struct BlockedChannels {
      started: mpsc::UnboundedSender<&'static str>,
      release_os: Mutex<std::sync::mpsc::Receiver<()>>,
      release_topmost: Mutex<std::sync::mpsc::Receiver<()>>,
      os: Mutex<Vec<String>>,
      topmost: Mutex<Vec<String>>,
    }
    impl super::super::notify::Toaster for BlockedChannels {
      fn show(&self, _: &str, body: &str) -> std::result::Result<(), String> {
        self.os.lock().unwrap().push(body.to_owned());
        if body == "before" {
          self.started.send("os").unwrap();
          self
            .release_os
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        }
        Ok(())
      }
      fn show_topmost(&self, content: &super::super::TopmostNotification) -> std::result::Result<(), String> {
        self.topmost.lock().unwrap().push(content.body.clone());
        if content.body == "before" {
          self.started.send("topmost").unwrap();
          self
            .release_topmost
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        }
        Ok(())
      }
    }
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let mut settings = config.get();
    settings.notifications.topmost_enabled = true;
    for rule in &mut settings.notifications.rules {
      rule.os = true;
      rule.topmost = true;
    }
    let (started, mut waiting) = mpsc::unbounded_channel();
    let (release_os, wait_os) = std::sync::mpsc::channel();
    let (release_topmost, wait_topmost) = std::sync::mpsc::channel();
    let toaster = Arc::new(BlockedChannels {
      started,
      release_os: Mutex::new(wait_os),
      release_topmost: Mutex::new(wait_topmost),
      os: Mutex::default(),
      topmost: Mutex::default(),
    });
    let notifier = Arc::new(Notifier::new(&settings, toaster.clone()));
    let store = Arc::new(Store::in_memory().unwrap());
    let (events, mut received) = broadcast::channel(32);
    let mqtt = Mqtt::new(SessionApp::Gui, store.clone(), notifier.clone(), config, events);
    let (send, incoming) = mpsc::unbounded_channel();
    let worker = tokio::spawn(pump(mqtt.shared.clone(), incoming));
    let message = |body: &str| IncomingMessage {
      topic: "hiveme".to_owned(),
      payload: crate::Message::new_text(crate::message::Sender::default(), body)
        .to_bytes()
        .unwrap(),
      qos: Qos::AtLeastOnce,
      retain: false,
      properties: Default::default(),
    };
    send.send(message("before")).unwrap();
    for _ in 0..2 {
      tokio::time::timeout(Duration::from_secs(1), waiting.recv())
        .await
        .unwrap()
        .unwrap();
    }
    for body in ["queued", "paused", "fresh"] {
      if body == "paused" {
        notifier.set_paused(true);
      }
      if body == "fresh" {
        notifier.set_paused(false);
      }
      send.send(message(body)).unwrap();
      tokio::time::timeout(Duration::from_secs(1), async {
        loop {
          if let SessionEvent::Message(row) = received.recv().await.unwrap()
            && row.body == body
          {
            break;
          }
        }
      })
      .await
      .unwrap();
    }
    release_os.send(()).unwrap();
    release_topmost.send(()).unwrap();
    drop(send);
    tokio::time::timeout(Duration::from_secs(2), worker)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(*toaster.os.lock().unwrap(), ["before", "fresh"]);
    assert_eq!(*toaster.topmost.lock().unwrap(), ["before", "fresh"]);
    assert_eq!(
      store.messages("hiveme", None, 10).unwrap().len(),
      4,
      "pause only affects notifications"
    );
    let mut fired = 0;
    while let Ok(event) = received.try_recv() {
      if matches!(event, SessionEvent::NotificationFired { .. }) {
        fired += 1;
      }
    }
    assert_eq!(
      fired, 2,
      "discarded notifications raise no fired event on either channel"
    );
  }

  #[tokio::test]
  async fn a_slow_toaster_blocks_neither_storage_nor_the_async_runtime() {
    struct Slow {
      entered: tokio::sync::mpsc::UnboundedSender<()>,
      release: Mutex<std::sync::mpsc::Receiver<()>>,
      shown: Mutex<Vec<String>>,
    }
    impl super::super::notify::Toaster for Slow {
      fn show(&self, _: &str, body: &str) -> std::result::Result<(), String> {
        self.shown.lock().unwrap().push(body.to_owned());
        let _ = self.entered.send(());
        let _ = self.release.lock().unwrap().recv_timeout(Duration::from_secs(3));
        Ok(())
      }
    }
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let store = Arc::new(Store::in_memory().unwrap());
    let (entered, mut started) = tokio::sync::mpsc::unbounded_channel();
    let (release, wait) = std::sync::mpsc::channel();
    let toaster = Arc::new(Slow {
      entered,
      release: Mutex::new(wait),
      shown: Mutex::new(Vec::new()),
    });
    let mut settings = config.get();
    for rule in &mut settings.notifications.rules {
      rule.os = true;
    }
    let (events, mut event_rx) = broadcast::channel(16);
    let mqtt = Mqtt::new(
      SessionApp::Gui,
      store.clone(),
      Arc::new(Notifier::new(&settings, toaster.clone())),
      config,
      events,
    );
    let (send, receive) = mpsc::unbounded_channel();
    let pump = tokio::spawn(pump(mqtt.shared.clone(), receive));
    let message = crate::message::Message::new_text(crate::message::Sender::default(), "first")
      .with_level(crate::message::Level::Error);
    send
      .send(IncomingMessage {
        topic: "hiveme".to_owned(),
        payload: message.to_bytes().unwrap(),
        qos: Qos::AtLeastOnce,
        retain: false,
        properties: Default::default(),
      })
      .unwrap();
    tokio::time::timeout(Duration::from_secs(1), started.recv())
      .await
      .unwrap()
      .unwrap();
    mqtt.shared.notifier.set_paused(true);
    let paused = crate::message::Message::new_text(crate::message::Sender::default(), "received while paused")
      .with_level(crate::message::Level::Error);
    send
      .send(IncomingMessage {
        topic: "hiveme".to_owned(),
        payload: paused.to_bytes().unwrap(),
        qos: Qos::AtLeastOnce,
        retain: false,
        properties: Default::default(),
      })
      .unwrap();
    let started_at = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(1), async {
      loop {
        if let SessionEvent::Message(row) = event_rx.recv().await.unwrap()
          && row.id == paused.id
        {
          break;
        }
      }
    })
    .await
    .expect("the second row must persist while the first toast is blocked");
    assert!(started_at.elapsed() < Duration::from_secs(1));
    assert_eq!(store.messages("hiveme", None, 10).unwrap().len(), 2);
    // Resuming resets the rate limiter. A paused message incorrectly queued behind
    // the first toast would therefore become visible when that worker resumes.
    mqtt.shared.notifier.set_paused(false);
    drop(release);
    drop(send);
    tokio::time::timeout(Duration::from_secs(1), pump)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(*toaster.shown.lock().unwrap(), ["first"]);
  }
}
