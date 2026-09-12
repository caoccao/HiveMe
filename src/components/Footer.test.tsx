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
import { beforeEach, describe, expect, it } from 'vitest';
import i18n from '../i18n';
import * as Protocol from '../lib/protocol';
import { INITIAL_STATUS, useAppStore } from '../lib/store';
import Footer from './Footer';

beforeEach(() => {
  useAppStore.setState({ status: INITIAL_STATUS });
});

describe('the status bar', () => {
  // The backend sends the state as one of these four names, and the footer is the one
  // place that turns a name into a word. A missing key renders the name itself, which
  // reads close enough to be believed while the rest of the GUI is treating the state
  // as unknown, so the table is checked rather than the rendering.
  it('has a word for every state the backend can send', () => {
    for (const state of Object.values(Protocol.ConnectionState)) {
      expect(i18n.exists(`footer.state.${state}`)).toBe(true);
    }
  });

  it('shows the connected state as connected', () => {
    useAppStore.setState({
      status: { ...INITIAL_STATUS, state: Protocol.ConnectionState.Connected, host: 'broker.example', port: 8883 },
    });

    render(<Footer />);

    expect(screen.getByText('connected')).toBeInTheDocument();
    expect(screen.getByText('broker.example:8883')).toBeInTheDocument();
  });

  // The color is read across the room, before the word is. Green is working, gray is
  // stopped, and amber is on its way, so a state that arrives under a name the badge
  // does not know must not be able to borrow the green.
  it.each([
    [Protocol.ConnectionState.Connected, 'MuiChip-colorSuccess'],
    [Protocol.ConnectionState.Disconnected, 'MuiChip-colorDefault'],
    [Protocol.ConnectionState.Connecting, 'MuiChip-colorWarning'],
    [Protocol.ConnectionState.Reconnecting, 'MuiChip-colorWarning'],
  ])('colors the %s badge', (state, color) => {
    useAppStore.setState({ status: { ...INITIAL_STATUS, state } });

    const { container } = render(<Footer />);

    expect(container.querySelector('.MuiChip-root')).toHaveClass(color);
  });
});
