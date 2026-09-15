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

//! The shared session of `docs/specs/session.md`, against a real broker.
//!
//! A HiveMQ CE container stands in for the cloud, as in `tests/mqtt.rs`. Its allow-all
//! extension accepts any credentials, so the config carries a username and a password
//! anyway: the session validates what it saves and connects with.
//!
//! Every test here needs Docker. Without it, or with `HIVEME_SKIP_DOCKER=1`, each test
//! says why it did nothing and passes, so that the macOS and Windows workflows stay
//! green; the Linux workflow has Docker and does run them.

use std::sync::Arc;
use std::time::{Duration, Instant};

use hiveme_core::config::Theme;
use hiveme_core::message::{Level, Message, Sender};
use hiveme_core::mqtt::{IncomingMessage, Qos, Role};
use hiveme_core::session::{PublishOptions, SHUTDOWN_TIMEOUT, Session, SessionApp, SessionEvent};
use tokio::sync::broadcast::Receiver;

use hiveme_core::test_support::{Broker, RECEIVE_TIMEOUT, Recorder, with_broker};

/// Waits for the first event `wanted` accepts, failing the test rather than hanging.
async fn next_event<T>(events: &mut Receiver<SessionEvent>, mut wanted: impl FnMut(SessionEvent) -> Option<T>) -> T {
  tokio::time::timeout(RECEIVE_TIMEOUT, async {
    loop {
      let event = events.recv().await.expect("the session is still raising events");
      if let Some(found) = wanted(event) {
        return found;
      }
    }
  })
  .await
  .expect("the event arrives within the timeout")
}

/// Every event that arrives within `window`.
async fn events_within(events: &mut Receiver<SessionEvent>, window: Duration) -> Vec<SessionEvent> {
  let mut seen = Vec::new();
  let deadline = tokio::time::Instant::now() + window;
  while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, events.recv()).await {
    seen.push(event);
  }
  seen
}

async fn next_message(incoming: &mut tokio::sync::mpsc::UnboundedReceiver<IncomingMessage>) -> IncomingMessage {
  tokio::time::timeout(RECEIVE_TIMEOUT, incoming.recv())
    .await
    .expect("a message arrives within the timeout")
    .expect("the incoming channel is still open")
}

/// Asks the broker whether `client_id` still has a session.
async fn session_present(broker: &Broker, client_id: &str) -> bool {
  hiveme_core::test_support::session_present(&broker.host, broker.port, client_id).await
}

async fn enable_both_channels(session: &Session) {
  let mut config = session.config();
  config.notifications.enabled = true;
  config.notifications.topmost_enabled = true;
  for rule in &mut config.notifications.rules {
    rule.os = true;
    rule.topmost = true;
  }
  session
    .set_config(config)
    .await
    .expect("both notification channels are enabled");
}

#[test]
fn connecting_subscribes_and_raises_the_status() {
  with_broker("connecting_subscribes_and_raises_the_status", |broker| async move {
    let session = broker.session(SessionApp::Tui, "status", Arc::default());
    let mut events = session.subscribe();

    let status = session.connect().await.expect("the session connects");

    assert_eq!(status.state, "Connected");
    assert_eq!(status.subscriptions, 1);
    assert!(
      status.client_id.starts_with("hiveme-hmc-status-"),
      "{}",
      status.client_id
    );
    let raised = next_event(&mut events, |event| match event {
      SessionEvent::Status(status) if status.state == "Connected" => Some(status),
      _ => None,
    })
    .await;
    assert_eq!(raised.subscriptions, 1);
    assert_eq!(session.status().state, "Connected");

    session.shutdown().await.expect("the session ends");
  });
}

