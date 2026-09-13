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

//! A settings panel as rows of labels and controls.
//!
//! A category builds its [`Form`] from the config on every frame, so that what is drawn,
//! the order `Tab` moves in, and what a key does to the focused control all come from
//! one description. [`layout`] places the rows for a width, and [`draw`] draws the rows
//! that are scrolled into view.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::{Field, Focus, SettingsState};
use crate::tui::keys::Action;
use crate::tui::theme::{Glyphs, Theme};
use crate::tui::widgets::{truncate, wrap};

/// The width of a one-line field that is not stretched.
const INPUT_WIDTH: u16 = 12;

/// The cells between the label column and the controls.
const LABEL_GAP: u16 = 2;

/// What a control is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
  /// A one-line text field, whose text lives in the tab's state. `masked` hides it.
  Input {
    masked: bool,
  },
  /// A select showing its value, which opens a popup of choices.
  Select {
    text: String,
    style: Style,
  },
  /// A checkbox, or a switch of the GUI, with its label.
  Check {
    checked: bool,
    text: String,
  },
  /// One choice of a radio row.
  Radio {
    checked: bool,
    text: String,
    index: usize,
  },
  Button {
    text: String,
    enabled: bool,
  },
  /// Text that takes no focus, such as a table header.
  Label {
    text: String,
    style: Style,
  },
}

impl Kind {
  /// The cells the control takes when it is not stretched.
  fn natural_width(&self, glyphs: &Glyphs) -> u16 {
    let width = match self {
      Self::Input { .. } => usize::from(INPUT_WIDTH),
      Self::Select { text, .. } => text.width() + glyphs.expanded.width() + 3,
      Self::Check { text, .. } if text.is_empty() => glyphs.checked.width(),
      Self::Check { text, .. } => glyphs.checked.width() + 1 + text.width(),
      Self::Radio { text, .. } => glyphs.radio_on.width() + 1 + text.width(),
      Self::Button { text, .. } => text.width() + 2,
      Self::Label { text, .. } => text.width(),
    };
    width as u16
  }

  /// Whether `Tab` stops on it. A button that cannot be used right now is skipped, as the
  /// GUI's disabled buttons are.
  pub fn focusable(&self) -> bool {
    !matches!(self, Self::Label { .. } | Self::Button { enabled: false, .. })
  }
}

/// How wide a control is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
  /// As wide as its content.
  Fit,
  /// Exactly this many cells, so the columns of a table line up.
  Cells(u16),
  /// A share of what the fixed controls of its line leave.
  Fill(u16),
}

/// A control, and the field it edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
  pub field: Option<Field>,
  pub kind: Kind,
  pub size: Size,
}

impl Item {
  pub fn input(field: Field, masked: bool, size: Size) -> Self {
    Self {
      field: Some(field),
      kind: Kind::Input { masked },
      size,
    }
  }

  pub fn select(field: Field, text: String, style: Style, size: Size) -> Self {
    Self {
      field: Some(field),
      kind: Kind::Select { text, style },
      size,
    }
  }

  pub fn check(field: Field, checked: bool, text: String, size: Size) -> Self {
    Self {
      field: Some(field),
      kind: Kind::Check { checked, text },
      size,
    }
  }

  pub fn button(field: Field, text: String, enabled: bool, size: Size) -> Self {
    Self {
      field: Some(field),
      kind: Kind::Button { text, enabled },
      size,
    }
  }

  pub fn label(text: String, style: Style, size: Size) -> Self {
    Self {
      field: None,
      kind: Kind::Label { text, style },
      size,
    }
  }

  /// A radio row: one item per choice, all editing `field`.
  pub fn radios(field: Field, choices: Vec<String>, selected: usize) -> Vec<Self> {
    choices
      .into_iter()
      .enumerate()
      .map(|(index, text)| Self {
        field: Some(field),
        kind: Kind::Radio {
          checked: index == selected,
          text,
          index,
        },
        size: Size::Fit,
      })
      .collect()
  }
}

