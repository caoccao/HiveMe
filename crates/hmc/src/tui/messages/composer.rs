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

//! The composer, `Composer.tsx` in cells: the message box, the Level select, More
//! Options, and Send, with the Topic, Title, and QoS rows below when the options are
//! open.
//!
//! `Enter` sends from every control, and inside the open Level popup picks a level
//! instead. The draft, the level, the options, and whether they are open are kept per
//! selected tree topic in memory, and a send that finishes after the selection moved
//! clears only the draft it came from.

use std::collections::HashMap;

use hiveme_core::i18n::t;
use hiveme_core::message::Level;
use hiveme_core::session::PublishOptions;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use super::super::app::{App, Outcome};
use super::super::keys::Action;
use super::super::service::Service;
use super::super::widgets::{Editor, TextInput, truncate};
use super::Focus;

/// The levels the select offers, in its order. Info is the default.
pub const LEVELS: [Level; 4] = [Level::Info, Level::Error, Level::Success, Level::Warn];

/// The fewest and the most rows the message box takes.
pub const EDITOR_MIN_ROWS: u16 = 3;
pub const EDITOR_MAX_ROWS: u16 = 6;

/// The QoS radios: the config's, then 0, 1, and 2.
pub const QOS_CHOICES: [Option<u8>; 4] = [None, Some(0), Some(1), Some(2)];

/// A control of the composer that takes the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
  Editor,
  Level,
  Options,
  Send,
  Topic,
  Title,
  Qos,
  Retain,
  Json,
}

/// What the composer holds for one tree topic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
  pub body: Editor,
  pub topic: TextInput,
  pub title: TextInput,
  /// An index into [`LEVELS`].
  pub level: usize,
  pub qos: Option<u8>,
  pub retain: bool,
  pub as_json: bool,
  pub expanded: bool,
}

/// The composer's state, kept while another tab is shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposerState {
  pub drafts: HashMap<String, Draft>,
  /// A publish is on its way; nothing else is sent until it is answered.
  pub sending: bool,
  /// The highlighted entry of the open Level popup.
  pub popup: Option<usize>,
}

impl<S: Service> App<S> {
  /// The draft of the selected topic, or the defaults.
  pub fn draft(&self) -> Draft {
    self
      .selected_topic
      .as_ref()
      .and_then(|topic| self.messages_tab.composer.drafts.get(topic))
      .cloned()
      .unwrap_or_default()
  }

  fn draft_mut(&mut self) -> Option<&mut Draft> {
    let topic = self.selected_topic.clone()?;
    Some(self.messages_tab.composer.drafts.entry(topic).or_default())
  }

  /// Whether the composer can be used: connected, a topic selected, nothing on its way.
  pub fn composer_enabled(&self) -> bool {
    self.status.state == "Connected" && self.selected_topic.is_some() && !self.messages_tab.composer.sending
  }

  /// Whether a control can take a key or a click right now.
  pub fn control_enabled(&self, control: Control) -> bool {
    let enabled = self.composer_enabled();
    let draft = self.draft();
    match control {
      Control::Options => self.selected_topic.is_some(),
      Control::Level | Control::Title => enabled && !draft.as_json,
      Control::Send => enabled && !draft.body.text().trim().is_empty(),
      Control::Editor | Control::Topic | Control::Qos | Control::Retain | Control::Json => enabled,
    }
  }

  /// The composer's controls in focus order, the option rows only while they are open.
  pub fn composer_controls(&self) -> Vec<Control> {
    let mut controls = vec![Control::Editor, Control::Level, Control::Options, Control::Send];
    if self.draft().expanded {
      controls.extend([
        Control::Topic,
        Control::Title,
        Control::Qos,
        Control::Retain,
        Control::Json,
      ]);
    }
    controls
  }

  /// Moves the focus off an option row that is no longer shown, after the selection
  /// moved to a topic whose options are closed.
  pub(in crate::tui) fn keep_composer_focus_visible(&mut self) {
    if let Focus::Composer(control) = self.messages_tab.focus
      && !self.composer_controls().contains(&control)
    {
      self.messages_tab.focus = Focus::Composer(Control::Options);
    }
  }

