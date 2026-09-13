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

//! The rows of the frame, see `docs/specs/tui.md`, "Layout": the toolbar, the update
//! notice, the tabs, the content, and the footer, with the overlays drawn last.

use hiveme_core::i18n::t_with;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::app::{App, Tab};
use super::service::Service;
use super::{about, footer, help, messages, settings, snackbar, tabs, toolbar, update_notice};

/// The smallest terminal the frame is drawn in, as `hmg` has a smallest window.
pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

/// The toolbar is a bordered block of exactly three rows.
pub const TOOLBAR_HEIGHT: u16 = 3;

/// Where each row of the frame goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Areas {
  pub toolbar: Rect,
  pub notice: Option<Rect>,
  pub tabs: Rect,
  pub content: Rect,
  pub footer: Rect,
}

impl Areas {
  pub fn new(area: Rect, notice: bool) -> Self {
    let [toolbar, notice_row, tabs, content, footer] = area.layout(&Layout::vertical([
      Constraint::Length(TOOLBAR_HEIGHT),
      Constraint::Length(u16::from(notice)),
      Constraint::Length(1),
      Constraint::Fill(1),
      Constraint::Length(1),
    ]));
    Self {
      toolbar,
      notice: notice.then_some(notice_row),
      tabs,
      content,
      footer,
    }
  }
}

/// Whether the terminal is too small for the frame.
pub fn is_too_small(area: Rect) -> bool {
  area.width < MIN_WIDTH || area.height < MIN_HEIGHT
}

/// Draws one frame and records where a click lands.
pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame) {
  app.hits.clear();
  let area = frame.area();
  frame.render_widget(Block::new().style(app.theme.base()), area);

  if is_too_small(area) {
    let line = t_with(
      app.locale,
      "tui.tooSmall",
      &[("columns", &MIN_WIDTH.to_string()), ("rows", &MIN_HEIGHT.to_string())],
    );
    let middle = area.centered_vertically(Constraint::Length(1));
    frame.render_widget(Paragraph::new(line).centered().wrap(Wrap { trim: true }), middle);
    return;
  }

  let areas = Areas::new(area, app.notice.is_some());
  toolbar::render(app, frame, areas.toolbar);
  if let Some(notice) = areas.notice {
    update_notice::render(app, frame, notice);
  }
  tabs::render(app, frame, areas.tabs);
  match app.current_tab() {
    Tab::Messages => messages::render(app, frame, areas.content),
    Tab::Settings => settings::render(app, frame, areas.content),
    Tab::About => about::render(app, frame, areas.content),
  }
  footer::render(app, frame, areas.footer);

  if app.help {
    help::render(app, frame, area);
  }
  if app.snackbar.is_some() {
    snackbar::render(app, frame, area);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_rows_are_toolbar_notice_tabs_content_footer() {
    let areas = Areas::new(Rect::new(0, 0, 80, 24), false);
    assert_eq!(areas.toolbar, Rect::new(0, 0, 80, 3));
    assert_eq!(areas.notice, None);
    assert_eq!(areas.tabs, Rect::new(0, 3, 80, 1));
    assert_eq!(areas.content, Rect::new(0, 4, 80, 19));
    assert_eq!(areas.footer, Rect::new(0, 23, 80, 1));

    let areas = Areas::new(Rect::new(0, 0, 120, 40), true);
    assert_eq!(areas.notice, Some(Rect::new(0, 3, 120, 1)));
    assert_eq!(areas.tabs, Rect::new(0, 4, 120, 1));
    assert_eq!(areas.content, Rect::new(0, 5, 120, 34));
  }

  #[test]
  fn below_eighty_by_twenty_four_is_too_small() {
    assert!(!is_too_small(Rect::new(0, 0, 80, 24)));
    assert!(is_too_small(Rect::new(0, 0, 79, 24)));
    assert!(is_too_small(Rect::new(0, 0, 80, 23)));
  }
}
