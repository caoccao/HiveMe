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

import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Box,
  Button,
  CssBaseline,
  Stack,
  ThemeProvider,
  Typography,
  createTheme,
  useMediaQuery,
} from '@mui/material';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import CheckCircleOutlineIcon from '@mui/icons-material/CheckCircleOutlined';
import ErrorOutlineIcon from '@mui/icons-material/ErrorOutlined';
import WarningAmberIcon from '@mui/icons-material/WarningAmber';
import { listen } from '@tauri-apps/api/event';
import type { TopmostSnapshot } from '../lib/protocol';
import * as Service from '../lib/service';

export default function TopmostNotification() {
  const [notification, setNotification] = useState<TopmostSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const dark = useMediaQuery('(prefers-color-scheme: dark)');
  const theme = useMemo(
    () =>
      createTheme({
        palette: { mode: dark ? 'dark' : 'light' },
        typography: { fontSize: 12, button: { textTransform: 'none' } },
      }),
    [dark]
  );
  const close = useCallback(() => {
    if (notification)
      Service.closeTopmostNotification(notification.revision).catch((reason) => setError(String(reason)));
  }, [notification]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const accept = (incoming: TopmostSnapshot | null) => {
      if (!disposed && incoming)
        setNotification((current) => (!current || incoming.revision > current.revision ? incoming : current));
    };
    (async () => {
      const stop = await listen<TopmostSnapshot>('topmost-notification', ({ payload }) => accept(payload));
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
      accept(await Service.getTopmostNotification());
    })().catch((reason) => {
      if (!disposed) setError(String(reason));
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (notification)
      Service.readyTopmostNotification(notification.revision).catch((reason) => setError(String(reason)));
  }, [notification]);

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close();
    };
    document.addEventListener('keydown', handleKey);
    return () => document.removeEventListener('keydown', handleKey);
  }, [close]);

  const icon =
    notification?.level === 'error' ? (
      <ErrorOutlineIcon color="error" />
    ) : notification?.level === 'warn' ? (
      <WarningAmberIcon color="warning" />
    ) : notification?.level === 'success' ? (
      <CheckCircleOutlineIcon color="success" />
    ) : (
      <InfoOutlinedIcon color="info" />
    );
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <Box
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="notification-title"
        sx={{
          height: '100vh',
          border: 1,
          borderColor: 'divider',
          bgcolor: 'background.paper',
          color: 'text.primary',
          p: 2,
        }}
      >
        <Stack spacing={1.25} sx={{ height: '100%' }}>
          <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
            {icon}
            <Typography id="notification-title" variant="h6" component="h1" noWrap title={notification?.title}>
              {notification?.title}
            </Typography>
          </Stack>
          <Typography
            variant="body2"
            sx={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', overflow: 'auto', flex: 1, minHeight: 0 }}
          >
            {notification?.body}
          </Typography>
          {error && <Alert severity="error">{error}</Alert>}
          <Button
            variant="contained"
            size="small"
            autoFocus
            onClick={close}
            sx={{ alignSelf: 'flex-end', minWidth: 88 }}
          >
            {notification?.closeLabel ?? 'Close'}
          </Button>
        </Stack>
      </Box>
    </ThemeProvider>
  );
}
