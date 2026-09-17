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

import { useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Checkbox,
  FormControl,
  FormControlLabel,
  IconButton,
  InputAdornment,
  MenuItem,
  Select,
  Stack,
  Tab,
  Tabs,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Tooltip,
  Typography,
} from '@mui/material';
import AddIcon from '@mui/icons-material/Add';
import AutorenewIcon from '@mui/icons-material/Autorenew';
import BrightnessAutoIcon from '@mui/icons-material/BrightnessAuto';
import CloudIcon from '@mui/icons-material/Cloud';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import DarkModeIcon from '@mui/icons-material/DarkMode';
import DeleteIcon from '@mui/icons-material/Delete';
import HighlightOffIcon from '@mui/icons-material/HighlightOff';
import HistoryIcon from '@mui/icons-material/History';
import EditNoteIcon from '@mui/icons-material/EditNote';
import LightModeIcon from '@mui/icons-material/LightMode';
import LinkIcon from '@mui/icons-material/Link';
import LockIcon from '@mui/icons-material/Lock';
import NotificationsIcon from '@mui/icons-material/Notifications';
import PaletteIcon from '@mui/icons-material/Palette';
import RssFeedIcon from '@mui/icons-material/RssFeed';
import RuleIcon from '@mui/icons-material/Rule';
import TopicIcon from '@mui/icons-material/Topic';
import TuneIcon from '@mui/icons-material/Tune';
import UpdateIcon from '@mui/icons-material/Update';
import VisibilityIcon from '@mui/icons-material/Visibility';
import VisibilityOffIcon from '@mui/icons-material/VisibilityOff';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useTranslation } from 'react-i18next';
import { resolveLanguage } from '../i18n';
import * as Protocol from '../lib/protocol';
import { BrokerProtocol, type BrokerUrlParts, effectivePort, joinBrokerUrl, splitBrokerUrl } from '../lib/brokerUrl';
import { getBrokerInit } from '../lib/service';
import { useAppStore } from '../lib/store';
import TabPanel from './TabPanel';

/** One category of settings: one entry in the sidebar, one panel beside it. */
export enum ConfigCategory {
  Appearance = 'Appearance',
  Broker = 'Broker',
  Editor = 'Editor',
  History = 'History',
  Notifications = 'Notifications',
  Topics = 'Topics',
  Update = 'Update',
  Advanced = 'Advanced',
}

const EDITOR_FIELDS = ['autoComplete', 'autoCorrect', 'autoCapitalize', 'spellCheck', 'writingSuggestions'] as const;

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

function SettingRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <Box
      sx={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        py: 1,
        borderBottom: 1,
        borderColor: 'divider',
      }}
    >
      <Typography variant="body2" color="text.secondary">
        {label}
      </Typography>
      <Box>{children}</Box>
    </Box>
  );
}

