# Development

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | pinned by `rust-toolchain.toml` (1.96.0) | `rustup` installs it automatically |
| Node.js | 24 | |
| pnpm | 11 | |
| Deno | 2.x | Runs every repository script under `scripts/ts/` |
| Docker | any recent | For MQTT integration tests and native end-to-end tests |

Unlike the reference project, HiveMe has no native library to build first. Clone and
build.

### Linux packages

Tauri needs the WebKit development packages:

```sh
sudo apt-get update
sudo apt-get install -y libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libwebkit2gtk-4.1-dev librsvg2-dev
```

## Commands

Building from source is these and nothing else. There is no native library to fetch or
compile first, and no generated file to produce by hand.

```sh
# Rust
cargo build --workspace
cargo test --workspace                          # -r for release mode
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings

# Repository automation
cargo xtask schema                              # regenerate schemas/ from the Rust types
cargo xtask check-spec                          # validate the tagged examples in docs/specs/
deno task -c scripts/ts/deno.json check              # type check the scripts themselves
deno task -c scripts/ts/deno.json check-license-headers
deno task -c scripts/ts/deno.json check-spec-sync [base-ref]

# Frontend
pnpm install
pnpm typecheck
pnpm test
pnpm test:e2e                                  # native apps + local broker; setup below
pnpm gen:types
pnpm dev                                        # vite on http://localhost:1420
pnpm tauri dev                                  # the GUI with hot reload
pnpm tauri build                                # the release bundle for this OS
```

`pnpm test` runs the frontend tests with vitest, configured in `vitest.config.ts`.

