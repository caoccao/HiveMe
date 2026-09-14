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

//! The Settings and About tabs, mirroring `Config.test.tsx` and `About.tsx`.

use hiveme_core::config::{
  DEFAULT_RULE_TITLE, DisplayMode, Scheme, Subscription, Theme as Palette, UpdateCheckInterval,
};
use hiveme_core::message::Level;
use ratatui::style::Color;

use super::super::about::{AUTHOR_URL, Link, WEBSITE_URL};
use super::super::theme::palette;
use super::*;

/// The config `Config.test.tsx` starts from, in `language`.
fn configured(language: &str) -> Config {
  let mut config = Config::default();
  config.device.id = "0f9c2d1e-1111-7000-8000-aaaabbbbcccc".to_owned();
  config.device.name = "sams-laptop".to_owned();
  config.broker.url = "mqtts://abc123.s1.eu.hivemq.cloud:8883".to_owned();
  config.broker.username = "hiveme-sam".to_owned();
  config.broker.password = "s3cret".to_owned();
  config.gui.language = language.to_owned();
  config
}

fn category_index(category: Category) -> usize {
  Category::ALL
    .iter()
    .position(|candidate| *candidate == category)
    .unwrap()
}

/// The Settings tab open on `category`, drawn once at 120 x 40.
fn settings_on(config: Config, category: Category) -> (Arc<Scripted>, App<Scripted>, Receivers) {
  let service = Scripted::new(config);
  let (mut app, receivers) = open_app(&service);
  press(&mut app, key(KeyCode::F(10)));
  press_action(&mut app, Action::SettingsCategory(category_index(category)));
  render(&mut app, 120, 40);
  (service, app, receivers)
}

fn press_action<S: Service>(app: &mut App<S>, action: Action) {
  app.perform(action, Instant::now());
}

/// Clicks a control of the last frame.
fn click_on<S: Service>(app: &mut App<S>, action: Action) {
  let (x, y) = target(app, &action);
  press(app, click(x, y));
}

/// Focuses a text field with a click and empties it.
fn clear<S: Service>(app: &mut App<S>, field: Field) {
  render(app, 120, 40);
  click_on(app, Action::SettingsField(field));
  press(app, key(KeyCode::End));
  press(app, chord(KeyCode::Char('u'), KeyModifiers::CONTROL));
}

fn paste<S: Service>(app: &mut App<S>, text: &str) {
  press(app, Input::Terminal(Event::Paste(text.to_owned())));
}

fn wheel(up: bool, column: u16, row: u16) -> Input {
  Input::Terminal(Event::Mouse(MouseEvent {
    kind: if up {
      MouseEventKind::ScrollUp
    } else {
      MouseEventKind::ScrollDown
    },
    column,
    row,
    modifiers: KeyModifiers::NONE,
  }))
}

#[test]
fn category_navigation_uses_the_requested_order_without_the_gui_editor() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Appearance);
  let order = [
    "Appearance",
    "Broker",
    "History",
    "Notifications",
    "Topics",
    "Update",
    "Advanced",
  ];
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(!screen.contains("Editor"));
  for (index, label) in order.into_iter().enumerate() {
    assert_eq!(t(app.locale, app.settings.category.label_key()), label);
    if index + 1 < order.len() {
      press(&mut app, key(KeyCode::Down));
    }
  }
}

#[test]
fn every_category_renders_in_every_size_and_language() {
  for (tag, locale) in [
    ("en-US", Locale::EnUs),
    ("de", Locale::De),
    ("ja", Locale::Ja),
    ("zh-CN", Locale::ZhCn),
  ] {
    for category in Category::ALL {
      for (width, height) in [(80, 24), (120, 40)] {
        let service = Scripted::new(configured(tag));
        let (mut app, _receivers) = open_app(&service);
        press(&mut app, key(KeyCode::F(10)));
        press_action(&mut app, Action::SettingsCategory(category_index(category)));
        let screen = render(&mut app, width, height).join("\n");
        let context = format!("{tag} {category:?} {width}x{height}:\n{screen}");
        assert!(screen.contains(&t(locale, category.label_key())), "{context}");
        let first = match category {
          Category::Appearance => "settings.mode",
          Category::Broker => "settings.protocol",
          Category::Topics => "settings.subscriptions",
          Category::Notifications => "settings.rules",
          Category::History => "settings.maxMessagesPerTopic",
          Category::Update => "settings.checkInterval",
          Category::Advanced => "settings.encryption",
        };
        let start: String = t(locale, first).chars().take(4).collect();
        assert!(screen.contains(&start), "{start}: {context}");
      }
    }
  }
}

