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

//! The key map of `docs/specs/tui.md`, "Keys and mouse".
//!
//! Terminals do not agree on which chords they report. `Ctrl+1`..`Ctrl+9`, `Ctrl+Tab`,
//! and `Ctrl+/` arrive as themselves only under the kitty keyboard protocol, so every
//! action also has a binding that a plain terminal delivers, and [`action`] accepts
//! both. Which keys a text field takes for itself is decided here too, so that the
//! quit keys work in every focus while `?` is typed into a field rather than opening
//! the help.

use std::io;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags};

use super::messages::{Control, Pane};

/// What a key or a click asks the terminal UI to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
  /// Leave the terminal UI through the quit path.
  Quit,
  About,
  /// Connect, or disconnect while connecting, connected, or reconnecting.
  Connection,
  /// Pause or resume notifications.
  Pause,
  ClearTopic,
  Settings,
  Help,
  SelectTab(usize),
  CloseTab(usize),
  PreviousTab,
  NextTab,
  /// Close the current closable tab.
  CloseCurrentTab,
  /// Close the topmost overlay, or go back to the list.
  Escape,
  FocusNext,
  FocusPrevious,
  /// Open or confirm what has the focus.
  Activate,
  Up,
  Down,
  Left,
  Right,
  PageUp,
  PageDown,
  Home,
  End,
  /// `Space`: toggle a branch, a checkbox, or every tree of a message.
  Toggle,
  /// `/`: focus the topic filter.
  FocusFilter,
  /// `c` on a message.
  CopyBody,
  /// `r` on a message.
  CopyRaw,
  /// `Alt+Enter`, `Ctrl+J`, or `Shift+Enter`: a new line in the composer.
  Newline,
  /// `Ctrl+Left` and `Ctrl+Right`: move the divider of the Messages tab.
  SplitLeft,
  SplitRight,
  TogglePassword,
  OpenReleases,
  ToggleSkipVersion,
  CloseUpdateNotice,
  /// Show the detail of an entry of the footer.
  ShowConfigError,
  ShowLastError,
  /// Focus a settings category.
  SettingsCategory(usize),
  /// Focus a settings field.
  SettingsField(usize),
  /// Dismiss the snackbar.
  DismissSnackbar,
  /// A click on a pane of the Messages tab, where nothing more specific was hit. The
  /// wheel scrolls the pane it is over.
  FocusPane(Pane),
  /// A click on the marker of a topic, by node id.
  ToggleTopic(String),
  /// A click on the label of a topic, by node id.
  SelectTopic(String),
  /// A click on a message, by row id.
  FocusMessage(i64),
  /// A click on a control of the composer.
  Composer(Control),
  /// A click on a QoS radio: the config's, or 0, 1, or 2.
  ComposerQos(Option<u8>),
  /// A click on an entry of the open Level popup.
  ComposerLevel(usize),
  /// Pressing the divider of the Messages tab, which a drag then moves.
  Divider,
  /// A key for the focused text field.
  Edit(KeyEvent),
}

/// What decides how a key is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Context {
  /// A text field has the focus, so printable keys are text.
  pub typing: bool,
  /// The update notice is on screen, so its letters are its keys.
  pub notice: bool,
  /// The terminal accepted the keyboard enhancement flags.
  pub enhanced: bool,
}

/// The flags requested from a terminal that supports the kitty keyboard protocol.
///
/// Disambiguating the escape codes is what makes `Ctrl+digit`, `Ctrl+Tab`, and
/// `Shift+Enter` distinct keys. Release events are not asked for, since nothing here
/// acts on a release.
pub const ENHANCEMENT_FLAGS: KeyboardEnhancementFlags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES;

/// Whether to request [`ENHANCEMENT_FLAGS`].
///
/// Only where the terminal says it supports them. crossterm reads the Windows console
/// API directly, which already reports the chords, and answers no there.
pub fn supports_enhancement() -> io::Result<bool> {
  crossterm::terminal::supports_keyboard_enhancement()
}