On Windows, `cargo build -r -p hmc` comes before anything that compiles `hmg`, because
the bundle carries `hmc`. See [Packaging `hmc`](#packaging-hmc).

## Running `hmg`

```sh
pnpm tauri dev                         # what to use while working on it
pnpm tauri build                       # the installers for this OS
pnpm tauri build --debug --no-bundle   # a target/debug/hmg.exe that runs on its own
```

**A cargo build does not produce a usable GUI, in either profile.** `tauri` chooses
between the dev server and a frontend compiled into the binary from its
`custom-protocol` feature, not from the profile, and the Tauri CLI is what turns that
feature on: every `pnpm tauri build` passes `--features tauri/custom-protocol` to
cargo, and cargo on its own never does. So the `target/release/hmg.exe` that
`cargo build -r --workspace` leaves behind loads the `devUrl` of `tauri.conf.json`,
the Vite server at `http://localhost:1420`, and opens on
*"localhost refused to connect"* when nothing is listening there. The window is the
only place that says so: the backend starts, connects, and logs exactly as it does in
a bundled build.

`cargo build -r --workspace` is still how `hmc` is built and how `hmg` is compile
checked, and the Rust tests need nothing else. It is simply not how the GUI is built.

The window is a WebView2 (Windows), WebKitGTK (Linux), or WKWebView (macOS) surface,
so a frontend failure looks like an empty window rather than a crash, and the backend
log still reads perfectly normally. `src/App.test.tsx` covers initial rendering in
jsdom with mocked IPC. The native [end-to-end test](#native-end-to-end-test) checks
the built frontend together with real IPC, MQTT delivery, and SQLite history.

## Running `hmc`

A cluster is set up in `hmg`, which shows a one line setup string to paste into `hmc`:

```sh
cargo run -p hmc -- --config ./HiveMe.json --init '{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"hiveme-sam","password":"s3cret","language":"en-US"}'
cargo run -p hmc -- --config ./HiveMe.json "hello"
echo hello | cargo run -p hmc -- --config ./HiveMe.json
cargo run -p hmc -- --help
```

Press **Copy CLI setup** in the Broker section of the `hmg` Settings tab to get the
string; the format is in [specs/config.md](specs/config.md#the-setup-string).
Publishing before any `--init` writes a default config and exits 3. The help and the
lines `hmc` prints follow `gui.language` of the config, so `--help` against a German
config is German. Full reference in [specs/cli.md](specs/cli.md).

## Packaging `hmc`

Every installer carries `hmc` beside `hmg`, so that installing the desktop application
installs the command line one, reading the config file the desktop one writes. What
goes in is `target/release/hmc`, which is what `cargo build -r -p hmc` leaves behind,
and nothing renames or stages it on the way.

`pnpm tauri dev` and `pnpm tauri build` run that build themselves, from
`beforeDevCommand` and `beforeBuildCommand`, so a bundle needs no extra step. The
workflows run it as a step of their own, before anything that compiles `hmg`, and the
`hmc` they publish on its own is that same file.

Each bundler is told where to put it:

| Bundle | Where it is configured | Where `hmc` lands |
|--------|------------------------|-------------------|
| deb, rpm | `bundle.linux.deb.files`, `bundle.linux.rpm.files` | `/usr/bin/hmc`, on `PATH` |
| AppImage | `bundle.linux.appimage.files` | `usr/bin/hmc`, inside the image |
| macOS app | `bundle.macOS.files` | `HiveMe.app/Contents/MacOS/hmc` |
| msi, nsis | `bundle.resources` of `src-tauri/tauri.windows.conf.json` | beside `hmg.exe` in the install folder |

Windows is the odd one out, because its bundlers have no file map of their own.
`bundle.resources` does the same job there, and it sits in the Windows only config file
rather than in `tauri.conf.json` because it is not a per-platform setting: in the shared
config it would put a second copy of `hmc` inside the macOS bundle and the Linux
packages as well.

That has one consequence on Windows. `tauri-build` reads `resources` from the build
script of `hmg`, so `target/release/hmc.exe` has to be there before anything compiles
`hmg`, `cargo clippy --workspace` and `cargo test --workspace` included. Run
`cargo build -r -p hmc` first and the rest follows; on Linux and macOS the file maps
are read by the bundler alone, so it only has to exist before `pnpm tauri build`.

## Logging

`hmc` prints warnings to stderr and nothing to stdout. `--verbose` adds the connection
details, and `RUST_LOG` overrides both.

```sh
RUST_LOG=debug cargo run -p hmc -- "hello"      # Unix
set RUST_LOG=debug                              # Windows
```

## Troubleshooting

**The GUI says disconnected, and the error mentions the password.** Credentials take up
to a minute to become active after you create them in the console. If it persists,
check the username and password in Settings, and that they belong to the cluster in the
URL.

**The connection fails during the TLS handshake.** The cluster hostname has to be the
one the console shows, because HiveMQ Cloud selects the certificate from the name the
client sends during the handshake (SNI). An IP address, or a hostname with the port
left in it, will not work. HiveMQ Cloud accepts TLS only, so the URL starts with
`mqtts://`, never `mqtt://`.

**Messages arrive and then the connection drops, over and over.** Two clients cannot
share a client identifier: the broker disconnects the older one, which reconnects, and
so on. HiveMe gives `hmg` and each `hmc` run different identifiers by construction, so
this usually means a second copy of `hmg` running against the same config, or another
MQTT client of yours using the same id. A Serverless cluster also allows 100
concurrent connections in total.

**`hmc` exits 3 and prints a config path.** There is no config yet, so it wrote a
default one. Run `hmc --init '<paste>'` with the string from the GUI.

**`hmc` exits 4 or 5.** 4 is the broker refusing or being unreachable, 5 is a message
the broker never acknowledged. `hmc -v` adds the connection details, and `RUST_LOG=debug`
adds everything.

**No desktop notifications.** On Linux, notifications need a running notification
daemon, which a bare window manager may not have. Everywhere, check that the toolbar
bell is not toggled off and that Notifications are enabled in Settings; a message this
device sent raises nothing unless **notify own messages** is on.

**A build of `hmg` on Windows stops on a resource path that does not exist.** The
message names `..\target\release\hmc.exe`: the installers carry `hmc`, and it has
not been built yet. Run `cargo build -r -p hmc`. See
[Packaging `hmc`](#packaging-hmc).

**The GUI says `localhost refused to connect`.** That is an `hmg` built by cargo
rather than by the Tauri CLI, which expects a Vite server to serve it whatever the
profile was. Run it with `pnpm tauri dev`, or build it with `pnpm tauri build`. An
empty window with no message is a different fault, in the frontend itself. See
[Running `hmg`](#running-hmg).

More detail lives in [specs/hivemq-cloud.md](specs/hivemq-cloud.md) and
[specs/cli.md](specs/cli.md#exit-codes).

## Layout notes

* `.config/` is a gitignored scratch folder for local broker details. Nothing in a
  build reads it except the opt-in cluster tests above.

* The Cargo target directory is the repository root `target/`, because `src-tauri` is
  a workspace member rather than a standalone package.
* `src-tauri` is a workspace member, which is why the target directory is the
  repository root `target/` rather than `src-tauri/target/`. It used to be commented
  out in
  `Cargo.toml` so that `cargo build --workspace` succeeds.
* `schemas/` and `src/generated/` are generated and committed. Never edit them by
  hand.
* Everything `hmg` does between the broker and the window is the shared session in
  `crates/hiveme-core/src/session/`, behind the `session` feature; `src-tauri/src` only
  adapts it to Tauri. See [specs/session.md](specs/session.md).
* The nine catalogs are `locales/*.json` at the repository root. The frontend imports
  them in `src/i18n/index.ts`, and `hiveme_core::i18n` embeds them into `hmc`, so a key
  is added to all nine files at once. Both `pnpm test` and
  `cargo test -p hiveme-core --lib i18n` fail on a key that a locale lacks or whose
  placeholders differ from English.
* `hmc` turns the `session` feature on for its terminal UI and links SQLite, `ureq`,
  and ratatui. Publishing never opens the database; only the terminal UI does.
* Run the terminal UI with `cargo run -p hmc` in a terminal, or
  `cargo run -p hmc -- --tui --config ./scratch/HiveMe.json` to keep it off your real
  config. It writes nothing to stderr while it is up; add `-v` or set `RUST_LOG` and
  follow `hmc.log` beside the config file from another terminal. `Ctrl+Q` leaves it.
* The terminal UI's screens are tested in process: `cargo test -p hmc --bin hmc tui`
  renders them on ratatui's `TestBackend` against a scripted session whose rows live in
  an in-memory store, see `crates/hmc/src/tui/tests/`. One of them,
  `against_a_real_broker_the_composer_and_one_shot_hmc_meet_in_the_view`, starts the
  HiveMQ CE container as `crates/hmc/tests/publish.rs` does and is skipped the same way
  with `HIVEME_SKIP_DOCKER=1`.

## Config file location

The config file `HiveMe.json` is resolved at runtime, in this order: `--config`, the
`HIVEME_CONFIG` environment variable, then the platform location.

* **Linux**: `$XDG_CONFIG_HOME/HiveMe/HiveMe.json` when `XDG_CONFIG_HOME` is set,
  otherwise `$HOME/.config/HiveMe/HiveMe.json`.
* **macOS**: `$HOME/Library/Application Support/HiveMe/HiveMe.json`.
* **Windows**: `%APPDATA%\HiveMe\HiveMe.json` when the executable lives under
  `%LOCALAPPDATA%`, `%ProgramFiles%`, or `%ProgramFiles(x86)%`, otherwise next to the
  executable.

The SQLite history database `HiveMe.db` sits in the same directory. Full reference in
[specs/config.md](specs/config.md).

## Native end-to-end test

`pnpm test:e2e` runs `scripts/ts/test-e2e.ts` against the built hmc and hmg binaries.
It creates an isolated HiveMQ CE Docker container, config, and SQLite database.
No IPC, application state, MQTT traffic, or database calls are mocked. Missing
Docker or WebDriver prerequisites fail the command; the test never silently skips.

Build and install the driver once:

```sh
cargo build -p hmc
pnpm tauri build --debug --no-bundle
cargo install tauri-driver --version 2.0.6 --locked
```

On Linux, install `webkit2gtk-driver` and `xvfb`, then run:

```sh
xvfb-run -a pnpm test:e2e
```

On Windows, install Microsoft Edge WebDriver matching the installed WebView2 Runtime.
Put `msedgedriver.exe` on PATH, or set `HIVEME_E2E_NATIVE_DRIVER` to its absolute path,
then run `pnpm test:e2e` from a **non-administrator terminal**. Elevated WebView2
processes ignore the environment flags WebDriver uses to attach. See
[Microsoft's WebView2 automation setup](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/webdriver)
and [privilege requirements](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security#for-an-elevated-host-app-use-appropriate-override-flags).
Native Tauri WebDriver testing is supported on Windows and Linux, not macOS.

Use `pnpm test:e2e --release` after `pnpm tauri build --no-bundle` to test the release
GUI. Both programs come from `target/debug` by default or `target/release` with
`--release`. To use a separate build directory, set `HIVEME_E2E_BIN_DIR` to the
absolute directory containing both binaries. The runner only reads those binaries;
all config and database writes are confined to its test directory. The Linux
workflow builds the applications and runs the release end-to-end test under Xvfb.

The scenario verifies:

- An empty database starts with `hiveme` visible, selected, and highlighted.
- Appearance opens first, settings save automatically, and no default-topic setting exists.
- `hmc haha` prints `Message sent to hiveme.`; all log levels stay in the payload,
  arrive on the root MQTT topic, and appear in the GUI without synthetic level topics.
  This includes `success`, which renders with the MUI success palette.
  Uppercase and mixed-case CLI levels are accepted; payload JSON and stored level
  fields are checked for lowercase values.
- The GUI composer sends directly to the selected root and reconciles the broker echo.
- Enter sends once from any focused composer control without also activating it;
  Shift+Enter adds a line in the message box, and text composition does not send.
- Message hover controls show level, QoS, time, and a split copy button whose menu
  contains Copy and Copy Raw JSON.
- Message headers show incoming sender details. Relative topic paths appear below
  the bubble before the level badge and update with selection; the entire row hides
  when the pointer leaves. Outgoing messages hide the sender and empty paths are omitted.
- The root is fixed at `hiveme`, with no prefix setting in config, settings, or setup
  strings. CLI and GUI topic input strip leading slashes and resolve relative to
  `hiveme` or the selected tree topic, respectively.
- The composer layout has 4 px margins to the panel edges, a three-row input minimum,
  both checkboxes on the QoS row, and an Info / Error / Success / Warn dropdown left
  of More Options. The entire panel state is remembered per topic in memory and
  resets on restart. Expanded options remain active when collapsed. Topic, title,
  level, QoS, retain, and raw JSON overrides are checked against actual published data.
- Selecting an intermediate topic loads its recursive children from SQLite.
  Topic labels preserve expansion; only the left icon expands or collapses children.
- Restart restores stored history while selecting `hiveme`. An unknown JSON field
  cannot redirect messages to a level suffix.

Each run keeps its test-only config, database, logs, and screenshot under
`target/e2e/<run-id>/`; failures also capture the DOM when a session is available.
The runner stops its own WebDriver session and removes its broker container.
CI uploads diagnostics on failure. No real broker credentials are needed.

## Testing against a broker

`crates/hiveme-core/tests/mqtt.rs` starts a `hivemq/hivemq-ce` container per test
through `testcontainers`. The image takes a few seconds to come up, so the whole file
runs in well under a minute.

```sh
cargo test -p hiveme-core --test mqtt           # the client
cargo test -p hiveme-core --test session        # the shared session hmg runs on
cargo test -p hmc --test publish                # CLI broker integration
```

Each test says why it did nothing and passes when Docker is unavailable, or when
`HIVEME_SKIP_DOCKER=1` is set, as the macOS and Windows workflows do. The Linux
workflow has Docker and runs them.

The container speaks plain MQTT, so nothing there exercises the TLS handshake. To test
against a real HiveMQ Cloud cluster, which does, put its setup string in
`.config/broker.json`:

```sh
mkdir -p .config
cat > .config/broker.json <<'JSON'
{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"hiveme-sam","password":"s3cret"}
JSON

cargo test -p hiveme-core --test mqtt -- a_real_cloud_cluster_accepts_a_message
cargo test -p hmc --test publish -- a_real_cloud_cluster_accepts_a_message_from_hmc
```

`.config/` is gitignored, and `broker.json` is the same string the `hmg` Settings tab
shows and `hmc --init` reads, so there is one format to know. It carries the broker
password in plain text; treat the file as a credential.

The `hmc` test runs the real `hmc --init` with that string and then publishes through
it, which is the walkthrough of [specs/cli.md](specs/cli.md) end to end over TLS.

Both tests publish under a unique `hiveme/test/<device>` child topic, so
they cannot disturb the messages a real installation keeps on the same cluster. Both
are opt in and never run in CI. `HIVEME_TEST_BROKER_URL`, `HIVEME_TEST_USERNAME`, and
`HIVEME_TEST_PASSWORD` still work for the `hiveme-core` test, so a CI secret needs no
file.

## Releasing

1. Edit the versions at the bottom of `scripts/ts/change-version.ts`.
2. Run `deno task -c scripts/ts/deno.json version`.
3. Add a section to [release_notes.md](release_notes.md).
4. Tag and push. The three workflows build the installers and the `hmc` binaries.

## Stack

* [Rust](https://www.rust-lang.org/) edition 2024
* [Tauri v2](https://tauri.app/)
* [rumqttc](https://crates.io/crates/rumqttc) for MQTT 5
* [React 19](https://react.dev/)
* [TypeScript](https://www.typescriptlang.org/)
* [Material UI](https://mui.com/) with [MUI X Tree View](https://mui.com/x/react-tree-view/)
* [Zustand](https://zustand.docs.pmnd.rs/)
* [react-i18next](https://react.i18next.com/)
* [Vite](https://vite.dev/)
