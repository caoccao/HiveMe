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
| `src/i18n/` | react-i18next with `en-US` |

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
| Notifications | Pause notifications | Toggle, uses the active colour while paused |
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
is small enough that virtualising the tree would buy nothing. The message list, which
can hold thousands of rows, is virtualised instead.

### Message view

`MessageView.tsx` is a chat view for the selected topic, virtualised, newest at the
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

`Footer.tsx` is the status bar: connection state with a colour, broker host,
reconnect countdown, subscription count, messages received this session, last error
(clicking shows the detail), and database size.

### Snackbar

`NotificationSnackbar.tsx` is a top-centre `Snackbar` with an `Alert`, driven by the
store's `dialogNotification`. It reports command errors and confirmations. It is
in-app feedback and unrelated to OS notifications.

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
minimum height for tabs. Components use theme values through the `sx` prop or the
`styled` API, never hard coded colours.

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
4. Turn `notifications.enabled` off in Settings, save, publish again, and see nothing.
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
  so the tables are created with `IF NOT EXISTS` and the version is written afterwards.
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

- `Status.state` is `Connecting`, `Connected`, `Reconnecting`, or `Disconnected`.
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

`Config.tsx` renders sections with the reference project's `SectionHeader` pattern.

| Section | Config path | Fields |
|---------|-------------|--------|
| Broker | `broker` | `url`, `username`, `password` with a visibility toggle, `clientIdPrefix`, `keepAliveSecs`, `sessionExpirySecs`, `connectTimeoutSecs`, `reconnect.initialDelayMs`, `reconnect.maxDelayMs`, and **Copy CLI setup** |
| Topics | `topics` | `prefix`, `default`, and a `subscriptions` editor where each row is a filter and an "absolute" box |
| Notifications | `notifications` | `enabled`, `notifyOwnMessages`, and a `rules` table of `id`, `topic`, `level`, `enabled`, `title`, `body` with add and delete |
| Appearance | `gui` | `displayMode`, `theme`, `language`, `history.maxMessagesPerTopic`, `history.retentionDays` |
| Update | `update` | `checkInterval` |
| Encryption | `encryption` | Read only placeholder until phase 6 |
| Cloud API | `cloudApi` | Read only placeholder until phase 6 |

A subscription row is written back as a bare string when it is relative and as
`{ "filter": "...", "absolute": true }` when it is not, which is the shape
[config.md](config.md#topic-resolution) describes. A row left blank is dropped rather
than written as an empty filter.

**Save** and **Revert** sit at the bottom, next to a button that shows the config file
in the file manager. Reverting takes the form back to what the backend holds.

Saving calls `set_config`. Validation errors surface in the snackbar. The backend
reconnects when broker or subscription fields changed.

TLS has no fields. A HiveMQ Cloud cluster presents a certificate that chains to a
public authority, which the operating system already trusts, so there is nothing for a
user to configure and nothing for either application to generate. See
[hivemq-cloud.md](hivemq-cloud.md#how-hiveme-connects).

### Copy CLI setup

`hmg` is where a cluster is set up, so it is also where `hmc` is set up. The Broker
section has a **Copy CLI setup** button that puts the setup string of
[config.md](config.md#the-setup-string) on the clipboard, next to the command that
consumes it:

```sh
hmc --init '<paste>'
```

The button is disabled until the broker fields validate, because a string that cannot
be applied is worse than no string. The password is in it, in plain text, and the
button says so.

The backend command is `get_broker_init`; `hiveme-core` renders the string, so the two
applications cannot disagree about the format.

## Window

`window.rs` owns window state, as in the reference project. `setup` sets the title to
`HiveMe v<version>`, restores the size and position from `gui.window`, centres the
window when the stored position is negative, shows the window, and starts the update
check when it is due. `on_window_event` persists size and position on move and
resize, ignoring minimised windows and sizes below 600 x 450.

## Build and run

```sh
pnpm install
pnpm tauri dev       # hot reloading dev build, frontend on http://localhost:1420
pnpm tauri build     # release bundle for the current OS
```

`pnpm test` runs the frontend tests with vitest, configured in `vitest.config.ts`.
`RUST_LOG=debug` turns on backend logging.

The Cargo target directory is the repository root `target/`, because `src-tauri` is a
workspace member, so the bundles are under `target/release/bundle/`.

At startup `hmg` reads the config, opens the history database beside it, restores the
window, connects when a broker is configured, and starts the release check when it is
due. A config file that cannot be read is reported rather than replaced: the window
opens on defaults, the status bar says so, and nothing is written until the user saves
the Settings tab.
