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

//! The state of the terminal UI, the Rust twin of `src/lib/store.tsx`.
//!
//! Everything a screen shows is here, and everything a key, a click, a session event,
//! or the tick changes goes through a method here, so that the whole behavior can be
//! driven in a test without a terminal. What waits on the broker is spawned, and its
//! answer comes back as an [`Outcome`] on a channel the event loop reads, so a slow
//! cluster never freezes the screen.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use hiveme_core::config::Config;
use hiveme_core::i18n::{Locale, t};
use hiveme_core::session::{GITHUB_URL, MessageRow, SessionEvent, Status, TopicNode};
use ratatui::layout::{Position, Rect};
use tokio::sync::{broadcast, mpsc};

use super::about::AboutState;
use super::keys::{self, Action, Context};
use super::messages::{Focus, MessagesState};
use super::service::Service;
use super::settings::{SettingsState, cli_setup_command, is_broker_usable};
use super::theme::{Glyphs, Theme};
use super::{clipboard, open};

/// Clears the selected badges immediately while the disk write runs in the background.
fn mark_tree_read(nodes: &mut [TopicNode], selected: &str) -> u32 {
  let mut cleared = 0;
  for node in nodes {
    if belongs_to_topic(&node.id, selected) {
      cleared += node.unread;
      node.unread = 0;
      mark_tree_read(&mut node.children, selected);
    } else {
      let children = mark_tree_read(&mut node.children, selected);
      node.unread = node.unread.saturating_sub(children);
      cleared += children;
    }
  }
  cleared
}

/// The topic the Messages tab selects at startup, so a new installation is ready to
/// compose. `STARTUP_TOPIC` in `src/lib/constants.ts`.
pub const STARTUP_TOPIC: &str = "hiveme";

/// How many rows one page of history holds. `MESSAGE_PAGE_SIZE` in the frontend.
pub const MESSAGE_PAGE_SIZE: u32 = 200;

/// How long the snackbar stays when no key dismisses it.
pub const SNACKBAR_DURATION: Duration = Duration::from_secs(4);

/// How long after the last edit the settings are saved.
pub const SAVE_DELAY: Duration = Duration::from_millis(500);

/// The page a newer release is described on.
pub fn releases_url() -> String {
  format!("{GITHUB_URL}/releases")
}

/// Something that reaches the event loop from outside the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
  /// A key, a click, a paste, or a resize.
  Terminal(Event),
  /// A signal or a console event that ends the terminal UI.
  Quit(QuitReason),
}

/// Why the terminal UI is leaving, which decides what the quit path can still do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitReason {
  /// The Quit tool, `Ctrl+Q`, or `Ctrl+C`.
  User,
  /// `SIGTERM` or `CTRL_BREAK_EVENT`. The terminal is still there.
  Terminate,
  /// `SIGHUP`, or the terminal stopped delivering input. There is no terminal left.
  Hangup,
  /// `CTRL_CLOSE_EVENT`, `CTRL_LOGOFF_EVENT`, or `CTRL_SHUTDOWN_EVENT`. Windows ends the
  /// process about five seconds later, so nothing but the broker session is attempted.
  #[cfg_attr(not(windows), allow(dead_code, reason = "only Windows has console control events"))]
  ConsoleClose,
}

impl QuitReason {
  /// Whether there is a terminal worth drawing the quitting line on.
  pub fn terminal_remains(self) -> bool {
    matches!(self, Self::User | Self::Terminate)
  }
}

/// A tab of the tab strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
  Messages,
  Settings,
  About,
}

impl Tab {
  /// The catalog key of the tab's label.
  pub fn label_key(self) -> &'static str {
    match self {
      Self::Messages => "tabs.messages",
      Self::Settings => "tabs.settings",
      Self::About => "tabs.about",
    }
  }
}

/// The transient line at the top, `NotificationSnackbar.tsx`: a confirmation in the
/// success color or a failure in the error color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snackbar {
  pub text: String,
  pub error: bool,
  pub until: Instant,
}

