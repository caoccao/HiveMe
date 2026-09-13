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

//! The chat view, `MessageView.tsx` in cells: the rows of the selected subtree as
//! bubbles, oldest at the top.
//!
//! Only the rows on screen are laid out. Where the view is is either "following", pinned
//! to the newest row, or a row id and how many of its lines are scrolled off the top, so
//! that a page of older rows arriving above never moves what is being read. The height
//! of a row is kept per row, width, and expansion, since it takes parsing the payload to
//! know it.

use std::collections::HashMap;
use std::time::Instant;

use hiveme_core::i18n::{Locale, format, t, t_count, t_with};
use hiveme_core::message::{self, Level, Message, Parsed};
use hiveme_core::session::MessageRow;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget, Wrap};

use super::super::app::App;
use super::super::keys::Action;
use super::super::service::Service;
use super::super::theme::{Glyphs, Theme};
use super::super::widgets::{truncate, wrap};
use super::json_tree::{self, Expansion, Json, Kind};
use super::{Focus, Pane, detail};

/// The cells between the pane's border and a bubble.
pub const MARGIN: u16 = 1;

/// The narrowest a bubble is drawn, however short its text.
pub const MIN_BUBBLE_WIDTH: u16 = 18;

/// How many lines one notch of the wheel scrolls.
pub const WHEEL_LINES: i64 = 3;

/// How many byte pairs one line of the hex preview holds, as `formatHex` groups them.
const HEX_GROUPS_PER_LINE: usize = 16;

/// What a stored row is, read again for display. `parseRow` in `src/lib/message.ts`.
#[derive(Debug, Clone, PartialEq)]
pub enum Reading {
  /// An envelope, and the payload again as ordered JSON for its `data` tree.
  Envelope {
    message: Box<Message>,
    json: Option<Json>,
  },
  Json(Json),
  Text(String),
  /// The backend hands a payload that is not text over as hex.
  Bytes {
    hex: String,
    length: u64,
  },
}

/// Reads a row with the same lenient reader that stored it.
pub fn read(row: &MessageRow) -> Reading {
  if row.tier == "bytes" {
    return Reading::Bytes {
      hex: row.raw.clone(),
      length: row.raw_length,
    };
  }
  match message::parse(row.raw.as_bytes()) {
    Parsed::Envelope(message) => Reading::Envelope {
      message,
      json: Json::parse(&row.raw),
    },
    Parsed::RawJson(_) => Json::parse(&row.raw).map_or_else(|| Reading::Text(row.raw.clone()), Reading::Json),
    Parsed::RawText(text) => Reading::Text(text),
    Parsed::RawBytes(bytes) => Reading::Bytes {
      hex: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
      length: bytes.len() as u64,
    },
  }
}

impl Reading {
  /// The envelope, when the payload is one.
  pub fn envelope(&self) -> Option<&Message> {
    match self {
      Self::Envelope { message, .. } => Some(message),
      _ => None,
    }
  }

  /// The tree the bubble shows, with its name and the path its nodes start from: the
  /// `data` of an envelope, or the whole of a raw JSON payload.
  pub fn tree(&self, locale: Locale) -> Option<(&Json, Option<String>, &'static str)> {
    match self {
      Self::Envelope { message, json } if !message.is_encrypted() => json
        .as_ref()
        .and_then(|json| json.get("payload"))
        .and_then(|payload| payload.get("data"))
        .filter(|data| **data != Json::Null)
        .map(|data| (data, Some(t(locale, "messages.data")), "data")),
      Self::Json(value) => Some((value, None, "")),
      _ => None,
    }
  }
}

/// The name above an incoming bubble: the sender's name, then its id, never its app.
pub fn sender_label(reading: &Reading) -> Option<String> {
  let sender = reading.envelope()?.sender.as_ref()?;
  sender
    .name
    .as_deref()
    .filter(|name| !name.is_empty())
    .or(sender.id.as_deref().filter(|id| !id.is_empty()))
    .map(str::to_owned)
}

