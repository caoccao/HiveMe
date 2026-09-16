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

//! The terminal UI driven in process, against a scripted session.
//!
//! The screens are rendered on ratatui's `TestBackend` and read back as text; the keys,
//! the clicks, the session events, and the ways out go through the same methods and the
//! same event loop the real terminal uses. The scripted session keeps its rows in a real
//! in-memory store, so the tree, the unread counts, and the paging are the store's own.
//!
//! This file holds the harness and the shell; `messages.rs` holds the Messages tab,
//! `settings.rs` the Settings and About tabs, and `performance.rs` a large history and a
//! terminal that changes size.

mod messages;
mod performance;
mod settings;

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use hiveme_core::config::Config;
use hiveme_core::i18n::{Locale, t, t_count, t_with};
use hiveme_core::message::{Level, Message, Sender};
use hiveme_core::session::{
  About, MessageRow, Notifier, PublishOptions, SHUTDOWN_TIMEOUT, Session, SessionApp, SessionEvent, Status, Toaster,
  TopicNode, UpdateCheckResult, build_tree,
};
use hiveme_core::storage::{NewMessage, Store};
use hiveme_core::{Error, Result};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tokio::sync::{broadcast, mpsc};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Input, QuitReason, Receivers, SAVE_DELAY, SNACKBAR_DURATION, Tab, releases_url};
use super::keys::Action;
use super::service::{Pending, Service};
use super::settings::{Category, Field, Focus};
use super::theme::Glyphs;
use super::{Exit, drive, layout};

use hiveme_core::test_support::Recorder;

/// A session whose answers the test decides.
struct Scripted {
  events: broadcast::Sender<SessionEvent>,
  config: Mutex<Config>,
  status: Mutex<Status>,
  store: Store,
  update: Mutex<Option<UpdateCheckResult>>,
  notifier: Notifier,
  toasts: Arc<Recorder>,
  connect_error: Mutex<Option<String>>,
  shutdown_hangs: AtomicBool,
  begin_shutdowns: AtomicUsize,
  shutdowns: AtomicUsize,
  saved: Mutex<Vec<Config>>,
  /// Why the next saves are refused, when they are.
  save_error: Mutex<Option<String>>,
  skipped: Mutex<Vec<String>>,
  cleared: Mutex<Vec<String>>,
  write_gate: Mutex<()>,
  /// Every publish asked for: the tree topic, the body, and the options.
  published: Mutex<Vec<(String, String, PublishOptions)>>,
  /// Why the broker refuses the next publishes, when it does.
  publish_error: Mutex<Option<String>>,
}

impl Scripted {
  fn new(config: Config) -> Arc<Self> {
    let toasts = Arc::new(Recorder::default());
    Arc::new(Self {
      events: broadcast::channel(64).0,
      notifier: Notifier::new(&config, toasts.clone()),
      config: Mutex::new(config),
      status: Mutex::new(Status::disconnected()),
      store: Store::in_memory().unwrap(),
      update: Mutex::new(None),
      toasts,
      connect_error: Mutex::new(None),
      shutdown_hangs: AtomicBool::new(false),
      begin_shutdowns: AtomicUsize::new(0),
      shutdowns: AtomicUsize::new(0),
      saved: Mutex::new(Vec::new()),
      save_error: Mutex::new(None),
      skipped: Mutex::new(Vec::new()),
      cleared: Mutex::new(Vec::new()),
      write_gate: Mutex::new(()),
      published: Mutex::new(Vec::new()),
      publish_error: Mutex::new(None),
    })
  }

  fn in_language(language: &str) -> Arc<Self> {
    let mut config = Config::default();
    config.gui.language = language.to_owned();
    Self::new(config)
  }

  fn set_state(&self, state: &str) {
    let mut status = self.status.lock().unwrap();
    status.state = state.to_owned();
    if state != "Disconnected" {
      status.host = "abc123.s1.eu.hivemq.cloud".to_owned();
      status.port = 8883;
      status.subscriptions = 1;
    }
    if state == "Reconnecting" {
      status.retry_in_ms = Some(4_000);
    }
  }

  fn emit_status(&self) {
    let _ = self.events.send(SessionEvent::Status(self.status()));
  }

  /// A payload in the store, as if it had arrived before the terminal UI opened.
  ///
  /// `outgoing` means this terminal UI sent it, so the row says the terminal UI is what
  /// produced it whatever the payload claims. A fixture that names `hmg` as its sender
  /// is one `hmg` sent, and the two cannot both be true of the same row.
  fn keep(&self, topic: &str, payload: &[u8], outgoing: bool) -> MessageRow {
    let mut row = NewMessage::from_payload(topic, payload.to_vec(), 1, false, outgoing);
    if outgoing {
      row.app = Some(SessionApp::Tui.app_id().to_owned());
    }
    MessageRow::seen_by(self.store.insert(&row).unwrap().message, SessionApp::Tui)
  }

  /// Stores a row and raises the events the session raises for it.
  fn store_and_announce(&self, row: &NewMessage) -> MessageRow {
    let insertion = self.store.insert(row).unwrap();
    if insertion.topic_is_new {
      let _ = self.events.send(SessionEvent::TopicAdded {
        topic: row.topic.clone(),
      });
    }
    let stored = MessageRow::seen_by(insertion.message, SessionApp::Tui);
    let _ = self.events.send(SessionEvent::Message(stored.clone()));
    stored
  }

