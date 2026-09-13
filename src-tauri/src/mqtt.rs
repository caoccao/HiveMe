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

//! The broker connection of `hmg`.
//!
//! [`hiveme_core::mqtt`] owns the protocol: TLS, the client identifier, reconnection
//! with backoff, and sending the subscriptions again when the broker has forgotten the
//! session. This module is the bridge from that client to the rest of the application:
//! every message that arrives is stored, turned into a frontend event, and offered to
//! the notification rules.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use hiveme_core::message::{Message, MessageProperties};
use hiveme_core::{MqttClient, NewMessage, Qos, Role, Store};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::task::JoinHandle;

use crate::notification::Notifier;
use crate::protocol::{EVENT_MESSAGE, EVENT_STATUS, EVENT_TOPIC_ADDED, MessageRow, Status, TopicAddedEvent};

/// One live connection and the tasks that serve it.
struct Session {
  client: Arc<MqttClient>,
  tasks: Vec<JoinHandle<()>>,
}

impl Drop for Session {
  fn drop(&mut self) {
    for task in &self.tasks {
      task.abort();
    }
    // Also stops the client when a shutdown future is canceled at its deadline,
    // even if a publish still holds another Arc to it.
    if self.client.status_now().state != hiveme_core::mqtt::State::Disconnected {
      self.client.abort();
    }
  }
}

/// The broker connection, and everything that happens to what comes out of it.
pub struct Mqtt {
  session: AsyncMutex<Option<Session>>,
  /// Serializes connection replacement, manual disconnect, and application shutdown.
  lifecycle: AsyncMutex<()>,
  quitting: watch::Sender<bool>,
  /// The last status the frontend was told, so `get_status` answers the same thing
  /// whether or not a connection exists.
  status: Mutex<Status>,
  store: Arc<Store>,
  notifier: Arc<Notifier>,
  received: Arc<AtomicU64>,
}

impl Mqtt {
  pub fn new(store: Arc<Store>, notifier: Arc<Notifier>, received: Arc<AtomicU64>) -> Self {
    Self {
      session: AsyncMutex::new(None),
      lifecycle: AsyncMutex::new(()),
      quitting: watch::channel(false).0,
      status: Mutex::new(Status::disconnected()),
      store,
      notifier,
      received,
    }
  }

  /// The status bar's view of the world.
  pub fn status(&self) -> Status {
    let mut status = self.status.lock().unwrap().clone();
    status.messages_received = self.received.load(Ordering::Relaxed);
    status.database_bytes = self.store.size_bytes();
    status.notifications_paused = self.notifier.is_paused();
    status.config_error = crate::config::load_error();
    status
  }

