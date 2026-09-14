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

//! Config version migrations.
//!
//! A migration runs on the raw JSON document, before it is deserialized, so that a
//! renamed or restructured key can be moved without the typed model knowing about the
//! old shape. See `docs/specs/config.md`, "Versioning and compatibility".

use std::path::Path;

use serde_json::Value;

use crate::error::{Error, Result};

/// The config version this build writes.
pub const CONFIG_VERSION: u32 = 1;

/// Steps from one version to the next. Index `n` migrates version `n + 1` to `n + 2`.
///
/// Version 1 is the first shape, so the table is empty until a breaking change lands.
const MIGRATIONS: &[fn(&mut Value)] = &[];

/// Reads the declared version of a raw config document.
///
/// A document without a `version` key is treated as the current version, matching the
/// default in the field table of `docs/specs/config.md`. The caller is warned so that
/// a hand edited file does not silently skip a future migration.
pub fn declared_version(document: &Value, path: &Path) -> Result<u32> {
  match document.get("version") {
    Some(value) => value
      .as_u64()
      .and_then(|v| u32::try_from(v).ok())
      .ok_or_else(|| Error::ConfigMigrate {
        path: path.to_path_buf(),
        reason: "version must be an unsigned 32-bit integer".to_owned(),
      }),
    None => {
      log::warn!("{} has no \"version\" key, assuming {}", path.display(), CONFIG_VERSION);
      Ok(CONFIG_VERSION)
    }
  }
}

/// Brings a raw config document up to [`CONFIG_VERSION`].
///
/// A document from a newer build is left untouched: its known keys are still read, and
/// the caller must not write the file back. A document from an unknown older version is
/// an error, because guessing at its shape would lose data.
pub fn migrate(document: &mut Value, path: &Path) -> Result<MigrationOutcome> {
  let from = declared_version(document, path)?;

  if from > CONFIG_VERSION {
    log::warn!(
      "{} declares config version {from}, newer than the {CONFIG_VERSION} this build writes; \
       reading it on a best effort basis and leaving it alone",
      path.display()
    );
    return Ok(MigrationOutcome::FromTheFuture { declared: from });
  }

  if from == 0 || from as usize > MIGRATIONS.len() + 1 {
    return Err(Error::ConfigMigrate {
      path: path.to_path_buf(),
      reason: format!("config version {from} has no migration path to {CONFIG_VERSION}"),
    });
  }

  if from == CONFIG_VERSION {
    return Ok(MigrationOutcome::AlreadyCurrent);
  }

  for step in &MIGRATIONS[(from - 1) as usize..] {
    step(document);
  }
  if let Some(object) = document.as_object_mut() {
    object.insert("version".to_owned(), Value::from(CONFIG_VERSION));
  }
  Ok(MigrationOutcome::Migrated { from })
}

/// What [`migrate`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
  /// The document already declares the current version.
  AlreadyCurrent,
  /// The document was brought forward from an older version.
  Migrated { from: u32 },
  /// The document is newer than this build and must not be rewritten.
  FromTheFuture { declared: u32 },
}

impl MigrationOutcome {
  /// Whether writing the file back would lose settings this build does not know.
  pub fn is_read_only(&self) -> bool {
    matches!(self, Self::FromTheFuture { .. })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn path() -> &'static Path {
    Path::new("/tmp/HiveMe.json")
  }

  #[test]
  fn a_current_document_is_left_alone() {
    let mut document = serde_json::json!({ "version": CONFIG_VERSION, "device": { "id": "x" } });
    let before = document.clone();
    assert_eq!(
      migrate(&mut document, path()).unwrap(),
      MigrationOutcome::AlreadyCurrent
    );
    assert_eq!(document, before);
  }

  #[test]
  fn a_document_without_a_version_is_assumed_current() {
    let document = serde_json::json!({ "device": { "id": "x" } });
    assert_eq!(declared_version(&document, path()).unwrap(), CONFIG_VERSION);
  }

  #[test]
  fn a_newer_document_is_read_only() {
    let mut document = serde_json::json!({ "version": CONFIG_VERSION + 7 });
    let outcome = migrate(&mut document, path()).unwrap();
    assert_eq!(
      outcome,
      MigrationOutcome::FromTheFuture {
        declared: CONFIG_VERSION + 7
      }
    );
    assert!(outcome.is_read_only());
  }

  #[test]
  fn version_zero_has_no_migration_path() {
    let mut document = serde_json::json!({ "version": 0 });
    assert!(migrate(&mut document, path()).is_err());
  }
}