  /// What the session's pump does with a message from the broker: the rules, the row,
  /// and the events.
  fn deliver(&self, topic: &str, message: &Message) {
    let bytes = message.to_bytes().unwrap();
    let parsed = hiveme_core::message::parse(&bytes);
    if let Some(rule_id) = self.notifier.notify(topic, &parsed) {
      let _ = self.events.send(SessionEvent::NotificationFired {
        rule_id,
        message_id: message.id.clone(),
        topic: topic.to_owned(),
      });
    }
    self.store_and_announce(&NewMessage::from_payload(topic, bytes, 1, false, false));
    self.status.lock().unwrap().messages_received += 1;
    self.emit_status();
  }
}

impl Service for Scripted {
  fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
    self.events.subscribe()
  }

  fn about(&self) -> About {
    About {
      app_version: hiveme_core::VERSION.to_owned(),
      config_path: "/home/sam/.config/HiveMe/HiveMe.json".to_owned(),
      database_path: "/home/sam/.config/HiveMe/HiveMe.db".to_owned(),
      device_id: "0f9c2d1e-1111-7000-8000-aaaabbbbcccc".to_owned(),
      device_name: "sams-macbook".to_owned(),
      github_url: hiveme_core::session::GITHUB_URL.to_owned(),
    }
  }

  fn config(&self) -> Config {
    self.config.lock().unwrap().clone()
  }

  fn set_config(&self, config: Config) -> Pending<'_, Config> {
    Box::pin(async move {
      self.saved.lock().unwrap().push(config.clone());
      if let Some(reason) = self.save_error.lock().unwrap().clone() {
        return Err(Error::ConfigInvalid(vec![reason]));
      }
      *self.config.lock().unwrap() = config.clone();
      Ok(config)
    })
  }

  fn broker_init(&self) -> Result<String> {
    let init = hiveme_core::config::BrokerInit::from_config(&self.config());
    init.validate()?;
    Ok(init.to_json())
  }

  fn status(&self) -> Status {
    let mut status = self.status.lock().unwrap().clone();
    status.notifications_paused = self.notifier.is_paused();
    status
  }

  fn connect(&self) -> Pending<'_, Status> {
    Box::pin(async move {
      match self.connect_error.lock().unwrap().clone() {
        Some(error) => Err(Error::Connect(error)),
        None => {
          self.set_state("Connected");
          Ok(self.status())
        }
      }
    })
  }

  fn disconnect(&self) -> Pending<'_, ()> {
    Box::pin(async move {
      self.set_state("Disconnected");
      Ok(())
    })
  }

  fn topic_tree(&self) -> Result<Vec<TopicNode>> {
    Ok(build_tree(&self.store.topics()?))
  }

  fn messages(&self, topic: &str, before: Option<i64>, limit: u32) -> Result<Vec<MessageRow>> {
    let rows = self.store.messages(topic, before, limit)?;
    Ok(
      rows
        .into_iter()
        .map(|row| MessageRow::seen_by(row, SessionApp::Tui))
        .collect(),
    )
  }

  fn mark_read_through(&self, topic: &str, row_id: i64, msg_id: &str) -> Result<()> {
    let _gate = self.write_gate.lock().unwrap();
    self.store.mark_read_through(topic, row_id, msg_id)
  }

  fn clear_topic(&self, topic: &str) -> Result<u64> {
    let _gate = self.write_gate.lock().unwrap();
    self.cleared.lock().unwrap().push(topic.to_owned());
    self.store.clear_topic(topic)
  }

  /// Publishes as the session does, with the broker's acknowledgement taken for granted
  /// unless `publish_error` says otherwise.
  fn publish<'a>(&'a self, topic: &'a str, body: &'a str, options: PublishOptions) -> Pending<'a, MessageRow> {
    Box::pin(async move {
      self
        .published
        .lock()
        .unwrap()
        .push((topic.to_owned(), body.to_owned(), options.clone()));
      if let Some(reason) = self.publish_error.lock().unwrap().clone() {
        return Err(Error::PublishRejected {
          topic: topic.to_owned(),
          reason,
        });
      }
      let resolved = hiveme_core::topic::resolve_publish(topic, options.topic.as_deref().unwrap_or(""));
      let payload = if options.json {
        body.as_bytes().to_vec()
      } else {
        let sender = Sender::from_device(&self.config().device, "hmc");
        let mut message = Message::new_text(sender, body);
        if let Some(title) = options.title.clone() {
          message = message.with_title(title);
        }
        if let Some(level) = options.level.as_deref() {
          message = message.with_level(Level::parse(level));
        }
        message.to_bytes().unwrap()
      };
      let mut row = NewMessage::from_payload(
        &resolved,
        payload,
        options.qos.unwrap_or(1),
        options.retain.unwrap_or(false),
        true,
      );
      // As the session does: a raw JSON publish has no envelope to read an application
      // out of, and without one the composer's own bubble would arrive on the left.
      row.app.get_or_insert_with(|| SessionApp::Tui.app_id().to_owned());
      Ok(self.store_and_announce(&row))
    })
  }

  fn set_notifications_paused(&self, paused: bool) -> Status {
    self.notifier.set_paused(paused);
    self.emit_status();
    self.status()
  }

  fn update_result(&self) -> Option<UpdateCheckResult> {
    self.update.lock().unwrap().clone()
  }

  fn skip_version(&self, version: &str) -> Result<()> {
    self.skipped.lock().unwrap().push(version.to_owned());
    Ok(())
  }

  fn begin_shutdown(&self) {
    self.begin_shutdowns.fetch_add(1, Ordering::SeqCst);
  }

  fn shutdown(&self) -> Pending<'_, ()> {
    self.shutdowns.fetch_add(1, Ordering::SeqCst);
    let hangs = self.shutdown_hangs.load(Ordering::SeqCst);
    Box::pin(async move {
      if hangs {
        std::future::pending::<()>().await;
      }
      Ok(())
    })
  }
}

