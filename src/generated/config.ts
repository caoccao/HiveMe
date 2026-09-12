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

// Generated from schemas/config.schema.json by scripts/ts/gen-types.ts.
// Do not edit; run `pnpm gen:types` instead.

/**
 * Where a secret really lives.
 */
export type SecretRef =
  | {
      name: string;
      type: 'Env';
    }
  | {
      account: string;
      service: string;
      type: 'Keychain';
    };
/**
 * One entry of `topics.subscriptions`.
 *
 * A bare string is relative to the prefix, unless it starts with `$`, which the MQTT
 * specification reserves for broker topics.
 */
export type Subscription =
  | string
  | {
      absolute?: boolean;
      filter: string;
    };

/**
 * The config shared by hmc and hmg. Generated from hiveme-core; see docs/specs/config.md.
 */
export interface HiveMeConfig {
  broker?: Broker;
  /**
   * The HiveMQ Cloud REST API, which needs the Starter plan. Phase 6.
   */
  cloudApi?: CloudApi | null;
  device?: Device;
  encryption?: Encryption;
  gui?: Gui;
  notifications?: Notifications;
  publish?: Publish;
  topics?: Topics;
  update?: Update;
  /**
   * The config schema version. The loader migrates older versions.
   */
  version?: number;
}
/**
 * How to reach the MQTT broker.
 */
export interface Broker {
  /**
   * The first segment of the MQTT client identifier.
   */
  clientIdPrefix?: string;
  connectTimeoutSecs?: number;
  keepAliveSecs?: number;
  /**
   * Stored in plain text. The file is created with mode 0600 on Unix.
   */
  password?: string;
  /**
   * Where to read the password instead of the field above.
   */
  passwordRef?: SecretRef | null;
  reconnect?: Reconnect;
  /**
   * `hmg` only. `hmc` always connects with a clean start and no session.
   */
  sessionExpirySecs?: number;
  tls?: Tls;
  /**
   * `host:8883` for HiveMQ Cloud, with or without the `mqtts://` in front of it: a URL
   * that names no scheme is read as TLS MQTT. See `docs/specs/hivemq-cloud.md`.
   */
  url?: string;
  username?: string;
}
/**
 * Exponential backoff for the `hmg` connection.
 */
export interface Reconnect {
  initialDelayMs?: number;
  maxDelayMs?: number;
}
/**
 * TLS settings for the broker connection.
 */
export interface Tls {
  /**
   * Extra PEM roots appended to the native trust store.
   */
  caFile?: string | null;
  /**
   * Honored only for hosts outside `hivemq.cloud`, and logs a warning when false.
   */
  verifyServer?: boolean;
}
/**
 * The HiveMQ Cloud REST API. Needs the Starter plan; phase 6.
 */
export interface CloudApi {
  /**
   * From the API Access tab, for example `https://api.a01.euc1.aws.hivemq.cloud`.
   */
  baseUrl?: string;
  /**
   * The cluster UUID.
   */
  clusterId?: string;
  /**
   * Six alphanumeric characters.
   */
  orgId?: string;
  /**
   * The bearer token, shown once at creation time.
   */
  token?: string;
  /**
   * Where to read the token instead of the field above.
   */
  tokenRef?: SecretRef | null;
}
/**
 * This installation's identity.
 */
export interface Device {
  /**
   * A stable identifier for this installation, used as `sender.id`.
   */
  id?: string;
  /**
   * The human readable name shown in the GUI.
   */
  name?: string;
}
/**
 * Message encryption. Designed in `docs/specs/message.md`, implemented in phase 6.
 */
export interface Encryption {
  keys?: KeyEntry[];
  /**
   * Whether messages are encrypted.
   */
  mode?: 'Off' | 'Opportunistic' | 'Required';
}
/**
 * One pre-shared encryption key.
 */
