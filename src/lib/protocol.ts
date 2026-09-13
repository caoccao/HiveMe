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

// The IPC types shared with the backend.
//
// This file and `src-tauri/src/protocol.rs` are hand synced: camelCase here,
// snake_case there with an explicit `#[serde(rename)]`.
// `scripts/ts/check-spec-sync.ts` fails when one moves without the other.
//
// The config and the message envelope are not written here. They are generated from
// the JSON schemas into `src/generated/` and re-exported below, because they are the
// shared format rather than a protocol of the GUI's own.

import type { HiveMeConfig } from '../generated/config';
import type { HiveMeMessage } from '../generated/message';

export type Config = HiveMeConfig;
export type Message = HiveMeMessage;
export type { Broker, Gui, Notifications, Publish, Rule, Subscription, Topics, Update } from '../generated/config';
export type { Enc, Payload, Sender } from '../generated/message';

/** The `status` event, and what `get_status` answers with. */
export const EVENT_STATUS = 'status';

/** The `message` event: one stored row, as `get_messages` would return it. */
export const EVENT_MESSAGE = 'message';

/** The `topic-added` event: a topic the tree has not shown before. */
export const EVENT_TOPIC_ADDED = 'topic-added';

/** The `notification-fired` event: a rule raised an OS notification. */
export const EVENT_NOTIFICATION_FIRED = 'notification-fired';

/** What the About tab shows. */
export interface About {
  appVersion: string;
  configPath: string;
  databasePath: string;
  deviceId: string;
  deviceName: string;
  githubUrl: string;
}

/** Where the connection is. Mirrors `hiveme_core::State`. */
export enum ConnectionState {
  Connecting = 'Connecting',
  Connected = 'Connected',
  Reconnecting = 'Reconnecting',
  Disconnected = 'Disconnected',
}

/** What the status bar renders. */
export interface Status {
  state: ConnectionState;
  host: string;
  port: number;
  clientId: string;
  subscriptions: number;
  attempt: number;
  retryInMs: number | null;
  lastError: string | null;
  messagesReceived: number;
  databaseBytes: number;
  notificationsPaused: boolean;
  configError: string | null;
}

/** Every nonempty path selects its full subtree, including parents without direct messages. */
export interface TopicNode {
  id: string;
  label: string;
  /** The selected subtree root; null only for an empty leading segment. */
  topic: string | null;
  unread: number;
  messages: number;
  children: TopicNode[];
}

/** How a payload was read. Mirrors `hiveme_core::message::Parsed`. */
export enum Tier {
  Envelope = 'envelope',
  Json = 'json',
  Text = 'text',
  Bytes = 'bytes',
}

/** One stored message, as the chat view renders it. */
export interface MessageRow {
  rowId: number;
  topic: string;
  id: string;
  ts: string;
  receivedTs: string;
  senderId: string | null;
  senderName: string | null;
  app: string | null;
  tier: string;
  level: string | null;
  title: string | null;
  body: string;
  /** The payload as text, or as hex for the `bytes` tier. */
  raw: string;
  rawLength: number;
  qos: number;
  retain: boolean;
  outgoing: boolean;
}

/** The publish overrides controlled by the composer's collapsible options panel. */
export interface PublishOptions {
  /** Relative to the selected topic; leading slashes are ignored. */
  topic?: string | null;
  json?: boolean;
  qos?: number | null;
  retain?: boolean | null;
  title?: string | null;
  level?: string | null;
}

/** The `topic-added` event. */
export interface TopicAddedEvent {
  topic: string;
}

/** The `notification-fired` event. */
export interface NotificationFiredEvent {
  ruleId: string;
  messageId: string;
  topic: string;
}

/** What the update check found. */
export interface UpdateCheckResult {
  hasUpdate: boolean;
  latestVersion: string | null;
}

/** Message severity, as it travels in the envelope. */
export enum Level {
  Debug = 'debug',
  Info = 'info',
  Success = 'success',
  Warn = 'warn',
  Error = 'error',
}

export const LEVELS: Level[] = [Level.Debug, Level.Info, Level.Success, Level.Warn, Level.Error];

/** Which color scheme the GUI follows. */
export enum DisplayMode {
  Auto = 'Auto',
  Light = 'Light',
  Dark = 'Dark',
}

/** The GUI palette. */
export enum Theme {
  Ocean = 'Ocean',
  Aqua = 'Aqua',
  Sky = 'Sky',
  Arctic = 'Arctic',
  Glacier = 'Glacier',
  Mist = 'Mist',
  Slate = 'Slate',
  Charcoal = 'Charcoal',
  Midnight = 'Midnight',
  Indigo = 'Indigo',
  Violet = 'Violet',
  Lavender = 'Lavender',
  Rose = 'Rose',
  Blush = 'Blush',
  Coral = 'Coral',
  Sunset = 'Sunset',
  Amber = 'Amber',
  Sand = 'Sand',
  Forest = 'Forest',
  Emerald = 'Emerald',
}

export const THEMES: Theme[] = Object.values(Theme);

/** How often to look for a new release. */
export enum UpdateCheckInterval {
  Daily = 'Daily',
  Weekly = 'Weekly',
  Monthly = 'Monthly',
}

/** The languages that ship today. */
export enum Language {
  De = 'de',
  EnUS = 'en-US',
  Es = 'es',
  Fr = 'fr',
  It = 'it',
  Ja = 'ja',
  ZhCN = 'zh-CN',
  ZhHK = 'zh-HK',
  ZhTW = 'zh-TW',
}

export const LANGUAGES: Language[] = Object.values(Language);

/** Autonyms keep the language picker readable regardless of the active locale. */
export const LANGUAGE_LABELS: Record<Language, string> = {
  [Language.De]: 'Deutsch',
  [Language.EnUS]: 'English (US)',
  [Language.Es]: 'Español',
  [Language.Fr]: 'Français',
  [Language.It]: 'Italiano',
  [Language.Ja]: '日本語',
  [Language.ZhCN]: '简体中文',
  [Language.ZhHK]: '繁體中文 (香港)',
  [Language.ZhTW]: '繁體中文 (臺灣)',
};

/** Whether a closable tab is absent, being opened, or already open. */
export enum ControlStatus {
  Hidden,
  Selected,
  Visible,
}

/** Which component a tab shows. */
export enum TabType {
  Messages,
  Config,
  About,
}

/** How the snackbar renders an in-app message. */
export enum DialogNotificationType {
  Info,
  Error,
}