pub(super) fn row(row_id: i64, topic: &str) -> MessageRow {
  MessageRow {
    row_id,
    topic: topic.to_owned(),
    id: format!("id-{row_id}"),
    ts: "2026-09-13T09:41:00Z".to_owned(),
    received_ts: "2026-09-13T09:41:00Z".to_owned(),
    sender_id: None,
    sender_name: None,
    app: None,
    tier: "text".to_owned(),
    level: None,
    title: None,
    body: "hello".to_owned(),
    raw: "hello".to_owned(),
    raw_length: 5,
    qos: 1,
    retain: false,
    outgoing: false,
  }
}

fn open_app(service: &Arc<Scripted>) -> (App<Scripted>, Receivers) {
  App::new(service.clone(), Glyphs::UNICODE, false, false)
}

fn key(code: KeyCode) -> Input {
  Input::Terminal(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn chord(code: KeyCode, modifiers: KeyModifiers) -> Input {
  Input::Terminal(Event::Key(KeyEvent::new(code, modifiers)))
}

fn click(column: u16, row: u16) -> Input {
  Input::Terminal(Event::Mouse(MouseEvent {
    kind: MouseEventKind::Down(MouseButton::Left),
    column,
    row,
    modifiers: KeyModifiers::NONE,
  }))
}

fn press<S: Service>(app: &mut App<S>, input: Input) {
  app.handle_input(input, Instant::now());
}

/// Renders a frame and reads it back one string per row, with a wide glyph counted
/// once rather than once per cell.
fn render<S: Service>(app: &mut App<S>, width: u16, height: u16) -> Vec<String> {
  let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
  terminal.draw(|frame| layout::render(app, frame)).unwrap();
  rows(&terminal)
}

fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
  buffer_rows(terminal.backend().buffer())
}

/// A buffer read back one string per row, a wide glyph counted once.
fn buffer_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
  let mut rows = Vec::new();
  for y in 0..buffer.area.height {
    let mut row = String::new();
    let mut skip = 0;
    for x in 0..buffer.area.width {
      if skip > 0 {
        skip -= 1;
        continue;
      }
      let symbol = buffer[(x, y)].symbol();
      skip = symbol.width().saturating_sub(1);
      row.push_str(symbol);
    }
    rows.push(row);
  }
  rows
}

/// Every event the session raised so far, handed to the application.
fn pump<S: Service>(app: &mut App<S>, receivers: &mut Receivers) {
  while let Ok(event) = receivers.events.try_recv() {
    app.on_session_event(event, Instant::now());
  }
}

