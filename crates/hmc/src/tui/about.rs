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

//! The About tab, `About.tsx` in cells.
//!
//! `HiveMe` in large letters with the GUI's amber to orange gradient spread over them,
//! the version chip, the tagline, the Author and GitHub cards, the table of the device,
//! the config file, the history database, and the license, and the copyright line. The
//! cards and the two names of the copyright line open their page on `Enter` or a click.
//! There is no icon image.

use std::time::Instant;

use hiveme_core::i18n::t;
use hiveme_core::session::About;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Widget};
use unicode_width::UnicodeWidthStr;

use super::app::App;
use super::keys::Action;
use super::service::Service;
use super::widgets::{truncate, wrap};

/// `AUTHOR_NAME` and `AUTHOR_URL` in `src/lib/constants.ts`.
pub const AUTHOR_NAME: &str = "Sam Cao";
pub const AUTHOR_URL: &str = "https://github.com/caoccao";

/// The site the copyright line links to.
pub const WEBSITE_NAME: &str = "caoccao.com";
pub const WEBSITE_URL: &str = "https://www.caoccao.com/";

/// The ends of `linear-gradient(135deg, #ffb300 0%, #e65100 100%)`.
const GRADIENT_START: (u8, u8, u8) = (0xff, 0xb3, 0x00);
const GRADIENT_END: (u8, u8, u8) = (0xe6, 0x51, 0x00);

/// The width of the name, the tagline, and the cards, as the GUI's 640 px column.
const COLUMN_MAX_WIDTH: u16 = 72;

/// How many rows one wheel notch scrolls.
const WHEEL_ROWS: u16 = 3;

/// `HiveMe` three rows high, one letter per entry.
const LETTERS: [[&str; 3]; 6] = [
  ["█  █", "█▀▀█", "█  █"],
  ["▄", "█", "█"],
  ["   ", "█ █", "▀▄▀"],
  ["   ", "█▀█", "█▄▄"],
  ["█▄ ▄█", "█ ▀ █", "█   █"],
  ["   ", "█▀█", "█▄▄"],
];

/// What opens a page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Link {
  #[default]
  Author,
  Repository,
  /// The author's name in the copyright line.
  CopyrightAuthor,
  Website,
}

impl Link {
  pub const ALL: [Self; 4] = [Self::Author, Self::Repository, Self::CopyrightAuthor, Self::Website];

  pub fn url(self, about: &About) -> String {
    match self {
      Self::Author | Self::CopyrightAuthor => AUTHOR_URL.to_owned(),
      Self::Repository => about.github_url.clone(),
      Self::Website => WEBSITE_URL.to_owned(),
    }
  }

  fn index(self) -> usize {
    Self::ALL.iter().position(|link| *link == self).unwrap_or(0)
  }
}

/// The state of the About tab, kept while it is closed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AboutState {
  pub focus: Link,
  /// How many rows are scrolled above the top.
  pub scroll: u16,
  /// The focus moved, so the next frame scrolls it into view.
  reveal: bool,
  /// The rows the tab shows and the rows it has, on the last frame.
  viewport: (u16, u16),
}

impl AboutState {
  fn scroll_by(&mut self, rows: i32) {
    let (visible, total) = self.viewport;
    let most = i32::from(total.saturating_sub(visible));
    self.scroll = (i32::from(self.scroll) + rows).clamp(0, most) as u16;
  }
}

