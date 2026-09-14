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

//! OS notifications, the way `hmg` shows them.
//!
//! Which rule fires, what it says, how often, and whether the toolbar toggle holds it
//! back are all decided in [`hiveme_core::session`]. What cannot live there is the
//! platform call, which is this file: the notification plugin, or on Windows the toast
//! API under an application identity of our own.

use std::sync::OnceLock;

use hiveme_core::session::Toaster;
use tauri::AppHandle;
#[cfg(not(target_os = "windows"))]
use tauri_plugin_notification::NotificationExt;

/// The application identity Windows prints above a toast.
///
/// A Tauri application raises its toasts through the Windows Runtime, and Windows
/// labels a toast with the identity of the process that raised it. Without an
/// identity of our own that label is PowerShell, so one is registered below.
#[cfg(target_os = "windows")]
const SYSTEM_NOTIFICATION_APP_ID: &str = crate::constants::APP_NAME;

/// Shows the notifications the session decides on.
///
/// The session exists before the Tauri application does, so the handle the plugin
/// needs is attached once the application is set up. Nothing can arrive before that,
/// because the first connection is started from the same setup.
pub struct TauriToaster {
  app: OnceLock<AppHandle>,
}

impl TauriToaster {
  pub fn new() -> Self {
    install_system_notification_identity();
    Self { app: OnceLock::new() }
  }

  /// Gives the toaster the application it shows notifications for.
  pub fn attach(&self, app: AppHandle) {
    let _ = self.app.set(app);
  }
}

impl Toaster for TauriToaster {
  /// Windows goes straight to the Windows Runtime rather than through the plugin, so
  /// that the toast carries the identity registered below. Every other platform takes
  /// the plugin, which already labels the notification with the bundle it came from.
  fn show(&self, title: &str, body: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
      let _ = &self.app;
      tauri_winrt_notification::Toast::new(SYSTEM_NOTIFICATION_APP_ID)
        .title(title)
        .text1(body)
        .show()
        .map_err(|error| error.to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
      let app = self
        .app
        .get()
        .ok_or_else(|| "the application is still starting".to_owned())?;
      app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| error.to_string())
    }
  }
}

/// Registers the Windows application identity, once per process.
///
/// A failure is logged and dropped: the toast still appears, under the wrong name, and
/// a notification with the wrong label is better than no notification at all.
fn install_system_notification_identity() {
  #[cfg(target_os = "windows")]
  {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
      if let Err(error) = register_system_notification_identity() {
        log::warn!("the Windows notification identity could not be registered: {error}");
      }
    });
  }
}

/// Writes the `AppUserModelId` entry Windows reads the toast label and icon from.
///
/// An installed build gets its icon from the Start menu shortcut the bundle creates,
/// so only a development build needs to say where the icon is.
#[cfg(target_os = "windows")]
fn register_system_notification_identity() -> Result<(), String> {
  let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/32x32.png");
  hiveme_core::session::register_toast_identity(tauri::is_dev().then_some(icon.as_path()))
}

impl std::fmt::Debug for TauriToaster {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("TauriToaster")
      .field("attached", &self.app.get().is_some())
      .finish()
  }
}
