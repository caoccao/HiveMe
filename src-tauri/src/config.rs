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

//! The GUI's view of the shared config file.
//!
//! A thin wrapper over [`hiveme_core::config`], which owns the format, the per-OS
//! location, the migrations, and the writer that preserves keys this build does not
//! know. `hmg` adds only what a long running process needs: one copy in memory, behind
//! a lock, so that a command, the MQTT task, and the window handler all see the same
//! thing.

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use anyhow::{Result, anyhow};
use hiveme_core::config::{Config, ConfigFile};

static CONFIG: OnceLock<RwLock<Holder>> = OnceLock::new();

/// The config file, plus the reason it could not be read when that is what happened.
struct Holder {
  file: ConfigFile,
  /// Set when the file on disk could not be parsed, which makes writing unsafe until
  /// the user says otherwise by saving the Settings tab.
  load_error: Option<String>,
}

/// Reads the config file, creating a default one when there is none.
///
/// A file that cannot be read is reported rather than replaced: the GUI opens on
/// defaults, says what went wrong, and writes nothing until the user saves the
/// Settings tab. Overwriting a config a user can still repair by hand would be worse
/// than starting without it.
pub fn init() -> Result<()> {
  let path = hiveme_core::config::config_path(None).map_err(|error| anyhow!(error.to_string()))?;
  let holder = match ConfigFile::load_or_create(&path) {
    Ok((file, created)) => {
      if created {
        log::info!("wrote a default config to {}", file.path().display());
      }
      Holder { file, load_error: None }
    }
    Err(error) => {
      log::error!("{} cannot be read, running on defaults: {error}", path.display());
      // `{}` parses into the defaults, so the GUI has a complete config to render and
      // the untouched document is still on disk.
      let file = ConfigFile::from_text(&path, "{}").map_err(|error| anyhow!(error.to_string()))?;
      Holder {
        file,
        load_error: Some(error.to_string()),
      }
    }
  };
  let error = holder.load_error.clone();
  CONFIG
    .set(RwLock::new(holder))
    .map_err(|_| anyhow!("the config was initialized twice"))?;
  match error {
    Some(error) => Err(anyhow!(error)),
    None => Ok(()),
  }
}

fn holder() -> &'static RwLock<Holder> {
  CONFIG
    .get()
    .expect("config::init runs before anything reads the config")
}

/// The config as it stands.
pub fn get_config() -> Config {
  holder().read().unwrap().file.config().clone()
}

/// Where the config file is.
pub fn path() -> PathBuf {
  holder().read().unwrap().file.path().to_path_buf()
}

/// Where the history database is, which is beside the config file.
pub fn database_path() -> PathBuf {
  holder().read().unwrap().file.database_path()
}

/// Why the config file could not be read, when that happened.
pub fn load_error() -> Option<String> {
  holder().read().unwrap().load_error.clone()
}

/// Validates and writes a config the user asked to save.
///
/// This is the one path that overwrites a file that could not be read, because saving
/// the Settings tab is the user saying to.
pub fn set_config(config: Config) -> Result<()> {
  config.validate().map_err(|error| anyhow!(error.to_string()))?;
  let mut holder = holder().write().unwrap();
  holder.file.set_config(config);
  holder.file.save().map_err(|error| anyhow!(error.to_string()))?;
  holder.load_error = None;
  Ok(())
}

/// Writes a config change the user did not ask for, such as the window geometry.
///
/// Silently does nothing while the file on disk is unreadable, so that moving the
/// window cannot destroy a config the user is in the middle of repairing.
pub fn set_config_quietly(config: Config) -> Result<()> {
  let mut holder = holder().write().unwrap();
  if let Some(error) = holder.load_error.as_deref() {
    log::debug!(
      "not saving: {} could not be read ({error})",
      holder.file.path().display()
    );
    holder.file.set_config(config);
    return Ok(());
  }
  holder.file.set_config(config);
  holder.file.save().map_err(|error| anyhow!(error.to_string()))
}

/// Whether a change between these two configs means the connection has to be remade.
///
/// Everything the CONNECT packet or the subscription list is built from. Changing a
/// theme must not drop the connection, and changing a password must.
pub fn needs_reconnect(before: &Config, after: &Config) -> bool {
  before.broker != after.broker || before.topics.subscriptions != after.topics.subscriptions
}

/// The directory the config lives in, for the "open config file" command.
pub fn directory() -> PathBuf {
  path()
    .parent()
    .map(Path::to_path_buf)
    .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_the_fields_a_connection_is_built_from_force_a_reconnect() {
    let before = Config::default();

    let mut appearance = before.clone();
    appearance.gui.theme = hiveme_core::config::Theme::Rose;
    assert!(!needs_reconnect(&before, &appearance));

    let mut password = before.clone();
    password.broker.password = "new".to_owned();
    assert!(needs_reconnect(&before, &password));

    let mut subscriptions = before.clone();
    subscriptions.topics.subscriptions.clear();
    assert!(needs_reconnect(&before, &subscriptions));
  }
}
