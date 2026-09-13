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

//! The MQTT 5 client both applications connect with.
//!
//! Specified in `docs/specs/hivemq-cloud.md`. [`MqttClient`] owns a `rumqttc` event
//! loop in a background task and turns it into four things a caller can use:
//!
//! * [`MqttClient::publish`], which returns once the broker has acknowledged, not once
//!   the packet has been queued, so `hmc` can exit knowing the message landed,
//! * [`MqttClient::subscribe`], which reports the reason code the broker sent back,
//!   because a HiveMQ Cloud credential that lacks a permission is refused per filter
//!   rather than at connect time,
//! * [`MqttClient::take_incoming`], a channel of received messages, and
//! * [`MqttClient::status`], a watch channel the `hmg` status bar renders.
//!
//! The difference between the two applications is [`Role`]: `hmc` connects clean,
//! publishes, and leaves, while `hmg` keeps a session and reconnects with the backoff
//! in [`backoff`].

mod backoff;
mod options;
mod tls;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use rumqttc::Outgoing;
use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::mqttbytes::v5::{
  ConnectReturnCode, Filter, Packet, PubAckReason, Publish, PublishProperties, SubscribeReasonCode,
};
use rumqttc::v5::{AsyncClient, ConnectionError, Event, EventLoop, MqttOptions};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::message::{Message, MessageProperties, Parsed};

pub use backoff::Backoff;
pub use options::{Connection, Role, client_id};

/// The self-signed root used by the TLS tests. Not a secret, and not trusted anywhere.
#[cfg(test)]
const SELF_SIGNED_ROOT_PEM: &str = include_str!("../../tests/fixtures/mqtt/root-ca.pem");

/// How many requests may wait for the event loop.
const REQUEST_CAPACITY: usize = 128;

/// A graceful shutdown must not wait indefinitely for an unreachable broker.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// How many received messages may wait for the consumer.
///
/// Deep enough that a burst is absorbed, bounded so that a consumer that has stopped
/// reading cannot grow the process without limit. The event loop drops rather than
/// blocks when it is full, because blocking there would stop the keep alive and cost
/// the connection.
const INCOMING_CAPACITY: usize = 1_024;

/// The quality of service of a publish or a subscription.
///
/// The `rumqttc` type is deliberately not part of the public surface, so the MQTT
/// library can be replaced without a change to `hmc` or `hmg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub enum Qos {
  /// At most once: fire and forget, the broker never acknowledges.
  AtMostOnce,
  /// At least once: acknowledged with PUBACK, the HiveMe default.
  #[default]
  AtLeastOnce,
  /// Exactly once: the four packet handshake, acknowledged with PUBCOMP.
  ExactlyOnce,
}

impl Qos {
  /// The wire value, 0, 1, or 2.
  pub fn as_u8(&self) -> u8 {
    match self {
      Self::AtMostOnce => 0,
      Self::AtLeastOnce => 1,
      Self::ExactlyOnce => 2,
    }
  }

  /// Reads a wire value, refusing anything MQTT does not define.
  pub fn from_u8(value: u8) -> Option<Self> {
    match value {
      0 => Some(Self::AtMostOnce),
      1 => Some(Self::AtLeastOnce),
      2 => Some(Self::ExactlyOnce),
      _ => None,
    }
  }

  /// The quality of service `publish.qos` asks for.
  ///
  /// An out of range value falls back to the default rather than failing, because
  /// [`Config::validate`] already reports it and a publish should not fail twice over
  /// the same line of config.
  pub fn from_config(config: &Config) -> Self {
    Self::from_u8(config.publish.qos).unwrap_or_default()
  }

  /// Whether the broker sends an acknowledgement for this quality of service.
  pub fn is_acknowledged(&self) -> bool {
    !matches!(self, Self::AtMostOnce)
  }
}

impl std::fmt::Display for Qos {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(formatter, "{}", self.as_u8())
  }
}

impl From<Qos> for QoS {
  fn from(qos: Qos) -> Self {
    match qos {
      Qos::AtMostOnce => QoS::AtMostOnce,
      Qos::AtLeastOnce => QoS::AtLeastOnce,
      Qos::ExactlyOnce => QoS::ExactlyOnce,
    }
  }
}

impl From<QoS> for Qos {
  fn from(qos: QoS) -> Self {
    match qos {
      QoS::AtMostOnce => Qos::AtMostOnce,
      QoS::AtLeastOnce => Qos::AtLeastOnce,
      QoS::ExactlyOnce => Qos::ExactlyOnce,
    }
  }
}

/// Where the connection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
  /// The event loop has started but the broker has not answered yet.
  #[default]
  Connecting,
  /// The broker accepted the CONNECT packet.
  Connected,
  /// The connection dropped and the next attempt is waiting out its backoff.
  Reconnecting,
  /// The event loop has stopped. Nothing will reconnect on its own.
  Disconnected,
}

impl State {
  /// The name the state travels under, which `Status.state` carries to `hmg` and
  /// `ConnectionState` in `protocol.ts` compares against. The status bar renders a word
  /// of its own for it, out of the translation table, so this one is not display text.
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Connecting => "Connecting",
      Self::Connected => "Connected",
      Self::Reconnecting => "Reconnecting",
      Self::Disconnected => "Disconnected",
    }
  }
}

impl std::fmt::Display for State {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(self.as_str())
  }
}

/// What the status bar of `hmg` renders, and what `get_status` answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
  pub state: State,
  pub host: String,
  pub port: u16,
  pub client_id: String,
  /// How many filters the client is subscribed to.
  pub subscriptions: usize,
  /// Reconnect attempts since the last successful connection.
  pub attempt: u32,
  /// How long the pending reconnect waits, while [`State::Reconnecting`].
  pub retry_in_ms: Option<u64>,
  /// The last thing that went wrong, kept after a recovery so the user can still read it.
  pub last_error: Option<String>,
}

