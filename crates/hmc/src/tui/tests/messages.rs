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

//! The Messages tab: the topic tree, the chat view, and the composer, mirroring
//! `TopicTree.test.tsx`, `MessageView.test.tsx`, and `Composer.test.tsx`.

use std::path::Path;

use hiveme_core::config::DisplayMode;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

use super::super::app::Outcome;
use super::super::messages::{Control, Focus};
use super::super::theme::Theme;
use super::*;

/// At 120 x 40 the list is drawn from row 5 to row 32, the composer below it.
const LIST_TOP: u16 = 5;
const LIST_BOTTOM: u16 = 33;

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Input {
  Input::Terminal(Event::Mouse(MouseEvent {
    kind,
    column,
    row,
    modifiers: KeyModifiers::NONE,
  }))
}

fn type_text<S: Service>(app: &mut App<S>, text: &str) {
  for character in text.chars() {
    press(app, key(KeyCode::Char(character)));
  }
}

/// A connected session in English and the application on it.
fn connected() -> (Arc<Scripted>, App<Scripted>, Receivers) {
  let service = Scripted::in_language("en-US");
  service.set_state("Connected");
  let (app, receivers) = open_app(&service);
  (service, app, receivers)
}

/// Hands the application the answer of what it spawned.
async fn settle<S: Service>(app: &mut App<S>, receivers: &mut Receivers) {
  let outcome = receivers.outcomes.recv().await.unwrap();
  app.on_outcome(outcome, Instant::now());
}

/// Renders a frame and hands back the buffer itself, for styles and positions.
fn render_buffer<S: Service>(app: &mut App<S>, width: u16, height: u16) -> Buffer {
  let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
  terminal.draw(|frame| layout::render(app, frame)).unwrap();
  terminal.backend().buffer().clone()
}

/// Where `text` starts on screen, counting a wide glyph as the cells it takes.
fn locate(buffer: &Buffer, text: &str, rows: std::ops::Range<u16>) -> Option<(u16, u16)> {
  for y in rows {
    let mut line = String::new();
    let mut columns = Vec::new();
    let mut skip = 0;
    for x in 0..buffer.area.width {
      if skip > 0 {
        skip -= 1;
        continue;
      }
      let symbol = buffer[(x, y)].symbol();
      skip = symbol.width().saturating_sub(1);
      columns.extend(std::iter::repeat_n(x, symbol.len()));
      line.push_str(symbol);
    }
    if let Some(offset) = line.find(text) {
      return Some((columns[offset], y));
    }
  }
  None
}

fn text_of(buffer: &Buffer) -> Vec<String> {
  buffer_rows(buffer)
}

