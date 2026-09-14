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

//! A one-line text field.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

/// The character a hidden password is drawn with.
const MASK: char = '*';

/// The text of one field and where its cursor is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
  text: String,
  /// In characters, not bytes, so that editing never splits a character.
  cursor: usize,
}

impl TextInput {
  /// A field holding `text`, with the cursor at its end.
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
  /// The keys are those of `docs/specs/tui.md`: the arrows, `Home` and `End`,
  /// `Backspace` and `Delete`, `Ctrl+A` and `Ctrl+E` to the start and the end, and
  /// `Ctrl+U` to clear what is before the cursor.
  pub fn handle(&mut self, key: KeyEvent) -> bool {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let length = self.text.chars().count();
    match key.code {
      KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
      KeyCode::Right => self.cursor = (self.cursor + 1).min(length),
      KeyCode::Home => self.cursor = 0,
      KeyCode::End => self.cursor = length,
      KeyCode::Char('a' | 'A') if control => self.cursor = 0,
      KeyCode::Char('e' | 'E') if control => self.cursor = length,
      KeyCode::Char('u' | 'U') if control => {
        if self.cursor == 0 {
          return false;
        }
        let start = self.byte_index(self.cursor);
        self.text.replace_range(..start, "");
        self.cursor = 0;
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
        if self.cursor >= length {
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

  /// Inserts pasted text at the cursor. A field is one line, so line breaks are
  /// dropped, which is also what a password copied with its newline needs.
  pub fn paste(&mut self, text: &str) -> bool {
    let line: String = text
      .chars()
      .filter(|character| !matches!(character, '\r' | '\n'))
      .collect();
    if line.is_empty() {
      return false;
    }
    self.insert(&line);
    true
  }

  /// Draws the text in `area` and places the terminal cursor when the field has the
  /// focus. A focused field is scrolled so the cursor stays in view; any other shows the
  /// start of its text, which is what tells one row of a table from the next.
  pub fn render(&self, frame: &mut Frame, area: Rect, style: Style, masked: bool, focused: bool) {
    if area.width == 0 || area.height == 0 {
      return;
    }
    let shown: Vec<char> = if masked {
      self.text.chars().map(|_| MASK).collect()
    } else {
      self.text.chars().collect()
    };
    let cells = |characters: &[char]| -> usize { characters.iter().map(|c| c.width().unwrap_or(0)).sum() };
    // Keep one cell after the cursor for the cursor itself.
    let room = usize::from(area.width).saturating_sub(1);
    let mut start = 0;
    let mut cursor_cells = cells(&shown[..self.cursor]);
    while focused && start < self.cursor && cursor_cells > room {
      cursor_cells -= shown[start].width().unwrap_or(0);
      start += 1;
    }
    let visible: String = shown[start..].iter().collect();
    frame.render_widget(Line::styled(visible, style), area);
    if focused {
      let offset = cursor_cells.min(room) as u16;
      frame.set_cursor_position(Position::new(area.x + offset, area.y));
    }
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

  fn press(input: &mut TextInput, code: KeyCode) -> bool {
    input.handle(KeyEvent::new(code, KeyModifiers::NONE))
  }

  fn control(input: &mut TextInput, letter: char) -> bool {
    input.handle(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::CONTROL))
  }

  #[test]
  fn typing_inserts_at_the_cursor() {
    let mut input = TextInput::new("host");
    assert!(press(&mut input, KeyCode::Char(':')));
    press(&mut input, KeyCode::Home);
    press(&mut input, KeyCode::Char('>'));
    assert_eq!(input.text(), ">host:");
  }

  #[test]
  fn the_editing_keys_move_and_delete_by_character() {
    let mut input = TextInput::new("añb");
    press(&mut input, KeyCode::Left);
    assert!(press(&mut input, KeyCode::Backspace));
    assert_eq!(input.text(), "ab");
    assert!(press(&mut input, KeyCode::Delete));
    assert_eq!(input.text(), "a");
    assert!(!press(&mut input, KeyCode::Delete), "nothing after the cursor");

    let mut input = TextInput::new("hello world");
    control(&mut input, 'a');
    assert!(!press(&mut input, KeyCode::Left), "moving changes no text");
    assert!(!control(&mut input, 'u'), "nothing before the cursor at the start");
    control(&mut input, 'e');
    for _ in 0..5 {
      press(&mut input, KeyCode::Left);
    }
    assert!(control(&mut input, 'u'));
    assert_eq!(input.text(), "world");
    assert!(!control(&mut input, 'u'), "nothing before the cursor");
  }

  #[test]
  fn a_paste_is_one_line() {
    let mut input = TextInput::default();
    assert!(input.paste("s3cret\r\n"));
    assert_eq!(input.text(), "s3cret");
    assert!(!input.paste("\n"));
  }

  #[test]
  fn a_long_text_scrolls_to_keep_the_cursor_in_view() {
    let mut terminal = Terminal::new(TestBackend::new(8, 1)).unwrap();
    let input = TextInput::new("abcdefghijkl");
    terminal
      .draw(|frame| input.render(frame, frame.area(), Style::new(), false, true))
      .unwrap();
    terminal.backend().assert_buffer_lines(["fghijkl "]);
    terminal.backend_mut().assert_cursor_position(Position::new(7, 0));
  }

  #[test]
  fn a_field_without_the_focus_shows_the_start_of_its_text() {
    let mut terminal = Terminal::new(TestBackend::new(8, 1)).unwrap();
    let input = TextInput::new("abcdefghijkl");
    terminal
      .draw(|frame| input.render(frame, frame.area(), Style::new(), false, false))
      .unwrap();
    terminal.backend().assert_buffer_lines(["abcdefgh"]);
  }

  #[test]
  fn a_masked_field_never_draws_its_text() {
    let mut terminal = Terminal::new(TestBackend::new(10, 1)).unwrap();
    let input = TextInput::new("s3cret");
    terminal
      .draw(|frame| input.render(frame, frame.area(), Style::new(), true, false))
      .unwrap();
    terminal.backend().assert_buffer_lines(["******    "]);
  }
  #[test]
  fn a_long_wide_character_field_scrolls_in_linear_time() {
    let input = TextInput::new(&"界".repeat(100000));
    let mut terminal = Terminal::new(TestBackend::new(8, 1)).unwrap();
    let start = std::time::Instant::now();
    terminal
      .draw(|frame| input.render(frame, frame.area(), Style::new(), false, true))
      .unwrap();
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
    terminal.backend().assert_buffer_lines(["界界界  "]);
  }
}
