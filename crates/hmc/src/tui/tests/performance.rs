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

//! A large history and a terminal that changes size while it is drawn.
//!
//! The budget is a frame: a key press, a page of history, or a message that arrives is
//! drawn in less time than it takes to notice a stall. It holds in a debug build, so the
//! release build the workflows test has a wide margin.

use super::super::messages::Focus;
use super::*;

/// The history: ten thousand rows spread over two hundred topics.
const ROW_COUNT: usize = 10_000;
const TOPIC_COUNT: usize = 200;

/// The largest screen the budget is held at, 200 columns by 60 rows.
const WIDE: (u16, u16) = (200, 60);

/// The longest a frame, and whatever it draws, may take.
const FRAME_BUDGET: Duration = Duration::from_millis(if cfg!(debug_assertions) { 750 } else { 150 });

/// The longest opening the application on the history may take.
const OPEN_BUDGET: Duration = Duration::from_millis(if cfg!(debug_assertions) { 3_000 } else { 750 });

/// The topic of row `index`: ten sites with twenty nodes each.
fn topic(index: usize) -> String {
  let node = index % TOPIC_COUNT;
  format!("hiveme/site-{}/node-{}", node / 20, node % 20)
}

/// A history with every tier the view draws: envelopes with titles and levels, raw
/// JSON with a tree, and plain text, incoming and outgoing, short and long.
fn large_history() -> Arc<Scripted> {
  let service = Scripted::in_language("en-US");
  service.set_state("Connected");
  let device = Config::new_for_this_device().device;
  for index in 0..ROW_COUNT {
    let topic = topic(index);
    let payload = match index % 4 {
      0 => Message::new_text(
        Sender::from_device(&device, "hmc"),
        format!("Nightly build {index} finished in {} minutes", index % 60),
      )
      .with_title(format!("CI {index}"))
      .with_level(match index % 3 {
        0 => Level::Info,
        1 => Level::Warn,
        _ => Level::Error,
      })
      .to_bytes()
      .unwrap(),
      1 => format!(r#"{{"build":{index},"branch":"main","failed":["config::migrate","tui::render"]}}"#).into_bytes(),
      2 => "a longer line of plain text that has to wrap inside its bubble "
        .repeat(1 + index % 5)
        .into_bytes(),
      _ => format!("message {index}").into_bytes(),
    };
    service.keep(&topic, &payload, index % 7 == 0);
  }
  service
}

/// Draws a frame and says how long it took.
fn timed_draw(app: &mut App<Scripted>, terminal: &mut Terminal<TestBackend>) -> Duration {
  let started = Instant::now();
  terminal.draw(|frame| layout::render(app, frame)).unwrap();
  started.elapsed()
}

/// The slowest of a set of timings, and what it was for.
#[derive(Default)]
struct Slowest {
  duration: Duration,
  what: String,
  frames: usize,
}

impl Slowest {
  fn record(&mut self, what: impl FnOnce() -> String, duration: Duration) {
    self.frames += 1;
    if duration > self.duration {
      self.duration = duration;
      self.what = what();
    }
  }
}

#[test]
fn ten_thousand_rows_on_two_hundred_topics_stay_within_a_frame() {
  let service = large_history();
  let started = Instant::now();
  let (mut app, mut receivers) = open_app(&service);
  let opened = started.elapsed();
  assert!(opened < OPEN_BUDGET, "opening took {opened:?}");
  assert_eq!(app.topics[0].messages as usize, ROW_COUNT);

  let (width, height) = WIDE;
  let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
  let mut slowest = Slowest::default();
  let first = timed_draw(&mut app, &mut terminal);
  slowest.record(|| "the first frame".to_owned(), first);
  let screen = rows(&terminal).join("\n");
  assert!(screen.contains("message 9999"), "the newest row is in view:\n{screen}");
  assert!(screen.contains("node-19"), "the tree is open:\n{screen}");

  // Up through the whole history, a page at a time, the way PageUp at the top asks.
  app.messages_tab.focus = Focus::List;
  while app.has_older.get("hiveme") == Some(&true) {
    press(&mut app, key(KeyCode::Home));
    let started = Instant::now();
    press(&mut app, key(KeyCode::PageUp));
    let duration = started.elapsed() + timed_draw(&mut app, &mut terminal);
    let loaded = app.selected_messages().len();
    slowest.record(|| format!("the page that brought {loaded} rows"), duration);
  }
  assert_eq!(app.selected_messages().len(), ROW_COUNT);
  let screen = rows(&terminal).join("\n");
  assert!(screen.contains("message 3"), "the oldest rows are in view:\n{screen}");

  // Back down, and the keys that move through everything that is loaded.
  for (name, code) in [
    ("End", KeyCode::End),
    ("Up", KeyCode::Up),
    ("PageUp", KeyCode::PageUp),
    ("Home", KeyCode::Home),
    ("PageDown", KeyCode::PageDown),
    ("End", KeyCode::End),
  ] {
    let started = Instant::now();
    press(&mut app, key(code));
    let duration = started.elapsed() + timed_draw(&mut app, &mut terminal);
    slowest.record(|| format!("{name} over {ROW_COUNT} rows"), duration);
  }
  assert!(rows(&terminal).join("\n").contains("message 9999"));

  // Messages arriving live on the loaded history, each with its tree refresh.
  let sender = Sender::from_device(&Config::new_for_this_device().device, "hmc");
  for index in 0..50 {
    service.deliver(
      &topic(index * 7),
      &Message::new_text(sender.clone(), format!("live {index}")).with_level(Level::Warn),
    );
    let started = Instant::now();
    pump(&mut app, &mut receivers);
    let duration = started.elapsed() + timed_draw(&mut app, &mut terminal);
    slowest.record(|| format!("live message {index}"), duration);
  }
  assert!(rows(&terminal).join("\n").contains("live 49"));

  // The tree: down through every node, then selecting the last one, which sorts as text.
  app.messages_tab.focus = Focus::Tree;
  for step in 0..TOPIC_COUNT + 20 {
    let started = Instant::now();
    press(&mut app, key(KeyCode::Down));
    let duration = started.elapsed() + timed_draw(&mut app, &mut terminal);
    slowest.record(|| format!("tree step {step}"), duration);
  }
  let started = Instant::now();
  press(&mut app, key(KeyCode::Enter));
  let duration = started.elapsed() + timed_draw(&mut app, &mut terminal);
  slowest.record(|| "selecting the last node".to_owned(), duration);
  assert_eq!(app.selected_topic.as_deref(), Some("hiveme/site-9/node-9"));

  eprintln!(
    "{} frames over {ROW_COUNT} rows: opened in {opened:?}, slowest {:?} ({})",
    slowest.frames, slowest.duration, slowest.what
  );
  assert!(
    slowest.duration < FRAME_BUDGET,
    "{} took {:?}, over the budget of {FRAME_BUDGET:?}",
    slowest.what,
    slowest.duration
  );
}

#[test]
fn a_terminal_resized_between_frames_is_drawn_whole_at_every_size() {
  let service = large_history();
  let (mut app, _receivers) = open_app(&service);
  app.messages_tab.focus = Focus::List;
  let mut terminal = Terminal::new(TestBackend::new(WIDE.0, WIDE.1)).unwrap();
  terminal.draw(|frame| layout::render(&mut app, frame)).unwrap();
  let too_small = t_with(Locale::EnUs, "tui.tooSmall", &[("columns", "80"), ("rows", "24")]);
  let footer_state = t(Locale::EnUs, "footer.state.Connected");

  let sizes = [
    (80, 24),
    (200, 60),
    (79, 24),
    (120, 40),
    (80, 23),
    (20, 5),
    (1, 1),
    (81, 25),
    WIDE,
  ];
  for (tab, action) in [
    (Tab::Messages, None),
    (Tab::Settings, Some(KeyCode::F(10))),
    (Tab::About, Some(KeyCode::F(1))),
  ] {
    if let Some(code) = action {
      press(&mut app, key(code));
    }
    assert_eq!(app.current_tab(), tab);
    let mut previous = WIDE;
    for (width, height) in sizes {
      // The terminal changes size after the last frame was laid out, so the keys, the
      // wheel, and a click below reach an application that still knows the old one.
      terminal.backend_mut().resize(width, height);
      app.handle_input(Input::Terminal(Event::Resize(width, height)), Instant::now());
      press(&mut app, key(KeyCode::PageUp));
      press(&mut app, mouse_wheel(previous.0 / 2, previous.1 / 2));
      press(
        &mut app,
        click(previous.0.saturating_sub(1), previous.1.saturating_sub(1)),
      );
      let duration = timed_draw(&mut app, &mut terminal);

      let screen = rows(&terminal);
      let context = format!("{tab:?} at {width}x{height}:\n{}", screen.join("\n"));
      assert_eq!(terminal.backend().buffer().area.width, width, "{context}");
      assert_eq!(terminal.backend().buffer().area.height, height, "{context}");
      if width >= 80 && height >= 24 {
        assert!(screen[0].starts_with(" HiveMe v"), "{context}");
        assert!(screen[usize::from(height) - 1].contains(&footer_state), "{context}");
        assert!(
          app
            .hits
            .iter()
            .all(|(area, _)| area.right() <= width && area.bottom() <= height),
          "{context}"
        );
      } else {
        assert!(app.hits.is_empty(), "{context}");
        if width >= too_small.width() as u16 {
          assert!(screen.iter().any(|row| row.contains(&too_small)), "{context}");
        }
      }
      assert!(duration < FRAME_BUDGET, "{context}\ntook {duration:?}");
      previous = (width, height);
    }
  }
}

fn mouse_wheel(column: u16, row: u16) -> Input {
  Input::Terminal(Event::Mouse(MouseEvent {
    kind: MouseEventKind::ScrollUp,
    column,
    row,
    modifiers: KeyModifiers::NONE,
  }))
}
