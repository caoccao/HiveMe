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

//! Generating and checking `schemas/`.
//!
//! The schemas are derived from the `hiveme-core` types, so the Rust model is the one
//! definition of the config and the message format. Editing the JSON by hand would
//! put the two out of step, which is why CI compares them on every run.

use std::path::{Path, PathBuf};

/// The directory holding the generated schemas.
pub const SCHEMA_DIRECTORY: &str = "schemas";

/// Every schema this build generates, as a file name and its content.
pub fn generated() -> Vec<(&'static str, serde_json::Value)> {
  hiveme_core::json_schemas()
}

/// The exact bytes a schema file should contain.
pub fn text(schema: &serde_json::Value) -> String {
  let mut text = serde_json::to_string_pretty(schema).expect("a schema serialises");
  text.push('\n');
  text
}

/// Writes every schema, returning the files it touched.
pub fn write(root: &Path) -> std::io::Result<Vec<PathBuf>> {
  let directory = root.join(SCHEMA_DIRECTORY);
  std::fs::create_dir_all(&directory)?;
  let mut written = Vec::new();
  for (name, schema) in generated() {
    let path = directory.join(name);
    std::fs::write(&path, text(&schema))?;
    written.push(path);
  }
  Ok(written)
}

/// A committed schema that does not match what this build would generate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
  pub name: &'static str,
  pub reason: String,
}

/// Compares the committed schemas with freshly generated ones.
pub fn stale(root: &Path) -> Vec<Stale> {
  let directory = root.join(SCHEMA_DIRECTORY);
  let mut stale = Vec::new();
  for (name, schema) in generated() {
    let path = directory.join(name);
    let expected = text(&schema);
    match std::fs::read_to_string(&path) {
      Ok(found) if found == expected => {}
      Ok(_) => stale.push(Stale {
        name,
        reason: "differs from what the Rust types generate".to_owned(),
      }),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => stale.push(Stale {
        name,
        reason: "is missing".to_owned(),
      }),
      Err(error) => stale.push(Stale {
        name,
        reason: format!("cannot be read: {error}"),
      }),
    }
  }
  stale
}

/// Reads a committed schema, for validating the examples against it.
pub fn load(root: &Path, name: &str) -> Result<serde_json::Value, String> {
  let path = root.join(SCHEMA_DIRECTORY).join(name);
  let text = std::fs::read_to_string(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
  serde_json::from_str(&text).map_err(|error| format!("{} is not valid JSON: {error}", path.display()))
}
