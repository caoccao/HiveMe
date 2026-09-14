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

//! The terminal UI of `hmc` as a real process, against a real broker.
//!
//! The tests in `crates/hmc/src/tui/tests/` drive the application state on ratatui's
//! `TestBackend`. What they cannot see is everything between the process and a
//! terminal: raw mode, the alternate screen, keys that arrive as bytes, a closed
//! terminal, and a broker session that has to end before the process does. So this file
//! runs the real binary in a pseudo-terminal from `portable-pty`, openpty on Unix and
//! ConPTY on Windows, and reads what it draws back into a screen with `vt100`.
//!
//! A HiveMQ CE container stands in for the cloud, as in `publish.rs`, and the tests are
//! skipped the same way: without Docker, or with `HIVEME_SKIP_DOCKER=1`, each test says
//! why it did nothing and passes.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use hiveme_core::config::{Config, DATABASE_FILE_NAME};
use hiveme_core::i18n::{Locale, t};
use hiveme_core::session::{SHUTDOWN_TIMEOUT, Session, SessionApp};
use hiveme_core::storage::Store;
use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};

use hiveme_core::test_support::{self, Broker};

/// The size of the pseudo-terminal, the larger of the two sizes the in-process tests use.
const ROWS: u16 = 40;
const COLUMNS: u16 = 120;

/// How long a test waits for something the terminal UI is already on its way to draw.
const SCREEN_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a message, a row, or a saved file may take to arrive.
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(15);

/// How often a condition is looked at again.
const POLL: Duration = Duration::from_millis(50);

/// The pause between two keys, so that an escape sequence never arrives split in two.
const KEY_GAP: Duration = Duration::from_millis(80);

/// The quit path's own bound, plus the time the process needs to get there and leave.
const EXIT_TIMEOUT: Duration = Duration::from_secs(SHUTDOWN_TIMEOUT.as_secs() + 15);

/// The line `hmc.log` has once the quit path has ended the broker session.
const SESSION_ENDED: &str = "the MQTT connection and session have ended";

const TAB: &[u8] = b"\t";
const ENTER: &[u8] = b"\r";
const UP: &[u8] = b"\x1b[A";
const F10: &[u8] = b"\x1b[21~";
const CTRL_Q: &[u8] = b"\x11";

fn with_broker<F, Fut>(name: &str, body: F)
where
  F: FnOnce(Fixture) -> Fut,
  Fut: std::future::Future<Output = ()>,
{
  test_support::with_broker(name, |broker| async move {
    body(Fixture::new(broker).expect("test fixture")).await
  });
}

/// A running broker, the installation the terminal UI runs on, and a second device.
struct Fixture {
  #[allow(dead_code, reason = "held so the container outlives the test that uses it")]
  container: Broker,
  directory: tempfile::TempDir,
  host: String,
  port: u16,
  /// The config of the installation under test; `HiveMe.db` and `hmc.log` go beside it.
  config_path: PathBuf,
  /// The config of another device on the same broker, which one-shot `hmc` publishes
  /// with, so that its messages are incoming and raise notifications.
  other_path: PathBuf,
}

impl Fixture {
  fn new(container: Broker) -> Result<Self, String> {
    let host = container.host.clone();
    let port = container.port;
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let config_path = directory.path().join("HiveMe.json");
    write_config(&config_path, &device_config(&host, port, "test-runner"))?;
    let other_path = directory.path().join("other").join("HiveMe.json");
    write_config(&other_path, &device_config(&host, port, "build-server"))?;

    Ok(Self {
      container,
      directory,
      host,
      port,
      config_path,
      other_path,
    })
  }

  /// Opens the terminal UI on the installation under test.
  fn open_terminal_ui(&self) -> Terminal {
    Terminal::open(&self.config_path)
  }