impl<S: Service> App<S> {
  /// A key or a click in the About tab.
  pub(in crate::tui) fn perform_about(&mut self, action: Action, now: Instant) {
    let index = self.about.focus.index();
    let count = Link::ALL.len();
    let page = i32::from(self.about.viewport.0.max(2) - 1);
    match action {
      Action::FocusNext | Action::Right | Action::Down => {
        self.about.focus = Link::ALL[(index + 1) % count];
        self.about.reveal = true;
      }
      Action::FocusPrevious | Action::Left | Action::Up => {
        self.about.focus = Link::ALL[(index + count - 1) % count];
        self.about.reveal = true;
      }
      Action::Activate | Action::Toggle => self.open_link(self.about.focus, now),
      Action::AboutLink(link) => {
        self.about.focus = link;
        self.open_link(link, now);
      }
      Action::PageUp => self.about.scroll_by(-page),
      Action::PageDown => self.about.scroll_by(page),
      Action::Home => self.about.scroll = 0,
      Action::End => self.about.scroll_by(i32::from(u16::MAX)),
      _ => {}
    }
  }

  /// A wheel notch scrolls the tab.
  pub(in crate::tui) fn wheel_about(&mut self, up: bool) {
    self.about.scroll_by(if up { -1 } else { 1 } * i32::from(WHEEL_ROWS));
  }

  fn open_link(&mut self, link: Link, now: Instant) {
    let url = link.url(&self.service.about());
    self.open_url(&url, now);
  }
}

/// The color of column `x` of a gradient `width` columns wide.
fn gradient(x: u16, width: u16) -> Color {
  let at = f32::from(x) / f32::from(width.saturating_sub(1).max(1));
  let mix = |from: u8, to: u8| (f32::from(from) + (f32::from(to) - f32::from(from)) * at).round() as u8;
  Color::Rgb(
    mix(GRADIENT_START.0, GRADIENT_END.0),
    mix(GRADIENT_START.1, GRADIENT_END.1),
    mix(GRADIENT_START.2, GRADIENT_END.2),
  )
}

/// Writes `line` into `buffer` at `x`, `y`, clipped to the buffer.
fn put(buffer: &mut Buffer, x: u16, y: u16, width: u16, line: Line) {
  let area = Rect::new(x, y, width, 1).intersection(buffer.area);
  if !area.is_empty() {
    line.render(area, buffer);
  }
}

