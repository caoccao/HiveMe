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

//! The main application's tray. The notification helper never creates one.

use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};

use anyhow::{Result, anyhow, ensure};
use hiveme_core::i18n::{Locale, t};
use tauri::{AppHandle, Manager};

use crate::{constants::APP_NAME, protocol::AppState, window};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod native;

#[derive(Default)]
pub struct State {
  online: Arc<AtomicBool>,
  #[cfg(target_os = "linux")]
  handle: tokio::sync::Mutex<Option<ksni::Handle<linux::Tray>>>,
}

fn locale(app: &AppHandle) -> Locale {
  Locale::resolve(&app.state::<AppState>().session.config().gui.language)
}

fn icon() -> tauri::image::Image<'static> {
  // Keep the app's colored hive icon. A template mask of this artwork would
  // erase its white speech bubble and leave only a solid hexagon on macOS.
  tauri::include_image!("icons/32x32.png")
}

pub async fn on_main<T: Send + 'static>(
  app: &AppHandle,
  f: impl FnOnce(&AppHandle) -> Result<T> + Send + 'static,
) -> Result<T> {
  let handle = app.clone();
  let (send, receive) = tokio::sync::oneshot::channel();
  app.run_on_main_thread(move || {
    let _ = send.send(f(&handle));
  })?;
  receive.await.map_err(|_| anyhow!("the window event loop stopped"))?
}

/// Register at startup, even while the main window is visible. The notification
/// helper uses a different setup path, so it never creates a second tray icon.
pub fn setup(app: &AppHandle) {
  #[cfg(not(target_os = "linux"))]
  if let Err(error) = native::prepare(app) {
    log::warn!("could not create the system tray at startup: {error}");
  }
  #[cfg(target_os = "linux")]
  {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
      // A desktop may start the StatusNotifier service after the application.
      // Keep the window responsive and retry without requiring a toolbar click.
      register_with_retry(|| linux::prepare(&app), std::time::Duration::from_secs(2)).await;
    });
  }
}

#[cfg(any(target_os = "linux", test))]
async fn register_with_retry<F, Fut>(mut register: F, retry_delay: std::time::Duration)
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = Result<()>>,
{
  let mut reported = false;
  loop {
    match register().await {
      Ok(()) => return,
      Err(error) if !reported => {
        log::warn!("waiting for the system tray service: {error}");
        reported = true;
      }
      Err(_) => {}
    }
    tokio::time::sleep(retry_delay).await;
  }
}

/// Create one usable tray before hiding anything. A missing Linux tray host must
/// leave the window accessible. Registration can be retried after a host appears.
pub async fn minimize(app: &AppHandle) -> Result<(), String> {
  let result = async {
    #[cfg(target_os = "linux")]
    linux::prepare(app).await?;
    on_main(app, |app| {
      #[cfg(not(target_os = "linux"))]
      native::prepare(app)?;
      ensure!(
        app.state::<State>().online.load(Ordering::SeqCst),
        "no system tray host is available"
      );
      window::hide_to_tray(app)
    })
    .await
  }
  .await;
  result.map_err(|error: anyhow::Error| {
    log::warn!("could not minimize to the system tray: {error}");
    t(locale(app), "tray.unavailable")
  })
}

/// Reuse the current language after a settings save without recreating the icon.
pub async fn refresh(app: &AppHandle) {
  #[cfg(target_os = "linux")]
  let result = linux::refresh(app).await;
  #[cfg(not(target_os = "linux"))]
  let result = on_main(app, native::refresh).await;
  if let Err(error) = result {
    log::warn!("could not update the tray menu: {error}");
  }
}

fn restore(app: &AppHandle) {
  let handle = app.clone();
  if let Err(error) = app.run_on_main_thread(move || {
    if let Err(error) = window::restore_from_tray(&handle) {
      log::warn!("could not restore the main window: {error}");
    }
  }) {
    log::warn!("could not schedule window restoration: {error}");
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn startup_registers_immediately_and_stops_after_success() {
    let mut attempts = 0;
    register_with_retry(
      || {
        attempts += 1;
        std::future::ready(Ok(()))
      },
      std::time::Duration::ZERO,
    )
    .await;
    assert_eq!(attempts, 1);
  }

  #[tokio::test]
  async fn startup_retries_until_a_late_desktop_tray_service_is_available() {
    let mut attempts = 0;
    register_with_retry(
      || {
        attempts += 1;
        std::future::ready(if attempts < 3 {
          Err(anyhow!("tray service is starting"))
        } else {
          Ok(())
        })
      },
      std::time::Duration::ZERO,
    )
    .await;
    assert_eq!(attempts, 3);
  }

  #[test]
  fn every_tray_string_is_translated_in_every_catalog() {
    for locale in Locale::ALL {
      for key in [
        "toolbar.minimizeToTray",
        "tray.restore",
        "tray.exit",
        "tray.unavailable",
      ] {
        let text = t(locale, key);
        assert!(!text.is_empty());
        assert_ne!(text, key);
        if locale != Locale::EnUs {
          assert_ne!(text, t(Locale::EnUs, key), "{locale}: {key}");
        }
      }
    }
  }

  #[test]
  fn the_tray_uses_the_embedded_app_icon_with_transparency() {
    let icon = icon();
    assert_eq!((icon.width(), icon.height()), (32, 32));
    assert!(icon.rgba().chunks_exact(4).any(|pixel| pixel[3] == 0));
    assert!(
      icon
        .rgba()
        .chunks_exact(4)
        .any(|pixel| pixel[3] == 255 && pixel[0] != pixel[2])
    );
  }
}
