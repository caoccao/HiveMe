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

//! The local message history of `hmg`.
//!
//! SQLite through `rusqlite` with the bundled library, in `HiveMe.db` beside the
//! config file. The schema is specified in `docs/specs/gui.md`.
//!
//! The store is what makes the topic tree and the chat view survive a restart. It also
//! decides what is new: a row exists once per topic and message id, so a message the
//! composer sent and the copy the broker echoes back collapse into one bubble rather
//! than two.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};

use crate::config::History;
use crate::error::{Error, Result};
use crate::message::{Level, Parsed};

/// The version of the database schema this build writes.
pub const SCHEMA_VERSION: i64 = 1;

/// How many messages one page of history holds when the caller does not say.
pub const DEFAULT_PAGE_SIZE: u32 = 200;

/// How often `hmg` prunes, after the pass it makes at startup.
pub const PRUNE_INTERVAL_SECS: u64 = 600;

/// One topic the store has seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicRow {
  pub topic: String,
  /// RFC 3339, when the first message on this topic was stored.
  pub first_seen_ts: String,
  /// RFC 3339, when the last one was.
  pub last_seen_ts: String,
  pub unread: u32,
  pub messages: u32,
}

/// One stored message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
  /// The row identifier, which is also the paging cursor.
  pub row_id: i64,
  pub topic: String,
  /// The envelope id, or a generated one for a payload HiveMe did not shape.
  pub msg_id: String,
  /// The producer's RFC 3339 timestamp.
  pub ts: String,
  /// When this installation stored it, RFC 3339.
  pub received_ts: String,
  pub sender_id: Option<String>,
  pub sender_name: Option<String>,
  pub app: Option<String>,
  /// `envelope`, `json`, `text`, or `bytes`, as [`Parsed::tier`] names them.
  pub tier: String,
  pub level: Option<String>,
  pub title: Option<String>,
  /// The text a list view shows without parsing `raw` again.
  pub body: String,
  /// The payload exactly as it arrived, so the view can render every tier.
  pub raw: Vec<u8>,
  pub qos: u8,
  pub retain: bool,
  /// Whether this installation published it.
  pub outgoing: bool,
}

/// A message on its way into the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
  pub topic: String,
  pub msg_id: String,
  pub ts: String,
  pub received_ts: String,
  pub sender_id: Option<String>,
  pub sender_name: Option<String>,
  pub app: Option<String>,
  pub tier: String,
  pub level: Option<String>,
  pub title: Option<String>,
  pub body: String,
  pub raw: Vec<u8>,
  pub qos: u8,
  pub retain: bool,
  pub outgoing: bool,
}

impl NewMessage {
  /// Reads an MQTT payload with the lenient parser and fills in what a row needs.
  ///
  /// A payload without an envelope has no identifier of its own, so one is generated.
  /// Two identical third party messages are therefore two rows, which is right: they
  /// are two messages, and only a HiveMe envelope can claim otherwise.
  pub fn from_payload(topic: impl Into<String>, raw: Vec<u8>, qos: u8, retain: bool, outgoing: bool) -> Self {
    let parsed = crate::message::parse(&raw);
    let received_ts = now();
    let mut message = Self {
      topic: topic.into(),
      msg_id: format!("raw-{}", uuid::Uuid::now_v7()),
      ts: received_ts.clone(),
      received_ts,
      sender_id: None,
      sender_name: None,
      app: None,
      tier: parsed.tier().to_owned(),
      level: None,
      title: None,
      body: parsed.notification_body(),
      raw,
      qos,
      retain,
      outgoing,
    };
    if let Parsed::Envelope(envelope) = &parsed {
      message.msg_id = envelope.id.clone();
      message.ts = envelope.ts.clone();
      message.level = Some(envelope.level().to_string());
      if let Some(payload) = envelope.payload.as_ref() {
        message.title = payload.title.clone();
      }
      if let Some(sender) = envelope.sender.as_ref() {
        message.sender_id = sender.id.clone();
        message.sender_name = sender.name.clone();
        message.app = sender.app.clone();
      }
    }
    message
  }