#[test]
fn every_message_fixture_is_a_bubble_both_ways_in_both_forced_modes() {
  let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../hiveme-core/tests/fixtures/message");
  let mut fixtures: Vec<_> = std::fs::read_dir(&directory)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .collect();
  fixtures.sort();
  assert_eq!(fixtures.len(), 9, "a new fixture needs its expectations here");

  for path in fixtures {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let payload = std::fs::read(&path).unwrap();
    for outgoing in [false, true] {
      for mode in [DisplayMode::Light, DisplayMode::Dark] {
        let mut config = Config::default();
        config.gui.display_mode = mode;
        let theme = Theme::from_gui(&config.gui);
        let service = Scripted::new(config);
        service.keep("hiveme/fixtures", &payload, outgoing);
        let (mut app, _receivers) = open_app(&service);
        app.messages_tab.focus = Focus::List;
        press(&mut app, key(KeyCode::End));
        let buffer = render_buffer(&mut app, 120, 40);
        let screen = text_of(&buffer).join("\n");
        let context = format!("{name} outgoing={outgoing} {mode:?}:\n{screen}");

        // Incoming bubbles start at the list's left margin, outgoing ones end at its right.
        let (x, y) = if outgoing {
          locate(&buffer, "╮", LIST_TOP..LIST_BOTTOM).expect(&context)
        } else {
          locate(&buffer, "╭", LIST_TOP..LIST_BOTTOM).expect(&context)
        };
        assert_eq!(x, if outgoing { 117 } else { 35 }, "{context}");

        let level = match name.as_str() {
          "success.json" => Level::Success,
          _ => Level::Info,
        };
        let severity = theme.severity(&level);
        assert_eq!(
          buffer[(x, y)].bg,
          theme.bubble(severity, outgoing).bg.unwrap(),
          "{context}"
        );
        if let Some(color) = severity {
          assert_eq!(
            buffer[(x, y)].fg,
            color,
            "the border takes the level's color: {context}"
          );
        }

        let metadata = text_of(&buffer)
          .into_iter()
          .find(|line| line.contains("QoS 1"))
          .unwrap_or_else(|| panic!("the focused row shows its metadata: {context}"));
        assert!(
          metadata.contains("fixtures  "),
          "the relative topic comes first: {context}"
        );

        let expected: &[&str] = match name.as_str() {
          "invalid_utf8.bin" => &["10 bytes", "ff fe 00 01 62 69 6e 61 72 79"],
          "minimal.json" => &["only the required keys"],
          "newer_version.json" => &["Heads up", "written by a future HiveMe", "newer version"],
          "raw_json.json" => &["temperature: 21.5", "unit: \"C\"", "▸ sensor: {1}"],
          "raw_text.txt" => &["plain text from some other tool"],
          "success.json" => &["Build succeeded", "Success"],
          "unknown_alg.json" => &["🔒 encrypted (key k-2027-01)"],
          "unknown_fields.json" => &["still readable"],
          "unknown_level.json" => &["from a newer writer", "catastrophe (Info)"],
          other => panic!("no expectations for {other}"),
        };
        for text in expected {
          assert!(screen.contains(text), "{text}: {context}");
        }
        assert!(!screen.contains("kitchen"), "the tree starts collapsed: {context}");
        assert!(!screen.contains("ciphertext"), "{context}");

        if name == "newer_version.json" {
          let (x, y) = locate(&buffer, "Heads up", LIST_TOP..LIST_BOTTOM).unwrap();
          assert!(
            buffer[(x, y)].modifier.contains(Modifier::BOLD),
            "the title is bold: {context}"
          );
        }
        let named = matches!(
          name.as_str(),
          "newer_version.json" | "unknown_alg.json" | "unknown_fields.json"
        );
        assert_eq!(
          locate(&buffer, "sams-macbook", LIST_TOP..LIST_BOTTOM).is_some(),
          named && !outgoing,
          "only an incoming message names its sender: {context}"
        );
        assert!(
          !screen.contains(" hmc") && !screen.contains(" hmg"),
          "never the app: {context}"
        );
      }
    }
  }
}

#[test]
fn a_data_tree_opens_with_space_and_node_by_node_in_the_detail_view() {
  let service = Scripted::in_language("en-US");
  let payload = br#"{"v":1,"id":"with-data","ts":"2026-09-13T09:41:00Z","payload":{"body":"Deployed","title":"CI","data":{"stage":"deploy","nested":{"deep":true}}}}"#;
  service.keep("hiveme", payload, false);
  let (mut app, _receivers) = open_app(&service);
  app.messages_tab.focus = Focus::List;
  press(&mut app, key(KeyCode::End));

  let screen = render(&mut app, 120, 40).join("\n");
  for text in ["CI", "Deployed", "▾ data: {2}", "stage: \"deploy\"", "▸ nested: {1}"] {
    assert!(screen.contains(text), "{text}:\n{screen}");
  }
  assert!(
    screen.find("stage").unwrap() < screen.find("nested").unwrap(),
    "the keys keep the order they were written in"
  );
  assert!(!screen.contains("deep: true"));

  press(&mut app, key(KeyCode::Char(' ')));
  assert!(
    render(&mut app, 120, 40).join("\n").contains("deep: true"),
    "Space opens every branch"
  );
  press(&mut app, key(KeyCode::Char(' ')));
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("▸ data: {2}"), "and closes them all again:\n{screen}");

  press(&mut app, key(KeyCode::Enter));
  assert!(app.messages_tab.view.detail.is_some());
  press(&mut app, key(KeyCode::Char(' ')));
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Char(' ')));
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(
    screen.contains("deep: true"),
    "the cursor opened data, then nested:\n{screen}"
  );

  press(&mut app, key(KeyCode::Esc));
  assert!(app.messages_tab.view.detail.is_none());
  assert_eq!(app.messages_tab.focus, Focus::List, "Esc closes the detail view first");
  assert!(
    render(&mut app, 120, 40).join("\n").contains("deep: true"),
    "the bubble kept it open"
  );
}

