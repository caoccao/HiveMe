# HiveMe GUI (`hmg`)

`hmg` is a Tauri 2 desktop application with a React and Material UI frontend. It
subscribes to the broker, keeps a local history, shows topics as a tree and messages
as a chat, and raises OS notifications from rules.

Its layout and architecture deliberately mirror the sibling project
`../BetterMediaInfo`. See [app.md](app.md#reference-architecture) for the mapping.

## Components

| File | What it is |
|------|------------|
| `src/App.tsx` | The MUI theme, the display mode, and the listeners for the backend events |
| `src/components/Layout.tsx` | The `auto 1fr auto` grid: toolbar, tabs, status bar |
| `src/components/Toolbar.tsx` | Connect, pause notifications, clear the topic, Settings, About |
| `src/components/MainContent.tsx` | The tab machinery and the update notice |
| `src/components/Messages.tsx` | Tab 0: the split pane and its draggable divider |
| `src/components/TopicTree.tsx` | The topic hierarchy and its filter |
| `src/components/MessageView.tsx` | The chat view, its bubbles, and the JSON tree |
| `src/components/Composer.tsx` | The input box, the send button, and its options menu |
| `src/components/Config.tsx` | The Settings tab |
| `src/components/About.tsx` | The About tab |
| `src/components/Footer.tsx` | The status bar |
| `src/components/NotificationSnackbar.tsx` | In-app errors and confirmations |
| `src/lib/store.tsx` | The Zustand store, the only caller of `service.ts` |
| `src/lib/service.ts` | One wrapper per `invoke`; components never call Tauri APIs |
| `src/lib/protocol.ts` | The IPC types, hand synced with `protocol.rs` |
| `src/lib/message.ts` | The reader that renders the parse tiers |
| `src/lib/format.ts` | Times, sizes, and the hex preview |
| `src/lib/constants.ts` | Names, links, and the layout constants |
| `src/i18n/` | react-i18next with nine locales; see [Languages](#languages) |

## Layout

`Layout.tsx` is a CSS grid with rows `auto 1fr auto` inside a `100vh` box.

```
+--------------------------------------------------------------+
| Toolbar        [connect] [pause] [clear] [settings] [about]   |
+--------------------------------------------------------------+
| Tabs:  Messages | Settings x | About x                        |
| +----------------------+-------------------------------------+|
| | filter               | MessageView                         ||
| | TopicTree            |   incoming bubble (left)            ||
| |   hiveme             |            outgoing bubble (right)  ||
| |     info      (2)    |                                     ||
| |     warn             +-------------------------------------+|
| |     error     (1)    | Composer  [ text ......... ] [send] ||
| +----------------------+-------------------------------------+|
+--------------------------------------------------------------+
| Footer: connected | host | 3 subs | 128 msgs | db 1.2 MB      |
+--------------------------------------------------------------+
```

### Toolbar

`ButtonGroup`s of small `IconButton`s with tooltips that name their shortcut.

| Group | Action | Notes |
|-------|--------|-------|
| Connection | Connect / Disconnect | Icon reflects the current state |
| Notifications | Pause notifications | Toggle, uses the active color while paused |
| History | Clear selected topic | Deletes the stored history of the selected topic |
| Tabs | Settings (F10) | Opens or focuses the Settings tab |
| Tabs | About | Opens or focuses the About tab |

### Tabs

`MainContent.tsx` uses MUI `Tabs` with the same `TabControl` and `ControlStatus`
(`Hidden`, `Selected`, `Visible`) pattern as the reference project. Tab 0, Messages,
is fixed. Settings and About open as closable tabs.

Shortcuts: `Ctrl+1` to `Ctrl+9` select a tab, `Ctrl+W` closes the current tab,
`Ctrl+Tab` and `Ctrl+Shift+Tab` cycle, `F10` opens Settings.

### Topic tree

`TopicTree.tsx` renders `@mui/x-tree-view` with the topic hierarchy split on `/`. The
root node is `topics.prefix`, and a topic outside it, such as one under `$SYS`, is a
root of its own. A node appears when the first message on that topic arrives or when a
message is sent to it, and it persists in SQLite so the tree survives a restart. Each
node carries an unread badge, rolled up from everything below it, so a collapsed
branch still shows that something in it is unread.

A node that stands only for a segment of a path, such as `hiveme/build` when the
messages are on `hiveme/build/ci`, is shown in italics and cannot be selected: there
is no history under it to show.

The filter field above the tree matches on the whole path, keeps a parent whose child
matches, and expands what it found.

The component is `SimpleTreeView` with hand-written `TreeItem` children rather than
`RichTreeView`, which the plan left open. `TreeItem` takes a `label` of arbitrary
content, which is what the unread badge needs, and the topic count of a desktop client
is small enough that virtualizing the tree would buy nothing. The message list, which
can hold thousands of rows, is virtualized instead.

### Message view

`MessageView.tsx` is a chat view for the selected topic, virtualized, newest at the
bottom, auto-scrolling unless the user has scrolled up.

| Message | Rendering |
|---------|-----------|
| Sent by this device (`sender.id == device.id`) | Bubble aligned right |
| Sent by anyone else | Bubble aligned left with the sender name and app |
| Envelope tier | `title` in bold, `body`, a collapsed `data` JSON tree, a `level` chip, the time |
| Raw JSON tier | Monospace bubble with a collapsible JSON tree |
| Raw text tier | Monospace bubble |
| Raw bytes | Hex preview with a size label |
| Encrypted | Lock icon and "encrypted (key `<kid>`)" plus sender and time |
| `v` newer than supported | Normal rendering plus a "newer version" chip |

A hover or context menu offers copy body and copy JSON, through the clipboard plugin.

### Composer

`Composer.tsx` sits at the bottom of the message view: a multi-line `TextField` and a
send `IconButton`. Enter sends, Shift+Enter inserts a newline. A small menu on the
send button toggles "send as raw JSON" and overrides QoS and retain for that one
message. The composer is disabled while disconnected or while no topic is selected.

Sending publishes through the same core path as `hmc`, so a message from the composer
is indistinguishable from one `hmc` sent. The bubble appears as soon as the broker has
accepted the message, and not before: a publish that failed must not leave a bubble
claiming it was sent, and the failure goes to the snackbar instead. The copy the broker
echoes back onto the GUI's own subscription collapses into that bubble by its message
id rather than opening a second one.

### Footer

`Footer.tsx` is the status bar: connection state with a color, broker host,
reconnect countdown, subscription count, messages received this session, last error
(clicking shows the detail), and database size.

The state is a `Chip` whose color is read before its word is: green for `Connected`,
amber for `Connecting` and `Reconnecting`, and gray for `Disconnected` and for
anything else. The label comes from `footer.state.<state>`, so the four names arrive
as `connected`, `connecting`, `reconnecting`, and `disconnected`.

### Snackbar

`NotificationSnackbar.tsx` is a top-center `Snackbar` with an `Alert`, driven by the
store's `dialogNotification`. It reports command errors and confirmations. It is
in-app feedback and unrelated to OS notifications.

## Languages

The frontend ships German (`de`), US English (`en-US`), Spanish (`es`), French (`fr`),
Italian (`it`), Japanese (`ja`), Simplified Chinese (`zh-CN`), and Traditional Chinese
for Hong Kong (`zh-HK`) and Taiwan (`zh-TW`). Appearance lists each language by its
native name. A selection applies throughout the window immediately and is saved
automatically, without restarting or pressing a confirmation button.
Startup uses the saved language. English is the default and the fallback for
unsupported tags. Regional tags such as `de-DE` resolve to their bundled language;
Chinese script and region tags resolve to the corresponding Chinese locale.
All English UI text uses US English spelling and terminology.

All frontend labels, tooltips, accessible names, theme and severity labels,
confirmations, empty states, and encrypted-message placeholders use the catalogs.
Counts have the locale's plural forms, and dates, times, numbers, byte sizes, and
reconnect durations follow the selected language. The document language updates too.
Protocol values, topic names, JSON keys and values, message content, device names,
identifiers, URLs, and backend diagnostic details remain as received. OS notification
templates and backend-generated notification summaries are outside these frontend
catalogs.

Catalog tests check key coverage, interpolation variables, and plural forms in every
locale, including keys selected dynamically by connection state, theme, and severity.

## Theme

`App.tsx` creates the MUI theme from `gui.displayMode` and `gui.theme`.
`displayMode` is `Auto` (follows `prefers-color-scheme`), `Light`, or `Dark`.
`gui.theme` selects a palette:

| | | | |
|---|---|---|---|
| Ocean | Aqua | Sky | Arctic |
| Glacier | Mist | Slate | Charcoal |
| Midnight | Indigo | Violet | Lavender |
| Rose | Blush | Coral | Sunset |
| Amber | Sand | Forest | Emerald |

Typography is `fontSize: 12`, and the component defaults are compact: `small` for
buttons, text fields, selects, checkboxes, radios, and icon buttons, and a 36 pixel
minimum height for tabs. `typography.button` sets `textTransform: 'none'`, which is
where buttons, tabs, and toggle buttons all read it from, so a label reads as it was
written and no component has to say so itself. Components use theme values through the
`sx` prop or the `styled` API, never hard coded colors.

## Notifications

A rule maps a topic filter to an OS notification.

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `id` | string | required | Unique within the config. The built-ins are `info`, `warn`, `error`. |
| `topic` | string | required | MQTT topic filter, relative to `topics.prefix` unless `absolute` is true. `+` and `#` are allowed. |
| `absolute` | boolean | false | |
| `level` | string | `info` | The notification level, and the default `payload.level` when publishing to a matching topic. Level inference uses the first matching rule whether or not it is `enabled`, because `enabled` governs notifications rather than what a topic means. |
| `enabled` | boolean | true | |
| `title` | string | `{title|topic}` | Template. |
| `body` | string | `{body}` | Template. |
| `match` | object or null | null | Reserved for payload matching, such as `{ "level": "error" }`. Not implemented. |

### Templates

Placeholders are `{title}`, `{body}`, `{topic}`, `{level}`, `{sender}`, and `{app}`.
The form `{a|b}` uses the first non-empty value. An unknown placeholder renders as an
empty string. There are no expressions.

### Evaluation

1. Skip everything when `notifications.enabled` is false or the toolbar toggle is
   paused.
2. Skip messages whose `sender.id` equals `device.id`, unless
   `notifications.notifyOwnMessages` is true.
3. Check rules in config order and fire the first enabled rule whose filter matches
   the topic.
4. Rate limit to one notification per rule per second. The next notification that rule
   raises carries the count that was held back, as "and N more messages".

A notification carries the rendered title and body and nothing else. The Tauri
notification plugin surfaces no click on the desktop platforms, so focusing the window
and selecting the topic is reserved rather than implemented; the frontend already
hears `notification-fired` and will use it when the plugin can say which notification
was clicked.

### Defaults

When `notifications.rules` is absent, the three built-in rules apply:

| id | topic | level |
|----|-------|-------|
| `info` | `info` | `info` |
| `warn` | `warn` | `warn` |
| `error` | `error` | `error` |

When the key is present, even as an empty array, only the listed rules apply. A user
overrides a built-in by reusing its `id`.

### Platform notes

Windows labels a toast with the identity of the process that raised it, and a Tauri
application raising one through the Windows Runtime has none of its own, so the label
would read PowerShell. `hmg` registers an `AppUserModelId` of its own under
`HKCU\SOFTWARE\Classes\AppUserModelId\HiveMe` at startup, with the display name and,
in a development build, the path of the application icon, and raises its toasts against
that identity rather than through the notification plugin. An installed build takes its
icon from the Start menu shortcut the bundle creates. Every other platform goes through
the plugin, which already labels a notification with the bundle it came from.

Linux needs a running notification daemon. A desktop without one is a reason for a
message to be silent, not for it to be lost: the failure is logged and the message is
still stored and shown.

The manual checklist per OS:

1. Publish to `<prefix>/info`, `<prefix>/warn`, and `<prefix>/error` with `hmc` from
   another terminal, and see three notifications.
2. Publish ten messages to one of them in a second, and see one notification that ends
   with "and N more messages".
3. Turn the toolbar toggle on, publish again, and see nothing.
4. Turn `notifications.enabled` off in Settings, wait for the automatic save, publish
   again, and see nothing.
5. Publish from the composer of the same installation and see nothing, unless
   `notifyOwnMessages` is on.

## Storage

SQLite, through `rusqlite` with the bundled library, in `HiveMe.db` next to the config
file.

```sql
schema_version(version INTEGER NOT NULL)

topics(id INTEGER PRIMARY KEY, topic TEXT NOT NULL UNIQUE,
       first_seen_ts TEXT NOT NULL, last_seen_ts TEXT NOT NULL,
       unread INTEGER NOT NULL DEFAULT 0)

messages(id INTEGER PRIMARY KEY,
         topic_id INTEGER NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
         msg_id TEXT NOT NULL, ts TEXT NOT NULL, received_ts TEXT NOT NULL,
         sender_id TEXT, sender_name TEXT, app TEXT,
         tier TEXT NOT NULL CHECK(tier IN ('envelope','json','text','bytes')),
         level TEXT, title TEXT, body TEXT NOT NULL, raw BLOB NOT NULL,
         qos INTEGER NOT NULL, retain INTEGER NOT NULL, outgoing INTEGER NOT NULL,
         UNIQUE(topic_id, msg_id))

CREATE INDEX messages_topic_id_id ON messages(topic_id, id)
CREATE INDEX messages_received_ts ON messages(received_ts)
```

The module is `hiveme_core::storage`, behind the `storage` feature so that `hmc` never
links SQLite. The database runs in WAL mode.

- `schema_version` holds the version of the layout above. Version 0 means a database
  this build has not stamped, whether it is brand new or older than the table itself,
  so the tables are created with `IF NOT EXISTS` and the version is written afterward.
  A database from a newer build is refused rather than guessed at.
- Inserts de-duplicate on `(topic_id, msg_id)`, which is how a message the composer
  sent and the copy the broker echoes back collapse into one bubble. The second insert
  refreshes only `qos` and `retain`, so the row stays the one this installation sent.
- A payload that is not a HiveMe envelope has no identifier of its own, so one is
  generated. Two identical third party messages are therefore two rows, which is right:
  they are two messages, and only an envelope can claim otherwise.
- `unread` counts a message that is new and came from the broker. What this
  installation published has been seen by definition.
- `raw` is the payload exactly as it arrived, which is what lets the view render every
  tier from the stored row without asking the broker again.
- History is read a page at a time from the newest end and handed back oldest first.
  The cursor is the row id, so paging upwards asks for what is `before` the oldest row
  on screen.
- Clearing a topic deletes its messages and keeps the node, because the user is still
  subscribed to it and asked to forget the messages, not the topic.
- Pruning runs at startup and every ten minutes, using
  `gui.history.maxMessagesPerTopic` and `gui.history.retentionDays`. Either limit set
  to 0 means no limit.

## IPC

The frontend never calls Tauri APIs directly. `src/lib/service.ts` wraps every
`invoke`, and `src-tauri/src/lib.rs` holds one thin `#[tauri::command]` per call that
delegates to `controller.rs`.

Shared types live in `src-tauri/src/protocol.rs` and `src/lib/protocol.ts`, which are
hand-synced: camelCase in TypeScript, snake_case in Rust with `#[serde(rename)]`.
Config and message types are **not** hand-written; they are generated from the JSON
schemas into `src/generated/` and re-exported from `protocol.ts`.

Every command answers `Result<T, String>`, and the error is the one line the snackbar
shows.

### Commands

| Command | Request | Response |
|---------|---------|----------|
| `clear_topic` | `{ "topic": "hiveme/info" }` | how many messages were deleted |
| `connect` | none | `Status` |
| `disconnect` | none | none |
| `get_about` | none | `About` |
| `get_broker_init` | none | the setup string for `hmc --init` |
| `get_config` | none | the config, as `schemas/config.schema.json` describes it |
| `get_messages` | `{ "topic": "hiveme/info", "before": 42, "limit": 200 }` | `MessageRow[]`, oldest first |
| `get_status` | none | `Status` |
| `get_update_result` | none | `{ "hasUpdate": false, "latestVersion": null }`, or nothing while the check is still running |
| `list_topics` | none | `TopicNode[]` |
| `mark_read` | `{ "topic": "hiveme/info" }` | none |
| `open_config_file` | none | none |
| `publish` | `{ "topic": "hiveme/info", "body": "Build finished", "options": PublishOptions }` | the stored `MessageRow` |
| `set_config` | `{ "config": Config }` | the config as it was saved |
| `set_notifications_paused` | `{ "paused": true }` | `Status` |
| `skip_version` | `{ "version": "0.2.0" }` | none |

`before` and `limit` in `get_messages` may both be null: no cursor means the newest
page, and no limit means the default of 200.

`connect` replaces a connection that is already up, which is what a saved change to
the broker or the subscriptions needs. `set_config` validates, writes, recompiles the
notification rules, and reconnects only when something the CONNECT packet or the
subscription list is built from changed: changing a theme does not drop the
connection, and changing a password does.

`get_broker_init` fails while the broker fields are not usable, because a setup string
that cannot be applied is worse than no string. It carries the password in plain text,
so it is never logged.

`set_notifications_paused` is the toolbar toggle. The rules and the rate limiter live
in the backend, so the pause does too; it lasts for the session and is not written to
the config.

### Types

```
About          = { appVersion, configPath, databasePath, deviceId, deviceName, githubUrl }
Status         = { state, host, port, clientId, subscriptions, attempt, retryInMs,
                   lastError, messagesReceived, databaseBytes, notificationsPaused, configError }
TopicNode      = { id, label, topic, unread, messages, children: TopicNode[] }
MessageRow     = { rowId, topic, id, ts, receivedTs, senderId, senderName, app,
                   tier, level, title, body, raw, rawLength, qos, retain, outgoing }
PublishOptions = { json?, qos?, retain?, title?, level? }
```

- `Status.state` is `Connecting`, `Connected`, `Reconnecting`, or `Disconnected`,
  typed as `ConnectionState` rather than as a string, and compared by name: the
  composer sends only on `Connected`, and the toolbar offers Disconnect on the other
  three. `hiveme_core::State::as_str` is where those names are spelled.
  `retryInMs` is how long the pending reconnect waits at the moment the status
  changed, so the footer counts down from it rather than being sent a stream of
  events. `lastError` survives a recovery, so the reason a connection dropped, or the
  filter a credential may not subscribe to, stays readable. `configError` is set when
  the config file could not be read, which is why nothing is connected.
- `TopicNode.topic` is set only on a node a message has been stored on. An
  intermediate segment such as `hiveme/build` exists in the tree and cannot be
  selected. `unread` and `messages` are rolled up, so a collapsed branch still shows
  that something below it is unread.
- `MessageRow.raw` is the payload as text, or as hex for the `bytes` tier, because a
  payload that is not valid UTF-8 cannot travel as JSON text. `rawLength` is the byte
  count. `src/lib/message.ts` reads `raw` again to render the envelope and the JSON
  tree, which is what keeps the two readers comparable.
- `PublishOptions.json` publishes the body as a raw JSON payload with no envelope, as
  `hmc --json` does, and refuses input that is not JSON. An absent `qos` or `retain`
  means the configured default, and an absent `level` means the level of the first
  rule whose filter matches the topic.

### Events

| Event | Payload |
|-------|---------|
| `status` | `Status` |
| `message` | `MessageRow` |
| `topic-added` | `{ "topic": "hiveme/info" }` |
| `notification-fired` | `{ "ruleId": "error", "messageId": "018f6b1e-...", "topic": "hiveme/error" }` |

A `message` event is emitted once per stored row, whether the message arrived or the
composer sent it. The echo of a message this installation published raises no second
event, because it collapses into the row that is already there.

## Settings

`Config.tsx` divides the settings into categories, as the reference project does. A
vertical `Tabs` strip on the left lists them and the panel beside it shows one at a
time, so the page asks for a handful of related fields rather than for all of them at
once. Each panel is built from the reference project's `SectionHeader` pattern, and a
panel with more than one group of fields puts every group after the first into an
outlined card with a header of its own.

Appearance is the first category and opens by default. The page uses the reference
project's centered, 960 px maximum width and sidebar spacing. Appearance follows its
`SettingRow` layout: Mode, Theme, and Language labels on the left, controls aligned
on the right, and horizontal dividers between rows. Mode uses compact Auto Mode,
Light Mode, and Dark Mode buttons with icons; Theme and Language use dropdown lists.

| Category | Config path | Groups and fields |
|----------|-------------|-------------------|
| Appearance | `gui` | `displayMode`, `theme`, `language` |
| Broker | `broker` | `url` as a protocol list and the rest of the URL, then `username` and `password` on one row, with a visibility toggle and **Copy CLI setup**; **Connection** with `clientIdPrefix`, `keepAliveSecs`, `sessionExpirySecs`, `connectTimeoutSecs`; **Reconnect** with `reconnect.initialDelayMs`, `reconnect.maxDelayMs` |
| Topics | `topics` | `prefix`, `default`; **Subscriptions**, where each row is a filter and an "absolute" box |
| Notifications | `notifications` | `enabled`, `notifyOwnMessages`; **Rules**, a table of `id`, `topic`, `level`, `enabled`, `title`, `body` with add and delete |
| History | `gui.history` | `maxMessagesPerTopic`, `retentionDays` |
| Update | `update` | `checkInterval` |
| Advanced | `encryption`, `cloudApi` | Read only placeholders until phase 6 |

`broker.url` is one string in the config file, and `src/lib/brokerUrl.ts` takes the
scheme off the front of it for the form and puts it back afterward. That is the whole
of what it does: the protocol is a list, and the rest of the URL is one box holding
exactly what the user put there.

The HiveMQ Cloud console shows a cluster three ways, as `host`, as `host:8883`, and as
`host:8884/mqtt`, and none of the three has a scheme in front of it. Each is pasted
into the box as it stands and saved as it stands, which is why the port and the path
are not boxes of their own: splitting them out would mean rebuilding a string the user
had already written, and a URL that came back from HiveMe differing from the one that
went in is a bug waiting for somebody to report it. `hiveme_core::config::url` is still
the one thing that reads a host, a port, and a path out of a URL.

The protocol list is the transports the backend speaks, headed by TLS MQTT, which a new
install starts on because it is the only one a HiveMQ Cloud cluster accepts. A URL
pasted with a scheme on it is read rather than refused, moving the list and leaving the
box with the rest. Under the box is the URL that will be saved and the port it will
connect on, which is worth saying because a URL that carries no port connects on the
protocol's default and nothing on screen would otherwise say which port that is.

Select Broker to configure a cluster. All edits update the shared application state
immediately, including language, theme, and display mode. They survive switching
categories or closing and reopening the Settings tab.

A subscription row is written back as a bare string when it is relative and as
`{ "filter": "...", "absolute": true }` when it is not, which is the shape
[config.md](config.md#topic-resolution) describes. Clearing a filter keeps its row
editable so it can be replaced. An invalid filter is rejected by backend validation;
the row's remove button deletes an unwanted subscription.

Settings use automatic saving, following BetterMediaInfo: edits are persisted through
`set_config` after a 500 ms pause. There are no Save, Revert, or Open config buttons.
The store owns the timer, so closing the Settings tab does not cancel a pending save.
Writes run one at a time, with newer edits saved after a write already in progress;
an older response never replaces newer values on screen. Successful saves are silent.
Validation or write failures surface in the snackbar and keep the edited values
visible; the next edit retries the latest configuration. The backend applies accepted
changes and reconnects when broker or subscription fields changed.

TLS has no fields. A HiveMQ Cloud cluster presents a certificate that chains to a
public authority, which the operating system already trusts, so there is nothing for a
user to configure and nothing for either application to generate. See
[hivemq-cloud.md](hivemq-cloud.md#how-hiveme-connects).

### Copy CLI setup

`hmg` is where a cluster is set up, so it is also where `hmc` is set up. The Broker
category has a **Copy CLI setup** button that puts the setup string of
[config.md](config.md#the-setup-string) on the clipboard, next to the command that
consumes it:

```sh
hmc --init '<paste>'
```

The button is disabled until the broker fields validate, because a string that cannot
be applied is worse than no string. The password is in it, in plain text, and the
button says so.

The backend command is `get_broker_init`; `hiveme-core` renders the string, so the two
applications cannot disagree about the format. Copy CLI setup first flushes pending
settings and waits for the write to finish, so the copied string uses the current
credentials. A failed save prevents copying an older setup string.

## Window

`window.rs` owns window state, as in the reference project. `setup` sets the title to
`HiveMe v<version>`, restores the size and position from `gui.window`, centers the
window when the stored position is negative, shows the window, and starts the update
check when it is due. `on_window_event` persists size and position on move and
resize, ignoring minimized windows and sizes below 600 x 450.

## Build and run

```sh
pnpm install
pnpm tauri dev       # hot reloading dev build, frontend on http://localhost:1420
pnpm tauri build     # release bundle for the current OS
```

Only the Tauri CLI produces a binary that runs on its own. It passes
`--features tauri/custom-protocol`, which is what makes `tauri` compile `frontendDist`
into the binary rather than load `devUrl`, and cargo never passes it, so a `cargo build`
of `hmg` gives a window that is still looking for the Vite server whatever the profile.
Both builds run the same backend. See
[development.md](../development.md#running-hmg).

The bundle carries `hmc` beside `hmg`, taken from `target/release/hmc`, so that
installing the GUI installs the CLI that reads the config it writes. The hook commands
of `tauri.conf.json` build it, and each bundler is told where to put it. See
[development.md](../development.md#packaging-hmc) and [app.md](app.md#install).

`pnpm test` runs the frontend tests with vitest, configured in `vitest.config.ts`.
`RUST_LOG=debug` turns on backend logging.

The Cargo target directory is the repository root `target/`, because `src-tauri` is a
workspace member, so the bundles are under `target/release/bundle/`.

At startup `hmg` reads the config, opens the history database beside it, restores the
window, connects when a broker is configured, and starts the release check when it is
due. A config file that cannot be read is reported rather than replaced: the window
opens on defaults, the status bar says so, and nothing is written until the user edits
a setting.
