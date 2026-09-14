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
//! The deciding is all in [`crate::rules`]: which rule matches, what the templates
//! render to, and how much one rule may say per second. This module adds the toggle
//! that holds everything back for the session, and hands the one thing it cannot do,
//! showing the toast, to the application through [`Toaster`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use crate::config::Config;
use crate::message::Parsed;
use crate::rules::{Admission, NotificationLimiter, RuleEngine};

/// Shows one OS notification, the way the application that runs the session does it.
pub trait Toaster: Send + Sync {
  fn show(&self, title: &str, body: &str) -> Result<(), String>;
}

/// The rule engine, its rate limiter, and the pause toggle.
pub struct Notifier {
  engine: RwLock<RuleEngine>,
  locale: RwLock<crate::i18n::Locale>,
  limiter: Mutex<NotificationLimiter>,
  paused: AtomicBool,
  toaster: Arc<dyn Toaster>,
}

impl Notifier {
  /// Compiles the rules of `config`.
  pub fn new(config: &Config, toaster: Arc<dyn Toaster>) -> Self {
    Self {
      engine: RwLock::new(RuleEngine::from_config(config)),
      locale: RwLock::new(crate::i18n::Locale::resolve(&config.gui.language)),
      limiter: Mutex::new(NotificationLimiter::default()),
      paused: AtomicBool::new(false),
      toaster,
    }
  }

  /// Compiles the rules again after the user saved the settings.
  ///
  /// The rate limiter is cleared as well, because what it remembers is about rules
  /// that may no longer exist.
  pub fn reload(&self, config: &Config) {
    *self.locale.write().unwrap_or_else(|poisoned| poisoned.into_inner()) =
      crate::i18n::Locale::resolve(&config.gui.language);
    *self.engine.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = RuleEngine::from_config(config);
    self
      .limiter
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .reset();
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
      self
        .limiter
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .reset();
    }
  }

  /// Raises a notification for a message, when the rules and the limiter allow it.
  ///
  /// Returns the rule that fired, so the caller can raise the event. A failure to show
  /// is logged and dropped: a desktop without a notification daemon is a reason for the
  /// message to be silent, not for it to be lost.
  pub fn notify(&self, topic: &str, parsed: &Parsed) -> Option<String> {
    if self.is_paused() {
      return None;
    }
    let notification = self
      .engine
      .read()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .evaluate(topic, parsed)?;
    let admission = self
      .limiter
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .admit(&notification.rule_id, Instant::now());
    let Admission::Show { suppressed } = admission else {
      log::debug!("rule {} is rate limited, holding back {topic}", notification.rule_id);
      return None;
    };
    let suffix = if suppressed == 0 {
      String::new()
    } else {
      crate::i18n::t_count(
        *self.locale.read().unwrap_or_else(|poisoned| poisoned.into_inner()),
        "notifications.suppressed",
        u64::from(suppressed),
      )
    };
    let notification = notification.with_suppressed(&suffix);

    if let Err(error) = self.toaster.show(&notification.title, &notification.body) {
      log::warn!("the OS would not show a notification: {error}");
      return None;
    }
    log::debug!("rule {} showed a notification for {topic}", notification.rule_id);
    Some(notification.rule_id)
  }
}

impl std::fmt::Debug for Notifier {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("Notifier")
      .field("paused", &self.is_paused())
      .field(
        "rules",
        &self
          .engine
          .read()
          .unwrap_or_else(|poisoned| poisoned.into_inner())
          .rules()
          .len(),
      )
      .finish()
  }
}