/// The answer of an operation that was spawned.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
  MarkedRead,
  ClearedTopic(String),
  Status(Status),
  Failed(String),
  Saved(Box<Config>),
  SaveFailed(String),
  /// The broker acknowledged a message from the composer of `topic`, whose draft said
  /// `body`.
  Sent {
    topic: String,
    body: String,
    row: Box<MessageRow>,
  },
  SendFailed(String),
}

/// The line above the tabs while a newer release exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateNotice {
  pub version: String,
  /// Whether Skip this version is checked, which closing the notice applies.
  pub skip: bool,
}

/// An entry of the footer that can take the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterEntry {
  ConfigError,
  LastError,
}

/// The receiving ends of what the application sends itself and hears from the session.
pub struct Receivers {
  pub outcomes: mpsc::UnboundedReceiver<Outcome>,
  pub events: broadcast::Receiver<SessionEvent>,
}

/// Opens a URL in the browser; replaced in tests.
pub type Opener = fn(&str) -> Result<(), String>;

/// Puts text on the clipboard; replaced in tests.
pub type Copier = fn(&str) -> Result<(), String>;

/// The whole state of the terminal UI.
pub struct App<S: Service> {
  pub service: Arc<S>,
  pub locale: Locale,
  pub theme: Theme,
  pub glyphs: Glyphs,
  pub config: Config,
  pub status: Status,

  /// The open tabs in the order they were opened; the first is always Messages.
  pub tabs: Vec<Tab>,
  pub tab: usize,

  /// The session's topic tree, with `hiveme` merged in.
  pub topics: Vec<TopicNode>,
  pub selected_topic: Option<String>,
  /// History per selected subtree, oldest first, merged by row id.
  pub messages: HashMap<String, Vec<MessageRow>>,
  loaded_topics: HashSet<String>,
  /// False once a subtree has handed back every row it has.
  pub has_older: HashMap<String, bool>,
  /// The tree, the chat view, and the composer.
  pub messages_tab: MessagesState,

  pub snackbar: Option<Snackbar>,
  pub help: bool,
  pub notice: Option<UpdateNotice>,
  update_answered: bool,
  pub settings: SettingsState,
  pub about: AboutState,
  /// When the pending reconnect happens, for the footer's countdown.
  pub retry_deadline: Option<Instant>,

  /// Whether the terminal accepted the keyboard enhancement flags.
  pub enhanced: bool,
  /// Set once a way out was taken; the event loop runs the quit path.
  pub quit: Option<QuitReason>,
  /// Set while the quit path waits for the broker, for the footer.
  pub quitting: bool,

  /// What a click at a position does, rebuilt with every frame. Later entries are drawn
  /// over earlier ones, so they win.
  pub hits: Vec<(Rect, Action)>,

  pub(super) outcomes: mpsc::UnboundedSender<Outcome>,
  write_outcomes: mpsc::UnboundedSender<Outcome>,
  write_results: mpsc::UnboundedReceiver<Outcome>,
  /// When the edited config is due to be written, 500 ms after the last edit. The
  /// config itself is `config`, which always holds the newest edits.
  pending_save: Option<Instant>,
  /// A write is on its way; the next one waits for it.
  saving: bool,
  /// Whether the last write was accepted, which Copy CLI setup needs.
  last_save_succeeded: bool,
  /// Copy CLI setup waits for the writes in progress.
  copy_after_save: bool,
  opener: Opener,
  pub(super) copier: Copier,
}

