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

//! The form controls ratatui does not have.
//!
//! `tui-textarea` has not caught up with ratatui 0.30, so the controls are written
//! here; see the deviations in `docs/specs/app.md`. The one-line text field came with
//! the shell, the multi-line editor with the composer. The selects, checkboxes, radio
//! rows, and table rows of the Settings panels are rows of `settings/form.rs`, the one
//! place that uses them.

mod editor;
mod text_input;

pub use editor::Editor;
pub use text_input::TextInput;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Cuts `text` to at most `width` cells, ending it with `ellipsis` when it was cut.
///
/// Measured in display cells, so a CJK label is never cut in the middle of a glyph.
pub fn truncate(text: &str, width: usize, ellipsis: &str) -> String {
  if text.width() <= width {
    return text.to_owned();
  }
  let room = width.saturating_sub(ellipsis.width());
  if room == 0 {
    return ellipsis.chars().take(width).collect();
  }
  let mut out = String::new();
  let mut used = 0;
  for character in text.chars() {
    let cells = character.width().unwrap_or(0);
    if used + cells > room {
      break;
    }
    used += cells;
    out.push(character);
  }
  out.push_str(ellipsis);
  out
}

/// Splits `text` into lines no wider than `width` cells.
///
/// Every line break of the text starts a new line. A line that is too wide is broken
/// after its last space, or anywhere when a word alone is wider than the line, as
/// `overflow-wrap: anywhere` breaks a bubble in the GUI. Tabs become four spaces and
/// carriage returns are dropped, since a cell cannot hold either.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
  let width = width.max(1);
  let mut lines = Vec::new();
  for source in text.split('\n') {
    let mut line = String::new();
    let mut used = 0;
    // Where the last space of `line` is, in bytes.
    let mut space: Option<usize> = None;
    for character in source.chars() {
      let (character, repeat) = match character {
        '\r' => continue,
        '\t' => (' ', 4),
        other => (other, 1),
      };
      for _ in 0..repeat {
        let cells = character.width().unwrap_or(0);
        if used + cells > width && used > 0 {
          let breaks_here = character == ' ';
          match space.take() {
            Some(index) if !breaks_here => {
              let rest = line.split_off(index + 1);
              line.pop();
              lines.push(std::mem::replace(&mut line, rest));
              used = line.width();
            }
            _ => {
              lines.push(std::mem::take(&mut line));
              used = 0;
            }
          }
          // The space that overflows is the break itself.
          if breaks_here {
            continue;
          }
        }
        if character == ' ' {
          space = Some(line.len());
        }
        line.push(character);
        used += cells;
      }
    }
    lines.push(line);
  }
  lines
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn text_that_fits_is_left_alone() {
    assert_eq!(truncate("Connect", 7, "…"), "Connect");
  }

  #[test]
  fn text_is_cut_by_cells_not_by_characters() {
    assert_eq!(truncate("Connect", 5, "…"), "Conn…");
    // Each of these takes two cells, so four cells hold one glyph and the ellipsis.
    assert_eq!(truncate("設定画面", 4, "…"), "設…");
    assert_eq!(truncate("設定画面", 5, "…"), "設定…");
    assert_eq!(truncate("Connect", 5, "..."), "Co...");
    assert_eq!(truncate("Connect", 2, "..."), "..");
  }

  #[test]
  fn wrapping_breaks_at_spaces_and_inside_words_only_when_it_must() {
    assert_eq!(
      wrap("Nightly build 482 finished", 14),
      ["Nightly build", "482 finished"]
    );
    assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    assert_eq!(wrap("first\r\n\nthird", 10), ["first", "", "third"]);
    assert_eq!(wrap("", 10), [""]);
    assert_eq!(wrap("設定画面", 5), ["設定", "画面"]);
    for line in wrap("a long sentence that wraps over several narrow lines of text", 7) {
      assert!(line.width() <= 7, "{line}");
    }
  }
}
