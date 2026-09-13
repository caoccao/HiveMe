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

//! The tab strip of `MainContent.tsx`: Messages, then Settings and About in the order
//! they were opened, each closable one with its close mark.

use hiveme_core::i18n::t;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Tab};
use super::keys::Action;
use super::service::Service;

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let theme = app.theme;
  let glyphs = app.glyphs;
  let mut spans = Vec::new();
  let mut x = area.x;
  let mut push = |spans: &mut Vec<Span<'static>>, text: String, style: Style| -> Rect {
    let cells = text.width() as u16;
    let rect = Rect::new(x, area.y, cells, 1);
    x += cells;
    spans.push(Span::styled(text, style));
    rect
  };
  for (index, tab) in app.tabs.clone().into_iter().enumerate() {
    if index > 0 {
      push(&mut spans, glyphs.separator.to_owned(), theme.muted());
    }
    let style = if index == app.tab { theme.active() } else { Style::new() };
    let label = push(&mut spans, format!(" {} ", t(app.locale, tab.label_key())), style);
    app.hits.push((label, Action::SelectTab(index)));
    if tab != Tab::Messages {
      let close = push(&mut spans, format!("{} ", glyphs.close), theme.muted());
      app.hits.push((close, Action::CloseTab(index)));
    }
  }
  frame.render_widget(Line::from(spans), area);
}
