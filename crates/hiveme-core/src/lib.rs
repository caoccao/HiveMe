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

//! Shared core of the HiveMe applications.
//!
//! `hmc` and `hmg` both build on this crate so that the config file, the message
//! format, the topic rules, and the MQTT behavior cannot drift apart.
//!
//! | Module    | Phase | Specification                |
//! |-----------|-------|------------------------------|
//! | [`config`]  | 1.1 | `docs/specs/config.md`       |
//! | [`message`] | 1.2 | `docs/specs/message.md`      |
//! | [`topic`]   | 1.3 | `docs/specs/config.md`       |
//! | [`rules`]   | 1.3 | `docs/specs/gui.md`          |
//! | [`mqtt`]    | 2.1 | `docs/specs/hivemq-cloud.md` |
//! | [`storage`] | 4.2 | `docs/specs/gui.md`          |
//! | `cloud`     | 6   | `docs/specs/hivemq-cloud.md` |
//! | `crypto`    | 6   | `docs/specs/message.md`      |

pub mod config;
pub mod error;
pub mod message;
pub mod mqtt;
pub mod rules;
#[cfg(feature = "storage")]
pub mod storage;
pub mod topic;

pub use config::{BrokerInit, Config, ConfigFile};
pub use error::{Error, Result};
pub use message::{Level, Message, Parsed, Payload, Sender};
pub use mqtt::{MqttClient, Qos, Role, State, Status};
#[cfg(feature = "storage")]
pub use storage::{NewMessage, Store, StoredMessage, TopicRow};

/// The version of the HiveMe applications, shared by every crate in the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The application name, used for the config directory and the window title.
pub const APP_NAME: &str = config::APP_NAME;

/// Both generated JSON schemas, keyed by the file name they are written to.
///
/// `cargo xtask schema` writes these and the freshness test compares against them, so
/// there is one definition of what the committed schemas should contain.
pub fn json_schemas() -> Vec<(&'static str, serde_json::Value)> {
  vec![
    ("broker-init.schema.json", config::broker_init_json_schema()),
    ("config.schema.json", config::json_schema()),
    ("message.schema.json", message::json_schema()),
  ]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn version_is_not_empty() {
    assert!(!VERSION.is_empty());
  }

  #[test]
  fn app_name_matches_the_config_directory_name() {
    // docs/specs/config.md resolves the config file to <config dir>/HiveMe/HiveMe.json.
    assert_eq!(APP_NAME, "HiveMe");
  }

  #[test]
  fn every_schema_is_generated() {
    let schemas = json_schemas();
    assert_eq!(schemas.len(), 3);
    for (name, schema) in schemas {
      assert!(schema.get("$id").is_some(), "{name} has no $id");
      assert_eq!(schema["type"], "object", "{name} is not an object schema");
    }
  }
}
