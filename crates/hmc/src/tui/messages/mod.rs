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

//! Tab 0, Messages, `Messages.tsx` in cells: the topic tree on the left, the chat view
//! and the composer on the right, and the divider between them.
//!
//! The focus goes round one ring with `Tab`: the filter, the tree, the list, the
//! composer's controls that can be used, and the footer's error entries. What a key does
//! is decided by where the focus is.

mod composer;
mod detail;
mod json_tree;
mod message_view;
mod topic_tree;

pub use composer::{ComposerState, Control};
pub use message_view::ViewState;
pub use topic_tree::TreeState;

use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType};

use super::app::{App, FooterEntry};
use super::keys::Action;
use super::service::Service;
use super::widgets::truncate;

/// Where the divider starts, and how far it moves, as a percentage of the tab's width.
pub const SPLIT_DEFAULT_PERCENT: u16 = 28;
pub const SPLIT_MIN_PERCENT: u16 = 15;
pub const SPLIT_MAX_PERCENT: u16 = 60;

/// How far `Ctrl+Left` and `Ctrl+Right` move the divider.
pub const SPLIT_STEP_PERCENT: u16 = 2;

/// How many rows one wheel notch moves the tree's cursor.
const WHEEL_ROWS: usize = 3;

/// A pane of the Messages tab, for a click that hit nothing more specific and for the
/// wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
  Filter,
  Tree,
  List,
}

/// What has the focus in the Messages tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
  Filter,
  Tree,
  List,
  Composer(Control),
  Footer(FooterEntry),
}

/// The state of the Messages tab, kept while another tab is shown.
#[derive(Debug, Clone, PartialEq)]
pub struct MessagesState {
  pub focus: Focus,
  /// The width of the topic pane, in percent.
  pub split: u16,
  /// The divider is being dragged.
  pub dragging: bool,
  /// Where the tab was drawn, for the drag.
  pub area: Rect,
  /// The width of the message box on the last frame, for `Up` and `Down` in it.
  pub editor_width: u16,
  pub tree: TreeState,
  pub view: ViewState,
  pub composer: ComposerState,
}

impl Default for MessagesState {
  fn default() -> Self {
    Self {
      focus: Focus::Tree,
      split: SPLIT_DEFAULT_PERCENT,
      dragging: false,
      area: Rect::new(0, 4, 80, 19),
      editor_width: 52,
      tree: TreeState::default(),
      view: ViewState::default(),
      composer: ComposerState::default(),
    }
  }
}

impl MessagesState {
  /// Whether the focus is in the message pane, whose border then takes the primary color.
  pub fn focus_in_right_pane(&self) -> bool {
    matches!(self.focus, Focus::List | Focus::Composer(_))
  }
}

impl<S: Service> App<S> {
  /// Whether a text field of the Messages tab has the focus, so printable keys are text.
  pub fn messages_typing(&self) -> bool {
    match self.messages_tab.focus {
      Focus::Filter => true,
      Focus::Composer(Control::Editor | Control::Topic | Control::Title) => self.messages_tab.composer.popup.is_none(),
      _ => false,
    }
  }

  /// Every place the focus can be, in `Tab` order, whether or not it can take it now.
  fn focus_order(&self) -> Vec<Focus> {
    let mut order = vec![Focus::Filter, Focus::Tree, Focus::List];
    order.extend(self.composer_controls().into_iter().map(Focus::Composer));
    order.extend(self.footer_entries().into_iter().map(Focus::Footer));
    order
  }

  fn can_focus(&self, focus: Focus) -> bool {
    match focus {
      Focus::Composer(control) => self.control_enabled(control),
      _ => true,
    }
  }

  fn move_focus(&mut self, forward: bool) {
    self.messages_tab.composer.popup = None;
    let order = self.focus_order();
    let current = order
      .iter()
      .position(|focus| *focus == self.messages_tab.focus)
      .unwrap_or(1);
    for step in 1..=order.len() {
      let index = if forward {
        (current + step) % order.len()
      } else {
        (current + order.len() * 2 - step) % order.len()
      };
      if self.can_focus(order[index]) {
        self.messages_tab.focus = order[index];
        return;
      }
    }
  }

