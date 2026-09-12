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

// The application state. Components read from here and call these actions; only
// service.ts talks to the backend, and only this file calls service.ts.

import { create } from 'zustand';
import * as Protocol from './protocol';
import * as Service from './service';
import { MESSAGE_PAGE_SIZE } from './constants';
import type { DialogNotification } from './types';
import { changeLanguage } from '../i18n';

/** The status a GUI that has not heard from the backend yet renders. */
export const INITIAL_STATUS: Protocol.Status = {
  state: Protocol.ConnectionState.Disconnected,
  host: '',
  port: 0,
  clientId: '',
  subscriptions: 0,
  attempt: 0,
  retryInMs: null,
  lastError: null,
  messagesReceived: 0,
  databaseBytes: 0,
  notificationsPaused: false,
  configError: null,
};

interface AppState {
  config: Protocol.Config | null;
  about: Protocol.About | null;
  status: Protocol.Status;

  topics: Protocol.TopicNode[];
  topicFilter: string;
  selectedTopic: string | null;
  /** History per topic, oldest first, as the chat view reads it. */
  messages: Map<string, Protocol.MessageRow[]>;
  /** Topics whose history has been fetched at least once. */
  loadedTopics: Set<string>;
  /** True while a page of older messages is on its way. */
  loadingOlder: boolean;
  /** False once a topic has handed back every message it has. */
  hasOlder: Map<string, boolean>;

  dialogNotification: DialogNotification | null;
  tabAboutStatus: Protocol.ControlStatus;
  tabSettingsStatus: Protocol.ControlStatus;

  initConfig: () => Promise<void>;
  initAbout: () => Promise<void>;
  initStatus: () => Promise<void>;
  refreshTopics: () => Promise<void>;

  updateConfig: (change: (config: Protocol.Config) => void) => void;
  flushConfig: () => Promise<boolean>;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  toggleNotificationsPaused: () => Promise<void>;

  selectTopic: (topic: string | null) => Promise<void>;
  loadOlderMessages: (topic: string) => Promise<void>;
  clearSelectedTopic: () => Promise<void>;
  publish: (topic: string, body: string, options: Protocol.PublishOptions) => Promise<boolean>;

  receiveMessage: (message: Protocol.MessageRow) => void;
  setStatus: (status: Protocol.Status) => void;
  setTopicFilter: (filter: string) => void;
  setDialogNotification: (notification: DialogNotification | null) => void;
  notifyInfo: (title: string) => void;
  notifyError: (error: unknown) => void;
  setTabAboutStatus: (status: Protocol.ControlStatus) => void;
  setTabSettingsStatus: (status: Protocol.ControlStatus) => void;
}

