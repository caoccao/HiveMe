# HiveMQ Cloud

How HiveMe uses HiveMQ Cloud: what the broker offers, what HiveMe relies on, and
what the REST API can and cannot do for this project.

Sources:

- [HiveMQ Cloud documentation](https://docs.hivemq.com/hivemq-cloud/index.html)
- [Quick start guide](https://docs.hivemq.com/hivemq-cloud/quick-start-guide.html)
- [Authentication and authorization](https://docs.hivemq.com/hivemq-cloud/authn-authz.html)
- [Console](https://docs.hivemq.com/hivemq-cloud/console.html)
- [REST API](https://docs.hivemq.com/hivemq-cloud/rest-api.html)
- [REST API specification](https://docs.hivemq.com/hivemq-cloud/rest-api/specification/) (`public-saas-openapi.yml`, OpenAPI 3.0.3)

## Connectivity

- HiveMQ Cloud accepts TLS connections only. MQTT over TLS on port 8883, MQTT over
  WebSocket TLS on port 8884 (path `/mqtt`).
- Cluster hostnames look like `<id>.s1.eu.hivemq.cloud` on Serverless, or a custom
  domain on Starter and above. TLS SNI is required, so the client must send the
  hostname during the handshake.
- The server certificate chain is issued by a public CA (Let's Encrypt). Operating
  system root stores already trust it, so HiveMe uses native roots by default and
  allows an optional custom CA file for local brokers.
- MQTT 3.1, 3.1.1, and 5.0 are supported. HiveMe uses MQTT 5.
- Serverless limits: 100 concurrent connections, 10 GB traffic per month, shared
  infrastructure, no uptime SLA.
- Every MQTT connection needs a unique client identifier, so `hmc` and `hmg` on the
  same device must not share one.

## Authentication and authorization

- Clients authenticate with a username and password in the CONNECT packet.
  Credentials are created in the Cloud Console under Access Management.
- Serverless attaches permissions directly to the credential. The default
  permission set is "Publish and Subscribe" on `#`.
- Starter and above add role based access with custom permissions (topic filter,
  publish and subscribe flags, QoS levels, retained messages, shared subscriptions,
  and dynamic variables such as `${mqtt-username}`), client certificates, and JWT
  authentication.
- Authorization is whitelist only: anything not explicitly allowed is denied.
- Credential changes take up to one minute to apply. Permission changes take up to
  five minutes, role changes up to a day.

## How HiveMe connects

Implemented in `hiveme-core::mqtt`, over [rumqttc](https://crates.io/crates/rumqttc)
with the MQTT 5 module and rustls. Both applications share the module so that a
message from the `hmg` composer is indistinguishable from one `hmc` sent.

### Role

The only difference between the two applications is the role they connect as.

| Concern | `hmc` (`Role::Cli`) | `hmg` (`Role::Gui`) |
|---------|---------------------|---------------------|
| Client id | `<prefix>-hmc-<8 of device.id>-<8 random>` | `<prefix>-hmg-<8 of device.id>` |
| Clean start | yes | no |
| Session expiry | 0 | `broker.sessionExpirySecs` |
| Reconnect | none; the first drop ends the connection | exponential backoff with jitter |
| Subscriptions | none | `topics.subscriptions` |

`<prefix>` is `broker.clientIdPrefix`, `hiveme` by default. The device segment is the
first eight alphanumeric characters of `device.id`. Every `hmc` run adds a random
suffix, because a broker disconnects the older of two connections that share an
identifier and `hmc` runs overlap with each other and with a running `hmg`.

### Transport and TLS

| `broker.url` scheme | Default port | Transport |
|---------------------|--------------|-----------|
| `mqtts` | 8883 | MQTT over TLS. The scheme HiveMQ Cloud is reached with. |
| `mqtt` | 1883 | Plain TCP, for a local test broker. Logs a warning, because the password crosses the network in the clear. |
| `wss` | 8884 | MQTT over WebSocket with TLS, path `/mqtt`. Needs the `websocket` feature of `hiveme-core`, which is off by default. |
| `ws` | 8083 | Plain WebSocket, same feature. |

The trust store is the operating system's, loaded with
[rustls-native-certs](https://crates.io/crates/rustls-native-certs). That is enough for
HiveMQ Cloud on its own, because the cluster certificate chains to a public CA.
`broker.tls.caFile` adds PEM certificates to those roots rather than replacing them,
so a configuration for a local broker with a private CA still reaches the cloud.
rustls sends the host of `broker.url` as the SNI name, which HiveMQ Cloud requires.

`broker.tls.verifyServer = false` skips the check that the certificate belongs to the
host. It is honoured only for hosts outside `hivemq.cloud`: on a cloud host the
credentials travel in the CONNECT packet, so an unverified connection would hand them
to whatever answered, and the certificate is verified anyway with a warning in the
log. Turning it off anywhere logs a warning.

### Session settings

* Keep alive is `broker.keepAliveSecs`, 30 seconds by default.
* The connect timeout is `broker.connectTimeoutSecs`, 10 seconds by default. It bounds
  both the TCP connection and the wait for the CONNACK.
* `MqttClient::connect` returns only once the broker has answered, so a wrong password
  is reported to the caller rather than retried in the background.
* A `Role::Gui` client retries a transient failure inside the connect timeout, but a
  refusal, a TLS failure, or the timeout running out is reported. Once the first
  connection is up it reconnects on its own for as long as it lives.

### Reconnect

`broker.reconnect.initialDelayMs` doubles per attempt up to
`broker.reconnect.maxDelayMs`, and half of each delay is randomised. The jitter matters
because several installations share a cluster, and a cluster restart would otherwise
bring them all back at the same instant. A successful connection resets the sequence.

These errors are never retried, because retrying them cannot work: a CONNACK refusal
(the credentials or the client id), a TLS failure, an answer that is not a CONNACK, and
the client handle being dropped.

When a reconnect comes back with no session, which is what the broker reports when the
session expired or the cluster was replaced, the remembered subscriptions are sent
again. A caller therefore subscribes once rather than on every reconnect.

### Publishing and subscribing

* `publish` returns once the broker has acknowledged: nothing to wait for at QoS 0, the
  PUBACK at QoS 1, the PUBCOMP at QoS 2. It gives up after `publish.timeoutSecs`. This
  is what lets `hmc` exit knowing the message landed.
* A PUBACK reason other than success is an error naming the topic.
  `NoMatchingSubscribers` is not an error: it is the normal answer when `hmg` is closed.
* `subscribe` returns once the SUBACK has arrived, and a SUBACK carries one reason code
  per filter. HiveMQ Cloud refuses a filter the credential has no permission for rather
  than refusing the whole packet, so a refusal is reported naming that filter.
* A topic with a wildcard, or a filter that is not a filter, is refused before it
  reaches the broker, so the error names the mistake instead of the packet.
* Every published message carries the MQTT 5 properties of its envelope: content type
  `application/json`, payload format indicator 1, the `hiveme-v` user property, and the
  message expiry interval when the envelope has a `ttlSecs`. See
  [message.md](message.md).

### Receiving

Received messages go onto a bounded channel, 1024 deep. When nothing is reading it the
event loop drops messages and counts them rather than blocking, because blocking there
would stop the keep alive and cost the connection.

### Serverless limits that shape this

* 100 concurrent connections, so `hmc` disconnects rather than leaving a session behind:
  a broker releases the session of a client that says goodbye at once instead of waiting
  out the keep alive.
* Credentials take up to a minute to become active, which is why a
  `BadUserNamePassword` refusal says so.
* There is no way to raise the connection limit, so the client identifiers of the two
  applications must differ; they do, by construction.

### Tested against

`crates/hiveme-core/tests/mqtt.rs` runs against a `hivemq/hivemq-ce` container through
`testcontainers`: the message round trip and its properties, every quality of service,
retained messages, a wildcard subscription over the prefix, payloads that are not HiveMe
envelopes, refusals, and a reconnect onto a replacement broker that has never heard of
the client. The container has no TLS, so the handshake against a public chain is covered
only by the opt-in test described in [development.md](../development.md#testing-against-a-broker).

## REST API

The HiveMQ Cloud REST API is **available on the Starter plan and above only**. It is
not usable on a Serverless cluster, so HiveMe treats it as optional. See
[app.md](app.md#decisions), decision 1.

- The base URL is region specific and shown on the cluster's API Access tab, for
  example `https://api.a01.euc1.aws.hivemq.cloud`.
- All paths live under `/api/v2/orgs/{orgId}/clusters/{clusterId}/`. `orgId` is a
  six character alphanumeric string, `clusterId` is a UUID.
- Authentication is `Authorization: Bearer <JWT>` with an API token created in the
  console. The token is displayed once, at creation time.
- Pagination is cursor based. `limit` is between 50 and 2500 and defaults to 500.
  Responses carry `_links.next`; HTTP 410 means the cursor expired.
- Errors are returned as `{ "errors": [ { "title": "...", "detail": "..." } ] }`.

### Endpoints

All paths are relative to `/api/v2/orgs/{orgId}/clusters/{clusterId}`.

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/mqtt/clients` | List all client sessions (paginated) |
| GET | `/a/mqtt/clients/search` | Filter clients by `boolean-filter`, `string-filter`, `number-filter` |
| GET | `/mqtt/clients/{clientId}` | Client details: connection, TLS info, queue, session expiry |
| DELETE | `/mqtt/clients/{clientId}` | Invalidate a session, disconnecting the client if online |
| GET | `/mqtt/clients/{clientId}/connection` | Connection state only |
| DELETE | `/mqtt/clients/{clientId}/connection` | Disconnect a client |
| GET | `/mqtt/clients/{clientId}/subscriptions` | List a client's subscriptions |
| GET | `/metrics` | Broker metrics as `text/plain`, Prometheus style |
| GET, POST | `/mqtt/credentials` | List (paginated) or create credentials |
| GET, DELETE | `/mqtt/credentials/username/{username}` | Get or delete credentials |
| GET, POST, PUT | `/mqtt/permissions` | List, create, or replace all permissions |
| PUT, DELETE | `/mqtt/permissions/{id}` | Update or delete a permission |
| GET, POST | `/mqtt/roles` | List or create roles |
| PUT, DELETE | `/mqtt/roles/{roleId}` | Update or delete a role |
| GET | `/mqtt/roles/permissions` | All role to permission links |
| GET | `/mqtt/roles/{roleIdOrName}/permissions` | Permissions of one role |
| PUT | `/mqtt/roles/{roleId}/permissions/{permissionId}/attach` | Link a permission to a role |
| PUT | `/mqtt/roles/{roleId}/permissions/{permissionId}/detach` | Unlink a permission from a role |
| GET | `/user/{username}/roles` | Roles of a credential |
| PUT | `/user/{username}/roles/{roleId}/attach` | Link a role to a credential |
| PUT | `/user/{username}/roles/{roleId}/detach` | Unlink a role from a credential |

### Key schemas

- `Credentials { username, password }`
- `UserInfo { username, roleRefs[] }`
- `MQTTPermission { id, name, description, topic, publishAllowed, subscribeAllowed, qos0Allowed, qos1Allowed, qos2Allowed, retainedMsgsAllowed, sharedSubAllowed, sharedGroup, roles[], applyTo, variables[] }`
- `RoleInfo { id, name, description }`
- `ClientDetails { id, connected, connectedAt, sessionExpiryInterval, messageQueueSize, willPresent, restrictions, connection }`
- `ClientSubscription { topicFilter, qos, retainHandling, retainAsPublished, noLocal, subscriptionIdentifier }`

### Consequences for HiveMe

1. There is no REST endpoint to publish a message or to enumerate topics.
   Publishing is MQTT only, and the GUI topic tree is built from the messages that
   arrive on its subscriptions.
2. The REST API is optional. Its natural uses are a "Cluster" tab in the GUI
   (connected clients, their subscriptions, broker metrics) and, later, credential
   management. It is configured through the `cloudApi` block in
   [config.md](config.md) and implemented in phase 6.
3. TLS is mandatory for the cloud. Plain `mqtt://` remains allowed in the config for
   local test brokers and logs a warning.
4. `hmc` and `hmg` must use distinct client identifiers on the same device.

## Console walkthrough

1. Create an account at [hivemq.com](https://www.hivemq.com/) and create a Serverless
   cluster.
2. On the cluster Overview tab, copy the URL and the TLS MQTT port (8883).
3. On the Access Management tab, select Edit in the Credentials section, then Add
   Credentials, and save a username and password.
4. Put the URL, username, and password into the `broker` block of the HiveMe config
   (see [config.md](config.md)) or into the Broker section of the `hmg` Settings tab.
5. Wait up to one minute for the new credentials to become active, then send a test
   message with `hmc`.
