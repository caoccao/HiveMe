# App

HiveMe is a Rust project with two applications built on
top of MQTT, sharing one config file and one message format.

1. **HiveMe CLI** (`hmc`) sends a message to the broker from a shell or a script. Run
   with no arguments on a terminal, it opens a terminal UI, specified in
   [tui.md](tui.md), that has every feature of the GUI.
2. **HiveMe GUI** (`hmg`) watches the broker, keeps a local history, and raises OS
   notifications from rules.

Both support Linux, macOS, and Windows, and both talk to a
[HiveMQ Cloud](https://docs.hivemq.com/hivemq-cloud/index.html) cluster over MQTT 5
with TLS.

## Specifications

| Document | Covers |
|----------|--------|
| [config.md](config.md) | The shared JSON config: location, fields, versioning, secrets, topic resolution |
| [message.md](message.md) | The JSON message envelope, its payload, parse tiers, compatibility rules, and the encryption design |
| [cli.md](cli.md) | `hmc`: usage, behavior, exit codes, the interactive mode trigger |
| [gui.md](gui.md) | `hmg`: layout, notifications, storage, IPC, settings, window |
| [tui.md](tui.md) | The terminal UI of `hmc`: layout, keys, theme, languages, startup, shutdown, tests |
| [session.md](session.md) | The backend both applications share: operations, events, types, notifications, two processes on one installation; `hmg` runs on it |
| [hivemq-cloud.md](hivemq-cloud.md) | What the broker offers, how HiveMe connects, and the REST API |

The implementation plans are
[docs/plans/plan-initialization.md](../plans/plan-initialization.md), which built what
exists, and [docs/plans/plan-terminal-ui.md](../plans/plan-terminal-ui.md), which
built the terminal UI and the shared session.

## Applications

### HiveMe CLI

`hmc` publishes one message and exits, or opens the terminal UI.

- `hmc --help` or `hmc -h` shows the help.
- `hmc <message>` sends to the default topic.
- `hmc -t <topic> <message>` sends to a topic.
- `echo <message> | hmc` reads the body from stdin.
- `hmc` alone, on a terminal, opens the terminal UI; `hmc --tui` opens it regardless of
  stdin.

Full reference in [cli.md](cli.md) and, for the terminal UI, [tui.md](tui.md).

### HiveMe GUI

`hmg` uses Tauri and React. The main window has a toolbar at the top, a status bar in
the footer, a topic tree on the left, and a message view on the right. The message
view is a chat: bubbles for the selected topic and its recursive children, with an input box and a send button at
the bottom.

The rule based notification system ships with four enabled built-in rules:

| Payload level | Notification |
|-------|--------------|
| `info` | info message |
| `success` | success message |
| `warn` | warning message |
| `error` | error message |

Rules are configurable. On every startup, hmg shows, selects, and highlights `hiveme`,
even with an empty database. Message boxes use regular
colors for `info`, MUI success colors for `success`, warning colors for `warn`, and
error colors for `error`.
Full reference in [gui.md](gui.md).

## Topics

The root topic is always `hiveme` and is not configurable. All log levels publish to
`hiveme` by default; severity lives in `payload.level`, independently of the MQTT topic.
The built-in notification rules match `hiveme/#` and filter by payload level. Publish
input is always relative: leading slashes are stripped before joining it to `hiveme`
in hmc or to the selected tree topic in hmg. Empty input uses that base topic itself.
Subscription and rule filters are relative to `hiveme` unless marked absolute. See
[config.md](config.md#topic-resolution).

## Authentication

The broker's TLS MQTT URL, username, and password live in the shared config file, in
the `broker` block. HiveMQ Cloud accepts TLS only, on port 8883 for MQTT and 8884 for
MQTT over WebSocket. See [config.md](config.md) and
[hivemq-cloud.md](hivemq-cloud.md).

## Decisions

These were settled before implementation started and are binding until a later plan
revises them.

| # | Question | Decision |
|---|----------|----------|
| 1 | HiveMQ Cloud plan and the role of the REST API | A Serverless cluster, and only that for now. The applications are MQTT only. Everything a Starter plan or above adds, the REST API included, is designed and reserved but implemented later. |
| 1a | Where a cluster is configured | In `hmg`. Its Settings tab takes the URL, username, and password. Copy CLI setup copies the complete `hmc --init '<json>'` command, ready to paste and run. See [config.md](config.md#the-setup-string). |
| 1b | TLS | Automatic and not configurable for the supported plan. A Serverless cluster chains to a public authority the OS already trusts, so neither application generates, enrolls, or asks about a certificate. `broker.tls` remains for a local test broker. |
| 2 | Config location and password storage | The per-OS config directory, with `--config` and `HIVEME_CONFIG` overrides. The password is plain text in the file, mode 0600 on Unix, with `passwordRef` reserved for the OS keychain. |
| 3 | Producers and GUI strictness | Producers are the HiveMe tools plus the user's own scripts. The GUI is lenient: it parses the envelope when valid and otherwise shows the raw payload. |
| 4 | Encryption key model | A symmetric pre-shared key, AES-256-GCM, HKDF derived, with a key id for rotation. Designed now, implemented in phase 6. |
| 5 | Profiles | One `broker` object. The config is versioned so a `profiles` map can be added later. |
| 6 | Topic layout | Fixed root `hiveme`, with no prefix setting. Publish inputs are relative, with leading slashes stripped. hmc uses the root; hmg uses the selected topic. |
| 7 | Message history | Persisted in SQLite next to the config, bounded per topic and by retention days. |
| 8 | Notification rules | Configurable, with four enabled built-in rules and separate OS and topmost channel switches. |
| 9 | Schema source of truth | Rust types with `serde` and `schemars` generate the JSON schemas. Tests fail when the committed schemas are stale. TypeScript types are generated from those schemas. |
| 10 | Spec files | Split by concern, as listed above. |
| 11 | Frontend stack | Vite, React 19, TypeScript, MUI with `@mui/x-tree-view`, Zustand, react-i18next, pnpm. |
| 12 | Tests and CI | Broker integration tests against HiveMQ CE. A native hmc-to-hmg end-to-end test requires Docker and WebDriver and fails if unavailable; Linux CI runs it under Xvfb. One GitHub Actions build workflow per OS. |
| 13 | CLI scope for phase 1 | Message argument, `-t`, stdin, and `--json`. No subcommands, then or later: a bare word is the message, and every mode is an option. See [cli.md](cli.md#the-body). |
| 14 | MQTT version | MQTT 5, through `rumqttc`. |
| 15 | GUI publishing | Inside the message view, chat style, with an input box and a send button at the bottom. |
| 16 | Payload fields | `title`, `body`, `level`, a free-form `data` object, and `sender` in the envelope. |
| 17 | Reference architecture | The UI layout and Tauri architecture follow the sibling project `../BetterMediaInfo`. See below. |
| 18 | Terminal UI | The decisions behind the interactive mode of `hmc`, the shared session, and the shared catalogs are section 1 of [the terminal UI plan](../plans/plan-terminal-ui.md). |

## Reference architecture

HiveMe mirrors `../BetterMediaInfo` so that both projects can be maintained with one
set of habits.

| BetterMediaInfo | HiveMe | Notes |
|-----------------|--------|-------|
| `src-tauri/` as a single Cargo package | `src-tauri/` as the package `hmg` inside a root workspace | The workspace is needed because `hmc` and `hiveme-core` are separate crates. The Cargo target directory moves to the repository root. |
| `lib.rs` with alphabetized `#[tauri::command]` wrappers, `convert_error`, and a `run()` that hands Tauri a tokio runtime | the same | Commands delegate to `controller.rs`. Logging through `log` and `env_logger`, controlled by `RUST_LOG`. |
| `controller.rs` holds the business logic | `controller.rs` orchestrates only | The logic lives in `hiveme-core` so `hmc` shares it. |
| `protocol.rs` and `protocol.ts` hand-synced | the same, for IPC-only types | Config and message types are generated from the schemas instead. |
| `config.rs` with `#[serde(default)]`, camelCase keys, `OnceLock<RwLock<Config>>`, and `<App>.json` in the per-OS config directory | a thin wrapper over `hiveme_core::config` | The file is `HiveMe/HiveMe.json`. `hmc` uses the same resolution code. |
| `window.rs` with `setup` and `on_window_event` | the same | Window state lives in `gui.window`. |
| `constants.rs` with `APP_NAME` | the same, `APP_NAME = "HiveMe"` | |
| Plugins: dialog, clipboard-manager, opener | the same | Shared native delivery replaces the notification plugin; the topmost host follows BatchMkvMerge. |
| `App.tsx` with `ThemeProvider`, display modes, twenty palettes, compact defaults | the same | |
| `Layout.tsx` grid `auto 1fr auto` with Toolbar, MainContent, Footer | the same | The footer is the status bar; the copyright moves to the About tab. |
| `MainContent.tsx` tabs with `ControlStatus` and keyboard shortcuts | the same | Tab 0 is the fixed Messages tab. |
| `Toolbar.tsx` icon button groups with tooltips | the same | |
| `NotificationSnackbar.tsx` driven by the store | the same | |
| `lib/store.tsx` Zustand, `lib/service.ts` invoke wrappers, `lib/constants.ts`, `lib/format.ts` | the same | Components never call Tauri APIs directly. |
| `src/i18n` with react-i18next and nine locales | the same structure and locale set | |
| `Config.tsx` settings tab with a vertical category strip, `SectionHeader` sections, and `SettingRow` appearance controls | the same, with HiveMe's categories | Appearance (default), Broker, Editor (`hmg` only), History, Notifications, Topics, Update, Advanced. |
| `About.tsx` | the same | |
| Update check against GitHub releases | the same, for `caoccao/HiveMe` | |
| Three per-OS build workflows | the same, plus lint, test, and spec checks | |
| `scripts/ts/change-version.ts` with Deno | the same | |
| Apache-2.0 header on every source file, rustfmt `max_width = 120` and `tab_spaces = 2`, edition 2024, a pinned toolchain | the same | |

## Repository layout

The layout as built; see [Status](#status) for the phase that built each part.

```
HiveMe/
  Cargo.toml                      # workspace: src-tauri, crates/hiveme-core, crates/hmc, xtask; [workspace.package] version
  rust-toolchain.toml             # channel pinned (1.96.0 at bootstrap), clippy + rustfmt, minimal profile
  .rustfmt.toml                   # max_width = 120, tab_spaces = 2
  .cargo/config.toml              # the `cargo xtask` alias
  .gitattributes                  # LF checkouts everywhere, so generated files compare equal
  package.json, pnpm-workspace.yaml, pnpm-lock.yaml
  index.html, vite.config.js, tsconfig.json
  public/                         # favicon, images used by About
  src/                            # React frontend of hmg
    main.tsx, App.tsx
    components/                   # Layout, Toolbar, MainContent, Footer, NotificationSnackbar,
                                  # Messages, TopicTree, MessageView, Composer, Config, About
    lib/                          # store.tsx, service.ts, protocol.ts, constants.ts, format.ts, types.ts, message.ts
    generated/                    # config.ts, message.ts generated from schemas/ (committed)
    i18n/                         # index.ts, index.test.ts: react-i18next over the catalogs in locales/
  locales/                        # the nine catalogs, shared by the frontend and hmc
  src-tauri/                      # Tauri 2 app, package `hmg`, lib `hmg_lib`, binary `hmg`
    Cargo.toml, tauri.conf.json, build.rs, capabilities/default.json, icons/
    tauri.windows.conf.json       # Windows only: the bundle entry that carries hmc
    src/                          # main.rs, lib.rs, controller.rs, protocol.rs, constants.rs, window.rs,
                                  # events.rs, notification.rs: adapters over hiveme_core::session
  crates/
    hiveme-core/                  # config, message, topic, rules, mqtt, storage (feature), session (feature), cloud (feature, later)
      src/session/                # the shared backend of session.md: mod, config, mqtt, notify, history, update, types
      src/i18n/                   # locale resolution, catalogs, plural rules, formatting (feature i18n)
    hmc/                          # CLI binary `hmc`
      build.rs, icons/            # the Windows executable icon and version information, and the
                                  # macOS .icns `cargo xtask icon` gives the built binary
      src/tui/                    # the terminal UI of tui.md: the shell, messages/, settings/, widgets/,
                                  # and tests/, the in-process tests
      tests/                      # cli.rs, publish.rs, and tui.rs, the terminal UI in a pseudo-terminal
  xtask/                          # `cargo xtask schema`, `cargo xtask check-spec`, `cargo xtask icon`
  schemas/                        # broker-init, config, message .schema.json (generated, committed), README.md
  scripts/
    ts/                           # Deno scripts: deno.json, deno.lock, change-version.ts,
                                  # check-license-headers.ts, check-spec-sync.ts, gen-types.ts
    license-header.txt
  docs/
    specs/                        # app.md, config.md, message.md, cli.md, gui.md, tui.md, session.md, hivemq-cloud.md
    plans/                        # plan-initialization.md, plan-terminal-ui.md
    installation.md, development.md, release_notes.md, screenshots.md, todos.md
  .github/workflows/              # linux_build.yml, macos_build.yml, windows_build.yml
  .config/                        # gitignored: broker.json, the setup string of a real cluster for the opt-in tests
  CLAUDE.md, AGENTS.md, README.md, LICENSE
```

## Spec sync

The specifications describe the code as built. These mechanisms make drift fail CI
rather than rely on memory.

| # | Mechanism | Where |
|---|-----------|-------|
| 1 | `cargo xtask schema` regenerates the JSON schemas from the Rust types; the test `schemas_are_current` fails when the committed files are stale | `xtask`, `schemas/` |
| 2 | `cargo xtask check-spec` extracts fenced blocks tagged `json hiveme:broker-init`, `json hiveme:config`, `json hiveme:message`, and `json hiveme:message-encrypted` from `docs/specs/*.md`, validates each against its schema, and round trips it through the Rust types | `xtask`, `docs/specs/` |
| 3 | Every compatibility rule in [message.md](message.md#compatibility-rules) names a fixture that proves it | `crates/hiveme-core/tests/fixtures/` |
| 4 | `cli_help_matches_spec` compares clap's rendered help with the `text hiveme:help` block, so the block is the help text rather than a description of it | `crates/hmc`, [cli.md](cli.md) |
| 5 | `pnpm gen:types` regenerates the TypeScript types from the schemas; CI fails when the output is not committed | `scripts/ts/gen-types.ts`, `src/generated/` |
| 6 | `scripts/ts/check-spec-sync.ts` fails when the config, message, MQTT, CLI, or IPC code changes without a matching change under `docs/specs/`, when `protocol.rs` or the session's `types.rs` changes without `protocol.ts`, and when `protocol.ts` changes without either | `scripts/ts/` |
| 7 | The status table below records what is built | this file |
| 8 | Definition of done for every step: code, tests, spec update, regenerated schemas, a status table row, and a release note for user visible changes | the plan |
| 9 | The same script maps the shared session (`crates/hiveme-core/src/session/`) to [session.md](session.md), and the terminal UI (`crates/hmc/src/tui/`) and the Rust catalogs (`crates/hiveme-core/src/i18n/`) to [tui.md](tui.md); the rest of `crates/hmc/src/` still maps to [cli.md](cli.md). The rules exist ahead of the directories, so the first code in them needs its spec | `scripts/ts/` |

Bypass the pairing check with the `spec-sync-exempt` pull request label when a change
is a pure refactor.

Mechanisms 1 and 5 compare generated files against freshly generated ones, so the
working tree has to hold the same bytes on every platform. `.gitattributes` sets
`text=auto eol=lf` to guarantee that, and the schema comparison ignores line endings as
well, so an editor that rewrites a file cannot be mistaken for a drifted schema.

## Status

| Feature | Spec | Module | Phase | Status |
|---------|------|--------|-------|--------|
| Specifications split by concern | all | `docs/specs` | 0.1 | done |
| Workspace, conventions, toolchain | [app.md](#repository-layout) | root | 0.2 | done |
| Build workflows | [app.md](#build-and-release) | `.github/workflows` | 0.3 | done |
| Config load, save, and compatibility | [config.md](config.md) | `hiveme-core::config` | 1.1 | done |
| Shared config initialization with unchanged, updated, and created outcomes | [config.md](config.md#the-setup-string), [cli.md](cli.md#setting-up) | `hiveme-core::config::ConfigFile::initialize`, `crates/hmc` | 3.1 | done |
| Message envelope and parser | [message.md](message.md) | `hiveme-core::message` | 1.2 | done |
| Fixed root `hiveme`; shared relative publishing with leading-slash normalization | [config.md](config.md#topic-resolution) | `hiveme-core::topic` | 1.3 | done |
| Notification rule engine: per-rule OS and topmost switches, session-based eligibility, and pause that discards both delivery queues | [gui.md](gui.md#notifications) | `hiveme-core::rules`, `hiveme-core::session` | 1.3 | done |
| Schema and spec tooling | [app.md](#spec-sync) | `xtask`, `scripts` | 1.4 | done |
| MQTT client | [hivemq-cloud.md](hivemq-cloud.md#how-hiveme-connects) | `hiveme-core::mqtt` | 2.1 | done |
| CLI | [cli.md](cli.md) | `crates/hmc` | 3.1 | done |
| CLI confirmation after a successful publish | [cli.md](cli.md#output) | `crates/hmc/src/run.rs` | 3.1 | done |
| Graceful GUI quit with MQTT session cleanup | [hivemq-cloud.md](hivemq-cloud.md#disconnect-and-quit), [gui.md](gui.md#window) | `hiveme-core::mqtt`, `src-tauri/src/window.rs` | 4.3 | done |
| Tauri scaffold | [gui.md](gui.md#build-and-run) | `src-tauri`, `src` | 4.1 | done |
| Message storage | [gui.md](gui.md#storage) | `hiveme-core::storage` | 4.2 | done |
| Backend commands and events | [gui.md](gui.md#ipc) | `src-tauri` | 4.3 | done |
| Payload levels independent of MQTT topics; message-box severity colors | [gui.md](gui.md#message-view), [cli.md](cli.md#the-message) | `hiveme-core::rules`, `crates/hmc`, `src/components/MessageView.tsx` | 4.4 | done |
| Success level in shared messages, CLI publishing, notification-rule choices, and MUI success colors | [message.md](message.md#payload), [gui.md](gui.md#message-view) | `hiveme-core::message`, `crates/hmc`, `src/components/MessageView.tsx` | 4.4 | done |
| Case-insensitive CLI levels and lowercase serialized/stored values with capitalized display labels | [message.md](message.md#payload), [cli.md](cli.md#the-message) | `hiveme-core::message`, `hiveme-core::storage`, `crates/hmc` | 4.4 | done |
| Recursive topic selection with database filtering, shared pagination, and live descendant updates | [gui.md](gui.md#storage) | `hiveme-core::storage`, `src/lib/store.tsx`, `src-tauri/src/controller.rs` | 4.4 | done |
| Rounded message bubbles with hover controls and direct copy | [gui.md](gui.md#message-view) | `src/components/MessageView.tsx` | 4.4 | done |
| Incoming sender headers and relative topic paths before the level badge below each message | [gui.md](gui.md#message-view) | `src/components/MessageView.tsx` | 4.4 | done |
| Always-visible, selected startup topic `hiveme`; label selection independent of icon expansion | [gui.md](gui.md#topic-tree) | `src/components/TopicTree.tsx`, `src/lib/store.tsx` | 4.4 | done |
| Topic filter with consistent 4 px margins | [gui.md](gui.md#topic-tree) | `src/components/TopicTree.tsx` | 4.4 | done |
| Messages tab | [gui.md](gui.md#layout) | `src/components` | 4.4 | done |
| Compact full-width composer with 4 px panel margins, a three-row input minimum, checkboxes beside QoS, collapsible options that stay active, and per-topic drafts in memory | [gui.md](gui.md#composer) | `src/components/Composer.tsx` | 4.4 | done |
| Enter sends from every focused composer control without also activating it | [gui.md](gui.md#composer) | `src/components/Composer.tsx` | 4.4 | done |
| Level dropdown left of More Options, remembered with all composer state per topic in memory | [gui.md](gui.md#composer) | `src/components/Composer.tsx` | 4.4 | done |
| Settings tab opening on Appearance, with immediate changes and automatic saving | [gui.md](gui.md#settings) | `src/components/Config.tsx`, `src/lib/store.tsx` | 4.5 | done |
| Copy CLI setup command, ready to paste and run | [gui.md](gui.md#copy-cli-setup) | `src/components/Config.tsx` | 4.5 | done |
| OS notifications | [gui.md](gui.md#notifications) | `src-tauri/notification.rs` | 4.6 | done |
| Update check and packaging | [app.md](#install) | `hiveme-core::session::update` (was `src-tauri/update.rs`) | 5.1 | done |
| Native hmc-to-hmg end-to-end regression | [development.md](../development.md#native-end-to-end-test) | `scripts/ts/test-e2e.ts`, Linux CI | 5.2 | done |
| Documentation and onboarding | [README](../../README.md) | `README.md`, `docs/` | 5.2 | done, except the screenshots |
| Encryption | [message.md](message.md#encryption) | `hiveme-core::crypto` | 6 | designed, types and parsing in place |
| REST API client | [hivemq-cloud.md](hivemq-cloud.md#rest-api) | `hiveme-core::cloud` | 6 | designed |
| Frontend localization in nine languages | [gui.md](gui.md#languages) | `src/i18n`, `src/components`, `src/lib/format.ts` | 6 | done |
| Review 01 fixes: connection identity/retries, indexed history, responsive UIs, shared helpers and regression coverage | [config.md](config.md), [session.md](session.md), [gui.md](gui.md), [tui.md](tui.md) | workspace | Review 01 | done |
| Startup recreates corrupt or incompatible development history while preserving config | [gui.md](gui.md#storage), [session.md](session.md) | `hiveme-core::storage` | Review 01 | done |
| Editor settings in `hmg` control five input assistance options, all off by default; settings categories reordered in both apps | [gui.md](gui.md#settings), [config.md](config.md), [tui.md](tui.md#settings) | `src/App.tsx`, `src/components/Config.tsx`, `hiveme-core::config`, `crates/hmc/src/tui/settings` | Input behavior | done |
| Independent OS and topmost notification channels, one shared latest-message window, native delivery errors, per-rule OS and topmost selection, four enabled built-ins, and four-row rule editors | [gui.md](gui.md#notifications), [tui.md](tui.md#notifications), [session.md](session.md#notifications) | `hiveme-core::desktop`, `src-tauri/src/notification_host.rs`, settings in both apps | Notifications | done |
| HiveMe icons in OS and topmost notifications; macOS delivery uses the app's identity and reuses its signed bundle, with the same identity for unbundled builds; topmost Close label follows the saved language in all nine catalogs | [gui.md](gui.md#notifications), [session.md](session.md#notifications) | `hiveme-core::desktop`, `hiveme-core::session`, `src/components/TopmostNotification.tsx` | Notification presentation | done |
| macOS OS delivery remains available while the notification UI thread is busy | [gui.md](gui.md#platform-notes) | `src-tauri/src/notification_host.rs`, native desktop regression test | Notification delivery | done |
| macOS registers HiveMe for OS notifications: the app bundle is ad-hoc signed, an app without a valid bundle signature delivers from the signed cached bundle, and a refused registration is reported as such rather than as denied permission | [gui.md](gui.md#platform-notes) | `src-tauri/tauri.conf.json`, `hiveme-core::desktop`, `src-tauri/src/notification_host.rs` | Notification registration | done |
| Clearing a topic deletes it and all its subtopics from both the topics and messages tables, and a cleared selection moves to its nearest remaining ancestor | [gui.md](gui.md#storage), [session.md](session.md#operations), [tui.md](tui.md) | `hiveme-core::storage`, `src/lib/store.tsx`, `crates/hmc/src/tui/app.rs` | Subtree clearing | done |

The rows below are the phases of [the terminal UI plan](../plans/plan-terminal-ui.md).
Their phase numbers are that plan's, not the initialization plan's.

| Feature | Spec | Module | Phase | Status |
|---------|------|--------|-------|--------|
| Terminal UI and shared session specified | [tui.md](tui.md), [session.md](session.md) | `docs/specs` | TUI 0 | done |
| Shared session: `hmg` on `hiveme_core::session`, `Role::Tui`, the `Toaster` split | [session.md](session.md) | `hiveme-core::session`, `src-tauri` | TUI 1 | done |
| SQLite busy timeout for two processes on one database | [session.md](session.md#two-processes-one-installation), [gui.md](gui.md#storage) | `hiveme-core::storage` | TUI 1 | done |
| Setup string carries the language; `hmc --init` applies it | [config.md](config.md#the-setup-string), [cli.md](cli.md#setting-up) | `hiveme-core::config::init`, `crates/hmc` | TUI 2 | done |
| Shared catalogs in `locales/`, `hiveme_core::i18n`, translated `hmc` lines and help | [cli.md](cli.md#languages), [tui.md](tui.md#languages), [gui.md](gui.md#languages) | `locales`, `hiveme-core::i18n`, `crates/hmc`, `src/i18n` | TUI 2 | done |
| Terminal UI shell: trigger, toolbar, tabs, footer, snackbar, help, theme, keys, OS notifications, update notice | [tui.md](tui.md), [cli.md](cli.md#interactive-mode) | `crates/hmc/src/tui` | TUI 3 | done |
| Terminal UI Messages tab: topic tree, message view, composer | [tui.md](tui.md#topic-tree) | `crates/hmc/src/tui/messages` | TUI 4 | done |
| Terminal UI Settings and About tabs | [tui.md](tui.md#settings) | `crates/hmc/src/tui/settings`, `crates/hmc/src/tui/about.rs` | TUI 5 | done |
| Terminal UI end-to-end test, performance tests, hardening, onboarding | [tui.md](tui.md#tests) | `crates/hmc/tests/tui.rs`, `crates/hmc/src/tui/tests/performance.rs`, `docs`, `README.md` | TUI 6 | done |
| Two processes on one database each show and notify what arrives | [session.md](session.md#two-processes-one-installation) | `hiveme-core::session`, `hiveme-core::storage` | TUI 6 | done |

## Build and release

| Command | Effect |
|---------|--------|
| `cargo build --workspace` | Build every Rust crate; `hmg` needs `pnpm tauri build` to be usable |
| `cargo test -r --workspace` | Run the Rust tests |
| `cargo fmt --check` and `cargo clippy --workspace -- -D warnings` | Lint |
| `cargo xtask schema` and `cargo xtask check-spec` | Regenerate schemas, validate spec examples |
| `cargo xtask icon` | Give the built binaries their Finder icon, on macOS |
| `pnpm install`, `pnpm typecheck`, `pnpm test` | Frontend dependencies, types, tests |
| `pnpm tauri dev` and `pnpm tauri build` | Run and bundle the GUI |
| `deno task -c scripts/ts/deno.json version` | Bump the version across the project |

A GUI binary comes from the Tauri CLI and nowhere else: it passes the
`tauri/custom-protocol` feature that compiles the frontend into the binary, and cargo
does not, so a `cargo build` of `hmg` leaves an executable that expects the Vite server.
See [gui.md](gui.md#build-and-run).

Every bundle carries `hmc` beside `hmg`, taken from `target/release/hmc`, so
`cargo build -r -p hmc` runs before `pnpm tauri build`: the Tauri hook commands do it,
and the workflows have a step for it. See
[development.md](../development.md#packaging-hmc).

One GitHub Actions workflow per OS runs the lint, check, test, and build steps and
uploads artifacts: `.deb`, `.rpm`, and `.AppImage` on Linux, `.dmg` on macOS (Intel
and Apple silicon), `.msi`, NSIS `.exe`, and a portable `.7z` on Windows, plus the
`hmc` binary on all three. The workflows run on every push that touches something
they build, so pushing a tag builds the same set of artifacts for that commit;
nothing is published automatically. A push that only edits markdown, the
documentation, or another workflow builds nothing, through the `paths-ignore` of the
reference project.

Releasing a version:

1. Edit the two version constants at the end of `scripts/ts/change-version.ts` and run
   `deno task -c scripts/ts/deno.json version`. That rewrites `package.json`, the
   workspace `Cargo.toml`, `src-tauri/tauri.conf.json`, and the `HIVEME_VERSION` of
   the three workflows. Every crate inherits the workspace version, which is also what
   `sender.appVersion` carries into every message.
2. Run any cargo command so that `Cargo.lock` picks the new version up, and commit.
3. Add the release notes, tag, and push. Take the artifacts from the three workflow
   runs and attach them to the GitHub release, which is what the update check in `hmg`
   reads.

## Install

A release installs only the application. The config, the history, and the update state
live elsewhere and survive an uninstall, which is what lets an upgrade keep a
configured cluster.

| OS | Where the programs land | Config and history |
|----|-------------------------|--------------------|
| Linux, deb and rpm | `/usr/bin/hmg` and `/usr/bin/hmc`, desktop entry in `/usr/share/applications/` | `$XDG_CONFIG_HOME/HiveMe/`, else `$HOME/.config/HiveMe/` |
| Linux, AppImage | wherever the file is, with `hmc` inside it | the same |
| macOS | `/Applications/HiveMe.app`, binaries at `Contents/MacOS/hmg` and `Contents/MacOS/hmc` | `$HOME/Library/Application Support/HiveMe/` |
| Windows, msi | `%ProgramFiles%\HiveMe`, both executables | `%APPDATA%\HiveMe\` |
| Windows, nsis | `%LOCALAPPDATA%\HiveMe`, both executables | `%APPDATA%\HiveMe\` |
| Windows, portable | wherever the archive is unpacked, both executables | beside the executable |

Windows tells an installed build from a portable one by where the executable is: under
`%LOCALAPPDATA%`, `%ProgramFiles%`, or `%ProgramFiles(x86)%` it is installed and uses
`%APPDATA%`; anywhere else it keeps the config beside itself. Both installers land in
one of those, so an installed `hmg` and an installed `hmc` share a config while a copy
on a memory stick carries its own. The rules are in
[config.md](config.md#location-and-precedence).

Every installer carries `hmc` beside `hmg`, because the two share a config file and
are of little use apart, and only the Linux packages land it on `PATH`. Each bundler
places it through a file map of its own, in `bundle.linux.*.files` and
`bundle.macOS.files`; Windows has no such map, so `bundle.resources` of
`src-tauri/tauri.windows.conf.json` does it there. `hmc` is published as a plain
executable for each platform as well, for a machine with no desktop to install a GUI
on. Full instructions in [installation.md](../installation.md).

## Deviations from the plan

Recorded so the plan and the tree can be reconciled later.

1. Resolved in step 4.1: `src-tauri` is a workspace member and the Cargo target
   directory is the repository root `target/`.
2. Every repository script is Deno TypeScript under `scripts/ts/`, so that the
   project stays cross platform. The plan wrote `scripts/check-spec-sync.sh`; there
   are no shell scripts. The set is `check-license-headers.ts`, `check-spec-sync.ts`,
   `gen-types.ts`, `change-version.ts`, and `test-e2e.ts`, plus the `scripts/license-header.txt`
   template. The `cargo xtask` alias needs `.cargo/config.toml`, and
   `schemas/README.md` keeps the generated directory present in a fresh clone.
3. Resolved in step 4.1: the workflows build and upload the bundles unconditionally.
   The guard that skipped them while `src-tauri` did not exist is gone.
4. Resolved: the workflows carry the `paths-ignore` of the reference project after
   all. The specifications are still load bearing, so their tagged examples are
   validated by the next push that touches code rather than by the one that edits
   them.
5. `cargo xtask` needs a library target as well as a binary, so that the checks and the
   tests in `xtask/tests/` run the same code.
6. The `hiveme-core` tests keep their fixtures in
   `crates/hiveme-core/tests/fixtures/`, split into `config/`, `message/`, and
   `mqtt/`. The validation rules are exercised from a table in the test rather than
   from one file per rule, and a single `config/invalid.json` proves that every
   problem is reported at once.
7. MQTT over WebSocket is behind the `websocket` feature of `hiveme-core`, which is
   off by default. The plan allowed it to slip to phase 6; the transport mapping is
   written and compiles under the feature, and a `wss://` URL without it fails with a
   message naming the feature. Turning it on by default waits for a workflow that
   builds and tests it.
8. `testcontainers` is taken with `default-features = false, features = ["aws-lc-rs"]`.
   Its default features pull in `rustls/ring`, and rustls refuses to pick a crypto
   provider when two are compiled in, which would make every TLS connection panic in a
   test build.
9. The integration tests replace the broker container rather than restarting it, on a
   host port the test picks, so that the client reconnects to a broker with no session
   for it and the resubscribe path is what carries the subscription across.
10. `hmc` uses `thiserror` rather than `anyhow`, against the convention for binaries.
    Its contract is the exit code table of [cli.md](cli.md#exit-codes), so the set of
    failures is closed and each one has to name its code; `anyhow` carries no such
    discriminant. There is one type, `Failure`, and it exists for that mapping alone.
11. The `hmc` integration tests give the anonymous test broker a username and password
    anyway. `Config::validate` insists on credentials, `MqttClient::connect` validates,
    and the HiveMQ CE allow-all extension accepts whatever it is sent, so the config a
    test writes is the shape a real one has.
12. `hiveme-core` chooses the rustls cryptography provider explicitly rather than
    letting rustls infer it. `hmg` reaches the GitHub releases API through `ureq`,
    which brings its own rustls with `ring`, so two providers are compiled into the
    same binary and rustls panics rather than guessing between them. `mqtt::tls`
    installs aws-lc-rs, the provider `rumqttc` is built against, once per process.
13. The topic tree is `SimpleTreeView` with hand-written `TreeItem` children rather
    than `RichTreeView`, which section 14 of the plan left to step 4.4. `TreeItem`
    takes a label of arbitrary content, which the unread badge needs, and a desktop
    client has few enough topics that virtualizing the tree would buy nothing. The
    message list is virtualized instead.
14. The composer stores its bubble once the broker has accepted the message, not
    before. The plan said the bubble appears immediately; a publish that failed would
    then leave a bubble claiming it was sent, and there is no event that could take it
    back. Reconciliation with the broker echo by message id is unchanged.
15. `set_notifications_paused` was added to the command list of
    [gui.md](gui.md#commands). The toolbar pause toggle is backend state, because the
    rules and the rate limiter are, and the plan's command list did not name it.
16. The frontend tests are configured in `vitest.config.ts` rather than in
    `vite.config.js`, so that the application build has nothing to do with the test
    environment. `hiveme-core` dev-depends on itself with `features = ["storage"]`,
    which is how its own tests reach a module that only `hmg` turns on.
17. On Windows, `hmg` raises its notifications through `tauri-winrt-notification`
    against an `AppUserModelId` it registers itself, rather than through the
    notification plugin. Windows labels a toast with the identity of the process that
    raised it, and without one of our own that label reads PowerShell. The approach is
    taken from the sibling project `../BatchMkvMerge`. Every other platform still goes
    through the plugin. See [gui.md](gui.md#platform-notes).
18. `hmc` has a build script, which the plan did not call for. `hmg` gets its icon and
    version information from `tauri_build`, and `hmc` has no equivalent, so
    `crates/hmc/build.rs` embeds them with `winresource` on Windows. It is the only
    thing that build script does, and on every other platform it does nothing. See
    [cli.md](cli.md#appearance).
19. The screenshots of step 5.2 are not taken. They need a desktop session, which the
    rest of the work did not, so [screenshots.md](../screenshots.md) carries the recipe
    for the state to shoot rather than the images. Everything else in 5.2 is done.
20. `src/App.test.tsx` mounts the whole application against a mocked backend. A
    frontend that fails after its first render leaves a window that is simply empty,
    while every effect has already run and the backend log reads normally, so the
    rendered output is the only thing that can tell the two apart. It is what step
    4.1 called the smoke test, written once there was a window to smoke test.
21. The troubleshooting section step 5.2 asked for is in
    [development.md](../development.md#troubleshooting) rather than in the README. The
    README is what a user reads to get their first message through, and a list of the
    ways a build, a certificate, or a client identifier can go wrong belongs with the
    other things a contributor needs. The README links to it.
22. The installers carry `hmc`, which step 5.1 did not ask for: it planned one
    executable per artifact and a separate download for the CLI. Two programs that
    share a config file and a message format are a pair, and a user who installs the
    desktop application should not have to fetch the other half by hand. The standalone
    `hmc` is still published, for machines with no desktop.

The entries below are against [the terminal UI plan](../plans/plan-terminal-ui.md).

23. Phase 1: the task that forwards session events to the frontend is
    `src-tauri/src/events.rs`, not a rewritten `mqtt.rs` as section 3.2 of the plan
    has it, because nothing in it is MQTT any more. `protocol.rs` keeps the
    `topic-added` and `notification-fired` payload structs beside the event names and
    `AppState`, since those payloads are the GUI's event shapes rather than session
    types, and `constants.rs` stays with `APP_NAME` alone.
24. Phase 1: `hmc` does not turn the `session` feature on yet. Nothing in `hmc` uses the
    session before the terminal UI of phase 3, so linking SQLite and `ureq` into the
    one-shot publisher now would only grow the binary; decision 1 of the plan is met in
    phase 3. `development.md` and [tui.md](tui.md#build-and-run) say so.
25. Phase 1: `Session` gains `with_parts`, to build a session on a config store and a
    store a caller already has, and `SHUTDOWN_TIMEOUT` both applications bound their quit with. `hiveme_core::Error`
    gains `InvalidTopic` beside the four variants the plan names, so the publish error
    for a topic the MQTT rules refuse keeps its wording. `set_notifications_paused`
    also raises a `Status` event, as session.md's event table says.
26. Phase 1: the Tauri notification plugin needs an `AppHandle`, and the session exists
    before the Tauri application does, so `TauriToaster` holds the handle in a
    `OnceLock` that `window.rs` fills during setup, before `start_background_work`
    opens the first connection. `window.rs` also enters the Tauri runtime around
    `start_background_work`, because the session spawns onto the runtime it is called
    from.
27. Phase 1, answered while it was built: the plan and [tui.md](tui.md#leaving-the-terminal-ui)
    gained a Quit tool on the toolbar and one quit path for every way out of the
    terminal UI, closing the terminal and `SIGTERM` included, so that the MQTT
    connection is closed and the broker session discarded however it ends. Decision
    26 and section 5.11 of the plan record it.
28. Phase 2: clap writes the `Usage`, `Arguments`, and `Options` headings and the
    descriptions of `--help` and `--version` in English itself, so `cli::command` gives
    every argument a heading from the catalog, puts the usage heading in the help
    template, and replaces the two built-in flags with equivalent ones; `clap` gains its
    `string` feature for the headings. The English rendering is byte for byte clap's
    default, which a test proves against the doc comments. clap's own parse errors and
    the placeholders of the usage line stay English, since they are clap's text rather
    than lines `hmc` authors; [cli.md](cli.md#languages) says so.
29. Phase 2: section 3.3 of the plan picks the language from "the config they loaded",
    but the help is printed before any config is loaded. `cli::config_argument` finds
    `--config` on the raw command line and `run::configured_locale` reads `gui.language`
    from that file, without writing it, before clap parses; a publish then switches to
    the language of the config it loaded, and `--init` reports in the language of the
    config it left.
30. Phase 2: the format functions are named after `src/lib/format.ts` (`integer`,
    `decimal`, `bytes`, `duration`, `time`, `date_time`, `day`, and `local` for the wall
    clock of a timestamp), and `Locale` carries `plural` and `plural_categories`, which
    section 3.3 leaves unnamed. The keys are `help.printHelp` and `help.printVersion`
    for the two built-in flags and `cli.*` for the lines, one per line;
    `--topic: <reason>` has no key, because its two halves are an option name and a
    core diagnostic. The Italian heading for positional arguments is `Parametri`, since
    `Argomenti` is that catalog's word for topics. A setup string with a blank
    `language` counts as one without it, and `BrokerInit` leaves the field out when it
    has no language, so the schema lets it be `null`.
31. Phase 3: `tui-textarea` 0.7 depends on ratatui 0.29 and `tui-tree-widget` was not
    needed yet, so `crates/hmc/src/tui/widgets/` holds an in-house one-line text input,
    the only form control phase 3 uses. The editor, the select popup, and the tree are
    written with the phases that use them.
32. Phase 3: the screens talk to the session through a `Service` trait in
    `crates/hmc/src/tui/service.rs`, which `Session` implements, so that the quit path
    can be tested against a session whose shutdown never finishes. Section 3.4 of the
    plan has no such file. `main.rs` only chooses the mode; the runtime, the log file,
    and the panic hook are set up in `tui/mod.rs`, which is where the terminal is.
33. Phase 3: `clipboard.rs` and the `arboard` dependency move to phase 4, since nothing in
    the shell copies; the copy actions of the chat view and Copy CLI setup are the
    first users. `portable-pty` arrives with the test of phase 6.
34. Phase 3: the toolbar has a rule above and below and no side borders, rather than the
    bordered block of section 5.1, because the seven English labels need 79 of the 80
    columns. Labels that do not fit are cut from the longest, down to the key alone.
    The labels are short `tui.toolbar.*` words rather than the GUI's tooltip keys, which
    are sentences.
35. Phase 3: until the tree and the chat view take the focus, `Tab` in the Messages tab
    reaches the footer's `config error` and `last error` entries so that `Enter` can open
    them, as section 5.8 asks. The About tab is drawn as text in phase 3 rather than
    left empty; phase 5 adds the gradient letters and the cards. The snackbar reports
    only failures until phase 4 brings the first confirmations.
36. Phase 3: the keyboard enhancement flags are requested only where crossterm reports
    support, which it never does on Windows, where the console API already reports the
    chords; this answers the open item of section 10. The glyph fallbacks are chosen by
    `TERM=linux` and by a Windows console without `WT_SESSION`. `--tui` on a redirected
    stdout is refused before the config path is resolved, so it writes nothing.
37. Phase 4: the topic tree is drawn by `messages/topic_tree.rs` itself rather than by
    `tui-tree-widget`, and `widgets/` gains a multi-line editor and `wrap` rather than
    `tui-textarea`, for the reason of entry 31. The JSON tree reads the payload again
    into its own order-preserving `Json` type, because `serde_json::Value` sorts object
    keys and the GUI shows them as written; `hmc` depends on `serde` for it. Turning on
    `serde_json`'s `preserve_order` instead would have changed the key order of every
    generated schema.
38. Phase 4: the composer's placeholder is `tui.composer.placeholder` and
    `tui.composer.placeholderJson`, the GUI's sentences with `Alt+Enter` in place of
    `Shift+Enter`, which a terminal without the keyboard protocol never reports. The help
    overlay's `tui.help.activate` is replaced by the keys of the Messages tab, and on a
    terminal too short for the list the overlay drops its blank lines, then puts the
    global keys beside the rest.
39. Phase 4: the footer's error entries stay in the focus ring, at its end after the
    composer, which replaces the interim of entry 35. Controls that cannot be used right
    now are skipped by `Tab`, as the GUI's disabled controls are. `Space` does what a
    click does on a button, a select, or a checkbox, which section 5.9 leaves unsaid for
    the composer, and `Left` and `Right` pick the QoS radio. The wheel scrolls the pane
    under the pointer rather than the focused one, and over the tree it moves the cursor,
    which the tree keeps in view.
40. Phase 4: the relative topic of the metadata row is muted, as gui.md's
    `text.disabled`, rather than the secondary color section 5.5 names, and the row runs
    on to the right of a bubble narrower than it instead of wrapping. A confirmation in
    the snackbar is drawn in the success color, which is what the GUI's filled `Alert`
    uses for anything but an error, rather than an info color. The detail view's cursor
    takes `Enter` as well as `Space`.
41. Phase 4: the plan's end-to-end test against the Docker broker is a unit test in
    `crates/hmc/src/tui/tests/messages.rs` rather than a file under `crates/hmc/tests/`,
    because it drives the application state, which an integration test of a binary
    crate cannot reach, and it publishes through the one-shot mode's `run::run` in
    process rather than as a child process. `tui/tests.rs` became `tui/tests/mod.rs`
    beside `tui/tests/messages.rs`, and the scripted session keeps its rows in an
    in-memory `Store`. `Service` gains `topic_tree` and `publish`. `arboard` is taken
    without its default image support and with the Wayland clipboard.
42. Phase 5: the plan's select popup, checkbox, radio row, number field, and editable
    table are not widgets of `widgets/` but rows of one form description,
    `settings/form.rs`, because only the Settings panels use them and a panel described
    once gives its drawing, its focus order, and its scrolling alike. A text field is one
    underlined row rather than the rounded block phase 3 drew, so more of a panel fits
    80 x 24, and a panel that still does not fit scrolls. A text field without the focus
    shows the start of its text, in the composer too, where it showed the end.
43. Phase 5: the Broker URL is saved with its protocol in front, as the GUI saves it,
    where phase 3 saved it as typed. `Enter` in any settings text field saves at once and
    moves on, which phase 3 did on the password alone. `Tab` stops on selects,
    checkboxes, and buttons as well as text fields, and skips a button that cannot be
    used. Copy CLI setup's tooltip is a hint line under the button. The catalogs lose
    `tui.settingsLater` and gain `tui.help.choose`, `tui.help.option`,
    `tui.help.scroll`, and `tui.help.open`, and the help overlay lists the keys of the
    About tab.
44. Phase 5: the category list and the panel are centered together, as the GUI centers
    its sidebar and panel, rather than the panel alone. The Theme select lists each
    palette in its own primary color. The About tab's two cards and the two names of its
    copyright line take the focus in the order they are drawn, and its block letters and
    a panel's scrollbar have no ASCII fallback. An edit made during a write is written as
    soon as the write finishes, as the GUI store's `flushConfig` loop does. `Service`
    gains `broker_init`, and `hiveme_core::config` exports `BrokerUrlParts`,
    `DEFAULT_SCHEME`, `Scheme::ALL`, and `Scheme::from_alias`; `BrokerUrl::parse` reads
    its scheme through `Scheme::from_alias`.
45. Phase 6: the end-to-end test reads the pseudo-terminal back into a screen with
    `vt100`, a dev-dependency the plan did not name, and answers the two queries a
    terminal answers and `vt100` does not: the cursor position ConPTY asks for before it
    passes anything on, and the device attributes crossterm asks for after the keyboard
    flags. On Unix it reads the terminal without blocking, through `libc`, so that
    closing the terminal can drop the reader's copy of it, which is what makes the kernel
    send `SIGHUP`, and it asks `tcgetattr` whether raw mode is on; `rumqttc` asks the
    broker whether the session is gone, as the session test does. The client identifier
    comes from `hmc.log`, which the test turns on with `--verbose`. The file runs on
    Windows through ConPTY as well, wherever Docker is, not only on the Linux workflow.
46. Phase 6: the plan's two-process check is part of that test rather than a check by
    hand with `hmg`. `hmg` is played by its session, `SessionApp::Gui`, in the test
    process, since everything `hmg` does between the broker and `HiveMe.db` is that
    session. The check found that whichever process stored a message second took the row
    for the echo of its own publish, so only one of the two applications showed the
    message live and raised its notification, and that two processes storing one message
    at the same moment could both find it missing, the second insert then failing on the
    unique key. The session now remembers the last 10,000 messages it stored or sent,
    duplicate deliveries within that session raise nothing, and `Store::insert` runs in one
    immediate transaction; [session.md](session.md#two-processes-one-installation)
    describes both. The notifier logs `rule <id> showed a notification for <topic>` at
    debug, which is how the test counts the toasts of the terminal UI.
47. Phase 6: closing the terminal under interactive `hmc` ended the broker session
    cleanly and then exited with code 101 on Linux: ratatui's `Terminal` shows the cursor
    again when it is dropped and reports a failure with `eprintln!`, which panics once
    the terminal is gone. The terminal is now never dropped, since `restore_terminal`
    already shows the cursor on every way out. The Linux run of the end-to-end test found
    it; on Windows the system ends the process after a console close either way.
48. Phase 6: on macOS `hmc` sets the bundle identifier of `hmg`, `com.caoccao.hiveme`,
    before its first toast, as the Tauri notification plugin does for `hmg`, rather than
    only recording the label an unbundled binary gets. Left alone, `mac-notification-sys`
    looks an application named `use_default` up with AppleScript and labels the toast
    Finder; with the identifier set, the label is HiveMe where HiveMe.app is installed
    and Terminal where it is not. That is read from the library's source and has not
    been seen on a Mac.
49. Phase 6: the benchmark is `crates/hmc/src/tui/tests/performance.rs`, in process on
    `TestBackend` like the other screen tests, with a budget per frame of 150 ms in a
    release build and 750 ms in a debug one. Resizing is tested there as a size change
    between two frames with input arriving in between, since the event loop draws
    synchronously and the size cannot change inside one draw.
50. Phase 6: the checks by hand the plan lists are not done: the terminal UI by eye in
    Windows Terminal and in the legacy Windows console, the toast label on Windows, and
    the toast label on macOS. That the toast of an unbundled binary appears at all is no
    longer among them: a macOS run of the end-to-end test has the toasts accepted and
    `com.caoccao.hiveme` taken as the identifier, which
    [tui.md](tui.md#notifications) records. A Linux desktop without a notification
    daemon is covered by the notifier's test of a refused toast and by the Linux run of
    the end-to-end test, which has no daemon and whose toasts are logged as refused while
    everything else goes on. [screenshots.md](../screenshots.md) has the recipe for
    `tui.png` but no image, as entry 19 has for the others. [todos.md](../todos.md) lists
    what is left.
51. Phase 6, found afterward: none of the tests that need a broker had ever run on Docker
    Desktop. Each asked for container port 1883, which the HiveMQ CE image exposes
    itself, so Docker published one port twice and the second bind failed with `address
    already in use`; a container that will not start is a skip, and a skip passes, so
    four suites read as green while doing nothing. They no longer name a port the image
    already names. Running the end-to-end test for the first time then found that its
    two-process check read the terminal UI's notifications after quitting it, which
    raced the slower of the two applications; it waits for them instead.
    [development.md](../development.md#testing-against-a-broker) says how to tell a
    skipped suite from a passing one.
52. Phase 6, found afterward: `hmg` drew what `hmc` had sent on the outgoing side. The
    column the database keeps says that this *installation* published a row, and the two
    applications share one `HiveMe.db`, so each read the other's messages as its own. It
    was asymmetric in practice only because the flag is latched and `hmc` usually won the
    insert, so `hmc` happened to read `hmg`'s messages correctly and `hmg` did not; read
    back from the database after a restart, both were wrong. The side is now decided by
    `MessageRow::seen_by`, which asks the narrower question the view means, and the
    `From<StoredMessage>` that had no application to ask is gone.
    [session.md](session.md#which-side-a-message-is-on) has the rule.
53. Phase 6, found afterward: on macOS the built `hmc` and `hmg` both showed Finder's
    generic `exec` icon. An icon there belongs to a bundle, which `HiveMe.app` has and a
    bare binary does not, so [cli.md](cli.md#appearance) had recorded that there was
    nothing to be done. macOS does let a plain file carry its own icon, so `cargo xtask
    icon` now writes one into each binary after the release builds, and
    `crates/hmc/icons/hmc.icns` was packed from the existing `hmc.png` to give `hmc` its
    own. The plan never asked for this; the pair looking like unidentified executables in
    Finder is the reason.
54. Phase 6, found afterward: the `hmg` window opened where macOS had centered it and
    jumped to its remembered position a frame later. `setup` restored the geometry the
    obvious way, by moving the window and then showing it, and on macOS that order does
    not hold: tao defers a move to the main dispatch queue because `NSWindow` frames are
    not thread safe, while a `show` from the main thread runs inline, and `setup` is on
    the main thread before the event loop turns. The geometry and the title are now
    written into the window's configuration by `window::place` before the builder creates
    the window, so there is no second place to draw it in.
    [gui.md](gui.md#window) has the rule.
55. Phase 6, found afterward: an unread badge outlived the messages it counted. Pruning
    deleted rows and left `topics.unread` where it was, so a topic that had been emptied
    by the retention window still showed a count, and nothing could ever clear it but
    opening that topic. The counts now come down with the rows, in the same transaction,
    under [session.md](session.md#history).
56. Phase 6, found afterward: a message could be drawn twice, and the second copy on the
    wrong side. Two causes, both of them the session not recognizing its own message
    coming back: a raw publish has no envelope, so the echo was given a second generated
    id and stored beside the row it was a copy of, and a message was spoken for only
    after it had been stored, so an echo that beat the acknowledgement back arrived at a
    session that had never heard of it. A raw publish now carries a unique MQTT publish ID and
    every publish is claimed before it is sent, under
    [session.md](session.md#publishing). The duplicate also raised a desktop notification
    for the user's own message. Matching raw payload bytes was subsequently replaced
    by the publish ID so another session's identical bytes remain eligible for rules.
57. Phase 6, found afterward: three ways a save could lose settings, all in the writer.
    Every writer used one temporary file name, so `hmc` and `hmg` saving at the same
    moment truncated and interleaved into a document neither meant to write, which the
    rename then published as the config; an array was replaced wholesale, so a key this
    build does not know inside a notification rule was dropped by a save of an unrelated
    preference, which is the one thing the merging writer exists to prevent; and the new
    config was held in memory before the write, so a save the file system refused left
    the screen showing settings the next start would not find.
    [config.md](config.md#location-and-precedence) and
    [config.md](config.md#versioning-and-compatibility) have the rules.
58. Phase 6, found afterward: the Settings tab could not be saved until the broker was
    filled in. Saving ran the same validation as connecting, so on a fresh install a
    theme, a language, or a notification rule was refused for the address and the
    credentials the user had not typed yet. Saving now validates everything but the
    broker login, and the connection that follows validates in full, so the address is
    still reported by the attempt that found out.
    [config.md](config.md#validation) has the split.
59. Phase 6, found afterward: every terminal UI reconnect asked the broker to discard its
    session, and with it the messages the broker had held while the network was down.
    Clean start had been set for `Role::Tui` on the reasoning that a fresh client
    identifier has no session to resume, which is true of the first connection and of no
    other: the flag is on the CONNECT packet, and the client sends the same packet every
    time it reconnects. [hivemq-cloud.md](hivemq-cloud.md#role) has the corrected table;
    the row in [the terminal UI plan](../plans/plan-terminal-ui.md) is the original
    reasoning.
60. Phase 6, found afterward: `broker.keepAliveSecs` below five panicked both
    applications. `rumqttc` asserts on a shorter keep alive, and nothing between the
    config file and that assert looked at the value. It is validated now, and clamped
    again where the connection is built, because a caller may skip validation and a
    single line of a config file must not be able to take the application down.
61. Phase 6, found afterward: a publish could return success on an acknowledgement that
    belonged to another message. `rumqttc` picks the packet identifier inside the event
    loop, so callers are paired with outgoing events in the order they were sent, and
    that pairing held only while every request produced exactly one event. A reconnect
    breaks it in both directions: with the session resumed the client sends
    unacknowledged publishes again, and without it the client drops what it was holding.
    Either one moves the queue a step out of line and every publish after it answers to
    the message behind it. [hivemq-cloud.md](hivemq-cloud.md#publishing-and-subscribing)
    has the rule.
62. Phase 6, found afterward: a topic with thousands of levels aborted the process with a
    stack overflow. The topic tree is built, converted, serialized, rendered, and dropped
    by recursion, one frame per level, and a topic may have as many levels as fit in its
    65,535 bytes. It arrives from the broker, so nothing local decides how deep it goes.
    The tree stops at 64 levels and the rest of the topic becomes one node, which still
    carries the whole topic. [session.md](session.md#history) has the bound.
