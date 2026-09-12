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

//! The names and links `hmg` shows and follows.

/// The product name, which is also the config directory and the window title.
pub const APP_NAME: &str = hiveme_core::APP_NAME;

/// The identifier `sender.app` carries for a message this GUI publishes.
pub const APP_ID: &str = "hmg";

/// The repository the update check and the About tab point at.
pub const GITHUB_URL: &str = "https://github.com/caoccao/HiveMe";

/// Where the update check reads the published releases.
pub const RELEASES_API_URL: &str = "https://api.github.com/repos/caoccao/HiveMe/releases";
