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

// Types that exist only in the frontend. Anything the backend also knows belongs in
// protocol.ts, which is hand synced with protocol.rs.

import type { DialogNotificationType } from './protocol';

/** One in-app message for the snackbar. */
export interface DialogNotification {
  title: string;
  type: DialogNotificationType;
}

/** One tab of the main content area. */
export interface TabControl {
  type: number;
  index: number;
  value: string | null;
}
