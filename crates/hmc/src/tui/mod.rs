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

//! The terminal UI of `hmc`, specified in `docs/specs/tui.md`.
//!
//! [`start`] sets the terminal up, opens the session `hmg` runs on, and runs the event
//! loop: the terminal's keys, the session's events, the answers of spawned operations,
//! and a tick, all in one `select!`. Every way out, a key, the Quit tool, a closed
//! terminal, or a signal, ends in [`quit`], which ends the broker session under
//! `SHUTDOWN_TIMEOUT` before the terminal is given back.

mod about;
mod app;
mod clipboard;
mod footer;
mod help;
mod keys;
mod layout;
mod messages;
mod notify;
mod open;
mod service;
mod settings;
mod snackbar;
mod tabs;
mod theme;
mod toolbar;
mod update_notice;
mod widgets;

#[cfg(test)]
mod tests;

use std::future::poll_fn;
use std::io::{self, IsTerminal, Write};
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{
  DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, EventStream,
  PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use futures_core::Stream;
use hiveme_core::config::config_path;
use hiveme_core::i18n::{Locale, t, t_with};
use hiveme_core::session::{SHUTDOWN_TIMEOUT, Session, SessionApp};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use crate::cli::Cli;
use crate::failure::{Failure, Result};

use app::{App, Input, QuitReason, Receivers};
use notify::DesktopToaster;
use service::Service;
use theme::Glyphs;

/// How often the screen advances without an event: the reconnect countdown, the
/// snackbar's time, the pending save, and the update check's answer.
pub const TICK: Duration = Duration::from_millis(250);

/// The log file interactive `hmc` writes beside the config, since stderr is the screen.
pub const LOG_FILE_NAME: &str = "hmc.log";

/// Whether the keyboard enhancement flags were pushed and must be popped on the way out.
static ENHANCED: AtomicBool = AtomicBool::new(false);

/// How the quit path ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
  /// The broker session ended.
  Clean,
  /// The session could not be ended cleanly; the broker expires it.
  Failed(String),
  /// The broker did not answer within `SHUTDOWN_TIMEOUT`.
  TimedOut,
  /// A second quit key skipped the rest of the cleanup.
  Abandoned,
}

/// Runs the terminal UI until the user leaves it.
///
/// `locale` is the language of the config before it is opened, for what can fail
/// before the screen exists.
pub fn start(cli: &Cli, locale: Locale) -> Result<()> {
  // Checked before anything is written, since there is nothing to draw on.
  if !io::stdout().is_terminal() {
    return Err(Failure::Usage(t(locale, "cli.notATerminal")));
  }
  let path = config_path(cli.config.as_deref())?;
  let first_run = !path.exists();
  init_logging(cli.verbose, &path);

  // The session's connection, its pump, and the screen run side by side, so the terminal
  // UI takes a multi-thread runtime where publish mode takes a single thread.
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .map_err(|source| {
      Failure::Unexpected(t_with(
        locale,
        "cli.runtimeUnavailable",
        &[("error", &source.to_string())],
      ))
    })?;
  let result = runtime.block_on(run(path, first_run, locale));
  // Whatever still runs, the pump of an abandoned session or the key reader, ends with
  // the process rather than holding the exit.
  runtime.shutdown_background();
  result
}

async fn run(path: PathBuf, first_run: bool, locale: Locale) -> Result<()> {
  let session = Session::open(Some(&path), SessionApp::Tui, Arc::new(DesktopToaster::new()))?;
  // Never dropped: `restore_terminal` shows the cursor on every way out, and ratatui's own
  // drop would show it again and, after a hangup, print why it could not to a stderr that
  // is gone, which panics and turns the exit code into 101.
  let mut terminal = ManuallyDrop::new(enter_terminal().map_err(|source| {
    restore_terminal();
    Failure::Unexpected(t_with(
      locale,
      "cli.terminalUnavailable",
      &[("error", &source.to_string())],
    ))
  })?);

  let (mut app, mut receivers) = App::new(
    session.clone(),
    Glyphs::detect(),
    ENHANCED.load(Ordering::SeqCst),
    first_run,
  );
  session.start_background_work();
  let mut inputs = forward_inputs();
  let exit = drive(&mut app, &mut *terminal, &mut inputs, &mut receivers, restore_terminal).await;
  log::info!("the terminal UI has ended: {exit:?}");
  Ok(())
}

