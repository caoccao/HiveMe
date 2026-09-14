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

// Every `invoke` the frontend makes. Components never call Tauri APIs directly, so
// that the IPC surface is one file and the store is the only thing that knows about
// the backend. The commands are the table in docs/specs/gui.md.

import { invoke } from '@tauri-apps/api/core';
import * as Protocol from './protocol';

export async function clearTopic(topic: string): Promise<number> {
  return await invoke<number>('clear_topic', { topic });
}

export async function connect(): Promise<Protocol.Status> {
  return await invoke<Protocol.Status>('connect');
}

export async function disconnect(): Promise<void> {
  return await invoke<void>('disconnect');
}

export async function getAbout(): Promise<Protocol.About> {
  return await invoke<Protocol.About>('get_about');
}

export async function getBrokerInit(): Promise<string> {
  return await invoke<string>('get_broker_init');
}

export async function getConfig(): Promise<Protocol.Config> {
  return await invoke<Protocol.Config>('get_config');
}

export async function getMessages(
  topic: string,
  before: number | null = null,
  limit: number | null = null
): Promise<Protocol.MessageRow[]> {
  return await invoke<Protocol.MessageRow[]>('get_messages', { topic, before, limit });
}

export async function getStatus(): Promise<Protocol.Status> {
  return await invoke<Protocol.Status>('get_status');
}

export async function getUpdateResult(): Promise<Protocol.UpdateCheckResult | null> {
  return await invoke<Protocol.UpdateCheckResult | null>('get_update_result');
}

export async function listTopics(): Promise<Protocol.TopicNode[]> {
  return await invoke<Protocol.TopicNode[]>('list_topics');
}

export async function markRead(topic: string): Promise<void> {
  return await invoke<void>('mark_read', { topic });
}

export async function publish(
  topic: string,
  body: string,
  options: Protocol.PublishOptions | null = null
): Promise<Protocol.MessageRow> {
  return await invoke<Protocol.MessageRow>('publish', { topic, body, options });
}

export async function setConfig(config: Protocol.Config): Promise<Protocol.Config> {
  return await invoke<Protocol.Config>('set_config', { config });
}

export async function setNotificationsPaused(paused: boolean): Promise<Protocol.Status> {
  return await invoke<Protocol.Status>('set_notifications_paused', { paused });
}

export async function skipVersion(version: string): Promise<void> {
  return await invoke<void>('skip_version', { version });
}
