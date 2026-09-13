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

//! The main window: its title, remembered geometry, startup, and graceful shutdown.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::Manager;

use crate::constants::APP_NAME;
use crate::protocol::{AppState, UpdateCheckResult};
use crate::{config, controller, update};

/// Set once the restored geometry has been applied, so that the moves and resizes the
/// restore itself causes are not written back as if the user had made them.
pub static WINDOW_READY: AtomicBool = AtomicBool::new(false);

static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static MQTT_STOPPED: AtomicBool = AtomicBool::new(false);
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);

const MIN_WINDOW_WIDTH: u32 = 600;
const MIN_WINDOW_HEIGHT: u32 = 450;

fn is_persistable_window_size(width: u32, height: u32) -> bool {
  width >= MIN_WINDOW_WIDTH && height >= MIN_WINDOW_HEIGHT
}

fn sanitize_window_size(width: u32, height: u32) -> (u32, u32) {
  (width.max(MIN_WINDOW_WIDTH), height.max(MIN_WINDOW_HEIGHT))
}

/// Persists the main window's geometry as the user moves and resizes it.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
  if window.label() != "main" {
    return;
  }
  match event {
    tauri::WindowEvent::CloseRequested { api, .. } => {
      // Keep the event loop alive until the MQTT shutdown below has finished.
      api.prevent_close();
      window.app_handle().exit(0);
    }
    tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
      if !WINDOW_READY.load(Ordering::SeqCst) {
        return;
      }
      if window.is_minimized().unwrap_or(false) {
        return;
      }
      let (Ok(scale), Ok(position), Ok(size)) = (window.scale_factor(), window.outer_position(), window.inner_size())
      else {
        return;
      };
      let position: tauri::LogicalPosition<i32> = position.to_logical(scale);
      let size: tauri::LogicalSize<u32> = size.to_logical(scale);
      if !is_persistable_window_size(size.width, size.height) {
        return;
      }
      let mut config = config::get_config();
      if config.gui.window.position.x == position.x
        && config.gui.window.position.y == position.y
        && config.gui.window.size.width == size.width
        && config.gui.window.size.height == size.height
      {
        return;
      }
      config.gui.window.position.x = position.x;
      config.gui.window.position.y = position.y;
      config.gui.window.size.width = size.width;
      config.gui.window.size.height = size.height;
      if let Err(error) = config::set_config_quietly(config) {
        log::error!("the window geometry could not be saved: {error}");
      }
    }
    _ => {}
  }
}

/// All quit paths pass here, including the window close button and the system menu.
pub fn on_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
  let tauri::RunEvent::ExitRequested { api, code, .. } = event else {
    return;
  };
  if MQTT_STOPPED.load(Ordering::SeqCst) {
    return;
  }
  api.prevent_exit();
  if EXIT_REQUESTED.swap(true, Ordering::SeqCst) {
    return;
  }
  let mqtt = app.state::<AppState>().mqtt.clone();
  mqtt.begin_shutdown();
  let app = app.clone();
  tauri::async_runtime::spawn(async move {
    match tokio::time::timeout(EXIT_TIMEOUT, mqtt.shutdown()).await {
      Ok(Ok(())) => log::info!("the MQTT connection and session have ended"),
      Ok(Err(error)) => log::warn!("the MQTT session could not be ended cleanly: {error}"),
      Err(_) => log::warn!("MQTT shutdown timed out; exiting the application"),
    }
    MQTT_STOPPED.store(true, Ordering::SeqCst);
    app.exit(code.unwrap_or(0));
  });
}

