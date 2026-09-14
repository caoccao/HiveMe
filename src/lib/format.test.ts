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

import { describe, expect, it } from 'vitest';
import { formatBytes, formatDuration, formatHex, formatTime } from './format';

describe('formatBytes', () => {
  it('picks the largest unit that keeps the number readable', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(1024)).toBe('1.0 KB');
    expect(formatBytes(1536)).toBe('1.5 KB');
    expect(formatBytes(1024 * 1024 * 3)).toBe('3.0 MB');
  });

  it('never shows a negative or unreadable size', () => {
    expect(formatBytes(-1)).toBe('0 B');
    expect(formatBytes(Number.NaN)).toBe('0 B');
  });
});

describe('formatDuration', () => {
  it('rounds up so that a countdown never shows zero while it is still waiting', () => {
    expect(formatDuration(1)).toBe('1s');
    expect(formatDuration(1500)).toBe('2s');
    expect(formatDuration(0)).toBe('0s');
    expect(formatDuration(-100)).toBe('0s');
  });

  it('switches to minutes past a minute', () => {
    expect(formatDuration(65_000)).toBe('1m 05s');
    expect(formatDuration(600_000)).toBe('10m 00s');
  });
});

describe('formatHex', () => {
  it('groups the bytes and wraps them', () => {
    expect(formatHex('fffe00')).toBe('ff fe 00');
    expect(formatHex('00112233', 2)).toBe('00 11\n22 33');
  });

  it('has nothing to show for an empty payload', () => {
    expect(formatHex('')).toBe('');
  });
});

describe('formatTime', () => {
  it('hands back anything it cannot read, rather than showing "Invalid Date"', () => {
    expect(formatTime('not a timestamp')).toBe('not a timestamp');
  });
});
