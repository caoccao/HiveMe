# Generated schemas

This directory holds the JSON schemas that `cargo xtask schema` generates from the
`schemars` annotations on the `hiveme-core` config and message types.

* `config.schema.json` comes from `hiveme_core::config` and is documented in
  [../docs/specs/config.md](../docs/specs/config.md).
* `message.schema.json` comes from `hiveme_core::message` and is documented in
  [../docs/specs/message.md](../docs/specs/message.md).

Both are committed, and CI fails when they are stale. **Do not edit them by hand**:
change the Rust types, then run `cargo xtask schema` and `pnpm gen:types`.

The generators arrive with steps 1.1 and 1.2 of
[the initialization plan](../docs/plans/plan-initialization.md), so the directory is
empty until then.
