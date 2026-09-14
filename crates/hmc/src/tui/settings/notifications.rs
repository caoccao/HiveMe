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

//! Notifications: the two switches and the rules table with Add and Remove.

use std::time::Instant;

use hiveme_core::config::{Config, DEFAULT_RULE_BODY, DEFAULT_RULE_TITLE, Rule};
use hiveme_core::i18n::t;
use hiveme_core::message::Level;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::{Field, Form, Item, Row, Size};
use crate::tui::app::App;
use crate::tui::service::Service;

/// The levels a rule matches, in the order of `LEVELS` in `src/lib/protocol.ts`.
const LEVELS: [Level; 5] = [Level::Debug, Level::Info, Level::Success, Level::Warn, Level::Error];

/// The widest the Enabled column grows for its header.
const ENABLED_MAX_WIDTH: u16 = 10;

/// The name a level is shown with: translated when it is known, as written otherwise.
fn level_name<S: Service>(app: &App<S>, level: &Level) -> String {
  if level.is_known() {
    t(app.locale, &format!("levels.{}", level.as_str()))
  } else {
    level.as_str().to_owned()
  }
}

fn level_style<S: Service>(app: &App<S>, level: &Level) -> Style {
  app
    .theme
    .severity(level)
    .map_or(Style::new(), |color| Style::new().fg(color))
}

pub fn form<S: Service>(app: &App<S>) -> Form {
  let locale = app.locale;
  let glyphs = app.glyphs;
  let notifications = &app.config.notifications;
  let header_style = app.theme.muted().add_modifier(Modifier::BOLD);
  let header = |key: &str, size| Item::label(t(locale, key), header_style, size);
  let level_width = LEVELS
    .iter()
    .map(|level| level_name(app, level).width())
    .max()
    .unwrap_or(0) as u16
    + glyphs.expanded.width() as u16
    + 3;
  let enabled_width =
    (t(locale, "settings.ruleEnabled").width() as u16).clamp(glyphs.checked.width() as u16, ENABLED_MAX_WIDTH);
  let remove_width = glyphs.close.width() as u16 + 2;

  let mut rows = vec![
    Row::controls(vec![Item::check(
      Field::NotificationsEnabled,
      notifications.enabled,
      t(locale, "settings.notificationsEnabled"),
      Size::Fit,
    )]),
    Row::controls(vec![Item::check(
      Field::NotifyOwnMessages,
      notifications.notify_own_messages,
      t(locale, "settings.notifyOwnMessages"),
      Size::Fit,
    )]),
    Row::Blank,
    Row::Heading {
      title: t(locale, "settings.rules"),
      action: Some(Item::button(
        Field::AddRule,
        t(locale, "settings.addRule"),
        true,
        Size::Fit,
      )),
    },
    Row::hint(t(locale, "settings.rawNotificationHint")),
    Row::controls(vec![
      header("settings.ruleId", Size::Fill(2)),
      header("settings.ruleTopic", Size::Fill(3)),
      header("settings.ruleLevel", Size::Cells(level_width)),
      header("settings.ruleEnabled", Size::Cells(enabled_width)),
      header("settings.ruleTitle", Size::Fill(3)),
      header("settings.ruleBody", Size::Fill(3)),
      Item::label(String::new(), Style::new(), Size::Cells(remove_width)),
    ]),
  ];
  for (index, rule) in notifications.rules.iter().enumerate() {
    rows.push(Row::controls(vec![
      Item::input(Field::RuleId(index), false, Size::Fill(2)),
      Item::input(Field::RuleTopic(index), false, Size::Fill(3)),
      Item::select(
        Field::RuleLevel(index),
        level_name(app, &rule.level),
        level_style(app, &rule.level),
        Size::Cells(level_width),
      ),
      Item::check(
        Field::RuleEnabled(index),
        rule.enabled,
        String::new(),
        Size::Cells(enabled_width),
      ),
      Item::input(Field::RuleTitle(index), false, Size::Fill(3)),
      Item::input(Field::RuleBody(index), false, Size::Fill(3)),
      Item::button(
        Field::RemoveRule(index),
        glyphs.close.to_owned(),
        true,
        Size::Cells(remove_width),
      ),
    ]));
  }
  Form { rows }
}