/// The event loop, then the quit path, then `restore`.
pub async fn drive<B, S>(
  app: &mut App<S>,
  terminal: &mut Terminal<B>,
  inputs: &mut mpsc::UnboundedReceiver<Input>,
  receivers: &mut Receivers,
  restore: impl FnOnce(),
) -> Exit
where
  B: Backend,
  S: Service,
{
  let mut tick = tokio::time::interval(TICK);
  tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
  let mut events_open = true;
  if !draw(app, terminal) {
    app.request_quit(QuitReason::Hangup);
  }

  while app.quit.is_none() {
    let mut dirty = true;
    tokio::select! {
      input = inputs.recv() => match input {
        Some(input) => app.handle_input(input, Instant::now()),
        // Nothing can arrive from the terminal any more.
        None => app.request_quit(QuitReason::Hangup),
      },
      event = receivers.events.recv(), if events_open => match event {
        Ok(event) => app.on_session_event(event, Instant::now()),
        Err(RecvError::Lagged(skipped)) => {
          log::warn!("the terminal UI fell behind, {skipped} session event(s) were dropped");
          app.recover_lag(Instant::now());
        }
        Err(RecvError::Closed) => events_open = false,
      },
      Some(outcome) = receivers.outcomes.recv() => app.on_outcome(outcome, Instant::now()),
      _ = tick.tick() => dirty = app.on_tick(Instant::now()),
    }
    if dirty && app.quit.is_none() && !draw(app, terminal) {
      app.request_quit(QuitReason::Hangup);
    }
  }

  let reason = app.quit.unwrap_or(QuitReason::User);
  let exit = quit(app, terminal, inputs, reason).await;
  restore();
  exit
}

/// The quit path of `docs/specs/tui.md`, "Leaving the terminal UI", up to giving the
/// terminal back.
async fn quit<B, S>(
  app: &mut App<S>,
  terminal: &mut Terminal<B>,
  inputs: &mut mpsc::UnboundedReceiver<Input>,
  reason: QuitReason,
) -> Exit
where
  B: Backend,
  S: Service,
{
  log::info!("leaving the terminal UI ({reason:?})");
  // Nothing connects or publishes behind the quit.
  app.service.begin_shutdown();
  if reason.terminal_remains() {
    app.quitting = true;
    draw(app, terminal);
  }

  let service = app.service.clone();
  let shutdown = tokio::time::timeout(SHUTDOWN_TIMEOUT, service.shutdown());
  tokio::pin!(shutdown);
  let mut inputs_open = true;
  loop {
    tokio::select! {
      // A second quit key is looked at first, so that it is never lost to a shutdown
      // that happens to finish in the same instant.
      biased;
      input = inputs.recv(), if inputs_open => match input {
        Some(Input::Terminal(Event::Key(key))) if keys::is_quit(&key) => {
          log::warn!("a second quit key skipped ending the broker session");
          return Exit::Abandoned;
        }
        Some(_) => {}
        None => inputs_open = false,
      },
      result = &mut shutdown => {
        return match result {
          Ok(Ok(())) => {
            log::info!("the MQTT connection and session have ended");
            Exit::Clean
          }
          Ok(Err(error)) => {
            log::warn!("the MQTT session could not be ended cleanly: {error}");
            Exit::Failed(error.to_string())
          }
          Err(_) => {
            log::warn!("MQTT shutdown timed out; leaving the terminal UI");
            Exit::TimedOut
          }
        };
      }
    }
  }
}

fn draw<B: Backend, S: Service>(app: &mut App<S>, terminal: &mut Terminal<B>) -> bool {
  terminal.draw(|frame| layout::render(app, frame)).is_ok()
}

/// Raw mode, the alternate screen, mouse capture, bracketed paste, and the keyboard
/// enhancement flags where the terminal supports them, with a panic hook that undoes it.
fn enter_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
  install_panic_hook();
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
  if matches!(keys::supports_enhancement(), Ok(true))
    && execute!(stdout, PushKeyboardEnhancementFlags(keys::ENHANCEMENT_FLAGS)).is_ok()
  {
    ENHANCED.store(true, Ordering::SeqCst);
  }
  let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
  terminal.clear()?;
  Ok(terminal)
}

