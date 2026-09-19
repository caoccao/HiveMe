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

import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';
import i18n, { changeLanguage, resolveLanguage } from './index';
import enUS from '../../locales/en-US.json';
import { LANGUAGES, LEVELS, THEMES, ConnectionState, UpdateCheckInterval } from '../lib/protocol';
import { BrokerProtocol } from '../lib/brokerUrl';
import { formatBytes, formatDateTime, formatDay, formatDuration, formatTime } from '../lib/format';

function flatten(object: Record<string, unknown>, prefix = ''): Record<string, string> {
  return Object.fromEntries(
    Object.entries(object).flatMap(([key, value]) =>
      typeof value === 'string'
        ? [[prefix + key, value]]
        : Object.entries(flatten(value as Record<string, unknown>, prefix + key + '.'))
    )
  );
}

const pluralSuffix = /_(zero|one|two|few|many|other)$/;
const baseKey = (key: string) => key.replace(pluralSuffix, '');
const variables = (value: string) => (value.match(/{{[^}]+}}/g) ?? []).sort();
const source = flatten(enUS);
const sourceByBase = Object.fromEntries(Object.entries(source).map(([key, value]) => [baseKey(key), value]));

describe('the bundled catalogs', () => {
  it.each(LANGUAGES)(
    '%s covers every string and preserves interpolation variables without English fallback',
    (language) => {
      const catalog = flatten(i18n.getResourceBundle(language, 'translation'));
      expect([...new Set(Object.keys(catalog).map(baseKey))].sort()).toEqual(Object.keys(sourceByBase).sort());
      for (const [key, value] of Object.entries(catalog)) {
        expect(value.trim(), language + ': ' + key).not.toBe('');
        expect(variables(value), language + ': ' + key).toEqual(variables(sourceByBase[baseKey(key)]));
      }
      const categories = new Intl.PluralRules(language).resolvedOptions().pluralCategories;
      for (const key of Object.keys(source).filter((key) => key.endsWith('_other'))) {
        for (const category of categories) {
          expect(catalog[baseKey(key) + '_' + category], language + ': ' + key + ' / ' + category).toBeTruthy();
        }
      }
      const dynamicKeys = [
        ...LEVELS.map((level) => 'levels.' + level),
        ...THEMES.map((theme) => 'settings.themes.' + theme),
        ...Object.values(ConnectionState).map((state) => 'footer.state.' + state),
        ...Object.values(UpdateCheckInterval).map((interval) => 'settings.interval.' + interval),
        ...Object.values(BrokerProtocol).map((protocol) => 'settings.protocols.' + protocol),
      ];
      for (const key of dynamicKeys) expect(catalog[key], language + ': ' + key).toBeTruthy();
    }
  );

  it('renders singular, plural, zero, and grouped counts', async () => {
    await changeLanguage('en-US');
    expect(i18n.t('messages.bytes', { count: 1 })).toBe('1 byte');
    expect(i18n.t('footer.subscriptions', { count: 0 })).toBe('0 subscriptions');
    expect(i18n.t('footer.received', { count: 2 })).toBe('2 messages this session');
    await changeLanguage('de');
    expect(i18n.t('topics.unread', { count: 1000 })).toBe('1.000 ungelesene Nachrichten');
    await changeLanguage('fr');
    expect(i18n.t('messages.bytes', { count: 0 })).toBe('0 octet');
    expect(i18n.t('messages.bytes', { count: 2 })).toBe('2 octets');
    expect(i18n.t('messages.bytes', { count: 1_000_000 })).toBe('1\u202f000\u202f000 octets');
    await changeLanguage('ja');
    expect(i18n.t('messages.bytes', { count: 1 })).toBe('1 バイト');
    expect(i18n.t('messages.bytes', { count: 2 })).toBe('2 バイト');
  });
});

