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

// Generated from schemas/message.schema.json by scripts/ts/gen-types.ts.
// Do not edit; run `pnpm gen:types` instead.

/**
 * A HiveMe message envelope. Generated from hiveme-core; see docs/specs/message.md.
 */
export type HiveMeMessage = {
  /**
   * The base64url AEAD output. Present exactly when `enc` is.
   */
  ciphertext?: string | null;
  /**
   * The encryption header. Absent on a plaintext message.
   */
  enc?: Enc | null;
  /**
   * A unique identifier, UUID v7 for messages HiveMe writes.
   */
  id: string;
  /**
   * The readable content. Absent on an encrypted message.
   */
  payload?: Payload | null;
  /**
   * The `id` of another message. Reserved for threading.
   */
  replyTo?: string | null;
  sender?: Sender | null;
  /**
   * RFC 3339 with a timezone and millisecond precision, from the producer's clock.
   */
  ts: string;
  /**
   * Mirrors the MQTT message expiry so a reader can show staleness.
   */
  ttlSecs?: number | null;
  /**
   * An open enum; `message` unless stated otherwise.
   */
  type?: string;
  /**
   * The envelope major version.
   */
  v: number;
} & (Plaintext | Encrypted);
/**
 * Message severity, normalized to lowercase. An open enum: a value outside the examples is preserved in lowercase and displayed as `info`.
 */
export type Level = string;

/**
 * How a message was encrypted.
 *
 * Designed but not implemented; see `docs/specs/message.md`. Readers already
 * recognize the shape so that an encrypted message is displayed as a placeholder
 * rather than as unreadable JSON.
 */
export interface Enc {
  /**
   * The AEAD algorithm. An open enum; `A256GCM` is the first one.
   */
  alg: string;
  /**
   * The base64url nonce.
   */
  iv: string;
  /**
   * The identifier of the pre-shared key that encrypted this message.
   */
  kid: string;
}
/**
 * The readable content of a message.
 */
export interface Payload {
  /**
   * The main text. May be empty when `data` carries the content.
   */
  body: string;
  /**
   * A hint for `body`, such as `text/markdown`. Defaults to `text/plain`.
   */
  contentType?: string | null;
  /**
   * Free form JSON for scripts.
   */
  data?: {
    [k: string]: unknown;
  } | null;
  /**
   * The severity. Defaults to `info`.
   */
  level?: Level | null;
  /**
   * Used as the notification title when present.
   */
  title?: string | null;
}
/**
 * Who produced a message.
 */
export interface Sender {
  /**
   * The producing application, `hmc` or `hmg` for the HiveMe tools.
   */
  app?: string | null;
  /**
   * The version of the producing application.
   */
  appVersion?: string | null;
  /**
   * The `device.id` of the installation that produced the message.
   */
  id?: string | null;
  /**
   * The human readable device name.
   */
  name?: string | null;
}
/**
 * Carries a readable payload.
 */
export interface Plaintext {
  [k: string]: unknown;
}
/**
 * Carries a ciphertext and the header needed to decrypt it.
 */
export interface Encrypted {
  [k: string]: unknown;
}
