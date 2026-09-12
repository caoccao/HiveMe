# Release Notes

## Unreleased

* Split the specification into `app.md`, `config.md`, `message.md`, `cli.md`,
  `gui.md`, and `hivemq-cloud.md`, and designed the config schema, the JSON message
  envelope, and the future encryption scheme.
* Set up the Cargo workspace (`hiveme-core`, `hmc`, `xtask`), the frontend toolchain,
  the repository scripts, and the three per-OS build workflows.
* Wrote every repository script as Deno TypeScript under `scripts/ts/`, so that the
  project has no shell scripts and behaves the same on Linux, macOS, and Windows.
* Built the shared core: the config file with its per-OS location, migrations, and a
  writer that preserves keys it does not know; the JSON message envelope with its parse
  tiers and compatibility rules; MQTT topic resolution and filter matching; and the
  notification rule engine with its templates and rate limiter.
* Generated `schemas/` from the Rust types and `src/generated/` from those schemas, and
  wired both into CI so a stale schema, a drifted example, or an out of date TypeScript
  type fails the build.
