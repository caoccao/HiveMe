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

//! The nine catalogs, flattened to dotted keys, and the lookups over them.
//!
//! The files are i18next catalogs: nested objects of strings, `{{name}}` and
//! `{{name, number}}` placeholders, and `_one`, `_many`, and `_other` suffixes for
//! counted keys. A key a locale lacks falls back to `en-US`, then to the key itself;
//! the tests below make a missing key a failure, so neither fallback is seen in a
//! release.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::{Locale, format};

/// The catalog files, in the order of [`Locale::ALL`].
const SOURCES: [&str; 9] = [
  include_str!("../../../../locales/de.json"),
  include_str!("../../../../locales/en-US.json"),
  include_str!("../../../../locales/es.json"),
  include_str!("../../../../locales/fr.json"),
  include_str!("../../../../locales/it.json"),
  include_str!("../../../../locales/ja.json"),
  include_str!("../../../../locales/zh-CN.json"),
  include_str!("../../../../locales/zh-HK.json"),
  include_str!("../../../../locales/zh-TW.json"),
];

/// One locale's strings by dotted key, such as `messages.bytes_one`.
type Catalog = HashMap<String, String>;

/// Every catalog, parsed on first use.
fn catalogs() -> &'static [Catalog] {
  static CATALOGS: OnceLock<Vec<Catalog>> = OnceLock::new();
  CATALOGS.get_or_init(|| {
    Locale::ALL
      .into_iter()
      .map(|locale| parse(SOURCES[locale.index()]).unwrap_or_else(|error| panic!("locales/{locale}.json: {error}")))
      .collect()
  })
}

fn catalog(locale: Locale) -> &'static Catalog {
  &catalogs()[locale.index()]
}

/// Flattens one catalog file.
fn parse(text: &str) -> Result<Catalog, String> {
  let value: serde_json::Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
  let mut catalog = Catalog::new();
  flatten(&value, "", &mut catalog)?;
  Ok(catalog)
}

fn flatten(value: &serde_json::Value, prefix: &str, catalog: &mut Catalog) -> Result<(), String> {
  match value {
    serde_json::Value::Object(object) => {
      for (key, value) in object {
        flatten(value, &format!("{prefix}{key}."), catalog)?;
      }
      Ok(())
    }
    serde_json::Value::String(text) => {
      catalog.insert(prefix.trim_end_matches('.').to_owned(), text.clone());
      Ok(())
    }
    other => Err(format!("{} is {other}, not a string", prefix.trim_end_matches('.'))),
  }
}

/// The text of `key` in `locale`, then in English.
fn lookup(locale: Locale, key: &str) -> Option<&'static str> {
  catalog(locale)
    .get(key)
    .or_else(|| catalog(Locale::EnUs).get(key))
    .map(String::as_str)
}

/// The text of `key`, as written in the catalog.
///
/// Placeholders are left as they are; use [`t_with`] for a string that has any.
pub fn t(locale: Locale, key: &str) -> String {
  lookup(locale, key).unwrap_or(key).to_owned()
}

/// The text of `key` with its `{{name}}` placeholders filled in.
///
/// A `{{name, number}}` placeholder whose value is an integer is grouped the way the
/// locale groups digits. A placeholder with no value is left as written, so that a
/// missing argument shows rather than vanishing.
pub fn t_with(locale: Locale, key: &str, values: &[(&str, &str)]) -> String {
  interpolate(locale, lookup(locale, key).unwrap_or(key), values)
}

/// The text of a counted key, such as `messages.bytes`, for `count`.
///
/// The key is suffixed with the plural category of the count in the locale, as i18next
/// does, and `{{count}}` is filled in.
pub fn t_count(locale: Locale, key: &str, count: u64) -> String {
  let text = plural_text(locale, key, count)
    .or_else(|| plural_text(Locale::EnUs, key, count))
    .unwrap_or(key);
  interpolate(locale, text, &[("count", &count.to_string())])
}

fn plural_text(locale: Locale, key: &str, count: u64) -> Option<&'static str> {
  let catalog = catalog(locale);
  [
    format!("{key}_{}", locale.plural(count).name()),
    format!("{key}_other"),
    key.to_owned(),
  ]
  .iter()
  .find_map(|candidate| catalog.get(candidate))
  .map(String::as_str)
}

fn interpolate(locale: Locale, text: &str, values: &[(&str, &str)]) -> String {
  let mut output = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(start) = rest.find("{{") {
    output.push_str(&rest[..start]);
    let inner_and_rest = &rest[start + 2..];
    let Some(end) = inner_and_rest.find("}}") else {
      output.push_str(&rest[start..]);
      return output;
    };
    let inner = &inner_and_rest[..end];
    let (name, style) = match inner.split_once(',') {
      Some((name, style)) => (name.trim(), Some(style.trim())),
      None => (inner.trim(), None),
    };
    match values.iter().find(|(candidate, _)| *candidate == name) {
      Some((_, value)) => match (style, value.parse::<i64>()) {
        (Some("number"), Ok(number)) => output.push_str(&format::integer(locale, number)),
        _ => output.push_str(value),
      },
      None => output.push_str(&rest[start..start + 2 + end + 2]),
    }
    rest = &inner_and_rest[end + 2..];
  }
  output.push_str(rest);
  output
}

