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

import { describe, expect, it } from 'vitest';
import {
  BrokerProtocol,
  DEFAULT_BROKER_PROTOCOL,
  EMPTY_BROKER_URL,
  defaultPort,
  effectivePort,
  joinBrokerUrl,
  splitBrokerUrl,
} from './brokerUrl';

// The three the console shows, which are the three that have to survive a paste.
const CONSOLE = {
  mqtt: 'abc123.s1.eu.hivemq.cloud',
  tlsMqtt: 'abc123.s1.eu.hivemq.cloud:8883',
  tlsWebsocket: 'abc123.s1.eu.hivemq.cloud:8884/mqtt',
};

describe('reading a broker URL', () => {
  it('starts a new install on TLS MQTT, which is all a HiveMQ Cloud cluster accepts', () => {
    expect(splitBrokerUrl(undefined)).toEqual(EMPTY_BROKER_URL);
    expect(splitBrokerUrl('')).toEqual(EMPTY_BROKER_URL);
    expect(DEFAULT_BROKER_PROTOCOL).toBe(BrokerProtocol.Mqtts);
  });

  it('leaves what the console shows exactly as it was pasted', () => {
    for (const shown of Object.values(CONSOLE)) {
      expect(splitBrokerUrl(shown).address).toBe(shown);
    }
  });

  it('keeps the protocol that is selected for a URL that names none', () => {
    const websocket = { protocol: BrokerProtocol.Wss, address: '' };
    expect(splitBrokerUrl(CONSOLE.tlsWebsocket, websocket)).toEqual({
      protocol: BrokerProtocol.Wss,
      address: CONSOLE.tlsWebsocket,
    });
  });

  it('takes a scheme off the front and moves the list to it', () => {
    expect(splitBrokerUrl(`mqtts://${CONSOLE.tlsMqtt}`)).toEqual({
      protocol: BrokerProtocol.Mqtts,
      address: CONSOLE.tlsMqtt,
    });
    expect(splitBrokerUrl(`wss://${CONSOLE.tlsWebsocket}`)).toEqual({
      protocol: BrokerProtocol.Wss,
      address: CONSOLE.tlsWebsocket,
    });
    expect(splitBrokerUrl('mqtt://localhost:1884')).toEqual({
      protocol: BrokerProtocol.Mqtt,
      address: 'localhost:1884',
    });
  });

  it('reads the spellings the backend also accepts', () => {
    expect(splitBrokerUrl('ssl://host').protocol).toBe(BrokerProtocol.Mqtts);
    expect(splitBrokerUrl('mqtt+ssl://host').protocol).toBe(BrokerProtocol.Mqtts);
    expect(splitBrokerUrl('tcp://host').protocol).toBe(BrokerProtocol.Mqtt);
    expect(splitBrokerUrl('MQTTS://host').protocol).toBe(BrokerProtocol.Mqtts);
  });
});

describe('writing a broker URL back', () => {
  it('writes the scheme the user no longer has to type, and nothing else', () => {
    expect(joinBrokerUrl({ protocol: BrokerProtocol.Mqtts, address: CONSOLE.tlsMqtt })).toBe(
      'mqtts://abc123.s1.eu.hivemq.cloud:8883'
    );
    expect(joinBrokerUrl({ protocol: BrokerProtocol.Wss, address: CONSOLE.tlsWebsocket })).toBe(
      'wss://abc123.s1.eu.hivemq.cloud:8884/mqtt'
    );
    expect(joinBrokerUrl({ protocol: BrokerProtocol.Mqtt, address: CONSOLE.mqtt })).toBe(
      'mqtt://abc123.s1.eu.hivemq.cloud'
    );
  });

  it('writes nothing at all while there is no URL, so that an empty form saves as empty', () => {
    expect(joinBrokerUrl({ ...EMPTY_BROKER_URL, address: '   ' })).toBe('');
  });

  it('round trips every protocol', () => {
    for (const protocol of Object.values(BrokerProtocol)) {
      const parts = { protocol, address: CONSOLE.tlsMqtt };
      expect(splitBrokerUrl(joinBrokerUrl(parts))).toEqual(parts);
    }
  });
});

describe('the port a URL will use', () => {
  it('is the one the URL carries', () => {
    expect(effectivePort({ protocol: BrokerProtocol.Mqtts, address: CONSOLE.tlsMqtt })).toBe(8883);
    expect(effectivePort({ protocol: BrokerProtocol.Wss, address: CONSOLE.tlsWebsocket })).toBe(8884);
    expect(effectivePort({ protocol: BrokerProtocol.Mqtt, address: 'localhost:1884' })).toBe(1884);
  });

  it('is the protocol default when the URL carries none, which is why it is shown', () => {
    expect(effectivePort({ protocol: BrokerProtocol.Mqtts, address: CONSOLE.mqtt })).toBe(defaultPort(BrokerProtocol.Mqtts));
    expect(effectivePort({ protocol: BrokerProtocol.Mqtt, address: CONSOLE.mqtt })).toBe(1883);
    expect(effectivePort({ protocol: BrokerProtocol.Wss, address: 'host/mqtt' })).toBe(8884);
    expect(effectivePort({ protocol: BrokerProtocol.Ws, address: 'host' })).toBe(8083);
  });

  it('ignores something after the colon that is not a port', () => {
    expect(effectivePort({ protocol: BrokerProtocol.Mqtts, address: 'host:' })).toBe(8883);
    expect(effectivePort({ protocol: BrokerProtocol.Mqtts, address: 'host:80ab' })).toBe(8883);
    expect(effectivePort({ protocol: BrokerProtocol.Mqtts, address: 'host:99999' })).toBe(8883);
  });
});