/** A group within a category, for the settings that belong together but are not the first thing asked for. */
function Section({ children }: { children: React.ReactNode }) {
  return (
    <Card variant="outlined" sx={{ mt: 2 }}>
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
  const [text, setText] = useState(String(value ?? 0));
  useEffect(() => setText(String(value ?? 0)), [value]);

  return (
    <TextField
      label={label}
      type="number"
      value={text}
      onChange={(event) => {
        const input = event.target.value;
        setText(input);
        const parsed = Number(input);
        if (input.trim() !== '' && Number.isInteger(parsed) && parsed >= 0) {
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
  return drafts.map((draft) =>
    draft.absolute ? { filter: draft.filter.trim(), absolute: true } : draft.filter.trim()
  );
}

export function cliSetupCommand(setup: string): string {
  const escaped = setup.replace(
    /['\u2018\u2019\u201a\u201b]/g,
    (quote) => `\\u${quote.charCodeAt(0).toString(16).padStart(4, '0')}`
  );
  return `hmc --init '${escaped}'`;
}

function RuleTextField({
  id,
  label,
  value,
  onChange,
  action,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  action?: ReactNode;
}) {
  return (
    <>
      <Typography component="label" htmlFor={id} sx={{ textAlign: 'right', gridColumn: 1 }}>
        {label}
      </Typography>
      <TextField
        id={id}
        value={value}
        fullWidth
        sx={{ gridColumn: action ? 2 : '2 / -1' }}
        onChange={(event) => onChange(event.target.value)}
      />
      {action && <Box sx={{ gridColumn: { xs: 2, sm: 3 } }}>{action}</Box>}
    </>
  );
}

export default function Config() {
  const { t } = useTranslation();
  const config = useAppStore((state) => state.config);
  const update = useAppStore((state) => state.updateConfig);
  const flushConfig = useAppStore((state) => state.flushConfig);
  const notifyInfo = useAppStore((state) => state.notifyInfo);
  const notifyError = useAppStore((state) => state.notifyError);
  const [category, setCategory] = useState<ConfigCategory>(ConfigCategory.Appearance);
  const [showPassword, setShowPassword] = useState(false);

  // The protocol is a box of its own but not a setting of its own: it is the scheme of
  // the one URL the config keeps. It is held here so that a protocol chosen before the
  // URL is typed stays chosen, rather than being erased along with the empty URL.
  const [url, setUrl] = useState<BrokerUrlParts>(() => splitBrokerUrl(config?.broker?.url));

  useEffect(() => {
    // Keep the raw text and a protocol selected before an address is entered.
    const stored = config?.broker?.url ?? '';
    setUrl((previous) => (joinBrokerUrl(previous) === stored ? previous : splitBrokerUrl(stored, previous)));
  }, [config?.broker?.url]);

  const subscriptions = useMemo(() => toDrafts(config?.topics?.subscriptions), [config?.topics?.subscriptions]);

  if (!config) {
    return (
      <Typography variant="body2" color="text.secondary" sx={{ p: 2 }}>
        {t('settings.loading')}
      </Typography>
    );
  }

  const editUrl = (parts: BrokerUrlParts) => {
    setUrl(parts);
    update((next) => ((next.broker ??= {}).url = joinBrokerUrl(parts)));
  };

  const broker = config.broker ?? {};
  const notifications = config.notifications ?? {};
  const gui = config.gui ?? {};
  const rules = notifications.rules ?? [];

  const copyCliSetup = async () => {
    try {
      if (!(await flushConfig())) return;
      await writeText(cliSetupCommand(await getBrokerInit()));
      notifyInfo(t('settings.cliSetupCopied'));
    } catch (error) {
      notifyError(error);
    }
  };

  const brokerPanel = (
    <Box>
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
              >
                {t('settings.copyCliSetup')}
              </Button>
            </span>
          </Tooltip>
        }
      />
      <Stack spacing={2}>
        <Stack direction="row" spacing={2}>
          <TextField
            select
            label={t('settings.protocol')}
            value={url.protocol}
            onChange={(event) => editUrl({ ...url, protocol: event.target.value as BrokerProtocol })}
            sx={{ minWidth: 200 }}
          >
            {Object.values(BrokerProtocol).map((protocol) => (
              <MenuItem key={protocol} value={protocol}>
                {t(`settings.protocols.${protocol}`)}
              </MenuItem>
            ))}
          </TextField>
          {/* Whatever the console shows, pasted as it is shown. A scheme in front of it
              is read rather than refused, which moves the list instead of leaving
              mqtts:// in the box; the port and the path stay where the user put them. */}
          <TextField
            label={t('settings.url')}
            placeholder="abc123.s1.eu.hivemq.cloud:8883"
            value={url.address}
            onChange={(event) => editUrl(splitBrokerUrl(event.target.value, url))}
            sx={{ flex: 1 }}
          />
        </Stack>
        <Typography variant="caption" color="text.secondary">
          {url.address.trim() === ''
            ? t('settings.urlHint')
            : t('settings.urlConnectsTo', { url: joinBrokerUrl(url), port: effectivePort(url) })}
        </Typography>
        <Stack direction="row" spacing={2}>
          <TextField
            label={t('settings.username')}
            value={broker.username ?? ''}
            onChange={(event) => update((next) => ((next.broker ??= {}).username = event.target.value))}
            sx={{ flex: 1 }}
          />
          <TextField
            label={t('settings.password')}
            type={showPassword ? 'text' : 'password'}
            value={broker.password ?? ''}
            onChange={(event) => update((next) => ((next.broker ??= {}).password = event.target.value))}
            sx={{ flex: 1 }}
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
        </Stack>
        <Alert severity="info" variant="outlined">
          {t('settings.tlsIsAutomatic')}
        </Alert>
      </Stack>

      <Section>
        <SectionHeader icon={<LinkIcon fontSize="small" />} title={t('settings.connection')} />
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
        </Box>
      </Section>

      <Section>
        <SectionHeader icon={<AutorenewIcon fontSize="small" />} title={t('settings.reconnect')} />
        <Box sx={{ display: 'flex', gap: 2, flexWrap: 'wrap' }}>
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
      </Section>
    </Box>
  );

  const topicsPanel = (
    <Box>
      <SectionHeader icon={<TopicIcon fontSize="small" />} title={t('settings.topics')} />
      <Section>
        <SectionHeader
          icon={<RssFeedIcon fontSize="small" />}
          title={t('settings.subscriptions')}
          action={
            <Button
              startIcon={<AddIcon />}
              onClick={() => {
                const next = [...subscriptions, { filter: '#', absolute: false }];
                update((config) => ((config.topics ??= {}).subscriptions = fromDrafts(next)));
              }}
            >
              {t('settings.addSubscription')}
            </Button>
          }
        />
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
        </Stack>
      </Section>
    </Box>
  );

  const notificationsPanel = (
    <Box>
      <SectionHeader icon={<NotificationsIcon fontSize="small" />} title={t('settings.notifications')} />
      <Stack spacing={1}>
        <FormControlLabel
          control={
            <Checkbox
              checked={notifications.enabled ?? true}
              onChange={(event) => update((next) => ((next.notifications ??= {}).enabled = event.target.checked))}
            />
          }
          label={t('settings.notificationsEnabled')}
        />
        <FormControlLabel
          control={
            <Checkbox
              checked={notifications.topmostEnabled ?? false}
              onChange={(event) =>
                update((next) => ((next.notifications ??= {}).topmostEnabled = event.target.checked))
              }
            />
          }
          label={t('settings.topmostNotificationsEnabled')}
        />
      </Stack>

      <Section>
        <SectionHeader
          icon={<RuleIcon fontSize="small" />}
          title={t('settings.rules')}
          action={
            <Button
              startIcon={<AddIcon />}
              onClick={() =>
                update((next) => {
                  const list = ((next.notifications ??= {}).rules ??= []);
                  let index = list.length + 1;
                  while (list.some((rule) => rule.id === `rule-${index}`)) index += 1;
                  list.push({
                    id: `rule-${index}`,
                    topic: 'info',
                    level: Protocol.Level.Info,
                    enabled: true,
                    os: false,
                    topmost: false,
                    title: '{title|topic}',
                    body: '{body}',
                  });
                })
              }
            >
              {t('settings.addRule')}
            </Button>
          }
        />
        <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
          {t('settings.rawNotificationHint')}
        </Typography>
        <Stack>
          {rules.map((rule, index) => (
            <Box
              key={index}
              role="group"
              aria-label={`${t('settings.ruleId')} ${rule.id ?? ''}`}
              sx={{
                display: 'grid',
                gridTemplateColumns: {
                  xs: 'max-content minmax(0, 1fr)',
                  sm: 'max-content minmax(0, 3fr) minmax(0, 2fr)',
                },
                columnGap: 2,
                rowGap: 1,
                py: 1.5,
                borderTop: 1,
                borderColor: 'divider',
                '&:last-child': { borderBottom: 1 },
                alignItems: 'center',
              }}
            >
              <Typography component="label" htmlFor={`rule-${index}-id`} sx={{ textAlign: 'right' }}>
                {t('settings.ruleId')}
              </Typography>
              <TextField
                id={`rule-${index}-id`}
                value={rule.id ?? ''}
                fullWidth
                onChange={(event) =>
                  update((next) => (((next.notifications ??= {}).rules ??= [])[index].id = event.target.value))
                }
              />
              <Stack
                direction="row"
                spacing={1}
                sx={{ gridColumn: { xs: 2, sm: 3 }, alignItems: 'center', justifyContent: 'space-between' }}
              >
                <FormControlLabel
                  sx={{ m: 0 }}
                  label={t('settings.ruleEnabled')}
                  control={
                    <Checkbox
                      checked={rule.enabled ?? true}
                      onChange={(event) =>
                        update(
                          (next) => (((next.notifications ??= {}).rules ??= [])[index].enabled = event.target.checked)
                        )
                      }
                    />
                  }
                />
                <IconButton
                  aria-label={t('settings.removeRule')}
                  onClick={() =>
                    update((next) => {
                      ((next.notifications ??= {}).rules ??= []).splice(index, 1);
                    })
                  }
                >
                  <HighlightOffIcon fontSize="small" />
                </IconButton>
              </Stack>
              {(['topic', 'title', 'body'] as const).map((field) => {
                const label = t(
                  { topic: 'settings.ruleTopic', title: 'settings.ruleTitle', body: 'settings.ruleBody' }[field]
                );
                return (
                  <RuleTextField
                    key={field}
                    id={`rule-${index}-${field}`}
                    label={label}
                    value={rule[field] ?? ''}
                    action={
                      field === 'topic' ? (
                        <Select
                          value={rule.level ?? Protocol.Level.Info}
                          inputProps={{ 'aria-label': t('settings.ruleLevel') }}
                          sx={{ minWidth: 100 }}
                          onChange={(event) =>
                            update(
                              (next) =>
                                (((next.notifications ??= {}).rules ??= [])[index].level = event.target.value as string)
                            )
                          }
                        >
                          {Protocol.LEVELS.map((level) => (
                            <MenuItem key={level} value={level}>
                              {t(`levels.${level}`)}
                            </MenuItem>
                          ))}
                        </Select>
                      ) : (
                        <FormControlLabel
                          sx={{ m: 0 }}
                          label={t(field === 'title' ? 'settings.ruleOs' : 'settings.ruleTopmost')}
                          control={
                            <Checkbox
                              checked={(field === 'title' ? rule.os : rule.topmost) ?? false}
                              onChange={(event) =>
                                update(
                                  (next) =>
                                    (((next.notifications ??= {}).rules ??= [])[index][
                                      field === 'title' ? 'os' : 'topmost'
                                    ] = event.target.checked)
                                )
                              }
                            />
                          }
                        />
                      )
                    }
                    onChange={(value) =>
                      update((next) => (((next.notifications ??= {}).rules ??= [])[index][field] = value))
                    }
                  />
                );
              })}
            </Box>
          ))}
        </Stack>
      </Section>
    </Box>
  );

  const appearancePanel = (
    <Box>
      <SectionHeader icon={<PaletteIcon fontSize="small" />} title={t('settings.appearance')} />
      <SettingRow label={t('settings.mode')}>
        <ToggleButtonGroup
          exclusive
          size="small"
          aria-label={t('settings.mode')}
          value={gui.displayMode ?? Protocol.DisplayMode.Auto}
          onChange={(_, value) => {
            if (value) {
              update((next) => ((next.gui ??= {}).displayMode = value));
            }
          }}
        >
          <ToggleButton value={Protocol.DisplayMode.Auto} sx={{ px: 1.5, gap: 0.5 }}>
            <BrightnessAutoIcon sx={{ fontSize: 16 }} />
            <Typography variant="caption">{t('settings.displayModeAuto')}</Typography>
          </ToggleButton>
          <ToggleButton value={Protocol.DisplayMode.Light} sx={{ px: 1.5, gap: 0.5 }}>
            <LightModeIcon sx={{ fontSize: 16 }} />
            <Typography variant="caption">{t('settings.displayModeLight')}</Typography>
          </ToggleButton>
          <ToggleButton value={Protocol.DisplayMode.Dark} sx={{ px: 1.5, gap: 0.5 }}>
            <DarkModeIcon sx={{ fontSize: 16 }} />
            <Typography variant="caption">{t('settings.displayModeDark')}</Typography>
          </ToggleButton>
        </ToggleButtonGroup>
      </SettingRow>
      <SettingRow label={t('settings.theme')}>
        <FormControl size="small" sx={{ minWidth: 150 }}>
          <Select
            value={gui.theme ?? Protocol.Theme.Ocean}
            onChange={(event) => update((next) => ((next.gui ??= {}).theme = event.target.value as never))}
            inputProps={{ 'aria-label': t('settings.theme') }}
          >
            {Protocol.THEMES.map((theme) => (
              <MenuItem key={theme} value={theme}>
                {t(`settings.themes.${theme}`)}
              </MenuItem>
            ))}
          </Select>
        </FormControl>
      </SettingRow>
      <SettingRow label={t('settings.language')}>
        <FormControl size="small" sx={{ minWidth: 150 }}>
          <Select
            value={resolveLanguage(gui.language)}
            onChange={(event) => update((next) => ((next.gui ??= {}).language = event.target.value))}
            inputProps={{ 'aria-label': t('settings.language') }}
          >
            {Protocol.LANGUAGES.map((language) => (
              <MenuItem key={language} value={language} lang={language}>
                {Protocol.LANGUAGE_LABELS[language]}
              </MenuItem>
            ))}
          </Select>
        </FormControl>
      </SettingRow>
    </Box>
  );

  const editorPanel = (
    <Box>
      <SectionHeader icon={<EditNoteIcon fontSize="small" />} title={t('settings.editor')} />
      <Stack spacing={1}>
        {EDITOR_FIELDS.map((field) => (
          <FormControlLabel
            key={field}
            sx={{ m: 0 }}
            control={
              <Checkbox
                checked={gui.editor?.[field] ?? false}
                onChange={(event) =>
                  update((next) => (((next.gui ??= {}).editor ??= {})[field] = event.target.checked))
                }
              />
            }
            label={t(`settings.${field}`)}
          />
        ))}
      </Stack>
    </Box>
  );

  const historyPanel = (
    <Box>
      <SectionHeader icon={<HistoryIcon fontSize="small" />} title={t('settings.history')} />
      <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
        {t('settings.historyHint')}
      </Typography>
      <Box sx={{ display: 'flex', gap: 2, flexWrap: 'wrap' }}>
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
    </Box>
  );

  const updatePanel = (
    <Box>
      <SectionHeader icon={<UpdateIcon fontSize="small" />} title={t('settings.update')} />
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, flexWrap: 'wrap' }}>
        <Typography variant="body2" color="text.secondary">
          {t('settings.checkInterval')}
        </Typography>
        <Select
          value={config.update?.checkInterval ?? Protocol.UpdateCheckInterval.Weekly}
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
      </Box>
    </Box>
  );

  const advancedPanel = (
    <Box>
      <SectionHeader icon={<TuneIcon fontSize="small" />} title={t('settings.advanced')} />
      <Typography variant="body2" color="text.secondary">
        {t('settings.advancedHint')}
      </Typography>
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
    </Box>
  );

  const panels: Record<ConfigCategory, React.ReactNode> = {
    [ConfigCategory.Appearance]: appearancePanel,
    [ConfigCategory.Broker]: brokerPanel,
    [ConfigCategory.Editor]: editorPanel,
    [ConfigCategory.History]: historyPanel,
    [ConfigCategory.Notifications]: notificationsPanel,
    [ConfigCategory.Topics]: topicsPanel,
    [ConfigCategory.Update]: updatePanel,
    [ConfigCategory.Advanced]: advancedPanel,
  };

  return (
    <Box
      sx={{
        width: '100%',
        maxWidth: 960,
        mx: 'auto',
        py: 2,
        px: 1,
        display: 'flex',
        gap: 2,
        height: '100%',
        boxSizing: 'border-box',
        minHeight: 0,
      }}
    >
      <Tabs
        orientation="vertical"
        variant="scrollable"
        value={category}
        onChange={(_, value: ConfigCategory) => setCategory(value)}
        aria-label={t('settings.categories')}
        sx={{
          borderRight: 1,
          borderColor: 'divider',
          flexShrink: 0,
          minWidth: 180,
          '& .MuiTab-root': {
            minHeight: 40,
            alignItems: 'center',
            justifyContent: 'flex-start',
            textAlign: 'left',
          },
        }}
      >
        <Tab
          value={ConfigCategory.Appearance}
          icon={<PaletteIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t('settings.appearance')}
        />
        <Tab
          value={ConfigCategory.Broker}
          icon={<CloudIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t('settings.broker')}
        />
        <Tab
          value={ConfigCategory.Editor}
          icon={<EditNoteIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t('settings.editor')}
        />
        <Tab
          value={ConfigCategory.History}
          icon={<HistoryIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t('settings.history')}
        />
        <Tab
          value={ConfigCategory.Notifications}
          icon={<NotificationsIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t('settings.notifications')}
        />
        <Tab
          value={ConfigCategory.Topics}
          icon={<TopicIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t('settings.topics')}
        />
        <Tab
          value={ConfigCategory.Update}
          icon={<UpdateIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t('settings.update')}
        />
        <Tab
          value={ConfigCategory.Advanced}
          icon={<TuneIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t('settings.advanced')}
        />
      </Tabs>
      <Box sx={{ flex: 1, minWidth: 0, minHeight: 0, position: 'relative' }}>
        {Object.values(ConfigCategory).map((value) => (
          <TabPanel key={value} active={value === category} label={t(`settings.${value.toLowerCase()}`)}>
            {panels[value]}
          </TabPanel>
        ))}
      </Box>
    </Box>
  );
}