#[test]
fn appearance_opens_first_with_the_mode_the_theme_and_the_language() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Appearance);
  press_action(&mut app, Action::SettingsCategory(0));
  assert_eq!(app.settings.category, Category::Appearance);
  let screen = render(&mut app, 80, 24).join("\n");
  for text in [
    "Mode",
    "(●) Auto Mode",
    "( ) Light Mode",
    "( ) Dark Mode",
    "Theme",
    "[Ocean ▾]",
    "Language",
    "[English (US) ▾]",
  ] {
    assert!(screen.contains(text), "{text}:\n{screen}");
  }
  assert!(!screen.contains("Username"), "one category at a time");
}

#[test]
fn tab_enters_the_panel_from_the_list_and_goes_round_back_to_it() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::History);
  press(&mut app, key(KeyCode::Tab));
  assert_eq!(app.settings.focus, Focus::Field(Field::MaxMessagesPerTopic));
  assert!(app.key_context().typing);
  press(&mut app, key(KeyCode::Tab));
  assert_eq!(app.settings.focus, Focus::Field(Field::RetentionDays));
  press(&mut app, key(KeyCode::Tab));
  assert_eq!(app.settings.focus, Focus::Categories);
  press(&mut app, chord(KeyCode::BackTab, KeyModifiers::SHIFT));
  assert_eq!(app.settings.focus, Focus::Field(Field::RetentionDays));
  press(&mut app, key(KeyCode::Up));
  assert_eq!(app.settings.focus, Focus::Field(Field::MaxMessagesPerTopic));
  press(&mut app, key(KeyCode::Up));
  assert_eq!(app.settings.focus, Focus::Categories);
  press(&mut app, key(KeyCode::Down));
  assert_eq!(
    app.settings.category,
    Category::Notifications,
    "on the list, Down is the next category"
  );
  press(&mut app, key(KeyCode::Tab));
  press(&mut app, key(KeyCode::Esc));
  assert_eq!(app.settings.focus, Focus::Categories, "Esc goes back to the list");

  press_action(&mut app, Action::SettingsCategory(category_index(Category::Advanced)));
  press(&mut app, key(KeyCode::Tab));
  assert_eq!(app.settings.focus, Focus::Categories, "Advanced has nothing to edit");
}

#[test]
fn the_broker_panel_splits_the_url_and_hides_the_password_until_ctrl_h() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Broker);
  assert_eq!(
    app.settings.input(Field::Url).unwrap().text(),
    "abc123.s1.eu.hivemq.cloud:8883"
  );
  let screen = render(&mut app, 120, 40).join("\n");
  for text in [
    "[TLS MQTT ▾]",
    "Connects to mqtts://abc123.s1.eu.hivemq.cloud:8883, on port 8883.",
    "hiveme-sam",
    "******",
    "Ctrl+H  Show the password",
    "TLS needs no settings.",
    "[Copy CLI setup]",
    "It contains the broker password in plain text.",
    "─ Connection ─",
    "Session expiry (s)",
    "3600",
    "─ Reconnect ─",
    "30000",
  ] {
    assert!(screen.contains(text), "{text}:\n{screen}");
  }
  assert!(!screen.contains("s3cret"));

  press(&mut app, chord(KeyCode::Char('h'), KeyModifiers::CONTROL));
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(
    screen.contains("s3cret") && screen.contains("Hide the password"),
    "{screen}"
  );
}

