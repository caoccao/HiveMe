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

//! The CLDR cardinal plural rules of the nine locales, for whole counts.
//!
//! These are the categories `Intl.PluralRules` reports and the frontend's catalogs are
//! keyed by. Every count this application shows is a whole number, so the rules for
//! fractions and compact exponents are not needed.

use super::Locale;

/// The suffix a counted catalog key carries for a count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluralCategory {
  One,
  Many,
  Other,
}

impl PluralCategory {
  /// The i18next suffix without its underscore.
  pub fn name(self) -> &'static str {
    match self {
      Self::One => "one",
      Self::Many => "many",
      Self::Other => "other",
    }
  }
}

pub(super) fn category(locale: Locale, count: u64) -> PluralCategory {
  // Spanish, French, and Italian use `many` for a nonzero multiple of a million, as in
  // "un million d'octets".
  let millions = count != 0 && count.is_multiple_of(1_000_000);
  match locale {
    Locale::De | Locale::EnUs => {
      if count == 1 {
        PluralCategory::One
      } else {
        PluralCategory::Other
      }
    }
    Locale::Es | Locale::It => {
      if count == 1 {
        PluralCategory::One
      } else if millions {
        PluralCategory::Many
      } else {
        PluralCategory::Other
      }
    }
    Locale::Fr => {
      if count <= 1 {
        PluralCategory::One
      } else if millions {
        PluralCategory::Many
      } else {
        PluralCategory::Other
      }
    }
    Locale::Ja | Locale::ZhCn | Locale::ZhHk | Locale::ZhTw => PluralCategory::Other,
  }
}

pub(super) fn categories(locale: Locale) -> &'static [PluralCategory] {
  match locale {
    Locale::De | Locale::EnUs => &[PluralCategory::One, PluralCategory::Other],
    Locale::Es | Locale::Fr | Locale::It => &[PluralCategory::One, PluralCategory::Many, PluralCategory::Other],
    Locale::Ja | Locale::ZhCn | Locale::ZhHk | Locale::ZhTw => &[PluralCategory::Other],
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn categories_match_intl_plural_rules() {
    // `new Intl.PluralRules(tag).select(n)` for 0, 1, 2, 1e6, 2e6, and 1000001.
    let counts = [0, 1, 2, 1_000_000, 2_000_000, 1_000_001];
    for (locale, expected) in [
      (Locale::De, "other one other other other other"),
      (Locale::EnUs, "other one other other other other"),
      (Locale::Es, "other one other many many other"),
      (Locale::Fr, "one one other many many other"),
      (Locale::It, "other one other many many other"),
      (Locale::Ja, "other other other other other other"),
      (Locale::ZhCn, "other other other other other other"),
      (Locale::ZhHk, "other other other other other other"),
      (Locale::ZhTw, "other other other other other other"),
    ] {
      let actual: Vec<&str> = counts.iter().map(|count| category(locale, *count).name()).collect();
      assert_eq!(actual.join(" "), expected, "{locale}");
    }
  }

  #[test]
  fn every_category_a_locale_selects_is_one_it_lists() {
    for locale in Locale::ALL {
      for count in [0, 1, 2, 5, 11, 100, 1_000_000, 3_000_000] {
        assert!(
          categories(locale).contains(&category(locale, count)),
          "{locale} {count}"
        );
      }
    }
  }
}