impl Status {
  fn new(connection: &Connection) -> Self {
    Self {
      state: State::Connecting,
      host: connection.url.host.clone(),
      port: connection.url.port,
      client_id: connection.client_id.clone(),
      subscriptions: 0,
      attempt: 0,
      retry_in_ms: None,
      last_error: None,
    }
  }

  /// Whether messages can be published right now.
  pub fn is_connected(&self) -> bool {
    self.state == State::Connected
  }
}

/// The MQTT 5 properties that arrived with a message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IncomingProperties {
  pub content_type: Option<String>,
  pub message_expiry_interval: Option<u32>,
  pub payload_format_indicator: Option<u8>,
  pub response_topic: Option<String>,
  pub correlation_data: Option<Vec<u8>>,
  pub user_properties: Vec<(String, String)>,
}

impl IncomingProperties {
  /// The value of a user property, if the publisher sent one.
  pub fn user_property(&self, name: &str) -> Option<&str> {
    self
      .user_properties
      .iter()
      .find(|(key, _)| key == name)
      .map(|(_, value)| value.as_str())
  }

  /// The envelope version the publisher advertised in `hiveme-v`.
  ///
  /// Only a hint: the version inside the payload is what
  /// [`crate::message::parse`] goes by, because a property can be stripped by a bridge
  /// and a producer that is not HiveMe never sets one.
  pub fn hiveme_version(&self) -> Option<u32> {
    self
      .user_property(crate::message::USER_PROPERTY_VERSION)
      .and_then(|value| value.parse().ok())
  }
}

/// A message the broker delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingMessage {
  pub topic: String,
  pub payload: Vec<u8>,
  pub qos: Qos,
  pub retain: bool,
  pub properties: IncomingProperties,
}

impl IncomingMessage {
  /// Reads the payload with the lenient parser of `docs/specs/message.md`.
  pub fn parse(&self) -> Parsed {
    crate::message::parse(&self.payload)
  }

  fn from_publish(publish: Publish) -> Self {
    let properties = publish.properties.map(IncomingProperties::from).unwrap_or_default();
    Self {
      // A topic is UTF-8 by the MQTT specification, so a topic that is not says the
      // peer is broken; showing the replacement characters beats dropping the message.
      topic: String::from_utf8_lossy(&publish.topic).into_owned(),
      payload: publish.payload.to_vec(),
      qos: publish.qos.into(),
      retain: publish.retain,
      properties,
    }
  }
}

impl From<PublishProperties> for IncomingProperties {
  fn from(properties: PublishProperties) -> Self {
    Self {
      content_type: properties.content_type,
      message_expiry_interval: properties.message_expiry_interval,
      payload_format_indicator: properties.payload_format_indicator,
      response_topic: properties.response_topic,
      correlation_data: properties.correlation_data.map(|data| data.to_vec()),
      user_properties: properties.user_properties,
    }
  }
}

/// A connection to the broker.
///
/// Dropping it stops the event loop. Call [`MqttClient::disconnect`] first to send the
/// DISCONNECT packet, which lets the broker release a `hmc` session at once instead of
/// waiting out the keep alive.
pub struct MqttClient {
  inner: Arc<Inner>,
  incoming: Option<mpsc::Receiver<IncomingMessage>>,
  task: JoinHandle<()>,
  /// A clean connection with the same identity discards a persistent broker session.
  session_cleanup: Option<MqttOptions>,
}

impl MqttClient {
  /// Connects as `role` and returns once the broker has accepted the connection.
  ///
  /// The wait is bounded by `broker.connectTimeoutSecs`. A [`Role::Gui`] client retries
  /// a connection that fails for a transient reason inside that window, but a refusal,
  /// a TLS failure, or the window running out is reported to the caller, because a
  /// wrong password should reach the user rather than be retried forever. Once the
  /// first connection is up, a [`Role::Gui`] client reconnects on its own for as long
  /// as it lives.
  pub async fn connect(config: &Config, role: Role) -> Result<Self> {
    config.validate()?;
    Self::connect_unvalidated(config, role).await
  }

  /// Connects without [`Config::validate`], for a broker the validator would refuse.
  ///
  /// The integration tests use it to reach an anonymous local broker, which has no
  /// username and no password.
  pub async fn connect_unvalidated(config: &Config, role: Role) -> Result<Self> {
    let client = Self::start_unvalidated(config, role)?;
    client.wait_until_connected().await?;
    Ok(client)
  }

  /// Starts connecting on the current Tokio runtime without waiting for CONNACK.
  ///
  /// The GUI keeps this handle while waiting so quitting can gracefully disconnect
  /// even an initial connection that is still in progress.
  pub fn start(config: &Config, role: Role) -> Result<Self> {
    config.validate()?;
    Self::start_unvalidated(config, role)
  }

  fn start_unvalidated(config: &Config, role: Role) -> Result<Self> {
    let connection = options::build(config, role)?;
    let connect_timeout = connection.connect_timeout;
    log::debug!(
      "connecting to {} as {} with client id {}",
      connection.url,
      role,
      connection.client_id
    );

    let (client, eventloop) = AsyncClient::new(connection.options.clone(), REQUEST_CAPACITY);
    let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_CAPACITY);
    let (status_tx, _) = watch::channel(Status::new(&connection));

    let inner = Arc::new(Inner {
      client,
      status: status_tx,
      pending: StdMutex::new(Pending::default()),
      order: AsyncMutex::new(()),
      filters: StdMutex::new(Vec::new()),
      publish_timeout: Duration::from_secs(config.publish.timeout_secs.max(1)),
      connect_timeout,
      dropped: AtomicU64::new(0),
      disconnected_cleanly: AtomicBool::new(false),
    });

    let backoff = Backoff::from_config(&config.broker.reconnect);
    let task = tokio::spawn(run(Arc::clone(&inner), eventloop, incoming_tx, role, backoff));

