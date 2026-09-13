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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{Mutex as AsyncMutex, broadcast, mpsc, watch};
use tokio::task::JoinHandle;

use crate::error::{Error, Result};
use crate::message::{Message, MessageProperties};
use crate::mqtt::{IncomingMessage, MqttClient, Qos, Role, State};
use crate::storage::{NewMessage, Store};

use super::config::ConfigStore;
use super::notify::Notifier;
use super::types::{MessageRow, SessionEvent, Status};

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
  /// The last status the screen was told, so `status` answers the same thing whether or
  /// not a connection exists.
  status: Mutex<Status>,
  received: AtomicU64,
  store: Arc<Store>,
  notifier: Arc<Notifier>,
  config: Arc<ConfigStore>,
  events: broadcast::Sender<SessionEvent>,
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
  fn publish_status(&self, status: Status) {
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
    role: Role,
    store: Arc<Store>,
    notifier: Arc<Notifier>,
    config: Arc<ConfigStore>,
    events: broadcast::Sender<SessionEvent>,
  ) -> Self {
    Self {
      role,
      connection: AsyncMutex::new(None),
      lifecycle: AsyncMutex::new(()),
      quitting: watch::channel(false).0,
      shared: Arc::new(Shared {
        status: Mutex::new(Status::disconnected()),
        received: AtomicU64::new(0),
        store,
        notifier,
        config,
        events,
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

  /// Publishes a HiveMe envelope through the same core path one-shot `hmc` uses.
  pub(super) async fn publish_message(&self, topic: &str, message: &Message, qos: Qos, retain: bool) -> Result<()> {
    self.client().await?.publish_message(topic, message, qos, retain).await
  }

  /// Publishes bytes HiveMe did not shape, as `hmc --json` does.
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
/// The store is SQLite, so each insert blocks for as long as one small statement
/// takes. That is short enough to do on the runtime, and doing it here keeps the
/// messages in the order the broker sent them, which is what a chat view shows.
async fn pump(shared: Arc<Shared>, mut incoming: mpsc::Receiver<IncomingMessage>) {
  while let Some(message) = incoming.recv().await {
    shared.received.fetch_add(1, Ordering::Relaxed);
    let parsed = message.parse();
    let row = NewMessage::from_payload(
      message.topic.clone(),
      message.payload.clone(),
      message.qos.as_u8(),
      message.retain,
      false,
    );
    let insertion = match shared.store.insert(&row) {
      Ok(insertion) => insertion,
      Err(error) => {
        log::error!("a message on {} could not be stored: {error}", message.topic);
        continue;
      }
    };
    if insertion.topic_is_new {
      shared.emit(SessionEvent::TopicAdded {
        topic: message.topic.clone(),
      });
    }
    if !insertion.is_new {
      // The echo of something this installation published. The bubble is already on
      // screen and the rules already had their say when it was sent.
      continue;
    }
    shared.emit(SessionEvent::Message(MessageRow::from(insertion.message)));
    if let Some(rule_id) = shared.notifier.notify(&message.topic, &parsed) {
      shared.emit(SessionEvent::NotificationFired {
        rule_id,
        message_id: row.msg_id.clone(),
        topic: message.topic.clone(),
      });
    }
  }
  log::debug!("the incoming message stream ended");
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

  #[tokio::test]
  async fn quitting_interrupts_network_waits_and_rejects_new_work() {
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let mqtt = Arc::new(Mqtt::new(
      Role::Gui,
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
  async fn publishing_without_a_connection_says_so() {
    let directory = tempfile::tempdir().unwrap();
    let config = Arc::new(ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap());
    let mqtt = Mqtt::new(
      Role::Tui,
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
}
