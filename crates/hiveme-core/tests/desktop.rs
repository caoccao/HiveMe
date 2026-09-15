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

//! Opt-in native smoke test. Run with hmg on PATH and a temporary
//! HIVEME_NOTIFICATION_DIR; HIVEME_TEST_OS_NOTIFICATION=1 also requests an OS banner.
#![cfg(feature = "desktop")]

use hiveme_core::desktop::DesktopToaster;
use hiveme_core::session::{Toaster, TopmostNotification};

#[test]
#[ignore = "raises native OS notifications while updating the window; requires hmg and an explicit notification cache"]
fn native_os_delivery_succeeds_while_the_topmost_window_is_updating() {
  std::env::var_os("HIVEME_NOTIFICATION_DIR").expect("choose the notification cache explicitly");
  let toaster = DesktopToaster::new();
  // Warm up the host, so both workers exercise the same running process.
  toaster
    .show_topmost(&TopmostNotification {
      title: "HiveMe concurrent notification test".into(),
      body: "Preparing UI updates".into(),
      level: hiveme_core::Level::Info,
      close_label: "Close".into(),
    })
    .unwrap();
  let start = std::sync::Barrier::new(2);
  std::thread::scope(|scope| {
    let ui = scope.spawn(|| {
      start.wait();
      for revision in 1..=24 {
        toaster
          .show_topmost(&TopmostNotification {
            title: format!("HiveMe UI update {revision}"),
            body: format!("Update {revision}: OS delivery must work while this window changes."),
            level: hiveme_core::Level::Success,
            close_label: "Close".into(),
          })
          .unwrap();
      }
    });
    start.wait();
    let results: Vec<_> = (1..=6)
      .map(|number| toaster.show("HiveMe delivery test", &format!("OS notification {number} of 6")))
      .collect();
    ui.join().unwrap();
    assert!(results.iter().all(Result::is_ok), "{results:?}");
  });
}

#[test]
#[ignore = "opens a native window; requires hmg and an explicitly isolated notification cache"]
fn the_native_host_reuses_one_window_for_the_latest_message() {
  let directory = std::path::PathBuf::from(std::env::var_os("HIVEME_NOTIFICATION_DIR").expect("use a temporary cache"));
  let toaster = DesktopToaster::new();
  let content = |body: &str| TopmostNotification {
    title: "HiveMe notification test".to_owned(),
    body: body.to_owned(),
    level: hiveme_core::Level::Success,
    close_label: hiveme_core::i18n::t(hiveme_core::i18n::Locale::De, "tabs.close"),
  };
  toaster.show_topmost(&content("First message")).unwrap();
  let endpoint = std::fs::read(directory.join("endpoint.json")).unwrap();
  toaster
    .show_topmost(&content("Latest message — one shared notification window."))
    .unwrap();
  assert_eq!(std::fs::read(directory.join("endpoint.json")).unwrap(), endpoint);
  assert!(!directory.join("HiveMe.json").exists());
  assert!(!directory.join("HiveMe.db").exists());
  if std::env::var_os("HIVEME_TEST_OS_NOTIFICATION").is_some() {
    toaster
      .show("HiveMe notification test", "OS notification delivery is working.")
      .unwrap();
  }
  // Keep the parent alive while a native UI inspector verifies the window.
  if std::env::var_os("HIVEME_TEST_WAIT_FOR_UI").is_some() {
    let start = std::time::Instant::now();
    while !directory.join("inspection-done").exists() {
      assert!(
        start.elapsed() < std::time::Duration::from_secs(300),
        "native inspection timed out"
      );
      std::thread::sleep(std::time::Duration::from_millis(100));
    }
  }
}
