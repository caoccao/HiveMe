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

use super::*;
use tauri::{
  menu::{Menu, MenuItem},
  tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const ID: &str = "hiveme-main-tray";
const RESTORE: &str = "tray-restore";
const EXIT: &str = "tray-exit";

fn menu(app: &AppHandle) -> Result<Menu<tauri::Wry>> {
  let language = locale(app);
  let restore = MenuItem::with_id(app, RESTORE, t(language, "tray.restore"), true, None::<&str>)?;
  let exit = MenuItem::with_id(app, EXIT, t(language, "tray.exit"), true, None::<&str>)?;
  Ok(Menu::with_items(app, &[&restore, &exit])?)
}

// Windows emits DoubleClick, while macOS reports a double-click as successive
// Click/Up events. Accept both so a double-click always restores the same window;
// normal left-click activation remains supported too.
fn activates(event: &TrayIconEvent) -> bool {
  matches!(
    event,
    TrayIconEvent::Click {
      button: MouseButton::Left,
      button_state: MouseButtonState::Up,
      ..
    } | TrayIconEvent::DoubleClick {
      button: MouseButton::Left,
      ..
    }
  )
}

pub(super) fn prepare(app: &AppHandle) -> Result<()> {
  if app.tray_by_id(ID).is_none() {
    TrayIconBuilder::with_id(ID)
      .icon(icon())
      .icon_as_template(false)
      .tooltip(APP_NAME)
      .menu(&menu(app)?)
      .show_menu_on_left_click(false)
      .on_tray_icon_event(|tray, event| {
        if activates(&event) {
          restore(tray.app_handle());
        }
      })
      .on_menu_event(|app, event| match event.id.as_ref() {
        RESTORE => restore(app),
        EXIT => app.exit(0), // RunEvent::ExitRequested owns the MQTT shutdown.
        _ => {}
      })
      .build(app)?;
  }
  app.state::<State>().online.store(true, Ordering::SeqCst);
  Ok(())
}

pub(super) fn refresh(app: &AppHandle) -> Result<()> {
  if let Some(tray) = app.tray_by_id(ID) {
    tray.set_menu(Some(menu(app)?))?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_left_double_click_restores_but_other_buttons_do_not() {
    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
      let event = TrayIconEvent::DoubleClick {
        id: ID.into(),
        position: tauri::PhysicalPosition::new(0.0, 0.0),
        rect: tauri::Rect {
          position: tauri::PhysicalPosition::new(0, 0).into(),
          size: tauri::PhysicalSize::new(32, 32).into(),
        },
        button,
      };
      assert_eq!(activates(&event), button == MouseButton::Left);
    }
  }

  #[test]
  fn only_a_completed_left_click_restores_the_window() {
    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
      for button_state in [MouseButtonState::Down, MouseButtonState::Up] {
        let event = TrayIconEvent::Click {
          id: ID.into(),
          position: tauri::PhysicalPosition::new(0.0, 0.0),
          rect: tauri::Rect {
            position: tauri::PhysicalPosition::new(0, 0).into(),
            size: tauri::PhysicalSize::new(32, 32).into(),
          },
          button,
          button_state,
        };
        assert_eq!(
          activates(&event),
          button == MouseButton::Left && button_state == MouseButtonState::Up
        );
      }
    }
  }
}
