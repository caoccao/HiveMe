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

//! The release check, against the GitHub releases of the repository.
//!
//! It runs on its own thread at startup when `update.checkInterval` says it is due, and
//! the result waits in the session until the screen asks for it. Nothing is downloaded
//! or installed; the user is told there is a newer version and taken to the releases
//! page.

use std::sync::{Arc, Mutex};

use crate::config::{Update, UpdateCheckInterval};

use super::config::ConfigStore;
use super::types::UpdateCheckResult;

/// The repository the update check and the About tab point at.
pub const GITHUB_URL: &str = "https://github.com/caoccao/HiveMe";

/// Where the update check reads the published releases.
pub const RELEASES_API_URL: &str = "https://api.github.com/repos/caoccao/HiveMe/releases";

/// How long a check interval is, in seconds.
pub fn interval_seconds(interval: UpdateCheckInterval) -> i64 {
  match interval {
    UpdateCheckInterval::Daily => 86_400,
    UpdateCheckInterval::Weekly => 604_800,
    UpdateCheckInterval::Monthly => 2_592_000,
  }
}

/// Unix seconds now.
pub fn now_seconds() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|elapsed| elapsed.as_secs() as i64)
    .unwrap_or(0)
}

/// The version this build is.
pub fn app_version() -> &'static str {
  crate::VERSION
}

/// Asks GitHub for the newest release.
pub fn check() -> Result<UpdateCheckResult, String> {
  let current = app_version();
  log::info!("checking for a newer release than {current}");
  let response = ureq::get(RELEASES_API_URL)
    .set("User-Agent", crate::APP_NAME)
    .call()
    .map_err(|error| format!("the releases could not be fetched: {error}"))?;
  let releases: serde_json::Value = response
    .into_json()
    .map_err(|error| format!("the releases could not be read: {error}"))?;
  let Some(latest) = releases.as_array().and_then(|releases| releases.first()) else {
    return Ok(UpdateCheckResult::none());
  };
  let tag = latest["tag_name"].as_str().unwrap_or_default();
  log::info!("the newest release is tagged {tag}");
  if is_newer(tag, current) {
    return Ok(UpdateCheckResult {
      has_update: true,
      latest_version: Some(tag.trim_start_matches('v').to_owned()),
    });
  }
  Ok(UpdateCheckResult::none())
}

/// Whether `latest` is a higher version than `current`.
///
/// Compares the dotted numbers, ignoring a leading `v` and anything that is not a
/// number, so a pre-release suffix never counts as newer than the release it precedes.
pub fn is_newer(latest: &str, current: &str) -> bool {
  let parts = |version: &str| -> Vec<u32> {
    version
      .trim_start_matches('v')
      .split('.')
      .filter_map(|part| part.parse().ok())
      .collect()
  };
  let latest = parts(latest);
  let current = parts(current);
  for index in 0..latest.len().max(current.len()) {
    let left = latest.get(index).copied().unwrap_or(0);
    let right = current.get(index).copied().unwrap_or(0);
    if left != right {
      return left > right;
    }
  }
  false
}

/// Whether the check should run now.
pub fn is_due(update: &Update, now: i64) -> bool {
  update.last_checked == 0 || now - update.last_checked > interval_seconds(update.check_interval)
}

/// What the last check found, when this start does not check again.
///
/// It still counts, so the notice survives a restart.
pub fn remembered(update: &Update, current: &str) -> UpdateCheckResult {
  let pending = !update.last_version.is_empty()
    && is_newer(&update.last_version, current)
    && update.last_version != update.ignore_version;
  UpdateCheckResult {
    has_update: pending,
    latest_version: pending.then(|| update.last_version.clone()),
  }
}

/// A fresh answer, with a version the user chose to skip taken back out of it.
fn without_ignored(outcome: UpdateCheckResult, ignored: &str) -> UpdateCheckResult {
  if outcome.has_update && !ignored.is_empty() && outcome.latest_version.as_deref() == Some(ignored) {
    UpdateCheckResult::none()
  } else {
    outcome
  }
}

