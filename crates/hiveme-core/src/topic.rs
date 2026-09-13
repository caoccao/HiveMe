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

//! Topic names, topic filters, and the prefix.
//!
//! The rules are specified in `docs/specs/config.md`. Matching follows section 4.7 of
//! the MQTT 5 specification, including the rule that a wildcard filter never matches a
//! topic that starts with `$`.

/// The longest topic MQTT allows, in bytes of UTF-8.
pub const MAX_TOPIC_BYTES: usize = 65_535;

/// The application's fixed MQTT root. It is not configurable.
pub const ROOT_TOPIC: &str = "hiveme";

/// Resolves a publish topic relative to its base, removing leading slashes from input.
///
/// The GUI uses its selected topic as the base; the CLI uses `ROOT_TOPIC`.
/// Existing MQTT paths in the base and empty levels inside the input stay intact.
pub fn resolve_publish(base: &str, topic: &str) -> String {
  let topic = topic.trim_start_matches('/');
  if topic.is_empty() {
    return base.to_owned();
  }
  if base.is_empty() {
    return topic.to_owned();
  }
  format!("{base}/{topic}")
}

/// The absolute topic that `topic` names under `prefix`.
///
/// An absolute topic, or an empty prefix, is returned unchanged.
pub fn resolve(prefix: &str, topic: &str, absolute: bool) -> String {
  let prefix = prefix.trim_matches('/');
  if absolute || prefix.is_empty() {
    return topic.to_owned();
  }
  if topic.is_empty() {
    return prefix.to_owned();
  }
  format!("{prefix}/{topic}")
}

/// Rejects a topic that cannot be published to.
pub fn validate_topic(topic: &str) -> Result<(), String> {
  if topic.is_empty() {
    return Err("a topic must not be empty".to_owned());
  }
  if topic.contains('+') || topic.contains('#') {
    return Err(format!(
      "'{topic}' contains a wildcard, which a published topic must not"
    ));
  }
  reject_control_characters(topic)?;
  reject_oversize(topic)
}

/// Rejects a filter that could not be subscribed to.
pub fn validate_filter(filter: &str) -> Result<(), String> {
  if filter.is_empty() {
    return Err("a topic filter must not be empty".to_owned());
  }
  reject_control_characters(filter)?;
  reject_oversize(filter)?;

  let levels: Vec<&str> = filter.split('/').collect();
  for (index, level) in levels.iter().enumerate() {
    if level.contains('#') {
      if *level != "#" {
        return Err(format!("'{filter}': '#' must take up a whole level"));
      }
      if index + 1 != levels.len() {
        return Err(format!("'{filter}': '#' must be the last level"));
      }
    }
    if level.contains('+') && *level != "+" {
      return Err(format!("'{filter}': '+' must take up a whole level"));
    }
  }
  Ok(())
}

fn reject_control_characters(value: &str) -> Result<(), String> {
  if value.contains('\0') {
    return Err("a topic must not contain a NUL byte".to_owned());
  }
  if value.chars().any(|character| character.is_control()) {
    return Err("a topic must not contain control characters".to_owned());
  }
  Ok(())
}

fn reject_oversize(value: &str) -> Result<(), String> {
  if value.len() > MAX_TOPIC_BYTES {
    return Err(format!(
      "a topic must be at most {MAX_TOPIC_BYTES} bytes, this one is {}",
      value.len()
    ));
  }
  Ok(())
}

