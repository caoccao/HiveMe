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

import { useEffect, useState } from 'react';
import { Box, Chip, Link, Tooltip, Typography } from '@mui/material';
import { useTranslation } from 'react-i18next';
import * as Protocol from '../lib/protocol';
import { formatBytes, formatDuration } from '../lib/format';
import { useAppStore } from '../lib/store';

/** The status colour of each connection state. */
function stateColour(state: string): 'success' | 'warning' | 'default' | 'error' {
  switch (state) {
    case Protocol.ConnectionState.Connected:
      return 'success';
    case Protocol.ConnectionState.Connecting:
    case Protocol.ConnectionState.Reconnecting:
      return 'warning';
    default:
      return 'default';
  }
}

export default function Footer() {
  const { t } = useTranslation();
  const status = useAppStore((state) => state.status);
  const setDialogNotification = useAppStore((state) => state.setDialogNotification);
  const [countdown, setCountdown] = useState<number | null>(null);

  // The backend reports how long the pending reconnect waits at the moment the status
  // changed, so the countdown itself is run here rather than by a stream of events.
  useEffect(() => {
    if (status.retryInMs === null || status.retryInMs === undefined) {
      setCountdown(null);
      return;
    }
    const deadline = Date.now() + status.retryInMs;
    setCountdown(status.retryInMs);
    const timer = setInterval(() => {
      const remaining = deadline - Date.now();
      setCountdown(remaining > 0 ? remaining : 0);
    }, 500);
    return () => clearInterval(timer);
  }, [status.retryInMs, status.state]);

  const broker = status.host ? `${status.host}:${status.port}` : t('footer.noBroker');

  return (
    <Box
      sx={{
        display: 'flex',
        alignItems: 'center',
        gap: 1.5,
        px: 1,
        py: 0.5,
        flexWrap: 'wrap',
        color: 'text.secondary',
      }}
    >
      <Chip
        size="small"
        color={stateColour(status.state)}
        variant="outlined"
        label={t(`footer.state.${status.state}`, { defaultValue: status.state })}
      />
      <Tooltip title={status.clientId || ''}>
        <Typography variant="caption">{broker}</Typography>
      </Tooltip>
      {countdown !== null && (
        <Typography variant="caption">{t('footer.retryIn', { duration: formatDuration(countdown) })}</Typography>
      )}
      <Typography variant="caption">{t('footer.subscriptions', { count: status.subscriptions })}</Typography>
      <Typography variant="caption">{t('footer.received', { count: status.messagesReceived })}</Typography>
      <Typography variant="caption">{t('footer.database', { size: formatBytes(status.databaseBytes) })}</Typography>
      {status.notificationsPaused && (
        <Typography variant="caption" sx={{ color: 'warning.main' }}>
          {t('footer.notificationsPaused')}
        </Typography>
      )}
      <Box sx={{ flex: 1 }} />
      {status.configError && (
        <Link
          component="button"
          variant="caption"
          sx={{ color: 'error.main' }}
          onClick={() =>
            setDialogNotification({
              title: status.configError ?? '',
              type: Protocol.DialogNotificationType.Error,
            })
          }
        >
          {t('footer.configError')}
        </Link>
      )}
      {status.lastError && (
        <Link
          component="button"
          variant="caption"
          sx={{ color: 'error.main' }}
          onClick={() =>
            setDialogNotification({
              title: status.lastError ?? '',
              type: Protocol.DialogNotificationType.Error,
            })
          }
        >
          {t('footer.lastError')}
        </Link>
      )}
    </Box>
  );
}
