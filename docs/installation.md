# Installation

> HiveMe has no release yet. This page describes what the release will contain, and is
> finished in step 5.1 of [the initialization plan](plans/plan-initialization.md).

Download the latest release from the
[Releases](https://github.com/caoccao/HiveMe/releases) page.

| OS | Artifacts |
|----|-----------|
| Linux | `HiveMe*.deb`, `HiveMe*.rpm`, `HiveMe*.AppImage` |
| macOS | `HiveMe*.dmg` for Intel and Apple silicon |
| Windows | `HiveMe*.msi`, `HiveMe*setup.exe`, and a portable `.7z` |

The `hmc` command line binary is published for every platform, and the Windows
portable archive contains both `hmg.exe` and `hmc.exe`.

## After installing

1. Create a HiveMQ Cloud cluster and a set of MQTT credentials. See
   [specs/hivemq-cloud.md](specs/hivemq-cloud.md#console-walkthrough).
2. Start `hmg` and fill in the Broker section of the Settings tab, or edit the config
   file directly. See [specs/config.md](specs/config.md#location-and-precedence) for
   where it lives on each platform.
3. Send a test message: `hmc "hello"`.
