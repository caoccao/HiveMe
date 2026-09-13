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

//! The Settings tab, `Config.tsx` in cells: the category list on the left and the panel
//! of the selected category on the right.
//!
//! Every panel is a [`Form`] built from the config on every frame, which is also where
//! the focus order and the kind of the focused control come from. An edit changes the
//! application's config at once, so a language or a theme applies to the whole screen
//! in the same frame, and the config is saved 500 ms after the last edit.

mod advanced;
mod appearance;
mod broker;
mod form;
mod history;
mod notifications;
mod topics;
mod update;

use std::collections::HashMap;
use std::time::Instant;

use hiveme_core::config::{BrokerUrlParts, Config, DEFAULT_SCHEME, Scheme};
use hiveme_core::i18n::t;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Scrollbar, ScrollbarOrientation, ScrollbarState};
use unicode_width::UnicodeWidthStr;

pub use form::{Form, Item, Kind, Row, Size};

use super::app::App;
use super::keys::Action;
use super::service::Service;
use super::widgets::{TextInput, truncate};

/// The widest the panel grows, in columns.
const PANEL_MAX_WIDTH: u16 = 96;

/// How many rows one wheel notch scrolls the panel.
const WHEEL_ROWS: u16 = 3;

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

/// A control of a panel. A row of a table carries its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
  Mode,
  Theme,
  Language,
  Protocol,
  Url,
  Username,
  Password,
  CopyCliSetup,
  ClientIdPrefix,
  KeepAlive,
  SessionExpiry,
  ConnectTimeout,
  InitialDelay,
  MaxDelay,
  AddSubscription,
  SubscriptionFilter(usize),
  SubscriptionAbsolute(usize),
  RemoveSubscription(usize),
  NotificationsEnabled,
  NotifyOwnMessages,
  AddRule,
  RuleId(usize),
  RuleTopic(usize),
  RuleLevel(usize),
  RuleEnabled(usize),
  RuleTitle(usize),
  RuleBody(usize),
  RemoveRule(usize),
  MaxMessagesPerTopic,
  RetentionDays,
  CheckInterval,
}

impl Field {
  /// The category whose panel holds the field.
  pub fn category(self) -> Category {
    match self {
      Self::Mode | Self::Theme | Self::Language => Category::Appearance,
      Self::Protocol
      | Self::Url
      | Self::Username
      | Self::Password
      | Self::CopyCliSetup
      | Self::ClientIdPrefix
      | Self::KeepAlive
      | Self::SessionExpiry
      | Self::ConnectTimeout
      | Self::InitialDelay
      | Self::MaxDelay => Category::Broker,
      Self::AddSubscription
      | Self::SubscriptionFilter(_)
      | Self::SubscriptionAbsolute(_)
      | Self::RemoveSubscription(_) => Category::Topics,
      Self::NotificationsEnabled
      | Self::NotifyOwnMessages
      | Self::AddRule
      | Self::RuleId(_)
      | Self::RuleTopic(_)
      | Self::RuleLevel(_)
      | Self::RuleEnabled(_)
      | Self::RuleTitle(_)
      | Self::RuleBody(_)
      | Self::RemoveRule(_) => Category::Notifications,
      Self::MaxMessagesPerTopic | Self::RetentionDays => Category::History,
      Self::CheckInterval => Category::Update,
    }
  }
}

/// Where the focus is inside the Settings tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
  Categories,
  Field(Field),
}

/// A text field, and the config value it was last filled from or wrote.
///
/// A field is filled again only when the config holds something else than that value, so
/// a number field keeps text that is not a number yet, and a field keeps its cursor while
/// the saved config comes back.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Synced {
  input: TextInput,
  value: String,
}

/// The state of the Settings tab, kept while the tab is closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
  pub category: Category,
  pub focus: Focus,
  /// The highlighted choice of the focused select's open popup.
  pub popup: Option<usize>,
  /// How many rows of the panel are scrolled above the top.
  pub scroll: u16,
  /// The focus moved, so the next frame scrolls it into view.
  reveal: bool,
  /// The protocol of the Broker URL. It is the scheme of the one URL the config keeps,
  /// held here so that a protocol chosen before a URL is typed stays chosen.
  pub protocol: Scheme,
  pub show_password: bool,
  inputs: HashMap<Field, Synced>,
  /// The rows the panel shows and the rows it has, on the last frame.
  viewport: (u16, u16),
}

