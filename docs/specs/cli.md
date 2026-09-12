# HiveMe CLI (`hmc`)

`hmc` publishes one message to the MQTT broker and exits. It is the scripting entry
point for HiveMe and shares its config with the GUI.

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
      --init <JSON>     Write the config from the setup string hmg shows, then exit
  -t, --topic <TOPIC>   Topic relative to topics.prefix [default: topics.default]
  -T, --absolute-topic  Treat --topic as an absolute topic
      --json            Publish MESSAGE (or stdin) as a raw JSON payload without the envelope
      --title <TITLE>   Optional title for the message
  -l, --level <LEVEL>   debug | info | warn | error [default: inferred from matching rule, else info]
  -q, --qos <QOS>       0 | 1 | 2 [default: publish.qos]
  -r, --retain          Set the retain flag
  -c, --config <PATH>   Config file path
  -v, --verbose         Log connection details to stderr
  -h, --help            Print help
  -V, --version         Print version
```

## Examples

```sh
hmc --init '{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"hiveme-sam","password":"s3cret","prefix":"hiveme"}'
hmc "Build finished"                  # publish to <prefix>/<topics.default>
hmc -t error "Disk full"              # publish to <prefix>/error
hmc -T -t '$SYS/status' "up"          # publish to an absolute topic
echo "Build finished" | hmc           # read the body from stdin
hmc --json '{"stage":"deploy","ok":true}'
```

## Setting up

The broker lives in `hmg`. The Settings tab is where a user pastes the URL,
username, and password from the HiveMQ Cloud console, and it renders those as one line
of JSON to copy. `hmc --init '<json>'` turns that line back into a config:

```sh
hmc --init '{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"hiveme-sam","password":"s3cret","prefix":"hiveme"}'
```

The format is in [config.md](config.md#the-setup-string) and is generated into
`schemas/broker-init.schema.json`, so the two applications cannot disagree about it.

- `--init` is a mode of its own. Everything that shapes a message conflicts with it,
  so an option that would be ignored is refused instead.
- It writes `broker.url`, `broker.username`, `broker.password`, and `topics.prefix`,
  and leaves the rest of the file alone. A `hmc` that has been in use keeps its
  `device.id`, its rules, and any key a newer build wrote.
- It creates the config when there is none, and prints the path it wrote on stdout.
- It never connects, so a setup string can be applied while the cluster is unreachable.
- A string that is not usable is a usage error and nothing is written.
- **The string carries the broker password in plain text.** It is a credential: paste
  it, do not commit it, and do not put it in a shell history that is shared.

`hmc` needs nothing else. There is no separate place to type a URL, and the TLS setup
is automatic: a HiveMQ Cloud cluster presents a certificate that chains to a public
authority, which the operating system already trusts. See
[hivemq-cloud.md](hivemq-cloud.md#how-hiveme-connects).

## Behaviour

### The body

- Exactly one of `MESSAGE` or stdin supplies the body. When the argument is absent
  and stdin is a terminal, `hmc` exits 2 with usage.
- Trailing whitespace is dropped, because the everyday way to reach `hmc` is
  `echo hi | hmc` and the newline `echo` adds is not part of what the user wrote.
  Leading whitespace is kept.
- A body that is empty once trimmed is a usage error. There is no message to send, and
  a notification with no text helps nobody.

### The topic

- The topic is `topics.prefix` joined with `--topic`, or with `topics.default` when
  `--topic` is absent. `--absolute-topic` skips the prefix and may only be given
  together with `--topic`. See [config.md](config.md#topic-resolution).
- The resolved topic is checked before connecting. A wildcard in `--topic` is a usage
  error and a wildcard in `topics.default` is a config error, because they are
  different people's mistakes.

### The message

- `--level` defaults to the level of the first notification rule whose filter matches
  the topic, and to `info` when no rule matches. A rule is considered here whether or
  not it is `enabled`, because `enabled` governs notifications rather than what a topic
  means.
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

- `hmc` writes to stdout only for `--init`, where the line is the path of the config it
  wrote. A publish prints nothing there.
- Errors go to stderr as a single line: `hmc: <category>: <detail>`, where the category
  is `usage`, `config`, `connection`, `timeout`, or `error`, matching the exit codes
  below. A config with several problems is folded onto that one line.
- Warnings go to stderr too, so a successful run over TLS prints nothing at all while
  an unencrypted `mqtt://` broker still says so. `--verbose` adds the connection
  details, and `RUST_LOG` overrides both.
- On Windows `hmc` is a console application, so it never opens a window.

## Client identifier

`<broker.clientIdPrefix>-hmc-<first 8 alphanumerics of device.id>-<8 random
characters>`. The random suffix keeps concurrent `hmc` invocations, and a running
`hmg`, from colliding on the broker, which disconnects duplicate client identifiers.
`hmg` uses the same shape without the suffix, so its session is stable across
restarts.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Unexpected error |
| 2 | Usage error, including `--json` with input that is not JSON and `--init` with a setup string that is not usable |
| 3 | Config error, including a missing config file and an unimplemented encryption mode |
| 4 | Connection or authentication error |
| 5 | Publish timeout |

## First run

The intended path is `hmc --init` with the string from the `hmg` Settings tab, which
creates the config and fills in the broker in one step.

Publishing before that has happened writes a config with defaults and a freshly
generated `device.id`, prints the path on stderr, and exits 3. The user then runs
`--init`, or fills in the `broker` block by hand. See
[config.md](config.md#location-and-precedence).

## Out of scope for phase 1

`hmc sub` (tail a topic filter), `hmc config` (show, path, set-password), and
`hmc key generate` are designed in the plan and land in phase 6. `hmc --init` covers
what `hmc config init` was going to.
