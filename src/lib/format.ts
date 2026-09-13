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

// The small conversions the views share: times, sizes, and the hex preview of a
// payload that is not text.

import i18n from '../i18n';

/** Format UI numbers using the selected application language. */
export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  return new Intl.NumberFormat(i18n.resolvedLanguage, options).format(value);
}

/** The clock time of an RFC 3339 timestamp, in the user's locale. */
export function formatTime(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return timestamp;
  }
  return date.toLocaleTimeString(i18n.resolvedLanguage, { hour: 'numeric', minute: '2-digit' });
}

/** The date and time of an RFC 3339 timestamp, for a tooltip. */
export function formatDateTime(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return timestamp;
  }
  return date.toLocaleString(i18n.resolvedLanguage);
}

/** The day an RFC 3339 timestamp falls on, used as the separator between bubbles. */
export function formatDay(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return timestamp;
  }
  return date.toLocaleDateString(i18n.resolvedLanguage, { year: 'numeric', month: 'short', day: 'numeric' });
}

/** A byte count in the largest unit that keeps it readable. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    bytes = 0;
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : 1;
  const rounded = formatNumber(value, { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return `${rounded} ${i18n.t(`format.units.${units[unit]}`)}`;
}

/** A duration in milliseconds, as the reconnect countdown shows it. */
export function formatDuration(milliseconds: number): string {
  const seconds = Math.max(0, Math.ceil(milliseconds / 1000));
  if (seconds < 60) {
    return i18n.t('format.seconds', { seconds: formatNumber(seconds) });
  }
  const minutes = Math.floor(seconds / 60);
  return i18n.t('format.minutesSeconds', {
    minutes: formatNumber(minutes),
    seconds: formatNumber(seconds % 60, { minimumIntegerDigits: 2 }),
  });
}

/** Groups a hex payload into bytes, so that it can be read and wrapped. */
export function formatHex(hex: string, groupsPerLine = 16): string {
  const pairs = hex.match(/.{1,2}/g) ?? [];
  const lines: string[] = [];
  for (let index = 0; index < pairs.length; index += groupsPerLine) {
    lines.push(pairs.slice(index, index + groupsPerLine).join(' '));
  }
  return lines.join('\n');
}

/** Shortens a long topic for a tab label or a tooltip-free chip. */
export function shrinkTopic(topic: string, maximum = 40): string {
  if (topic.length <= maximum) {
    return topic;
  }
  return `…${topic.slice(topic.length - maximum + 1)}`;
}
