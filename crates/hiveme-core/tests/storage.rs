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

//! The message history of `hmg`, specified in `docs/specs/gui.md`.

use hiveme_core::config::History;
use hiveme_core::message::{Level, Message, Sender};
use hiveme_core::storage::{NewMessage, Store};

/// An envelope from `device`, as `hmg` or `hmc` would publish it.
fn envelope(device: &str, body: &str) -> Message {
  Message::new_text(
    Sender {
      id: Some(device.to_owned()),
      name: Some("sams-laptop".to_owned()),
      app: Some("hmc".to_owned()),
      app_version: Some("0.1.0".to_owned()),
    },
    body,
  )
}

fn incoming(topic: &str, message: &Message) -> NewMessage {
  NewMessage::from_payload(topic, message.to_bytes().unwrap(), 1, false, false)
}

#[test]
fn payload_and_direct_insert_levels_are_stored_in_lowercase() {
  let store = Store::in_memory().unwrap();
  for input in ["INFO", "Error", "SuCcEsS", "WARN", "DeBuG", "CUSTOM"] {
    let mut value = serde_json::to_value(envelope("device-1", input)).unwrap();
    value["payload"]["level"] = serde_json::json!(input);
    let mut message = NewMessage::from_payload("hiveme", serde_json::to_vec(&value).unwrap(), 1, false, false);
    assert_eq!(message.level.as_deref(), Some(input.to_lowercase().as_str()));
    // The insert boundary also handles callers that supply a row directly.
    message.level = Some(input.to_owned());
    let inserted = store.insert(&message).unwrap().message;
    assert_eq!(inserted.level.as_deref(), Some(input.to_lowercase().as_str()));
  }
  for row in store.messages("hiveme", None, 100).unwrap() {
    let level = row.level.unwrap();
    assert_eq!(level, level.to_lowercase());
  }
}

#[test]
fn payload_levels_share_one_topic_and_keep_original_history_paths() {
  let store = Store::in_memory().unwrap();
  for level in [Level::Info, Level::Warn, Level::Error] {
    let message = envelope("device-1", "hello").with_level(level.clone());
    let insertion = store.insert(&incoming("hiveme", &message)).unwrap();
    assert_eq!(insertion.message.level.as_deref(), Some(level.as_str()));
  }
  let topics = store.topics().unwrap();
  assert_eq!(topics.len(), 1);
  assert_eq!(topics[0].topic, "hiveme");
  assert_eq!(store.messages("hiveme", None, 10).unwrap().len(), 3);

  for topic in ["hiveme/info", "hiveme/warn", "hiveme/error", "build/ci"] {
    let message = envelope("device-1", "original path");
    store.insert(&incoming(topic, &message)).unwrap();
    assert_eq!(store.messages(topic, None, 10).unwrap()[0].topic, topic);
  }
  assert_eq!(store.topics().unwrap().len(), 5);
}

#[test]
fn an_envelope_becomes_a_row_that_renders_without_reparsing() {
  let store = Store::in_memory().unwrap();
  let message = envelope("device-1", "Build finished").with_title("CI");

  let insertion = store.insert(&incoming("hiveme/info", &message)).unwrap();

  assert!(insertion.is_new);
  assert!(insertion.topic_is_new);
  let stored = insertion.message;
  assert_eq!(stored.topic, "hiveme/info");
  assert_eq!(stored.msg_id, message.id);
  assert_eq!(stored.ts, message.ts);
  assert_eq!(stored.tier, "envelope");
  assert_eq!(stored.level.as_deref(), Some("info"));
  assert_eq!(stored.title.as_deref(), Some("CI"));
  assert_eq!(stored.body, "Build finished");
  assert_eq!(stored.sender_id.as_deref(), Some("device-1"));
  assert_eq!(stored.sender_name.as_deref(), Some("sams-laptop"));
  assert_eq!(stored.app.as_deref(), Some("hmc"));
  assert!(!stored.outgoing);
  assert_eq!(stored.raw, message.to_bytes().unwrap());
}

