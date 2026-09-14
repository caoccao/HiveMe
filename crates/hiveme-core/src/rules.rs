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

//! The notification rule engine.
//!
//! Specified in `docs/specs/gui.md`. A rule maps a topic filter to an OS
//! notification; the first enabled rule matching the MQTT topic and payload level wins.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::config::{Config, Rule};
use crate::message::{Level, Parsed};

/// How long a rule waits before it may raise another notification.
pub const DEFAULT_RATE_LIMIT: Duration = Duration::from_secs(1);

/// A rule with its filter already resolved against the prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledRule {
  pub id: String,
  /// The absolute filter this rule matches against.
  pub filter: String,
  pub level: Level,
  pub enabled: bool,
  pub title: String,
  pub body: String,
}

impl CompiledRule {
  fn from_rule(rule: &Rule, prefix: &str) -> Self {
    Self {
      id: rule.id.clone(),
      filter: rule.resolve(prefix),
      level: rule.level.clone(),
      enabled: rule.enabled,
      title: rule.title.clone(),
      body: rule.body.clone(),
    }
  }
}

/// What to show the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
  pub rule_id: String,
  pub level: Level,
  pub title: String,
  pub body: String,
}

impl Notification {
  /// Appends the summary of what the rate limiter held back.
  #[must_use]
  pub fn with_suppressed(mut self, suffix: &str) -> Self {
    if !suffix.is_empty() {
      self.body = format!("{} {suffix}", self.body.trim_end());
    }
    self
  }
}

/// Decides which messages become notifications.
#[derive(Debug, Clone)]
pub struct RuleEngine {
  enabled: bool,
  notify_own_messages: bool,
  device_id: String,
  rules: Vec<CompiledRule>,
}

impl RuleEngine {
  /// Compiles the rules of `config` against its topic prefix.
  pub fn from_config(config: &Config) -> Self {
    Self {
      enabled: config.notifications.enabled,
      notify_own_messages: config.notifications.notify_own_messages,
      device_id: config.device.id.clone(),
      rules: config
        .notifications
        .rules
        .iter()
        .map(|rule| CompiledRule::from_rule(rule, crate::topic::ROOT_TOPIC))
        .collect(),
    }
  }

  /// The compiled rules, in the order they are checked.
  pub fn rules(&self) -> &[CompiledRule] {
    &self.rules
  }

  /// The first enabled rule matching both the topic filter and payload level.
  pub fn matching_enabled(&self, topic: &str, level: &Level) -> Option<&CompiledRule> {
    self
      .rules
      .iter()
      .find(|rule| rule.enabled && &rule.level == level && crate::topic::matches(&rule.filter, topic))
  }

  /// The notification a message raises, if any.
  pub fn evaluate(&self, topic: &str, parsed: &Parsed) -> Option<Notification> {
    if !self.enabled {
      return None;
    }
    if !self.notify_own_messages && self.is_own(parsed) {
      return None;
    }
    let level = parsed
      .envelope()
      .map(|message| message.level().displayed())
      .unwrap_or_default();
    let rule = self.matching_enabled(topic, &level)?;
    let context = Context::new(topic, parsed, &level);
    Some(Notification {
      rule_id: rule.id.clone(),
      level,
      title: render(&rule.title, &context),
      body: render(&rule.body, &context),
    })
  }

  /// Whether this installation produced the message.
  fn is_own(&self, parsed: &Parsed) -> bool {
    if self.device_id.is_empty() {
      return false;
    }
    parsed
      .envelope()
      .and_then(|message| message.sender.as_ref())
      .and_then(|sender| sender.id.as_deref())
      .is_some_and(|id| id == self.device_id)
  }
}

/// The values a rule template can refer to.
#[derive(Debug, Clone, Default)]
struct Context {
  title: String,
  body: String,
  topic: String,
  level: String,
  sender: String,
  app: String,
}

impl Context {
  fn new(topic: &str, parsed: &Parsed, level: &Level) -> Self {
    let envelope = parsed.envelope();
    Self {
      title: envelope
        .and_then(|message| message.payload.as_ref())
        .and_then(|payload| payload.title.clone())
        .unwrap_or_default(),
      body: parsed.notification_body(),
      topic: topic.to_owned(),
      level: level.to_string(),
      sender: envelope
        .and_then(|message| message.sender.as_ref())
        .map(|sender| sender.label().to_owned())
        .unwrap_or_default(),
      app: envelope
        .and_then(|message| message.sender.as_ref())
        .and_then(|sender| sender.app.clone())
        .unwrap_or_default(),
    }
  }

  fn lookup(&self, name: &str) -> Option<&str> {
    match name {
      "title" => Some(&self.title),
      "body" => Some(&self.body),
      "topic" => Some(&self.topic),
      "level" => Some(&self.level),
      "sender" => Some(&self.sender),
      "app" => Some(&self.app),
      _ => None,
    }
  }
}

