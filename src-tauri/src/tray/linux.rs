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

//! StatusNotifier activation works on Linux, where Tauri tray mouse events do not.

use super::*;
use ksni::{TrayMethods, menu::StandardItem};

pub(super) struct Tray {
  app: AppHandle,
  language: Locale,
  online: Arc<AtomicBool>,
}

fn argb_icon() -> ksni::Icon {
  let icon = icon();
  ksni::Icon {
    width: icon.width() as i32,
    height: icon.height() as i32,
    data: icon
      .rgba()
      .chunks_exact(4)
      .flat_map(|p| [p[3], p[0], p[1], p[2]])
      .collect(),
  }
}

impl ksni::Tray for Tray {
  // Advertise activation separately from the context menu. A normal left click
  // activates the window; the host owns the right-click context menu. Linux
  // reports Activate rather than a click count, including double-click gestures.
  const MENU_ON_ACTIVATE: bool = false;

  fn id(&self) -> String {
    "hiveme".into()
  }
  fn title(&self) -> String {
    APP_NAME.into()
  }
  fn icon_pixmap(&self) -> Vec<ksni::Icon> {
    vec![argb_icon()]
  }
  fn tool_tip(&self) -> ksni::ToolTip {
    ksni::ToolTip {
      title: APP_NAME.into(),
      ..Default::default()
    }
  }
  fn activate(&mut self, _x: i32, _y: i32) {
    restore(&self.app);
  }
  fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
    vec![
      StandardItem {
        label: t(self.language, "tray.restore"),
        activate: Box::new(|tray: &mut Self| restore(&tray.app)),
        ..Default::default()
      }
      .into(),
      StandardItem {
        label: t(self.language, "tray.exit"),
        activate: Box::new(|tray: &mut Self| tray.app.exit(0)),
        ..Default::default()
      }
      .into(),
    ]
  }
  fn watcher_online(&self) {
    self.online.store(true, Ordering::SeqCst);
  }
  fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
    self.online.store(false, Ordering::SeqCst);
    log::warn!("system tray host went offline: {reason:?}");
    let app = self.app.clone();
    let _ = self.app.run_on_main_thread(move || {
      if let Some(window) = app.get_webview_window("main")
        && !window.is_visible().unwrap_or(true)
        && let Err(error) = window::restore_from_tray(&app)
      {
        log::warn!("could not restore the window after losing the tray host: {error}");
      }
    });
    true // Re-register if the desktop's tray host comes back.
  }
}

pub(super) async fn prepare(app: &AppHandle) -> Result<()> {
  let state = app.state::<State>();
  let mut handle = state.handle.lock().await;
  if handle.as_ref().is_some_and(|handle| !handle.is_closed()) {
    return Ok(());
  }
  // Set before spawn: watcher_offline may run during registration and must win.
  state.online.store(true, Ordering::SeqCst);
  let tray = Tray {
    app: app.clone(),
    language: locale(app),
    online: state.online.clone(),
  };
  // A unique D-Bus connection name permits more than one MQTT GUI session.
  match tray.disable_dbus_name(true).spawn().await {
    Ok(tray) => {
      *handle = Some(tray);
      Ok(())
    }
    Err(error) => {
      state.online.store(false, Ordering::SeqCst);
      Err(error.into())
    }
  }
}

pub(super) async fn refresh(app: &AppHandle) -> Result<()> {
  if let Some(handle) = app.state::<State>().handle.lock().await.as_ref() {
    handle.update(|tray| tray.language = locale(app)).await;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn linux_pixmap_converts_rgba_to_network_order_argb() {
    let source = icon();
    let converted = argb_icon();
    assert_eq!((converted.width, converted.height), (32, 32));
    assert_eq!(converted.data.len(), source.rgba().len());
    for (rgba, argb) in source.rgba().chunks_exact(4).zip(converted.data.chunks_exact(4)) {
      assert_eq!(argb, [rgba[3], rgba[0], rgba[1], rgba[2]]);
    }
    // The activation policy is a compile-time contract; pixel data above is runtime.
    const { assert!(!<Tray as ksni::Tray>::MENU_ON_ACTIVATE) };
  }
}