#[tokio::test]
async fn the_console_urls_save_as_they_were_pasted_with_the_protocol_in_front() {
  const HOST: &str = "abc123.s1.eu.hivemq.cloud";
  for (protocol, shown, saved) in [
    (Scheme::Mqtt, HOST.to_owned(), format!("mqtt://{HOST}")),
    (Scheme::Mqtts, format!("{HOST}:8883"), format!("mqtts://{HOST}:8883")),
    (
      Scheme::Wss,
      format!("{HOST}:8884/mqtt"),
      format!("wss://{HOST}:8884/mqtt"),
    ),
  ] {
    let (service, mut app, mut receivers) = settings_on(configured("en-US"), Category::Broker);
    click_on(&mut app, Action::SettingsField(Field::Protocol));
    assert_eq!(app.settings.popup, Some(0), "the popup opens on the protocol in use");
    let index = Scheme::ALL.iter().position(|scheme| *scheme == protocol).unwrap();
    render(&mut app, 120, 40);
    click_on(&mut app, Action::SettingsChoice(Field::Protocol, index));
    assert_eq!(app.settings.popup, None);

    clear(&mut app, Field::Url);
    paste(&mut app, &shown);
    assert_eq!(app.config.broker.url, saved);
    assert_eq!(app.settings.input(Field::Url).unwrap().text(), shown);
    app.on_tick(Instant::now() + SAVE_DELAY);
    settle(&mut app, &mut receivers).await;
    assert_eq!(service.saved.lock().unwrap().last().unwrap().broker.url, saved);
  }
}

#[test]
fn a_scheme_pasted_into_the_box_moves_the_protocol_and_leaves_the_rest() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Broker);
  clear(&mut app, Field::Url);
  paste(&mut app, "mqtt://localhost:1884");
  assert_eq!(app.settings.input(Field::Url).unwrap().text(), "localhost:1884");
  assert_eq!(app.settings.protocol, Scheme::Mqtt);
  assert_eq!(app.config.broker.url, "mqtt://localhost:1884");
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("[MQTT ▾]"), "{screen}");
  assert!(
    screen.contains("Connects to mqtt://localhost:1884, on port 1884."),
    "{screen}"
  );
}

#[test]
fn a_protocol_chosen_before_a_url_is_typed_stays_chosen() {
  let mut config = configured("en-US");
  config.broker.url.clear();
  let (_service, mut app, _receivers) = settings_on(config, Category::Broker);
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("[TLS MQTT ▾]"), "a new install starts on TLS MQTT");
  assert!(screen.contains("The URL from the HiveMQ Cloud console"), "{screen}");

  press(&mut app, key(KeyCode::Tab));
  assert_eq!(app.settings.focus, Focus::Field(Field::Protocol));
  press(&mut app, key(KeyCode::Enter));
  assert_eq!(app.settings.popup, Some(0));
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Down));
  press(&mut app, key(KeyCode::Enter));
  assert_eq!(app.settings.protocol, Scheme::Wss);
  assert_eq!(app.config.broker.url, "", "an empty form saves as empty");
  assert!(render(&mut app, 120, 40).join("\n").contains("[TLS WebSocket ▾]"));

  press(&mut app, key(KeyCode::Tab));
  type_text(&mut app, "host");
  assert_eq!(app.config.broker.url, "wss://host");
}

#[tokio::test]
async fn edits_apply_at_once_and_are_saved_once_after_typing_pauses() {
  let (service, mut app, mut receivers) = settings_on(configured("en-US"), Category::Broker);
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::Username));
  let start = Instant::now();
  app.handle_input(key(KeyCode::Char('-')), start);
  app.handle_input(key(KeyCode::Char('x')), start + Duration::from_millis(300));
  assert_eq!(app.config.broker.username, "hiveme-sam-x", "applied before it is saved");

  app.on_tick(start + Duration::from_millis(799));
  tokio::task::yield_now().await;
  assert!(
    service.saved.lock().unwrap().is_empty(),
    "the last edit was under 500 ms ago"
  );
  app.on_tick(start + Duration::from_millis(800));
  settle(&mut app, &mut receivers).await;
  let saved = service.saved.lock().unwrap().clone();
  assert_eq!(saved.len(), 1, "two edits, one write");
  assert_eq!(saved[0].broker.username, "hiveme-sam-x");

  // Moving the cursor or switching categories edits nothing, so nothing is written.
  press(&mut app, key(KeyCode::Left));
  press_action(&mut app, Action::SettingsCategory(category_index(Category::Update)));
  app.on_tick(start + Duration::from_secs(5));
  tokio::task::yield_now().await;
  assert!(receivers.outcomes.try_recv().is_err());
  assert_eq!(service.saved.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn an_edit_during_a_write_is_saved_right_after_it_with_the_newest_values() {
  let (service, mut app, mut receivers) = settings_on(configured("en-US"), Category::Broker);
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::Username));
  let start = Instant::now();
  app.handle_input(key(KeyCode::Char('a')), start);
  app.on_tick(start + SAVE_DELAY);
  app.handle_input(key(KeyCode::Char('b')), start + Duration::from_millis(600));
  app.on_tick(start + Duration::from_millis(1_100));

  settle(&mut app, &mut receivers).await;
  assert_eq!(service.saved.lock().unwrap().len(), 1, "one write at a time");
  assert_eq!(service.saved.lock().unwrap()[0].broker.username, "hiveme-sama");
  assert_eq!(
    app.config.broker.username, "hiveme-samab",
    "an older answer never replaces a newer edit"
  );

  settle(&mut app, &mut receivers).await;
  let saved = service.saved.lock().unwrap().clone();
  assert_eq!(saved.len(), 2);
  assert_eq!(saved[1].broker.username, "hiveme-samab");
}

