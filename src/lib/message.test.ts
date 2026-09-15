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

// The TypeScript reader against the same fixtures the Rust reader uses. The tiers in
// docs/specs/message.md are what both applications and the GUI agree on, so a fixture
// that parses to one tier in Rust has to parse to the same tier here.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  displayedLevel,
  isEncrypted,
  isKnownLevel,
  isNewerVersion,
  parseRow,
  parseText,
  payloadOf,
  previewOf,
  senderLabel,
  senderOf,
} from './message';
import { Level, Tier } from './protocol';

// Vitest runs from the repository root, which is where its config lives.
const FIXTURES = join(process.cwd(), 'crates', 'hiveme-core', 'tests', 'fixtures', 'message');

function fixture(name: string): string {
  return readFileSync(join(FIXTURES, name), 'utf8');
}

describe('the parse tiers', () => {
  it.each([
    ['unknown_fields.json', Tier.Envelope],
    ['minimal.json', Tier.Envelope],
    ['unknown_level.json', Tier.Envelope],
    ['unknown_alg.json', Tier.Envelope],
    ['newer_version.json', Tier.Envelope],
    ['raw_json.json', Tier.Json],
    ['raw_text.txt', Tier.Text],
  ])('reads %s as the %s tier', (name, tier) => {
    expect(parseText(fixture(name)).tier).toBe(tier);
  });

  it('keeps unknown keys rather than rejecting them', () => {
    const parsed = parseText(fixture('unknown_fields.json'));
    expect(parsed.tier).toBe(Tier.Envelope);
    if (parsed.tier !== Tier.Envelope) {
      return;
    }
    expect((parsed.message as Record<string, unknown>).futureHeaderKey).toEqual({ nested: true });
    expect(payloadOf(parsed.message)?.body).toBe('still readable');
    expect(senderLabel(senderOf(parsed.message))).toBe('sams-macbook');
  });

  it('fills in the documented defaults for a message with only the required keys', () => {
    const parsed = parseText(fixture('minimal.json'));
    expect(parsed.tier).toBe(Tier.Envelope);
    if (parsed.tier !== Tier.Envelope) {
      return;
    }
    expect(payloadOf(parsed.message)?.title).toBeUndefined();
    expect(displayedLevel(payloadOf(parsed.message)?.level)).toBe(Level.Info);
    expect(isNewerVersion(parsed.message)).toBe(false);
  });

  it('recognizes success as its own level', () => {
    const parsed = parseText(fixture('success.json'));
    expect(parsed.tier).toBe(Tier.Envelope);
    if (parsed.tier !== Tier.Envelope) return;
    const level = payloadOf(parsed.message)?.level;
    expect(level).toBe('success');
    expect(isKnownLevel(level)).toBe(true);
    expect(displayedLevel(level)).toBe(Level.Success);
  });

  it.each(['INFO', 'Error', 'SuCcEsS', 'WARN'])(
    'normalizes %s before choosing its display label and color',
    (level) => {
      const message = JSON.parse(fixture('success.json'));
      message.payload.level = level;
      const parsed = parseText(JSON.stringify(message));
      expect(parsed.tier).toBe(Tier.Envelope);
      if (parsed.tier !== Tier.Envelope) return;
      expect(payloadOf(parsed.message)?.level).toBe(level.toLowerCase());
      expect(displayedLevel(level)).toBe(level.toLowerCase());
      expect(isKnownLevel(level)).toBe(true);
    }
  );

  it.each(['debug', 'DEBUG', 'DeBuG'])('treats %s as an unknown level with the Info fallback', (level) => {
    expect(isKnownLevel(level)).toBe(false);
    expect(displayedLevel(level)).toBe(Level.Info);
  });

  it('displays a level it does not know as info while keeping the raw value', () => {
    const parsed = parseText(fixture('unknown_level.json'));
    expect(parsed.tier).toBe(Tier.Envelope);
    if (parsed.tier !== Tier.Envelope) {
      return;
    }
    const level = payloadOf(parsed.message)?.level;
    expect(level).toBe('catastrophe');
    expect(isKnownLevel(level)).toBe(false);
    expect(displayedLevel(level)).toBe(Level.Info);
  });

  it('recognizes an encrypted envelope whose algorithm it does not know', () => {
    const parsed = parseText(fixture('unknown_alg.json'));
    expect(parsed.tier).toBe(Tier.Envelope);
    if (parsed.tier !== Tier.Envelope) {
      return;
    }
    expect(isEncrypted(parsed.message)).toBe(true);
    expect(payloadOf(parsed.message)).toBeNull();
    expect(previewOf(parsed)).toBe('encrypted (key k-2027-01)');
  });

  it('flags a newer version rather than dropping the message', () => {
    const parsed = parseText(fixture('newer_version.json'));
    expect(parsed.tier).toBe(Tier.Envelope);
    if (parsed.tier !== Tier.Envelope) {
      return;
    }
    expect(isNewerVersion(parsed.message)).toBe(true);
    expect(payloadOf(parsed.message)?.body).toBe('written by a future HiveMe');
  });

  it('shows JSON that is not an envelope as JSON', () => {
    const parsed = parseText(fixture('raw_json.json'));
    expect(parsed.tier).toBe(Tier.Json);
    if (parsed.tier !== Tier.Json) {
      return;
    }
    expect(parsed.value).toEqual({ temperature: 21.5, unit: 'C', sensor: { id: 'kitchen' } });
  });

  it('shows text that is not JSON as text', () => {
    const parsed = parseText(fixture('raw_text.txt'));
    expect(parsed.tier).toBe(Tier.Text);
    if (parsed.tier !== Tier.Text) {
      return;
    }
    expect(parsed.text.trim()).toBe('plain text from some other tool');
  });
});

