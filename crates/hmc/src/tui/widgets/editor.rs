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

//! A multi-line text field: the composer's message box.
//!
//! The text wraps at the width of the field, character by character, so that the
//! cursor maps to exactly one cell. `Up` and `Down` move between the rows as they are
//! drawn, and `Home` and `End` go to the start and the end of the line the cursor is
//! on.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

/// The text of the field and where its cursor is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Editor {
  text: String,
  /// In characters, not bytes.
  cursor: usize,
}

impl Editor {
  /// A field holding `text`, with the cursor at its end.
  #[cfg(test)]
  pub fn new(text: &str) -> Self {
    Self {
      text: text.to_owned(),
      cursor: text.chars().count(),
    }
  }

  pub fn text(&self) -> &str {
    &self.text
  }

  /// Applies an editing key. Returns whether the text changed.
  ///
  /// `Ctrl+A` and `Ctrl+E` go to the start and the end of the line, and `Ctrl+U` clears
  /// the line before the cursor, as in a shell.
  pub fn handle(&mut self, key: KeyEvent) -> bool {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let characters: Vec<char> = self.text.chars().collect();
    let line_start = characters[..self.cursor]
      .iter()
      .rposition(|character| *character == '\n')
      .map_or(0, |index| index + 1);
    let line_end = characters[self.cursor..]
      .iter()
      .position(|character| *character == '\n')
      .map_or(characters.len(), |index| self.cursor + index);
    match key.code {
      KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
      KeyCode::Right => self.cursor = (self.cursor + 1).min(characters.len()),
      KeyCode::Home => self.cursor = line_start,
      KeyCode::End => self.cursor = line_end,
      KeyCode::Char('a' | 'A') if control => self.cursor = line_start,
      KeyCode::Char('e' | 'E') if control => self.cursor = line_end,
      KeyCode::Char('u' | 'U') if control => {
        if self.cursor == line_start {
          return false;
        }
        let (start, end) = (self.byte_index(line_start), self.byte_index(self.cursor));
        self.text.replace_range(start..end, "");
        self.cursor = line_start;
        return true;
      }
      KeyCode::Backspace => {
        if self.cursor == 0 {
          return false;
        }
        self.cursor -= 1;
        let index = self.byte_index(self.cursor);
        self.text.remove(index);
        return true;
      }
      KeyCode::Delete => {
        if self.cursor >= characters.len() {
          return false;
        }
        let index = self.byte_index(self.cursor);
        self.text.remove(index);
        return true;
      }
      KeyCode::Char(character) if !control && !key.modifiers.contains(KeyModifiers::ALT) => {
        self.insert(&character.to_string());
        return true;
      }
      _ => {}
    }
    false
  }

  /// Starts a new line at the cursor.
  pub fn newline(&mut self) -> bool {
    self.insert("\n");
    true
  }

  /// Inserts pasted text at the cursor, with its line breaks.
  pub fn paste(&mut self, text: &str) -> bool {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text.is_empty() {
      return false;
    }
    self.insert(&text);
    true
  }

  /// Moves the cursor to the row above or below, as the text is drawn `width` cells
  /// wide. Returns whether there was a row to move to.
  pub fn move_vertically(&mut self, down: bool, width: u16) -> bool {
    let segments = self.segments(usize::from(width));
    let (row, column) = self.cursor_cell(&segments);
    let target = if down {
      row + 1
    } else {
      match row.checked_sub(1) {
        Some(target) => target,
        None => return false,
      }
    };
    let Some(&(start, end)) = segments.get(target) else {
      return false;
    };
    let characters: Vec<char> = self.text.chars().collect();
    let mut cursor = start;
    let mut used = 0;
    while cursor < end {
      let cells = characters[cursor].width().unwrap_or(0);
      if used + cells > column {
        break;
      }
      used += cells;
      cursor += 1;
    }
    self.cursor = cursor;
    true
  }

  /// How many rows the text takes at `width` cells.
  pub fn rows(&self, width: u16) -> usize {
    self.segments(usize::from(width)).len()
  }

  /// Draws the text scrolled so the cursor's row is in view, and places the terminal
  /// cursor when the field has the focus.
  pub fn render(&self, frame: &mut Frame, area: Rect, style: Style, focused: bool) {
    if area.width == 0 || area.height == 0 {
      return;
    }
    let segments = self.segments(usize::from(area.width));
    let (row, column) = self.cursor_cell(&segments);
    let first = row.saturating_sub(usize::from(area.height) - 1);
    let characters: Vec<char> = self.text.chars().collect();
    for (offset, &(start, end)) in segments.iter().skip(first).take(usize::from(area.height)).enumerate() {
      let text: String = characters[start..end].iter().collect();
      let line = Rect::new(area.x, area.y + offset as u16, area.width, 1);
      frame.render_widget(Line::styled(text, style), line);
    }
    if focused {
      let x = (column as u16).min(area.width - 1);
      frame.set_cursor_position(Position::new(area.x + x, area.y + (row - first) as u16));
    }
  }