#[test]
fn the_tree_rolls_badges_up_selects_by_label_and_toggles_by_marker() {
  let service = Scripted::in_language("en-US");
  service.keep("hiveme/build/ci", b"one", false);
  service.keep("sensors/kitchen", b"21.5", false);
  service.keep("sensors/kitchen", b"21.6", false);
  service.keep("sensors/garage", b"9.0", false);
  let (mut app, _receivers) = open_app(&service);

  let screen = render(&mut app, 120, 40);
  let tree: Vec<String> = screen[6..12]
    .iter()
    .map(|row| row.chars().take(32).collect::<String>().trim_end().to_owned())
    .collect();
  assert_eq!(
    tree,
    [
      "│▾ hiveme",
      "│  ▾ build",
      "│      ci",
      "│▾ sensors (3)",
      "│    garage (1)",
      "│    kitchen (2)"
    ],
    "hiveme was read at startup; the rest is rolled up"
  );

  let (x, y) = target(&app, &Action::ToggleTopic("sensors".to_owned()));
  press(&mut app, click(x, y));
  assert!(
    !render(&mut app, 120, 40).join("\n").contains("kitchen"),
    "the marker closes it"
  );
  let (x, y) = target(&app, &Action::SelectTopic("sensors".to_owned()));
  press(&mut app, click(x + 3, y));
  assert_eq!(app.selected_topic.as_deref(), Some("sensors"));
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(!screen.contains("kitchen"), "selecting leaves the expansion alone");
  assert!(!screen.contains("sensors (3)"), "and marks the subtree read:\n{screen}");
  assert_eq!(app.selected_messages().len(), 3, "the whole subtree is shown");

  // Keys: Right opens, Down moves, Enter selects.
  press(&mut app, key(KeyCode::Right));
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Enter));
  assert_eq!(app.selected_topic.as_deref(), Some("sensors/garage"));
  press(&mut app, key(KeyCode::Left));
  press(&mut app, key(KeyCode::Left));
  assert_eq!(
    app.messages_tab.tree.cursor.as_deref(),
    Some("sensors"),
    "Left goes to the parent, then closes it"
  );
}

#[test]
fn a_filter_keeps_parents_opens_what_it_found_and_never_hides_hiveme() {
  let service = Scripted::in_language("en-US");
  service.keep("hiveme/build/ci", b"one", false);
  service.keep("sensors/kitchen", b"21.5", false);
  let (mut app, _receivers) = open_app(&service);
  let (x, y) = target(app_rendered(&mut app), &Action::ToggleTopic("hiveme/build".to_owned()));
  press(&mut app, click(x, y));

  press(&mut app, key(KeyCode::Char('/')));
  assert_eq!(app.messages_tab.focus, Focus::Filter);
  type_text(&mut app, "CI");
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("│> CI"), "{screen}");
  assert!(
    screen.contains("ci"),
    "the closed branch was opened for the match:\n{screen}"
  );
  assert!(!screen.contains("sensors"), "{screen}");

  press(&mut app, key(KeyCode::Backspace));
  press(&mut app, key(KeyCode::Backspace));
  type_text(&mut app, "nowhere");
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("No topic matches the filter."), "{screen}");
  assert!(screen.contains("│▾ hiveme") || screen.contains("│  hiveme"), "{screen}");

  press(&mut app, key(KeyCode::Esc));
  assert_eq!(app.messages_tab.focus, Focus::Tree);
}

/// Renders once so that the click targets exist, and hands the application back.
fn app_rendered(app: &mut App<Scripted>) -> &App<Scripted> {
  render(app, 120, 40);
  app
}

#[test]
fn a_live_message_appears_and_raises_the_badge_until_its_topic_is_selected() {
  let (service, mut app, mut receivers) = connected();
  let sender = Sender::from_device(&Config::new_for_this_device().device, "hmc");
  service.deliver(
    "hiveme/ci",
    &Message::new_text(sender, "Disk full").with_level(Level::Error),
  );
  pump(&mut app, &mut receivers);

  let screen = render(&mut app, 120, 40).join("\n");
  assert!(
    screen.contains("Disk full"),
    "the selected subtree shows it live:\n{screen}"
  );
  assert!(screen.contains("hiveme (1)") && screen.contains("ci (1)"), "{screen}");

  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Enter));
  assert_eq!(app.selected_topic.as_deref(), Some("hiveme/ci"));
  assert!(!render(&mut app, 120, 40).join("\n").contains("(1)"));
}

