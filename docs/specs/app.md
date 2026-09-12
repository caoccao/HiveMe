# App

HiveMe is a Rust project with two applications built on top of MQTT, sharing one
config file and one message format.

1. **HiveMe CLI** (`hmc`) sends a message to the broker from a shell or a script.
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
| [cli.md](cli.md) | `hmc`: usage, behaviour, exit codes |
| [gui.md](gui.md) | `hmg`: layout, notifications, storage, IPC, settings, window |
| [hivemq-cloud.md](hivemq-cloud.md) | What the broker offers, how HiveMe connects, and the REST API |

The implementation plan is [docs/plans/plan-initialization.md](../plans/plan-initialization.md).

## Applications

### HiveMe CLI

`hmc` publishes one message and exits.

- `hmc --help` or `hmc -h` shows the help.
- `hmc <message>` sends to the default topic.
- `hmc -t <topic> <message>` sends to a topic.
- `echo <message> | hmc` reads the body from stdin.

Full reference in [cli.md](cli.md).

### HiveMe GUI

`hmg` uses Tauri and React. The main window has a toolbar at the top, a status bar in
the footer, a topic tree on the left, and a message view on the right. The message
view is a chat: bubbles for the selected topic, with an input box and a send button at
the bottom.

The rule based notification system ships with three built-in rules:

| Topic | Notification |
|-------|--------------|
| `info` | info message |
| `warn` | warning message |
| `error` | error message |

Rules are configurable. Full reference in [gui.md](gui.md).

## Topics

