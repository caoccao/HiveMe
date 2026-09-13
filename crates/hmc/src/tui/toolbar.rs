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

//! The toolbar, `Toolbar.tsx` with the key of each tool written on it.
//!
//! A block three rows high, a rule above and below the tools, titled
//! `HiveMe v<version>`. It has no side borders, which is what lets every English label
//! fit an 80 column terminal. The tools are labeled buttons; Help and Quit sit at the
//! right end. When the labels of a language do not
//! fit the width, the longest label is cut first, down to the key alone, so every tool
//! stays on screen with its key.

use hiveme_core::i18n::t;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Tab};
use super::keys::Action;
use super::service::Service;
use super::widgets::truncate;

/// One button of the toolbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
  pub key: &'static str,
  pub label: String,
  pub action: Action,
  pub active: bool,
  pub enabled: bool,
}

impl Tool {
  fn text(&self, label: &str) -> String {
    if label.is_empty() {
      format!("[{}]", self.key)
    } else {
      format!("[{} {label}]", self.key)
    }
  }
}

/// The tools on the left and on the right, as the state of `app` has them.
pub fn tools<S: Service>(app: &App<S>) -> (Vec<Tool>, Vec<Tool>) {
  let locale = app.locale;
  let live = app.is_live();
  let paused = app.status.notifications_paused;
  let tool = |key, label: &str, action, active, enabled| Tool {
    key,
    label: t(locale, label),
    action,
    active,
    enabled,
  };
  let left = vec![
    tool(
      "F2",
      if live {
        "tui.toolbar.disconnect"
      } else {
        "tui.toolbar.connect"
      },
      Action::Connection,
      live,
      true,
    ),
    tool(
      "F3",
      if paused {
        "tui.toolbar.resume"
      } else {
        "tui.toolbar.pause"
      },
      Action::Pause,
      paused,
      true,
    ),
    tool(
      "F4",
      "tui.toolbar.clear",
      Action::ClearTopic,
      false,
      app.selected_topic.is_some(),
    ),
    tool(
      "F10",
      "tui.toolbar.settings",
      Action::Settings,
      app.tabs.contains(&Tab::Settings),
      true,
    ),
    tool(
      "F1",
      "tui.toolbar.about",
      Action::About,
      app.tabs.contains(&Tab::About),
      true,
    ),
  ];
  let right = vec![
    tool("?", "tui.toolbar.help", Action::Help, app.help, true),
    tool("^Q", "tui.toolbar.quit", Action::Quit, false, true),
  ];
  (left, right)
}

/// The label widths that fit `room` cells, cutting the longest label first.
///
/// A width of zero means the key alone. The tools are separated by one cell, and the two
/// groups by at least one.
pub fn fit(tools: &[Tool], room: usize, ellipsis: &str) -> Vec<usize> {
  let mut widths: Vec<usize> = tools.iter().map(|tool| tool.label.width()).collect();
  let total = |widths: &[usize]| -> usize {
    let buttons: usize = tools
      .iter()
      .zip(widths)
      .map(|(tool, width)| 2 + tool.key.width() + if *width == 0 { 0 } else { 1 + width })
      .sum();
    buttons + tools.len().saturating_sub(1)
  };
  let floor = ellipsis.width() + 1;
  while total(&widths) > room {
    let Some((index, widest)) = widths.iter().copied().enumerate().max_by_key(|(_, width)| *width) else {
      break;
    };
    if widest == 0 {
      break;
    }
    // A label cut below one character and the ellipsis says nothing; drop it instead.
    widths[index] = if widest - 1 < floor { 0 } else { widest - 1 };
  }
  widths
}

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let theme = app.theme;
  let block = Block::new()
    .borders(Borders::TOP | Borders::BOTTOM)
    .border_type(BorderType::Plain)
    .border_style(theme.muted())
    .title(Span::styled(
      format!(" {} v{} ", hiveme_core::APP_NAME, hiveme_core::VERSION),
      theme.base(),
    ));
  let inner = block.inner(area);
  frame.render_widget(block, area);
  if inner.width < 1 || inner.height == 0 {
    return;
  }

  let (left, right) = tools(app);
  let all: Vec<Tool> = left.iter().chain(right.iter()).cloned().collect();
  // One cell of margin on the left; the gap between the groups takes up the rest.
  let room = usize::from(inner.width).saturating_sub(1);
  let widths = fit(&all, room, app.glyphs.ellipsis);

  let texts: Vec<String> = all
    .iter()
    .zip(&widths)
    .map(|(tool, width)| tool.text(&truncate(&tool.label, *width, app.glyphs.ellipsis)))
    .collect();
  let used: usize = texts.iter().map(|text| text.width()).sum::<usize>() + texts.len().saturating_sub(1);
  let mut spans = vec![Span::raw(" ")];
  let mut x = inner.x + 1;
  for (index, (tool, text)) in all.iter().zip(texts).enumerate() {
    // One cell between tools, and whatever is left between the two groups.
    let gap = if index == left.len() {
      room.saturating_sub(used) + 1
    } else {
      usize::from(index > 0)
    };
    spans.push(Span::raw(" ".repeat(gap)));
    x += gap as u16;
    let cells = text.width() as u16;
    let style = if !tool.enabled {
      theme.disabled()
    } else if tool.active {
      theme.active()
    } else {
      Style::new()
    };
    spans.push(Span::styled(text, style));
    if tool.enabled {
      app.hits.push((Rect::new(x, inner.y, cells, 1), tool.action.clone()));
    }
    x += cells;
  }
  frame.render_widget(Line::from(spans), Rect::new(inner.x, inner.y, inner.width, 1));
}

#[cfg(test)]
mod tests {
  use super::*;

  fn tool(key: &'static str, label: &str) -> Tool {
    Tool {
      key,
      label: label.to_owned(),
      action: Action::Help,
      active: false,
      enabled: true,
    }
  }

  #[test]
  fn labels_that_fit_are_kept_whole() {
    let tools = [tool("F2", "Connect"), tool("F3", "Pause")];
    // "[F2 Connect]" is 12, "[F3 Pause]" is 10, with one cell between them.
    assert_eq!(fit(&tools, 23, "…"), vec![7, 5]);
    assert_eq!(fit(&tools, 22, "…"), vec![6, 5]);
  }

  #[test]
  fn the_longest_label_is_cut_first_and_then_dropped() {
    let tools = [tool("F2", "Disconnect"), tool("F3", "Pause")];
    assert_eq!(fit(&tools, 22, "…"), vec![6, 5]);
    // With no room for any label, only the keys remain.
    assert_eq!(fit(&tools, 9, "…"), vec![0, 0]);
  }
}