/// Fills a rule template.
///
/// `{name}` inserts a value, `{a|b}` inserts the first of them that is not empty, and
/// a name this build does not know inserts nothing. There are no expressions.
fn render(template: &str, context: &Context) -> String {
  let mut out = String::with_capacity(template.len());
  let mut rest = template;
  while let Some(start) = rest.find('{') {
    out.push_str(&rest[..start]);
    let after = &rest[start + 1..];
    let Some(end) = after.find('}') else {
      // An unclosed brace is literal text, not a broken template.
      out.push_str(&rest[start..]);
      return out;
    };
    let placeholder = &after[..end];
    let value = placeholder
      .split('|')
      .map(str::trim)
      .filter_map(|name| context.lookup(name))
      .find(|value| !value.is_empty())
      .unwrap_or_default();
    out.push_str(value);
    rest = &after[end + 1..];
  }
  out.push_str(rest);
  out
}

/// What the rate limiter decided about one notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
  /// Show it, summarizing this many that were held back since the last one.
  Show { suppressed: u32 },
  /// Hold it back.
  Suppress,
}

/// Keeps one rule from flooding the desktop.
#[derive(Debug, Clone)]
pub struct NotificationLimiter {
  window: Duration,
  entries: HashMap<String, Entry>,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
  last_shown: Instant,
  suppressed: u32,
}

impl Default for NotificationLimiter {
  fn default() -> Self {
    Self::new(DEFAULT_RATE_LIMIT)
  }
}

impl NotificationLimiter {
  pub fn new(window: Duration) -> Self {
    Self {
      window,
      entries: HashMap::new(),
    }
  }

  /// Whether the rule may raise a notification now.
  ///
  /// `now` is passed in rather than read from the clock so that the behavior is
  /// testable.
  pub fn admit(&mut self, rule_id: &str, now: Instant) -> Admission {
    match self.entries.get_mut(rule_id) {
      None => {
        self.entries.insert(
          rule_id.to_owned(),
          Entry {
            last_shown: now,
            suppressed: 0,
          },
        );
        Admission::Show { suppressed: 0 }
      }
      Some(entry) => {
        if now.duration_since(entry.last_shown) >= self.window {
          let suppressed = entry.suppressed;
          entry.last_shown = now;
          entry.suppressed = 0;
          Admission::Show { suppressed }
        } else {
          entry.suppressed = entry.suppressed.saturating_add(1);
          Admission::Suppress
        }
      }
    }
  }

