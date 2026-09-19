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

//! Numbers, sizes, durations, times, and dates in the words of a locale.
//!
//! The functions mirror `src/lib/format.ts`, which asks `Intl` for them. The patterns
//! below are what `Intl` in the ICU of Node 24 produces for the options that file
//! passes, written out by hand so that `hmc` needs no ICU data: digit grouping and
//! the decimal separator, the short month names, and the order and padding of the
//! date and time fields.

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, Timelike};

use super::{Locale, t, t_with};

/// The digit group separator, the decimal separator, and the fewest integer digits
/// that are grouped at all.
fn symbols(locale: Locale) -> (&'static str, &'static str, usize) {
  match locale {
    Locale::EnUs | Locale::Ja | Locale::ZhCn | Locale::ZhHk | Locale::ZhTw => (",", ".", 4),
    Locale::De => (".", ",", 4),
    // Spanish and Italian leave a four digit number alone: 1000, but 12.345.
    Locale::Es | Locale::It => (".", ",", 5),
    // A narrow no-break space, as `Intl.NumberFormat("fr")` writes it.
    Locale::Fr => ("\u{202f}", ",", 4),
  }
}

fn group(locale: Locale, digits: &str) -> String {
  let (separator, _, minimum) = symbols(locale);
  if digits.len() < minimum {
    return digits.to_owned();
  }
  let mut output = String::with_capacity(digits.len() + digits.len() / 3 * separator.len());
  for (index, digit) in digits.chars().enumerate() {
    if index > 0 && (digits.len() - index).is_multiple_of(3) {
      output.push_str(separator);
    }
    output.push(digit);
  }
  output
}

/// A whole number, grouped: `1,000` in English, `1.000` in German.
pub fn integer(locale: Locale, value: i64) -> String {
  let grouped = group(locale, &value.unsigned_abs().to_string());
  if value < 0 { format!("-{grouped}") } else { grouped }
}

/// A number with exactly `fraction_digits` digits after the separator.
///
/// Halves round away from zero, as `Intl.NumberFormat` rounds them, rather than to
/// even.
pub fn decimal(locale: Locale, value: f64, fraction_digits: u32) -> String {
  let value = if value.is_finite() { value } else { 0.0 };
  let factor = 10u64.pow(fraction_digits);
  let scaled = (value.abs() * factor as f64).round() as u64;
  let mut output = String::new();
  if value < 0.0 && scaled != 0 {
    output.push('-');
  }
  output.push_str(&group(locale, &(scaled / factor).to_string()));
  if fraction_digits > 0 {
    let (_, separator, _) = symbols(locale);
    output.push_str(separator);
    output.push_str(&format!(
      "{:0width$}",
      scaled % factor,
      width = fraction_digits as usize
    ));
  }
  output
}

/// A byte count in the largest unit that keeps it readable: `512 B`, `1.5 KB`.
pub fn bytes(locale: Locale, bytes: u64) -> String {
  const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
  let mut value = bytes as f64;
  let mut unit = 0;
  while value >= 1024.0 && unit < UNITS.len() - 1 {
    value /= 1024.0;
    unit += 1;
  }
  let digits = if unit == 0 { 0 } else { 1 };
  format!(
    "{} {}",
    decimal(locale, value, digits),
    t(locale, &format!("format.units.{}", UNITS[unit]))
  )
}

/// A duration in milliseconds, as the reconnect countdown shows it: `42s`, `1m 05s`.
pub fn duration(locale: Locale, milliseconds: u64) -> String {
  let seconds = milliseconds.div_ceil(1000);
  if seconds < 60 {
    return t_with(
      locale,
      "format.seconds",
      &[("seconds", &integer(locale, seconds as i64))],
    );
  }
  t_with(
    locale,
    "format.minutesSeconds",
    &[
      ("minutes", &integer(locale, (seconds / 60) as i64)),
      ("seconds", &format!("{:02}", seconds % 60)),
    ],
  )
}

/// The clock time, hours and minutes: `9:41 AM`, `9:41`, `上午9:41`.
pub fn time(locale: Locale, at: &NaiveDateTime) -> String {
  let (hour, minute) = (at.hour(), at.minute());
  match locale {
    Locale::EnUs => format!("{}:{minute:02} {}", hour12(hour), meridiem_en(hour)),
    Locale::ZhHk | Locale::ZhTw => format!("{}{}:{minute:02}", meridiem_zh(hour), hour12(hour)),
    _ => format!("{hour}:{minute:02}"),
  }
}

