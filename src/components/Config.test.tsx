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

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Config as ConfigType } from '../lib/protocol';
import { useAppStore } from '../lib/store';
import Config, { fromDrafts, isBrokerUsable, toDrafts } from './Config';

const CONFIG: ConfigType = {
  version: 1,
  device: { id: 'device-1', name: 'sams-laptop' },
  broker: {
    url: 'mqtts://abc123.s1.eu.hivemq.cloud:8883',
    username: 'hiveme-sam',
    password: 's3cret',
    clientIdPrefix: 'hiveme',
    keepAliveSecs: 30,
    sessionExpirySecs: 3600,
    connectTimeoutSecs: 10,
    reconnect: { initialDelayMs: 1000, maxDelayMs: 30000 },
  },
  topics: { prefix: 'hiveme', default: 'info', subscriptions: ['#'] },
  publish: { qos: 1, retain: false, timeoutSecs: 10 },
  notifications: {
    enabled: true,
    notifyOwnMessages: false,
    rules: [{ id: 'info', topic: 'info', level: 'info', enabled: true, title: '{title|topic}', body: '{body}' }],
  },
  gui: { displayMode: 'Auto', theme: 'Ocean', language: 'en-US' },
  update: { checkInterval: 'Weekly', lastChecked: 0, lastVersion: '', ignoreVersion: '' },
  encryption: { mode: 'Off', keys: [] },
};

beforeEach(() => {
  useAppStore.setState({ config: structuredClone(CONFIG), about: null });
});

describe('isBrokerUsable', () => {
  it('needs a URL, a username, and somewhere to read the password', () => {
    expect(isBrokerUsable(CONFIG.broker)).toBe(true);
    expect(isBrokerUsable({ ...CONFIG.broker, url: '  ' })).toBe(false);
    expect(isBrokerUsable({ ...CONFIG.broker, username: '' })).toBe(false);
    expect(isBrokerUsable({ ...CONFIG.broker, password: '' })).toBe(false);
    expect(isBrokerUsable(undefined)).toBe(false);
  });

  it('accepts a password that lives somewhere else', () => {
    expect(
      isBrokerUsable({ ...CONFIG.broker, password: '', passwordRef: { type: 'Env', name: 'HIVEME_PASSWORD' } })
    ).toBe(true);
  });
});

describe('the subscription editor', () => {
  it('reads both shapes a subscription can be written in', () => {
    expect(toDrafts(['#', { filter: '$SYS/#', absolute: true }])).toEqual([
      { filter: '#', absolute: false },
      { filter: '$SYS/#', absolute: true },
    ]);
    expect(toDrafts(undefined)).toEqual([]);
  });

  it('writes a relative filter as a bare string and an absolute one as an object', () => {
    expect(fromDrafts([{ filter: '#', absolute: false }])).toEqual(['#']);
    expect(fromDrafts([{ filter: '$SYS/#', absolute: true }])).toEqual([{ filter: '$SYS/#', absolute: true }]);
  });

  it('drops a row the user left blank rather than writing an empty filter', () => {
    expect(fromDrafts([{ filter: '  ', absolute: false }, { filter: 'info', absolute: false }])).toEqual(['info']);
  });
});

describe('the settings tab', () => {
  it('fills the broker fields from the config', () => {
    render(<Config />);
    expect(screen.getByDisplayValue('mqtts://abc123.s1.eu.hivemq.cloud:8883')).toBeInTheDocument();
    expect(screen.getByDisplayValue('hiveme-sam')).toBeInTheDocument();
    expect(screen.getByDisplayValue('s3cret')).toBeInTheDocument();
  });

  it('hides the password until the toggle is used', async () => {
    render(<Config />);
    const password = screen.getByDisplayValue('s3cret');
    expect(password).toHaveAttribute('type', 'password');

    await userEvent.click(screen.getByLabelText('Show the password'));

    expect(screen.getByDisplayValue('s3cret')).toHaveAttribute('type', 'text');
  });

  it('says that TLS needs no settings, because it does not', () => {
    render(<Config />);
    expect(screen.getByText(/TLS needs no settings/)).toBeInTheDocument();
  });

  it('saves what the user edited', async () => {
    const saveConfig = vi.fn().mockResolvedValue(true);
    useAppStore.setState({ saveConfig });
    render(<Config />);

    await userEvent.clear(screen.getByDisplayValue('hiveme-sam'));
    await userEvent.type(screen.getByLabelText('Username'), 'someone-else');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(saveConfig).toHaveBeenCalledTimes(1);
    expect(saveConfig.mock.calls[0][0].broker.username).toBe('someone-else');
  });

  it('reverts to what the backend has', async () => {
    render(<Config />);

    await userEvent.type(screen.getByLabelText('Username'), '-edited');
    expect(screen.getByDisplayValue('hiveme-sam-edited')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Revert' }));

    expect(screen.getByDisplayValue('hiveme-sam')).toBeInTheDocument();
  });

  it('offers the CLI setup string only while the broker fields are usable', () => {
    render(<Config />);
    expect(screen.getByRole('button', { name: /Copy CLI setup/ })).toBeEnabled();
  });

  it('refuses to offer a setup string that could not be applied', () => {
    useAppStore.setState({ config: { ...structuredClone(CONFIG), broker: { url: '', username: '', password: '' } } });
    render(<Config />);
    expect(screen.getByRole('button', { name: /Copy CLI setup/ })).toBeDisabled();
  });

  it('keeps the sections that are designed but not implemented visibly so', () => {
    render(<Config />);
    expect(screen.getAllByText(/not implemented yet/)).toHaveLength(2);
  });
});