/// The topic of a row relative to the selected tree topic; empty on the topic itself.
pub fn relative_topic<'a>(topic: &'a str, selected: &str) -> &'a str {
  if topic == selected {
    return "";
  }
  topic
    .strip_prefix(selected)
    .and_then(|rest| rest.strip_prefix('/'))
    .unwrap_or(topic)
}

/// The level badge's text and the level whose colors it takes. A level this build does
/// not know shows as `info`, with the raw name kept: `catastrophe (Info)`.
pub fn level_label(locale: Locale, raw: Option<&str>) -> (String, Level) {
  let level = raw.map(Level::parse).unwrap_or_default();
  let displayed = level.displayed();
  let name = t(locale, &format!("levels.{}", displayed.as_str()));
  let label = match raw {
    Some(raw) if !raw.is_empty() && !level.is_known() => {
      t_with(locale, "messages.unknownLevel", &[("level", raw), ("fallback", &name)])
    }
    _ => name,
  };
  (label, displayed)
}

/// The hex preview, sixteen byte pairs a line.
pub fn hex_lines(hex: &str) -> Vec<String> {
  let pairs: Vec<&str> = hex
    .as_bytes()
    .chunks(2)
    .map(|pair| std::str::from_utf8(pair).unwrap_or(""))
    .collect();
  pairs.chunks(HEX_GROUPS_PER_LINE).map(|line| line.join(" ")).collect()
}

/// The day separator's text for a timestamp.
pub fn day_of(locale: Locale, ts: &str) -> String {
  format::local(ts).map_or_else(|| ts.to_owned(), |at| format::day(locale, &at))
}

/// What the view is drawn with.
#[derive(Debug, Clone, Copy)]
pub struct Look<'a> {
  pub locale: Locale,
  pub glyphs: Glyphs,
  pub theme: Theme,
  pub selected: &'a str,
}

/// One line inside a bubble, and the tree node it starts, if it starts one.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentLine {
  pub line: Line<'static>,
  /// The node's path and depth.
  pub node: Option<(String, usize)>,
}

/// The lines inside a bubble `width` cells wide, as `Bubble` renders each tier.
pub fn content(reading: &Reading, look: &Look, width: usize, expansion: Option<&Expansion>) -> Vec<ContentLine> {
  let mut lines = Vec::new();
  let push = |lines: &mut Vec<ContentLine>, text: &str, style: Style| {
    for part in wrap(text, width) {
      lines.push(ContentLine {
        line: Line::styled(part, style),
        node: None,
      });
    }
  };
  match reading {
    Reading::Envelope { message, .. } if message.is_encrypted() => {
      let kid = message.enc.as_ref().map_or("", |enc| enc.kid.as_str());
      let text = format!(
        "{} {}",
        look.glyphs.lock,
        t_with(look.locale, "messages.encrypted", &[("kid", kid)])
      );
      push(&mut lines, &text, Style::new());
    }
    Reading::Envelope { message, .. } => {
      let payload = message.payload.as_ref();
      if let Some(title) = payload
        .and_then(|payload| payload.title.as_deref())
        .filter(|title| !title.is_empty())
      {
        push(&mut lines, title, Style::new().add_modifier(Modifier::BOLD));
      }
      let body = payload.map_or("", |payload| payload.body.as_str());
      let tree = reading.tree(look.locale);
      if !body.is_empty() || (lines.is_empty() && tree.is_none()) {
        push(&mut lines, body, Style::new());
      }
      if let Some((value, name, path)) = tree {
        tree_lines(&mut lines, value, name.as_deref(), path, look, width, expansion);
      }
    }
    Reading::Json(value) => tree_lines(&mut lines, value, None, "", look, width, expansion),
    Reading::Text(text) => push(&mut lines, text, Style::new()),
    Reading::Bytes { hex, length } => {
      push(
        &mut lines,
        &t_count(look.locale, "messages.bytes", *length),
        Style::new().add_modifier(Modifier::DIM),
      );
      for line in hex_lines(hex) {
        push(&mut lines, &line, Style::new());
      }
    }
  }
  lines
}

