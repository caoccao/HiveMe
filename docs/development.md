# Development

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | pinned by `rust-toolchain.toml` (1.96.0) | `rustup` installs it automatically |
| Node.js | 24 | |
| pnpm | 11 | |
| Deno | 2.x | Runs every repository script under `scripts/ts/` |
| Docker | any recent | Only for the MQTT integration tests |

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
pnpm gen:types
pnpm dev                                        # vite on http://localhost:1420
pnpm tauri dev                                  # the GUI with hot reload
pnpm tauri build                                # the release bundle for this OS
```

`pnpm test` runs the frontend tests with vitest, configured in `vitest.config.ts`.

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
log still reads perfectly normally. `src/App.test.tsx` mounts the whole application in
jsdom against a mocked backend for exactly that reason: it is the only check that says
whether anything was rendered at all.

## Running `hmc`

A cluster is set up in `hmg`, which shows a one line setup string to paste into `hmc`:

```sh
cargo run -p hmc -- --config ./HiveMe.json --init '{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"hiveme-sam","password":"s3cret","prefix":"hiveme"}'
cargo run -p hmc -- --config ./HiveMe.json "hello"
echo hello | cargo run -p hmc -- --config ./HiveMe.json
cargo run -p hmc -- --help
```

Press **Copy CLI setup** in the Broker section of the `hmg` Settings tab to get the
string; the format is in [specs/config.md](specs/config.md#the-setup-string).
Publishing before any `--init` writes a default config and exits 3. Full reference in
[specs/cli.md](specs/cli.md).

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

## Testing against a broker

`crates/hiveme-core/tests/mqtt.rs` starts a `hivemq/hivemq-ce` container per test
through `testcontainers`. The image takes a few seconds to come up, so the whole file
runs in well under a minute.

```sh
cargo test -p hiveme-core --test mqtt           # the client
cargo test -p hmc --test publish                # hmc end to end
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

Both tests publish under `hiveme-test/<device>` rather than the configured prefix, so
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
