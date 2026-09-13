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

//! The Broker panel: URL, username, and password.
//!
//! The URL is saved as it is typed, which the core reads as TLS MQTT when it carries
//! no scheme, the form the HiveMQ Cloud console shows. The protocol list, the port line,
//! Copy CLI setup, and the connection numbers are phase 5.

use hiveme_core::config::Config;
use hiveme_core::i18n::t;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use super::Focus;
use crate::tui::app::App;
use crate::tui::keys::Action;
use crate::tui::service::Service;
use crate::tui::widgets::TextInput;

/// A field of the panel, in focus order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerField {
  Url,
  Username,
  Password,
}

impl BrokerField {
  pub const ALL: [Self; 3] = [Self::Url, Self::Username, Self::Password];

  pub fn next(self) -> Option<Self> {
    match self {
      Self::Url => Some(Self::Username),
      Self::Username => Some(Self::Password),
      Self::Password => None,
    }
  }

  pub fn previous(self) -> Option<Self> {
    match self {
      Self::Url => None,
      Self::Username => Some(Self::Url),
      Self::Password => Some(Self::Username),
    }
  }

  fn label_key(self) -> &'static str {
    match self {
      Self::Url => "settings.url",
      Self::Username => "settings.username",
      Self::Password => "settings.password",
    }
  }
}

/// What the three fields hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerForm {
  pub url: TextInput,
  pub username: TextInput,
  pub password: TextInput,
  pub show_password: bool,
}

impl BrokerForm {
  pub fn from_config(config: &Config) -> Self {
    Self {
      url: TextInput::new(&config.broker.url),
      username: TextInput::new(&config.broker.username),
      password: TextInput::new(&config.broker.password),
      show_password: false,
    }
  }

  pub fn input_mut(&mut self, field: BrokerField) -> &mut TextInput {
    match field {
      BrokerField::Url => &mut self.url,
      BrokerField::Username => &mut self.username,
      BrokerField::Password => &mut self.password,
    }
  }

  fn input(&self, field: BrokerField) -> &TextInput {
    match field {
      BrokerField::Url => &self.url,
      BrokerField::Username => &self.username,
      BrokerField::Password => &self.password,
    }
  }

  /// Writes the fields into a config. The URL is trimmed as `joinBrokerUrl` trims it;
  /// the username and password are the user's own and kept as typed.
  pub fn apply(&self, config: &mut Config) {
    config.broker.url = self.url.text().trim().to_owned();
    config.broker.username = self.username.text().to_owned();
    config.broker.password = self.password.text().to_owned();
  }
}

/// Draws the three fields, the password hint, and the TLS note.
pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let locale = app.locale;
  let theme = app.theme;
  let area = area.inner(Margin::new(1, 0));
  let [url, username, password, hint, note] = area.layout(&Layout::vertical([
    Constraint::Length(3),
    Constraint::Length(3),
    Constraint::Length(3),
    Constraint::Length(1),
    Constraint::Fill(1),
  ]));

  for (index, (field, slot)) in BrokerField::ALL.into_iter().zip([url, username, password]).enumerate() {
    let focused = app.settings.focus == Focus::Field(field);
    let block = Block::bordered()
      .border_type(BorderType::Rounded)
      .border_style(if focused { theme.focused() } else { theme.muted() })
      .title(format!(" {} ", t(locale, field.label_key())));
    let inner = block.inner(slot).inner(Margin::new(1, 0));
    frame.render_widget(block, slot);
    let masked = field == BrokerField::Password && !app.settings.broker.show_password;
    app
      .settings
      .broker
      .input(field)
      .render(frame, inner, theme.base(), masked, focused);
    app.hits.push((slot, Action::SettingsField(index)));
  }

  let toggle = if app.settings.broker.show_password {
    "settings.hidePassword"
  } else {
    "settings.showPassword"
  };
  frame.render_widget(
    Paragraph::new(format!("Ctrl+H  {}", t(locale, toggle))).style(theme.muted()),
    hint,
  );
  frame.render_widget(
    Paragraph::new(t(locale, "settings.tlsIsAutomatic"))
      .style(theme.muted())
      .wrap(Wrap { trim: true }),
    note.inner(Margin::new(0, 1)),
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_form_reads_and_writes_the_broker_block() {
    let mut config = Config::default();
    config.broker.url = "abc123.s1.eu.hivemq.cloud:8883".to_owned();
    config.broker.username = "hiveme".to_owned();
    let mut form = BrokerForm::from_config(&config);
    assert_eq!(form.url.text(), "abc123.s1.eu.hivemq.cloud:8883");

    form.input_mut(BrokerField::Url).paste("  ");
    form.input_mut(BrokerField::Password).paste(" s3cret ");
    let mut written = Config::default();
    form.apply(&mut written);
    assert_eq!(
      written.broker.url, "abc123.s1.eu.hivemq.cloud:8883",
      "the URL is trimmed"
    );
    assert_eq!(written.broker.username, "hiveme");
    assert_eq!(written.broker.password, " s3cret ", "a password is kept as typed");
  }
}