fn tree_lines(
  lines: &mut Vec<ContentLine>,
  value: &Json,
  name: Option<&str>,
  path: &str,
  look: &Look,
  width: usize,
  expansion: Option<&Expansion>,
) {
  let dim = Style::new().add_modifier(Modifier::DIM);
  for entry in json_tree::entries(value, name, path, expansion) {
    let indent = "  ".repeat(entry.depth);
    let label = entry
      .name
      .as_deref()
      .map(|name| format!("{name}: "))
      .unwrap_or_default();
    let spans = match &entry.kind {
      Kind::Branch { open, size } => {
        let marker = if *open {
          look.glyphs.expanded
        } else {
          look.glyphs.collapsed
        };
        vec![
          Span::raw(format!("{indent}{marker} ")),
          Span::styled(format!("{label}{size}"), dim),
        ]
      }
      Kind::Leaf(json) => vec![
        Span::raw(format!("{indent}  ")),
        Span::styled(label, dim),
        Span::raw(json.clone()),
      ],
    };
    let line = Line::from(spans);
    let node = Some((entry.path.clone(), entry.depth));
    if line.width() <= width {
      lines.push(ContentLine { line, node });
      continue;
    }
    let text: String = line.spans.iter().map(|span| span.content.as_ref()).collect();
    for (index, part) in wrap(&text, width).into_iter().enumerate() {
      lines.push(ContentLine {
        line: Line::raw(part),
        node: if index == 0 { node.clone() } else { None },
      });
    }
  }
}

/// The width of the bubbles of a list `width` cells wide: at most 80 percent of it, at
/// least 18 cells.
pub fn widest_bubble(width: u16) -> u16 {
  let usable = width.saturating_sub(2 * MARGIN);
  (usable * 4 / 5).max(MIN_BUBBLE_WIDTH).min(usable)
}

/// The shape of one row that does not depend on its neighbors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
  header: bool,
  bubble_width: u16,
  content: u16,
}

/// The one message whose trees are navigated node by node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
  pub row_id: i64,
  /// Which tree node the cursor is on.
  pub cursor: usize,
  /// The first content line on screen.
  pub offset: usize,
}

/// Where the view is and what the user did to it, kept while another tab is shown.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
  /// The row the keys act on, by row id.
  pub focused: Option<i64>,
  /// Pinned to the newest row.
  pub follow: bool,
  /// When not following: the first row on screen and how many of its lines are above.
  pub top: Option<(i64, u16)>,
  /// Which tree nodes the user opened or closed, per row.
  pub expansion: HashMap<i64, Expansion>,
  pub detail: Option<Detail>,
  /// Where the list was drawn on the last frame.
  pub viewport: Rect,
  shapes: HashMap<i64, (u16, u64, Shape)>,
  /// Bumped when anything a shape depends on changes for every row: the language.
  generation: u64,
}

impl Default for ViewState {
  fn default() -> Self {
    Self {
      focused: None,
      follow: true,
      top: None,
      expansion: HashMap::new(),
      detail: None,
      // Until the first frame, a guess the size of the smallest terminal's list.
      viewport: Rect::new(0, 0, 54, 8),
      shapes: HashMap::new(),
      generation: 0,
    }
  }
}

impl ViewState {
  /// Back to the newest row, as a newly selected topic opens.
  pub fn reset(&mut self) {
    self.focused = None;
    self.follow = true;
    self.top = None;
    self.detail = None;
  }

  /// Forgets every height, after a change that affects all rows.
  pub fn invalidate(&mut self) {
    self.generation += 1;
    self.shapes.clear();
  }

  /// Forgets one row's height, after its trees were opened or closed.
  pub fn invalidate_row(&mut self, row_id: i64) {
    self.shapes.remove(&row_id);
  }