/// A row of a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
  Blank,
  /// The title of a group of fields, `SectionHeader` in the GUI, with its action at the
  /// right end.
  Heading {
    title: String,
    action: Option<Item>,
  },
  /// Text that wraps, under the controls when `indent` is set and it fits there on one
  /// line, and across the whole panel otherwise.
  Note {
    text: String,
    indent: bool,
  },
  /// Controls, after a label when there is one. Controls that do not fit move to the
  /// next row, unless the line stretches one of them, as a table row does.
  Line {
    label: Option<String>,
    items: Vec<Item>,
  },
}

impl Row {
  pub fn line(label: String, items: Vec<Item>) -> Self {
    Self::Line {
      label: Some(label),
      items,
    }
  }

  pub fn controls(items: Vec<Item>) -> Self {
    Self::Line { label: None, items }
  }

  pub fn note(text: String) -> Self {
    Self::Note { text, indent: false }
  }

  pub fn hint(text: String) -> Self {
    Self::Note { text, indent: true }
  }

  pub fn heading(title: String) -> Self {
    Self::Heading { title, action: None }
  }
}

/// A whole panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Form {
  pub rows: Vec<Row>,
}

impl Form {
  fn items(&self) -> impl Iterator<Item = &Item> {
    self.rows.iter().flat_map(|row| -> Box<dyn Iterator<Item = &Item>> {
      match row {
        Row::Heading {
          action: Some(action), ..
        } => Box::new(std::iter::once(action)),
        Row::Line { items, .. } => Box::new(items.iter()),
        _ => Box::new(std::iter::empty()),
      }
    })
  }

  /// The fields `Tab` stops on, in order, a radio row once.
  pub fn fields(&self) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();
    for item in self.items().filter(|item| item.kind.focusable()) {
      if let Some(field) = item.field
        && !fields.contains(&field)
      {
        fields.push(field);
      }
    }
    fields
  }

  /// What the control of a field is: the first one, for a radio row.
  pub fn kind(&self, field: Field) -> Option<&Kind> {
    self
      .items()
      .find(|item| item.field == Some(field))
      .map(|item| &item.kind)
  }

  /// The fields that are text fields.
  pub fn inputs(&self) -> Vec<Field> {
    self
      .items()
      .filter(|item| matches!(item.kind, Kind::Input { .. }))
      .filter_map(|item| item.field)
      .collect()
  }
}

/// Where a control was placed, relative to the top left of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
  pub row: usize,
  /// The index into a line's items, or `None` for a heading's action.
  pub item: Option<usize>,
  pub x: u16,
  pub y: u16,
  pub width: u16,
}

/// A form placed for a width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
  /// The first row and the height of each row of the form.
  pub rows: Vec<(u16, u16)>,
  pub placements: Vec<Placement>,
  /// The wrapped lines of each note, by row.
  notes: Vec<Vec<String>>,
  pub label_width: u16,
  /// Where the controls of a labeled line start.
  pub field_x: u16,
  pub height: u16,
}

impl Layout {
  /// Where the first control of `field` is.
  pub fn place(&self, form: &Form, field: Field) -> Option<Placement> {
    self
      .placements
      .iter()
      .copied()
      .find(|placement| item(form, placement).field == Some(field))
  }
}

fn item<'a>(form: &'a Form, placement: &Placement) -> &'a Item {
  match (&form.rows[placement.row], placement.item) {
    (
      Row::Heading {
        action: Some(action), ..
      },
      None,
    ) => action,
    (Row::Line { items, .. }, Some(index)) => &items[index],
    _ => unreachable!("a placement points at a control"),
  }
}