#[test]
fn a_payload_hiveme_did_not_shape_is_stored_at_its_own_tier() {
  let store = Store::in_memory().unwrap();

  let json = store
    .insert(&NewMessage::from_payload(
      "sensors/kitchen",
      br#"{"temperature":21}"#.to_vec(),
      0,
      false,
      false,
    ))
    .unwrap()
    .message;
  let text = store
    .insert(&NewMessage::from_payload(
      "sensors/kitchen",
      b"just text".to_vec(),
      0,
      false,
      false,
    ))
    .unwrap()
    .message;
  let bytes = store
    .insert(&NewMessage::from_payload(
      "sensors/kitchen",
      vec![0xff, 0xfe, 0x00],
      0,
      true,
      false,
    ))
    .unwrap()
    .message;

  assert_eq!(json.tier, "json");
  assert_eq!(text.tier, "text");
  assert_eq!(text.body, "just text");
  assert_eq!(bytes.tier, "bytes");
  assert_eq!(bytes.body, "3 bytes");
  assert!(bytes.retain);
  // Nothing outside an envelope carries an identifier, so each one is its own row.
  assert_ne!(json.msg_id, text.msg_id);
  assert_eq!(store.topics().unwrap().len(), 1);
}

#[test]
fn the_broker_echo_of_a_sent_message_collapses_into_the_bubble_that_is_there() {
  let store = Store::in_memory().unwrap();
  let message = envelope("this-device", "Sent from the composer");
  let raw = message.to_bytes().unwrap();

  let sent = store
    .insert(&NewMessage::from_payload("hiveme/info", raw.clone(), 1, false, true))
    .unwrap();
  let echo = store
    .insert(&NewMessage::from_payload("hiveme/info", raw, 1, false, false))
    .unwrap();

  assert!(sent.is_new);
  assert!(!echo.is_new, "the echo must not open a second bubble");
  assert_eq!(sent.message.row_id, echo.message.row_id);
  assert_eq!(store.messages("hiveme/info", None, 100).unwrap().len(), 1);
  assert!(
    store.messages("hiveme/info", None, 100).unwrap()[0].outgoing,
    "the row stays the one this installation sent"
  );
}

#[test]
fn outgoing_options_survive_broker_echoes_in_either_arrival_order() {
  for echo_first in [false, true] {
    let store = Store::in_memory().unwrap();
    let raw = envelope("this-device", "Composer override").to_bytes().unwrap();
    let sent = NewMessage::from_payload("hiveme", raw.clone(), 2, true, true);
    // The subscription has a lower QoS and live delivery does not set retain.
    let echo = NewMessage::from_payload("hiveme", raw, 1, false, false);
    let (first, second) = if echo_first { (&echo, &sent) } else { (&sent, &echo) };
    let initial = store.insert(first).unwrap();
    let merged = store.insert(second).unwrap();
    assert!(initial.is_new);
    assert!(!merged.is_new);
    assert_eq!(initial.message.row_id, merged.message.row_id);
    assert!(merged.message.outgoing);
    assert_eq!(merged.message.qos, 2);
    assert!(merged.message.retain);
    let stored = store.messages("hiveme", None, 10).unwrap();
    assert_eq!(stored.len(), 1);
    assert!(stored[0].outgoing);
    assert_eq!(stored[0].qos, 2);
    assert!(stored[0].retain);
    assert_eq!(store.topics().unwrap()[0].unread, 0);
  }
}

#[test]
fn the_same_message_on_two_topics_is_two_rows() {
  let store = Store::in_memory().unwrap();
  let message = envelope("device-1", "Disk full");
  let raw = message.to_bytes().unwrap();

  store
    .insert(&NewMessage::from_payload("hiveme/warn", raw.clone(), 1, false, false))
    .unwrap();
  store
    .insert(&NewMessage::from_payload("hiveme/error", raw, 1, false, false))
    .unwrap();

  assert_eq!(store.message_count().unwrap(), 2);
  assert_eq!(store.topics().unwrap().len(), 2);
}

