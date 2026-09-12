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

//! Checking the examples embedded in `docs/specs/`.
//!
//! A fenced block tagged `json hiveme:<tag>` is pulled out, parsed, validated against
//! the schema for its tag, and round tripped through the Rust types. An example that
//! drifts from the code therefore fails CI rather than quietly misleading a reader.

use std::path::{Path, PathBuf};

use crate::schema;

/// The directory holding the specification files.
pub const SPEC_DIRECTORY: &str = "docs/specs";

/// The fence that opens a checked example.
const OPEN: &str = "```json hiveme:";

/// One checked example.
#[derive(Debug, Clone)]
pub struct Example {
  pub file: PathBuf,
  /// The line the opening fence is on, counting from one.
  pub line: usize,
  pub tag: String,
  pub body: String,
}

/// The schema that validates a tag.
pub fn schema_for_tag(tag: &str) -> Option<&'static str> {
  match tag {
    "config" => Some("config.schema.json"),
    "message" | "message-encrypted" => Some("message.schema.json"),
    _ => None,
  }
}

/// Pulls every tagged block out of one Markdown document.
pub fn extract_from(file: &Path, text: &str) -> Vec<Example> {
  let mut examples = Vec::new();
  let mut lines = text.lines().enumerate();
  while let Some((index, line)) = lines.next() {
    let Some(tag) = line.strip_prefix(OPEN) else {
      continue;
    };
    let tag = tag.trim().to_owned();
    let mut body = String::new();
    for (_, line) in lines.by_ref() {
      if line.trim_end() == "```" {
        break;
      }
      body.push_str(line);
      body.push('\n');
    }
    examples.push(Example {
      file: file.to_path_buf(),
      line: index + 1,
      tag,
      body,
    });
  }
  examples
}

/// Every tagged block in `docs/specs`, in a stable order.
pub fn examples(root: &Path) -> Result<Vec<Example>, String> {
  let directory = root.join(SPEC_DIRECTORY);
  let entries =
    std::fs::read_dir(&directory).map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
  let mut files: Vec<PathBuf> = entries
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
    .collect();
  files.sort();

  let mut examples = Vec::new();
  for file in &files {
    let text = std::fs::read_to_string(file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    examples.extend(extract_from(file, &text));
  }
  Ok(examples)
}

/// What checking one example produced.
#[derive(Debug, Clone)]
pub struct Outcome {
  pub example: Example,
  /// Empty when the example is good.
  pub problems: Vec<String>,
}

impl Outcome {
  pub fn is_ok(&self) -> bool {
    self.problems.is_empty()
  }
}

/// Checks one example against its schema and the Rust types.
pub fn check_example(root: &Path, example: &Example) -> Outcome {
  let mut problems = Vec::new();

  let value = match serde_json::from_str::<serde_json::Value>(&example.body) {
    Ok(value) => value,
    Err(error) => {
      return Outcome {
        example: example.clone(),
        problems: vec![format!("is not valid JSON: {error}")],
      };
    }
  };

  match schema_for_tag(&example.tag) {
    None => problems.push(format!("has the unknown tag '{}'", example.tag)),
    Some(name) => match schema::load(root, name) {
      Err(reason) => problems.push(reason),
      Ok(schema) => match jsonschema::validator_for(&schema) {
        Err(error) => problems.push(format!("{name} is not a usable schema: {error}")),
        Ok(validator) => {
          for error in validator.iter_errors(&value) {
            problems.push(format!("does not match {name} at {}: {error}", error.instance_path()));
          }
        }
      },
    },
  }

  if let Err(reason) = round_trip(&example.tag, &value) {
    problems.push(reason);
  }

  Outcome {
    example: example.clone(),
    problems,
  }
}

/// Reads the example into the Rust type and writes it back, to prove the model agrees.
///
/// The comparison is on the reparsed value rather than on the text, because the types
/// legitimately fill in defaults that a documented example leaves out.
fn round_trip(tag: &str, value: &serde_json::Value) -> Result<(), String> {
  fn compare<T>(value: &serde_json::Value, what: &str) -> Result<(), String>
  where
    T: serde::de::DeserializeOwned + serde::Serialize,
  {
    let parsed: T =
      serde_json::from_value(value.clone()).map_err(|error| format!("does not read as a {what}: {error}"))?;
    let written = serde_json::to_value(&parsed).map_err(|error| format!("does not write back as a {what}: {error}"))?;
    let reparsed: T =
      serde_json::from_value(written.clone()).map_err(|error| format!("does not round trip as a {what}: {error}"))?;
    let rewritten =
      serde_json::to_value(&reparsed).map_err(|error| format!("does not round trip as a {what}: {error}"))?;
    if written == rewritten {
      Ok(())
    } else {
      Err(format!("is not stable when round tripped as a {what}"))
    }
  }

  match tag {
    "config" => compare::<hiveme_core::config::Config>(value, "config"),
    "message" | "message-encrypted" => compare::<hiveme_core::message::Message>(value, "message"),
    _ => Ok(()),
  }
}

/// Checks every example under `docs/specs`.
pub fn check(root: &Path) -> Result<Vec<Outcome>, String> {
  let examples = examples(root)?;
  if examples.is_empty() {
    return Err(format!(
      "no `{OPEN}<tag>` examples found under {}; they are part of the specification sync \
       mechanism, see docs/specs/app.md",
      root.join(SPEC_DIRECTORY).display()
    ));
  }
  Ok(examples.iter().map(|example| check_example(root, example)).collect())
}