  /// A key or a click on a composer control.
  pub(in crate::tui) fn composer_action(&mut self, control: Control, action: &Action) {
    if let Some(highlighted) = self.messages_tab.composer.popup {
      match action {
        Action::Up => self.messages_tab.composer.popup = Some(highlighted.saturating_sub(1)),
        Action::Down => self.messages_tab.composer.popup = Some((highlighted + 1).min(LEVELS.len() - 1)),
        Action::Activate | Action::Toggle => self.pick_level(highlighted),
        _ => {}
      }
      return;
    }
    if matches!(action, Action::Activate) {
      return self.send();
    }
    if !self.control_enabled(control) && !matches!(action, Action::Composer(_)) {
      return;
    }
    match (control, action) {
      (Control::Editor, Action::Edit(key)) => {
        if let Some(draft) = self.draft_mut() {
          draft.body.handle(*key);
        }
      }
      (Control::Editor, Action::Newline) => {
        if let Some(draft) = self.draft_mut() {
          draft.body.newline();
        }
      }
      (Control::Editor, Action::Up | Action::Down) => {
        let width = self.messages_tab.editor_width;
        if let Some(draft) = self.draft_mut() {
          draft.body.move_vertically(*action == Action::Down, width);
        }
      }
      (Control::Topic, Action::Edit(key)) => {
        if let Some(draft) = self.draft_mut() {
          draft.topic.handle(*key);
          strip_leading_slashes(&mut draft.topic);
        }
      }
      (Control::Title, Action::Edit(key)) => {
        if let Some(draft) = self.draft_mut() {
          draft.title.handle(*key);
        }
      }
      (Control::Level, Action::Toggle | Action::Up | Action::Down) => self.open_level_popup(),
      (Control::Options, Action::Toggle) => self.toggle_options(),
      (Control::Send, Action::Toggle) => self.send(),
      (Control::Retain, Action::Toggle) => {
        if let Some(draft) = self.draft_mut() {
          draft.retain = !draft.retain;
        }
      }
      (Control::Json, Action::Toggle) => {
        if let Some(draft) = self.draft_mut() {
          draft.as_json = !draft.as_json;
        }
      }
      (Control::Qos, Action::Toggle | Action::Right | Action::Left) => {
        if let Some(draft) = self.draft_mut() {
          let index = QOS_CHOICES.iter().position(|choice| *choice == draft.qos).unwrap_or(0);
          let next = if *action == Action::Left {
            (index + QOS_CHOICES.len() - 1) % QOS_CHOICES.len()
          } else {
            (index + 1) % QOS_CHOICES.len()
          };
          draft.qos = QOS_CHOICES[next];
        }
      }
      // A click: the control takes the focus and does what Space does.
      (_, Action::Composer(_)) => {
        self.messages_tab.focus = Focus::Composer(control);
        if self.control_enabled(control)
          && !matches!(
            control,
            Control::Editor | Control::Topic | Control::Title | Control::Qos
          )
        {
          self.composer_action(control, &Action::Toggle);
        }
      }
      _ => {}
    }
  }

  /// A click on a QoS radio.
  pub(in crate::tui) fn click_qos(&mut self, qos: Option<u8>) {
    self.messages_tab.focus = Focus::Composer(Control::Qos);
    if self.control_enabled(Control::Qos)
      && let Some(draft) = self.draft_mut()
    {
      draft.qos = qos;
    }
  }

  /// Text pasted into a composer field.
  pub(in crate::tui) fn composer_paste(&mut self, control: Control, text: &str) {
    if !self.control_enabled(control) {
      return;
    }
    let Some(draft) = self.draft_mut() else {
      return;
    };
    match control {
      Control::Editor => {
        draft.body.paste(text);
      }
      Control::Topic => {
        draft.topic.paste(text);
        strip_leading_slashes(&mut draft.topic);
      }
      Control::Title => {
        draft.title.paste(text);
      }
      _ => {}
    }
  }

  fn open_level_popup(&mut self) {
    self.messages_tab.composer.popup = Some(self.draft().level);
  }

  /// Chooses a level in the popup, which closes it without sending.
  pub(in crate::tui) fn pick_level(&mut self, index: usize) {
    self.messages_tab.composer.popup = None;
    self.messages_tab.focus = Focus::Composer(Control::Level);
    if self.control_enabled(Control::Level)
      && let Some(draft) = self.draft_mut()
    {
      draft.level = index.min(LEVELS.len() - 1);
    }
  }

  fn toggle_options(&mut self) {
    if let Some(draft) = self.draft_mut() {
      draft.expanded = !draft.expanded;
    }
  }

