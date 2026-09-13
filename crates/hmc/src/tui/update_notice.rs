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

//! The line above the tabs while a newer release exists, the update `Alert` of
//! `MainContent.tsx`: the announcement opens the releases page with `o`, Skip this
//! version toggles with `s`, and the close mark closes it with `x`, skipping the version
//! when that is checked.

use hiveme_core::i18n::{t, t_with};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::app::App;
use super::keys::Action;
use super::service::Service;

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let Some(notice) = app.notice.as_ref() else {
    return;
  };
  let theme = app.theme;
  let glyphs = app.glyphs;
  let announcement = format!(
    " {} (o)",
    t_with(
      app.locale,
      "update.newVersionAvailable",
      &[("version", &notice.version)]
    )
  );
  let skip = format!(
    "{} {} (s)",
    if notice.skip { glyphs.checked } else { glyphs.unchecked },
    t(app.locale, "update.skipThisVersion")
  );
  let close = format!("{} (x) ", glyphs.close);

  let right_cells = (skip.width() + 2 + close.width()) as u16;
  let announcement_cells = announcement.width() as u16;
  let gap = area.width.saturating_sub(announcement_cells + right_cells);
  let line = Line::from(vec![
    Span::styled(
      announcement,
      Style::new().fg(theme.primary).add_modifier(Modifier::UNDERLINED),
    ),
    Span::raw(" ".repeat(usize::from(gap))),
    Span::raw(skip.clone()),
    Span::raw("  "),
    Span::styled(close.clone(), theme.muted()),
  ]);
  frame.render_widget(line, area);

  let skip_x = area.x + announcement_cells + gap;
  let close_x = skip_x + skip.width() as u16 + 2;
  app.hits.push((
    Rect::new(area.x, area.y, announcement_cells.min(area.width), 1),
    Action::OpenReleases,
  ));
  app.hits.push((
    Rect::new(skip_x, area.y, skip.width() as u16, 1).intersection(area),
    Action::ToggleSkipVersion,
  ));
  app.hits.push((
    Rect::new(close_x, area.y, close.width() as u16, 1).intersection(area),
    Action::CloseUpdateNotice,
  ));
}