#[tokio::test]
async fn enter_sends_exactly_once_from_every_composer_control() {
  for control in [
    Control::Editor,
    Control::Level,
    Control::Options,
    Control::Send,
    Control::Topic,
    Control::Title,
    Control::Qos,
    Control::Retain,
    Control::Json,
  ] {
    let (service, mut app, mut receivers) = connected();
    // A refused publish keeps the text, so a control that also acted would send twice.
    *service.publish_error.lock().unwrap() = Some("not today".to_owned());
    app.messages_tab.focus = Focus::Composer(Control::Editor);
    type_text(&mut app, "Build finished");
    app.messages_tab.focus = Focus::Composer(Control::Options);
    press(&mut app, key(KeyCode::Char(' ')));
    assert!(app.draft().expanded);

    app.messages_tab.focus = Focus::Composer(control);
    press(&mut app, key(KeyCode::Enter));
    settle(&mut app, &mut receivers).await;
    let published = service.published.lock().unwrap().clone();
    assert_eq!(
      published,
      [(
        "hiveme".to_owned(),
        "Build finished".to_owned(),
        PublishOptions {
          topic: None,
          json: false,
          qos: None,
          retain: Some(false),
          title: None,
          level: Some("info".to_owned()),
        }
      )],
      "{control:?}"
    );
    assert!(app.draft().expanded, "{control:?}");
    assert_eq!(app.draft().body.text(), "Build finished", "a failure keeps the text");
    assert!(
      app
        .snackbar
        .as_ref()
        .is_some_and(|snackbar| snackbar.error && snackbar.text.contains("not today"))
    );
  }
}

#[test]
fn the_newline_keys_add_lines_and_blank_text_is_not_sent() {
  let (service, mut app, _receivers) = connected();
  app.messages_tab.focus = Focus::Composer(Control::Editor);
  type_text(&mut app, "first");
  press(&mut app, chord(KeyCode::Enter, KeyModifiers::ALT));
  type_text(&mut app, "second");
  press(&mut app, chord(KeyCode::Char('j'), KeyModifiers::CONTROL));
  type_text(&mut app, "third");
  press(&mut app, chord(KeyCode::Enter, KeyModifiers::SHIFT));
  assert_eq!(app.draft().body.text(), "first\nsecond\nthird\n");
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("│ first") && screen.contains("│ third"), "{screen}");

  let (service_blank, mut blank, _receivers) = connected();
  blank.messages_tab.focus = Focus::Composer(Control::Editor);
  type_text(&mut blank, "   ");
  press(&mut blank, key(KeyCode::Enter));
  assert!(service_blank.published.lock().unwrap().is_empty());
  assert!(service.published.lock().unwrap().is_empty());
}

#[test]
fn the_composer_says_why_it_cannot_be_used_and_takes_no_text() {
  let service = Scripted::in_language("en-US");
  let (mut app, _receivers) = open_app(&service);
  assert!(
    render(&mut app, 80, 24)
      .join("\n")
      .contains("Connect to the broker to send")
  );
  app.messages_tab.focus = Focus::Composer(Control::Editor);
  type_text(&mut app, "x");
  assert_eq!(app.draft().body.text(), "");

  let (_service, mut app, _receivers) = connected();
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(
    screen.contains("Write a message. Enter sends, Alt+Enter adds a line."),
    "{screen}"
  );
  assert!(screen.contains("[Info ▾] [More Options ▸] [Send]"), "{screen}");
}

#[test]
fn tab_goes_round_the_filter_the_tree_the_list_and_the_usable_controls() {
  let (_service, mut app, _receivers) = connected();
  let mut seen = Vec::new();
  for _ in 0..6 {
    press(&mut app, key(KeyCode::Tab));
    seen.push(app.messages_tab.focus);
  }
  assert_eq!(
    seen,
    [
      Focus::List,
      Focus::Composer(Control::Editor),
      Focus::Composer(Control::Level),
      Focus::Composer(Control::Options),
      // Send is skipped while there is nothing to send.
      Focus::Filter,
      Focus::Tree,
    ]
  );
}

