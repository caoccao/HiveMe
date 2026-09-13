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

//! The key binding overlay. A terminal has no tooltips to name the keys, so this lists
//! the keys of the current scope: everywhere, the open tab, and the update notice when
//! it is on screen. The key names are written as keyboards print them, in every
//! language; what they do is translated.

use hiveme_core::i18n::{Locale, t};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Tab};
use super::keys::Action;
use super::service::Service;

/// A heading and its rows of keys and what they do, as catalog keys.
pub type Section = (String, Vec<(&'static str, &'static str)>);

/// What the overlay lists for the open tab and the notice.
pub fn sections(locale: Locale, tab: Tab, notice: bool) -> Vec<Section> {
  let mut sections = vec![(
    t(locale, "tui.help.global"),
    vec![
      ("Ctrl+Q, Ctrl+C", "tui.help.quit"),
      ("F2", "tui.help.connection"),
      ("F3", "tui.help.pause"),
      ("F4", "tui.help.clear"),
      ("F10", "tui.help.settings"),
      ("F1", "tui.help.about"),
      ("Alt+1..9", "tui.help.selectTab"),
      ("Alt+Left, Alt+Right", "tui.help.cycleTabs"),
      ("Ctrl+W", "tui.help.closeTab"),
      ("?, Ctrl+/", "tui.help.help"),
      ("Esc", "tui.help.escape"),
    ],
  )];
  match tab {
    Tab::Messages => sections.push((
      t(locale, "tabs.messages"),
      vec![("Tab, Shift+Tab", "tui.help.focus"), ("Enter", "tui.help.activate")],
    )),
    Tab::Settings => sections.push((
      t(locale, "tabs.settings"),
      vec![
        ("Up, Down", "tui.help.category"),
        ("Tab, Shift+Tab, Enter", "tui.help.field"),
        ("Ctrl+H", "tui.help.password"),
        ("Ctrl+A, Ctrl+E, Ctrl+U", "tui.help.edit"),
      ],
    )),
    Tab::About => {}
  }
  if notice {
    sections.push((
      t(locale, "tui.help.notice"),
      vec![
        ("o", "tui.help.releases"),
        ("s", "update.skipThisVersion"),
        ("x", "tui.help.dismiss"),
      ],
    ));
  }
  sections
}

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let locale = app.locale;
  let theme = app.theme;
  let sections = sections(locale, app.current_tab(), app.notice.is_some());
  let key_width = sections
    .iter()
    .flat_map(|(_, rows)| rows.iter().map(|(keys, _)| keys.width()))
    .max()
    .unwrap_or(0);

  let mut lines = Vec::new();
  for (index, (heading, rows)) in sections.into_iter().enumerate() {
    if index > 0 {
      lines.push(Line::raw(""));
    }
    lines.push(Line::styled(heading, theme.active()));
    for (keys, action) in rows {
      lines.push(Line::from(vec![
        Span::styled(
          format!("{keys:<key_width$}  "),
          theme.base().add_modifier(Modifier::BOLD),
        ),
        Span::raw(t(locale, action)),
      ]));
    }
  }
  let width = lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 4;
  let height = lines.len() as u16 + 2;
  let popup = area.centered(Constraint::Length(width), Constraint::Length(height));

  let block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(theme.focused())
    .title(format!(" {} ", t(locale, "tui.help.title")))
    .title_bottom(Line::from(format!(" Esc: {} ", t(locale, "tui.help.close"))).right_aligned())
    .style(theme.base());
  frame.render_widget(Clear, popup);
  frame.render_widget(
    Paragraph::new(lines).block(block.padding(ratatui::widgets::Padding::horizontal(1))),
    popup,
  );
  // A click anywhere closes the overlay, as a key does.
  app.hits.push((area, Action::Help));
}
