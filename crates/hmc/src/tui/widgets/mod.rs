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
//! here; see the deviations in `docs/specs/app.md`. Phase 3 needs the one-line text
//! field; the editor, the select popup, and the rest arrive with the tabs that use
//! them.

mod text_input;

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
}