describe('a document that claims to be an envelope but is not one', () => {
  it.each([
    ['no id', '{"v":1,"ts":"2026-09-12T09:41:23.512Z","payload":{"body":"x"}}'],
    ['enc without ciphertext', '{"v":1,"id":"a","ts":"b","enc":{"alg":"A256GCM","kid":"k","iv":"i"}}'],
    ['ciphertext without enc', '{"v":1,"id":"a","ts":"b","payload":{"body":"x"},"ciphertext":"c"}'],
  ])('shows %s as raw JSON rather than dropping it', (_name, text) => {
    expect(parseText(text).tier).toBe(Tier.Json);
  });

  it('is not an envelope without a payload or an enc header', () => {
    expect(parseText('{"v":1,"id":"a","ts":"b"}').tier).toBe(Tier.Json);
  });
});

describe('a stored row', () => {
  it('reads the bytes tier from the hex the backend sent', () => {
    const parsed = parseRow({ tier: Tier.Bytes, raw: 'fffe00', rawLength: 3 });
    expect(parsed.tier).toBe(Tier.Bytes);
    if (parsed.tier !== Tier.Bytes) {
      return;
    }
    expect(parsed.hex).toBe('fffe00');
    expect(previewOf(parsed)).toBe('3 bytes');
  });

  it('reads every other tier from the payload text', () => {
    expect(parseRow({ tier: Tier.Text, raw: 'hello', rawLength: 5 }).tier).toBe(Tier.Text);
    expect(parseRow({ tier: Tier.Json, raw: '{"a":1}', rawLength: 7 }).tier).toBe(Tier.Json);
  });
});

describe('previews', () => {
  it('truncates a long JSON payload the way a notification body does', () => {
    const long = JSON.stringify({ text: 'x'.repeat(500) });
    const preview = previewOf(parseText(long));
    expect(Array.from(preview)).toHaveLength(201);
    expect(preview.endsWith('…')).toBe(true);
  });

  it('uses the body of an envelope', () => {
    expect(previewOf(parseText(fixture('minimal.json')))).toBe('only the required keys');
  });
});

it('rejects malformed envelope field shapes and trusts the stored tier', () => {
  const base = { v: 1, id: 'x', ts: 'now', payload: { body: 'ok' } };
  for (const change of [
    { payload: 'not an object' },
    { payload: {} },
    { payload: { body: 1 } },
    { payload: { body: 'ok', level: 5 } },
    { enc: {}, ciphertext: 'x' },
    { enc: { alg: 'AES', kid: 'k', iv: 2 }, ciphertext: 'x' },
    { sender: 1 },
    { ttlSecs: -1 },
    { type: null },
    { v: 4294967296 },
  ])
    expect(parseText(JSON.stringify({ ...base, ...change })).tier).toBe('json');
  const raw = '{"v":1.0,"id":"x","ts":"now","payload":{"body":"ok"}}';
  expect(parseRow({ tier: 'json', raw, rawLength: raw.length }).tier).toBe('json');
  expect(parseRow({ tier: 'text', raw, rawLength: raw.length })).toEqual({ tier: 'text', text: raw });
});
