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

//! OS notifications from the terminal UI.
//!
//! Which rule fires, what it says, how often, and whether the pause toggle holds it back
//! are decided in [`hiveme_core::session`], the same code `hmg` runs. This is only the
//! show call: `notify-rust` on Linux and macOS, and on Windows the toast API under the
//! `HiveMe` application identity `hmg` registers, so the toast is labeled HiveMe rather
//! than the console host.

use hiveme_core::session::Toaster;

/// The application identity a toast is shown under.
const APP_ID: &str = hiveme_core::APP_NAME;

/// Shows the notifications the session decides on.
#[derive(Debug)]
pub struct DesktopToaster;

impl DesktopToaster {
  pub fn new() -> Self {
    #[cfg(target_os = "windows")]
    if let Err(error) = register_identity() {
      // The toast still appears, under the console host's name.
      log::warn!("the Windows notification identity could not be registered: {error}");
    }
    Self
  }
}

impl Toaster for DesktopToaster {
  fn show(&self, title: &str, body: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
      tauri_winrt_notification::Toast::new(APP_ID)
        .title(title)
        .text1(body)
        .show()
        .map_err(|error| error.to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
      notify_rust::Notification::new()
        .appname(APP_ID)
        .summary(title)
        .body(body)
        .show()
        .map(|_| ())
        .map_err(|error| error.to_string())
    }
  }
}

/// Writes the `AppUserModelId` entry Windows reads the toast label from, as `hmg` does.
///
/// Whichever application starts first writes it; the values are the same. The icon
/// comes from the Start menu shortcut an installed `hmg` has.
#[cfg(target_os = "windows")]
fn register_identity() -> Result<(), String> {
  let key = windows_registry::CURRENT_USER
    .create(format!(r"SOFTWARE\Classes\AppUserModelId\{APP_ID}"))
    .map_err(|error| error.to_string())?;
  key
    .set_expand_string("DisplayName", APP_ID)
    .map_err(|error| error.to_string())?;
  key
    .set_string("IconBackgroundColor", "0")
    .map_err(|error| error.to_string())
}