impl SettingsState {
  /// Appearance opens first, as in `hmg`.
  pub fn new(config: &Config) -> Self {
    Self {
      category: Category::Appearance,
      focus: Focus::Categories,
      popup: None,
      scroll: 0,
      reveal: false,
      protocol: BrokerUrlParts::split(&config.broker.url, DEFAULT_SCHEME).scheme,
      show_password: false,
      inputs: HashMap::new(),
      viewport: (0, 0),
    }
  }

  /// Opens on the Broker URL, which is where a first run starts.
  pub fn open_broker(&mut self) {
    self.select_category(Category::Broker);
    self.focus_field(Field::Url);
  }

  /// The text of a text field, once the panel has filled it.
  pub fn input(&self, field: Field) -> Option<&TextInput> {
    self.inputs.get(&field).map(|synced| &synced.input)
  }

  fn select_category(&mut self, category: Category) {
    if self.category != category {
      self.scroll = 0;
    }
    self.category = category;
    self.focus = Focus::Categories;
    self.popup = None;
  }

  fn focus_field(&mut self, field: Field) {
    self.focus = Focus::Field(field);
    self.reveal = true;
  }

  fn scroll_by(&mut self, rows: i32) {
    let (visible, total) = self.viewport;
    let most = i32::from(total.saturating_sub(visible));
    self.scroll = (i32::from(self.scroll) + rows).clamp(0, most) as u16;
  }
}

/// Whether the broker fields make a setup string `hmc --init` can apply, which is also
/// what a connection needs. `isBrokerUsable` in `Config.tsx`.
pub fn is_broker_usable(config: &Config) -> bool {
  let broker = &config.broker;
  !broker.url.trim().is_empty()
    && !broker.username.trim().is_empty()
    && (!broker.password.trim().is_empty() || broker.password_ref.is_some())
}

/// The command Copy CLI setup puts on the clipboard.
///
/// The setup JSON is one single-quoted shell argument, so an apostrophe, and the quotes
/// that look like one, are written as JSON escapes, which parsing turns back into the
/// credentials as they were.
pub fn cli_setup_command(setup: &str) -> String {
  let mut escaped = String::with_capacity(setup.len());
  for character in setup.chars() {
    match character {
      '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' => {
        escaped.push_str(&format!("\\u{:04x}", u32::from(character)));
      }
      other => escaped.push(other),
    }
  }
  format!("hmc --init '{escaped}'")
}

/// Reads a number field: a whole number that fits, or nothing, which leaves the config
/// alone as the GUI's `NumberField` does.
pub fn number<T: std::str::FromStr>(text: &str) -> Option<T> {
  let text = text.trim();
  if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
    return None;
  }
  text.parse().ok()
}

impl<S: Service> App<S> {
  /// The panel of the selected category.
  pub fn settings_form(&self) -> Form {
    match self.settings.category {
      Category::Appearance => appearance::form(self),
      Category::Broker => broker::form(self),
      Category::Topics => topics::form(self),
      Category::Notifications => notifications::form(self),
      Category::History => history::form(self),
      Category::Update => update::form(self),
      Category::Advanced => advanced::form(self),
    }
  }

  /// Whether a text field of the panel has the focus, so printable keys are text.
  pub fn settings_typing(&self) -> bool {
    match self.settings.focus {
      Focus::Field(field) => {
        self.settings.popup.is_none() && matches!(self.settings_form().kind(field), Some(Kind::Input { .. }))
      }
      Focus::Categories => false,
    }
  }

  /// Fills the text fields of the panel from the config where it holds something new.
  fn sync_settings_inputs(&mut self) {
    for field in self.settings_form().inputs() {
      let value = match field {
        Field::Url => self.config.broker.url.clone(),
        _ => text_value(&self.config, field).unwrap_or_default(),
      };
      if self
        .settings
        .inputs
        .get(&field)
        .is_some_and(|synced| synced.value == value)
      {
        continue;
      }
      let text = if field == Field::Url {
        let parts = BrokerUrlParts::split(&value, self.settings.protocol);
        self.settings.protocol = parts.scheme;
        parts.address
      } else {
        value.clone()
      };
      self.settings.inputs.insert(
        field,
        Synced {
          input: TextInput::new(&text),
          value,
        },
      );
    }
  }

