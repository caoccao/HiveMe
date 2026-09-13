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

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { ThemeProvider, alpha, createTheme } from '@mui/material/styles';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import i18n, { changeLanguage } from '../i18n';
import { useAppStore } from '../lib/store';
import type { MessageRow } from '../lib/protocol';
import { Tier } from '../lib/protocol';
import { Bubble, JsonTree } from './MessageView';

vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ writeText: vi.fn(async () => undefined) }));
beforeEach(() => vi.clearAllMocks());

function openDetails() {
  fireEvent.click(screen.getByLabelText(i18n.t('messages.actions')));
}

function row(overrides: Partial<MessageRow> = {}): MessageRow {
  const raw = JSON.stringify({
    v: 1,
    id: 'msg-1',
    ts: '2026-09-12T09:41:23.512Z',
    type: 'message',
    sender: { id: 'device-1', name: 'sams-laptop', app: 'hmc' },
    payload: { body: 'Build finished', title: 'CI', level: 'info' },
  });
  return {
    rowId: 1,
    topic: 'hiveme',
    id: 'msg-1',
    ts: '2026-09-12T09:41:23.512Z',
    receivedTs: '2026-09-12T09:41:23.512Z',
    senderId: 'device-1',
    senderName: 'sams-laptop',
    app: 'hmc',
    tier: Tier.Envelope,
    level: 'info',
    title: 'CI',
    body: 'Build finished',
    raw,
    rawLength: raw.length,
    qos: 1,
    retain: false,
    outgoing: false,
    ...overrides,
  };
}

/** The flex alignment of the row that holds the bubble. */
function alignmentOf(container: HTMLElement): string {
  return getComputedStyle(container.firstElementChild as Element).justifyContent;
}

describe('bubble alignment', () => {
  it('puts a message this device sent on the right', () => {
    const { container } = render(<Bubble row={row({ outgoing: true })} />);
    expect(alignmentOf(container)).toBe('flex-end');
  });

  it('puts a message from anyone else on the left, with the sender named', () => {
    const { container } = render(<Bubble row={row()} />);
    expect(alignmentOf(container)).toBe('flex-start');
    expect(screen.getByText('sams-laptop')).toBeInTheDocument();
    expect(screen.getByText('hmc')).toBeInTheDocument();
  });
});

describe('message severity colors', () => {
  it.each(['light', 'dark'] as const)('colors the whole bubble by payload level in %s mode', (mode) => {
    const theme = createTheme({ palette: { mode } });
    render(
      <ThemeProvider theme={theme}>
        {['info', 'warn', 'error', 'debug'].flatMap((level) => [false, true].map((outgoing) => (
          <Bubble key={`${level}-${outgoing}`} row={row({ level, outgoing })} />
        )))}
      </ThemeProvider>
    );
    const bubbles = screen.getAllByRole('article').map((article) => article.querySelector('.message-bubble')!);
    for (const index of [0, 6]) {
      expect(bubbles[index]).toHaveStyle({ backgroundColor: theme.palette.action.hover });
      expect(bubbles[index + 1]).toHaveStyle({
        backgroundColor: mode === 'dark' ? theme.palette.grey[800] : theme.palette.common.black,
        color: theme.palette.common.white,
      });
    }
    for (const [index, severity] of [[2, 'warning'], [4, 'error']] as const) {
      for (const bubble of bubbles.slice(index, index + 2)) {
        expect(bubble).toHaveStyle({
          backgroundColor: alpha(theme.palette[severity].main, mode === 'dark' ? 0.24 : 0.12),
          borderColor: theme.palette[severity].main,
        });
      }
    }
  });
});