#[test]
fn drafts_options_and_expansion_are_kept_per_topic_across_tabs_and_reconnects() {
  let service = Scripted::in_language("en-US");
  service.set_state("Connected");
  service.keep("hiveme/child", b"hello", false);
  let (mut app, _receivers) = open_app(&service);

  app.messages_tab.focus = Focus::Composer(Control::Editor);
  type_text(&mut app, "Root draft");
  app.messages_tab.focus = Focus::Composer(Control::Options);
  press(&mut app, key(KeyCode::Char(' ')));
  app.messages_tab.focus = Focus::Composer(Control::Title);
  type_text(&mut app, "Root title");
  app.messages_tab.focus = Focus::Composer(Control::Level);
  press(&mut app, key(KeyCode::Char(' ')));
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Enter));
  app.messages_tab.focus = Focus::Composer(Control::Retain);
  press(&mut app, key(KeyCode::Char(' ')));

  render(&mut app, 120, 40);
  let (x, y) = target(&app, &Action::SelectTopic("hiveme/child".to_owned()));
  press(&mut app, click(x + 3, y));
  assert_eq!(app.selected_topic.as_deref(), Some("hiveme/child"));
  assert_eq!(
    app.draft(),
    Default::default(),
    "a topic not used yet starts with the defaults"
  );
  app.messages_tab.focus = Focus::Composer(Control::Editor);
  type_text(&mut app, "Child draft");

  press(&mut app, key(KeyCode::F(10)));
  press(&mut app, chord(KeyCode::Char('1'), KeyModifiers::ALT));
  service.set_state("Disconnected");
  app.set_status(service.status(), Instant::now());
  service.set_state("Connected");
  app.set_status(service.status(), Instant::now());
  assert_eq!(
    app.draft().body.text(),
    "Child draft",
    "after a tab switch and a reconnect"
  );

  render(&mut app, 120, 40);
  let (x, y) = target(&app, &Action::SelectTopic("hiveme".to_owned()));
  press(&mut app, click(x + 3, y));
  let root = app.draft();
  assert_eq!(root.body.text(), "Root draft");
  assert_eq!(root.title.text(), "Root title");
  assert_eq!(root.level, 1, "Error");
  assert!(root.retain && root.expanded);
}

#[tokio::test]
async fn raw_json_disables_level_and_title_and_keeps_their_values() {
  let (service, mut app, mut receivers) = connected();
  app.messages_tab.focus = Focus::Composer(Control::Options);
  press(&mut app, key(KeyCode::Char(' ')));
  app.messages_tab.focus = Focus::Composer(Control::Title);
  type_text(&mut app, "Saved title");
  app.messages_tab.focus = Focus::Composer(Control::Level);
  press(&mut app, key(KeyCode::Char(' ')));
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Enter));
  assert_eq!(app.draft().level, 2, "Success");
  app.messages_tab.focus = Focus::Composer(Control::Json);
  press(&mut app, key(KeyCode::Char(' ')));
  assert!(!app.control_enabled(Control::Level) && !app.control_enabled(Control::Title));
  assert!(render(&mut app, 120, 40).join("\n").contains("Write a JSON payload."));

  app.messages_tab.focus = Focus::Composer(Control::Editor);
  press(&mut app, Input::Terminal(Event::Paste("{\"ok\":true}".to_owned())));
  press(&mut app, key(KeyCode::Enter));
  settle(&mut app, &mut receivers).await;
  let (_, body, options) = service.published.lock().unwrap()[0].clone();
  assert_eq!(body, "{\"ok\":true}");
  assert!(options.json && options.title.is_none() && options.level.is_none());
  assert_eq!(app.draft().body.text(), "", "sent");

  app.messages_tab.focus = Focus::Composer(Control::Json);
  press(&mut app, key(KeyCode::Char(' ')));
  let draft = app.draft();
  assert_eq!((draft.title.text(), draft.level), ("Saved title", 2));
  assert!(app.control_enabled(Control::Level) && app.control_enabled(Control::Title));
}

#[tokio::test]
async fn enter_in_the_level_popup_picks_a_level_and_enter_on_the_select_sends_it() {
  let (service, mut app, mut receivers) = connected();
  app.messages_tab.focus = Focus::Composer(Control::Editor);
  type_text(&mut app, "Build finished");
  app.messages_tab.focus = Focus::Composer(Control::Level);
  press(&mut app, key(KeyCode::Char(' ')));
  let screen = render(&mut app, 120, 40).join("\n");
  for level in ["│ Info ", "│ Error ", "│ Success ", "│ Warn "] {
    assert!(screen.contains(level), "{level}:\n{screen}");
  }
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Enter));
  assert!(service.published.lock().unwrap().is_empty(), "picking does not send");
  assert!(render(&mut app, 120, 40).join("\n").contains("[Error ▾]"));

  press(&mut app, key(KeyCode::Enter));
  settle(&mut app, &mut receivers).await;
  pump(&mut app, &mut receivers);
  assert_eq!(service.published.lock().unwrap()[0].2.level.as_deref(), Some("error"));
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(
    screen.contains("Build finished"),
    "the acknowledged message is a bubble:\n{screen}"
  );
  assert_eq!(app.selected_messages().len(), 1, "and the echo did not double it");
}

