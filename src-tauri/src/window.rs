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

use hiveme_core::session::{SHUTDOWN_TIMEOUT, Session};
use tauri::Manager;

use crate::constants::APP_NAME;
use crate::events;
use crate::protocol::AppState;

/// Set once the window is on screen, so that the moves and resizes of the opening itself
/// are not written back as if the user had made them.
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

/// The frame a remembered geometry asks for: a size clamped to the minimums, and a top
/// left corner, or [`None`] to let the window be centered.
fn window_frame(window: &hiveme_core::config::Window) -> ((u32, u32), Option<(i32, i32)>) {
  let size = sanitize_window_size(window.size.width, window.size.height);
  // Either coordinate being negative centers the window, because a half restored position
  // is not a position. A fresh config is -1, -1 and lands wherever the display is.
  let corner = (window.position.x >= 0 && window.position.y >= 0).then_some((window.position.x, window.position.y));
  (size, corner)
}

fn saves_geometry(event: &tauri::WindowEvent) -> bool {
  matches!(event, tauri::WindowEvent::Focused(false))
}

/// Saves geometry on focus loss; moving and resizing never write the config.
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
    event if saves_geometry(event) => save_geometry(window),
    _ => {}
  }
}

fn save_geometry(window: &tauri::Window) {
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
  if let Some(window) = app.get_webview_window("main") {
    save_geometry(&window.as_ref().window());
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

/// Gives the window its remembered geometry and its title before the window is created.
///
/// Moving a window and showing it is the obvious way to restore a position, and on macOS
/// it flickers. `NSWindow` frame changes are not thread safe, so tao puts every move,
/// resize, and retitle on the main dispatch queue, while showing a window runs inline
/// when it is already on the main thread. [`setup`] is on the main thread and runs before
/// the event loop turns, so its `show` went first and the queued move only landed a frame
/// later: the window appeared where macOS had centered it and then jumped.
///
/// Writing the geometry into the window's own configuration instead leaves nothing to
/// defer. Tauri resolves both this and `center` into the frame the window is built with,
/// so the first frame drawn is the right one, on every platform.
///
/// A size below the minimums is clamped, and the clamped size is written back so that a
/// config edited by hand is corrected once rather than argued with on every move.
pub fn place<R: tauri::Runtime>(context: &mut tauri::Context<R>, session: &Session) {
  let Some(window) = context.config_mut().app.windows.first_mut() else {
    return;
  };
  window.title = format!("{APP_NAME} v{}", hiveme_core::VERSION);

  let mut config = session.config();
  let ((width, height), corner) = window_frame(&config.gui.window);
  window.width = f64::from(width);
  window.height = f64::from(height);
  match corner {
    Some((x, y)) => {
      window.x = Some(f64::from(x));
      window.y = Some(f64::from(y));
    }
    None => window.center = true,
  }

  if config.gui.window.size.width == width && config.gui.window.size.height == height {
    return;
  }
  config.gui.window.size.width = width;
  config.gui.window.size.height = height;
  if let Err(error) = session.set_config_quietly(config) {
    log::warn!("the clamped window size could not be saved: {error}");
  }
}

/// Shows the window, then starts the session's connection, pruning, and update check.
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
  let window = app.get_webview_window("main").expect("the main window is configured");

  let state = app.state::<AppState>();
  let session = state.session.clone();
  state.toaster.attach(app.handle().clone());

  // The window was built with its title and its remembered geometry already on it, by
  // `place`. It is configured hidden so that nothing reaches the screen until here.
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

  fn remembered(x: i32, y: i32, width: u32, height: u32) -> hiveme_core::config::Window {
    hiveme_core::config::Window {
      position: hiveme_core::config::WindowPosition { x, y },
      size: hiveme_core::config::WindowSize { width, height },
    }
  }

  #[test]
  fn a_remembered_corner_is_the_frame_the_window_is_built_with() {
    assert_eq!(
      window_frame(&remembered(837, 531, 1200, 900)),
      ((1200, 900), Some((837, 531)))
    );
    // The edge of a display is a corner like any other, and zero is not negative.
    assert_eq!(window_frame(&remembered(0, 0, 1200, 900)), ((1200, 900), Some((0, 0))));
  }

  #[test]
  fn a_negative_coordinate_centers_the_window_instead() {
    let default = hiveme_core::config::Window::default();
    assert_eq!(
      window_frame(&default).1,
      None,
      "a fresh config has no corner to restore"
    );
    assert_eq!(window_frame(&remembered(-1, 531, 1200, 900)).1, None);
    assert_eq!(window_frame(&remembered(837, -1, 1200, 900)).1, None);
  }

  #[test]
  fn the_frame_clamps_a_size_the_window_could_not_take() {
    assert_eq!(
      window_frame(&remembered(837, 531, 1, 2)).0,
      (MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)
    );
  }
  #[test]
  fn geometry_waits_for_focus_loss_instead_of_writing_during_a_drag() {
    use tauri::{PhysicalPosition, PhysicalSize, WindowEvent};
    for point in 0..100 {
      assert!(!saves_geometry(&WindowEvent::Moved(PhysicalPosition::new(
        point, point
      ))));
      assert!(!saves_geometry(&WindowEvent::Resized(PhysicalSize::new(
        800 + point as u32,
        600
      ))));
    }
    assert!(!saves_geometry(&WindowEvent::Focused(true)));
    assert!(saves_geometry(&WindowEvent::Focused(false)));
  }
}
