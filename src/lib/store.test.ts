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
import type { Config, MessageRow, TopicNode } from './protocol';
import { MESSAGE_PAGE_SIZE } from './constants';
import * as Service from './service';
import { INITIAL_STATUS, useAppStore } from './store';

vi.mock('./service', () => ({
  setConfig: vi.fn(async (config: Config) => config),
  getStatus: vi.fn(async () => INITIAL_STATUS),
  getMessages: vi.fn(async () => [] as MessageRow[]),
  listTopics: vi.fn(async () => []),
  markRead: vi.fn(async () => undefined),
  minimizeToTray: vi.fn(async () => undefined),
  clearTopic: vi.fn(async () => 1),
}));

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  useAppStore.setState({
    config: { version: 1, broker: { username: 'initial' }, gui: { language: 'en-US' } },
    dialogNotification: null,
    selectedTopic: null,
    messages: new Map(),
    loadedTopics: new Set(),
    hasOlder: new Map(),
    loadingOlder: false,
  });
});

afterEach(async () => {
  await useAppStore.getState().flushConfig();
  vi.useRealTimers();
});

function message(rowId: number, topic: string, body = String(rowId)): MessageRow {
  return {
    rowId,
    topic,
    id: 'shared-envelope',
    body,
    raw: body,
    rawLength: body.length,
    ts: '2026-09-13T12:00:00Z',
    receivedTs: '2026-09-13T12:00:00Z',
    senderId: null,
    senderName: null,
    app: null,
    tier: 'text',
    level: null,
    title: null,
    qos: 1,
    retain: false,
    outgoing: false,
  };
}