export interface KeyEntry {
  /**
   * The AEAD algorithm, `A256GCM` today.
   */
  alg?: string;
  /**
   * The date the key was created, as `YYYY-MM-DD`.
   */
  createdAt?: string;
  /**
   * The identifier written into `enc.kid`.
   */
  kid?: string;
  /**
   * 32 random bytes, base64url encoded.
   */
  secret?: string;
  /**
   * Whether a key is still used for sending.
   */
  state?: 'Active' | 'Retired';
}
/**
 * Settings that only `hmg` reads.
 */
export interface Gui {
  /**
   * Which color scheme the GUI follows.
   */
  displayMode?: ('Light' | 'Dark') | 'Auto';
  history?: History;
  /**
   * A BCP 47 tag. The GUI supports de, en-US, es, fr, it, ja, zh-CN, zh-HK, and zh-TW.
   */
  language?: string;
  /**
   * The GUI palette.
   */
  theme?:
    | 'Ocean'
    | 'Aqua'
    | 'Sky'
    | 'Arctic'
    | 'Glacier'
    | 'Mist'
    | 'Slate'
    | 'Charcoal'
    | 'Midnight'
    | 'Indigo'
    | 'Violet'
    | 'Lavender'
    | 'Rose'
    | 'Blush'
    | 'Coral'
    | 'Sunset'
    | 'Amber'
    | 'Sand'
    | 'Forest'
    | 'Emerald';
  window?: Window;
}
/**
 * How much message history to keep.
 */
export interface History {
  /**
   * Older rows beyond this count are deleted per topic. 0 keeps everything.
   */
  maxMessagesPerTopic?: number;
  /**
   * Older rows than this are deleted. 0 disables time based pruning.
   */
  retentionDays?: number;
}
/**
 * The remembered main window geometry.
 */
export interface Window {
  position?: WindowPosition;
  size?: WindowSize;
}
/**
 * The remembered window position. Negative means "center the window".
 */
export interface WindowPosition {
  x?: number;
  y?: number;
}
/**
 * The remembered window size. The GUI clamps it to 600 by 450.
 */
export interface WindowSize {
  height?: number;
  width?: number;
}
/**
 * The rule based OS notification system.
 */
export interface Notifications {
  enabled?: boolean;
  /**
   * When false, a message this installation sent never raises a notification.
   */
  notifyOwnMessages?: boolean;
  /**
   * Absent means the three built-in rules. Present, even empty, replaces them.
   */
  rules?: Rule[];
}
/**
 * One notification rule.
 */
export interface Rule {
  absolute?: boolean;
  body?: string;
  enabled?: boolean;
  /**
   * Unique within the config. Reusing a built-in id overrides that rule.
   */
  id: string;
  /**
   * The notification level, and the level `hmc` infers for a matching topic.
   */
  level?: string;
  /**
   * Reserved for payload matching. Not implemented.
   */
  match?: {
    [k: string]: unknown;
  } | null;
  title?: string;
  /**
   * An MQTT topic filter, relative to `topics.prefix` unless `absolute` is true.
   */
  topic: string;
}
/**
 * Publishing defaults.
 */
export interface Publish {
  /**
   * 0, 1, or 2.
   */
  qos?: number;
  retain?: boolean;
  /**
   * How long `hmc` waits for the acknowledgement.
   */
  timeoutSecs?: number;
}
/**
 * The topic namespace.
 */
export interface Topics {
  /**
   * Where `hmc` publishes when no topic is given, relative to the prefix.
   */
  default?: string;
  /**
   * Prepended to every relative topic. May be empty, which puts topics at the root.
   */
  prefix?: string;
  /**
   * What `hmg` subscribes to, relative to the prefix.
   */
  subscriptions?: Subscription[];
}
/**
 * The GitHub release check.
 */
export interface Update {
  /**
   * How often to look for a new release.
   */
  checkInterval?: 'Daily' | 'Weekly' | 'Monthly';
  /**
   * A version the user chose to skip.
   */
  ignoreVersion?: string;
  /**
   * Unix seconds of the last successful check.
   */
  lastChecked?: number;
  /**
   * The latest version seen on GitHub.
   */
  lastVersion?: string;
}