  /// A key or a click in the Messages tab.
  pub(in crate::tui) fn perform_messages(&mut self, action: Action, now: Instant) {
    // A click anywhere but on the popup closes it, and a click on the Level button that
    // opened it only closes it.
    if matches!(
      action,
      Action::FocusPane(_)
        | Action::ToggleTopic(_)
        | Action::SelectTopic(_)
        | Action::FocusMessage(_)
        | Action::Composer(_)
        | Action::ComposerQos(_)
        | Action::Divider
    ) && self.messages_tab.composer.popup.take().is_some()
      && action == Action::Composer(Control::Level)
    {
      return;
    }
    match &action {
      Action::FocusNext => self.move_focus(true),
      Action::FocusPrevious => self.move_focus(false),
      Action::FocusFilter => self.messages_tab.focus = Focus::Filter,
      Action::SplitLeft | Action::SplitRight => {
        let split = if action == Action::SplitLeft {
          self.messages_tab.split.saturating_sub(SPLIT_STEP_PERCENT)
        } else {
          self.messages_tab.split + SPLIT_STEP_PERCENT
        };
        self.messages_tab.split = split.clamp(SPLIT_MIN_PERCENT, SPLIT_MAX_PERCENT);
      }
      Action::FocusPane(pane) => {
        self.messages_tab.focus = match pane {
          Pane::Filter => Focus::Filter,
          Pane::Tree => Focus::Tree,
          Pane::List => Focus::List,
        };
      }
      Action::ToggleTopic(id) => self.toggle_topic(id),
      Action::SelectTopic(id) => self.click_topic(id, now),
      Action::FocusMessage(row_id) => self.focus_message(*row_id),
      Action::Composer(control) => self.composer_action(*control, &action),
      Action::ComposerQos(qos) => self.click_qos(*qos),
      Action::ComposerLevel(index) => self.pick_level(*index),
      Action::Divider => self.messages_tab.dragging = true,
      _ => match self.messages_tab.focus {
        Focus::Filter => match action {
          Action::Edit(_) => self.filter_edit(&action),
          Action::Down | Action::Activate => self.messages_tab.focus = Focus::Tree,
          _ => {}
        },
        Focus::Tree => self.tree_action(&action, now),
        Focus::List if self.messages_tab.view.detail.is_some() => self.detail_action(&action, now),
        Focus::List => self.list_action(&action, now),
        Focus::Composer(control) => self.composer_action(control, &action),
        Focus::Footer(entry) => {
          if action == Action::Activate {
            self.perform(
              match entry {
                FooterEntry::ConfigError => Action::ShowConfigError,
                FooterEntry::LastError => Action::ShowLastError,
              },
              now,
            );
          }
        }
      },
    }
  }

  /// `Esc` in the Messages tab: the Level popup, then the detail view, then back to the
  /// tree.
  pub(in crate::tui) fn escape_messages(&mut self) {
    let state = &mut self.messages_tab;
    if state.composer.popup.take().is_some() {
      return;
    }
    if state.view.detail.take().is_some() {
      return;
    }
    state.focus = Focus::Tree;
  }

  /// A paste into the focused field of the Messages tab.
  pub(in crate::tui) fn paste_messages(&mut self, text: &str) {
    match self.messages_tab.focus {
      Focus::Filter => {
        if self.messages_tab.tree.filter.paste(text) {
          self.after_filter_change();
        }
      }
      Focus::Composer(control) => self.composer_paste(control, text),
      _ => {}
    }
  }

  /// A wheel notch: the pane under the pointer scrolls, or the focused one.
  pub(in crate::tui) fn wheel_messages(&mut self, position: Position, up: bool, now: Instant) {
    let pane = self
      .hits
      .iter()
      .rev()
      .find_map(|(area, action)| match action {
        Action::FocusPane(pane) if area.contains(position) => Some(*pane),
        _ => None,
      })
      .unwrap_or(match self.messages_tab.focus {
        Focus::Filter | Focus::Tree => Pane::Tree,
        _ => Pane::List,
      });
    match pane {
      Pane::Filter | Pane::Tree => {
        let action = if up { Action::Up } else { Action::Down };
        for _ in 0..WHEEL_ROWS {
          self.tree_action(&action, now);
        }
      }
      Pane::List => self.wheel_list(up, now),
    }
  }

  /// The divider follows a drag.
  pub(in crate::tui) fn drag_divider(&mut self, column: u16) {
    let area = self.messages_tab.area;
    if area.width == 0 {
      return;
    }
    let percent = (u32::from(column.saturating_sub(area.x) + 1) * 100 / u32::from(area.width)) as u16;
    self.messages_tab.split = percent.clamp(SPLIT_MIN_PERCENT, SPLIT_MAX_PERCENT);
  }
}

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  app.messages_tab.area = area;
  let theme = app.theme;
  let left_width = area.width * app.messages_tab.split / 100;
  let left = Rect::new(area.x, area.y, left_width, area.height);
  let right = Rect::new(area.x + left_width, area.y, area.width - left_width, area.height);
  topic_tree::render(app, frame, left);

  let border = if app.messages_tab.dragging {
    Style::new().fg(theme.secondary)
  } else if app.messages_tab.focus_in_right_pane() {
    theme.focused()
  } else {
    theme.muted()
  };
  let mut block = Block::bordered().border_type(BorderType::Rounded).border_style(border);
  if let Some(topic) = app.selected_topic.as_deref() {
    let room = usize::from(right.width.saturating_sub(4));
    block = block.title(format!(" {} ", truncate(topic, room, app.glyphs.ellipsis)));
  }
  let inner = block.inner(right);
  frame.render_widget(block, right);

  let composer_height = if app.selected_topic.is_some() {
    composer::height(app, inner.width).min(inner.height * 7 / 10)
  } else {
    0
  };
  let list = Rect::new(inner.x, inner.y, inner.width, inner.height - composer_height);
  let composer_area = Rect::new(inner.x, list.bottom(), inner.width, composer_height);
  message_view::render(app, frame, list);
  if composer_height > 0 {
    composer::render(app, frame, composer_area, right);
  }
  // The borders on either side of the divider are what a drag grabs.
  app.hits.push((
    Rect::new(left.right().saturating_sub(1), area.y, 2, area.height),
    Action::Divider,
  ));
}
