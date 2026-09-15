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

/// Unlike the desktop plugin, this returns the delivery error to the caller.
#[cfg(not(target_os = "macos"))]
pub fn show_system_notification(title: &str, body: &str) -> Result<(), String> {
  #[cfg(target_os = "windows")]
  {
    crate::session::register_toast_identity(None)?;
    tauri_winrt_notification::Toast::new(crate::APP_NAME)
      .title(title)
      .text1(body)
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
      .show()
      .map(|_| ())
      .map_err(|error| error.to_string())
  }
}
