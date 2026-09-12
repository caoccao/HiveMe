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
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Checkbox,
  FormControlLabel,
  IconButton,
  InputAdornment,
  MenuItem,
  Select,
  Stack,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Tooltip,
  Typography,
} from '@mui/material';
import AddIcon from '@mui/icons-material/Add';
import BrightnessAutoIcon from '@mui/icons-material/BrightnessAuto';
import CloudIcon from '@mui/icons-material/Cloud';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import DarkModeIcon from '@mui/icons-material/DarkMode';
import DeleteIcon from '@mui/icons-material/Delete';
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import LightModeIcon from '@mui/icons-material/LightMode';
import LockIcon from '@mui/icons-material/Lock';
import NotificationsIcon from '@mui/icons-material/Notifications';
import PaletteIcon from '@mui/icons-material/Palette';
import TopicIcon from '@mui/icons-material/Topic';
import UpdateIcon from '@mui/icons-material/Update';
import VisibilityIcon from '@mui/icons-material/Visibility';
import VisibilityOffIcon from '@mui/icons-material/VisibilityOff';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useTranslation } from 'react-i18next';
import * as Protocol from '../lib/protocol';
import { getBrokerInit, openConfigFile } from '../lib/service';
import { useAppStore } from '../lib/store';

/** The broker fields `hmc --init` needs, which is also what makes a connection possible. */
export function isBrokerUsable(broker: Protocol.Broker | undefined): boolean {
  return Boolean(broker?.url?.trim() && broker?.username?.trim() && (broker?.password?.trim() || broker?.passwordRef));
}

function SectionHeader({ icon, title, action }: { icon: React.ReactNode; title: string; action?: React.ReactNode }) {
  return (
    <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 1, mb: 2 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, minWidth: 0 }}>
        <Box sx={{ color: 'primary.main', display: 'flex' }}>{icon}</Box>
        <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
          {title}
        </Typography>
      </Box>
      {action}
    </Box>
  );
}

function Section({ children }: { children: React.ReactNode }) {
  return (
    <Card variant="outlined" sx={{ mb: 2 }}>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

/** A number field that leaves the draft alone until the text is a number. */
function NumberField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number | undefined;
  onChange: (value: number) => void;
}) {
  return (
    <TextField
      label={label}
      type="number"
      value={value ?? 0}
      onChange={(event) => {
        const parsed = Number.parseInt(event.target.value, 10);
        if (Number.isFinite(parsed) && parsed >= 0) {
          onChange(parsed);
        }
      }}
      sx={{ minWidth: 140 }}
    />
  );
}

/** A subscription in the shape the editor works with. */
interface SubscriptionDraft {
  filter: string;
  absolute: boolean;
}

export function toDrafts(subscriptions: Protocol.Subscription[] | undefined): SubscriptionDraft[] {
  return (subscriptions ?? []).map((subscription) =>
    typeof subscription === 'string'
      ? { filter: subscription, absolute: false }
      : { filter: subscription.filter, absolute: subscription.absolute ?? false }
  );
}

export function fromDrafts(drafts: SubscriptionDraft[]): Protocol.Subscription[] {
  return drafts
    .filter((draft) => draft.filter.trim() !== '')
    .map((draft) => (draft.absolute ? { filter: draft.filter.trim(), absolute: true } : draft.filter.trim()));
}