describe('the tiers', () => {
  it('renders an envelope with its title, body, and level', () => {
    render(<Bubble row={row()} />);
    expect(screen.getByText('CI')).toBeInTheDocument();
    expect(screen.getByText('Build finished')).toBeInTheDocument();
    openDetails();
    expect(screen.getByText('info')).toBeInTheDocument();
  });

  it('shows a level it does not know beside the level it displays', () => {
    const raw = JSON.stringify({
      v: 1,
      id: 'msg-2',
      ts: '2026-09-12T09:41:23.512Z',
      payload: { body: 'from a newer writer', level: 'catastrophe' },
    });
    render(<Bubble row={row({ raw, rawLength: raw.length, level: 'catastrophe', title: null })} />);
    openDetails();
    expect(screen.getByText('catastrophe (info)')).toBeInTheDocument();
  });

  it('marks an envelope written by a newer HiveMe', () => {
    const raw = JSON.stringify({
      v: 99,
      id: 'msg-3',
      ts: '2026-09-12T09:41:23.512Z',
      payload: { body: 'written by a future HiveMe' },
    });
    render(<Bubble row={row({ raw, rawLength: raw.length, title: null })} />);
    openDetails();
    expect(screen.getByText('newer version')).toBeInTheDocument();
  });

  it('shows an encrypted message as a placeholder rather than as unreadable JSON', () => {
    const raw = JSON.stringify({
      v: 1,
      id: 'msg-4',
      ts: '2026-09-12T09:41:23.512Z',
      enc: { alg: 'A256GCM', kid: 'k-2026-09', iv: 'u2m1xwK7Ck3NoMbz' },
      ciphertext: '8Qy0m5jI1n1F0y7b',
    });
    render(<Bubble row={row({ raw, rawLength: raw.length, body: 'encrypted (key k-2026-09)', title: null })} />);
    expect(screen.getByText('encrypted (key k-2026-09)')).toBeInTheDocument();
  });

  it('shows a raw text payload verbatim', () => {
    render(
      <Bubble row={row({ tier: Tier.Text, raw: 'plain text from some other tool', body: 'x', title: null })} />
    );
    expect(screen.getByText('plain text from some other tool')).toBeInTheDocument();
  });

  it('localizes encrypted previews instead of displaying the stored English placeholder', async () => {
    await changeLanguage('de');
    const raw = JSON.stringify({
      v: 1, id: 'encrypted', ts: '2026-09-12T09:41:23Z',
      enc: { alg: 'A256GCM', kid: 'key-42', iv: 'nonce' }, ciphertext: 'ciphertext',
      payload: { body: 'Ignore plaintext when encryption is present' },
    });
    render(<Bubble row={row({ raw, body: 'encrypted (key key-42)', level: 'warn' })} />);
    expect(screen.getByText('verschlüsselt (Schlüssel key-42)')).toBeInTheDocument();
    openDetails();
    expect(screen.getByText('Warnung')).toBeInTheDocument();
    expect(screen.queryByText('encrypted (key key-42)')).not.toBeInTheDocument();
    expect(screen.queryByText('Ignore plaintext when encryption is present')).not.toBeInTheDocument();
  });

  it('keeps user messages and unknown levels verbatim while translating the fallback level', async () => {
    await changeLanguage('ja');
    render(<Bubble row={row({ tier: Tier.Text, raw: 'Original text', level: 'custom-level' })} />);
    expect(screen.getByText('Original text')).toBeInTheDocument();
    openDetails();
    expect(screen.getByText('custom-level（情報）')).toBeInTheDocument();
  });

  it('shows a payload that is not text as hex with its size', () => {
    render(<Bubble row={row({ tier: Tier.Bytes, raw: 'fffe00', rawLength: 3, body: '3 bytes', title: null })} />);
    expect(screen.getByText('3 bytes')).toBeInTheDocument();
    expect(screen.getByText('ff fe 00')).toBeInTheDocument();
  });

  it('marks a retained message', () => {
    render(<Bubble row={row({ retain: true })} />);
    expect(screen.getByLabelText('Message actions')).toBeInTheDocument();
    openDetails();
    expect(screen.getByTestId('PushPinIcon')).toBeInTheDocument();
  });
});

describe('message controls', () => {
  it('keeps time and actions outside the bubble and hidden at rest', () => {
    render(<Bubble row={row()} />);
    const article = screen.getByRole('article');
    const controls = article.querySelector('.message-controls');
    expect(controls).toHaveStyle({ opacity: 0, pointerEvents: 'none' });
    expect(controls).toContainElement(article.querySelector('time'));
    expect(controls).toContainElement(screen.getByLabelText('Copy body'));
    expect(article.querySelector('.message-bubble')).not.toContainElement(article.querySelector('time'));
    expect(screen.queryByText('QoS 1')).not.toBeInTheDocument();
  });

  it('copies the body directly and reports success', async () => {
    const message = row({ body: 'Original text\nwith a second line' });
    render(<Bubble row={message} />);
    fireEvent.click(screen.getByLabelText('Copy body'));
    await waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith(message.body));
    expect(useAppStore.getState().dialogNotification?.title).toBe(i18n.t('messages.copiedBody'));
  });

  it('keeps controls visible while the menu is open and copies the original JSON', async () => {
    const message = row({ retain: true });
    render(<Bubble row={message} />);
    openDetails();
    expect(document.querySelector('.message-controls')).toHaveStyle({ opacity: 1, pointerEvents: 'auto' });
    expect(screen.getByText('QoS 1')).toBeInTheDocument();
    expect(screen.getByText('Retained by the broker')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('menuitem', { name: 'Copy JSON' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith(message.raw));
  });

  it('reports a clipboard failure without changing the message', async () => {
    vi.mocked(writeText).mockRejectedValueOnce(new Error('Clipboard unavailable'));
    render(<Bubble row={row()} />);
    fireEvent.click(screen.getByLabelText('Copy body'));
    await waitFor(() => expect(useAppStore.getState().dialogNotification?.title).toBe('Clipboard unavailable'));
    expect(screen.getByText('Build finished')).toBeInTheDocument();
  });
});

describe('the JSON tree', () => {
  it('shows the top level open and the branches below it closed', () => {
    render(<JsonTree value={{ stage: 'deploy', nested: { deep: true } }} />);
    expect(screen.getByText('stage:')).toBeInTheDocument();
    expect(screen.getByText('nested: {1}')).toBeInTheDocument();
    expect(screen.queryByText('deep:')).not.toBeInTheDocument();
  });

  it('counts the entries of an array', () => {
    render(<JsonTree value={['a', 'b', 'c']} />);
    expect(screen.getByText('[3]')).toBeInTheDocument();
  });
});
