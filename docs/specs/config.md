# Config

`hmc` and `hmg` share one JSON config file. This document is the reference for its
location, its fields, and its compatibility rules.

The machine readable schema is [`schemas/config.schema.json`](../../schemas/config.schema.json),
generated from the `hiveme-core::config` types with `cargo xtask schema`. The schema,
not this document, is what the code validates against; this document explains it.
The example below is extracted and validated by `cargo xtask check-spec`.

JSON keys are camelCase. Enum values that exist only inside the config are
PascalCase (`Auto`, `Ocean`, `Weekly`, `Off`). Values that also travel inside
messages keep their wire casing (`level`, `alg`, `kid`).

## Location and precedence

The config path is resolved in this order:

1. `--config <path>`, accepted by both applications.
2. The `HIVEME_CONFIG` environment variable.
3. The per-OS location below.

| OS | Path |
|----|------|
| Linux | `$XDG_CONFIG_HOME/HiveMe/HiveMe.json` when `XDG_CONFIG_HOME` is set, otherwise `$HOME/.config/HiveMe/HiveMe.json` |
| macOS | `$HOME/Library/Application Support/HiveMe/HiveMe.json` |
| Windows, installed | `%APPDATA%\HiveMe\HiveMe.json` when the executable lives under `%LOCALAPPDATA%`, `%ProgramFiles%`, or `%ProgramFiles(x86)%` |
| Windows, portable | next to the executable |

The SQLite database `HiveMe.db` sits in the same directory as the config file. The
directory is created on first launch.

On first run either application writes a default config with a freshly generated
`device.id` and the hostname as `device.name`, then reports the path it used. A config
that exists but has no `device.id` is given one and written back, so that a hand
written file still gets an identity.

Writes go through a temporary file in the same directory and are then renamed, so an
interrupted write cannot leave a half finished config. On Unix the file is created with
mode 0600, because it holds a password in plain text.

