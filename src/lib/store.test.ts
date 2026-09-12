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

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import i18n from '../i18n';
import type { Config } from './protocol';
import * as Service from './service';
import { INITIAL_STATUS, useAppStore } from './store';

vi.mock('./service', () => ({
  setConfig: vi.fn(async (config: Config) => config),
  getStatus: vi.fn(async () => INITIAL_STATUS),
}));

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  useAppStore.setState({
    config: { version: 1, broker: { username: 'initial' }, gui: { language: 'en-US' } },
    dialogNotification: null,
  });
});

afterEach(async () => {
  await useAppStore.getState().flushConfig();
  vi.useRealTimers();
});

describe('automatic settings saves', () => {
  it('applies every edit immediately but writes once after the last 500 ms pause', async () => {
    const store = useAppStore.getState();
    store.updateConfig((config) => { config.broker!.username = 'first'; });
    await vi.advanceTimersByTimeAsync(400);
    store.updateConfig((config) => { config.gui!.language = 'de'; });
    expect(i18n.resolvedLanguage).toBe('de');
    expect(useAppStore.getState().config?.broker?.username).toBe('first');
    await vi.advanceTimersByTimeAsync(499);
    expect(Service.setConfig).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(Service.setConfig).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
      broker: { username: 'first' }, gui: { language: 'de' },
    }));
    expect(useAppStore.getState().dialogNotification).toBeNull();
  });

  it('serializes writes and never replaces a newer edit with an older response', async () => {
    let finishFirst!: (config: Config) => void;
    let finishSecond!: (config: Config) => void;
    vi.mocked(Service.setConfig)
      .mockImplementationOnce(() => new Promise((resolve) => { finishFirst = resolve; }))
      .mockImplementationOnce(() => new Promise((resolve) => { finishSecond = resolve; }));

    const store = useAppStore.getState();
    store.updateConfig((config) => { config.gui!.language = 'de'; });
    const firstSnapshot = useAppStore.getState().config!;
    const saving = store.flushConfig();

    store.updateConfig((config) => { config.gui!.language = 'ja'; });
    store.updateConfig((config) => { config.broker!.username = 'latest'; });
    await vi.advanceTimersByTimeAsync(500);
    expect(Service.setConfig).toHaveBeenCalledTimes(1);

    finishFirst(firstSnapshot);
    await vi.advanceTimersByTimeAsync(0);
    expect(Service.setConfig).toHaveBeenCalledTimes(2);
    expect(i18n.resolvedLanguage).toBe('ja');
    expect(useAppStore.getState().config?.gui?.language).toBe('ja');
    expect(useAppStore.getState().config?.broker?.username).toBe('latest');

    // A caller such as Copy CLI setup must wait for the entire queue.
    const flushing = store.flushConfig();
    let done = false;
    void flushing.then(() => { done = true; });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);
    const latest = useAppStore.getState().config!;
    finishSecond({ ...latest, device: { name: 'normalized' } });
    expect(await saving).toBe(true);
    expect(await flushing).toBe(true);
    expect(useAppStore.getState().config?.device?.name).toBe('normalized');
    expect(i18n.resolvedLanguage).toBe('ja');
    expect(Service.setConfig).toHaveBeenCalledTimes(2);
  });

  it('keeps failed edits visible and saves the whole configuration on the next edit', async () => {
    vi.mocked(Service.setConfig).mockRejectedValueOnce(new Error('Disk is read-only'));
    const store = useAppStore.getState();
    store.updateConfig((config) => { config.gui!.language = 'fr'; });
    expect(await store.flushConfig()).toBe(false);
    expect(useAppStore.getState().config?.gui?.language).toBe('fr');
    expect(i18n.resolvedLanguage).toBe('fr');
    expect(useAppStore.getState().dialogNotification?.title).toBe('Disk is read-only');
    expect(await store.flushConfig()).toBe(false);

    store.updateConfig((config) => { config.broker!.username = 'corrected'; });
    expect(await store.flushConfig()).toBe(true);
    expect(Service.setConfig).toHaveBeenLastCalledWith(expect.objectContaining({
      broker: { username: 'corrected' }, gui: { language: 'fr' },
    }));
  });
});
