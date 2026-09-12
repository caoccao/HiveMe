# HiveMe

[![Linux Build](https://github.com/caoccao/HiveMe/actions/workflows/linux_build.yml/badge.svg)](https://github.com/caoccao/HiveMe/actions/workflows/linux_build.yml) [![MacOS Build](https://github.com/caoccao/HiveMe/actions/workflows/macos_build.yml/badge.svg)](https://github.com/caoccao/HiveMe/actions/workflows/macos_build.yml) [![Windows Build](https://github.com/caoccao/HiveMe/actions/workflows/windows_build.yml/badge.svg)](https://github.com/caoccao/HiveMe/actions/workflows/windows_build.yml)

HiveMe is a pair of desktop applications for your own MQTT topics, built on
[HiveMQ Cloud](https://www.hivemq.com/).

* **`hmc`**, a CLI that sends a message from a shell or a script.
* **`hmg`**, a GUI that watches your topics, keeps a local history, and raises OS
  notifications from rules.

Both share one config file and one JSON message format, and both run on Linux, macOS,
and Windows.

> **Status: phase 4.** Both applications are built. What is left is the release
> packaging and the onboarding documentation of phase 5, following
> [docs/plans/plan-initialization.md](docs/plans/plan-initialization.md). There is no
> published release yet, so build from source for now.

## Quick start

1. Create a HiveMQ Cloud Serverless cluster and a set of MQTT credentials in the
   [console](https://console.hivemq.cloud/). See
   [docs/specs/hivemq-cloud.md](docs/specs/hivemq-cloud.md).
2. Build and start the GUI, then fill in the Broker section of its Settings tab with
   the cluster URL, the username, and the password.
3. Press **Copy CLI setup** and hand the string to the CLI, which is all `hmc` needs.
4. Send a message and watch it arrive in the GUI.

```sh
pnpm install
pnpm tauri dev                 # the GUI
cargo build -r -p hmc          # the CLI, at target/release/hmc

hmc --init '<paste the setup string>'
hmc "Build finished"
hmc -t error "Disk full"
```

### Building from source

```sh
cargo build --workspace     # build the Rust crates
cargo test --workspace      # run the Rust tests
cargo xtask check-spec      # validate the examples in docs/specs
pnpm install                # frontend dependencies
pnpm test                   # run the frontend tests
pnpm tauri build            # bundle the GUI for this OS
```

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
