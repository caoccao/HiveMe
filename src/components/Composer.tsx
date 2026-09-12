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

import { useState } from 'react';
import {
  Box,
  Checkbox,
  Divider,
  FormControlLabel,
  IconButton,
  ListItemText,
  Menu,
  MenuItem,
  TextField,
  Tooltip,
} from '@mui/material';
import ArrowDropDownIcon from '@mui/icons-material/ArrowDropDown';
import SendIcon from '@mui/icons-material/Send';
import { useTranslation } from 'react-i18next';
import * as Protocol from '../lib/protocol';
import { useAppStore } from '../lib/store';

/** Enter sends; Shift+Enter is a newline. Exported so the behaviour can be tested. */
export function isSendKey(event: { key: string; shiftKey: boolean; ctrlKey: boolean; altKey: boolean }): boolean {
  return event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.altKey;
}

export default function Composer() {
  const { t } = useTranslation();
  const selectedTopic = useAppStore((state) => state.selectedTopic);
  const status = useAppStore((state) => state.status);
  const publish = useAppStore((state) => state.publish);

  const [body, setBody] = useState('');
  const [title, setTitle] = useState('');
  const [asJson, setAsJson] = useState(false);
  const [qos, setQos] = useState<number | null>(null);
  const [retain, setRetain] = useState<boolean | null>(null);
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const [sending, setSending] = useState(false);

  const connected = status.state === Protocol.ConnectionState.Connected;
  const disabled = !connected || selectedTopic === null || sending;
  const reason = !connected ? t('composer.disconnected') : selectedTopic === null ? t('composer.noTopic') : '';

  const send = async () => {
    if (disabled || body.trim() === '' || !selectedTopic) {
      return;
    }
    setSending(true);
    const options: Protocol.PublishOptions = {
      json: asJson,
      qos,
      retain,
      title: asJson || title.trim() === '' ? null : title.trim(),
      level: null,
    };
    const sent = await publish(selectedTopic, body, options);
    setSending(false);
    if (sent) {
      setBody('');
      setTitle('');
    }
  };

  return (
    <Box sx={{ borderTop: 1, borderColor: 'divider', pt: 1, display: 'flex', flexDirection: 'column', gap: 0.5 }}>
      {!asJson && (
        <TextField
          placeholder={t('composer.title')}
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          disabled={disabled}
          size="small"
          fullWidth
          slotProps={{ htmlInput: { 'aria-label': t('composer.title') } }}
        />
      )}
      <Box sx={{ display: 'flex', alignItems: 'flex-end', gap: 0.5 }}>
        <Tooltip title={reason}>
          <Box sx={{ flex: 1 }}>
            <TextField
              placeholder={asJson ? t('composer.placeholderJson') : t('composer.placeholder')}
              value={body}
              onChange={(event) => setBody(event.target.value)}
              onKeyDown={(event) => {
                if (isSendKey(event)) {
                  event.preventDefault();
                  send();
                }
              }}
              disabled={disabled}
              multiline
              maxRows={6}
              fullWidth
              size="small"
              slotProps={{ htmlInput: { 'aria-label': t('composer.placeholder') } }}
            />
          </Box>
        </Tooltip>
        <IconButton
          color="primary"
          aria-label={t('composer.send')}
          disabled={disabled || body.trim() === ''}
          onClick={send}
        >
          <SendIcon fontSize="small" />
        </IconButton>
        <IconButton aria-label={t('composer.options')} onClick={(event) => setAnchor(event.currentTarget)}>
          <ArrowDropDownIcon fontSize="small" />
        </IconButton>
      </Box>

      <Menu anchorEl={anchor} open={anchor !== null} onClose={() => setAnchor(null)}>
        <MenuItem>
          <FormControlLabel
            control={<Checkbox checked={asJson} onChange={(event) => setAsJson(event.target.checked)} />}
            label={t('composer.sendAsJson')}
          />
        </MenuItem>
        <Divider />
        <MenuItem onClick={() => setQos(null)} selected={qos === null}>
          <ListItemText primary={t('composer.qosDefault')} />
        </MenuItem>
        {[0, 1, 2].map((value) => (
          <MenuItem key={value} onClick={() => setQos(value)} selected={qos === value}>
            <ListItemText primary={t('composer.qos', { qos: value })} />
          </MenuItem>
        ))}
        <Divider />
        <MenuItem onClick={() => setRetain(retain === true ? null : true)} selected={retain === true}>
          <ListItemText primary={t('composer.retain')} />
        </MenuItem>
      </Menu>
    </Box>
  );
}
