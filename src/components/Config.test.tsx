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

import i18n from '../i18n';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { Config as ConfigType } from '../lib/protocol';
import { INITIAL_STATUS, useAppStore } from '../lib/store';
import * as Service from '../lib/service';
import Config, { ConfigCategory, fromDrafts, isBrokerUsable, toDrafts } from './Config';

vi.mock('../lib/service', () => ({
  setConfig: vi.fn(async (config: ConfigType) => config),
  getStatus: vi.fn(async () => INITIAL_STATUS),
  getBrokerInit: vi.fn(async () => 'current setup'),
}));

vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ writeText: vi.fn() }));

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
  topics: { subscriptions: ['#'] },
  publish: { qos: 1, retain: false, timeoutSecs: 10 },
  notifications: {
    enabled: true,
    rules: [{ id: 'info', topic: 'info', level: 'info', enabled: true, title: '{title|topic}', body: '{body}' }],
  },
  gui: { displayMode: 'Auto', theme: 'Ocean', language: 'en-US' },
  update: { checkInterval: 'Weekly', lastChecked: 0, lastVersion: '', ignoreVersion: '' },
  encryption: { mode: 'Off', keys: [] },
};

beforeEach(() => {
  vi.clearAllMocks();
  useAppStore.setState({ config: structuredClone(CONFIG), about: null, dialogNotification: null });
});

afterEach(async () => {
  cleanup();
  await useAppStore.getState().flushConfig();
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

  it('keeps a blank row editable until the user replaces or removes it', () => {
    expect(
      fromDrafts([
        { filter: '  ', absolute: false },
        { filter: 'info', absolute: false },
      ])
    ).toEqual(['', 'info']);
  });
});

async function renderBroker() {
  const view = render(<Config />);
  await userEvent.click(screen.getByRole('tab', { name: 'Broker' }));
  return view;
}