export default function Config() {
  const { t } = useTranslation();
  const config = useAppStore((state) => state.config);
  const saveConfig = useAppStore((state) => state.saveConfig);
  const notifyInfo = useAppStore((state) => state.notifyInfo);
  const notifyError = useAppStore((state) => state.notifyError);
  const about = useAppStore((state) => state.about);

  const [draft, setDraft] = useState<Protocol.Config | null>(config);
  const [showPassword, setShowPassword] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(config);
  }, [config]);

  const subscriptions = useMemo(() => toDrafts(draft?.topics?.subscriptions), [draft]);

  if (!draft) {
    return (
      <Typography variant="body2" color="text.secondary" sx={{ p: 2 }}>
        {t('settings.loading')}
      </Typography>
    );
  }

  const update = (change: (next: Protocol.Config) => void) => {
    const next = structuredClone(draft) as Protocol.Config;
    change(next);
    setDraft(next);
  };

  const broker = draft.broker ?? {};
  const topics = draft.topics ?? {};
  const notifications = draft.notifications ?? {};
  const gui = draft.gui ?? {};
  const rules = notifications.rules ?? [];

  const save = async () => {
    setSaving(true);
    const saved = await saveConfig(draft);
    setSaving(false);
    if (saved) {
      notifyInfo(t('settings.saved'));
    }
  };

  const copyCliSetup = async () => {
    try {
      await writeText(await getBrokerInit());
      notifyInfo(t('settings.cliSetupCopied'));
    } catch (error) {
      notifyError(error);
    }
  };

  return (
    <Box sx={{ p: 1 }}>
      <Section>
        <SectionHeader
          icon={<CloudIcon fontSize="small" />}
          title={t('settings.broker')}
          action={
            <Tooltip title={t('settings.copyCliSetupHint')}>
              <span>
                <Button
                  variant="outlined"
                  startIcon={<ContentCopyIcon />}
                  disabled={!isBrokerUsable(broker)}
                  onClick={copyCliSetup}
                  sx={{ textTransform: 'none' }}
                >
                  {t('settings.copyCliSetup')}
                </Button>
              </span>
            </Tooltip>
          }
        />
        <Stack spacing={2}>
          <TextField
            label={t('settings.url')}
            placeholder="mqtts://abc123.s1.eu.hivemq.cloud:8883"
            value={broker.url ?? ''}
            onChange={(event) => update((next) => ((next.broker ??= {}).url = event.target.value))}
            fullWidth
          />
          <TextField
            label={t('settings.username')}
            value={broker.username ?? ''}
            onChange={(event) => update((next) => ((next.broker ??= {}).username = event.target.value))}
            fullWidth
          />
          <TextField
            label={t('settings.password')}
            type={showPassword ? 'text' : 'password'}
            value={broker.password ?? ''}
            onChange={(event) => update((next) => ((next.broker ??= {}).password = event.target.value))}
            fullWidth
            slotProps={{
              input: {
                endAdornment: (
                  <InputAdornment position="end">
                    <IconButton
                      aria-label={showPassword ? t('settings.hidePassword') : t('settings.showPassword')}
                      onClick={() => setShowPassword((previous) => !previous)}
                      edge="end"
                    >
                      {showPassword ? <VisibilityOffIcon fontSize="small" /> : <VisibilityIcon fontSize="small" />}
                    </IconButton>
                  </InputAdornment>
                ),
              },
            }}
          />
          <Alert severity="info" variant="outlined">
            {t('settings.tlsIsAutomatic')}
          </Alert>
          <Box sx={{ display: 'flex', gap: 2, flexWrap: 'wrap' }}>
            <TextField
              label={t('settings.clientIdPrefix')}
              value={broker.clientIdPrefix ?? ''}
              onChange={(event) => update((next) => ((next.broker ??= {}).clientIdPrefix = event.target.value))}
              sx={{ minWidth: 160 }}
            />
            <NumberField
              label={t('settings.keepAliveSecs')}
              value={broker.keepAliveSecs}
              onChange={(value) => update((next) => ((next.broker ??= {}).keepAliveSecs = value))}
            />
            <NumberField
              label={t('settings.sessionExpirySecs')}
              value={broker.sessionExpirySecs}
              onChange={(value) => update((next) => ((next.broker ??= {}).sessionExpirySecs = value))}
            />
            <NumberField
              label={t('settings.connectTimeoutSecs')}
              value={broker.connectTimeoutSecs}
              onChange={(value) => update((next) => ((next.broker ??= {}).connectTimeoutSecs = value))}
            />
            <NumberField
              label={t('settings.initialDelayMs')}
              value={broker.reconnect?.initialDelayMs}
              onChange={(value) => update((next) => (((next.broker ??= {}).reconnect ??= {}).initialDelayMs = value))}
            />
            <NumberField
              label={t('settings.maxDelayMs')}
              value={broker.reconnect?.maxDelayMs}
              onChange={(value) => update((next) => (((next.broker ??= {}).reconnect ??= {}).maxDelayMs = value))}
            />
          </Box>
        </Stack>
      </Section>

      <Section>
        <SectionHeader icon={<TopicIcon fontSize="small" />} title={t('settings.topics')} />
        <Stack spacing={2}>
          <Box sx={{ display: 'flex', gap: 2, flexWrap: 'wrap' }}>
            <TextField
              label={t('settings.prefix')}
              value={topics.prefix ?? ''}
              onChange={(event) => update((next) => ((next.topics ??= {}).prefix = event.target.value))}
              sx={{ minWidth: 200 }}
            />
            <TextField
              label={t('settings.defaultTopic')}
              value={topics.default ?? ''}
              onChange={(event) => update((next) => ((next.topics ??= {}).default = event.target.value))}
              sx={{ minWidth: 200 }}
            />
          </Box>
          <Box>
            <Typography variant="body2" sx={{ mb: 1 }}>
              {t('settings.subscriptions')}
            </Typography>
            <Stack spacing={1}>
              {subscriptions.map((subscription, index) => (
                <Box key={index} sx={{ display: 'flex', gap: 1, alignItems: 'center' }}>
                  <TextField
                    value={subscription.filter}
                    onChange={(event) => {
                      const next = [...subscriptions];
                      next[index] = { ...next[index], filter: event.target.value };
                      update((config) => ((config.topics ??= {}).subscriptions = fromDrafts(next)));
                    }}
                    sx={{ flex: 1 }}
                    slotProps={{ htmlInput: { 'aria-label': t('settings.subscriptionFilter', { index: index + 1 }) } }}
                  />
                  <FormControlLabel
                    control={
                      <Checkbox
                        checked={subscription.absolute}
                        onChange={(event) => {
                          const next = [...subscriptions];
                          next[index] = { ...next[index], absolute: event.target.checked };
                          update((config) => ((config.topics ??= {}).subscriptions = fromDrafts(next)));
                        }}
                      />
                    }
                    label={t('settings.absolute')}
                  />
                  <IconButton
                    aria-label={t('settings.removeSubscription')}
                    onClick={() => {
                      const next = subscriptions.filter((_, at) => at !== index);
                      update((config) => ((config.topics ??= {}).subscriptions = fromDrafts(next)));
                    }}
                  >
                    <DeleteIcon fontSize="small" />
                  </IconButton>
                </Box>
              ))}
              <Box>
                <Button
                  startIcon={<AddIcon />}
                  sx={{ textTransform: 'none' }}
                  onClick={() => {
                    const next = [...subscriptions, { filter: '#', absolute: false }];
                    update((config) => ((config.topics ??= {}).subscriptions = fromDrafts(next)));
                  }}
                >
                  {t('settings.addSubscription')}
                </Button>
              </Box>
            </Stack>
          </Box>
        </Stack>
      </Section>

      <Section>
        <SectionHeader icon={<NotificationsIcon fontSize="small" />} title={t('settings.notifications')} />
        <Stack spacing={1}>
          <FormControlLabel
            control={
              <Switch
                checked={notifications.enabled ?? true}
                onChange={(event) => update((next) => ((next.notifications ??= {}).enabled = event.target.checked))}
              />
            }
            label={t('settings.notificationsEnabled')}
          />
          <FormControlLabel
            control={
              <Switch
                checked={notifications.notifyOwnMessages ?? false}
                onChange={(event) =>
                  update((next) => ((next.notifications ??= {}).notifyOwnMessages = event.target.checked))
                }
              />
            }
            label={t('settings.notifyOwnMessages')}
          />
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>{t('settings.ruleId')}</TableCell>
                <TableCell>{t('settings.ruleTopic')}</TableCell>
                <TableCell>{t('settings.ruleLevel')}</TableCell>
                <TableCell>{t('settings.ruleEnabled')}</TableCell>
                <TableCell>{t('settings.ruleTitle')}</TableCell>
                <TableCell>{t('settings.ruleBody')}</TableCell>
                <TableCell />
              </TableRow>
            </TableHead>
            <TableBody>
              {rules.map((rule, index) => (
                <TableRow key={index}>
                  <TableCell>
                    <TextField
                      value={rule.id ?? ''}
                      onChange={(event) =>
                        update((next) => ((next.notifications ??= {}).rules ??= [])[index].id = event.target.value)
                      }
                      slotProps={{ htmlInput: { 'aria-label': t('settings.ruleId') } }}
                    />
                  </TableCell>
                  <TableCell>
                    <TextField
                      value={rule.topic ?? ''}
                      onChange={(event) =>
                        update((next) => ((next.notifications ??= {}).rules ??= [])[index].topic = event.target.value)
                      }
                      slotProps={{ htmlInput: { 'aria-label': t('settings.ruleTopic') } }}
                    />
                  </TableCell>
                  <TableCell>
                    <Select
                      value={rule.level ?? Protocol.Level.Info}
                      onChange={(event) =>
                        update(
                          (next) =>
                            (((next.notifications ??= {}).rules ??= [])[index].level = event.target.value as string)
                        )
                      }
                    >
                      {Protocol.LEVELS.map((level) => (
                        <MenuItem key={level} value={level}>
                          {level}
                        </MenuItem>
                      ))}
                    </Select>
                  </TableCell>
                  <TableCell>
                    <Checkbox
                      checked={rule.enabled ?? true}
                      onChange={(event) =>
                        update(
                          (next) =>
                            (((next.notifications ??= {}).rules ??= [])[index].enabled = event.target.checked)
                        )
                      }
                      slotProps={{ input: { 'aria-label': t('settings.ruleEnabled') } }}
                    />
                  </TableCell>
                  <TableCell>
                    <TextField
                      value={rule.title ?? ''}
                      onChange={(event) =>
                        update((next) => ((next.notifications ??= {}).rules ??= [])[index].title = event.target.value)
                      }
                      slotProps={{ htmlInput: { 'aria-label': t('settings.ruleTitle') } }}
                    />
                  </TableCell>
                  <TableCell>
                    <TextField
                      value={rule.body ?? ''}
                      onChange={(event) =>
                        update((next) => ((next.notifications ??= {}).rules ??= [])[index].body = event.target.value)
                      }
                      slotProps={{ htmlInput: { 'aria-label': t('settings.ruleBody') } }}
                    />
                  </TableCell>
                  <TableCell>
                    <IconButton
                      aria-label={t('settings.removeRule')}
                      onClick={() =>
                        update((next) => {
                          const list = ((next.notifications ??= {}).rules ??= []);
                          list.splice(index, 1);
                        })
                      }
                    >
                      <DeleteIcon fontSize="small" />
                    </IconButton>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Box>
            <Button
              startIcon={<AddIcon />}
              sx={{ textTransform: 'none' }}
              onClick={() =>
                update((next) => {
                  const list = ((next.notifications ??= {}).rules ??= []);
                  list.push({
                    id: `rule-${list.length + 1}`,
                    topic: 'info',
                    level: Protocol.Level.Info,
                    enabled: true,
                    title: '{title|topic}',
                    body: '{body}',
                  });
                })
              }
            >
              {t('settings.addRule')}
            </Button>
          </Box>
        </Stack>
      </Section>

      <Section>
        <SectionHeader icon={<PaletteIcon fontSize="small" />} title={t('settings.appearance')} />
        <Stack spacing={2}>
          <ToggleButtonGroup
            exclusive
            value={gui.displayMode ?? Protocol.DisplayMode.Auto}
            onChange={(_, value) => {
              if (value) {
                update((next) => ((next.gui ??= {}).displayMode = value));
              }
            }}
          >
            <ToggleButton value={Protocol.DisplayMode.Auto}>
              <BrightnessAutoIcon fontSize="small" sx={{ mr: 0.5 }} />
              {t('settings.displayModeAuto')}
            </ToggleButton>
            <ToggleButton value={Protocol.DisplayMode.Light}>
              <LightModeIcon fontSize="small" sx={{ mr: 0.5 }} />
              {t('settings.displayModeLight')}
            </ToggleButton>
            <ToggleButton value={Protocol.DisplayMode.Dark}>
              <DarkModeIcon fontSize="small" sx={{ mr: 0.5 }} />
              {t('settings.displayModeDark')}
            </ToggleButton>
          </ToggleButtonGroup>
          <Box sx={{ display: 'flex', gap: 2, flexWrap: 'wrap' }}>
            <Box>
              <Typography variant="caption" color="text.secondary">
                {t('settings.theme')}
              </Typography>
              <Select
                value={gui.theme ?? Protocol.Theme.Ocean}
                onChange={(event) => update((next) => ((next.gui ??= {}).theme = event.target.value as never))}
                sx={{ minWidth: 180, display: 'block' }}
                inputProps={{ 'aria-label': t('settings.theme') }}
              >
                {Protocol.THEMES.map((theme) => (
                  <MenuItem key={theme} value={theme}>
                    {theme}
                  </MenuItem>
                ))}
              </Select>
            </Box>
            <Box>
              <Typography variant="caption" color="text.secondary">
                {t('settings.language')}
              </Typography>
              <Select
                value={gui.language ?? Protocol.Language.EnUS}
                onChange={(event) => update((next) => ((next.gui ??= {}).language = event.target.value))}
                sx={{ minWidth: 180, display: 'block' }}
                inputProps={{ 'aria-label': t('settings.language') }}
              >
                <MenuItem value={Protocol.Language.EnUS}>English (US)</MenuItem>
              </Select>
            </Box>
            <NumberField
              label={t('settings.maxMessagesPerTopic')}
              value={gui.history?.maxMessagesPerTopic}
              onChange={(value) => update((next) => (((next.gui ??= {}).history ??= {}).maxMessagesPerTopic = value))}
            />
            <NumberField
              label={t('settings.retentionDays')}
              value={gui.history?.retentionDays}
              onChange={(value) => update((next) => (((next.gui ??= {}).history ??= {}).retentionDays = value))}
            />
          </Box>
        </Stack>
      </Section>

      <Section>
        <SectionHeader icon={<UpdateIcon fontSize="small" />} title={t('settings.update')} />
        <Select
          value={draft.update?.checkInterval ?? Protocol.UpdateCheckInterval.Weekly}
          onChange={(event) => update((next) => ((next.update ??= {}).checkInterval = event.target.value as never))}
          sx={{ minWidth: 180 }}
          inputProps={{ 'aria-label': t('settings.checkInterval') }}
        >
          {Object.values(Protocol.UpdateCheckInterval).map((interval) => (
            <MenuItem key={interval} value={interval}>
              {t(`settings.interval.${interval}`)}
            </MenuItem>
          ))}
        </Select>
      </Section>

      <Section>
        <SectionHeader icon={<LockIcon fontSize="small" />} title={t('settings.encryption')} />
        <Typography variant="body2" color="text.secondary">
          {t('settings.notImplemented')}
        </Typography>
      </Section>

      <Section>
        <SectionHeader icon={<CloudIcon fontSize="small" />} title={t('settings.cloudApi')} />
        <Typography variant="body2" color="text.secondary">
          {t('settings.notImplemented')}
        </Typography>
      </Section>

      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, pb: 2 }}>
        <Button variant="contained" onClick={save} disabled={saving} sx={{ textTransform: 'none' }}>
          {t('settings.save')}
        </Button>
        <Button variant="outlined" onClick={() => setDraft(config)} sx={{ textTransform: 'none' }}>
          {t('settings.revert')}
        </Button>
        <Box sx={{ flex: 1 }} />
        <Tooltip title={about?.configPath ?? ''}>
          <Button
            variant="text"
            startIcon={<FolderOpenIcon />}
            onClick={() => openConfigFile().catch(notifyError)}
            sx={{ textTransform: 'none' }}
          >
            {t('settings.openConfigFile')}
          </Button>
        </Tooltip>
      </Box>
    </Box>
  );
}