  /// Publishes as the other device with one-shot `hmc`, off the runtime's threads.
  async fn publish_as_other_device(&self, arguments: Vec<&'static str>) {
    let path = self.other_path.clone();
    let output = tokio::task::spawn_blocking(move || {
      let mut command = Command::cargo_bin("hmc").expect("the hmc binary is built");
      command.env_remove("HIVEME_CONFIG");
      command.env_remove("HIVEME_PASSWORD");
      command.env_remove("RUST_LOG");
      command.arg("--config").arg(&path).args(arguments);
      command.timeout(Duration::from_secs(60)).output().expect("hmc runs")
    })
    .await
    .expect("the hmc process runs");
    assert!(
      output.status.success(),
      "hmc exited {:?}\nstdout: {}\nstderr: {}",
      output.status.code(),
      String::from_utf8_lossy(&output.stdout),
      String::from_utf8_lossy(&output.stderr)
    );
  }

  /// What the terminal UI has logged so far.
  fn log(&self) -> String {
    std::fs::read_to_string(self.directory.path().join("hmc.log")).unwrap_or_default()
  }

  /// The history database of the installation, opened as one more reader.
  fn store(&self) -> Store {
    Store::open(&self.directory.path().join(DATABASE_FILE_NAME)).expect("HiveMe.db opens")
  }

  /// The config as the terminal UI last saved it.
  fn saved_config(&self) -> Option<Config> {
    serde_json::from_str(&std::fs::read_to_string(&self.config_path).ok()?).ok()
  }

  /// Asks the broker whether `client_id` still has a session.
  async fn session_present(&self, client_id: &str) -> bool {
    test_support::session_present(&self.host, self.port, client_id).await
  }

  /// Fails unless the log says the quit path ended the session and the broker agrees.
  async fn assert_session_ended(&self) {
    let log = self.log();
    assert!(log.contains(SESSION_ENDED), "hmc.log:\n{log}");
    let client_id = client_id(&log);
    assert!(
      !self.session_present(&client_id).await,
      "the broker still keeps a session for {client_id}"
    );
  }
}

/// A config for one device on the test broker.
fn device_config(host: &str, port: u16, name: &str) -> Config {
  let mut config = Config::new_for_this_device();
  config.device.name = name.to_owned();
  config.broker.url = format!("mqtt://{host}:{port}");
  // Any credentials do: the broker accepts all of them, and Config::validate insists
  // on something being there.
  config.broker.username = "hiveme".to_owned();
  config.broker.password = "test".to_owned();
  config.broker.connect_timeout_secs = 20;
  config.publish.timeout_secs = 20;
  // Checked just now, so the terminal UI has no reason to ask GitHub for a release.
  config.update.last_checked = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|elapsed| elapsed.as_secs() as i64)
    .unwrap_or(0);
  config
}

fn write_config(path: &Path, config: &Config) -> Result<(), String> {
  config.validate().map_err(|error| error.to_string())?;
  if let Some(directory) = path.parent() {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
  }
  std::fs::write(
    path,
    serde_json::to_string_pretty(config).map_err(|error| error.to_string())?,
  )
  .map_err(|error| error.to_string())
}

/// The client identifier the terminal UI connected with, as its log names it.
fn client_id(log: &str) -> String {
  log
    .lines()
    .find_map(|line| line.split_once(" with client id ").map(|(_, id)| id.trim().to_owned()))
    .unwrap_or_else(|| panic!("hmc.log names no client identifier:\n{log}"))
}

/// What `hmc` wrote to the terminal, and the screen it amounts to.
struct Output {
  parser: vt100::Parser,
  bytes: Vec<u8>,
}

/// The queries a terminal answers, which `vt100` does not, with their answers.
const QUERIES: [(&[u8], &[u8]); 2] = [
  // ConPTY asks where the cursor is before it passes anything on.
  (b"\x1b[6n", b"\x1b[1;1R"),
  // crossterm asks for the keyboard flags and then for the device attributes; the
  // attributes alone say that the keyboard protocol is not supported.
  (b"\x1b[c", b"\x1b[?62;22c"),
];

/// `hmc --tui` in a pseudo-terminal.
struct Terminal {
  master: Option<Box<dyn MasterPty + Send>>,
  writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
  child: Box<dyn Child + Send + Sync>,
  output: Arc<Mutex<Output>>,
  closing: Arc<AtomicBool>,
  #[cfg_attr(
    windows,
    allow(dead_code, reason = "joined on Unix only; on Windows it ends with the output pipe")
  )]
  reader: Option<JoinHandle<()>>,
}

