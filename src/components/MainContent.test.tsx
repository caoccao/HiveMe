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

import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as Protocol from '../lib/protocol';
import * as Service from '../lib/service';
import { INITIAL_STATUS, useAppStore } from '../lib/store';
import MainContent from './MainContent';

vi.mock('../lib/service', () => ({
  setConfig: vi.fn(async (config: Protocol.Config) => config),
  getStatus: vi.fn(async () => INITIAL_STATUS),
  getUpdateResult: vi.fn(async () => null),
  getAbout: vi.fn(async () => ({ appVersion: '0.1.0', configPath: '/test/config', databasePath: '/test/history' })),
}));

const initialState = useAppStore.getState();
const observers = new Set<TestResizeObserver>();

// Keep the actual virtualizer. jsdom has no layout, so supply browser geometry,
// including the zero-sized measurements produced by display:none ancestors.
function hasLayout(element: HTMLElement): boolean {
  for (let node: HTMLElement | null = element; node; node = node.parentElement) {
    if (getComputedStyle(node).display === 'none') return false;
  }
  return true;
}

class TestResizeObserver {
  targets = new Set<HTMLElement>();
  constructor(private callback: ResizeObserverCallback) {
    observers.add(this);
  }
  observe(target: HTMLElement) {
    this.targets.add(target);
  }
  unobserve(target: HTMLElement) {
    this.targets.delete(target);
  }
  disconnect() {
    this.targets.clear();
    observers.delete(this);
  }
  deliver() {
    const entries = [...this.targets]
      .filter((target) => target.isConnected)
      .map(
        (target) =>
          ({
            target,
            borderBoxSize: [{ inlineSize: target.offsetWidth, blockSize: target.offsetHeight }],
          }) as unknown as ResizeObserverEntry
      );
    if (entries.length) this.callback(entries, this as unknown as ResizeObserver);
  }
}

function resize() {
  act(() => {
    for (const observer of observers) observer.deliver();
  });
}

function message(rowId: number): Protocol.MessageRow {
  return {
    rowId,
    topic: 'hiveme',
    id: String(rowId),
    ts: '2026-09-17T10:00:00Z',
    receivedTs: '2026-09-17T10:00:00Z',
    senderId: null,
    senderName: null,
    app: null,
    tier: Protocol.Tier.Text,
    level: 'info',
    title: null,
    body: `Message ${rowId}`,
    raw: `Message ${rowId}`,
    rawLength: 10,
    qos: 1,
    retain: false,
    outgoing: false,
  };
}

const originalScrollTo = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollTo');

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] });
  observers.clear();
  vi.stubGlobal('ResizeObserver', TestResizeObserver);
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return hasLayout(this) ? (this.hasAttribute('data-index') ? 120 + (Number(this.dataset.index) % 3) * 20 : 400) : 0;
  });
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockImplementation(function (this: HTMLElement) {
    return hasLayout(this) ? 800 : 0;
  });
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return this.offsetHeight;
  });
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return hasLayout(this)
      ? Math.max(
          this.clientHeight,
          this.firstElementChild ? parseFloat(getComputedStyle(this.firstElementChild).height) || 0 : 0
        )
      : 0;
  });
  Object.defineProperty(HTMLElement.prototype, 'scrollTo', {
    configurable: true,
    value: function (this: HTMLElement, options: ScrollToOptions) {
      const top = Math.max(0, Math.min(options.top ?? 0, this.scrollHeight - this.clientHeight));
      if (this.scrollTop !== top) {
        this.scrollTop = top;
        queueMicrotask(() => {
          if (this.isConnected) this.dispatchEvent(new Event('scroll'));
        });
      }
    },
  });
  useAppStore.setState(
    {
      ...initialState,
      config: { version: 1, broker: { keepAliveSecs: 30, password: 'test-password' } },
      status: { ...INITIAL_STATUS, state: Protocol.ConnectionState.Connected },
      loadedTopics: new Set(['hiveme']),
      messages: new Map([['hiveme', Array.from({ length: 60 }, (_, index) => message(index + 1))]]),
    },
    true
  );
});

afterEach(async () => {
  cleanup();
  await useAppStore.getState().flushConfig();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  if (originalScrollTo) Object.defineProperty(HTMLElement.prototype, 'scrollTo', originalScrollTo);
  else Reflect.deleteProperty(HTMLElement.prototype, 'scrollTo');
});

async function openTab(name: 'Settings' | 'About') {
  await act(async () => {
    if (name === 'Settings') useAppStore.getState().setTabSettingsStatus(Protocol.ControlStatus.Selected);
    else useAppStore.getState().setTabAboutStatus(Protocol.ControlStatus.Selected);
  });
}

function selectTab(name: string) {
  fireEvent.click(screen.getByRole('tab', { name }));
}

function closeTab(name: string) {
  fireEvent.click(within(screen.getByRole('tab', { name: new RegExp(name) })).getByRole('button', { name: 'Close' }));
}

function messageScroller(): HTMLElement {
  return screen.getByRole('tabpanel', { name: 'Messages' }).querySelector('[data-index]')!.parentElement!
    .parentElement!;
}

async function settleScroll() {
  // The virtualizer reconciles dynamic row measurements over animation frames.
  await act(async () => {
    await vi.advanceTimersByTimeAsync(160);
  });
}