  fn shape(&mut self, row: &MessageRow, look: &Look) -> Shape {
    let width = self.viewport.width;
    if let Some((cached_width, generation, shape)) = self.shapes.get(&row.row_id)
      && *cached_width == width
      && *generation == self.generation
    {
      return *shape;
    }
    let reading = read(row);
    let widest = widest_bubble(width);
    let lines = content(
      &reading,
      look,
      usize::from(widest.saturating_sub(4)),
      self.expansion.get(&row.row_id),
    );
    let longest = lines.iter().map(|line| line.line.width()).max().unwrap_or(0) as u16;
    let shape = Shape {
      header: !row.outgoing && sender_label(&reading).is_some(),
      bubble_width: (longest + 4).clamp(MIN_BUBBLE_WIDTH.min(widest), widest),
      content: lines.len().max(1) as u16,
    };
    if self.shapes.len() > 4096 {
      self.shapes.clear();
    }
    self.shapes.insert(row.row_id, (width, self.generation, shape));
    shape
  }

  /// Whether a day separator comes before the row at `index`.
  fn starts_day(rows: &[MessageRow], index: usize, locale: Locale) -> bool {
    index == 0 || day_of(locale, &rows[index - 1].ts) != day_of(locale, &rows[index].ts)
  }

  /// The lines the row at `index` takes: the day separator, the sender, the bubble with
  /// its borders, and the reserved metadata row.
  fn height(&mut self, rows: &[MessageRow], index: usize, look: &Look) -> u16 {
    let shape = self.shape(&rows[index], look);
    u16::from(Self::starts_day(rows, index, look.locale)) + u16::from(shape.header) + shape.content + 2 + 1
  }

  /// The lines from `(index, skip)` to the end, counted up to `limit`.
  fn remaining(&mut self, rows: &[MessageRow], look: &Look, index: usize, skip: u16, limit: u32) -> u32 {
    let mut lines = 0u32;
    for current in index..rows.len() {
      lines += u32::from(self.height(rows, current, look));
      if lines >= limit + u32::from(skip) {
        break;
      }
    }
    lines.saturating_sub(u32::from(skip))
  }

  /// The first row on screen and the lines of it above the view.
  fn resolve(&mut self, rows: &[MessageRow], look: &Look) -> (usize, u16) {
    if rows.is_empty() {
      return (0, 0);
    }
    let visible = u32::from(self.viewport.height);
    if !self.follow
      && let Some((row_id, skip)) = self.top
    {
      let index = rows.partition_point(|row| row.row_id < row_id);
      if index < rows.len() {
        let skip = skip.min(self.height(rows, index, look).saturating_sub(1));
        if index == 0 || self.remaining(rows, look, index, skip, visible) >= visible {
          return (index, skip);
        }
      }
      // What is left below the anchor no longer fills the view, so it is the bottom.
      self.follow = true;
      self.top = None;
    }
    let mut index = rows.len();
    let mut used = 0u32;
    while index > 0 && used < visible {
      index -= 1;
      used += u32::from(self.height(rows, index, look));
    }
    if used <= visible {
      (index, 0)
    } else {
      (index, (used - visible) as u16)
    }
  }

  /// The rows on screen: index, the line they start on (negative when cut at the top),
  /// and height.
  pub fn placed(&mut self, rows: &[MessageRow], look: &Look) -> Vec<(usize, i32, u16)> {
    let (index, skip) = self.resolve(rows, look);
    let mut y = -i32::from(skip);
    let mut placed = Vec::new();
    for current in index..rows.len() {
      if y >= i32::from(self.viewport.height) {
        break;
      }
      let height = self.height(rows, current, look);
      placed.push((current, y, height));
      y += i32::from(height);
    }
    placed
  }

  /// Whether the oldest loaded row is at the top of the view.
  pub fn at_top(&mut self, rows: &[MessageRow], look: &Look) -> bool {
    self.resolve(rows, look) == (0, 0)
  }