#[test]
fn a_published_envelope_is_stored_raised_once_and_its_echo_is_recognized() {
  with_broker(
    "a_published_envelope_is_stored_raised_once_and_its_echo_is_recognized",
    |broker| async move {
      let recorder = Arc::new(Recorder::default());
      let session = broker.session(SessionApp::Gui, "composer", recorder.clone());
      enable_both_channels(&session).await;
      session.connect().await.expect("the session connects");
      let mut events = session.subscribe();

      let row = session
        .publish(
          "hiveme",
          "Deployed to staging",
          PublishOptions {
            topic: Some("/deploy".to_owned()),
            title: Some("Deploy".to_owned()),
            level: Some("success".to_owned()),
            ..PublishOptions::default()
          },
        )
        .await
        .expect("the broker acknowledges the publish");

      assert_eq!(row.topic, "hiveme/deploy");
      assert!(row.outgoing);
      assert_eq!(row.app.as_deref(), Some("hmg"));
      assert_eq!(row.level.as_deref(), Some("success"));
      assert_eq!(row.title.as_deref(), Some("Deploy"));

      // The echo comes back on the session's own subscription. It collapses into the
      // row that is there and raises nothing, so the only row event is the publish.
      tokio::time::timeout(RECEIVE_TIMEOUT, async {
        while session.status().messages_received == 0 {
          tokio::task::yield_now().await;
        }
      })
      .await
      .expect("the broker echo has been stored");
      let seen = events_within(&mut events, Duration::from_millis(100)).await;
      let rows: Vec<_> = seen
        .iter()
        .filter_map(|event| match event {
          SessionEvent::Message(message) => Some(message.row_id),
          _ => None,
        })
        .collect();
      assert_eq!(rows, vec![row.row_id], "{seen:?}");
      assert!(seen.contains(&SessionEvent::TopicAdded {
        topic: "hiveme/deploy".to_owned()
      }));

      let stored = session.messages("hiveme", None, 0).expect("the history is readable");
      assert_eq!(stored.len(), 1);
      assert_eq!(stored[0].id, row.id);
      let tree = session.topic_tree().expect("the tree is readable");
      assert_eq!(tree[0].unread, 0, "what this installation sent is never unread");
      assert!(session.status().messages_received >= 1, "the echo was received");

      session.shutdown().await.expect("the session ends");
      assert!(recorder.shown().is_empty(), "this session's own publish never notifies");
      assert!(recorder.topmost_shown().is_empty());
    },
  );
}

#[test]
fn a_raw_json_publish_claims_no_envelope() {
  with_broker("a_raw_json_publish_claims_no_envelope", |broker| async move {
    let mut subscriber = broker.client("raw-reader", Role::Gui).await;
    let mut incoming = subscriber.take_incoming().expect("the incoming channel");
    subscriber
      .subscribe(["hiveme/#"], Qos::AtLeastOnce)
      .await
      .expect("the broker accepts the subscription");

    let recorder = Arc::new(Recorder::default());
    let session = broker.session(SessionApp::Tui, "raw-writer", recorder.clone());
    enable_both_channels(&session).await;
    session.connect().await.expect("the session connects");
    let mut events = session.subscribe();
    let body = r#"{"build":482,"ok":true}"#;
    let row = session
      .publish(
        "hiveme/ci",
        body,
        PublishOptions {
          json: true,
          ..PublishOptions::default()
        },
      )
      .await
      .expect("the broker acknowledges the publish");
    assert_eq!(row.tier, "json");
    assert_eq!(row.raw, body);

    let received = next_message(&mut incoming).await;
    assert_eq!(received.topic, "hiveme/ci");
    assert_eq!(received.payload, body.as_bytes());
    assert_eq!(received.properties.content_type.as_deref(), Some("application/json"));
    assert_eq!(received.properties.hiveme_version(), None);
    assert!(
      received
        .properties
        .user_properties
        .contains(&("hiveme-publish-id".to_owned(), row.id.clone()))
    );

    // The session is subscribed to what it just published, so the echo arrives here
    // too. A payload HiveMe did not shape carries no id, so the echo is recognized by
    // its MQTT publish property; without that it is given an id of its own and drawn a second time, as
    // somebody else's message.
    tokio::time::timeout(RECEIVE_TIMEOUT, async {
      while session.status().messages_received == 0 {
        tokio::task::yield_now().await;
      }
    })
    .await
    .expect("the broker echo has been stored");
    let seen = events_within(&mut events, Duration::from_millis(100)).await;
    let rows: Vec<_> = seen
      .iter()
      .filter_map(|event| match event {
        SessionEvent::Message(message) => Some(message.row_id),
        _ => None,
      })
      .collect();
    assert_eq!(rows, vec![row.row_id], "{seen:?}");

    let stored = session.messages("hiveme/ci", None, 0).expect("the history is readable");
    assert_eq!(stored.len(), 1, "the echo is that message, not another one");
    assert!(stored[0].outgoing, "and it is still the one this session sent");
    let tree = session.topic_tree().expect("the tree is readable");
    assert_eq!(tree[0].unread, 0, "what this installation sent is never unread");

    session.shutdown().await.expect("the session ends");
    assert!(
      recorder.shown().is_empty(),
      "this session's own raw publish never notifies"
    );
    assert!(recorder.topmost_shown().is_empty());
    subscriber.disconnect().await.expect("the subscriber says goodbye");
  });
}

