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

//! The small, always-on-top notification window, following BatchMkvMerge's design.
//! This mode opens neither a session nor a config/history file. Both applications
//! address this one host through the authenticated, per-user local endpoint.

use crate::protocol::TopmostSnapshot;
use hiveme_core::desktop::host::{Request, Server};
use hiveme_core::session::TopmostNotification;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const LABEL: &str = "notification";
const EVENT: &str = "topmost-notification";

#[derive(Default)]
struct WindowState {
  revision: u64,
  current: Option<TopmostSnapshot>,
  visible: bool,
}

impl WindowState {
  fn replace(&mut self, content: TopmostNotification) -> TopmostSnapshot {
    self.revision += 1;
    let snapshot = TopmostSnapshot {
      revision: self.revision,
      content,
    };
    self.current = Some(snapshot.clone());
    snapshot
  }

  fn dismiss(&mut self, revision: u64) -> bool {
    if revision != self.revision {
      return false;
    }
    self.current = None;
    self.visible = false;
    true
  }
}

#[derive(Default)]
pub struct State(Mutex<WindowState>);

fn check_window(window: &tauri::WebviewWindow) -> Result<(), String> {
  if window.label() != LABEL {
    return Err("only the notification window may use this command".to_owned());
  }
  Ok(())
}

pub fn current(window: &tauri::WebviewWindow, state: &State) -> Result<Option<TopmostSnapshot>, String> {
  check_window(window)?;
  Ok(state.0.lock().unwrap_or_else(|e| e.into_inner()).current.clone())
}

async fn on_window_thread(
  window: &tauri::WebviewWindow,
  action: impl FnOnce(&tauri::WebviewWindow, &mut WindowState) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
  check_window(window)?;
  let target = window.clone();
  let (send, recv) = tokio::sync::oneshot::channel();
  window
    .run_on_main_thread(move || {
      let state = target.state::<State>();
      let result = action(&target, &mut state.0.lock().unwrap_or_else(|e| e.into_inner()));
      let _ = send.send(result);
    })
    .map_err(|e| e.to_string())?;
  recv.await.map_err(|e| e.to_string())?
}