  /// A text field changed: the config follows it.
  fn settings_text_changed(&mut self, field: Field, now: Instant) {
    let Some(text) = self.settings.input(field).map(|input| input.text().to_owned()) else {
      return;
    };
    if field == Field::Url {
      // A URL typed or pasted with a scheme moves the protocol, and the box keeps the
      // rest, as `editUrl(splitBrokerUrl(...))` does.
      let parts = BrokerUrlParts::split(&text, self.settings.protocol);
      self.settings.protocol = parts.scheme;
      let url = parts.join();
      self.edit_config(now, |config| config.broker.url = url.clone());
      if let Some(synced) = self.settings.inputs.get_mut(&field) {
        if parts.address != text {
          synced.input = TextInput::new(&parts.address);
        }
        synced.value = url;
      }
      return;
    }
    self.edit_config(now, |config| set_text(config, field, &text));
    let value = text_value(&self.config, field).unwrap_or_default();
    if let Some(synced) = self.settings.inputs.get_mut(&field) {
      synced.value = value;
    }
  }

  /// The choices of a select or a radio row, each with its style, and the chosen one.
  pub fn settings_choices(&self, field: Field) -> (Vec<(String, Style)>, usize) {
    match field.category() {
      Category::Appearance => appearance::choices(self, field),
      Category::Broker => broker::choices(self, field),
      Category::Notifications => notifications::choices(self, field),
      Category::Update => update::choices(self, field),
      _ => (Vec::new(), 0),
    }
  }

  fn settings_choose(&mut self, field: Field, index: usize, now: Instant) {
    match field.category() {
      Category::Appearance => appearance::choose(self, field, index, now),
      Category::Broker => broker::choose(self, field, index, now),
      Category::Notifications => notifications::choose(self, field, index, now),
      Category::Update => update::choose(self, field, index, now),
      _ => {}
    }
  }

  /// What `Space`, `Enter`, or a click does to a control that is not a text field.
  fn settings_activate(&mut self, field: Field, kind: &Kind, now: Instant) {
    match kind {
      Kind::Select { .. } => self.settings.popup = Some(self.settings_choices(field).1),
      Kind::Radio { .. } => {
        let (choices, selected) = self.settings_choices(field);
        if !choices.is_empty() {
          self.settings_choose(field, (selected + 1) % choices.len(), now);
        }
      }
      Kind::Check { .. } => self.edit_config(now, |config| toggle(config, field)),
      Kind::Button { enabled: true, .. } => match field.category() {
        Category::Broker => self.copy_cli_setup(now),
        Category::Topics => topics::press(self, field, now),
        Category::Notifications => notifications::press(self, field, now),
        _ => {}
      },
      Kind::Button { .. } | Kind::Input { .. } | Kind::Label { .. } => {}
    }
  }