#[test]
fn a_saved_theme_keeps_the_connection_and_a_saved_password_replaces_it() {
  with_broker(
    "a_saved_theme_keeps_the_connection_and_a_saved_password_replaces_it",
    |broker| async move {
      // The terminal UI takes a fresh client identifier for every connection, so a
      // changed identifier is a replaced connection.
      let session = broker.session(SessionApp::Tui, "settings", Arc::default());
      let first = session.connect().await.expect("the session connects").client_id;

      let mut config = session.config();
      config.gui.theme = Theme::Rose;
      let saved = session.set_config(config).await.expect("the theme is saved");
      assert_eq!(saved.gui.theme, Theme::Rose);
      assert_eq!(session.status().client_id, first, "a theme keeps the connection");

      let mut config = session.config();
      config.broker.password = "changed".to_owned();
      session.set_config(config).await.expect("the password is saved");
      let status = session.status();
      assert_eq!(status.state, "Connected");
      assert_ne!(status.client_id, first, "a password replaces the connection");

      session.shutdown().await.expect("the session ends");
    },
  );
}

#[test]
fn the_pause_toggle_keeps_the_toaster_quiet_and_the_message_arriving() {
  with_broker(
    "the_pause_toggle_keeps_the_toaster_quiet_and_the_message_arriving",
    |broker| async move {
      let recorder = Arc::new(Recorder::default());
      let session = broker.session(SessionApp::Gui, "watcher", recorder.clone());
      enable_both_channels(&session).await;
      session.connect().await.expect("the session connects");
      let mut events = session.subscribe();
      let publisher = broker.client("elsewhere", Role::Cli).await;
      let config = broker.config("elsewhere");
      let error = |body: &str| {
        Message::new_text(Sender::from_device(&config.device, "hmc"), body)
          .with_title("Disk")
          .with_level(Level::Error)
      };

      session.set_notifications_paused(true);
      publisher
        .publish_message("hiveme/disk", &error("Disk full"), Qos::AtLeastOnce, false)
        .await
        .expect("the broker acknowledges the publish");
      let paused = next_event(&mut events, |event| match event {
        SessionEvent::Message(row) => Some(row),
        _ => None,
      })
      .await;
      assert_eq!(paused.body, "Disk full", "a paused session still stores and shows");
      assert!(!paused.outgoing);
      assert!(recorder.shown().is_empty(), "a paused session raises no toast");
      assert!(
        recorder.topmost_shown().is_empty(),
        "a paused session raises no topmost window"
      );

      session.set_notifications_paused(false);
      let message = error("Disk still full");
      publisher
        .publish_message("hiveme/disk", &message, Qos::AtLeastOnce, false)
        .await
        .expect("the broker acknowledges the publish");
      let fired = next_event(&mut events, |event| match event {
        SessionEvent::NotificationFired {
          rule_id,
          message_id,
          topic,
        } => Some((rule_id, message_id, topic)),
        _ => None,
      })
      .await;
      assert_eq!(
        fired,
        ("error".to_owned(), message.id.clone(), "hiveme/disk".to_owned())
      );
      assert_eq!(
        recorder.shown(),
        vec![("Disk".to_owned(), "Disk still full".to_owned())]
      );

      tokio::time::timeout(RECEIVE_TIMEOUT, async {
        while recorder.shown().is_empty() || recorder.topmost_shown().is_empty() {
          tokio::task::yield_now().await;
        }
      })
      .await
      .unwrap();
      assert_eq!(recorder.topmost_shown(), recorder.shown());
      publisher.disconnect().await.expect("the publisher says goodbye");
      session.shutdown().await.expect("the session ends");
    },
  );
}