  /// The rows the text is drawn in, as ranges of characters. A line break ends a row
  /// and belongs to none.
  fn segments(&self, width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let mut segments = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (index, character) in self.text.chars().enumerate() {
      if character == '\n' {
        segments.push((start, index));
        start = index + 1;
        used = 0;
        continue;
      }
      let cells = character.width().unwrap_or(0);
      if used + cells > width && index > start {
        segments.push((start, index));
        start = index;
        used = 0;
      }
      used += cells;
    }
    segments.push((start, self.text.chars().count()));
    segments
  }

  /// The row and the cell column of the cursor.
  fn cursor_cell(&self, segments: &[(usize, usize)]) -> (usize, usize) {
    let characters: Vec<char> = self.text.chars().collect();
    for (row, &(start, end)) in segments.iter().enumerate() {
      // A cursor on the boundary of two rows of one line is drawn at the start of the
      // second; at the end of a line, it stays on the line.
      let continues = segments.get(row + 1).is_some_and(|&(next, _)| next == end);
      if self.cursor >= start && (self.cursor < end || (self.cursor == end && !continues)) {
        let column = characters[start..self.cursor]
          .iter()
          .map(|character| character.width().unwrap_or(0))
          .sum();
        return (row, column);
      }
    }
    (segments.len().saturating_sub(1), 0)
  }

  fn insert(&mut self, text: &str) {
    let index = self.byte_index(self.cursor);
    self.text.insert_str(index, text);
    self.cursor += text.chars().count();
  }

  fn byte_index(&self, cursor: usize) -> usize {
    self
      .text
      .char_indices()
      .nth(cursor)
      .map(|(index, _)| index)
      .unwrap_or(self.text.len())
  }
}

#[cfg(test)]
mod tests {
  use ratatui::Terminal;
  use ratatui::backend::TestBackend;

  use super::*;

  fn press(editor: &mut Editor, code: KeyCode) -> bool {
    editor.handle(KeyEvent::new(code, KeyModifiers::NONE))
  }

  #[test]
  fn lines_are_added_and_edited_where_the_cursor_is() {
    let mut editor = Editor::default();
    for character in "first".chars() {
      press(&mut editor, KeyCode::Char(character));
    }
    assert!(editor.newline());
    assert!(editor.paste("second\r\nthird"));
    assert_eq!(editor.text(), "first\nsecond\nthird");

    press(&mut editor, KeyCode::Home);
    assert!(
      press(&mut editor, KeyCode::Backspace),
      "joins the line with the one above"
    );
    assert_eq!(editor.text(), "first\nsecondthird");
    assert!(editor.handle(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)));
    assert_eq!(editor.text(), "first\nthird", "Ctrl+U clears only this line");
    editor.handle(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert!(press(&mut editor, KeyCode::Char('!')));
    assert_eq!(editor.text(), "first\nthird!");
  }

  #[test]
  fn up_and_down_follow_the_rows_as_drawn() {
    let mut editor = Editor::new("abcdefgh\nxy");
    // Eight characters at a width of five are two rows, then the second line.
    assert_eq!(editor.rows(5), 3);
    assert!(editor.move_vertically(false, 5));
    assert!(editor.move_vertically(false, 5));
    assert!(!editor.move_vertically(false, 5), "the first row has nothing above it");
    press(&mut editor, KeyCode::Char('>'));
    assert_eq!(editor.text(), "ab>cdefgh\nxy");
    assert!(editor.move_vertically(true, 5));
    assert!(editor.move_vertically(true, 5));
    press(&mut editor, KeyCode::Char('<'));
    assert_eq!(editor.text(), "ab>cdefgh\nxy<");
    assert!(!editor.move_vertically(true, 5));
  }

  #[test]
  fn a_long_text_scrolls_to_the_cursor_row() {
    let mut terminal = Terminal::new(TestBackend::new(4, 2)).unwrap();
    let editor = Editor::new("one\ntwo\nsix");
    terminal
      .draw(|frame| editor.render(frame, frame.area(), Style::new(), true))
      .unwrap();
    terminal.backend().assert_buffer_lines(["two ", "six "]);
    terminal.backend_mut().assert_cursor_position(Position::new(3, 1));
  }
}
