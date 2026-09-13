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

//! OS notifications, from the rules in the config.
//!
//! The deciding is all in [`hiveme_core::rules`]: which rule matches, what the
//! templates render to, and how much one rule may say per second. This module is the
//! part that cannot live in the core, namely the plugin call and the toolbar toggle
//! that holds everything back for the session.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Instant;

use hiveme_core::config::Config;
use hiveme_core::message::Parsed;
use hiveme_core::rules::{Admission, NotificationLimiter, RuleEngine};
use tauri::{AppHandle, Emitter};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_notification::NotificationExt;

use crate::protocol::{EVENT_NOTIFICATION_FIRED, NotificationFiredEvent};

/// The application identity Windows prints above a toast.
///
/// A Tauri application raises its toasts through the Windows Runtime, and Windows
/// labels a toast with the identity of the process that raised it. Without an
/// identity of our own that label is PowerShell, so one is registered below.
#[cfg(target_os = "windows")]
const SYSTEM_NOTIFICATION_APP_ID: &str = crate::constants::APP_NAME;

/// The rule engine, its rate limiter, and the pause toggle.
pub struct Notifier {
  engine: RwLock<RuleEngine>,
  limiter: Mutex<NotificationLimiter>,
  paused: AtomicBool,
}

impl Notifier {
  /// Compiles the rules of `config`.
  pub fn new(config: &Config) -> Self {
    install_system_notification_identity();
    Self {
      engine: RwLock::new(RuleEngine::from_config(config)),
      limiter: Mutex::new(NotificationLimiter::default()),
      paused: AtomicBool::new(false),
    }
  }

  /// Compiles the rules again after the user saved the Settings tab.
  ///
  /// The rate limiter is cleared as well, because what it remembers is about rules
  /// that may no longer exist.
  pub fn reload(&self, config: &Config) {
    *self.engine.write().unwrap() = RuleEngine::from_config(config);
    self.limiter.lock().unwrap().reset();
  }

  /// Whether the toolbar toggle is holding notifications back.
  pub fn is_paused(&self) -> bool {
    self.paused.load(Ordering::Relaxed)
  }

  /// Pauses or resumes for the rest of the session.
  ///
  /// Resuming forgets what the limiter was holding, so the first message after a pause
  /// is shown on its own rather than carrying a count from before the pause.
  pub fn set_paused(&self, paused: bool) {
    self.paused.store(paused, Ordering::Relaxed);
    if !paused {
      self.limiter.lock().unwrap().reset();
    }
  }

  /// Raises a notification for a message, when the rules and the limiter allow it.
  ///
  /// Returns the rule that fired, so the caller can tell the frontend. A failure to
  /// show is logged and dropped: a desktop without a notification daemon is a reason
  /// for the message to be silent, not for it to be lost.
  pub fn notify(&self, app: &AppHandle, topic: &str, parsed: &Parsed, message_id: &str) -> Option<String> {
    if self.is_paused() {
      return None;
    }
    let notification = self.engine.read().unwrap().evaluate(topic, parsed)?;
    let admission = self
      .limiter
      .lock()
      .unwrap()
      .admit(&notification.rule_id, Instant::now());
    let Admission::Show { suppressed } = admission else {
      log::debug!("rule {} is rate limited, holding back {topic}", notification.rule_id);
      return None;
    };
    let notification = notification.with_suppressed(suppressed);

    if let Err(error) = show(app, &notification.title, &notification.body) {
      log::warn!("the OS would not show a notification: {error}");
      return None;
    }

    let _ = app.emit(
      EVENT_NOTIFICATION_FIRED,
      NotificationFiredEvent {
        rule_id: notification.rule_id.clone(),
        message_id: message_id.to_owned(),
        topic: topic.to_owned(),
      },
    );
    Some(notification.rule_id)
  }
}

/// Raises one OS notification.
///
/// Windows goes straight to the Windows Runtime rather than through the plugin, so
/// that the toast carries the identity registered above. Every other platform takes
/// the plugin, which already labels the notification with the bundle it came from.
fn show(app: &AppHandle, title: &str, body: &str) -> anyhow::Result<()> {
  #[cfg(target_os = "windows")]
  {
    let _ = app;
    tauri_winrt_notification::Toast::new(SYSTEM_NOTIFICATION_APP_ID)
      .title(title)
      .text1(body)
      .show()?;
  }

  #[cfg(not(target_os = "windows"))]
  app.notification().builder().title(title).body(body).show()?;

  Ok(())
}

/// Registers the Windows application identity, once per process.
///
/// A failure is logged and dropped: the toast still appears, under the wrong name, and
/// a notification with the wrong label is better than no notification at all.
fn install_system_notification_identity() {
  #[cfg(target_os = "windows")]
  {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
      if let Err(error) = register_system_notification_identity() {
        log::warn!("the Windows notification identity could not be registered: {error}");
      }
    });
  }
}

/// Writes the `AppUserModelId` entry Windows reads the toast label and icon from.
///
/// An installed build gets its icon from the Start menu shortcut the bundle creates,
/// so only a development build needs to say where the icon is.
#[cfg(target_os = "windows")]
fn register_system_notification_identity() -> anyhow::Result<()> {
  let key =
    windows_registry::CURRENT_USER.create(format!(r"SOFTWARE\Classes\AppUserModelId\{SYSTEM_NOTIFICATION_APP_ID}"))?;
  key.set_expand_string("DisplayName", SYSTEM_NOTIFICATION_APP_ID)?;
  key.set_string("IconBackgroundColor", "0")?;

  if tauri::is_dev() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("icons")
      .join("32x32.png");
    if icon.is_file() {
      key.set_expand_string("IconUri", icon.to_string_lossy())?;
    }
  }
  Ok(())
}

impl std::fmt::Debug for Notifier {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("Notifier")
      .field("paused", &self.is_paused())
      .field("rules", &self.engine.read().unwrap().rules().len())
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use hiveme_core::message::Level;

  #[test]
  fn pausing_is_the_first_thing_checked() {
    let notifier = Notifier::new(&Config::default());
    assert!(!notifier.is_paused());

    notifier.set_paused(true);
    assert!(notifier.is_paused());

    notifier.set_paused(false);
    assert!(!notifier.is_paused());
  }

  #[test]
  fn rules_match_the_payload_level_on_the_same_topic() {
    let notifier = Notifier::new(&Config::default());
    for level in [Level::Info, Level::Warn, Level::Error] {
      let engine = notifier.engine.read().unwrap();
      assert_eq!(engine.matching_enabled("hiveme", &level).unwrap().level, level);
    }
  }

  #[test]
  fn reloading_picks_up_rules_the_user_saved() {
    let notifier = Notifier::new(&Config::default());
    let mut config = Config::default();
    config.notifications.rules.clear();
    config.topics.prefix = "other".to_owned();

    notifier.reload(&config);

    assert!(
      notifier
        .engine
        .read()
        .unwrap()
        .matching_enabled("hiveme", &Level::Error)
        .is_none()
    );
  }
}
