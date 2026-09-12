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

Filled in by step 2.1 of [the initialization plan](../plans/plan-initialization.md).
It will record the exact client identifier scheme, session settings, TLS
configuration, reconnect policy, and the observed Serverless behaviour.

Planned behaviour, for reference until then:

| Concern | `hmc` | `hmg` |
|---------|-------|-------|
| Client id | `<prefix>-hmc-<8 of device.id>-<8 random>` | `<prefix>-hmg-<8 of device.id>` |
| Clean start | yes | no |
| Session expiry | 0 | `broker.sessionExpirySecs` |
| Reconnect | none, one shot | exponential backoff with jitter |
| Subscriptions | none | `topics.subscriptions` |

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
