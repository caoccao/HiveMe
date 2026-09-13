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

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createTheme, ThemeProvider } from '@mui/material/styles';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConnectionState } from '../lib/protocol';
import { INITIAL_STATUS, useAppStore } from '../lib/store';
import Composer, { isSendKey } from './Composer';

function connected(publish = vi.fn().mockResolvedValue(true)) {
  useAppStore.setState({
    status: { ...INITIAL_STATUS, state: ConnectionState.Connected },
    selectedTopic: 'hiveme',
    publish,
  });
  return publish;
}

beforeEach(() => {
  useAppStore.setState({ status: INITIAL_STATUS, selectedTopic: null });
});

describe('isSendKey', () => {
  it('sends on Enter alone', () => {
    expect(isSendKey({ key: 'Enter', shiftKey: false, ctrlKey: false, altKey: false })).toBe(true);
  });

  it('does not send when a modifier is held, so that Shift+Enter adds a line', () => {
    expect(isSendKey({ key: 'Enter', shiftKey: true, ctrlKey: false, altKey: false })).toBe(false);
    expect(isSendKey({ key: 'Enter', shiftKey: false, ctrlKey: true, altKey: false })).toBe(false);
    expect(isSendKey({ key: 'Enter', shiftKey: false, ctrlKey: false, altKey: true })).toBe(false);
  });

  it('ignores every other key', () => {
    expect(isSendKey({ key: 'a', shiftKey: false, ctrlKey: false, altKey: false })).toBe(false);
  });
});

