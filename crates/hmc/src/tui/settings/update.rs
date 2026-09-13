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

//! Update: how often the GitHub release check runs.

use std::time::Instant;

use hiveme_core::config::UpdateCheckInterval;
use hiveme_core::i18n::t;
use ratatui::style::Style;

use super::{Field, Form, Item, Row, Size};
use crate::tui::app::App;
use crate::tui::service::Service;

pub fn form<S: Service>(app: &App<S>) -> Form {
  let (intervals, interval) = choices(app, Field::CheckInterval);
  Form {
    rows: vec![Row::line(
      t(app.locale, "settings.checkInterval"),
      vec![Item::select(
        Field::CheckInterval,
        intervals[interval].0.clone(),
        Style::new(),
        Size::Fit,
      )],
    )],
  }
}

pub fn choices<S: Service>(app: &App<S>, field: Field) -> (Vec<(String, Style)>, usize) {
  if field != Field::CheckInterval {
    return (Vec::new(), 0);
  }
  let all = UpdateCheckInterval::all();
  (
    all
      .iter()
      .map(|interval| {
        (
          t(app.locale, &format!("settings.interval.{}", interval.as_str())),
          Style::new(),
        )
      })
      .collect(),
    all
      .iter()
      .position(|interval| *interval == app.config.update.check_interval)
      .unwrap_or(0),
  )
}

pub fn choose<S: Service>(app: &mut App<S>, field: Field, index: usize, now: Instant) {
  if field == Field::CheckInterval {
    let all = UpdateCheckInterval::all();
    let interval = all[index.min(all.len() - 1)];
    app.edit_config(now, |config| config.update.check_interval = interval);
  }
}
