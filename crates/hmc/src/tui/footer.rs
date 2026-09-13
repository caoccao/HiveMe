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

//! The status bar, `Footer.tsx` in one row.
//!
//! The state in its color, the broker, the reconnect countdown, the counts, the
//! database size, and the paused marker on the left; the config error and the last
//! error on the right, which show their detail in the snackbar when clicked or focused
//! and confirmed with `Enter`.

use std::time::Instant;

use hiveme_core::i18n::{format, t, t_count, t_with};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::app::{App, FooterEntry};
use super::keys::Action;
use super::service::Service;
use super::theme::Theme;

/// The color of a connection state, as `stateColor` picks it.
pub fn state_style(theme: &Theme, state: &str) -> Style {
  match state {
    "Connected" => Style::new().fg(theme.success),
    "Connecting" | "Reconnecting" => Style::new().fg(theme.warning),
    _ => theme.muted(),
  }
}

/// The entries on the left, each with its style.
pub fn entries<S: Service>(app: &App<S>, now: Instant) -> Vec<(String, Style)> {
  let locale = app.locale;
  let theme = &app.theme;
  let status = &app.status;
  let state_key = format!("footer.state.{}", status.state);
  let state = match t(locale, &state_key) {
    // A state this build has no words for is shown as the session spelled it.
    text if text == state_key => status.state.clone(),
    text => text,
  };
  let mut entries = Vec::new();
  if app.quitting {
    entries.push((t(locale, "tui.quitting"), Style::new().fg(theme.warning)));
  }
  entries.push((format!("{} {state}", app.glyphs.dot), state_style(theme, &status.state)));
  let broker = if status.host.is_empty() {
    t(locale, "footer.noBroker")
  } else {
    format!("{}:{}", status.host, status.port)
  };
  entries.push((broker, theme.muted()));
  if let Some(remaining) = app.retry_in(now) {
    let duration = format::duration(locale, remaining);
    entries.push((
      t_with(locale, "footer.retryIn", &[("duration", &duration)]),
      theme.muted(),
    ));
  }
  entries.push((
    t_count(locale, "footer.subscriptions", status.subscriptions as u64),
    theme.muted(),
  ));
  entries.push((
    t_count(locale, "footer.received", status.messages_received),
    theme.muted(),
  ));
  let size = format::bytes(locale, status.database_bytes);
  entries.push((t_with(locale, "footer.database", &[("size", &size)]), theme.muted()));
  if status.notifications_paused {
    entries.push((t(locale, "footer.notificationsPaused"), Style::new().fg(theme.warning)));
  }
  entries
}

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let now = Instant::now();
  let theme = app.theme;
  let mut spans = vec![Span::raw(" ")];
  for (index, (text, style)) in entries(app, now).into_iter().enumerate() {
    if index > 0 {
      spans.push(Span::raw("  "));
    }
    spans.push(Span::styled(text, style));
  }
  frame.render_widget(Line::from(spans), area);

  // The error entries are drawn from the right edge inwards.
  let mut right = area.right().saturating_sub(1);
  for entry in app.footer_entries().into_iter().rev() {
    let (key, action) = match entry {
      FooterEntry::ConfigError => ("footer.configError", Action::ShowConfigError),
      FooterEntry::LastError => ("footer.lastError", Action::ShowLastError),
    };
    let text = t(app.locale, key);
    let cells = text.width() as u16;
    let x = right.saturating_sub(cells).max(area.x);
    let rect = Rect::new(x, area.y, cells.min(area.right() - x), 1);
    let mut style = Style::new().fg(theme.error).add_modifier(Modifier::UNDERLINED);
    if app.footer_focus() == Some(entry) {
      style = style.add_modifier(Modifier::REVERSED);
    }
    frame.render_widget(Line::from(Span::styled(text, style)), rect);
    app.hits.push((rect, action));
    right = x.saturating_sub(2);
  }
}
