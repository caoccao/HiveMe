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

import { useCallback, useEffect, useRef, useState } from 'react';
import { Box } from '@mui/material';
import { SPLIT_DEFAULT_PERCENT, SPLIT_MAX_PERCENT, SPLIT_MIN_PERCENT, SPLIT_STORAGE_KEY } from '../lib/constants';
import Composer from './Composer';
import MessageView from './MessageView';
import TopicTree from './TopicTree';

/** The remembered divider position, clamped to what the layout allows. */
function readSplit(): number {
  if (typeof window === 'undefined') {
    return SPLIT_DEFAULT_PERCENT;
  }
  const stored = Number.parseFloat(window.localStorage.getItem(SPLIT_STORAGE_KEY) ?? '');
  if (!Number.isFinite(stored)) {
    return SPLIT_DEFAULT_PERCENT;
  }
  return Math.min(SPLIT_MAX_PERCENT, Math.max(SPLIT_MIN_PERCENT, stored));
}

export default function Messages() {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [split, setSplit] = useState(readSplit);
  const [dragging, setDragging] = useState(false);

  const onPointerMove = useCallback((event: PointerEvent) => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const bounds = container.getBoundingClientRect();
    if (bounds.width === 0) {
      return;
    }
    const percent = ((event.clientX - bounds.left) / bounds.width) * 100;
    setSplit(Math.min(SPLIT_MAX_PERCENT, Math.max(SPLIT_MIN_PERCENT, percent)));
  }, []);

  useEffect(() => {
    if (!dragging) {
      return;
    }
    const stop = () => setDragging(false);
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', stop);
    return () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', stop);
    };
  }, [dragging, onPointerMove]);

  useEffect(() => {
    if (dragging || typeof window === 'undefined') {
      return;
    }
    window.localStorage.setItem(SPLIT_STORAGE_KEY, String(split));
  }, [dragging, split]);

  return (
    <Box
      ref={containerRef}
      sx={{ display: 'flex', flex: 1, minHeight: 0, width: '100%', userSelect: dragging ? 'none' : 'auto' }}
    >
      <Box sx={{ width: `${split}%`, minWidth: 0, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <TopicTree />
      </Box>

      <Box
        role="separator"
        aria-orientation="vertical"
        onPointerDown={(event) => {
          event.preventDefault();
          setDragging(true);
        }}
        sx={{
          width: 6,
          flexShrink: 0,
          cursor: 'col-resize',
          borderLeft: 1,
          borderRight: 1,
          borderColor: 'divider',
          bgcolor: dragging ? 'primary.main' : 'transparent',
          '&:hover': { bgcolor: 'action.hover' },
        }}
      />

      <Box sx={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <MessageView />
        <Composer />
      </Box>
    </Box>
  );
}