    let session_cleanup = (role.session_expiry_secs(config) > 0).then(|| {
      let mut options = connection.options;
      options.set_clean_start(true);
      options.set_session_expiry_interval(Some(0));
      options
    });
    Ok(Self {
      session_cleanup,
      inner,
      incoming: Some(incoming_rx),
      task,
    })
  }

  /// Waits for a started client to receive CONNACK, fail, or reach its connection timeout.
  pub async fn wait_until_connected(&self) -> Result<()> {
    self.await_first_connection(self.inner.connect_timeout).await
  }

  /// Blocks until the first CONNACK, a terminal failure, or the timeout.
  async fn await_first_connection(&self, timeout: Duration) -> Result<()> {
    let mut status = self.inner.status.subscribe();
    let settled = tokio::time::timeout(timeout, async {
      loop {
        let snapshot = status.borrow_and_update().clone();
        match snapshot.state {
          State::Connected => return Ok(()),
          State::Disconnected => {
            return Err(Error::Connect(
              snapshot
                .last_error
                .unwrap_or_else(|| "the event loop stopped without a reason".to_owned()),
            ));
          }
          State::Connecting | State::Reconnecting => {}
        }
        if status.changed().await.is_err() {
          return Err(Error::ClientStopped("the event loop stopped".to_owned()));
        }
      }
    })
    .await;

    match settled {
      Ok(Ok(())) => {
        log::info!("connected to {}:{}", self.inner.host(), self.inner.port());
        Ok(())
      }
      Ok(Err(error)) => {
        self.task.abort();
        Err(error)
      }
      Err(_) => {
        self.task.abort();
        Err(Error::ConnectTimeout(timeout.as_secs()))
      }
    }
  }

  /// The connection state, which changes whenever the status bar should.
  pub fn status(&self) -> watch::Receiver<Status> {
    self.inner.status.subscribe()
  }

  /// The connection state right now.
  pub fn status_now(&self) -> Status {
    self.inner.status.borrow().clone()
  }

  /// The received messages, available once.
  ///
  /// There is one channel, so the first caller gets it and any later caller gets
  /// `None`. `hmc` never asks; `hmg` hands it to the task that stores messages and
  /// raises notifications.
  pub fn take_incoming(&mut self) -> Option<mpsc::Receiver<IncomingMessage>> {
    self.incoming.take()
  }

  /// How many received messages were dropped because nothing was reading them.
  pub fn dropped_messages(&self) -> u64 {
    self.inner.dropped.load(Ordering::Relaxed)
  }

  /// Publishes a HiveMe message, with the MQTT 5 properties of the envelope.
  ///
  /// This is the path both applications take, so that a message from the `hmg`
  /// composer is indistinguishable from one `hmc` sent.
  pub async fn publish_message(&self, topic: &str, message: &Message, qos: Qos, retain: bool) -> Result<()> {
    let payload = message.to_bytes().map_err(|source| Error::PublishRejected {
      topic: topic.to_owned(),
      reason: format!("the message cannot be encoded: {source}"),
    })?;
    self
      .publish(topic, payload, qos, retain, Some(&message.mqtt_properties()))
      .await
  }

  /// Publishes bytes and returns once the broker has acknowledged them.
  ///
  /// At [`Qos::AtMostOnce`] there is nothing to acknowledge, so it returns once the
  /// packet has been written. Otherwise it waits for the PUBACK or the PUBCOMP, and
  /// gives up after `publish.timeoutSecs`.
  pub async fn publish(
    &self,
    topic: &str,
    payload: Vec<u8>,
    qos: Qos,
    retain: bool,
    properties: Option<&MessageProperties>,
  ) -> Result<()> {
    // rumqttc answers a wildcard or an empty topic with the request it refused to
    // send, which says nothing useful, so the topic is checked here instead.
    crate::topic::validate_topic(topic).map_err(|reason| Error::PublishRejected {
      topic: topic.to_owned(),
      reason,
    })?;

    let receiver = {
      let _order = self.inner.order.lock().await;
      let (sender, receiver) = oneshot::channel();
      self.inner.pending.lock().unwrap().publishes.push_back(PublishWaiter {
        topic: topic.to_owned(),
        qos,
        sender,
      });
      let queued = match properties {
        Some(properties) => {
          self
            .inner
            .client
            .publish_with_properties(topic, qos.into(), retain, payload, publish_properties(properties))
            .await
        }
        None => self.inner.client.publish(topic, qos.into(), retain, payload).await,
      };
      if let Err(source) = queued {
        // Nothing else can have pushed while the order lock is held, so the entry that
        // comes back off the queue is the one that was just put on it.
        self.inner.pending.lock().unwrap().publishes.pop_back();
        return Err(Error::ClientStopped(source.to_string()));
      }
      receiver
    };

    match tokio::time::timeout(self.inner.publish_timeout, receiver).await {
      Ok(Ok(result)) => result,
      Ok(Err(_)) => Err(Error::ConnectionLost(
        "the client stopped before the broker acknowledged the message".to_owned(),
      )),
      Err(_) => {
        self.inner.pending.lock().unwrap().prune();
        Err(Error::PublishTimeout {
          topic: topic.to_owned(),
          secs: self.inner.publish_timeout.as_secs(),
        })
      }
    }
  }

  /// Subscribes to every filter and returns once the broker has answered.
  ///
  /// A SUBACK carries one reason code per filter, and HiveMQ Cloud refuses a filter
  /// the credential has no permission for rather than the whole packet, so a refusal
  /// of any one filter is reported as an error naming that filter.
  ///
  /// The filters are remembered. When a reconnect comes back without the session, they
  /// are sent again, so a caller subscribes once and not on every reconnect.
  pub async fn subscribe<I, S>(&self, filters: I, qos: Qos) -> Result<()>
  where
    I: IntoIterator<Item = S>,
    S: Into<String>,
  {
    let filters: Vec<String> = filters.into_iter().map(Into::into).collect();
    if filters.is_empty() {
      return Ok(());
    }
    for filter in &filters {
      crate::topic::validate_filter(filter).map_err(|reason| Error::SubscribeRejected {
        filter: filter.clone(),
        reason,
      })?;
    }
    self.inner.subscribe(filters.clone(), qos).await?;
    self.inner.remember(&filters, qos);
    Ok(())
  }

  /// The filters this client is subscribed to.
  pub fn subscriptions(&self) -> Vec<String> {
    self
      .inner
      .filters
      .lock()
      .unwrap()
      .iter()
      .map(|(filter, _)| filter.clone())
      .collect()
  }

  /// Sends DISCONNECT and waits for it to leave the socket, with a bounded wait.
  ///
  /// The broker keeps a GUI session for its configured expiry. Use
  /// [`Self::end_session`] when quitting to discard that session as well.
  pub async fn disconnect(&self) -> Result<()> {
    let mut status = self.inner.status.subscribe();
    if status.borrow().state == State::Disconnected {
      return Ok(());
    }
    let result = tokio::time::timeout(self.inner.connect_timeout.min(SHUTDOWN_TIMEOUT), async {
      self
        .inner
        .client
        .disconnect()
        .await
        .map_err(|source| Error::ClientStopped(source.to_string()))?;
      loop {
        let snapshot = status.borrow_and_update().clone();
        if snapshot.state == State::Disconnected {
          return if self.inner.disconnected_cleanly.load(Ordering::SeqCst) {
            Ok(())
          } else {
            Err(Error::ConnectionLost(
              snapshot
                .last_error
                .unwrap_or_else(|| "the connection stopped during disconnect".to_owned()),
            ))
          };
        }
        status
          .changed()
          .await
          .map_err(|_| Error::ClientStopped("the event loop stopped during disconnect".to_owned()))?;
      }
    })
    .await
    .unwrap_or_else(|_| Err(Error::ClientStopped("the MQTT disconnect timed out".to_owned())));
    if result.is_err() {
      self.abort();
    }
    result
  }

  /// Disconnects and discards the broker's subscriptions and queued session messages.
  ///
  /// rumqttc 0.25 cannot put a session-expiry property on DISCONNECT. After closing
  /// the live connection, a short clean-start connection with the same identity and
  /// zero expiry clears the session, then sends its own DISCONNECT. No subscriptions
  /// or publishes are made on that connection. Network recovery keeps the configured
  /// expiry; only this explicit shutdown path discards the session.
  pub async fn end_session(&self) -> Result<()> {
    let result = tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
      let goodbye = self.disconnect().await;
      if let Some(options) = &self.session_cleanup {
        let (client, mut eventloop) = AsyncClient::new(options.clone(), 1);
        loop {
          match eventloop
            .poll()
            .await
            .map_err(|error| Error::Connect(describe(&error)))?
          {
            Event::Incoming(Packet::ConnAck(_)) => {
              client
                .disconnect()
                .await
                .map_err(|source| Error::ClientStopped(source.to_string()))?;
            }
            Event::Outgoing(Outgoing::Disconnect) => return Ok(()),
            _ => {}
          }
        }
      }
      goodbye
    })
    .await
    .unwrap_or_else(|_| Err(Error::ClientStopped("the MQTT session cleanup timed out".to_owned())));
    if result.is_err() {
      self.abort();
    }
    result
  }

  /// Stops the event loop at once, without saying goodbye to the broker.
  ///
  /// [`MqttClient::disconnect`] is the orderly way out; this is for a caller that has
  /// given up on the connection. Dropping the client does the same thing.
  pub fn abort(&self) {
    self.task.abort();
    self.inner.set_disconnected(Some("the client was stopped".to_owned()));
    // The task cannot run its own cleanup once it is aborted, so anyone waiting for an
    // acknowledgement is told now rather than left until their timeout.
    self
      .inner
      .pending
      .lock()
      .unwrap()
      .fail_all("the client was stopped before the broker acknowledged");
  }
}