#[test]
fn shutdown_ends_the_broker_session_within_the_bound() {
  with_broker(
    "shutdown_ends_the_broker_session_within_the_bound",
    |broker| async move {
      let session = broker.session(SessionApp::Tui, "quitter", Arc::default());
      let client_id = session.connect().await.expect("the session connects").client_id;

      let started = Instant::now();
      session.begin_shutdown();
      tokio::time::timeout(SHUTDOWN_TIMEOUT, session.shutdown())
        .await
        .expect("shutdown finishes within its bound")
        .expect("the session ends cleanly");
      assert!(started.elapsed() < SHUTDOWN_TIMEOUT);

      assert!(
        !session_present(&broker, &client_id).await,
        "quitting discards the broker session"
      );
      let refused = session.connect().await.unwrap_err();
      assert_eq!(refused.to_string(), "the application is quitting");
      let refused = session.publish("hiveme", "late", PublishOptions::default()).await;
      assert!(refused.is_err(), "nothing is published behind a quit");
    },
  );
}

#[test]
fn two_sessions_on_one_database_each_raise_what_arrives_once() {
  with_broker(
    "two_sessions_on_one_database_each_raise_what_arrives_once",
    |broker| async move {
      // hmg and the terminal UI of hmc on one config and one HiveMe.db.
      let gui_toasts = Arc::new(Recorder::default());
      let gui = broker.session(SessionApp::Gui, "shared", gui_toasts.clone());
      enable_both_channels(&gui).await;
      let tui_toasts = Arc::new(Recorder::default());
      let path = broker.directory.path().join("shared").join("HiveMe.json");
      let tui = Session::open(Some(&path), SessionApp::Tui, tui_toasts.clone()).expect("the second session opens");
      gui.connect().await.expect("the first session connects");
      tui.connect().await.expect("the second session connects");
      let mut gui_events = gui.subscribe();
      let mut tui_events = tui.subscribe();
      let rows = |events: Vec<SessionEvent>| -> Vec<String> {
        events
          .into_iter()
          .filter_map(|event| match event {
            SessionEvent::Message(row) => Some(row.body),
            _ => None,
          })
          .collect()
      };

      // From another session: one row, unread once, and in each session one event and
      // one notification, whichever of the two stored it first.
      let publisher = broker.client("elsewhere", Role::Cli).await;
      let config = broker.config("elsewhere");
      let message = Message::new_text(Sender::from_device(&config.device, "hmc"), "Disk full")
        .with_title("Disk")
        .with_level(Level::Error);
      publisher
        .publish_message("hiveme/disk", &message, Qos::AtLeastOnce, false)
        .await
        .expect("the broker acknowledges the publish");
      let (gui_seen, tui_seen) = tokio::join!(
        events_within(&mut gui_events, Duration::from_secs(3)),
        events_within(&mut tui_events, Duration::from_secs(3))
      );
      let arrived = match summaries(gui_seen.clone()).as_slice() {
        [(row_id, false)] => *row_id,
        other => panic!("hmg saw {other:?} rather than one incoming row"),
      };
      assert_eq!(rows(gui_seen), ["Disk full"]);
      assert_eq!(summaries(tui_seen.clone()), [(arrived, false)]);
      assert_eq!(rows(tui_seen), ["Disk full"]);
      assert_eq!(gui.messages("hiveme", None, 0).unwrap().len(), 1);
      assert_eq!(tui.topic_tree().unwrap()[0].unread, 1);
      let shown = vec![("Disk".to_owned(), "Disk full".to_owned())];
      assert_eq!(gui_toasts.shown(), shown);
      assert_eq!(tui_toasts.shown(), shown);
      assert_eq!(gui_toasts.topmost_shown(), shown);
      assert_eq!(tui_toasts.topmost_shown(), shown);

      // Sent from one of them: that one raises its own row only, and its echo is
      // recognized; the other shows it as it arrives, and on the incoming side. The
      // two share a device and a database, but they are two applications, and the
      // message came from the other one.
      let sent_by_gui = gui
        .publish("hiveme", "Deployed", PublishOptions::default())
        .await
        .expect("the broker acknowledges the publish");
      assert!(sent_by_gui.outgoing, "hmg composed it");
      let (gui_seen, tui_seen) = tokio::join!(
        events_within(&mut gui_events, Duration::from_secs(3)),
        events_within(&mut tui_events, Duration::from_secs(3))
      );
      assert_eq!(rows(gui_seen), ["Deployed"]);
      assert_eq!(summaries(tui_seen), [(sent_by_gui.row_id, false)]);
      assert_eq!(gui.messages("hiveme", None, 0).unwrap().len(), 2);
      let deployed = ("hiveme".to_owned(), "Deployed".to_owned());
      assert_eq!(gui_toasts.shown(), shown, "the publishing session stays silent");
      assert_eq!(gui_toasts.topmost_shown(), shown);
      assert_eq!(tui_toasts.shown(), [shown[0].clone(), deployed.clone()]);
      assert_eq!(tui_toasts.topmost_shown(), tui_toasts.shown());

      // And the same question asked the other way around, which is the one the shared
      // `outgoing` column used to get wrong: hmc's message is hmc's own, and hmg reads
      // it as incoming however quickly the echo arrives.
      let sent_by_tui = tui
        .publish("hiveme", "Restarted", PublishOptions::default())
        .await
        .expect("the broker acknowledges the publish");
      assert!(sent_by_tui.outgoing, "hmc composed it");
      let (gui_seen, tui_seen) = tokio::join!(
        events_within(&mut gui_events, Duration::from_secs(3)),
        events_within(&mut tui_events, Duration::from_secs(3))
      );
      assert_eq!(summaries(gui_seen), [(sent_by_tui.row_id, false)]);
      assert_eq!(
        summaries(tui_seen),
        [(sent_by_tui.row_id, true)],
        "hmc raises its own row once, and recognizes the echo"
      );
      let restarted = ("hiveme".to_owned(), "Restarted".to_owned());
      assert_eq!(gui_toasts.shown(), [shown[0].clone(), restarted]);
      assert_eq!(gui_toasts.topmost_shown(), gui_toasts.shown());
      assert_eq!(tui_toasts.shown(), [shown[0].clone(), deployed]);
      assert_eq!(tui_toasts.topmost_shown(), tui_toasts.shown());

      // Read back from the database rather than taken from the live event, because
      // that is what a restart does and the answer has to be the same one.
      let stored = |session: &Session| -> Vec<MessageRowSummary> {
        session
          .messages("hiveme", None, 0)
          .unwrap()
          .into_iter()
          .map(|row| (row.row_id, row.outgoing))
          .collect()
      };
      assert_eq!(
        stored(&gui),
        [
          (arrived, false),
          (sent_by_gui.row_id, true),
          (sent_by_tui.row_id, false)
        ],
        "hmg owns only what hmg sent"
      );
      assert_eq!(
        stored(&tui),
        [
          (arrived, false),
          (sent_by_gui.row_id, false),
          (sent_by_tui.row_id, true)
        ],
        "hmc owns only what hmc sent"
      );

      // Two hmc sessions also share sender.id AND sender.app. Only the actual
      // publishing session stays silent; neither metadata field determines ownership.
      let other_toasts = Arc::new(Recorder::default());
      let other = Session::open(Some(&path), SessionApp::Tui, other_toasts.clone()).unwrap();
      let other_status = other.connect().await.unwrap();
      assert_ne!(other_status.client_id, tui.status().client_id);
      let mut other_events = other.subscribe();
      tui
        .publish(
          "hiveme",
          "I made it.",
          PublishOptions {
            level: Some("success".to_owned()),
            ..Default::default()
          },
        )
        .await
        .unwrap();
      let (gui_seen, tui_seen, other_seen) = tokio::join!(
        events_within(&mut gui_events, Duration::from_secs(3)),
        events_within(&mut tui_events, Duration::from_secs(3)),
        events_within(&mut other_events, Duration::from_secs(3))
      );
      assert_eq!(rows(gui_seen), ["I made it."]);
      assert_eq!(rows(tui_seen), ["I made it."]);
      assert_eq!(rows(other_seen), ["I made it."]);
      let success = ("hiveme".to_owned(), "I made it.".to_owned());
      assert_eq!(gui_toasts.shown().last(), Some(&success));
      assert_eq!(gui_toasts.topmost_shown(), gui_toasts.shown());
      assert_eq!(other_toasts.shown(), [success]);
      assert_eq!(other_toasts.topmost_shown(), other_toasts.shown());
      assert_eq!(tui_toasts.shown().len(), 2, "the publishing hmc session stays silent");
      assert_eq!(tui_toasts.topmost_shown(), tui_toasts.shown());

      // One-shot hmc is another MQTT session even when it uses the same config.
      let cli = broker.client("shared", Role::Cli).await;
      let message =
        Message::new_text(Sender::from_device(&tui.config().device, "hmc"), "From the CLI").with_level(Level::Warn);
      cli
        .publish_message("hiveme", &message, Qos::AtLeastOnce, false)
        .await
        .unwrap();
      let (gui_seen, tui_seen, other_seen) = tokio::join!(
        events_within(&mut gui_events, Duration::from_secs(3)),
        events_within(&mut tui_events, Duration::from_secs(3)),
        events_within(&mut other_events, Duration::from_secs(3))
      );
      assert_eq!(rows(gui_seen), ["From the CLI"]);
      assert_eq!(rows(tui_seen), ["From the CLI"]);
      assert_eq!(rows(other_seen), ["From the CLI"]);
      for recorder in [&gui_toasts, &tui_toasts, &other_toasts] {
        assert_eq!(
          recorder.shown().last(),
          Some(&("hiveme".to_owned(), "From the CLI".to_owned()))
        );
        assert_eq!(recorder.topmost_shown(), recorder.shown());
      }
      cli.disconnect().await.unwrap();
      other.shutdown().await.unwrap();
      publisher.disconnect().await.expect("the publisher says goodbye");
      gui.shutdown().await.expect("the first session ends");
      tui.shutdown().await.expect("the second session ends");
    },
  );
}