impl<S: Service> App<S> {
  /// The application as it opens: `hiveme` selected with the tree focused, and on a
  /// first run the Settings tab on the Broker category with the URL focused.
  pub fn new(service: Arc<S>, glyphs: Glyphs, enhanced: bool, first_run: bool) -> (Self, Receivers) {
    let (sender, outcomes) = mpsc::unbounded_channel();
    let (write_outcomes, write_results) = mpsc::unbounded_channel();
    let events = service.subscribe();
    let config = service.config();
    let status = service.status();
    let mut app = Self {
      locale: Locale::resolve(&config.gui.language),
      theme: Theme::from_gui(&config.gui),
      glyphs,
      settings: SettingsState::new(&config),
      about: AboutState::default(),
      config,
      status,
      service,
      tabs: vec![Tab::Messages],
      tab: 0,
      topics: Vec::new(),
      selected_topic: None,
      messages: HashMap::new(),
      loaded_topics: HashSet::new(),
      has_older: HashMap::new(),
      messages_tab: MessagesState::default(),
      snackbar: None,
      help: false,
      notice: None,
      update_answered: false,
      retry_deadline: None,
      enhanced,
      quit: None,
      quitting: false,
      hits: Vec::new(),
      outcomes: sender,
      write_outcomes,
      write_results,
      pending_save: None,
      saving: false,
      last_save_succeeded: true,
      copy_after_save: false,
      opener: open::open_url,
      copier: clipboard::copy_text,
    };
    let now = Instant::now();
    app.retry_deadline = app.status.retry_in_ms.map(|ms| now + Duration::from_millis(ms));
    // Selecting reads the topic tree as well.
    app.select_topic(STARTUP_TOPIC, now);
    if first_run {
      app.open_tab(Tab::Settings);
      app.settings.open_broker();
    }
    (app, Receivers { outcomes, events })
  }

  /// Replaces how URLs are opened, so a test never starts a browser.
  #[cfg(test)]
  pub fn with_opener(mut self, opener: Opener) -> Self {
    self.opener = opener;
    self
  }

  /// Replaces the clipboard, so a test never touches the real one.
  #[cfg(test)]
  pub fn with_copier(mut self, copier: Copier) -> Self {
    self.copier = copier;
    self
  }

  pub fn current_tab(&self) -> Tab {
    self.tabs[self.tab]
  }

  /// How keys are read right now.
  pub fn key_context(&self) -> Context {
    let typing = match self.current_tab() {
      Tab::Messages => self.messages_typing(),
      Tab::Settings => self.settings_typing(),
      Tab::About => false,
    };
    Context {
      typing: !self.help && typing,
      notice: self.notice.is_some(),
      enhanced: self.enhanced,
    }
  }

  /// Reacts to a key, a click, a paste, or a way out.
  pub fn handle_input(&mut self, input: Input, now: Instant) {
    match input {
      Input::Quit(reason) => self.request_quit(reason),
      Input::Terminal(Event::Key(key)) => self.on_key(key, now),
      Input::Terminal(Event::Mouse(mouse)) => self.on_mouse(mouse, now),
      Input::Terminal(Event::Paste(text)) => self.on_paste(&text, now),
      Input::Terminal(_) => {}
    }
  }

  /// Takes a way out. The first one taken is the reason the quit path sees.
  pub fn request_quit(&mut self, reason: QuitReason) {
    if self.quit.is_none() {
      self.quit = Some(reason);
    }
  }

  fn on_key(&mut self, key: KeyEvent, now: Instant) {
    if key.kind == KeyEventKind::Release {
      return;
    }
    let action = keys::action(key, self.key_context());
    // The snackbar goes with the next key. Escape is spent on it; any other key still
    // does what it does, so typing on is never lost to a message.
    if self.snackbar.take().is_some() && action == Some(Action::Escape) {
      return;
    }
    let Some(action) = action else {
      return;
    };
    if self.help {
      self.help = false;
      if matches!(action, Action::Escape | Action::Help | Action::Edit(_)) {
        return;
      }
    }
    self.perform(action, now);
  }

  fn on_mouse(&mut self, mouse: MouseEvent, now: Instant) {
    let position = Position::new(mouse.column, mouse.row);
    match mouse.kind {
      MouseEventKind::Down(MouseButton::Left) => {}
      MouseEventKind::Drag(MouseButton::Left) if self.messages_tab.dragging => {
        return self.drag_divider(mouse.column);
      }
      MouseEventKind::Up(MouseButton::Left) => {
        self.messages_tab.dragging = false;
        return;
      }
      MouseEventKind::ScrollUp | MouseEventKind::ScrollDown if !self.help => {
        let up = mouse.kind == MouseEventKind::ScrollUp;
        return match self.current_tab() {
          Tab::Messages => self.wheel_messages(position, up, now),
          Tab::Settings => self.wheel_settings(up),
          Tab::About => self.wheel_about(up),
        };
      }
      _ => return,
    }
    let Some(action) = self
      .hits
      .iter()
      .rev()
      .find(|(area, _)| area.contains(position))
      .map(|(_, action)| action.clone())
    else {
      return;
    };
    if self.help && action != Action::Help {
      self.help = false;
    }
    self.perform(action, now);
  }

