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

//! Advanced: the groups of the phases that are specified but not built yet, as text.

use hiveme_core::i18n::t;

use super::{Form, Row};
use crate::tui::app::App;
use crate::tui::service::Service;

pub fn form<S: Service>(app: &App<S>) -> Form {
  let locale = app.locale;
  Form {
    rows: vec![
      Row::note(t(locale, "settings.advancedHint")),
      Row::Blank,
      Row::heading(t(locale, "settings.encryption")),
      Row::note(t(locale, "settings.notImplemented")),
      Row::Blank,
      Row::heading(t(locale, "settings.cloudApi")),
      Row::note(t(locale, "settings.notImplemented")),
    ],
  }
}
