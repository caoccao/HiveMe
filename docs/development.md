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
pnpm tauri dev      # what to use while working on it
pnpm tauri build    # the bundle, with the frontend compiled into the binary
```

**Do not run `target/debug/hmg.exe` by hand.** `cargo build -p hmg` produces a
development build, and a development build loads the frontend from the `devUrl` in
`tauri.conf.json`, which is the Vite server at `http://localhost:1420`. Started on its
own, with no server behind that address, it opens a window showing
*"localhost refused to connect"*. `pnpm tauri dev` starts Vite first, which is why it
is the command to use.

To run the binary directly, build it so the frontend is inside it:

```sh
pnpm tauri build --debug --no-bundle   # then target/debug/hmg.exe runs on its own
```

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