  fn on_paste(&mut self, text: &str, now: Instant) {
    if self.help {
      return;
    }
    match self.current_tab() {
      Tab::Messages => self.paste_messages(text),
      Tab::Settings => self.paste_settings(text, now),
      Tab::About => {}
    }
  }

  /// Does what a key or a click asked for.
  pub fn perform(&mut self, action: Action, now: Instant) {
    match action {
      Action::Quit => self.request_quit(QuitReason::User),
      Action::About => self.open_tab(Tab::About),
      Action::Settings => self.open_tab(Tab::Settings),
      Action::Connection => self.toggle_connection(),
      Action::Pause => self.toggle_notifications_paused(),
      Action::ClearTopic => self.clear_selected_topic(now),
      Action::Help => self.help = !self.help,
      Action::SelectTab(index) => {
        if index < self.tabs.len() {
          self.tab = index;
        }
      }
      Action::CloseTab(index) => self.close_tab(index),
      Action::CloseCurrentTab => self.close_tab(self.tab),
      Action::PreviousTab => self.tab = (self.tab + self.tabs.len() - 1) % self.tabs.len(),
      Action::NextTab => self.tab = (self.tab + 1) % self.tabs.len(),
      Action::Escape => self.escape(),
      Action::OpenReleases => self.open_releases(),
      Action::ToggleSkipVersion => {
        if let Some(notice) = self.notice.as_mut() {
          notice.skip = !notice.skip;
        }
      }
      Action::CloseUpdateNotice => self.close_update_notice(),
      Action::ShowConfigError => {
        if let Some(error) = self.status.config_error.clone() {
          self.notify_error(error, now);
        }
      }
      Action::ShowLastError => {
        if let Some(error) = self.status.last_error.clone() {
          self.notify_error(error, now);
        }
      }
      Action::DismissSnackbar => self.snackbar = None,
      Action::FocusNext
      | Action::FocusPrevious
      | Action::Up
      | Action::Down
      | Action::Left
      | Action::Right
      | Action::PageUp
      | Action::PageDown
      | Action::Home
      | Action::End
      | Action::Toggle
      | Action::FocusFilter
      | Action::CopyBody
      | Action::CopyRaw
      | Action::Newline
      | Action::SplitLeft
      | Action::SplitRight
      | Action::FocusPane(_)
      | Action::ToggleTopic(_)
      | Action::SelectTopic(_)
      | Action::FocusMessage(_)
      | Action::Composer(_)
      | Action::ComposerQos(_)
      | Action::ComposerLevel(_)
      | Action::Divider
      | Action::Activate
      | Action::TogglePassword
      | Action::SettingsCategory(_)
      | Action::SettingsField(_)
      | Action::SettingsChoice(..)
      | Action::AboutLink(_)
      | Action::Edit(_) => match self.current_tab() {
        Tab::Messages => self.perform_messages(action, now),
        Tab::Settings => self.perform_settings(action, now),
        Tab::About => self.perform_about(action, now),
      },
    }
  }

  /// The footer entry that has the focus, which only the Messages tab gives it.
  pub fn footer_focus(&self) -> Option<FooterEntry> {
    match self.messages_tab.focus {
      Focus::Footer(entry) if self.current_tab() == Tab::Messages => Some(entry),
      _ => None,
    }
  }

  /// The footer entries that exist right now, in the order they are drawn.
  pub fn footer_entries(&self) -> Vec<FooterEntry> {
    let mut entries = Vec::new();
    if self.status.config_error.is_some() {
      entries.push(FooterEntry::ConfigError);
    }
    if self.status.last_error.is_some() {
      entries.push(FooterEntry::LastError);
    }
    entries
  }