describe('the composer', () => {
  it('is disabled while nothing is connected', () => {
    render(<Composer />);
    expect(screen.getByLabelText(/Write a message/)).toBeDisabled();
  });

  it('is disabled while no topic is selected', () => {
    useAppStore.setState({ status: { ...INITIAL_STATUS, state: ConnectionState.Connected } });
    render(<Composer />);
    expect(screen.getByLabelText(/Write a message/)).toBeDisabled();
  });

  it('publishes to the selected topic when Enter is pressed', async () => {
    const publish = connected();
    render(<Composer />);

    const input = screen.getByLabelText(/Write a message/);
    await userEvent.type(input, 'Build finished');
    await userEvent.keyboard('{Enter}');

    expect(publish).toHaveBeenCalledWith(
      'hiveme',
      'Build finished',
      expect.objectContaining({ topic: null, json: false, qos: null, retain: false, title: null })
    );
  });

  it('clears the input once the message is away', async () => {
    connected();
    render(<Composer />);

    const input = screen.getByLabelText(/Write a message/);
    await userEvent.type(input, 'Build finished');
    await userEvent.keyboard('{Enter}');

    expect(input).toHaveValue('');
  });

  it.each([
    ['textbox', /Write a message/], ['textbox', 'Topic'], ['textbox', 'Title'],
    ['radio', 'Config'], ['radio', '0'], ['radio', '1'], ['radio', '2'],
    ['checkbox', 'Retain Message'], ['checkbox', 'As Raw JSON'],
    ['combobox', 'Level'],
    ['button', 'More Options'], ['button', 'Send'],
  ] as const)('sends exactly once on Enter from the focused %s %s', async (role, name) => {
    // Keep the body after sending so accidental button activation would send twice.
    const publish = connected(vi.fn().mockResolvedValue(false));
    render(<Composer />);
    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));
    fireEvent.change(screen.getByLabelText(/Write a message/), { target: { value: 'Build finished' } });
    const control = screen.getByRole(role, { name });
    act(() => control.focus());
    expect(control).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    expect(publish).toHaveBeenCalledExactlyOnceWith('hiveme', 'Build finished', {
      topic: null, title: null, qos: null, retain: false, json: false, level: 'info',
    });
    expect(screen.getByRole('button', { name: 'More Options' })).toHaveAttribute('aria-expanded', 'true');
  });

  it('keeps what the user typed when the publish failed', async () => {
    connected(vi.fn().mockResolvedValue(false));
    render(<Composer />);

    const input = screen.getByLabelText(/Write a message/);
    await userEvent.type(input, 'Build finished');
    await userEvent.keyboard('{Enter}');

    expect(input).toHaveValue('Build finished');
  });

  it('adds a line instead of sending when Shift is held', async () => {
    const publish = connected();
    render(<Composer />);

    const input = screen.getByLabelText(/Write a message/);
    await userEvent.type(input, 'first');
    await userEvent.keyboard('{Shift>}{Enter}{/Shift}');
    await userEvent.type(input, 'second');

    expect(publish).not.toHaveBeenCalled();
    expect(input).toHaveValue('first\nsecond');
  });

  it('sends nothing when the body is only whitespace', async () => {
    const publish = connected();
    render(<Composer />);

    const input = screen.getByLabelText(/Write a message/);
    await userEvent.type(input, '   ');
    await userEvent.keyboard('{Enter}');

    expect(publish).not.toHaveBeenCalled();
  });

  it('carries the title the user typed', async () => {
    const publish = connected();
    render(<Composer />);

    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));
    await userEvent.type(screen.getByLabelText('Title'), 'CI');
    await userEvent.type(screen.getByLabelText(/Write a message/), 'Build finished');
    await userEvent.keyboard('{Enter}');

    expect(publish).toHaveBeenCalledWith('hiveme', 'Build finished', expect.objectContaining({ title: 'CI' }));
    expect(screen.getByLabelText('Title')).toHaveValue('CI');
  });

  it('starts collapsed and reveals the three option rows with their defaults', async () => {
    connected();
    render(<Composer />);
    const toggle = screen.getByRole('button', { name: 'More Options' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByLabelText('Topic')).not.toBeInTheDocument();
    expect(screen.getByLabelText(/Write a message/).tagName).toBe('TEXTAREA');

    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByLabelText('Topic')).toHaveValue('');
    expect(screen.getByLabelText('Title')).toHaveValue('');
    expect(screen.getByRole('radiogroup', { name: 'QoS' })).toBeVisible();
    expect(screen.getAllByRole('radio').map((radio) => radio.getAttribute('value'))).toEqual(['config', '0', '1', '2']);
    expect(screen.getByRole('radio', { name: 'Config' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Retain Message' })).not.toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'As Raw JSON' })).not.toBeChecked();
  });

  it.each([['Info', 'info'], ['Error', 'error'], ['Success', 'success'], ['Warn', 'warn']])(
    'sends the selected %s level while More Options is collapsed', async (label, level) => {
      const publish = connected();
      render(<Composer />);
      const select = screen.getByRole('combobox', { name: 'Level' });
      expect(select).toHaveTextContent('Info');
      await userEvent.click(select);
      expect(screen.getAllByRole('option').map(option => option.textContent)).toEqual(['Info', 'Error', 'Success', 'Warn']);
      await userEvent.click(screen.getByRole('option', { name: label }));
      await userEvent.type(screen.getByLabelText(/Write a message/), 'Build finished');
      await userEvent.click(screen.getByRole('button', { name: 'Send' }));
      expect(publish).toHaveBeenCalledExactlyOnceWith('hiveme', 'Build finished', expect.objectContaining({ level }));
      expect(select).toHaveTextContent(label);
      expect(screen.getByRole('button', { name: 'More Options' })).toHaveAttribute('aria-expanded', 'false');
    }
  );

  it('uses Enter to choose an open menu option, then sends from the closed level control', async () => {
    const publish = connected();
    render(<Composer />);
    fireEvent.change(screen.getByLabelText(/Write a message/), { target: { value: 'Build finished' } });
    const select = screen.getByRole('combobox', { name: 'Level' });
    await userEvent.click(select);
    await userEvent.keyboard('{ArrowDown}{Enter}');
    expect(select).toHaveTextContent('Error');
    expect(publish).not.toHaveBeenCalled();
    act(() => select.focus());
    await userEvent.keyboard('{Enter}');
    expect(publish).toHaveBeenCalledExactlyOnceWith('hiveme', 'Build finished', expect.objectContaining({ level: 'error' }));
  });

  it.each(['light', 'dark'] as const)('colors the selected level and all dropdown options in %s mode', async (mode) => {
    connected();
    const theme = createTheme({ palette: { mode } });
    render(<ThemeProvider theme={theme}><Composer /></ThemeProvider>);
    const select = screen.getByRole('combobox', { name: 'Level' });
    expect(select).toHaveStyle({ color: theme.palette.text.primary });
    const colors = {
      Info: theme.palette.text.primary, Error: theme.palette.error.main,
      Success: theme.palette.success.main, Warn: theme.palette.warning.main,
    };
    for (const [label, color] of Object.entries(colors)) {
      await userEvent.click(select);
      const option = screen.getByRole('option', { name: label });
      expect(option).toHaveStyle({ color });
      await userEvent.click(option);
      expect(select).toHaveStyle({ color });
    }
  });

  it.each([['Config', null], ['0', 0], ['1', 1], ['2', 2]] as const)(
    'sends using QoS %s and keeps overrides active while collapsed',
    async (label, qos) => {
      const publish = connected();
      render(<Composer />);
      const toggle = screen.getByRole('button', { name: 'More Options' });
      await userEvent.click(toggle);
      await userEvent.type(screen.getByLabelText('Topic'), 'custom/build');
      await userEvent.type(screen.getByLabelText('Title'), 'CI');
      await userEvent.click(screen.getByRole('radio', { name: label }));
      await userEvent.click(screen.getByRole('checkbox', { name: 'Retain Message' }));
      await userEvent.click(toggle);
      await waitFor(() => expect(screen.queryByLabelText('Topic')).not.toBeInTheDocument());
      await userEvent.type(screen.getByLabelText(/Write a message/), 'Build finished');
      await userEvent.click(screen.getByRole('button', { name: 'Send' }));

      expect(publish).toHaveBeenCalledWith('hiveme', 'Build finished', {
        topic: 'custom/build', json: false, qos, retain: true, title: 'CI', level: 'info',
      });
      expect(screen.getByLabelText(/Write a message/)).toHaveValue('');
      expect(toggle).toHaveAttribute('aria-expanded', 'false');
      await userEvent.click(toggle);
      expect(screen.getByLabelText('Topic')).toHaveValue('custom/build');
      expect(screen.getByLabelText('Title')).toHaveValue('CI');
      expect(screen.getByRole('radio', { name: label })).toBeChecked();
      expect(screen.getByRole('checkbox', { name: 'Retain Message' })).toBeChecked();
    }
  );

  it('keeps raw JSON active while collapsed and preserves the unused title and level', async () => {
    const publish = connected();
    render(<Composer />);
    const toggle = screen.getByRole('button', { name: 'More Options' });
    await userEvent.click(toggle);
    await userEvent.type(screen.getByLabelText('Title'), 'Saved title');
    await userEvent.click(screen.getByRole('combobox', { name: 'Level' }));
    await userEvent.click(screen.getByRole('option', { name: 'Success' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'As Raw JSON' }));
    expect(screen.getByLabelText('Title')).toBeDisabled();
    expect(screen.getByRole('combobox', { name: 'Level' })).toHaveAttribute('aria-disabled', 'true');
    await userEvent.click(toggle);
    fireEvent.change(screen.getByLabelText(/Write a JSON payload/), { target: { value: '{"ok":true}' } });
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(publish).toHaveBeenCalledWith('hiveme', '{"ok":true}', expect.objectContaining({ json: true, title: null, level: null, retain: false }));
    await userEvent.click(toggle);
    expect(screen.getByRole('checkbox', { name: 'As Raw JSON' })).toBeChecked();
    await userEvent.click(screen.getByRole('checkbox', { name: 'As Raw JSON' }));
    expect(screen.getByLabelText('Title')).toHaveValue('Saved title');
    expect(screen.getByLabelText('Title')).toBeEnabled();
    expect(screen.getByRole('combobox', { name: 'Level' })).toHaveTextContent('Success');
    expect(screen.getByRole('combobox', { name: 'Level' })).not.toHaveAttribute('aria-disabled', 'true');
  });

  it.each(['ci', '/ci', '///ci'])('normalizes %s and sends it relative to the selected topic', async (input) => {
    const publish = connected();
    useAppStore.setState({ selectedTopic: 'hiveme/build' });
    render(<Composer />);
    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));
    fireEvent.change(screen.getByLabelText('Topic'), { target: { value: input } });
    expect(screen.getByLabelText('Topic')).toHaveValue('ci');
    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));
    await userEvent.type(screen.getByLabelText(/Write a message/), 'Relative message');
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(publish).toHaveBeenCalledWith('hiveme/build', 'Relative message', expect.objectContaining({ topic: 'ci' }));
  });

  it('restores drafts, options, and expansion per selected topic, including after disconnection', async () => {
    connected();
    render(<Composer />);
    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));
    await userEvent.type(screen.getByLabelText('Topic'), 'custom/build');
    await userEvent.type(screen.getByLabelText('Title'), 'Root title');
    await userEvent.click(screen.getByRole('combobox', { name: 'Level' }));
    await userEvent.click(screen.getByRole('option', { name: 'Success' }));
    await userEvent.click(screen.getByRole('radio', { name: '2' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'Retain Message' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'As Raw JSON' }));
    fireEvent.change(screen.getByLabelText(/Write a JSON payload/), { target: { value: '{"root":true}' } });
    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));

    act(() => useAppStore.setState({ selectedTopic: 'hiveme/child' }));
    expect(screen.getByLabelText(/Write a message/)).toHaveValue('');
    expect(screen.getByRole('combobox', { name: 'Level' })).toHaveTextContent('Info');
    expect(screen.getByRole('button', { name: 'More Options' })).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));
    expect(screen.getByLabelText('Topic')).toHaveValue('');
    expect(screen.getByLabelText('Title')).toHaveValue('');
    expect(screen.getByRole('radio', { name: 'Config' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Retain Message' })).not.toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'As Raw JSON' })).not.toBeChecked();
    await userEvent.type(screen.getByLabelText('Title'), 'Child title');
    await userEvent.click(screen.getByRole('combobox', { name: 'Level' }));
    await userEvent.click(screen.getByRole('option', { name: 'Warn' }));
    await userEvent.type(screen.getByLabelText(/Write a message/), 'Child draft');
    act(() => useAppStore.setState({ status: INITIAL_STATUS }));
    act(() => useAppStore.setState({ status: { ...INITIAL_STATUS, state: ConnectionState.Connected }, selectedTopic: 'hiveme' }));

    expect(screen.getByLabelText(/Write a JSON payload/)).toHaveValue('{"root":true}');
    expect(screen.getByRole('combobox', { name: 'Level' })).toHaveTextContent('Success');
    expect(screen.getByRole('button', { name: 'More Options' })).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));
    expect(screen.getByLabelText('Topic')).toHaveValue('custom/build');
    expect(screen.getByLabelText('Title')).toHaveValue('Root title');
    expect(screen.getByRole('radio', { name: '2' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Retain Message' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'As Raw JSON' })).toBeChecked();

    act(() => useAppStore.setState({ selectedTopic: 'hiveme/child' }));
    expect(screen.getByRole('button', { name: 'More Options' })).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByLabelText('Title')).toHaveValue('Child title');
    expect(screen.getByRole('combobox', { name: 'Level' })).toHaveTextContent('Warn');
    expect(screen.getByLabelText(/Write a message/)).toHaveValue('Child draft');
  });

  it('clears only the originating draft when the user switches topics during a send', async () => {
    let finish!: (value: boolean) => void;
    const publish = connected(vi.fn(() => new Promise<boolean>((resolve) => { finish = resolve; })));
    render(<Composer />);
    await userEvent.type(screen.getByLabelText(/Write a message/), 'Root draft');
    act(() => useAppStore.setState({ selectedTopic: 'hiveme/child' }));
    await userEvent.type(screen.getByLabelText(/Write a message/), 'Child draft');
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    act(() => useAppStore.setState({ selectedTopic: 'hiveme' }));
    await act(async () => finish(true));
    expect(publish).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText(/Write a message/)).toHaveValue('Root draft');
    act(() => useAppStore.setState({ selectedTopic: 'hiveme/child' }));
    expect(screen.getByLabelText(/Write a message/)).toHaveValue('');
  });

  it.each([/Write a message/, 'Topic', 'Title'])('does not send Enter used to finish IME composition in %s', async (name) => {
    const publish = connected();
    render(<Composer />);
    await userEvent.click(screen.getByRole('button', { name: 'More Options' }));
    const input = screen.getByLabelText(/Write a message/);
    fireEvent.change(input, { target: { value: '日本語' } });
    fireEvent.keyDown(screen.getByLabelText(name), { key: 'Enter', isComposing: true });
    expect(publish).not.toHaveBeenCalled();
  });
});
