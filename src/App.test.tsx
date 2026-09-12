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

// The smoke test of the whole window: mount the application the way main.tsx does,
// with the backend replaced by the answers it would give, and check that the three
// rows of the layout are on screen.
//
// It exists because a window that renders nothing still runs every effect, so the
// backend log looks healthy while the user sees an empty rectangle. Only the rendered
// output says whether the layout is there.

import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App';
import { INITIAL_STATUS, useAppStore } from './lib/store';
import * as Protocol from './lib/protocol';

const about = {
  appVersion: '0.1.0',
  configPath: '/home/sam/.config/HiveMe/HiveMe.json',
  databasePath: '/home/sam/.config/HiveMe/HiveMe.db',
  deviceId: 'device-1',
  deviceName: 'sams-desktop',
  githubUrl: 'https://github.com/caoccao/HiveMe',
};

const topics: Protocol.TopicNode[] = [
  {
    id: 'hiveme',
    label: 'hiveme',
    topic: null,
    unread: 2,
    messages: 3,
    children: [{ id: 'hiveme/info', label: 'info', topic: 'hiveme/info', unread: 2, messages: 3, children: [] }],
  },
];

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (command: string) => {
    switch (command) {
      case 'get_about':
        return about;
      case 'get_config':
        return { version: 1, broker: {}, topics: { prefix: 'hiveme' }, gui: {} };
      case 'get_status':
        return { ...INITIAL_STATUS, state: 'Connected', host: 'abc.s1.eu.hivemq.cloud', port: 8883 };
      case 'list_topics':
        return topics;
      case 'get_messages':
        return [];
      case 'get_update_result':
        return null;
      default:
        return null;
    }
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => undefined),
}));

beforeEach(() => {
  useAppStore.setState({
    config: null,
    about: null,
    status: INITIAL_STATUS,
    topics: [],
    selectedTopic: null,
    tabAboutStatus: Protocol.ControlStatus.Hidden,
    tabSettingsStatus: Protocol.ControlStatus.Hidden,
  });
});

describe('the application window', () => {
  it('renders the toolbar, the tabs, and the status bar', async () => {
    render(<App />);

    expect(await screen.findByLabelText(/Connect to the broker|Disconnect from the broker/)).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Messages/ })).toBeInTheDocument();
    expect(screen.getByLabelText('Settings (F10)')).toBeInTheDocument();
    expect(screen.getByLabelText('About')).toBeInTheDocument();
  });

  it('fills the store from the backend on the first render', async () => {
    render(<App />);

    await waitFor(() => expect(useAppStore.getState().about).not.toBeNull());
    expect(useAppStore.getState().topics).toHaveLength(1);
    expect(useAppStore.getState().status.state).toBe('Connected');
  });

  it('shows the topic tree and the status bar the backend described', async () => {
    render(<App />);

    expect(await screen.findByText('hiveme')).toBeInTheDocument();
    expect(screen.getByText('info')).toBeInTheDocument();
    expect(screen.getByText('abc.s1.eu.hivemq.cloud:8883')).toBeInTheDocument();
    expect(screen.getByText(/Select a topic/)).toBeInTheDocument();
  });
});
