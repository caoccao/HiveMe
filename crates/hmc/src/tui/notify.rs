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
//! than the console host. macOS labels a toast with the application of a bundle
//! identifier, and an unbundled `hmc` has none, so it borrows the one of `hmg`.

use hiveme_core::session::Toaster;

/// The application identity a toast is shown under.
const APP_ID: &str = hiveme_core::APP_NAME;

/// The bundle identifier of `hmg`, `identifier` in `src-tauri/tauri.conf.json`.
#[cfg_attr(
  not(target_os = "macos"),
  allow(dead_code, reason = "only macOS labels toasts by bundle")
)]
const MACOS_BUNDLE_ID: &str = "com.caoccao.hiveme";

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
    // Set before the first toast, which would otherwise look an application up by name
    // and settle on Finder. Where HiveMe.app is not installed this fails, and the toasts
    // are labeled Terminal instead.
    #[cfg(target_os = "macos")]
    if let Err(error) = notify_rust::set_application(MACOS_BUNDLE_ID) {
      log::info!("toasts are not labeled HiveMe, {MACOS_BUNDLE_ID} is not installed: {error}");
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
  hiveme_core::session::register_toast_identity(None)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_macos_toast_identity_is_the_bundle_identifier_of_hmg() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/tauri.conf.json");
    let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(config["identifier"], MACOS_BUNDLE_ID);
  }
}