impl Terminal {
  fn open(config_path: &Path) -> Self {
    let pair = native_pty_system()
      .openpty(PtySize {
        rows: ROWS,
        cols: COLUMNS,
        pixel_width: 0,
        pixel_height: 0,
      })
      .expect("a pseudo-terminal opens");
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_hmc"));
    command.args(["--tui", "--verbose", "--config"]);
    command.arg(config_path);
    for variable in ["HIVEME_CONFIG", "HIVEME_PASSWORD", "RUST_LOG"] {
      command.env_remove(variable);
    }
    command.env("TERM", "xterm-256color");
    let child = pair
      .slave
      .spawn_command(command)
      .expect("hmc starts in the pseudo-terminal");
    // The child holds the terminal side now; a copy here would outlive it.
    drop(pair.slave);

    let master = pair.master;
    #[cfg(unix)]
    set_non_blocking(&*master);
    let mut reader = master.try_clone_reader().expect("the terminal can be read");
    let writer = Arc::new(Mutex::new(Some(
      master.take_writer().expect("the terminal can be written"),
    )));
    let output = Arc::new(Mutex::new(Output {
      parser: vt100::Parser::new(ROWS, COLUMNS, 0),
      bytes: Vec::new(),
    }));
    let closing = Arc::new(AtomicBool::new(false));

    let reader = std::thread::spawn({
      let writer = writer.clone();
      let output = output.clone();
      let closing = closing.clone();
      move || {
        let mut buffer = [0u8; 16 * 1024];
        loop {
          match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => receive(&output, &writer, &buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
              if closing.load(Ordering::SeqCst) {
                return;
              }
              std::thread::sleep(Duration::from_millis(5));
            }
            // Linux reports the end of the other side as an error rather than as 0.
            Err(_) => return,
          }
        }
      }
    });

    Self {
      master: Some(master),
      writer,
      child,
      output,
      closing,
      reader: Some(reader),
    }
  }

  /// The screen as text, one line per row.
  fn contents(&self) -> String {
    self.output.lock().unwrap().parser.screen().contents()
  }

  /// Sends keys one at a time, the way a person types them.
  async fn press(&self, keys: &[&[u8]]) {
    for key in keys {
      {
        let mut writer = self.writer.lock().unwrap();
        let writer = writer.as_mut().expect("the terminal is still open");
        writer.write_all(key).expect("the key is sent");
        writer.flush().expect("the key is sent");
      }
      tokio::time::sleep(KEY_GAP).await;
    }
  }

  async fn type_text(&self, text: &str) {
    let keys: Vec<&[u8]> = text.as_bytes().chunks(1).collect();
    self.press(&keys).await;
  }

  /// Waits until the running terminal UI draws what `condition` looks for.
  async fn wait_for(&mut self, what: &str, condition: impl Fn(&vt100::Screen) -> bool) {
    let deadline = Instant::now() + SCREEN_TIMEOUT;
    loop {
      if condition(self.output.lock().unwrap().parser.screen()) {
        return;
      }
      if let Some(status) = self.child.try_wait().expect("the child can be asked") {
        panic!("hmc exited {status:?} before {what}:\n{}", self.contents());
      }
      assert!(
        Instant::now() < deadline,
        "no {what} within {SCREEN_TIMEOUT:?}:\n{}",
        self.contents()
      );
      tokio::time::sleep(POLL).await;
    }
  }

  async fn wait_for_text(&mut self, text: &str) {
    self
      .wait_for(&format!("`{text}`"), |screen| screen.contents().contains(text))
      .await;
  }

  /// Waits for the process to end.
  async fn wait_for_exit(&mut self) -> ExitStatus {
    let deadline = Instant::now() + EXIT_TIMEOUT;
    loop {
      if let Some(status) = self.child.try_wait().expect("the child can be asked") {
        return status;
      }
      assert!(
        Instant::now() < deadline,
        "hmc is still running after {EXIT_TIMEOUT:?}:\n{}",
        self.contents()
      );
      tokio::time::sleep(POLL).await;
    }
  }

  /// Waits until the screen is the one there was before the terminal UI opened: the
  /// frame gone and every mode it turned on turned off again.
  async fn wait_for_restored_screen(&self) {
    let frame = format!("HiveMe v{}", hiveme_core::VERSION);
    let deadline = Instant::now() + SCREEN_TIMEOUT;
    loop {
      let restored = {
        let output = self.output.lock().unwrap();
        let screen = output.parser.screen();
        !screen.contents().contains(&frame)
          && !screen.alternate_screen()
          && !screen.hide_cursor()
          && !screen.bracketed_paste()
          && screen.mouse_protocol_mode() == vt100::MouseProtocolMode::None
      };
      if restored {
        return;
      }
      assert!(
        Instant::now() < deadline,
        "the terminal was not given back:\n{}",
        self.contents()
      );
      tokio::time::sleep(POLL).await;
    }
  }

  /// Whether the terminal reads whole lines and echoes them, which raw mode turns off.
  #[cfg(unix)]
  fn is_cooked(&self) -> bool {
    let fd = self
      .master
      .as_ref()
      .and_then(|master| master.as_raw_fd())
      .expect("the terminal has a descriptor");
    // SAFETY: a zeroed termios is a valid value for tcgetattr to fill in, and `self`
    // keeps the descriptor open for the duration of the call.
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::tcgetattr(fd, &mut termios) }, 0, "tcgetattr");
    termios.c_lflag & (libc::ICANON | libc::ECHO) == (libc::ICANON | libc::ECHO)
  }

  /// Closes the terminal under the running process, as closing its window does.
  fn close(&mut self) {
    self.closing.store(true, Ordering::SeqCst);
    // On Unix the reader holds a copy of the terminal side too, and the terminal is only
    // closed, and SIGHUP only sent, once every copy is gone.
    #[cfg(unix)]
    if let Some(reader) = self.reader.take() {
      reader.join().expect("the reader ends");
    }
    self.writer.lock().unwrap().take();
    self.master.take();
  }
}

