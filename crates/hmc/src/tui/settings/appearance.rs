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

//! Appearance: the display mode as a radio row, the theme and the language as selects.
//! A change applies to the whole screen in the frame it is made.

use std::time::Instant;

use hiveme_core::config::{DisplayMode, Theme as Palette};
use hiveme_core::i18n::{Locale, t};
use ratatui::style::Style;

use super::{Field, Form, Item, Row, Size};
use crate::tui::app::App;
use crate::tui::service::Service;
use crate::tui::theme::palette;

/// The display modes in the order of the radio row.
const MODES: [DisplayMode; 3] = [DisplayMode::Auto, DisplayMode::Light, DisplayMode::Dark];

/// The name of a language in that language, `LANGUAGE_LABELS` in `src/lib/protocol.ts`,
/// so the list stays readable whatever language the screen is in.
pub fn autonym(locale: Locale) -> &'static str {
  match locale {
    Locale::De => "Deutsch",
    Locale::EnUs => "English (US)",
    Locale::Es => "Español",
    Locale::Fr => "Français",
    Locale::It => "Italiano",
    Locale::Ja => "日本語",
    Locale::ZhCn => "简体中文",
    Locale::ZhHk => "繁體中文 (香港)",
    Locale::ZhTw => "繁體中文 (臺灣)",
  }
}

pub fn form<S: Service>(app: &App<S>) -> Form {
  let locale = app.locale;
  let (modes, mode) = choices(app, Field::Mode);
  let (themes, theme) = choices(app, Field::Theme);
  let (languages, language) = choices(app, Field::Language);
  Form {
    rows: vec![
      Row::line(
        t(locale, "settings.mode"),
        Item::radios(Field::Mode, modes.into_iter().map(|(text, _)| text).collect(), mode),
      ),
      Row::line(
        t(locale, "settings.theme"),
        vec![Item::select(
          Field::Theme,
          themes[theme].0.clone(),
          Style::new(),
          Size::Fit,
        )],
      ),
      Row::line(
        t(locale, "settings.language"),
        vec![Item::select(
          Field::Language,
          languages[language].0.clone(),
          Style::new(),
          Size::Fit,
        )],
      ),
    ],
  }
}

pub fn choices<S: Service>(app: &App<S>, field: Field) -> (Vec<(String, Style)>, usize) {
  let locale = app.locale;
  let gui = &app.config.gui;
  match field {
    Field::Mode => (
      [
        "settings.displayModeAuto",
        "settings.displayModeLight",
        "settings.displayModeDark",
      ]
      .iter()
      .map(|key| (t(locale, key), Style::new()))
      .collect(),
      MODES.iter().position(|mode| *mode == gui.display_mode).unwrap_or(0),
    ),
    // Each theme is listed in its own primary color, which is a preview of the choice.
    Field::Theme => (
      Palette::all()
        .iter()
        .map(|theme| {
          (
            t(locale, &format!("settings.themes.{}", theme.as_str())),
            Style::new().fg(palette(*theme).0),
          )
        })
        .collect(),
      Palette::all().iter().position(|theme| *theme == gui.theme).unwrap_or(0),
    ),
    Field::Language => (
      Locale::ALL
        .iter()
        .map(|locale| (autonym(*locale).to_owned(), Style::new()))
        .collect(),
      Locale::ALL
        .iter()
        .position(|locale| *locale == Locale::resolve(&gui.language))
        .unwrap_or(0),
    ),
    _ => (Vec::new(), 0),
  }
}

pub fn choose<S: Service>(app: &mut App<S>, field: Field, index: usize, now: Instant) {
  match field {
    Field::Mode => {
      let mode = MODES[index.min(MODES.len() - 1)];
      app.edit_config(now, |config| config.gui.display_mode = mode);
    }
    Field::Theme => {
      let theme = Palette::all()[index.min(Palette::all().len() - 1)];
      app.edit_config(now, |config| config.gui.theme = theme);
    }
    Field::Language => {
      let locale = Locale::ALL[index.min(Locale::ALL.len() - 1)];
      app.edit_config(now, |config| config.gui.language = locale.tag().to_owned());
    }
    _ => {}
  }
}