pub fn text(config: &Config, field: Field) -> Option<String> {
  let rules = &config.notifications.rules;
  match field {
    Field::RuleId(index) => rules.get(index).map(|rule| rule.id.clone()),
    Field::RuleTopic(index) => rules.get(index).map(|rule| rule.topic.clone()),
    Field::RuleTitle(index) => rules.get(index).map(|rule| rule.title.clone()),
    Field::RuleBody(index) => rules.get(index).map(|rule| rule.body.clone()),
    _ => None,
  }
}

pub fn set_text(config: &mut Config, field: Field, text: &str) {
  let rules = &mut config.notifications.rules;
  let (index, slot): (usize, fn(&mut Rule) -> &mut String) = match field {
    Field::RuleId(index) => (index, |rule| &mut rule.id),
    Field::RuleTopic(index) => (index, |rule| &mut rule.topic),
    Field::RuleTitle(index) => (index, |rule| &mut rule.title),
    Field::RuleBody(index) => (index, |rule| &mut rule.body),
    _ => return,
  };
  if let Some(rule) = rules.get_mut(index) {
    *slot(rule) = text.to_owned();
  }
}

pub fn toggle(config: &mut Config, field: Field) {
  let notifications = &mut config.notifications;
  match field {
    Field::NotificationsEnabled => notifications.enabled = !notifications.enabled,
    Field::NotifyOwnMessages => notifications.notify_own_messages = !notifications.notify_own_messages,
    Field::RuleEnabled(index) => {
      if let Some(rule) = notifications.rules.get_mut(index) {
        rule.enabled = !rule.enabled;
      }
    }
    _ => {}
  }
}

pub fn choices<S: Service>(app: &App<S>, field: Field) -> (Vec<(String, Style)>, usize) {
  let Field::RuleLevel(index) = field else {
    return (Vec::new(), 0);
  };
  let current = app.config.notifications.rules.get(index).map(|rule| &rule.level);
  (
    LEVELS
      .iter()
      .map(|level| (level_name(app, level), level_style(app, level)))
      .collect(),
    LEVELS.iter().position(|level| Some(level) == current).unwrap_or(1),
  )
}

pub fn choose<S: Service>(app: &mut App<S>, field: Field, index: usize, now: Instant) {
  if let Field::RuleLevel(row) = field {
    let level = LEVELS[index.min(LEVELS.len() - 1)].clone();
    app.edit_config(now, |config| {
      if let Some(rule) = config.notifications.rules.get_mut(row) {
        rule.level = level;
      }
    });
  }
}

/// Add appends the rule the GUI adds and moves to its id; Remove deletes its row and
/// stays on the Remove buttons while there are rows.
pub fn press<S: Service>(app: &mut App<S>, field: Field, now: Instant) {
  let count = app.config.notifications.rules.len();
  match field {
    Field::AddRule => {
      app.edit_config(now, |config| {
        let rules = &mut config.notifications.rules;
        rules.push(Rule {
          id: (rules.len() + 1..)
            .map(|index| format!("rule-{index}"))
            .find(|id| rules.iter().all(|rule| &rule.id != id))
            .expect("finite rules leave a free id"),
          topic: "info".to_owned(),
          absolute: false,
          level: Level::Info,
          enabled: true,
          title: DEFAULT_RULE_TITLE.to_owned(),
          body: DEFAULT_RULE_BODY.to_owned(),
          matches: None,
        });
      });
      app.settings.focus_field(Field::RuleId(count));
    }
    Field::RemoveRule(index) if index < count => {
      app.edit_config(now, |config| {
        config.notifications.rules.remove(index);
      });
      app.settings.focus_field(match count - 1 {
        0 => Field::AddRule,
        left => Field::RemoveRule(index.min(left - 1)),
      });
    }
    _ => {}
  }
}
