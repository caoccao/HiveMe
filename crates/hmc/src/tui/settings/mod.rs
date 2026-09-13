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

//! The Settings tab: the category list on the left and a panel on the right.
//!
//! Phase 3 builds the list and the Broker fields a first run needs, so a fresh install
//! can be finished without leaving the terminal UI. The other panels, and the rest of
//! Broker, are phase 5 of the terminal UI plan.

mod broker;

pub use broker::{BrokerField, BrokerForm};

use hiveme_core::config::Config;
use hiveme_core::i18n::t;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use super::app::App;
use super::keys::Action;
use super::service::Service;

/// The widest the panel grows, in columns.
const PANEL_MAX_WIDTH: u16 = 96;

/// A category of the list, in the order `hmg` shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
  Appearance,
  Broker,
  Topics,
  Notifications,
  History,
  Update,
  Advanced,
}

impl Category {
  pub const ALL: [Self; 7] = [
    Self::Appearance,
    Self::Broker,
    Self::Topics,
    Self::Notifications,
    Self::History,
    Self::Update,
    Self::Advanced,
  ];

  /// The catalog key of the category's name.
  pub fn label_key(self) -> &'static str {
    match self {
      Self::Appearance => "settings.appearance",
      Self::Broker => "settings.broker",
      Self::Topics => "settings.topics",
      Self::Notifications => "settings.notifications",
      Self::History => "settings.history",
      Self::Update => "settings.update",
      Self::Advanced => "settings.advanced",
    }
  }

  fn index(self) -> usize {
    Self::ALL.iter().position(|category| *category == self).unwrap_or(0)
  }
}

/// Where the focus is inside the Settings tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
  Categories,
  Field(BrokerField),
}

/// The state of the Settings tab, kept while the tab is closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
  pub category: Category,
  pub focus: Focus,
  pub broker: BrokerForm,
}

impl SettingsState {
  /// Appearance opens first, as in `hmg`.
  pub fn new(config: &Config) -> Self {
    Self {
      category: Category::Appearance,
      focus: Focus::Categories,
      broker: BrokerForm::from_config(config),
    }
  }

  /// Opens on the Broker URL, which is where a first run starts.
  pub fn open_broker(&mut self) {
    self.category = Category::Broker;
    self.focus = Focus::Field(BrokerField::Url);
  }

  /// Whether a text field has the focus.
  pub fn is_typing(&self) -> bool {
    matches!(self.focus, Focus::Field(_))
  }

  /// Whether the focus is on the last field, where `Enter` saves at once.
  pub fn is_last_field(&self) -> bool {
    self.focus == Focus::Field(BrokerField::Password)
  }

  /// Goes back to the list. Returns whether there was somewhere to go back from.
  pub fn escape(&mut self) -> bool {
    if self.is_typing() {
      self.focus = Focus::Categories;
      true
    } else {
      false
    }
  }

  /// Pastes into the focused field. Returns whether a field changed.
  pub fn paste(&mut self, text: &str) -> bool {
    match self.focus {
      Focus::Field(field) => self.broker.input_mut(field).paste(text),
      Focus::Categories => false,
    }
  }

  /// Applies a key or a click. Returns whether a broker field changed.
  pub fn perform(&mut self, action: &Action) -> bool {
    match (self.focus, action) {
      (_, Action::SettingsCategory(index)) => {
        self.category = Category::ALL[(*index).min(Category::ALL.len() - 1)];
        self.focus = Focus::Categories;
      }
      (_, Action::SettingsField(index)) => {
        if self.category == Category::Broker {
          self.focus = Focus::Field(BrokerField::ALL[(*index).min(BrokerField::ALL.len() - 1)]);
        }
      }
      (Focus::Categories, Action::Up) => self.category = Category::ALL[self.category.index().saturating_sub(1)],
      (Focus::Categories, Action::Down) => {
        self.category = Category::ALL[(self.category.index() + 1).min(Category::ALL.len() - 1)];
      }
      (Focus::Categories, Action::FocusNext | Action::Activate) => {
        if self.category == Category::Broker {
          self.focus = Focus::Field(BrokerField::Url);
        }
      }
      (Focus::Field(field), Action::FocusNext | Action::Down | Action::Activate) => {
        self.focus = match field.next() {
          Some(next) => Focus::Field(next),
          None if *action == Action::Activate => Focus::Field(field),
          None => Focus::Categories,
        };
      }
      (Focus::Field(field), Action::FocusPrevious | Action::Up) => {
        self.focus = field.previous().map_or(Focus::Categories, Focus::Field);
      }
      (Focus::Field(_), Action::TogglePassword) => self.broker.show_password = !self.broker.show_password,
      (Focus::Field(field), Action::Edit(key)) => return self.broker.input_mut(field).handle(*key),
      _ => {}
    }
    false
  }
}

