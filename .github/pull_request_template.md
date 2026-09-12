## What this changes

<!-- One or two sentences. Link the plan step, for example "step 1.2 of docs/plans/plan-initialization.md". -->

## Definition of done

From `docs/specs/app.md`, "Spec sync". Tick what applies, and say why for anything left
unticked.

- [ ] Code
- [ ] Tests, including a failing case for each new rule
- [ ] The specification under `docs/specs/` is updated, or this change does not touch
      any behaviour a specification describes
- [ ] `cargo xtask schema` was rerun and `schemas/` is committed
- [ ] `pnpm gen:types` was rerun and `src/generated/` is committed
- [ ] `src-tauri/src/protocol.rs` and `src/lib/protocol.ts` moved together
- [ ] The status table at the end of `docs/specs/app.md` reflects what now exists
- [ ] `docs/release_notes.md` has a line for anything a user would notice

## Checks

- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `deno task -c scripts/ts/deno.json check`
- [ ] `pnpm typecheck` and `pnpm test`

<!--
A pure refactor that cannot change behaviour may carry the `spec-sync-exempt` label,
which skips the code-and-specification pairing check in CI.
-->
