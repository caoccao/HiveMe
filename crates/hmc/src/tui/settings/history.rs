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

//! History: how many messages the database keeps per topic and for how long.

use hiveme_core::config::Config;
use hiveme_core::i18n::t;

use super::{Field, Form, Item, Row, Size, number};
use crate::tui::app::App;
use crate::tui::service::Service;

/// How wide a number field is.
pub const NUMBER_WIDTH: u16 = 12;

pub fn form<S: Service>(app: &App<S>) -> Form {
  let locale = app.locale;
  Form {
    rows: vec![
      Row::note(t(locale, "settings.historyHint")),
      Row::Blank,
      Row::line(
        t(locale, "settings.maxMessagesPerTopic"),
        vec![Item::input(
          Field::MaxMessagesPerTopic,
          false,
          Size::Cells(NUMBER_WIDTH),
        )],
      ),
      Row::line(
        t(locale, "settings.retentionDays"),
        vec![Item::input(Field::RetentionDays, false, Size::Cells(NUMBER_WIDTH))],
      ),
    ],
  }
}

pub fn text(config: &Config, field: Field) -> Option<String> {
  let history = &config.gui.history;
  match field {
    Field::MaxMessagesPerTopic => Some(history.max_messages_per_topic.to_string()),
    Field::RetentionDays => Some(history.retention_days.to_string()),
    _ => None,
  }
}

pub fn set_text(config: &mut Config, field: Field, text: &str) {
  let history = &mut config.gui.history;
  match field {
    Field::MaxMessagesPerTopic => {
      if let Some(value) = number(text) {
        history.max_messages_per_topic = value;
      }
    }
    Field::RetentionDays => {
      if let Some(value) = number(text) {
        history.retention_days = value;
      }
    }
    _ => {}
  }
}