pub async fn close(window: &tauri::WebviewWindow, revision: u64) -> Result<(), String> {
  on_window_thread(window, move |window, state| {
    if state.dismiss(revision) {
      window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
  })
  .await
}

pub async fn ready(window: &tauri::WebviewWindow, revision: u64) -> Result<(), String> {
  on_window_thread(window, move |window, state| {
    if state.revision != revision || state.current.is_none() {
      return Ok(());
    }
    center_on_screen(window)?;
    window.set_always_on_top(true).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    state.visible = true;
    Ok(())
  })
  .await
}

fn center_on_screen(window: &tauri::WebviewWindow) -> Result<(), String> {
  let monitor = window
    .current_monitor()
    .map_err(|e| e.to_string())?
    .or(window.primary_monitor().map_err(|e| e.to_string())?)
    .ok_or("the notification screen is unavailable")?;
  let position = centered_position(
    *monitor.position(),
    *monitor.size(),
    window.outer_size().map_err(|e| e.to_string())?,
  );
  log::debug!(
    "notification screen {:?} at {:?}; centered position {position:?}",
    monitor.size(),
    monitor.position()
  );
  window.set_position(position).map_err(|e| e.to_string())
}

fn centered_position(
  screen_origin: tauri::PhysicalPosition<i32>,
  screen_size: tauri::PhysicalSize<u32>,
  window_size: tauri::PhysicalSize<u32>,
) -> tauri::PhysicalPosition<i32> {
  // All three values come from Rust's native window APIs in physical pixels.
  // Center in the full screen, including its origin on a multi-monitor desktop.
  // NSWindow.center() intentionally places windows above the mathematical center.
  let axis = |origin: i32, screen: u32, window: u32| {
    (i64::from(origin) + (i64::from(screen) - i64::from(window)) / 2).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
      as i32
  };
  tauri::PhysicalPosition::new(
    axis(screen_origin.x, screen_size.width, window_size.width),
    axis(screen_origin.y, screen_size.height, window_size.height),
  )
}

fn show(app: &tauri::AppHandle, content: TopmostNotification) -> Result<(), String> {
  let snapshot = app
    .state::<State>()
    .0
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .replace(content);
  let window = app
    .get_webview_window(LABEL)
    .ok_or("notification window is unavailable")?;
  window.set_title(&snapshot.content.title).map_err(|e| e.to_string())?;
  window.emit(EVENT, snapshot).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn show_system(title: &str, body: &str) -> Result<(), String> {
  // The modern API checks actual OS authorization and presents banners even when
  // the notification window has focus. The host is a signed accessory app.
  if !mac_usernotifications::blocking::request_auth().map_err(|e| e.to_string())? {
    // The wrapper drops the NSError. A request that leaves the status undetermined
    // was refused without asking, which macOS does for an invalidly signed bundle.
    let settings = mac_usernotifications::blocking::get_notification_settings().map_err(|e| e.to_string())?;
    if settings.authorization_status == mac_usernotifications::AuthorizationStatus::NotDetermined {
      return Err(format!(
        "macOS refused to register {} for notifications; its application bundle is not validly signed",
        std::env::current_exe()
          .map(|path| path.display().to_string())
          .unwrap_or_else(|_| "hmg".to_owned())
      ));
    }
    return Err("OS notification permission is denied; enable HiveMe in System Settings > Notifications".to_owned());
  }
  let notification = mac_usernotifications::Notification::new()
    .title(title)
    .message(body)
    .default_sound();
  // Schedule the request and wait only for OS acceptance. Notification::send_blocking
  // first checks CFRunLoopIsWaiting, which is false whenever the UI is doing work
  // even though Tauri is driving its event loop. That races with topmost rendering.
  mac_usernotifications::blocking::send(notification)
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
fn show_system(title: &str, body: &str) -> Result<(), String> {
  hiveme_core::desktop::show_system_notification(title, body)
}

fn serve(app: tauri::AppHandle, server: Server) {
  let server = Arc::new(server);
  let busy = Arc::new(AtomicUsize::new(0));
  let mut last_request = Instant::now();
  if let Err(error) = server.listener.set_nonblocking(true) {
    log::error!("notification listener: {error}");
    app.exit(1);
    return;
  }
  loop {
    match server.listener.accept() {
      Ok((mut stream, _)) => {
        let request = match server.receive(&mut stream) {
          Ok(request) => request,
          Err(error) => {
            Server::reply(&mut stream, Err(error));
            continue;
          }
        };
        last_request = Instant::now();
        match request {
          Request::System { title, body } => {
            let busy = busy.clone();
            busy.fetch_add(1, Ordering::Relaxed);
            std::thread::spawn(move || {
              Server::reply(&mut stream, show_system(&title, &body));
              busy.fetch_sub(1, Ordering::Relaxed);
            });
          }
          Request::Topmost(content) => {
            // Use the existing window; never create windows from callbacks (which
            // can deadlock WebView2 on Windows). Native UI changes run on its thread.
            let handle = app.clone();
            let (send, recv) = std::sync::mpsc::sync_channel(1);
            let result = app
              .run_on_main_thread(move || {
                let _ = send.send(show(&handle, content));
              })
              .map_err(|e| e.to_string())
              .and_then(|_| recv.recv_timeout(Duration::from_secs(5)).map_err(|e| e.to_string()))
              .and_then(|result| result);
            Server::reply(&mut stream, result);
          }
        }
      }
      Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
        if last_request.elapsed() > Duration::from_secs(60) && busy.load(Ordering::Relaxed) == 0 {
          let state = app.state::<State>();
          let state = state.0.lock().unwrap_or_else(|e| e.into_inner());
          if !state.visible && state.current.is_none() {
            app.exit(0);
            break;
          }
        }
        std::thread::sleep(Duration::from_millis(20));
      }
      Err(error) => {
        log::error!("notification listener: {error}");
        app.exit(1);
        break;
      }
    }
  }
}

/// Finder/LaunchServices may reopen the cached macOS bundle without arguments.
/// Its executable must never enter the normal application's session startup.
pub fn is_host_launch() -> bool {
  std::env::args_os()
    .nth(1)
    .is_some_and(|arg| arg == "--notification-host")
    || std::env::current_exe().is_ok_and(|path| is_host_bundle(&path))
}

fn is_host_bundle(executable: &std::path::Path) -> bool {
  executable
    .parent()
    .and_then(std::path::Path::parent)
    .and_then(std::path::Path::parent)
    .and_then(std::path::Path::file_name)
    .is_some_and(|name| name == "HiveMe Notifications.app")
}

pub fn run(mut context: tauri::Context<tauri::Wry>) {
  let result = hiveme_core::desktop::host::directory().and_then(|directory| Server::bind(&directory));
  let server = match result {
    Ok(Some(server)) => server,
    Ok(None) => return,
    Err(error) => {
      log::error!("notification host: {error}");
      return;
    }
  };
  context.config_mut().app.windows = vec![
    serde_json::from_value(serde_json::json!({
      "label": LABEL, "url": "index.html?view=notification", "title": "HiveMe",
      "width": 440, "height": 240, "resizable": false, "maximizable": false,
      "minimizable": false, "decorations": false, "alwaysOnTop": true,
      "skipTaskbar": true, "visible": false, "focus": false
    }))
    .expect("the notification window configuration is valid"),
  ];
  tauri::Builder::default()
    .manage(State::default())
    .setup(move |app| {
      #[cfg(target_os = "macos")]
      app.set_activation_policy(tauri::ActivationPolicy::Accessory);
      let handle = app.handle().clone();
      std::thread::spawn(move || serve(handle, server));
      Ok(())
    })
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::Moved(position) = event {
        log::debug!("notification window moved to {position:?}");
      }
      if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let state = window.state::<State>();
        let mut state = state.0.lock().unwrap_or_else(|e| e.into_inner());
        let revision = state.revision;
        state.dismiss(revision);
        let _ = window.hide();
      }
    })
    .invoke_handler(tauri::generate_handler![
      crate::close_topmost_notification,
      crate::get_topmost_notification,
      crate::ready_topmost_notification
    ])
    .build(context)
    .expect("the notification host builds")
    .run(|_, event| {
      if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
        api.prevent_exit();
      }
    });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_notification_is_centered_in_physical_screen_coordinates() {
    for (origin, screen, window, expected) in [
      ((0, 0), (1920, 1080), (440, 240), (740, 420)),
      // Retina: both screen and actual window dimensions are doubled.
      ((0, 0), (3840, 2160), (880, 480), (1480, 840)),
      // A display left of and above the primary display.
      ((-2560, -1440), (2560, 1440), (660, 360), (-1610, -900)),
      ((1920, 120), (2560, 1440), (440, 240), (2980, 720)),
      ((0, 0), (400, 200), (440, 240), (-20, -20)),
    ] {
      assert_eq!(
        centered_position(origin.into(), screen.into(), window.into()),
        tauri::PhysicalPosition::from(expected)
      );
    }
  }

  #[test]
  fn the_cached_bundle_is_notification_only_even_without_arguments() {
    assert!(is_host_bundle(std::path::Path::new(
      "/cache/HiveMe Notifications.app/Contents/MacOS/hmg"
    )));
    assert!(!is_host_bundle(std::path::Path::new(
      "/Applications/HiveMe.app/Contents/MacOS/hmg"
    )));
    assert!(!is_host_bundle(std::path::Path::new("/work/target/release/hmg")));
  }

  #[test]
  fn newer_content_replaces_the_only_notification_and_stale_dismissals_do_not_hide_it() {
    let content = |body: &str| TopmostNotification {
      title: "title".into(),
      body: body.into(),
      level: hiveme_core::Level::Info,
      close_label: "Close".into(),
    };
    let mut state = WindowState::default();
    let first = state.replace(content("first"));
    let second = state.replace(content("latest"));
    assert_eq!(state.current.as_ref().unwrap().content.body, "latest");
    assert!(!state.dismiss(first.revision));
    assert_eq!(state.current, Some(second.clone()));
    assert!(state.dismiss(second.revision));
    assert!(state.current.is_none());
    assert!(state.replace(content("reopened")).revision > second.revision);
  }
}
