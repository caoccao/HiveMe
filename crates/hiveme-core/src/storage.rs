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

//! The local message history of `hmg` and the terminal UI of `hmc`.
//!
//! SQLite through `rusqlite` with the bundled library, in `HiveMe.db` beside the
//! config file. The schema is specified in `docs/specs/gui.md`. Both applications may
//! have the file open at once, which `docs/specs/session.md` describes.
//!
//! The store is what makes the topic tree and the chat view survive a restart. It also
//! decides what is new: a row exists once per topic and message id, so a message the
//! composer sent and the copy the broker echoes back collapse into one bubble rather
//! than two.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::config::History;
use crate::error::{Error, Result};
use crate::message::{Level, Parsed};

const HISTORY_SQL: &str = "SELECT m.id, t.topic, m.msg_id, m.ts, m.received_ts, m.sender_id, m.sender_name, m.app,
                m.tier, m.level, m.title, m.body, m.raw, m.qos, m.retain, m.outgoing
         FROM messages m NOT INDEXED CROSS JOIN topics t ON t.id = m.topic_id
         WHERE m.id < COALESCE(?4, 9223372036854775807)
           AND m.topic_id IN (SELECT id FROM topics WHERE topic = ?1 OR (topic >= ?2 AND topic < ?3))
         ORDER BY m.id DESC LIMIT ?5";

/// The version of the database schema this build writes.
pub const SCHEMA_VERSION: i64 = 1;

/// How many messages one page of history holds when the caller does not say.
pub const DEFAULT_PAGE_SIZE: u32 = 200;

/// How often the session prunes, after the pass it makes at startup.
pub const PRUNE_INTERVAL_SECS: u64 = 600;

