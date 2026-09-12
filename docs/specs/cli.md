# HiveMe CLI (`hmc`)

`hmc` publishes one message to the MQTT broker and exits. It is the scripting entry
point for HiveMe and shares its config with the GUI.

## Usage

The block below is the target help text. From step 3.1 onward, the test
`cli_help_matches_spec` renders clap's help and fails if it differs from this block.

```text hiveme:help
hmc [OPTIONS] [MESSAGE]

Arguments:
  [MESSAGE]  Message body. Read from stdin when omitted.

Options:
  -t, --topic <TOPIC>        Topic relative to topics.prefix [default: topics.default]
  -T, --absolute-topic       Treat --topic as an absolute topic
      --json                 Publish MESSAGE (or stdin) as a raw JSON payload without the envelope
      --title <TITLE>        Optional title for the message
  -l, --level <LEVEL>        debug | info | warn | error [default: inferred from matching rule, else info]
  -q, --qos <QOS>            0 | 1 | 2 [default: publish.qos]
  -r, --retain               Set the retain flag
  -c, --config <PATH>        Config file path
  -v, --verbose              Log connection details to stderr
  -h, --help                 Print help
  -V, --version              Print version
```

## Examples

```sh
hmc "Build finished"                  # publish to <prefix>/<topics.default>
hmc -t error "Disk full"              # publish to <prefix>/error
hmc -T -t '$SYS/status' "up"          # publish to an absolute topic
echo "Build finished" | hmc           # read the body from stdin
hmc --json '{"stage":"deploy","ok":true}'
```

## Behaviour

- Exactly one of `MESSAGE` or stdin supplies the body. When the argument is absent
  and stdin is a terminal, `hmc` exits 2 with usage.
- The topic is `topics.prefix` joined with `--topic`, or with `topics.default` when
  `--topic` is absent. `--absolute-topic` skips the prefix. See
  [config.md](config.md#topic-resolution).
- `--level` defaults to the level of the first notification rule whose filter matches
  the topic, and to `info` when no rule matches. A rule is considered here whether or
  not it is `enabled`, because `enabled` governs notifications rather than what a topic
  means.
- `--json` requires the input to parse as JSON and publishes it unchanged, with no
  HiveMe envelope. The MQTT content type is still `application/json`, but the
  `hiveme-v` user property is not set.
- The message is built as described in [message.md](message.md): `v`, a UUID v7 `id`,
  an RFC 3339 `ts`, `type` `message`, and `sender` taken from the `device` block.
- `hmc` connects with a clean start and a session expiry of 0, publishes at the
  configured QoS, waits up to `publish.timeoutSecs` for the acknowledgement at QoS 1
  and 2, then disconnects. QoS 0 returns once the packet is written.
- On success `hmc` prints nothing. Errors go to stderr as a single line:
  `hmc: <category>: <detail>`.
- `hmc` refuses to publish while `encryption.mode` is not `Off`, because encryption is
  designed but not implemented. See [message.md](message.md#encryption).
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
| 2 | Usage error, including `--json` with input that is not JSON |
| 3 | Config error, including a missing config file and an unimplemented encryption mode |
| 4 | Connection or authentication error |
| 5 | Publish timeout |

## First run

When no config file exists, `hmc` writes one with defaults and a freshly generated
`device.id`, prints the path, and exits 3. The user fills in the `broker` block and
runs the command again. See [config.md](config.md#location-and-precedence).

## Out of scope for phase 1

`hmc sub` (tail a topic filter), `hmc config` (init, show, path, set-password), and
`hmc key generate` are designed in the plan and land in phase 6.
