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
import { ConnectionState } from '../lib/protocol';
import { INITIAL_STATUS, useAppStore } from '../lib/store';
import Composer, { isSendKey } from './Composer';

function connected(publish = vi.fn().mockResolvedValue(true)) {
  useAppStore.setState({
    status: { ...INITIAL_STATUS, state: ConnectionState.Connected },
    selectedTopic: 'hiveme/info',
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
      'hiveme/info',
      'Build finished',
      expect.objectContaining({ json: false, qos: null, retain: null, title: null })
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

    await userEvent.type(screen.getByLabelText(/Title/), 'CI');
    await userEvent.type(screen.getByLabelText(/Write a message/), 'Build finished');
    await userEvent.keyboard('{Enter}');

    expect(publish).toHaveBeenCalledWith('hiveme/info', 'Build finished', expect.objectContaining({ title: 'CI' }));
  });
});