  fn escape(&mut self) {
    match self.current_tab() {
      Tab::Messages => self.escape_messages(),
      Tab::Settings => {
        self.escape_settings();
      }
      Tab::About => {}
    }
  }

  /// Opens a tab, or selects it when it is open.
  pub fn open_tab(&mut self, tab: Tab) {
    match self.tabs.iter().position(|open| *open == tab) {
      Some(index) => self.tab = index,
      None => {
        self.tabs.push(tab);
        self.tab = self.tabs.len() - 1;
      }
    }
  }

  /// Closes a closable tab. Messages stays.
  pub fn close_tab(&mut self, index: usize) {
    if index == 0 || index >= self.tabs.len() {
      return;
    }
    self.tabs.remove(index);
    if self.tab > index || self.tab >= self.tabs.len() {
      self.tab -= 1;
    }
  }

  /// Whether the Connect tool offers Disconnect, which it does while the client is
  /// connecting, connected, or still trying to reconnect.
  pub fn is_live(&self) -> bool {
    matches!(self.status.state.as_str(), "Connecting" | "Connected" | "Reconnecting")
  }

  fn toggle_connection(&self) {
    let service = self.service.clone();
    let outcomes = self.outcomes.clone();
    if self.is_live() {
      tokio::spawn(async move {
        let outcome = match service.disconnect().await {
          Ok(()) => Outcome::Status(service.status()),
          Err(error) => Outcome::Failed(error.to_string()),
        };
        let _ = outcomes.send(outcome);
      });
    } else {
      tokio::spawn(async move {
        let outcome = match service.connect().await {
          Ok(status) => Outcome::Status(status),
          Err(error) => Outcome::Failed(error.to_string()),
        };
        let _ = outcomes.send(outcome);
      });
    }
  }

  fn toggle_notifications_paused(&mut self) {
    let status = self.service.set_notifications_paused(!self.status.notifications_paused);
    self.set_status(status, Instant::now());
  }

  /// Selects a topic, loads its subtree once, and marks it read.
  pub fn select_topic(&mut self, topic: &str, now: Instant) {
    if self.selected_topic.as_deref() != Some(topic) {
      self.messages_tab.view.reset();
      self.messages_tab.composer.popup = None;
    }
    self.selected_topic = Some(topic.to_owned());
    self.keep_composer_focus_visible();
    if !self.loaded_topics.contains(topic) {
      match self.service.messages(topic, None, MESSAGE_PAGE_SIZE) {
        Ok(page) => {
          self
            .has_older
            .insert(topic.to_owned(), page.len() >= MESSAGE_PAGE_SIZE as usize);
          let rows = self.messages.remove(topic).unwrap_or_default();
          self.messages.insert(topic.to_owned(), merge(page, rows));
          self.loaded_topics.insert(topic.to_owned());
        }
        Err(error) => return self.notify_error(error.to_string(), now),
      }
    }
    self.refresh_topics(now);
    mark_tree_read(&mut self.topics, topic);
    let topic = topic.to_owned();
    let service = self.service.clone();
    let outcomes = self.write_outcomes.clone();
    std::thread::spawn(move || {
      let outcome = match service.mark_read(&topic) {
        Ok(()) => Outcome::MarkedRead,
        Err(error) => Outcome::Failed(error.to_string()),
      };
      let _ = outcomes.send(outcome);
    });
  }

  /// Recovers state after the broadcast channel reports missing events.
  pub fn recover_lag(&mut self, now: Instant) {
    self.set_status(self.service.status(), now);
    self.refresh_topics(now);
    if let Some(topic) = self.selected_topic.clone() {
      self.loaded_topics.remove(&topic);
      self.messages.remove(&topic);
      self.has_older.remove(&topic);
      self.select_topic(&topic, now);
    }
  }

