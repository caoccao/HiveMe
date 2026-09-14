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

//! A long running process's view of the shared config file.
//!
//! A thin wrapper over [`crate::config`], which owns the format, the per-OS location,
//! the migrations, and the writer that preserves keys this build does not know. The
//! session adds only what a long running process needs: one copy in memory, behind a
//! lock, so that an operation, the MQTT task, and a window handler all see the same
//! thing.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use crate::config::{Config, ConfigFile};
use crate::error::Result;

/// The config file, plus the reason it could not be read when that is what happened.
struct Holder {
  file: ConfigFile,
  /// Set when the file on disk could not be parsed, which makes writing unsafe until
  /// the user says otherwise by saving the settings.
  load_error: Option<String>,
}

/// The config in memory.
pub struct ConfigStore {
  holder: RwLock<Holder>,
  writer: Mutex<()>,
}

impl ConfigStore {
  /// Reads the config file at `path`, creating a default one when there is none.
  ///
  /// A file that cannot be read is reported rather than replaced: the application opens
  /// on defaults, says what went wrong, and writes nothing until the user saves the
  /// settings. Overwriting a config a user can still repair by hand would be worse than
  /// starting without it.
  pub fn open(path: &Path) -> Result<Self> {
    let holder = match ConfigFile::load_or_create(path) {
      Ok((file, created)) => {
        if created {
          log::info!("wrote a default config to {}", file.path().display());
        }
        Holder { file, load_error: None }
      }
      Err(error) => {
        log::error!("{} cannot be read, running on defaults: {error}", path.display());
        // `{}` parses into the defaults, so the screen has a complete config to render
        // and the untouched document is still on disk.
        let file = ConfigFile::from_text(path, "{}")?;
        Holder {
          file,
          load_error: Some(error.to_string()),
        }
      }
    };
    Ok(Self {
      holder: RwLock::new(holder),
      writer: Mutex::new(()),
    })
  }

  /// The config as it stands.
  pub fn get(&self) -> Config {
    self.read().file.config().clone()
  }

  /// Where the config file is.
  pub fn path(&self) -> PathBuf {
    self.read().file.path().to_path_buf()
  }

  /// Where the history database is, which is beside the config file.
  pub fn database_path(&self) -> PathBuf {
    self.read().file.database_path()
  }

  /// Why the config file could not be read, when that happened.
  pub fn load_error(&self) -> Option<String> {
    self.read().load_error.clone()
  }

  /// Validates and writes a config the user asked to save.
  ///
  /// This is the one path that overwrites a file that could not be read, because saving
  /// the settings is the user saying to. The broker is not part of what is validated
  /// here: see [`Config::validate_for_save`].
  pub fn set(&self, config: Config) -> Result<()> {
    config.validate_for_save()?;
    self.update(|file, _| {
      file.save_config(config)?;
      Ok(None)
    })
  }

  /// Writes a config change the user did not ask for, such as the window geometry.
  ///
  /// Silently does nothing while the file on disk is unreadable, so that moving the
  /// window cannot destroy a config the user is in the middle of repairing.
  pub fn set_quietly(&self, config: Config) -> Result<()> {
    self.update(|file, load_error| {
      if let Some(error) = &load_error {
        log::debug!("not saving: {} could not be read ({error})", file.path().display());
        file.set_config(config);
      } else {
        file.save_config(config)?;
      }
      Ok(load_error)
    })
  }

  /// Serializes writers while readers retain the last successfully saved snapshot.
  fn update(&self, save: impl FnOnce(&mut ConfigFile, Option<String>) -> Result<Option<String>>) -> Result<()> {
    let _writer = self.writer.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (mut file, load_error) = {
      let holder = self.read();
      (holder.file.clone(), holder.load_error.clone())
    };
    let load_error = save(&mut file, load_error)?;
    *self.write() = Holder { file, load_error };
    Ok(())
  }