  /// A key or a click in the Settings tab.
  pub(in crate::tui) fn perform_settings(&mut self, action: Action, now: Instant) {
    self.sync_settings_inputs();
    if let (Some(highlighted), Focus::Field(field)) = (self.settings.popup, self.settings.focus) {
      let last = self.settings_choices(field).0.len().saturating_sub(1);
      let page = usize::from(self.settings.viewport.0.max(2) - 1);
      let chosen = match &action {
        Action::Activate | Action::Toggle => Some(highlighted),
        Action::SettingsChoice(choice, index) if *choice == field => Some(*index),
        _ => None,
      };
      let highlight = match &action {
        Action::Up => Some(highlighted.saturating_sub(1)),
        Action::Down => Some((highlighted + 1).min(last)),
        Action::PageUp => Some(highlighted.saturating_sub(page)),
        Action::PageDown => Some((highlighted + page).min(last)),
        Action::Home => Some(0),
        Action::End => Some(last),
        _ => None,
      };
      if let Some(index) = chosen {
        self.settings.popup = None;
        self.settings_choose(field, index, now);
        return;
      }
      if highlight.is_some() {
        self.settings.popup = highlight;
        return;
      }
      match &action {
        // A click on the select that opened the popup only closes it.
        Action::SettingsField(clicked) if *clicked == field => {
          self.settings.popup = None;
          return;
        }
        Action::Left | Action::Right | Action::Edit(_) => return,
        // Anything else closes the popup and then does what it does.
        _ => self.settings.popup = None,
      }
    }

    let form = self.settings_form();
    let fields = form.fields();
    let position = |field: Field| fields.iter().position(|candidate| *candidate == field);
    match (self.settings.focus, action) {
      (_, Action::SettingsCategory(index)) => {
        self
          .settings
          .select_category(Category::ALL[index.min(Category::ALL.len() - 1)]);
      }
      (_, Action::SettingsField(field)) => {
        self.settings.focus_field(field);
        if let Some(kind) = form.kind(field) {
          self.settings_activate(field, kind, now);
        }
      }
      (_, Action::SettingsChoice(field, index)) => {
        self.settings.focus_field(field);
        self.settings_choose(field, index, now);
      }
      (_, Action::TogglePassword) => {
        if self.settings.category == Category::Broker {
          self.settings.show_password = !self.settings.show_password;
        }
      }
      (_, Action::PageUp) => self.settings.scroll_by(-i32::from(self.settings.viewport.0.max(2) - 1)),
      (_, Action::PageDown) => self.settings.scroll_by(i32::from(self.settings.viewport.0.max(2) - 1)),
      (Focus::Categories, Action::Up) => {
        let index = self.settings.category.index().saturating_sub(1);
        self.settings.select_category(Category::ALL[index]);
      }
      (Focus::Categories, Action::Down) => {
        let index = (self.settings.category.index() + 1).min(Category::ALL.len() - 1);
        self.settings.select_category(Category::ALL[index]);
      }
      (Focus::Categories, Action::FocusNext | Action::Activate | Action::Right) => {
        if let Some(first) = fields.first() {
          self.settings.focus_field(*first);
        }
      }
      (Focus::Categories, Action::FocusPrevious) => {
        if let Some(last) = fields.last() {
          self.settings.focus_field(*last);
        }
      }
      (Focus::Field(field), Action::FocusNext) => match position(field).and_then(|index| fields.get(index + 1)) {
        Some(next) => self.settings.focus_field(*next),
        None => self.settings.focus = Focus::Categories,
      },
      (Focus::Field(field), Action::Down) => {
        if let Some(next) = position(field).and_then(|index| fields.get(index + 1)) {
          self.settings.focus_field(*next);
        }
      }
      (Focus::Field(field), Action::FocusPrevious | Action::Up) => {
        match position(field).and_then(|index| index.checked_sub(1)) {
          Some(previous) => self.settings.focus_field(fields[previous]),
          None => self.settings.focus = Focus::Categories,
        }
      }
      (Focus::Field(field), Action::Activate | Action::Toggle) => match form.kind(field) {
        // Enter in a text field saves at once and moves on; on the last one it stays.
        Some(Kind::Input { .. }) => {
          self.flush_save(now);
          if let Some(next) = position(field).and_then(|index| fields.get(index + 1)) {
            self.settings.focus_field(*next);
          }
        }
        Some(kind) => self.settings_activate(field, kind, now),
        None => {}
      },
      (Focus::Field(field), direction @ (Action::Left | Action::Right)) => {
        if let Some(Kind::Radio { .. }) = form.kind(field) {
          let (choices, selected) = self.settings_choices(field);
          let count = choices.len().max(1);
          let index = if direction == Action::Left {
            (selected + count - 1) % count
          } else {
            (selected + 1) % count
          };
          self.settings_choose(field, index, now);
        }
      }
      (Focus::Field(field), Action::Edit(key)) => {
        if let Some(synced) = self.settings.inputs.get_mut(&field)
          && synced.input.handle(key)
        {
          self.settings_text_changed(field, now);
        }
      }
      _ => {}
    }

    // A row that was removed takes its focus with it.
    if let Focus::Field(field) = self.settings.focus
      && self.settings_form().kind(field).is_none()
    {
      self.settings.focus = Focus::Categories;
    }
  }

  /// A paste into the focused text field.
  pub(in crate::tui) fn paste_settings(&mut self, text: &str, now: Instant) {
    self.sync_settings_inputs();
    if let Focus::Field(field) = self.settings.focus
      && self.settings.popup.is_none()
      && let Some(synced) = self.settings.inputs.get_mut(&field)
      && synced.input.paste(text)
    {
      self.settings_text_changed(field, now);
    }
  }

  /// `Esc`: the popup, then back to the list. Returns whether there was somewhere to go
  /// back from.
  pub(in crate::tui) fn escape_settings(&mut self) -> bool {
    if self.settings.popup.take().is_some() {
      return true;
    }
    if matches!(self.settings.focus, Focus::Field(_)) {
      self.settings.focus = Focus::Categories;
      return true;
    }
    false
  }

