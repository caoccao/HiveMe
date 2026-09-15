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

import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { ThemeProvider, alpha, createTheme } from '@mui/material/styles';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import i18n, { changeLanguage } from '../i18n';
import { useAppStore } from '../lib/store';
import type { MessageRow } from '../lib/protocol';
import { Tier } from '../lib/protocol';
import MessageView, { Bubble, JsonTree } from './MessageView';

const virtual = vi.hoisted(() => ({
  scrollToOffset: vi.fn(),
  scrollToIndex: vi.fn(),
  element: null as (() => HTMLElement | null) | null,
  key: null as ((index: number) => number) | null,
}));
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (options: {
    count: number;
    getScrollElement: () => HTMLElement | null;
    getItemKey: (index: number) => number;
  }) => {
    virtual.element = options.getScrollElement;
    virtual.key = options.getItemKey;
    return {
      getTotalSize: () => options.count * 88,
      getVirtualItems: () => [],
      scrollToOffset: virtual.scrollToOffset,
      scrollToIndex: virtual.scrollToIndex,
      measureElement: vi.fn(),
    };
  },
}));

vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ writeText: vi.fn(async () => undefined) }));
beforeEach(() => vi.clearAllMocks());

function openCopyMenu() {
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
    const { container } = render(<Bubble selectedTopic="hiveme" row={row({ outgoing: true })} />);
    expect(alignmentOf(container)).toBe('flex-end');
  });

  it('puts a message from anyone else on the left, with the sender named', () => {
    const { container } = render(<Bubble selectedTopic="hiveme" row={row()} />);
    expect(alignmentOf(container)).toBe('flex-start');
    expect(screen.getByText('sams-laptop')).toBeInTheDocument();
    expect(screen.queryByText('hmc')).not.toBeInTheDocument();
  });
});

describe('message sender and topic labels', () => {
  it('keeps the sender above the bubble and the relative topic before the level below it', () => {
    render(<Bubble selectedTopic="hiveme/a" row={row({ topic: 'hiveme/a/b/c' })} />);
    const article = screen.getByRole('article');
    const header = article.querySelector('header')!;
    expect(header).toHaveTextContent(/^sams-laptop$/);
    expect(header).toBeVisible();
    expect(header.nextElementSibling).toBe(article.querySelector('.message-bubble'));
    const footer = article.querySelector('footer')!;
    const topic = within(footer).getByText('b/c');
    expect(footer.previousElementSibling).toBe(article.querySelector('.message-bubble'));
    expect(topic.nextElementSibling).toHaveTextContent(/^Info$/);
    expect(topic).not.toBeVisible();
    expect(footer).toHaveStyle({ opacity: '0', pointerEvents: 'none' });
  });

  it('shows only the relative topic on an outgoing child message', () => {
    render(<Bubble selectedTopic="hiveme/a" row={row({ topic: 'hiveme/a/b/c', outgoing: true })} />);
    expect(screen.getByRole('article').querySelector('header')).toBeNull();
    expect(screen.getByRole('article').querySelector('footer .message-topic')).toHaveTextContent(/^b\/c$/);
    expect(screen.queryByText('sams-laptop')).not.toBeInTheDocument();
    expect(screen.queryByText('hmc')).not.toBeInTheDocument();
  });

  it('omits the header for an outgoing message on the selected topic', () => {
    render(<Bubble selectedTopic="hiveme" row={row({ outgoing: true })} />);
    expect(screen.getByRole('article').querySelector('header')).toBeNull();
  });

  it.each([
    JSON.stringify({
      v: 1,
      id: 'anonymous',
      ts: '2026-09-12T09:41:23Z',
      sender: { app: 'hmg' },
      payload: { body: 'Anonymous message' },
    }),
    JSON.stringify({ body: 'Raw JSON' }),
    'Raw text',
  ])('shows only the topic when the payload has no sender identity: %s', (raw) => {
    const message = row({
      raw,
      rawLength: raw.length,
      topic: 'hiveme/a/b/c',
      senderId: null,
      senderName: null,
      app: null,
    });
    const { rerender } = render(<Bubble selectedTopic="hiveme/a" row={message} />);
    expect(screen.getByRole('article').querySelector('header')).toBeNull();
    expect(screen.getByRole('article').querySelector('footer .message-topic')).toHaveTextContent(/^b\/c$/);
    expect(screen.queryByText(i18n.t('messages.unknownSender'))).not.toBeInTheDocument();
    rerender(<Bubble selectedTopic="hiveme/a/b/c" row={message} />);
    expect(screen.getByRole('article').querySelector('header')).toBeNull();
    expect(screen.getByRole('article').querySelector('.message-topic')).toBeNull();
  });

  it('updates the path with selection and keeps the sender when the path becomes empty', () => {
    const message = row({ topic: 'hiveme/a/b/c' });
    const { rerender } = render(<Bubble selectedTopic="hiveme" row={message} />);
    expect(screen.getByRole('article').querySelector('.message-topic')).toHaveTextContent(/^a\/b\/c$/);
    rerender(<Bubble selectedTopic="hiveme/a" row={message} />);
    expect(screen.getByRole('article').querySelector('.message-topic')).toHaveTextContent(/^b\/c$/);
    rerender(<Bubble selectedTopic="hiveme/a/b/c" row={message} />);
    expect(screen.getByRole('article').querySelector('.message-topic')).toBeNull();
    expect(screen.getByRole('article').querySelector('header')).toHaveTextContent('sams-laptop');
  });
});