  /// Scrolls by `lines`, up when negative. Reaching the bottom follows the newest row.
  pub fn scroll(&mut self, rows: &[MessageRow], look: &Look, lines: i64) {
    if rows.is_empty() {
      return;
    }
    let (mut index, skip) = self.resolve(rows, look);
    let mut skip = u32::from(skip);
    if lines < 0 {
      let mut remaining = lines.unsigned_abs() as u32;
      while remaining > 0 {
        if skip > 0 {
          let taken = skip.min(remaining);
          skip -= taken;
          remaining -= taken;
        } else if index > 0 {
          index -= 1;
          skip = u32::from(self.height(rows, index, look));
        } else {
          break;
        }
      }
    } else {
      skip += lines as u32;
      while index + 1 < rows.len() {
        let height = u32::from(self.height(rows, index, look));
        if skip < height {
          break;
        }
        skip -= height;
        index += 1;
      }
      skip = skip.min(u32::from(self.height(rows, index, look)).saturating_sub(1));
    }
    self.follow = false;
    self.top = Some((rows[index].row_id, skip as u16));
    let visible = u32::from(self.viewport.height);
    if lines > 0 && self.remaining(rows, look, index, skip as u16, visible) <= visible {
      self.follow = true;
      self.top = None;
    }
  }

  /// Scrolls just enough for the row at `index` to be on screen.
  pub fn reveal(&mut self, rows: &[MessageRow], look: &Look, index: usize) {
    if index >= rows.len() {
      return;
    }
    let (top, skip) = self.resolve(rows, look);
    let visible = u32::from(self.viewport.height);
    let height = self.height(rows, index, look);
    if index < top || (index == top && skip > 0) {
      self.follow = false;
      self.top = Some((rows[index].row_id, 0));
      return;
    }
    let mut lines = 0u32;
    for current in top..=index {
      lines += u32::from(self.height(rows, current, look));
    }
    if lines.saturating_sub(u32::from(skip)) <= visible {
      return;
    }
    if index + 1 == rows.len() {
      self.follow = true;
      self.top = None;
      return;
    }
    self.follow = false;
    self.top = Some((rows[index].row_id, 0));
    if u32::from(height) < visible {
      self.scroll(rows, look, -i64::from(visible - u32::from(height)));
    }
  }
}

impl<S: Service> App<S> {
  /// A key or a wheel notch while the list has the focus, or is under the pointer.
  pub(in crate::tui) fn list_action(&mut self, action: &Action, now: Instant) {
    let Some(topic) = self.selected_topic.clone() else {
      return;
    };
    match action {
      Action::CopyBody | Action::CopyRaw => return self.copy_focused(action, now),
      Action::Toggle => return self.toggle_focused_trees(),
      Action::Activate => {
        if let Some(row_id) = self.messages_tab.view.focused {
          self.messages_tab.view.detail = Some(Detail {
            row_id,
            cursor: 0,
            offset: 0,
          });
        }
        return;
      }
      _ => {}
    }

    // Moving past the oldest row, or paging at the top, asks for the page before it.
    let at_top = {
      let rows = self.messages.get(&topic).map_or(&[][..], Vec::as_slice);
      let look = self.look(&topic);
      let focused_first = self.messages_tab.view.focused.is_some()
        && rows.first().map(|row| row.row_id) == self.messages_tab.view.focused;
      let view = &mut self.messages_tab.view;
      match action {
        Action::PageUp => view.at_top(rows, &look),
        Action::Up => focused_first && view.at_top(rows, &look),
        _ => false,
      }
    };
    if at_top {
      self.load_older_messages(&topic, now);
    }

    let rows = self.messages.get(&topic).map_or(&[][..], Vec::as_slice);
    if rows.is_empty() {
      return;
    }
    let look = Look {
      locale: self.locale,
      glyphs: self.glyphs,
      theme: self.theme,
      selected: &topic,
    };
    let view = &mut self.messages_tab.view;
    let focused = view
      .focused
      .and_then(|row_id| rows.iter().position(|row| row.row_id == row_id));
    let page = i64::from(view.viewport.height.saturating_sub(1).max(1));
    let target = match action {
      Action::Up => Some(focused.map_or(rows.len() - 1, |index| index.saturating_sub(1))),
      Action::Down => Some(focused.map_or(rows.len() - 1, |index| (index + 1).min(rows.len() - 1))),
      Action::Home => Some(0),
      Action::End => {
        view.follow = true;
        view.top = None;
        Some(rows.len() - 1)
      }
      Action::PageUp | Action::PageDown => {
        view.scroll(rows, &look, if *action == Action::PageUp { -page } else { page });
        let placed = view.placed(rows, &look);
        let height = i32::from(view.viewport.height);
        let pick = if *action == Action::PageUp {
          placed.iter().find(|(_, y, _)| *y >= 0).or(placed.first())
        } else {
          placed
            .iter()
            .rev()
            .find(|(_, y, lines)| *y + i32::from(*lines) <= height)
            .or(placed.last())
        };
        view.focused = pick.map(|(index, _, _)| rows[*index].row_id);
        None
      }
      _ => None,
    };
    if let Some(index) = target {
      view.focused = Some(rows[index].row_id);
      view.reveal(rows, &look, index);
    }
  }