/// How long a write waits for another process's write to finish before it fails.
///
/// `hmg` and the terminal UI of `hmc` share `HiveMe.db`. WAL lets readers and one
/// writer work side by side, but a second writer is refused at once unless it is told
/// to wait.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

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
    Self::from_parsed(topic, raw, &parsed, qos, retain, outgoing)
  }

  /// Builds a row from the parse already used for rule evaluation.
  pub fn from_parsed(
    topic: impl Into<String>,
    raw: Vec<u8>,
    parsed: &Parsed,
    qos: u8,
    retain: bool,
    outgoing: bool,
  ) -> Self {
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
    if let Parsed::Envelope(envelope) = parsed {
      message.msg_id = envelope.id.clone();
      message.ts = envelope.ts.clone();
      message.level = Some(envelope.level().as_str().to_owned());
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

/// Only a corrupt file or an incompatible layout justifies discarding history.
enum OpenError {
  Recreate(Error),
  Keep(Error),
}

impl OpenError {
  fn into_error(self) -> Error {
    match self {
      Self::Recreate(error) | Self::Keep(error) => error,
    }
  }
}

fn opening_failed(operation: &str, source: rusqlite::Error) -> OpenError {
  let corrupt = matches!(
    source.sqlite_error_code(),
    Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
  );
  let error = failed(operation, source);
  if corrupt {
    OpenError::Recreate(error)
  } else {
    OpenError::Keep(error)
  }
}

impl Store {
  /// Opens history, recreating corrupt or incompatible unpublished databases.
  ///
  /// Config files are untouched. Permission, locking, and other I/O failures do not
  /// discard history. Recovery closes the failed connection and retries only once.
  pub fn open(path: &Path) -> Result<Self> {
    if let Some(directory) = path.parent()
      && !directory.as_os_str().is_empty()
    {
      std::fs::create_dir_all(directory).map_err(|source| Error::ConfigWrite {
        path: directory.to_path_buf(),
        source,
      })?;
    }
    match Self::open_once(path) {
      Ok(store) => Ok(store),
      Err(OpenError::Keep(error)) => Err(error),
      Err(OpenError::Recreate(error)) => {
        log::warn!("recreating the message history at {}: {error}", path.display());
        // open_once has dropped its connection. Remove journals before the main file
        // so none of the discarded database's pages can enter its replacement.
        for suffix in ["-wal", "-shm", "-journal", ""] {
          let mut file = path.as_os_str().to_os_string();
          file.push(suffix);
          if let Err(source) = std::fs::remove_file(&file)
            && source.kind() != std::io::ErrorKind::NotFound
          {
            return Err(Error::Storage {
              operation: "remove unusable history".to_owned(),
              reason: format!("{}: {source}", Path::new(&file).display()),
            });
          }
        }
        Self::open_once(path).map_err(OpenError::into_error)
      }
    }
  }

  fn open_once(path: &Path) -> std::result::Result<Self, OpenError> {
    let connection = Connection::open(path).map_err(|source| opening_failed("open the database", source))?;
    connection
      .busy_timeout(BUSY_TIMEOUT)
      .map_err(|source| opening_failed("set its busy timeout", source))?;
    let store = Self {
      connection: Mutex::new(connection),
      path: path.to_path_buf(),
    };
    store.initialize_schema()?;
    Ok(store)
  }

  /// Opens a database that lives only as long as the process, for tests.
  pub fn in_memory() -> Result<Self> {
    let connection = Connection::open_in_memory().map_err(|source| failed("open the database", source))?;
    let store = Self {
      connection: Mutex::new(connection),
      path: PathBuf::from(":memory:"),
    };
    store.initialize_schema().map_err(OpenError::into_error)?;
    Ok(store)
  }

  /// Where the database file is.
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// How large the database file is, for the status bar. Zero when it has no file.
  pub fn size_bytes(&self) -> u64 {
    ["", "-wal", "-shm"]
      .iter()
      .map(|suffix| {
        let mut path = self.path.as_os_str().to_os_string();
        path.push(suffix);
        std::fs::metadata(Path::new(&path)).map(|meta| meta.len()).unwrap_or(0)
      })
      .sum()
  }

  /// Creates the unpublished application schema; no format migrations are performed.
  ///
  /// Version 0 means a database this build has not stamped, whether it is brand new or
  /// was written before `schema_version` existed, so the tables are created with
  /// `IF NOT EXISTS` and the version is written afterward.
  fn initialize_schema(&self) -> std::result::Result<(), OpenError> {
    let connection = self.lock();
    // Validate an existing development database before any PRAGMA or CREATE writes.
    // The app is unpublished: incompatible layouts are recreated, never migrated.
    for (table, required) in [
      ("schema_version", &["version"][..]),
      (
        "topics",
        &["id", "topic", "first_seen_ts", "last_seen_ts", "unread", "messages"][..],
      ),
      (
        "messages",
        &[
          "id",
          "topic_id",
          "msg_id",
          "ts",
          "received_ts",
          "sender_id",
          "sender_name",
          "app",
          "tier",
          "level",
          "title",
          "body",
          "raw",
          "qos",
          "retain",
          "outgoing",
          "unread",
        ][..],
      ),
    ] {
      let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|source| opening_failed("inspect its tables", source))?;
      let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|source| opening_failed("inspect its tables", source))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|source| opening_failed("inspect its tables", source))?;
      if !columns.is_empty()
        && let Some(missing) = required
          .iter()
          .find(|required| !columns.iter().any(|column| column == **required))
      {
        return Err(OpenError::Recreate(Error::Storage {
          operation: "open the database".to_owned(),
          reason: format!("the existing {table} table lacks {missing}"),
        }));
      }
    }
    connection
      .execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS topics (
           id            INTEGER PRIMARY KEY,
           topic         TEXT NOT NULL UNIQUE,
           first_seen_ts TEXT NOT NULL,
           last_seen_ts  TEXT NOT NULL,
           unread        INTEGER NOT NULL DEFAULT 0,
           messages      INTEGER NOT NULL DEFAULT 0
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
           unread      INTEGER NOT NULL DEFAULT 0,
           UNIQUE(topic_id, msg_id)
         );
         CREATE INDEX IF NOT EXISTS messages_topic_id_id ON messages(topic_id, id);
         CREATE INDEX IF NOT EXISTS messages_received_ts ON messages(received_ts);
         CREATE TRIGGER IF NOT EXISTS messages_insert_counts AFTER INSERT ON messages BEGIN
           UPDATE topics SET messages = messages + 1, unread = unread + NEW.unread WHERE id = NEW.topic_id;
         END;
         CREATE TRIGGER IF NOT EXISTS messages_delete_counts AFTER DELETE ON messages BEGIN
           UPDATE topics SET messages = messages - 1, unread = unread - OLD.unread WHERE id = OLD.topic_id;
         END;
         CREATE TRIGGER IF NOT EXISTS messages_read_counts AFTER UPDATE OF unread ON messages BEGIN
           UPDATE topics SET unread = unread + NEW.unread - OLD.unread WHERE id = NEW.topic_id;
         END;",
      )
      .map_err(|source| opening_failed("create its tables", source))?;

    let version: i64 = connection
      .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| row.get(0))
      .optional()
      .map_err(|source| opening_failed("read its schema version", source))?
      .unwrap_or(0);
    // A future version with all known columns remains usable; keep its stamp.
    if version == 0 {
      connection
        .execute("DELETE FROM schema_version", [])
        .and_then(|_| {
          connection.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            params![SCHEMA_VERSION],
          )
        })
        .map_err(|source| opening_failed("stamp its schema version", source))?;
    }
    Ok(())
  }

  /// Stores a message, or recognizes one that is already there.
  ///
  /// The unread count only moves for a message that is new and came from the broker;
  /// what this installation sent has been seen by definition.
  pub fn insert(&self, message: &NewMessage) -> Result<Insertion> {
    let level = message.level.as_ref().map(|level| level.to_lowercase());
    let mut connection = self.lock();
    // Immediate, so that two processes on one database never both find a message missing
    // and both insert it: the second waits out the first under the busy timeout and then
    // finds the row.
    let transaction = connection
      .transaction_with_behavior(TransactionBehavior::Immediate)
      .map_err(|source| failed("begin storing a message", source))?;
    let existing_topic: Option<i64> = transaction
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
        transaction
          .execute(
            "UPDATE topics SET last_seen_ts = ?2 WHERE id = ?1",
            params![id, message.received_ts],
          )
          .map_err(|source| failed("touch a topic", source))?;
        id
      }
      None => {
        transaction
          .execute(
            "INSERT INTO topics (topic, first_seen_ts, last_seen_ts, unread) VALUES (?1, ?2, ?2, 0)",
            params![message.topic, message.received_ts],
          )
          .map_err(|source| failed("record a topic", source))?;
        transaction.last_insert_rowid()
      }
    };

    let existing: Option<(i64, bool, u8, bool)> = transaction
      .query_row(
        "SELECT id, outgoing, qos, retain FROM messages WHERE topic_id = ?1 AND msg_id = ?2",
        params![topic_id, message.msg_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
      )
      .optional()
      .map_err(|source| failed("look up a message", source))?;

    let (row_id, is_new, outgoing, qos, retain) = match existing {
      // The broker echo can arrive before or after the publish completes. Preserve
      // the sender's publish options, which can differ from subscription delivery.
      Some((row_id, was_outgoing, stored_qos, stored_retain)) => {
        let outgoing = was_outgoing || message.outgoing;
        let (qos, retain) = if was_outgoing && !message.outgoing {
          (stored_qos, stored_retain)
        } else {
          (message.qos, message.retain)
        };
        transaction
          .execute(
            "UPDATE messages SET qos = ?2, retain = ?3, outgoing = ?4,
               unread = CASE WHEN ?4 THEN 0 ELSE unread END,
               app = COALESCE(app, ?5), sender_id = COALESCE(sender_id, ?6), sender_name = COALESCE(sender_name, ?7)
             WHERE id = ?1",
            params![
              row_id,
              qos,
              retain,
              outgoing,
              message.app,
              message.sender_id,
              message.sender_name
            ],
          )
          .map_err(|source| failed("update a message", source))?;
        (row_id, false, outgoing, qos, retain)
      }
      None => {
        transaction
          .execute(
            "INSERT INTO messages (
               topic_id, msg_id, ts, received_ts, sender_id, sender_name, app,
               tier, level, title, body, raw, qos, retain, outgoing, unread
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
              topic_id,
              message.msg_id,
              message.ts,
              message.received_ts,
              message.sender_id,
              message.sender_name,
              message.app,
              message.tier,
              level,
              message.title,
              message.body,
              message.raw,
              message.qos,
              message.retain,
              message.outgoing,
              !message.outgoing,
            ],
          )
          .map_err(|source| failed("store a message", source))?;
        let row_id = transaction.last_insert_rowid();
        (row_id, true, message.outgoing, message.qos, message.retain)
      }
    };
    transaction
      .commit()
      .map_err(|source| failed("store a message", source))?;

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
        level,
        title: message.title.clone(),
        body: message.body.clone(),
        raw: message.raw.clone(),
        qos,
        retain,
        outgoing,
      },
      is_new,
      topic_is_new,
    })
  }

  /// Every topic, in alphabetical order, which is the order the tree renders.
  pub fn topics(&self) -> Result<Vec<TopicRow>> {
    let connection = self.lock();
    let mut statement = connection
      .prepare("SELECT topic, first_seen_ts, last_seen_ts, unread, messages FROM topics ORDER BY topic")
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
      .prepare(HISTORY_SQL)
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
        "UPDATE messages SET unread = 0 WHERE unread != 0 AND topic_id IN (SELECT id FROM topics WHERE topic = ?1 OR (topic >= ?2 AND topic < ?3))",
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
    let mut connection = self.lock();
    let transaction = connection
      .transaction_with_behavior(TransactionBehavior::Immediate)
      .map_err(|source| failed("begin clearing a topic", source))?;
    let deleted = transaction
      .execute(
        "DELETE FROM messages WHERE topic_id IN (SELECT id FROM topics WHERE topic = ?1)",
        params![topic],
      )
      .map_err(|source| failed("clear a topic", source))?;
    transaction.commit().map_err(|source| failed("clear a topic", source))?;
    Ok(deleted as u64)
  }

  /// Applies `gui.history`: a cap per topic and a retention window.
  ///
  /// Both limits treat 0 as "no limit", as `docs/specs/config.md` says.
  ///
  /// The unread counts come down with the messages. A count is a number of rows that
  /// arrived and were never looked at, and a row that has been deleted can never be
  /// looked at now, so a badge left standing over an empty topic is counting messages
  /// nobody can ever read. The whole pass is one transaction, so the counts and the
  /// rows they describe are never briefly out of step for a reader in the other
  /// application.
  pub fn prune(&self, history: &History) -> Result<Pruned> {
    let mut pruned = Pruned::default();
    let mut connection = self.lock();
    let transaction = connection
      .transaction_with_behavior(TransactionBehavior::Immediate)
      .map_err(|source| failed("begin pruning", source))?;
    if history.max_messages_per_topic > 0 {
      let deleted = transaction
        .execute(
          "DELETE FROM messages WHERE id IN (
             SELECT id FROM (
               SELECT id, ROW_NUMBER() OVER (PARTITION BY topic_id ORDER BY id DESC) AS rank FROM messages
               WHERE topic_id IN (SELECT id FROM topics WHERE messages > ?1)
             ) WHERE rank > ?1
           )",
          params![history.max_messages_per_topic],
        )
        .map_err(|source| failed("prune by count", source))?;
      pruned.by_count = deleted as u64;
    }
    if history.retention_days > 0 {
      let cutoff = chrono::Utc::now() - chrono::Duration::days(i64::from(history.retention_days));
      let deleted = transaction
        .execute(
          "DELETE FROM messages WHERE received_ts < ?1",
          params![cutoff.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)],
        )
        .map_err(|source| failed("prune by age", source))?;
      pruned.by_age = deleted as u64;
    }
    transaction.commit().map_err(|source| failed("prune", source))?;
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

