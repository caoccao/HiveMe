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

// The reader the chat view renders with: the TypeScript twin of the parse tiers in
// `hiveme_core::message`, specified in docs/specs/message.md.
//
// It is for display only. The backend has already read the payload, stored it, and
// decided which tier it is; this reader turns the stored text back into something the
// bubbles can show, and its tiers are tested against the same fixtures the Rust
// reader uses so that the two cannot disagree.

import type { Message, Payload, Sender } from './protocol';
import { Level, Tier } from './protocol';
import i18n from '../i18n';

/** The envelope version this build fully understands. */
export const ENVELOPE_VERSION = 1;

/** How much of a raw JSON payload a one line preview shows. */
export const RAW_JSON_PREVIEW_LIMIT = 200;

/** What a reader made of a payload. */
export type Parsed =
  | { tier: Tier.Envelope; message: Message }
  | { tier: Tier.Json; value: unknown }
  | { tier: Tier.Text; text: string }
  | { tier: Tier.Bytes; hex: string; length: number };

/** An object with an integer `v` and either `payload` or `enc` claims to be an envelope. */
function looksLikeEnvelope(value: unknown): boolean {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  const version = candidate.v;
  if (typeof version !== 'number' || !Number.isInteger(version) || version < 0 || version > 0xffffffff) {
    return false;
  }
  return 'payload' in candidate || 'enc' in candidate;
}

/** The combinations docs/specs/message.md forbids. Mirrors `Message::validate`. */
function isValidEnvelope(candidate: Record<string, unknown>): boolean {
  const hasPayload = candidate.payload !== undefined && candidate.payload !== null;
  const hasEnc = candidate.enc !== undefined && candidate.enc !== null;
  const hasCiphertext = candidate.ciphertext !== undefined && candidate.ciphertext !== null;
  if (typeof candidate.id !== 'string' || typeof candidate.ts !== 'string') {
    return false;
  }
  if (hasEnc && !hasCiphertext) {
    return false;
  }
  if (hasCiphertext && !hasEnc) {
    return false;
  }
  const object = (value: unknown): value is Record<string, unknown> =>
    typeof value === 'object' && value !== null && !Array.isArray(value);
  const optionalString = (value: unknown) => value == null || typeof value === 'string';
  const uint32 = (value: unknown) =>
    typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= 0xffffffff;
  if (
    hasPayload &&
    (!object(candidate.payload) ||
      typeof candidate.payload.body !== 'string' ||
      !['title', 'level', 'contentType'].every((key) =>
        optionalString(candidate.payload && (candidate.payload as Record<string, unknown>)[key])
      ))
  )
    return false;
  if (
    hasEnc &&
    (!object(candidate.enc) ||
      !['alg', 'kid', 'iv'].every((key) => typeof (candidate.enc as Record<string, unknown>)[key] === 'string'))
  )
    return false;
  if (!optionalString(candidate.ciphertext) || !optionalString(candidate.replyTo)) return false;
  if (candidate.type !== undefined && typeof candidate.type !== 'string') return false;
  if (candidate.ttlSecs != null && !uint32(candidate.ttlSecs)) return false;
  if (
    candidate.sender != null &&
    (!object(candidate.sender) ||
      !['id', 'name', 'app', 'appVersion'].every((key) =>
        optionalString((candidate.sender as Record<string, unknown>)[key])
      ))
  )
    return false;
  return hasPayload || hasEnc;
}

/**
 * Reads a payload that is text, degrading through the tiers rather than failing.
 *
 * A document that claims to be an envelope but is malformed is shown as raw JSON, the
 * same way the Rust reader shows it: nothing is ever discarded.
 */
export function parseText(text: string): Parsed {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return { tier: Tier.Text, text };
  }
  if (looksLikeEnvelope(value) && isValidEnvelope(value as Record<string, unknown>)) {
    const message = value as Message;
    const payload = payloadOf(message);
    if (typeof payload?.level === 'string') payload.level = payload.level.toLowerCase();
    return { tier: Tier.Envelope, message };
  }
  return { tier: Tier.Json, value };
}