**Loading never validates.** A config is read as far as it can be, so that `hmg` can
open an incomplete file and let the user finish it in the Settings tab. Both
applications call validation separately, before they connect. See
[Validation](#validation).

## Schema

A complete config, with every optional key written out. The `device` and `broker`
values are per installation and have no useful default; everything else shows the
value the applications use when the key is absent.

```json hiveme:config
{
  "version": 1,
  "device": {
    "id": "0f7a1c2e-5d4b-4a6e-9c1d-2b3e4f5a6b7c",
    "name": "sams-macbook"
  },
  "broker": {
    "url": "mqtts://abc123def456.s1.eu.hivemq.cloud:8883",
    "username": "hiveme-sam",
    "password": "change-me",
    "passwordRef": null,
    "clientIdPrefix": "hiveme",
    "keepAliveSecs": 30,
    "sessionExpirySecs": 3600,
    "connectTimeoutSecs": 10,
    "tls": {
      "verifyServer": true,
      "caFile": null
    },
    "reconnect": {
      "initialDelayMs": 1000,
      "maxDelayMs": 30000
    }
  },
  "topics": {
    "prefix": "hiveme",
    "default": "info",
    "subscriptions": ["#"]
  },
  "publish": {
    "qos": 1,
    "retain": false,
    "timeoutSecs": 10
  },
  "notifications": {
    "enabled": true,
    "notifyOwnMessages": false,
    "rules": [
      { "id": "info",  "topic": "info",  "level": "info",  "enabled": true, "title": "{title|topic}", "body": "{body}" },
      { "id": "warn",  "topic": "warn",  "level": "warn",  "enabled": true, "title": "{title|topic}", "body": "{body}" },
      { "id": "error", "topic": "error", "level": "error", "enabled": true, "title": "{title|topic}", "body": "{body}" }
    ]
  },
  "gui": {
    "displayMode": "Auto",
    "theme": "Ocean",
    "language": "en-US",
    "history": {
      "maxMessagesPerTopic": 1000,
      "retentionDays": 30
    },
    "window": {
      "position": { "x": -1, "y": -1 },
      "size": { "width": 1200, "height": 900 }
    }
  },
  "update": {
    "checkInterval": "Weekly",
    "lastChecked": 0,
    "lastVersion": "",
    "ignoreVersion": ""
  },
  "encryption": {
    "mode": "Off",
    "keys": []
  },
  "cloudApi": null
}
```

## Field reference

| Field | Type | Required | Default | Notes |
|-------|------|----------|---------|-------|
| `version` | integer | yes | 1 | Config schema version. The loader migrates older versions. |
| `device.id` | UUID string | yes | generated | Stable identity of this installation. Used as `sender.id` and to derive client identifiers. |
| `device.name` | string | no | hostname | Shown in the GUI as the sender name. |
| `broker.url` | string | yes | none | `<scheme>://<host>[:<port>][/<path>]`. The schemes are `mqtts` (port 8883), `mqtt` (1883), `wss` (8884, path `/mqtt`), and `ws` (8083); `ssl` and `tcp` are accepted as aliases of `mqtts` and `mqtt`. The port defaults per scheme. Credentials in the URL are rejected: they belong in the fields below. |
| `broker.username` | string | yes | none | HiveMQ Cloud credential username. |
| `broker.password` | string | yes | none | May be empty when `passwordRef` is set. |
| `broker.passwordRef` | object or null | no | null | `{ "type": "Env", "name": "<VARIABLE>" }` is implemented. `{ "type": "Keychain", "service": "HiveMe", "account": "<username>" }` is reserved for phase 6 and reports that it is not implemented rather than failing silently. |
| `broker.clientIdPrefix` | string | no | `hiveme` | Client id is `<prefix>-<app>-<first 8 hex of device.id>` plus a random suffix for `hmc`. |
| `broker.keepAliveSecs` | integer | no | 30 | MQTT keep alive. |
| `broker.sessionExpirySecs` | integer | no | 3600 | `hmg` only. `hmc` always uses 0. |
| `broker.connectTimeoutSecs` | integer | no | 10 | |
| `broker.tls.verifyServer` | boolean | no | true | `false` is honoured only for hosts outside `hivemq.cloud` and logs a warning. |
| `broker.tls.caFile` | path or null | no | null | Extra PEM roots appended to the native trust store. |
| `broker.reconnect.initialDelayMs` | integer | no | 1000 | |
| `broker.reconnect.maxDelayMs` | integer | no | 30000 | Exponential backoff with jitter, `hmg` only. |
| `topics.prefix` | string | no | `hiveme` | May be empty, in which case relative topics are absolute. No leading or trailing `/`, no wildcards. |
| `topics.default` | string | no | `info` | Relative to the prefix. Used by `hmc` when `--topic` is absent. |
| `topics.subscriptions` | (string or object)[] | no | `["#"]` | Filters relative to the prefix. A filter starting with `$`, or written as `{ "filter": "...", "absolute": true }`, is used verbatim. |
| `publish.qos` | 0, 1, 2 | no | 1 | |
| `publish.retain` | boolean | no | false | |
| `publish.timeoutSecs` | integer | no | 10 | How long `hmc` waits for the acknowledgement. |
| `notifications.enabled` | boolean | no | true | Master switch. |
| `notifications.notifyOwnMessages` | boolean | no | false | When false, messages whose `sender.id` equals `device.id` never notify. |
| `notifications.rules[]` | object[] | no | the three built-ins | See [gui.md](gui.md#notifications). |
| `gui.displayMode` | `Auto`, `Light`, `Dark` | no | `Auto` | `Auto` follows `prefers-color-scheme`. |
| `gui.theme` | theme name | no | `Ocean` | One of the twenty palette names listed in [gui.md](gui.md#theme). |
| `gui.language` | BCP 47 tag | no | detected | Only `en-US` ships in phase 1. |
| `gui.history.maxMessagesPerTopic` | integer | no | 1000 | Older rows beyond this count are deleted per topic. 0 keeps everything. |
| `gui.history.retentionDays` | integer | no | 30 | 0 disables time based pruning. |
| `gui.window.position` | `{ x, y }` | no | `-1, -1` | Negative means "center the window". |
| `gui.window.size` | `{ width, height }` | no | `1200 x 900` | Minimum 600 x 450. |
| `update.checkInterval` | `Daily`, `Weekly`, `Monthly` | no | `Weekly` | |
| `update.lastChecked` | integer | no | 0 | Unix seconds. |
| `update.lastVersion` | string | no | `""` | Latest version seen on GitHub. |
| `update.ignoreVersion` | string | no | `""` | Version the user chose to skip. |
| `encryption.mode` | `Off`, `Opportunistic`, `Required` | no | `Off` | Only `Off` is implemented. See [message.md](message.md#encryption). |
| `encryption.keys[]` | object[] | no | `[]` | `{ "kid", "alg", "secret", "createdAt", "state" }`. |
| `cloudApi` | object or null | no | null | `{ "baseUrl", "orgId", "clusterId", "token", "tokenRef" }`, phase 6. See [hivemq-cloud.md](hivemq-cloud.md#rest-api). |

## Topic resolution

- A relative topic `t` resolves to `<prefix>/t`, or to `t` when the prefix is empty.
- `hmc --topic` is relative. `hmc --absolute-topic` (`-T`) makes it verbatim.
- Topics are validated before publishing: non-empty, no wildcard characters, no NUL
  byte, at most 65535 bytes of UTF-8.
- Topic filters in `topics.subscriptions` and in notification rules are relative
  unless marked absolute, and may use the MQTT wildcards `+` and `#`.
- Filter matching follows the MQTT 5 specification, including the rule that a
  wildcard filter does not match a topic starting with `$`.

## Validation

Validation is separate from loading. Both applications call it before they connect:
`hmc` reports the problems and exits 3, `hmg` shows them in the snackbar and leaves the
Settings tab open. Every problem is reported at once rather than one at a time.

| Rule | Message mentions |
|------|------------------|
| `device.id` is not empty | `device.id` |
| `broker.url` parses, and uses a known scheme with a host | `broker.url` |
| A `hivemq.cloud` host uses `mqtts` or `wss`, because the service accepts TLS only | `TLS only` |
| `broker.username` is not empty | `broker.username` |
| A password is reachable: the field, `passwordRef`, or `HIVEME_PASSWORD` | `broker.password` |
| `broker.reconnect.initialDelayMs` is not greater than `maxDelayMs` | `initialDelayMs` |
| `topics.prefix` has no leading or trailing `/` and no wildcard | `topics.prefix` |
| `topics.default` is a publishable topic, so no wildcard | `topics.default` |
| `topics.subscriptions` is not empty, and every filter is well formed | `topics.subscriptions` |
| `publish.qos` is 0, 1, or 2 | `publish.qos` |
| Every notification rule has a non-empty id, and no two share one | `empty id`, `share the id` |
| Every notification rule topic is a well formed filter | `rules['<id>'].topic` |
| No two encryption keys share a `kid` | `share the kid` |
| When `encryption.mode` is not `Off`, exactly one key is `Active` | `Active` |
| `cloudApi.baseUrl`, when set, is HTTPS | `cloudApi.baseUrl` |
| `cloudApi.orgId`, when set, is six alphanumeric characters | `cloudApi.orgId` |

A filter is well formed when it is not empty, `#` is the last level and takes up a whole
level, `+` takes up a whole level, and there are no control characters. A topic is
publishable when it is not empty, carries no wildcard, and is at most 65535 bytes of
UTF-8.

## Versioning and compatibility

- `version` is an integer, and every writer emits it. A document without one is read
  as the current version and logs a warning, so that a hand written file still loads.
- A value this build does not know in a closed enum, such as a `gui.theme` added by a
  later release, falls back to that field's default with a warning rather than making
  the whole file unreadable.
- Unknown fields are ignored on read and **preserved on write**: a write merges the
  typed structure into the `serde_json::Value` that was read, so a newer `hmc` and an
  older `hmg` can share one file without losing each other's settings. This is a
  deliberate departure from the reference project, which serialises the struct
  directly.
- Adding an optional field with a default does not bump `version`.
- Renaming a field or changing its meaning bumps `version` and adds a
  `migrate_vN_to_vN+1` function plus a fixture pair under
  `crates/hiveme-core/tests/fixtures/config/`.
- A config whose `version` is newer than the binary understands is loaded on a best
  effort basis with a warning, and is never rewritten.
- The schema carries `$id` `https://hiveme.dev/schemas/config/v1.json`. That URL is an
  identifier, not a location.

## The setup string

A cluster is set up in `hmg`, which is where a user has the HiveMQ Cloud console open
and can paste a URL and credentials into fields. `hmc` has no such place, so the
Settings tab renders the same three values as one line of JSON, the user copies it, and
`hmc --init '<json>'` turns it back into a config.

```json hiveme:broker-init
{
  "v": 1,
  "url": "mqtts://abc123.s1.eu.hivemq.cloud:8883",
  "username": "hiveme-sam",
  "password": "s3cret",
  "prefix": "hiveme"
}
```

`hmg` writes it on one line; the example is indented only to be read here.

| Field | Config path | Notes |
|-------|-------------|-------|
| `v` | — | The format version, 1. A reader refuses a version it does not know rather than guessing at fields. |
| `url` | `broker.url` | Must be `mqtts` for a `hivemq.cloud` host, which accepts TLS only. |
| `username` | `broker.username` | Required. |
| `password` | `broker.password` | Required, plain text, because the CONNECT packet needs it in plain text. |
| `prefix` | `topics.prefix` | Optional. Carried so that `hmc` publishes where `hmg` is listening. Absent leaves the receiving config's prefix alone. |

The schema is generated from the Rust type into `schemas/broker-init.schema.json`, and
the example above is validated against it by `cargo xtask check-spec`.

Applying a string touches only those fields. A `hmc` that has been in use keeps its
`device.id`, its rules, and any key a newer build wrote, because the config writer
merges into the document it read. Reading one tolerates surrounding whitespace and a
single pair of wrapping quotes, since the string crosses a clipboard and a shell.

The string is a credential. It carries the broker password in plain text, so it should
be pasted rather than committed, and `.config/` is gitignored for exactly that reason.
See [cli.md](cli.md#setting-up) and [development.md](../development.md#testing-against-a-broker).

## Secrets

- Phase 1 stores `broker.password` in plain text in the config file. The file is
  created with mode 0600 on Unix.
- The password sent in the CONNECT packet is resolved in this order, at the moment of
  connecting rather than at load time, so that an override is never written back into
  the file:
  1. the `HIVEME_PASSWORD` environment variable, when it is set and not empty,
  2. `broker.passwordRef`, when it is set,
  3. `broker.password`.
- Phase 6 adds `passwordRef.type = "Keychain"`, backed by the OS keychain, plus the
  same `*Ref` mechanism for `encryption.keys[].secret` and `cloudApi.token`.
- Logs never print `broker.password`, `encryption.keys[].secret`, or
  `cloudApi.token`. The Settings tab does display the password, because the user
  edits it there, and so does the setup string, because `hmc` needs it.
- A setup string is redacted before it reaches a log, the same way a config is.