  /// A wheel notch scrolls the panel.
  pub(in crate::tui) fn wheel_settings(&mut self, up: bool) {
    if self.settings.popup.is_none() {
      self.settings.scroll_by(if up { -1 } else { 1 } * i32::from(WHEEL_ROWS));
    }
  }
}

/// The config value a text field shows.
fn text_value(config: &Config, field: Field) -> Option<String> {
  match field.category() {
    Category::Broker => broker::text(config, field),
    Category::Topics => topics::text(config, field),
    Category::Notifications => notifications::text(config, field),
    Category::History => history::text(config, field),
    _ => None,
  }
}

/// Writes what a text field holds into the config.
fn set_text(config: &mut Config, field: Field, text: &str) {
  match field.category() {
    Category::Broker => broker::set_text(config, field, text),
    Category::Topics => topics::set_text(config, field, text),
    Category::Notifications => notifications::set_text(config, field, text),
    Category::History => history::set_text(config, field, text),
    _ => {}
  }
}

/// Flips a checkbox.
fn toggle(config: &mut Config, field: Field) {
  match field.category() {
    Category::Topics => topics::toggle(config, field),
    Category::Notifications => notifications::toggle(config, field),
    _ => {}
  }
}

/// Draws the Settings tab.
pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  app.sync_settings_inputs();
  let locale = app.locale;
  let theme = app.theme;
  let labels: Vec<String> = Category::ALL
    .iter()
    .map(|category| t(locale, category.label_key()))
    .collect();
  let list_width = labels.iter().map(|label| label.width()).max().unwrap_or(0) as u16 + 4;
  // The list and the panel are centered together, as the GUI centers its sidebar and
  // panel.
  let area = area.centered_horizontally(Constraint::Max(list_width + PANEL_MAX_WIDTH));
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
    let mut style = if *category == app.settings.category {
      theme.active()
    } else {
      Style::new()
    };
    if list_focused && *category == app.settings.category {
      style = style.add_modifier(Modifier::REVERSED);
    }
    frame.render_widget(Line::from(Span::styled(format!(" {label} "), style)), row);
    app.hits.push((row, Action::SettingsCategory(index)));
  }

  let title = t(locale, app.settings.category.label_key());
  let block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(if list_focused { theme.muted() } else { theme.focused() })
    .title(format!(" {title} "));
  let panel_inner = block.inner(panel);
  let inner = panel_inner.inner(Margin::new(1, 0));
  frame.render_widget(block, panel);
  if inner.width == 0 || inner.height == 0 {
    return;
  }

  let form = app.settings_form();
  let layout = form::layout(&form, inner.width, &app.glyphs);
  let state = &mut app.settings;
  state.viewport = (inner.height, layout.height);
  if state.reveal
    && let Focus::Field(field) = state.focus
    && let Some(placement) = layout.place(&form, field)
  {
    if placement.y < state.scroll {
      state.scroll = placement.y;
    } else if placement.y >= state.scroll + inner.height {
      state.scroll = placement.y + 1 - inner.height;
    }
  }
  state.reveal = false;
  state.scroll = state.scroll.min(layout.height.saturating_sub(inner.height));
  let scroll = state.scroll;
  let look = form::Look {
    area: inner,
    scroll,
    theme,
    glyphs: app.glyphs,
  };
  form::draw(frame, &form, &layout, look, &app.settings, &mut app.hits);

  if layout.height > inner.height {
    let mut scrollbar = ScrollbarState::new(usize::from(layout.height - inner.height)).position(usize::from(scroll));
    frame.render_stateful_widget(
      Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .thumb_symbol("┃")
        .style(theme.muted()),
      panel.inner(Margin::new(0, 1)),
      &mut scrollbar,
    );
  }

  // The open popup of the focused select, below it when it fits and above it otherwise.
  if let (Some(highlighted), Focus::Field(field)) = (app.settings.popup, app.settings.focus)
    && let Some(placement) = layout.place(&form, field)
    && placement.y >= scroll
    && placement.y - scroll < inner.height
  {
    let (choices, _) = app.settings_choices(field);
    let anchor = Position::new(inner.x + placement.x, inner.y + placement.y - scroll);
    render_popup(app, frame, panel_inner, anchor, field, &choices, highlighted);
  }
}