#[tokio::test]
async fn a_refused_config_keeps_the_edits_on_screen_and_says_why() {
  let (service, mut app, mut receivers) = settings_on(configured("en-US"), Category::History);
  *service.save_error.lock().unwrap() = Some("gui.history.retentionDays is too long".to_owned());
  clear(&mut app, Field::RetentionDays);
  type_text(&mut app, "99999");
  app.on_tick(Instant::now() + SAVE_DELAY);
  settle(&mut app, &mut receivers).await;

  let snackbar = app.snackbar.clone().expect("the refusal is shown");
  assert!(
    snackbar.error && snackbar.text.contains("is too long"),
    "{}",
    snackbar.text
  );
  assert_eq!(app.config.gui.history.retention_days, 99_999);
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("99999"), "{screen}");
}

#[tokio::test]
async fn copy_cli_setup_saves_the_edits_first_and_copies_the_whole_command() {
  static COPIED: Mutex<Vec<String>> = Mutex::new(Vec::new());
  fn copier(text: &str) -> std::result::Result<(), String> {
    COPIED.lock().unwrap().push(text.to_owned());
    Ok(())
  }

  let (service, app, mut receivers) = settings_on(configured("de"), Category::Broker);
  let mut app = app.with_copier(copier);
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::Password));
  press(&mut app, key(KeyCode::End));
  type_text(&mut app, "-'edited");
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::CopyCliSetup));
  assert!(COPIED.lock().unwrap().is_empty(), "the edits are saved first");

  settle(&mut app, &mut receivers).await;
  assert_eq!(service.saved.lock().unwrap()[0].broker.password, "s3cret-'edited");
  let copied = COPIED.lock().unwrap().clone();
  assert_eq!(copied.len(), 1);
  let command = &copied[0];
  assert!(
    command.starts_with("hmc --init '") && command.ends_with('\''),
    "{command}"
  );
  let argument = &command["hmc --init '".len()..command.len() - 1];
  assert!(!argument.contains('\''), "{command}");
  let setup: serde_json::Value = serde_json::from_str(argument).unwrap();
  assert_eq!(setup["url"], "mqtts://abc123.s1.eu.hivemq.cloud:8883");
  assert_eq!(setup["username"], "hiveme-sam");
  assert_eq!(setup["password"], "s3cret-'edited");
  assert_eq!(setup["language"], "de");
  let snackbar = app.snackbar.clone().unwrap();
  assert!(!snackbar.error);
  assert_eq!(snackbar.text, t(Locale::De, "settings.cliSetupCopied"));
}

#[tokio::test]
async fn a_save_that_fails_copies_no_stale_credentials() {
  static COPIED: Mutex<Vec<String>> = Mutex::new(Vec::new());
  fn copier(text: &str) -> std::result::Result<(), String> {
    COPIED.lock().unwrap().push(text.to_owned());
    Ok(())
  }

  let (service, app, mut receivers) = settings_on(configured("en-US"), Category::Broker);
  let mut app = app.with_copier(copier);
  *service.save_error.lock().unwrap() = Some("Cannot save credentials".to_owned());
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::Password));
  type_text(&mut app, "-edited");
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::CopyCliSetup));
  settle(&mut app, &mut receivers).await;
  assert!(COPIED.lock().unwrap().is_empty());
  assert!(app.snackbar.clone().unwrap().text.contains("Cannot save credentials"));
}

