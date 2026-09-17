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

import { useState, type ReactNode } from 'react';
import { Box } from '@mui/material';

/** A lazy, window-lifetime panel. Its parent must establish a positioned viewport. */
export default function TabPanel({
  active,
  label,
  padded = false,
  children,
}: {
  active: boolean;
  label: string;
  padded?: boolean;
  children: ReactNode;
}) {
  const [visited, setVisited] = useState(active);
  if (active && !visited) setVisited(true);

  return (
    <Box
      role="tabpanel"
      aria-label={label}
      aria-hidden={!active}
      inert={!active}
      sx={{
        position: 'absolute',
        inset: 0,
        display: 'flex',
        flexDirection: 'column',
        minWidth: 0,
        minHeight: 0,
        overflow: 'auto',
        p: padded ? 1 : 0,
        // display:none makes ResizeObserver report zero-sized virtual rows and
        // viewports, destroying both their measurements and the reading position.
        // Hidden panels retain their geometry and DOM state, but cannot take input.
        visibility: active ? 'inherit' : 'hidden',
        pointerEvents: active ? 'auto' : 'none',
      }}
    >
      {(active || visited) && children}
    </Box>
  );
}