impl Drop for Terminal {
  fn drop(&mut self) {
    if matches!(self.child.try_wait(), Ok(None)) {
      let _ = self.child.kill();
    }
    self.closing.store(true, Ordering::SeqCst);
  }
}

/// Feeds what `hmc` wrote to the screen and answers the terminal queries in it.
fn receive(output: &Mutex<Output>, writer: &Mutex<Option<Box<dyn Write + Send>>>, bytes: &[u8]) {
  let mut output = output.lock().unwrap();
  let scanned = output.bytes.len();
  output.bytes.extend_from_slice(bytes);
  output.parser.process(bytes);
  for (query, answer) in QUERIES {
    // A query that started in an earlier read and ends in this one counts here.
    let from = scanned.saturating_sub(query.len() - 1);
    let asked = output.bytes[from..]
      .windows(query.len())
      .filter(|window| *window == query)
      .count();
    for _ in 0..asked {
      if let Some(writer) = writer.lock().unwrap().as_mut() {
        let _ = writer.write_all(answer).and_then(|()| writer.flush());
      }
    }
  }
}

/// Makes a read of the terminal return at once when there is nothing to read, so that
/// the reader notices the test closing the terminal.
#[cfg(unix)]
fn set_non_blocking(master: &dyn MasterPty) {
  let fd = master.as_raw_fd().expect("the terminal has a descriptor");
  // SAFETY: fcntl on a descriptor the master keeps open, with the flags read from it.
  unsafe {
    let flags = libc::fcntl(fd, libc::F_GETFL);
    assert!(flags >= 0, "F_GETFL");
    assert!(libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) >= 0, "F_SETFL");
  }
}

/// Waits until `condition` holds, failing with `what` when it does not in time.
async fn eventually(what: &str, condition: impl Fn() -> bool) {
  let deadline = Instant::now() + RECEIVE_TIMEOUT;
  while !condition() {
    assert!(
      Instant::now() < deadline,
      "{what} did not happen within {RECEIVE_TIMEOUT:?}"
    );
    tokio::time::sleep(POLL).await;
  }
}

/// How many notifications the terminal UI raised, counted from what it logged.
///
/// A desktop without a notification daemon refuses the toast, which the session logs and
/// drops, so a refusal counts too: the rules had their say either way.
fn toasts(log: &str) -> usize {
  log
    .lines()
    .filter(|line| line.contains("showed a notification for") || line.contains("the OS would not show a notification"))
    .count()
}

