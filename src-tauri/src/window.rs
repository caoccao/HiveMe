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

use hiveme_core::session::SHUTDOWN_TIMEOUT;
use tauri::Manager;

use crate::constants::APP_NAME;
use crate::events;
use crate::protocol::AppState;

/// Set once the restored geometry has been applied, so that the moves and resizes the
/// restore itself causes are not written back as if the user had made them.
pub static WINDOW_READY: AtomicBool = AtomicBool::new(false);

static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static MQTT_STOPPED: AtomicBool = AtomicBool::new(false);

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
      let session = &window.state::<AppState>().session;
      let mut config = session.config();
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
      if let Err(error) = session.set_config_quietly(config) {
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
  let session = app.state::<AppState>().session.clone();
  session.begin_shutdown();
  let app = app.clone();
  tauri::async_runtime::spawn(async move {
    match tokio::time::timeout(SHUTDOWN_TIMEOUT, session.shutdown()).await {
      Ok(Ok(())) => log::info!("the MQTT connection and session have ended"),
      Ok(Err(error)) => log::warn!("the MQTT session could not be ended cleanly: {error}"),
      Err(_) => log::warn!("MQTT shutdown timed out; exiting the application"),
    }
    MQTT_STOPPED.store(true, Ordering::SeqCst);
    app.exit(code.unwrap_or(0));
  });
}

/// Titles and shows the window, then starts the session's connection, pruning, and
/// update check.
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
  let window = app.get_webview_window("main").expect("the main window is configured");
  let _ = window.set_title(&format!("{APP_NAME} v{}", hiveme_core::VERSION));

  let state = app.state::<AppState>();
  let session = state.session.clone();
  state.toaster.attach(app.handle().clone());

  let mut config = session.config();
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
  if resized && let Err(error) = session.set_config_quietly(config) {
    log::warn!("the clamped window size could not be saved: {error}");
  }
  let _ = window.show();
  let _ = window.set_focus();
  WINDOW_READY.store(true, Ordering::SeqCst);

  // Subscribed before anything can happen, so the first status reaches the frontend.
  tauri::async_runtime::spawn(events::forward(app.handle().clone(), session.subscribe()));
  // The session spawns onto the runtime it is started from, which is the one Tauri runs
  // the commands on.
  let _runtime = tauri::async_runtime::handle().inner().enter();
  session.start_background_work();
  Ok(())
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