describe('message severity colors', () => {
  it.each(['light', 'dark'] as const)('colors the whole bubble by payload level in %s mode', (mode) => {
    const theme = createTheme({ palette: { mode } });
    render(
      <ThemeProvider theme={theme}>
        {['info', 'warn', 'error', 'success'].flatMap((level) =>
          [false, true].map((outgoing) => (
            <Bubble selectedTopic="hiveme" key={`${level}-${outgoing}`} row={row({ level, outgoing })} />
          ))
        )}
      </ThemeProvider>
    );
    const bubbles = screen.getAllByRole('article').map((article) => article.querySelector('.message-bubble')!);
    expect(bubbles[0]).toHaveStyle({ backgroundColor: theme.palette.action.hover });
    expect(bubbles[1]).toHaveStyle({
      backgroundColor: mode === 'dark' ? theme.palette.grey[800] : theme.palette.common.black,
      color: theme.palette.common.white,
    });
    for (const [index, severity] of [
      [2, 'warning'],
      [4, 'error'],
      [6, 'success'],
    ] as const) {
      for (const bubble of bubbles.slice(index, index + 2)) {
        expect(bubble).toHaveStyle({
          backgroundColor: alpha(theme.palette[severity].main, mode === 'dark' ? 0.24 : 0.12),
          borderColor: theme.palette[severity].main,
        });
        expect(bubble.parentElement!.querySelector('.MuiChip-root')).toHaveClass(
          `MuiChip-color${severity[0].toUpperCase() + severity.slice(1)}`
        );
      }
    }
  });
});