  /// The level of this row, defaulting the way [`Level`] does.
  pub fn level(&self) -> Level {
    self.level.as_deref().map(Level::parse).unwrap_or_default()
  }
}

/// What [`Store::insert`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insertion {
  pub message: StoredMessage,
  /// False when the row was already there, which is how a broker echo is recognized.
  pub is_new: bool,
  /// Whether this is the first message ever stored on the topic.
  pub topic_is_new: bool,
}

/// What [`Store::prune`] removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pruned {
  /// Rows deleted because their topic held more than `maxMessagesPerTopic`.
  pub by_count: u64,
  /// Rows deleted because they were older than `retentionDays`.
  pub by_age: u64,
}

impl Pruned {
  /// How many rows went altogether.
  pub fn total(&self) -> u64 {
    self.by_count + self.by_age
  }
}

/// The message history.
///
/// One connection behind a mutex. SQLite serializes writers anyway, and `hmg` reads
/// history a page at a time from the IPC thread while one task writes what arrives, so
/// a pool would buy nothing.
pub struct Store {
  connection: Mutex<Connection>,
  path: PathBuf,
}

impl Store {
  /// Opens, creating the file and the schema when they are not there.
  pub fn open(path: &Path) -> Result<Self> {
    if let Some(directory) = path.parent()
      && !directory.as_os_str().is_empty()
    {
      std::fs::create_dir_all(directory).map_err(|source| Error::ConfigWrite {
        path: directory.to_path_buf(),
        source,
      })?;
    }
    let connection = Connection::open(path).map_err(|source| failed("open the database", source))?;
    let store = Self {
      connection: Mutex::new(connection),
      path: path.to_path_buf(),
    };
    store.migrate()?;
    Ok(store)
  }

  /// Opens a database that lives only as long as the process, for tests.
  pub fn in_memory() -> Result<Self> {
    let connection = Connection::open_in_memory().map_err(|source| failed("open the database", source))?;
    let store = Self {
      connection: Mutex::new(connection),
      path: PathBuf::from(":memory:"),
    };
    store.migrate()?;
    Ok(store)
  }

  /// Where the database file is.
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// How large the database file is, for the status bar. Zero when it has no file.
  pub fn size_bytes(&self) -> u64 {
    std::fs::metadata(&self.path).map(|meta| meta.len()).unwrap_or(0)
  }

