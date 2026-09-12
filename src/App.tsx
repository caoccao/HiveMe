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
import { Box, CssBaseline, ThemeProvider, createTheme } from '@mui/material';
import { listen } from '@tauri-apps/api/event';
import * as Protocol from './lib/protocol';
import { useAppStore } from './lib/store';
import { changeLanguage } from './i18n';
import Layout from './components/Layout';
import NotificationSnackbar from './components/NotificationSnackbar';

/** The twenty palettes of the reference project, keyed by `gui.theme`. */
const PALETTES: Record<Protocol.Theme, { primary: string; secondary: string }> = {
  [Protocol.Theme.Ocean]: { primary: '#0288d1', secondary: '#26c6da' },
  [Protocol.Theme.Aqua]: { primary: '#00acc1', secondary: '#4dd0e1' },
  [Protocol.Theme.Sky]: { primary: '#42a5f5', secondary: '#90caf9' },
  [Protocol.Theme.Arctic]: { primary: '#4fc3f7', secondary: '#b3e5fc' },
  [Protocol.Theme.Glacier]: { primary: '#5c6bc0', secondary: '#9fa8da' },
  [Protocol.Theme.Mist]: { primary: '#90a4ae', secondary: '#cfd8dc' },
  [Protocol.Theme.Slate]: { primary: '#546e7a', secondary: '#78909c' },
  [Protocol.Theme.Charcoal]: { primary: '#37474f', secondary: '#607d8b' },
  [Protocol.Theme.Midnight]: { primary: '#1a237e', secondary: '#3949ab' },
  [Protocol.Theme.Indigo]: { primary: '#3f51b5', secondary: '#7986cb' },
  [Protocol.Theme.Violet]: { primary: '#7e57c2', secondary: '#b39ddb' },
  [Protocol.Theme.Lavender]: { primary: '#9575cd', secondary: '#d1c4e9' },
  [Protocol.Theme.Rose]: { primary: '#c2185b', secondary: '#f06292' },
  [Protocol.Theme.Blush]: { primary: '#ec407a', secondary: '#f48fb1' },
  [Protocol.Theme.Coral]: { primary: '#ff7043', secondary: '#ffab91' },
  [Protocol.Theme.Sunset]: { primary: '#ef6c00', secondary: '#ff8a65' },
  [Protocol.Theme.Amber]: { primary: '#ff8f00', secondary: '#ffca28' },
  [Protocol.Theme.Sand]: { primary: '#bcaaa4', secondary: '#d7ccc8' },
  [Protocol.Theme.Forest]: { primary: '#2e7d32', secondary: '#66bb6a' },
  [Protocol.Theme.Emerald]: { primary: '#00897b', secondary: '#4db6ac' },
};

export function getPaletteByTheme(theme: Protocol.Theme, mode: 'light' | 'dark') {
  const palette = PALETTES[theme] ?? PALETTES[Protocol.Theme.Ocean];
  return { mode, primary: { main: palette.primary }, secondary: { main: palette.secondary } };
}

function App() {
  const displayMode = useAppStore((state) => state.config?.gui?.displayMode ?? Protocol.DisplayMode.Auto);
  const selectedTheme = useAppStore((state) => state.config?.gui?.theme ?? Protocol.Theme.Ocean);
  const language = useAppStore((state) => state.config?.gui?.language);
  const initConfig = useAppStore((state) => state.initConfig);
  const initAbout = useAppStore((state) => state.initAbout);
  const initStatus = useAppStore((state) => state.initStatus);
  const refreshTopics = useAppStore((state) => state.refreshTopics);
  const receiveMessage = useAppStore((state) => state.receiveMessage);
  const setStatus = useAppStore((state) => state.setStatus);

  const [prefersDark, setPrefersDark] = useState(
    () => typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches
  );

  useEffect(() => {
    initConfig();
    initAbout();
    initStatus();
    refreshTopics();
  }, [initConfig, initAbout, initStatus, refreshTopics]);

  useEffect(() => {
    if (language) {
      changeLanguage(language);
    }
  }, [language]);

  // The backend is the only source of truth for the connection, the history, and the
  // tree, so every change arrives as an event rather than being polled.
  useEffect(() => {
    const unlisteners = [
      listen<Protocol.Status>(Protocol.EVENT_STATUS, (event) => setStatus(event.payload)),
      listen<Protocol.MessageRow>(Protocol.EVENT_MESSAGE, (event) => {
        receiveMessage(event.payload);
        refreshTopics();
      }),
      listen<Protocol.TopicAddedEvent>(Protocol.EVENT_TOPIC_ADDED, () => refreshTopics()),
    ];
    return () => {
      unlisteners.forEach((unlisten) => {
        unlisten.then((stop) => stop()).catch(() => undefined);
      });
    };
  }, [receiveMessage, refreshTopics, setStatus]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    const query = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (event: MediaQueryListEvent) => setPrefersDark(event.matches);
    query.addEventListener('change', onChange);
    return () => query.removeEventListener('change', onChange);
  }, []);

  const mode = useMemo<'light' | 'dark'>(() => {
    if (displayMode === Protocol.DisplayMode.Auto) {
      return prefersDark ? 'dark' : 'light';
    }
    return displayMode === Protocol.DisplayMode.Dark ? 'dark' : 'light';
  }, [displayMode, prefersDark]);

  const theme = useMemo(
    () =>
      createTheme({
        palette: { ...getPaletteByTheme(selectedTheme as Protocol.Theme, mode) },
        typography: { fontSize: 12, button: { textTransform: 'none' } },
        components: {
          MuiButton: { defaultProps: { size: 'small' } },
          MuiButtonGroup: { defaultProps: { size: 'small' } },
          MuiTextField: { defaultProps: { size: 'small' } },
          MuiSelect: { defaultProps: { size: 'small' } },
          MuiFormControl: { defaultProps: { size: 'small' } },
          MuiCheckbox: { defaultProps: { size: 'small' } },
          MuiRadio: { defaultProps: { size: 'small' } },
          MuiIconButton: { defaultProps: { size: 'small' } },
          MuiTab: { defaultProps: { sx: { minHeight: 36, py: 0.5 } } },
          MuiTabs: { defaultProps: { sx: { minHeight: 36 } } },
          MuiTableCell: { styleOverrides: { root: { padding: '4px 8px', fontSize: '0.75rem' } } },
        },
      }),
    [mode, selectedTheme]
  );

  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <Box sx={{ height: '100vh', bgcolor: 'background.default', color: 'text.primary' }}>
        <Layout />
        <NotificationSnackbar />
      </Box>
    </ThemeProvider>
  );
}

export default App;