#[cfg(test)]
mod review_tests {
  use super::*;

  #[test]
  fn a_large_subtree_pages_by_primary_key_without_a_temporary_sort() {
    let store = Store::in_memory().unwrap();
    {
      let connection = store.lock();
      connection
        .execute_batch(
          "INSERT INTO topics (topic, first_seen_ts, last_seen_ts) VALUES ('hiveme/child','now','now');
        WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000)
        INSERT INTO messages(topic_id,msg_id,ts,received_ts,tier,body,raw,qos,retain,outgoing,unread)
        SELECT 1,CAST(x AS TEXT),'now','now','text','body',X'00',1,0,0,1 FROM n;",
        )
        .unwrap();
      let mut query = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {HISTORY_SQL}"))
        .unwrap();
      let plan = query
        .query_map(params!["hiveme", "hiveme/", "hiveme0", 99000, 200], |row| {
          row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n");
      assert!(!plan.contains("TEMP B-TREE"), "{plan}");
      assert!(plan.contains("INTEGER PRIMARY KEY"), "{plan}");
    }
    let start = std::time::Instant::now();
    let rows = store.messages("hiveme", Some(99000), 200).unwrap();
    assert_eq!(rows.len(), 200);
    assert_eq!(rows.last().unwrap().row_id, 98999);
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(store.topics().unwrap()[0].messages, 100000);
    store
      .prune(&History {
        max_messages_per_topic: 100,
        retention_days: 0,
      })
      .unwrap();
    assert_eq!(
      (store.topics().unwrap()[0].messages, store.topics().unwrap()[0].unread),
      (100, 100)
    );
    store.clear_topic("hiveme/child").unwrap();
    assert_eq!(
      (store.topics().unwrap()[0].messages, store.topics().unwrap()[0].unread),
      (0, 0)
    );
  }
  #[test]
  fn an_incompatible_development_layout_is_recreated_and_can_store_new_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.db");
    let connection = Connection::open(&path).unwrap();
    connection
      .execute_batch(
        "CREATE TABLE topics(id INTEGER PRIMARY KEY, topic TEXT, unread INTEGER);
      CREATE TABLE messages(id INTEGER PRIMARY KEY, body TEXT);
      INSERT INTO messages(body) VALUES ('old development history');",
      )
      .unwrap();
    drop(connection);
    assert_fresh_usable_history(&path);
  }

