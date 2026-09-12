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
//! It runs on its own thread at startup when `update.checkInterval` says it is due,
//! and the result is polled by the frontend. Nothing is downloaded or installed; the
//! user is told there is a newer version and taken to the releases page.

use anyhow::{Result, anyhow};

use crate::constants::{APP_NAME, RELEASES_API_URL};
use crate::protocol::UpdateCheckResult;

/// How long a check interval is, in seconds.
pub fn interval_seconds(interval: hiveme_core::config::UpdateCheckInterval) -> i64 {
  match interval {
    hiveme_core::config::UpdateCheckInterval::Daily => 86_400,
    hiveme_core::config::UpdateCheckInterval::Weekly => 604_800,
    hiveme_core::config::UpdateCheckInterval::Monthly => 2_592_000,
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
  env!("CARGO_PKG_VERSION")
}

/// Asks GitHub for the newest release.
pub fn check() -> Result<UpdateCheckResult> {
  let current = app_version();
  log::info!("checking for a newer release than {current}");
  let response = ureq::get(RELEASES_API_URL)
    .set("User-Agent", APP_NAME)
    .call()
    .map_err(|error| anyhow!("the releases could not be fetched: {error}"))?;
  let releases: serde_json::Value = response
    .into_json()
    .map_err(|error| anyhow!("the releases could not be read: {error}"))?;
  let Some(latest) = releases.as_array().and_then(|releases| releases.first()) else {
    return Ok(UpdateCheckResult {
      has_update: false,
      latest_version: None,
    });
  };
  let tag = latest["tag_name"].as_str().unwrap_or_default();
  log::info!("the newest release is tagged {tag}");
  if is_newer(tag, current) {
    return Ok(UpdateCheckResult {
      has_update: true,
      latest_version: Some(tag.trim_start_matches('v').to_owned()),
    });
  }
  Ok(UpdateCheckResult {
    has_update: false,
    latest_version: None,
  })
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
    use hiveme_core::config::UpdateCheckInterval;

    assert_eq!(interval_seconds(UpdateCheckInterval::Daily), 86_400);
    assert_eq!(interval_seconds(UpdateCheckInterval::Weekly), 7 * 86_400);
    assert_eq!(interval_seconds(UpdateCheckInterval::Monthly), 30 * 86_400);
  }
}
