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

import { useId, useRef, useState } from 'react';
import {
  Box, Button, Checkbox, Collapse, FormControlLabel, MenuItem, Radio, RadioGroup,
  Select, TextField, Tooltip, Typography,
} from '@mui/material';
import ExpandLessIcon from '@mui/icons-material/ExpandLess';
import ExpandMoreIcon from '@mui/icons-material/ExpandMore';
import { useTranslation } from 'react-i18next';
import * as Protocol from '../lib/protocol';
import { levelColor } from '../lib/message';
import { useAppStore } from '../lib/store';

interface Draft {
  body: string;
  topic: string;
  title: string;
  level: Protocol.Level;
  qos: number | null;
  retain: boolean;
  asJson: boolean;
  expanded: boolean;
}

const EMPTY_DRAFT: Draft = {
  body: '',
  topic: '',
  title: '',
  level: Protocol.Level.Info,
  qos: null,
  retain: false,
  asJson: false,
  expanded: false,
};

export function isSendKey(event: {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey?: boolean;
  isComposing?: boolean;
}): boolean {
  return event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.altKey
    && !event.metaKey && !event.isComposing;
}

export default function Composer() {
  const { t } = useTranslation();
  const id = useId();
  const selectedTopic = useAppStore((state) => state.selectedTopic);
  const status = useAppStore((state) => state.status);
  const publish = useAppStore((state) => state.publish);

  // Messages stays mounted across tabs. Drafts live only in this window's memory.
  const [drafts, setDrafts] = useState(() => new Map<string, Draft>());
  const [sending, setSending] = useState(false);
  const sendPending = useRef(false);
  const draft = selectedTopic === null ? EMPTY_DRAFT : drafts.get(selectedTopic) ?? EMPTY_DRAFT;
  const { body, topic, title, level, qos, retain, asJson, expanded } = draft;
  const color = levelColor(level);

  const connected = status.state === Protocol.ConnectionState.Connected;
  const disabled = !connected || selectedTopic === null || sending;
  const reason = !connected ? t('composer.disconnected') : selectedTopic === null ? t('composer.noTopic') : '';
  const placeholder = asJson ? t('composer.placeholderJson') : t('composer.placeholder');

  const updateDraft = (changes: Partial<Draft>) => {
    if (selectedTopic === null) return;
    setDrafts((previous) => new Map(previous).set(selectedTopic, {
      ...(previous.get(selectedTopic) ?? EMPTY_DRAFT), ...changes,
    }));
  };

  const send = async () => {
    if (disabled || sendPending.current || body.trim() === '' || !selectedTopic) return;
    sendPending.current = true;
    setSending(true);
    try {
      const options: Protocol.PublishOptions = {
        topic: topic.trim() || null,
        json: asJson,
        qos,
        retain,
        title: asJson || title.trim() === '' ? null : title.trim(),
        level: asJson ? null : level,
      };
      const sent = await publish(selectedTopic, body, options);
      if (sent) {
        // Completion belongs to the originating topic, even if selection changed.
        setDrafts((previous) => {
          const current = previous.get(selectedTopic);
          if (!current || current.body !== body) return previous;
          return new Map(previous).set(selectedTopic, { ...current, body: '' });
        });
      }
    } finally {
      sendPending.current = false;
      setSending(false);
    }
  };

  return (
    <Box
      onKeyDownCapture={(event) => {
        // Dropdown menu items live in a portal; Enter there chooses an option.
        if (!event.currentTarget.contains(event.target as Node)) return;
        if (isSendKey({ ...event, isComposing: event.nativeEvent.isComposing })) {
          event.preventDefault();
          event.stopPropagation();
          void send();
        }
      }}
      sx={{
        borderTop: 1, borderColor: 'divider', p: '4px',
        display: 'flex', flexDirection: 'column',
        flexShrink: 0, maxHeight: '70%', overflow: 'auto',
        '& .MuiOutlinedInput-root': { p: '4px' },
        '& .MuiOutlinedInput-input': { p: 0 },
        '& .MuiButton-root': { p: '2px 4px', minHeight: 24 },
        '& .MuiButton-endIcon': { ml: '4px', mr: 0 },
        '& .MuiRadio-root, & .MuiCheckbox-root': { p: '2px' },
      }}
    >
      <Tooltip title={reason}>
        <Box sx={{ mb: '4px' }}>
          <TextField
            placeholder={placeholder}
            value={body}
            onChange={(event) => updateDraft({ body: event.target.value })}
            disabled={disabled}
            multiline
            minRows={3}
            maxRows={6}
            fullWidth
            size="small"
            slotProps={{ htmlInput: { 'aria-label': placeholder } }}
          />
        </Box>
      </Tooltip>

      <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'flex-end', flexWrap: 'wrap', gap: '4px' }}>
        <Select
          name="composer-level"
          value={level}
          onChange={(event) => updateDraft({ level: event.target.value as Protocol.Level })}
          disabled={disabled || asJson}
          size="small"
          inputProps={{ 'aria-label': t('composer.level') }}
          sx={{
            minWidth: 96, height: 24,
            '& .MuiSelect-select': { pr: '24px', color: color === 'default' ? 'text.primary' : `${color}.main` },
          }}
        >
          {[Protocol.Level.Info, Protocol.Level.Error, Protocol.Level.Success, Protocol.Level.Warn].map((value) => {
            const optionColor = levelColor(value);
            return (
              <MenuItem key={value} value={value} sx={{ color: optionColor === 'default' ? 'text.primary' : `${optionColor}.main` }}>
                {t(`levels.${value}`)}
              </MenuItem>
            );
          })}
        </Select>
        <Button
          color="inherit"
          endIcon={expanded ? <ExpandLessIcon /> : <ExpandMoreIcon />}
          aria-expanded={expanded}
          aria-controls={id + '-options'}
          onClick={() => updateDraft({ expanded: !expanded })}
          disabled={selectedTopic === null}
        >
          {t('composer.options')}
        </Button>
        <Button
          variant="outlined"
          aria-label={t('composer.send')}
          disabled={disabled || body.trim() === ''}
          onClick={() => void send()}
          sx={{ minWidth: 64 }}
        >
          {t('composer.send')}
        </Button>
      </Box>

      <Collapse id={id + '-options'} in={expanded} unmountOnExit>
        <Box sx={{ pt: '4px', display: 'grid', gridTemplateColumns: 'max-content minmax(0, 1fr)', alignItems: 'center', gap: '4px' }}>
          <Typography component="label" htmlFor={id + '-topic'} sx={{ textAlign: 'right' }}>
            {t('composer.topic')}
          </Typography>
          <TextField
            id={id + '-topic'}
            value={topic}
            onChange={(event) => updateDraft({ topic: event.target.value.replace(/^\/+/, '') })}
            disabled={disabled}
            size="small"
            fullWidth
          />
          <Typography component="label" htmlFor={id + '-title'} sx={{ textAlign: 'right' }}>
            {t('composer.title')}
          </Typography>
          <TextField
            id={id + '-title'}
            value={title}
            onChange={(event) => updateDraft({ title: event.target.value })}
            disabled={disabled || asJson}
            size="small"
            fullWidth
          />
          <Typography id={id + '-qos'} sx={{ textAlign: 'right' }}>
            {t('composer.qos')}
          </Typography>
          <Box sx={{ display: 'flex', alignItems: 'center', flexWrap: 'wrap', rowGap: '4px', columnGap: '16px' }}>
            <RadioGroup
              row
              aria-labelledby={id + '-qos'}
              name={id + '-qos'}
              value={qos ?? 'config'}
              onChange={(_, value) => updateDraft({ qos: value === 'config' ? null : Number(value) })}
              sx={{ columnGap: '4px' }}
            >
              <FormControlLabel value="config" control={<Radio size="small" />} label={t('composer.qosDefault')} disabled={disabled} sx={{ m: 0 }} />
              {[0, 1, 2].map((value) => (
                <FormControlLabel key={value} value={value} control={<Radio size="small" />} label={value} disabled={disabled} sx={{ m: 0 }} />
              ))}
            </RadioGroup>
            <FormControlLabel
              control={<Checkbox size="small" checked={retain} onChange={(event) => updateDraft({ retain: event.target.checked })} />}
              label={t('composer.retain')}
              disabled={disabled}
              sx={{ ml: 0, mr: 0 }}
            />
            <FormControlLabel
              control={<Checkbox size="small" checked={asJson} onChange={(event) => updateDraft({ asJson: event.target.checked })} />}
              label={t('composer.sendAsJson')}
              disabled={disabled}
              sx={{ ml: 0, mr: 0 }}
            />
          </Box>
        </Box>
      </Collapse>
    </Box>
  );
}