describe('recursive topic history', () => {
  it('loads a subtree through the backend and marks that selection read', async () => {
    const rows = [message(1, 'hiveme'), message(2, 'hiveme/build/ci')];
    vi.mocked(Service.getMessages).mockResolvedValueOnce(rows);
    await useAppStore.getState().selectTopic('hiveme');
    expect(Service.getMessages).toHaveBeenCalledExactlyOnceWith('hiveme', null, MESSAGE_PAGE_SIZE);
    expect(Service.markRead).toHaveBeenCalledWith('hiveme');
    expect(useAppStore.getState().messages.get('hiveme')).toEqual(rows);
  });

  it('updates every cached ancestor with live messages and deduplicates only by row id', () => {
    const roots = ['hiveme', 'hiveme/build', 'hiveme/build/ci', 'hiveme/builder', 'HiveMe'];
    useAppStore.setState({ loadedTopics: new Set(roots), messages: new Map(roots.map((root) => [root, []])) });
    const receive = useAppStore.getState().receiveMessage;
    receive(message(2, 'hiveme/build/ci'));
    receive(message(1, 'hiveme/build'));
    receive(message(2, 'hiveme/build/ci', 'delivery updated'));
    receive(message(3, 'hiveme2'));
    const views = useAppStore.getState().messages;
    expect(views.get('hiveme')?.map((row) => row.rowId)).toEqual([1, 2]);
    expect(views.get('hiveme/build')?.map((row) => row.rowId)).toEqual([1, 2]);
    expect(views.get('hiveme/build/ci')).toEqual([message(2, 'hiveme/build/ci', 'delivery updated')]);
    expect(views.get('hiveme/builder')).toEqual([]);
    expect(views.get('HiveMe')).toEqual([]);
  });

  it('keeps live descendants that arrive while the first page is loading', async () => {
    let finish!: (rows: MessageRow[]) => void;
    vi.mocked(Service.getMessages).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    const selecting = useAppStore.getState().selectTopic('hiveme');
    useAppStore.getState().receiveMessage(message(2, 'hiveme/child', 'latest delivery'));
    useAppStore.getState().receiveMessage(message(3, 'hiveme/child/deep'));
    finish([message(1, 'hiveme'), message(2, 'hiveme/child')]);
    await selecting;
    expect(useAppStore.getState().messages.get('hiveme')).toEqual([
      message(1, 'hiveme'),
      message(2, 'hiveme/child', 'latest delivery'),
      message(3, 'hiveme/child/deep'),
    ]);
  });

  it('pages older descendants with one row cursor while retaining live updates', async () => {
    useAppStore.setState({
      loadedTopics: new Set(['hiveme']),
      messages: new Map([['hiveme', [message(5, 'hiveme/child'), message(6, 'hiveme')]]]),
      hasOlder: new Map([['hiveme', true]]),
    });
    let finish!: (rows: MessageRow[]) => void;
    vi.mocked(Service.getMessages).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    const paging = useAppStore.getState().loadOlderMessages('hiveme');
    useAppStore.getState().receiveMessage(message(7, 'hiveme/deep/child'));
    finish([message(1, 'hiveme/other'), message(4, 'hiveme')]);
    await paging;
    expect(Service.getMessages).toHaveBeenCalledWith('hiveme', 5, MESSAGE_PAGE_SIZE);
    expect(
      useAppStore
        .getState()
        .messages.get('hiveme')
        ?.map((row) => row.rowId)
    ).toEqual([1, 4, 5, 6, 7]);
    expect(useAppStore.getState().hasOlder.get('hiveme')).toBe(false);
  });

  it('ignores a stale page that finishes after clearing its topic', async () => {
    let finishOld!: (rows: MessageRow[]) => void;
    vi.mocked(Service.getMessages)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finishOld = resolve;
          })
      )
      .mockResolvedValueOnce([]);
    const selecting = useAppStore.getState().selectTopic('hiveme');
    await useAppStore.getState().clearSelectedTopic();
    finishOld([message(1, 'hiveme'), message(2, 'hiveme/child')]);
    await selecting;
    expect(useAppStore.getState().selectedTopic).toBe('hiveme');
    expect(useAppStore.getState().messages.get('hiveme')).toEqual([]);
  });

  it('forgets the cleared subtree, reloads its ancestors, and selects the nearest remaining topic', async () => {
    const root = message(1, 'hiveme');
    const ci = message(2, 'hiveme/build/ci');
    const deep = message(3, 'hiveme/build/ci/deep');
    const cd = message(4, 'hiveme/build/cd');
    useAppStore.setState({
      selectedTopic: 'hiveme/build/ci',
      loadedTopics: new Set(['hiveme', 'hiveme/build/ci', 'hiveme/build/ci/deep', 'hiveme/build/cd']),
      messages: new Map([
        ['hiveme', [root, ci, deep, cd]],
        ['hiveme/build/ci', [ci, deep]],
        ['hiveme/build/ci/deep', [deep]],
        ['hiveme/build/cd', [cd]],
      ]),
    });
    const node = (id: string, children: TopicNode[] = []): TopicNode => ({
      id,
      label: id.slice(id.lastIndexOf('/') + 1),
      topic: id,
      unread: 0,
      messages: 1,
      children,
    });
    vi.mocked(Service.listTopics).mockResolvedValueOnce([
      node('hiveme', [node('hiveme/build', [node('hiveme/build/cd')])]),
    ]);
    vi.mocked(Service.getMessages).mockResolvedValueOnce([cd]);

    await useAppStore.getState().clearSelectedTopic();

    expect(Service.clearTopic).toHaveBeenCalledExactlyOnceWith('hiveme/build/ci');
    const state = useAppStore.getState();
    expect(state.selectedTopic).toBe('hiveme/build');
    expect(Service.getMessages).toHaveBeenCalledExactlyOnceWith('hiveme/build', null, MESSAGE_PAGE_SIZE);
    expect(state.messages.get('hiveme/build')).toEqual([cd]);
    for (const gone of ['hiveme/build/ci', 'hiveme/build/ci/deep']) {
      expect(state.messages.has(gone)).toBe(false);
      expect(state.loadedTopics.has(gone)).toBe(false);
    }
    expect(state.loadedTopics.has('hiveme')).toBe(false);
    expect(state.messages.get('hiveme/build/cd')).toEqual([cd]);
    vi.mocked(Service.getMessages).mockResolvedValueOnce([root, cd]);
    await useAppStore.getState().selectTopic('hiveme');
    expect(useAppStore.getState().messages.get('hiveme')).toEqual([root, cd]);
  });

  it('selects hiveme once nothing between it and a cleared topic remains', async () => {
    useAppStore.setState({
      selectedTopic: 'hiveme/child/deep',
      loadedTopics: new Set(['hiveme/child/deep']),
      messages: new Map([['hiveme/child/deep', [message(1, 'hiveme/child/deep')]]]),
    });
    await useAppStore.getState().clearSelectedTopic();
    expect(useAppStore.getState().selectedTopic).toBe('hiveme');
    expect(useAppStore.getState().messages.has('hiveme/child/deep')).toBe(false);
  });
});