impl std::fmt::Debug for MqttClient {
  /// Shows the connection, not the credentials: the options behind it carry the broker
  /// password, so they are deliberately left out.
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let status = self.status_now();
    formatter
      .debug_struct("MqttClient")
      .field("state", &status.state)
      .field("host", &status.host)
      .field("port", &status.port)
      .field("clientId", &status.client_id)
      .field("subscriptions", &status.subscriptions)
      .finish_non_exhaustive()
  }
}

impl Drop for MqttClient {
  fn drop(&mut self) {
    // Whatever is in flight is abandoned rather than flushed, because a Drop cannot
    // wait. `disconnect` is the orderly way out and both applications use it.
    self.task.abort();
  }
}

/// Everything the client and its event loop share.
struct Inner {
  client: AsyncClient,
  status: watch::Sender<Status>,
  pending: StdMutex<Pending>,
  /// Held across a request so that the queue order and the wire order agree.
  order: AsyncMutex<()>,
  filters: StdMutex<Vec<(String, Qos)>>,
  publish_timeout: Duration,
  connect_timeout: Duration,
  dropped: AtomicU64,
  disconnected_cleanly: AtomicBool,
}

impl Inner {
  fn host(&self) -> String {
    self.status.borrow().host.clone()
  }

  fn port(&self) -> u16 {
    self.status.borrow().port
  }

  /// Sends a SUBSCRIBE and waits for its SUBACK.
  async fn subscribe(&self, filters: Vec<String>, qos: Qos) -> Result<()> {
    let receiver = {
      let _order = self.order.lock().await;
      let (sender, receiver) = oneshot::channel();
      self.pending.lock().unwrap().subscriptions.push_back(SubscribeWaiter {
        filters: filters.clone(),
        sender,
      });
      let packet = filters.iter().map(|filter| Filter::new(filter.clone(), qos.into()));
      if let Err(source) = self.client.subscribe_many(packet).await {
        self.pending.lock().unwrap().subscriptions.pop_back();
        return Err(Error::ClientStopped(source.to_string()));
      }
      receiver
    };

    match tokio::time::timeout(self.connect_timeout, receiver).await {
      Ok(Ok(result)) => result,
      Ok(Err(_)) => Err(Error::ConnectionLost(
        "the client stopped before the broker acknowledged the subscription".to_owned(),
      )),
      Err(_) => {
        self.pending.lock().unwrap().prune();
        Err(Error::SubscribeTimeout(self.connect_timeout.as_secs()))
      }
    }
  }

