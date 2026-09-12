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
`device.id` and the hostname as `device.name`, then reports the path it used. On
Unix the file is created with mode 0600.

## Schema

A complete config with every default spelled out:

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
| `broker.url` | string | yes | none | `mqtts://host:8883` for TLS, `wss://host:8884/mqtt` for WebSocket TLS (phase 6), `mqtt://host:1883` for a plain local broker, which logs a warning. |
| `broker.username` | string | yes | none | HiveMQ Cloud credential username. |
| `broker.password` | string | yes | none | May be empty when `passwordRef` is set. |
| `broker.passwordRef` | object or null | no | null | `{ "type": "Env", "name": "HIVEME_PASSWORD" }` is implemented in phase 1. `{ "type": "Keychain", "service": "HiveMe", "account": "<username>" }` is reserved for phase 6. |
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
| `gui.history.maxMessagesPerTopic` | integer | no | 1000 | Older rows beyond this count are deleted per topic. |
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

## Versioning and compatibility

- `version` is a required integer.
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

## Secrets

- Phase 1 stores `broker.password` in plain text in the config file. The file is
  created with mode 0600 on Unix.
- `HIVEME_PASSWORD` overrides `broker.password` when set.
- Phase 6 adds `passwordRef.type = "Keychain"`, backed by the OS keychain, plus the
  same `*Ref` mechanism for `encryption.keys[].secret` and `cloudApi.token`.
- Logs never print `broker.password`, `encryption.keys[].secret`, or
  `cloudApi.token`. The Settings tab does display the password, because the user
  edits it there.
