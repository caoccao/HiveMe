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

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import i18n from './i18n';
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

let backendConfig: Protocol.Config;
let backendTopics: Protocol.TopicNode[];
let failSave = false;
let failTray = false;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (command: string, args?: { config?: Protocol.Config }) => {
    switch (command) {
      case 'minimize_to_tray':
        if (failTray) throw new Error(i18n.t('tray.unavailable'));
        return null;
      case 'get_about':
        return about;
      case 'get_config':
        return backendConfig;
      case 'set_config':
        if (failSave) throw new Error('Cannot write config');
        backendConfig = args?.config as Protocol.Config;
        return backendConfig;
      case 'get_status':
        return { ...INITIAL_STATUS, state: 'Connected', host: 'abc.s1.eu.hivemq.cloud', port: 8883 };
      case 'list_topics':
        return backendTopics;
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
  vi.clearAllMocks();
  failSave = false;
  failTray = false;
  backendTopics = topics;
  backendConfig = { version: 1, broker: {}, topics: { subscriptions: ['#'] }, gui: { language: 'en-US' } };
  useAppStore.setState({
    dialogNotification: null,
    config: null,
    about: null,
    status: INITIAL_STATUS,
    topics: [],
    selectedTopic: null,
    topicFilter: '',
    messages: new Map(),
    loadedTopics: new Set(),
    hasOlder: new Map(),
    tabAboutStatus: Protocol.ControlStatus.Hidden,
    tabSettingsStatus: Protocol.ControlStatus.Hidden,
  });
});

afterEach(async () => {
  cleanup();
  await useAppStore.getState().flushConfig();
});