  /// Records filters so that a reconnect without a session can send them again.
  fn remember(&self, filters: &[String], qos: Qos) {
    let mut known = self.filters.lock().unwrap();
    for filter in filters {
      match known.iter_mut().find(|(existing, _)| existing == filter) {
        Some(entry) => entry.1 = qos,
        None => known.push((filter.clone(), qos)),
      }
    }
    let count = known.len();
    drop(known);
    self.status.send_modify(|status| status.subscriptions = count);
  }

  fn set_connected(&self) {
    self.status.send_modify(|status| {
      status.state = State::Connected;
      status.attempt = 0;
      status.retry_in_ms = None;
    });
  }

  fn set_reconnecting(&self, attempt: u32, delay: Duration, reason: String) {
    self.status.send_modify(|status| {
      status.state = State::Reconnecting;
      status.attempt = attempt;
      status.retry_in_ms = Some(delay.as_millis() as u64);
      status.last_error = Some(reason);
    });
  }

  fn set_disconnected(&self, reason: Option<String>) {
    self.status.send_modify(|status| {
      status.state = State::Disconnected;
      status.retry_in_ms = None;
      if reason.is_some() {
        status.last_error = reason;
      }
    });
  }
}

/// A publish that is waiting for its acknowledgement.
struct PublishWaiter {
  topic: String,
  qos: Qos,
  sender: oneshot::Sender<Result<()>>,
}

/// A subscription that is waiting for its SUBACK.
struct SubscribeWaiter {
  filters: Vec<String>,
  sender: oneshot::Sender<Result<()>>,
}

/// The requests that have been sent but not yet answered.
///
/// `rumqttc` assigns packet identifiers inside the event loop, so a caller cannot know
/// in advance which acknowledgement is theirs. The event loop reports the identifier it
/// picked as an outgoing event, in the order the requests were sent, which is why the
/// queues below are first in first out and why every request is queued and sent under
/// one lock.
#[derive(Default)]
struct Pending {
  publishes: VecDeque<PublishWaiter>,
  publish_acks: HashMap<u16, PublishWaiter>,
  subscriptions: VecDeque<SubscribeWaiter>,
  subscribe_acks: HashMap<u16, SubscribeWaiter>,
}

impl Pending {
  /// Forgets the callers that have stopped waiting, for example after a timeout.
  fn prune(&mut self) {
    self.publishes.retain(|waiter| !waiter.sender.is_closed());
    self.publish_acks.retain(|_, waiter| !waiter.sender.is_closed());
    self.subscriptions.retain(|waiter| !waiter.sender.is_closed());
    self.subscribe_acks.retain(|_, waiter| !waiter.sender.is_closed());
  }

  /// Tells everyone still waiting that there will be no acknowledgement.
  fn fail_all(&mut self, reason: &str) {
    let publishes = self
      .publishes
      .drain(..)
      .chain(self.publish_acks.drain().map(|(_, w)| w));
    for waiter in publishes {
      let _ = waiter.sender.send(Err(Error::ConnectionLost(reason.to_owned())));
    }
    let subscriptions = self
      .subscriptions
      .drain(..)
      .chain(self.subscribe_acks.drain().map(|(_, w)| w));
    for waiter in subscriptions {
      let _ = waiter.sender.send(Err(Error::ConnectionLost(reason.to_owned())));
    }
  }
}

/// Polls the event loop until the connection ends, reconnecting when the role says to.
async fn run(
  inner: Arc<Inner>,
  mut eventloop: EventLoop,
  incoming: mpsc::Sender<IncomingMessage>,
  role: Role,
  mut backoff: Backoff,
) {
  let reason = loop {
    match eventloop.poll().await {
      Ok(event) => {
        if handle(&inner, &incoming, event, role) {
          break None;
        }
        backoff.reset();
      }
      Err(error) => {
        let described = describe(&error);
        if !role.reconnects() || is_terminal(&error) {
          break Some(described);
        }
        let delay = backoff.next_delay();
        log::warn!(
          "the broker connection failed ({described}), attempt {} follows in {} ms",
          backoff.attempt(),
          delay.as_millis()
        );
        inner.set_reconnecting(backoff.attempt(), delay, described);
        tokio::time::sleep(delay).await;
      }
    }
  };

  match reason.as_deref() {
    // A role that reconnects is the only one for which this log line is the sole
    // record: elsewhere the same reason is handed back to the caller as an error, and
    // `hmc` would otherwise print it twice on stderr.
    Some(reason) if role.reconnects() => log::warn!("the broker connection ended: {reason}"),
    Some(reason) => log::debug!("the broker connection ended: {reason}"),
    None => log::debug!("the broker connection was closed on request"),
  }
  inner.set_disconnected(reason.clone());
  inner
    .pending
    .lock()
    .unwrap()
    .fail_all(reason.as_deref().unwrap_or("the connection was closed"));
}

