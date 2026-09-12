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
import type { TopicNode } from '../lib/protocol';
import { useAppStore } from '../lib/store';
import TopicTree, { allNodeIds, filterNodes } from './TopicTree';

function node(id: string, label: string, topic: string | null, unread = 0, children: TopicNode[] = []): TopicNode {
  return { id, label, topic, unread, messages: 0, children };
}

const TREE: TopicNode[] = [
  node('hiveme', 'hiveme', null, 3, [
    node('hiveme/build', 'build', null, 1, [node('hiveme/build/ci', 'ci', 'hiveme/build/ci', 1)]),
    node('hiveme/error', 'error', 'hiveme/error', 2),
    node('hiveme/info', 'info', 'hiveme/info', 0),
  ]),
];

beforeEach(() => {
  useAppStore.setState({ topics: TREE, topicFilter: '', selectedTopic: null });
});

describe('filterNodes', () => {
  it('hands back everything when the filter is blank', () => {
    expect(filterNodes(TREE, '   ')).toBe(TREE);
  });

  it('keeps a parent whose child matches, because hiding it would hide the match', () => {
    const filtered = filterNodes(TREE, 'ci');
    expect(filtered).toHaveLength(1);
    expect(filtered[0].children.map((child) => child.id)).toEqual(['hiveme/build']);
    expect(filtered[0].children[0].children[0].id).toBe('hiveme/build/ci');
  });

  it('matches on the whole path, not only the label', () => {
    expect(filterNodes(TREE, 'hiveme/err')).toHaveLength(1);
    expect(filterNodes(TREE, 'hiveme/err')[0].children.map((child) => child.id)).toEqual(['hiveme/error']);
  });

  it('is case insensitive', () => {
    expect(filterNodes(TREE, 'ERROR')[0].children).toHaveLength(1);
  });

  it('hands back nothing when nothing matches', () => {
    expect(filterNodes(TREE, 'nowhere')).toEqual([]);
  });
});

describe('allNodeIds', () => {
  it('lists every node, which is what expands the tree while a filter is typed', () => {
    expect(allNodeIds(TREE)).toEqual([
      'hiveme',
      'hiveme/build',
      'hiveme/build/ci',
      'hiveme/error',
      'hiveme/info',
    ]);
  });
});

describe('the topic tree', () => {
  it('renders the hierarchy with its unread badges', () => {
    render(<TopicTree />);

    expect(screen.getByText('hiveme')).toBeInTheDocument();
    expect(screen.getByText('info')).toBeInTheDocument();
    expect(screen.getByText('error')).toBeInTheDocument();
    // The badge of `hiveme` carries the two of `error` plus the one of `ci`.
    expect(screen.getByText('3')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('says so when there is nothing to show', () => {
    useAppStore.setState({ topics: [] });
    render(<TopicTree />);
    expect(screen.getByText(/No messages yet/)).toBeInTheDocument();
  });

  it('says when a filter matched nothing', () => {
    useAppStore.setState({ topicFilter: 'nowhere' });
    render(<TopicTree />);
    expect(screen.getByText(/No topic matches/)).toBeInTheDocument();
  });

  it('selects a node that is a real topic', async () => {
    const selectTopic = vi.fn();
    useAppStore.setState({ selectTopic });
    render(<TopicTree />);

    await userEvent.click(screen.getByText('error'));

    expect(selectTopic).toHaveBeenCalledWith('hiveme/error');
  });

  it('does not select an intermediate segment, which has no history of its own', async () => {
    const selectTopic = vi.fn();
    useAppStore.setState({ selectTopic });
    render(<TopicTree />);

    await userEvent.click(screen.getByText('build'));

    expect(selectTopic).not.toHaveBeenCalled();
  });
});