/// Draws the Settings tab.
pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let locale = app.locale;
  let theme = app.theme;
  let labels: Vec<String> = Category::ALL
    .iter()
    .map(|category| t(locale, category.label_key()))
    .collect();
  let list_width = labels.iter().map(|label| label.width()).max().unwrap_or(0) as u16 + 4;
  let [list, panel] = area.layout(&Layout::horizontal([
    Constraint::Length(list_width),
    Constraint::Fill(1),
  ]));

  let list_focused = app.settings.focus == Focus::Categories;
  let block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(if list_focused { theme.focused() } else { theme.muted() });
  let inner = block.inner(list);
  frame.render_widget(block, list);
  for (index, (category, label)) in Category::ALL.iter().zip(labels).enumerate() {
    let row = Rect::new(inner.x, inner.y + index as u16, inner.width, 1);
    if row.y >= inner.bottom() {
      break;
    }
    let style = if *category == app.settings.category {
      theme.active()
    } else {
      Style::new()
    };
    frame.render_widget(Line::from(Span::styled(format!(" {label}"), style)), row);
    app.hits.push((row, Action::SettingsCategory(index)));
  }

  let panel = panel.centered_horizontally(Constraint::Max(PANEL_MAX_WIDTH));
  let title = t(locale, app.settings.category.label_key());
  let block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(theme.muted())
    .title(format!(" {title} "));
  let inner = block.inner(panel);
  frame.render_widget(block, panel);
  match app.settings.category {
    Category::Broker => broker::render(app, frame, inner),
    _ => frame.render_widget(
      Paragraph::new(t(locale, "tui.settingsLater"))
        .style(theme.muted())
        .wrap(Wrap { trim: true }),
      inner.inner(ratatui::layout::Margin::new(1, 0)),
    ),
  }
}

#[cfg(test)]
mod tests {
  use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

  use super::*;

  #[test]
  fn the_list_moves_between_categories_and_enters_only_broker() {
    let mut state = SettingsState::new(&Config::default());
    assert_eq!(state.category, Category::Appearance);
    state.perform(&Action::FocusNext);
    assert_eq!(state.focus, Focus::Categories, "Appearance has no fields yet");

    state.perform(&Action::Down);
    assert_eq!(state.category, Category::Broker);
    state.perform(&Action::FocusNext);
    assert_eq!(state.focus, Focus::Field(BrokerField::Url));
    assert!(state.is_typing());
    assert!(state.escape());
    assert!(!state.escape(), "nothing to go back from on the list");

    state.perform(&Action::Up);
    state.perform(&Action::Up);
    assert_eq!(state.category, Category::Appearance, "the list stops at the top");
    state.perform(&Action::SettingsCategory(6));
    state.perform(&Action::Down);
    assert_eq!(state.category, Category::Advanced, "and at the bottom");
  }

  #[test]
  fn the_broker_fields_are_one_ring_that_returns_to_the_list() {
    let mut state = SettingsState::new(&Config::default());
    state.open_broker();
    state.perform(&Action::FocusNext);
    assert_eq!(state.focus, Focus::Field(BrokerField::Username));
    state.perform(&Action::Activate);
    assert!(state.is_last_field());
    state.perform(&Action::Activate);
    assert!(state.is_last_field(), "Enter on the last field stays there");
    state.perform(&Action::FocusNext);
    assert_eq!(state.focus, Focus::Categories);
    state.perform(&Action::SettingsField(2));
    state.perform(&Action::FocusPrevious);
    state.perform(&Action::Up);
    assert_eq!(state.focus, Focus::Field(BrokerField::Url));
    state.perform(&Action::FocusPrevious);
    assert_eq!(state.focus, Focus::Categories);
  }

  #[test]
  fn typing_and_pasting_change_the_focused_field() {
    let mut state = SettingsState::new(&Config::default());
    state.open_broker();
    assert!(state.perform(&Action::Edit(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))));
    assert!(state.paste("ost:8883\n"));
    assert_eq!(state.broker.url.text(), "host:8883");
    assert!(!state.perform(&Action::Edit(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))));

    assert!(!state.broker.show_password);
    state.perform(&Action::TogglePassword);
    assert!(state.broker.show_password);

    state.escape();
    assert!(!state.paste("ignored"), "the list takes no paste");
  }
}