  /// A wheel notch over the list, which scrolls without moving the focus.
  pub(in crate::tui) fn wheel_list(&mut self, up: bool, now: Instant) {
    let Some(topic) = self.selected_topic.clone() else {
      return;
    };
    if up {
      let rows = self.messages.get(&topic).map_or(&[][..], Vec::as_slice);
      let look = self.look(&topic);
      if self.messages_tab.view.at_top(rows, &look) {
        self.load_older_messages(&topic, now);
      }
    }
    let rows = self.messages.get(&topic).map_or(&[][..], Vec::as_slice);
    let look = Look {
      locale: self.locale,
      glyphs: self.glyphs,
      theme: self.theme,
      selected: &topic,
    };
    self
      .messages_tab
      .view
      .scroll(rows, &look, if up { -WHEEL_LINES } else { WHEEL_LINES });
  }

  /// A click on a message.
  pub(in crate::tui) fn focus_message(&mut self, row_id: i64) {
    self.messages_tab.focus = Focus::List;
    self.messages_tab.view.focused = Some(row_id);
  }

  pub(in crate::tui) fn look<'a>(&self, selected: &'a str) -> Look<'a> {
    Look {
      locale: self.locale,
      glyphs: self.glyphs,
      theme: self.theme,
      selected,
    }
  }

  /// The focused row, when there is one on the selected topic.
  pub(in crate::tui) fn focused_row(&self) -> Option<&MessageRow> {
    let row_id = self
      .messages_tab
      .view
      .detail
      .as_ref()
      .map(|detail| detail.row_id)
      .or(self.messages_tab.view.focused)?;
    self.selected_messages().iter().find(|row| row.row_id == row_id)
  }

  /// `c` and `r`: the body or the raw payload on the clipboard, and the snackbar says so.
  pub(in crate::tui) fn copy_focused(&mut self, action: &Action, now: Instant) {
    let Some(row) = self.focused_row() else {
      return;
    };
    let (text, confirmation) = if *action == Action::CopyRaw {
      (row.raw.clone(), "messages.copiedJson")
    } else {
      (row.body.clone(), "messages.copiedBody")
    };
    match (self.copier)(&text) {
      Ok(()) => self.notify_info(t(self.locale, confirmation), now),
      Err(error) => self.notify_error(error, now),
    }
  }

  /// `Space` on a message: opens every tree in it, or closes them all when all are open.
  fn toggle_focused_trees(&mut self) {
    let Some(row) = self.focused_row().cloned() else {
      return;
    };
    let reading = read(&row);
    if let Some((value, _, path)) = reading.tree(self.locale) {
      let expansion = self.messages_tab.view.expansion.entry(row.row_id).or_default();
      json_tree::toggle_all(expansion, value, path);
      self.messages_tab.view.invalidate_row(row.row_id);
    }
  }
}

