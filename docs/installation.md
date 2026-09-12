# Installation

HiveMe, short for HiveMQ Messager, ships two programs from one repository: `hmg`, the
desktop application, and `hmc`, the command line publisher. They share a config file,
and every installer carries both, so setting the cluster up once is enough for both.

Download the latest release from the
[Releases](https://github.com/caoccao/HiveMe/releases) page.

## What is published

| OS | GUI | CLI |
|----|-----|-----|
| Linux x86_64 | `HiveMe*.deb`, `HiveMe*.rpm`, `HiveMe*.AppImage` | `hmc` |
| macOS Intel | `HiveMe*.dmg` | `hmc` |
| macOS Apple silicon | `HiveMe*.dmg` | `hmc` |
| Windows x86_64 | `HiveMe*.msi`, `HiveMe*-setup.exe` (NSIS), `HiveMe*-portable.7z` | `hmc.exe` |

Every GUI artifact carries `hmc` as well, beside `hmg`:

| OS | Where the GUI artifact puts `hmc` | On `PATH` |
|----|-----------------------------------|-----------|
| Linux, deb and rpm | `/usr/bin/hmc` | yes |
| Linux, AppImage | inside the image, at `usr/bin/hmc` | no |
| macOS | `HiveMe.app/Contents/MacOS/hmc` | no |
| Windows, msi | `%ProgramFiles%\HiveMe\hmc.exe` | no |
| Windows, nsis | `%LOCALAPPDATA%\HiveMe\hmc.exe` | no |
| Windows, portable | beside `hmg.exe`, wherever the archive was unpacked | no |

Only the Linux packages land it somewhere a shell already looks. Everywhere else, call
it by its full path, add that folder to `PATH`, or take the standalone `hmc` and put it
where you want it. An AppImage is one file that mounts itself when it runs, so the copy
inside is out of reach until the image is unpacked with `--appimage-extract`;
downloading `hmc` is easier.

`hmc` is published on its own as well, as a plain executable for every platform, for a
machine that runs scripts and has no desktop to install a GUI on.

## Installing the GUI

### Linux

```sh
sudo apt install ./HiveMe_0.1.0_amd64.deb      # Debian, Ubuntu
sudo dnf install ./HiveMe-0.1.0-1.x86_64.rpm   # Fedora, RHEL
chmod +x HiveMe_0.1.0_amd64.AppImage && ./HiveMe_0.1.0_amd64.AppImage
```

The packages put the binaries at `/usr/bin/hmg` and `/usr/bin/hmc`, and a desktop
entry at `/usr/share/applications/`, so HiveMe appears in the application menu and
`hmc` is on `PATH` straight away. The AppImage runs from wherever it sits and installs
nothing, which is also why its copy of `hmc` is only reachable by unpacking it.

Notifications need a running notification daemon, which every desktop environment
provides; a bare window manager may not.

### macOS

Open the `.dmg` and drag HiveMe to Applications. The binaries are inside the bundle,
at `HiveMe.app/Contents/MacOS/hmg` and `HiveMe.app/Contents/MacOS/hmc`.

The build is not signed or notarised, so the first launch has to be through the
right-click Open menu, or Gatekeeper will refuse it.

### Windows

Run the `.msi` or the NSIS `-setup.exe`. The MSI installs for the machine under
`%ProgramFiles%\HiveMe`, the NSIS installer for the current user under
`%LOCALAPPDATA%\HiveMe`; both add a Start menu entry, and both put `hmc.exe` in that
same folder, which is not on `PATH`.

For the portable archive, unpack it anywhere and run `hmg.exe`. `hmc.exe` is beside
it. Nothing is written outside the folder it sits in, which is what makes it portable.
See [Where the config lives](#where-the-config-lives).

## Installing the CLI

Installing the GUI installs `hmc` too, so this is for a machine that wants only the
CLI, or for putting `hmc` where a shell can find it on the platforms whose installer
does not. It is a single executable with no dependencies:

```sh
# Linux and macOS
chmod +x hmc && sudo mv hmc /usr/local/bin/

# Windows, PowerShell, for the current user
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\Programs\HiveMe" | Out-Null
Move-Item hmc.exe "$env:LOCALAPPDATA\Programs\HiveMe\"
# then add that folder to PATH, or call the executable by its full path
```

Check it with `hmc --version`.

## Where the config lives

Both programs read the same file, and neither has an installer that writes it: it is
created on first run.

| OS | Config file |
|----|-------------|
| Linux | `$XDG_CONFIG_HOME/HiveMe/HiveMe.json`, or `$HOME/.config/HiveMe/HiveMe.json` |
| macOS | `$HOME/Library/Application Support/HiveMe/HiveMe.json` |
| Windows, installed | `%APPDATA%\HiveMe\HiveMe.json` |
| Windows, portable | beside the executable |

Windows decides between the last two by where the executable is: under
`%LOCALAPPDATA%`, `%ProgramFiles%`, or `%ProgramFiles(x86)%` it is an installed build
and uses `%APPDATA%`; anywhere else it is portable and keeps the config next to
itself. So an installed `hmg` and an installed `hmc` share a config, and a portable
copy on a memory stick carries its own.

`--config <path>` and the `HIVEME_CONFIG` environment variable override all of it, for
either program. The message history, `HiveMe.db`, always sits beside the config file.
Full rules in [specs/config.md](specs/config.md#location-and-precedence).

## After installing

1. Create a HiveMQ Cloud cluster and a set of MQTT credentials. See
   [specs/hivemq-cloud.md](specs/hivemq-cloud.md#console-walkthrough).
2. Start `hmg`, open the Settings tab, and fill in the Broker category with the cluster
   URL copied from the console as it is shown there, the username, and the password.
   The protocol is the list beside the URL and starts on TLS MQTT, so there is no
   `mqtts://` to add to what was copied. Save.
3. Press **Copy CLI setup** in that same section and run
   `hmc --init '<paste>'`, which is the whole of setting up the CLI.
4. Send a test message: `hmc "hello"`. It appears in the GUI.

The [README](../README.md#quick-start) walks through the same path in more detail,
and [development.md](development.md#troubleshooting) lists what to do when a step does
not work.

## Updating

`hmg` checks the GitHub releases on the interval in the Settings tab, weekly by
default, and shows a bar at the top of the window when there is a newer version, with
a box to skip that version for good. It downloads and installs nothing: follow the
link, take the new artifacts, and install them over the old ones. The config and the
history are untouched by an install, because neither lives in the install directory.

## Uninstalling

| OS | How |
|----|-----|
| Linux | Remove the HiveMe package with the package manager that installed it; delete the AppImage |
| macOS | Drag HiveMe out of Applications |
| Windows | Apps and Features, or delete the portable folder |

That leaves the config and the message history, which are not in the install
directory. Delete the `HiveMe` folder listed under
[Where the config lives](#where-the-config-lives) to remove those as well. It holds
the broker password, so it is worth removing from a machine that is being handed on.