#[test]
fn identical_raw_json_from_another_session_is_not_an_own_echo() {
  with_broker(
    "identical_raw_json_from_another_session_is_not_an_own_echo",
    |broker| async move {
      let recorder = Arc::new(Recorder::default());
      let session = broker.session(SessionApp::Tui, "raw-sessions", recorder.clone());
      enable_both_channels(&session).await;
      let mut config = session.config();
      config.topics.subscriptions = vec![hiveme_core::config::Subscription::Relative("elsewhere/#".to_owned())];
      session.set_config(config).await.unwrap();
      session.connect().await.unwrap();

      // There is no subscription to this topic yet, so no own echo consumes the claim.
      let body = r#"{"status":"ok"}"#;
      let sent = session
        .publish(
          "hiveme/ci",
          body,
          PublishOptions {
            json: true,
            ..Default::default()
          },
        )
        .await
        .unwrap();
      assert!(recorder.shown().is_empty());
      assert!(recorder.topmost_shown().is_empty());
      let mut config = session.config();
      config.topics.subscriptions = vec![hiveme_core::config::Subscription::Relative("#".to_owned())];
      session.set_config(config).await.unwrap();
      let mut events = session.subscribe();

      // Identical bytes from another MQTT session must never satisfy that own claim.
      let publisher = broker.client("raw-sessions", Role::Cli).await;
      publisher
        .publish("hiveme/ci", body.as_bytes().to_vec(), Qos::AtLeastOnce, false, None)
        .await
        .unwrap();
      next_event(&mut events, |event| match event {
        SessionEvent::NotificationFired { message_id, .. } if message_id != sent.id => Some(()),
        _ => None,
      })
      .await;
      tokio::time::timeout(RECEIVE_TIMEOUT, async {
        while recorder.shown().is_empty() || recorder.topmost_shown().is_empty() {
          tokio::task::yield_now().await;
        }
      })
      .await
      .unwrap();
      assert_eq!(recorder.shown(), [("hiveme/ci".to_owned(), body.to_owned())]);
      assert_eq!(recorder.topmost_shown(), recorder.shown());
      let rows = session.messages("hiveme/ci", None, 0).unwrap();
      assert_eq!(rows.len(), 2);
      assert_eq!(rows.iter().filter(|row| row.outgoing).count(), 1);
      publisher.disconnect().await.unwrap();
      session.shutdown().await.unwrap();
    },
  );
}

