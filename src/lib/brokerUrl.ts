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

// The broker URL, minus the scheme, which the Settings form asks for as a list instead.
//
// The HiveMQ Cloud console shows a cluster as `host`, as `host:8883`, or as
// `host:8884/mqtt`, and the point of this module is that all three are pasted in and
// saved as they are. Only the scheme is ever taken off the front: the rest of the URL
// is one string here, as it is in the config file and in `hiveme_core::config::url`,
// which stays the one thing that reads a host, a port, and a path out of it.

/** A transport HiveMe speaks. The value is the scheme it is written with. */
export enum BrokerProtocol {
  /** MQTT over TLS, the only transport HiveMQ Cloud accepts. */
  Mqtts = 'mqtts',
  /** MQTT over plain TCP, for a local test broker. */
  Mqtt = 'mqtt',
  /** MQTT over WebSocket with TLS. */
  Wss = 'wss',
  /** MQTT over plain WebSocket, for a local test broker. */
  Ws = 'ws',
}

/** What a new install connects with, because a HiveMQ Cloud cluster accepts nothing else. */
export const DEFAULT_BROKER_PROTOCOL = BrokerProtocol.Mqtts;

/** The spellings the backend also accepts, so a pasted URL is not rejected over its scheme. */
const ALIASES: Record<string, BrokerProtocol> = {
  mqtts: BrokerProtocol.Mqtts,
  ssl: BrokerProtocol.Mqtts,
  'mqtt+ssl': BrokerProtocol.Mqtts,
  mqtt: BrokerProtocol.Mqtt,
  tcp: BrokerProtocol.Mqtt,
  wss: BrokerProtocol.Wss,
  ws: BrokerProtocol.Ws,
};

const DEFAULT_PORTS: Record<BrokerProtocol, number> = {
  [BrokerProtocol.Mqtts]: 8883,
  [BrokerProtocol.Mqtt]: 1883,
  [BrokerProtocol.Wss]: 8884,
  [BrokerProtocol.Ws]: 8083,
};

/** The port the backend connects on when the URL does not carry one. */
export function defaultPort(protocol: BrokerProtocol): number {
  return DEFAULT_PORTS[protocol];
}

/** A broker URL as the form holds it: a protocol, and the rest of the URL untouched. */
export interface BrokerUrlParts {
  protocol: BrokerProtocol;
  /** The host, with the port and the path if the URL carries them, exactly as written. */
  address: string;
}

/** The form a setting that has never been filled in starts from. */
export const EMPTY_BROKER_URL: BrokerUrlParts = {
  protocol: DEFAULT_BROKER_PROTOCOL,
  address: '',
};

/**
 * Takes the scheme off a URL, and nothing else.
 *
 * What the console shows carries no scheme, so most of the time there is nothing to
 * take off and the text is the address as it stands. One that does carry a scheme is
 * read rather than refused, so that pasting a whole URL moves the protocol list instead
 * of leaving `mqtts://` in the box. A scheme HiveMe does not know is still taken off,
 * leaving the protocol on whatever is selected, because the alternative is a URL with
 * two schemes in it.
 */
export function splitBrokerUrl(raw: string | undefined, fallback: BrokerUrlParts = EMPTY_BROKER_URL): BrokerUrlParts {
  const text = (raw ?? '').trim();
  const mark = text.indexOf('://');
  if (mark < 0) {
    return { protocol: fallback.protocol, address: text };
  }
  return {
    protocol: ALIASES[text.slice(0, mark).toLowerCase()] ?? fallback.protocol,
    address: text.slice(mark + 3),
  };
}

/** Writes the scheme back on. Empty while there is no address, so an empty form saves as empty. */
export function joinBrokerUrl(parts: BrokerUrlParts): string {
  const address = parts.address.trim();
  return address === '' ? '' : `${parts.protocol}://${address}`;
}

/**
 * The port the connection will use, for the line under the box that says so.
 *
 * A URL that carries a port says what it is; one that does not is the reason this
 * exists, since the port is then the protocol's own and worth showing rather than
 * leaving the user to guess. Reading it is all this does: the address is still stored
 * and sent as one string.
 */
export function effectivePort(parts: BrokerUrlParts): number {
  const authority = parts.address.split('/')[0] ?? '';
  const written = authority.slice(authority.lastIndexOf(':') + 1);
  const port = Number.parseInt(written, 10);
  const carriesPort = authority.includes(':') && String(port) === written && port > 0 && port <= 65535;
  return carriesPort ? port : defaultPort(parts.protocol);
}
