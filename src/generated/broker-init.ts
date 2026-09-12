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
 * Everything `hmc` needs to reach the cluster `hmg` is already talking to.
 */
export interface HiveMeBrokerSetupString {
  /**
   * Plain text, because the CONNECT packet needs it in plain text.
   */
  password?: string;
  /**
   * The topic namespace, carried so that `hmc` publishes where `hmg` is listening.
   *
   * Absent means the receiving config keeps the prefix it already has, which is what
   * a string written by a build that predates this field looks like.
   */
  prefix?: string | null;
  /**
   * `mqtts://<cluster>.s1.eu.hivemq.cloud:8883` for a HiveMQ Cloud cluster.
   */
  url?: string;
  username?: string;
  /**
   * The format version. See [`BROKER_INIT_VERSION`].
   */
  v?: number;
}