#[tokio::test]
async fn the_topic_is_relative_to_the_selection_and_the_options_stay_in_effect_collapsed() {
  let (service, mut app, mut receivers) = connected();
  app.select_topic("hiveme/build", Instant::now());
  app.messages_tab.focus = Focus::Composer(Control::Options);
  press(&mut app, key(KeyCode::Char(' ')));
  app.messages_tab.focus = Focus::Composer(Control::Topic);
  type_text(&mut app, "///ci");
  assert_eq!(app.draft().topic.text(), "ci");
  app.messages_tab.focus = Focus::Composer(Control::Qos);
  press(&mut app, key(KeyCode::Right));
  press(&mut app, key(KeyCode::Right));
  app.messages_tab.focus = Focus::Composer(Control::Retain);
  press(&mut app, key(KeyCode::Char(' ')));
  app.messages_tab.focus = Focus::Composer(Control::Options);
  press(&mut app, key(KeyCode::Char(' ')));
  assert!(!app.draft().expanded);

  app.messages_tab.focus = Focus::Composer(Control::Editor);
  type_text(&mut app, "Relative message");
  press(&mut app, key(KeyCode::Enter));
  settle(&mut app, &mut receivers).await;
  let (topic, _, options) = service.published.lock().unwrap()[0].clone();
  assert_eq!(topic, "hiveme/build");
  assert_eq!(
    (options.topic.as_deref(), options.qos, options.retain),
    (Some("ci"), Some(1), Some(true))
  );
  assert_eq!(app.selected_messages()[0].topic, "hiveme/build/ci");
  let draft = app.draft();
  assert_eq!(
    (draft.topic.text(), draft.qos, draft.retain),
    ("ci", Some(1), true),
    "only the text is cleared"
  );
}

#[tokio::test]
async fn a_send_that_finishes_after_the_selection_moved_clears_only_its_own_draft() {
  let (_service, mut app, mut receivers) = connected();
  app.select_topic("hiveme/other", Instant::now());
  app.messages_tab.focus = Focus::Composer(Control::Editor);
  type_text(&mut app, "Other draft");
  app.select_topic("hiveme", Instant::now());
  type_text(&mut app, "Root message");
  press(&mut app, key(KeyCode::Enter));
  assert!(!app.composer_enabled(), "nothing else is sent while one is on its way");

  app.select_topic("hiveme/other", Instant::now());
  settle(&mut app, &mut receivers).await;
  assert_eq!(app.draft().body.text(), "Other draft");
  app.select_topic("hiveme", Instant::now());
  assert_eq!(app.draft().body.text(), "");
  assert!(app.composer_enabled());
}

#[test]
fn paging_loads_the_previous_page_at_the_top_until_there_is_none() {
  let service = Scripted::in_language("en-US");
  for index in 0..450 {
    let topic = ["hiveme/a", "hiveme/b", "hiveme/c"][index % 3];
    service.keep(topic, format!("message {index}").as_bytes(), false);
  }
  let (mut app, _receivers) = open_app(&service);
  let first = |app: &App<Scripted>| app.selected_messages()[0].body.clone();
  assert_eq!(app.selected_messages().len(), 200, "the first page is the newest 200");
  assert_eq!(first(&app), "message 250");
  assert_eq!(app.has_older.get("hiveme"), Some(&true));

  app.messages_tab.focus = Focus::List;
  render(&mut app, 120, 40);
  let (x, y) = (60, 20);
  press(&mut app, mouse(MouseEventKind::ScrollUp, x, y));
  assert!(
    !app.messages_tab.view.follow,
    "the wheel scrolls away from the newest row"
  );

  press(&mut app, key(KeyCode::Home));
  assert_eq!(app.messages_tab.view.focused, Some(app.selected_messages()[0].row_id));
  press(&mut app, key(KeyCode::PageUp));
  assert_eq!(app.selected_messages().len(), 400);
  assert_eq!(first(&app), "message 50");
  assert_eq!(app.has_older.get("hiveme"), Some(&true));
  let focused: usize = app.focused_row().unwrap().body["message ".len()..].parse().unwrap();
  assert!(
    focused < 250,
    "the page moved into the older rows, at message {focused}"
  );

  press(&mut app, key(KeyCode::Home));
  press(&mut app, key(KeyCode::PageUp));
  assert_eq!(app.selected_messages().len(), 450);
  assert_eq!(first(&app), "message 0");
  assert_eq!(app.has_older.get("hiveme"), Some(&false), "the last page was short");
  press(&mut app, key(KeyCode::Home));
  press(&mut app, key(KeyCode::PageUp));
  assert_eq!(app.selected_messages().len(), 450, "nothing more is asked for");
  assert!(app.snackbar.is_none());

  press(&mut app, key(KeyCode::End));
  assert!(app.messages_tab.view.follow);
  assert!(render(&mut app, 120, 40).join("\n").contains("message 449"));
}