  fn assert_fresh_usable_history(path: &Path) {
    let store = Store::open(path).expect("unusable history is recreated");
    assert!(store.topics().unwrap().is_empty());
    assert!(store.messages("hiveme", None, 10).unwrap().is_empty());
    let message = NewMessage::from_payload("hiveme", b"new history".to_vec(), 1, false, false);
    store.insert(&message).unwrap();
    assert_eq!(
      (store.topics().unwrap()[0].messages, store.topics().unwrap()[0].unread),
      (1, 1)
    );
    drop(store);
    let reopened = Store::open(path).unwrap();
    assert_eq!(reopened.messages("hiveme", None, 10).unwrap()[0].body, "new history");
    assert_eq!(
      reopened
        .lock()
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .unwrap(),
      "ok"
    );
  }

  #[test]
  fn a_non_database_file_and_its_stale_journals_are_recreated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.db");
    for suffix in ["", "-wal", "-shm", "-journal"] {
      std::fs::write(directory.path().join(format!("HiveMe.db{suffix}")), b"not a database").unwrap();
    }
    assert_fresh_usable_history(&path);
    assert!(!directory.path().join("HiveMe.db-journal").exists());
  }

  #[test]
  fn a_corrupt_sqlite_schema_page_is_recreated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.db");
    drop(Store::open(&path).unwrap());
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[100] = 0xff; // Invalid b-tree page type, after the valid SQLite file header.
    std::fs::write(&path, bytes).unwrap();
    assert_fresh_usable_history(&path);
  }

  #[test]
  fn compatible_future_history_keeps_its_messages_and_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.db");
    let store = Store::open(&path).unwrap();
    store
      .insert(&NewMessage::from_payload(
        "hiveme",
        b"preserved".to_vec(),
        1,
        false,
        false,
      ))
      .unwrap();
    store.lock().execute_batch("UPDATE schema_version SET version = 99; CREATE TABLE future_extension(value TEXT); INSERT INTO future_extension VALUES ('preserved too');").unwrap();
    drop(store);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.messages("hiveme", None, 10).unwrap()[0].body, "preserved");
    let connection = reopened.lock();
    assert_eq!(
      connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get::<_, i64>(0))
        .unwrap(),
      99
    );
    assert_eq!(
      connection
        .query_row("SELECT value FROM future_extension", [], |row| row.get::<_, String>(0))
        .unwrap(),
      "preserved too"
    );
  }

  #[test]
  fn a_locked_database_is_not_deleted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.db");
    let store = Store::open(&path).unwrap();
    store
      .insert(&NewMessage::from_payload(
        "hiveme",
        b"preserved".to_vec(),
        1,
        false,
        false,
      ))
      .unwrap();
    drop(store);
    let connection = Connection::open(&path).unwrap();
    connection
      .execute_batch("PRAGMA journal_mode = DELETE; BEGIN EXCLUSIVE;")
      .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(Store::open(&path).unwrap_err().to_string().contains("locked"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    connection.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
      Store::open(&path).unwrap().messages("hiveme", None, 10).unwrap()[0].body,
      "preserved"
    );
  }

  #[test]
  fn an_unopenable_path_is_not_removed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.db");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("keep"), b"preserved").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(std::fs::read(path.join("keep")).unwrap(), b"preserved");
  }
}
