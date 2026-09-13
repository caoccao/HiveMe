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

//! The topic tree and its filter, `TopicTree.tsx` in cells.
//!
//! The nodes are the session's tree with `hiveme` merged in, so the startup topic is
//! always there. Expansion follows the GUI: the first tree is fully open until the user
//! changes it, a filter opens everything it found, and selecting a label never changes
//! what is open.

use std::collections::HashSet;
use std::time::Instant;

use hiveme_core::i18n::{format, t};
use hiveme_core::session::TopicNode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use unicode_width::UnicodeWidthStr;

use super::super::app::{App, STARTUP_TOPIC};
use super::super::keys::Action;
use super::super::service::Service;
use super::super::widgets::{TextInput, truncate};
use super::{Focus, Pane};

/// The most an unread badge counts before it says `999+`.
const BADGE_CAP: u32 = 999;

/// The state of the tree, kept while another tab is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeState {
  pub filter: TextInput,
  /// The ids of the open nodes.
  pub expanded: HashSet<String>,
  /// Whether the user has opened or closed a node, after which new topics arrive closed.
  pub touched: bool,
  /// The id of the row the keys move, which is not necessarily the selected topic.
  pub cursor: Option<String>,
  /// The first row on screen.
  pub offset: usize,
  /// How many rows the tree had on the last frame, for paging.
  pub height: usize,
}

/// One row of the tree as it is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
  pub id: String,
  pub label: String,
  /// The topic the row selects; `None` for the empty leading segment of an absolute
  /// path, which is only a group.
  pub topic: Option<String>,
  pub unread: u32,
  pub depth: usize,
  pub has_children: bool,
  pub open: bool,
  pub parent: Option<String>,
}

impl Default for TreeState {
  fn default() -> Self {
    Self {
      filter: TextInput::default(),
      expanded: HashSet::new(),
      touched: false,
      cursor: Some(STARTUP_TOPIC.to_owned()),
      offset: 0,
      height: 0,
    }
  }
}

/// The stored tree with `hiveme` always present. A stored `hiveme` keeps its children and
/// counts.
pub fn with_startup(stored: &[TopicNode]) -> Vec<TopicNode> {
  if stored.iter().any(|node| node.id == STARTUP_TOPIC) {
    return stored
      .iter()
      .map(|node| {
        let mut node = node.clone();
        if node.id == STARTUP_TOPIC {
          node.topic = Some(STARTUP_TOPIC.to_owned());
        }
        node
      })
      .collect();
  }
  let mut topics = vec![TopicNode {
    id: STARTUP_TOPIC.to_owned(),
    label: STARTUP_TOPIC.to_owned(),
    topic: Some(STARTUP_TOPIC.to_owned()),
    unread: 0,
    messages: 0,
    children: Vec::new(),
  }];
  topics.extend(stored.iter().cloned());
  topics
}

/// The nodes whose path, or a descendant's path, contains `filter`, case-insensitively.
/// A parent is kept when a child matches, since hiding it would hide the match.
pub fn filter_nodes(nodes: &[TopicNode], filter: &str) -> Vec<TopicNode> {
  let needle = filter.trim().to_lowercase();
  if needle.is_empty() {
    return nodes.to_vec();
  }
  fn keep(node: &TopicNode, needle: &str) -> Option<TopicNode> {
    let children: Vec<TopicNode> = node.children.iter().filter_map(|child| keep(child, needle)).collect();
    if !children.is_empty() || node.id.to_lowercase().contains(needle) {
      let mut kept = node.clone();
      kept.children = children;
      Some(kept)
    } else {
      None
    }
  }
  nodes.iter().filter_map(|node| keep(node, &needle)).collect()
}

/// Every node id, which is what opens the tree while a filter is typed.
pub fn all_node_ids(nodes: &[TopicNode]) -> Vec<String> {
  nodes
    .iter()
    .flat_map(|node| std::iter::once(node.id.clone()).chain(all_node_ids(&node.children)))
    .collect()
}

/// What the tree shows: the filtered nodes, with `hiveme` kept even when it did not
/// match.
pub fn visible(topics: &[TopicNode], filter: &str) -> Vec<TopicNode> {
  let filtered = filter_nodes(topics, filter);
  if filtered.iter().any(|node| node.id == STARTUP_TOPIC) {
    return filtered;
  }
  let mut startup = topics
    .iter()
    .find(|node| node.id == STARTUP_TOPIC)
    .cloned()
    .unwrap_or_else(|| with_startup(&[]).remove(0));
  startup.children.clear();
  std::iter::once(startup).chain(filtered).collect()
}

