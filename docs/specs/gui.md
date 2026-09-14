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
| `src/components/Composer.tsx` | The input box, the send button, and its collapsible options |
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
| `src/i18n/` | react-i18next with nine locales; see [Languages](#languages). The catalogs themselves are `locales/*.json` at the repository root, shared with `hmc` |

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
| |     build     (2)    |                                     ||
| |       ci             +-------------------------------------+|
| |     deploy    (1)    | Composer  [ text ......... ] [send] ||
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
`hiveme` topic is always visible and selectable, even with no database rows, after
clearing history or while disconnected. The root is fixed and has no config setting.
Every startup selects `hiveme`, highlights it, and loads its entire subtree, so the composer
is ready as soon as the connection is available. The selected topic has a highlighted
background and a bold label in the theme color. Clicking a topic label selects it
without changing expansion. Only the icon to its left expands or collapses its
children; the standard keyboard arrow controls remain available.

Other nodes follow actual stored MQTT paths split on `/`. A node appears when a
message arrives or is sent to it, and persists in SQLite. Each node carries an unread
badge rolled up from its descendants. Log levels are never added as synthetic topic
nodes. Historical entries under paths such as `hiveme/info`, `hiveme/warn`, and
`hiveme/error` stay on their original paths. No database migration is added.

Every nonempty topic path is selectable, including a parent such as `hiveme/build`
when messages exist only on `hiveme/build/ci`. Selecting it displays its direct
messages and all recursive descendants and marks that subtree read. Matching is
case sensitive and requires a `/` boundary: `hiveme/building` is not a child of
`hiveme/build`. The empty leading segment of an absolute path is only a group.

The filter field has 4 px margins from the top and sides of the topic pane and a
4 px gap before the tree. It matches on the whole path, keeps a parent whose child
matches, and expands what it found. `hiveme` stays visible even when the filter does
not match it.

The component is `SimpleTreeView` with hand-written `TreeItem` children rather than
`RichTreeView`, which the plan left open. `TreeItem` takes a `label` of arbitrary
content, which is what the unread badge needs, and the topic count of a desktop client
is small enough that virtualizing the tree would buy nothing. The message list, which
can hold thousands of rows, is virtualized instead.

### Message view

`MessageView.tsx` is a chat view for the selected topic and all recursive descendants,
virtualized, newest at the bottom, auto-scrolling unless the user has scrolled up.
The database filters each page by the stored topic field. Rows retain their actual
MQTT topic; the frontend caches each selected subtree and merges live descendant
messages into every matching cached view, including a view whose first page is still
loading. Database row ids preserve order and prevent duplicate bubbles across pages
and events, while the same envelope on two topics remains two rows.

| Message | Rendering |
|---------|-----------|
| Sent by this application (`outgoing`, see [session.md](session.md#which-side-a-message-is-on)) | Rounded bubble aligned right; regular messages use a dark fill with white text |
| Sent by anyone else, `hmc` on the same device included | Rounded bubble aligned left with available sender details above it |
| Envelope tier | `title` in bold, `body`, and a collapsed `data` JSON tree; metadata is in the hover row |
| Raw JSON tier | Monospace bubble with a collapsible JSON tree |
| Raw text tier | Monospace bubble |
| Raw bytes | Hex preview with a size label |
| Encrypted | Lock icon and "encrypted (key `<kid>`)" plus sender |
| `v` newer than supported | Normal rendering with a "newer version" chip in the hover row |

Each message shows its topic path relative to the selected tree topic,
including messages sent by the current device. For example, selecting `hiveme/a`
shows `b/c` below a message on `hiveme/a/b/c`, immediately to the left of its level
badge. A message on the selected topic has an
empty relative path, so no topic label is shown. The path updates when selection
changes and preserves the topic's original characters and levels. The path uses a
muted gray from the active theme to distinguish it from the sender details.

Incoming messages show the sender's name above the bubble, falling back to the
sender ID. Application names are neither displayed
nor used as a sender fallback. Outgoing messages omit the sender. A message without
sender details omits the header, without an unknown-sender placeholder. Sender
headers stay visible. Relative paths appear with the entire metadata row only
while the pointer is over the message. Labels wrap when needed.

Bubble colors follow `payload.level`, independently of the topic or direction:
`info` and `debug` use the regular background. `success` uses MUI's success palette,
`warn` uses its warning palette, and `error` uses its error palette for the border,
tinted bubble background, and level badge.
Colors come from the active theme and work in both light and dark modes. Unknown
levels display as `info` while preserving the raw level label.

Bubbles use generous padding, 24 px corners, and readable message text. A separate
row below each bubble is aligned to its right edge and contains the relative path,
a rectangular level badge, QoS, timestamp, and a split copy button. Retained status
and a newer-version marker also appear here when applicable. The entire row,
including the relative path, is hidden until the pointer is over the message or
its controls and hides again when the pointer leaves. Its space is reserved so
hovering does not move adjacent messages.

The timestamp shows hours and minutes in the selected language, with the full date
and time in its tooltip. The split button copies the body directly; its arrow opens
a right-aligned menu containing **Copy** and **Copy Raw JSON**. Both copy actions use the
clipboard plugin and report success or failure through the snackbar.

### Composer

`Composer.tsx` sits at the bottom of the message view. A multi-line `TextField`
fills the available width with 4 px padding around the composer and 4 px gaps
between its rows and most controls. The Messages tab adds no extra padding, so the right
and bottom gaps to the panel's inner border are also 4 px. Fields have 4 px internal
padding, buttons and selection controls use compact padding, and the message input
starts at three lines and grows to six. Below it, a **Level** dropdown,
**More Options**, and **Send** share a row aligned to the right. The dropdown sits
immediately left of More Options and offers **Info**, **Error**, **Success**, and
**Warn**, in that order, with Info selected by default. It sets `payload.level` and
uses normal MUI text color for Info, `error.main` for Error, `success.main` for
Success, and `warning.main` for Warn, both in the menu and in the selected value. It
stays available when More Options is collapsed. Its selection is preserved but
disabled in raw JSON mode, which publishes the supplied JSON unchanged. The chevron to the
right of More Options indicates whether the options below are expanded or collapsed.
Enter sends from any focused composer control: the message box, closed Level dropdown, Topic, Title, QoS
radios, checkboxes, More Options, or Send. It does not also activate the focused
control. Inside the open dropdown menu, Enter selects the highlighted level without
sending. Shift+Enter inserts a newline in the message box. Enter used for text
composition does not send. The composer is disabled while disconnected or while
no topic is selected.

The options start collapsed and contain three rows:

1. **Topic**: relative to the selected tree topic. An empty field uses that topic
   itself. Leading slashes are stripped: with `hiveme/build` selected, `ci` and `/ci`
   both publish to `hiveme/build/ci`. Resolution uses the same Rust function as hmc.
2. **Title**: empty by default. Preserved but disabled and excluded from publishing
   when raw JSON is selected.
3. **QoS**: radio buttons **Config**, **0**, **1**, **2**, followed on the same row
   by **Retain Message** and **As Raw JSON**. Config is selected by default and uses
   the current configured QoS at send time. Both checkboxes start unchecked. An
   unchecked Retain Message explicitly sends without retain, regardless of the config
   default. The QoS group and each checkbox are separated by 16 px horizontally.
   Controls wrap with a 4 px row gap when the pane is too narrow to fit them on one line.

Collapsing the panel only hides the controls; every option remains in effect.
The message draft, selected level, option values, and expansion state are remembered separately for
each selected tree topic in window memory. Selecting a previously unused topic starts
with the defaults; returning to a topic restores its values, including after visiting
another tab or reconnecting. They are never written to config, browser storage, or
SQLite and reset when the window restarts. Successful sending clears only the
originating topic's message text, preserving its options; failure preserves the text
as well. A send completing after selection changes cannot clear another topic's draft.

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
Visible level labels start with a capital letter and share one translation catalog
across the composer, message badges, and notification-rule choices. Protocol values
remain lowercase in generated JSON and the database's `level` field. CLI level
arguments accept any capitalization and are normalized before publishing.

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

The nine catalogs live in `locales/` at the repository root, one set for both
applications: `src/i18n/index.ts` imports them from there, and `hiveme_core::i18n`
embeds the same files for `hmc`. The keys the CLI adds sit in the same files under the
`cli` and `help` namespaces, and the terminal UI adds `tui`, so the catalog test above
covers them as well; the frontend does not use them. Which strings `hmc` translates is
in [cli.md](cli.md#languages) and [tui.md](tui.md#languages).

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

A rule matches an MQTT topic filter and a payload log level to an OS notification.
The rules below are the shared rules: their evaluation, the rate limiter, and the
pause toggle live in the session of [session.md](session.md#notifications), and each
application supplies only the final call that shows the toast. Interactive `hmc` raises
the same notifications, see [tui.md](tui.md#notifications).

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `id` | string | required | Unique within the config. The built-ins are `info`, `warn`, `error`. |
| `topic` | string | required | MQTT topic filter, relative to `hiveme` unless `absolute` is true. `+` and `#` are allowed. |
| `absolute` | boolean | false | |
| `level` | string | `info` | The `payload.level` to match: `debug`, `info`, `success`, `warn`, or `error`. The topic never determines a message's level; publishing defaults to `info`. |
| `enabled` | boolean | true | |
| `title` | string | `{title|topic}` | Template. |
| `body` | string | `{body}` | Template. |
| `match` | object or null | null | Reserved for additional payload conditions. Not implemented. |

### Templates

Placeholders are `{title}`, `{body}`, `{topic}`, `{level}`, `{sender}`, and `{app}`.
The form `{a|b}` uses the first non-empty value. An unknown placeholder renders as an
empty string. There are no expressions.

### Evaluation

1. Skip everything when `notifications.enabled` is false or the toolbar toggle is
   paused.
2. Skip messages whose `sender.id` equals `device.id`, unless
   `notifications.notifyOwnMessages` is true.
3. Read the payload level, defaulting to `info` for missing or unknown levels and raw
   payloads. Check rules in config order and fire the first enabled rule matching both
   the MQTT topic filter and payload level.
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
| `info` | `#` | `info` |
| `warn` | `#` | `warn` |
| `error` | `#` | `error` |

The default filters resolve to `hiveme/#`, which includes `hiveme` itself and its
custom subtopics. `info`, `warn`, and `error` are rule IDs and payload levels.

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

1. Publish to `hiveme` with `hmc --level info`, `hmc --level warn`, and
   `hmc --level error` from another terminal, and see three notifications and three
   message-box colors on the same topic. Confirm a fresh database opens with
   `hiveme` selected and the composer ready when connected.
2. Publish ten messages with the same level in a second, and see one notification that ends
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

The module is `hiveme_core::storage`, behind the `storage` feature, which the `session`
feature implies. The database runs in WAL mode with a five second busy timeout, so a
write that meets another process's write waits instead of failing. `hmc` links SQLite
too, because its terminal UI runs on the shared session and opens this same file;
one-shot publishing never opens it. The rules for two processes on
one database are in [session.md](session.md#two-processes-one-installation).

- `schema_version` holds the version of the layout above. Version 0 means a database
  this build has not stamped, whether it is brand new or older than the table itself,
  so the tables are created with `IF NOT EXISTS` and the version is written afterward.
  A database from a newer build is refused rather than guessed at.
- Inserts de-duplicate on `(topic_id, msg_id)`, in one immediate transaction, which is
  how a message the composer sent and the copy the broker echoes back collapse into one
  bubble, regardless of arrival order. An outgoing publish sets the row's outgoing flag and preserves the
  QoS and retain options used to send it; a subscription echo cannot replace them
  with its delivery flags. An echo stored before the outgoing publish is reconciled
  as sent and removed from the unread count.
- A payload that is not a HiveMe envelope has no identifier of its own, so one is
  generated. Two identical third party messages are therefore two rows, which is right:
  they are two messages, and only an envelope can claim otherwise. The echo of a raw
  publish is not a second message, and the session hands it the id of the row it belongs
  to before the insert sees it, see [session.md](session.md#receiving).
- `unread` counts a message that is new and came from the broker. What this
  installation published has been seen by definition.
- `raw` is the payload exactly as it arrived, which is what lets the view render every
  tier from the stored row without asking the broker again.
- History is filtered in SQL using `topics.topic`, joined to each message through
  `messages.topic_id`. A selection includes the exact topic and all names starting
  with that topic followed by `/`. The indexed BINARY range from `<topic>/`
  (inclusive) to `<topic>0` (exclusive) implements the descendant prefix without
  treating `%` or `_` as wildcards or ignoring case. No schema change is needed.
- History is read a page at a time across the entire selected subtree, from the newest
  end and handed back oldest first. The cursor is the row id, so paging upwards asks
  for what is `before` the oldest row on screen, regardless of its child topic.
- Clearing a topic deletes only messages stored directly on that topic and keeps the
  node and descendant messages. Cached ancestor views are invalidated and the active
  view is reloaded; stale pending pages cannot restore deleted messages.
- Pruning runs at startup and every ten minutes, using
  `gui.history.maxMessagesPerTopic` and `gui.history.retentionDays`. Either limit set
  to 0 means no limit.

## IPC

The frontend never calls Tauri APIs directly. `src/lib/service.ts` wraps every
`invoke`, and `src-tauri/src/lib.rs` holds one thin `#[tauri::command]` per call that
delegates to `controller.rs`, which is one call into the shared session of
[session.md](session.md) per command. The session is what each command does; this
section is only the IPC surface of `hmg` over it, and the terminal UI of `hmc` drives
the same session without any IPC.

| `src-tauri/src` | What it adds to the session |
|-----------------|-----------------------------|
| `lib.rs`, `controller.rs` | The commands below, each one line of `controller.rs`; `open_config_file` is the one the session cannot answer, because it needs the opener plugin |
| `events.rs` | A task that emits every `SessionEvent` under the event names below |
| `notification.rs` | The `Toaster` of [session.md](session.md#notifications) |
| `protocol.rs` | Re-exports the session's types, and holds the event names, the event payloads, and the managed state |
| `window.rs` | Window geometry, `start_background_work`, and the quit path |

The types live in `crates/hiveme-core/src/session/types.rs`, re-exported by
`src-tauri/src/protocol.rs`, and in `src/lib/protocol.ts`, which are hand-synced:
camelCase in TypeScript, snake_case in Rust with `#[serde(rename)]`. Config and message
types are **not** hand-written; they are generated from the JSON schemas into
`src/generated/` and re-exported from `protocol.ts`.

Every command answers `Result<T, String>`, and the error is the one line the snackbar
shows.

### Commands

| Command | Request | Response |
|---------|---------|----------|
| `clear_topic` | `{ "topic": "hiveme" }` | how many messages were deleted |
| `connect` | none | `Status` |
| `disconnect` | none | none |
| `get_about` | none | `About` |
| `get_broker_init` | none | the setup string for `hmc --init` |
| `get_config` | none | the config, as `schemas/config.schema.json` describes it |
| `get_messages` | `{ "topic": "hiveme", "before": 42, "limit": 200 }` | `MessageRow[]` for the topic and all descendants, oldest first |
| `get_status` | none | `Status` |
| `get_update_result` | none | `{ "hasUpdate": false, "latestVersion": null }`, or nothing while the check is still running |
| `list_topics` | none | `TopicNode[]` |
| `mark_read` | `{ "topic": "hiveme" }` | none; clears unread counts throughout the selected subtree |
| `open_config_file` | none | none |
| `publish` | `{ "topic": "hiveme", "body": "Build finished", "options": PublishOptions }` | the stored `MessageRow` |
| `set_config` | `{ "config": Config }` | the config as it was saved |
| `set_notifications_paused` | `{ "paused": true }` | `Status` |
| `skip_version` | `{ "version": "0.2.0" }` | none |

`before` and `limit` in `get_messages` may both be null: no cursor means the newest
page, and no limit means the default of 200.

What each command does is the operation of the same name in
[session.md](session.md#operations): `get_about` is `about`, `get_config` is `config`,
`get_broker_init` is `broker_init`, `get_messages` is `messages`, `get_status` is
`status`, `get_update_result` is `update_result`, `list_topics` is `topic_tree`, and
the rest keep their names. In short: `connect` replaces a connection that is already
up; `set_config` reconnects only when the broker or the subscriptions changed, so a
theme keeps the connection and a password does not; `set_notifications_paused` is the
toolbar toggle and lasts for the process.

`get_broker_init` carries the password in plain text, so it is never logged.

### Types

```
About          = { appVersion, configPath, databasePath, deviceId, deviceName, githubUrl }
Status         = { state, host, port, clientId, subscriptions, attempt, retryInMs,
                   lastError, messagesReceived, databaseBytes, notificationsPaused, configError }
TopicNode      = { id, label, topic, unread, messages, children: TopicNode[] }
MessageRow     = { rowId, topic, id, ts, receivedTs, senderId, senderName, app,
                   tier, level, title, body, raw, rawLength, qos, retain, outgoing }
PublishOptions = { topic?, json?, qos?, retain?, title?, level? }
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
- The backend sets `TopicNode.topic` on every nonempty path, including intermediate
  parents such as `hiveme/build`, to select that subtree. Only an empty leading path
  segment has a null topic. The frontend merges in the always-selectable `hiveme`
  root without creating a database row or duplicating an existing root. `unread`
  and `messages` are rolled up, so a collapsed branch still shows that something
  below it is unread.
- `MessageRow.raw` is the payload as text, or as hex for the `bytes` tier, because a
  payload that is not valid UTF-8 cannot travel as JSON text. `rawLength` is the byte
  count. `src/lib/message.ts` reads `raw` again to render the envelope and the JSON
  tree, which is what keeps the two readers comparable.
- `PublishOptions.topic` is the relative input under the selected `topic` argument.
  The shared core strips leading slashes and joins it to that selected path. An absent
  or empty input sends to the selected topic itself.
- `PublishOptions.json` publishes the body as a raw JSON payload with no envelope, as
  `hmc --json` does, and refuses input that is not JSON. An absent `qos` or `retain`
  means the configured default, and an absent `level` means `info`, independently of
  the MQTT topic.

### Events

| Event | Payload |
|-------|---------|
| `status` | `Status` |
| `message` | `MessageRow` |
| `topic-added` | `{ "topic": "hiveme" }` |
| `notification-fired` | `{ "ruleId": "error", "messageId": "018f6b1e-...", "topic": "hiveme" }` |

The events are the session's, see [session.md](session.md#events). A `message` event
is emitted once per stored row, whether the message arrived or the composer sent it.
The echo of a message this process published raises no second event, because it
collapses into the row that is already there; a row that interactive `hmc` stored first
on the same database is raised all the same, see
[session.md](session.md#two-processes-one-installation). `status` is also emitted when the pause
toggle changes.

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
| Topics | `topics` | **Subscriptions**, where each row is a filter and an "absolute" box |
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

`BrokerUrlParts` in `hiveme_core::config` is the Rust twin of `brokerUrl.ts`, which the
Broker panel of the terminal UI uses, so both applications take a pasted URL apart the same
way; `crates/hiveme-core/tests/fixtures/broker_url.json` holds the cases the Rust tests and
`brokerUrl.test.ts` both read. The frontend keeps its own copy because it is display code.

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
category has a **Copy CLI setup** button that puts the complete command on the
clipboard, with the setup JSON from [config.md](config.md#the-setup-string) already
included as a single-quoted argument:

```sh
hmc --init '{"v":1,"url":"mqtts://abc123.s1.eu.hivemq.cloud:8883","username":"hiveme-sam","password":"s3cret","language":"en-US"}'
```

The user pastes the command into a terminal and runs it without adding anything.
Apostrophes in the JSON use Unicode escapes so they cannot end the shell argument;
parsing the JSON restores the original credential values.

The button is disabled until the broker fields validate, because a string that cannot
be applied is worse than no string. The password is in it, in plain text, and the
button says so.

The backend command is `get_broker_init`; `hiveme-core` renders the string, so the two
applications cannot disagree about the format. The frontend wraps that JSON in the
command before writing it to the clipboard. Copy CLI setup first flushes pending
settings and waits for the write to finish, so the copied string uses the current
credentials. A failed save prevents copying an older setup string.

The string also carries `gui.language`, so that the `hmc` it initializes speaks the
language this window is in. See [config.md](config.md#the-setup-string).

## Window

`window.rs` owns window state, as in the reference project. `place` writes the title
`HiveMe v<version>` and the remembered `gui.window` geometry into the window's own
configuration, and `lib.rs` calls it on the generated context before the builder runs.
A size below 600 x 450 is clamped there and the clamped size written back once; a
negative stored coordinate asks for `center` instead of a corner, which is what a fresh
config's `-1, -1` does. `setup` then only shows the window and starts the update check
when it is due. `on_window_event` persists size and position on move and resize,
ignoring minimized windows and sizes below 600 x 450.

The geometry is set before the window exists rather than after, because a window that is
built somewhere and moved afterwards is drawn in both places. macOS is where that shows:
`NSWindow` frame changes are not thread safe, so tao defers every move, resize, and
retitle to the main dispatch queue, while showing a window already on the main thread
runs inline. `setup` runs on the main thread before the event loop turns, so a `show`
there went first and the queued move landed a frame later — the window opened where
macOS had centered it and jumped to the remembered place mid-animation. Tauri resolves
both an explicit position and `center` into the frame the window is built with, so
nothing is left to defer. The `width`, `height`, and `title` in `tauri.conf.json` are
only the defaults this overrides.

Closing the main window or quitting through the system menu first ends the MQTT
connection and broker session. The event loop stays alive while cleanup runs, and
repeated quit requests share that cleanup. New connections and publishes are blocked;
startup and subscription waits are interrupted so their clients can disconnect.
The application allows up to ten seconds, including a connection replacement already
in progress, then exits even if the broker is unreachable. Cleanup failures are
logged. See [hivemq-cloud.md](hivemq-cloud.md#disconnect-and-quit).

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