/// Whether `filter` matches `topic`, per section 4.7 of the MQTT 5 specification.
pub fn matches(filter: &str, topic: &str) -> bool {
  // The broker's own topics are reachable only by naming them, never by a wildcard.
  if topic.starts_with('$') && (filter.starts_with('+') || filter.starts_with('#')) {
    return false;
  }

  let filter_levels: Vec<&str> = filter.split('/').collect();
  let topic_levels: Vec<&str> = topic.split('/').collect();

  for (index, level) in filter_levels.iter().enumerate() {
    match *level {
      // '#' also matches the parent level, so "sport/#" matches "sport".
      "#" => return true,
      "+" => {
        if index >= topic_levels.len() {
          return false;
        }
      }
      literal => {
        if index >= topic_levels.len() || topic_levels[index] != literal {
          return false;
        }
      }
    }
  }
  filter_levels.len() == topic_levels.len()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn publish_input_is_always_relative_and_leading_slashes_are_removed() {
    for base in ["hiveme", "hiveme/build", "custom/prefix"] {
      for input in ["ci", "/ci", "///ci"] {
        assert_eq!(resolve_publish(base, input), format!("{base}/ci"));
      }
      for input in ["", "/", "///"] {
        assert_eq!(resolve_publish(base, input), base);
      }
    }
    assert_eq!(resolve_publish("", "/ci"), "ci");
    assert_eq!(resolve_publish("", "/"), "");
    assert_eq!(resolve_publish("hiveme", "/a//b/"), "hiveme/a//b/");
    assert_eq!(resolve_publish("/existing/topic/", "/ci"), "/existing/topic//ci");
    assert_eq!(resolve_publish("hiveme", "/$SYS/status"), "hiveme/$SYS/status");
    assert!(validate_topic(&resolve_publish("hiveme", "/bad/#")).is_err());
  }

  #[test]
  fn a_relative_topic_takes_the_prefix() {
    assert_eq!(resolve("hiveme", "info", false), "hiveme/info");
    assert_eq!(resolve("hiveme", "build/done", false), "hiveme/build/done");
  }

  #[test]
  fn an_absolute_topic_ignores_the_prefix() {
    assert_eq!(resolve("hiveme", "$SYS/broker/uptime", true), "$SYS/broker/uptime");
  }

  #[test]
  fn an_empty_prefix_leaves_the_topic_alone() {
    assert_eq!(resolve("", "info", false), "info");
  }

  #[test]
  fn a_prefix_written_with_slashes_is_tidied_up() {
    assert_eq!(resolve("/hiveme/", "info", false), "hiveme/info");
  }

  #[test]
  fn the_prefix_alone_resolves_to_itself() {
    assert_eq!(resolve("hiveme", "", false), "hiveme");
  }

  #[test]
  fn topics_are_validated() {
    assert!(validate_topic("hiveme/info").is_ok());
    assert!(validate_topic("").is_err());
    assert!(validate_topic("hiveme/+").is_err());
    assert!(validate_topic("hiveme/#").is_err());
    assert!(validate_topic("hiveme/\u{1}").is_err());
    assert!(validate_topic(&"a".repeat(MAX_TOPIC_BYTES + 1)).is_err());
  }

  #[test]
  fn filters_are_validated() {
    assert!(validate_filter("#").is_ok());
    assert!(validate_filter("hiveme/#").is_ok());
    assert!(validate_filter("hiveme/+/info").is_ok());
    assert!(validate_filter("$SYS/#").is_ok());
    assert!(validate_filter("").is_err());
    assert!(validate_filter("hiveme/#/info").is_err());
    assert!(validate_filter("hiveme/in#fo").is_err());
    assert!(validate_filter("hiveme/in+fo").is_err());
  }

  /// The examples in section 4.7 of the MQTT 5 specification.
  #[test]
  fn matching_follows_the_mqtt_specification() {
    let cases: &[(&str, &str, bool)] = &[
      ("sport/tennis/player1/#", "sport/tennis/player1", true),
      ("sport/tennis/player1/#", "sport/tennis/player1/ranking", true),
      ("sport/tennis/player1/#", "sport/tennis/player1/score/wimbledon", true),
      ("sport/#", "sport", true),
      ("#", "sport/tennis/player1", true),
      ("sport/tennis/+", "sport/tennis/player1", true),
      ("sport/tennis/+", "sport/tennis/player2", true),
      ("sport/tennis/+", "sport/tennis/player1/ranking", false),
      ("sport/+", "sport", false),
      ("sport/+", "sport/", true),
      ("+/tennis/#", "sport/tennis/player1", true),
      ("+", "sport", true),
      ("+", "sport/tennis", false),
      ("sport/tennis/player1", "sport/tennis/player1", true),
      ("sport/tennis/player1", "sport/tennis/player2", false),
    ];
    for (filter, topic, expected) in cases {
      assert_eq!(matches(filter, topic), *expected, "{filter} against {topic}");
    }
  }

  #[test]
  fn a_wildcard_never_reaches_a_dollar_topic() {
    assert!(!matches("#", "$SYS/broker/uptime"));
    assert!(!matches("+/monitor/Clients", "$SYS/monitor/Clients"));
    assert!(matches("$SYS/#", "$SYS/broker/uptime"));
    assert!(matches("$SYS/monitor/+", "$SYS/monitor/Clients"));
  }

  #[test]
  fn the_gui_default_subscription_reaches_the_whole_prefix() {
    let filter = resolve("hiveme", "#", false);
    assert_eq!(filter, "hiveme/#");
    assert!(matches(&filter, "hiveme/info"));
    assert!(matches(&filter, "hiveme"));
    assert!(!matches(&filter, "other/info"));
  }
}