  /// Publishes the selected topic's draft, as `send` in `Composer.tsx` does.
  pub(in crate::tui) fn send(&mut self) {
    let Some(topic) = self.selected_topic.clone() else {
      return;
    };
    let draft = self.draft();
    let body = draft.body.text().to_owned();
    if !self.composer_enabled() || body.trim().is_empty() {
      return;
    }
    let title = draft.title.text().trim();
    let options = PublishOptions {
      topic: Some(draft.topic.text().trim().to_owned()).filter(|topic| !topic.is_empty()),
      json: draft.as_json,
      qos: draft.qos,
      retain: Some(draft.retain),
      title: (!draft.as_json && !title.is_empty()).then(|| title.to_owned()),
      level: (!draft.as_json).then(|| LEVELS[draft.level].as_str().to_owned()),
    };
    self.messages_tab.composer.sending = true;
    let service = self.service.clone();
    let outcomes = self.outcomes.clone();
    tokio::spawn(async move {
      let outcome = match service.publish(&topic, &body, options).await {
        Ok(row) => Outcome::Sent {
          topic,
          body,
          row: Box::new(row),
        },
        Err(error) => Outcome::SendFailed(error.to_string()),
      };
      let _ = outcomes.send(outcome);
    });
  }

  /// A publish was acknowledged: the draft it came from is emptied, if it still says what
  /// was sent, and its options stay.
  pub(in crate::tui) fn sent(&mut self, topic: &str, body: &str) {
    self.messages_tab.composer.sending = false;
    if let Some(draft) = self.messages_tab.composer.drafts.get_mut(topic)
      && draft.body.text() == body
    {
      draft.body = Editor::default();
    }
  }
}

/// A topic is relative to the selected one, so leading slashes go as they are typed.
fn strip_leading_slashes(input: &mut TextInput) {
  let text = input.text();
  if text.starts_with('/') {
    *input = TextInput::new(text.trim_start_matches('/'));
  }
}

/// The rows of the options below the controls row: Topic, Title, and the QoS row, which
/// wraps onto more rows when the checkboxes do not fit beside the radios.
fn option_items<S: Service>(app: &App<S>) -> Vec<(String, Option<Action>)> {
  let draft = app.draft();
  let glyphs = app.glyphs;
  let mut items = Vec::new();
  for choice in QOS_CHOICES {
    let label = choice.map_or_else(|| t(app.locale, "composer.qosDefault"), |qos| qos.to_string());
    let mark = if draft.qos == choice {
      glyphs.radio_on
    } else {
      glyphs.radio_off
    };
    items.push((format!("{mark} {label}"), Some(Action::ComposerQos(choice))));
  }
  for (checked, key, control) in [
    (draft.retain, "composer.retain", Control::Retain),
    (draft.as_json, "composer.sendAsJson", Control::Json),
  ] {
    let mark = if checked { glyphs.checked } else { glyphs.unchecked };
    items.push((
      format!("{mark} {}", t(app.locale, key)),
      Some(Action::Composer(control)),
    ));
  }
  items
}

/// Lays the QoS row out: each item's row and column, and how many rows it took.
fn flow(widths: &[u16], room: u16) -> (Vec<(u16, u16)>, u16) {
  let mut placed = Vec::new();
  let (mut row, mut column) = (0u16, 0u16);
  for (index, width) in widths.iter().enumerate() {
    // The radios sit one cell apart; the checkboxes two, as the GUI spaces its groups.
    let gap = match index {
      0 => 0,
      1..=3 => 1,
      _ => 2,
    };
    if column > 0 && column + gap + width > room {
      row += 1;
      column = 0;
    } else {
      column += gap;
    }
    placed.push((row, column));
    column += width;
  }
  (placed, row + 1)
}

fn label_width<S: Service>(app: &App<S>) -> u16 {
  ["composer.topic", "composer.title", "composer.qos"]
    .iter()
    .map(|key| t(app.locale, key).width())
    .max()
    .unwrap_or(0) as u16
}

/// How many rows the composer takes in a pane `width` cells wide.
pub fn height<S: Service>(app: &App<S>, width: u16) -> u16 {
  let draft = app.draft();
  let editor_width = width.saturating_sub(2);
  let editor = (draft.body.rows(editor_width) as u16).clamp(EDITOR_MIN_ROWS, EDITOR_MAX_ROWS);
  let mut rows = 1 + editor + 1;
  if draft.expanded {
    let room = width.saturating_sub(label_width(app) + 3);
    let widths: Vec<u16> = option_items(app).iter().map(|(text, _)| text.width() as u16).collect();
    rows += 2 + flow(&widths, room).1;
  }
  rows
}

