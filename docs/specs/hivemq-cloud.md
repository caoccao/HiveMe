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

## Supported plans

**Serverless only, for now.** Everything below that needs a Starter plan or above is
designed and reserved, not implemented: the REST API, role based access, client
certificates, and custom domains. They arrive in a later phase, and the config fields
they need are already in [config.md](config.md) so that adding them will not bump the
config `version`.

What that means in practice:

- Authentication is a username and password in the CONNECT packet. There is no
  anonymous access and no client certificate on this plan, so there is nothing for
  `hmc` or `hmg` to generate or to enroll.
- TLS is server side only, and the certificate chains to a public authority. Neither
  application asks a user anything about certificates.
- The cluster is set up in the `hmg` Settings tab, which also renders the setup string
  `hmc --init` reads. See [config.md](config.md#the-setup-string).

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
- Every MQTT connection needs a unique client identifier, so no two HiveMe processes
  on the same device may share one: `hmc` runs, interactive `hmc`, and `hmg`.

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

The only difference between the applications is the role they connect as.

| Concern | one-shot `hmc` (`Role::Cli`) | `hmg` (`Role::Gui`) | interactive `hmc` (`Role::Tui`) |
|---------|------------------------------|---------------------|---------------------------------|
| Client id | `<prefix>-hmc-<8 of device.id>-<8 random>` | `<prefix>-hmg-<8 of device.id>` | `<prefix>-hmc-<8 of device.id>-<8 random>` |
| Clean start | yes | no | no; the identifier is new, so the only session to resume is this run's own |
| Session expiry | 0 | `broker.sessionExpirySecs` during network interruptions; discarded on explicit disconnect or quit | as `hmg`, for the life of the process |
| Reconnect | none; the first drop ends the connection | exponential backoff with jitter | as `hmg` |
| Subscriptions | none | `topics.subscriptions` | `topics.subscriptions` |

`<prefix>` is `broker.clientIdPrefix`, `hiveme` by default. The device segment is the
first eight alphanumeric characters of `device.id`. Every `hmc` run adds a random
suffix, because a broker disconnects the older of two connections that share an
identifier and `hmc` runs overlap with each other and with a running `hmg`.

`Role::Tui` was added in phase 1 of [the terminal UI plan](../plans/plan-terminal-ui.md)
and is used by the terminal UI of [tui.md](tui.md). It keeps the random
suffix so that several interactive `hmc` processes on one device do not collide, and
takes everything else from the GUI role, clean start included.

Clean start is a flag on the CONNECT packet, and the client sends the same packet every
time it reconnects, so asking for a clean start is asking the broker to throw the
session away on every recovery, along with the messages it held while the network was
down. That is right for one-shot `hmc`, which wants no session at all, and wrong for the
other two. A fresh identifier does not need it: the broker has no session under a name
it has never seen, so the first connection behaves exactly as a clean one, and every
connection after it is a reconnect of this process's own session. Quitting discards that
session through `end_session`, under [Disconnect and quit](#disconnect-and-quit).

### Transport and TLS

| `broker.url` scheme | Default port | Transport |
|---------------------|--------------|-----------|
| `mqtts` | 8883 | MQTT over TLS. The scheme HiveMQ Cloud is reached with. |
| `mqtt` | 1883 | Plain TCP, for a local test broker. Logs a warning, because the password crosses the network in the clear. |
| `wss` | 8884 | MQTT over WebSocket with TLS, path `/mqtt`. Needs the `websocket` feature of `hiveme-core`, which is off by default. |
| `ws` | 8083 | Plain WebSocket, same feature. |

TLS needs no configuration and nothing is generated. The trust store is the operating
system's, loaded with
[rustls-native-certs](https://crates.io/crates/rustls-native-certs), and that is enough
for HiveMQ Cloud on its own, because the cluster certificate chains to a public
authority every OS already trusts. rustls sends the host of `broker.url` as the SNI
name, which HiveMQ Cloud requires. A user never sees a certificate field, in `hmg` or
in the setup string `hmc` reads.

`broker.tls.caFile` and `broker.tls.verifyServer` exist for the case the supported plan
does not cover: a local test broker with a private CA. Neither is needed for, or
applies to, a Serverless cluster. `caFile` adds PEM certificates to the native roots
rather than replacing them, so a config written for a local broker still reaches the
cloud.

`broker.tls.verifyServer = false` skips the check that the certificate belongs to the
host. It is honored only for hosts outside `hivemq.cloud`: on a cloud host the
credentials travel in the CONNECT packet, so an unverified connection would hand them
to whatever answered, and the certificate is verified anyway with a warning in the
log. Turning it off anywhere logs a warning.

The cryptography rustls uses is chosen rather than inferred. rustls picks a provider on
its own only when exactly one is compiled in, and panics rather than guessing when
there are two; the update check of the shared session reaches the GitHub releases API
through `ureq`, which brings its own rustls with `ring` alongside the `aws-lc-rs` that
`rumqttc` is built against. The
client installs aws-lc-rs once per process before it builds any TLS configuration, so
neither application depends on which crates happen to be linked beside it.

### Session settings

* Keep alive is `broker.keepAliveSecs`, 30 seconds by default, and is clamped to the
  five seconds the client accepts as its shortest. Validation reports a smaller value,
  and the clamp is here as well because a caller may skip validation, and because
  `rumqttc` answers a shorter one by asserting: a single line of a config file must not
  be able to take the application down.
* The connect timeout is `broker.connectTimeoutSecs`, 10 seconds by default. It bounds
  both the TCP connection and the wait for the CONNACK.
* `MqttClient::connect` returns only once the broker has answered, so a wrong password
  is reported to the caller rather than retried in the background.
* A `Role::Gui` client retries a transient failure inside the connect timeout, but a
  refusal, a TLS failure, or the timeout running out is reported. Once the first
  connection is up it reconnects on its own for as long as it lives.

### The URL

The console shows a cluster three ways, and none of the three carries a scheme:
`<cluster>.s1.eu.hivemq.cloud` under **MQTT URL**, the same with `:8883` under **TLS
MQTT URL**, and the same with `:8884/mqtt` under **TLS Websocket URL**. Any of the three
is a `broker.url` as it stands, because a URL that names no scheme is read as `mqtts`,
which is the only transport a cluster accepts anyway. A scheme that is written is the
one that is used, so a WebSocket says `wss://` and a local test broker says `mqtt://`.

That rule belongs here rather than in the GUI, so that it holds for a config file
edited by hand and for `hmc --init` as much as for the Settings form.

### Connection state

`State` is `Connecting`, `Connected`, `Reconnecting`, or `Disconnected`, and
`State::as_str` spells them exactly so, because the name travels to `hmg` inside
`Status.state` and the frontend compares it against `ConnectionState` in
`protocol.ts`. It is a name on a wire, not a word on a screen: the status bar looks
each one up in its own translation table, which is what lets the badge read
`connected` in lower case while the value behind it stays `Connected`.

### Reconnect

`broker.reconnect.initialDelayMs` doubles per attempt up to
`broker.reconnect.maxDelayMs`, and half of each delay is randomized. The jitter matters
because several installations share a cluster, and a cluster restart would otherwise
bring them all back at the same instant. A successful connection resets the sequence.

These errors are never retried, because retrying them cannot work: a CONNACK refusal
(the credentials or the client id), a TLS failure, an answer that is not a CONNACK, and
the client handle being dropped.

When a reconnect comes back with no session, which is what the broker reports when the
session expired or the cluster was replaced, the remembered subscriptions are sent
again. A caller therefore subscribes once rather than on every reconnect.

### Disconnect and quit

`disconnect` sends MQTT DISCONNECT and waits for the event loop to stop, for at most
five seconds or `broker.connectTimeoutSecs`, whichever is shorter. A timeout stops
reconnection and fails pending requests instead of leaving background work running.
The CLI uses a zero-expiry session, so this also releases its session.

The GUI, and interactive `hmc`, use
`end_session`, through the shared session, on quit, explicit disconnect, and
connection replacement. Quit means every way the application ends that it can act
on: closing the `hmg` window, and for the terminal UI the Quit tool, its keys,
closing the terminal, and `SIGTERM`, as [tui.md](tui.md#leaving-the-terminal-ui)
lists.
It closes the live connection and discards the broker's subscriptions and queued
session messages. Retained topic messages and local history are unaffected. The
configured session expiry still applies to unexpected network interruptions.

The pinned rumqttc 0.25 client cannot attach session-expiry properties to DISCONNECT.
The shared core therefore follows the disconnect with a brief clean-start connection
using the same client identity and zero session expiry, then sends DISCONNECT on that
connection. It makes no subscriptions or publishes. The entire cleanup is bounded
by five seconds. If the broker cannot be reached, cleanup reports an error and stops
local work; broker-side state then expires according to its configured interval.

### Publishing and subscribing

* `publish` returns once the broker has acknowledged: nothing to wait for at QoS 0, the
  PUBACK at QoS 1, the PUBCOMP at QoS 2. It gives up after `publish.timeoutSecs`. This
  is what lets `hmc` exit knowing the message landed.
* A caller cannot know which acknowledgement is theirs, because `rumqttc` picks the
  packet identifier inside the event loop and reports it afterward, as an outgoing event
  in the order the requests were sent. Callers are therefore queued and sent under one
  lock and paired with those events in turn. Two things would break that pairing, and
  both are handled where the CONNACK is read, so that no caller is ever handed the
  acknowledgement of somebody else's message:
  * a reconnect the broker resumed the session for makes the client send every
    unacknowledged publish again under the identifier it already has, which is not a new
    request and must not take the next caller's place in the queue, and
  * a reconnect that comes back without the session makes the client drop everything it
    was holding, so every caller still waiting is waiting for a packet that will never
    be sent and is told so rather than left until its timeout.
  An identifier is also free again as soon as the broker has answered the message that
  held it, which at QoS 2 is one packet before the caller is done with it, so a caller
  whose identifier is handed on is told rather than left parked under it.
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

### Logging

Everything goes through the `log` crate, so the host application decides where it
lands. A connection that ends is logged as a warning only for a role that reconnects,
because that log line is then the only record; for `hmc` the same reason is handed back
as an error and printing it twice would be noise.

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
* There is no way to raise the connection limit, so the client identifiers of the
  applications, and of their concurrent processes, must differ; they do, by
  construction.

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

The [HiveMe README](../../README.md#quick-start) walks a new user through this with
what to do when a step does not work. The HiveMQ documentation for each step:

1. Create an account at [hivemq.com](https://www.hivemq.com/) and create a **Serverless**
   cluster. See the
   [quick start guide](https://docs.hivemq.com/hivemq-cloud/quick-start-guide.html).
2. On the cluster **Overview** tab, copy the hostname. The TLS MQTT port is 8883, and
   the URL HiveMe wants is `mqtts://<hostname>:8883`. See
   [the console](https://docs.hivemq.com/hivemq-cloud/console.html).
3. On the **Access Management** tab, select **Edit** in the Credentials section, then
   **Add Credentials**, and save a username and password. On Serverless the permissions
   are attached to the credential and default to publish and subscribe on `#`, which is
   what HiveMe needs. See
   [authentication and authorization](https://docs.hivemq.com/hivemq-cloud/authn-authz.html).
4. Put the URL, the username, and the password into the Broker section of the `hmg`
   Settings tab, and save. Editing the `broker` block of the config file by hand does
   the same thing; see [config.md](config.md).
5. Press **Copy CLI setup** and run `hmc --init '<paste>'`, which is all `hmc` needs.
   See [config.md](config.md#the-setup-string).
6. Credentials take up to a minute to become active, so a refusal straight afterward
   usually means waiting rather than a wrong password. Then send a test message with
   `hmc` and watch it arrive in `hmg`.