/// A message's local time, including its date unless it falls on today.
pub fn message_time(locale: Locale, timestamp: &str) -> String {
  local(timestamp).map_or_else(
    || timestamp.to_owned(),
    |at| message_time_on_day(locale, &at, Local::now().date_naive()),
  )
}

fn message_time_on_day(locale: Locale, at: &NaiveDateTime, today: NaiveDate) -> String {
  let clock = time(locale, at);
  if at.date() == today {
    clock
  } else {
    format!("{} {clock}", day(locale, at))
  }
}

/// The date and the time to the second, for a tooltip or a detail line.
pub fn date_time(locale: Locale, at: &NaiveDateTime) -> String {
  let (year, month, day) = (at.year(), at.month(), at.day());
  let (hour, minute, second) = (at.hour(), at.minute(), at.second());
  match locale {
    Locale::EnUs => format!(
      "{month}/{day}/{year}, {}:{minute:02}:{second:02} {}",
      hour12(hour),
      meridiem_en(hour)
    ),
    Locale::De => format!("{day}.{month}.{year}, {hour:02}:{minute:02}:{second:02}"),
    Locale::Es => format!("{day}/{month}/{year}, {hour}:{minute:02}:{second:02}"),
    Locale::Fr => format!("{day:02}/{month:02}/{year} {hour:02}:{minute:02}:{second:02}"),
    Locale::It => format!("{day:02}/{month:02}/{year}, {hour:02}:{minute:02}:{second:02}"),
    Locale::Ja => format!("{year}/{month}/{day} {hour}:{minute:02}:{second:02}"),
    Locale::ZhCn => format!("{year}/{month}/{day} {hour:02}:{minute:02}:{second:02}"),
    Locale::ZhHk => format!(
      "{day}/{month}/{year} {}{}:{minute:02}:{second:02}",
      meridiem_zh(hour),
      hour12(hour)
    ),
    Locale::ZhTw => format!(
      "{year}/{month}/{day} {}{}:{minute:02}:{second:02}",
      meridiem_zh(hour),
      hour12(hour)
    ),
  }
}

/// The day, with a short month name, as the separator between message bubbles shows it.
pub fn day(locale: Locale, at: &NaiveDateTime) -> String {
  let (year, day) = (at.year(), at.day());
  let month = at.month0() as usize;
  match locale {
    Locale::EnUs => format!("{} {day}, {year}", MONTHS_EN[month]),
    Locale::De => format!("{day}. {} {year}", MONTHS_DE[month]),
    Locale::Es => format!("{day} {} {year}", MONTHS_ES[month]),
    Locale::Fr => format!("{day} {} {year}", MONTHS_FR[month]),
    Locale::It => format!("{day} {} {year}", MONTHS_IT[month]),
    Locale::Ja | Locale::ZhCn | Locale::ZhHk | Locale::ZhTw => format!("{year}年{}月{day}日", month + 1),
  }
}

/// The local wall clock time of an RFC 3339 timestamp, or `None` when it is not one.
pub fn local(timestamp: &str) -> Option<NaiveDateTime> {
  DateTime::parse_from_rfc3339(timestamp)
    .ok()
    .map(|at| at.with_timezone(&Local).naive_local())
}

const MONTHS_EN: [&str; 12] = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_DE: [&str; 12] = [
  "Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez.",
];
const MONTHS_ES: [&str; 12] = [
  "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic",
];
const MONTHS_FR: [&str; 12] = [
  "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.",
];
const MONTHS_IT: [&str; 12] = [
  "gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic",
];

fn hour12(hour: u32) -> u32 {
  match hour % 12 {
    0 => 12,
    hour => hour,
  }
}

fn meridiem_en(hour: u32) -> &'static str {
  if hour < 12 { "AM" } else { "PM" }
}

fn meridiem_zh(hour: u32) -> &'static str {
  if hour < 12 { "上午" } else { "下午" }
}

#[cfg(test)]
mod tests {
  use chrono::NaiveDate;

  use super::*;

  fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, month, day)
      .unwrap()
      .and_hms_opt(hour, minute, second)
      .unwrap()
  }

  #[test]
  fn numbers_are_grouped_as_intl_groups_them() {
    for (locale, expected) in [
      (Locale::De, ["1.000", "12.345", "1.234.567"]),
      (Locale::EnUs, ["1,000", "12,345", "1,234,567"]),
      (Locale::Es, ["1000", "12.345", "1.234.567"]),
      (Locale::Fr, ["1\u{202f}000", "12\u{202f}345", "1\u{202f}234\u{202f}567"]),
      (Locale::It, ["1000", "12.345", "1.234.567"]),
      (Locale::Ja, ["1,000", "12,345", "1,234,567"]),
      (Locale::ZhCn, ["1,000", "12,345", "1,234,567"]),
      (Locale::ZhHk, ["1,000", "12,345", "1,234,567"]),
      (Locale::ZhTw, ["1,000", "12,345", "1,234,567"]),
    ] {
      let actual = [1000, 12345, 1234567].map(|value| integer(locale, value));
      assert_eq!(actual, expected, "{locale}");
      assert_eq!(integer(locale, 999), "999");
      assert_eq!(integer(locale, 0), "0");
    }
    assert_eq!(integer(Locale::EnUs, -1234), "-1,234");
  }

  #[test]
  fn decimals_round_halves_away_from_zero() {
    assert_eq!(decimal(Locale::EnUs, 1.5, 1), "1.5");
    assert_eq!(decimal(Locale::De, 1.5, 1), "1,5");
    assert_eq!(decimal(Locale::EnUs, 1.25, 1), "1.3");
    assert_eq!(decimal(Locale::EnUs, 1023.95, 1), "1,024.0");
    assert_eq!(decimal(Locale::Fr, 1023.95, 1), "1\u{202f}024,0");
    assert_eq!(decimal(Locale::Es, 1023.95, 1), "1024,0");
    assert_eq!(decimal(Locale::EnUs, 512.0, 0), "512");
    assert_eq!(decimal(Locale::EnUs, f64::NAN, 1), "0.0");
  }

  #[test]
  fn sizes_and_durations_come_out_as_the_frontend_renders_them() {
    // The values `src/i18n/index.test.ts` asserts through `src/lib/format.ts`.
    assert_eq!(bytes(Locale::De, 1536), "1,5 KB");
    assert_eq!(bytes(Locale::Fr, 1536), "1,5 Ko");
    assert_eq!(duration(Locale::De, 65_000), "1 min 05 s");
    assert_eq!(duration(Locale::Ja, 65_000), "1分05秒");

    assert_eq!(bytes(Locale::EnUs, 0), "0 B");
    assert_eq!(bytes(Locale::EnUs, 1023), "1,023 B");
    assert_eq!(bytes(Locale::EnUs, 5 * 1024 * 1024), "5.0 MB");
    assert_eq!(bytes(Locale::EnUs, u64::MAX), "16,777,216.0 TB");
    assert_eq!(duration(Locale::EnUs, 0), "0s");
    assert_eq!(duration(Locale::EnUs, 1), "1s");
    assert_eq!(duration(Locale::EnUs, 59_001), "1m 00s");
    assert_eq!(duration(Locale::EnUs, 3_723_000), "62m 03s");
  }

  #[test]
  fn times_follow_the_locale() {
    // `toLocaleTimeString(tag, { hour: "numeric", minute: "2-digit" })` at 00:07,
    // 09:07, 12:07, 13:07, and 23:07.
    let hours = [0, 9, 12, 13, 23];
    for (locale, expected) in [
      (Locale::De, ["0:07", "9:07", "12:07", "13:07", "23:07"]),
      (Locale::EnUs, ["12:07 AM", "9:07 AM", "12:07 PM", "1:07 PM", "11:07 PM"]),
      (Locale::Es, ["0:07", "9:07", "12:07", "13:07", "23:07"]),
      (Locale::Fr, ["0:07", "9:07", "12:07", "13:07", "23:07"]),
      (Locale::It, ["0:07", "9:07", "12:07", "13:07", "23:07"]),
      (Locale::Ja, ["0:07", "9:07", "12:07", "13:07", "23:07"]),
      (Locale::ZhCn, ["0:07", "9:07", "12:07", "13:07", "23:07"]),
      (
        Locale::ZhHk,
        ["上午12:07", "上午9:07", "下午12:07", "下午1:07", "下午11:07"],
      ),
      (
        Locale::ZhTw,
        ["上午12:07", "上午9:07", "下午12:07", "下午1:07", "下午11:07"],
      ),
    ] {
      let actual = hours.map(|hour| time(locale, &at(2026, 11, 25, hour, 7, 3)));
      assert_eq!(actual, expected, "{locale}");
    }
  }

  #[test]
  fn message_times_include_dates_except_on_the_current_local_day() {
    let today = at(2026, 9, 19, 0, 5, 0).date();
    for (date, expected) in [
      (at(2026, 9, 19, 0, 1, 0), "12:01 AM"),
      (at(2026, 9, 19, 23, 59, 0), "11:59 PM"),
      (at(2026, 9, 18, 23, 59, 0), "Sep 18, 2026 11:59 PM"),
      (at(2026, 8, 19, 9, 41, 0), "Aug 19, 2026 9:41 AM"),
      (at(2025, 9, 19, 9, 41, 0), "Sep 19, 2025 9:41 AM"),
      (at(2026, 9, 20, 9, 41, 0), "Sep 20, 2026 9:41 AM"),
    ] {
      assert_eq!(message_time_on_day(Locale::EnUs, &date, today), expected);
    }
    assert_eq!(
      message_time_on_day(
        Locale::EnUs,
        &at(2026, 12, 31, 23, 59, 0),
        at(2027, 1, 1, 0, 1, 0).date()
      ),
      "Dec 31, 2026 11:59 PM"
    );
    assert_eq!(message_time(Locale::EnUs, "invalid"), "invalid");
  }

  #[test]
  fn message_dates_and_times_follow_the_selected_language() {
    let timestamp = at(2026, 9, 18, 9, 41, 0);
    for locale in Locale::ALL {
      assert_eq!(
        message_time_on_day(locale, &timestamp, timestamp.date()),
        time(locale, &timestamp)
      );
      assert_eq!(
        message_time_on_day(locale, &timestamp, at(2026, 9, 19, 0, 0, 0).date()),
        format!("{} {}", day(locale, &timestamp), time(locale, &timestamp))
      );
    }
    assert_eq!(
      message_time_on_day(Locale::De, &timestamp, at(2026, 9, 19, 0, 0, 0).date()),
      "18. Sept. 2026 9:41"
    );
  }

  #[test]
  fn date_times_follow_the_locale() {
    // `toLocaleString(tag)` at 2026-09-12 09:41:23 and 2026-01-03 21:05:09.
    for (locale, morning, evening) in [
      (Locale::De, "12.9.2026, 09:41:23", "3.1.2026, 21:05:09"),
      (Locale::EnUs, "9/12/2026, 9:41:23 AM", "1/3/2026, 9:05:09 PM"),
      (Locale::Es, "12/9/2026, 9:41:23", "3/1/2026, 21:05:09"),
      (Locale::Fr, "12/09/2026 09:41:23", "03/01/2026 21:05:09"),
      (Locale::It, "12/09/2026, 09:41:23", "03/01/2026, 21:05:09"),
      (Locale::Ja, "2026/9/12 9:41:23", "2026/1/3 21:05:09"),
      (Locale::ZhCn, "2026/9/12 09:41:23", "2026/1/3 21:05:09"),
      (Locale::ZhHk, "12/9/2026 上午9:41:23", "3/1/2026 下午9:05:09"),
      (Locale::ZhTw, "2026/9/12 上午9:41:23", "2026/1/3 下午9:05:09"),
    ] {
      assert_eq!(date_time(locale, &at(2026, 9, 12, 9, 41, 23)), morning, "{locale}");
      assert_eq!(date_time(locale, &at(2026, 1, 3, 21, 5, 9)), evening, "{locale}");
    }
  }

  #[test]
  fn days_follow_the_locale() {
    // `toLocaleDateString(tag, { year: "numeric", month: "short", day: "numeric" })`.
    for (locale, september, all_months) in [
      (Locale::De, "12. Sept. 2026", MONTHS_DE),
      (Locale::EnUs, "Sep 12, 2026", MONTHS_EN),
      (Locale::Es, "12 sept 2026", MONTHS_ES),
      (Locale::Fr, "12 sept. 2026", MONTHS_FR),
      (Locale::It, "12 set 2026", MONTHS_IT),
    ] {
      assert_eq!(day(locale, &at(2026, 9, 12, 9, 41, 23)), september, "{locale}");
      for (month, name) in all_months.iter().enumerate() {
        assert!(day(locale, &at(2026, month as u32 + 1, 5, 0, 7, 3)).contains(name));
      }
    }
    for locale in [Locale::Ja, Locale::ZhCn, Locale::ZhHk, Locale::ZhTw] {
      assert_eq!(day(locale, &at(2026, 9, 12, 9, 41, 23)), "2026年9月12日", "{locale}");
    }
    assert_eq!(day(Locale::De, &at(2026, 3, 5, 0, 0, 0)), "5. März 2026");
    assert_eq!(day(Locale::Fr, &at(2026, 2, 5, 0, 0, 0)), "5 févr. 2026");
  }

  #[test]
  fn a_timestamp_that_is_not_rfc_3339_has_no_local_time() {
    assert!(local("invalid").is_none());
    let parsed = local("2026-09-12T09:41:23Z").unwrap();
    let expected = DateTime::parse_from_rfc3339("2026-09-12T09:41:23Z")
      .unwrap()
      .with_timezone(&Local)
      .naive_local();
    assert_eq!(parsed, expected);
  }
}
