# HiveMe Initialization Plan

Status: proposed (2026-09-12)
Inputs: `docs/specs/app.md`, HiveMQ Cloud docs, HiveMQ Cloud Public REST API (OpenAPI 3.0.3, `public-saas-openapi.yml`), and the sibling project `../BetterMediaInfo`, whose UI layout and Tauri architecture HiveMe mirrors.
Output: a Cargo workspace with `hmc` (CLI), `hmg` (Tauri + React GUI), a shared `hiveme-core` crate, machine-checked JSON schemas, and specs that tests keep in sync with the code.

---

## 1. Decisions

Answers gathered before writing this plan. They are binding for the phases below unless a later plan revises them.

| # | Question | Decision |
|---|----------|----------|
| 1 | HiveMQ Cloud plan and role of the REST API | Serverless cluster now. Core app is MQTT-only. The REST client is designed (config block, endpoints, GUI panel) but implemented in a later phase, gated on an API token being configured. |
| 2 | Config location and password storage | OS config directory, same resolution rules as BetterMediaInfo (section 4.1). Overridable with `--config` and `HIVEME_CONFIG`. Password stored in plaintext in the file (mode 0600 on Unix). Schema reserves `passwordRef` for keychain storage later. |
| 3 | Producers and GUI strictness | Producers are HiveMe tools plus the user's own scripts. GUI is lenient: it parses the HiveMe envelope when valid and otherwise shows the raw payload. |
| 4 | Encryption key model (spec only) | Symmetric pre-shared key. AES-256-GCM with HKDF-derived keys, key id in the envelope for rotation. Not implemented in this plan. |
| 5 | Profiles | Single `broker` object. Config is versioned so a `profiles` map can be added later without breaking existing files. |
| 6 | Topic layout | Fixed root `hiveme`, with no prefix or default-topic setting. Publish inputs are relative and leading slashes are stripped. hmc resolves under `hiveme`; hmg resolves under the selected topic. Built-in rules match `hiveme/#` and filter by payload level. GUI subscribes to `hiveme/#` by default, additional filters configurable. |
| 7 | Message history | Persisted in a local SQLite database next to the config file. Bounded per topic and by retention days. |
| 8 | Notification rules | Configurable `notifications.rules` array with the three built-in rules as defaults. |
| 9 | Schema source of truth | Rust types (`serde` + `schemars`) in `hiveme-core` generate `schemas/config.schema.json` and `schemas/message.schema.json`. Tests fail if committed schemas are stale. Spec examples are validated in tests. TypeScript types for config and message are generated from the schemas. |
| 10 | Spec files | Split by concern: `app.md` (overview), `config.md`, `message.md`, `cli.md`, `gui.md`, `hivemq-cloud.md`. |
| 11 | Frontend stack | Vite + React 19 + TypeScript + MUI (Material UI) with `@mui/x-tree-view`, Zustand, react-i18next, pnpm. Same stack as BetterMediaInfo plus the tree view. |
| 12 | Tests and CI | Integration tests run against a local HiveMQ CE container via Docker (skipped when Docker is absent). GitHub Actions build workflows per OS, mirroring BetterMediaInfo's `linux_build.yml`, `macos_build.yml`, `windows_build.yml`. |
| 13 | CLI scope | `hmc <message>`, `hmc -t <topic> <message>`, read from stdin when no message argument, `--json` for a raw JSON payload. No `sub` or `config` subcommands in phase 1. |
| 14 | MQTT version | MQTT 5 via `rumqttc` v5 client. |
| 15 | GUI publishing | The message view is a chat view (WhatsApp-like). Messages for the selected topic are shown as bubbles, with an input box and a send button at the bottom. Sending publishes to the selected topic. |
| 16 | Payload fields | `title` (optional), `body`, `level` (`debug`, `info`, `success`, `warn`, `error`), `sender` in the envelope, plus a free-form `data` object. |
| 17 | Reference architecture | UI layout and Tauri app architecture follow `../BetterMediaInfo`: Tauri app at the repository root (`src/`, `src-tauri/`), thin `#[tauri::command]` wrappers in `lib.rs` delegating to `controller.rs`, `protocol.rs`/`protocol.ts` pair, `config.rs` with camelCase keys, `window.rs` for window state, Zustand store plus `service.ts` invoke layer, tabbed main content with Settings and About tabs, react-i18next, per-OS build workflows, Deno version-bump script, Apache-2.0 license headers, rustfmt `max_width = 120`, `tab_spaces = 2`. Section 3 lists the concrete mapping. |

---

## 2. Research: HiveMQ Cloud facts that shape the design

Sources: <https://docs.hivemq.com/hivemq-cloud/index.html>, `quick-start-guide.html`, `authn-authz.html`, `console.html`, `rest-api.html`, `metrics.html`, and the OpenAPI file at <https://docs.hivemq.com/hivemq-cloud/rest-api/specification/public-saas-openapi.yml>.

### 2.1 Connectivity

