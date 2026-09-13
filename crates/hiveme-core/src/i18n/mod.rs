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

//! The languages of `hmc`, from the catalogs the frontend of `hmg` reads.
//!
//! The nine catalogs live in `locales/` at the repository root and are embedded here
//! with `include_str!`, so the two applications cannot ship different words. `hmg`
//! keeps react-i18next; this module is the part of i18next that `hmc` needs: locale
//! resolution, key lookup with an English fallback, `{{name}}` interpolation, plural
//! suffixes, and the number, size, duration, and date formats of `src/lib/format.ts`,
//! written by hand rather than through an ICU dependency. See `docs/specs/tui.md`,
//! "Languages".

mod catalog;
pub mod format;
mod plural;

use std::fmt;

pub use catalog::{t, t_count, t_with};
pub use plural::PluralCategory;

/// A language both applications ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale {
  De,
  #[default]
  EnUs,
  Es,
  Fr,
  It,
  Ja,
  ZhCn,
  ZhHk,
  ZhTw,
}

impl Locale {
  /// Every locale, in the order of `LANGUAGES` in `src/lib/protocol.ts`.
  pub const ALL: [Self; 9] = [
    Self::De,
    Self::EnUs,
    Self::Es,
    Self::Fr,
    Self::It,
    Self::Ja,
    Self::ZhCn,
    Self::ZhHk,
    Self::ZhTw,
  ];

  /// The tag the catalog file and `gui.language` use.
  pub fn tag(self) -> &'static str {
    match self {
      Self::De => "de",
      Self::EnUs => "en-US",
      Self::Es => "es",
      Self::Fr => "fr",
      Self::It => "it",
      Self::Ja => "ja",
      Self::ZhCn => "zh-CN",
      Self::ZhHk => "zh-HK",
      Self::ZhTw => "zh-TW",
    }
  }

  /// The bundled locale a BCP 47 tag means, with English as the fallback.
  ///
  /// The rules are `resolveLanguage` of `src/i18n/index.ts`: case and underscores do
  /// not matter, a regional tag such as `de-DE` resolves to its language, and Chinese
  /// resolves by script and region, so `zh-Hant` is Taiwan and `zh-MO` is Hong Kong.
  pub fn resolve(tag: &str) -> Self {
    let tag = tag.trim().replace('_', "-").to_lowercase();
    if let Some(exact) = Self::ALL.into_iter().find(|locale| locale.tag().to_lowercase() == tag) {
      return exact;
    }
    let mut parts = tag.split('-');
    let base = parts.next().unwrap_or_default();
    let subtags: Vec<&str> = parts.collect();
    if base == "zh" {
      if subtags.contains(&"hk") || subtags.contains(&"mo") {
        return Self::ZhHk;
      }
      if subtags.contains(&"hant") || subtags.contains(&"tw") {
        return Self::ZhTw;
      }
      return Self::ZhCn;
    }
    Self::ALL
      .into_iter()
      .find(|locale| locale.tag() == base)
      .unwrap_or(Self::EnUs)
  }

  /// The plural category of a count, which picks the `_one`, `_many`, or `_other` key.
  pub fn plural(self, count: u64) -> PluralCategory {
    plural::category(self, count)
  }

  /// Every plural category a catalog of this locale needs for a counted key.
  pub fn plural_categories(self) -> &'static [PluralCategory] {
    plural::categories(self)
  }

  /// The position of this locale in [`Locale::ALL`].
  fn index(self) -> usize {
    self as usize
  }
}

impl fmt::Display for Locale {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.tag())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_index_is_the_position_in_all() {
    for (index, locale) in Locale::ALL.into_iter().enumerate() {
      assert_eq!(locale.index(), index, "{locale}");
    }
  }

  #[test]
  fn tags_resolve_as_the_frontend_resolves_them() {
    // The table of `src/i18n/index.test.ts`.
    for (requested, expected) in [
      ("de-DE", Locale::De),
      ("es-MX", Locale::Es),
      ("fr-CA", Locale::Fr),
      ("it-CH", Locale::It),
      ("ja-JP", Locale::Ja),
      ("en-GB", Locale::EnUs),
      ("zh", Locale::ZhCn),
      ("zh-Hans-SG", Locale::ZhCn),
      ("zh-Hant", Locale::ZhTw),
      ("zh-Hant-HK", Locale::ZhHk),
      ("zh-MO", Locale::ZhHk),
      ("zh_TW", Locale::ZhTw),
      (" DE-at ", Locale::De),
      ("unsupported", Locale::EnUs),
      ("", Locale::EnUs),
    ] {
      assert_eq!(Locale::resolve(requested), expected, "{requested:?}");
    }
    for locale in Locale::ALL {
      assert_eq!(Locale::resolve(locale.tag()), locale);
    }
  }

  #[test]
  fn english_is_the_default() {
    assert_eq!(Locale::default(), Locale::EnUs);
    assert_eq!(Locale::EnUs.to_string(), "en-US");
  }
}
