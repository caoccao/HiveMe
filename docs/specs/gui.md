# HiveMe GUI (`hmg`)

`hmg` is a Tauri 2 desktop application with a React and Material UI frontend. It
subscribes to the broker, keeps a local history, shows topics as a tree and messages
as a chat, and raises OS notifications from rules.

Its layout and architecture deliberately mirror the sibling project
`../BetterMediaInfo`. See [app.md](app.md#reference-architecture) for the mapping.

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
root node is `topics.prefix`. A node appears when the first message on that topic
arrives or when a message is sent to it, and it persists in SQLite so the tree
survives a restart. Each node carries an unread badge. A filter field sits above the
tree.

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

Sending publishes through the same core path as `hmc`. The bubble appears
immediately as outgoing and is reconciled with the broker echo by `id`.

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

Clicking a notification focuses the window and selects the topic, on the platforms
that support it.

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

Windows requires an installed application identity for notifications, which the Tauri
bundle provides; a `cargo tauri dev` build may not show them. Linux needs a running
notification daemon. A per-OS manual test checklist is added in step 4.6.

## Storage

SQLite, through `rusqlite` with the bundled library, in `HiveMe.db` next to the config
file.

```sql
topics(id INTEGER PRIMARY KEY, topic TEXT UNIQUE, first_seen_ts, last_seen_ts, unread INTEGER)
messages(id INTEGER PRIMARY KEY, topic_id, msg_id TEXT, ts TEXT, received_ts TEXT,
         sender_id TEXT, sender_name TEXT, app TEXT,
         tier TEXT CHECK(tier IN ('envelope','json','text','bytes')),
         level TEXT, title TEXT, body TEXT, raw BLOB,
         qos INTEGER, retain INTEGER, outgoing INTEGER,
         UNIQUE(topic_id, msg_id))
schema_version(version INTEGER)
```

Inserts de-duplicate on `(topic_id, msg_id)`, which is how a GUI-sent message and its
broker echo collapse into one row. Pruning runs at startup and every ten minutes,
using `gui.history.maxMessagesPerTopic` and `gui.history.retentionDays`.

## IPC

The frontend never calls Tauri APIs directly. `src/lib/service.ts` wraps every
`invoke`, and `src-tauri/src/lib.rs` holds one thin `#[tauri::command]` per call that
delegates to `controller.rs`.

Shared types live in `src-tauri/src/protocol.rs` and `src/lib/protocol.ts`, which are
hand-synced: camelCase in TypeScript, snake_case in Rust with `#[serde(rename)]`.
Config and message types are **not** hand-written; they are generated from the JSON
schemas into `src/generated/` and re-exported from `protocol.ts`.

### Commands

| Command | Purpose |
|---------|---------|
| `get_about` | App name, version, and links |
| `get_config` | The effective config |
| `set_config` | Validate, persist, and reconnect when broker or subscription fields changed |
| `get_status` | Connection state snapshot |
| `get_broker_init` | The setup string for `hmc --init`, from the saved broker settings |
| `connect`, `disconnect` | Manual connection control |
| `list_topics` | The topic tree with unread counts |
| `get_messages` | One page of history for a topic: `(topic, before, limit)` |
| `mark_read` | Clear the unread count of a topic |
| `publish` | Publish to a topic: `(topic, body or json, options)` |
| `clear_topic` | Delete the stored history of a topic |
| `get_update_result`, `skip_version` | Update check |
| `open_config_file` | Reveal the config file with the opener plugin |

### Events

| Event | Payload |
|-------|---------|
| `status` | Connection state, host, subscription count, last error |
| `message` | One stored message row plus its topic |
| `topic-added` | A newly seen topic |
| `notification-fired` | The rule id and the message id, for the UI to reflect |

The exact request and response JSON for every command is added in step 4.3.

## Settings

`Config.tsx` renders sections with the reference project's `SectionHeader` pattern.

| Section | Fields |
|---------|--------|
| Broker | URL, username, password with a visibility toggle, keep alive, session expiry, connect timeout, reconnect delays, and **Copy CLI setup** |
| Topics | Prefix, default topic, subscriptions list editor |
| Notifications | Enabled, notify own messages, rules table with add, edit, and delete |
| Appearance | Display mode toggle, theme select, language select |
| Update | Check interval |
| Encryption | Read only placeholder until phase 6 |
| Cloud API | Read only placeholder until phase 6 |

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

`RUST_LOG=debug` turns on backend logging. The Tauri scaffold lands in step 4.1; until
then the frontend is a placeholder.