fn type_text<S: Service>(app: &mut App<S>, text: &str) {
  for character in text.chars() {
    press(app, key(KeyCode::Char(character)));
  }
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

/// Where a click performs `action` on the last frame.
fn target<S: Service>(app: &App<S>, action: &Action) -> (u16, u16) {
  let (area, _) = app
    .hits
    .iter()
    .rev()
    .find(|(_, candidate)| candidate == action)
    .unwrap_or_else(|| panic!("nothing on screen does {action:?}"));
  (area.x, area.y)
}

#[test]
fn the_frame_renders_in_every_connection_state_size_and_language() {
  for (width, height) in [(80, 24), (120, 40)] {
    for (tag, locale) in [
      ("en-US", Locale::EnUs),
      ("de", Locale::De),
      ("ja", Locale::Ja),
      ("zh-CN", Locale::ZhCn),
    ] {
      for state in ["Disconnected", "Connecting", "Connected", "Reconnecting"] {
        let service = Scripted::in_language(tag);
        service.set_state(state);
        let (mut app, _receivers) = open_app(&service);
        let screen = render(&mut app, width, height);
        let context = format!("{tag} {state} {width}x{height}:\n{}", screen.join("\n"));

        assert!(
          screen[0].starts_with(&format!(" HiveMe v{} ─", hiveme_core::VERSION)),
          "{context}"
        );
        assert!(screen[2].chars().all(|cell| cell == '─'), "{context}");
        for tool in ["[F2", "[F3", "[F4", "[F10", "[F1", "[?", "[^Q"] {
          assert!(screen[1].contains(tool), "{tool} is missing: {context}");
        }
        let quit = screen[1].rfind("[^Q").unwrap();
        let help = screen[1].rfind("[?").unwrap();
        assert!(help < quit && screen[1][quit..].trim_end().ends_with(']'), "{context}");

        assert!(
          screen[3].starts_with(&format!(" {} ", t(locale, "tabs.messages"))),
          "{context}"
        );

        let footer = &screen[usize::from(height) - 1];
        assert!(
          footer.contains(&t(locale, &format!("footer.state.{state}"))),
          "{context}"
        );
        if state == "Disconnected" {
          assert!(footer.contains(&t(locale, "footer.noBroker")), "{context}");
        } else {
          assert!(footer.contains("abc123.s1.eu.hivemq.cloud:8883"), "{context}");
        }
        if width == 120 {
          assert!(footer.contains(&t_count(locale, "footer.subscriptions", 1)) || state == "Disconnected");
          let live = state != "Disconnected";
          let label = t(
            locale,
            if live {
              "tui.toolbar.disconnect"
            } else {
              "tui.toolbar.connect"
            },
          );
          assert!(screen[1].contains(&format!("[F2 {label}]")), "{context}");
        }
        if state == "Reconnecting" && width == 120 {
          let prefix = t_with(locale, "footer.retryIn", &[("duration", "")]);
          assert!(footer.contains(prefix.trim()), "{context}");
        }
        assert!(
          screen.iter().any(|row| row.contains(&t(locale, "messages.empty"))),
          "{context}"
        );
      }
    }
  }
}

#[test]
fn a_terminal_smaller_than_eighty_by_twenty_four_shows_one_line_and_still_quits() {
  let service = Scripted::in_language("en-US");
  let (mut app, _receivers) = open_app(&service);
  let screen = render(&mut app, 79, 24);
  let expected = t_with(Locale::EnUs, "tui.tooSmall", &[("columns", "80"), ("rows", "24")]);
  assert!(
    screen.iter().any(|row| row.contains(&expected)),
    "{}",
    screen.join("\n")
  );
  assert!(!screen.iter().any(|row| row.contains("[F2")));
  assert!(app.hits.is_empty(), "nothing on a too small screen can be clicked");

  press(&mut app, chord(KeyCode::Char('q'), KeyModifiers::CONTROL));
  assert_eq!(app.quit, Some(QuitReason::User));
}

#[test]
fn tabs_open_in_order_cycle_and_close_but_messages_stays() {
  let service = Scripted::in_language("en-US");
  let (mut app, _receivers) = open_app(&service);

  press(&mut app, key(KeyCode::F(10)));
  press(&mut app, key(KeyCode::F(1)));
  assert_eq!(app.tabs, vec![Tab::Messages, Tab::Settings, Tab::About]);
  assert_eq!(app.current_tab(), Tab::About);
  let screen = render(&mut app, 80, 24);
  assert!(
    screen[3].starts_with(" Messages │ Settings ✕ │ About ✕"),
    "{}",
    screen[3]
  );

  press(&mut app, key(KeyCode::F(10)));
  assert_eq!(
    app.current_tab(),
    Tab::Settings,
    "an open tab is focused, not opened twice"
  );
  assert_eq!(app.tabs.len(), 3);

  press(&mut app, chord(KeyCode::Char('1'), KeyModifiers::ALT));
  assert_eq!(app.current_tab(), Tab::Messages);
  press(&mut app, chord(KeyCode::Left, KeyModifiers::ALT));
  assert_eq!(app.current_tab(), Tab::About, "cycling wraps");
  press(&mut app, chord(KeyCode::Tab, KeyModifiers::CONTROL));
  assert_eq!(app.current_tab(), Tab::Messages);
  press(&mut app, chord(KeyCode::Char('9'), KeyModifiers::ALT));
  assert_eq!(app.current_tab(), Tab::Messages, "a tab that does not exist is ignored");

  press(&mut app, chord(KeyCode::Char('w'), KeyModifiers::CONTROL));
  assert_eq!(app.tabs.len(), 3, "Messages cannot be closed");

  press(&mut app, chord(KeyCode::Char('2'), KeyModifiers::ALT));
  press(&mut app, chord(KeyCode::Char('w'), KeyModifiers::CONTROL));
  assert_eq!(app.tabs, vec![Tab::Messages, Tab::About]);
  assert_eq!(
    app.current_tab(),
    Tab::About,
    "the tab after the closed one takes its place"
  );

  render(&mut app, 80, 24);
  let (x, y) = target(&app, &Action::CloseTab(1));
  press(&mut app, click(x, y));
  assert_eq!(app.tabs, vec![Tab::Messages]);
  assert_eq!(app.current_tab(), Tab::Messages);
  assert!(!render(&mut app, 80, 24)[3].contains('✕'));
}

#[test]
fn a_click_on_a_tool_does_what_its_key_does() {
  let service = Scripted::in_language("en-US");
  let (mut app, _receivers) = open_app(&service);
  let screen = render(&mut app, 80, 24);

  let settings = screen[1].find("[F10").unwrap();
  press(&mut app, click(settings as u16 + 1, 1));
  assert_eq!(app.current_tab(), Tab::Settings);

  let screen = render(&mut app, 80, 24);
  assert!(screen[1].contains("[F10 Settings]"));
  let (x, y) = target(&app, &Action::Help);
  press(&mut app, click(x, y));
  assert!(app.help);
  // With the overlay open, a click anywhere closes it.
  render(&mut app, 80, 24);
  press(&mut app, click(40, 12));
  assert!(!app.help);

  render(&mut app, 80, 24);
  let (x, y) = target(&app, &Action::Quit);
  press(&mut app, click(x, y));
  assert_eq!(app.quit, Some(QuitReason::User));
}

#[test]
fn pausing_holds_notifications_back_and_the_footer_and_toolbar_say_so() {
  let mut config = Config::default();
  config.notifications.topmost_enabled = true;
  for rule in &mut config.notifications.rules {
    rule.os = true;
    rule.topmost = true;
  }
  let service = Scripted::new(config);
  service.set_state("Connected");
  let (mut app, mut receivers) = open_app(&service);
  let sender = Sender::from_device(&Config::new_for_this_device().device, "hmc");
  let error = Message::new_text(sender, "Disk full").with_level(Level::Error);

  service.deliver("hiveme/ci", &error);
  pump(&mut app, &mut receivers);
  assert_eq!(service.toasts.shown.lock().unwrap().len(), 1, "the error rule fired");
  assert_eq!(service.toasts.topmost_shown(), service.toasts.shown());
  let footer = render(&mut app, 120, 40).pop().unwrap();
  assert!(footer.contains("1 message this session"), "{footer}");

  press(&mut app, key(KeyCode::F(3)));
  pump(&mut app, &mut receivers);
  let screen = render(&mut app, 120, 40);
  assert!(screen[39].contains("notifications paused"), "{}", screen[39]);
  assert!(screen[1].contains("[F3 Resume]"), "{}", screen[1]);

  service.deliver("hiveme/ci", &error);
  pump(&mut app, &mut receivers);
  assert_eq!(
    service.toasts.shown.lock().unwrap().len(),
    1,
    "nothing is shown while paused"
  );
  assert!(render(&mut app, 120, 40)[39].contains("2 messages this session"));
  assert_eq!(service.toasts.topmost_shown(), service.toasts.shown());

  press(&mut app, key(KeyCode::F(3)));
  service.deliver("hiveme/ci", &error);
  pump(&mut app, &mut receivers);
  assert_eq!(service.toasts.shown.lock().unwrap().len(), 2, "resumed");
  assert_eq!(service.toasts.topmost_shown(), service.toasts.shown());
  let screen = render(&mut app, 120, 40);
  assert!(!screen[39].contains("notifications paused"));
  assert!(screen[1].contains("[F3 Pause]"));
}

#[tokio::test]
async fn connecting_and_disconnecting_follow_the_state_and_a_failure_goes_to_the_snackbar() {
  let service = Scripted::in_language("en-US");
  let (mut app, mut receivers) = open_app(&service);
  assert!(render(&mut app, 80, 24)[1].contains("[F2 Connect]"));
  // Disconnect is the longest English label, and 80 columns is one cell short for it.

  press(&mut app, key(KeyCode::F(2)));
  let outcome = receivers.outcomes.recv().await.unwrap();
  app.on_outcome(outcome, Instant::now());
  assert!(app.is_live());
  assert!(render(&mut app, 120, 40)[1].contains("[F2 Disconnect]"));
  let toolbar = render(&mut app, 80, 24).remove(1);
  assert!(toolbar.contains("[F2 Disconn…]"), "{toolbar}");

  press(&mut app, key(KeyCode::F(2)));
  let outcome = receivers.outcomes.recv().await.unwrap();
  app.on_outcome(outcome, Instant::now());
  assert!(!app.is_live());

  *service.connect_error.lock().unwrap() = Some("no route to host".to_owned());
  press(&mut app, key(KeyCode::F(2)));
  let outcome = receivers.outcomes.recv().await.unwrap();
  let now = Instant::now();
  app.on_outcome(outcome, now);
  let screen = render(&mut app, 80, 24);
  assert!(
    screen[0].contains("cannot connect to the broker: no route to host"),
    "the snackbar is the top row: {}",
    screen[0]
  );

  app.on_tick(now + SNACKBAR_DURATION - Duration::from_millis(1));
  assert!(app.snackbar.is_some());
  app.on_tick(now + SNACKBAR_DURATION);
  assert!(app.snackbar.is_none(), "it goes after four seconds");

  app.notify_error("again".to_owned(), Instant::now());
  press(&mut app, key(KeyCode::Esc));
  assert!(app.snackbar.is_none(), "or with a key");
  app.notify_error("again".to_owned(), Instant::now());
  press(&mut app, key(KeyCode::F(10)));
  assert!(app.snackbar.is_none());
  assert_eq!(app.current_tab(), Tab::Settings, "and the key still does its work");
}

#[test]
fn the_footer_errors_open_their_detail_by_key_and_by_click() {
  let service = Scripted::in_language("en-US");
  {
    let mut status = service.status.lock().unwrap();
    status.last_error = Some("the broker refused the password".to_owned());
    status.config_error = Some("HiveMe.json: expected value at line 1".to_owned());
  }
  let (mut app, _receivers) = open_app(&service);
  let footer = render(&mut app, 120, 40).pop().unwrap();
  assert!(footer.trim_end().ends_with("config error  last error"), "{footer}");

  // The entries end the focus ring, so going backwards from the tree past the filter
  // reaches the last one.
  press(&mut app, chord(KeyCode::BackTab, KeyModifiers::SHIFT));
  press(&mut app, chord(KeyCode::BackTab, KeyModifiers::SHIFT));
  assert_eq!(app.footer_focus(), Some(super::app::FooterEntry::LastError));
  press(&mut app, key(KeyCode::Enter));
  assert_eq!(app.snackbar.as_ref().unwrap().text, "the broker refused the password");
  assert!(app.snackbar.as_ref().unwrap().error);

  press(&mut app, key(KeyCode::Esc));
  render(&mut app, 120, 40);
  let (x, y) = target(&app, &Action::ShowConfigError);
  press(&mut app, click(x, y));
  assert_eq!(
    app.snackbar.as_ref().unwrap().text,
    "HiveMe.json: expected value at line 1"
  );
}

#[test]
fn an_update_raises_the_notice_whose_keys_open_skip_and_close() {
  static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());
  fn opener(url: &str) -> std::result::Result<(), String> {
    OPENED.lock().unwrap().push(url.to_owned());
    Ok(())
  }

  let service = Scripted::in_language("en-US");
  let (app, _receivers) = open_app(&service);
  let mut app = app.with_opener(opener);
  app.on_tick(Instant::now());
  assert!(app.notice.is_none(), "no answer yet");

  *service.update.lock().unwrap() = Some(UpdateCheckResult {
    has_update: true,
    latest_version: Some("9.9.9".to_owned()),
  });
  app.on_tick(Instant::now());
  let screen = render(&mut app, 80, 24);
  assert!(
    screen[3].starts_with(" HiveMe v9.9.9 is available. (o)"),
    "{}",
    screen[3]
  );
  assert!(screen[3].contains("[ ] Skip this version (s)"), "{}", screen[3]);
  assert!(screen[4].starts_with(" Messages"), "the tabs move down one row");

  press(&mut app, key(KeyCode::Char('o')));
  assert_eq!(*OPENED.lock().unwrap(), vec![releases_url()]);
  press(&mut app, key(KeyCode::Char('s')));
  assert!(render(&mut app, 80, 24)[3].contains("[✓] Skip this version (s)"));
  press(&mut app, key(KeyCode::Char('x')));
  assert!(app.notice.is_none());
  assert_eq!(*service.skipped.lock().unwrap(), vec!["9.9.9".to_owned()]);

  // The check is answered once; the notice does not come back on the next tick.
  app.on_tick(Instant::now());
  assert!(app.notice.is_none());
}