  fn read(&self) -> std::sync::RwLockReadGuard<'_, Holder> {
    self.holder.read().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  fn write(&self) -> std::sync::RwLockWriteGuard<'_, Holder> {
    self.holder.write().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

impl std::fmt::Debug for ConfigStore {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ConfigStore")
      .field("path", &self.path())
      .field("load_error", &self.load_error())
      .finish()
  }
}

/// Whether a change between these two configs means the connection has to be remade.
///
/// Everything the CONNECT packet or the subscription list is built from. Changing a
/// theme must not drop the connection, and changing a password must.
pub fn needs_reconnect(before: &Config, after: &Config) -> bool {
  before.broker != after.broker || before.topics.subscriptions != after.topics.subscriptions
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_the_fields_a_connection_is_built_from_force_a_reconnect() {
    let before = Config::default();

    let mut appearance = before.clone();
    appearance.gui.theme = crate::config::Theme::Rose;
    assert!(!needs_reconnect(&before, &appearance));

    let mut password = before.clone();
    password.broker.password = "new".to_owned();
    assert!(needs_reconnect(&before, &password));

    let mut subscriptions = before.clone();
    subscriptions.topics.subscriptions.clear();
    assert!(needs_reconnect(&before, &subscriptions));
  }

  #[test]
  fn a_config_that_cannot_be_read_is_kept_on_disk_until_the_user_saves() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    std::fs::write(&path, "{ not json").unwrap();

    let store = ConfigStore::open(&path).unwrap();
    assert!(store.load_error().is_some());

    let mut config = store.get();
    config.gui.window.size.width = 1_024;
    store.set_quietly(config).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");

    let mut config = store.get();
    config.device.id = "0f9c2d1e-1111-7000-8000-aaaabbbbcccc".to_owned();
    config.broker.url = "mqtts://abc123.s1.eu.hivemq.cloud:8883".to_owned();
    config.broker.username = "hiveme".to_owned();
    config.broker.password = "secret".to_owned();
    store.set(config).unwrap();
    assert!(store.load_error().is_none());
    assert_ne!(std::fs::read_to_string(&path).unwrap(), "{ not json");
  }

  #[test]
  fn a_config_the_validator_refuses_is_not_written() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    let store = ConfigStore::open(&path).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();

    let mut config = store.get();
    config.notifications.rules[0].topic = "build/#/ci".to_owned();
    let error = store.set(config).unwrap_err();

    assert!(error.is_config(), "{error}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
  }

  #[test]
  fn the_settings_save_before_the_broker_is_filled_in() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    let store = ConfigStore::open(&path).unwrap();

    // What a fresh install looks like: no address, no credentials, and a user who has
    // just picked a theme in the Settings tab.
    let mut config = store.get();
    assert!(config.broker.url.is_empty());
    config.gui.theme = crate::config::Theme::Rose;
    store.set(config).unwrap();

    assert_eq!(store.get().gui.theme, crate::config::Theme::Rose);
    assert!(std::fs::read_to_string(&path).unwrap().contains("\"theme\": \"Rose\""));
  }
  #[test]
  fn saving_does_not_block_readers_and_swaps_only_after_persistence() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::open(&directory.path().join("HiveMe.json")).unwrap();
    let before = store.get();
    let mut after = before.clone();
    after.gui.theme = crate::config::Theme::Rose;
    store
      .update(|file, load_error| {
        // This runs at the persistence boundary, while the serialized writer is busy.
        let holder = store.holder.try_read().expect("disk I/O must not hold the reader lock");
        assert_eq!(holder.file.config(), &before);
        assert!(store.writer.try_lock().is_err(), "another writer must wait");
        drop(holder);
        file.save_config(after.clone())?;
        assert_eq!(
          store.get(),
          before,
          "unpublished snapshot stays unchanged until save completes"
        );
        Ok(load_error)
      })
      .unwrap();
    assert_eq!(store.get(), after);
    assert_eq!(ConfigFile::load(&store.path()).unwrap().config(), &after);
  }
}