#[test]
fn a_retained_publish_from_a_previous_session_runs_the_rules() {
  with_broker(
    "a_retained_publish_from_a_previous_session_runs_the_rules",
    |broker| async move {
      let original_toasts = Arc::new(Recorder::default());
      let original = broker.session(SessionApp::Tui, "retained", original_toasts.clone());
      enable_both_channels(&original).await;
      original.connect().await.unwrap();
      let sent = original
        .publish(
          "hiveme",
          "Saved by the previous session",
          PublishOptions {
            level: Some("success".to_owned()),
            retain: Some(true),
            ..Default::default()
          },
        )
        .await
        .unwrap();
      original.shutdown().await.unwrap();
      assert!(original_toasts.shown().is_empty());
      assert!(original_toasts.topmost_shown().is_empty());

      let path = broker.directory.path().join("retained").join("HiveMe.json");
      let toasts = Arc::new(Recorder::default());
      let current = Session::open(Some(&path), SessionApp::Tui, toasts.clone()).unwrap();
      let mut events = current.subscribe();
      current.connect().await.unwrap();
      next_event(&mut events, |event| match event {
        SessionEvent::NotificationFired { message_id, .. } if message_id == sent.id => Some(()),
        _ => None,
      })
      .await;
      // Let both ordered notification workers finish; no native UI is opened by this test.
      tokio::time::timeout(RECEIVE_TIMEOUT, async {
        while toasts.shown().is_empty() || toasts.topmost_shown().is_empty() {
          tokio::task::yield_now().await;
        }
      })
      .await
      .unwrap();
      assert_eq!(toasts.shown(), [("hiveme".to_owned(), sent.body)]);
      assert_eq!(toasts.topmost_shown(), toasts.shown());
      assert_eq!(
        current.messages("hiveme", None, 0).unwrap().len(),
        1,
        "history still deduplicates"
      );
      current.shutdown().await.unwrap();
    },
  );
}

/// A row event reduced to its row id and direction.
type MessageRowSummary = (i64, bool);

/// The message events among `events`, each as its row id and direction.
fn summaries(events: Vec<SessionEvent>) -> Vec<MessageRowSummary> {
  events
    .into_iter()
    .filter_map(|event| match event {
      SessionEvent::Message(row) => Some((row.row_id, row.outgoing)),
      _ => None,
    })
    .collect()
}
