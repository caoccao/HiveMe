# HiveMe

[![Linux Build](https://github.com/caoccao/HiveMe/actions/workflows/linux_build.yml/badge.svg)](https://github.com/caoccao/HiveMe/actions/workflows/linux_build.yml) [![MacOS Build](https://github.com/caoccao/HiveMe/actions/workflows/macos_build.yml/badge.svg)](https://github.com/caoccao/HiveMe/actions/workflows/macos_build.yml) [![Windows Build](https://github.com/caoccao/HiveMe/actions/workflows/windows_build.yml/badge.svg)](https://github.com/caoccao/HiveMe/actions/workflows/windows_build.yml)

HiveMe is a pair of desktop applications for your own MQTT topics, built on
[HiveMQ Cloud](https://www.hivemq.com/).

* **`hmc`**, a CLI that sends a message from a shell or a script.
* **`hmg`**, a GUI that watches your topics, keeps a local history, and raises OS
  notifications from rules.

Both share one config file and one JSON message format, and both run on Linux, macOS,
and Windows.

> **Status: phase 5.** Both applications are built and documented. There is no
> published release yet, so build from source; the packaging is described in
> [docs/installation.md](docs/installation.md) and the remaining work in
> [docs/plans/plan-initialization.md](docs/plans/plan-initialization.md).

## Quick start

From nothing to a desktop notification. Steps 1 and 2 happen once, in a browser.

### 1. Create a cluster

Sign up at [hivemq.com](https://www.hivemq.com/) and create a **Serverless** cluster,
which is free and is the plan HiveMe supports today. On the cluster's **Overview** tab,
note the hostname; the TLS MQTT port is 8883.

### 2. Create credentials

On the cluster's **Access Management** tab, open **Credentials**, choose **Edit**, then
**Add Credentials**, and save a username and a password. On Serverless the default
permission set is publish and subscribe on everything, which is what HiveMe needs.

Credentials take up to a minute to become active. A connection refused straight after
creating them usually just means waiting.

### 3. Install, or build

Take the artifacts for your platform from the
[Releases](https://github.com/caoccao/HiveMe/releases) page and follow
[docs/installation.md](docs/installation.md). To build instead:

```sh
pnpm install
cargo build -r -p hmc  # the CLI, at target/release/hmc
pnpm tauri build       # the bundle for this OS, carrying both, under target/release/bundle/
```

[docs/development.md](docs/development.md#commands) has the rest of the commands, and
what each one is for.

### 4. Tell the GUI about the cluster

Start HiveMe, press **F10** for the Settings tab, and fill in the Broker section:

| Field | Value |
|-------|-------|
| URL | `mqtts://<your-cluster>.s1.eu.hivemq.cloud:8883` |
| Username | the username from step 2 |
| Password | the password from step 2 |

Press **Save**. The status bar at the bottom turns to **connected**. There is nothing
to configure for TLS: a HiveMQ Cloud certificate chains to an authority your operating
system already trusts.

### 5. Tell the CLI, without retyping any of it

In that same Broker section, press **Copy CLI setup**, then:

```sh
hmc --init '<paste>'
```

That writes the CLI's config, which is the same file the GUI just wrote. The string
carries the password in plain text, so paste it and do not commit it or leave it in a
shared shell history.

Installing HiveMe installed `hmc` too, beside `hmg`. On Linux that is `/usr/bin/hmc`
and a shell finds it already; elsewhere it is in the install folder, and
[docs/installation.md](docs/installation.md#what-is-published) says where.

### 6. Send a message

```sh
hmc "Build finished"          # goes to hiveme/info
hmc -t warn "Disk at 87%"     # goes to hiveme/warn
hmc -t error "Build failed"   # goes to hiveme/error
```

Each one appears in the GUI within a moment, under its topic in the tree on the left,
and raises a desktop notification: the three built-in rules match `info`, `warn`, and
`error`. Click a topic to read its history, and type in the box at the bottom to
publish back to it.

That is the whole loop. Put `hmc` at the end of a long build, a backup script, or a
cron job, and the machine tells you when it is done.

When something does not work, [docs/development.md](docs/development.md#troubleshooting)
lists what usually goes wrong and what to do about it.

## Documentation

* [Specifications](docs/specs/app.md)
  * [Config](docs/specs/config.md)
  * [Message format](docs/specs/message.md)
  * [CLI](docs/specs/cli.md)
  * [GUI](docs/specs/gui.md)
  * [HiveMQ Cloud](docs/specs/hivemq-cloud.md)
* [Initialization plan](docs/plans/plan-initialization.md)
* [Development](docs/development.md)
* [Installation](docs/installation.md)
* [Release Notes](docs/release_notes.md)
* [Screenshots](docs/screenshots.md)
* [TODOs](docs/todos.md)

## License

[APACHE LICENSE, VERSION 2.0](LICENSE)