describe('the settings tab', () => {
  it('retains every category viewport and unfinished input while moving among all categories', async () => {
    render(<Config />);
    const panels = new Map<string, HTMLElement>();
    for (const [index, category] of Object.values(ConfigCategory).entries()) {
      await userEvent.click(screen.getByRole('tab', { name: category }));
      const panel = screen.getByRole('tabpanel', { name: category });
      panel.scrollTop = (index + 1) * 30;
      panels.set(category, panel);
      expect(screen.getAllByRole('tabpanel')).toEqual([panel]);
      if (category === ConfigCategory.Broker) await userEvent.clear(screen.getByLabelText('Keep alive (s)'));
      if (category === ConfigCategory.History) await userEvent.clear(screen.getByLabelText('Messages per topic'));
    }
    for (const [index, category] of Object.values(ConfigCategory).entries()) {
      await userEvent.click(screen.getByRole('tab', { name: category }));
      expect(screen.getByRole('tabpanel', { name: category })).toBe(panels.get(category));
      expect(panels.get(category)!.scrollTop).toBe((index + 1) * 30);
      if (category === ConfigCategory.Broker) expect(screen.getByLabelText('Keep alive (s)')).toHaveValue(null);
      if (category === ConfigCategory.History) expect(screen.getByLabelText('Messages per topic')).toHaveValue(null);
    }
    expect(Service.setConfig).not.toHaveBeenCalled();
  });

  it('orders the categories and gives Editor five unchecked rows', async () => {
    render(<Config />);
    expect(screen.getAllByRole('tab').map((tab) => tab.textContent)).toEqual([
      'Appearance',
      'Broker',
      'Editor',
      'History',
      'Notifications',
      'Topics',
      'Update',
      'Advanced',
    ]);
    await userEvent.click(screen.getByRole('tab', { name: 'Editor' }));
    const boxes = screen.getAllByRole('checkbox');
    expect(boxes).toHaveLength(5);
    const labels = ['Autocomplete', 'Autocorrect', 'Automatic capitalization', 'Spellcheck', 'Writing suggestions'];
    const rows = boxes.map((box, index) => {
      expect(box).not.toBeChecked();
      expect(box).toHaveAccessibleName(labels[index]);
      return box.closest('label');
    });
    expect(new Set(rows).size).toBe(5);
    expect(rows[0]?.parentElement?.children).toHaveLength(5);
    expect(Service.setConfig).not.toHaveBeenCalled();
  });

  it('saves independent notification checkboxes and edits the four-row rules', async () => {
    render(<Config />);
    await userEvent.click(screen.getByRole('tab', { name: 'Notifications' }));
    const os = screen.getByRole('checkbox', { name: 'Raise OS Notifications' });
    const topmost = screen.getByRole('checkbox', { name: 'Raise Topmost Window Notifications' });
    expect(screen.queryByRole('checkbox', { name: 'Notify about messages this device sent' })).not.toBeInTheDocument();
    expect(os).toBeChecked();
    expect(topmost).not.toBeChecked();
    await userEvent.click(topmost);
    await userEvent.click(os);
    expect(useAppStore.getState().config?.notifications).toMatchObject({ enabled: false, topmostEnabled: true });
    const rule = within(screen.getByRole('group', { name: 'Name info' }));
    expect(rule.getAllByRole('textbox').map((input) => input.getAttribute('id'))).toEqual([
      'rule-0-id',
      'rule-0-topic',
      'rule-0-title',
      'rule-0-body',
    ]);
    expect(rule.getByRole('combobox', { name: 'Level' })).toBeInTheDocument();
    expect(rule.getByRole('checkbox', { name: 'Enabled' })).toBeChecked();
    const ruleOs = rule.getByRole('checkbox', { name: 'OS Notification' });
    expect(ruleOs).not.toBeChecked();
    await userEvent.click(ruleOs);
    const ruleTopmost = rule.getByRole('checkbox', { name: 'Topmost Window' });
    expect(ruleTopmost).not.toBeChecked();
    await userEvent.click(ruleTopmost);
    for (const [label, text] of [
      ['Name', 'my-rule'],
      ['Topic filter', 'build/#'],
      ['Title template', 'Build'],
      ['Body template', 'Ready'],
    ]) {
      await userEvent.clear(rule.getByLabelText(label));
      await userEvent.type(rule.getByLabelText(label), text);
    }
    await act(async () => {
      await useAppStore.getState().flushConfig();
    });
    expect(vi.mocked(Service.setConfig).mock.lastCall?.[0].notifications).toMatchObject({
      enabled: false,
      topmostEnabled: true,
      rules: [
        { id: 'my-rule', topic: 'build/#', title: 'Build', body: 'Ready', enabled: true, os: true, topmost: true },
      ],
    });
    await userEvent.click(rule.getByRole('button', { name: i18n.t('settings.removeRule') }));
    expect(useAppStore.getState().config?.notifications?.rules).toEqual([]);
  });

  it('adds enabled rules with both channels unchecked and treats omitted enabled as checked', async () => {
    useAppStore.setState({ config: { ...CONFIG, notifications: { rules: [{ id: 'minimal', topic: '#' }] } } });
    render(<Config />);
    await userEvent.click(screen.getByRole('tab', { name: 'Notifications' }));
    const minimal = within(screen.getByRole('group', { name: 'Name minimal' }));
    expect(minimal.getByRole('checkbox', { name: 'Enabled' })).toBeChecked();
    expect(minimal.getByRole('checkbox', { name: 'OS Notification' })).not.toBeChecked();
    expect(minimal.getByRole('checkbox', { name: 'Topmost Window' })).not.toBeChecked();
    await userEvent.click(screen.getByRole('button', { name: 'Add a rule' }));
    const added = within(screen.getByRole('group', { name: 'Name rule-2' }));
    expect(added.getByRole('checkbox', { name: 'Enabled' })).toBeChecked();
    expect(added.getByRole('checkbox', { name: 'OS Notification' })).not.toBeChecked();
    expect(added.getByRole('checkbox', { name: 'Topmost Window' })).not.toBeChecked();
    await userEvent.click(added.getByRole('checkbox', { name: 'Enabled' }));
    await act(async () => {
      await useAppStore.getState().flushConfig();
    });
    expect(vi.mocked(Service.setConfig).mock.lastCall?.[0].notifications?.rules?.[1]).toMatchObject({
      id: 'rule-2',
      enabled: false,
      os: false,
      topmost: false,
    });
  });

  it('fills the broker fields from the config', async () => {
    await renderBroker();
    expect(screen.getByLabelText('URL')).toHaveValue('abc123.s1.eu.hivemq.cloud:8883');
    expect(screen.getByLabelText('Protocol')).toHaveTextContent('TLS MQTT');
    expect(screen.getByDisplayValue('hiveme-sam')).toBeInTheDocument();
    expect(screen.getByDisplayValue('s3cret')).toBeInTheDocument();
  });

  // The console shows three URLs, none of them with a scheme in front. Each is pasted
  // into the box as it stands, and the protocol beside it is what says how to read it.
  it.each([
    ['MQTT', 'abc123.s1.eu.hivemq.cloud', 'mqtt://abc123.s1.eu.hivemq.cloud'],
    ['TLS MQTT', 'abc123.s1.eu.hivemq.cloud:8883', 'mqtts://abc123.s1.eu.hivemq.cloud:8883'],
    ['TLS WebSocket', 'abc123.s1.eu.hivemq.cloud:8884/mqtt', 'wss://abc123.s1.eu.hivemq.cloud:8884/mqtt'],
  ])('saves the %s URL of the console as it was pasted', async (protocol, shown, saved) => {
    await renderBroker();

    await userEvent.click(screen.getByLabelText('Protocol'));
    await userEvent.click(screen.getByRole('option', { name: protocol }));
    await userEvent.clear(screen.getByLabelText('URL'));
    await userEvent.type(screen.getByLabelText('URL'), shown);
    expect(useAppStore.getState().config?.broker?.url).toBe(saved);
    await waitFor(() =>
      expect(Service.setConfig).toHaveBeenLastCalledWith(
        expect.objectContaining({ broker: expect.objectContaining({ url: saved }) })
      )
    );
  });

  it('starts a broker that has none on TLS MQTT, which is the only one the cloud accepts', async () => {
    useAppStore.setState({ config: { ...structuredClone(CONFIG), broker: { url: '', username: '', password: '' } } });
    await renderBroker();
    expect(screen.getByLabelText('Protocol')).toHaveTextContent('TLS MQTT');
    expect(screen.getByLabelText('URL')).toHaveValue('');
  });

  it('says which port a URL that names none will use, because the protocol decides it', async () => {
    await renderBroker();
    expect(screen.getByText(/Connects to mqtts:\/\/abc123.s1.eu.hivemq.cloud:8883, on port 8883/)).toBeInTheDocument();

    await userEvent.clear(screen.getByLabelText('URL'));
    await userEvent.type(screen.getByLabelText('URL'), 'abc123.s1.eu.hivemq.cloud');

    expect(screen.getByText(/Connects to mqtts:\/\/abc123.s1.eu.hivemq.cloud, on port 8883/)).toBeInTheDocument();
  });

  it('takes a scheme off a URL pasted with one rather than leaving it in the box', async () => {
    await renderBroker();

    await userEvent.clear(screen.getByLabelText('URL'));
    await userEvent.type(screen.getByLabelText('URL'), 'mqtt://localhost:1884');

    expect(screen.getByLabelText('URL')).toHaveValue('localhost:1884');
    expect(screen.getByLabelText('Protocol')).toHaveTextContent('MQTT');
  });

  it('hides the password until the toggle is used', async () => {
    await renderBroker();
    const password = screen.getByDisplayValue('s3cret');
    expect(password).toHaveAttribute('type', 'password');

    await userEvent.click(screen.getByLabelText('Show the password'));

    expect(screen.getByDisplayValue('s3cret')).toHaveAttribute('type', 'text');
  });

  it('says that TLS needs no settings, because it does not', async () => {
    await renderBroker();
    expect(screen.getByText(/TLS needs no settings/)).toBeInTheDocument();
  });

  it('applies edits immediately and saves after typing pauses', async () => {
    await renderBroker();

    await userEvent.clear(screen.getByDisplayValue('hiveme-sam'));
    await userEvent.type(screen.getByLabelText('Username'), 'someone-else');
    expect(useAppStore.getState().config?.broker?.username).toBe('someone-else');
    expect(Service.setConfig).not.toHaveBeenCalled();
    await waitFor(() => expect(Service.setConfig).toHaveBeenCalledTimes(1));
    expect(Service.setConfig).toHaveBeenCalledWith(
      expect.objectContaining({ broker: expect.objectContaining({ username: 'someone-else' }) })
    );
  });

  it('has no Save, Revert, or Open config button and does not save on mount', async () => {
    render(<Config />);
    expect(screen.queryByRole('button', { name: /Save|Revert|config file/i })).not.toBeInTheDocument();
    await act(async () => {
      await useAppStore.getState().flushConfig();
    });
    expect(Service.setConfig).not.toHaveBeenCalled();
  });

  it('offers the CLI setup string only while the broker fields are usable', async () => {
    await renderBroker();
    expect(screen.getByRole('button', { name: /Copy CLI setup/ })).toBeEnabled();
  });

  it('refuses to offer a setup string that could not be applied', async () => {
    useAppStore.setState({ config: { ...structuredClone(CONFIG), broker: { url: '', username: '', password: '' } } });
    await renderBroker();
    expect(screen.getByRole('button', { name: /Copy CLI setup/ })).toBeDisabled();
  });

  it('opens on Appearance as the first category with mode, theme, and language controls', () => {
    render(<Config />);
    expect(screen.getAllByRole('tab')[0]).toBe(screen.getByRole('tab', { name: 'Appearance', selected: true }));
    expect(screen.getByRole('group', { name: 'Mode' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Auto Mode', pressed: true })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Light Mode' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Dark Mode' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Theme' })).toHaveTextContent('Ocean');
    expect(screen.getByRole('combobox', { name: 'Language' })).toHaveTextContent('English (US)');
    expect(screen.queryByLabelText('Username')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Messages per topic')).not.toBeInTheDocument();
  });

  it('shows one category at a time and switches on a click', async () => {
    render(<Config />);
    expect(screen.getByRole('group', { name: 'Mode' })).toBeInTheDocument();

    await userEvent.click(screen.getByRole('tab', { name: 'History' }));

    expect(screen.queryByRole('group', { name: 'Mode' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('Messages per topic')).toBeInTheDocument();
  });

  it('keeps an edit made in one category while the user is in another', async () => {
    await renderBroker();

    await userEvent.type(screen.getByLabelText('Username'), '-edited');
    await userEvent.click(screen.getByRole('tab', { name: 'Update' }));
    expect(useAppStore.getState().config?.broker?.username).toBe('hiveme-sam-edited');
    await waitFor(() =>
      expect(Service.setConfig).toHaveBeenCalledWith(
        expect.objectContaining({ broker: expect.objectContaining({ username: 'hiveme-sam-edited' }) })
      )
    );
  });

  it('finishes an automatic save after the settings tab is closed', async () => {
    const view = await renderBroker();
    await userEvent.type(screen.getByLabelText('Username'), '-edited');
    view.unmount();
    await waitFor(() =>
      expect(Service.setConfig).toHaveBeenCalledWith(
        expect.objectContaining({ broker: expect.objectContaining({ username: 'hiveme-sam-edited' }) })
      )
    );
  });

  it('flushes current edits before copying the complete CLI setup command', async () => {
    const setup = JSON.stringify({ v: 1, url: CONFIG.broker?.url, username: 'hiveme-sam', password: 's3cret-edited' });
    vi.mocked(Service.getBrokerInit).mockResolvedValueOnce(setup);
    await renderBroker();
    await userEvent.type(screen.getByLabelText('Password'), '-edited');
    await userEvent.click(screen.getByRole('button', { name: 'Copy CLI setup' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(`hmc --init '${setup}'`));
    expect(useAppStore.getState().dialogNotification?.title).toBe(
      'CLI setup command copied. Paste it into your terminal and run it.'
    );
    expect(Service.setConfig).toHaveBeenCalledWith(
      expect.objectContaining({ broker: expect.objectContaining({ password: 's3cret-edited' }) })
    );
    expect(vi.mocked(Service.setConfig).mock.invocationCallOrder[0]).toBeLessThan(
      vi.mocked(Service.getBrokerInit).mock.invocationCallOrder[0]
    );
  });

  it('keeps quotes and shell characters in credentials inside the JSON argument', async () => {
    const setup = {
      v: 1,
      url: CONFIG.broker?.url,
      username: "O'Brien’s account",
      password: '\'‘’‚‛"$HOME`echo`; & | \\ password',
    };
    vi.mocked(Service.getBrokerInit).mockResolvedValueOnce(JSON.stringify(setup));
    await renderBroker();
    await userEvent.click(screen.getByRole('button', { name: 'Copy CLI setup' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    const command = vi.mocked(writeText).mock.calls[0][0];
    expect(command).toMatch(/^hmc --init '[^'‘’‚‛]*'$/);
    expect(JSON.parse(command.slice("hmc --init '".length, -1))).toEqual(setup);
  });

  it('lets a subscription filter be replaced without removing its row', async () => {
    render(<Config />);
    await userEvent.click(screen.getByRole('tab', { name: 'Topics' }));
    await userEvent.clear(screen.getByLabelText('Subscription filter 1'));
    expect(screen.getByLabelText('Subscription filter 1')).toHaveValue('');
    await userEvent.type(screen.getByLabelText('Subscription filter 1'), 'build/#');
    expect(useAppStore.getState().config?.topics?.subscriptions).toEqual(['build/#']);
    expect(screen.queryByLabelText('Prefix')).not.toBeInTheDocument();
    expect(useAppStore.getState().config?.topics).not.toHaveProperty('prefix');
  });

  it('does not copy stale credentials when their automatic save fails', async () => {
    vi.mocked(Service.setConfig).mockRejectedValueOnce(new Error('Cannot save credentials'));
    await renderBroker();
    await userEvent.type(screen.getByLabelText('Password'), '-edited');
    await userEvent.click(screen.getByRole('button', { name: 'Copy CLI setup' }));
    await waitFor(() => expect(useAppStore.getState().dialogNotification?.title).toBe('Cannot save credentials'));
    expect(Service.getBrokerInit).not.toHaveBeenCalled();
    expect(writeText).not.toHaveBeenCalled();
  });

  it('applies numeric settings and switches immediately', async () => {
    render(<Config />);
    await userEvent.click(screen.getByRole('tab', { name: 'History' }));
    await userEvent.clear(screen.getByLabelText('Messages per topic'));
    await userEvent.type(screen.getByLabelText('Messages per topic'), '250');
    expect(useAppStore.getState().config?.gui?.history?.maxMessagesPerTopic).toBe(250);
    await userEvent.click(screen.getByRole('tab', { name: 'Notifications' }));
    await userEvent.click(screen.getByLabelText('Raise OS Notifications'));
    expect(useAppStore.getState().config?.notifications?.enabled).toBe(false);
    await userEvent.click(screen.getByRole('combobox', { name: 'Level' }));
    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'Info',
      'Success',
      'Warn',
      'Error',
    ]);
    await userEvent.click(screen.getByRole('option', { name: 'Success' }));
    expect(useAppStore.getState().config?.notifications?.rules?.[0].level).toBe('success');
  });

  it('keeps the sections that are designed but not implemented visibly so', async () => {
    render(<Config />);

    await userEvent.click(screen.getByRole('tab', { name: 'Advanced' }));

    expect(screen.getAllByText(/not implemented yet/)).toHaveLength(2);
  });
});

it('adding a rule after a deletion chooses an unused ID', async () => {
  const config = structuredClone(CONFIG);
  config.notifications!.rules = [{ ...CONFIG.notifications!.rules![0], id: 'rule-2' }];
  useAppStore.setState({ config });
  render(<Config />);
  await userEvent.click(screen.getByRole('tab', { name: 'Notifications' }));
  expect(screen.getByText(i18n.t('settings.rawNotificationHint'))).toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: i18n.t('settings.addRule') }));
  const ids = useAppStore.getState().config!.notifications!.rules!.map((rule) => rule.id);
  expect(new Set(ids).size).toBe(ids.length);
});

it('keeps a blank subscription editable without attempting an invalid autosave', async () => {
  useAppStore.getState().updateConfig((config) => {
    config.topics!.subscriptions = [''];
  });
  expect(await useAppStore.getState().flushConfig()).toBe(false);
  expect(Service.setConfig).not.toHaveBeenCalled();
  expect(useAppStore.getState().dialogNotification).toBeNull();
  useAppStore.getState().updateConfig((config) => {
    config.topics!.subscriptions = ['#'];
  });
  expect(await useAppStore.getState().flushConfig()).toBe(true);
  expect(Service.setConfig).toHaveBeenCalledOnce();
});