describe('the tiers', () => {
  it('renders an envelope with its title, body, and level', () => {
    render(<Bubble selectedTopic="hiveme" row={row()} />);
    expect(screen.getByText('CI')).toBeInTheDocument();
    expect(screen.getByText('Build finished')).toBeInTheDocument();
    expect(screen.getByText('Info')).toBeInTheDocument();
  });

  it('shows a level it does not know beside the level it displays', () => {
    const raw = JSON.stringify({
      v: 1,
      id: 'msg-2',
      ts: '2026-09-12T09:41:23.512Z',
      payload: { body: 'from a newer writer', level: 'catastrophe' },
    });
    render(
      <Bubble selectedTopic="hiveme" row={row({ raw, rawLength: raw.length, level: 'catastrophe', title: null })} />
    );
    expect(screen.getByText('catastrophe (Info)')).toBeInTheDocument();
  });

  it('marks an envelope written by a newer HiveMe', () => {
    const raw = JSON.stringify({
      v: 99,
      id: 'msg-3',
      ts: '2026-09-12T09:41:23.512Z',
      payload: { body: 'written by a future HiveMe' },
    });
    render(<Bubble selectedTopic="hiveme" row={row({ raw, rawLength: raw.length, title: null })} />);
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
    render(
      <Bubble
        selectedTopic="hiveme"
        row={row({ raw, rawLength: raw.length, body: 'encrypted (key k-2026-09)', title: null })}
      />
    );
    expect(screen.getByText('encrypted (key k-2026-09)')).toBeInTheDocument();
  });

  it('shows a raw text payload verbatim', () => {
    render(
      <Bubble
        selectedTopic="hiveme"
        row={row({ tier: Tier.Text, raw: 'plain text from some other tool', body: 'x', title: null })}
      />
    );
    expect(screen.getByText('plain text from some other tool')).toBeInTheDocument();
  });

  it('localizes encrypted previews instead of displaying the stored English placeholder', async () => {
    await changeLanguage('de');
    const raw = JSON.stringify({
      v: 1,
      id: 'encrypted',
      ts: '2026-09-12T09:41:23Z',
      enc: { alg: 'A256GCM', kid: 'key-42', iv: 'nonce' },
      ciphertext: 'ciphertext',
      payload: { body: 'Ignore plaintext when encryption is present' },
    });
    render(<Bubble selectedTopic="hiveme" row={row({ raw, body: 'encrypted (key key-42)', level: 'warn' })} />);
    expect(screen.getByText('verschlüsselt (Schlüssel key-42)')).toBeInTheDocument();
    expect(screen.getByText('Warnung')).toBeInTheDocument();
    expect(screen.queryByText('encrypted (key key-42)')).not.toBeInTheDocument();
    expect(screen.queryByText('Ignore plaintext when encryption is present')).not.toBeInTheDocument();
  });

  it('keeps user messages and unknown levels verbatim while translating the fallback level', async () => {
    await changeLanguage('ja');
    render(
      <Bubble selectedTopic="hiveme" row={row({ tier: Tier.Text, raw: 'Original text', level: 'custom-level' })} />
    );
    expect(screen.getByText('Original text')).toBeInTheDocument();
    expect(screen.getByText('custom-level（情報）')).toBeInTheDocument();
  });

  it('shows a payload that is not text as hex with its size', () => {
    render(
      <Bubble
        selectedTopic="hiveme"
        row={row({ tier: Tier.Bytes, raw: 'fffe00', rawLength: 3, body: '3 bytes', title: null })}
      />
    );
    expect(screen.getByText('3 bytes')).toBeInTheDocument();
    expect(screen.getByText('ff fe 00')).toBeInTheDocument();
  });

  it('marks a retained message', () => {
    render(<Bubble selectedTopic="hiveme" row={row({ retain: true })} />);
    expect(screen.getByLabelText('Retained by the broker')).toBeInTheDocument();
    expect(screen.getByTestId('PushPinIcon')).toBeInTheDocument();
  });
});