#[test]
fn copy_cli_setup_is_offered_only_while_the_broker_fields_are_usable() {
  let mut config = configured("en-US");
  config.broker.password.clear();
  let (_service, mut app, _receivers) = settings_on(config, Category::Broker);
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("[Copy CLI setup]"));
  assert!(
    !app
      .hits
      .iter()
      .any(|(_, action)| *action == Action::SettingsField(Field::CopyCliSetup)),
    "a disabled button takes no click"
  );
  click_on(&mut app, Action::SettingsField(Field::Password));
  press(&mut app, key(KeyCode::Tab));
  assert_eq!(app.settings.focus, Focus::Field(Field::ClientIdPrefix), "nor the focus");
}

#[tokio::test]
async fn subscriptions_are_written_back_as_strings_or_objects_and_rows_come_and_go() {
  let (service, mut app, mut receivers) = settings_on(configured("en-US"), Category::Topics);
  let screen = render(&mut app, 80, 24).join("\n");
  for text in ["─ Subscriptions ─", "[Add a subscription]", "[ ] Absolute", "[✕]"] {
    assert!(screen.contains(text), "{text}:\n{screen}");
  }

  clear(&mut app, Field::SubscriptionFilter(0));
  assert_eq!(
    app.config.topics.subscriptions,
    vec![Subscription::Relative(String::new())]
  );
  assert!(
    app.settings.input(Field::SubscriptionFilter(0)).is_some(),
    "the row stays"
  );
  type_text(&mut app, "build/#");
  assert_eq!(
    app.config.topics.subscriptions,
    vec![Subscription::Relative("build/#".to_owned())]
  );

  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::SubscriptionAbsolute(0)));
  assert_eq!(
    serde_json::to_value(&app.config.topics.subscriptions).unwrap(),
    serde_json::json!([{ "filter": "build/#", "absolute": true }])
  );

  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::AddSubscription));
  assert_eq!(app.config.topics.subscriptions.len(), 2);
  assert_eq!(app.settings.focus, Focus::Field(Field::SubscriptionFilter(1)));

  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::RemoveSubscription(0)));
  assert_eq!(
    app.config.topics.subscriptions,
    vec![Subscription::Relative("#".to_owned())]
  );
  assert_eq!(app.settings.focus, Focus::Field(Field::RemoveSubscription(0)));
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(!screen.contains("build/#"), "the field follows the row that moved up");

  press(&mut app, key(KeyCode::Enter));
  assert!(app.config.topics.subscriptions.is_empty());
  assert_eq!(app.settings.focus, Focus::Field(Field::AddSubscription));
  app.on_tick(Instant::now() + SAVE_DELAY);
  settle(&mut app, &mut receivers).await;
  let saved = service.saved.lock().unwrap().clone();
  assert_eq!(saved.len(), 1);
  assert!(saved[0].topics.subscriptions.is_empty());
}

#[test]
fn the_switches_the_rules_and_their_levels_change_the_notifications_block() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Notifications);
  let screen = render(&mut app, 120, 40).join("\n");
  for text in [
    "[✓] Raise OS notifications",
    "[ ] Notify about messages this device sent",
    "─ Rules ─",
    "Raw text and JSON use the Info level.",
    "[Add a rule]",
    "Topic filter",
    "Title template",
    "[Info ▾]",
    "[Warn ▾]",
    "[Error ▾]",
  ] {
    assert!(screen.contains(text), "{text}:\n{screen}");
  }

  click_on(&mut app, Action::SettingsField(Field::NotificationsEnabled));
  assert!(!app.config.notifications.enabled);

  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::RuleLevel(0)));
  assert_eq!(app.settings.popup, Some(1), "Info is highlighted");
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("Debug") && screen.contains("Success"), "{screen}");
  click_on(&mut app, Action::SettingsChoice(Field::RuleLevel(0), 2));
  assert_eq!(app.config.notifications.rules[0].level, Level::Success);

  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::RuleEnabled(1)));
  assert!(!app.config.notifications.rules[1].enabled);

  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::AddRule));
  let rules = &app.config.notifications.rules;
  assert_eq!(rules.len(), 4);
  assert_eq!(
    (
      rules[3].id.as_str(),
      rules[3].topic.as_str(),
      &rules[3].level,
      rules[3].enabled
    ),
    ("rule-4", "info", &Level::Info, true)
  );
  assert_eq!(rules[3].title, DEFAULT_RULE_TITLE);
  assert_eq!(app.settings.focus, Focus::Field(Field::RuleId(3)));
  render(&mut app, 120, 40);
  press(&mut app, key(KeyCode::End));
  type_text(&mut app, "x");
  assert_eq!(app.config.notifications.rules[3].id, "rule-4x");

  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::RemoveRule(0)));
  let ids: Vec<&str> = app
    .config
    .notifications
    .rules
    .iter()
    .map(|rule| rule.id.as_str())
    .collect();
  assert_eq!(ids, ["warn", "error", "rule-4x"]);
}