impl TreeState {
  /// Opens what the GUI opens when the topics or the filter change: everything a filter
  /// found, and the whole tree until the user has changed its expansion.
  pub fn sync_expansion(&mut self, topics: &[TopicNode]) {
    if !self.filter.text().trim().is_empty() {
      self.expanded = all_node_ids(&filter_nodes(topics, self.filter.text()))
        .into_iter()
        .collect();
    } else if !self.touched {
      self.expanded = all_node_ids(topics).into_iter().collect();
    }
  }

  /// The rows on screen, in order.
  pub fn rows(&self, topics: &[TopicNode]) -> Vec<Row> {
    fn walk(nodes: &[TopicNode], depth: usize, parent: Option<&str>, state: &TreeState, out: &mut Vec<Row>) {
      for node in nodes {
        let open = state.expanded.contains(&node.id);
        out.push(Row {
          id: node.id.clone(),
          label: node.label.clone(),
          topic: node.topic.clone(),
          unread: node.unread,
          depth,
          has_children: !node.children.is_empty(),
          open,
          parent: parent.map(str::to_owned),
        });
        if open {
          walk(&node.children, depth + 1, Some(&node.id), state, out);
        }
      }
    }
    let mut out = Vec::new();
    walk(&visible(topics, self.filter.text()), 0, None, self, &mut out);
    out
  }

  /// Opens or closes a node.
  pub fn set_open(&mut self, id: &str, open: bool) {
    self.touched = true;
    if open {
      self.expanded.insert(id.to_owned());
    } else {
      self.expanded.remove(id);
    }
  }

  /// Where the cursor is among `rows`: its row, the selected topic's, or the first.
  pub fn cursor_index(&self, rows: &[Row], selected: Option<&str>) -> usize {
    self
      .cursor
      .as_deref()
      .and_then(|cursor| rows.iter().position(|row| row.id == cursor))
      .or_else(|| selected.and_then(|selected| rows.iter().position(|row| row.id == selected)))
      .unwrap_or(0)
  }
}

impl<S: Service> App<S> {
  /// Asks the session for the tree again, after anything that changes it.
  pub fn refresh_topics(&mut self, now: Instant) {
    match self.service.topic_tree() {
      Ok(stored) => {
        self.topics = with_startup(&stored);
        self.messages_tab.tree.sync_expansion(&self.topics);
      }
      Err(error) => self.notify_error(error.to_string(), now),
    }
  }

  /// A key while the tree has the focus.
  pub(in crate::tui) fn tree_action(&mut self, action: &Action, now: Instant) {
    let rows = self.messages_tab.tree.rows(&self.topics);
    if rows.is_empty() {
      return;
    }
    let index = self
      .messages_tab
      .tree
      .cursor_index(&rows, self.selected_topic.as_deref());
    let row = &rows[index];
    let page = self.messages_tab.tree.height.max(1);
    let move_to = |target: usize| Some(rows[target.min(rows.len() - 1)].id.clone());
    let tree = &mut self.messages_tab.tree;
    match action {
      Action::Up => tree.cursor = move_to(index.saturating_sub(1)),
      Action::Down => tree.cursor = move_to(index + 1),
      Action::PageUp => tree.cursor = move_to(index.saturating_sub(page)),
      Action::PageDown => tree.cursor = move_to(index + page),
      Action::Home => tree.cursor = move_to(0),
      Action::End => tree.cursor = move_to(rows.len() - 1),
      Action::Right if row.has_children && !row.open => tree.set_open(&row.id, true),
      Action::Right if row.has_children => tree.cursor = move_to(index + 1),
      Action::Left if row.has_children && row.open => tree.set_open(&row.id, false),
      Action::Left => {
        if let Some(parent) = row.parent.clone() {
          tree.cursor = Some(parent);
        }
      }
      Action::Toggle if row.has_children => tree.set_open(&row.id, !row.open),
      Action::Activate | Action::Newline => {
        tree.cursor = Some(row.id.clone());
        if let Some(topic) = row.topic.clone() {
          self.select_topic(&topic, now);
        }
      }
      _ => {}
    }
  }

  /// A click on a node's marker.
  pub(in crate::tui) fn toggle_topic(&mut self, id: &str) {
    self.messages_tab.focus = Focus::Tree;
    let rows = self.messages_tab.tree.rows(&self.topics);
    if let Some(row) = rows.iter().find(|row| row.id == id) {
      self.messages_tab.tree.cursor = Some(row.id.clone());
      if row.has_children {
        self.messages_tab.tree.set_open(id, !row.open);
      }
    }
  }

  /// A click on a node's label, which selects it and leaves its expansion alone.
  pub(in crate::tui) fn click_topic(&mut self, id: &str, now: Instant) {
    self.messages_tab.focus = Focus::Tree;
    self.messages_tab.tree.cursor = Some(id.to_owned());
    let topic = self
      .messages_tab
      .tree
      .rows(&self.topics)
      .into_iter()
      .find(|row| row.id == id)
      .and_then(|row| row.topic);
    if let Some(topic) = topic {
      self.select_topic(&topic, now);
    }
  }

