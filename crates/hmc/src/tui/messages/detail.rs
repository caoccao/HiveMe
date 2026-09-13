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

//! The detail view of one message: its content the width of the pane, with a cursor
//! that moves from tree node to tree node, since a terminal has no pointer to click a
//! branch with. `Enter` on a message opens it and `Esc` goes back to the list.

use std::time::Instant;

use hiveme_core::i18n::format;
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType};

use super::super::app::App;
use super::super::keys::Action;
use super::super::service::Service;
use super::super::widgets::truncate;
use super::json_tree;
use super::message_view::{self, ContentLine, Look, read};

/// The content lines of the message in the detail view and where the tree nodes start.
fn lines<S: Service>(app: &App<S>, width: u16) -> Option<(Vec<ContentLine>, Vec<usize>)> {
  let row = app.focused_row()?;
  let topic = app.selected_topic.as_deref().unwrap_or("");
  let look = app.look(topic);
  let reading = read(row);
  let lines = message_view::content(
    &reading,
    &look,
    usize::from(width),
    app.messages_tab.view.expansion.get(&row.row_id),
  );
  let nodes = lines
    .iter()
    .enumerate()
    .filter_map(|(index, line)| line.node.as_ref().map(|_| index))
    .collect();
  Some((lines, nodes))
}

/// The width the content is laid out at, inside the border and a cell of padding.
fn content_width(area: Rect) -> u16 {
  area.width.saturating_sub(4)
}

/// The content lines on screen, below the metadata line and a blank one.
fn content_height(area: Rect) -> usize {
  usize::from(area.height.saturating_sub(4))
}

impl<S: Service> App<S> {
  /// A key while the detail view is open.
  pub(in crate::tui) fn detail_action(&mut self, action: &Action, now: Instant) {
    let area = self.messages_tab.view.viewport;
    let Some((lines, nodes)) = lines(self, content_width(area)) else {
      self.messages_tab.view.detail = None;
      return;
    };
    let page = content_height(area).max(1);
    let row_id = self.focused_row().map(|row| row.row_id);
    let Some(detail) = self.messages_tab.view.detail.as_mut() else {
      return;
    };
    match action {
      Action::CopyBody | Action::CopyRaw => self.copy_focused(action, now),
      Action::Up | Action::Down if !nodes.is_empty() => {
        detail.cursor = if *action == Action::Up {
          detail.cursor.saturating_sub(1)
        } else {
          (detail.cursor + 1).min(nodes.len() - 1)
        };
      }
      Action::Up => detail.offset = detail.offset.saturating_sub(1),
      Action::Down => detail.offset = (detail.offset + 1).min(lines.len().saturating_sub(1)),
      Action::PageUp => detail.offset = detail.offset.saturating_sub(page),
      Action::PageDown => detail.offset = (detail.offset + page).min(lines.len().saturating_sub(1)),
      Action::Home => {
        detail.cursor = 0;
        detail.offset = 0;
      }
      Action::End => {
        detail.cursor = nodes.len().saturating_sub(1);
        detail.offset = lines.len().saturating_sub(page);
      }
      Action::Toggle | Action::Activate => {
        let node = nodes.get(detail.cursor).and_then(|index| lines[*index].node.clone());
        if let (Some((path, depth)), Some(row_id)) = (node, row_id) {
          let expansion = self.messages_tab.view.expansion.entry(row_id).or_default();
          json_tree::toggle(expansion, &path, depth);
          self.messages_tab.view.invalidate_row(row_id);
        }
      }
      _ => {}
    }
  }
}

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let theme = app.theme;
  let Some(row) = app.focused_row().cloned() else {
    app.messages_tab.view.detail = None;
    return;
  };
  let topic = app.selected_topic.clone().unwrap_or_default();
  let look: Look = app.look(&topic);
  let date_time = format::local(&row.ts).map_or_else(|| row.ts.clone(), |at| format::date_time(app.locale, &at));
  let block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(theme.focused())
    .title(format!(
      " {} ",
      truncate(
        &row.topic,
        usize::from(area.width.saturating_sub(4)),
        app.glyphs.ellipsis
      )
    ))
    .title_bottom(Line::from(format!(" {date_time} ")).right_aligned());
  let inner = block.inner(area).inner(Margin::new(1, 0));
  frame.render_widget(block, area);
  if inner.height == 0 {
    return;
  }

  let reading = read(&row);
  frame.render_widget(
    Line::from(message_view::metadata(&row, &reading, &look)),
    Rect::new(inner.x, inner.y, inner.width, 1),
  );
  let Some((lines, nodes)) = lines(app, content_width(area)) else {
    return;
  };
  let height = content_height(area);
  let Some(detail) = app.messages_tab.view.detail.as_mut() else {
    return;
  };
  detail.cursor = detail.cursor.min(nodes.len().saturating_sub(1));
  let cursor_line = nodes.get(detail.cursor).copied();
  if let Some(line) = cursor_line {
    if line < detail.offset {
      detail.offset = line;
    } else if height > 0 && line >= detail.offset + height {
      detail.offset = line + 1 - height;
    }
  }
  detail.offset = detail.offset.min(lines.len().saturating_sub(1));
  let offset = detail.offset;
  for (index, line) in lines.iter().enumerate().skip(offset).take(height) {
    let mut line = line.line.clone();
    if Some(index) == cursor_line {
      line = line.patch_style(theme.base().add_modifier(Modifier::REVERSED));
    }
    let y = inner.y + 2 + (index - offset) as u16;
    frame.render_widget(line, Rect::new(inner.x, y, inner.width, 1));
  }
}
