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

//! The snackbar, `NotificationSnackbar.tsx`: one row at the top center, over whatever
//! is there, until its time is up or a key is pressed. A confirmation is drawn in the
//! success color and a failure in the error color, as the GUI's filled `Alert` is.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Clear;
use unicode_width::UnicodeWidthStr;

use super::app::App;
use super::keys::Action;
use super::service::Service;
use super::widgets::truncate;

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let Some(snackbar) = app.snackbar.as_ref() else {
    return;
  };
  // One line, however long the detail: a config with several problems is folded onto
  // it, as `hmc` folds it on stderr.
  let folded = snackbar
    .text
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join("; ");
  let room = usize::from(area.width) * 4 / 5;
  let text = format!(" {} ", truncate(&folded, room.saturating_sub(2), app.glyphs.ellipsis));
  let cells = text.width() as u16;
  let row = Rect::new(area.x, area.y, area.width, 1).centered_horizontally(Constraint::Length(cells));
  frame.render_widget(Clear, row);
  frame.render_widget(
    Line::styled(
      text,
      Style::new()
        .bg(if snackbar.error {
          app.theme.error
        } else {
          app.theme.success
        })
        .fg(Color::Rgb(255, 255, 255)),
    ),
    row,
  );
  app.hits.push((row, Action::DismissSnackbar));
}