describe('message controls', () => {
  it('keeps time and actions outside the bubble and hidden at rest', () => {
    render(<Bubble selectedTopic="hiveme" row={row()} />);
    const article = screen.getByRole('article');
    const controls = article.querySelector('.message-controls');
    expect(controls).toHaveStyle({ opacity: 0, pointerEvents: 'none' });
    expect(controls).toContainElement(article.querySelector('time'));
    expect(controls).toContainElement(screen.getByLabelText('Copy'));
    expect(controls).toContainElement(screen.getByLabelText('Copy options'));
    expect(controls).toContainElement(screen.getByText('Info'));
    expect(controls).toContainElement(screen.getByText('QoS 1'));
    expect(article.querySelector('.message-bubble')).not.toContainElement(article.querySelector('time'));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('copies the body directly and reports success', async () => {
    const message = row({ body: 'Original text\nwith a second line' });
    render(<Bubble selectedTopic="hiveme" row={message} />);
    fireEvent.click(screen.getByLabelText('Copy'));
    await waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith(message.body));
    expect(useAppStore.getState().dialogNotification?.title).toBe(i18n.t('messages.copiedBody'));
  });

  it('copies the original JSON from the menu', async () => {
    const message = row({ retain: true });
    render(<Bubble selectedTopic="hiveme" row={message} />);
    openCopyMenu();
    expect(screen.getByText('QoS 1')).toBeInTheDocument();
    expect(screen.getByLabelText('Retained by the broker')).toBeInTheDocument();
    expect(screen.getAllByRole('menuitem').map((item) => item.textContent)).toEqual(['Copy', 'Copy Raw JSON']);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Copy Raw JSON' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith(message.raw));
  });

  it('copies the body from the dropdown menu', async () => {
    const message = row();
    render(<Bubble selectedTopic="hiveme" row={message} />);
    openCopyMenu();
    fireEvent.click(screen.getByRole('menuitem', { name: 'Copy' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith(message.body));
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());
  });

  it('reports a clipboard failure without changing the message', async () => {
    vi.mocked(writeText).mockRejectedValueOnce(new Error('Clipboard unavailable'));
    render(<Bubble selectedTopic="hiveme" row={row()} />);
    fireEvent.click(screen.getByLabelText('Copy'));
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

describe('history reading position', () => {
  it('anchors the same row after older rows are prepended and stops following the bottom', () => {
    const loadOlderMessages = vi.fn();
    const existing = [row({ rowId: 201 }), row({ rowId: 202 })];
    useAppStore.setState({
      selectedTopic: 'hiveme',
      messages: new Map([['hiveme', existing]]),
      hasOlder: new Map([['hiveme', true]]),
      loadOlderMessages,
    });
    render(<MessageView />);
    const scroller = virtual.element!()!;
    Object.defineProperties(scroller, {
      scrollTop: { value: 20, writable: true },
      scrollHeight: { value: 1000, configurable: true },
      clientHeight: { value: 400 },
    });
    fireEvent.scroll(scroller);
    expect(loadOlderMessages).toHaveBeenCalledWith('hiveme');
    virtual.scrollToIndex.mockClear();
    act(() =>
      useAppStore.setState({ messages: new Map([['hiveme', [row({ rowId: 199 }), row({ rowId: 200 }), ...existing]]]) })
    );
    expect(virtual.scrollToOffset).toHaveBeenLastCalledWith(20 + 2 * 88);
    expect(virtual.key!(2)).toBe(201);
    expect(virtual.scrollToIndex).not.toHaveBeenCalled();
    act(() =>
      useAppStore.setState({
        messages: new Map([['hiveme', [...useAppStore.getState().messages.get('hiveme')!, row({ rowId: 203 })]]]),
      })
    );
    expect(virtual.scrollToIndex).not.toHaveBeenCalled();
  });

  it('discards an outstanding history anchor when switching topics', () => {
    useAppStore.setState({
      selectedTopic: 'hiveme',
      messages: new Map([['hiveme', [row({ rowId: 10 })]]]),
      hasOlder: new Map([['hiveme', true]]),
      loadOlderMessages: vi.fn(),
    });
    render(<MessageView />);
    fireEvent.scroll(virtual.element!()!);
    act(() =>
      useAppStore.setState({
        selectedTopic: 'other',
        messages: new Map([['other', [row({ rowId: 3 }), row({ rowId: 10 })]]]),
      })
    );
    expect(virtual.scrollToOffset).not.toHaveBeenCalled();
  });
});
