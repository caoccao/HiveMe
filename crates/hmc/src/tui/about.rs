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

//! The About tab, as text.
//!
//! Phase 3 shows what `About.tsx` says: the name and version, the tagline, the author
//! and the repository, the device, the config file, the history database, the license,
//! and the copyright. The large gradient letters and the cards that open their URL are
//! phase 5 of the terminal UI plan.

use hiveme_core::i18n::t;
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use super::app::App;
use super::service::Service;

/// The author `src/lib/constants.ts` names.
const AUTHOR: &str = "Sam Cao (https://github.com/caoccao)";

/// The start of the GUI's amber to orange gradient.
const AMBER: Color = Color::Rgb(0xff, 0xb3, 0x00);

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let locale = app.locale;
  let theme = app.theme;
  let about = app.service.about();
  let rows = [
    (t(locale, "about.author"), AUTHOR.to_owned()),
    (t(locale, "about.github"), about.github_url.clone()),
    (
      t(locale, "about.device"),
      format!("{} ({})", about.device_name, about.device_id),
    ),
    (t(locale, "about.configPath"), about.config_path.clone()),
    (t(locale, "about.databasePath"), about.database_path.clone()),
    (t(locale, "about.license"), "Apache-2.0".to_owned()),
  ];
  let label_width = rows.iter().map(|(label, _)| label.width()).max().unwrap_or(0);

  let mut lines = vec![
    Line::from(vec![
      Span::styled(
        hiveme_core::APP_NAME,
        Style::new().fg(AMBER).add_modifier(Modifier::BOLD),
      ),
      Span::raw("  "),
      Span::styled(format!("v{}", about.app_version), theme.active()),
    ]),
    Line::styled(t(locale, "about.tagline"), theme.muted()),
    Line::raw(""),
  ];
  for (label, value) in rows {
    let padding = " ".repeat(label_width - label.width() + 2);
    lines.push(Line::from(vec![
      Span::styled(format!("{label}{padding}"), theme.base().add_modifier(Modifier::BOLD)),
      Span::raw(value),
    ]));
  }
  lines.push(Line::raw(""));
  lines.push(Line::styled(t(locale, "about.copyright"), theme.muted()));

  let block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(theme.muted());
  let inner = block.inner(area).inner(Margin::new(1, 0));
  frame.render_widget(block, area);
  frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
