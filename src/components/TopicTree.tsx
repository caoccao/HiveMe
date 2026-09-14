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

import { useEffect, useMemo, useState } from 'react';
import { Badge, Box, TextField, Typography } from '@mui/material';
import { SimpleTreeView } from '@mui/x-tree-view/SimpleTreeView';
import { TreeItem } from '@mui/x-tree-view/TreeItem';
import { useTranslation } from 'react-i18next';
import type { TopicNode } from '../lib/protocol';
import { formatNumber } from '../lib/format';
import { STARTUP_TOPIC } from '../lib/constants';
import { useAppStore } from '../lib/store';

/**
 * Keeps the nodes whose path, or whose descendants' paths, contain `filter`.
 *
 * A parent is kept when a child matches, because hiding it would hide the match with
 * it. Exported for the component test.
 */
export function filterNodes(nodes: TopicNode[], filter: string): TopicNode[] {
  const needle = filter.trim().toLowerCase();
  if (needle === '') {
    return nodes;
  }
  const keep = (node: TopicNode): TopicNode | null => {
    const children = node.children.map(keep).filter((child): child is TopicNode => child !== null);
    if (children.length > 0 || node.id.toLowerCase().includes(needle)) {
      return { ...node, children };
    }
    return null;
  };
  return nodes.map(keep).filter((node): node is TopicNode => node !== null);
}

/** Every node id in the tree, which is what expands it while a filter is typed. */
export function allNodeIds(nodes: TopicNode[]): string[] {
  return nodes.flatMap((node) => [node.id, ...allNodeIds(node.children)]);
}

export default function TopicTree() {
  const { t } = useTranslation();
  const storedTopics = useAppStore((state) => state.topics);
  // The startup topic is always available to publish to, even without stored history.
  // Merge it with the existing root so historical children and counts stay intact.
  const topics = useMemo(() => {
    if (storedTopics.some((node) => node.id === STARTUP_TOPIC)) {
      return storedTopics.map((node) => (node.id === STARTUP_TOPIC ? { ...node, topic: STARTUP_TOPIC } : node));
    }
    return [
      { id: STARTUP_TOPIC, label: STARTUP_TOPIC, topic: STARTUP_TOPIC, unread: 0, messages: 0, children: [] },
      ...storedTopics,
    ];
  }, [storedTopics]);
  const filter = useAppStore((state) => state.topicFilter);
  const setFilter = useAppStore((state) => state.setTopicFilter);
  const selectedTopic = useAppStore((state) => state.selectedTopic);
  const selectTopic = useAppStore((state) => state.selectTopic);
  const [expanded, setExpanded] = useState<string[]>([]);
  const [touched, setTouched] = useState(false);

  const filtered = useMemo(() => filterNodes(topics, filter), [topics, filter]);
  const visible = filtered.some((node) => node.id === STARTUP_TOPIC)
    ? filtered
    : [{ ...topics.find((node) => node.id === STARTUP_TOPIC)!, children: [] }, ...filtered];

  // A filter is only useful when what it matched is visible, and the first tree a user
  // sees should be open rather than a single collapsed root.
  useEffect(() => {
    if (filter.trim() !== '') {
      setExpanded(allNodeIds(filtered));
      return;
    }
    if (!touched && topics.length > 0) {
      setExpanded(allNodeIds(topics));
    }
  }, [filter, filtered, topics, touched]);

  const render = (node: TopicNode) => (
    <TreeItem
      key={node.id}
      itemId={node.id}
      aria-selected={node.topic ? node.id === selectedTopic : undefined}
      sx={{
        '& > .MuiTreeItem-content[data-selected]': { bgcolor: 'action.selected', color: 'primary.main' },
        '& > .MuiTreeItem-content[data-selected]:hover': { bgcolor: 'action.selected' },
      }}
      label={
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, pr: 1, minWidth: 0 }}>
          <Typography
            variant="body2"
            sx={{
              flex: 1,
              minWidth: 0,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              fontStyle: node.topic ? 'normal' : 'italic',
              color: node.id === selectedTopic ? 'primary.main' : node.topic ? 'text.primary' : 'text.secondary',
              fontWeight: node.id === selectedTopic ? 600 : 400,
            }}
          >
            {node.label}
          </Typography>
          {node.unread > 0 && (
            <Badge
              badgeContent={formatNumber(Math.min(node.unread, 999)) + (node.unread > 999 ? '+' : '')}
              aria-label={t('topics.unread', { count: node.unread })}
              color="primary"
              max={999}
              sx={{ mr: 1.5, '& .MuiBadge-badge': { position: 'static', transform: 'none' } }}
            />
          )}
        </Box>
      }
    >
      {node.children.map(render)}
    </TreeItem>
  );

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', minHeight: 0, flex: 1, gap: '4px', p: '4px' }}>
      <TextField
        placeholder={t('topics.filter')}
        value={filter}
        onChange={(event) => setFilter(event.target.value)}
        size="small"
        fullWidth
        slotProps={{ htmlInput: { 'aria-label': t('topics.filter') } }}
      />
      <Box sx={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
        {filtered.length === 0 && (
          <Typography variant="caption" color="text.secondary" sx={{ p: 1, display: 'block' }}>
            {t('topics.noMatch')}
          </Typography>
        )}
        <SimpleTreeView
          aria-label={t('settings.topics')}
          expansionTrigger="iconContainer"
          expandedItems={expanded}
          selectedItems={selectedTopic ?? null}
          onExpandedItemsChange={(_, items) => {
            setTouched(true);
            setExpanded(items);
          }}
          onSelectedItemsChange={(_, itemId) => {
            if (typeof itemId !== 'string') {
              return;
            }
            // Parent paths select their complete subtree, even without direct messages.
            const found = findNode(visible, itemId);
            if (found?.topic) {
              selectTopic(found.topic);
            }
          }}
        >
          {visible.map(render)}
        </SimpleTreeView>
      </Box>
    </Box>
  );
}

function findNode(nodes: TopicNode[], id: string): TopicNode | null {
  for (const node of nodes) {
    if (node.id === id) {
      return node;
    }
    const found = findNode(node.children, id);
    if (found) {
      return found;
    }
  }
  return null;
}
