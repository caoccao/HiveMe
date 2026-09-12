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

//! The error type shared by the HiveMe applications.
//!
//! `hmc` maps these onto the exit codes documented in `docs/specs/cli.md`, so the
//! variants are grouped by the category a user sees rather than by where they arise.

use std::path::PathBuf;

/// Anything that can go wrong inside `hiveme-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("no config file at {0}")]
  ConfigNotFound(PathBuf),

  #[error("cannot work out where the config lives: {0}")]
  ConfigPath(String),

  #[error("cannot read {path}: {source}")]
  ConfigRead { path: PathBuf, source: std::io::Error },

  #[error("cannot write {path}: {source}")]
  ConfigWrite { path: PathBuf, source: std::io::Error },

  #[error("{path} is not valid JSON: {source}")]
  ConfigParse { path: PathBuf, source: serde_json::Error },

  #[error("{path} was written by a version of HiveMe this build cannot read: {reason}")]
  ConfigMigrate { path: PathBuf, reason: String },

  #[error("the config is not usable:\n  {}", .0.join("\n  "))]
  ConfigInvalid(Vec<String>),

  #[error("the broker password is not available: {0}")]
  PasswordUnavailable(String),

  #[error("the setup string is not usable: {0}")]
  BrokerInit(String),

  #[error("TLS cannot be set up: {0}")]
  Tls(String),

  #[error("cannot connect to the broker: {0}")]
  Connect(String),

  #[error("the broker did not answer the connection within {0}s")]
  ConnectTimeout(u64),

  #[error("the connection to the broker was lost: {0}")]
  ConnectionLost(String),

  #[error("the MQTT client is not running: {0}")]
  ClientStopped(String),

  #[error("the broker rejected the message published to {topic}: {reason}")]
  PublishRejected { topic: String, reason: String },

  #[error("the broker did not acknowledge the message published to {topic} within {secs}s")]
  PublishTimeout { topic: String, secs: u64 },

  #[error("the broker rejected the subscription to {filter}: {reason}")]
  SubscribeRejected { filter: String, reason: String },

  #[error("the broker did not acknowledge the subscription within {0}s")]
  SubscribeTimeout(u64),

  #[error("the message store cannot {operation}: {reason}")]
  Storage { operation: String, reason: String },

  #[error("{0}")]
  NotImplemented(String),
}

impl Error {
  /// Whether this is a configuration problem, which `hmc` reports as exit code 3.
  pub fn is_config(&self) -> bool {
    matches!(
      self,
      Self::ConfigNotFound(_)
        | Self::ConfigPath(_)
        | Self::ConfigRead { .. }
        | Self::ConfigWrite { .. }
        | Self::ConfigParse { .. }
        | Self::ConfigMigrate { .. }
        | Self::ConfigInvalid(_)
        | Self::PasswordUnavailable(_)
        | Self::BrokerInit(_)
        // A trust store that cannot be assembled is a problem with broker.tls, not
        // with the network, so it is reported as one.
        | Self::Tls(_)
        | Self::NotImplemented(_)
    )
  }

  /// Whether the broker could not be reached or refused what was asked, which `hmc`
  /// reports as exit code 4.
  pub fn is_connection(&self) -> bool {
    matches!(
      self,
      Self::Connect(_)
        | Self::ConnectTimeout(_)
        | Self::ConnectionLost(_)
        | Self::ClientStopped(_)
        | Self::PublishRejected { .. }
        | Self::SubscribeRejected { .. }
        | Self::SubscribeTimeout(_)
    )
  }

  /// Whether a published message went unacknowledged, which `hmc` reports as exit
  /// code 5.
  pub fn is_publish_timeout(&self) -> bool {
    matches!(self, Self::PublishTimeout { .. })
  }
}

/// The result type used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