#[test]
fn c_and_r_copy_the_focused_message_and_the_snackbar_says_so() {
  static COPIED: Mutex<Vec<String>> = Mutex::new(Vec::new());
  fn copier(text: &str) -> std::result::Result<(), String> {
    COPIED.lock().unwrap().push(text.to_owned());
    Ok(())
  }

  let service = Scripted::in_language("en-US");
  let sender = Sender::from_device(&Config::new_for_this_device().device, "hmc");
  let message = Message::new_text(sender, "Original text\nwith a second line");
  let row = service.keep("hiveme", &message.to_bytes().unwrap(), false);
  let (app, _receivers) = open_app(&service);
  let mut app = app.with_copier(copier);
  app.messages_tab.focus = Focus::List;
  press(&mut app, key(KeyCode::Char('c')));
  assert!(COPIED.lock().unwrap().is_empty(), "nothing is focused yet");

  press(&mut app, key(KeyCode::Up));
  press(&mut app, key(KeyCode::Char('c')));
  assert_eq!(COPIED.lock().unwrap().last(), Some(&row.body));
  let snackbar = app.snackbar.clone().unwrap();
  assert_eq!(snackbar.text, "The body is on the clipboard.");
  assert!(!snackbar.error);
  let buffer = render_buffer(&mut app, 80, 24);
  let (x, y) = locate(&buffer, "The body is on the clipboard.", 0..1).unwrap();
  assert_eq!(
    buffer[(x, y)].bg,
    app.theme.success,
    "a confirmation is in the success color"
  );

  press(&mut app, key(KeyCode::Char('r')));
  assert_eq!(COPIED.lock().unwrap().last(), Some(&row.raw));
  assert_eq!(app.snackbar.as_ref().unwrap().text, "The payload is on the clipboard.");

  let (app, _receivers) = open_app(&service);
  let mut app = app.with_copier(|_| Err("Clipboard unavailable".to_owned()));
  app.messages_tab.focus = Focus::List;
  press(&mut app, key(KeyCode::End));
  press(&mut app, key(KeyCode::Char('c')));
  let snackbar = app.snackbar.clone().unwrap();
  assert_eq!(
    (snackbar.text.as_str(), snackbar.error),
    ("Clipboard unavailable", true)
  );
}

#[test]
fn the_divider_moves_with_ctrl_arrows_and_a_drag_within_its_bounds() {
  let (_service, mut app, _receivers) = connected();
  assert_eq!(app.messages_tab.split, 28);
  press(&mut app, chord(KeyCode::Right, KeyModifiers::CONTROL));
  assert_eq!(app.messages_tab.split, 30);
  for _ in 0..20 {
    press(&mut app, chord(KeyCode::Left, KeyModifiers::CONTROL));
  }
  assert_eq!(app.messages_tab.split, 15);

  render(&mut app, 100, 30);
  let (x, y) = target(&app, &Action::Divider);
  press(&mut app, mouse(MouseEventKind::Down(MouseButton::Left), x, y));
  assert!(app.messages_tab.dragging);
  press(&mut app, mouse(MouseEventKind::Drag(MouseButton::Left), 49, y));
  assert_eq!(app.messages_tab.split, 50);
  press(&mut app, mouse(MouseEventKind::Drag(MouseButton::Left), 99, y));
  assert_eq!(app.messages_tab.split, 60, "clamped");
  press(&mut app, mouse(MouseEventKind::Up(MouseButton::Left), 99, y));
  press(&mut app, mouse(MouseEventKind::Drag(MouseButton::Left), 20, y));
  assert_eq!(app.messages_tab.split, 60, "a drag after the release does nothing");
}

#[test]
fn the_messages_tab_renders_in_german_japanese_and_chinese_at_the_smallest_size() {
  for (tag, locale) in [("de", Locale::De), ("ja", Locale::Ja), ("zh-CN", Locale::ZhCn)] {
    let service = Scripted::in_language(tag);
    service.set_state("Connected");
    service.keep("hiveme/ci", b"Nightly build 482 finished", false);
    let (mut app, _receivers) = open_app(&service);
    app.messages_tab.focus = Focus::List;
    press(&mut app, key(KeyCode::End));
    let screen = render(&mut app, 80, 24).join("\n");
    for text in [
      t(locale, "topics.filter"),
      t(locale, "composer.send"),
      t_with(locale, "messages.qos", &[("qos", "1")]),
      "Nightly build 482 finished".to_owned(),
    ] {
      assert!(screen.contains(&text), "{tag} {text}:\n{screen}");
    }
  }
}

