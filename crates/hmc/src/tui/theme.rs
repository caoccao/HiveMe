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

//! `gui.theme` and `gui.displayMode` in cells, see `docs/specs/tui.md`, "Theme".
//!
//! The twenty palettes are those of `src/App.tsx`, so the two applications mark the
//! same things in the same colors. `Auto` keeps the terminal's own background and
//! foreground, because they are unknown; `Light` and `Dark` paint both.

use hiveme_core::config::{DisplayMode, Gui, Theme as Palette};
use hiveme_core::message::Level;
use ratatui::style::{Color, Modifier, Style};

/// The colors of one screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
  pub primary: Color,
  pub secondary: Color,
  /// None in `Auto`, which keeps the terminal's own.
  pub background: Option<Color>,
  pub text: Option<Color>,
  /// Secondary text: hints, separators, the footer.
  pub muted: Color,
  pub success: Color,
  pub warning: Color,
  pub error: Color,
  /// Which fills can be drawn: none in `Auto`, whose background is unknown.
  pub mode: DisplayMode,
}

impl Theme {
  pub fn from_gui(gui: &Gui) -> Self {
    let (primary, secondary) = palette(gui.theme);
    let (background, text, muted) = match gui.display_mode {
      DisplayMode::Auto => (None, None, Color::Gray),
      // MUI's light background and its text.secondary, rgba(0, 0, 0, 0.6) on white.
      DisplayMode::Light => (
        Some(Color::Rgb(255, 255, 255)),
        Some(Color::Rgb(0, 0, 0)),
        rgb(0x666666),
      ),
      // MUI's dark background and its text.secondary, rgba(255, 255, 255, 0.7).
      DisplayMode::Dark => (Some(rgb(0x121212)), Some(Color::Rgb(255, 255, 255)), rgb(0xb3b3b3)),
    };
    Self {
      primary,
      secondary,
      background,
      text,
      muted,
      success: rgb(0x2e7d32),
      warning: rgb(0xed6c02),
      error: rgb(0xd32f2f),
      mode: gui.display_mode,
    }
  }

  /// The style of the whole screen.
  pub fn base(&self) -> Style {
    let mut style = Style::new();
    if let Some(background) = self.background {
      style = style.bg(background);
    }
    if let Some(text) = self.text {
      style = style.fg(text);
    }
    style
  }

  /// A tool, a tab, or a label that is on.
  pub fn active(&self) -> Style {
    Style::new().fg(self.primary).add_modifier(Modifier::BOLD)
  }

  /// The border of what has the focus.
  pub fn focused(&self) -> Style {
    Style::new().fg(self.primary)
  }

  pub fn muted(&self) -> Style {
    Style::new().fg(self.muted)
  }

  /// Something that cannot be used right now.
  pub fn disabled(&self) -> Style {
    Style::new().fg(self.muted).add_modifier(Modifier::DIM)
  }

  /// The MUI palette color of a level: success, warning, or error, and none for `info`,
  /// which uses the regular colors. `levelColor` in `src/lib/message.ts`.
  pub fn severity(&self, level: &Level) -> Option<Color> {
    match level {
      Level::Success => Some(self.success),
      Level::Warn => Some(self.warning),
      Level::Error => Some(self.error),
      _ => None,
    }
  }

  /// The fill and the text of a bubble, as `MessageView.tsx` picks them.
  ///
  /// A severity tints the fill with 12 percent of its color on the light background and
  /// 24 percent on the dark one; a regular outgoing bubble is black, or grey 800 in the
  /// dark mode, with white text; a regular incoming bubble is the `action.hover` gray.
  /// `Auto` draws no fill, since it cannot know what the fill would sit on.
  pub fn bubble(&self, severity: Option<Color>, outgoing: bool) -> Style {
    let white = Color::Rgb(255, 255, 255);
    match (self.mode, severity, outgoing) {
      (DisplayMode::Auto, _, _) => Style::new(),
      (DisplayMode::Light, Some(color), _) => Style::new().bg(blend(color, 0xffffff, 0.12)).fg(Color::Rgb(0, 0, 0)),
      (DisplayMode::Dark, Some(color), _) => Style::new().bg(blend(color, 0x121212, 0.24)).fg(white),
      (DisplayMode::Light, None, true) => Style::new().bg(Color::Rgb(0, 0, 0)).fg(white),
      (DisplayMode::Light, None, false) => Style::new()
        .bg(blend(Color::Rgb(0, 0, 0), 0xffffff, 0.04))
        .fg(Color::Rgb(0, 0, 0)),
      (DisplayMode::Dark, None, true) => Style::new().bg(rgb(0x424242)).fg(white),
      (DisplayMode::Dark, None, false) => Style::new().bg(blend(white, 0x121212, 0.08)).fg(white),
    }
  }
}