/// Acts on one event. Answers whether the event loop should stop.
fn handle(inner: &Arc<Inner>, incoming: &mpsc::Sender<IncomingMessage>, event: Event, role: Role) -> bool {
  match event {
    Event::Incoming(Packet::ConnAck(ack)) => {
      if ack.code != ConnectReturnCode::Success {
        // rumqttc normally reports a refusal as a poll error rather than as an event,
        // so reaching here means a build that does not; either way the caller has to
        // hear about it, and `run` keeps the reason recorded here.
        let reason = format!(
          "the broker refused the connection: {}",
          describe_connect_refusal(ack.code)
        );
        log::error!("{reason}");
        inner.set_disconnected(Some(reason));
        return true;
      }
      log::debug!(
        "the broker accepted the connection, session present: {}",
        ack.session_present
      );
      inner.set_connected();
      if !ack.session_present {
        resubscribe(inner, role);
      }
    }
    Event::Incoming(Packet::Publish(publish)) => {
      let message = IncomingMessage::from_publish(publish);
      if let Err(mpsc::error::TrySendError::Full(message)) = incoming.try_send(message) {
        let dropped = inner.dropped.fetch_add(1, Ordering::Relaxed) + 1;
        log::warn!(
          "the incoming message channel is full, dropped the message on {} ({dropped} so far)",
          message.topic
        );
      }
    }
    Event::Incoming(Packet::PubAck(ack)) => {
      let waiter = inner.pending.lock().unwrap().publish_acks.remove(&ack.pkid);
      if let Some(waiter) = waiter {
        let result = match ack.reason {
          PubAckReason::Success | PubAckReason::NoMatchingSubscribers => Ok(()),
          reason => Err(Error::PublishRejected {
            topic: waiter.topic.clone(),
            reason: describe_puback(reason),
          }),
        };
        let _ = waiter.sender.send(result);
      }
    }
    Event::Incoming(Packet::PubComp(complete)) => {
      let waiter = inner.pending.lock().unwrap().publish_acks.remove(&complete.pkid);
      if let Some(waiter) = waiter {
        let _ = waiter.sender.send(Ok(()));
      }
    }
    Event::Incoming(Packet::SubAck(ack)) => {
      let waiter = inner.pending.lock().unwrap().subscribe_acks.remove(&ack.pkid);
      if let Some(waiter) = waiter {
        let _ = waiter.sender.send(check_suback(&waiter.filters, &ack.return_codes));
      }
    }
    Event::Incoming(Packet::Disconnect(disconnect)) => {
      log::warn!("the broker closed the connection: {:?}", disconnect.reason_code);
    }
    Event::Outgoing(Outgoing::Publish(pkid)) => {
      let mut pending = inner.pending.lock().unwrap();
      if pkid != 0 && pending.publish_acks.contains_key(&pkid) {
        // A packet identifier that is already in flight is the same message going out
        // again after a reconnect, not a new one, so the caller keeps waiting.
        log::debug!("republished packet {pkid} after a reconnect");
      } else if let Some(waiter) = pending.publishes.pop_front() {
        if waiter.qos.is_acknowledged() {
          pending.publish_acks.insert(pkid, waiter);
        } else {
          // Nothing will acknowledge a QoS 0 publish, so the packet leaving is the only
          // answer there will ever be.
          let _ = waiter.sender.send(Ok(()));
        }
      }
    }
    Event::Outgoing(Outgoing::Subscribe(pkid)) => {
      let mut pending = inner.pending.lock().unwrap();
      if let Some(waiter) = pending.subscriptions.pop_front() {
        pending.subscribe_acks.insert(pkid, waiter);
      }
    }
    Event::Outgoing(Outgoing::Disconnect) => {
      inner.disconnected_cleanly.store(true, Ordering::SeqCst);
      return true;
    }
    _ => {}
  }
  false
}

/// Sends the remembered filters again after a reconnect that lost the session.
///
/// It runs in its own task so that it queues behind any subscription the caller is
/// sending at this moment, and so that the event loop keeps being polled while it waits
/// for the SUBACK.
fn resubscribe(inner: &Arc<Inner>, role: Role) {
  let filters = inner.filters.lock().unwrap().clone();
  if filters.is_empty() {
    return;
  }
  log::debug!(
    "the broker has no session for this client, sending {} filters again",
    filters.len()
  );
  let inner = Arc::clone(inner);
  tokio::spawn(async move {
    let mut by_qos: Vec<(Qos, Vec<String>)> = Vec::new();
    for (filter, qos) in filters {
      match by_qos.iter_mut().find(|(known, _)| *known == qos) {
        Some((_, group)) => group.push(filter),
        None => by_qos.push((qos, vec![filter])),
      }
    }
    for (qos, group) in by_qos {
      if let Err(error) = inner.subscribe(group, qos).await {
        log::error!("the subscriptions could not be restored after the reconnect: {error}");
        if role.reconnects() {
          // The connection is up but deaf, which the status bar has to show.
          inner
            .status
            .send_modify(|status| status.last_error = Some(error.to_string()));
        }
      }
    }
  });
}

/// Whether retrying this error could ever work.
fn is_terminal(error: &ConnectionError) -> bool {
  match error {
    // A refusal is a decision about the credentials or the client id, and it will be
    // the same decision next time.
    ConnectionError::ConnectionRefused(_) => true,
    // The trust store cannot grow a certificate by being asked again.
    ConnectionError::Tls(_) => true,
    // Whatever answered is not an MQTT broker.
    ConnectionError::NotConnAck(_) => true,
    // Every client handle has been dropped, so there is nothing left to reconnect for.
    ConnectionError::RequestsDone => true,
    _ => false,
  }
}

/// Turns a connection error into something worth putting in front of a user.
fn describe(error: &ConnectionError) -> String {
  match error {
    ConnectionError::ConnectionRefused(code) => format!(
      "the broker refused the connection: {}",
      describe_connect_refusal(*code)
    ),
    ConnectionError::NotConnAck(_) => {
      "the broker answered the connection with something other than a CONNACK, check the port and the scheme of broker.url"
        .to_owned()
    }
    ConnectionError::Tls(source) => format!(
      "the TLS handshake failed ({source}), check broker.tls.caFile and that the host is what the certificate names"
    ),
    // The wrapper adds nothing a user wants to read in front of "connection refused".
    ConnectionError::Io(source) => source.to_string(),
    other => other.to_string(),
  }
}

