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

//! Tab 0, Messages.
//!
//! Phase 3 draws the selected topic and whether it has messages. The topic tree, the
//! chat view, and the composer of `docs/specs/tui.md` are phase 4 of the terminal UI
//! plan, and fill this pane.

use hiveme_core::i18n::t;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use super::app::App;
use super::service::Service;

pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect) {
  let theme = app.theme;
  let mut block = Block::bordered()
    .border_type(BorderType::Rounded)
    .border_style(theme.muted());
  if let Some(topic) = app.selected_topic.as_deref() {
    block = block.title(format!(" {topic} "));
  }
  let inner = block.inner(area);
  frame.render_widget(block, area);

  let key = match app.selected_topic {
    None => "messages.selectTopic",
    Some(_) if app.selected_messages().is_empty() => "messages.empty",
    Some(_) => return,
  };
  let middle = inner.centered_vertically(Constraint::Length(1));
  frame.render_widget(
    Paragraph::new(t(app.locale, key))
      .style(theme.muted())
      .centered()
      .wrap(Wrap { trim: true }),
    middle,
  );
}
