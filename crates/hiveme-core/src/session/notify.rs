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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use crate::config::Config;
use crate::message::Parsed;
use crate::rules::{Admission, NotificationLimiter, RuleEngine};

/// Delivers the enabled notification channels for the application running the session.
pub trait Toaster: Send + Sync {
  fn show(&self, title: &str, body: &str) -> Result<(), String>;

  fn show_topmost(&self, _notification: &TopmostNotification) -> Result<(), String> {
    Err("topmost notifications are unavailable".to_owned())
  }
}

/// Content shared by the desktop notification host and its window.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopmostNotification {
  pub title: String,
  pub body: String,
  pub level: crate::message::Level,
  pub close_label: String,
}

#[derive(Clone, Copy)]
pub(super) enum Channel {
  Os,
  Topmost,
}

/// A queued message belongs to one uninterrupted period of active notifications.
#[derive(Clone, Copy)]
pub(super) struct NotificationPermit(u64);

/// The rule engine, its rate limiter, and the pause toggle.
pub struct Notifier {
  engine: RwLock<RuleEngine>,
  locale: RwLock<crate::i18n::Locale>,
  limiter: Mutex<NotificationLimiter>,
  /// Odd means paused. Each transition invalidates every older queued permit.
  pause_state: AtomicU64,
  os_enabled: AtomicBool,
  topmost_enabled: AtomicBool,
  toaster: Arc<dyn Toaster>,
}

impl Notifier {
  /// Compiles the rules of `config`.
  pub fn new(config: &Config, toaster: Arc<dyn Toaster>) -> Self {
    Self {
      engine: RwLock::new(RuleEngine::from_config(config)),
      locale: RwLock::new(crate::i18n::Locale::resolve(&config.gui.language)),
      limiter: Mutex::new(NotificationLimiter::default()),
      pause_state: AtomicU64::new(0),
      os_enabled: AtomicBool::new(config.notifications.enabled),
      topmost_enabled: AtomicBool::new(config.notifications.topmost_enabled),
      toaster,
    }
  }

  /// Compiles the rules again after the user saved the settings.
  ///
  /// The rate limiter is cleared as well, because what it remembers is about rules
  /// that may no longer exist.
  pub fn reload(&self, config: &Config) {
    self.os_enabled.store(config.notifications.enabled, Ordering::Relaxed);
    self
      .topmost_enabled
      .store(config.notifications.topmost_enabled, Ordering::Relaxed);
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
    self.pause_state.load(Ordering::Acquire) & 1 != 0
  }

  pub(super) fn permit(&self) -> Option<NotificationPermit> {
    let state = self.pause_state.load(Ordering::Acquire);
    (state & 1 == 0).then_some(NotificationPermit(state))
  }

  fn permits(&self, permit: NotificationPermit) -> bool {
    self.pause_state.load(Ordering::Acquire) == permit.0
  }