#[test]
fn a_number_field_leaves_the_config_alone_until_it_holds_a_number() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::History);
  clear(&mut app, Field::MaxMessagesPerTopic);
  assert_eq!(app.config.gui.history.max_messages_per_topic, 1_000);
  assert_eq!(app.settings.input(Field::MaxMessagesPerTopic).unwrap().text(), "");
  type_text(&mut app, "250");
  assert_eq!(app.config.gui.history.max_messages_per_topic, 250);
  type_text(&mut app, "x");
  assert_eq!(app.config.gui.history.max_messages_per_topic, 250);
  render(&mut app, 120, 40);
  assert_eq!(
    app.settings.input(Field::MaxMessagesPerTopic).unwrap().text(),
    "250x",
    "a frame does not fill the field again"
  );
  press(&mut app, key(KeyCode::Esc));
  assert!(render(&mut app, 120, 40).join("\n").contains("250x"));
}

#[tokio::test]
async fn a_language_change_re_renders_every_label_in_the_same_frame() {
  let (service, mut app, mut receivers) = settings_on(configured("en-US"), Category::Appearance);
  click_on(&mut app, Action::SettingsField(Field::Language));
  assert_eq!(app.settings.popup, Some(1), "English (US) is highlighted");
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsChoice(Field::Language, 0));
  assert_eq!(app.locale, Locale::De);

  let screen = render(&mut app, 120, 40);
  let text = screen.join("\n");
  let de = |key: &str| t(Locale::De, key);
  assert!(
    screen[1].contains(&format!("[F10 {}]", de("tui.toolbar.settings"))),
    "{}",
    screen[1]
  );
  assert!(screen[3].starts_with(&format!(" {} │ {} ✕", de("tabs.messages"), de("tabs.settings"))));
  for key in [
    "settings.appearance",
    "settings.broker",
    "settings.mode",
    "settings.displayModeAuto",
  ] {
    assert!(text.contains(&de(key)), "{key}:\n{text}");
  }
  assert!(text.contains("[Deutsch ▾]"));
  assert!(screen[39].contains(&de("footer.state.Disconnected")), "{}", screen[39]);
  for english in ["Appearance", "Auto Mode", "Settings"] {
    assert!(!text.contains(english), "{english}:\n{text}");
  }

  app.on_tick(Instant::now() + SAVE_DELAY);
  settle(&mut app, &mut receivers).await;
  assert_eq!(service.saved.lock().unwrap()[0].gui.language, "de");
}

#[test]
fn a_theme_change_recolors_the_selected_topic() {
  let service = Scripted::new(configured("en-US"));
  let (mut app, _receivers) = open_app(&service);
  let selected_color = |app: &mut App<Scripted>| {
    let buffer = render_buffer(app, 120, 40);
    let (x, y) = locate(&buffer, "hiveme", 5..40).expect("the tree shows hiveme");
    assert!(x < 40, "the tree is on the left");
    buffer[(x, y)].fg
  };
  assert_eq!(selected_color(&mut app), palette(Palette::Ocean).0);

  press(&mut app, key(KeyCode::F(10)));
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::Theme));
  let amber = Palette::all()
    .iter()
    .position(|theme| *theme == Palette::Amber)
    .unwrap();
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsChoice(Field::Theme, amber));
  assert_eq!(app.config.gui.theme, Palette::Amber);

  press(&mut app, chord(KeyCode::Char('1'), KeyModifiers::ALT));
  assert_eq!(selected_color(&mut app), palette(Palette::Amber).0);
}