/// Against the Docker broker of `crates/hmc/tests/publish.rs`: a message sent from the
/// composer is acknowledged, stored once, and shown once after its echo; a message
/// published by the one-shot mode appears live and raises the badge until selected.
#[test]
fn against_a_real_broker_the_composer_and_one_shot_hmc_meet_in_the_view() {
  use testcontainers::core::{IntoContainerPort, WaitFor};
  use testcontainers::runners::AsyncRunner;
  use testcontainers::{GenericImage, ImageExt};

  if std::env::var("HIVEME_SKIP_DOCKER").as_deref() == Ok("1") {
    eprintln!("skipping the broker test: HIVEME_SKIP_DOCKER=1");
    return;
  }
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();
  runtime.block_on(async {
    let container = match GenericImage::new("hivemq/hivemq-ce", "latest")
      .with_exposed_port(1883.tcp())
      .with_wait_for(WaitFor::message_on_stdout("Started HiveMQ"))
      .with_startup_timeout(Duration::from_secs(180))
      .start()
      .await
    {
      Ok(container) => container,
      Err(reason) => {
        eprintln!("skipping the broker test: the HiveMQ CE container did not start ({reason})");
        return;
      }
    };
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(1883.tcp()).await.unwrap();

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    let mut config = Config::new_for_this_device();
    config.device.name = "test-runner".to_owned();
    config.broker.url = format!("mqtt://{host}:{port}");
    config.broker.username = "hiveme".to_owned();
    config.broker.password = "test".to_owned();
    config.broker.connect_timeout_secs = 20;
    config.publish.timeout_secs = 20;
    std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    let toasts = Arc::new(Recorder::default());
    let session = Session::open(Some(&path), SessionApp::Tui, toasts).unwrap();
    let (mut app, mut receivers) = App::new(session.clone(), Glyphs::UNICODE, false, false);
    session.connect().await.expect("the session connects");
    app.set_status(session.status(), Instant::now());
    assert!(app.composer_enabled());

    app.messages_tab.focus = Focus::Composer(Control::Editor);
    type_text(&mut app, "Hello from the terminal");
    press(&mut app, key(KeyCode::Enter));
    let outcome = tokio::time::timeout(Duration::from_secs(30), receivers.outcomes.recv())
      .await
      .expect("the broker acknowledges")
      .unwrap();
    assert!(matches!(outcome, Outcome::Sent { .. }), "{outcome:?}");
    app.on_outcome(outcome, Instant::now());

    // The echo arrives on the session's own subscription and collapses into the row.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while session.status().messages_received == 0 && tokio::time::Instant::now() < deadline {
      tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(session.status().messages_received >= 1, "the echo came back");
    while let Ok(event) = receivers.events.try_recv() {
      app.on_session_event(event, Instant::now());
    }
    assert_eq!(session.messages("hiveme", None, 200).unwrap().len(), 1, "stored once");
    assert_eq!(app.selected_messages().len(), 1, "shown once");
    assert!(render(&mut app, 120, 40).join("\n").contains("Hello from the terminal"));

    // The one-shot mode, in this process, publishes to hiveme/ci.
    let cli = crate::cli::Cli::parse_in(
      Locale::EnUs,
      ["hmc", "--config", path.to_str().unwrap(), "-t", "ci", "Disk full"].map(std::ffi::OsString::from),
    );
    crate::run::run(cli, Locale::EnUs)
      .await
      .expect("the one-shot publish works");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !app.selected_messages().iter().any(|row| row.body == "Disk full") {
      assert!(tokio::time::Instant::now() < deadline, "the message never arrived");
      match tokio::time::timeout(Duration::from_millis(200), receivers.events.recv()).await {
        Ok(Ok(event)) => app.on_session_event(event, Instant::now()),
        _ => continue,
      }
    }
    let screen = render(&mut app, 120, 40).join("\n");
    assert!(screen.contains("Disk full") && screen.contains("ci (1)"), "{screen}");

    let (x, y) = target(&app, &Action::SelectTopic("hiveme/ci".to_owned()));
    press(&mut app, click(x + 3, y));
    assert!(
      !render(&mut app, 120, 40).join("\n").contains("ci (1)"),
      "selected, so read"
    );

    let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, session.shutdown()).await;
    drop(container);
  });
}
