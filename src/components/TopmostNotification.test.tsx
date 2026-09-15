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

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import * as Service from '../lib/service';
import type { TopmostSnapshot } from '../lib/protocol';
import TopmostNotification from './TopmostNotification';

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('../lib/service', () => ({
  getTopmostNotification: vi.fn(),
  closeTopmostNotification: vi.fn(),
  readyTopmostNotification: vi.fn(),
}));
let receive: (event: { payload: TopmostSnapshot }) => void;
const stop = vi.fn();
const content = (revision: number, body: string): TopmostSnapshot => ({
  revision,
  body,
  title: `Message ${revision}`,
  level: 'error',
  closeLabel: 'Close',
});

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listen).mockImplementation(async (_name, handler) => {
    receive = handler as typeof receive;
    return stop;
  });
  vi.mocked(Service.getTopmostNotification).mockResolvedValue(content(1, 'first'));
  vi.mocked(Service.closeTopmostNotification).mockResolvedValue(undefined);
  vi.mocked(Service.readyTopmostNotification).mockResolvedValue(undefined);
});
afterEach(cleanup);

it('replaces the same dialog with the latest message and dismisses that revision', async () => {
  render(<TopmostNotification />);
  expect(await screen.findByText('first')).toBeInTheDocument();
  await waitFor(() => expect(Service.readyTopmostNotification).toHaveBeenCalledWith(1));
  act(() => {
    receive({ payload: content(2, 'second') });
    receive({ payload: content(3, '<img src=x onerror=alert(1)>') });
  });
  expect(screen.getAllByRole('alertdialog')).toHaveLength(1);
  expect(screen.queryByText('first')).not.toBeInTheDocument();
  expect(screen.queryByText('second')).not.toBeInTheDocument();
  expect(screen.getByText('<img src=x onerror=alert(1)>')).toBeInTheDocument();
  expect(document.querySelector('img')).toBeNull();
  await waitFor(() => expect(Service.readyTopmostNotification).toHaveBeenCalledWith(3));
  await userEvent.click(screen.getByRole('button', { name: 'Close' }));
  expect(Service.closeTopmostNotification).toHaveBeenLastCalledWith(3);
  fireEvent.keyDown(document, { key: 'Escape' });
  expect(Service.closeTopmostNotification).toHaveBeenCalledTimes(2);
});

it('does not replace a live update with an older initial read or out-of-order event', async () => {
  let resolve!: (value: TopmostSnapshot) => void;
  vi.mocked(Service.getTopmostNotification).mockReturnValue(
    new Promise((done) => {
      resolve = done;
    })
  );
  render(<TopmostNotification />);
  await waitFor(() => expect(Service.getTopmostNotification).toHaveBeenCalled());
  act(() => receive({ payload: content(5, 'latest') }));
  await act(async () => resolve(content(1, 'stale')));
  act(() => receive({ payload: content(4, 'also stale') }));
  expect(screen.getByText('latest')).toBeInTheDocument();
  expect(screen.queryByText('stale')).not.toBeInTheDocument();
  expect(screen.queryByText('also stale')).not.toBeInTheDocument();
});

it('unsubscribes on close and reports dismissal failures in the dialog', async () => {
  vi.mocked(Service.closeTopmostNotification).mockRejectedValue(new Error('Cannot close window'));
  const view = render(<TopmostNotification />);
  await screen.findByText('first');
  await userEvent.click(screen.getByRole('button', { name: 'Close' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Cannot close window');
  view.unmount();
  expect(stop).toHaveBeenCalledOnce();
});