/// The metadata row of a focused bubble: the relative topic, the level, the QoS, the
/// retained marker, the newer-version chip, and the time.
pub fn metadata(row: &MessageRow, reading: &Reading, look: &Look) -> Vec<Span<'static>> {
  let theme = look.theme;
  let mut spans = Vec::new();
  let mut push = |span: Span<'static>| {
    if !spans.is_empty() {
      spans.push(Span::raw("  "));
    }
    spans.push(span);
  };
  let relative = relative_topic(&row.topic, look.selected);
  if !relative.is_empty() {
    push(Span::styled(relative.to_owned(), theme.muted()));
  }
  let (label, level) = level_label(look.locale, row.level.as_deref());
  push(Span::styled(
    label,
    theme
      .severity(&level)
      .map_or(Style::new(), |color| Style::new().fg(color)),
  ));
  push(Span::raw(t_with(
    look.locale,
    "messages.qos",
    &[("qos", &row.qos.to_string())],
  )));
  if row.retain {
    push(Span::raw(look.glyphs.pin));
  }
  if reading.envelope().is_some_and(Message::is_newer_version) {
    push(Span::styled(
      t(look.locale, "messages.newerVersion"),
      Style::new().fg(theme.warning),
    ));
  }
  let time = format::local(&row.ts).map_or_else(|| row.ts.clone(), |at| format::time(look.locale, &at));
  push(Span::styled(time, theme.muted()));
  spans
}

/// Draws one row into `buffer`, which is exactly the row's height.
fn draw_row(
  buffer: &mut Buffer,
  row: &MessageRow,
  day: Option<String>,
  shape: Shape,
  look: &Look,
  view: &ViewState,
  focused: bool,
) -> Rect {
  let theme = look.theme;
  let area = buffer.area;
  let mut y = 0;
  if let Some(day) = day {
    Line::styled(day, theme.muted())
      .centered()
      .render(Rect::new(0, y, area.width, 1), buffer);
    y += 1;
  }
  let reading = read(row);
  let x = if row.outgoing {
    area.width.saturating_sub(MARGIN + shape.bubble_width)
  } else {
    MARGIN
  };
  if shape.header
    && let Some(name) = sender_label(&reading)
  {
    let room = usize::from(area.width.saturating_sub(x + 2));
    Line::styled(
      truncate(&name, room, look.glyphs.ellipsis),
      theme.muted().add_modifier(Modifier::BOLD),
    )
    .render(Rect::new(x + 2, y, area.width.saturating_sub(x + 2), 1), buffer);
    y += 1;
  }

  let bubble = Rect::new(x, y, shape.bubble_width, shape.content + 2);
  let level = row.level.as_deref().map(Level::parse).unwrap_or_default().displayed();
  let severity = theme.severity(&level);
  let fill = theme.bubble(severity, row.outgoing);
  let border = severity.unwrap_or(if focused { theme.primary } else { theme.muted });
  let mut border_style = fill.fg(border);
  if focused {
    border_style = border_style.add_modifier(Modifier::BOLD);
  }
  Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(border_style)
    .style(fill)
    .render(bubble, buffer);
  let lines = content(
    &reading,
    look,
    usize::from(shape.bubble_width.saturating_sub(4)),
    view.expansion.get(&row.row_id),
  );
  for (index, line) in lines.iter().enumerate().take(usize::from(shape.content)) {
    line.line.clone().render(
      Rect::new(x + 2, y + 1 + index as u16, shape.bubble_width.saturating_sub(4), 1),
      buffer,
    );
  }

  if focused {
    let spans = metadata(row, &reading, look);
    let line = Line::from(spans);
    // Aligned to the bubble's right edge, and running past it to the right when the
    // bubble is narrower than the row.
    let width = line.width() as u16;
    let start = (x + shape.bubble_width).saturating_sub(width).max(MARGIN.min(x));
    let end = (start + width).min(area.width.saturating_sub(MARGIN));
    line.render(
      Rect::new(start, y + shape.content + 2, end.saturating_sub(start), 1),
      buffer,
    );
  }
  bubble
}

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  app.hits.push((area, Action::FocusPane(Pane::List)));
  if app.messages_tab.view.viewport.width != area.width {
    app.messages_tab.view.invalidate();
  }
  app.messages_tab.view.viewport = area;
  let theme = app.theme;
  let Some(topic) = app.selected_topic.clone() else {
    return centered(frame, area, &t(app.locale, "messages.selectTopic"), theme.muted());
  };
  if app.messages_tab.view.detail.is_some() {
    return detail::render(app, frame, area);
  }
  let rows = app.messages.get(&topic).map_or(&[][..], Vec::as_slice);
  if rows.is_empty() {
    return centered(frame, area, &t(app.locale, "messages.empty"), theme.muted());
  }
  let look = Look {
    locale: app.locale,
    glyphs: app.glyphs,
    theme,
    selected: &topic,
  };
  let list_focused = app.messages_tab.focus == Focus::List;
  let view = &mut app.messages_tab.view;
  let placed = view.placed(rows, &look);
  for (index, y, height) in placed {
    let row = &rows[index];
    let shape = view.shape(row, &look);
    let day = ViewState::starts_day(rows, index, look.locale).then(|| day_of(look.locale, &row.ts));
    let focused = list_focused && view.focused == Some(row.row_id);
    let mut buffer = Buffer::empty(Rect::new(0, 0, area.width, height));
    buffer.set_style(buffer.area, theme.base());
    let bubble = draw_row(&mut buffer, row, day, shape, &look, view, focused);

    for line in 0..height {
      let target = y + i32::from(line);
      if target < 0 || target >= i32::from(area.height) {
        continue;
      }
      for column in 0..area.width {
        frame.buffer_mut()[(area.x + column, area.y + target as u16)] = buffer[(column, line)].clone();
      }
    }
    let top = (y + i32::from(bubble.y)).max(0);
    let bottom = (y + i32::from(bubble.bottom()) + 1).min(i32::from(area.height));
    if bottom > top {
      app.hits.push((
        Rect::new(
          area.x + bubble.x,
          area.y + top as u16,
          bubble.width,
          (bottom - top) as u16,
        ),
        Action::FocusMessage(row.row_id),
      ));
    }
  }
}