  /// Creates the schema, or brings an older one forward.
  ///
  /// Version 0 means a database this build has not stamped, whether it is brand new or
  /// was written before `schema_version` existed, so the tables are created with
  /// `IF NOT EXISTS` and the version is written afterward.
  fn migrate(&self) -> Result<()> {
    let connection = self.lock();
    connection
      .execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS topics (
           id            INTEGER PRIMARY KEY,
           topic         TEXT NOT NULL UNIQUE,
           first_seen_ts TEXT NOT NULL,
           last_seen_ts  TEXT NOT NULL,
           unread        INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS messages (
           id          INTEGER PRIMARY KEY,
           topic_id    INTEGER NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
           msg_id      TEXT NOT NULL,
           ts          TEXT NOT NULL,
           received_ts TEXT NOT NULL,
           sender_id   TEXT,
           sender_name TEXT,
           app         TEXT,
           tier        TEXT NOT NULL CHECK(tier IN ('envelope','json','text','bytes')),
           level       TEXT,
           title       TEXT,
           body        TEXT NOT NULL,
           raw         BLOB NOT NULL,
           qos         INTEGER NOT NULL,
           retain      INTEGER NOT NULL,
           outgoing    INTEGER NOT NULL,
           UNIQUE(topic_id, msg_id)
         );
         CREATE INDEX IF NOT EXISTS messages_topic_id_id ON messages(topic_id, id);
         CREATE INDEX IF NOT EXISTS messages_received_ts ON messages(received_ts);",
      )
      .map_err(|source| failed("create its tables", source))?;

    let version: i64 = connection
      .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| row.get(0))
      .optional()
      .map_err(|source| failed("read its schema version", source))?
      .unwrap_or(0);
    if version > SCHEMA_VERSION {
      return Err(Error::Storage {
        operation: "open the database".to_owned(),
        reason: format!("it is at schema version {version}, and this build only knows {SCHEMA_VERSION}"),
      });
    }
    if version != SCHEMA_VERSION {
      connection
        .execute("DELETE FROM schema_version", [])
        .and_then(|_| {
          connection.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            params![SCHEMA_VERSION],
          )
        })
        .map_err(|source| failed("stamp its schema version", source))?;
    }
    Ok(())
  }

  /// Stores a message, or recognizes one that is already there.
  ///
  /// The unread count only moves for a message that is new and came from the broker;
  /// what this installation sent has been seen by definition.
  pub fn insert(&self, message: &NewMessage) -> Result<Insertion> {
    let connection = self.lock();
    let existing_topic: Option<i64> = connection
      .query_row(
        "SELECT id FROM topics WHERE topic = ?1",
        params![message.topic],
        |row| row.get(0),
      )
      .optional()
      .map_err(|source| failed("look up a topic", source))?;
    let topic_is_new = existing_topic.is_none();
    let topic_id = match existing_topic {
      Some(id) => {
        connection
          .execute(
            "UPDATE topics SET last_seen_ts = ?2 WHERE id = ?1",
            params![id, message.received_ts],
          )
          .map_err(|source| failed("touch a topic", source))?;
        id
      }
      None => {
        connection
          .execute(
            "INSERT INTO topics (topic, first_seen_ts, last_seen_ts, unread) VALUES (?1, ?2, ?2, 0)",
            params![message.topic, message.received_ts],
          )
          .map_err(|source| failed("record a topic", source))?;
        connection.last_insert_rowid()
      }
    };

    let existing: Option<i64> = connection
      .query_row(
        "SELECT id FROM messages WHERE topic_id = ?1 AND msg_id = ?2",
        params![topic_id, message.msg_id],
        |row| row.get(0),
      )
      .optional()
      .map_err(|source| failed("look up a message", source))?;

    let (row_id, is_new) = match existing {
      // The echo of a message the composer sent. The stored row already renders it, so
      // only the delivery facts are refreshed and the bubble stays where it is.
      Some(row_id) => {
        connection
          .execute(
            "UPDATE messages SET qos = ?2, retain = ?3 WHERE id = ?1",
            params![row_id, message.qos, message.retain],
          )
          .map_err(|source| failed("update a message", source))?;
        (row_id, false)
      }
      None => {
        connection
          .execute(
            "INSERT INTO messages (
               topic_id, msg_id, ts, received_ts, sender_id, sender_name, app,
               tier, level, title, body, raw, qos, retain, outgoing
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
              topic_id,
              message.msg_id,
              message.ts,
              message.received_ts,
              message.sender_id,
              message.sender_name,
              message.app,
              message.tier,
              message.level,
              message.title,
              message.body,
              message.raw,
              message.qos,
              message.retain,
              message.outgoing,
            ],
          )
          .map_err(|source| failed("store a message", source))?;
        let row_id = connection.last_insert_rowid();
        if !message.outgoing {
          connection
            .execute("UPDATE topics SET unread = unread + 1 WHERE id = ?1", params![topic_id])
            .map_err(|source| failed("count an unread message", source))?;
        }
        (row_id, true)
      }
    };

    Ok(Insertion {
      message: StoredMessage {
        row_id,
        topic: message.topic.clone(),
        msg_id: message.msg_id.clone(),
        ts: message.ts.clone(),
        received_ts: message.received_ts.clone(),
        sender_id: message.sender_id.clone(),
        sender_name: message.sender_name.clone(),
        app: message.app.clone(),
        tier: message.tier.clone(),
        level: message.level.clone(),
        title: message.title.clone(),
        body: message.body.clone(),
        raw: message.raw.clone(),
        qos: message.qos,
        retain: message.retain,
        outgoing: message.outgoing,
      },
      is_new,
      topic_is_new,
    })
  }

  /// Every topic, in alphabetical order, which is the order the tree renders.
  pub fn topics(&self) -> Result<Vec<TopicRow>> {
    let connection = self.lock();
    let mut statement = connection
      .prepare(
        "SELECT t.topic, t.first_seen_ts, t.last_seen_ts, t.unread, COUNT(m.id)
         FROM topics t LEFT JOIN messages m ON m.topic_id = t.id
         GROUP BY t.id ORDER BY t.topic",
      )
      .map_err(|source| failed("list its topics", source))?;
    let rows = statement
      .query_map([], |row| {
        Ok(TopicRow {
          topic: row.get(0)?,
          first_seen_ts: row.get(1)?,
          last_seen_ts: row.get(2)?,
          unread: row.get::<_, i64>(3)?.max(0) as u32,
          messages: row.get::<_, i64>(4)?.max(0) as u32,
        })
      })
      .map_err(|source| failed("list its topics", source))?;
    rows
      .collect::<rusqlite::Result<Vec<_>>>()
      .map_err(|source| failed("list its topics", source))
  }

  /// One page of a topic and all its descendants, oldest first.
  ///
  /// `before` is the `row_id` of the oldest message already on screen, so paging
  /// upwards is `before = rows.first().row_id`. The page is taken from the newest end
  /// and handed back in reading order.
  pub fn messages(&self, topic: &str, before: Option<i64>, limit: u32) -> Result<Vec<StoredMessage>> {
    let limit = if limit == 0 { DEFAULT_PAGE_SIZE } else { limit };
    let (descendants, after_descendants) = descendant_topic_bounds(topic);
    let connection = self.lock();
    let mut statement = connection
      .prepare(
        "SELECT m.id, t.topic, m.msg_id, m.ts, m.received_ts, m.sender_id, m.sender_name, m.app,
                m.tier, m.level, m.title, m.body, m.raw, m.qos, m.retain, m.outgoing
         FROM messages m JOIN topics t ON t.id = m.topic_id
         WHERE (t.topic = ?1 OR (t.topic >= ?2 AND t.topic < ?3))
           AND (?4 IS NULL OR m.id < ?4)
         ORDER BY m.id DESC LIMIT ?5",
      )
      .map_err(|source| failed("read a topic's history", source))?;
    let rows = statement
      .query_map(
        params![topic, descendants, after_descendants, before, limit],
        read_message,
      )
      .map_err(|source| failed("read a topic's history", source))?;
    let mut messages = rows
      .collect::<rusqlite::Result<Vec<_>>>()
      .map_err(|source| failed("read a topic's history", source))?;
    messages.reverse();
    Ok(messages)
  }

  /// One stored message, by its topic and message id.
  pub fn message(&self, topic: &str, msg_id: &str) -> Result<Option<StoredMessage>> {
    let connection = self.lock();
    connection
      .query_row(
        "SELECT m.id, t.topic, m.msg_id, m.ts, m.received_ts, m.sender_id, m.sender_name, m.app,
                m.tier, m.level, m.title, m.body, m.raw, m.qos, m.retain, m.outgoing
         FROM messages m JOIN topics t ON t.id = m.topic_id
         WHERE t.topic = ?1 AND m.msg_id = ?2",
        params![topic, msg_id],
        read_message,
      )
      .optional()
      .map_err(|source| failed("read a message", source))
  }

  /// Clears unread counts for the selected topic and all its descendants.
  pub fn mark_read(&self, topic: &str) -> Result<()> {
    let (descendants, after_descendants) = descendant_topic_bounds(topic);
    let connection = self.lock();
    connection
      .execute(
        "UPDATE topics SET unread = 0 WHERE topic = ?1 OR (topic >= ?2 AND topic < ?3)",
        params![topic, descendants, after_descendants],
      )
      .map_err(|source| failed("mark a topic read", source))?;
    Ok(())
  }

  /// Deletes the history of one topic, keeping the topic itself.
  ///
  /// The node stays in the tree because the user is still subscribed to it and asked
  /// to forget the messages, not the topic.
  pub fn clear_topic(&self, topic: &str) -> Result<u64> {
    let connection = self.lock();
    let deleted = connection
      .execute(
        "DELETE FROM messages WHERE topic_id IN (SELECT id FROM topics WHERE topic = ?1)",
        params![topic],
      )
      .map_err(|source| failed("clear a topic", source))?;
    connection
      .execute("UPDATE topics SET unread = 0 WHERE topic = ?1", params![topic])
      .map_err(|source| failed("clear a topic", source))?;
    Ok(deleted as u64)
  }

  /// Applies `gui.history`: a cap per topic and a retention window.
  ///
  /// Both limits treat 0 as "no limit", as `docs/specs/config.md` says.
  pub fn prune(&self, history: &History) -> Result<Pruned> {
    let mut pruned = Pruned::default();
    let connection = self.lock();
    if history.max_messages_per_topic > 0 {
      let deleted = connection
        .execute(
          "DELETE FROM messages WHERE id IN (
             SELECT id FROM (
               SELECT id, ROW_NUMBER() OVER (PARTITION BY topic_id ORDER BY id DESC) AS rank FROM messages
             ) WHERE rank > ?1
           )",
          params![history.max_messages_per_topic],
        )
        .map_err(|source| failed("prune by count", source))?;
      pruned.by_count = deleted as u64;
    }
    if history.retention_days > 0 {
      let cutoff = chrono::Utc::now() - chrono::Duration::days(i64::from(history.retention_days));
      let deleted = connection
        .execute(
          "DELETE FROM messages WHERE received_ts < ?1",
          params![cutoff.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)],
        )
        .map_err(|source| failed("prune by age", source))?;
      pruned.by_age = deleted as u64;
    }
    Ok(pruned)
  }

  /// How many messages are stored altogether.
  pub fn message_count(&self) -> Result<u64> {
    let connection = self.lock();
    let count: i64 = connection
      .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
      .map_err(|source| failed("count its messages", source))?;
    Ok(count.max(0) as u64)
  }

  /// A poisoned lock means an earlier caller panicked mid-statement, not that the
  /// database is broken, so the connection is taken anyway rather than turning one
  /// panic into a panic in every later call.
  fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
    self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

