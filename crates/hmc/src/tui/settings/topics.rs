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

//! Topics: the subscriptions, each a filter and an Absolute checkbox, with Add and
//! Remove.

use std::time::Instant;

use hiveme_core::config::{Config, Subscription};
use hiveme_core::i18n::t;

use super::{Field, Form, Item, Row, Size};
use crate::tui::app::App;
use crate::tui::service::Service;

/// A subscription as the editor works with it. `SubscriptionDraft` in `Config.tsx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
  pub filter: String,
  pub absolute: bool,
}

/// Reads both shapes a subscription can be written in. `toDrafts` in `Config.tsx`.
pub fn to_drafts(subscriptions: &[Subscription]) -> Vec<Draft> {
  subscriptions
    .iter()
    .map(|subscription| match subscription {
      Subscription::Relative(filter) => Draft {
        filter: filter.clone(),
        absolute: false,
      },
      Subscription::Explicit { filter, absolute } => Draft {
        filter: filter.clone(),
        absolute: *absolute,
      },
    })
    .collect()
}

/// Writes a relative filter as a bare string and an absolute one as an object, each
/// trimmed. `fromDrafts` in `Config.tsx`.
pub fn from_drafts(drafts: &[Draft]) -> Vec<Subscription> {
  drafts
    .iter()
    .map(|draft| {
      let filter = draft.filter.trim().to_owned();
      if draft.absolute {
        Subscription::Explicit { filter, absolute: true }
      } else {
        Subscription::Relative(filter)
      }
    })
    .collect()
}

pub fn form<S: Service>(app: &App<S>) -> Form {
  let locale = app.locale;
  let mut rows = vec![Row::Heading {
    title: t(locale, "settings.subscriptions"),
    action: Some(Item::button(
      Field::AddSubscription,
      t(locale, "settings.addSubscription"),
      true,
      Size::Fit,
    )),
  }];
  for (index, draft) in to_drafts(&app.config.topics.subscriptions).into_iter().enumerate() {
    rows.push(Row::controls(vec![
      Item::input(Field::SubscriptionFilter(index), false, Size::Fill(1)),
      Item::check(
        Field::SubscriptionAbsolute(index),
        draft.absolute,
        t(locale, "settings.absolute"),
        Size::Fit,
      ),
      Item::button(
        Field::RemoveSubscription(index),
        app.glyphs.close.to_owned(),
        true,
        Size::Fit,
      ),
    ]));
  }
  Form { rows }
}

pub fn text(config: &Config, field: Field) -> Option<String> {
  match field {
    Field::SubscriptionFilter(index) => config
      .topics
      .subscriptions
      .get(index)
      .map(|subscription| subscription.filter().to_owned()),
    _ => None,
  }
}

pub fn set_text(config: &mut Config, field: Field, text: &str) {
  let mut drafts = to_drafts(&config.topics.subscriptions);
  if let Field::SubscriptionFilter(index) = field
    && let Some(draft) = drafts.get_mut(index)
  {
    draft.filter = text.to_owned();
    config.topics.subscriptions = from_drafts(&drafts);
  }
}

pub fn toggle(config: &mut Config, field: Field) {
  let mut drafts = to_drafts(&config.topics.subscriptions);
  if let Field::SubscriptionAbsolute(index) = field
    && let Some(draft) = drafts.get_mut(index)
  {
    draft.absolute = !draft.absolute;
    config.topics.subscriptions = from_drafts(&drafts);
  }
}

/// Add appends `#` and moves to its filter; Remove deletes its row and stays on the
/// Remove buttons while there are rows.
pub fn press<S: Service>(app: &mut App<S>, field: Field, now: Instant) {
  let mut drafts = to_drafts(&app.config.topics.subscriptions);
  match field {
    Field::AddSubscription => {
      drafts.push(Draft {
        filter: "#".to_owned(),
        absolute: false,
      });
      app.settings.focus_field(Field::SubscriptionFilter(drafts.len() - 1));
    }
    Field::RemoveSubscription(index) if index < drafts.len() => {
      drafts.remove(index);
      app.settings.focus_field(match drafts.len() {
        0 => Field::AddSubscription,
        rows => Field::RemoveSubscription(index.min(rows - 1)),
      });
    }
    _ => return,
  }
  app.edit_config(now, |config| config.topics.subscriptions = from_drafts(&drafts));
}

#[cfg(test)]
mod tests {
  use super::*;

  fn draft(filter: &str, absolute: bool) -> Draft {
    Draft {
      filter: filter.to_owned(),
      absolute,
    }
  }

  #[test]
  fn both_shapes_are_read_and_written_back_as_the_gui_writes_them() {
    let read = to_drafts(&[
      Subscription::Relative("#".to_owned()),
      Subscription::Explicit {
        filter: "$SYS/#".to_owned(),
        absolute: true,
      },
    ]);
    assert_eq!(read, vec![draft("#", false), draft("$SYS/#", true)]);
    assert_eq!(
      serde_json::to_value(from_drafts(&read)).unwrap(),
      serde_json::json!(["#", { "filter": "$SYS/#", "absolute": true }])
    );
    assert_eq!(
      serde_json::to_value(from_drafts(&[draft("  ", false), draft("info", false)])).unwrap(),
      serde_json::json!(["", "info"]),
      "a blank row stays editable"
    );
  }
}