#[test]
fn closing_the_notice_without_the_skip_checked_skips_nothing() {
  let service = Scripted::in_language("en-US");
  *service.update.lock().unwrap() = Some(UpdateCheckResult {
    has_update: true,
    latest_version: Some("9.9.9".to_owned()),
  });
  let (mut app, _receivers) = open_app(&service);
  app.on_tick(Instant::now());
  render(&mut app, 80, 24);
  let (x, y) = target(&app, &Action::CloseUpdateNotice);
  press(&mut app, click(x, y));
  assert!(app.notice.is_none());
  assert!(service.skipped.lock().unwrap().is_empty());
}

#[test]
fn the_help_lists_the_keys_of_the_scope_and_any_key_closes_it() {
  let service = Scripted::in_language("en-US");
  let (mut app, _receivers) = open_app(&service);
  press(&mut app, key(KeyCode::Char('?')));
  assert!(app.help);
  let screen = render(&mut app, 80, 24).join("\n");
  for text in [
    "Keys",
    "Ctrl+Q, Ctrl+C",
    "Quit and end the broker session",
    "Tab, Shift+Tab",
    "PageUp, PageDown, Home, End",
    "Copy the body or the raw payload",
    "Alt+Enter, Ctrl+J",
    "Ctrl+Left, Ctrl+Right",
  ] {
    assert!(screen.contains(text), "{text}:\n{screen}");
  }
  assert!(!screen.contains("Ctrl+H"), "the Settings keys are for the Settings tab");
  press(&mut app, key(KeyCode::Esc));
  assert!(!app.help);

  press(&mut app, chord(KeyCode::Char('/'), KeyModifiers::CONTROL));
  assert!(app.help);
  press(&mut app, key(KeyCode::F(10)));
  assert!(!app.help);
  assert_eq!(
    app.current_tab(),
    Tab::Settings,
    "a listed key closes the help and acts"
  );
  press(&mut app, chord(KeyCode::Char('/'), KeyModifiers::CONTROL));
  let screen = render(&mut app, 80, 24).join("\n");
  assert!(screen.contains("Ctrl+H"), "{screen}");

  let service = Scripted::in_language("ja");
  let (mut app, _receivers) = open_app(&service);
  press(&mut app, key(KeyCode::Char('?')));
  let screen = render(&mut app, 80, 24).join("\n");
  assert!(screen.contains(&t(Locale::Ja, "tui.help.quit")), "{screen}");
}