- HiveMQ Cloud accepts TLS connections only. MQTT over TLS on port 8883, MQTT over WebSocket TLS on port 8884 (path `/mqtt`).
- Cluster hostnames look like `<id>.s1.eu.hivemq.cloud` (Serverless) or a custom domain (Starter and above). TLS SNI is required, so the client must send the hostname during the handshake.
- The server certificate chain is issued by a public CA (Let's Encrypt). Operating-system root stores already trust it, so the client uses native roots by default and allows an optional custom CA file.
- MQTT 3.1, 3.1.1, and 5.0 are supported.
- Serverless: 100 concurrent connections, 10 GB traffic per month, shared infrastructure, no SLA. Each MQTT connection must have a unique client id, so `hmc` and `hmg` on the same device must use different client ids.

### 2.2 Authentication and authorization

- Clients authenticate with username and password in the CONNECT packet. Credentials are created in the Cloud Console under Access Management.
- Serverless: permissions are attached directly to the credential. The default permission set is "Publish and Subscribe" on `#`.
- Starter and above: role-based access with custom permissions (topic filter, publish/subscribe, QoS, retain, shared subscriptions, dynamic variables like `${mqtt-username}`), client certificates, and JWT auth.
- Authorization is whitelist-only: anything not explicitly allowed is denied. Credential changes take up to one minute to apply.

### 2.3 REST API

- Available on the Starter plan and above only. Not usable on Serverless.
- Base URL is region-specific and shown on the cluster's API Access tab, for example `https://api.a01.euc1.aws.hivemq.cloud`. All paths are under `/api/v2/orgs/{orgId}/clusters/{clusterId}/`. `orgId` is a 6-character alphanumeric string, `clusterId` a UUID.
- Auth: `Authorization: Bearer <JWT>` with an API token created in the console (name, lifetime, Full Access or custom scope). The token is shown once.
- Pagination: cursor based. `limit` between 50 and 2500 (default 500). Responses carry `_links.next` with the URL of the next page; HTTP 410 means the cursor expired.
- Error body: `{ "errors": [ { "title": "...", "detail": "..." } ] }`.

Endpoints (all relative to the base path above):

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/mqtt/clients` | List all client sessions (paginated) |
| GET | `/a/mqtt/clients/search` | Filter clients by `boolean-filter`, `string-filter`, `number-filter` |
| GET | `/mqtt/clients/{clientId}` | Client details: connection, TLS info, queue, session expiry |
| DELETE | `/mqtt/clients/{clientId}` | Invalidate a session (disconnects if online) |
| GET | `/mqtt/clients/{clientId}/connection` | Connection state only |
| DELETE | `/mqtt/clients/{clientId}/connection` | Disconnect a client |
| GET | `/mqtt/clients/{clientId}/subscriptions` | List a client's subscriptions |
| GET | `/metrics` | Broker metrics as `text/plain` (Prometheus-style) |
| GET, POST | `/mqtt/credentials` | List (paginated) or create credentials |
| GET, DELETE | `/mqtt/credentials/username/{username}` | Get or delete credentials |
| GET, POST, PUT | `/mqtt/permissions` | List, create, or replace all permissions |
| PUT, DELETE | `/mqtt/permissions/{id}` | Update or delete a permission |
| GET, POST | `/mqtt/roles` | List or create roles |
| PUT, DELETE | `/mqtt/roles/{roleId}` | Update or delete a role |
| GET | `/mqtt/roles/permissions` | All role-permission links |
| GET | `/mqtt/roles/{roleIdOrName}/permissions` | Permissions of one role |
| PUT | `/mqtt/roles/{roleId}/permissions/{permissionId}/attach` or `/detach` | Link or unlink permission and role |
| GET | `/user/{username}/roles` | Roles of a credential |
| PUT | `/user/{username}/roles/{roleId}/attach` or `/detach` | Link or unlink role and credential |

Key schemas: `Credentials { username, password }`, `UserInfo { username, roleRefs[] }`, `MQTTPermission { id, name, description, topic, publishAllowed, subscribeAllowed, qos0Allowed, qos1Allowed, qos2Allowed, retainedMsgsAllowed, sharedSubAllowed, sharedGroup, roles[], applyTo, variables[] }`, `RoleInfo { id, name, description }`, `ClientDetails { id, connected, connectedAt, sessionExpiryInterval, messageQueueSize, willPresent, restrictions, connection }`, `ClientSubscription { topicFilter, qos, retainHandling, retainAsPublished, noLocal, subscriptionIdentifier }`.

### 2.4 Consequences for the design

1. There is no REST endpoint to publish or to discover topics. Publishing is MQTT-only, and the GUI topic tree is built from messages received on its subscriptions.
2. The REST API is optional for HiveMe. Its natural uses are a "Cluster" tab in the GUI (connected clients, their subscriptions, broker metrics) and, later, credential management. It is designed in `docs/specs/hivemq-cloud.md` and in the `cloudApi` config block but implemented after the MQTT features.
3. TLS is mandatory for the cloud. Plain `mqtt://` is still allowed in config for local test brokers and produces a warning.
4. Two apps on one device need distinct client ids: `hiveme-hmg-<device-short-id>` for the GUI (persistent session) and `hiveme-hmc-<device-short-id>-<random>` for each CLI run (clean start).

---

## 3. Architecture

### 3.1 Mapping to BetterMediaInfo

| BetterMediaInfo | HiveMe | Notes |
|-----------------|--------|-------|
| `src-tauri/` single Cargo package | `src-tauri/` package `hmg` inside a root Cargo workspace | Workspace is needed because `hmc` and `hiveme-core` are separate crates. `cargo tauri` works unchanged; the target dir moves to the repo root `target/`. |
| `src-tauri/src/lib.rs`: alphabetized `#[tauri::command]` wrappers, `convert_error`, `run()` with a tokio runtime handed to Tauri | Same | Commands delegate to `controller.rs`; logging via `log` + `env_logger` (`RUST_LOG`). |
| `controller.rs` business logic | `controller.rs` orchestration only | Business logic lives in `hiveme-core` so `hmc` shares it. |
| `protocol.rs` / `src/lib/protocol.ts` hand-synced | Same for IPC-only types (status, events, tree nodes) | Config and message types are generated from the JSON schemas instead of hand-written (decision 9) and re-exported from `protocol.ts`. |
| `config.rs`: `Config` with `#[serde(default)]`, camelCase keys, `OnceLock<RwLock<Config>>`, `get_config`/`set_config`, `<App>.json` in the per-OS config dir | Same wrapper in `src-tauri/src/config.rs` over `hiveme_core::config` | File is `HiveMe/HiveMe.json`; `hmc` uses the same resolution code from the core crate. |
| `window.rs`: `setup` (title with version, restore size/position, show), `on_window_event` (persist size/position) | Same | Window state lives in `gui.window` of the config. |
| `constants.rs` `APP_NAME` | Same, `APP_NAME = "HiveMe"` | |
| Plugins: dialog, clipboard-manager, opener | dialog, clipboard-manager, opener, notification | Notification plugin drives OS notifications. |
| `src/App.tsx`: MUI `ThemeProvider`, `displayMode` Auto/Light/Dark, 20 named color themes, compact component defaults | Same, palette table copied | |
| `Layout.tsx` grid `auto 1fr auto`: Toolbar, MainContent, Footer | Same | Footer renders the status bar required by the spec; copyright moves to the About tab. |
| `MainContent.tsx`: tabs with `ControlStatus` Hidden/Selected/Visible for Settings and About, Ctrl+1..9, Ctrl+W, Ctrl+Tab | Same | Tab 0 is the fixed "Messages" tab holding the tree and chat split. |
| `Toolbar.tsx`: `ButtonGroup` of `IconButton`s with tooltips and shortcuts (F10 settings) | Same | Buttons listed in section 8. |
| `NotificationSnackbar.tsx` driven by store `dialogNotification` | Same | In-app errors and confirmations. |
| `lib/store.tsx` Zustand `useAppStore`, `lib/service.ts` invoke wrappers, `lib/constants.ts`, `lib/format.ts` | Same | Components never call Tauri APIs directly. |
| `src/i18n` with react-i18next and 9 locales | Same structure, `en-US` only in phase 1 | Other locales are a later phase; `gui.language` is detected from the system like BetterMediaInfo. |
| `Config.tsx` settings tab with `SectionHeader` sections | `Config.tsx` with Broker, Topics, Notifications, Appearance, Update sections | |
| `About.tsx` | `About.tsx` with app version, links, license | |
| Update check against GitHub releases, `update` config block | Same, repository `caoccao/HiveMe` | Phase 5. |
| `.github/workflows/{linux,macos,windows}_build.yml`, artifacts per OS, `cargo test -r` before build | Same three workflows plus lint and schema steps | `hmc` binaries are uploaded alongside the bundles. |
| `scripts/ts/change-version.ts` (Deno) | Same, adapted file list | Workspace version inheritance keeps the list short. |
| Docs: `README.md` with badges, `docs/{installation,development,release_notes,screenshots,todos}.md` | Same set plus `docs/specs/` and `docs/plans/` | |
| Apache-2.0 header on every source file, rustfmt `max_width = 120`, `tab_spaces = 2`, edition 2024, pinned `rust-toolchain.toml` | Same | `CLAUDE.md` and `AGENTS.md` carry identical content. |

### 3.2 Repository layout

```
HiveMe/
  Cargo.toml                      # workspace: src-tauri, crates/hiveme-core, crates/hmc, xtask; [workspace.package] version
  rust-toolchain.toml             # channel pinned (1.96.0 at bootstrap), clippy + rustfmt, minimal profile
  .rustfmt.toml                   # max_width = 120, tab_spaces = 2
  package.json, pnpm-workspace.yaml, pnpm-lock.yaml
  index.html, vite.config.js, tsconfig.json
  public/                         # favicon, images used by About
  src/                            # React frontend of hmg
    main.tsx, App.tsx
    components/                   # Layout, Toolbar, MainContent, Footer, NotificationSnackbar,
                                  # Messages, TopicTree, MessageView, Composer, Config, About
    lib/                          # store.tsx, service.ts, protocol.ts, constants.ts, format.ts, types.ts, message.ts
    generated/                    # config.ts, message.ts generated from schemas/ (committed)
    i18n/                         # index.ts, locales/en-US.json
  src-tauri/                      # Tauri 2 app, package `hmg`, lib `hmg_lib`, binary `hmg`
    Cargo.toml, tauri.conf.json, build.rs, capabilities/default.json, icons/
    src/                          # main.rs, lib.rs, controller.rs, protocol.rs, config.rs, constants.rs,
                                  # window.rs, mqtt.rs, notification.rs, storage.rs, update.rs
  crates/
    hiveme-core/                  # config, message, topic, rules, mqtt, storage (feature), cloud (feature, later)
    hmc/                          # CLI binary `hmc`
  xtask/                          # `cargo xtask schema`, `cargo xtask check-spec`
  schemas/                        # config.schema.json, message.schema.json (generated, committed)
  scripts/
    ts/                           # Deno scripts: change-version.ts, check-license-headers.ts,
                                  # check-spec-sync.ts, gen-types.ts
  docs/
    specs/                        # app.md, config.md, message.md, cli.md, gui.md, hivemq-cloud.md
    plans/                        # this file
    installation.md, development.md, release_notes.md, screenshots.md, todos.md
  .github/workflows/              # linux_build.yml, macos_build.yml, windows_build.yml
  CLAUDE.md, AGENTS.md, README.md, LICENSE
```

### 3.3 Crates and responsibilities

`hiveme-core` (library, `thiserror` errors)

- `config`: types with `#[serde(default, rename_all = "camelCase")]` and `schemars` derives; path resolution (4.1); load, validate, migrate; write that merges into the original `serde_json::Value` so unknown keys survive; redaction helper.
- `message`: envelope and payload types, lenient parser with three tiers (envelope, raw JSON, raw text), encoder, MQTT 5 property helpers, schema derivation.
- `topic`: fixed-root relative resolution, topic validation, MQTT filter matching (`+`, `#`).
- `mqtt`: async client over `rumqttc` v5 with rustls and native roots. Connect, publish with acknowledgement, subscribe, reconnect with backoff, connection state watch channel.
- `rules`: notification rule matching and title/body templating.
- `storage` (feature `storage`): SQLite via `rusqlite` (bundled). Message history and topic index. GUI only.
- `cloud` (feature `cloud-api`, later phase): typed REST client for the endpoints in section 2.3.
- `crypto` (later phase): AEAD behind the `enc` fields. Only types in this plan.

`hmc` (binary, `clap`, `anyhow`)

- Loads config, builds the envelope, publishes, waits for the acknowledgement, exits with a documented code.

`hmg` (`src-tauri`, `anyhow`)

- `lib.rs`: command wrappers and `run()`. `controller.rs`: orchestration. `mqtt.rs`: owns the core client task, forwards incoming messages to storage, rules, and frontend events. `notification.rs`: plugin calls and rate limiting. `storage.rs`: opens the database and exposes queries. `update.rs`: GitHub release check. `window.rs`, `config.rs`, `constants.rs`, `protocol.rs` as in BetterMediaInfo.

### 3.4 Runtime flows

`hmc "Build finished"`

1. Resolve the config path (`--config`, then `HIVEME_CONFIG`, then the per-OS location). If the file is missing, write a default config with a fresh `device.id` and exit with code 3 and a message telling the user to edit the broker section.
2. Build the envelope: `v: 1`, UUID v7 `id`, RFC 3339 `ts`, `sender` from `device`, `payload.body` from the argument or stdin, `payload.level` from `--level` (default `info`, independently of the topic).
3. Resolve the topic with the shared Rust function: `hiveme` without `--topic`, otherwise strip leading slashes and append the relative input under `hiveme`.
4. Connect (clean start, session expiry 0, TLS with native roots), publish QoS 1 with MQTT 5 properties, wait for PUBACK, disconnect. Exit 0.

`hmg`

1. `run()` initializes logging, the tokio runtime, managed state (`AppState { config, client handle, store, rules, notification limiter }`), plugins, `window::setup`, and the command handler.
2. `setup` restores the window, opens SQLite, connects with a persistent session, subscribes to every filter in `topics.subscriptions`, and starts the update check when due.
3. Each incoming message is parsed leniently, stored, added to the topic tree, pushed to the frontend as a `message` event, and evaluated against notification rules. Matching messages not sent by this device raise an OS notification.
4. Sending from the composer calls the `publish` command, which uses the same core publish path as `hmc`. The message appears immediately as an outgoing bubble and is reconciled with the broker echo by `id`.
5. Saving settings calls `set_config`; if broker or subscription fields changed, the backend reconnects and emits a `status` event.

### 3.5 Technology choices

| Concern | Choice | Reason |
|---------|--------|--------|
| MQTT client | `rumqttc` 0.25 with `use-rustls`, v5 module | Pure Rust, cross-platform builds, MQTT 5 properties |
| TLS roots | `rustls-native-certs` plus optional CA file | Let's Encrypt chain is in OS stores; custom CA for local brokers |
| Config and message types | `serde`, `serde_json`, `schemars` 1.x | Single source of truth for JSON schemas |
| CLI | `clap` 4 (derive) | Generates the help text embedded in `cli.md` |
| IDs and time | `uuid` v7, `chrono` (RFC 3339, UTC) | Sortable ids, unambiguous timestamps |
| Storage | `rusqlite` with `bundled` | No system SQLite dependency on Windows |
| GUI shell | Tauri 2.11 with dialog, clipboard-manager, opener, notification plugins | Same plugin set as BetterMediaInfo plus notifications |
| Frontend | Vite 8, React 19, TypeScript, MUI 9, `@mui/x-tree-view` 9, Zustand 5, react-i18next | BetterMediaInfo stack plus the tree view |
| Integration tests | `testcontainers` with `hivemq/hivemq-ce` | Anonymous MQTT 5 broker on port 1883, no TLS setup needed |
| Errors | `thiserror` in core, `anyhow` in binaries | Typed errors map to CLI exit codes and to `convert_error` |
| Logging | `log` + `env_logger` | Same as BetterMediaInfo, `RUST_LOG=debug` |
| Update check | `ureq` against the GitHub releases API | Same as BetterMediaInfo |

---

## 4. Config design

Spec file: `docs/specs/config.md`. Schema: `schemas/config.schema.json` (generated). JSON keys are camelCase; enum variants used only by the app are PascalCase (as in BetterMediaInfo); values that also travel inside messages (`level`, `alg`, `kid`) keep their wire casing.

### 4.1 Location and precedence

1. `--config <path>` (both apps).
2. `HIVEME_CONFIG` environment variable.
3. Per-OS location, identical to BetterMediaInfo's rules with the app name `HiveMe` and file `HiveMe.json`:
   - Linux: `$XDG_CONFIG_HOME/HiveMe/HiveMe.json` if set, else `$HOME/.config/HiveMe/HiveMe.json`.
   - macOS: `$HOME/Library/Application Support/HiveMe/HiveMe.json`.
   - Windows: `%APPDATA%\HiveMe\HiveMe.json` when the executable lives under `%LOCALAPPDATA%`, `%ProgramFiles%`, or `%ProgramFiles(x86)%` (installed); otherwise next to the executable (portable and development mode).

The SQLite database `HiveMe.db` sits in the same directory as the config file. The directory is created on first launch. On first run either app writes a default config with a generated `device.id` and the hostname as `device.name`, then reports the path. On Unix the file is created with mode 0600.

### 4.2 Schema

Full example with defaults:

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
      { "id": "info",  "topic": "#",  "level": "info",  "enabled": true, "title": "{title|topic}", "body": "{body}" },
      { "id": "warn",  "topic": "#",  "level": "warn",  "enabled": true, "title": "{title|topic}", "body": "{body}" },
      { "id": "error", "topic": "#", "level": "error", "enabled": true, "title": "{title|topic}", "body": "{body}" }
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

Field reference:

| Field | Type | Required | Default | Notes |
|-------|------|----------|---------|-------|
| `version` | integer | yes | 1 | Config schema version. Loader migrates older versions. |
| `device.id` | UUID string | yes | generated | Stable identity of this installation. Used as `sender.id` and to derive client ids. |
| `device.name` | string | no | hostname | Shown in the GUI as the sender name. |
| `broker.url` | string | yes | none | `mqtts://host:8883` (TLS), `wss://host:8884/mqtt` (WebSocket TLS, later phase), `mqtt://host:1883` (plain, warns). |
| `broker.username`, `broker.password` | string | yes | none | HiveMQ Cloud credentials. Password may be empty when `passwordRef` is set. |
| `broker.passwordRef` | object or null | no | null | `{ "type": "Env", "name": "HIVEME_PASSWORD" }` is implemented in phase 1. `{ "type": "Keychain", "service": "HiveMe", "account": "<username>" }` is reserved. |
| `broker.clientIdPrefix` | string | no | `hiveme` | Client id = `<prefix>-<app>-<first 8 hex of device.id>[-<random>]`. |
| `broker.keepAliveSecs` | integer | no | 30 | |
| `broker.sessionExpirySecs` | integer | no | 3600 | GUI retention during network interruptions; explicit disconnect and quit discard the session. `hmc` always uses 0. |
| `broker.connectTimeoutSecs` | integer | no | 10 | |
| `broker.tls.verifyServer` | boolean | no | true | `false` is only honored for non-`hivemq.cloud` hosts and logs a warning. |
| `broker.tls.caFile` | path or null | no | null | Extra PEM roots appended to the native store. |
| `broker.reconnect.*` | integer | no | 1000 / 30000 | Exponential backoff with jitter. |
| `topics.subscriptions` | (string or object)[] | no | `["#"]` | Filters relative to `hiveme`. A filter starting with `$`, or given as `{ "filter": "...", "absolute": true }`, is used verbatim. |
| `publish.qos` | 0, 1, 2 | no | 1 | |
| `publish.retain` | boolean | no | false | |
| `publish.timeoutSecs` | integer | no | 10 | Time to wait for the acknowledgement in `hmc`. |
| `notifications.enabled` | boolean | no | true | |
| `notifications.notifyOwnMessages` | boolean | no | false | Suppress notifications for messages whose `sender.id` equals `device.id`. |
| `notifications.rules[]` | object[] | no | built-ins | See section 7. |
| `gui.displayMode` | `Auto`, `Light`, `Dark` | no | `Auto` | Same semantics as BetterMediaInfo. |
| `gui.theme` | one of the 20 BetterMediaInfo theme names | no | `Ocean` | Palette table copied from BetterMediaInfo `App.tsx`. |
| `gui.language` | BCP 47 tag | no | detected | `en-US` only in phase 1. |
| `gui.history.maxMessagesPerTopic` | integer | no | 1000 | Oldest rows beyond this are deleted per topic. |
| `gui.history.retentionDays` | integer | no | 30 | 0 disables time-based pruning. |
| `gui.window.position`, `gui.window.size` | objects | no | `-1,-1` / `1200x900` | Persisted by `window.rs`; negative position means "center". Minimum 600x450. |
| `update.*` | object | no | weekly | Same fields and semantics as BetterMediaInfo. |
| `encryption.mode` | `Off`, `Opportunistic`, `Required` | no | `Off` | Only `Off` is implemented in this plan. See section 6. |
| `encryption.keys[]` | object[] | no | `[]` | `{ "kid", "alg", "secret", "createdAt", "state" }`. See section 6. |
| `cloudApi` | object or null | no | null | `{ "baseUrl", "orgId", "clusterId", "token", "tokenRef" }`. Later phase. |

### 4.3 Topic resolution rules

- The root topic is fixed at `hiveme` and is not configurable.
- Publish input is always relative, with leading slashes stripped. hmc appends it to
  `hiveme`; hmg appends it to the selected tree topic. Empty input uses the base topic.
- Topics are validated before publishing: non-empty, no wildcard characters, no NUL, at most 65535 bytes UTF-8.
- Rule topic filters are relative unless the rule sets `"absolute": true`.

### 4.4 Versioning and compatibility

- `version` is a required integer. Unknown fields are ignored on read (no `deny_unknown_fields`) and preserved on write, because writes merge the typed struct into the original `Value`. This is the one deliberate departure from BetterMediaInfo's `to_writer_pretty(&self)`, chosen so that a newer `hmc` and an older `hmg` can share one file.
- Adding optional fields with defaults does not bump `version`. Renaming or changing the meaning of a field bumps `version` and adds a migration function `migrate_vN_to_vN+1` plus a fixture pair in `crates/hiveme-core/tests/fixtures/config/`.
- A config with a `version` newer than the binary understands is loaded best-effort with a warning; the app never rewrites it.
- The schema carries `$id` `https://hiveme.dev/schemas/config/v1.json` as an identifier only.

### 4.5 Secrets

- Phase 1: plaintext `password` in the config file. `HIVEME_PASSWORD` overrides it when set.
- Later: `passwordRef.type = "Keychain"` via the `keyring` crate; a `hmc config set-password` command writes the keychain entry. Encryption keys and the REST token follow the same `*Ref` pattern.
- `get_config` for the frontend, any logs, and the settings tab redact nothing in transit (the settings form needs the password to edit it), but logs never print `password`, `encryption.keys[].secret`, or `cloudApi.token`.

---

## 5. Message design

Spec file: `docs/specs/message.md`. Schema: `schemas/message.schema.json` (generated).

### 5.1 Goals

- JSON payload on the wire. Readable by `jq` and third-party tools.
- Backward compatible: new readers read old messages.
- Forward compatible: old readers read new messages, degrading gracefully.
- One envelope shape for plaintext and encrypted messages so readers can route on the header before touching the body.
- Third-party JSON and plain text are still displayed.

### 5.2 Plaintext envelope

```json hiveme:message
{
  "v": 1,
  "id": "018f6b1e-7c2a-7d3e-9f1a-4b2c8d9e0f11",
  "ts": "2026-09-12T09:41:23.512Z",
  "type": "message",
  "sender": {
    "id": "0f7a1c2e-5d4b-4a6e-9c1d-2b3e4f5a6b7c",
    "name": "sams-macbook",
    "app": "hmc",
    "appVersion": "0.1.0"
  },
  "payload": {
    "level": "info",
    "title": "Build finished",
    "body": "hiveme v0.1.0 built in 42s",
    "data": { "duration_s": 42 }
  }
}
```

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `v` | integer | yes | Envelope major version. `1` for this plan. Bumped only for incompatible changes. |
| `id` | string | yes | UUID v7 recommended. Used for de-duplication and for matching a GUI-sent message with its broker echo. |
| `ts` | string | yes | RFC 3339 with timezone, millisecond precision, producer clock. |
| `type` | string | no, default `message` | Open enum. Readers show unknown types as raw JSON. Reserved: `message`, `ack`, `presence`. |
| `sender` | object | no | `id` (UUID of the device), `name`, `app` (`hmc`, `hmg`, or any string), `appVersion`. All optional inside; unknown keys allowed. |
| `payload` | object | yes when `enc` is absent | See 5.3. |
| `enc` | object | no | Present only for encrypted messages. See 5.4. |
| `ciphertext` | string | yes when `enc` is present | base64url without padding. |
| `replyTo` | string | no | `id` of another message. Reserved for threading. |
| `ttlSecs` | integer | no | Mirrors the MQTT message expiry so readers can show staleness. |

### 5.3 Payload

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `body` | string | yes | Main text. May be empty when `data` carries the content. |
| `title` | string | no | Used as the notification title when present. |
| `level` | string | no, default `info` | Case-insensitive open enum: `debug`, `info`, `success`, `warn`, `error`. Serialized JSON and database level fields use lowercase; display labels start with a capital. Unknown values retain their lowercase name with the Info fallback. |
| `data` | object | no | Free-form JSON for scripts. Rendered as a collapsible JSON tree in the GUI. |
| `contentType` | string | no | Hint for `body`, for example `text/markdown`. Default `text/plain`. |

### 5.4 Encrypted envelope

```json hiveme:message-encrypted
{
  "v": 1,
  "id": "018f6b1e-7c2a-7d3e-9f1a-4b2c8d9e0f12",
  "ts": "2026-09-12T09:41:23.512Z",
  "type": "message",
  "sender": { "id": "0f7a1c2e-5d4b-4a6e-9c1d-2b3e4f5a6b7c", "name": "sams-macbook", "app": "hmc" },
  "enc": {
    "alg": "A256GCM",
    "kid": "k-2026-09",
    "iv": "u2m1xwK7Ck3NoMbz"
  },
  "ciphertext": "8Qy0m5jI1n1F0y7b3z9K2Qw3e5r7t9y1u3i5o7p9a1s3d5f7g9h1j3k5l7"
}
```

- `enc.alg`: open enum. `A256GCM` (AES-256-GCM) is the first algorithm. `XC20P` (XChaCha20-Poly1305) is reserved.
- `enc.kid`: key id that selects the pre-shared key.
- `enc.iv`: base64url nonce, 96 bits for `A256GCM`.
- `ciphertext`: AEAD output including the tag, base64url. Plaintext is the UTF-8 JSON of the `payload` object.
- The `payload` key is absent when `enc` is present. Readers that see both prefer `enc` and ignore `payload`.

### 5.5 MQTT 5 properties

Publishers set, and readers may use before parsing:

- Content type: `application/json`.
- User property `hiveme-v`: `"1"`.
- User property `hiveme-enc`: the `enc.alg` value when encrypted.
- Message expiry interval: set only when `ttlSecs` is present.

MQTT 3.1.1 readers ignore these; everything needed is also inside the JSON.

### 5.6 Compatibility rules

Readers (Rust and TypeScript) follow these rules, and tests enforce them with fixture files under `crates/hiveme-core/tests/fixtures/message/`:

1. Unknown keys at any level are ignored, never rejected.
2. Missing optional keys take their documented defaults.
3. Open enums (`type`, `payload.level`, `enc.alg`) never fail parsing. Unknown `enc.alg` or unknown `kid` produce a "cannot decrypt" placeholder that still shows `sender`, `ts`, and topic.
4. `v` greater than the supported version: parse known fields, mark the message as `newerVersion` in the UI, never drop it.
5. Lenient tiers for non-envelope input:
   - Tier A, envelope: JSON object with integer `v` and either `payload` or `enc`.
   - Tier B, raw JSON: any other valid JSON. Displayed as a JSON tree; `body` for notifications is the compact JSON truncated to 200 characters.
   - Tier C, raw text: bytes that are valid UTF-8 are shown as text. Other bytes are shown as hex with a size label. Nothing is ever discarded.
6. Writers always emit `v`, `id`, `ts`, `type`, and `sender`, so future readers can rely on them.
7. Breaking changes (renaming a field, changing a type, changing semantics) require `v: 2`, a new schema file `message-v2.schema.json`, and readers that accept both.

### 5.7 Schema sketch

The generated schema expresses the structure as `oneOf` two variants sharing a header, with `additionalProperties: true` on every object:

```
Message   = Header & (Plain | Encrypted)
Header    = { v: integer >= 1, id: string, ts: date-time, type?: string, sender?: Sender, replyTo?: string, ttlSecs?: integer }
Plain     = { payload: Payload }                      # enc must be absent
Encrypted = { enc: Enc, ciphertext: string }          # payload must be absent
Payload   = { body: string, title?: string, level?: string, data?: object, contentType?: string }
Enc       = { alg: string, kid: string, iv: string }
Sender    = { id?: string, name?: string, app?: string, appVersion?: string }
```

In Rust, `Message` is a struct with `payload: Option<Payload>`, `enc: Option<Enc>`, `ciphertext: Option<String>` plus a `validate()` method, because serde's untagged enums produce poor errors. The schema generator post-processes the derived schema to add the `oneOf` constraint.

---

## 6. Encryption design (spec only, not implemented in this plan)

Spec section: `docs/specs/message.md#encryption`. Config section: `encryption` (4.2).

### 6.1 Threat model

- Protects the `payload` from the broker operator and from anyone holding broker credentials but not the HiveMe key.
- Does not hide topic names, `sender`, `ts`, `id`, message sizes, or timing.
- Does not provide sender authentication beyond "holds the shared key". Anyone with the key can forge messages.
- Replay: receivers de-duplicate by `id` and may reject messages whose `ts` is older than a configurable window. Full replay protection is out of scope.

### 6.2 Algorithm

- `A256GCM`: AES-256-GCM, 96-bit random nonce per message, 128-bit tag.
- Message key: `HKDF-SHA256(ikm = secret, salt = "hiveme/v1", info = "msg:" || kid)`, 32 bytes. Deriving instead of using the secret directly gives domain separation for future uses of the same secret.
- Plaintext: UTF-8 bytes of the serialized `payload` object.
- Associated data: the compact UTF-8 JSON array `[v, id, ts, sender.id or "", type, alg, kid]`. Binding the header prevents moving a ciphertext to another id, sender, or key.
- Nonce reuse is the main risk with GCM. Random 96-bit nonces are safe for well under 2^32 messages per key; the rotation policy keeps keys far below that. `XC20P` is reserved for a future variant with 192-bit nonces.

### 6.3 Secret and key management

- Secret: 32 random bytes, base64url encoded, generated by `hmc key generate` (future command) or any tool that can produce 32 random bytes.
- Config entry: `{ "kid": "k-2026-09", "alg": "A256GCM", "secret": "<base64url>", "createdAt": "2026-09-12", "state": "Active" }`. `state` is `Active` (used for sending) or `Retired` (decrypt only). Exactly one key is `Active` when `mode` is not `Off`.
- Distribution: manual and out of band. The user copies the key entry into the config on every device. No key exchange over MQTT in this design; that would need the asymmetric model, which the envelope can accommodate through `enc.alg` and a future `enc.recipients` field.
- Rotation: add a new `Active` key, mark the old one `Retired`, deploy to all devices, delete the retired key after `retentionDays`. Receivers select the key by `kid`, so old messages remain readable while the retired key exists.
- Storage: plaintext in the config file at first, with the same `secretRef` mechanism as `passwordRef` for OS keychain storage later.
- Modes: `Off` sends plaintext and accepts both. `Opportunistic` encrypts outgoing messages and accepts both. `Required` encrypts outgoing and flags incoming plaintext envelopes in the UI without dropping them (decision 3).

### 6.4 What this plan implements for encryption

- Types `Enc`, `EncryptionConfig`, `KeyEntry` in `hiveme-core`, included in both schemas.
- Parser recognizes encrypted envelopes and returns `Message { enc: Some(_), payload: None }`.
- GUI renders an encrypted bubble as a lock icon with "encrypted (key k-2026-09)" plus sender and time.
- `hmc` and `hmg` refuse to publish while `encryption.mode != "Off"` and report "encryption is not implemented yet" (exit code 3 in `hmc`, snackbar in `hmg`), so a half-configured setup cannot silently send plaintext.
- No cryptography crate is added yet.

---

## 7. Notification rules

Spec section: `docs/specs/gui.md#notifications`.

Rule object:

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `id` | string | required | Unique within the config. Built-ins: `info`, `warn`, `error`. |
| `topic` | string | required | MQTT topic filter, relative to `hiveme` unless `absolute` is true. Wildcards `+` and `#` allowed. |
| `absolute` | boolean | false | |
| `level` | string | `info` | The payload level to match, independently of the MQTT topic. |
| `enabled` | boolean | true | |
| `title` | string | `{title|topic}` | Template. |
| `body` | string | `{body}` | Template. |
| `match` | object or null | null | Reserved for payload matching (for example `{ "level": "error" }`). Not implemented in this plan. |

Templates support `{title}`, `{body}`, `{topic}`, `{level}`, `{sender}`, `{app}`, and the fallback form `{a|b}` which uses the first non-empty value. Unknown placeholders render as empty strings. No expressions.

Evaluation: rules are checked in config order; the first enabled rule matching both the MQTT topic filter and payload level fires. Messages whose `sender.id` equals `device.id` are skipped unless `notifyOwnMessages` is true. Notifications are rate-limited to one per rule per second with an "and N more" suffix. The toolbar "pause notifications" toggle suppresses all rules for the session.

Defaults: if `notifications.rules` is absent, the three built-in rules apply. If present (even empty), only the listed rules apply. Users override a built-in by reusing its `id`.

---

## 8. GUI design

Spec file: `docs/specs/gui.md`. Layout and component structure follow BetterMediaInfo.

`Layout.tsx`: CSS grid with rows `auto 1fr auto` inside a `100vh` box.

- `Toolbar.tsx` (top): `ButtonGroup`s of small `IconButton`s with tooltips, keyboard shortcuts in the tooltip text. Groups: Connect/Disconnect (state-aware icon), Pause notifications (toggle, active color), Clear selected topic history, Settings (F10, opens the Settings tab), About (opens the About tab).
- `MainContent.tsx` (middle): MUI `Tabs`, same `TabControl` and `ControlStatus` pattern as BetterMediaInfo. Tab 0 "Messages" is fixed; "Settings" and "About" open as closable tabs. Shortcuts Ctrl+1..9, Ctrl+W, Ctrl+Tab, Ctrl+Shift+Tab.
- `Footer.tsx` (bottom): the status bar required by the spec. Connection state with color, broker host, reconnect countdown, subscription count, messages received this session, last error (click opens details), database size.
- `NotificationSnackbar.tsx`: top-center `Snackbar` with `Alert` driven by the store's `dialogNotification`, used for command errors and confirmations.

`Messages.tsx` (tab 0): horizontal split with a draggable divider persisted in `localStorage`.

- `TopicTree.tsx` (left, `@mui/x-tree-view` `SimpleTreeView`): topic hierarchy split on `/`, with an always-visible, selectable `hiveme` root. Other nodes follow actual stored MQTT paths. Every nonempty parent path is selectable, including paths with only descendant messages. Each node shows an unread badge. A filter field sits above the tree and keeps `hiveme` visible even when it does not match.
- `hiveme` always appears and is selected and highlighted at startup, even without history. Message boxes use regular colors for info, MUI success colors for success, warning colors for warn, and error colors for error. Existing history stays on its original MQTT paths; no migration is added.
- `MessageView.tsx` (right): chat view for the selected topic and all recursive descendants, filtered by the stored topic field in SQL and paged together. Messages from this device (`sender.id == device.id`) align right, others align left with available sender details. Incoming messages show the sender name or ID above the bubble, without the application name; outgoing and senderless messages omit the header. Every message shows its topic path relative to the selected tree topic below the bubble, immediately before the level badge, in muted gray; empty paths are omitted. Rounded bubbles show `title` (bold), `body`, and a collapsed `data` JSON tree. A reserved row below reveals the relative path, level, QoS, time, and a split copy button only while the pointer is over the message, aligned to the bubble's right edge. The dropdown offers Copy and Copy Raw JSON through the clipboard plugin. Retained status and newer-version markers appear in the same row when applicable. Raw JSON and raw text tiers use a monospace bubble. Encrypted messages show a lock placeholder. Virtualized list (`@tanstack/react-virtual`, already used by BetterMediaInfo), newest at the bottom, auto-scroll unless the user scrolled up.
- `Composer.tsx` (bottom of the message view): full-width multi-line `TextField` with a three-row minimum and 4 px margins to the panel edges, then a right-aligned level dropdown followed by More Options and Send. The level choices are Info (default), Error, Success, and Warn; its value is remembered with the entire composer state per topic in memory. In raw JSON mode the dropdown is disabled and its saved value is excluded from publishing. More Options expands three rows for Topic, Title, and QoS (Config/0/1/2) with Retain Message / As Raw JSON on the QoS row. Topic and title default to empty, QoS to Config, and both checkboxes to unchecked. Topic is relative to the selected tree topic; leading slashes are stripped. Options stay active when collapsed; drafts and options are remembered per selected topic in memory only. Enter sends from any focused composer control without also activating it; inside the open level menu, Enter selects an option without sending. Shift+Enter inserts a newline in the message box; Enter used for text composition does not send. Disabled when disconnected or when no topic is selected.

`Config.tsx` (Settings tab): `SectionHeader` sections as in BetterMediaInfo: Broker (URL, username, password with visibility toggle, TLS options with a CA file picker via the dialog plugin, advanced timings), Topics (subscriptions list editor), Notifications (enabled, notify own messages, rules table with add/edit/delete), Appearance (display mode toggle, theme select, language select), Update (check interval). Changes apply immediately and automatically call `set_config`; the backend reconnects when broker or subscription fields changed. Encryption and Cloud API sections are read-only placeholders that state "not implemented yet" until their phases.

`About.tsx`: app icon, name, version, links to GitHub and author, license, copyright. Update notice appears here and as a dialog in `MainContent` like BetterMediaInfo.

Theme: `App.tsx` copies BetterMediaInfo's `getPaletteByTheme` (20 themes), `displayMode` handling with `prefers-color-scheme`, `typography.fontSize: 12`, and the compact `components` defaults.

Backend contract (Tauri commands and events) is written in `gui.md` with request and response JSON, mirrored by `protocol.rs` and `protocol.ts` (hand-synced, camelCase via `#[serde(rename)]`), and by the generated `src/generated/*.ts` for config and message types.

Commands: `get_about`, `get_config`, `set_config`, `get_status`, `connect`, `disconnect`, `list_topics`, `get_messages(topic, before, limit)`, `mark_read(topic)`, `publish(topic, body | json, options)`, `clear_topic(topic)`, `get_update_result`, `skip_version`, `open_config_file`.
Events: `status`, `message`, `topic-added`, `notification-fired`.

Storage schema (SQLite, `HiveMe.db`):

```
topics(id INTEGER PK, topic TEXT UNIQUE, first_seen_ts, last_seen_ts, unread INTEGER)
messages(id INTEGER PK, topic_id, msg_id TEXT, ts TEXT, received_ts TEXT, sender_id TEXT, sender_name TEXT, app TEXT,
         tier TEXT CHECK(tier IN ('envelope','json','text')), level TEXT, title TEXT, body TEXT,
         raw BLOB, qos INTEGER, retain INTEGER, outgoing INTEGER, UNIQUE(topic_id, msg_id))
schema_version(version INTEGER)
```

Pruning runs on startup and every 10 minutes using `gui.history`.

---

## 9. CLI design

Spec file: `docs/specs/cli.md`, which embeds the exact `hmc --help` output and is checked by a test.

```
hmc [OPTIONS] [MESSAGE]

Arguments:
  [MESSAGE]  Message body. Read from stdin when omitted.

Options:
  -t, --topic <TOPIC>        Topic relative to hiveme; leading slashes are ignored [default: hiveme]
      --json                 Publish MESSAGE (or stdin) as a raw JSON payload without the envelope
      --title <TITLE>        Optional title for the message
  -l, --level <LEVEL>        debug | info | success | warn | error [default: info; independent of topic]
  -q, --qos <QOS>            0 | 1 | 2 [default: publish.qos]
  -r, --retain               Set the retain flag
  -c, --config <PATH>        Config file path
  -v, --verbose              Log connection details to stderr
  -h, --help                 Print help
  -V, --version              Print version
```

Behavior:

- Exactly one of MESSAGE or stdin is used. If both are absent (no argument and stdin is a TTY), exit 2 with usage.
- `--json` requires the input to parse as JSON; otherwise exit 2. The MQTT content type is still `application/json`, but no `hiveme-v` property is set.
- Publishes with the configured QoS and waits up to `publish.timeoutSecs` for the acknowledgement (QoS 1 and 2). QoS 0 returns after the packet is written.
- Output: nothing on success. Errors go to stderr as one line: `hmc: <category>: <detail>`.
- `hmc` is a console application on Windows (no `windows_subsystem` attribute).

Exit codes: 0 success, 1 unexpected error, 2 usage error, 3 config error (including missing config and unimplemented encryption mode), 4 connection or authentication error, 5 publish timeout.

---

## 10. Spec-sync mechanism

The specs must describe the code as built. These mechanisms make drift fail CI rather than rely on memory.

1. Generated schemas. `cargo xtask schema` writes `schemas/config.schema.json` and `schemas/message.schema.json` from the `schemars` derives in `hiveme-core`. Test `schemas_are_current` regenerates in memory and diffs against the committed files, failing with "run `cargo xtask schema`".
2. Validated spec examples. Fenced code blocks in `docs/specs/*.md` whose info string is `json hiveme:config`, `json hiveme:message`, or `json hiveme:message-encrypted` are extracted by test `spec_examples_are_valid`, validated against the matching schema with the `jsonschema` crate, and round-tripped through the Rust types. The examples in sections 4.2, 5.2, and 5.4 are copied into the specs and therefore checked.
3. Fixture-backed compatibility rules. Every rule in section 5.6 has at least one fixture in `crates/hiveme-core/tests/fixtures/message/` (`unknown_fields.json`, `newer_version.json`, `unknown_alg.json`, `raw_json.json`, `raw_text.txt`, ...). `message.md` lists the fixtures next to the rules.
4. CLI help snapshot. Test `cli_help_matches_spec` renders clap's help and compares it with the ```` ```text hiveme:help ```` block in `cli.md`.
5. TypeScript types. `pnpm gen:types` runs `scripts/ts/gen-types.ts`, a Deno script wrapping `json-schema-to-typescript`, over `schemas/` into `src/generated/`. CI fails if the output is not committed. Frontend tests parse the same fixture files through `src/lib/message.ts` to keep both readers in agreement.
6. Protocol pairing. `protocol.rs` and `protocol.ts` stay hand-synced as in BetterMediaInfo; `scripts/ts/check-spec-sync.ts` fails when one changes without the other in the same diff range, and when `crates/hiveme-core/src/{config,message}/**`, `crates/hmc/src/cli.rs`, or `src-tauri/src/{lib,protocol}.rs` change without a matching file under `docs/specs/`. Bypass with the `spec-sync-exempt` PR label for refactors.
7. Status table. `docs/specs/app.md` ends with a table: feature, spec section, implementing module, phase, status (`planned`, `in progress`, `done`). Each phase updates it as its last task.
8. Definition of done for every step: code, tests, spec update, schema regeneration, status table row, and `docs/release_notes.md` entry for user-visible changes. A step without a spec change must say so in its PR description.

---

## 11. Implementation steps

Each step lists tasks, spec sync, tests, and the done condition. Steps inside a phase are sequential; phases 3 and 4 can overlap once phase 2 is done.

### Phase 0: Bootstrap

**Step 0.1 Split the specs.**
- Tasks: create `config.md`, `message.md`, `cli.md`, `gui.md`, `hivemq-cloud.md` from sections 2, 4, 5, 6, 7, 8, 9 of this plan. Rewrite `app.md` as the overview with links, the topic layout decision, the BetterMediaInfo mapping (3.1), and the status table. Add a `Decisions` section mirroring section 1.
- Spec sync: this step is the spec.
- Done when: every design question in this plan has a home in a spec file, and `app.md` lists all files.

**Step 0.2 Workspace and conventions.**
- Tasks: root `Cargo.toml` workspace (`src-tauri`, `crates/hiveme-core`, `crates/hmc`, `xtask`) with `[workspace.package]` version, edition 2024, license, authors; `rust-toolchain.toml` pinned; `.rustfmt.toml`; clippy `-D warnings`; Apache-2.0 header template and a `scripts/ts/check-license-headers.ts`; `package.json` (`dev`, `build`, `tauri`, `typecheck`, `test`, `gen:types`), `pnpm-workspace.yaml` (`onlyBuiltDependencies: [esbuild]`), `index.html`, `vite.config.js` (port 1420, ignore `src-tauri`), `tsconfig.json` copied from BetterMediaInfo; `.gitignore` union of BetterMediaInfo's and cargo's; `README.md` with badges; `CLAUDE.md` and `AGENTS.md` (identical) describing layout, commands, protocol-sync and spec-sync rules; `docs/development.md`, `docs/installation.md`, `docs/release_notes.md`, `docs/todos.md`, `docs/screenshots.md` stubs.
- Spec sync: `app.md` "Repository layout" section identical to 3.2.
- Tests: `cargo build --workspace`, `pnpm install`.
- Done when: `cargo build --workspace` succeeds on the developer machine.

**Step 0.3 Build workflows.**
- Tasks: `.github/workflows/linux_build.yml`, `macos_build.yml` (macos-15-intel and macos-15 matrix), `windows_build.yml` following BetterMediaInfo step for step (LF config, checkout, Node 24, Rust from `rust-toolchain.toml`, pnpm 11, Linux webkit packages) with added steps: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test -r --workspace`, `pnpm typecheck`, `pnpm test`, schema and generated-types freshness, `scripts/ts/check-spec-sync.ts`; then `pnpm tauri build`; upload deb/rpm/AppImage, dmg, msi/nsis plus a portable 7z that contains `hmg.exe` and `hmc.exe`; upload `hmc` for every OS. Docker-based tests only on the Linux job. `env: HIVEME_VERSION`. `scripts/ts/change-version.ts` covering `package.json`, root `Cargo.toml`, `src-tauri/tauri.conf.json`, and the three workflows.
- Done when: all three workflows are green on a PR that touches nothing.

### Phase 1: Core model

**Step 1.1 Config module.**
- Tasks: types with `serde` and `schemars` (`#[serde(default, rename_all = "camelCase")]`, `Default` impls per struct as in BetterMediaInfo); path resolution (4.1) including the Windows installed-vs-portable rule; loader with `version` migration table; validation (URL scheme, rule ids unique, exactly one active key when mode is not `Off`); writer that merges into the original `Value` and sets file mode 0600; `HIVEME_PASSWORD` override; redaction helper for logs.
- Spec sync: `config.md` field table matches the struct field by field; example block validated by test.
- Tests: defaults round-trip; unknown keys survive a read-modify-write; each validation rule has a failing fixture; path precedence with env and flag; Windows rule tested with injected environment; migration fixture for v1 (identity).
- Done when: `schemas/config.schema.json` is generated and committed and `spec_examples_are_valid` passes for `config.md`.

**Step 1.2 Message module.**
- Tasks: `Message`, `Payload`, `Sender`, `Enc` types; `Message::new_text(...)` builder (UUID v7, UTC ts, sender from config); `parse(bytes) -> Parsed { Envelope | RawJson | RawText }` implementing 5.6; `to_bytes()`; MQTT 5 property helpers; `xtask schema` post-processing that adds the `oneOf` for plain versus encrypted.
- Spec sync: `message.md` sections 5.2 to 5.7 and section 6 verbatim from this plan, with fixture names next to each compatibility rule.
- Tests: fixtures for every rule in 5.6; encrypted example parses to a placeholder; builder output validates against the schema; property-based test that any JSON value parses without panic.
- Done when: `schemas/message.schema.json` committed, `spec_examples_are_valid` passes for `message.md`.

**Step 1.3 Topic and rules modules.**
- Tasks: fixed-root relative topic resolution, topic validation, filter matching with `+`, `#`, and `$`-prefixed topics; rule engine with templating and first-match semantics; per-rule rate limiter; payload levels independent of publishing topics.
- Spec sync: `gui.md#notifications` and `config.md#topics` updated with the exact matching semantics and the template grammar.
- Tests: MQTT filter matching table from the MQTT 5 spec examples; template rendering including the `{a|b}` fallback; built-in defaults apply only when `rules` is absent.
- Done when: rule behavior in the spec is executable as tests.

**Step 1.4 Schema and spec tooling.**
- Tasks: `xtask schema`; `xtask check-spec` (extract fenced blocks and validate); `scripts/ts/check-spec-sync.ts`; `spec_examples_are_valid`, `schemas_are_current` tests; `pnpm gen:types`; PR template with the definition-of-done checklist.
- Spec sync: `app.md` documents the tooling and the fenced-block tags.
- Done when: deleting a field from a schema file makes CI fail, and an invalid example in a spec makes CI fail.

### Phase 2: MQTT client

**Step 2.1 Async client wrapper.**
- Tasks: `MqttClient::connect(&Config, Role::Cli | Role::Gui)`; URL parsing for `mqtts`, `mqtt`, `wss` (wss behind a feature, may slip to phase 6); rustls config with native roots plus `caFile`; SNI from the host; client id scheme from 2.4; reconnect with exponential backoff and jitter (GUI only); `publish(topic, bytes, qos, retain, props) -> Result<()>` that resolves on acknowledgement with timeout; `subscribe(filters)`; `incoming()` stream of `(topic, bytes, props)`; `status()` watch channel.
- Spec sync: `hivemq-cloud.md` gains "How HiveMe connects": ports, TLS, client ids, session settings, known Serverless limits.
- Tests: unit tests for URL parsing and client id derivation; integration tests with `testcontainers` and `hivemq/hivemq-ce`: publish then receive, QoS 1 acknowledgement, reconnect after container restart, subscription to `hiveme/#`. Skipped with a clear message when Docker is unavailable. Opt-in test against the real cloud when `HIVEME_TEST_BROKER_URL`, `HIVEME_TEST_USERNAME`, `HIVEME_TEST_PASSWORD` are set; this is the only test that exercises TLS with public roots.
- Done when: integration tests pass locally with Docker and in the Linux workflow.

### Phase 3: `hmc`

**Step 3.1 CLI.**
- Tasks: clap definition from section 9; stdin handling; `--json`; explicit payload levels; exit code mapping from core error types; first-run default config creation; refusal when `encryption.mode != Off`.
- Spec sync: `cli.md` gets the exact help text (checked by `cli_help_matches_spec`), the exit code table, and stdin semantics.
- Tests: `assert_cmd` tests for usage errors, missing config (exit 3, file created), `--json` with invalid JSON (exit 2); integration test that publishes through the Docker broker and a subscriber receives a valid envelope.
- Done when: `echo hi | hmc` and `hmc -t error "disk full"` work against the user's Serverless cluster (manual check recorded in the PR).

### Phase 4: `hmg`

**Step 4.1 Tauri scaffold.**
- Tasks: `src-tauri` package `hmg` with lib name `hmg_lib` (`crate-type = ["staticlib", "cdylib", "rlib"]`), `main.rs` with the `windows_subsystem` attribute calling `hmg_lib::run()`, `build.rs` calling `tauri_build::build()`, `tauri.conf.json` (`productName` "HiveMe", `mainBinaryName` "hmg", identifier `com.caoccao.hiveme`, window 1200x900 min 600x450 `visible: false`, `csp: null`, bundle targets all, NSIS and WiX language lists copied), icons, `capabilities/default.json` with core window, dialog, opener, clipboard-manager, and notification permissions; `lib.rs` with `run()` (env_logger, tokio runtime, managed `AppState`, plugins, `window::setup`, `window::on_window_event`, `generate_handler!`), `constants.rs`, `config.rs` wrapper, `window.rs` copied and adapted; frontend `main.tsx`, `App.tsx` (theme code copied), `Layout.tsx`, `Toolbar.tsx`, `MainContent.tsx` (tab machinery), `Footer.tsx` (status bar placeholder), `NotificationSnackbar.tsx`, `About.tsx`, `lib/store.tsx`, `lib/service.ts`, `lib/protocol.ts`, `lib/constants.ts`, `i18n` with `en-US.json`; MUI, `@mui/x-tree-view`, `@tanstack/react-virtual`, Zustand, react-i18next, Tauri plugin packages; vitest configured.
- Spec sync: `gui.md` gains "Build and run", the component map, and the IPC contract skeleton.
- Tests: `pnpm typecheck`, `pnpm test` (smoke), `pnpm tauri build --debug` in all three workflows without signing.
- Done when: the window opens on macOS with toolbar, empty Messages tab, Settings and About tabs working, window state restored across restarts, and all workflows build.

**Step 4.2 Storage.**
- Tasks: `hiveme-core::storage` with the SQLite schema from section 8 and migrations; insert with de-duplication on `(topic, msg_id)`; queries for the tree, paged history per topic, unread counts; pruning job; `src-tauri/src/storage.rs` wrapper.
- Spec sync: `gui.md#storage` documents the tables, the pruning policy, and the database path.
- Tests: insert and query round-trips, de-duplication, pruning by count and by age, migration from an empty file.
- Done when: history survives an app restart.

**Step 4.3 Backend commands and events.**
- Tasks: `AppState` (config, client handle, store, rule engine, notification limiter); `mqtt.rs` task bridging the core client to storage, rules, and `Emitter`; commands and events listed in section 8 as alphabetized wrappers in `lib.rs` delegating to `controller.rs`; `protocol.rs` and `protocol.ts` types (`Status`, `TopicNode`, `MessageRow`, `PublishOptions`, `*Event`); own-message reconciliation by `id`; reconnect on `set_config` when broker or subscription fields changed.
- Spec sync: `gui.md#ipc` lists each command with request and response JSON, and each event payload, referencing the generated types.
- Tests: Rust unit tests for the state machine with a fake client; a vitest test that parses fixture messages with `src/lib/message.ts` and matches the Rust tiers.
- Done when: the frontend can drive the whole flow through IPC with a mocked broker.

**Step 4.4 Frontend Messages tab.**
- Tasks: `Messages.tsx` split pane; `TopicTree.tsx` with unread badges and filter; `MessageView.tsx` with virtualization and bubble alignment by sender; `Composer.tsx` with Enter to send; `Footer.tsx` status bar wired to `status` events; store slices for topics, messages per topic, selection, status; `lib/format.ts` for times and sizes; `lib/message.ts` lenient parser (TypeScript twin of the Rust tiers, used for display only).
- Spec sync: `gui.md` layout description, keyboard shortcuts, and the exact rendering rules for each message tier.
- Tests: component tests for tree building from topic lists, bubble alignment, composer key handling, and parser tier rendering.
- Done when: the user can select a topic, read history, send a message, and see it echoed; `hmc -t error` from another terminal appears live in the GUI.

**Step 4.5 Settings tab.**
- Tasks: `Config.tsx` sections from section 8 with `SectionHeader`; rules table editor; CA file picker through the dialog plugin; save with validation errors surfaced in the snackbar; About tab content.
- Spec sync: `gui.md#settings` lists every field with its config path.
- Tests: component tests for form to config mapping and validation messages.
- Done when: the broker can be configured entirely from the GUI on a fresh install and the connection comes up after save.

**Step 4.6 Notifications end to end.**
- Tasks: `notification.rs` wiring the rule engine to the notification plugin on all three OSes; pause toggle; rate limiting; clicking a notification focuses the window and selects the topic where the platform supports it.
- Spec sync: `gui.md#notifications` matches behavior including platform caveats (Windows app identity in debug builds).
- Tests: rule engine unit tests from 1.3; manual test checklist per OS in `gui.md`.
- Done when: the three built-in rules produce notifications on macOS, Linux (GNOME), and Windows.

### Phase 5: Hardening and release

**Step 5.1 Update check and packaging.**
- Tasks: `update.rs` with the GitHub releases check for `caoccao/HiveMe`, `update` config handling, update dialog and skip-version flow as in BetterMediaInfo; release builds of `hmc` included in the portable archive and uploaded per OS; versions bumped with `scripts/ts/change-version.ts` and propagated to `sender.appVersion`; `docs/installation.md` describing dmg, msi, nsis, portable, deb, rpm, AppImage.
- Spec sync: `app.md#install` documents install paths and the shared config location per OS.
- Done when: a tagged build produces installers and `hmc` binaries for all three OSes.

**Step 5.2 Docs and onboarding.**
- Tasks: README quick start covering cluster creation, credential creation in the HiveMQ Cloud console, filling the Broker section in Settings or `HiveMe.json`, and the first `hmc` message; troubleshooting section (TLS, SNI, client id collisions, credential caching delay); screenshots.
- Spec sync: `hivemq-cloud.md` cross-links the console steps.
- Done when: a new user can go from zero to a notification with the README alone.

### Phase 6: Designed now, implemented later (not in this plan's scope)

- Encryption (`A256GCM`, `hmc key generate`, `Opportunistic` and `Required` modes).
- REST API client and GUI "Cluster" tab (`cloudApi` config, list clients, subscriptions, metrics).
- Keychain-backed `passwordRef`, `secretRef`, `tokenRef`.
- Named profiles.
- Additional locales (BetterMediaInfo's set: de, es, fr, it, ja, zh-CN, zh-HK, zh-TW).
- WebSocket transport if it slipped from 2.1.
- Payload-based rule matching (`rules[].match`).
- `hmc sub` and `hmc config` subcommands.

Each of these already has its config fields and spec sections reserved so adding them does not bump `version` or `v`.

---

## 12. Testing and CI

| Layer | Tool | Where it runs |
|-------|------|---------------|
| Rust unit | `cargo test -r --workspace` | all three workflows |
| Schema and spec checks | tests from section 10, `xtask check-spec`, `scripts/ts/check-spec-sync.ts` | all three workflows |
| MQTT integration | `testcontainers` + `hivemq/hivemq-ce` | Linux workflow, local with Docker |
| Real cloud | opt-in env vars | local only |
| CLI behavior | `assert_cmd` | all three workflows |
| Frontend | `vitest`, React Testing Library, `tsc -b`, eslint | all three workflows |
| GUI build | `pnpm tauri build` | all three workflows |
| Lint | `cargo fmt --check`, `cargo clippy --workspace -D warnings`, license header check | all three workflows |

Workflows cache cargo and pnpm stores. macOS and Windows jobs skip Docker tests via `HIVEME_SKIP_DOCKER=1`.

---

## 13. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| `rumqttc` v5 API gaps (property handling, TLS options) | Wrap the client behind a `hiveme-core::mqtt` trait so it can be swapped; verify properties in the Docker integration test in phase 2 before building on them. |
| HiveMQ CE container lacks TLS, so TLS is only covered by the opt-in cloud test | Add a TLS-enabled Mosquitto container with a self-signed CA and `caFile` to the integration suite in phase 2 if time allows; otherwise document the gap in `hivemq-cloud.md`. |
| Serverless credentials may be publish-only or subscribe-only | `hmg` surfaces SUBACK failure reasons in the status bar; `hmc` maps PUBACK reason codes to exit code 4 with the reason string. |
| Workspace changes Tauri's target directory to the repo root | Workflow artifact paths use `target/release/bundle/...`; `docs/development.md` states it. |
| Windows notification behavior differs (app identity required) | Tauri handles app identity via the bundle; document the debug-build limitation in `gui.md`. |
| Tree view performance with many topics | Virtualized `RichTreeView`, topic query capped, filter field. |
| Config rewrite by the GUI loses user comments | JSON has no comments; the merge-on-write preserves unknown keys, and the GUI only writes when the user saves settings or moves the window. |
| Window-state writes on every move/resize contend with `hmc` reads | Writes are atomic (temp file + rename); `hmc` retries a read once on parse failure. |

---

## 14. Open items to confirm during implementation

- Whether `wss://` transport ships in phase 2 or phase 6 depends on `rumqttc` WebSocket support quality with rustls at implementation time.
- The exact MUI tree component (`RichTreeView` vs `SimpleTreeView`) is chosen in step 4.4 based on virtualization support in `@mui/x-tree-view` 9.
- The `$id` URLs for the schemas are placeholders until a docs site exists.
- Whether `pnpm tauri build` in a workspace needs `--config` tweaks for the portable 7z step on Windows is verified in step 0.3.
