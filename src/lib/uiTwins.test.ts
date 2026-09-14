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

import { expect, it } from 'vitest';
import cases from '../../crates/hiveme-core/tests/fixtures/ui_twins.json';
import { getPaletteByTheme } from '../App';
import { cliSetupCommand } from '../components/Config';
import { relativeTopicOf } from '../components/MessageView';
import { Theme, Language, LANGUAGE_LABELS } from './protocol';

it('matches the terminal palettes, autonyms, shell commands, and relative topics', () => {
  expect(Object.keys(cases.palettes)).toHaveLength(Object.values(Theme).length);
  for (const [name, colors] of Object.entries(cases.palettes)) {
    const palette = getPaletteByTheme(name as Theme, 'light');
    expect([palette.primary.main, palette.secondary.main]).toEqual([colors.primary, colors.secondary]);
  }
  for (const [tag, name] of Object.entries(cases.languages)) expect(LANGUAGE_LABELS[tag as Language]).toBe(name);
  for (const item of cases.setup) expect(cliSetupCommand(item.json)).toBe(item.command);
  for (const item of cases.topics) expect(relativeTopicOf(item.topic, item.selected)).toBe(item.relative);
});