/** What the backend sent, as one line of text. */
export function errorMessage(error: unknown): string {
  if (typeof error === 'string') {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export const useAppStore = create<AppState>((set, get) => {
  // Keep automatic saves outside the Settings component so closing its tab cannot
  // cancel an edit. One writer drains the newest snapshot after each pending write.
  let configSaveTimer: ReturnType<typeof setTimeout> | undefined;
  let pendingConfig: Protocol.Config | null = null;
  let configSaveInFlight: Promise<boolean> | null = null;
  let lastConfigSaveSucceeded = true;

  return {
    config: null,
    about: null,
    status: INITIAL_STATUS,

    topics: [],
    topicFilter: '',
    selectedTopic: null,
    messages: new Map(),
    loadedTopics: new Set(),
    loadingOlder: false,
    hasOlder: new Map(),

    dialogNotification: null,
    tabAboutStatus: Protocol.ControlStatus.Hidden,
    tabSettingsStatus: Protocol.ControlStatus.Hidden,

    initConfig: async () => {
      try {
        const config = await Service.getConfig();
        await changeLanguage(config.gui?.language);
        set({ config });
      } catch (error) {
        get().notifyError(error);
      }
    },

    initAbout: async () => {
      try {
        set({ about: await Service.getAbout() });
      } catch (error) {
        get().notifyError(error);
      }
    },

    initStatus: async () => {
      try {
        set({ status: await Service.getStatus() });
      } catch (error) {
        get().notifyError(error);
      }
    },

    refreshTopics: async () => {
      try {
        set({ topics: await Service.listTopics() });
      } catch (error) {
        get().notifyError(error);
      }
    },

    updateConfig: (change) => {
      const current = get().config;
      if (!current) return;
      const config = structuredClone(current);
      change(config);
      set({ config });
      void changeLanguage(config.gui?.language);
      pendingConfig = config;
      clearTimeout(configSaveTimer);
      configSaveTimer = setTimeout(() => void get().flushConfig(), 500);
    },

    flushConfig: () => {
      clearTimeout(configSaveTimer);
      configSaveTimer = undefined;
      if (configSaveInFlight) return configSaveInFlight;
      if (!pendingConfig) return Promise.resolve(lastConfigSaveSucceeded);

      configSaveInFlight = (async () => {
        while (pendingConfig) {
          const snapshot = pendingConfig;
          pendingConfig = null;
          try {
            const saved = await Service.setConfig(snapshot);
            // Only accept normalized values if no newer edit is already on screen.
            if (get().config === snapshot) set({ config: saved });
            lastConfigSaveSucceeded = true;
            await get().initStatus();
          } catch (error) {
            lastConfigSaveSucceeded = false;
            // A newer edit may already have corrected the rejected value. Keep edits
            // visible on failure; the next change retries the complete configuration.
            if (!pendingConfig) get().notifyError(error);
          }
        }
        return lastConfigSaveSucceeded;
      })().finally(() => {
        configSaveInFlight = null;
      });
      return configSaveInFlight;
    },

    connect: async () => {
      try {
        set({ status: await Service.connect() });
      } catch (error) {
        get().notifyError(error);
        await get().initStatus();
      }
    },

    disconnect: async () => {
      try {
        await Service.disconnect();
        await get().initStatus();
      } catch (error) {
        get().notifyError(error);
      }
    },

    toggleNotificationsPaused: async () => {
      try {
        set({ status: await Service.setNotificationsPaused(!get().status.notificationsPaused) });
      } catch (error) {
        get().notifyError(error);
      }
    },

    selectTopic: async (topic) => {
      set({ selectedTopic: topic });
      if (topic === null) {
        return;
      }
      try {
        if (!get().loadedTopics.has(topic)) {
          const page = await Service.getMessages(topic, null, MESSAGE_PAGE_SIZE);
          const messages = new Map(get().messages);
          messages.set(topic, page);
          const loadedTopics = new Set(get().loadedTopics);
          loadedTopics.add(topic);
          const hasOlder = new Map(get().hasOlder);
          hasOlder.set(topic, page.length >= MESSAGE_PAGE_SIZE);
          set({ messages, loadedTopics, hasOlder });
        }
        await Service.markRead(topic);
        await get().refreshTopics();
      } catch (error) {
        get().notifyError(error);
      }
    },

    loadOlderMessages: async (topic) => {
      const state = get();
      if (state.loadingOlder || state.hasOlder.get(topic) === false) {
        return;
      }
      const current = state.messages.get(topic) ?? [];
      if (current.length === 0) {
        return;
      }
      set({ loadingOlder: true });
      try {
        const page = await Service.getMessages(topic, current[0].rowId, MESSAGE_PAGE_SIZE);
        const messages = new Map(get().messages);
        messages.set(topic, [...page, ...(get().messages.get(topic) ?? [])]);
        const hasOlder = new Map(get().hasOlder);
        hasOlder.set(topic, page.length >= MESSAGE_PAGE_SIZE);
        set({ messages, hasOlder });
      } catch (error) {
        get().notifyError(error);
      } finally {
        set({ loadingOlder: false });
      }
    },

    clearSelectedTopic: async () => {
      const topic = get().selectedTopic;
      if (!topic) {
        return;
      }
      try {
        await Service.clearTopic(topic);
        const messages = new Map(get().messages);
        messages.set(topic, []);
        const hasOlder = new Map(get().hasOlder);
        hasOlder.set(topic, false);
        set({ messages, hasOlder });
        await get().refreshTopics();
      } catch (error) {
        get().notifyError(error);
      }
    },

    publish: async (topic, body, options) => {
      try {
        const row = await Service.publish(topic, body, options);
        get().receiveMessage(row);
        await get().refreshTopics();
        return true;
      } catch (error) {
        get().notifyError(error);
        return false;
      }
    },

    // A row that is already there is replaced rather than appended, so that the copy the
    // broker echoes back of something this installation sent never doubles the bubble.
    receiveMessage: (message) => {
      const state = get();
      if (!state.loadedTopics.has(message.topic)) {
        return;
      }
      const current = state.messages.get(message.topic) ?? [];
      const index = current.findIndex((row) => row.rowId === message.rowId);
      const next = index >= 0 ? current.map((row, at) => (at === index ? message : row)) : [...current, message];
      const messages = new Map(state.messages);
      messages.set(message.topic, next);
      set({ messages });
    },

    setStatus: (status) => set({ status }),
    setTopicFilter: (topicFilter) => set({ topicFilter }),
    setDialogNotification: (dialogNotification) => set({ dialogNotification }),

    notifyInfo: (title) =>
      set({ dialogNotification: { title, type: Protocol.DialogNotificationType.Info } }),

    notifyError: (error) =>
      set({ dialogNotification: { title: errorMessage(error), type: Protocol.DialogNotificationType.Error } }),

    setTabAboutStatus: (tabAboutStatus) => set({ tabAboutStatus }),
    setTabSettingsStatus: (tabSettingsStatus) => set({ tabSettingsStatus }),
  };
});
