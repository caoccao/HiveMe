# Generated schemas

This directory holds the JSON schemas that `cargo xtask schema` generates from the
`schemars` annotations on the `hiveme-core` config and message types.

* `config.schema.json` comes from `hiveme_core::config` and is documented in
  [../docs/specs/config.md](../docs/specs/config.md).
* `broker-init.schema.json` comes from `hiveme_core::config::BrokerInit`, documented in
  [../docs/specs/config.md](../docs/specs/config.md).
* `message.schema.json` comes from `hiveme_core::message` and is documented in
  [../docs/specs/message.md](../docs/specs/message.md).

All three are committed, and CI fails when they are stale. **Do not edit them by hand**:
change the Rust types, then run `cargo xtask schema` and `pnpm gen:types`.

`pnpm gen:types` turns these into `src/generated/config.ts` and
`src/generated/message.ts`, plus `src/generated/broker-init.ts`, which are committed for the same reason.

`cargo xtask check-spec` validates the examples embedded in the specifications against
these schemas, so an example that drifts from the code fails CI.
