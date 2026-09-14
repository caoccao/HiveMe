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

//! The specification sync mechanism described in `docs/specs/app.md`.
//!
//! These run under `cargo test`, and CI also calls `cargo xtask check-spec`, so the
//! same checks fire whichever way the repository is built.

use std::path::Path;

use xtask::{relative, repo_root, schema, spec};

/// The committed schemas match what the Rust types generate.
#[test]
fn schemas_are_current() {
  let root = repo_root();
  let stale = schema::stale(&root);
  assert!(
    stale.is_empty(),
    "run `cargo xtask schema` and commit the result: {}",
    stale
      .iter()
      .map(|entry| format!("schemas/{} {}", entry.name, entry.reason))
      .collect::<Vec<_>>()
      .join(", ")
  );
}

/// Every tagged example in the specifications matches its schema and the Rust types.
#[test]
fn spec_examples_are_valid() {
  let root = repo_root();
  let outcomes = spec::check(&root).expect("the specifications can be read");

  let failures: Vec<String> = outcomes
    .iter()
    .filter(|outcome| !outcome.is_ok())
    .map(|outcome| {
      format!(
        "{}:{} [{}] {}",
        relative(&root, &outcome.example.file),
        outcome.example.line,
        outcome.example.tag,
        outcome.problems.join("; ")
      )
    })
    .collect();

  assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every specification that should carry an example still does.
#[test]
fn the_specifications_still_carry_their_examples() {
  let root = repo_root();
  let examples = spec::examples(&root).expect("the specifications can be read");
  let tags: std::collections::BTreeSet<&str> = examples.iter().map(|example| example.tag.as_str()).collect();
  for expected in ["broker-init", "config", "message", "message-encrypted"] {
    assert!(tags.contains(expected), "the '{expected}' example has gone missing");
  }
}

/// A schema file that drifts from the types is reported rather than ignored.
#[test]
fn a_stale_schema_is_detected() {
  let scratch = tempfile::tempdir().expect("a temporary directory");
  let root = scratch.path();
  schema::write(root).expect("the schemas write");
  assert!(schema::stale(root).is_empty(), "a freshly written set is current");

  // Drop a field, the way a hand edit or a forgotten regeneration would.
  let path = root.join(schema::SCHEMA_DIRECTORY).join("config.schema.json");
  let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  value["properties"]
    .as_object_mut()
    .unwrap()
    .remove("topics")
    .expect("the config schema describes topics");
  std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

  let stale = schema::stale(root);
  assert_eq!(stale.len(), 1, "{stale:?}");
  assert_eq!(stale[0].name, "config.schema.json");

  std::fs::remove_file(&path).unwrap();
  assert!(schema::stale(root).iter().any(|entry| entry.reason.contains("missing")));
}

/// An example that no longer matches the schema fails the check.
#[test]
fn a_broken_example_is_caught() {
  let root = repo_root();
  let cases: &[(&str, &str, &str)] = &[
    ("invalid JSON", "config", "{ \"version\": 1, }"),
    ("an unknown tag", "bogus", "{}"),
    (
      "a config with the wrong type for a field",
      "config",
      r#"{ "version": "one" }"#,
    ),
    (
      "a message missing a required header field",
      "message",
      r#"{ "v": 1, "payload": { "body": "no id or ts" } }"#,
    ),
    (
      "a message carrying both a payload and an encryption header",
      "message",
      r#"{
        "v": 1,
        "id": "018f6b1e-7c2a-7d3e-9f1a-4b2c8d9e0f11",
        "ts": "2026-09-12T09:41:23.512Z",
        "payload": { "body": "plain" },
        "enc": { "alg": "A256GCM", "kid": "k", "iv": "iv" },
        "ciphertext": "abc"
      }"#,
    ),
    (
      "an encrypted message with no ciphertext",
      "message-encrypted",
      r#"{
        "v": 1,
        "id": "018f6b1e-7c2a-7d3e-9f1a-4b2c8d9e0f11",
        "ts": "2026-09-12T09:41:23.512Z",
        "enc": { "alg": "A256GCM", "kid": "k", "iv": "iv" }
      }"#,
    ),
  ];

  for (what, tag, body) in cases {
    let example = spec::Example {
      file: Path::new("docs/specs/made-up.md").to_path_buf(),
      line: 1,
      tag: (*tag).to_owned(),
      body: (*body).to_owned(),
    };
    let outcome = spec::check_example(&root, &example);
    assert!(!outcome.is_ok(), "{what} should have been caught");
  }
}

