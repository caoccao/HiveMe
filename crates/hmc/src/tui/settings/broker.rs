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

//! Broker: the protocol and the URL, the credentials, Copy CLI setup, and the
//! Connection and Reconnect groups.
//!
//! The protocol is a list and the URL box holds the rest of the URL exactly as it was
//! pasted, split and joined by `hiveme_core::config::BrokerUrlParts` as `hmg` splits and
//! joins it, so a URL saved by either application reads the same in the other.

use std::time::Instant;

use hiveme_core::config::{BrokerUrlParts, Config, Scheme};
use hiveme_core::i18n::{t, t_with};
use ratatui::style::Style;

use super::history::NUMBER_WIDTH;
use super::{Field, Form, Item, Row, Size, is_broker_usable, number};
use crate::tui::app::App;
use crate::tui::service::Service;

/// How wide the client id prefix is.
const PREFIX_WIDTH: u16 = 24;

/// The URL as the panel holds it: the selected protocol and the text of the box.
fn parts<S: Service>(app: &App<S>) -> BrokerUrlParts {
  match app.settings.input(Field::Url) {
    Some(input) => BrokerUrlParts {
      scheme: app.settings.protocol,
      address: input.text().to_owned(),
    },
    None => BrokerUrlParts::split(&app.config.broker.url, app.settings.protocol),
  }
}

pub fn form<S: Service>(app: &App<S>) -> Form {
  let locale = app.locale;
  let (protocols, protocol) = choices(app, Field::Protocol);
  let url = parts(app);
  let url_hint = if url.address.trim().is_empty() {
    t(locale, "settings.urlHint")
  } else {
    t_with(
      locale,
      "settings.urlConnectsTo",
      &[("url", &url.join()), ("port", &url.effective_port().to_string())],
    )
  };
  let password_hint = if app.settings.show_password {
    "settings.hidePassword"
  } else {
    "settings.showPassword"
  };
  let number_line = |key: &str, field: Field| {
    Row::line(
      t(locale, key),
      vec![Item::input(field, false, Size::Cells(NUMBER_WIDTH))],
    )
  };
  Form {
    rows: vec![
      Row::line(
        t(locale, "settings.protocol"),
        vec![Item::select(
          Field::Protocol,
          protocols[protocol].0.clone(),
          Style::new(),
          Size::Fit,
        )],
      ),
      Row::line(
        t(locale, "settings.url"),
        vec![Item::input(Field::Url, false, Size::Fill(1))],
      ),
      Row::hint(url_hint),
      Row::line(
        t(locale, "settings.username"),
        vec![Item::input(Field::Username, false, Size::Fill(1))],
      ),
      Row::line(
        t(locale, "settings.password"),
        vec![Item::input(Field::Password, !app.settings.show_password, Size::Fill(1))],
      ),
      Row::hint(format!("Ctrl+H  {}", t(locale, password_hint))),
      Row::note(t(locale, "settings.tlsIsAutomatic")),
      Row::controls(vec![Item::button(
        Field::CopyCliSetup,
        t(locale, "settings.copyCliSetup"),
        is_broker_usable(&app.config),
        Size::Fit,
      )]),
      Row::note(t(locale, "settings.copyCliSetupHint")),
      Row::Blank,
      Row::heading(t(locale, "settings.connection")),
      Row::line(
        t(locale, "settings.clientIdPrefix"),
        vec![Item::input(Field::ClientIdPrefix, false, Size::Cells(PREFIX_WIDTH))],
      ),
      number_line("settings.keepAliveSecs", Field::KeepAlive),
      number_line("settings.sessionExpirySecs", Field::SessionExpiry),
      number_line("settings.connectTimeoutSecs", Field::ConnectTimeout),
      Row::Blank,
      Row::heading(t(locale, "settings.reconnect")),
      number_line("settings.initialDelayMs", Field::InitialDelay),
      number_line("settings.maxDelayMs", Field::MaxDelay),
    ],
  }
}

pub fn text(config: &Config, field: Field) -> Option<String> {
  let broker = &config.broker;
  Some(match field {
    Field::Username => broker.username.clone(),
    Field::Password => broker.password.clone(),
    Field::ClientIdPrefix => broker.client_id_prefix.clone(),
    Field::KeepAlive => broker.keep_alive_secs.to_string(),
    Field::SessionExpiry => broker.session_expiry_secs.to_string(),
    Field::ConnectTimeout => broker.connect_timeout_secs.to_string(),
    Field::InitialDelay => broker.reconnect.initial_delay_ms.to_string(),
    Field::MaxDelay => broker.reconnect.max_delay_ms.to_string(),
    _ => return None,
  })
}

/// Writes a field. A number field that does not hold a number leaves the config alone.
pub fn set_text(config: &mut Config, field: Field, text: &str) {
  let broker = &mut config.broker;
  match field {
    Field::Username => broker.username = text.to_owned(),
    Field::Password => broker.password = text.to_owned(),
    Field::ClientIdPrefix => broker.client_id_prefix = text.to_owned(),
    Field::KeepAlive => broker.keep_alive_secs = number(text).unwrap_or(broker.keep_alive_secs),
    Field::SessionExpiry => broker.session_expiry_secs = number(text).unwrap_or(broker.session_expiry_secs),
    Field::ConnectTimeout => broker.connect_timeout_secs = number(text).unwrap_or(broker.connect_timeout_secs),
    Field::InitialDelay => {
      broker.reconnect.initial_delay_ms = number(text).unwrap_or(broker.reconnect.initial_delay_ms);
    }
    Field::MaxDelay => broker.reconnect.max_delay_ms = number(text).unwrap_or(broker.reconnect.max_delay_ms),
    _ => {}
  }
}

pub fn choices<S: Service>(app: &App<S>, field: Field) -> (Vec<(String, Style)>, usize) {
  if field != Field::Protocol {
    return (Vec::new(), 0);
  }
  (
    Scheme::ALL
      .iter()
      .map(|scheme| {
        (
          t(app.locale, &format!("settings.protocols.{}", scheme.as_str())),
          Style::new(),
        )
      })
      .collect(),
    Scheme::ALL
      .iter()
      .position(|scheme| *scheme == app.settings.protocol)
      .unwrap_or(0),
  )
}

/// A protocol from the list: the URL is written again with it in front.
pub fn choose<S: Service>(app: &mut App<S>, field: Field, index: usize, now: Instant) {
  if field != Field::Protocol {
    return;
  }
  let mut url = parts(app);
  url.scheme = Scheme::ALL[index.min(Scheme::ALL.len() - 1)];
  app.settings.protocol = url.scheme;
  let joined = url.join();
  app.edit_config(now, |config| config.broker.url = joined.clone());
  if let Some(synced) = app.settings.inputs.get_mut(&Field::Url) {
    synced.value = joined;
  }
}
