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

//! Repository automation for HiveMe.
//!
//! `cargo xtask schema`     regenerates the JSON schemas under `schemas/`.
//! `cargo xtask check-spec` validates the examples embedded in `docs/specs/`.
//!
//! Both commands are part of the specification sync mechanism described in
//! `docs/specs/app.md`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A fenced block in a specification file, tagged `json hiveme:<tag>`.
struct Example {
  file: PathBuf,
  line: usize,
  tag: String,
  body: String,
}

fn main() -> ExitCode {
  let mut args = std::env::args().skip(1);
  match args.next().as_deref() {
    Some("schema") => schema(),
    Some("check-spec") => check_spec(),
    None | Some("help" | "--help" | "-h") => {
      usage();
      ExitCode::SUCCESS
    }
    Some(other) => {
      eprintln!("xtask: unknown command '{other}'");
      usage();
      ExitCode::from(2)
    }
  }
}

fn usage() {
  println!("Usage: cargo xtask <command>");
  println!();
  println!("Commands:");
  println!("  schema       Regenerate the JSON schemas under schemas/ from the Rust types");
  println!("  check-spec   Validate the tagged JSON examples in docs/specs/");
  println!("  help         Print this help");
}

/// The repository root, derived from this crate's manifest directory.
fn repo_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .expect("xtask lives one level below the repository root")
    .to_path_buf()
}

/// Maps an example tag to the schema that validates it.
fn schema_for_tag(tag: &str) -> Option<&'static str> {
  match tag {
    "config" => Some("config.schema.json"),
    "message" | "message-encrypted" => Some("message.schema.json"),
    _ => None,
  }
}

fn schema() -> ExitCode {
  // The generators are derived from the `schemars` annotations on the
  // `hiveme-core` config and message types, which arrive in steps 1.1 and 1.2.
  // Until then there is nothing to write, and the freshness check in CI passes
  // because the schemas directory matches what this command would produce.
  println!("xtask schema: no schema generators are registered yet (steps 1.1 and 1.2), nothing written");
  ExitCode::SUCCESS
}

fn check_spec() -> ExitCode {
  let root = repo_root();
  let specs = root.join("docs").join("specs");
  let schemas = root.join("schemas");

  let mut files: Vec<PathBuf> = match std::fs::read_dir(&specs) {
    Ok(entries) => entries
      .filter_map(Result::ok)
      .map(|entry| entry.path())
      .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
      .collect(),
    Err(error) => {
      eprintln!("xtask check-spec: cannot read {}: {error}", specs.display());
      return ExitCode::from(1);
    }
  };
  files.sort();

  let mut examples = Vec::new();
  for file in &files {
    match std::fs::read_to_string(file) {
      Ok(text) => examples.extend(extract_examples(file, &text)),
      Err(error) => {
        eprintln!("xtask check-spec: cannot read {}: {error}", file.display());
        return ExitCode::from(1);
      }
    }
  }

  if examples.is_empty() {
    eprintln!(
      "xtask check-spec: no `json hiveme:<tag>` examples found in {}",
      specs.display()
    );
    eprintln!("  the examples are part of the specification sync mechanism, see docs/specs/app.md");
    return ExitCode::from(1);
  }

  let mut failures = 0usize;
  let mut unvalidated = 0usize;
  for example in &examples {
    let location = format!("{}:{}", relative(&root, &example.file), example.line);
    match serde_json::from_str::<serde_json::Value>(&example.body) {
      Ok(_) => {}
      Err(error) => {
        eprintln!("FAIL {location} [{}]: invalid JSON: {error}", example.tag);
        failures += 1;
        continue;
      }
    }
    match schema_for_tag(&example.tag) {
      None => {
        eprintln!("FAIL {location}: unknown example tag '{}'", example.tag);
        failures += 1;
      }
      Some(schema_file) if schemas.join(schema_file).exists() => {
        // Schema validation is wired up in step 1.4, once the schemas exist.
        println!("OK   {location} [{}] parsed", example.tag);
      }
      Some(schema_file) => {
        println!(
          "OK   {location} [{}] parsed, {schema_file} not generated yet",
          example.tag
        );
        unvalidated += 1;
      }
    }
  }

  println!();
  println!("{} example(s) checked, {failures} failed", examples.len());
  if unvalidated > 0 {
    println!("{unvalidated} example(s) parsed but not schema validated, see step 1.4");
  }

  if failures > 0 {
    ExitCode::from(1)
  } else {
    ExitCode::SUCCESS
  }
}

/// Pulls every ```` ```json hiveme:<tag> ```` block out of one Markdown file.
fn extract_examples(file: &Path, text: &str) -> Vec<Example> {
  const OPEN: &str = "```json hiveme:";
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

fn relative(root: &Path, path: &Path) -> String {
  path.strip_prefix(root).unwrap_or(path).display().to_string()
}