/// Draws the choices of a select as a bordered list next to `anchor`, scrolled to keep
/// the highlighted one in view.
fn render_popup<S: Service>(
  app: &mut App<S>,
  frame: &mut Frame,
  bounds: Rect,
  anchor: Position,
  field: Field,
  choices: &[(String, Style)],
  highlighted: usize,
) {
  if choices.is_empty() {
    return;
  }
  let theme = app.theme;
  let glyphs = app.glyphs;
  let width = (choices.iter().map(|(text, _)| text.width()).max().unwrap_or(0) as u16 + 4).min(bounds.width);
  let below = bounds.bottom().saturating_sub(anchor.y + 1);
  let above = anchor.y.saturating_sub(bounds.y);
  let wanted = choices.len() as u16 + 2;
  let (y, height) = if wanted <= below || below >= above {
    (anchor.y + 1, wanted.min(below))
  } else {
    let height = wanted.min(above);
    (anchor.y - height, height)
  };
  if height < 3 {
    return;
  }
  let x = anchor.x.min(bounds.right().saturating_sub(width));
  let popup = Rect::new(x, y, width, height);
  frame.render_widget(Clear, popup);
  let block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(theme.focused())
    .style(theme.base());
  let inner = block.inner(popup);
  frame.render_widget(block, popup);
  let rows = usize::from(inner.height);
  let first = highlighted
    .saturating_sub(rows.saturating_sub(1))
    .min(choices.len().saturating_sub(rows));
  for (offset, (index, (text, style))) in choices.iter().enumerate().skip(first).take(rows).enumerate() {
    let mut style = *style;
    if index == highlighted {
      style = style.add_modifier(Modifier::REVERSED);
    }
    let rect = Rect::new(inner.x, inner.y + offset as u16, inner.width, 1);
    let text = truncate(text, usize::from(inner.width.saturating_sub(2)), glyphs.ellipsis);
    let padding = usize::from(inner.width).saturating_sub(text.width() + 1);
    frame.render_widget(
      Line::from(Span::styled(format!(" {text}{}", " ".repeat(padding)), style)),
      rect,
    );
    app.hits.push((rect, Action::SettingsChoice(field, index)));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_setup_command_keeps_every_quote_inside_its_argument() {
    let quotes = ['\'', '\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}'];
    let username = format!("O{}Brien{}s", quotes[0], quotes[2]);
    let password = format!("{}{}{}\"$HOME`x`", quotes[1], quotes[3], quotes[4]);
    let setup = serde_json::json!({ "v": 1, "username": username, "password": password }).to_string();
    let command = cli_setup_command(&setup);
    let escape = |code: &str| format!("{}u{code}", '\\');
    assert!(
      command.contains(&format!("O{}Brien{}s", escape("0027"), escape("2019"))),
      "{command}"
    );
    assert!(command.starts_with("hmc --init '") && command.ends_with('\''));
    let argument = &command["hmc --init '".len()..command.len() - 1];
    assert!(!argument.contains(quotes), "{command}");
    let parsed: serde_json::Value = serde_json::from_str(argument).unwrap();
    assert_eq!(parsed["username"], username);
    assert_eq!(parsed["password"], password);
  }

  #[test]
  fn a_number_field_takes_only_a_whole_number_that_fits() {
    assert_eq!(number::<u16>(" 30 "), Some(30));
    assert_eq!(number::<u16>("70000"), None);
    assert_eq!(number::<u32>(""), None);
    assert_eq!(number::<u32>("-1"), None);
    assert_eq!(number::<u32>("1.5"), None);
    assert_eq!(number::<u64>("007"), Some(7));
  }

  #[test]
  fn the_broker_is_usable_with_a_url_a_username_and_a_password_from_somewhere() {
    let mut config = Config::default();
    assert!(!is_broker_usable(&config));
    config.broker.url = "abc123.s1.eu.hivemq.cloud:8883".to_owned();
    config.broker.username = "hiveme-sam".to_owned();
    config.broker.password = "s3cret".to_owned();
    assert!(is_broker_usable(&config));
    config.broker.password = " ".to_owned();
    assert!(!is_broker_usable(&config));
    config.broker.password_ref = Some(hiveme_core::config::SecretRef::Env {
      name: "HIVEME_PASSWORD".to_owned(),
    });
    assert!(is_broker_usable(&config));
    config.broker.url = "  ".to_owned();
    assert!(!is_broker_usable(&config));
  }
}
