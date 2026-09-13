# HiveMe CLI (`hmc`)

`hmc` publishes one message to the MQTT broker and exits. It is the scripting entry
point for HiveMe and shares its config with the GUI. Run with no arguments on a
terminal, it opens the terminal UI of [tui.md](tui.md) instead, see
[Interactive mode](#interactive-mode); the shell of that UI is built, and its Messages,
Settings, and About tabs are completed by the later phases of
[the terminal UI plan](../plans/plan-terminal-ui.md).

## Usage

The block below is the help text verbatim. The test `cli_help_matches_spec` renders
clap's help and fails if it differs, so the block is the specification rather than a
description of it. On Windows the usage line names `hmc.exe`, because it is taken from
the name the shell used.

```text hiveme:help
HiveMe CLI: send a message to the MQTT broker

Usage: hmc [OPTIONS] [MESSAGE]

Arguments:
  [MESSAGE]  Message body. Read from stdin when omitted

Options:
      --init <JSON>    Initialize the shared config from a setup string, then exit
      --tui            Open the terminal UI, even when stdin is not a terminal
  -t, --topic <TOPIC>  Topic relative to hiveme; leading slashes are ignored [default: hiveme]
      --json           Publish MESSAGE (or stdin) as a raw JSON payload without the envelope
      --title <TITLE>  Optional title for the message
  -l, --level <LEVEL>  debug | info | success | warn | error [default: info; independent of topic]
  -q, --qos <QOS>      0 | 1 | 2 [default: publish.qos]
  -r, --retain         Set the retain flag
  -c, --config <PATH>  Config file path
  -v, --verbose        Log connection details to stderr
  -h, --help           Print help
  -V, --version        Print version

Run hmc with no arguments on a terminal to open the terminal UI.
```

The block is the `en-US` rendering. `hmc` renders the same help in the language of the
config, see [Languages](#languages); `cli_help_matches_spec` compares the `en-US`
rendering, and a second test proves that the doc comments of `cli.rs` and the English
catalog render the same help, the closing sentence about the terminal UI included.

## Examples

```sh
hmc --init '{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"hiveme-sam","password":"s3cret","language":"en-US"}'
hmc                                   # open the terminal UI, see tui.md
hmc --tui --config ./HiveMe.json      # open it on another config
hmc "Build finished"                  # publish to hiveme
hmc --level success "Build succeeded" # success payload on hiveme
hmc --level error "Disk full"         # error payload on the default topic
hmc -t /build/ci "Build finished"     # publish to hiveme/build/ci
echo "Build finished" | hmc           # read the body from stdin
hmc --json '{"stage":"deploy","ok":true}'
```

## Setting up

The broker lives in `hmg`. The Settings tab is where a user pastes the URL,
username, and password from the HiveMQ Cloud console. **Copy CLI setup** copies the
complete command, including the JSON, ready to paste into a terminal and run:

```sh
hmc --init '{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"hiveme-sam","password":"s3cret","language":"en-US"}'
```

The format is in [config.md](config.md#the-setup-string) and is generated into
`schemas/broker-init.schema.json`, so the two applications cannot disagree about it.

- `--init` is a mode of its own. Everything that shapes a message conflicts with it,
  so an option that would be ignored is refused instead.
- It uses `hiveme-core::config::ConfigFile::initialize`, sharing the complete config
  schema, defaults, loader, and atomic writer with `hmg`.
- It compares `broker.url`, `broker.username`, `broker.password`, and, when the string
  carries one, `gui.language` with the existing config. Matching values leave the file
  untouched and print `Config is not changed: <path>` on stdout.
- Different values update only those fields and print `Config has been updated: <path>`.
  All other values, including GUI preferences, device identity, unknown keys, and
  omitted fields, remain intact. The setup string has no topic configuration.
- When there is no file, it creates the full shared config, including defaults for
  fields the CLI does not use, and prints `Config has been created: <path>`.
- It never connects, so a setup string can be applied while the cluster is unreachable.
- The string also carries the language of the `hmg` that copied it, as an optional
  `language` field. `--init` writes it into `gui.language` when it creates the file
  and updates a `gui.language` that differs, so the two applications stay in the same
  language. A string without the field leaves an existing language alone and means
  `en-US` on a fresh file. The outcomes keep their meaning: a language that already
  matches changes nothing.
- The outcome line is in the language of the config as `--init` left it, which is the
  language of the string when the string carries one: a string copied from a German
  `hmg` into a fresh install prints `Konfiguration wurde erstellt: <path>`. The lines
  above are the `en-US` ones.
- A string that is not usable is a usage error and nothing is written.
- An unreadable config or a change to a newer-version config is a config error and
  the existing file is preserved. Unrelated existing settings are validated before
  connecting, so initialization never resets them to make them pass validation.
- **The string carries the broker password in plain text.** It is a credential: paste
  it, do not commit it, and do not put it in a shell history that is shared.

`hmc` needs nothing else. There is no separate place to type a URL, and the TLS setup
is automatic: a HiveMQ Cloud cluster presents a certificate that chains to a public
authority, which the operating system already trusts. See
[hivemq-cloud.md](hivemq-cloud.md#how-hiveme-connects).

## Behavior

### The body

- There are no subcommands. `hmc [OPTIONS] [MESSAGE]` is the whole grammar, so a bare
  word is the message: `hmc config` publishes the body `config`. Every mode is an
  option, as `--init` and `--tui` are.
- Exactly one of `MESSAGE` or stdin supplies the body. When the argument is absent,
  no publish option was given, and stdin is a terminal, `hmc` opens the terminal UI;
  see [Interactive mode](#interactive-mode). With a publish option such as `-t ci` but
  no message on a terminal, `hmc` exits 2 with usage. Piped stdin keeps publishing.
- Trailing whitespace is dropped, because the everyday way to reach `hmc` is
  `echo hi | hmc` and the newline `echo` adds is not part of what the user wrote.
  Leading whitespace is kept.
- A body that is empty once trimmed is a usage error. There is no message to send, and
  a notification with no text helps nobody.

### The topic

- The root is always `hiveme`; there is no configurable topic prefix. Without
  `--topic`, hmc publishes to `hiveme`, regardless of log level.
- `--topic` is always relative to `hiveme`. Leading slashes are removed, so `-t ci`,
  `-t /ci`, and `-t ///ci` all publish to `hiveme/ci`. An empty or slash-only topic
  publishes to `hiveme`. Internal and trailing slashes stay as entered.
- There is no absolute-topic flag. hmc and hmg use the same Rust resolver; hmg uses
  the selected tree topic as its base. See [config.md](config.md#topic-resolution).
- The resolved topic is checked before connecting. An invalid `--topic` is a usage error.

### The message

- `--level` defaults to `info` independently of the MQTT topic and notification rules.
  Input is case-insensitive: `info`, `INFO`, `Info`, and mixed-case spellings are
  accepted and normalized to lowercase before publishing. For example,
  `hmc --level SUCCESS "Build succeeded"` writes `"level":"success"` in JSON.
  `hmc --level warn "Disk at 87%"` and `hmc --level error "Build failed"` both publish
  to `hiveme` with different `payload.level` values under the default configuration.
  `--topic` selects a custom topic without changing the payload level.
- The message is built as described in [message.md](message.md): `v`, a UUID v7 `id`,
  an RFC 3339 `ts`, `type` `message`, and `sender` taken from the `device` block with
  `app` `hmc`.
- `--json` requires the input to parse as JSON and publishes those bytes unchanged,
  with no HiveMe envelope. The MQTT content type is still `application/json`, but the
  `hiveme-v` user property is not set, so a reader is never told that a payload is an
  envelope when it is not. `--title` and `--level` have no place in a payload HiveMe
  did not shape, so they conflict with `--json`.

### The connection

- `hmc` connects with a clean start and a session expiry of 0, publishes at the
  configured QoS, waits up to `publish.timeoutSecs` for the acknowledgement at QoS 1
  and 2, then disconnects. QoS 0 returns once the packet is written. See
  [hivemq-cloud.md](hivemq-cloud.md#how-hiveme-connects).
- The DISCONNECT packet is sent whether or not the publish succeeded, so the broker
  releases the session at once instead of holding one of the connections a Serverless
  cluster allows until the keep alive runs out.
- `--retain` sets the retain flag. Without it the flag is `publish.retain`.
- `hmc` refuses to publish while `encryption.mode` is not `Off`, because encryption is
  designed but not implemented. See [message.md](message.md#encryption).

### Output

- After a successful publish, `hmc` prints `Message sent to <resolved-topic>.` on
  stdout, for example `Message sent to hiveme.`, in the language of the config, see
  [Languages](#languages). At QoS 1 and 2 this follows the
  broker acknowledgement; at QoS 0 it follows writing the packet. A failed publish
  prints no success message. A subsequent disconnect failure still goes to stderr
  and produces a nonzero exit code.
- `--init` reports whether the config was unchanged, updated, or created on stdout,
  followed by the path.
- Errors go to stderr as a single line: `hmc: <category>: <detail>`, where the category
  is `usage`, `config`, `connection`, `timeout`, or `error`, matching the exit codes
  below. A config with several problems is folded onto that one line.
- Warnings go to stderr too. A successful run over TLS normally leaves stderr empty;
  an unencrypted `mqtt://` broker still warns about the plaintext connection.
  `--verbose` adds the connection details, and `RUST_LOG` overrides both.
- On Windows `hmc` is a console application, so it never opens a window.

### Languages

`hmc` speaks the nine languages of `hmg`, from the same catalogs in `locales/` at the
repository root, under their `help` and `cli` keys.

- **Which language.** `gui.language` of the config, resolved as `hmg` resolves it, so
  `de-AT` is German and an unsupported tag is English. Before clap parses the command
  line, `hmc` finds `--config` on it, or `HIVEME_CONFIG`, or the per-OS path, and reads
  the language from that file without writing anything; a missing or unreadable config
  means `en-US`. That language is the one of `--help` and of a usage error found
  before the config is read, such as an empty stdin. A publish then uses the
  language of the config it loaded, and `--init` the language of the config as it left
  it, which is the setup string's when the string carries one.
- **What is translated.** The help, including the `Usage`, `Arguments`, and `Options`
  headings and the descriptions of `--help` and `--version`; the publish confirmation;
  the three init outcomes; and the details `hmc` itself writes: no message, an empty or
  unreadable stdin, `--json` input that is not JSON, the refused encryption mode, the
  default config that was written, a runtime that would not start, `--tui` on a stdout
  that is not a terminal, and a terminal that could not be set up.
- **What is not.** The `hmc: <category>:` prefix and the exit codes, which scripts
  read. The option names, value names, and the `[OPTIONS]` and `[MESSAGE]` placeholders
  of the usage line. clap's own parse errors, such as an unknown option or two options
  that conflict, which clap writes in English with its `error:` prefix. Diagnostics
  authored by `hiveme-core`, such as a setup string that is not usable, a broker
  refusal, a validation message, or the reason a `--topic` is refused, which stay
  English as they do in `hmg`. Topic names, JSON, paths, keys such as `encryption.mode`,
  and identifiers.

The rules the terminal UI adds are in [tui.md](tui.md#languages).

## Interactive mode

*Phase 3 of [the terminal UI plan](../plans/plan-terminal-ui.md).*

- `hmc` with no message argument, no publish option, and a terminal on stdin opens the
  terminal UI of [tui.md](tui.md). `--config` and `--verbose` still apply; nothing
  else may be given.
- `--tui` opens it regardless of stdin. It conflicts with every publish and init
  option, so an option that would be ignored is refused instead, as `--init` already
  is.
- `--tui` on a stdout that is not a terminal is a usage error, exit 2, because there is
  nothing to draw on. It is checked before anything is read or written. The same
  applies when the terminal UI opens without `--tui` and stdout has been redirected.
- With no config file, interactive mode writes the default config with a fresh
  `device.id` and `en-US`, opens on the Settings tab's Broker category, and stays
  disconnected until a broker is entered, as `hmg` does on a fresh install. Publish
  mode keeps its exit 3, see [First run](#first-run).
- While the terminal UI is up nothing is written to stderr; `--verbose` and `RUST_LOG`
  append to `hmc.log` beside the config file instead. See [tui.md](tui.md#logging).
- The terminal UI runs on a multi-thread runtime; publish mode keeps its single
  thread.
- The Quit tool, `Ctrl+Q`, `Ctrl+C`, closing the terminal, and `SIGTERM` all leave the
  terminal UI the same way: the MQTT connection is closed, the broker session is
  discarded, the terminal is restored, and `hmc` exits 0. See
  [tui.md](tui.md#leaving-the-terminal-ui).
- Interactive `hmc` opens `HiveMe.db` beside the config, the same history `hmg` keeps,
  and connects as `Role::Tui`, see [Client identifier](#client-identifier). Both
  applications may run at once; the rules are in
  [session.md](session.md#two-processes-one-installation).

## Appearance

`hmc` has an icon of its own: the same honey colored hive cell as `hmg`, holding a
command prompt rather than a message bubble, so the pair reads as a pair. The source
is `crates/hmc/icons/hmc.png` and the packed sizes are `crates/hmc/icons/hmc.ico`.

Windows reads an icon and a version out of the executable itself, so
`crates/hmc/build.rs` puts them there: the icon, `HiveMe` as the product, the crate
description as the file description, and the workspace version. A build machine with
no resource compiler produces an executable without them and a warning, rather than no
executable at all.

Linux and macOS carry no icon inside a binary; an icon belongs to a desktop entry or
an application bundle, and a command line tool has neither. `hmg` supplies both on
those platforms.

## Client identifier

`<broker.clientIdPrefix>-hmc-<first 8 alphanumerics of device.id>-<8 random
characters>`. The random suffix keeps concurrent `hmc` invocations, and a running
`hmg`, from colliding on the broker, which disconnects duplicate client identifiers.
`hmg` uses the same shape without the suffix to resume its session after a network
interruption. A normal quit ends that session.

Interactive `hmc` connects as `Role::Tui`: the suffixed shape above with
`hmg`'s connection behavior, reconnecting with backoff, resubscribing when the broker
has forgotten the session, keeping `broker.sessionExpirySecs` for the life of the
process, and ending the session on quit. Several interactive `hmc` processes and a
running `hmg` therefore never collide. The role table is in
[hivemq-cloud.md](hivemq-cloud.md#role).

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Unexpected error |
| 2 | Usage error, including `--json` with input that is not JSON, `--init` with a setup string that is not usable, and `--tui` on a stdout that is not a terminal |
| 3 | Config error, including a missing config file and an unimplemented encryption mode |
| 4 | Connection or authentication error |
| 5 | Publish timeout |

## First run

The intended path is to paste and run the command copied from the `hmg` Settings tab.
It creates the config and fills in the broker in one step.

Publishing before that has happened writes a config with defaults and a freshly
generated `device.id`, prints the path on stderr, and exits 3. The user then runs
`--init`, or fills in the `broker` block by hand. See
[config.md](config.md#location-and-precedence).

Interactive mode writes the same default config and opens on the Settings tab's Broker
category instead of exiting, so the broker can be entered there.
See [Interactive mode](#interactive-mode).

## Later phases

- The terminal UI is built by [the terminal UI plan](../plans/plan-terminal-ui.md):
  the shared session in phase 1, the setup string language and the catalogs in phase
  2, the shell in phase 3, and the Messages tab in phase 4 are built; Settings and About
  are phase 5, the end-to-end test phase 6. The status table in [app.md](app.md#status) says
  what has landed.
- There are no subcommands, now or later: a bare word is a message, see
  [The body](#the-body). Every later mode is an option.
- The config and keychain helpers (showing the config, printing its path, storing the
  password in the OS keychain) and the key generation for encryption are designed in
  [the initialization plan](../plans/plan-initialization.md) as options for its phase
  6; their names are settled when they are built. `hmc --init` covers what a config
  initializer was going to.
- A non-interactive tail of a topic filter is superseded by interactive mode, which
  tails every configured subscription; whether an option for it is still wanted is an
  open item in [todos.md](../todos.md).
