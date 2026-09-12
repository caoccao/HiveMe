# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in
this repository. `AGENTS.md` links to this file; do not modify it.

## Project Overview

HiveMe, short for HiveMQ Messager, is two applications on top of MQTT, sharing one
config file and one message format:

* **`hmc`**, a CLI that publishes one message and exits.
* **`hmg`**, a Tauri 2 desktop GUI with a React 19, TypeScript, and Material UI
  frontend.

The broker is [HiveMQ Cloud](https://docs.hivemq.com/hivemq-cloud/index.html), reached
over MQTT 5 with TLS.

**Read the specifications before changing anything**: [docs/specs/app.md](docs/specs/app.md)
is the index. The build order is [docs/plans/plan-initialization.md](docs/plans/plan-initialization.md).

**The reference project is `../BetterMediaInfo`**, by the same author. HiveMe copies
its layout, module split, and conventions on purpose. When you add structure, open the
matching BetterMediaInfo file first and follow it, unless
[docs/specs/app.md](docs/specs/app.md#reference-architecture) records a deliberate
departure.

## Development Commands

```sh
# Rust
cargo build --workspace
cargo test --workspace                          # add -r for release mode
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings

# Repository automation
cargo xtask schema                              # regenerate schemas/ from the Rust types
cargo xtask check-spec                          # validate the tagged examples in docs/specs/
deno task -c scripts/ts/deno.json check              # type check the scripts themselves
deno task -c scripts/ts/deno.json check-license-headers
deno task -c scripts/ts/deno.json check-spec-sync   # pass a base ref, or set SPEC_SYNC_BASE

# Frontend
pnpm install
pnpm typecheck                                  # tsc -b
pnpm test                                       # vitest
pnpm gen:types                                  # schemas/ -> src/generated/, runs the Deno task
pnpm dev                                        # vite on http://localhost:1420
pnpm tauri dev                                  # GUI with hot reload
pnpm tauri build                                # release bundle

# Version bump
deno task -c scripts/ts/deno.json version
```

`RUST_LOG=debug` turns on backend logging.

## Architecture

### Workspace

```
Cargo.toml           workspace root, [workspace.package] version and shared deps
crates/hiveme-core   config, message, topic, rules, mqtt, storage, cloud
crates/hmc           the CLI binary
xtask                repository automation
src-tauri            the Tauri backend of hmg
src                  the React frontend of hmg
schemas              generated JSON schemas, committed
```

The Cargo target directory is the repository root `target/`, not `src-tauri/target/`,
because `src-tauri` is a workspace member. Build workflows and documentation use the
root path.

### Frontend and backend communication

The same protocol pattern as BetterMediaInfo:

1. **Protocol files**: `src/lib/protocol.ts` and `src-tauri/src/protocol.rs` are
   hand-synced. camelCase in TypeScript, snake_case in Rust with `#[serde(rename)]`.
   `scripts/ts/check-spec-sync.ts` fails when one changes without the other.
2. **IPC layer**: `src/lib/service.ts` wraps every `invoke`; `src-tauri/src/lib.rs`
   holds one thin `#[tauri::command]` per call, alphabetized, each returning
   `Result<T, String>` through `convert_error`.
3. **Business logic**: `hiveme-core`, so that `hmc` and `hmg` cannot diverge.
   `src-tauri/src/controller.rs` only orchestrates.
4. **State**: Zustand in `src/lib/store.tsx`. Components never call Tauri APIs
   directly.

### Config and message types are generated, not hand written

`hiveme-core` derives `schemars` schemas for the config and message types.
`cargo xtask schema` writes `schemas/*.schema.json`, and `pnpm gen:types` turns those
into `src/generated/*.ts`. Both outputs are committed and CI fails when they are
stale. Never edit `schemas/` or `src/generated/` by hand.

## Specification sync

The specifications describe the code as built, and CI enforces it. See
[docs/specs/app.md](docs/specs/app.md#spec-sync) for the full table. In short:

* Change the config or message types, update `docs/specs/config.md` or
  `docs/specs/message.md`, and rerun `cargo xtask schema` and `pnpm gen:types`.
* Change the CLI, update `docs/specs/cli.md`, including the `text hiveme:help` block.
* Change the IPC surface, update `docs/specs/gui.md` and both protocol files.
* JSON examples in the specifications are tagged `json hiveme:<tag>` and are parsed
  and validated by `cargo xtask check-spec`.
* Update the status table at the end of `docs/specs/app.md` when a feature lands, and
  add a line to `docs/release_notes.md` for anything a user would notice.
* A pure refactor can carry the `spec-sync-exempt` pull request label.

## Conventions

* All project-authored English uses US English spelling and terminology, including
  UI text, CLI and backend messages, documentation, comments, and translation keys.
  Other locales keep their own language conventions.
* Every `.rs`, `.ts`, and `.tsx` file starts with the Apache-2.0 header from
  `scripts/license-header.txt`. `scripts/ts/check-license-headers.ts` enforces it.
* Rust: edition 2024, `max_width = 120`, `tab_spaces = 2`, `thiserror` in libraries,
  `anyhow` in binaries, clippy clean with `-D warnings`.
* The toolchain is pinned in `rust-toolchain.toml`.
* Config JSON keys are camelCase. Enum values that live only in the config are
  PascalCase; values that also travel in messages keep their wire casing.
* MUI components take colors from the theme through `sx` or `styled`, never hard
  coded, so both display modes work.
* Text is never transformed. The theme sets `typography.button.textTransform` to
  `none`, which covers buttons, tabs, and toggle buttons; do not add `textTransform`
  to a component.

## Common Pitfalls

1. **Editing generated files.** `schemas/` and `src/generated/` come from the Rust
   types. Change the types, then regenerate.
2. **Forgetting the other half of the protocol.** `protocol.rs` and `protocol.ts` move
   together.
3. **Writing the config from the struct alone.** The config writer merges into the
   `serde_json::Value` that was read so that unknown keys survive; a plain
   `to_writer_pretty(&self)` would silently drop another version's settings.
4. **Assuming the REST API is available.** It needs the HiveMQ Cloud Starter plan. The
   applications are MQTT only.
5. **Client identifier collisions.** `hmc` and `hmg` must use different client
   identifiers or the broker disconnects one of them.
6. **Building `hmg` before `hmc`.** Every bundle carries `hmc` from
   `target/release/hmc`. On Windows that entry is a `resources` one, which
   `tauri-build` reads, so `cargo build -r -p hmc` has to come first or nothing that
   touches `hmg` compiles.