#[cfg(test)]
mod tests {
  use std::collections::{BTreeMap, BTreeSet};

  use super::*;

  /// The key without its plural suffix, as the frontend's catalog test computes it.
  fn base_key(key: &str) -> &str {
    for suffix in ["_zero", "_one", "_two", "_few", "_many", "_other"] {
      if let Some(base) = key.strip_suffix(suffix) {
        return base;
      }
    }
    key
  }

  fn variables(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
      let Some(end) = rest[start..].find("}}") else {
        break;
      };
      found.push(rest[start..start + end + 2].to_owned());
      rest = &rest[start + end + 2..];
    }
    found.sort();
    found
  }

  fn english_by_base() -> BTreeMap<&'static str, &'static str> {
    catalog(Locale::EnUs)
      .iter()
      .map(|(key, value)| (base_key(key), value.as_str()))
      .collect()
  }

  #[test]
  fn every_locale_covers_every_english_string_and_keeps_its_variables() {
    let english = english_by_base();
    for locale in Locale::ALL {
      let catalog = catalog(locale);
      let bases: BTreeSet<&str> = catalog.keys().map(|key| base_key(key)).collect();
      let missing: Vec<&&str> = english.keys().filter(|key| !bases.contains(**key)).collect();
      assert!(missing.is_empty(), "{locale} is missing {missing:?}");
      let extra: Vec<&&str> = bases.iter().filter(|key| !english.contains_key(**key)).collect();
      assert!(extra.is_empty(), "{locale} has keys English does not: {extra:?}");
      for (key, value) in catalog {
        assert!(!value.trim().is_empty(), "{locale}: {key} is empty");
        assert_eq!(
          variables(value),
          variables(english[base_key(key)]),
          "{locale}: {key} changes the interpolation variables"
        );
      }
    }
  }

  #[test]
  fn every_counted_key_has_every_plural_category_of_its_locale() {
    let counted: Vec<&str> = catalog(Locale::EnUs)
      .keys()
      .filter_map(|key| key.strip_suffix("_other"))
      .collect();
    assert!(!counted.is_empty());
    for locale in Locale::ALL {
      for key in &counted {
        for category in locale.plural_categories() {
          let suffixed = format!("{key}_{}", category.name());
          assert!(catalog(locale).contains_key(&suffixed), "{locale}: {suffixed}");
        }
      }
    }
  }

  #[test]
  fn counts_come_out_as_the_frontend_renders_them() {
    // The values `src/i18n/index.test.ts` asserts through i18next.
    assert_eq!(t_count(Locale::EnUs, "messages.bytes", 1), "1 byte");
    assert_eq!(t_count(Locale::EnUs, "footer.subscriptions", 0), "0 subscriptions");
    assert_eq!(t_count(Locale::EnUs, "footer.received", 2), "2 messages this session");
    assert_eq!(
      t_count(Locale::De, "topics.unread", 1000),
      "1.000 ungelesene Nachrichten"
    );
    assert_eq!(t_count(Locale::Fr, "messages.bytes", 0), "0 octet");
    assert_eq!(t_count(Locale::Fr, "messages.bytes", 2), "2 octets");
    assert_eq!(
      t_count(Locale::Fr, "messages.bytes", 1_000_000),
      "1\u{202f}000\u{202f}000 octets"
    );
    assert_eq!(t_count(Locale::Ja, "messages.bytes", 1), "1 バイト");
    assert_eq!(t_count(Locale::Ja, "messages.bytes", 2), "2 バイト");
  }

  #[test]
  fn placeholders_are_filled_in_and_a_missing_value_shows() {
    assert_eq!(
      t_with(Locale::EnUs, "messages.qos", &[("qos", "1")]),
      "QoS 1",
      "a plain placeholder"
    );
    assert_eq!(
      interpolate(
        Locale::De,
        "{{count, number}} und {{name}}",
        &[("count", "12345"), ("name", "x")]
      ),
      "12.345 und x"
    );
    assert_eq!(interpolate(Locale::EnUs, "{{absent}} stays", &[]), "{{absent}} stays");
    assert_eq!(
      interpolate(Locale::EnUs, "unclosed {{name", &[("name", "x")]),
      "unclosed {{name"
    );
    assert_eq!(interpolate(Locale::EnUs, "{{ name }}", &[("name", "x")]), "x");
  }

  #[test]
  fn an_unknown_key_falls_back_to_itself() {
    assert_eq!(t(Locale::De, "no.such.key"), "no.such.key");
    assert_eq!(t_count(Locale::Fr, "no.such.count", 3), "no.such.count");
  }

  #[test]
  fn a_key_is_found_in_its_own_locale_before_english() {
    assert_eq!(t(Locale::EnUs, "tabs.messages"), "Messages");
    assert_eq!(t(Locale::De, "tabs.settings"), "Einstellungen");
    assert_ne!(t(Locale::Ja, "tabs.messages"), t(Locale::EnUs, "tabs.messages"));
  }

  #[test]
  fn a_catalog_holds_only_strings() {
    assert!(parse(r#"{"a": {"b": "c"}}"#).unwrap().contains_key("a.b"));
    assert!(parse(r#"{"a": 1}"#).unwrap_err().contains("a is 1"));
    assert!(parse("not json").is_err());
  }
}