#[test]
fn unread_counts_only_what_arrived_and_only_once() {
  let store = Store::in_memory().unwrap();
  let received = envelope("someone-else", "one");
  let sent = envelope("this-device", "two");

  store.insert(&incoming("hiveme/info", &received)).unwrap();
  store
    .insert(&NewMessage::from_payload(
      "hiveme/info",
      sent.to_bytes().unwrap(),
      1,
      false,
      true,
    ))
    .unwrap();
  // The same message again, which is what a reconnect with a retained message does.
  store.insert(&incoming("hiveme/info", &received)).unwrap();

  let topics = store.topics().unwrap();
  assert_eq!(topics.len(), 1);
  assert_eq!(topics[0].unread, 1);
  assert_eq!(topics[0].messages, 2);

  store.mark_read("hiveme/info").unwrap();
  assert_eq!(store.topics().unwrap()[0].unread, 0);
}

#[test]
fn history_is_paged_from_the_newest_end_and_handed_back_in_reading_order() {
  let store = Store::in_memory().unwrap();
  for index in 0..10 {
    store
      .insert(&incoming(
        "hiveme/info",
        &envelope("device-1", &format!("message {index}")),
      ))
      .unwrap();
  }

  let newest = store.messages("hiveme/info", None, 4).unwrap();
  let bodies: Vec<&str> = newest.iter().map(|row| row.body.as_str()).collect();
  assert_eq!(bodies, ["message 6", "message 7", "message 8", "message 9"]);

  let older = store.messages("hiveme/info", Some(newest[0].row_id), 4).unwrap();
  let bodies: Vec<&str> = older.iter().map(|row| row.body.as_str()).collect();
  assert_eq!(bodies, ["message 2", "message 3", "message 4", "message 5"]);

  assert!(store.messages("never/seen", None, 4).unwrap().is_empty());
}

#[test]
fn selecting_a_topic_includes_recursive_children_and_marks_the_same_subtree_read() {
  let store = Store::in_memory().unwrap();
  let topics = [
    "hiveme",
    "hiveme/build",
    "hiveme/build/ci",
    "hiveme/",
    "hiveme//nested",
    "hiveme2",
    "hiveme0",
    "HiveMe/build",
    "elsewhere/hiveme",
    "hiveme-build",
  ];
  for topic in topics {
    store.insert(&incoming(topic, &envelope("device-1", topic))).unwrap();
  }
  let rows = store.messages("hiveme", None, 100).unwrap();
  assert_eq!(
    rows.iter().map(|row| row.topic.as_str()).collect::<Vec<_>>(),
    topics[..5]
  );
  store.mark_read("hiveme").unwrap();
  for row in store.topics().unwrap() {
    assert_eq!(
      row.unread,
      if topics[..5].contains(&row.topic.as_str()) {
        0
      } else {
        1
      },
      "{}",
      row.topic
    );
  }
}

#[test]
fn subtree_queries_treat_wildcards_and_punctuation_as_literal_topic_names() {
  let store = Store::in_memory().unwrap();
  for root in ["fleet_%", "fleet-A", "fleet_[*?]", "fleet-'quote", "设备", "fleet/"] {
    let child = format!("{root}/child");
    for topic in [root, &child, &format!("{root}other/child")] {
      store.insert(&incoming(topic, &envelope("device-1", topic))).unwrap();
    }
    let rows = store.messages(root, None, 100).unwrap();
    assert_eq!(
      rows.iter().map(|row| row.topic.as_str()).collect::<Vec<_>>(),
      [root, child.as_str()]
    );
  }
  assert_eq!(store.messages("fleet_%/child", None, 100).unwrap().len(), 1);
}

#[test]
fn a_parent_without_direct_messages_pages_across_its_children() {
  let store = Store::in_memory().unwrap();
  let message = envelope("device-1", "one envelope on several topics");
  let mut expected = Vec::new();
  for topic in [
    "hiveme/build/one",
    "other",
    "hiveme/build/two",
    "hiveme/build/deep/three",
    "hiveme/building",
  ] {
    let row = store.insert(&incoming(topic, &message)).unwrap().message;
    if topic.starts_with("hiveme/build/") {
      expected.push(row.row_id);
    }
  }
  let newest = store.messages("hiveme/build", None, 2).unwrap();
  assert_eq!(newest.iter().map(|row| row.row_id).collect::<Vec<_>>(), expected[1..]);
  let older = store.messages("hiveme/build", Some(newest[0].row_id), 2).unwrap();
  assert_eq!(older.iter().map(|row| row.row_id).collect::<Vec<_>>(), expected[..1]);
  assert!(
    store
      .messages("hiveme/build", Some(older[0].row_id), 2)
      .unwrap()
      .is_empty()
  );
  assert!(!store.topics().unwrap().iter().any(|row| row.topic == "hiveme/build"));
}