/// Draws the composer in `area`, the bottom of the message pane whose block is `pane`.
pub fn render<S: Service>(app: &mut App<S>, frame: &mut Frame, area: Rect, pane: Rect) {
  if area.height < 3 {
    return;
  }
  let theme = app.theme;
  let glyphs = app.glyphs;
  let locale = app.locale;
  let focus = app.messages_tab.focus;
  let draft = app.draft();
  let enabled = app.composer_enabled();
  let focused = |control: Control| focus == Focus::Composer(control);

  // The rule above the message box joins the pane's borders, and takes the primary color
  // while the box has the focus.
  let rule_style = if focused(Control::Editor) {
    theme.focused()
  } else {
    theme.muted()
  };
  let rule = Rect::new(pane.x, area.y, pane.width, 1);
  frame.render_widget(Block::new().borders(Borders::TOP).border_style(rule_style), rule);
  let join = if app.messages_tab.focus_in_right_pane() {
    theme.focused()
  } else {
    theme.muted()
  };
  let buffer = frame.buffer_mut();
  buffer[(pane.x, area.y)].set_symbol("├").set_style(join);
  buffer[(pane.right() - 1, area.y)].set_symbol("┤").set_style(join);

  let editor_width = area.width.saturating_sub(2);
  app.messages_tab.editor_width = editor_width;
  let editor_rows = (draft.body.rows(editor_width) as u16).clamp(EDITOR_MIN_ROWS, EDITOR_MAX_ROWS);
  let editor = Rect::new(area.x + 1, area.y + 1, editor_width, editor_rows.min(area.height - 2));
  if draft.body.text().is_empty() {
    let reason = if app.selected_topic.is_none() {
      t(locale, "composer.noTopic")
    } else if !enabled && app.status.state != "Connected" {
      t(locale, "composer.disconnected")
    } else if draft.as_json {
      t(locale, "tui.composer.placeholderJson")
    } else {
      t(locale, "tui.composer.placeholder")
    };
    frame.render_widget(
      Paragraph::new(reason).style(theme.muted()).wrap(Wrap { trim: true }),
      editor,
    );
    if focused(Control::Editor) {
      frame.set_cursor_position((editor.x, editor.y));
    }
  } else {
    let style = if enabled { Style::new() } else { theme.disabled() };
    draft.body.render(frame, editor, style, focused(Control::Editor));
  }
  app.hits.push((editor, Action::Composer(Control::Editor)));

  // The controls row, right-aligned: Level, More Options, Send.
  let controls_y = editor.bottom();
  if controls_y >= area.bottom() {
    return;
  }
  let level = &LEVELS[draft.level];
  let level_color = theme
    .severity(level)
    .map_or(Style::new(), |color| Style::new().fg(color));
  let options_marker = if draft.expanded {
    glyphs.expanded
  } else {
    glyphs.collapsed
  };
  let mut buttons = vec![
    (
      format!(
        "[{} {}]",
        t(locale, &format!("levels.{}", level.as_str())),
        glyphs.expanded
      ),
      Control::Level,
      level_color,
    ),
    (
      format!("[{} {options_marker}]", t(locale, "composer.options")),
      Control::Options,
      Style::new(),
    ),
    (
      format!("[{}]", t(locale, "composer.send")),
      Control::Send,
      Style::new().fg(theme.primary),
    ),
  ];
  let total: u16 = buttons.iter().map(|(text, _, _)| text.width() as u16 + 1).sum();
  if total > area.width {
    let cut = usize::from(total - area.width);
    let label = t(locale, "composer.options");
    let short = truncate(&label, label.width().saturating_sub(cut), glyphs.ellipsis);
    buttons[1].0 = format!("[{short} {options_marker}]");
  }
  let total: u16 = buttons.iter().map(|(text, _, _)| text.width() as u16 + 1).sum();
  let mut x = area.right().saturating_sub(total).max(area.x);
  let mut level_x = x;
  for (text, control, style) in buttons {
    let mut style = if app.control_enabled(control) {
      style
    } else {
      theme.disabled()
    };
    if focused(control) {
      style = style.add_modifier(Modifier::REVERSED);
    }
    let width = text.width() as u16;
    let rect = Rect::new(x, controls_y, width, 1).intersection(area);
    frame.render_widget(Line::from(Span::styled(text, style)), rect);
    app.hits.push((rect, Action::Composer(control)));
    if control == Control::Level {
      level_x = x;
    }
    x += width + 1;
  }

  if draft.expanded {
    let labels = label_width(app);
    let field_x = area.x + 1 + labels + 1;
    let field_width = area.right().saturating_sub(field_x + 1);
    let mut y = controls_y + 1;
    for (key, control, input) in [
      ("composer.topic", Control::Topic, &draft.topic),
      ("composer.title", Control::Title, &draft.title),
    ] {
      if y >= area.bottom() {
        break;
      }
      let label = t(locale, key);
      let label_style = if focused(control) {
        theme.active()
      } else {
        theme.muted()
      };
      frame.render_widget(
        Line::from(Span::styled(
          format!("{}{label}", " ".repeat(usize::from(labels) - label.width())),
          label_style,
        )),
        Rect::new(area.x + 1, y, labels, 1),
      );
      let field = Rect::new(field_x, y, field_width, 1);
      let style = if app.control_enabled(control) {
        Style::new().add_modifier(Modifier::UNDERLINED)
      } else {
        theme.disabled().add_modifier(Modifier::UNDERLINED)
      };
      frame.render_widget(Line::styled(" ".repeat(usize::from(field_width)), style), field);
      input.render(frame, field, style, false, focused(control));
      app.hits.push((field, Action::Composer(control)));
      y += 1;
    }
    if y < area.bottom() {
      let label = t(locale, "composer.qos");
      frame.render_widget(
        Line::from(Span::styled(
          format!("{}{label}", " ".repeat(usize::from(labels) - label.width())),
          if focused(Control::Qos) {
            theme.active()
          } else {
            theme.muted()
          },
        )),
        Rect::new(area.x + 1, y, labels, 1),
      );
      let items = option_items(app);
      let widths: Vec<u16> = items.iter().map(|(text, _)| text.width() as u16).collect();
      let room = area.right().saturating_sub(field_x + 1);
      let (placed, _) = flow(&widths, room);
      for (index, ((text, action), (row, column))) in items.into_iter().zip(placed).enumerate() {
        let item_y = y + row;
        if item_y >= area.bottom() {
          break;
        }
        let control = match index {
          0..=3 => Control::Qos,
          4 => Control::Retain,
          _ => Control::Json,
        };
        let mut style = if app.control_enabled(control) {
          Style::new()
        } else {
          theme.disabled()
        };
        let selected_radio = index <= 3 && QOS_CHOICES[index] == draft.qos;
        if focused(control) && (control != Control::Qos || selected_radio) {
          style = style.add_modifier(Modifier::REVERSED);
        }
        let width = text.width() as u16;
        let rect = Rect::new(field_x + column, item_y, width, 1).intersection(area);
        frame.render_widget(Line::from(Span::styled(text, style)), rect);
        if let Some(action) = action {
          app.hits.push((rect, action));
        }
      }
    }
  }

  // The open Level popup, above its button.
  if let Some(highlighted) = app.messages_tab.composer.popup {
    let labels: Vec<String> = LEVELS
      .iter()
      .map(|level| t(locale, &format!("levels.{}", level.as_str())))
      .collect();
    let width = labels.iter().map(|label| label.width()).max().unwrap_or(0) as u16 + 4;
    let height = LEVELS.len() as u16 + 2;
    let popup = Rect::new(
      level_x.min(area.right().saturating_sub(width)),
      controls_y.saturating_sub(height),
      width,
      height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
      .border_type(BorderType::Rounded)
      .border_style(theme.focused())
      .style(theme.base());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    for (index, (label, level)) in labels.into_iter().zip(LEVELS.iter()).enumerate() {
      let mut style = theme
        .severity(level)
        .map_or(Style::new(), |color| Style::new().fg(color));
      if index == highlighted {
        style = style.add_modifier(Modifier::REVERSED);
      }
      let rect = Rect::new(inner.x, inner.y + index as u16, inner.width, 1);
      frame.render_widget(Line::from(Span::styled(format!(" {label} "), style)), rect);
      app.hits.push((rect, Action::ComposerLevel(index)));
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_qos_row_wraps_the_checkboxes_when_they_do_not_fit() {
    let widths = [10, 5, 5, 5, 18, 15];
    let (placed, rows) = flow(&widths, 80);
    assert_eq!(rows, 1);
    assert_eq!(placed[1], (0, 11));
    assert_eq!(placed[4], (0, 30));
    let (placed, rows) = flow(&widths, 40);
    assert_eq!(rows, 2);
    assert_eq!(placed[4], (1, 0));
    assert_eq!(placed[5], (1, 20));
  }

  #[test]
  fn leading_slashes_go_as_they_are_typed() {
    for typed in ["ci", "/ci", "///ci"] {
      let mut input = TextInput::new(typed);
      strip_leading_slashes(&mut input);
      assert_eq!(input.text(), "ci");
    }
  }
}