  /// Loads the page before the oldest row of a subtree, as `loadOlderMessages` does.
  /// Returns whether rows were added.
  pub fn load_older_messages(&mut self, topic: &str, now: Instant) -> bool {
    if self.has_older.get(topic) == Some(&false) {
      return false;
    }
    let Some(oldest) = self
      .messages
      .get(topic)
      .and_then(|rows| rows.first())
      .map(|row| row.row_id)
    else {
      return false;
    };
    match self.service.messages(topic, Some(oldest), MESSAGE_PAGE_SIZE) {
      Ok(page) => {
        self
          .has_older
          .insert(topic.to_owned(), page.len() >= MESSAGE_PAGE_SIZE as usize);
        let added = !page.is_empty();
        let rows = self.messages.remove(topic).unwrap_or_default();
        self.messages.insert(topic.to_owned(), merge(page, rows));
        added
      }
      Err(error) => {
        self.notify_error(error.to_string(), now);
        false
      }
    }
  }

  /// Deletes the selected topic's history, then reloads every cached view that
  /// contained it, as `clearSelectedTopic` does.
  pub fn clear_selected_topic(&mut self, _now: Instant) {
    let Some(topic) = self.selected_topic.clone() else {
      return;
    };
    let service = self.service.clone();
    let outcomes = self.write_outcomes.clone();
    std::thread::spawn(move || {
      let outcome = match service.clear_topic(&topic) {
        Ok(_) => Outcome::ClearedTopic(topic),
        Err(error) => Outcome::Failed(error.to_string()),
      };
      let _ = outcomes.send(outcome);
    });
  }

  fn topic_cleared(&mut self, topic: &str, now: Instant) {
    let roots: Vec<String> = self
      .messages
      .keys()
      .chain(self.loaded_topics.iter())
      .filter(|root| belongs_to_topic(topic, root))
      .cloned()
      .collect();
    for root in roots {
      self.messages.remove(&root);
      self.loaded_topics.remove(&root);
      self.has_older.remove(&root);
    }
    match self.selected_topic.clone() {
      Some(selected) if belongs_to_topic(topic, &selected) => {
        self.messages_tab.view.reset();
        self.select_topic(&selected, now);
      }
      _ => self.refresh_topics(now),
    }
  }

  /// The rows of the selected subtree.
  pub fn selected_messages(&self) -> &[MessageRow] {
    self
      .selected_topic
      .as_ref()
      .and_then(|topic| self.messages.get(topic))
      .map(Vec::as_slice)
      .unwrap_or(&[])
  }

  /// A stored row, merged into every loaded view it belongs to. A row that is already
  /// there is replaced, so the broker's echo of a message sent here never doubles it.
  pub fn receive_message(&mut self, row: MessageRow) {
    self.messages_tab.view.invalidate_row(row.row_id);
    let roots: Vec<String> = self
      .loaded_topics
      .iter()
      .filter(|root| belongs_to_topic(&row.topic, root))
      .cloned()
      .collect();
    for root in roots {
      let rows = self.messages.remove(&root).unwrap_or_default();
      self.messages.insert(root, merge(rows, vec![row.clone()]));
    }
  }

  /// Reacts to what the session raised.
  pub fn on_session_event(&mut self, event: SessionEvent, now: Instant) {
    match event {
      SessionEvent::Status(status) => self.set_status(status, now),
      // The tree's unread badges change with every row, as the GUI refreshes them.
      SessionEvent::Message(row) => {
        self.receive_message(row);
        self.refresh_topics(now);
      }
      SessionEvent::TopicAdded { .. } => self.refresh_topics(now),
      // The session's notifier already delivered the notification.
      SessionEvent::NotificationFired { .. } => {}
    }
  }