/// Places the rows of `form` in a panel `width` cells wide.
pub fn layout(form: &Form, width: u16, glyphs: &Glyphs) -> Layout {
  let label_width = form
    .rows
    .iter()
    .filter_map(|row| match row {
      Row::Line { label: Some(label), .. } => Some(label.width() as u16),
      _ => None,
    })
    .max()
    .map_or(0, |widest| widest.min(width * 2 / 5));
  let field_x = if label_width > 0 { label_width + LABEL_GAP } else { 0 };

  let mut rows = Vec::new();
  let mut placements = Vec::new();
  let mut notes = Vec::new();
  let mut y = 0u16;
  for (index, row) in form.rows.iter().enumerate() {
    let mut lines = Vec::new();
    let height = match row {
      Row::Blank => 1,
      Row::Heading { action, .. } => {
        if let Some(action) = action {
          let cells = size_of(action, glyphs).min(width);
          placements.push(Placement {
            row: index,
            item: None,
            x: width - cells,
            y,
            width: cells,
          });
        }
        1
      }
      Row::Note { text, indent } => {
        lines = wrap(text, usize::from(width.saturating_sub(field_x)));
        if !*indent || lines.len() > 1 {
          lines = wrap(text, usize::from(width));
        }
        lines.len() as u16
      }
      Row::Line { label, items } => {
        let x = if label.is_some() { field_x } else { 0 };
        let room = width.saturating_sub(x);
        let placed = place_line(items, room, glyphs);
        let height = placed.iter().map(|(row, _, _)| row + 1).max().unwrap_or(1);
        for (item, (line, column, cells)) in placed.into_iter().enumerate() {
          placements.push(Placement {
            row: index,
            item: Some(item),
            x: x + column,
            y: y + line,
            width: cells,
          });
        }
        height
      }
    };
    notes.push(lines);
    rows.push((y, height));
    y += height;
  }
  Layout {
    rows,
    placements,
    notes,
    label_width,
    field_x,
    height: y,
  }
}

fn size_of(item: &Item, glyphs: &Glyphs) -> u16 {
  match item.size {
    Size::Cells(cells) => cells,
    Size::Fit | Size::Fill(_) => item.kind.natural_width(glyphs),
  }
}

/// Each control's row within the line, its column, and its width.
fn place_line(items: &[Item], room: u16, glyphs: &Glyphs) -> Vec<(u16, u16, u16)> {
  let stretches = items.iter().any(|item| matches!(item.size, Size::Fill(_)));
  if stretches {
    // One row: the fixed controls keep their width and the rest is shared by weight.
    let gaps = items.len().saturating_sub(1) as u16;
    let fixed: u16 = items
      .iter()
      .filter(|item| !matches!(item.size, Size::Fill(_)))
      .map(|item| size_of(item, glyphs))
      .sum();
    let weights: u16 = items
      .iter()
      .map(|item| match item.size {
        Size::Fill(weight) => weight.max(1),
        _ => 0,
      })
      .sum();
    let mut left = room.saturating_sub(fixed + gaps);
    let mut weights_left = weights;
    let mut column = 0u16;
    let mut placed = Vec::new();
    for item in items {
      let cells = match item.size {
        Size::Fill(weight) => {
          let weight = weight.max(1);
          let share = if weights_left == weight {
            left
          } else {
            (u32::from(left) * u32::from(weight) / u32::from(weights_left)) as u16
          };
          left -= share;
          weights_left -= weight;
          share.max(1)
        }
        _ => size_of(item, glyphs),
      };
      let cells = cells.min(room.saturating_sub(column));
      placed.push((0, column, cells));
      column = (column + cells + 1).min(room);
    }
    placed
  } else {
    // Controls of their own width flow onto the next row when they do not fit.
    let mut placed = Vec::new();
    let (mut line, mut column) = (0u16, 0u16);
    for item in items {
      let cells = size_of(item, glyphs).min(room);
      if column > 0 && column + 2 + cells > room {
        line += 1;
        column = 0;
      } else if column > 0 {
        column += 2;
      }
      placed.push((line, column, cells));
      column += cells;
    }
    placed
  }
}

/// Where a panel is drawn, how far it is scrolled, and its colors and glyphs.
#[derive(Debug, Clone, Copy)]
pub struct Look {
  pub area: Rect,
  pub scroll: u16,
  pub theme: Theme,
  pub glyphs: Glyphs,
}