/// The good examples in the specifications really do pass the same checker.
#[test]
fn a_good_example_passes() {
  let root = repo_root();
  let example = spec::Example {
    file: Path::new("docs/specs/made-up.md").to_path_buf(),
    line: 1,
    tag: "message".to_owned(),
    body: r#"{
      "v": 1,
      "id": "018f6b1e-7c2a-7d3e-9f1a-4b2c8d9e0f11",
      "ts": "2026-09-12T09:41:23.512Z",
      "payload": { "body": "hello" }
    }"#
      .to_owned(),
  };
  let outcome = spec::check_example(&root, &example);
  assert!(outcome.is_ok(), "{:?}", outcome.problems);
}

/// The extractor finds a tagged block and leaves everything else alone.
#[test]
fn only_tagged_blocks_are_extracted() {
  let markdown = "# Title\n\n```json\n{ \"untagged\": true }\n```\n\n```json hiveme:config\n{ \"version\": 1 }\n```\n\n```text hiveme:help\nnot json\n```\n";
  let examples = spec::extract_from(Path::new("made-up.md"), markdown);
  assert_eq!(examples.len(), 1);
  assert_eq!(examples[0].tag, "config");
  assert_eq!(examples[0].line, 7);
  assert_eq!(examples[0].body.trim(), "{ \"version\": 1 }");
}

/// What the builder writes matches the schema the builder's own types generate.
///
/// `hiveme-core` deliberately has no JSON Schema dependency, so this check lives here,
/// where the validator is already available.
#[test]
fn a_built_message_matches_the_schema() {
  let root = repo_root();
  let schema = schema::load(&root, "message.schema.json").expect("the message schema is committed");
  let validator = jsonschema::validator_for(&schema).expect("the message schema is usable");

  let sender = hiveme_core::message::Sender {
    id: Some("0f7a1c2e-5d4b-4a6e-9c1d-2b3e4f5a6b7c".to_owned()),
    name: Some("sams-macbook".to_owned()),
    app: Some("hmc".to_owned()),
    app_version: Some("0.1.0".to_owned()),
  };

  let messages = vec![
    (
      "the plainest message",
      hiveme_core::message::Message::new_text(sender.clone(), "hello"),
    ),
    (
      "every optional field set",
      hiveme_core::message::Message::new_text(sender.clone(), "body")
        .with_title("title")
        .with_level(hiveme_core::message::Level::Error)
        .with_data(serde_json::json!({ "duration_s": 42 }))
        .with_ttl_secs(60),
    ),
    (
      "a message with no sender",
      hiveme_core::message::Message::new_text(hiveme_core::message::Sender::default(), ""),
    ),
    (
      "a level this build does not know",
      hiveme_core::message::Message::new_text(sender, "body")
        .with_level(hiveme_core::message::Level::Other("catastrophe".to_owned())),
    ),
  ];

  for (what, message) in messages {
    let value: serde_json::Value = serde_json::from_slice(&message.to_bytes().unwrap()).unwrap();
    let problems: Vec<String> = validator
      .iter_errors(&value)
      .map(|error| format!("{} at {}", error, error.instance_path()))
      .collect();
    assert!(problems.is_empty(), "{what}: {}", problems.join("; "));
  }
}

/// A config the applications write matches the committed config schema.
#[test]
fn a_written_config_matches_the_schema() {
  let root = repo_root();
  let schema = schema::load(&root, "config.schema.json").expect("the config schema is committed");
  let validator = jsonschema::validator_for(&schema).expect("the config schema is usable");

  let value = serde_json::to_value(hiveme_core::config::Config::new_for_this_device()).unwrap();
  let problems: Vec<String> = validator
    .iter_errors(&value)
    .map(|error| format!("{} at {}", error, error.instance_path()))
    .collect();
  assert!(problems.is_empty(), "{}", problems.join("; "));
}