#[test]
fn clearing_an_exact_topic_preserves_children_in_its_recursive_view() {
  let store = Store::in_memory().unwrap();
  for topic in ["hiveme", "hiveme/child", "hiveme/child/deep"] {
    store.insert(&incoming(topic, &envelope("device-1", topic))).unwrap();
  }
  assert_eq!(store.clear_topic("hiveme").unwrap(), 1);
  let rows = store.messages("hiveme", None, 100).unwrap();
  assert_eq!(
    rows.iter().map(|row| row.topic.as_str()).collect::<Vec<_>>(),
    ["hiveme/child", "hiveme/child/deep"]
  );
}

#[test]
fn one_message_can_be_looked_up_by_its_id() {
  let store = Store::in_memory().unwrap();
  let message = envelope("device-1", "Build finished");
  store.insert(&incoming("hiveme/info", &message)).unwrap();

  let found = store.message("hiveme/info", &message.id).unwrap();
  assert_eq!(found.map(|row| row.body), Some("Build finished".to_owned()));
  assert!(store.message("hiveme/info", "no-such-id").unwrap().is_none());
}

#[test]
fn clearing_a_topic_forgets_its_messages_and_keeps_the_node() {
  let store = Store::in_memory().unwrap();
  store
    .insert(&incoming("hiveme/info", &envelope("device-1", "one")))
    .unwrap();
  store
    .insert(&incoming("hiveme/warn", &envelope("device-1", "two")))
    .unwrap();

  assert_eq!(store.clear_topic("hiveme/info").unwrap(), 1);

  assert!(store.messages("hiveme/info", None, 10).unwrap().is_empty());
  let topics = store.topics().unwrap();
  assert_eq!(
    topics.iter().map(|row| row.topic.as_str()).collect::<Vec<_>>(),
    ["hiveme/info", "hiveme/warn"],
    "the tree keeps a topic the user is still subscribed to"
  );
  assert_eq!(topics[0].unread, 0);
  assert_eq!(store.message_count().unwrap(), 1);
}

#[test]
fn pruning_caps_each_topic_separately() {
  let store = Store::in_memory().unwrap();
  for topic in ["hiveme/info", "hiveme/warn"] {
    for index in 0..6 {
      store
        .insert(&incoming(topic, &envelope("device-1", &format!("{topic} {index}"))))
        .unwrap();
    }
  }

  let pruned = store
    .prune(&History {
      max_messages_per_topic: 2,
      retention_days: 0,
    })
    .unwrap();

  assert_eq!(pruned.by_count, 8);
  assert_eq!(pruned.by_age, 0);
  assert_eq!(pruned.total(), 8);
  for topic in ["hiveme/info", "hiveme/warn"] {
    let kept: Vec<String> = store
      .messages(topic, None, 10)
      .unwrap()
      .into_iter()
      .map(|row| row.body)
      .collect();
    assert_eq!(kept, [format!("{topic} 4"), format!("{topic} 5")]);
  }
}

#[test]
fn pruning_drops_what_is_older_than_the_retention_window() {
  let store = Store::in_memory().unwrap();
  let mut old = incoming("hiveme/info", &envelope("device-1", "last month"));
  old.received_ts = "2020-01-01T00:00:00.000Z".to_owned();
  store.insert(&old).unwrap();
  store
    .insert(&incoming("hiveme/info", &envelope("device-1", "today")))
    .unwrap();

  let pruned = store
    .prune(&History {
      max_messages_per_topic: 0,
      retention_days: 30,
    })
    .unwrap();

  assert_eq!(pruned.by_age, 1);
  let kept: Vec<String> = store
    .messages("hiveme/info", None, 10)
    .unwrap()
    .into_iter()
    .map(|row| row.body)
    .collect();
  assert_eq!(kept, ["today"]);
}

