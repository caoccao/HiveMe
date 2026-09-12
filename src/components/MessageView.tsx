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

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Box, Chip, IconButton, Menu, MenuItem, Tooltip, Typography } from '@mui/material';
import ExpandMoreIcon from '@mui/icons-material/ExpandMore';
import ChevronRightIcon from '@mui/icons-material/ChevronRight';
import LockIcon from '@mui/icons-material/Lock';
import MoreVertIcon from '@mui/icons-material/MoreVert';
import PushPinIcon from '@mui/icons-material/PushPin';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import * as Protocol from '../lib/protocol';
import { formatDay, formatDateTime, formatHex, formatTime } from '../lib/format';
import {
  displayedLevel,
  isEncrypted,
  isKnownLevel,
  isNewerVersion,
  parseRow,
  payloadOf,
  senderLabel,
  senderOf,
} from '../lib/message';
import { useAppStore } from '../lib/store';

/**
 * The page a topic with no history has.
 *
 * One shared array, because a selector that builds a new one each time it is called
 * hands the store a different snapshot on every render. React compares snapshots by
 * reference, so it would re-render for ever and then tear the tree down, leaving an
 * empty window with a backend that looks perfectly healthy.
 */
const NO_MESSAGES: Protocol.MessageRow[] = [];

/** How close to the bottom still counts as "following the conversation". */
const STICK_THRESHOLD_PX = 48;

/** How close to the top asks for the previous page. */
const LOAD_OLDER_THRESHOLD_PX = 64;

/** The chip colour of each level. */
function levelColour(level: Protocol.Level): 'default' | 'info' | 'warning' | 'error' {
  switch (level) {
    case Protocol.Level.Warn:
      return 'warning';
    case Protocol.Level.Error:
      return 'error';
    case Protocol.Level.Info:
      return 'info';
    default:
      return 'default';
  }
}

/** A collapsible JSON tree, which is how `data` and a raw JSON payload are shown. */
export function JsonTree({ value, name, depth = 0 }: { value: unknown; name?: string; depth?: number }) {
  const [open, setOpen] = useState(depth < 1);
  const isBranch = typeof value === 'object' && value !== null;

  if (!isBranch) {
    return (
      <Box sx={{ display: 'flex', gap: 0.5, pl: depth * 1.5, fontFamily: 'monospace', fontSize: '0.72rem' }}>
        {name !== undefined && <Box component="span" sx={{ color: 'text.secondary' }}>{name}:</Box>}
        <Box component="span">{JSON.stringify(value)}</Box>
      </Box>
    );
  }

  const entries = Array.isArray(value)
    ? value.map((item, index) => [String(index), item] as const)
    : Object.entries(value as Record<string, unknown>);

  return (
    <Box sx={{ pl: depth * 1.5 }}>
      <Box
        onClick={() => setOpen((previous) => !previous)}
        sx={{ display: 'flex', alignItems: 'center', cursor: 'pointer', fontFamily: 'monospace', fontSize: '0.72rem' }}
      >
        {open ? <ExpandMoreIcon sx={{ fontSize: 14 }} /> : <ChevronRightIcon sx={{ fontSize: 14 }} />}
        <Box component="span" sx={{ color: 'text.secondary' }}>
          {name !== undefined ? `${name}: ` : ''}
          {Array.isArray(value) ? `[${entries.length}]` : `{${entries.length}}`}
        </Box>
      </Box>
      {open &&
        entries.map(([key, item]) => <JsonTree key={key} name={key} value={item} depth={depth + 1} />)}
    </Box>
  );
}

