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
  Tab,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
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
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import HistoryIcon from '@mui/icons-material/History';
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
import * as Protocol from '../lib/protocol';
import {
  BrokerProtocol,
  type BrokerUrlParts,
  effectivePort,
  joinBrokerUrl,
  splitBrokerUrl,
} from '../lib/brokerUrl';
import { getBrokerInit, openConfigFile } from '../lib/service';
import { useAppStore } from '../lib/store';

/** One category of settings: one entry in the sidebar, one panel beside it. */
export enum ConfigCategory {
  Broker = 'Broker',
  Topics = 'Topics',
  Notifications = 'Notifications',
  Appearance = 'Appearance',
  History = 'History',
  Update = 'Update',
  Advanced = 'Advanced',
}

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
  const [category, setCategory] = useState<ConfigCategory>(ConfigCategory.Broker);
  const [showPassword, setShowPassword] = useState(false);
  const [saving, setSaving] = useState(false);

  // The protocol is a box of its own but not a setting of its own: it is the scheme of
  // the one URL the config keeps. It is held here so that a protocol chosen before the
  // URL is typed stays chosen, rather than being erased along with the empty URL.
  const [url, setUrl] = useState<BrokerUrlParts>(() => splitBrokerUrl(config?.broker?.url));

  const reload = (loaded: Protocol.Config | null) => {
    setDraft(loaded);
    setUrl(splitBrokerUrl(loaded?.broker?.url));
  };

  useEffect(() => {
    setDraft(config);
    setUrl(splitBrokerUrl(config?.broker?.url));
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

  const editUrl = (parts: BrokerUrlParts) => {
    setUrl(parts);
    update((next) => ((next.broker ??= {}).url = joinBrokerUrl(parts)));
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
          }
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
                        (next) => (((next.notifications ??= {}).rules ??= [])[index].enabled = event.target.checked)
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
      </Section>
    </Box>
  );

  const appearancePanel = (
    <Box>
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
        </Box>
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
    [ConfigCategory.Broker]: brokerPanel,
    [ConfigCategory.Topics]: topicsPanel,
    [ConfigCategory.Notifications]: notificationsPanel,
    [ConfigCategory.Appearance]: appearancePanel,
    [ConfigCategory.History]: historyPanel,
    [ConfigCategory.Update]: updatePanel,
    [ConfigCategory.Advanced]: advancedPanel,
  };

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0, p: 1 }}>
      <Box sx={{ display: 'flex', gap: 2, flex: 1, minHeight: 0 }}>
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
            value={ConfigCategory.Broker}
            icon={<CloudIcon sx={{ fontSize: 18 }} />}
            iconPosition="start"
            label={t('settings.broker')}
          />
          <Tab
            value={ConfigCategory.Topics}
            icon={<TopicIcon sx={{ fontSize: 18 }} />}
            iconPosition="start"
            label={t('settings.topics')}
          />
          <Tab
            value={ConfigCategory.Notifications}
            icon={<NotificationsIcon sx={{ fontSize: 18 }} />}
            iconPosition="start"
            label={t('settings.notifications')}
          />
          <Tab
            value={ConfigCategory.Appearance}
            icon={<PaletteIcon sx={{ fontSize: 18 }} />}
            iconPosition="start"
            label={t('settings.appearance')}
          />
          <Tab
            value={ConfigCategory.History}
            icon={<HistoryIcon sx={{ fontSize: 18 }} />}
            iconPosition="start"
            label={t('settings.history')}
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
        <Box sx={{ flex: 1, minWidth: 0, overflow: 'auto', pr: 1, pb: 1 }}>{panels[category]}</Box>
      </Box>

      <Box
        sx={{
          display: 'flex',
          alignItems: 'center',
          gap: 1,
          flexShrink: 0,
          borderTop: 1,
          borderColor: 'divider',
          pt: 1,
          mt: 1,
        }}
      >
        <Button variant="contained" onClick={save} disabled={saving}>
          {t('settings.save')}
        </Button>
        <Button variant="outlined" onClick={() => reload(config)}>
          {t('settings.revert')}
        </Button>
        <Box sx={{ flex: 1 }} />
        <Tooltip title={about?.configPath ?? ''}>
          <Button
            variant="text"
            startIcon={<FolderOpenIcon />}
            onClick={() => openConfigFile().catch(notifyError)}
          >
            {t('settings.openConfigFile')}
          </Button>
        </Tooltip>
      </Box>
    </Box>
  );
}
