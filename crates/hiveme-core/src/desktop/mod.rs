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

//! Desktop delivery shared by hmg and interactive hmc.
//!
//! A single hmg notification host owns the topmost window for this desktop user.
//! macOS also delivers OS banners there, from a real, signed application bundle.

pub mod host;
#[cfg(target_os = "macos")]
mod macos;

use crate::session::{Toaster, TopmostNotification};

#[derive(Debug, Default)]
pub struct DesktopToaster;

impl DesktopToaster {
  pub fn new() -> Self {
    Self
  }
}

impl Toaster for DesktopToaster {
  fn show(&self, title: &str, body: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return host::send(&host::Request::System {
      title: title.to_owned(),
      body: body.to_owned(),
    });
    #[cfg(not(target_os = "macos"))]
    show_system_notification(title, body)
  }

  fn show_topmost(&self, notification: &TopmostNotification) -> Result<(), String> {
    host::send(&host::Request::Topmost(notification.clone()))
  }
}

/// Native notification APIs need a file, including when hmc runs without a bundle.
#[cfg(any(target_os = "windows", target_os = "linux", test))]
fn system_notification_icon(directory: &std::path::Path) -> Result<std::path::PathBuf, String> {
  use std::fs;
  const PNG: &[u8] = include_bytes!("../../../../src-tauri/icons/128x128.png");
  let icon = directory.join("icon.png");
  if fs::read(&icon).ok().as_deref() == Some(PNG) {
    return Ok(icon);
  }
  fs::create_dir_all(directory).map_err(|e| e.to_string())?;
  // Both applications can request an icon concurrently. Publish only complete files.
  let temporary = directory.join(format!("icon-{}.tmp", uuid::Uuid::now_v7()));
  let result = fs::write(&temporary, PNG).and_then(|()| fs::rename(&temporary, &icon));
  if result.is_err() {
    let _ = fs::remove_file(&temporary);
  }
  result.map_err(|e| e.to_string())?;
  Ok(icon)
}

/// Unlike the desktop plugin, this returns the delivery error to the caller.
#[cfg(not(target_os = "macos"))]
pub fn show_system_notification(title: &str, body: &str) -> Result<(), String> {
  let icon = system_notification_icon(&host::directory()?)?;
  #[cfg(target_os = "windows")]
  {
    crate::session::register_toast_identity(Some(&icon))?;
    tauri_winrt_notification::Toast::new(crate::APP_NAME)
      .title(title)
      .text1(body)
      .icon(&icon, tauri_winrt_notification::IconCrop::Square, crate::APP_NAME)
      .show()
      .map_err(|error| error.to_string())
  }
  #[cfg(target_os = "linux")]
  {
    // freedesktop notification bodies may interpret markup; MQTT text is plain text.
    let body = body.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    notify_rust::Notification::new()
      .appname(crate::APP_NAME)
      .summary(title)
      .body(&body)
      .icon(&icon.to_string_lossy())
      .show()
      .map(|_| ())
      .map_err(|error| error.to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  #[test]
  fn native_icon_is_available_without_an_app_bundle_and_repairs_a_damaged_cache() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("notifications");
    let icon = system_notification_icon(&directory).unwrap();
    let expected = include_bytes!("../../../../src-tauri/icons/128x128.png");
    assert_eq!(fs::read(&icon).unwrap(), expected);
    let modified = fs::metadata(&icon).unwrap().modified().unwrap();
    assert_eq!(system_notification_icon(&directory).unwrap(), icon);
    assert_eq!(fs::metadata(&icon).unwrap().modified().unwrap(), modified);
    fs::write(&icon, "incomplete icon").unwrap();
    assert_eq!(system_notification_icon(&directory).unwrap(), icon);
    assert_eq!(fs::read(&icon).unwrap(), expected);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
  }
}