/// `color` at `alpha` over the solid `background`, which is what MUI's `alpha()` looks
/// like once it is painted.
fn blend(color: Color, background: u32, alpha: f32) -> Color {
  let Color::Rgb(red, green, blue) = color else {
    return color;
  };
  let mix = |over: u8, under: u8| (f32::from(over) * alpha + f32::from(under) * (1.0 - alpha)).round() as u8;
  Color::Rgb(
    mix(red, (background >> 16) as u8),
    mix(green, (background >> 8) as u8),
    mix(blue, background as u8),
  )
}

/// The primary and secondary colors of a palette.
pub fn palette(theme: Palette) -> (Color, Color) {
  let (primary, secondary) = match theme {
    Palette::Ocean => (0x0288d1, 0x26c6da),
    Palette::Aqua => (0x00acc1, 0x4dd0e1),
    Palette::Sky => (0x42a5f5, 0x90caf9),
    Palette::Arctic => (0x4fc3f7, 0xb3e5fc),
    Palette::Glacier => (0x5c6bc0, 0x9fa8da),
    Palette::Mist => (0x90a4ae, 0xcfd8dc),
    Palette::Slate => (0x546e7a, 0x78909c),
    Palette::Charcoal => (0x37474f, 0x607d8b),
    Palette::Midnight => (0x1a237e, 0x3949ab),
    Palette::Indigo => (0x3f51b5, 0x7986cb),
    Palette::Violet => (0x7e57c2, 0xb39ddb),
    Palette::Lavender => (0x9575cd, 0xd1c4e9),
    Palette::Rose => (0xc2185b, 0xf06292),
    Palette::Blush => (0xec407a, 0xf48fb1),
    Palette::Coral => (0xff7043, 0xffab91),
    Palette::Sunset => (0xef6c00, 0xff8a65),
    Palette::Amber => (0xff8f00, 0xffca28),
    Palette::Sand => (0xbcaaa4, 0xd7ccc8),
    Palette::Forest => (0x2e7d32, 0x66bb6a),
    Palette::Emerald => (0x00897b, 0x4db6ac),
  };
  (rgb(primary), rgb(secondary))
}

fn rgb(hex: u32) -> Color {
  Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The glyphs of the design and the ASCII they fall back to.
///
/// A glyph a terminal's font lacks is drawn as a box or a question mark, which reads
/// worse than plain ASCII, so the fallback is chosen for the whole screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
  pub close: &'static str,
  pub dot: &'static str,
  pub checked: &'static str,
  pub unchecked: &'static str,
  pub ellipsis: &'static str,
  pub separator: &'static str,
  /// The marker of an open branch in the topic tree, the JSON tree, and More Options.
  pub expanded: &'static str,
  /// The marker of a closed branch.
  pub collapsed: &'static str,
  /// Before the placeholder of an encrypted message.
  pub lock: &'static str,
  /// The retained marker of the metadata row.
  pub pin: &'static str,
  pub radio_on: &'static str,
  pub radio_off: &'static str,
}

impl Glyphs {
  pub const UNICODE: Self = Self {
    close: "✕",
    dot: "●",
    checked: "[✓]",
    unchecked: "[ ]",
    ellipsis: "…",
    separator: "│",
    expanded: "▾",
    collapsed: "▸",
    lock: "🔒",
    pin: "📌",
    radio_on: "(●)",
    radio_off: "( )",
  };

  pub const ASCII: Self = Self {
    close: "x",
    dot: "*",
    checked: "[x]",
    unchecked: "[ ]",
    ellipsis: "...",
    separator: "|",
    expanded: "v",
    collapsed: ">",
    lock: "[enc]",
    pin: "[R]",
    radio_on: "(*)",
    radio_off: "( )",
  };

  /// The set this terminal can draw.
  ///
  /// The Linux console and the legacy Windows console have fonts without these
  /// glyphs; Windows Terminal sets `WT_SESSION`, and every other terminal is assumed
  /// to have a font that covers them.
  pub fn detect() -> Self {
    let linux_console = std::env::var("TERM").is_ok_and(|term| term == "linux");
    let modern = [
      "WT_SESSION",
      "TERM_PROGRAM",
      "WEZTERM_EXECUTABLE",
      "ALACRITTY_LOG",
      "ConEmuANSI",
      "SSH_TTY",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some());
    Self::for_environment(cfg!(target_os = "windows"), linux_console, modern)
  }

