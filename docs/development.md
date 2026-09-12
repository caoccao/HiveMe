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
pnpm tauri dev                                  # from step 4.1
pnpm tauri build                                # from step 4.1
```

`pnpm test` runs `vitest run --passWithNoTests`; the flag comes out once the first
frontend test lands in step 4.3.

## Logging

```sh
RUST_LOG=debug cargo run -p hmc -- "hello"      # Unix
set RUST_LOG=debug                              # Windows
```

## Layout notes

* The Cargo target directory is the repository root `target/`, because `src-tauri` is
  a workspace member rather than a standalone package.
* `src-tauri` joins the workspace in step 4.1; until then it is commented out in
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
cargo test -p hiveme-core --test mqtt
```

Each test says why it did nothing and passes when Docker is unavailable, or when
`HIVEME_SKIP_DOCKER=1` is set, as the macOS and Windows workflows do. The Linux
workflow has Docker and runs them.

The container speaks plain MQTT, so nothing there exercises the TLS handshake. To test
against a real HiveMQ Cloud cluster, which does, set all three of:

```sh
export HIVEME_TEST_BROKER_URL=mqtts://<id>.s1.eu.hivemq.cloud:8883
export HIVEME_TEST_USERNAME=<username>
export HIVEME_TEST_PASSWORD=<password>
cargo test -p hiveme-core --test mqtt -- a_real_cloud_cluster_accepts_a_message
```

That test publishes under `hiveme-test/<device>` rather than the usual prefix, so it
cannot disturb the messages a real installation keeps on the same cluster. It is opt in
and never runs in CI.

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