#[test]
fn a_history_of_zero_and_zero_keeps_everything() {
  let store = Store::in_memory().unwrap();
  let mut old = incoming("hiveme/info", &envelope("device-1", "ancient"));
  old.received_ts = "2000-01-01T00:00:00.000Z".to_owned();
  store.insert(&old).unwrap();
  store
    .insert(&incoming("hiveme/info", &envelope("device-1", "recent")))
    .unwrap();

  let pruned = store
    .prune(&History {
      max_messages_per_topic: 0,
      retention_days: 0,
    })
    .unwrap();

  assert_eq!(pruned.total(), 0);
  assert_eq!(store.message_count().unwrap(), 2);
}

#[test]
fn history_survives_a_restart() {
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("HiveMe.db");
  let message = envelope("device-1", "Build finished");

  {
    let store = Store::open(&path).unwrap();
    store.insert(&incoming("hiveme/info", &message)).unwrap();
    assert!(store.size_bytes() > 0);
  }

  let reopened = Store::open(&path).unwrap();
  let rows = reopened.messages("hiveme/info", None, 10).unwrap();
  assert_eq!(rows.len(), 1);
  assert_eq!(rows[0].msg_id, message.id);
  assert_eq!(reopened.topics().unwrap()[0].unread, 1);

  // The second open migrates an existing database rather than an empty one, and the
  // echo of the same message must still be recognized across the restart.
  let again = reopened.insert(&incoming("hiveme/info", &message)).unwrap();
  assert!(!again.is_new);
}

#[test]
fn the_database_directory_is_created_on_first_open() {
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("nested").join("HiveMe.db");

  let store = Store::open(&path).unwrap();

  assert!(path.exists());
  assert_eq!(store.path(), path);
  assert_eq!(store.message_count().unwrap(), 0);
  assert!(store.topics().unwrap().is_empty());
}

#[test]
fn two_processes_on_one_database_store_an_envelope_once_and_count_it_once() {
  // hmg and the terminal UI of hmc open the same HiveMe.db and both receive what the
  // broker delivers. Two handles on one file stand in for the two processes.
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("HiveMe.db");
  let gui = Store::open(&path).unwrap();
  let tui = Store::open(&path).unwrap();
  let message = envelope("device-2", "Deployed to staging");

  let first = gui.insert(&incoming("hiveme/deploy", &message)).unwrap();
  let second = tui.insert(&incoming("hiveme/deploy", &message)).unwrap();

  assert!(first.is_new);
  assert!(!second.is_new, "the second process recognizes the envelope");
  assert_eq!(second.message.row_id, first.message.row_id);
  assert_eq!(gui.message_count().unwrap(), 1);
  assert_eq!(tui.topics().unwrap()[0].unread, 1, "the envelope counts as unread once");
}

#[test]
fn two_processes_storing_the_same_messages_at_once_store_each_once() {
  // Both applications receive a message at the same moment. Without a transaction
  // around the lookup and the insert, both could find it missing, and the second insert
  // would fail on the unique key instead of recognizing the row.
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("HiveMe.db");
  let messages: Vec<Message> = (0..200)
    .map(|index| envelope("device-2", &format!("message {index}")))
    .collect();
  let stores = [Store::open(&path).unwrap(), Store::open(&path).unwrap()];

  let new_rows: usize = std::thread::scope(|scope| {
    let workers: Vec<_> = stores
      .iter()
      .map(|store| {
        let messages = &messages;
        scope.spawn(move || {
          messages
            .iter()
            .map(|message| store.insert(&incoming("hiveme/race", message)).unwrap())
            .filter(|insertion| insertion.is_new)
            .count()
        })
      })
      .collect();
    workers.into_iter().map(|worker| worker.join().unwrap()).sum()
  });

  assert_eq!(new_rows, messages.len(), "each message is new to exactly one process");
  assert_eq!(stores[0].message_count().unwrap(), 200);
  assert_eq!(stores[1].topics().unwrap()[0].unread, 200);
}