describe('the application window', () => {
  it('renders the toolbar, the tabs, and the status bar', async () => {
    render(<App />);

    expect(await screen.findByLabelText(/Connect to the broker|Disconnect from the broker/)).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Messages/ })).toBeInTheDocument();
    expect(screen.getByLabelText('Settings (F10)')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'About' })).toBeInTheDocument();
  });

  it('offers a localized tray button at the right and keeps the message draft intact', async () => {
    backendConfig.gui = { language: 'de' };
    render(<App />);
    const button = await screen.findByRole('button', { name: 'In den Infobereich minimieren' });
    await waitFor(() => expect(useAppStore.getState().selectedTopic).toBe('hiveme'));
    expect(button.closest('.MuiButtonGroup-root')).toHaveStyle({ marginLeft: 'auto' });
    // Like MUI SVG icons, the bitmap glyph must not shrink inside IconButton padding.
    expect(button.querySelector('[aria-hidden="true"]')).toHaveStyle({ flexShrink: '0', width: '20px' });
    const composer = screen.getByRole('textbox', { name: i18n.t('composer.placeholder') });
    fireEvent.change(composer, { target: { value: 'Unsent draft' } });
    await userEvent.click(button);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('minimize_to_tray'));
    expect(composer).toHaveValue('Unsent draft');
    expect(useAppStore.getState().status.state).toBe('Connected');
  });

  it('shows the localized error when the desktop cannot provide a tray', async () => {
    failTray = true;
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: 'Minimize to system tray' }));
    expect(await screen.findByText(i18n.t('tray.unavailable'))).toBeVisible();
    expect(screen.getByRole('tab', { name: /Messages/ })).toBeVisible();
  });

  it('fills the store from the backend on the first render', async () => {
    render(<App />);

    await waitFor(() => expect(useAppStore.getState().about).not.toBeNull());
    expect(useAppStore.getState().topics).toHaveLength(1);
    expect(useAppStore.getState().status.state).toBe('Connected');
  });

  it('shows the topic tree and the status bar the backend described', async () => {
    render(<App />);

    expect(await screen.findByText('info')).toBeInTheDocument();
    expect(screen.getByText('abc.s1.eu.hivemq.cloud:8883')).toBeInTheDocument();
    expect(screen.getByRole('treeitem', { name: /^hiveme/ })).toHaveAttribute('aria-selected', 'true');
    expect(screen.queryByText(/Select a topic/)).not.toBeInTheDocument();
  });

  it('opens on hiveme with an empty database even after a different topic was selected', async () => {
    backendTopics = [];
    useAppStore.setState({ selectedTopic: 'team/build' });
    render(<App />);

    await waitFor(() => expect(useAppStore.getState().status.state).toBe('Connected'));
    expect(useAppStore.getState().selectedTopic).toBe('hiveme');
    const startup = screen.getByRole('treeitem', { name: 'hiveme' });
    expect(startup).toHaveAttribute('aria-selected', 'true');
    expect(startup.querySelector('.MuiTreeItem-content')).toHaveAttribute('data-selected');
    expect(invoke).toHaveBeenCalledWith('get_messages', expect.objectContaining({ topic: 'hiveme' }));
    const composer = screen.getByRole('textbox', { name: i18n.t('composer.placeholder') });
    expect(composer).toBeEnabled();
    await userEvent.type(composer, 'Ready to send');
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();

    await useAppStore.getState().clearSelectedTopic();
    expect(screen.getByRole('treeitem', { name: 'hiveme' })).toHaveAttribute('aria-selected', 'true');
  });

  it.each([Protocol.DisplayMode.Light, Protocol.DisplayMode.Dark])(
    'disables text suggestions and corrections in every input in %s mode',
    async (displayMode) => {
      backendConfig.gui = { language: 'en-US', displayMode };
      backendConfig.notifications = {
        rules: [{ id: 'info', topic: '#', level: 'info', title: '{title}', body: '{body}' }],
      };
      const { container } = render(<App />);
      await waitFor(() => expect(useAppStore.getState().status.state).toBe('Connected'));

      const checkInputs = () => {
        const inputs = container.querySelectorAll(
          'input:not([type="checkbox"]):not([type="radio"]):not([aria-hidden="true"]), textarea:not([aria-hidden="true"])'
        );
        expect(inputs.length).toBeGreaterThan(0);
        for (const input of inputs) {
          expect(input).toHaveAttribute('autocomplete', 'off');
          expect(input).toHaveAttribute('autocorrect', 'off');
          expect(input).toHaveAttribute('autocapitalize', 'none');
          expect(input).toHaveAttribute('spellcheck', 'false');
          expect(input).toHaveAttribute('writingsuggestions', 'false');
        }
      };

      checkInputs(); // Topic filter and multiline composer, both with custom input props.
      const options = await screen.findByRole('button', { name: i18n.t('composer.options') });
      if (options.getAttribute('aria-expanded') !== 'true') await userEvent.click(options);
      expect(await screen.findByLabelText('Title')).toBeInTheDocument();
      checkInputs();

      await userEvent.click(screen.getByLabelText('Settings (F10)'));
      for (const category of ['Broker', 'Topics', 'Notifications', 'History']) {
        await userEvent.click(screen.getByRole('tab', { name: category }));
        checkInputs();
        if (category === 'Broker') {
          expect(screen.getByLabelText('Password')).toHaveAttribute('type', 'password');
          await userEvent.click(screen.getByLabelText(i18n.t('settings.showPassword')));
          expect(screen.getByLabelText('Password')).toHaveAttribute('type', 'text');
          checkInputs();
        } else if (category === 'Topics' || category === 'Notifications') {
          await userEvent.click(
            screen.getByRole('button', {
              name: i18n.t(category === 'Topics' ? 'settings.addSubscription' : 'settings.addRule'),
            })
          );
          checkInputs();
        }
      }
    }
  );

  describe('the Editor switches', () => {
    const options = [
      ['autoComplete', 'autocomplete', 'on', 'off'],
      ['autoCorrect', 'autocorrect', 'on', 'off'],
      ['autoCapitalize', 'autocapitalize', 'sentences', 'none'],
      ['spellCheck', 'spellcheck', 'true', 'false'],
      ['writingSuggestions', 'writingsuggestions', 'true', 'false'],
    ] as const;
    const checkInputs = (enabled: readonly string[]) => {
      const inputs = document.querySelectorAll(
        'input:not([type="checkbox"]):not([type="radio"]):not([aria-hidden="true"]), textarea:not([aria-hidden="true"])'
      );
      expect(inputs.length).toBeGreaterThan(0);
      for (const input of inputs) {
        for (const [key, attribute, on, off] of options) {
          expect(input).toHaveAttribute(attribute, enabled.includes(key) ? on : off);
        }
      }
    };
    const flushConfig = () =>
      act(async () => {
        await useAppStore.getState().flushConfig();
      });
    const openEditor = async () => {
      await userEvent.click(screen.getByLabelText('Settings (F10)'));
      await userEvent.click(screen.getByRole('tab', { name: 'Editor' }));
    };

    // One case per switch rather than one loop over all five: each pass mounts the
    // whole window and a second settings panel, which is more than a shared CI runner
    // reliably finishes inside one test's time budget.
    it.each(options.map(([key]) => key))('applies %s independently and saves it', async (key) => {
      render(<App />);
      await waitFor(() => expect(useAppStore.getState().config).not.toBeNull());
      await openEditor();

      await userEvent.click(screen.getByRole('checkbox', { name: i18n.t(`settings.${key}`) }));
      checkInputs([key]);
      expect(screen.getAllByRole('checkbox', { checked: true })).toHaveLength(1);
      await flushConfig();
      expect(backendConfig.gui?.editor?.[key]).toBe(true);
      await userEvent.click(screen.getByRole('tab', { name: 'Broker' }));
      checkInputs([key]); // Newly mounted settings fields use the same policy.

      await userEvent.click(screen.getByRole('tab', { name: 'Editor' }));
      await userEvent.click(screen.getByRole('checkbox', { name: i18n.t(`settings.${key}`) }));
      checkInputs([]);
      await flushConfig();
      expect(backendConfig.gui?.editor?.[key]).toBe(false);
    });

    it('restores every switch on startup', async () => {
      const view = render(<App />);
      await waitFor(() => expect(useAppStore.getState().config).not.toBeNull());
      await openEditor();
      for (const [key] of options) {
        await userEvent.click(screen.getByRole('checkbox', { name: i18n.t(`settings.${key}`) }));
      }
      await flushConfig();
      view.unmount();

      useAppStore.setState({ config: null, tabSettingsStatus: Protocol.ControlStatus.Hidden });
      render(<App />);
      await waitFor(() => expect(useAppStore.getState().config?.gui?.editor?.writingSuggestions).toBe(true));
      checkInputs(options.map(([key]) => key));
      await openEditor();
      expect(screen.getAllByRole('checkbox', { checked: true })).toHaveLength(5);
    });
  });

  it.each(Protocol.LANGUAGES)('loads the saved %s language throughout the window', async (language) => {
    backendConfig.gui = { language };
    render(<App />);

    await waitFor(() => expect(useAppStore.getState().config?.gui?.language).toBe(language));
    expect(document.documentElement.lang).toBe(language);
    const t = i18n.getFixedT(language);
    expect(screen.getByRole('tab', { name: t('tabs.messages') })).toBeInTheDocument();
    expect(screen.getByLabelText(t('toolbar.settings'))).toBeInTheDocument();
    expect(screen.getByText(t('footer.state.Connected'))).toBeInTheDocument();
    expect(screen.getByLabelText(t('topics.filter'))).toBeInTheDocument();
    expect(screen.getByLabelText(t('composer.send'))).toBeInTheDocument();
    expect(screen.getByText('info')).toBeInTheDocument(); // A topic name is user data.
  });

  it('applies language and theme immediately, then saves their wire values automatically', async () => {
    render(<App />);
    await waitFor(() => expect(useAppStore.getState().config).not.toBeNull());
    await userEvent.click(screen.getByLabelText('Settings (F10)'));
    await userEvent.click(screen.getByRole('tab', { name: 'Appearance' }));
    await userEvent.click(screen.getByRole('combobox', { name: 'Language' }));
    expect(screen.getAllByRole('option')).toHaveLength(9);
    await userEvent.click(screen.getByRole('option', { name: 'Deutsch' }));
    expect(i18n.resolvedLanguage).toBe('de');
    expect(backendConfig.gui?.language).toBe('en-US');
    expect(screen.getByRole('tab', { name: 'Nachrichten' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Farbschema' })).toHaveTextContent('Ozean');
    expect(screen.getAllByLabelText('Schließen').length).toBeGreaterThan(0);
    expect(document.documentElement.lang).toBe('de');

    await userEvent.click(screen.getByRole('combobox', { name: 'Farbschema' }));
    await userEvent.click(screen.getByRole('option', { name: 'Wald' }));
    expect(useAppStore.getState().config?.gui?.theme).toBe('Forest');
    await waitFor(() => expect(backendConfig.gui?.theme).toBe('Forest'));
    expect(invoke).toHaveBeenCalledWith(
      'set_config',
      expect.objectContaining({
        config: expect.objectContaining({ gui: expect.objectContaining({ language: 'de', theme: 'Forest' }) }),
      })
    );
    expect(useAppStore.getState().dialogNotification).toBeNull();
  });

  it('keeps immediate changes visible on a save failure and retries on the next edit', async () => {
    render(<App />);
    await waitFor(() => expect(useAppStore.getState().config).not.toBeNull());
    await userEvent.click(screen.getByLabelText('Settings (F10)'));
    await userEvent.click(screen.getByRole('tab', { name: 'Appearance' }));
    failSave = true;
    await userEvent.click(screen.getByRole('combobox', { name: 'Language' }));
    await userEvent.click(screen.getByRole('option', { name: '日本語' }));
    expect(i18n.resolvedLanguage).toBe('ja');
    expect(await screen.findByText('Cannot write config')).toBeInTheDocument();
    expect(backendConfig.gui?.language).toBe('en-US');
    expect(screen.getByRole('tab', { name: 'メッセージ' })).toBeInTheDocument();
    failSave = false;
    failTray = false;
    await userEvent.click(screen.getByRole('combobox', { name: 'テーマ' }));
    await userEvent.click(screen.getByRole('option', { name: '森' }));
    await waitFor(() => expect(backendConfig.gui?.language).toBe('ja'));
    expect(backendConfig.gui?.theme).toBe('Forest');
  });
});

it('handles uppercase Ctrl+W on keydown and cancels the webview shortcut', async () => {
  render(<App />);
  await waitFor(() => expect(useAppStore.getState().config).not.toBeNull());
  fireEvent.keyDown(document, { key: 'F10' });
  await waitFor(() => expect(useAppStore.getState().tabSettingsStatus).toBe(Protocol.ControlStatus.Visible));
  const event = new KeyboardEvent('keydown', { key: 'W', ctrlKey: true, bubbles: true, cancelable: true });
  fireEvent(document, event);
  expect(event.defaultPrevented).toBe(true);
  expect(useAppStore.getState().tabSettingsStatus).toBe(Protocol.ControlStatus.Hidden);
});