  /// Forgets what it knows, for example when the user unpauses notifications.
  pub fn reset(&mut self) {
    self.entries.clear();
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::Config;
  use crate::message::{Message, Sender};

  fn config() -> Config {
    let mut config = Config::default();
    config.device.id = "device-1".to_owned();
    config
  }

  fn envelope(body: &str, sender_id: Option<&str>) -> Parsed {
    let sender = Sender {
      id: sender_id.map(str::to_owned),
      name: Some("laptop".to_owned()),
      app: Some("hmc".to_owned()),
      app_version: Some("0.1.0".to_owned()),
    };
    Parsed::Envelope(Box::new(Message::new_text(sender, body)))
  }

  #[test]
  fn the_built_in_rules_match_the_prefixed_topics() {
    let engine = RuleEngine::from_config(&config());
    let filters: Vec<&str> = engine.rules().iter().map(|rule| rule.filter.as_str()).collect();
    assert_eq!(filters, ["hiveme/#", "hiveme/#", "hiveme/#"]);
  }

  #[test]
  fn the_first_matching_rule_wins() {
    let mut config = config();
    config.notifications.rules.insert(
      0,
      Rule {
        id: "everything".to_owned(),
        topic: "#".to_owned(),
        absolute: false,
        level: Level::Error,
        enabled: true,
        title: "{topic}".to_owned(),
        body: "{body}".to_owned(),
        matches: None,
      },
    );
    let engine = RuleEngine::from_config(&config);
    assert_eq!(
      engine.matching_enabled("hiveme", &Level::Error).unwrap().id,
      "everything"
    );
    assert_eq!(engine.matching_enabled("hiveme", &Level::Info).unwrap().id, "info");
  }

  #[test]
  fn notification_severity_comes_from_the_payload_on_every_topic() {
    let engine = RuleEngine::from_config(&config());
    for topic in ["hiveme", "hiveme/build", "hiveme/info", "hiveme/warn", "hiveme/error"] {
      for level in [Level::Info, Level::Warn, Level::Error] {
        let parsed = Parsed::Envelope(Box::new(
          Message::new_text(Sender::default(), "hello").with_level(level.clone()),
        ));
        let notification = engine.evaluate(topic, &parsed).unwrap();
        assert_eq!(notification.rule_id, level.as_str());
        assert_eq!(notification.level, level);
      }
    }
    assert!(engine.evaluate("outside", &envelope("hello", None)).is_none());
  }

  #[test]
  fn disabling_a_rule_suppresses_only_its_payload_level() {
    let mut config = config();
    config.notifications.rules[2].enabled = false;
    let engine = RuleEngine::from_config(&config);
    let error = Parsed::Envelope(Box::new(
      Message::new_text(Sender::default(), "boom").with_level(Level::Error),
    ));
    assert!(engine.evaluate("hiveme", &error).is_none());
    assert!(engine.evaluate("hiveme", &envelope("hello", None)).is_some());
  }

  #[test]
  fn a_disabled_system_raises_nothing() {
    let mut config = config();
    config.notifications.enabled = false;
    let engine = RuleEngine::from_config(&config);
    assert!(engine.evaluate("hiveme/error", &envelope("boom", None)).is_none());
  }

  #[test]
  fn a_message_from_this_device_is_skipped_by_default() {
    let engine = RuleEngine::from_config(&config());
    assert!(
      engine
        .evaluate("hiveme/info", &envelope("mine", Some("device-1")))
        .is_none()
    );
    assert!(
      engine
        .evaluate("hiveme/info", &envelope("theirs", Some("device-2")))
        .is_some()
    );
  }

  #[test]
  fn own_messages_can_be_notified_on_request() {
    let mut config = config();
    config.notifications.notify_own_messages = true;
    let engine = RuleEngine::from_config(&config);
    assert!(
      engine
        .evaluate("hiveme/info", &envelope("mine", Some("device-1")))
        .is_some()
    );
  }

  #[test]
  fn the_default_templates_fall_back_from_title_to_topic() {
    let engine = RuleEngine::from_config(&config());
    let notification = engine.evaluate("hiveme/info", &envelope("hello", None)).unwrap();
    assert_eq!(notification.title, "hiveme/info");
    assert_eq!(notification.body, "hello");
    assert_eq!(notification.level, Level::Info);
    assert_eq!(notification.rule_id, "info");
  }

  #[test]
  fn a_title_beats_the_topic_in_the_fallback() {
    let engine = RuleEngine::from_config(&config());
    let parsed = match envelope("hello", None) {
      Parsed::Envelope(message) => Parsed::Envelope(Box::new(message.with_title("Build finished"))),
      other => other,
    };
    let notification = engine.evaluate("hiveme/info", &parsed).unwrap();
    assert_eq!(notification.title, "Build finished");
  }

  #[test]
  fn templates_render_every_placeholder() {
    let context = Context {
      title: "T".to_owned(),
      body: "B".to_owned(),
      topic: "hiveme/info".to_owned(),
      level: "info".to_owned(),
      sender: "laptop".to_owned(),
      app: "hmc".to_owned(),
    };
    assert_eq!(
      render("{title} {body} {topic} {level} {sender} {app}", &context),
      "T B hiveme/info info laptop hmc"
    );
  }

  #[test]
  fn an_unknown_placeholder_renders_as_nothing() {
    let context = Context::default();
    assert_eq!(render("[{nope}]", &context), "[]");
  }

  #[test]
  fn the_fallback_form_takes_the_first_non_empty_value() {
    let mut context = Context {
      topic: "hiveme/info".to_owned(),
      ..Context::default()
    };
    assert_eq!(render("{title|topic}", &context), "hiveme/info");
    context.title = "Title".to_owned();
    assert_eq!(render("{title|topic}", &context), "Title");
    assert_eq!(render("{nope|missing}", &Context::default()), "");
  }

  #[test]
  fn an_unclosed_brace_is_literal_text() {
    assert_eq!(render("a {body", &Context::default()), "a {body");
  }

  #[test]
  fn the_limiter_allows_one_notification_per_window() {
    let mut limiter = NotificationLimiter::new(Duration::from_secs(1));
    let start = Instant::now();
    assert_eq!(limiter.admit("error", start), Admission::Show { suppressed: 0 });
    assert_eq!(
      limiter.admit("error", start + Duration::from_millis(100)),
      Admission::Suppress
    );
    assert_eq!(
      limiter.admit("error", start + Duration::from_millis(200)),
      Admission::Suppress
    );
    assert_eq!(
      limiter.admit("error", start + Duration::from_millis(1_100)),
      Admission::Show { suppressed: 2 }
    );
    assert_eq!(
      limiter.admit("error", start + Duration::from_millis(2_200)),
      Admission::Show { suppressed: 0 }
    );
  }

  #[test]
  fn the_limiter_counts_each_rule_separately() {
    let mut limiter = NotificationLimiter::new(Duration::from_secs(1));
    let start = Instant::now();
    assert_eq!(limiter.admit("info", start), Admission::Show { suppressed: 0 });
    assert_eq!(limiter.admit("error", start), Admission::Show { suppressed: 0 });
  }

  #[test]
  fn the_suppressed_summary_reads_naturally() {
    let notification = Notification {
      rule_id: "error".to_owned(),
      level: Level::Error,
      title: "t".to_owned(),
      body: "boom".to_owned(),
    };
    assert_eq!(notification.clone().with_suppressed("").body, "boom");
    assert_eq!(
      notification.clone().with_suppressed("and 1 more message").body,
      "boom and 1 more message"
    );
    assert_eq!(
      notification.with_suppressed("and 4 more messages").body,
      "boom and 4 more messages"
    );
  }
}