/// Draws the rows of `form` that are in view, and records where a click lands.
pub fn draw(
  frame: &mut Frame,
  form: &Form,
  layout: &Layout,
  look: Look,
  state: &SettingsState,
  hits: &mut Vec<(Rect, Action)>,
) {
  let Look {
    area,
    scroll,
    theme,
    glyphs,
  } = look;
  let focused_field = match state.focus {
    Focus::Field(field) => Some(field),
    Focus::Categories => None,
  };
  let screen_y = |y: u16| -> Option<u16> { (y >= scroll && y - scroll < area.height).then(|| area.y + y - scroll) };

  for (index, (row, &(top, _))) in form.rows.iter().zip(&layout.rows).enumerate() {
    match row {
      Row::Blank => {}
      Row::Heading { title, action } => {
        let Some(y) = screen_y(top) else {
          continue;
        };
        let end = match action {
          Some(_) => layout
            .placements
            .iter()
            .find(|placement| placement.row == index)
            .map_or(area.width, |placement| placement.x.saturating_sub(1)),
          None => area.width,
        };
        let title = truncate(title, usize::from(end.saturating_sub(4)), glyphs.ellipsis);
        let rule = usize::from(end).saturating_sub(title.width() + 3);
        let line = Line::from(vec![
          Span::styled("─ ", theme.muted()),
          Span::styled(title, theme.active()),
          Span::raw(" "),
          Span::styled("─".repeat(rule), theme.muted()),
        ]);
        frame.render_widget(line, Rect::new(area.x, y, end.min(area.width), 1));
      }
      Row::Note { indent, .. } => {
        let lines = &layout.notes[index];
        let x = if *indent && lines.len() == 1 { layout.field_x } else { 0 };
        for (offset, text) in lines.iter().enumerate() {
          if let Some(y) = screen_y(top + offset as u16) {
            frame.render_widget(
              Line::styled(text.clone(), theme.muted()),
              Rect::new(area.x + x, y, area.width.saturating_sub(x), 1),
            );
          }
        }
      }
      Row::Line { label, items } => {
        let Some(label) = label else {
          continue;
        };
        let Some(y) = screen_y(top) else {
          continue;
        };
        let focused = items
          .iter()
          .any(|item| item.field.is_some() && item.field == focused_field);
        let style = if focused { theme.active() } else { theme.muted() };
        let text = truncate(label, usize::from(layout.label_width), glyphs.ellipsis);
        frame.render_widget(
          Line::styled(text, style),
          Rect::new(area.x, y, layout.label_width.min(area.width), 1),
        );
      }
    }
  }

  for placement in &layout.placements {
    let Some(y) = screen_y(placement.y) else {
      continue;
    };
    let item = item(form, placement);
    let rect = Rect::new(area.x + placement.x, y, placement.width, 1).intersection(area);
    if rect.width == 0 {
      continue;
    }
    let focused = item.field.is_some() && item.field == focused_field;
    let reversed = |style: Style, on: bool| {
      if on {
        style.add_modifier(Modifier::REVERSED)
      } else {
        style
      }
    };
    let room = usize::from(rect.width);
    match &item.kind {
      Kind::Input { masked } => {
        let style = Style::new().add_modifier(Modifier::UNDERLINED);
        frame.render_widget(Line::styled(" ".repeat(room), style), rect);
        if let Some(input) = item.field.and_then(|field| state.input(field)) {
          input.render(frame, rect, style, *masked, focused);
        }
      }
      Kind::Select { text, style } => {
        let inner = room.saturating_sub(glyphs.expanded.width() + 3);
        let text = format!("[{} {}]", truncate(text, inner, glyphs.ellipsis), glyphs.expanded);
        frame.render_widget(Line::styled(text, reversed(*style, focused)), rect);
      }
      Kind::Check { checked, text } => {
        let mark = if *checked { glyphs.checked } else { glyphs.unchecked };
        let text = if text.is_empty() {
          mark.to_owned()
        } else {
          format!("{mark} {text}")
        };
        let text = truncate(&text, room, glyphs.ellipsis);
        frame.render_widget(Line::styled(text, reversed(Style::new(), focused)), rect);
      }
      Kind::Radio { checked, text, .. } => {
        let mark = if *checked { glyphs.radio_on } else { glyphs.radio_off };
        let text = truncate(&format!("{mark} {text}"), room, glyphs.ellipsis);
        frame.render_widget(Line::styled(text, reversed(Style::new(), focused && *checked)), rect);
      }
      Kind::Button { text, enabled } => {
        let style = if *enabled {
          Style::new().fg(theme.primary)
        } else {
          theme.disabled()
        };
        let text = format!("[{}]", truncate(text, room.saturating_sub(2), glyphs.ellipsis));
        frame.render_widget(Line::styled(text, reversed(style, focused)), rect);
      }
      Kind::Label { text, style } => {
        frame.render_widget(Line::styled(truncate(text, room, glyphs.ellipsis), *style), rect);
      }
    }
    if !item.kind.focusable() {
      continue;
    }
    let Some(field) = item.field else {
      continue;
    };
    let action = match item.kind {
      Kind::Radio { index, .. } => Action::SettingsChoice(field, index),
      _ => Action::SettingsField(field),
    };
    hits.push((rect, action));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const GLYPHS: Glyphs = Glyphs::UNICODE;

  #[test]
  fn a_table_row_stretches_its_columns_and_keeps_the_fixed_ones() {
    let items = vec![
      Item::input(Field::RuleId(0), false, Size::Fill(1)),
      Item::input(Field::RuleTopic(0), false, Size::Fill(3)),
      Item::button(Field::RemoveRule(0), "✕".to_owned(), true, Size::Cells(3)),
    ];
    // 25 cells: 3 fixed, 2 gaps, 20 shared one to three.
    assert_eq!(place_line(&items, 25, &GLYPHS), vec![(0, 0, 5), (0, 6, 15), (0, 22, 3)]);
  }

  #[test]
  fn controls_of_their_own_width_flow_onto_the_next_row() {
    let items = Item::radios(
      Field::Mode,
      vec!["Auto Mode".to_owned(), "Light Mode".to_owned(), "Dark Mode".to_owned()],
      0,
    );
    // "(●) Auto Mode" is 13 cells, "(●) Light Mode" 14, with two cells between them.
    assert_eq!(
      place_line(&items, 80, &GLYPHS),
      vec![(0, 0, 13), (0, 15, 14), (0, 31, 13)]
    );
    assert_eq!(
      place_line(&items, 30, &GLYPHS),
      vec![(0, 0, 13), (0, 15, 14), (1, 0, 13)]
    );
  }

  #[test]
  fn the_labels_share_one_column_and_notes_wrap_under_the_controls() {
    let form = Form {
      rows: vec![
        Row::line("URL".to_owned(), vec![Item::input(Field::Url, false, Size::Fill(1))]),
        Row::hint("Connects to mqtts://host, on port 8883.".to_owned()),
        Row::line(
          "Username".to_owned(),
          vec![Item::input(Field::Username, false, Size::Fill(1))],
        ),
        Row::Heading {
          title: "Connection".to_owned(),
          action: Some(Item::button(Field::CopyCliSetup, "Copy".to_owned(), false, Size::Fit)),
        },
      ],
    };
    let layout = layout(&form, 30, &GLYPHS);
    assert_eq!(layout.label_width, 8);
    assert_eq!(layout.field_x, 10);
    // The hint would take three lines in the 20 cells after the labels, so it takes the
    // whole width, where it needs two.
    assert_eq!(layout.rows, vec![(0, 1), (1, 2), (3, 1), (4, 1)]);
    assert_eq!(layout.height, 5);
    let username = layout.place(&form, Field::Username).unwrap();
    assert_eq!((username.x, username.y, username.width), (10, 3, 20));
    let copy = layout.place(&form, Field::CopyCliSetup).unwrap();
    assert_eq!((copy.x, copy.y, copy.width), (24, 4, 6));
    assert_eq!(
      form.fields(),
      vec![Field::Url, Field::Username],
      "a disabled button is skipped"
    );
    assert_eq!(form.inputs(), vec![Field::Url, Field::Username]);
  }
}