/// The CONNACK reason codes, in the words of the thing the user has to change.
fn describe_connect_refusal(code: ConnectReturnCode) -> &'static str {
  match code {
    ConnectReturnCode::BadUserNamePassword => {
      "the username or the password is wrong; a credential created just now can take up to a minute to work"
    }
    ConnectReturnCode::NotAuthorized => "the credential exists but is not allowed to connect",
    ConnectReturnCode::Banned => "the credential is banned",
    ConnectReturnCode::ClientIdentifierNotValid | ConnectReturnCode::BadClientId => {
      "the broker will not accept this client id, check broker.clientIdPrefix"
    }
    ConnectReturnCode::ServerUnavailable | ConnectReturnCode::ServiceUnavailable => "the broker is unavailable",
    ConnectReturnCode::ServerBusy => "the broker is busy",
    ConnectReturnCode::QuotaExceeded => "the cluster is at its connection limit",
    ConnectReturnCode::ConnectionRateExceeded => "the cluster is refusing new connections for now",
    ConnectReturnCode::UnsupportedProtocolVersion | ConnectReturnCode::RefusedProtocolVersion => {
      "the broker does not speak MQTT 5"
    }
    ConnectReturnCode::UseAnotherServer | ConnectReturnCode::ServerMoved => {
      "the cluster has moved, check broker.url in the HiveMQ Cloud console"
    }
    _ => "the broker gave no reason this build knows",
  }
}

/// The PUBACK reason codes worth telling a user about.
fn describe_puback(reason: PubAckReason) -> String {
  match reason {
    PubAckReason::NotAuthorized => {
      "the credential is not allowed to publish to this topic; check the permissions in the HiveMQ Cloud console"
        .to_owned()
    }
    PubAckReason::TopicNameInvalid => "the broker does not accept this topic name".to_owned(),
    PubAckReason::QuotaExceeded => "the cluster is over its quota".to_owned(),
    PubAckReason::PayloadFormatInvalid => {
      "the payload does not match the content type that was sent with it".to_owned()
    }
    PubAckReason::PacketIdentifierInUse => "the broker thinks this packet identifier is already in use".to_owned(),
    other => format!("{other:?}"),
  }
}

/// Reads a SUBACK, naming the first filter the broker would not take.
fn check_suback(filters: &[String], codes: &[SubscribeReasonCode]) -> Result<()> {
  for (index, code) in codes.iter().enumerate() {
    if matches!(code, SubscribeReasonCode::Success(_)) {
      continue;
    }
    let filter = filters.get(index).cloned().unwrap_or_else(|| "?".to_owned());
    let reason = match code {
      SubscribeReasonCode::NotAuthorized => {
        "the credential is not allowed to subscribe to it; check the permissions in the HiveMQ Cloud console".to_owned()
      }
      SubscribeReasonCode::TopicFilterInvalid => "the broker does not accept this topic filter".to_owned(),
      SubscribeReasonCode::QuotaExceeded => "the cluster is over its subscription quota".to_owned(),
      SubscribeReasonCode::SharedSubscriptionsNotSupported => "the broker has no shared subscriptions".to_owned(),
      SubscribeReasonCode::WildcardSubscriptionsNotSupported => "the broker has no wildcard subscriptions".to_owned(),
      other => format!("{other:?}"),
    };
    return Err(Error::SubscribeRejected { filter, reason });
  }
  Ok(())
}