describe('automatic settings saves', () => {
  it('applies every edit immediately but writes once after the last 500 ms pause', async () => {
    const store = useAppStore.getState();
    store.updateConfig((config) => {
      config.broker!.username = 'first';
    });
    await vi.advanceTimersByTimeAsync(400);
    store.updateConfig((config) => {
      config.gui!.language = 'de';
    });
    expect(i18n.resolvedLanguage).toBe('de');
    expect(useAppStore.getState().config?.broker?.username).toBe('first');
    await vi.advanceTimersByTimeAsync(499);
    expect(Service.setConfig).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(Service.setConfig).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        broker: { username: 'first' },
        gui: { language: 'de' },
      })
    );
    expect(useAppStore.getState().dialogNotification).toBeNull();
  });

  it('serializes writes and never replaces a newer edit with an older response', async () => {
    let finishFirst!: (config: Config) => void;
    let finishSecond!: (config: Config) => void;
    vi.mocked(Service.setConfig)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finishFirst = resolve;
          })
      )
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finishSecond = resolve;
          })
      );

    const store = useAppStore.getState();
    store.updateConfig((config) => {
      config.gui!.language = 'de';
    });
    const firstSnapshot = useAppStore.getState().config!;
    const saving = store.flushConfig();

    store.updateConfig((config) => {
      config.gui!.language = 'ja';
    });
    store.updateConfig((config) => {
      config.broker!.username = 'latest';
    });
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
    void flushing.then(() => {
      done = true;
    });
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
    store.updateConfig((config) => {
      config.gui!.language = 'fr';
    });
    expect(await store.flushConfig()).toBe(false);
    expect(useAppStore.getState().config?.gui?.language).toBe('fr');
    expect(i18n.resolvedLanguage).toBe('fr');
    expect(useAppStore.getState().dialogNotification?.title).toBe('Disk is read-only');
    expect(await store.flushConfig()).toBe(false);

    store.updateConfig((config) => {
      config.broker!.username = 'corrected';
    });
    expect(await store.flushConfig()).toBe(true);
    expect(Service.setConfig).toHaveBeenLastCalledWith(
      expect.objectContaining({
        broker: { username: 'corrected' },
        gui: { language: 'fr' },
      })
    );
  });
});

describe('minimize to tray', () => {
  it('finishes the pending language save before hiding without changing session state', async () => {
    const store = useAppStore.getState();
    store.updateConfig((config) => {
      config.gui!.language = 'de';
    });
    const messages = store.messages;
    let finish!: (config: Config) => void;
    vi.mocked(Service.setConfig).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    const minimize = store.minimizeToTray();
    expect(Service.minimizeToTray).not.toHaveBeenCalled();
    const config = useAppStore.getState().config!;
    finish(config);
    await minimize;
    expect(Service.setConfig).toHaveBeenCalledExactlyOnceWith(config);
    expect(Service.minimizeToTray).toHaveBeenCalledExactlyOnceWith();
    expect(useAppStore.getState().messages).toBe(messages);
    expect(useAppStore.getState().config).toBe(config);
  });

  it('keeps a failed save visible instead of hiding its error', async () => {
    useAppStore.getState().updateConfig((config) => {
      config.gui!.language = 'fr';
    });
    vi.mocked(Service.setConfig).mockRejectedValueOnce(new Error('Cannot write config'));
    await useAppStore.getState().minimizeToTray();
    expect(Service.minimizeToTray).not.toHaveBeenCalled();
    expect(useAppStore.getState().dialogNotification?.title).toBe('Cannot write config');
    // Correcting settings permits a later minimize attempt.
    useAppStore.getState().updateConfig((config) => {
      config.gui!.language = 'en-US';
    });
    await useAppStore.getState().minimizeToTray();
    expect(Service.minimizeToTray).toHaveBeenCalledExactlyOnceWith();
  });
});
