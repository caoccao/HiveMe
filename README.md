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
pnpm tauri build       # the GUI bundle for this OS, under target/release/bundle/
cargo build -r -p hmc  # the CLI, at target/release/hmc
```

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

## Troubleshooting

**The GUI says disconnected, and the error mentions the password.** Credentials take up
to a minute to become active after you create them in the console. If it persists,
check the username and password in Settings, and that they belong to the cluster in the
URL.

**The connection fails during the TLS handshake.** The cluster hostname has to be the
one the console shows, because HiveMQ Cloud selects the certificate from the name the
client sends during the handshake (SNI). An IP address, or a hostname with the port
left in it, will not work. HiveMQ Cloud accepts TLS only, so the URL starts with
`mqtts://`, never `mqtt://`.

**Messages arrive and then the connection drops, over and over.** Two clients cannot
share a client identifier: the broker disconnects the older one, which reconnects, and
so on. HiveMe gives `hmg` and each `hmc` run different identifiers by construction, so
this usually means a second copy of `hmg` running against the same config, or another
MQTT client of yours using the same id. A Serverless cluster also allows 100
concurrent connections in total.

**`hmc` exits 3 and prints a config path.** There is no config yet, so it wrote a
default one. Run `hmc --init '<paste>'` with the string from the GUI.

**`hmc` exits 4 or 5.** 4 is the broker refusing or being unreachable, 5 is a message
the broker never acknowledged. `hmc -v` adds the connection details, and `RUST_LOG=debug`
adds everything.

**No desktop notifications.** On Linux, notifications need a running notification
daemon, which a bare window manager may not have. Everywhere, check that the toolbar
bell is not toggled off and that Notifications are enabled in Settings; a message this
device sent raises nothing unless **notify own messages** is on.

**The GUI window is empty, or says `localhost refused to connect`.** That is a
development build started without its Vite server. Use `pnpm tauri dev`, or build with
`pnpm tauri build`. See
[docs/development.md](docs/development.md#running-hmg).

More detail lives in [docs/specs/hivemq-cloud.md](docs/specs/hivemq-cloud.md) and
[docs/specs/cli.md](docs/specs/cli.md#exit-codes).

## Building from source

```sh
cargo build --workspace     # build the Rust crates
cargo test --workspace      # run the Rust tests
cargo xtask check-spec      # validate the examples in docs/specs
pnpm install                # frontend dependencies
pnpm test                   # run the frontend tests
pnpm tauri dev              # run the GUI with hot reload
pnpm tauri build            # bundle the GUI for this OS
```

See [docs/development.md](docs/development.md) for the rest.

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