#[tokio::test]
async fn a_first_run_opens_on_the_broker_url_and_saves_what_is_typed_once() {
  let service = Scripted::in_language("en-US");
  let (mut app, mut receivers) = App::new(service.clone(), Glyphs::UNICODE, false, true);
  assert_eq!(app.current_tab(), Tab::Settings);
  assert_eq!(app.settings.category, Category::Broker);
  assert_eq!(app.settings.focus, Focus::Field(Field::Url));
  let screen = render(&mut app, 80, 24).join("\n");
  for text in [
    "Protocol",
    "[TLS MQTT ▾]",
    "URL",
    "Username",
    "Password",
    "Ctrl+H  Show the password",
  ] {
    assert!(screen.contains(text), "{text}:\n{screen}");
  }

  let start = Instant::now();
  app.handle_input(key(KeyCode::Char('h')), start);
  app.handle_input(key(KeyCode::Char('?')), start + Duration::from_millis(300));
  assert!(!app.help, "a question mark in a field is text");
  app.handle_input(
    Input::Terminal(Event::Paste("ost:8883".to_owned())),
    start + Duration::from_millis(400),
  );
  app.on_tick(start + Duration::from_millis(600));
  assert!(
    service.saved.lock().unwrap().is_empty(),
    "the last edit was under 500 ms ago"
  );
  app.on_tick(start + Duration::from_millis(400) + SAVE_DELAY);
  let outcome = receivers.outcomes.recv().await.unwrap();
  app.on_outcome(outcome, Instant::now());
  let saved = service.saved.lock().unwrap().clone();
  assert_eq!(saved.len(), 1, "three edits, one write");
  assert_eq!(saved[0].broker.url, "mqtts://h?ost:8883", "the protocol goes in front");

  // Enter on the password saves at once.
  press(&mut app, key(KeyCode::Tab));
  press(&mut app, key(KeyCode::Char('u')));
  press(&mut app, key(KeyCode::Tab));
  press(&mut app, key(KeyCode::Char('p')));
  let screen = render(&mut app, 80, 24);
  let row = screen.iter().find(|row| row.contains("Password")).unwrap();
  let password = &row[row.find("Password").unwrap() + "Password".len()..];
  assert!(
    password.contains('*') && !password.contains('p'),
    "the password is hidden: {row}"
  );
  app.perform(Action::TogglePassword, Instant::now());
  assert!(render(&mut app, 80, 24).join("\n").contains("Hide the password"));
  press(&mut app, key(KeyCode::Enter));
  let outcome = receivers.outcomes.recv().await.unwrap();
  app.on_outcome(outcome, Instant::now());
  let saved = service.saved.lock().unwrap().clone();
  assert_eq!(saved.len(), 2);
  assert_eq!(saved[1].broker.username, "u");
  assert_eq!(saved[1].broker.password, "p");
}

