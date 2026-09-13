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

// Generated from schemas/broker-init.schema.json by scripts/ts/gen-types.ts.
// Do not edit; run `pnpm gen:types` instead.

/**
 * Everything `hmc` needs to reach the cluster `hmg` is already talking to, and the
 * language to talk about it in.
 */
export interface HiveMeBrokerSetupString {
  /**
   * The `gui.language` of the application that copied the string, a BCP 47 tag.
   * Optional: a string without it leaves an existing language alone, and a new config
   * gets `en-US`.
   */
  language?: string | null;
  /**
   * Plain text, because the CONNECT packet needs it in plain text.
   */
  password?: string;
  /**
   * `<cluster>.s1.eu.hivemq.cloud:8883` for a HiveMQ Cloud cluster, as the console
   * shows it. A URL that names no scheme is read as TLS MQTT.
   */
  url?: string;
  username?: string;
  /**
   * The format version. See [`BROKER_INIT_VERSION`].
   */
  v?: number;
}
