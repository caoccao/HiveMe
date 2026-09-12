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
//! Both are part of the specification sync mechanism described in
//! `docs/specs/app.md`. The logic lives in the library beside this file, so the tests
//! and the command line cannot disagree.

use std::process::ExitCode;

use xtask::{relative, repo_root, schema, spec};

fn main() -> ExitCode {
  let mut args = std::env::args().skip(1);
  match args.next().as_deref() {
    Some("schema") => generate_schemas(),
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

fn generate_schemas() -> ExitCode {
  let root = repo_root();
  match schema::write(&root) {
    Ok(written) => {
      for path in written {
        println!("wrote {}", relative(&root, &path));
      }
      ExitCode::SUCCESS
    }
    Err(error) => {
      eprintln!("xtask schema: {error}");
      ExitCode::from(1)
    }
  }
}

fn check_spec() -> ExitCode {
  let root = repo_root();

  let stale = schema::stale(&root);
  for entry in &stale {
    eprintln!("FAIL schemas/{} {}", entry.name, entry.reason);
  }
  if !stale.is_empty() {
    eprintln!("  run `cargo xtask schema` and commit the result");
  }

  let outcomes = match spec::check(&root) {
    Ok(outcomes) => outcomes,
    Err(reason) => {
      eprintln!("xtask check-spec: {reason}");
      return ExitCode::from(1);
    }
  };

  let mut failed = 0usize;
  for outcome in &outcomes {
    let location = format!("{}:{}", relative(&root, &outcome.example.file), outcome.example.line);
    if outcome.is_ok() {
      println!("OK   {location} [{}]", outcome.example.tag);
    } else {
      failed += 1;
      for problem in &outcome.problems {
        eprintln!("FAIL {location} [{}] {problem}", outcome.example.tag);
      }
    }
  }

  println!();
  println!("{} example(s) checked, {failed} failed", outcomes.len());

  if failed > 0 || !stale.is_empty() {
    ExitCode::from(1)
  } else {
    ExitCode::SUCCESS
  }
}
