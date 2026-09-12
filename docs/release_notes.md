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
* Added `hmg`, the desktop application: a topic tree with unread badges on the left, a
  chat view of the selected topic on the right, and an input box that publishes through
  the same path `hmc` does. Messages arrive live, and a message this device sent is
  shown once whether it came from the composer or came back from the broker.
* Gave `hmg` a local history in SQLite beside the config file, so topics and messages
  survive a restart. It is bounded per topic and by age, and pruned at startup and
  every ten minutes.
* Rendered every kind of payload rather than only the ones HiveMe wrote: an envelope
  shows its title, body, level, and a collapsible `data` tree; JSON from another tool
  shows as a tree; text shows as text; anything else shows as hex with its size; and an
  encrypted message shows as a lock with its key id.
* Made the Settings tab the one place a cluster is set up, with a **Copy CLI setup**
  button that puts the `hmc --init` string on the clipboard. Saving reconnects only
  when something the connection is built from changed.
* Turned the notification rules into OS notifications on all three platforms, with the
  rate limit, the "and N more messages" summary, and a toolbar toggle that holds them
  back for the session. On Windows the notification is labelled HiveMe, because the
  application registers an identity of its own instead of borrowing the one of whatever
  process raised the toast.
* Added a status bar that shows the connection, the broker, the reconnect countdown,
  the subscriptions, the messages this session, and the size of the history.
* Drew the pair an icon: a honey coloured hive cell holding a message bubble for `hmg`
  and a command prompt for `hmc`. On Windows `hmc.exe` carries its own icon and version
  information, so Explorer and the taskbar name it rather than showing a blank
  executable.
* Wrote the onboarding: a README that goes from creating a HiveMQ Cloud cluster to a
  desktop notification in six steps, a troubleshooting section for the ways that path
  goes wrong, and an installation guide covering every published artifact, where each
  one installs, and where the config and the history live on each platform.
* Put `hmc` inside every installer, beside `hmg`, so that installing the desktop
  application installs the command line one that reads the same config file. The deb
  and the rpm put it on `PATH` at `/usr/bin/hmc`; the macOS bundle and both Windows
  installers put it next to `hmg`. `hmc` is still published on its own, for a machine
  with no desktop.
