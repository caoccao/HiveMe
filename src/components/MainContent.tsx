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
import { Alert, Box, Checkbox, FormControlLabel, IconButton, Link, Tab, Tabs, Tooltip } from '@mui/material';
import CloseIcon from '@mui/icons-material/Close';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useTranslation } from 'react-i18next';
import * as Protocol from '../lib/protocol';
import { RELEASES_URL } from '../lib/constants';
import { getUpdateResult, skipVersion } from '../lib/service';
import { useAppStore } from '../lib/store';
import type { TabControl } from '../lib/types';
import About from './About';
import Config from './Config';
import Messages from './Messages';

export default function MainContent() {
  const { t } = useTranslation();
  const [tabIndex, setTabIndex] = useState(0);
  const [tabControls, setTabControls] = useState<TabControl[]>([
    { type: Protocol.TabType.Messages, index: 0, value: null },
  ]);
  const [newVersion, setNewVersion] = useState<string | null>(null);
  const [skipChecked, setSkipChecked] = useState(false);
  const updatePollRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined);

  const tabAboutStatus = useAppStore((state) => state.tabAboutStatus);
  const tabSettingsStatus = useAppStore((state) => state.tabSettingsStatus);
  const setTabAboutStatus = useAppStore((state) => state.setTabAboutStatus);
  const setTabSettingsStatus = useAppStore((state) => state.setTabSettingsStatus);

  useEffect(() => {
    setTabControls((previous) => {
      let controls = [...previous];

      const hasSettings = controls.some((control) => control.type === Protocol.TabType.Config);
      if (tabSettingsStatus !== Protocol.ControlStatus.Hidden && !hasSettings) {
        controls.push({ type: Protocol.TabType.Config, index: 0, value: null });
      } else if (tabSettingsStatus === Protocol.ControlStatus.Hidden && hasSettings) {
        controls = controls.filter((control) => control.type !== Protocol.TabType.Config);
      }

      const hasAbout = controls.some((control) => control.type === Protocol.TabType.About);
      if (tabAboutStatus !== Protocol.ControlStatus.Hidden && !hasAbout) {
        controls.push({ type: Protocol.TabType.About, index: 0, value: null });
      } else if (tabAboutStatus === Protocol.ControlStatus.Hidden && hasAbout) {
        controls = controls.filter((control) => control.type !== Protocol.TabType.About);
      }

      controls.forEach((control, index) => {
        control.index = index;
      });
      return controls;
    });
  }, [tabAboutStatus, tabSettingsStatus]);

  useEffect(() => {
    if (tabSettingsStatus === Protocol.ControlStatus.Selected) {
      const tab = tabControls.find((control) => control.type === Protocol.TabType.Config);
      if (tab) {
        setTabIndex(tab.index);
        setTabSettingsStatus(Protocol.ControlStatus.Visible);
      }
    }
  }, [tabSettingsStatus, tabControls, setTabSettingsStatus]);

  useEffect(() => {
    if (tabAboutStatus === Protocol.ControlStatus.Selected) {
      const tab = tabControls.find((control) => control.type === Protocol.TabType.About);
      if (tab) {
        setTabIndex(tab.index);
        setTabAboutStatus(Protocol.ControlStatus.Visible);
      }
    }
  }, [tabAboutStatus, tabControls, setTabAboutStatus]);

  const closeTab = useCallback(
    (index: number) => {
      const control = tabControls[index];
      if (!control) {
        return;
      }
      switch (control.type) {
        case Protocol.TabType.Config:
          setTabSettingsStatus(Protocol.ControlStatus.Hidden);
          break;
        case Protocol.TabType.About:
          setTabAboutStatus(Protocol.ControlStatus.Hidden);
          break;
        default:
          break;
      }
    },
    [tabControls, setTabAboutStatus, setTabSettingsStatus]
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && !event.altKey && !event.shiftKey) {
        if (event.key >= '1' && event.key <= '9') {
          const index = Number.parseInt(event.key, 10) - 1;
          if (index >= 0 && index < tabControls.length) {
            event.preventDefault();
            event.stopPropagation();
            setTabIndex(index);
          }
        } else if (event.key.toLowerCase() === 'w') {
          event.preventDefault();
          event.stopPropagation();
          closeTab(tabIndex);
        } else if (event.key === 'Tab') {
          event.preventDefault();
          event.stopPropagation();
          setTabIndex((previous) => (previous >= tabControls.length - 1 ? 0 : previous + 1));
        }
      } else if (event.ctrlKey && !event.altKey && event.shiftKey && event.key === 'Tab') {
        event.preventDefault();
        event.stopPropagation();
        setTabIndex((previous) => (previous > 0 ? previous - 1 : tabControls.length - 1));
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [closeTab, tabIndex, tabControls.length]);

  useEffect(() => {
    if (tabIndex >= tabControls.length && tabControls.length > 0) {
      setTabIndex(tabControls.length - 1);
    }
  }, [tabIndex, tabControls.length]);

  // The release check runs on its own thread in the backend, so the answer is polled
  // until it is there and then the poll stops.
  useEffect(() => {
    updatePollRef.current = setInterval(async () => {
      try {
        const result = await getUpdateResult();
        if (result) {
          if (updatePollRef.current) {
            clearInterval(updatePollRef.current);
            updatePollRef.current = undefined;
          }
          if (result.hasUpdate && result.latestVersion) {
            setNewVersion(result.latestVersion);
          }
        }
      } catch {
        // A failed poll is not worth telling the user about; the next one may work.
      }
    }, 1000);
    return () => {
      if (updatePollRef.current) {
        clearInterval(updatePollRef.current);
        updatePollRef.current = undefined;
      }
    };
  }, []);

  const label = (control: TabControl) => {
    switch (control.type) {
      case Protocol.TabType.Config:
        return t('tabs.settings');
      case Protocol.TabType.About:
        return t('tabs.about');
      default:
        return t('tabs.messages');
    }
  };

  return (
    <Box sx={{ width: '100%', height: '100%', overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
      {newVersion && (
        <Alert
          severity="info"
          closeText={t('tabs.close')}
          onClose={async () => {
            if (skipChecked) {
              await skipVersion(newVersion);
            }
            setNewVersion(null);
            setSkipChecked(false);
          }}
          sx={{ flexShrink: 0, '& .MuiAlert-message': { flex: 1 } }}
        >
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
            <Link component="button" variant="body2" onClick={() => openUrl(RELEASES_URL)} sx={{ cursor: 'pointer' }}>
              {t('update.newVersionAvailable', { version: newVersion })}
            </Link>
            <Box sx={{ flex: 1 }} />
            <FormControlLabel
              control={
                <Checkbox
                  size="small"
                  sx={{ p: 0.5 }}
                  checked={skipChecked}
                  onChange={(event) => setSkipChecked(event.target.checked)}
                />
              }
              label={t('update.skipThisVersion')}
              slotProps={{ typography: { variant: 'body2' } }}
              sx={{ mr: 0 }}
            />
          </Box>
        </Alert>
      )}

      <Box sx={{ borderBottom: 1, borderColor: 'divider', flexShrink: 0 }}>
        <Tabs
          value={tabIndex}
          onChange={(_, value) => setTabIndex(value)}
          variant="scrollable"
          scrollButtons="auto"
          sx={{ mt: 0, minHeight: '24px' }}
        >
          {tabControls.map((control) => (
            <Tab
              key={control.type}
              style={{ minHeight: '24px' }}
              label={
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
                  <span>{label(control)}</span>
                  {control.type !== Protocol.TabType.Messages && (
                    <Tooltip title={t('tabs.close')}>
                      <IconButton
                        size="small"
                        aria-label={t('tabs.close')}
                        sx={{ ml: 0.5, p: 0.25 }}
                        onClick={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          closeTab(control.index);
                        }}
                      >
                        <CloseIcon sx={{ fontSize: 14 }} />
                      </IconButton>
                    </Tooltip>
                  )}
                </Box>
              }
              sx={{ py: 0, my: 0 }}
            />
          ))}
        </Tabs>
      </Box>

      <Box
        sx={{
          border: 1,
          borderColor: 'divider',
          borderTop: 0,
          borderRadius: '0 0 4px 4px',
          width: '100%',
          flex: 1,
          minHeight: 0,
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {tabControls.map((control) => {
          const visible = control.index === tabIndex;
          const ownsScroll = control.type === Protocol.TabType.Messages;
          return (
            <Box
              key={`content-${control.type}`}
              sx={{
                display: visible ? 'flex' : 'none',
                flexDirection: 'column',
                flex: 1,
                minHeight: 0,
                ...(!ownsScroll && { p: 1, overflow: 'auto' }),
              }}
            >
              {control.type === Protocol.TabType.Messages && <Messages />}
              {control.type === Protocol.TabType.Config && <Config />}
              {control.type === Protocol.TabType.About && <About />}
            </Box>
          );
        })}
      </Box>
    </Box>
  );
}