/// Draws the About tab.
pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  if area.is_empty() {
    return;
  }
  let locale = app.locale;
  let theme = app.theme;
  let glyphs = app.glyphs;
  let about = app.service.about();
  let focus = app.about.focus;
  let width = area.width;
  let column = width.saturating_sub(2).min(COLUMN_MAX_WIDTH);
  let column_x = (width - column) / 2;

  let tagline = wrap(&t(locale, "about.tagline"), usize::from(column));
  let table = [
    (
      t(locale, "about.device"),
      format!("{} ({})", about.device_name, about.device_id),
    ),
    (t(locale, "about.configPath"), about.config_path.clone()),
    (t(locale, "about.databasePath"), about.database_path.clone()),
    (t(locale, "about.license"), "Apache-2.0".to_owned()),
  ];
  let label_width = table.iter().map(|(label, _)| label.width()).max().unwrap_or(0) as u16;
  let value_x = 1 + label_width + 2;
  let value_width = width.saturating_sub(value_x + 1).max(1);
  let values: Vec<Vec<String>> = table
    .iter()
    .map(|(_, value)| wrap(value, usize::from(value_width)))
    .collect();
  let letters_width: u16 = LETTERS.iter().map(|letter| letter[0].width() as u16).sum::<u16>() + 5;
  let large = letters_width <= column;
  let name_rows = if large { 3 } else { 1 };
  let table_rows: u16 = values.iter().map(|lines| lines.len() as u16).sum();
  let cards_y = name_rows + 1 + tagline.len() as u16 + 1;
  let table_y = cards_y + 4 + 1;
  let copyright_y = table_y + table_rows + 1;
  let height = copyright_y + 1;

  let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
  buffer.set_style(buffer.area, theme.base());
  let mut hits: Vec<(Rect, Action)> = Vec::new();

  // The name, with the gradient spread over the letters.
  if large {
    let x = (width - letters_width) / 2;
    for row in 0..3u16 {
      let mut spans = Vec::new();
      let mut column_index = 0u16;
      for (index, letter) in LETTERS.iter().enumerate() {
        if index > 0 {
          spans.push(Span::raw(" "));
          column_index += 1;
        }
        for character in letter[usize::from(row)].chars() {
          spans.push(Span::styled(
            character.to_string(),
            Style::new().fg(gradient(column_index, letters_width)),
          ));
          column_index += 1;
        }
      }
      put(&mut buffer, x, row, letters_width, Line::from(spans));
    }
  } else {
    let name = hiveme_core::APP_NAME;
    let x = (width.saturating_sub(name.width() as u16)) / 2;
    let spans: Vec<Span> = name
      .chars()
      .enumerate()
      .map(|(index, character)| {
        Span::styled(
          character.to_string(),
          Style::new()
            .fg(gradient(index as u16, name.len() as u16))
            .add_modifier(Modifier::BOLD),
        )
      })
      .collect();
    put(&mut buffer, x, 0, width, Line::from(spans));
  }

  // The version chip and the tagline.
  let chip = Line::from(vec![
    Span::styled("(", theme.muted()),
    Span::styled(
      format!("v{}", about.app_version),
      theme.base().add_modifier(Modifier::BOLD),
    ),
    Span::styled(")", theme.muted()),
  ]);
  let chip_width = chip.width() as u16;
  put(
    &mut buffer,
    (width.saturating_sub(chip_width)) / 2,
    name_rows,
    chip_width,
    chip,
  );
  for (offset, line) in tagline.iter().enumerate() {
    let line_width = line.width() as u16;
    put(
      &mut buffer,
      (width.saturating_sub(line_width)) / 2,
      name_rows + 1 + offset as u16,
      line_width,
      Line::styled(line.clone(), theme.muted()),
    );
  }

  // The cards: the author at its own width, the repository taking the rest.
  let author_label = t(locale, "about.author");
  let github_label = t(locale, "about.github");
  let author_width = (author_label.width().max(AUTHOR_NAME.width()) as u16 + 4).min(column / 2);
  let cards = [
    (
      Link::Author,
      Rect::new(column_x, cards_y, author_width, 4),
      author_label,
      AUTHOR_NAME.to_owned(),
    ),
    (
      Link::Repository,
      Rect::new(
        column_x + author_width + 1,
        cards_y,
        column.saturating_sub(author_width + 1),
        4,
      ),
      github_label,
      about.github_url.clone(),
    ),
  ];
  for (link, rect, label, value) in cards {
    let block = Block::bordered()
      .border_type(BorderType::Rounded)
      .border_style(if focus == link { theme.focused() } else { theme.muted() });
    let inner = block.inner(rect);
    block.render(rect, &mut buffer);
    let room = usize::from(inner.width.saturating_sub(2));
    put(
      &mut buffer,
      inner.x + 1,
      inner.y,
      inner.width,
      Line::styled(
        truncate(&label, room, glyphs.ellipsis),
        theme.muted().add_modifier(Modifier::BOLD),
      ),
    );
    let mut style = theme.base();
    if link == Link::Author {
      style = style.add_modifier(Modifier::BOLD);
    }
    if focus == link {
      style = style.fg(theme.primary);
    }
    put(
      &mut buffer,
      inner.x + 1,
      inner.y + 1,
      inner.width,
      Line::styled(truncate(&value, room, glyphs.ellipsis), style),
    );
    hits.push((rect, Action::AboutLink(link)));
  }

  // The table.
  let mut y = table_y;
  for ((label, _), lines) in table.iter().zip(&values) {
    put(
      &mut buffer,
      1,
      y,
      label_width,
      Line::styled(label.clone(), theme.base().add_modifier(Modifier::BOLD)),
    );
    for line in lines {
      put(&mut buffer, value_x, y, value_width, Line::raw(line.clone()));
      y += 1;
    }
  }

  // The copyright line, whose names are links.
  let link_style = |link: Link| {
    let style = Style::new().fg(theme.primary).add_modifier(Modifier::UNDERLINED);
    if focus == link {
      style.add_modifier(Modifier::REVERSED)
    } else {
      style
    }
  };
  let copyright = t(locale, "about.copyright");
  let line_width = (copyright.width() + 1 + AUTHOR_NAME.width() + 1 + WEBSITE_NAME.width()) as u16;
  let mut x = (width.saturating_sub(line_width)) / 2;
  put(
    &mut buffer,
    x,
    copyright_y,
    width,
    Line::from(vec![
      Span::styled(copyright.clone(), theme.muted()),
      Span::raw(" "),
      Span::styled(AUTHOR_NAME, link_style(Link::CopyrightAuthor)),
      Span::raw(" "),
      Span::styled(WEBSITE_NAME, link_style(Link::Website)),
    ]),
  );
  x += copyright.width() as u16 + 1;
  hits.push((
    Rect::new(x, copyright_y, AUTHOR_NAME.width() as u16, 1),
    Action::AboutLink(Link::CopyrightAuthor),
  ));
  x += AUTHOR_NAME.width() as u16 + 1;
  hits.push((
    Rect::new(x, copyright_y, WEBSITE_NAME.width() as u16, 1),
    Action::AboutLink(Link::Website),
  ));

  // Scroll the focused link into view when the focus moved, then copy what is in view.
  let state = &mut app.about;
  state.viewport = (area.height, height);
  if state.reveal {
    let (top, rows) = match focus {
      Link::Author | Link::Repository => (cards_y, 4),
      Link::CopyrightAuthor | Link::Website => (copyright_y, 1),
    };
    if top < state.scroll {
      state.scroll = top;
    } else if top + rows > state.scroll + area.height {
      state.scroll = (top + rows).saturating_sub(area.height);
    }
    state.reveal = false;
  }
  state.scroll = state.scroll.min(height.saturating_sub(area.height));
  let scroll = state.scroll;

  let target = frame.buffer_mut();
  for row in 0..area.height.min(height.saturating_sub(scroll)) {
    for column_index in 0..width {
      let source = &buffer[(column_index, row + scroll)];
      if let Some(cell) = target.cell_mut(Position::new(area.x + column_index, area.y + row)) {
        *cell = source.clone();
      }
    }
  }
  for (rect, action) in hits {
    if rect.bottom() <= scroll || rect.y >= scroll + area.height {
      continue;
    }
    let top = rect.y.max(scroll);
    let bottom = rect.bottom().min(scroll + area.height);
    let visible = Rect::new(area.x + rect.x, area.y + top - scroll, rect.width, bottom - top);
    app.hits.push((visible.intersection(area), action));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_letters_line_up_and_the_gradient_runs_from_amber_to_orange() {
    for letter in LETTERS {
      assert_eq!(letter[0].width(), letter[1].width());
      assert_eq!(letter[1].width(), letter[2].width());
    }
    assert_eq!(gradient(0, 24), Color::Rgb(0xff, 0xb3, 0x00));
    assert_eq!(gradient(23, 24), Color::Rgb(0xe6, 0x51, 0x00));
  }

  #[test]
  fn every_link_has_its_page() {
    let about = About {
      app_version: "0.1.0".to_owned(),
      config_path: String::new(),
      database_path: String::new(),
      device_id: String::new(),
      device_name: String::new(),
      github_url: "https://github.com/caoccao/HiveMe".to_owned(),
    };
    assert_eq!(Link::Author.url(&about), AUTHOR_URL);
    assert_eq!(Link::CopyrightAuthor.url(&about), AUTHOR_URL);
    assert_eq!(Link::Repository.url(&about), "https://github.com/caoccao/HiveMe");
    assert_eq!(Link::Website.url(&about), WEBSITE_URL);
  }
}