describe('language resolution and formatting', () => {
  it.each([
    ['de-DE', 'de'],
    ['es-MX', 'es'],
    ['fr-CA', 'fr'],
    ['it-CH', 'it'],
    ['ja-JP', 'ja'],
    ['en-GB', 'en-US'],
    ['zh', 'zh-CN'],
    ['zh-Hans-SG', 'zh-CN'],
    ['zh-Hant', 'zh-TW'],
    ['zh-Hant-HK', 'zh-HK'],
    ['zh-MO', 'zh-HK'],
    ['zh_TW', 'zh-TW'],
    [' DE-at ', 'de'],
    ['unsupported', 'en-US'],
    ['', 'en-US'],
    [undefined, 'en-US'],
  ])('resolves %s to %s', async (requested, expected) => {
    expect(resolveLanguage(requested)).toBe(expected);
    await changeLanguage(requested);
    expect(i18n.resolvedLanguage).toBe(expected);
    expect(document.documentElement.lang).toBe(expected);
  });

  it('uses the selected language for dates, decimal sizes, and countdowns', async () => {
    const timestamp = '2026-09-12T09:41:23Z';
    const date = new Date(timestamp);
    const nextDay = new Date(date);
    nextDay.setDate(nextDay.getDate() + 1);
    for (const language of LANGUAGES) {
      await changeLanguage(language);
      const time = date.toLocaleTimeString(language, { hour: 'numeric', minute: '2-digit' });
      expect(formatTime(timestamp, date)).toBe(time);
      expect(formatTime(timestamp, nextDay)).toBe(`${formatDay(timestamp)} ${time}`);
      expect(formatDateTime(timestamp)).toBe(date.toLocaleString(language));
      expect(formatDay(timestamp)).toBe(
        date.toLocaleDateString(language, {
          year: 'numeric',
          month: 'short',
          day: 'numeric',
        })
      );
    }
    await changeLanguage('de');
    expect(formatBytes(1536)).toBe('1,5 KB');
    expect(formatDuration(65_000)).toBe('1 min 05 s');
    await changeLanguage('fr');
    expect(formatBytes(1536)).toBe('1,5 Ko');
    await changeLanguage('ja');
    expect(formatDuration(65_000)).toBe('1分05秒');
    expect(formatDay('invalid')).toBe('invalid');
  });
});

/** Guard the audit against future hardcoded JSX labels and missing literal keys. */
describe('frontend string audit', () => {
  it('keeps UI prose in the catalogs', () => {
    const root = join(process.cwd(), 'src');
    const files = (directory: string): string[] =>
      readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) return ['generated', 'i18n', 'test'].includes(entry.name) ? [] : files(path);
        return /\.tsx?$/.test(entry.name) && !/\.test\.|\.d\.ts$/.test(entry.name) ? [path] : [];
      });
    // Product names, license identifiers, and example addresses are language invariant.
    const invariant = new Set(['Apache-2.0', 'caoccao.com', 'abc123.s1.eu.hivemq.cloud:8883']);
    const uiProps = new Set(['label', 'title', 'placeholder', 'helperText', 'aria-label', 'alt', 'closeText']);
    const failures: string[] = [];
    for (const file of files(root)) {
      const syntax = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
      const checkText = (value: string, node: ts.Node) => {
        const text = value.trim();
        if (/\p{L}/u.test(text) && !invariant.has(text)) {
          failures.push(file + ':' + (syntax.getLineAndCharacterOfPosition(node.getStart()).line + 1) + ': ' + text);
        }
      };
      const visit = (node: ts.Node) => {
        if (ts.isJsxText(node)) checkText(node.text, node);
        if (ts.isJsxAttribute(node) && uiProps.has(node.name.getText(syntax)) && node.initializer) {
          if (ts.isStringLiteral(node.initializer)) checkText(node.initializer.text, node);
          if (
            ts.isJsxExpression(node.initializer) &&
            node.initializer.expression &&
            ts.isStringLiteral(node.initializer.expression)
          )
            checkText(node.initializer.expression.text, node);
        }
        if (
          ts.isCallExpression(node) &&
          node.arguments[0] &&
          ts.isStringLiteral(node.arguments[0]) &&
          (node.expression.getText(syntax) === 't' || node.expression.getText(syntax) === 'i18n.t')
        ) {
          const key = node.arguments[0].text;
          if (!(key in sourceByBase)) failures.push(file + ': missing translation ' + key);
        }
        ts.forEachChild(node, visit);
      };
      visit(syntax);
    }
    expect(failures).toEqual([]);
  });
});