  fn for_environment(windows: bool, linux_console: bool, modern: bool) -> Self {
    if linux_console || (windows && !modern) {
      Self::ASCII
    } else {
      Self::UNICODE
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_palette_is_the_one_the_gui_uses() {
    let mut gui = Gui::default();
    assert_eq!(Theme::from_gui(&gui).primary, Color::Rgb(0x02, 0x88, 0xd1), "Ocean");
    gui.theme = Palette::Amber;
    let theme = Theme::from_gui(&gui);
    assert_eq!(theme.primary, Color::Rgb(0xff, 0x8f, 0x00));
    assert_eq!(theme.secondary, Color::Rgb(0xff, 0xca, 0x28));
  }

  #[test]
  fn auto_keeps_the_terminals_colors_and_a_forced_mode_paints_them() {
    let mut gui = Gui {
      display_mode: DisplayMode::Auto,
      ..Gui::default()
    };
    assert_eq!(Theme::from_gui(&gui).base(), Style::new());

    gui.display_mode = DisplayMode::Light;
    let light = Theme::from_gui(&gui);
    assert_eq!(light.background, Some(Color::Rgb(255, 255, 255)));
    assert_eq!(light.text, Some(Color::Rgb(0, 0, 0)));

    gui.display_mode = DisplayMode::Dark;
    let dark = Theme::from_gui(&gui);
    assert_eq!(dark.background, Some(Color::Rgb(0x12, 0x12, 0x12)));
    assert_eq!(dark.text, Some(Color::Rgb(255, 255, 255)));
  }

  #[test]
  fn the_severity_colors_are_the_mui_palette() {
    let theme = Theme::from_gui(&Gui::default());
    assert_eq!(theme.success, Color::Rgb(0x2e, 0x7d, 0x32));
    assert_eq!(theme.warning, Color::Rgb(0xed, 0x6c, 0x02));
    assert_eq!(theme.error, Color::Rgb(0xd3, 0x2f, 0x2f));
  }

  #[test]
  fn a_bubble_is_filled_only_when_a_mode_is_forced() {
    let mut gui = Gui::default();
    let auto = Theme::from_gui(&gui);
    assert_eq!(auto.bubble(Some(auto.error), true), Style::new());
    assert_eq!(auto.severity(&Level::Other("catastrophe".to_owned())), None);

    gui.display_mode = DisplayMode::Light;
    let light = Theme::from_gui(&gui);
    assert_eq!(light.bubble(None, true).bg, Some(Color::Rgb(0, 0, 0)));
    assert_eq!(light.bubble(None, false).bg, Some(Color::Rgb(0xf5, 0xf5, 0xf5)));
    // alpha(#d32f2f, 0.12) on white.
    assert_eq!(
      light.bubble(light.severity(&Level::Error), false).bg,
      Some(Color::Rgb(0xfa, 0xe6, 0xe6))
    );

    gui.display_mode = DisplayMode::Dark;
    let dark = Theme::from_gui(&gui);
    assert_eq!(dark.bubble(None, true).bg, Some(Color::Rgb(0x42, 0x42, 0x42)));
    assert_eq!(dark.bubble(None, true).fg, Some(Color::Rgb(255, 255, 255)));
  }

  #[test]
  fn the_ascii_glyphs_are_ascii() {
    let Glyphs {
      close,
      dot,
      checked,
      unchecked,
      ellipsis,
      separator,
      expanded,
      collapsed,
      lock,
      pin,
      radio_on,
      radio_off,
    } = Glyphs::ASCII;
    for glyph in [
      close, dot, checked, unchecked, ellipsis, separator, expanded, collapsed, lock, pin, radio_on, radio_off,
    ] {
      assert!(glyph.is_ascii(), "{glyph}");
    }
  }
  #[test]
  fn modern_windows_terminals_use_unicode_and_legacy_consoles_use_ascii() {
    assert_eq!(Glyphs::for_environment(true, false, true), Glyphs::UNICODE);
    assert_eq!(Glyphs::for_environment(true, false, false), Glyphs::ASCII);
    assert_eq!(Glyphs::for_environment(false, true, true), Glyphs::ASCII);
    assert_eq!(Glyphs::for_environment(false, false, false), Glyphs::UNICODE);
  }
}