/// Registers the shared Windows toast label, optionally with a development icon.
#[cfg(target_os = "windows")]
pub fn register_toast_identity(icon: Option<&std::path::Path>) -> Result<(), String> {
  let key = windows_registry::CURRENT_USER
    .create(format!(r"SOFTWARE\Classes\AppUserModelId\{}", crate::APP_NAME))
    .map_err(|error| error.to_string())?;
  key
    .set_expand_string("DisplayName", crate::APP_NAME)
    .map_err(|error| error.to_string())?;
  key
    .set_string("IconBackgroundColor", "0")
    .map_err(|error| error.to_string())?;
  if let Some(icon) = icon.filter(|icon| icon.is_file()) {
    key
      .set_expand_string("IconUri", icon.to_string_lossy())
      .map_err(|error| error.to_string())?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::message::{Level, Message, Sender};

  use crate::test_support::Recorder;

  fn error_from_elsewhere(body: &str) -> Parsed {
    let sender = Sender {
      id: Some("another-device".to_owned()),
      ..Sender::default()
    };
    let message = Message::new_text(sender, body).with_level(Level::Error);
    crate::message::parse(&message.to_bytes().unwrap())
  }

  #[test]
  fn pausing_is_the_first_thing_checked() {
    let recorder = Arc::new(Recorder::default());
    let notifier = Notifier::new(&Config::default(), recorder.clone());
    assert!(!notifier.is_paused());

    notifier.set_paused(true);
    assert!(notifier.is_paused());
    assert_eq!(notifier.notify("hiveme", &error_from_elsewhere("Disk full")), None);
    assert!(
      recorder
        .shown
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_empty(),
      "a paused session shows nothing"
    );

    notifier.set_paused(false);
    assert!(!notifier.is_paused());
    assert_eq!(
      notifier.notify("hiveme", &error_from_elsewhere("Disk full")).as_deref(),
      Some("error")
    );
    assert_eq!(
      recorder
        .shown
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len(),
      1
    );
  }

  #[test]
  fn rules_match_the_payload_level_on_the_same_topic() {
    let notifier = Notifier::new(&Config::default(), Arc::new(Recorder::default()));
    for level in [Level::Info, Level::Warn, Level::Error] {
      let engine = notifier.engine.read().unwrap_or_else(|poisoned| poisoned.into_inner());
      assert_eq!(engine.matching_enabled("hiveme", &level).unwrap().level, level);
    }
  }

  #[test]
  fn reloading_picks_up_rules_the_user_saved() {
    let notifier = Notifier::new(&Config::default(), Arc::new(Recorder::default()));
    let mut config = Config::default();
    config.notifications.rules.clear();

    notifier.reload(&config);

    assert!(
      notifier
        .engine
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .matching_enabled("hiveme", &Level::Error)
        .is_none()
    );
  }

  #[test]
  fn a_toast_the_os_refuses_is_dropped_without_claiming_it_fired() {
    let recorder = Arc::new(Recorder::default());
    recorder.fail.store(true, Ordering::Relaxed);
    let notifier = Notifier::new(&Config::default(), recorder);

    assert_eq!(notifier.notify("hiveme", &error_from_elsewhere("Disk full")), None);
  }
  #[test]
  fn rate_limit_summaries_follow_the_saved_language_and_survive_poison() {
    let recorder = Arc::new(Recorder::default());
    let mut config = Config::default();
    config.gui.language = "de".to_owned();
    let notifier = Arc::new(Notifier::new(&config, recorder.clone()));
    let past = Instant::now() - std::time::Duration::from_secs(2);
    {
      let mut limiter = notifier.limiter.lock().unwrap();
      limiter.admit("error", past);
      limiter.admit("error", past);
    }
    let poisoned = notifier.clone();
    let _ = std::thread::spawn(move || {
      let _guard = poisoned.engine.write().unwrap();
      panic!("test poison");
    })
    .join();
    assert!(notifier.notify("hiveme", &error_from_elsewhere("Disk full")).is_some());
    assert!(recorder.shown()[0].1.ends_with("und 1 weitere Nachricht"));
    config.gui.language = "ja".to_owned();
    notifier.reload(&config);
    assert_eq!(*notifier.locale.read().unwrap(), crate::i18n::Locale::Ja);
  }
}
