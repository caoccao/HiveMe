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

//! Concurrent readers can briefly meet a delete-pending file during Windows atomic
//! replacement. Retry access/sharing conflicts without weakening error reporting.

use std::io;

#[cfg(not(windows))]
pub(super) fn retry<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
  operation()
}

#[cfg(windows)]
pub(super) fn retry<T>(operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
  retry_windows(operation, std::thread::sleep)
}

// Keep the Windows policy testable on every platform, with no real sleeps in tests.
#[cfg(any(windows, test))]
fn retry_windows<T>(
  mut operation: impl FnMut() -> io::Result<T>,
  mut wait: impl FnMut(std::time::Duration),
) -> io::Result<T> {
  for attempt in 0..=8 {
    match operation() {
      // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION.
      Err(error) if matches!(error.raw_os_error(), Some(5 | 32 | 33)) && attempt < 8 => {
        wait(std::time::Duration::from_millis(1 << attempt));
      }
      result => return result,
    }
  }
  unreachable!("the last attempt returns its result")
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  #[test]
  fn a_transient_windows_conflict_retries_the_operation() {
    for code in [5, 32, 33] {
      let mut attempts = 0;
      let mut waits = Vec::new();
      let result = retry_windows(
        || {
          attempts += 1;
          if attempts < 4 {
            Err(io::Error::from_raw_os_error(code))
          } else {
            Ok("complete document")
          }
        },
        |delay| waits.push(delay),
      );
      assert_eq!(result.unwrap(), "complete document");
      assert_eq!(attempts, 4);
      assert_eq!(waits, [1, 2, 4].map(Duration::from_millis));
    }
  }

  #[test]
  fn persistent_windows_conflicts_remain_errors_after_bounded_retries() {
    for code in [5, 32, 33] {
      let mut attempts = 0;
      let mut waited = Duration::ZERO;
      let result: io::Result<()> = retry_windows(
        || {
          attempts += 1;
          Err(io::Error::from_raw_os_error(code))
        },
        |delay| waited += delay,
      );
      assert_eq!(result.unwrap_err().raw_os_error(), Some(code));
      assert_eq!(attempts, 9);
      assert_eq!(waited, Duration::from_millis(255));
    }
  }

  #[test]
  fn missing_files_and_other_io_failures_are_returned_immediately() {
    for code in [2, 3, 13, 112] {
      let result: io::Result<()> = retry_windows(
        || Err(io::Error::from_raw_os_error(code)),
        |_| panic!("unrelated errors must not be retried"),
      );
      assert_eq!(result.unwrap_err().raw_os_error(), Some(code));
    }
    assert_eq!(
      retry_windows(|| Ok(42), |_| panic!("success needs no wait")).unwrap(),
      42
    );
  }

  #[cfg(not(windows))]
  #[test]
  fn other_platforms_do_not_interpret_windows_error_numbers() {
    let mut attempts = 0;
    let result: io::Result<()> = retry(|| {
      attempts += 1;
      Err(io::Error::from_raw_os_error(5))
    });
    assert_eq!(result.unwrap_err().raw_os_error(), Some(5));
    assert_eq!(attempts, 1);
  }

  #[cfg(windows)]
  #[test]
  fn real_windows_read_and_rename_conflicts_recover_when_the_handle_closes() {
    use std::os::windows::fs::OpenOptionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("HiveMe.json");
    std::fs::write(&path, "original").unwrap();
    let lock = || {
      std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap()
    };

    let mut held = Some(lock());
    let loaded = retry_windows(|| std::fs::read_to_string(&path), |_| drop(held.take())).unwrap();
    assert!(held.is_none(), "the initial read met the exclusive handle");
    assert_eq!(loaded, "original");

    let temporary = directory.path().join("replacement.tmp");
    std::fs::write(&temporary, "replacement").unwrap();
    let mut held = Some(lock());
    retry_windows(|| std::fs::rename(&temporary, &path), |_| drop(held.take())).unwrap();
    assert!(held.is_none(), "the initial rename met the exclusive handle");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement");
    assert!(!temporary.exists());
  }
}