/// How many stored rows under `hiveme` have this body.
fn rows_with_body(store: &Store, body: &str) -> usize {
  store
    .messages("hiveme", None, 1_000)
    .expect("the history reads")
    .iter()
    .filter(|row| row.body == body)
    .count()
}

#[test]
fn the_terminal_ui_meets_a_real_broker_and_ends_its_session_on_ctrl_q() {
  with_broker(
    "the_terminal_ui_meets_a_real_broker_and_ends_its_session_on_ctrl_q",
    |fixture| async move {
      let en = |key: &str| t(Locale::EnUs, key);
      let mut terminal = fixture.open_terminal_ui();

      // The frame comes up and connects on its own.
      terminal
        .wait_for_text(&format!("[F2 {}]", en("tui.toolbar.disconnect")))
        .await;
      let contents = terminal.contents();
      assert!(
        contents.starts_with(&format!(" HiveMe v{}", hiveme_core::VERSION)),
        "{contents}"
      );
      #[cfg(unix)]
      assert!(!terminal.is_cooked(), "the terminal UI runs the terminal in raw mode");

      // A message from another device appears live, under its sender's name.
      fixture
        .publish_as_other_device(vec!["--title", "CI", "Nightly build 482 finished"])
        .await;
      terminal.wait_for_text("Nightly build 482 finished").await;
      terminal.wait_for_text("build-server").await;

      // Tab reaches the message box through the message list, and Enter sends.
      terminal.press(&[TAB, TAB]).await;
      terminal.type_text("Deployed to staging").await;
      terminal.press(&[ENTER]).await;
      let store = fixture.store();
      eventually("the sent message being stored", || {
        rows_with_body(&store, "Deployed to staging") == 1
      })
      .await;
      let placeholder: String = en("tui.composer.placeholder").chars().take(16).collect();
      terminal
        .wait_for("the sent bubble above an empty message box", |screen| {
          let contents = screen.contents();
          contents.matches("Deployed to staging").count() == 1 && contents.contains(&placeholder)
        })
        .await;
      let rows = store.messages("hiveme", None, 1_000).expect("the history reads");
      let sent = rows
        .iter()
        .find(|row| row.body == "Deployed to staging")
        .expect("the sent row");
      assert!(sent.outgoing);
      assert_eq!(sent.app.as_deref(), Some("hmc"));
      assert_eq!(sent.sender_name.as_deref(), Some("test-runner"));

      // F10 opens Settings on Appearance; Tab goes through Mode and Theme to Language.
      terminal.press(&[F10]).await;
      terminal.wait_for_text(&en("settings.mode")).await;
      terminal.press(&[TAB, TAB, TAB, ENTER, UP, ENTER]).await;
      let de = |key: &str| t(Locale::De, key);
      terminal
        .wait_for_text(&format!("[F10 {}]", de("tui.toolbar.settings")))
        .await;
      terminal.wait_for_text(&de("settings.appearance")).await;
      // Saved half a second after the change, as every edit is.
      eventually("the language being saved", || {
        fixture.saved_config().is_some_and(|config| config.gui.language == "de")
      })
      .await;

      terminal.press(&[CTRL_Q]).await;
      let status = terminal.wait_for_exit().await;
      assert!(status.success(), "hmc exited {status:?}\nhmc.log:\n{}", fixture.log());
      terminal.wait_for_restored_screen().await;
      #[cfg(unix)]
      assert!(terminal.is_cooked(), "raw mode is turned off on the way out");

      fixture.assert_session_ended().await;
      // The echo of the sent message did not become a second row.
      assert_eq!(rows_with_body(&store, "Deployed to staging"), 1);
      assert_eq!(rows_with_body(&store, "Nightly build 482 finished"), 1);
    },
  );
}

