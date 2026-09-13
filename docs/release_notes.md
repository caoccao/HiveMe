# Release Notes

## Unreleased

* `hmg` and the terminal UI of `hmc` run side by side on one installation: a message
  that arrives shows up live and raises its notification in both, where one of them used
  to take it for a message already seen and stay silent.

* Closing the terminal window under the terminal UI of `hmc` still ends the broker
  session, and on Linux and macOS `hmc` now exits with 0 afterward rather than 101.

* On macOS the notifications of `hmc` are labeled HiveMe when HiveMe.app is installed,
  as those of `hmg` are.

* The README walks from nothing to a live session in the terminal UI, without the GUI.

* The terminal UI of `hmc` has all of Settings and the About tab. Appearance applies a
  display mode, a theme, or a language to the whole screen at once; Broker has the
  protocol list, the line that says which URL and port will be used, the connection and
  reconnect numbers, and **Copy CLI setup**, which copies the complete `hmc --init`
  command; Topics edits the subscriptions and Notifications the switches and the rules;
  History and Update have their limits and interval. Edits are saved half a second after
  the last one, as in `hmg`, and a URL saved in either application reads the same in the
  other. About shows HiveMe in large letters and opens the author's and the repository's
  pages.

* The terminal UI of `hmc` has its Messages tab: the topic tree with its filter and
  unread badges, the chat view with the bubbles, levels, and data trees of `hmg`, and the
  composer with the Level select and More Options. `Enter` sends, `Alt+Enter` or
  `Ctrl+J` adds a line, `PageUp` at the top loads older messages, `c` and `r` copy a
  message, and `Enter` on a message opens it to browse its data node by node. The divider
  moves with `Ctrl+Left` and `Ctrl+Right` or the mouse. Copying works over SSH too, in
  terminals that accept the OSC 52 clipboard sequence.

* `hmc` run with no arguments in a terminal opens a terminal UI, and `hmc --tui` opens
  it on purpose. It connects to the cluster of the shared config, raises the same OS
  notifications as `hmg`, and has the toolbar (connect, pause notifications, clear the
  topic, Settings, About, help, and quit), the tabs, the status bar, the update notice,
  and a key help on `?`. A fresh install opens on the broker URL, username, and password
  so the cluster can be entered without leaving the terminal. `Ctrl+Q`, `Ctrl+C`, the
  Quit tool, and closing the terminal all end the broker session before `hmc` exits.
  `echo hi | hmc` and every publish option work as before.

* `hmc` speaks the language of the config in all nine languages: `--help`, the publish
  confirmation, the `--init` outcomes, and its own usage errors follow `gui.language`.
  The `hmc: <category>:` prefix and the exit codes are unchanged, so scripts keep
  working.

* **Copy CLI setup** now carries the window's language, and `hmc --init` writes it to
  `gui.language`, so a CLI set up from a German `hmg` answers in German. A setup string
  without a language still works and leaves the language alone.

* CLI level input is case-insensitive. Levels are normalized to lowercase in the
  shared message model, generated JSON, and database level fields; only UI labels
  use an initial capital.

* Capitalized visible level labels throughout the UI. The composer dropdown uses
  normal text color for Info and MUI error, success, and warning colors for Error,
  Success, and Warn, including both its selected value and menu items.

* Added a level dropdown left of More Options: Info (default), Error, Success, and
  Warn. Its selection is remembered with the entire composer state per topic in
  memory, including while options are collapsed. Raw JSON preserves the selection
  without applying it to the payload.

* Added the `success` payload level to CLI publishing, message parsing, and
  notification-rule choices. Its message bubble and badge use MUI's success palette
  in light and dark modes, with labels translated into all nine languages.

* Fixed the topic filter spacing to 4 px at the top, left, right, and below the field.

* Enter now sends from any focused control in the message input panel, including
  option fields, radios, checkboxes, and buttons. Shift+Enter still adds a line in
  the message box, and text composition does not send.

* Messages show their relative topic path in muted gray below the bubble, immediately
  before the level badge. The entire row appears only on hover. Paths update with
  selection and are omitted when empty. Incoming sender names or IDs appear above the bubble, without the
  application name; outgoing and senderless messages omit the sender header.

* Message hover controls now show the level badge, QoS, timestamp, and a split copy
  button with Copy and Copy Raw JSON in its menu, aligned below the bubble.

* Tightened the message composer's outer spacing and field padding to 4 px, including
  the right and bottom panel edges. The input starts at three lines, with compact
  controls and both checkboxes on the QoS row, separated from the radio group and
  each other by 16 px.

* Made `hiveme` the fixed root and removed topic-prefix configuration from settings,
  config files, and CLI setup strings. Publish input is always relative, with leading
  slashes stripped: hmc resolves under `hiveme`, and hmg under the selected topic.
  Removed hmc’s absolute-topic flag.

* Redesigned the message composer with a full-width multi-line input, right-aligned
  More Options and Send buttons, and three expandable option rows. Topic, title,
  QoS, retain, and raw JSON settings remain active when collapsed. Drafts and options
  are remembered per topic in memory only and reset when the app restarts.

* Fixed broker echoes racing with a GUI send: sent messages now keep their outgoing
  status and the QoS and retain options used to publish them.

* Added a native end-to-end test covering CLI confirmation, MQTT topics and payload
  levels, GUI rendering and sending, settings, tree expansion, and SQLite history
  after restart. Linux CI runs it with a real local broker and WebDriver.