fn centered(frame: &mut Frame, area: Rect, text: &str, style: Style) {
  let middle = area.centered_vertically(Constraint::Length(1));
  frame.render_widget(
    Paragraph::new(text.to_owned())
      .style(style)
      .centered()
      .wrap(Wrap { trim: true }),
    middle,
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_relative_topic_is_empty_on_the_selection_and_needs_a_slash_boundary() {
    assert_eq!(relative_topic("hiveme/a/b/c", "hiveme/a"), "b/c");
    assert_eq!(relative_topic("hiveme/a", "hiveme/a"), "");
    assert_eq!(relative_topic("hiveme/ab", "hiveme/a"), "hiveme/ab");
  }

  #[test]
  fn an_unknown_level_keeps_its_name_beside_the_level_it_displays_as() {
    assert_eq!(
      level_label(Locale::EnUs, Some("warn")),
      ("Warn".to_owned(), Level::Warn)
    );
    assert_eq!(level_label(Locale::EnUs, None), ("Info".to_owned(), Level::Info));
    assert_eq!(
      level_label(Locale::EnUs, Some("catastrophe")),
      ("catastrophe (Info)".to_owned(), Level::Info)
    );
    assert_eq!(level_label(Locale::Ja, Some("custom-level")).0, "custom-level（情報）");
  }

  #[test]
  fn bytes_are_grouped_in_pairs_sixteen_to_a_line() {
    assert_eq!(hex_lines("fffe00"), ["ff fe 00"]);
    let long = "ab".repeat(17);
    assert_eq!(hex_lines(&long).len(), 2);
    assert_eq!(hex_lines(&long)[1], "ab");
  }

  #[test]
  fn a_bubble_is_at_most_four_fifths_of_the_list_and_at_least_eighteen_cells() {
    assert_eq!(widest_bubble(102), 80);
    assert_eq!(widest_bubble(20), 18);
    assert_eq!(widest_bubble(10), 8, "never wider than the list");
  }
}