#[test]
fn closing_the_terminal_ends_the_broker_session_all_the_same() {
  with_broker(
    "closing_the_terminal_ends_the_broker_session_all_the_same",
    |fixture| async move {
      let mut terminal = fixture.open_terminal_ui();
      terminal
        .wait_for_text(&format!("[F2 {}]", t(Locale::EnUs, "tui.toolbar.disconnect")))
        .await;

      // SIGHUP on Unix, CTRL_CLOSE_EVENT on Windows.
      tokio::task::block_in_place(|| terminal.close());
      let status = terminal.wait_for_exit().await;
      // Windows ends a console process once its close handler returns, whatever code the
      // process meant to exit with; on Unix the quit path decides.
      #[cfg(unix)]
      assert!(status.success(), "hmc exited {status:?}\nhmc.log:\n{}", fixture.log());
      #[cfg(windows)]
      let _ = status;

      let log = fixture.log();
      assert!(
        log.contains("leaving the terminal UI (Hangup)") || log.contains("leaving the terminal UI (ConsoleClose)"),
        "hmc.log:\n{log}"
      );
      fixture.assert_session_ended().await;
    },
  );
}

use hiveme_core::test_support::Recorder;

/// `hmg` and interactive `hmc` on one config and one database.
///
/// `hmg` is played by its session, `SessionApp::Gui` over the same two files, run in this
/// process: everything `hmg` does between the broker and the database is that session,
/// and its Tauri side only forwards the session's events to a window. A third process,
/// one-shot `hmc` as another device, publishes.
#[test]
fn hmg_and_the_terminal_ui_share_one_installation() {
  with_broker("hmg_and_the_terminal_ui_share_one_installation", |fixture| async move {
    let mut terminal = fixture.open_terminal_ui();
    terminal
      .wait_for_text(&format!("[F2 {}]", t(Locale::EnUs, "tui.toolbar.disconnect")))
      .await;
    let recorder = Arc::new(Recorder::default());
    let gui =
      Session::open(Some(&fixture.config_path), SessionApp::Gui, recorder.clone()).expect("hmg's session opens");
    let status = gui.connect().await.expect("hmg's session connects");
    assert_ne!(
      status.client_id,
      client_id(&fixture.log()),
      "the two never share a client identifier"
    );

    // An envelope is one row, counted unread once, and one notification in each.
    fixture
      .publish_as_other_device(vec!["--title", "Disk", "--level", "error", "Disk full"])
      .await;
    terminal.wait_for_text("Disk full").await;
    let store = fixture.store();
    eventually("hmg storing the envelope", || {
      gui
        .messages("hiveme", None, 1_000)
        .is_ok_and(|rows| rows.iter().any(|row| row.body == "Disk full"))
    })
    .await;
    assert_eq!(rows_with_body(&store, "Disk full"), 1);
    let unread = || gui.topic_tree().expect("the tree reads")[0].unread;
    assert_eq!(unread(), 1);
    eventually("hmg's notification", || recorder.shown.lock().unwrap().len() == 1).await;

    // A payload with no id of its own is a row in each: the documented duplicate.
    let raw = r#"{"disk":"full"}"#;
    fixture
      .publish_as_other_device(vec!["--json", r#"{"disk":"full"}"#])
      .await;
    eventually("both applications storing the raw JSON", || {
      store
        .messages("hiveme", None, 1_000)
        .is_ok_and(|rows| rows.iter().filter(|row| row.raw == raw.as_bytes()).count() == 2)
    })
    .await;
    assert_eq!(unread(), 3);
    eventually("hmg's second notification", || {
      recorder.shown.lock().unwrap().len() == 2
    })
    .await;

    // The terminal UI raised one for each message as well. It is waited for rather than
    // read after the quit, because the two applications answer the same message at their
    // own pace and the quit would otherwise be a race with the slower one.
    eventually("the terminal UI's two notifications", || toasts(&fixture.log()) == 2).await;

    terminal.press(&[CTRL_Q]).await;
    let exit = terminal.wait_for_exit().await;
    assert!(exit.success(), "hmc exited {exit:?}\nhmc.log:\n{}", fixture.log());
    gui.begin_shutdown();
    gui.shutdown().await.expect("hmg's session ends");

    let log = fixture.log();
    assert_eq!(toasts(&log), 2, "hmc.log:\n{log}");
    assert_eq!(recorder.shown.lock().unwrap().len(), 2);
    fixture.assert_session_ended().await;
  });
}