/// The action a key event asks for, if any.
pub fn action(key: KeyEvent, context: Context) -> Option<Action> {
  // Windows reports releases as well as presses, and so does the kitty protocol when
  // asked; a key acts once, when it goes down or repeats.
  if key.kind == KeyEventKind::Release {
    return None;
  }
  let control = key.modifiers.contains(KeyModifiers::CONTROL);
  let alt = key.modifiers.contains(KeyModifiers::ALT);

  if control && !alt {
    match key.code {
      KeyCode::Char('q' | 'Q' | 'c' | 'C') => return Some(Action::Quit),
      KeyCode::Char('w' | 'W') => return Some(Action::CloseCurrentTab),
      KeyCode::Char('h' | 'H') => return Some(Action::TogglePassword),
      KeyCode::Char('/') => return Some(Action::Help),
      // Without the keyboard protocol `Ctrl+/` is the byte 0x1F, which crossterm reads
      // as `Ctrl+7`; with it, `Ctrl+7` is itself.
      KeyCode::Char('7') if !context.enhanced => return Some(Action::Help),
      KeyCode::Char(digit @ '1'..='9') => return Some(Action::SelectTab(tab_index(digit))),
      KeyCode::Tab => return Some(Action::NextTab),
      KeyCode::BackTab => return Some(Action::PreviousTab),
      KeyCode::Left => return Some(Action::SplitLeft),
      KeyCode::Right => return Some(Action::SplitRight),
      // A plain terminal sends Ctrl+J as the line feed byte, which raw mode reports as
      // itself rather than as Enter.
      KeyCode::Char('j' | 'J') => return Some(Action::Newline),
      _ => {}
    }
  }
  if alt && !control {
    match key.code {
      KeyCode::Char(digit @ '1'..='9') => return Some(Action::SelectTab(tab_index(digit))),
      KeyCode::Left => return Some(Action::PreviousTab),
      KeyCode::Right => return Some(Action::NextTab),
      KeyCode::Enter => return Some(Action::Newline),
      _ => {}
    }
  }
  if !control && !alt {
    match key.code {
      KeyCode::F(1) => return Some(Action::About),
      KeyCode::F(2) => return Some(Action::Connection),
      KeyCode::F(3) => return Some(Action::Pause),
      KeyCode::F(4) => return Some(Action::ClearTopic),
      KeyCode::F(10) => return Some(Action::Settings),
      KeyCode::Esc => return Some(Action::Escape),
      KeyCode::Tab => return Some(Action::FocusNext),
      KeyCode::BackTab => return Some(Action::FocusPrevious),
      // Shift+Enter arrives as itself only under the keyboard protocol.
      KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => return Some(Action::Newline),
      KeyCode::Enter => return Some(Action::Activate),
      // Up and Down leave a one-line field for its neighbor as well.
      KeyCode::Up => return Some(Action::Up),
      KeyCode::Down => return Some(Action::Down),
      _ => {}
    }
  }
  if context.typing {
    return Some(Action::Edit(key));
  }
  if control || alt {
    return None;
  }
  match key.code {
    KeyCode::Char('?') => Some(Action::Help),
    KeyCode::Char('o') if context.notice => Some(Action::OpenReleases),
    KeyCode::Char('s') if context.notice => Some(Action::ToggleSkipVersion),
    KeyCode::Char('x') if context.notice => Some(Action::CloseUpdateNotice),
    KeyCode::Char('/') => Some(Action::FocusFilter),
    KeyCode::Char('c') => Some(Action::CopyBody),
    KeyCode::Char('r') => Some(Action::CopyRaw),
    KeyCode::Char(' ') => Some(Action::Toggle),
    KeyCode::Left => Some(Action::Left),
    KeyCode::Right => Some(Action::Right),
    KeyCode::PageUp => Some(Action::PageUp),
    KeyCode::PageDown => Some(Action::PageDown),
    KeyCode::Home => Some(Action::Home),
    KeyCode::End => Some(Action::End),
    _ => None,
  }
}

/// Whether a key is one of the quit keys, which during the quit path means leave now.
pub fn is_quit(key: &KeyEvent) -> bool {
  action(*key, Context::default()) == Some(Action::Quit)
}

fn tab_index(digit: char) -> usize {
  digit as usize - '1' as usize
}

#[cfg(test)]
mod tests {
  use super::*;

  fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
  }

  fn plain(code: KeyCode) -> KeyEvent {
    key(code, KeyModifiers::NONE)
  }

  const BROWSING: Context = Context {
    typing: false,
    notice: false,
    enhanced: false,
  };

  const TYPING: Context = Context {
    typing: true,
    notice: false,
    enhanced: false,
  };

  #[test]
  fn ctrl_q_and_ctrl_c_quit_in_every_focus() {
    for context in [
      BROWSING,
      TYPING,
      Context {
        enhanced: true,
        ..TYPING
      },
    ] {
      for code in [KeyCode::Char('q'), KeyCode::Char('c'), KeyCode::Char('Q')] {
        assert_eq!(
          action(key(code, KeyModifiers::CONTROL), context),
          Some(Action::Quit),
          "{code:?} {context:?}"
        );
      }
    }
    assert!(is_quit(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)));
    assert!(!is_quit(&plain(KeyCode::Char('q'))));
  }

  #[test]
  fn the_function_keys_are_the_toolbar() {
    for context in [BROWSING, TYPING] {
      assert_eq!(action(plain(KeyCode::F(1)), context), Some(Action::About));
      assert_eq!(action(plain(KeyCode::F(2)), context), Some(Action::Connection));
      assert_eq!(action(plain(KeyCode::F(3)), context), Some(Action::Pause));
      assert_eq!(action(plain(KeyCode::F(4)), context), Some(Action::ClearTopic));
      assert_eq!(action(plain(KeyCode::F(10)), context), Some(Action::Settings));
    }
  }

  #[test]
  fn alt_digits_select_a_tab_everywhere_and_ctrl_digits_where_reported() {
    for (digit, index) in [('1', 0), ('2', 1), ('9', 8)] {
      assert_eq!(
        action(key(KeyCode::Char(digit), KeyModifiers::ALT), TYPING),
        Some(Action::SelectTab(index))
      );
      assert_eq!(
        action(key(KeyCode::Char(digit), KeyModifiers::CONTROL), BROWSING),
        Some(Action::SelectTab(index))
      );
    }
    assert_eq!(
      action(key(KeyCode::Char('0'), KeyModifiers::ALT), BROWSING),
      None,
      "there is no tab 0"
    );
  }

  #[test]
  fn tabs_cycle_with_alt_arrows_and_with_ctrl_tab_where_reported() {
    assert_eq!(
      action(key(KeyCode::Left, KeyModifiers::ALT), TYPING),
      Some(Action::PreviousTab)
    );
    assert_eq!(
      action(key(KeyCode::Right, KeyModifiers::ALT), TYPING),
      Some(Action::NextTab)
    );
    assert_eq!(
      action(key(KeyCode::Tab, KeyModifiers::CONTROL), BROWSING),
      Some(Action::NextTab)
    );
    assert_eq!(
      action(
        key(KeyCode::BackTab, KeyModifiers::CONTROL | KeyModifiers::SHIFT),
        BROWSING
      ),
      Some(Action::PreviousTab)
    );
    assert_eq!(
      action(key(KeyCode::Char('w'), KeyModifiers::CONTROL), TYPING),
      Some(Action::CloseCurrentTab)
    );
  }

  #[test]
  fn the_help_opens_with_ctrl_slash_anywhere_and_a_question_mark_outside_text() {
    assert_eq!(
      action(key(KeyCode::Char('/'), KeyModifiers::CONTROL), TYPING),
      Some(Action::Help)
    );
    // The byte a plain terminal sends for Ctrl+/.
    assert_eq!(
      action(key(KeyCode::Char('7'), KeyModifiers::CONTROL), TYPING),
      Some(Action::Help)
    );
    // Under the keyboard protocol Ctrl+7 is the seventh tab.
    assert_eq!(
      action(
        key(KeyCode::Char('7'), KeyModifiers::CONTROL),
        Context {
          enhanced: true,
          ..BROWSING
        }
      ),
      Some(Action::SelectTab(6))
    );
    assert_eq!(
      action(key(KeyCode::Char('?'), KeyModifiers::SHIFT), BROWSING),
      Some(Action::Help)
    );
    let typed = key(KeyCode::Char('?'), KeyModifiers::SHIFT);
    assert_eq!(action(typed, TYPING), Some(Action::Edit(typed)));
  }

  #[test]
  fn escape_tab_enter_and_the_arrows_move_the_focus_in_every_scope() {
    for context in [BROWSING, TYPING] {
      assert_eq!(action(plain(KeyCode::Esc), context), Some(Action::Escape));
      assert_eq!(action(plain(KeyCode::Tab), context), Some(Action::FocusNext));
      assert_eq!(
        action(key(KeyCode::BackTab, KeyModifiers::SHIFT), context),
        Some(Action::FocusPrevious)
      );
      assert_eq!(action(plain(KeyCode::Enter), context), Some(Action::Activate));
      assert_eq!(action(plain(KeyCode::Up), context), Some(Action::Up));
      assert_eq!(action(plain(KeyCode::Down), context), Some(Action::Down));
    }
  }

  #[test]
  fn ctrl_h_shows_or_hides_the_password() {
    assert_eq!(
      action(key(KeyCode::Char('h'), KeyModifiers::CONTROL), TYPING),
      Some(Action::TogglePassword)
    );
  }

  #[test]
  fn the_update_notice_letters_are_keys_only_outside_text() {
    let notice = Context {
      notice: true,
      ..BROWSING
    };
    assert_eq!(action(plain(KeyCode::Char('o')), notice), Some(Action::OpenReleases));
    assert_eq!(
      action(plain(KeyCode::Char('s')), notice),
      Some(Action::ToggleSkipVersion)
    );
    assert_eq!(
      action(plain(KeyCode::Char('x')), notice),
      Some(Action::CloseUpdateNotice)
    );
    assert_eq!(action(plain(KeyCode::Char('o')), BROWSING), None, "no notice, no key");

    let typing = Context { typing: true, ..notice };
    let letter = plain(KeyCode::Char('s'));
    assert_eq!(action(letter, typing), Some(Action::Edit(letter)));
  }

  #[test]
  fn a_text_field_takes_the_editing_keys() {
    for code in [
      KeyCode::Char('a'),
      KeyCode::Left,
      KeyCode::Right,
      KeyCode::Home,
      KeyCode::End,
      KeyCode::Backspace,
      KeyCode::Delete,
    ] {
      let event = plain(code);
      assert_eq!(action(event, TYPING), Some(Action::Edit(event)), "{code:?}");
    }
    for letter in ['u', 'a', 'e'] {
      let event = key(KeyCode::Char(letter), KeyModifiers::CONTROL);
      assert_eq!(action(event, TYPING), Some(Action::Edit(event)), "Ctrl+{letter}");
    }
    assert_eq!(action(plain(KeyCode::Char('a')), BROWSING), None);
  }

  #[test]
  fn the_messages_keys_outside_text_move_toggle_page_and_copy() {
    for (code, expected) in [
      (KeyCode::Char('/'), Action::FocusFilter),
      (KeyCode::Char('c'), Action::CopyBody),
      (KeyCode::Char('r'), Action::CopyRaw),
      (KeyCode::Char(' '), Action::Toggle),
      (KeyCode::Left, Action::Left),
      (KeyCode::Right, Action::Right),
      (KeyCode::PageUp, Action::PageUp),
      (KeyCode::PageDown, Action::PageDown),
      (KeyCode::Home, Action::Home),
      (KeyCode::End, Action::End),
    ] {
      assert_eq!(action(plain(code), BROWSING), Some(expected), "{code:?}");
      // In a text field every one of them is editing.
      assert_eq!(action(plain(code), TYPING), Some(Action::Edit(plain(code))), "{code:?}");
    }
  }

  #[test]
  fn a_new_line_has_a_key_on_every_terminal() {
    for context in [BROWSING, TYPING] {
      assert_eq!(
        action(key(KeyCode::Enter, KeyModifiers::ALT), context),
        Some(Action::Newline)
      );
      assert_eq!(
        action(key(KeyCode::Char('j'), KeyModifiers::CONTROL), context),
        Some(Action::Newline)
      );
      // Reported under the keyboard protocol.
      assert_eq!(
        action(key(KeyCode::Enter, KeyModifiers::SHIFT), context),
        Some(Action::Newline)
      );
      assert_eq!(action(plain(KeyCode::Enter), context), Some(Action::Activate));
    }
  }

  #[test]
  fn ctrl_arrows_move_the_divider_in_every_focus() {
    for context in [BROWSING, TYPING] {
      assert_eq!(
        action(key(KeyCode::Left, KeyModifiers::CONTROL), context),
        Some(Action::SplitLeft)
      );
      assert_eq!(
        action(key(KeyCode::Right, KeyModifiers::CONTROL), context),
        Some(Action::SplitRight)
      );
    }
  }

  #[test]
  fn a_release_does_nothing_and_a_repeat_acts_again() {
    let mut release = key(KeyCode::Char('q'), KeyModifiers::CONTROL);
    release.kind = KeyEventKind::Release;
    assert_eq!(action(release, BROWSING), None);
    let mut repeat = plain(KeyCode::Down);
    repeat.kind = KeyEventKind::Repeat;
    assert_eq!(action(repeat, BROWSING), Some(Action::Down));
  }
}