  /// Connects with the saved config and subscribes to every configured filter.
  ///
  /// Replaces a connection that is already up, which is what a saved change to the
  /// broker or the subscriptions needs.
  pub async fn connect(&self, app: &AppHandle) -> Result<Status> {
    let _lifecycle = self.lifecycle.lock().await;
    self.ensure_running()?;
    let config = crate::config::get_config();
    self.stop_session().await;
    self.ensure_running()?;

    let mut client = MqttClient::start(&config, Role::Gui)?;
    let incoming = client
      .take_incoming()
      .ok_or_else(|| anyhow!("the incoming stream was already taken"))?;
    let status_channel = client.status();
    let client = Arc::new(client);
    *self.session.lock().await = Some(Session {
      client: client.clone(),
      tasks: Vec::new(),
    });
    if let Err(error) = self.while_running(client.wait_until_connected()).await {
      // Shutdown owns the still-connecting client when canceled; on an ordinary
      // connection failure there is no active session to keep.
      if !*self.quitting.borrow() {
        self.session.lock().await.take();
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
    self.publish_status(app, status.clone());

    let tasks = vec![
      tokio::spawn(pump(
        app.clone(),
        self.store.clone(),
        self.notifier.clone(),
        self.received.clone(),
        incoming,
      )),
      tokio::spawn(watch(app.clone(), status_channel)),
    ];
    self
      .session
      .lock()
      .await
      .as_mut()
      .expect("the lifecycle lock keeps the session alive")
      .tasks = tasks;
    Ok(self.status())
  }

  /// Sends DISCONNECT, stops the tasks, and tells the frontend.
  pub async fn disconnect(&self, app: &AppHandle) -> Result<()> {
    let _lifecycle = self.lifecycle.lock().await;
    self.ensure_running()?;
    self.stop_session().await;
    self.publish_status(app, Status::disconnected());
    Ok(())
  }

  /// Prevents startup, settings changes, or queued commands from opening a new session.
  pub fn begin_shutdown(&self) {
    self.quitting.send_replace(true);
  }

  /// Ends the broker session before the application's event loop exits.
  pub async fn shutdown(&self) -> Result<()> {
    self.begin_shutdown();
    let _lifecycle = self.lifecycle.lock().await;
    let session = self.session.lock().await.take();
    if let Some(session) = session {
      session.client.end_session().await?;
    }
    Ok(())
  }

  /// Cancel only the wait, keeping the client available for graceful shutdown.
  async fn while_running<T>(&self, operation: impl std::future::Future<Output = hiveme_core::Result<T>>) -> Result<T> {
    let mut quitting = self.quitting.subscribe();
    tokio::select! {
      biased;
      _ = quitting.wait_for(|value| *value) => Err(anyhow!("the application is quitting")),
      result = operation => result.map_err(Into::into),
    }
  }

  fn ensure_running(&self) -> Result<()> {
    if *self.quitting.borrow() {
      return Err(anyhow!("the application is quitting"));
    }
    Ok(())
  }

  /// Publishes a HiveMe envelope through the same core path `hmc` uses.
  pub async fn publish_message(&self, topic: &str, message: &Message, qos: Qos, retain: bool) -> Result<()> {
    self.ensure_running()?;
    let session = self.session.lock().await;
    let client = session
      .as_ref()
      .map(|session| session.client.clone())
      .ok_or_else(|| anyhow!("not connected to the broker"))?;
    drop(session);
    client
      .publish_message(topic, message, qos, retain)
      .await
      .map_err(|error| anyhow!(error.to_string()))
  }

  /// Publishes bytes HiveMe did not shape, as `hmc --json` does.
  pub async fn publish_bytes(
    &self,
    topic: &str,
    payload: Vec<u8>,
    qos: Qos,
    retain: bool,
    properties: Option<MessageProperties>,
  ) -> Result<()> {
    self.ensure_running()?;
    let session = self.session.lock().await;
    let client = session
      .as_ref()
      .map(|session| session.client.clone())
      .ok_or_else(|| anyhow!("not connected to the broker"))?;
    drop(session);
    client
      .publish(topic, payload, qos, retain, properties.as_ref())
      .await
      .map_err(|error| anyhow!(error.to_string()))
  }

  /// Ends the current session, if there is one.
  async fn stop_session(&self) {
    let session = self.session.lock().await.take();
    if let Some(session) = session {
      if let Err(error) = session.client.end_session().await {
        log::debug!("the broker connection did not close cleanly: {error}");
      }
      drop(session);
    }
  }

  /// Remembers a status and sends it to the frontend.
  ///
  /// A status that names no error keeps the last one, so that the reason a connection
  /// dropped, or the filter a credential may not subscribe to, stays readable in the
  /// status bar after the connection has recovered.
  fn publish_status(&self, app: &AppHandle, status: Status) {
    {
      let mut held = self.status.lock().unwrap();
      let previous = held.last_error.take();
      *held = status;
      if held.last_error.is_none() {
        held.last_error = previous;
      }
    }
    let _ = app.emit(EVENT_STATUS, self.status());
  }
}

impl std::fmt::Debug for Mqtt {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("Mqtt")
      .field("status", &self.status.lock().unwrap().state)
      .finish()
  }
}

/// Stores everything the broker delivers, and turns it into events and notifications.
///
/// The store is SQLite, so each insert blocks for as long as one small statement
/// takes. That is short enough to do on the runtime, and doing it here keeps the
/// messages in the order the broker sent them, which is what the chat view shows.
async fn pump(
  app: AppHandle,
  store: Arc<Store>,
  notifier: Arc<Notifier>,
  received: Arc<AtomicU64>,
  mut incoming: tokio::sync::mpsc::Receiver<hiveme_core::mqtt::IncomingMessage>,
) {
  while let Some(message) = incoming.recv().await {
    received.fetch_add(1, Ordering::Relaxed);
    let parsed = message.parse();
    let row = NewMessage::from_payload(
      message.topic.clone(),
      message.payload.clone(),
      message.qos.as_u8(),
      message.retain,
      false,
    );
    let insertion = match store.insert(&row) {
      Ok(insertion) => insertion,
      Err(error) => {
        log::error!("a message on {} could not be stored: {error}", message.topic);
        continue;
      }
    };
    if insertion.topic_is_new {
      let _ = app.emit(
        EVENT_TOPIC_ADDED,
        TopicAddedEvent {
          topic: message.topic.clone(),
        },
      );
    }
    if !insertion.is_new {
      // The echo of something this installation published. The bubble is already on
      // screen and the rules already had their say when it was sent.
      continue;
    }
    let _ = app.emit(EVENT_MESSAGE, MessageRow::from(insertion.message));
    notifier.notify(&app, &message.topic, &parsed, &row.msg_id);
  }
  log::debug!("the incoming message stream ended");
}

/// Forwards every connection state change to the status bar.
async fn watch(app: AppHandle, mut channel: tokio::sync::watch::Receiver<hiveme_core::Status>) {
  while channel.changed().await.is_ok() {
    let status = channel.borrow_and_update().clone();
    let Some(state) = app.try_state::<crate::protocol::AppState>() else {
      return;
    };
    state.mqtt.publish_status(&app, Status::from_client(&status));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  #[tokio::test]
  async fn quitting_interrupts_network_waits_and_rejects_new_work() {
    let directory = tempfile::tempdir().unwrap();
    let mqtt = Arc::new(Mqtt::new(
      Arc::new(Store::open(&directory.path().join("messages.db")).unwrap()),
      Arc::new(Notifier::new(&hiveme_core::Config::default())),
      Arc::new(AtomicU64::new(0)),
    ));
    let waiting = tokio::spawn({
      let mqtt = mqtt.clone();
      async move {
        mqtt
          .while_running(std::future::pending::<hiveme_core::Result<()>>())
          .await
      }
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
}