#[test]
fn the_display_mode_moves_with_left_and_right_and_paints_the_screen() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Appearance);
  press(&mut app, key(KeyCode::Tab));
  assert_eq!(app.settings.focus, Focus::Field(Field::Mode));
  assert!(!app.key_context().typing);
  press(&mut app, key(KeyCode::Right));
  assert_eq!(app.config.gui.display_mode, DisplayMode::Light);
  let buffer = render_buffer(&mut app, 80, 24);
  assert_eq!(
    buffer[(79, 12)].bg,
    Color::Rgb(255, 255, 255),
    "the whole screen is painted"
  );
  assert!(buffer_rows(&buffer).join("\n").contains("(●) Light Mode"));

  press(&mut app, key(KeyCode::Right));
  press(&mut app, key(KeyCode::Right));
  assert_eq!(app.config.gui.display_mode, DisplayMode::Auto, "the row wraps around");
  press(&mut app, key(KeyCode::Left));
  assert_eq!(app.config.gui.display_mode, DisplayMode::Dark);
  press(&mut app, key(KeyCode::Char(' ')));
  assert_eq!(app.config.gui.display_mode, DisplayMode::Auto);
  render(&mut app, 80, 24);
  click_on(&mut app, Action::SettingsChoice(Field::Mode, 1));
  assert_eq!(app.config.gui.display_mode, DisplayMode::Light);
}

#[test]
fn a_select_opens_a_popup_whose_choice_is_picked_by_click_or_by_keys() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Update);
  click_on(&mut app, Action::SettingsField(Field::CheckInterval));
  assert_eq!(app.settings.popup, Some(1));
  let screen = render(&mut app, 120, 40).join("\n");
  assert!(screen.contains("Daily") && screen.contains("Monthly"), "{screen}");
  click_on(&mut app, Action::SettingsChoice(Field::CheckInterval, 2));
  assert_eq!(app.config.update.check_interval, UpdateCheckInterval::Monthly);
  assert_eq!(app.settings.popup, None);

  press(&mut app, key(KeyCode::Enter));
  assert_eq!(app.settings.popup, Some(2));
  press(&mut app, key(KeyCode::Up));
  press(&mut app, key(KeyCode::Esc));
  assert_eq!(app.settings.popup, None);
  assert_eq!(
    app.settings.focus,
    Focus::Field(Field::CheckInterval),
    "Esc closes only the popup"
  );
  assert_eq!(app.config.update.check_interval, UpdateCheckInterval::Monthly);

  press(&mut app, key(KeyCode::Char(' ')));
  press(&mut app, key(KeyCode::Up));
  press(&mut app, key(KeyCode::Enter));
  assert_eq!(app.config.update.check_interval, UpdateCheckInterval::Weekly);

  press(&mut app, key(KeyCode::Enter));
  render(&mut app, 120, 40);
  click_on(&mut app, Action::SettingsField(Field::CheckInterval));
  assert_eq!(app.settings.popup, None, "a click on the open select only closes it");
}

#[test]
fn the_advanced_category_keeps_the_designed_sections_visibly_unbuilt() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Advanced);
  let screen = render(&mut app, 120, 40).join("\n");
  assert_eq!(screen.matches("not implemented yet").count(), 2, "{screen}");
  assert!(screen.contains("─ Encryption ─") && screen.contains("─ Cloud API ─"));
}

#[test]
fn the_panel_scrolls_to_the_focused_field_and_with_the_wheel() {
  let (_service, mut app, _receivers) = settings_on(configured("en-US"), Category::Broker);
  let screen = render(&mut app, 80, 24).join("\n");
  assert!(!screen.contains("Longest retry (ms)"), "{screen}");

  press(&mut app, chord(KeyCode::BackTab, KeyModifiers::SHIFT));
  assert_eq!(app.settings.focus, Focus::Field(Field::MaxDelay));
  let screen = render(&mut app, 80, 24).join("\n");
  assert!(screen.contains("Longest retry (ms)"), "{screen}");
  assert!(!screen.contains("Protocol"), "{screen}");
  assert!(app.settings.scroll > 0);

  for _ in 0..10 {
    press(&mut app, wheel(true, 60, 12));
  }
  assert_eq!(app.settings.scroll, 0);
  assert!(render(&mut app, 80, 24).join("\n").contains("Protocol"));
  // In a text field PageDown is the field's; from the list it scrolls the panel.
  press(&mut app, key(KeyCode::Esc));
  press(&mut app, key(KeyCode::PageDown));
  assert!(app.settings.scroll > 0);
}