  /// Reacts to the answer of something that was spawned.
  pub fn on_outcome(&mut self, outcome: Outcome, now: Instant) {
    match outcome {
      Outcome::MarkedRead => self.refresh_topics(now),
      Outcome::ClearedTopic(topic) => self.topic_cleared(&topic, now),
      Outcome::Status(status) => self.set_status(status, now),
      Outcome::Failed(error) => {
        self.notify_error(error, now);
        let status = self.service.status();
        self.set_status(status, now);
      }
      Outcome::Saved(saved) => {
        self.saving = false;
        self.last_save_succeeded = true;
        // Only accept the normalized values when no newer edit is on screen.
        if self.pending_save.is_none() {
          self.apply_config(*saved);
        }
        let status = self.service.status();
        self.set_status(status, now);
        self.after_save(now);
      }
      Outcome::SaveFailed(error) => {
        self.saving = false;
        self.last_save_succeeded = false;
        // A newer edit may already have corrected the refused value, and the edits stay
        // on screen; the next save retries the complete config.
        if self.pending_save.is_none() {
          self.notify_error(error, now);
        }
        self.after_save(now);
      }
      Outcome::Sent { topic, body, row } => {
        self.sent(&topic, &body);
        self.receive_message(*row);
        self.refresh_topics(now);
      }
      Outcome::SendFailed(error) => {
        self.messages_tab.composer.sending = false;
        self.notify_error(error, now);
      }
    }
  }

  pub fn set_status(&mut self, status: Status, now: Instant) {
    if status.retry_in_ms != self.status.retry_in_ms || status.state != self.status.state {
      self.retry_deadline = status.retry_in_ms.map(|ms| now + Duration::from_millis(ms));
    }
    self.status = status;
    if let Focus::Footer(entry) = self.messages_tab.focus
      && !self.footer_entries().contains(&entry)
    {
      self.messages_tab.focus = Focus::Tree;
    }
  }

  /// What the tick advances: the snackbar's time, the pending save, the update check.
  pub fn on_tick(&mut self, now: Instant) -> bool {
    let before = (self.snackbar.is_some(), self.saving, self.notice.clone());
    let mut written = false;
    while let Ok(outcome) = self.write_results.try_recv() {
      self.on_outcome(outcome, now);
      written = true;
    }
    if self.snackbar.as_ref().is_some_and(|snackbar| now >= snackbar.until) {
      self.snackbar = None;
    }
    if self.pending_save.is_some_and(|at| now >= at) {
      self.start_save();
    }
    if !self.update_answered
      && let Some(result) = self.service.update_result()
    {
      self.update_answered = true;
      if result.has_update
        && let Some(version) = result.latest_version
      {
        self.notice = Some(UpdateNotice { version, skip: false });
      }
    }
    written || self.retry_deadline.is_some() || before != (self.snackbar.is_some(), self.saving, self.notice.clone())
  }

  /// Milliseconds until the pending reconnect, for the footer.
  pub fn retry_in(&self, now: Instant) -> Option<u64> {
    self
      .retry_deadline
      .map(|deadline| deadline.saturating_duration_since(now).as_millis() as u64)
  }

  /// Changes the config the screen shows at once and saves it 500 ms after the last
  /// change, as `updateConfig` does. A language, a theme, or a display mode applies to
  /// the whole screen in the same frame.
  pub(super) fn edit_config(&mut self, now: Instant, change: impl FnOnce(&mut Config)) {
    let mut config = self.config.clone();
    change(&mut config);
    if config == self.config {
      return;
    }
    self.apply_config(config);
    self.pending_save = Some(now + SAVE_DELAY);
  }

  /// Writes the edited config now rather than after the delay, as `flushConfig` does. A
  /// write in progress is waited for, and the newest edits follow it.
  pub(super) fn flush_save(&mut self, now: Instant) {
    if self.pending_save.is_some() {
      self.pending_save = Some(now);
      self.start_save();
    }
  }

  /// Writes the newest config, one write at a time.
  fn start_save(&mut self) {
    if self
      .config
      .topics
      .subscriptions
      .iter()
      .any(|row| row.filter().trim().is_empty())
    {
      return;
    }
    if self.saving || self.pending_save.take().is_none() {
      return;
    }
    self.saving = true;
    let config = self.config.clone();
    let service = self.service.clone();
    let outcomes = self.outcomes.clone();
    tokio::spawn(async move {
      let outcome = match service.set_config(config).await {
        Ok(saved) => Outcome::Saved(Box::new(saved)),
        Err(error) => Outcome::SaveFailed(error.to_string()),
      };
      let _ = outcomes.send(outcome);
    });
  }