impl std::fmt::Debug for Store {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.debug_struct("Store").field("path", &self.path).finish()
  }
}

/// An indexed, case-sensitive prefix range under SQLite's BINARY collation.
/// '0' immediately follows '/' in byte order, so every name beginning with
/// "{topic}/" is in this range. SQL wildcard characters in names stay literal.
fn descendant_topic_bounds(topic: &str) -> (String, String) {
  (format!("{topic}/"), format!("{topic}0"))
}

fn read_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessage> {
  Ok(StoredMessage {
    row_id: row.get(0)?,
    topic: row.get(1)?,
    msg_id: row.get(2)?,
    ts: row.get(3)?,
    received_ts: row.get(4)?,
    sender_id: row.get(5)?,
    sender_name: row.get(6)?,
    app: row.get(7)?,
    tier: row.get(8)?,
    level: row.get(9)?,
    title: row.get(10)?,
    body: row.get(11)?,
    raw: row.get(12)?,
    qos: row.get::<_, i64>(13)?.clamp(0, 2) as u8,
    retain: row.get(14)?,
    outgoing: row.get(15)?,
  })
}

fn failed(operation: &str, source: rusqlite::Error) -> Error {
  Error::Storage {
    operation: operation.to_owned(),
    reason: source.to_string(),
  }
}

/// Now, in the same RFC 3339 shape a message envelope carries.
fn now() -> String {
  chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