  /// A key in the filter field.
  pub(in crate::tui) fn filter_edit(&mut self, action: &Action) {
    let changed = match action {
      Action::Edit(key) => self.messages_tab.tree.filter.handle(*key),
      _ => false,
    };
    if changed {
      self.after_filter_change();
    }
  }

  pub(in crate::tui) fn after_filter_change(&mut self) {
    let tree = &mut self.messages_tab.tree;
    tree.sync_expansion(&self.topics);
    tree.offset = 0;
    let rows = tree.rows(&self.topics);
    if !rows.iter().any(|row| Some(row.id.as_str()) == tree.cursor.as_deref()) {
      tree.cursor = rows.first().map(|row| row.id.clone());
    }
  }
}

/// The unread badge: ` (3)`, ` (1.000)` in German, ` (999+)` past the cap.
pub fn badge<S: Service>(app: &App<S>, unread: u32) -> String {
  if unread == 0 {
    return String::new();
  }
  let count = format::integer(app.locale, i64::from(unread.min(BADGE_CAP)));
  let more = if unread > BADGE_CAP { "+" } else { "" };
  format!(" ({count}{more})")
}

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let theme = app.theme;
  let glyphs = app.glyphs;
  let focus = app.messages_tab.focus;
  let focused = matches!(focus, Focus::Filter | Focus::Tree);
  let block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(if focused { theme.focused() } else { theme.muted() })
    .title(format!(" {} ", t(app.locale, "topics.filter")));
  let inner = block.inner(area);
  frame.render_widget(block, area);
  app.hits.push((area, Action::FocusPane(Pane::Tree)));
  if inner.height == 0 || inner.width < 3 {
    return;
  }

  let filter_row = Rect::new(inner.x, inner.y, inner.width, 1);
  let prompt_style = if focus == Focus::Filter {
    theme.active()
  } else {
    theme.muted()
  };
  frame.render_widget(Line::from(Span::styled("> ", prompt_style)), filter_row);
  let field = Rect::new(inner.x + 2, inner.y, inner.width - 2, 1);
  app
    .messages_tab
    .tree
    .filter
    .render(frame, field, theme.base(), false, focus == Focus::Filter);
  app.hits.push((filter_row, Action::FocusPane(Pane::Filter)));

  let mut top = inner.y + 1;
  let filter = app.messages_tab.tree.filter.text().to_owned();
  if !filter.trim().is_empty() && filter_nodes(&app.topics, &filter).is_empty() && top < inner.bottom() {
    let line = truncate(
      &t(app.locale, "topics.noMatch"),
      usize::from(inner.width),
      glyphs.ellipsis,
    );
    frame.render_widget(
      Line::styled(line, theme.muted()),
      Rect::new(inner.x, top, inner.width, 1),
    );
    top += 1;
  }
  let height = usize::from(inner.bottom().saturating_sub(top));
  let rows = app.messages_tab.tree.rows(&app.topics);
  let cursor = app.messages_tab.tree.cursor_index(&rows, app.selected_topic.as_deref());
  let tree = &mut app.messages_tab.tree;
  tree.height = height;
  if cursor < tree.offset {
    tree.offset = cursor;
  } else if height > 0 && cursor >= tree.offset + height {
    tree.offset = cursor + 1 - height;
  }
  tree.offset = tree.offset.min(rows.len().saturating_sub(height));
  let offset = tree.offset;

  for (line, row) in rows.iter().skip(offset).take(height).enumerate() {
    let y = top + line as u16;
    let indent = (row.depth * 2) as u16;
    let marker = if !row.has_children {
      " "
    } else if row.open {
      glyphs.expanded
    } else {
      glyphs.collapsed
    };
    let selected = app.selected_topic.as_deref() == Some(row.id.as_str());
    let mut label_style = if selected {
      theme.active()
    } else if row.topic.is_none() {
      Style::new().fg(theme.secondary).add_modifier(Modifier::ITALIC)
    } else {
      Style::new()
    };
    let badge = badge(app, row.unread);
    let room = usize::from(inner.width)
      .saturating_sub(usize::from(indent) + 2)
      .saturating_sub(badge.width());
    // The empty leading segment of an absolute topic would be an empty label.
    let label = if row.label.is_empty() { "/" } else { row.label.as_str() };
    let label = truncate(label, room, glyphs.ellipsis);
    let mut marker_style = theme.muted();
    let mut badge_style = Style::new().fg(theme.primary);
    let at_cursor = focus == Focus::Tree && line + offset == cursor;
    if at_cursor {
      label_style = label_style.add_modifier(Modifier::REVERSED);
      marker_style = marker_style.add_modifier(Modifier::REVERSED);
      badge_style = badge_style.add_modifier(Modifier::REVERSED);
    }
    let text = Line::from(vec![
      Span::raw(" ".repeat(usize::from(indent))),
      Span::styled(format!("{marker} "), marker_style),
      Span::styled(label.clone(), label_style),
      Span::styled(badge.clone(), badge_style),
    ]);
    let rect = Rect::new(inner.x, y, inner.width, 1);
    frame.render_widget(text, rect);
    let marker_x = inner.x + indent.min(inner.width);
    let label_width = (2 + label.width() + badge.width()) as u16;
    app.hits.push((
      Rect::new(marker_x, y, label_width, 1).intersection(rect),
      Action::SelectTopic(row.id.clone()),
    ));
    if row.has_children {
      app.hits.push((
        Rect::new(marker_x, y, 2, 1).intersection(rect),
        Action::ToggleTopic(row.id.clone()),
      ));
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn node(id: &str, label: &str, topic: Option<&str>, unread: u32, children: Vec<TopicNode>) -> TopicNode {
    TopicNode {
      id: id.to_owned(),
      label: label.to_owned(),
      topic: topic.map(str::to_owned),
      unread,
      messages: 0,
      children,
    }
  }

  /// The tree of `TopicTree.test.tsx`.
  fn tree() -> Vec<TopicNode> {
    vec![node(
      "hiveme",
      "hiveme",
      None,
      3,
      vec![
        node(
          "hiveme/build",
          "build",
          Some("hiveme/build"),
          1,
          vec![node("hiveme/build/ci", "ci", Some("hiveme/build/ci"), 1, vec![])],
        ),
        node("hiveme/error", "error", Some("hiveme/error"), 2, vec![]),
        node("hiveme/info", "info", Some("hiveme/info"), 0, vec![]),
      ],
    )]
  }

  fn ids(nodes: &[TopicNode]) -> Vec<String> {
    nodes.iter().map(|node| node.id.clone()).collect()
  }

  #[test]
  fn a_blank_filter_keeps_everything() {
    assert_eq!(filter_nodes(&tree(), "   "), tree());
  }

  #[test]
  fn a_filter_keeps_a_parent_whose_child_matches() {
    let filtered = filter_nodes(&tree(), "ci");
    assert_eq!(ids(&filtered), ["hiveme"]);
    assert_eq!(ids(&filtered[0].children), ["hiveme/build"]);
    assert_eq!(ids(&filtered[0].children[0].children), ["hiveme/build/ci"]);
  }

  #[test]
  fn a_filter_matches_the_whole_path_without_regard_to_case() {
    assert_eq!(ids(&filter_nodes(&tree(), "hiveme/err")[0].children), ["hiveme/error"]);
    assert_eq!(filter_nodes(&tree(), "ERROR")[0].children.len(), 1);
    assert!(filter_nodes(&tree(), "nowhere").is_empty());
  }

  #[test]
  fn every_node_id_is_listed_in_order() {
    assert_eq!(
      all_node_ids(&tree()),
      [
        "hiveme",
        "hiveme/build",
        "hiveme/build/ci",
        "hiveme/error",
        "hiveme/info"
      ]
    );
  }

  #[test]
  fn hiveme_is_always_there_and_survives_a_filter_that_does_not_match_it() {
    let empty = with_startup(&[]);
    assert_eq!(ids(&empty), ["hiveme"]);
    assert_eq!(empty[0].topic.as_deref(), Some("hiveme"));

    let merged = with_startup(&tree());
    assert_eq!(merged.len(), 1, "a stored hiveme is not doubled");
    assert_eq!(merged[0].topic.as_deref(), Some("hiveme"));
    assert_eq!(merged[0].children.len(), 3, "and keeps its children");

    let mut others = vec![node("sensors", "sensors", Some("sensors"), 0, vec![])];
    others.extend(tree());
    let shown = visible(&with_startup(&others), "sensors");
    assert_eq!(ids(&shown), ["hiveme", "sensors"]);
    assert!(shown[0].children.is_empty());
  }

  #[test]
  fn the_first_tree_is_open_until_touched_and_a_filter_opens_what_it_found() {
    let topics = with_startup(&tree());
    let mut state = TreeState::default();
    state.sync_expansion(&topics);
    assert_eq!(state.rows(&topics).len(), 5, "fully open");

    state.set_open("hiveme/build", false);
    state.sync_expansion(&topics);
    assert_eq!(state.rows(&topics).len(), 4, "a touched tree keeps what the user did");

    state.filter = TextInput::new("ci");
    state.sync_expansion(&topics);
    let rows = state.rows(&topics);
    assert_eq!(
      rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
      ["hiveme", "hiveme/build", "hiveme/build/ci"]
    );
    assert_eq!(rows[2].parent.as_deref(), Some("hiveme/build"));
    assert_eq!(rows[2].depth, 2);
  }
}