  /// A write finished: edits made during it are written right away, and a Copy CLI setup
  /// that waited for the writes goes on once there are none left.
  fn after_save(&mut self, now: Instant) {
    if self.pending_save.is_some() {
      self.pending_save = Some(now);
      return self.start_save();
    }
    if std::mem::take(&mut self.copy_after_save) && self.last_save_succeeded {
      self.copy_cli_setup_now(now);
    }
  }

  /// Copy CLI setup: the pending edits are saved first, so the command carries the
  /// credentials on screen, and a save that fails copies nothing.
  pub(super) fn copy_cli_setup(&mut self, now: Instant) {
    if !is_broker_usable(&self.config) {
      return;
    }
    if self.pending_save.is_some() || self.saving {
      self.copy_after_save = true;
      return self.flush_save(now);
    }
    if self.last_save_succeeded {
      self.copy_cli_setup_now(now);
    }
  }

  fn copy_cli_setup_now(&mut self, now: Instant) {
    let setup = match self.service.broker_init() {
      Ok(setup) => setup,
      Err(error) => return self.notify_error(error.to_string(), now),
    };
    match (self.copier)(&cli_setup_command(&setup)) {
      Ok(()) => self.notify_info(t(self.locale, "settings.cliSetupCopied"), now),
      Err(error) => self.notify_error(error, now),
    }
  }

  fn apply_config(&mut self, config: Config) {
    let look_changed = config.gui != self.config.gui;
    self.locale = Locale::resolve(&config.gui.language);
    self.theme = Theme::from_gui(&config.gui);
    self.config = config;
    if look_changed {
      // Bubbles hold translated text and colors, so every height is measured again.
      self.messages_tab.view.invalidate();
    }
  }

  /// Opens a page in the browser, with a failure in the snackbar.
  pub(super) fn open_url(&mut self, url: &str, now: Instant) {
    if let Err(error) = (self.opener)(url) {
      self.notify_error(error, now);
    }
  }

  fn open_releases(&mut self) {
    self.open_url(&releases_url(), Instant::now());
  }

  fn close_update_notice(&mut self) {
    let Some(notice) = self.notice.take() else {
      return;
    };
    if notice.skip
      && let Err(error) = self.service.skip_version(&notice.version)
    {
      self.notify_error(error.to_string(), Instant::now());
    }
  }

  pub fn notify_error(&mut self, text: String, now: Instant) {
    self.snackbar = Some(Snackbar {
      text,
      error: true,
      until: now + SNACKBAR_DURATION,
    });
  }

  /// A confirmation, such as a copy.
  pub fn notify_info(&mut self, text: String, now: Instant) {
    self.snackbar = Some(Snackbar {
      text,
      error: false,
      until: now + SNACKBAR_DURATION,
    });
  }
}

/// MQTT topics are case sensitive and split on `/`, never on a bare prefix.
fn belongs_to_topic(topic: &str, root: &str) -> bool {
  topic == root || topic.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

/// Merges pages and live rows by row id, in the order they were stored.
fn merge(mut rows: Vec<MessageRow>, more: Vec<MessageRow>) -> Vec<MessageRow> {
  for row in more {
    match rows.iter().position(|existing| existing.row_id == row.row_id) {
      Some(index) => rows[index] = row,
      None => rows.push(row),
    }
  }
  rows.sort_by_key(|row| row.row_id);
  rows
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_topic_contains_its_descendants_and_nothing_that_only_shares_a_prefix() {
    assert!(belongs_to_topic("hiveme", "hiveme"));
    assert!(belongs_to_topic("hiveme/build/ci", "hiveme"));
    assert!(!belongs_to_topic("hivemeow", "hiveme"));
    assert!(!belongs_to_topic("hiveme", "hiveme/build"));
  }

  #[test]
  fn only_the_user_and_sigterm_leave_a_terminal_to_draw_on() {
    assert!(QuitReason::User.terminal_remains());
    assert!(QuitReason::Terminate.terminal_remains());
    assert!(!QuitReason::Hangup.terminal_remains());
    assert!(!QuitReason::ConsoleClose.terminal_remains());
  }
}