/**
 * Reads a stored row.
 *
 * The `bytes` tier never reaches the frontend as bytes: the backend hands over a hex
 * string, because a payload that is not valid UTF-8 cannot travel as JSON text.
 */
export function parseRow(row: { tier: string; raw: string; rawLength: number }): Parsed {
  if (row.tier === Tier.Bytes) {
    return { tier: Tier.Bytes, hex: row.raw, length: row.rawLength };
  }
  if (row.tier === Tier.Text) return { tier: Tier.Text, text: row.raw };
  if (row.tier === Tier.Json) {
    try {
      return { tier: Tier.Json, value: JSON.parse(row.raw) };
    } catch {
      return { tier: Tier.Text, text: row.raw };
    }
  }
  return parseText(row.raw);
}

/** The envelope, when the payload was one. */
export function envelopeOf(parsed: Parsed): Message | null {
  return parsed.tier === Tier.Envelope ? parsed.message : null;
}

/** The readable content of an envelope, when it is not encrypted. */
export function payloadOf(message: Message): Payload | null {
  const payload = (message as { payload?: Payload | null }).payload;
  return payload ?? null;
}

/** Who sent a message, when it says. */
export function senderOf(message: Message): Sender | null {
  return message.sender ?? null;
}

/** Whether the content is encrypted, which the view shows as a lock. */
export function isEncrypted(message: Message): boolean {
  const enc = (message as { enc?: unknown }).enc;
  return enc !== undefined && enc !== null;
}

/** Whether the writer used an envelope version newer than this build understands. */
export function isNewerVersion(message: Message): boolean {
  return typeof message.v === 'number' && message.v > ENVELOPE_VERSION;
}

/** The name to show for a sender: its name, then its id. The app is not a sender label. */
export function senderLabel(sender: Sender | null): string {
  if (!sender) {
    return '';
  }
  return sender.name || sender.id || '';
}

/** The level a reader displays. A level this build does not know displays as `info`. */
export function displayedLevel(level: string | null | undefined): Level {
  const normalized = level?.toLowerCase();
  switch (normalized) {
    case Level.Info:
    case Level.Success:
    case Level.Warn:
    case Level.Error:
      return normalized;
    default:
      return Level.Info;
  }
}

/** Whether this build knows what a level means. */
export function isKnownLevel(level: string | null | undefined): boolean {
  const normalized = level?.toLowerCase();
  return (
    normalized === Level.Info || normalized === Level.Success || normalized === Level.Warn || normalized === Level.Error
  );
}

/** The MUI palette used for message bubbles, badges, and level selections. */
export function levelColor(level: Level): 'default' | 'success' | 'warning' | 'error' {
  switch (level) {
    case Level.Success:
      return 'success';
    case Level.Warn:
      return 'warning';
    case Level.Error:
      return 'error';
    default:
      return 'default';
  }
}

/** The one line preview a list or a notification shows for a payload. */
export function previewOf(parsed: Parsed): string {
  switch (parsed.tier) {
    case Tier.Envelope: {
      const enc = (parsed.message as { enc?: { kid?: string } }).enc;
      if (enc) {
        return i18n.t('messages.encrypted', { kid: enc.kid ?? '' });
      }
      const payload = payloadOf(parsed.message);
      return payload?.body ?? '';
    }
    case Tier.Json:
      return truncate(JSON.stringify(parsed.value) ?? '', RAW_JSON_PREVIEW_LIMIT);
    case Tier.Text:
      return parsed.text;
    case Tier.Bytes:
      return i18n.t('messages.bytes', { count: parsed.length });
  }
}

function truncate(text: string, limit: number): string {
  const characters = Array.from(text);
  if (characters.length <= limit) {
    return text;
  }
  return `${characters.slice(0, limit).join('')}…`;
}