#[test]
fn the_about_tab_draws_the_name_the_cards_and_the_table_and_opens_the_links() {
  static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());
  fn opener(url: &str) -> std::result::Result<(), String> {
    OPENED.lock().unwrap().push(url.to_owned());
    Ok(())
  }

  let service = Scripted::in_language("en-US");
  let (app, _receivers) = open_app(&service);
  let mut app = app.with_opener(opener);
  press(&mut app, key(KeyCode::F(1)));
  let version = format!("(v{})", hiveme_core::VERSION);
  for (width, height) in [(80, 24), (120, 40)] {
    let screen = render(&mut app, width, height).join("\n");
    for text in [
      "█  █ ▄",
      version.as_str(),
      "One message format, two applications, and a HiveMQ Cloud cluster.",
      "Author",
      "Sam Cao",
      "GitHub",
      "https://github.com/caoccao/HiveMe",
      "Device",
      "sams-macbook (0f9c2d1e-1111-7000-8000-aaaabbbbcccc)",
      "/home/sam/.config/HiveMe/HiveMe.json",
      "/home/sam/.config/HiveMe/HiveMe.db",
      "Apache-2.0",
      "Copyright © 2026 Sam Cao caoccao.com",
    ] {
      assert!(screen.contains(text), "{text} at {width}x{height}:\n{screen}");
    }
  }

  press(&mut app, key(KeyCode::Enter));
  press(&mut app, key(KeyCode::Tab));
  press(&mut app, key(KeyCode::Char(' ')));
  render(&mut app, 120, 40);
  click_on(&mut app, Action::AboutLink(Link::Website));
  assert_eq!(app.about.focus, Link::Website);
  assert_eq!(
    *OPENED.lock().unwrap(),
    vec![
      AUTHOR_URL.to_owned(),
      hiveme_core::session::GITHUB_URL.to_owned(),
      WEBSITE_URL.to_owned()
    ]
  );

  press(&mut app, key(KeyCode::Char('?')));
  let help = render(&mut app, 120, 40).join("\n");
  assert!(help.contains("Open the link"), "{help}");
}

#[test]
fn adding_a_rule_after_deletion_skips_an_id_that_is_still_in_use() {
  let mut config = configured("en-US");
  config.notifications.rules[2].id = "rule-4".to_owned();
  let (_service, mut app, _receivers) = settings_on(config, Category::Notifications);
  press_action(&mut app, Action::SettingsField(Field::AddRule));
  assert_eq!(app.config.notifications.rules.last().unwrap().id, "rule-5");
  let ids: std::collections::HashSet<_> = app.config.notifications.rules.iter().map(|rule| &rule.id).collect();
  assert_eq!(ids.len(), app.config.notifications.rules.len());
}

#[tokio::test]
async fn a_blank_subscription_draft_waits_for_a_valid_filter_before_saving() {
  let (service, mut app, mut receivers) = settings_on(configured("en-US"), Category::Topics);
  clear(&mut app, Field::SubscriptionFilter(0));
  app.on_tick(Instant::now() + SAVE_DELAY);
  tokio::task::yield_now().await;
  assert!(service.saved.lock().unwrap().is_empty());
  type_text(&mut app, "build/#");
  app.on_tick(Instant::now() + SAVE_DELAY);
  let outcome = tokio::time::timeout(Duration::from_secs(1), receivers.outcomes.recv())
    .await
    .unwrap()
    .unwrap();
  app.on_outcome(outcome, Instant::now());
  let saved = service.saved.lock().unwrap();
  assert_eq!(saved.len(), 1);
  assert_eq!(
    saved[0].topics.subscriptions,
    vec![Subscription::Relative("build/#".to_owned())]
  );
}