  /// Pauses or resumes for the rest of the session.
  ///
  /// Pausing invalidates queued delivery on both channels, including after resuming.
  /// Resuming also clears the OS rate limiter, so no paused backlog or summary returns.
  pub fn set_paused(&self, paused: bool) {
    let mut limiter = self.limiter.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let changed = self
      .pause_state
      .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
        ((state & 1 != 0) != paused).then(|| state.wrapping_add(1))
      });
    if !paused && changed.is_ok() {
      limiter.reset();
    }
  }

  /// Raises a notification for a message, when the rules and the limiter allow it.
  ///
  /// Returns the rule that fired, so the caller can raise the event. A failure to show
  /// is logged and dropped: a desktop without a notification daemon is a reason for the
  /// message to be silent, not for it to be lost.
  pub fn notify(&self, topic: &str, parsed: &Parsed) -> Option<String> {
    self.notify_channels(topic, parsed, true, true, self.permit()?)
  }

  pub(super) fn notify_channel(
    &self,
    topic: &str,
    parsed: &Parsed,
    channel: Channel,
    permit: NotificationPermit,
  ) -> Option<String> {
    self.notify_channels(
      topic,
      parsed,
      matches!(channel, Channel::Os),
      matches!(channel, Channel::Topmost),
      permit,
    )
  }

  fn notify_channels(
    &self,
    topic: &str,
    parsed: &Parsed,
    os: bool,
    topmost: bool,
    permit: NotificationPermit,
  ) -> Option<String> {
    if !self.permits(permit) {
      return None;
    }
    let notification = self
      .engine
      .read()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .evaluate(topic, parsed)?;
    // Updating the one window is never rate limited. In a burst it must end on the
    // newest match, even when OS banners are disabled, suppressed, or fail.
    let mut shown = false;
    if topmost && notification.topmost && self.topmost_enabled.load(Ordering::Relaxed) {
      let content = TopmostNotification {
        title: notification.title.clone(),
        body: notification.body.clone(),
        level: notification.level.clone(),
        close_label: crate::i18n::t(
          *self.locale.read().unwrap_or_else(|poisoned| poisoned.into_inner()),
          "common.close",
        ),
      };
      if !self.permits(permit) {
        return None;
      }
      match self.toaster.show_topmost(&content) {
        Ok(()) => shown = true,
        Err(error) => log::warn!("the topmost notification could not be shown: {error}"),
      }
    }
    if !os || !notification.os || !self.os_enabled.load(Ordering::Relaxed) || !self.permits(permit) {
      return shown.then_some(notification.rule_id);
    }
    let admission = {
      let mut limiter = self.limiter.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
      if !self.permits(permit) {
        return shown.then_some(notification.rule_id);
      }
      limiter.admit(&notification.rule_id, Instant::now())
    };
    let Admission::Show { suppressed } = admission else {
      log::debug!("rule {} is rate limited, holding back {topic}", notification.rule_id);
      return shown.then_some(notification.rule_id);
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

    if !self.permits(permit) {
      return shown.then_some(notification.rule_id);
    }
    if let Err(error) = self.toaster.show(&notification.title, &notification.body) {
      log::warn!("the OS would not show a notification: {error}");
      return shown.then_some(notification.rule_id);
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

  fn configured() -> Config {
    let mut config = Config::default();
    for rule in &mut config.notifications.rules {
      rule.os = true;
    }
    config
  }

  fn error_from_elsewhere(body: &str) -> Parsed {
    let sender = Sender {
      id: Some("another-device".to_owned()),
      ..Sender::default()
    };
    let message = Message::new_text(sender, body).with_level(Level::Error);
    crate::message::parse(&message.to_bytes().unwrap())
  }

  #[derive(Default)]
  struct Channels {
    os: Mutex<Vec<String>>,
    topmost: Mutex<Vec<TopmostNotification>>,
    fail_os: bool,
    fail_topmost: bool,
  }

  impl Toaster for Channels {
    fn show(&self, _: &str, body: &str) -> Result<(), String> {
      self.os.lock().unwrap().push(body.to_owned());
      if self.fail_os {
        Err("OS unavailable".to_owned())
      } else {
        Ok(())
      }
    }
    fn show_topmost(&self, notification: &TopmostNotification) -> Result<(), String> {
      self.topmost.lock().unwrap().push(notification.clone());
      if self.fail_topmost {
        Err("window unavailable".to_owned())
      } else {
        Ok(())
      }
    }
  }

  #[test]
  fn per_rule_channels_obey_both_global_switches_and_rule_enabled() {
    let recorder = Arc::new(Channels::default());
    let mut config = configured();
    config
      .notifications
      .rules
      .retain(|rule| rule.level == crate::Level::Error);
    let notifier = Notifier::new(&config, recorder.clone());
    for enabled in [false, true] {
      for os in [false, true] {
        for global_topmost in [false, true] {
          for rule_os in [false, true] {
            for rule_topmost in [false, true] {
              config.notifications.enabled = os;
              config.notifications.topmost_enabled = global_topmost;
              config.notifications.rules[0].enabled = enabled;
              config.notifications.rules[0].os = rule_os;
              config.notifications.rules[0].topmost = rule_topmost;
              notifier.reload(&config);
              recorder.os.lock().unwrap().clear();
              recorder.topmost.lock().unwrap().clear();
              let wants_os = enabled && os && rule_os;
              let wants_topmost = enabled && global_topmost && rule_topmost;
              assert_eq!(
                notifier.notify("hiveme", &error_from_elsewhere("message")).is_some(),
                wants_os || wants_topmost
              );
              assert_eq!(recorder.os.lock().unwrap().len(), usize::from(wants_os));
              assert_eq!(recorder.topmost.lock().unwrap().len(), usize::from(wants_topmost));
            }
          }
        }
      }
    }
    // A later rule cannot opt the first matching rule into either channel.
    config.notifications.enabled = true;
    config.notifications.rules[0].os = false;
    config.notifications.rules[0].topmost = false;
    let mut later = config.notifications.rules[0].clone();
    later.id = "later".into();
    later.os = true;
    later.topmost = true;
    config.notifications.rules.push(later);
    notifier.reload(&config);
    recorder.topmost.lock().unwrap().clear();
    recorder.os.lock().unwrap().clear();
    assert!(
      notifier
        .notify("hiveme", &error_from_elsewhere("first match wins"))
        .is_none()
    );
    assert!(recorder.topmost.lock().unwrap().is_empty());
    assert!(recorder.os.lock().unwrap().is_empty());
  }

  #[test]
  fn pause_discards_both_channel_permits_even_after_resuming() {
    let recorder = Arc::new(Channels::default());
    let mut config = configured();
    config.notifications.topmost_enabled = true;
    for rule in &mut config.notifications.rules {
      rule.topmost = true;
    }
    let notifier = Notifier::new(&config, recorder.clone());
    let queued = notifier.permit().unwrap();
    notifier.set_paused(true);
    assert!(notifier.permit().is_none());
    assert!(notifier.notify("hiveme", &error_from_elsewhere("paused")).is_none());
    for channel in [Channel::Os, Channel::Topmost] {
      assert!(
        notifier
          .notify_channel("hiveme", &error_from_elsewhere("queued"), channel, queued)
          .is_none()
      );
    }
    notifier.set_paused(false);
    for channel in [Channel::Os, Channel::Topmost] {
      assert!(
        notifier
          .notify_channel("hiveme", &error_from_elsewhere("queued"), channel, queued)
          .is_none()
      );
    }
    assert!(recorder.os.lock().unwrap().is_empty());
    assert!(recorder.topmost.lock().unwrap().is_empty());
    assert!(notifier.notify("hiveme", &error_from_elsewhere("fresh")).is_some());
    assert_eq!(*recorder.os.lock().unwrap(), ["fresh"]);
    assert_eq!(recorder.topmost.lock().unwrap()[0].body, "fresh");
  }

  #[test]
  fn notification_channels_are_independent_and_reload_without_reconnecting() {
    let recorder = Arc::new(Channels::default());
    let mut config = configured();
    for rule in &mut config.notifications.rules {
      rule.topmost = true;
    }
    let notifier = Notifier::new(&config, recorder.clone());
    for (os, topmost) in [(true, false), (false, true), (true, true), (false, false)] {
      config.notifications.enabled = os;
      config.notifications.topmost_enabled = topmost;
      notifier.reload(&config);
      recorder.os.lock().unwrap().clear();
      recorder.topmost.lock().unwrap().clear();
      assert_eq!(
        notifier.notify("hiveme", &error_from_elsewhere("message")).is_some(),
        os || topmost
      );
      assert_eq!(recorder.os.lock().unwrap().len(), usize::from(os));
      assert_eq!(recorder.topmost.lock().unwrap().len(), usize::from(topmost));
    }
  }

  #[test]
  fn topmost_displays_the_latest_message_even_when_os_banners_are_rate_limited() {
    let recorder = Arc::new(Channels::default());
    let mut config = configured();
    config.notifications.topmost_enabled = true;
    for rule in &mut config.notifications.rules {
      rule.topmost = true;
    }
    config.gui.language = "de".to_owned();
    let notifier = Notifier::new(&config, recorder.clone());
    for body in ["first", "second", "latest"] {
      assert!(notifier.notify("hiveme", &error_from_elsewhere(body)).is_some());
    }
    assert_eq!(recorder.os.lock().unwrap().len(), 1);
    let notifications = recorder.topmost.lock().unwrap();
    assert_eq!(notifications.len(), 3);
    assert_eq!(notifications.last().unwrap().body, "latest");
    assert_eq!(
      notifications.last().unwrap().close_label,
      crate::i18n::t(crate::i18n::Locale::De, "common.close")
    );
    drop(notifications);
    notifier.set_paused(true);
    assert!(notifier.notify("hiveme", &error_from_elsewhere("paused")).is_none());
    assert_eq!(recorder.topmost.lock().unwrap().len(), 3);
  }

  #[test]
  fn failure_of_either_channel_does_not_prevent_the_other() {
    for (fail_os, fail_topmost) in [(true, false), (false, true), (true, true)] {
      let recorder = Arc::new(Channels {
        fail_os,
        fail_topmost,
        ..Channels::default()
      });
      let mut config = configured();
      config.notifications.topmost_enabled = true;
      for rule in &mut config.notifications.rules {
        rule.topmost = true;
      }
      let notifier = Notifier::new(&config, recorder.clone());
      assert_eq!(
        notifier.notify("hiveme", &error_from_elsewhere("message")).is_some(),
        !(fail_os && fail_topmost)
      );
      assert_eq!(recorder.os.lock().unwrap().len(), 1);
      assert_eq!(recorder.topmost.lock().unwrap().len(), 1);
    }
  }

  #[test]
  fn pausing_is_the_first_thing_checked() {
    let recorder = Arc::new(Recorder::default());
    let notifier = Notifier::new(&configured(), recorder.clone());
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
    let notifier = Notifier::new(&configured(), Arc::new(Recorder::default()));
    for level in [Level::Info, Level::Warn, Level::Error] {
      let engine = notifier.engine.read().unwrap_or_else(|poisoned| poisoned.into_inner());
      assert_eq!(engine.matching_enabled("hiveme", &level).unwrap().level, level);
    }
  }

  #[test]
  fn reloading_picks_up_rules_the_user_saved() {
    let notifier = Notifier::new(&configured(), Arc::new(Recorder::default()));
    let mut config = configured();
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
    let notifier = Notifier::new(&configured(), recorder);

    assert_eq!(notifier.notify("hiveme", &error_from_elsewhere("Disk full")), None);
  }
  #[test]
  fn rate_limit_summaries_follow_the_saved_language_and_survive_poison() {
    let recorder = Arc::new(Recorder::default());
    let mut config = configured();
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
