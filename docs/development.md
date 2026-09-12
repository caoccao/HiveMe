# Development

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | pinned by `rust-toolchain.toml` (1.96.0) | `rustup` installs it automatically |
| Node.js | 24 | |
| pnpm | 11 | |
| Deno | 2.x | Runs every repository script under `scripts/ts/` |
| Docker | any recent | Only for the MQTT integration tests, from phase 2 |

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

From phase 2, the integration tests start a `hivemq/hivemq-ce` container through
`testcontainers`. They are skipped with a message when Docker is unavailable, and on
the macOS and Windows workflows, which set `HIVEME_SKIP_DOCKER=1`.

To test against a real HiveMQ Cloud cluster instead, set `HIVEME_TEST_BROKER_URL`,
`HIVEME_TEST_USERNAME`, and `HIVEME_TEST_PASSWORD`. Those tests are opt in and never
run in CI.

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
