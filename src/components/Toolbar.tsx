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

import { useCallback, useEffect } from 'react';
import { Box, ButtonGroup, IconButton, Tooltip } from '@mui/material';
import CloudDoneIcon from '@mui/icons-material/CloudDone';
import CloudOffIcon from '@mui/icons-material/CloudOff';
import DeleteSweepIcon from '@mui/icons-material/DeleteSweep';
import InfoIcon from '@mui/icons-material/Info';
import NotificationsActiveIcon from '@mui/icons-material/NotificationsActive';
import NotificationsPausedIcon from '@mui/icons-material/NotificationsPaused';
import SettingsIcon from '@mui/icons-material/Settings';
import { useTranslation } from 'react-i18next';
import * as Protocol from '../lib/protocol';
import { useAppStore } from '../lib/store';

export default function Toolbar() {
  const { t } = useTranslation();
  const status = useAppStore((state) => state.status);
  const selectedTopic = useAppStore((state) => state.selectedTopic);
  const tabAboutStatus = useAppStore((state) => state.tabAboutStatus);
  const tabSettingsStatus = useAppStore((state) => state.tabSettingsStatus);
  const connect = useAppStore((state) => state.connect);
  const disconnect = useAppStore((state) => state.disconnect);
  const clearSelectedTopic = useAppStore((state) => state.clearSelectedTopic);
  const togglePaused = useAppStore((state) => state.toggleNotificationsPaused);
  const setTabAboutStatus = useAppStore((state) => state.setTabAboutStatus);
  const setTabSettingsStatus = useAppStore((state) => state.setTabSettingsStatus);

  // Reconnecting counts as connected for this button: the client is still trying, and
  // what the user would want is a way to stop it.
  const live =
    status.state === Protocol.ConnectionState.Connected ||
    status.state === Protocol.ConnectionState.Connecting ||
    status.state === Protocol.ConnectionState.Reconnecting;

  const handleSelectTabSettings = useCallback(() => {
    setTabSettingsStatus(Protocol.ControlStatus.Selected);
  }, [setTabSettingsStatus]);

  const handleSelectTabAbout = useCallback(() => {
    setTabAboutStatus(Protocol.ControlStatus.Selected);
  }, [setTabAboutStatus]);

  useEffect(() => {
    const handleKeyUp = (event: KeyboardEvent) => {
      if (!event.altKey && !event.ctrlKey && !event.shiftKey && event.key === 'F10') {
        event.stopPropagation();
        handleSelectTabSettings();
      }
    };
    document.addEventListener('keyup', handleKeyUp);
    return () => document.removeEventListener('keyup', handleKeyUp);
  }, [handleSelectTabSettings]);

  const buttonSx = { width: 28, height: 28, margin: '2px', borderRadius: 1 };
  const activeButtonSx = { ...buttonSx, color: 'primary.main' };

  return (
    <Box sx={{ mx: 1, my: 0, display: 'flex', gap: 1 }}>
      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={live ? t('toolbar.disconnect') : t('toolbar.connect')}>
          <IconButton
            aria-label={live ? t('toolbar.disconnect') : t('toolbar.connect')}
            sx={live ? activeButtonSx : buttonSx}
            onClick={() => (live ? disconnect() : connect())}
          >
            {live ? <CloudDoneIcon fontSize="small" /> : <CloudOffIcon fontSize="small" />}
          </IconButton>
        </Tooltip>
      </ButtonGroup>

      <ButtonGroup variant="outlined" size="small">
        <Tooltip
          title={status.notificationsPaused ? t('toolbar.resumeNotifications') : t('toolbar.pauseNotifications')}
        >
          <IconButton
            aria-label={status.notificationsPaused ? t('toolbar.resumeNotifications') : t('toolbar.pauseNotifications')}
            sx={status.notificationsPaused ? activeButtonSx : buttonSx}
            onClick={() => togglePaused()}
          >
            {status.notificationsPaused ? (
              <NotificationsPausedIcon fontSize="small" />
            ) : (
              <NotificationsActiveIcon fontSize="small" />
            )}
          </IconButton>
        </Tooltip>
      </ButtonGroup>

      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t('toolbar.clearTopic')}>
          <span>
            <IconButton
              aria-label={t('toolbar.clearTopic')}
              sx={buttonSx}
              disabled={selectedTopic === null}
              onClick={() => clearSelectedTopic()}
            >
              <DeleteSweepIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
      </ButtonGroup>

      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t('toolbar.settings')}>
          <IconButton
            aria-label={t('toolbar.settings')}
            sx={tabSettingsStatus !== Protocol.ControlStatus.Hidden ? activeButtonSx : buttonSx}
            onClick={handleSelectTabSettings}
          >
            <SettingsIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <Tooltip title={t('toolbar.about')}>
          <IconButton
            aria-label={t('toolbar.about')}
            sx={tabAboutStatus !== Protocol.ControlStatus.Hidden ? activeButtonSx : buttonSx}
            onClick={handleSelectTabAbout}
          >
            <InfoIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </ButtonGroup>
    </Box>
  );
}
