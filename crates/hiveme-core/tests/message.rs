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

//! The compatibility rules of `docs/specs/message.md`, one test per rule.
//!
//! Each rule in the specification names the fixture that proves it, and the fixtures
//! live beside this file so that the TypeScript reader can be held to the same ones.

use hiveme_core::message::{self, Level, Message, Parsed, Sender};

fn fixture(name: &str) -> Vec<u8> {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("tests/fixtures/message")
    .join(name);
  std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn envelope(name: &str) -> Message {
  match message::parse(&fixture(name)) {
    Parsed::Envelope(message) => *message,
    other => panic!("{name} parsed as {} rather than an envelope", other.tier()),
  }
}

/// Rule 1: unknown keys at any level are ignored, never rejected.
#[test]
fn unknown_keys_are_ignored() {
  let message = envelope("unknown_fields.json");
  assert_eq!(message.payload.as_ref().unwrap().body, "still readable");
  let sender = message.sender.as_ref().unwrap();
  assert_eq!(sender.name.as_deref(), Some("sams-macbook"));
  assert_eq!(sender.app.as_deref(), Some("hmc"));
}

/// Rule 2: missing optional keys take their documented defaults.
#[test]
fn missing_optional_keys_take_their_defaults() {
  let message = envelope("minimal.json");
  assert_eq!(message.kind, message::DEFAULT_KIND);
  assert!(message.sender.is_none());
  assert!(message.reply_to.is_none());
  assert!(message.ttl_secs.is_none());
  let payload = message.payload.as_ref().unwrap();
  assert_eq!(payload.level(), Level::Info);
  assert!(payload.title.is_none());
  assert!(payload.data.is_none());
  assert!(payload.content_type.is_none());
}

/// A successful result is a recognized level in both readers of the shared fixture.
#[test]
fn success_is_a_known_level_and_survives_serialization() {
  let message = envelope("success.json");
  let level = message.level();
  assert_eq!(level, Level::Success);
  assert!(level.is_known());
  assert!(Level::known().contains(&level));
  assert_eq!(level.displayed(), Level::Success);
  assert_eq!(serde_json::to_value(&message).unwrap()["payload"]["level"], "success");
}

#[test]
fn levels_are_case_insensitive_and_serialize_in_lowercase() {
  for input in ["INFO", "Error", "SuCcEsS", "WARN", "DeBuG", "CUSTOM"] {
    let expected = input.to_lowercase();
    let level: Level = serde_json::from_value(serde_json::json!(input)).unwrap();
    assert_eq!(level.to_string(), expected);
    assert_eq!(serde_json::to_value(&level).unwrap(), expected);
    assert_eq!(level.is_known(), expected != "custom");
  }
  assert_eq!(serde_json::to_value(Level::Other("CUSTOM".into())).unwrap(), "custom");
}

/// Rule 3: an unknown level parses, is kept verbatim, and displays as `info`.
#[test]
fn an_unknown_level_still_parses() {
  let message = envelope("unknown_level.json");
  let level = message.level();
  assert_eq!(level, Level::Other("catastrophe".to_owned()));
  assert!(!level.is_known());
  assert_eq!(level.displayed(), Level::Info);
  assert_eq!(level.as_str(), "catastrophe");
}

/// Rule 3: an unknown algorithm parses to a placeholder that still shows its header.
#[test]
fn an_unknown_algorithm_still_parses() {
  let message = envelope("unknown_alg.json");
  assert!(message.is_encrypted());
  assert!(message.payload.is_none());
  let enc = message.enc.as_ref().unwrap();
  assert_eq!(enc.alg, "XC20P");
  assert_eq!(enc.kid, "k-2027-01");
  assert!(!enc.is_known_algorithm());
  // The sender and the time are still readable, which is what the placeholder shows.
  assert_eq!(message.sender.as_ref().unwrap().label(), "sams-macbook");
  assert!(message.timestamp().is_some());
}

/// Rule 4: a newer envelope version keeps its known fields, is flagged, and is kept.
#[test]
fn a_newer_version_is_flagged_rather_than_dropped() {
  let message = envelope("newer_version.json");
  assert!(message.is_newer_version());
  assert_eq!(message.v, 99);
  assert_eq!(message.kind, "announcement");
  assert_eq!(message.payload.as_ref().unwrap().title.as_deref(), Some("Heads up"));
}

/// Rule 5, tier B: valid JSON that is not an envelope is shown as JSON.
#[test]
fn other_json_falls_back_to_the_json_tier() {
  let parsed = message::parse(&fixture("raw_json.json"));
  assert_eq!(parsed.tier(), "json");
  let Parsed::RawJson(value) = &parsed else {
    panic!("expected the json tier");
  };
  assert_eq!(value["temperature"], 21.5);
  assert!(parsed.notification_body().contains("temperature"));
}

/// Rule 5, tier C: UTF-8 bytes that are not JSON are shown as text.
#[test]
fn other_text_falls_back_to_the_text_tier() {
  let parsed = message::parse(&fixture("raw_text.txt"));
  assert_eq!(parsed.tier(), "text");
  assert_eq!(parsed.notification_body().trim(), "plain text from some other tool");
}

/// Rule 5, tier C: anything else is shown as bytes, and is still not discarded.
#[test]
fn other_bytes_fall_back_to_the_bytes_tier() {
  let bytes = fixture("invalid_utf8.bin");
  let parsed = message::parse(&bytes);
  assert_eq!(parsed.tier(), "bytes");
  let Parsed::RawBytes(kept) = &parsed else {
    panic!("expected the bytes tier");
  };
  assert_eq!(kept, &bytes);
  assert_eq!(parsed.notification_body(), format!("{} bytes", bytes.len()));
}

/// Rule 5: a document that claims to be an envelope but is malformed is still shown.
#[test]
fn a_malformed_envelope_falls_back_rather_than_disappearing() {
  let broken = br#"{"v": 1, "id": "x", "ts": "now", "payload": "not an object"}"#;
  assert_eq!(message::parse(broken).tier(), "json");
}

/// Rule 6: writers always emit the header keys future readers rely on.
#[test]
fn a_written_message_carries_every_header_key() {
  let sender = Sender {
    id: Some("device-1".to_owned()),
    name: Some("laptop".to_owned()),
    app: Some("hmc".to_owned()),
    app_version: Some("0.1.0".to_owned()),
  };
  let message = Message::new_text(sender, "Build finished").with_title("Build");
  let value: serde_json::Value = serde_json::from_slice(&message.to_bytes().unwrap()).unwrap();

  for key in ["v", "id", "ts", "type", "sender", "payload"] {
    assert!(value.get(key).is_some(), "a written message has no {key}");
  }
  // Absent optional keys are left out rather than written as null.
  for key in ["enc", "ciphertext", "replyTo", "ttlSecs"] {
    assert!(value.get(key).is_none(), "a plaintext message should not carry {key}");
  }
  assert_eq!(value["v"], serde_json::json!(message::ENVELOPE_VERSION));
  assert_eq!(value["type"], "message");
}

#[test]
fn a_written_message_is_stamped_with_a_sortable_id_and_an_rfc_3339_time() {
  let first = Message::new_text(Sender::default(), "one");
  let second = Message::new_text(Sender::default(), "two");
  let first_id = uuid::Uuid::parse_str(&first.id).expect("a UUID");
  assert_eq!(first_id.get_version_num(), 7, "ids should be sortable UUID v7");
  assert!(first.id < second.id, "UUID v7 ids sort by time");
  assert!(first.timestamp().is_some());
  assert!(first.ts.ends_with('Z'), "{} should be UTC", first.ts);
  assert_eq!(first.ts.len(), "2026-09-12T09:41:23.512Z".len());
}

#[test]
fn a_round_trip_through_json_preserves_the_message() {
  let message = Message::new_text(Sender::default(), "body")
    .with_title("title")
    .with_level(Level::Warn)
    .with_data(serde_json::json!({ "duration_s": 42 }))
    .with_ttl_secs(60);
  let bytes = message.to_bytes().unwrap();
  assert_eq!(message::parse(&bytes).envelope(), Some(&message));
}

#[test]
fn the_mqtt_properties_describe_the_message() {
  let plain = Message::new_text(Sender::default(), "body");
  let properties = plain.mqtt_properties();
  assert_eq!(properties.content_type, message::CONTENT_TYPE);
  assert_eq!(
    properties.user_properties,
    vec![(message::USER_PROPERTY_VERSION.to_owned(), "1".to_owned())]
  );
  assert_eq!(properties.message_expiry_interval, None);

  let expiring = plain.with_ttl_secs(90);
  assert_eq!(expiring.mqtt_properties().message_expiry_interval, Some(90));

  let encrypted = envelope("unknown_alg.json");
  let properties = encrypted.mqtt_properties();
  assert!(
    properties
      .user_properties
      .contains(&(message::USER_PROPERTY_ENCRYPTION.to_owned(), "XC20P".to_owned()))
  );
}

#[test]
fn the_forbidden_combinations_are_rejected() {
  let mut message = Message::new_text(Sender::default(), "body");
  assert!(message.validate().is_ok());

  message.ciphertext = Some("abc".to_owned());
  assert!(message.validate().is_err(), "ciphertext without enc");

  message.payload = None;
  message.ciphertext = None;
  assert!(message.validate().is_err(), "neither payload nor enc");
}

/// Nothing a broker can deliver may make the reader panic.
#[test]
fn any_bytes_parse_without_panicking() {
  let inputs: Vec<Vec<u8>> = vec![
    b"".to_vec(),
    b"null".to_vec(),
    b"[]".to_vec(),
    b"[1,2,3]".to_vec(),
    b"true".to_vec(),
    b"42".to_vec(),
    br#""a string""#.to_vec(),
    br#"{"v": "one", "payload": {}}"#.to_vec(),
    br#"{"v": -1, "payload": {"body": "x"}}"#.to_vec(),
    br#"{"v": 1}"#.to_vec(),
    br#"{"enc": {}}"#.to_vec(),
    vec![0x00, 0x01, 0x02],
    "{ unbalanced".as_bytes().to_vec(),
    "🐝".as_bytes().to_vec(),
  ];
  for input in inputs {
    let parsed = message::parse(&input);
    // Every tier can describe itself, which is what the GUI renders.
    assert!(!parsed.tier().is_empty());
    let _ = parsed.notification_body();
  }
}

/// The JSON tier truncates a long preview so a notification stays readable.
#[test]
fn a_long_json_preview_is_truncated() {
  let long = serde_json::json!({ "text": "x".repeat(1_000) });
  let parsed = message::parse(long.to_string().as_bytes());
  let body = parsed.notification_body();
  assert!(body.chars().count() <= message::RAW_JSON_PREVIEW_LIMIT + 1);
  assert!(body.ends_with('\u{2026}'));
}

/// A sweep over mutations of every fixture, to make sure no input can crash a reader.
///
/// Truncating and corrupting real payloads reaches the parser paths a fixed list of
/// inputs misses, without pulling in a property testing dependency.
#[test]
fn no_mutation_of_a_real_payload_can_crash_the_reader() {
  let fixtures = [
    "unknown_fields.json",
    "minimal.json",
    "unknown_alg.json",
    "newer_version.json",
    "raw_json.json",
    "raw_text.txt",
    "invalid_utf8.bin",
  ];

  let mut checked = 0usize;
  for name in fixtures {
    let original = fixture(name);
    for cut in 0..original.len() {
      // Every truncation, which is what a half delivered payload looks like.
      let parsed = message::parse(&original[..cut]);
      let _ = parsed.notification_body();
      let _ = parsed.tier();

      // And a single corrupted byte at the same offset.
      let mut corrupted = original.clone();
      corrupted[cut] = corrupted[cut].wrapping_add(0x80);
      let parsed = message::parse(&corrupted);
      let _ = parsed.notification_body();
      let _ = parsed.tier();

      checked += 2;
    }
  }
  assert!(checked > 1_000, "the sweep should be broad, only {checked} inputs");
}