Topics are namespaced under a configurable prefix, `hiveme` by default. The built-in
rules therefore match `hiveme/info`, `hiveme/warn`, and `hiveme/error`, and `hmc`
publishes to `hiveme/info` when no topic is given. Setting `topics.prefix` to an empty
string puts the topics at the root instead. Topic arguments and rule filters are
relative to the prefix unless explicitly marked absolute. See
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
| 1a | Where a cluster is configured | In `hmg`. Its Settings tab takes the URL, username, and password, and renders them as a one line setup string. `hmc --init '<json>'` turns that string back into a config, so `hmc` needs nothing typed into it. See [config.md](config.md#the-setup-string). |
| 1b | TLS | Automatic and not configurable for the supported plan. A Serverless cluster chains to a public authority the OS already trusts, so neither application generates, enrols, or asks about a certificate. `broker.tls` remains for a local test broker. |
| 2 | Config location and password storage | The per-OS config directory, with `--config` and `HIVEME_CONFIG` overrides. The password is plain text in the file, mode 0600 on Unix, with `passwordRef` reserved for the OS keychain. |
| 3 | Producers and GUI strictness | Producers are the HiveMe tools plus the user's own scripts. The GUI is lenient: it parses the envelope when valid and otherwise shows the raw payload. |
| 4 | Encryption key model | A symmetric pre-shared key, AES-256-GCM, HKDF derived, with a key id for rotation. Designed now, implemented in phase 6. |
| 5 | Profiles | One `broker` object. The config is versioned so a `profiles` map can be added later. |
| 6 | Topic layout | A configurable prefix, `hiveme` by default, with a configurable default topic. |
| 7 | Message history | Persisted in SQLite next to the config, bounded per topic and by retention days. |
| 8 | Notification rules | Configurable, with the three built-in rules as defaults. |
| 9 | Schema source of truth | Rust types with `serde` and `schemars` generate the JSON schemas. Tests fail when the committed schemas are stale. TypeScript types are generated from those schemas. |
| 10 | Spec files | Split by concern, as listed above. |
| 11 | Frontend stack | Vite, React 19, TypeScript, MUI with `@mui/x-tree-view`, Zustand, react-i18next, pnpm. |
| 12 | Tests and CI | Integration tests against a local HiveMQ CE container, skipped without Docker. One GitHub Actions build workflow per OS. |
| 13 | CLI scope for phase 1 | Message argument, `-t`, stdin, and `--json`. No subcommands. |
| 14 | MQTT version | MQTT 5, through `rumqttc`. |
| 15 | GUI publishing | Inside the message view, chat style, with an input box and a send button at the bottom. |
| 16 | Payload fields | `title`, `body`, `level`, a free-form `data` object, and `sender` in the envelope. |
| 17 | Reference architecture | The UI layout and Tauri architecture follow the sibling project `../BetterMediaInfo`. See below. |

## Reference architecture

HiveMe mirrors `../BetterMediaInfo` so that both projects can be maintained with one
set of habits.

| BetterMediaInfo | HiveMe | Notes |
|-----------------|--------|-------|
| `src-tauri/` as a single Cargo package | `src-tauri/` as the package `hmg` inside a root workspace | The workspace is needed because `hmc` and `hiveme-core` are separate crates. The Cargo target directory moves to the repository root. |
| `lib.rs` with alphabetised `#[tauri::command]` wrappers, `convert_error`, and a `run()` that hands Tauri a tokio runtime | the same | Commands delegate to `controller.rs`. Logging through `log` and `env_logger`, controlled by `RUST_LOG`. |
| `controller.rs` holds the business logic | `controller.rs` orchestrates only | The logic lives in `hiveme-core` so `hmc` shares it. |
| `protocol.rs` and `protocol.ts` hand-synced | the same, for IPC-only types | Config and message types are generated from the schemas instead. |
| `config.rs` with `#[serde(default)]`, camelCase keys, `OnceLock<RwLock<Config>>`, and `<App>.json` in the per-OS config directory | a thin wrapper over `hiveme_core::config` | The file is `HiveMe/HiveMe.json`. `hmc` uses the same resolution code. |
| `window.rs` with `setup` and `on_window_event` | the same | Window state lives in `gui.window`. |
| `constants.rs` with `APP_NAME` | the same, `APP_NAME = "HiveMe"` | |
| Plugins: dialog, clipboard-manager, opener | the same plus notification | The notification plugin drives OS notifications. |
| `App.tsx` with `ThemeProvider`, display modes, twenty palettes, compact defaults | the same | |
| `Layout.tsx` grid `auto 1fr auto` with Toolbar, MainContent, Footer | the same | The footer is the status bar; the copyright moves to the About tab. |
| `MainContent.tsx` tabs with `ControlStatus` and keyboard shortcuts | the same | Tab 0 is the fixed Messages tab. |
| `Toolbar.tsx` icon button groups with tooltips | the same | |
| `NotificationSnackbar.tsx` driven by the store | the same | |
| `lib/store.tsx` Zustand, `lib/service.ts` invoke wrappers, `lib/constants.ts`, `lib/format.ts` | the same | Components never call Tauri APIs directly. |
| `src/i18n` with react-i18next and nine locales | the same structure, `en-US` only in phase 1 | |
| `Config.tsx` settings tab with `SectionHeader` sections | the same, with HiveMe's sections | |
| `About.tsx` | the same | |
| Update check against GitHub releases | the same, for `caoccao/HiveMe` | |
| Three per-OS build workflows | the same, plus lint, test, and spec checks | |
| `scripts/ts/change-version.ts` with Deno | the same | |
| Apache-2.0 header on every source file, rustfmt `max_width = 120` and `tab_spaces = 2`, edition 2024, a pinned toolchain | the same | |

## Repository layout

The target layout. Directories that belong to a later phase, such as `src-tauri/`
and `src/components/`, are listed here but do not exist yet; see
[Status](#status) for what is built.

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
    i18n/                         # index.ts, locales/en-US.json
  src-tauri/                      # Tauri 2 app, package `hmg`, lib `hmg_lib`, binary `hmg`
    Cargo.toml, tauri.conf.json, build.rs, capabilities/default.json, icons/
    src/                          # main.rs, lib.rs, controller.rs, protocol.rs, config.rs, constants.rs,
                                  # window.rs, mqtt.rs, notification.rs, storage.rs, update.rs
  crates/
    hiveme-core/                  # config, message, topic, rules, mqtt, storage (feature), cloud (feature, later)
    hmc/                          # CLI binary `hmc`
      build.rs, icons/            # the Windows executable icon and version information
  xtask/                          # `cargo xtask schema`, `cargo xtask check-spec`
  schemas/                        # broker-init, config, message .schema.json (generated, committed), README.md
  scripts/
    ts/                           # Deno scripts: deno.json, deno.lock, change-version.ts,
                                  # check-license-headers.ts, check-spec-sync.ts, gen-types.ts
    license-header.txt
  docs/
    specs/                        # app.md, config.md, message.md, cli.md, gui.md, hivemq-cloud.md
    plans/                        # plan-initialization.md
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
| 6 | `scripts/ts/check-spec-sync.ts` fails when the config, message, MQTT, CLI, or IPC code changes without a matching change under `docs/specs/`, and when `protocol.rs` changes without `protocol.ts` | `scripts/ts/` |
| 7 | The status table below records what is built | this file |
| 8 | Definition of done for every step: code, tests, spec update, regenerated schemas, a status table row, and a release note for user visible changes | the plan |

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
| Config load, save, migrate | [config.md](config.md) | `hiveme-core::config` | 1.1 | done |
| Message envelope and parser | [message.md](message.md) | `hiveme-core::message` | 1.2 | done |
| Topic resolution and filters | [config.md](config.md#topic-resolution) | `hiveme-core::topic` | 1.3 | done |
| Notification rule engine | [gui.md](gui.md#notifications) | `hiveme-core::rules` | 1.3 | done |
| Schema and spec tooling | [app.md](#spec-sync) | `xtask`, `scripts` | 1.4 | done |
| MQTT client | [hivemq-cloud.md](hivemq-cloud.md#how-hiveme-connects) | `hiveme-core::mqtt` | 2.1 | done |
| CLI | [cli.md](cli.md) | `crates/hmc` | 3.1 | done |
| Tauri scaffold | [gui.md](gui.md#build-and-run) | `src-tauri`, `src` | 4.1 | done |
| Message storage | [gui.md](gui.md#storage) | `hiveme-core::storage` | 4.2 | done |
| Backend commands and events | [gui.md](gui.md#ipc) | `src-tauri` | 4.3 | done |
| Messages tab | [gui.md](gui.md#layout) | `src/components` | 4.4 | done |
| Settings tab | [gui.md](gui.md#settings) | `src/components/Config.tsx` | 4.5 | done |
| OS notifications | [gui.md](gui.md#notifications) | `src-tauri/notification.rs` | 4.6 | done |
| Update check and packaging | [app.md](#install) | `src-tauri/update.rs` | 5.1 | done |
| Documentation and onboarding | [README](../../README.md) | `README.md`, `docs/` | 5.2 | done, except the screenshots |
| Encryption | [message.md](message.md#encryption) | `hiveme-core::crypto` | 6 | designed, types and parsing in place |
| REST API client | [hivemq-cloud.md](hivemq-cloud.md#rest-api) | `hiveme-core::cloud` | 6 | designed |
| Additional locales | [gui.md](gui.md) | `src/i18n` | 6 | designed |

## Build and release

| Command | Effect |
|---------|--------|
| `cargo build --workspace` | Build every Rust crate |
| `cargo test -r --workspace` | Run the Rust tests |
| `cargo fmt --check` and `cargo clippy --workspace -- -D warnings` | Lint |
| `cargo xtask schema` and `cargo xtask check-spec` | Regenerate schemas, validate spec examples |
| `pnpm install`, `pnpm typecheck`, `pnpm test` | Frontend dependencies, types, tests |
| `pnpm tauri dev` and `pnpm tauri build` | Run and bundle the GUI |
| `deno task -c scripts/ts/deno.json version` | Bump the version across the project |

One GitHub Actions workflow per OS runs the lint, check, test, and build steps and
uploads artifacts: `.deb`, `.rpm`, and `.AppImage` on Linux, `.dmg` on macOS (Intel
and Apple silicon), `.msi`, NSIS `.exe`, and a portable `.7z` on Windows, plus the
`hmc` binary on all three. The workflows run on every push, so pushing a tag builds
the same set of artifacts for that commit; nothing is published automatically.

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

| OS | Where the GUI lands | Config and history |
|----|---------------------|--------------------|
| Linux, deb and rpm | `/usr/bin/hmg`, desktop entry in `/usr/share/applications/` | `$XDG_CONFIG_HOME/HiveMe/`, else `$HOME/.config/HiveMe/` |
| Linux, AppImage | wherever the file is | the same |
| macOS | `/Applications/HiveMe.app`, binary at `Contents/MacOS/hmg` | `$HOME/Library/Application Support/HiveMe/` |
| Windows, msi | `%ProgramFiles%\HiveMe` | `%APPDATA%\HiveMe\` |
| Windows, nsis | `%LOCALAPPDATA%\HiveMe` | `%APPDATA%\HiveMe\` |
| Windows, portable | wherever the archive is unpacked | beside the executable |

Windows tells an installed build from a portable one by where the executable is: under
`%LOCALAPPDATA%`, `%ProgramFiles%`, or `%ProgramFiles(x86)%` it is installed and uses
`%APPDATA%`; anywhere else it keeps the config beside itself. Both installers land in
one of those, so an installed `hmg` and an installed `hmc` share a config while a copy
on a memory stick carries its own. The rules are in
[config.md](config.md#location-and-precedence).

`hmc` is not inside the installers. It is published as a plain executable for each
platform, and the Windows portable archive is the one artifact carrying both programs.
Full instructions in [installation.md](../installation.md).

## Deviations from the plan

Recorded so the plan and the tree can be reconciled later.

1. Resolved in step 4.1: `src-tauri` is a workspace member and the Cargo target
   directory is the repository root `target/`.
2. Every repository script is Deno TypeScript under `scripts/ts/`, so that the
   project stays cross platform. The plan wrote `scripts/check-spec-sync.sh`; there
   are no shell scripts. The set is `check-license-headers.ts`, `check-spec-sync.ts`,
   `gen-types.ts`, and `change-version.ts`, plus the `scripts/license-header.txt`
   template. The `cargo xtask` alias needs `.cargo/config.toml`, and
   `schemas/README.md` keeps the generated directory present in a fresh clone.
3. Resolved in step 4.1: the workflows build and upload the bundles unconditionally.
   The guard that skipped them while `src-tauri` did not exist is gone.
4. The workflows do not use `paths-ignore`, unlike the reference project, because the
   specifications are load bearing here and their examples are validated in CI.
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
    client has few enough topics that virtualising the tree would buy nothing. The
    message list is virtualised instead.
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
