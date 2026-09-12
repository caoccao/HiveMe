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

//! What `hmc` tells a script when it did not publish.
//!
//! A script reads the exit code and a person reads the line on stderr, so the two are
//! defined together here rather than derived at the point of failure. The codes are the
//! table in `docs/specs/cli.md`, and every variant of [`hiveme_core::Error`] reaches one
//! of them through [`Failure::from`].

/// A reason `hmc` stopped, in the terms of the exit code table.
#[derive(Debug, thiserror::Error)]
pub enum Failure {
  /// The command line does not say what to publish. Exit code 2.
  #[error("{0}")]
  Usage(String),

  /// The config is missing, unreadable, or incomplete. Exit code 3.
  #[error("{0}")]
  Config(String),

  /// The broker could not be reached, or refused what was asked. Exit code 4.
  #[error("{0}")]
  Connection(String),

  /// The message went out but was never acknowledged. Exit code 5.
  #[error("{0}")]
  PublishTimeout(String),

  /// Anything else. Exit code 1.
  #[error("{0}")]
  Unexpected(String),
}

impl Failure {
  /// The exit code of `docs/specs/cli.md`.
  pub fn code(&self) -> u8 {
    match self {
      Self::Unexpected(_) => 1,
      Self::Usage(_) => 2,
      Self::Config(_) => 3,
      Self::Connection(_) => 4,
      Self::PublishTimeout(_) => 5,
    }
  }

  /// The word between `hmc:` and the detail on stderr.
  pub fn category(&self) -> &'static str {
    match self {
      Self::Unexpected(_) => "error",
      Self::Usage(_) => "usage",
      Self::Config(_) => "config",
      Self::Connection(_) => "connection",
      Self::PublishTimeout(_) => "timeout",
    }
  }

  /// The one line `hmc` writes to stderr.
  ///
  /// A detail that runs over several lines, which a config with several problems does,
  /// is folded onto one so that a log or a shell pipeline keeps one failure per line.
  pub fn line(&self) -> String {
    let detail = self.to_string();
    let folded: Vec<&str> = detail.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    format!("hmc: {}: {}", self.category(), folded.join("; "))
  }
}

impl From<hiveme_core::Error> for Failure {
  /// Sorts a core error into the exit code table.
  ///
  /// The core type already groups its variants by the category a user sees, so this
  /// asks it rather than matching the variants again and drifting from it.
  fn from(error: hiveme_core::Error) -> Self {
    let detail = error.to_string();
    if error.is_config() {
      Self::Config(detail)
    } else if error.is_publish_timeout() {
      Self::PublishTimeout(detail)
    } else if error.is_connection() {
      Self::Connection(detail)
    } else {
      Self::Unexpected(detail)
    }
  }
}

/// The result of every step of a `hmc` run.
pub type Result<T> = std::result::Result<T, Failure>;

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use super::*;
  use hiveme_core::Error;

  #[test]
  fn the_codes_are_the_ones_the_specification_documents() {
    assert_eq!(Failure::Unexpected(String::new()).code(), 1);
    assert_eq!(Failure::Usage(String::new()).code(), 2);
    assert_eq!(Failure::Config(String::new()).code(), 3);
    assert_eq!(Failure::Connection(String::new()).code(), 4);
    assert_eq!(Failure::PublishTimeout(String::new()).code(), 5);
  }

  #[test]
  fn a_missing_config_is_a_config_failure() {
    let failure = Failure::from(Error::ConfigNotFound(PathBuf::from("/nowhere/HiveMe.json")));
    assert_eq!(failure.code(), 3);
    assert_eq!(failure.category(), "config");
  }

  #[test]
  fn an_unreachable_broker_is_a_connection_failure() {
    let failure = Failure::from(Error::Connect("no route to host".to_owned()));
    assert_eq!(failure.code(), 4);
    assert_eq!(
      failure.line(),
      "hmc: connection: cannot connect to the broker: no route to host"
    );
  }

  #[test]
  fn an_unacknowledged_publish_is_its_own_code() {
    let failure = Failure::from(Error::PublishTimeout {
      topic: "hiveme/info".to_owned(),
      secs: 10,
    });
    assert_eq!(failure.code(), 5);
    assert_eq!(failure.category(), "timeout");
  }

  #[test]
  fn an_encryption_mode_this_build_cannot_honour_is_a_config_failure() {
    let failure = Failure::from(Error::NotImplemented("encryption arrives in phase 6".to_owned()));
    assert_eq!(failure.code(), 3);
  }

  #[test]
  fn a_config_with_several_problems_still_reports_one_line() {
    let failure = Failure::from(Error::ConfigInvalid(vec![
      "broker.url is empty".to_owned(),
      "broker.username is empty".to_owned(),
    ]));
    let line = failure.line();
    assert!(!line.trim_end().contains('\n'), "{line}");
    assert!(line.starts_with("hmc: config: "), "{line}");
    assert!(line.contains("broker.url is empty"), "{line}");
    assert!(line.contains("broker.username is empty"), "{line}");
  }
}
