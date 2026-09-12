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
* Added the MQTT 5 client both applications connect with: TLS from the system trust
  store plus an optional CA file, publishes that return only once the broker has
  acknowledged them, subscriptions that report the reason the broker gave, a connection
  state the GUI status bar will render, and, for the GUI, reconnection with backoff and
  jitter that sends the subscriptions again when the broker has forgotten the session.
* Added `hmc`, the command line publisher: a message from an argument or from stdin, a
  topic relative to the configured prefix or absolute, `--json` for a payload HiveMe
  does not shape, a level inferred from the notification rule that matches the topic,
  and an exit code per kind of failure so a script can tell a wrong password from an
  unacknowledged message. A first run with no config writes one and says where.
* Made `hmg` the one place a HiveMQ Cloud cluster is set up. Its Settings tab will show
  a one line setup string; `hmc --init '<json>'` turns that string into a config, so
  there is no URL, username, or password to retype into the CLI. The format is
  generated into `schemas/broker-init.schema.json` from the same Rust type both
  applications use.
* Said plainly that only Serverless clusters are supported for now, and that TLS is
  automatic for them: the cluster certificate chains to a public authority the
  operating system already trusts, so neither application generates, enrols, or asks
  about a certificate.