/// Titles and shows the window, opens the history, connects, and checks for updates.
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
  let window = app.get_webview_window("main").expect("the main window is configured");
  let _ = window.set_title(&format!("{APP_NAME} v{}", update::app_version()));

  let mut config = config::get_config();
  let (width, height) = sanitize_window_size(config.gui.window.size.width, config.gui.window.size.height);
  let resized = config.gui.window.size.width != width || config.gui.window.size.height != height;
  config.gui.window.size.width = width;
  config.gui.window.size.height = height;
  let _ = window.set_size(tauri::LogicalSize::new(width, height));
  if config.gui.window.position.x < 0 || config.gui.window.position.y < 0 {
    let _ = window.center();
  } else {
    let _ = window.set_position(tauri::LogicalPosition::new(
      config.gui.window.position.x,
      config.gui.window.position.y,
    ));
  }
  if resized && let Err(error) = config::set_config_quietly(config.clone()) {
    log::warn!("the clamped window size could not be saved: {error}");
  }
  let _ = window.show();
  let _ = window.set_focus();
  WINDOW_READY.store(true, Ordering::SeqCst);

  start_background_work(app.handle().clone(), &config);
  Ok(())
}

/// Everything that should be running by the time the user looks at the window.
fn start_background_work(app: tauri::AppHandle, config: &hiveme_core::Config) {
  let handle = app.clone();
  tauri::async_runtime::spawn(async move {
    controller::prune_forever(handle).await;
  });

  // A config without a broker cannot connect, and saying so in the status bar is more
  // use than a failed connection attempt the user did not ask for.
  if config.broker.url.trim().is_empty() {
    log::info!("no broker is configured yet, not connecting");
  } else {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
      if let Err(error) = controller::connect(&handle).await {
        log::warn!("the first connection did not come up: {error}");
      }
    });
  }

  start_update_check(app, config);
}

/// Runs, or skips, the release check, and leaves the answer where the frontend polls.
fn start_update_check(app: tauri::AppHandle, config: &hiveme_core::Config) {
  let Some(state) = app.try_state::<AppState>() else {
    return;
  };
  let result = state.update.clone();
  let last_checked = config.update.last_checked;
  let last_version = config.update.last_version.clone();
  let ignore_version = config.update.ignore_version.clone();
  let interval = update::interval_seconds(config.update.check_interval);
  let now = update::now_seconds();

  if last_checked == 0 || now - last_checked > interval {
    std::thread::spawn(move || {
      let checked = update::check();
      match checked {
        Ok(outcome) => {
          let mut config = config::get_config();
          config.update.last_checked = update::now_seconds();
          if let Some(version) = outcome.latest_version.as_ref() {
            config.update.last_version = version.clone();
          }
          let ignored = config.update.ignore_version.clone();
          let _ = config::set_config_quietly(config);
          let outcome =
            if outcome.has_update && !ignored.is_empty() && outcome.latest_version.as_deref() == Some(&ignored) {
              UpdateCheckResult {
                has_update: false,
                latest_version: None,
              }
            } else {
              outcome
            };
          *result.lock().unwrap() = Some(outcome);
        }
        Err(error) => {
          log::warn!("the release check failed: {error}");
          *result.lock().unwrap() = Some(UpdateCheckResult {
            has_update: false,
            latest_version: None,
          });
        }
      }
    });
    return;
  }

  // Not due. What the last check found still counts, so the notice survives a restart.
  let pending = !last_version.is_empty()
    && update::is_newer(&last_version, update::app_version())
    && last_version != ignore_version;
  *result.lock().unwrap() = Some(UpdateCheckResult {
    has_update: pending,
    latest_version: pending.then_some(last_version),
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn persistable_window_size_respects_the_configured_minimums() {
    assert!(is_persistable_window_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT));
    assert!(!is_persistable_window_size(MIN_WINDOW_WIDTH - 1, MIN_WINDOW_HEIGHT));
    assert!(!is_persistable_window_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT - 1));
  }

  #[test]
  fn sanitize_window_size_clamps_a_poisoned_config_value() {
    assert_eq!(sanitize_window_size(1, 2), (MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT));
    assert_eq!(
      sanitize_window_size(MIN_WINDOW_WIDTH + 100, MIN_WINDOW_HEIGHT + 100),
      (MIN_WINDOW_WIDTH + 100, MIN_WINDOW_HEIGHT + 100)
    );
  }
}
