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

export const APP_NAME = 'HiveMe';
export const AUTHOR_NAME = 'Sam Cao';
export const AUTHOR_URL = 'https://github.com/caoccao';
export const GITHUB_URL = 'https://github.com/caoccao/HiveMe';
export const RELEASES_URL = 'https://github.com/caoccao/HiveMe/releases';
export const HIVEMQ_CONSOLE_URL = 'https://console.hivemq.cloud/';

/** How many messages one page of history holds. Mirrors `DEFAULT_PAGE_SIZE` in the core. */
export const MESSAGE_PAGE_SIZE = 200;

/** Where the draggable divider of the Messages tab remembers its position. */
export const SPLIT_STORAGE_KEY = 'hiveme.messages.split';

/** The narrowest the topic tree may be dragged, as a percentage of the tab. */
export const SPLIT_MIN_PERCENT = 15;

/** The widest it may be dragged. */
export const SPLIT_MAX_PERCENT = 60;

/** Where the divider sits before the user has moved it. */
export const SPLIT_DEFAULT_PERCENT = 28;

/** The topic hmg always opens so a new installation is ready to compose. */
export const STARTUP_TOPIC = 'hiveme';