/** One message bubble. Exported so the rendering of each tier can be tested. */
export function Bubble({ row }: { row: Protocol.MessageRow }) {
  const { t } = useTranslation();
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const notifyInfo = useAppStore((state) => state.notifyInfo);
  const notifyError = useAppStore((state) => state.notifyError);
  const parsed = useMemo(() => parseRow(row), [row]);
  const envelope = parsed.tier === Protocol.Tier.Envelope ? parsed.message : null;
  const payload = envelope ? payloadOf(envelope) : null;
  const sender = envelope ? senderOf(envelope) : null;
  const level = displayedLevel(row.level);

  const copy = async (text: string, message: string) => {
    setAnchor(null);
    try {
      await writeText(text);
      notifyInfo(message);
    } catch (error) {
      notifyError(error);
    }
  };

  return (
    <Box sx={{ display: 'flex', justifyContent: row.outgoing ? 'flex-end' : 'flex-start', px: 1, py: 0.5 }}>
      <Box
        sx={{
          maxWidth: '80%',
          minWidth: 160,
          border: 1,
          borderColor: 'divider',
          borderRadius: 2,
          px: 1.25,
          py: 0.75,
          bgcolor: row.outgoing ? 'action.selected' : 'background.paper',
        }}
      >
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.75, mb: 0.25 }}>
          {!row.outgoing && (
            <Typography variant="caption" sx={{ fontWeight: 600 }}>
              {senderLabel(sender) || t('messages.unknownSender')}
            </Typography>
          )}
          {sender?.app && !row.outgoing && (
            <Typography variant="caption" color="text.secondary">
              {sender.app}
            </Typography>
          )}
          <Box sx={{ flex: 1 }} />
          {envelope && isNewerVersion(envelope) && (
            <Chip size="small" variant="outlined" color="warning" label={t('messages.newerVersion')} />
          )}
          {row.retain && (
            <Tooltip title={t('messages.retained')}>
              <PushPinIcon sx={{ fontSize: 14, color: 'text.secondary' }} />
            </Tooltip>
          )}
          <Chip
            size="small"
            variant="outlined"
            color={levelColour(level)}
            label={isKnownLevel(row.level) ? level : `${row.level} (${level})`}
          />
          <IconButton
            size="small"
            aria-label={t('messages.actions')}
            sx={{ p: 0.25 }}
            onClick={(event) => setAnchor(event.currentTarget)}
          >
            <MoreVertIcon sx={{ fontSize: 14 }} />
          </IconButton>
        </Box>

        {envelope && isEncrypted(envelope) ? (
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.75 }}>
            <LockIcon sx={{ fontSize: 16 }} />
            <Typography variant="body2">{row.body}</Typography>
          </Box>
        ) : parsed.tier === Protocol.Tier.Envelope ? (
          <>
            {payload?.title && (
              <Typography variant="body2" sx={{ fontWeight: 700 }}>
                {payload.title}
              </Typography>
            )}
            <Typography variant="body2" sx={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
              {payload?.body ?? ''}
            </Typography>
            {payload?.data !== undefined && payload?.data !== null && (
              <Box sx={{ mt: 0.5 }}>
                <JsonTree name={t('messages.data')} value={payload.data} />
              </Box>
            )}
          </>
        ) : parsed.tier === Protocol.Tier.Json ? (
          <JsonTree value={parsed.value} />
        ) : parsed.tier === Protocol.Tier.Text ? (
          <Typography
            variant="body2"
            sx={{ fontFamily: 'monospace', whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}
          >
            {parsed.text}
          </Typography>
        ) : (
          <Box>
            <Typography variant="caption" color="text.secondary">
              {t('messages.bytes', { count: parsed.length })}
            </Typography>
            <Typography variant="body2" sx={{ fontFamily: 'monospace', whiteSpace: 'pre-wrap' }}>
              {formatHex(parsed.hex)}
            </Typography>
          </Box>
        )}

        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mt: 0.25 }}>
          <Box sx={{ flex: 1 }} />
          <Typography variant="caption" color="text.secondary">
            {t('messages.qos', { qos: row.qos })}
          </Typography>
          <Tooltip title={formatDateTime(row.ts)}>
            <Typography variant="caption" color="text.secondary">
              {formatTime(row.ts)}
            </Typography>
          </Tooltip>
        </Box>

        <Menu anchorEl={anchor} open={anchor !== null} onClose={() => setAnchor(null)}>
          <MenuItem onClick={() => copy(row.body, t('messages.copiedBody'))}>{t('messages.copyBody')}</MenuItem>
          <MenuItem onClick={() => copy(row.raw, t('messages.copiedJson'))}>{t('messages.copyJson')}</MenuItem>
        </Menu>
      </Box>
    </Box>
  );
}

export default function MessageView() {
  const { t } = useTranslation();
  const selectedTopic = useAppStore((state) => state.selectedTopic);
  const messages = useAppStore((state) =>
    selectedTopic ? (state.messages.get(selectedTopic) ?? NO_MESSAGES) : NO_MESSAGES
  );
  const hasOlder = useAppStore((state) => (selectedTopic ? (state.hasOlder.get(selectedTopic) ?? false) : false));
  const loadOlderMessages = useAppStore((state) => state.loadOlderMessages);

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const stickRef = useRef(true);

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 88,
    overscan: 8,
  });

  const onScroll = useCallback(() => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    const distanceToBottom = element.scrollHeight - element.scrollTop - element.clientHeight;
    stickRef.current = distanceToBottom <= STICK_THRESHOLD_PX;
    if (element.scrollTop <= LOAD_OLDER_THRESHOLD_PX && hasOlder && selectedTopic) {
      loadOlderMessages(selectedTopic);
    }
  }, [hasOlder, loadOlderMessages, selectedTopic]);

  // New messages scroll into view only while the user is at the bottom; someone
  // reading older history is not dragged away from it.
  useEffect(() => {
    if (messages.length === 0 || !stickRef.current) {
      return;
    }
    virtualizer.scrollToIndex(messages.length - 1, { align: 'end' });
  }, [messages.length, virtualizer]);

  useEffect(() => {
    stickRef.current = true;
  }, [selectedTopic]);

  if (!selectedTopic) {
    return (
      <Box sx={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', minHeight: 0 }}>
        <Typography variant="body2" color="text.secondary">
          {t('messages.selectTopic')}
        </Typography>
      </Box>
    );
  }

  const items = virtualizer.getVirtualItems();

  return (
    <Box sx={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
      <Box sx={{ px: 1, py: 0.5, borderBottom: 1, borderColor: 'divider' }}>
        <Typography variant="subtitle2" sx={{ wordBreak: 'break-all' }}>
          {selectedTopic}
        </Typography>
      </Box>
      <Box ref={scrollRef} onScroll={onScroll} sx={{ flex: 1, minHeight: 0, overflowY: 'auto' }}>
        {messages.length === 0 ? (
          <Typography variant="body2" color="text.secondary" sx={{ p: 2, textAlign: 'center' }}>
            {t('messages.empty')}
          </Typography>
        ) : (
          <Box sx={{ height: virtualizer.getTotalSize(), width: '100%', position: 'relative' }}>
            {items.map((item) => {
              const row = messages[item.index];
              const previous = item.index > 0 ? messages[item.index - 1] : null;
              const showDay = !previous || formatDay(previous.ts) !== formatDay(row.ts);
              return (
                <Box
                  key={row.rowId}
                  data-index={item.index}
                  ref={virtualizer.measureElement}
                  sx={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${item.start}px)` }}
                >
                  {showDay && (
                    <Typography variant="caption" color="text.secondary" sx={{ display: 'block', textAlign: 'center' }}>
                      {formatDay(row.ts)}
                    </Typography>
                  )}
                  <Bubble row={row} />
                </Box>
              );
            })}
          </Box>
        )}
      </Box>
    </Box>
  );
}