describe('tab state in memory', () => {
  it('preserves the measured message viewport and reading position across hidden-tab resize observations and arrivals', async () => {
    render(<MainContent />);
    const scroller = messageScroller();
    await settleScroll();
    await waitFor(() => expect(scroller.scrollTop).toBeGreaterThan(0));
    fireEvent.scroll(scroller, { target: { scrollTop: 2400 } });
    resize();
    await settleScroll();
    const offset = scroller.scrollTop;
    const height = scroller.scrollHeight;
    const firstIndex = scroller.querySelector('[data-index]')!.getAttribute('data-index');
    await openTab('Settings');
    resize();
    expect(scroller.clientHeight).toBe(400);
    expect(scroller.scrollHeight).toBe(height);
    expect(scroller.scrollTop).toBe(offset);
    expect(scroller.querySelector('[data-index]')!.getAttribute('data-index')).toBe(firstIndex);
    act(() => useAppStore.getState().receiveMessage(message(61)));
    await openTab('About');
    resize();
    selectTab('Messages');
    expect(messageScroller()).toBe(scroller);
    expect(scroller.scrollTop).toBe(offset);
    expect(scroller.querySelector('[data-index]')!.getAttribute('data-index')).toBe(firstIndex);
  });

  it('continues following new messages only when the reader left the message view at the bottom', async () => {
    render(<MainContent />);
    const scroller = messageScroller();
    await settleScroll();
    await waitFor(() => expect(scroller.scrollTop).toBe(scroller.scrollHeight - scroller.clientHeight));
    const oldBottom = scroller.scrollTop;
    await openTab('Settings');
    resize();
    act(() => useAppStore.getState().receiveMessage(message(61)));
    selectTab('Messages');
    await settleScroll();
    await waitFor(() => {
      expect(scroller.scrollTop).toBeGreaterThan(oldBottom);
      expect(scroller.scrollTop).toBe(scroller.scrollHeight - scroller.clientHeight);
    });
  });

  it('retains composer draft, selection, options, filter, tree expansion, and pane scroll across tabs', async () => {
    useAppStore.setState({
      topics: [
        {
          id: 'hiveme',
          label: 'hiveme',
          topic: 'hiveme',
          unread: 0,
          messages: 1,
          children: [
            { id: 'hiveme/child', label: 'child', topic: 'hiveme/child', unread: 0, messages: 1, children: [] },
          ],
        },
      ],
    });
    render(<MainContent />);
    const filter = screen.getByPlaceholderText('Filter topics');
    fireEvent.change(filter, { target: { value: 'hiveme' } });
    const root = screen.getByRole('treeitem', { name: /hiveme/ });
    fireEvent.click(root.querySelector('.MuiTreeItem-iconContainer')!);
    const expanded = root.getAttribute('aria-expanded');
    const treeScroller = root.closest('[role="tree"]')!.parentElement!;
    treeScroller.scrollTop = 75;
    const draft = screen.getByRole('textbox', { name: /Write a message/ }) as HTMLTextAreaElement;
    fireEvent.change(draft, { target: { value: 'unfinished message' } });
    draft.setSelectionRange(3, 8);
    fireEvent.click(screen.getByRole('button', { name: 'More Options' }));
    await openTab('Settings');
    await openTab('About');
    selectTab('Messages');
    expect(draft).toHaveValue('unfinished message');
    expect([draft.selectionStart, draft.selectionEnd]).toEqual([3, 8]);
    expect(screen.getByRole('button', { name: 'More Options' })).toHaveAttribute('aria-expanded', 'true');
    expect(filter).toHaveValue('hiveme');
    expect(root).toHaveAttribute('aria-expanded', expanded);
    expect(treeScroller.scrollTop).toBe(75);
  });

  it('keeps Settings category, unfinished numeric input, password visibility, and scroll after closing and reopening', async () => {
    render(<MainContent />);
    await openTab('Settings');
    selectTab('Broker');
    fireEvent.click(screen.getByRole('button', { name: 'Show the password' }));
    const password = screen.getByLabelText('Password');
    const keepAlive = screen.getByLabelText('Keep alive (s)');
    fireEvent.change(keepAlive, { target: { value: '' } });
    const broker = screen.getByRole('tabpanel', { name: 'Broker' });
    broker.scrollTop = 215;
    closeTab('Settings');
    expect(password).not.toBeVisible();
    expect(screen.queryByRole('tabpanel', { name: 'Broker' })).not.toBeInTheDocument();
    await openTab('Settings');
    expect(screen.getByRole('tab', { name: 'Broker', selected: true })).toBeInTheDocument();
    expect(screen.getByLabelText('Keep alive (s)')).toBe(keepAlive);
    expect(keepAlive).toHaveValue(null);
    expect(password).toHaveAttribute('type', 'text');
    expect(password).toBeVisible();
    expect(screen.getByRole('tabpanel', { name: 'Broker' }).scrollTop).toBe(215);
    expect(useAppStore.getState().config?.broker?.keepAliveSecs).toBe(30);
  });

  it('keeps About scroll and loaded content when closed and reopened, and selects tabs by identity', async () => {
    render(<MainContent />);
    expect(Service.getAbout).not.toHaveBeenCalled();
    await openTab('Settings');
    await openTab('About');
    const about = screen.getByRole('tabpanel', { name: 'About' });
    about.scrollTop = 155;
    closeTab('Settings');
    expect(screen.getByRole('tab', { name: /About/, selected: true })).toBeInTheDocument();
    await openTab('Settings');
    selectTab('About');
    closeTab('About');
    expect(screen.getByRole('tab', { name: 'Messages', selected: true })).toBeInTheDocument();
    await openTab('About');
    expect(screen.getByRole('tabpanel', { name: 'About' })).toBe(about);
    expect(about.scrollTop).toBe(155);
    expect(Service.getAbout).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(document, { key: '1', ctrlKey: true });
    expect(screen.getByRole('tab', { name: 'Messages', selected: true })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Tab', ctrlKey: true, shiftKey: true });
    expect(screen.getByRole('tab', { name: /About/, selected: true })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Tab', ctrlKey: true });
    expect(screen.getByRole('tab', { name: 'Messages', selected: true })).toBeInTheDocument();
  });
});
