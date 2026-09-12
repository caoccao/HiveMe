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

import { useEffect } from 'react';
import {
  Avatar,
  Box,
  Card,
  CardActionArea,
  CardContent,
  Chip,
  Link,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableRow,
  Typography,
} from '@mui/material';
import GitHubIcon from '@mui/icons-material/GitHub';
import PersonIcon from '@mui/icons-material/Person';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useTranslation } from 'react-i18next';

import appIconUrl from '../../src-tauri/icons/icon.png';
import { APP_NAME, AUTHOR_NAME, AUTHOR_URL, GITHUB_URL } from '../lib/constants';
import { useAppStore } from '../lib/store';

const GRADIENT = 'linear-gradient(135deg, #ffb300 0%, #e65100 100%)';

export default function About() {
  const { t } = useTranslation();
  const about = useAppStore((state) => state.about);
  const initAbout = useAppStore((state) => state.initAbout);

  useEffect(() => {
    initAbout();
  }, [initAbout]);

  const cardSx = {
    border: 1,
    borderColor: 'divider',
    borderRadius: 3,
    transition: 'transform 0.2s, box-shadow 0.2s, border-color 0.2s',
    '&:hover': { transform: 'translateY(-2px)', borderColor: 'primary.main' },
  };
  const labelSx = { letterSpacing: 1, fontWeight: 600 };

  return (
    <Box sx={{ display: 'grid', gap: 2, p: 1 }}>
      <Box sx={{ maxWidth: 640, mx: 'auto', px: 2, py: 3, width: '100%' }}>
        <Stack spacing={4} sx={{ alignItems: 'center' }}>
          <Stack spacing={1.5} sx={{ alignItems: 'center' }}>
            <Box component="img" src={appIconUrl} alt={APP_NAME} sx={{ width: 96, height: 96 }} />
            <Typography
              variant="h3"
              sx={{
                fontWeight: 800,
                letterSpacing: '-0.02em',
                backgroundImage: GRADIENT,
                backgroundClip: 'text',
                WebkitBackgroundClip: 'text',
                color: 'transparent',
              }}
            >
              {APP_NAME}
            </Typography>
            {about?.appVersion && (
              <Chip label={`v${about.appVersion}`} size="small" variant="outlined" sx={{ fontWeight: 600 }} />
            )}
            <Typography variant="body2" color="text.secondary" sx={{ textAlign: 'center' }}>
              {t('about.tagline')}
            </Typography>
          </Stack>

          <Stack direction="row" spacing={2} sx={{ width: '100%', alignItems: 'stretch' }}>
            <Card elevation={0} sx={{ ...cardSx, flexShrink: 0 }}>
              <CardActionArea onClick={() => openUrl(AUTHOR_URL)} sx={{ height: '100%' }}>
                <CardContent sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
                  <Avatar sx={{ background: GRADIENT, width: 48, height: 48 }}>
                    <PersonIcon />
                  </Avatar>
                  <Box sx={{ minWidth: 0 }}>
                    <Typography variant="caption" color="text.secondary" sx={labelSx}>
                      {t('about.author')}
                    </Typography>
                    <Typography variant="h6" sx={{ lineHeight: 1.2, whiteSpace: 'nowrap' }}>
                      {AUTHOR_NAME}
                    </Typography>
                  </Box>
                </CardContent>
              </CardActionArea>
            </Card>

            <Card elevation={0} sx={{ ...cardSx, flex: 1, minWidth: 0 }}>
              <CardActionArea onClick={() => openUrl(GITHUB_URL)} sx={{ height: '100%' }}>
                <CardContent sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
                  <Avatar sx={{ bgcolor: '#24292f', width: 48, height: 48 }}>
                    <GitHubIcon sx={{ color: '#fff' }} />
                  </Avatar>
                  <Box sx={{ minWidth: 0, flex: 1 }}>
                    <Typography variant="caption" color="text.secondary" sx={labelSx}>
                      {t('about.github')}
                    </Typography>
                    <Typography
                      variant="body2"
                      sx={{
                        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
                        whiteSpace: 'nowrap',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                      }}
                    >
                      {GITHUB_URL}
                    </Typography>
                  </Box>
                </CardContent>
              </CardActionArea>
            </Card>
          </Stack>
        </Stack>
      </Box>

      <Table size="small">
        <TableBody>
          <TableRow>
            <TableCell sx={{ fontWeight: 600 }}>{t('about.device')}</TableCell>
            <TableCell sx={{ wordBreak: 'break-all' }}>
              {about?.deviceName} ({about?.deviceId})
            </TableCell>
          </TableRow>
          <TableRow>
            <TableCell sx={{ fontWeight: 600 }}>{t('about.configPath')}</TableCell>
            <TableCell sx={{ wordBreak: 'break-all' }}>{about?.configPath}</TableCell>
          </TableRow>
          <TableRow>
            <TableCell sx={{ fontWeight: 600 }}>{t('about.databasePath')}</TableCell>
            <TableCell sx={{ wordBreak: 'break-all' }}>{about?.databasePath}</TableCell>
          </TableRow>
          <TableRow>
            <TableCell sx={{ fontWeight: 600 }}>{t('about.licence')}</TableCell>
            <TableCell>Apache-2.0</TableCell>
          </TableRow>
        </TableBody>
      </Table>

      <Box sx={{ textAlign: 'center', color: 'text.secondary', pb: 2 }}>
        <Typography variant="caption" component="div">
          {t('about.copyright')}{' '}
          <Link component="button" onClick={() => openUrl(AUTHOR_URL)}>
            {AUTHOR_NAME}
          </Link>{' '}
          <Link component="button" onClick={() => openUrl('https://www.caoccao.com/')}>
            caoccao.com
          </Link>
        </Typography>
      </Box>
    </Box>
  );
}