/// Gives the terminal back. Every step is attempted, and failures are ignored: after a
/// hangup there is no terminal left to give back.
fn restore_terminal() {
  let mut stdout = io::stdout();
  if ENHANCED.swap(false, Ordering::SeqCst) {
    let _ = execute!(stdout, PopKeyboardEnhancementFlags);
  }
  let _ = execute!(
    stdout,
    DisableBracketedPaste,
    DisableMouseCapture,
    LeaveAlternateScreen,
    crossterm::cursor::Show
  );
  let _ = disable_raw_mode();
  let _ = stdout.flush();
}

/// Restores the terminal before the default hook prints a panic, so the message lands
/// on a usable screen rather than inside the alternate one.
fn install_panic_hook() {
  let previous = std::panic::take_hook();
  std::panic::set_hook(Box::new(move |info| {
    restore_terminal();
    previous(info);
  }));
}

/// Reads the terminal and the signals into one channel.
fn forward_inputs() -> mpsc::UnboundedReceiver<Input> {
  let (sender, receiver) = mpsc::unbounded_channel();
  let keys = sender.clone();
  tokio::spawn(async move {
    let mut events = EventStream::new();
    loop {
      match poll_fn(|context| Pin::new(&mut events).poll_next(context)).await {
        Some(Ok(event)) => {
          if keys.send(Input::Terminal(event)).is_err() {
            return;
          }
        }
        Some(Err(error)) => {
          log::warn!("the terminal can no longer be read: {error}");
          let _ = keys.send(Input::Quit(QuitReason::Hangup));
          return;
        }
        None => {
          let _ = keys.send(Input::Quit(QuitReason::Hangup));
          return;
        }
      }
    }
  });
  watch_signals(sender);
  receiver
}

/// `SIGHUP` when the terminal is closed, and `SIGTERM` when the process is asked to end.
#[cfg(unix)]
fn watch_signals(sender: mpsc::UnboundedSender<Input>) {
  use tokio::signal::unix::{SignalKind, signal};

  for (kind, reason) in [
    (SignalKind::hangup(), QuitReason::Hangup),
    (SignalKind::terminate(), QuitReason::Terminate),
  ] {
    match signal(kind) {
      Ok(mut stream) => {
        let sender = sender.clone();
        tokio::spawn(async move {
          while stream.recv().await.is_some() {
            if sender.send(Input::Quit(reason)).is_err() {
              return;
            }
          }
        });
      }
      Err(error) => log::warn!("cannot listen for {reason:?}: {error}"),
    }
  }
}

/// The console control events: closing the window, logging off, and shutting down
/// leave about five seconds; `Ctrl+Break` is Windows' `SIGTERM`.
#[cfg(windows)]
fn watch_signals(sender: mpsc::UnboundedSender<Input>) {
  use tokio::signal::windows;

  macro_rules! watch {
    ($listener:expr, $reason:expr) => {
      match $listener {
        Ok(mut stream) => {
          let sender = sender.clone();
          tokio::spawn(async move {
            while stream.recv().await.is_some() {
              if sender.send(Input::Quit($reason)).is_err() {
                return;
              }
            }
          });
        }
        Err(error) => log::warn!("cannot listen for {:?}: {error}", $reason),
      }
    };
  }

  watch!(windows::ctrl_close(), QuitReason::ConsoleClose);
  watch!(windows::ctrl_logoff(), QuitReason::ConsoleClose);
  watch!(windows::ctrl_shutdown(), QuitReason::ConsoleClose);
  watch!(windows::ctrl_break(), QuitReason::Terminate);
}

/// Sends `log` output to `hmc.log` beside the config, and only when asked.
///
/// stderr is the screen while the terminal UI is up, so nothing may be written there.
/// `--verbose` logs at debug and `RUST_LOG` at the level it names; without either,
/// nothing is logged at all.
fn init_logging(verbose: bool, config_path: &Path) {
  if !verbose && std::env::var_os("RUST_LOG").is_none() {
    return;
  }
  let path = log_path(config_path);
  if let Some(directory) = path.parent() {
    let _ = std::fs::create_dir_all(directory);
  }
  let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
    return;
  };
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
    .target(env_logger::Target::Pipe(Box::new(file)))
    .format_target(false)
    .try_init();
}

/// Where the log file goes.
pub fn log_path(config_path: &Path) -> PathBuf {
  config_path
    .parent()
    .map(|directory| directory.join(LOG_FILE_NAME))
    .unwrap_or_else(|| PathBuf::from(LOG_FILE_NAME))
}