#[test]
fn clearing_the_topic_reloads_it_and_an_echo_never_doubles_a_row() {
  let service = Scripted::in_language("en-US");
  service.keep("hiveme", b"hello", false);
  let (mut app, _receivers) = open_app(&service);
  assert_eq!(app.selected_topic.as_deref(), Some("hiveme"));
  assert_eq!(app.selected_messages().len(), 1);
  assert!(
    !render(&mut app, 80, 24)
      .join("\n")
      .contains("No messages on this topic.")
  );

  app.receive_message(row(2, "hiveme/ci"));
  app.receive_message(row(2, "hiveme/ci"));
  app.receive_message(row(3, "hivemeow"));
  let ids: Vec<i64> = app.selected_messages().iter().map(|row| row.row_id).collect();
  assert_eq!(ids, vec![1, 2]);

  press(&mut app, key(KeyCode::F(4)));
  let deadline = Instant::now() + Duration::from_secs(2);
  while !app.selected_messages().is_empty() && Instant::now() < deadline {
    app.on_tick(Instant::now());
    std::thread::yield_now();
  }
  assert_eq!(*service.cleared.lock().unwrap(), vec!["hiveme".to_owned()]);
  assert!(
    app.selected_messages().is_empty(),
    "the view was loaded again from the store"
  );
  assert!(
    render(&mut app, 80, 24)
      .join("\n")
      .contains("No messages on this topic.")
  );
}

/// Runs the event loop until it ends, counting how often the terminal was restored.
async fn run_until_exit(
  app: &mut App<Scripted>,
  receivers: &mut Receivers,
  inputs: Vec<Input>,
) -> (Exit, usize, Vec<String>) {
  let (sender, mut receiver) = mpsc::unbounded_channel();
  for input in inputs {
    sender.send(input).unwrap();
  }
  let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
  let restored = Cell::new(0);
  let exit = drive(app, &mut terminal, &mut receiver, receivers, || {
    restored.set(restored.get() + 1)
  })
  .await;
  drop(sender);
  (exit, restored.get(), rows(&terminal))
}