* Topic labels select messages without expanding or collapsing children. Only the
  icon to the left toggles expansion.

* Removed the default-topic setting from the shared config and settings UI. Without
  an explicit topic, hmc always publishes to `hiveme`.

* Selecting a topic now shows its direct messages and every recursive child topic.
  Parent paths without direct messages are selectable. History is filtered in SQL,
  paginated across the subtree, and updated live as descendant messages arrive.
  Selecting a parent also clears its descendants' unread counts.

* hmg now disconnects MQTT and ends its broker session before quitting. Startup and
  reconnection cannot keep the application alive indefinitely when the broker is
  unreachable. Explicit disconnect and connection replacement also end the session.
* hmc now prints `Message sent to <topic>.` after a successful publish, including
  the resolved topic. Failed publishes do not print a success message.

* Restyled messages with rounded bubbles and more padding. Timestamps, direct copy,
  and options appear below the bubble only on hover, without shifting
  the layout. Payload level and delivery details appear in the same hover row.

* All log levels now publish to `hiveme` by default and remain in the JSON payload.
  Notification rules match both the MQTT topic and payload level. Message boxes use
  regular colors for info, warning colors for warn, and error colors for error.
* hmg always shows, selects, and highlights `hiveme` at startup, even without history,
  or while disconnected. Existing history keeps its original
  topic paths. No config or history migration was added.

* Moved CLI config initialization into the shared Rust config library. Matching
  setup values leave the existing file untouched; changed values update only those
  fields and preserve other settings. New files include the complete shared defaults.
  The CLI reports whether the config was unchanged, updated, or created.
* Copy CLI setup now copies the complete `hmc --init` command with quoted JSON,
  ready to paste and run. Apostrophes in credentials are preserved with JSON escapes.
* Made Appearance the first and default settings category. Copied BetterMediaInfo's
  settings spacing and appearance rows, with mode buttons and dropdowns aligned
  on the right.
* Standardized English text on US English across the UI, backend diagnostics, CLI
  and CI messages, documentation, and generated schema descriptions.
* Localized the frontend in German, US English, Spanish, French, Italian, Japanese,
  Simplified Chinese, and Traditional Chinese for Hong Kong and Taiwan. The language
  picker now applies the selection immediately and saves automatically. Theme names,
  message levels, encrypted previews, accessible labels, plural counts, dates, and
  numeric formatting follow the selected language.
* Made all settings apply immediately and save automatically after a short typing
  pause, following BetterMediaInfo. Removed Save, Revert, and Open config buttons.
  Pending writes survive closing the Settings tab, and Copy CLI setup waits for
  current edits to be saved before copying.

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
  topic relative to `hiveme`, `--json` for a payload HiveMe
  does not shape, a payload level independent of the topic,
  and an exit code per kind of failure so a script can tell a wrong password from an
  unacknowledged message. A first run with no config writes one and says where.
* Made `hmg` the one place a HiveMQ Cloud cluster is set up. Its Settings tab will show
  a one line setup string; `hmc --init '<json>'` turns that string into a config, so
  there is no URL, username, or password to retype into the CLI. The format is
  generated into `schemas/broker-init.schema.json` from the same Rust type both
  applications use.
* Said plainly that only Serverless clusters are supported for now, and that TLS is
  automatic for them: the cluster certificate chains to a public authority the
  operating system already trusts, so neither application generates, enrolls, or asks
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
  back for the session. On Windows the notification is labeled HiveMe, because the
  application registers an identity of its own instead of borrowing the one of whatever
  process raised the toast.
* Added a status bar that shows the connection, the broker, the reconnect countdown,
  the subscriptions, the messages this session, and the size of the history.
* Drew the pair an icon: a honey colored hive cell holding a message bubble for `hmg`
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
* Read a broker URL that names no scheme as TLS MQTT, everywhere a URL is read: the
  config file, `hmc --init`, and the Settings form alike. The HiveMQ Cloud console shows
  a cluster as `host` or `host:8883`, and TLS MQTT is the only transport a cluster
  accepts, so refusing that string for the scheme it does not have was a rule that
  taught nobody anything. A scheme that is written is still the one that is used.
* Moved the scheme of the broker URL out of the URL box and into a list beside it. The
  HiveMQ Cloud console shows a cluster as `host`, `host:8883`, or `host:8884/mqtt`, and
  every one of those is now copied straight across: the box keeps what was pasted, and
  the list says whether it is MQTT, TLS MQTT, TLS WebSocket, or WebSocket. It starts on
  TLS MQTT, which is all a cloud cluster accepts. A URL pasted with a scheme still on it
  moves the list instead of being refused, and the line under the box says which URL
  will be saved and which port it will connect on, since a URL that names no port
  connects on the protocol's own.
* Fixed a connected `hmg` that behaved as though it were not: the composer stayed
  disabled, the badge stayed gray, and the toolbar still offered Connect. The backend
  named the state `connected` while the frontend was looking for `Connected`, so the
  one place the two ever compared it always disagreed. The state now travels under the
  name both halves already documented, and a test on each side holds it there.
* Divided the Settings tab into categories, listed down the left as the reference
  project lists them: Broker, Topics, Notifications, Appearance, History, Update, and
  Advanced. One page of every setting at once was more than anyone needed to read to
  change one of them. An edit made in one survives a move to another.