/// Runs, or skips, the release check, and leaves the answer in `result`.
pub(super) fn start(config: Arc<ConfigStore>, result: Arc<Mutex<Option<UpdateCheckResult>>>) {
  let update = config.get().update;
  if !is_due(&update, now_seconds()) {
    *result.lock().unwrap() = Some(remembered(&update, app_version()));
    return;
  }
  std::thread::spawn(move || match check() {
    Ok(outcome) => {
      let mut saved = config.get();
      saved.update.last_checked = now_seconds();
      if let Some(version) = outcome.latest_version.as_ref() {
        saved.update.last_version = version.clone();
      }
      let ignored = saved.update.ignore_version.clone();
      let _ = config.set_quietly(saved);
      *result.lock().unwrap() = Some(without_ignored(outcome, &ignored));
    }
    Err(error) => {
      log::warn!("the release check failed: {error}");
      *result.lock().unwrap() = Some(UpdateCheckResult::none());
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_higher_number_anywhere_is_newer() {
    assert!(is_newer("v0.2.0", "0.1.0"));
    assert!(is_newer("1.0.0", "v0.9.9"));
    assert!(is_newer("0.1.1", "0.1.0"));
    assert!(!is_newer("0.1.0", "0.1.0"));
    assert!(!is_newer("0.1.0", "0.2.0"));
  }

  #[test]
  fn a_missing_segment_counts_as_zero() {
    assert!(is_newer("0.2", "0.1.9"));
    assert!(!is_newer("0.1", "0.1.0"));
    assert!(is_newer("0.1.0.1", "0.1.0"));
  }

  #[test]
  fn a_pre_release_suffix_is_not_newer_than_the_release() {
    assert!(!is_newer("v0.1.0-rc1", "0.1.0"));
    assert!(!is_newer("nonsense", "0.1.0"));
  }

  #[test]
  fn each_interval_is_the_period_it_names() {
    assert_eq!(interval_seconds(UpdateCheckInterval::Daily), 86_400);
    assert_eq!(interval_seconds(UpdateCheckInterval::Weekly), 7 * 86_400);
    assert_eq!(interval_seconds(UpdateCheckInterval::Monthly), 30 * 86_400);
  }

  #[test]
  fn a_check_is_due_first_and_then_once_per_interval() {
    let mut update = Update::default();
    assert!(is_due(&update, 1_000), "a config that never checked is due");

    update.last_checked = 1_000;
    update.check_interval = UpdateCheckInterval::Daily;
    assert!(!is_due(&update, 1_000 + 86_400));
    assert!(is_due(&update, 1_000 + 86_401));
  }

  #[test]
  fn the_last_answer_survives_a_restart_unless_it_was_skipped() {
    let mut update = Update {
      last_version: "9.0.0".to_owned(),
      ..Update::default()
    };
    assert_eq!(
      remembered(&update, "0.1.0"),
      UpdateCheckResult {
        has_update: true,
        latest_version: Some("9.0.0".to_owned()),
      }
    );
    assert_eq!(remembered(&update, "9.0.0"), UpdateCheckResult::none());

    update.ignore_version = "9.0.0".to_owned();
    assert_eq!(remembered(&update, "0.1.0"), UpdateCheckResult::none());
  }

  #[test]
  fn a_skipped_version_is_not_announced_by_a_fresh_check() {
    let found = UpdateCheckResult {
      has_update: true,
      latest_version: Some("0.2.0".to_owned()),
    };
    assert_eq!(without_ignored(found.clone(), "0.2.0"), UpdateCheckResult::none());
    assert_eq!(without_ignored(found.clone(), ""), found);
    assert_eq!(without_ignored(found.clone(), "0.1.9"), found);
  }
}