fn assert_ended_once(service: &Scripted) {
  assert_eq!(service.begin_shutdowns.load(Ordering::SeqCst), 1);
  assert_eq!(service.shutdowns.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_quit_tool_ends_the_session_once_and_gives_the_terminal_back() {
  let service = Scripted::in_language("en-US");
  let (mut app, mut receivers) = open_app(&service);
  render(&mut app, 80, 24);
  let (x, y) = target(&app, &Action::Quit);

  let (exit, restored, screen) = run_until_exit(&mut app, &mut receivers, vec![click(x, y)]).await;
  assert_eq!(exit, Exit::Clean);
  assert_eq!(restored, 1);
  assert_ended_once(&service);
  assert!(
    screen[23].contains(&t(Locale::EnUs, "tui.quitting")),
    "the footer says the broker is being waited for: {}",
    screen[23]
  );
}

#[tokio::test]
async fn ctrl_q_quits_and_ctrl_c_quits_even_from_a_text_field() {
  let service = Scripted::in_language("en-US");
  let (mut app, mut receivers) = open_app(&service);
  let (exit, restored, _) = run_until_exit(
    &mut app,
    &mut receivers,
    vec![chord(KeyCode::Char('q'), KeyModifiers::CONTROL)],
  )
  .await;
  assert_eq!((exit, restored), (Exit::Clean, 1));
  assert_ended_once(&service);

  let service = Scripted::in_language("en-US");
  let (mut app, mut receivers) = App::new(service.clone(), Glyphs::UNICODE, false, true);
  assert!(app.key_context().typing);
  let (exit, restored, _) = run_until_exit(
    &mut app,
    &mut receivers,
    vec![
      key(KeyCode::Char('?')),
      chord(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ],
  )
  .await;
  assert_eq!((exit, restored), (Exit::Clean, 1));
  assert_ended_once(&service);
  assert_eq!(
    app.settings.input(Field::Url).unwrap().text(),
    "?",
    "Ctrl+C typed nothing"
  );
}

#[tokio::test]
async fn a_signal_takes_the_same_way_out_without_drawing_on_a_terminal_that_is_gone() {
  for reason in [QuitReason::Hangup, QuitReason::Terminate, QuitReason::ConsoleClose] {
    let service = Scripted::in_language("en-US");
    let (mut app, mut receivers) = open_app(&service);
    let (exit, restored, screen) = run_until_exit(&mut app, &mut receivers, vec![Input::Quit(reason)]).await;
    assert_eq!((exit, restored), (Exit::Clean, 1), "{reason:?}");
    assert_ended_once(&service);
    assert_eq!(
      screen[23].contains(&t(Locale::EnUs, "tui.quitting")),
      reason.terminal_remains(),
      "{reason:?}"
    );
  }
}

#[tokio::test]
async fn a_second_quit_key_during_a_shutdown_that_never_ends_leaves_at_once() {
  let service = Scripted::in_language("en-US");
  service.shutdown_hangs.store(true, Ordering::SeqCst);
  let (mut app, mut receivers) = open_app(&service);
  let (exit, restored, _) = run_until_exit(
    &mut app,
    &mut receivers,
    vec![
      chord(KeyCode::Char('q'), KeyModifiers::CONTROL),
      key(KeyCode::Char('x')),
      chord(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ],
  )
  .await;
  assert_eq!((exit, restored), (Exit::Abandoned, 1));
  assert_ended_once(&service);
}

#[tokio::test(start_paused = true)]
async fn a_shutdown_that_hangs_is_cut_off_at_the_timeout() {
  let service = Scripted::in_language("en-US");
  service.shutdown_hangs.store(true, Ordering::SeqCst);
  let (mut app, mut receivers) = open_app(&service);
  let (sender, mut receiver) = mpsc::unbounded_channel();
  sender.send(chord(KeyCode::Char('q'), KeyModifiers::CONTROL)).unwrap();
  let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
  let started = tokio::time::Instant::now();
  let exit = drive(&mut app, &mut terminal, &mut receiver, &mut receivers, || {}).await;
  assert_eq!(exit, Exit::TimedOut);
  assert!(started.elapsed() >= SHUTDOWN_TIMEOUT);
  assert_ended_once(&service);
  drop(sender);
}

#[tokio::test]
async fn the_real_session_drives_the_frame() {
  struct Silent;
  impl Toaster for Silent {
    fn show(&self, _title: &str, _body: &str) -> std::result::Result<(), String> {
      Ok(())
    }
  }

  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("HiveMe.json");
  let session = Session::open(Some(&path), SessionApp::Tui, Arc::new(Silent)).unwrap();
  let (mut app, mut receivers) = App::new(session.clone(), Glyphs::UNICODE, false, true);
  let screen = render(&mut app, 120, 40);
  assert!(screen[39].contains("no broker configured"), "{}", screen[39]);

  press(&mut app, key(KeyCode::F(3)));
  assert!(session.status().notifications_paused);
  while let Ok(event) = receivers.events.try_recv() {
    app.on_session_event(event, Instant::now());
  }
  assert!(render(&mut app, 120, 40)[39].contains("notifications paused"));

  let (sender, mut receiver) = mpsc::unbounded_channel();
  sender.send(chord(KeyCode::Char('q'), KeyModifiers::CONTROL)).unwrap();
  let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
  let exit = drive(&mut app, &mut terminal, &mut receiver, &mut receivers, || {}).await;
  assert_eq!(exit, Exit::Clean, "a session that never connected ends at once");
}

#[test]
fn lag_recovery_reloads_the_tree_and_selected_rows() {
  let service = Scripted::in_language("en-US");
  let (mut app, _) = open_app(&service);
  service.keep("hiveme/missed", b"missed while lagging", false);
  app.recover_lag(Instant::now());
  assert_eq!(app.selected_messages().len(), 1);
  assert_eq!(app.selected_messages()[0].topic, "hiveme/missed");
  assert_eq!(app.topics[0].messages, 1);
}

#[test]
fn idle_ticks_request_no_redraw_and_expiration_does() {
  let service = Scripted::in_language("en-US");
  let (mut app, _) = open_app(&service);
  let now = Instant::now();
  for _ in 0..100 {
    app.on_tick(now);
    std::thread::yield_now();
  }
  assert!(!app.on_tick(now));
  app.notify_error("temporary".to_owned(), now);
  assert!(app.on_tick(now + Duration::from_secs(10)));
  assert!(app.snackbar.is_none());
}

#[test]
fn very_wide_terminals_do_not_overflow_the_split_or_bubble_width() {
  let service = Scripted::in_language("en-US");
  let (mut app, _) = open_app(&service);
  app.messages_tab.split = 60;
  render(&mut app, 2000, 24);
}

#[test]
fn slow_history_writes_leave_input_and_drawing_responsive() {
  let service = Scripted::in_language("en-US");
  service.keep("hiveme", b"Unread history", false);
  let (mut app, _receivers) = open_app(&service);
  let blocked_write = service.write_gate.lock().unwrap();
  let start = Instant::now();
  app.select_topic("hiveme", start);
  app.clear_selected_topic(start);
  app.perform(Action::Help, start);
  assert!(app.help);
  assert!(!render(&mut app, 80, 24).is_empty());
  assert!(
    start.elapsed() < Duration::from_millis(500),
    "UI must not wait for the held write gate"
  );
  assert!(service.cleared.lock().unwrap().is_empty());
  drop(blocked_write);
  let deadline = Instant::now() + Duration::from_secs(2);
  while service.cleared.lock().unwrap().is_empty() && Instant::now() < deadline {
    std::thread::yield_now();
  }
  assert_eq!(*service.cleared.lock().unwrap(), ["hiveme"]);
}

#[test]
fn clearing_a_topic_removes_its_subtree_and_selects_the_nearest_remaining_topic() {
  let service = Scripted::in_language("en-US");
  for topic in ["hiveme", "hiveme/build/ci", "hiveme/build/ci/deep", "hiveme/build/cd"] {
    service.keep(topic, topic.as_bytes(), false);
  }
  let (mut app, _receivers) = open_app(&service);
  let now = Instant::now();
  app.select_topic("hiveme/build/ci/deep", now);
  app.select_topic("hiveme/build/ci", now);
  assert_eq!(app.selected_messages().len(), 2);

  // The clear runs off the event loop and its answer is applied on a tick.
  app.clear_selected_topic(now);
  let deadline = Instant::now() + Duration::from_secs(5);
  while app.selected_topic.as_deref() == Some("hiveme/build/ci") {
    assert!(Instant::now() < deadline, "the cleared topic was never applied");
    app.on_tick(Instant::now());
    std::thread::sleep(Duration::from_millis(5));
  }

  assert_eq!(app.selected_topic.as_deref(), Some("hiveme/build"));
  let bodies: Vec<&str> = app.selected_messages().iter().map(|row| row.body.as_str()).collect();
  assert_eq!(bodies, ["hiveme/build/cd"]);
  assert!(!app.messages.contains_key("hiveme/build/ci"));
  assert!(!app.messages.contains_key("hiveme/build/ci/deep"));
  fn ids(nodes: &[TopicNode]) -> Vec<String> {
    nodes
      .iter()
      .flat_map(|node| std::iter::once(node.id.clone()).chain(ids(&node.children)))
      .collect()
  }
  assert_eq!(
    ids(&app.topics),
    ["hiveme", "hiveme/build", "hiveme/build/cd"],
    "the cleared topic and its subtopic are gone from the tree"
  );
  assert_eq!(service.store.message_count().unwrap(), 2);
}