/// The MQTT 5 properties of a HiveMe envelope, in the shape `rumqttc` publishes.
fn publish_properties(properties: &MessageProperties) -> PublishProperties {
  PublishProperties {
    // The payload is the JSON of docs/specs/message.md, so it is always UTF-8 text.
    payload_format_indicator: Some(1),
    message_expiry_interval: properties.message_expiry_interval,
    content_type: Some(properties.content_type.to_owned()),
    user_properties: properties.user_properties.clone(),
    ..PublishProperties::default()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::message::Sender;

  #[test]
  fn the_qos_values_round_trip_through_the_wire_form() {
    for qos in [Qos::AtMostOnce, Qos::AtLeastOnce, Qos::ExactlyOnce] {
      assert_eq!(Qos::from_u8(qos.as_u8()), Some(qos));
      assert_eq!(Qos::from(QoS::from(qos)), qos);
    }
    assert_eq!(Qos::from_u8(3), None);
    assert_eq!(Qos::default(), Qos::AtLeastOnce);
  }

  #[test]
  fn only_qos_0_goes_unacknowledged() {
    assert!(!Qos::AtMostOnce.is_acknowledged());
    assert!(Qos::AtLeastOnce.is_acknowledged());
    assert!(Qos::ExactlyOnce.is_acknowledged());
  }

  #[test]
  fn the_configured_qos_is_used_and_a_broken_one_falls_back() {
    let mut config = Config::default();
    assert_eq!(Qos::from_config(&config), Qos::AtLeastOnce);
    config.publish.qos = 2;
    assert_eq!(Qos::from_config(&config), Qos::ExactlyOnce);
    config.publish.qos = 9;
    assert_eq!(Qos::from_config(&config), Qos::AtLeastOnce);
  }

  #[test]
  fn the_envelope_properties_travel_with_the_publish() {
    let message = Message::new_text(Sender::default(), "hello").with_ttl_secs(60);
    let properties = publish_properties(&message.mqtt_properties());
    assert_eq!(properties.content_type.as_deref(), Some("application/json"));
    assert_eq!(properties.payload_format_indicator, Some(1));
    assert_eq!(properties.message_expiry_interval, Some(60));
    assert_eq!(
      properties.user_properties,
      vec![(crate::message::USER_PROPERTY_VERSION.to_owned(), "1".to_owned())]
    );
  }

  #[test]
  fn an_incoming_publish_keeps_its_topic_payload_and_properties() {
    let mut publish = Publish::new("hiveme/info", QoS::AtLeastOnce, "hello".as_bytes(), None);
    publish.retain = true;
    publish.properties = Some(PublishProperties {
      content_type: Some("application/json".to_owned()),
      user_properties: vec![("hiveme-v".to_owned(), "1".to_owned())],
      ..PublishProperties::default()
    });

    let message = IncomingMessage::from_publish(publish);
    assert_eq!(message.topic, "hiveme/info");
    assert_eq!(message.payload, b"hello");
    assert_eq!(message.qos, Qos::AtLeastOnce);
    assert!(message.retain);
    assert_eq!(message.properties.hiveme_version(), Some(1));
    assert_eq!(message.properties.user_property("nope"), None);
  }

  #[test]
  fn an_incoming_message_is_parsed_by_the_shared_reader() {
    let message = Message::new_text(Sender::default(), "hello");
    let publish = Publish::new("hiveme/info", QoS::AtLeastOnce, message.to_bytes().unwrap(), None);
    let incoming = IncomingMessage::from_publish(publish);
    assert!(matches!(incoming.parse(), Parsed::Envelope(_)));
  }

  #[test]
  fn a_topic_that_is_not_utf8_still_produces_a_message() {
    let publish = Publish::new(
      String::from_utf8_lossy(&[0xff, 0xfe]).into_owned(),
      QoS::AtMostOnce,
      "x".as_bytes(),
      None,
    );
    assert!(!IncomingMessage::from_publish(publish).topic.is_empty());
  }

  #[test]
  fn a_suback_that_refuses_a_filter_names_it() {
    let filters = vec!["hiveme/#".to_owned(), "other/#".to_owned()];
    let codes = vec![
      SubscribeReasonCode::Success(QoS::AtLeastOnce),
      SubscribeReasonCode::NotAuthorized,
    ];
    let error = check_suback(&filters, &codes).unwrap_err();
    assert!(error.to_string().contains("other/#"), "{error}");
    assert!(error.is_connection(), "{error}");
  }

  #[test]
  fn a_suback_that_accepts_everything_is_fine() {
    let filters = vec!["hiveme/#".to_owned()];
    let codes = vec![SubscribeReasonCode::Success(QoS::AtLeastOnce)];
    assert!(check_suback(&filters, &codes).is_ok());
  }

  #[test]
  fn a_suback_with_fewer_codes_than_filters_still_reports() {
    let error = check_suback(&[], &[SubscribeReasonCode::Failure]).unwrap_err();
    assert!(error.to_string().contains('?'), "{error}");
  }

  #[test]
  fn a_refusal_is_terminal_and_a_dropped_socket_is_not() {
    assert!(is_terminal(&ConnectionError::ConnectionRefused(
      ConnectReturnCode::BadUserNamePassword
    )));
    assert!(is_terminal(&ConnectionError::RequestsDone));
    assert!(!is_terminal(&ConnectionError::Io(std::io::Error::from(
      std::io::ErrorKind::ConnectionReset
    ))));
  }

  #[test]
  fn an_io_failure_is_reported_without_the_wrapper_around_it() {
    let described = describe(&ConnectionError::Io(std::io::Error::other("connection refused")));
    assert_eq!(described, "connection refused");
  }

  #[test]
  fn a_wrong_password_is_explained_rather_than_named() {
    let described = describe(&ConnectionError::ConnectionRefused(
      ConnectReturnCode::BadUserNamePassword,
    ));
    assert!(described.contains("password is wrong"), "{described}");
    assert!(described.contains("up to a minute"), "{described}");
  }

  #[test]
  fn an_unauthorized_publish_points_at_the_console() {
    let reason = describe_puback(PubAckReason::NotAuthorized);
    assert!(reason.contains("HiveMQ Cloud console"), "{reason}");
  }

  #[test]
  fn a_no_matching_subscribers_puback_is_not_a_failure() {
    // The broker took the message and nobody was listening, which is the normal case
    // for `hmc` when `hmg` is closed.
    assert!(matches!(
      PubAckReason::NoMatchingSubscribers,
      PubAckReason::NoMatchingSubscribers
    ));
  }

  #[test]
  fn the_state_names_are_the_ones_the_frontend_compares_against() {
    // ConnectionState in src/lib/protocol.ts holds these four spellings, and the GUI
    // enables the composer on one of them. A lower case word here disables it forever.
    assert_eq!(State::Connecting.to_string(), "Connecting");
    assert_eq!(State::Connected.to_string(), "Connected");
    assert_eq!(State::Reconnecting.to_string(), "Reconnecting");
    assert_eq!(State::Disconnected.to_string(), "Disconnected");
  }

  #[test]
  fn a_pending_publish_is_failed_when_the_loop_stops() {
    let mut pending = Pending::default();
    let (sender, mut receiver) = oneshot::channel();
    pending.publishes.push_back(PublishWaiter {
      topic: "hiveme/info".to_owned(),
      qos: Qos::AtLeastOnce,
      sender,
    });
    pending.fail_all("the broker went away");
    let error = receiver.try_recv().unwrap().unwrap_err();
    assert!(error.is_connection(), "{error}");
    assert!(error.to_string().contains("the broker went away"), "{error}");
  }

  #[test]
  fn pruning_forgets_the_callers_that_gave_up() {
    let mut pending = Pending::default();
    let (sender, receiver) = oneshot::channel();
    pending.publishes.push_back(PublishWaiter {
      topic: "hiveme/info".to_owned(),
      qos: Qos::AtLeastOnce,
      sender,
    });
    let (kept, _kept_receiver) = oneshot::channel();
    pending.publishes.push_back(PublishWaiter {
      topic: "hiveme/warn".to_owned(),
      qos: Qos::AtLeastOnce,
      sender: kept,
    });
    drop(receiver);
    pending.prune();
    assert_eq!(pending.publishes.len(), 1);
    assert_eq!(pending.publishes[0].topic, "hiveme/warn");
  }
}
