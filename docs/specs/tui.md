# HiveMe terminal UI (`hmc` interactive mode)

`hmc` run with no arguments on a terminal opens a terminal UI that has every feature
of `hmg`: toolbar, tabs, topic tree, chat view, composer, settings, about, status bar,
snackbar, OS notifications, and the update notice, rendered with
[ratatui](https://ratatui.rs/) in the terminal, in all nine languages, on the same
backend `hmg` uses. The one-shot publish mode of [cli.md](cli.md) is unchanged.

**Status: the languages and the shell are built; the Messages tab, Settings, and
About are specified.** [The terminal UI plan](../plans/plan-terminal-ui.md) builds it
phase by phase. Every section below names the phase that builds it, and the status
table in [app.md](app.md#status) records what has landed. Phases 1 to 3 are done; until
a later section's phase is done, that section describes intent rather than code, the
way [message.md](message.md#encryption) describes encryption.

The behaviors are those of [gui.md](gui.md), rendered in cells. Where the terminal
cannot do what the GUI does, this document says what replaces it. The shared backend
behind both applications is specified in [session.md](session.md).

## Scope and trigger

*Phase 3.*

- `hmc` with no message argument, no publish option, and a terminal on stdin opens the
  terminal UI. Piped stdin still publishes the body, so `echo hi | hmc` is unchanged.
- `--tui` forces the terminal UI regardless of stdin. It conflicts with every publish
  and init option, so an option that would be ignored is refused instead, as `--init`
  already is. `--config` and `--verbose` still apply.
- `--tui` on a stdout that is not a terminal is a usage error, exit 2, because there is
  nothing to draw on. So is the trigger above with stdout redirected.
- The rules are in [cli.md](cli.md#interactive-mode); this document is what the mode
  does once it is open.

## Components

*Phase 3 for the shell, 4 for `messages/`, 5 for `settings/` and `about.rs`.*

The file split mirrors `src/components/` on purpose, as the component table of
[gui.md](gui.md#components) does, so that a feature has the same name in both
applications.

| File | What it is |
|------|------------|
| `crates/hmc/src/main.rs` | Chooses the mode: `Cli::is_interactive` sends the run to the terminal UI, anything else publishes |
| `crates/hmc/src/cli.rs` | clap, `--tui`, the translated help |
| `crates/hmc/src/tui/mod.rs` | `start()`: the multi-thread runtime, the log file, terminal setup and the panic hook that restores it, the signals, the event loop, and the quit path |
| `crates/hmc/src/tui/app.rs` | The application state, the Rust twin of `src/lib/store.tsx`, plus focus, tabs, drafts, and overlays |
| `crates/hmc/src/tui/service.rs` | The `Service` trait: the session operations the terminal UI uses, which `Session` implements and the tests script |
| `crates/hmc/src/tui/keys.rs` | The key map and the keyboard enhancement flags |
| `crates/hmc/src/tui/theme.rs` | The twenty palettes, the display modes, the severity colors |
| `crates/hmc/src/tui/layout.rs` | The rows: toolbar, update notice, tabs, content, footer |
| `crates/hmc/src/tui/toolbar.rs` | Connect, pause notifications, clear the topic, Settings, About, Help |
| `crates/hmc/src/tui/tabs.rs` | The tab strip and its close marks |
| `crates/hmc/src/tui/footer.rs` | The status bar |
| `crates/hmc/src/tui/snackbar.rs` | The transient overlay for errors and confirmations |
| `crates/hmc/src/tui/help.rs` | The key binding overlay |
| `crates/hmc/src/tui/update_notice.rs` | The line above the tabs when a newer release exists |
| `crates/hmc/src/tui/messages/mod.rs` | Tab 0: the split pane and its divider. Phase 3 draws the selected topic and `messages.empty` or `messages.selectTopic` |
| `crates/hmc/src/tui/messages/topic_tree.rs` | The topic hierarchy and its filter |
| `crates/hmc/src/tui/messages/message_view.rs` | The chat view, its bubbles, the virtual rows |
| `crates/hmc/src/tui/messages/json_tree.rs` | The collapsible tree used by `data` and raw JSON |
| `crates/hmc/src/tui/messages/detail.rs` | Per-node navigation of one message's trees |
| `crates/hmc/src/tui/messages/composer.rs` | The input, the send action, the collapsible options |
| `crates/hmc/src/tui/settings/mod.rs` | The Settings tab: category strip and panel |
| `crates/hmc/src/tui/settings/{appearance,broker,topics,notifications,history,update,advanced}.rs` | One panel per category. Phase 3 has `broker.rs` with the URL, username, and password |
| `crates/hmc/src/tui/about.rs` | The About tab. Phase 3 draws it as text |
| `crates/hmc/src/tui/widgets/` | Text input, multi-line editor, select popup, checkbox, radio row, number field, editable table. Phase 3 has the text input and `truncate` |
| `crates/hmc/src/tui/clipboard.rs` | `arboard`, then the OSC 52 escape sequence. Phase 4, with the copy actions that use it |
| `crates/hmc/src/tui/notify.rs` | The `Toaster` of [session.md](session.md#notifications): `notify-rust`, or the Windows toast crate |
| `crates/hmc/src/tui/open.rs` | Opens URLs and the config directory |
| `crates/hiveme-core/src/i18n/` | Locale resolution, the catalogs, plural rules, formatting; see [Languages](#languages). Built in phase 2 |
| `locales/*.json` | The nine catalogs, shared with the frontend |

Third party ratatui widgets (`tui-textarea`, `tui-tree-widget`) are used only if they
support the ratatui release in use. When phase 3 started, `tui-textarea` 0.7 still
depended on ratatui 0.29, so `widgets/` holds in-house controls; the choice is recorded
in the deviations of [app.md](app.md).

## Layout

*Phase 3.*

Rows, top to bottom: the toolbar (3), the update notice (0 or 1), the tabs (1), the
content (the rest), the footer (1).

```
 HiveMe v0.1.0 ─────────────────────────────────────────────────────────────────
 [F2 Connect] [F3 Pause] [F4 Clear] [F10 Settings] [F1 About] [? Help] [^Q Quit]
────────────────────────────────────────────────────────────────────────────────
 Messages │ Settings ✕ │ About ✕
┌ Filter topics ──────────┐┌ hiveme ────────────────────────────────────────────┐
│ >                       ││  sams-macbook                                       │
│ ▾ hiveme (2)            ││  ╭────────────────────────────╮                    │
│   ▾ build (2)           ││  │ CI                         │                    │
│       ci (2)            ││  │ Nightly build 482 finished │                    │
│     deploy              ││  ╰────────────────────────────╯                    │
│                         ││  build/ci  Info  QoS 1  09:41                      │
│                         ││                    ╭────────────────────────────╮  │
│                         ││                    │ Deployed hiveme to staging │  │
│                         ││                    ╰────────────────────────────╯  │
│                         │├────────────────────────────────────────────────────┤
│                         ││ Write a message. Enter sends, Alt+Enter adds a line│
│                         ││                                                    │
│                         ││                        [Info ▾] [More Options ▸] [Send]│
└─────────────────────────┘└────────────────────────────────────────────────────┘
 ● connected  abc123.s1.eu.hivemq.cloud:8883  1 subscription  128 messages this session  database 1.2 MB
```

- The pane split starts at 28 percent of the width, is clamped to 15..60, and moves
  with `Ctrl+Left` and `Ctrl+Right` or by dragging the divider with the mouse. It is
  remembered for the process only, as the GUI remembers its divider in the window.
- Below 80 columns by 24 rows the whole frame is replaced by one centered line,
  `tui.tooSmall`, the way `hmg` enforces its 600 x 450 minimum.
- The Messages tab stays mounted while another tab is shown, so its scroll position,
  focus, and drafts survive a switch.
- Until phase 4 the Messages tab is one rounded block titled with the selected topic,
  `hiveme` at startup, holding `messages.empty` while that subtree has no stored rows
  and `messages.selectTopic` when nothing is selected.

### Toolbar

*Phase 3.*

A block exactly three rows high: a rule above, the row of tools, a rule below. It has
no side borders, which is what lets the English labels fit an 80 column terminal. Its
title, on the rule above, is `HiveMe v<version>`, the window title of `hmg`. Each tool
is a labeled button, `[<key> <label>]`, and the active one is drawn bold in the
primary color, as the GUI's `activeButtonSx` does: while connecting, connected, or
reconnecting, while paused, and while the corresponding tab is open. Clear is dimmed
while no topic is selected. Help and Quit sit at the right end, and a click on a
button does what its key does.

The labels are the `tui.toolbar.*` keys, short words rather than the GUI's tooltips.
When a language's labels do not fit the width, the longest label is cut first and ends
in an ellipsis, one cell at a time, down to the key alone, so every tool stays on
screen with its key. At exactly 80 columns the English Disconnect label is cut; at 120
columns every language fits.

| Tool | Key | Behavior |
|------|-----|----------|
| Connect / Disconnect | `F2` | As [gui.md](gui.md#toolbar): Disconnect is offered while Connecting, Connected, or Reconnecting |
| Pause notifications | `F3` | The session's pause toggle; the footer says `notifications paused` while it is on |
| Clear selected topic | `F4` | Deletes the stored history of the selected topic, invalidates cached ancestor views, reloads the selection |
| Settings | `F10` | Opens or focuses the Settings tab |
| About | `F1` | Opens or focuses the About tab |
| Help | `?` outside a text field, `Ctrl+/` anywhere | The key binding overlay. Not a GUI feature; a terminal has no tooltips to name the keys |
| Quit | `Ctrl+Q`, `Ctrl+C` | Leaves the terminal UI, see [Leaving the terminal UI](#leaving-the-terminal-ui). The GUI's equivalent is the window's close button, which a terminal does not have, so the tool is always shown. Its label writes the key as `^Q` |

### Tabs

*Phase 3.*

Tab 0, Messages, is fixed. Settings and About open as closable tabs, in the order they
were opened, each with a `✕`; opening one that is open selects it. `Alt+1`..`Alt+9`
select a tab, `Ctrl+W` closes the current closable tab, `Alt+Left` and `Alt+Right`
cycle and wrap. `Ctrl+1`..`Ctrl+9`, `Ctrl+Tab`, and `Ctrl+Shift+Tab` are also accepted
when the terminal reports them, which needs the keyboard protocol of
[Terminal requirements](#terminal-requirements). The tabs are separated by `│`, the
selected one is bold in the primary color, a click on a label selects it, and a click
on its `✕` closes it. Closing a tab selects the one that took its place.

### Topic tree

*Phase 4.*

- A filter field sits on top. `/` focuses it from anywhere in the Messages tab and
  `Esc` leaves it. Matching is on the whole path, case-insensitive, keeps a parent
  whose child matches, and expands what it found. `hiveme` stays visible even when it
  does not match, as in the GUI.
- The tree is drawn with `▾` and `▸` markers and two-cell indentation. `hiveme` is
  always present, selected, and highlighted at startup, even with an empty database.
  Every nonempty path is selectable, including a parent such as `hiveme/build` whose
  messages are all on `hiveme/build/ci`; an empty leading segment is an italic group in
  the secondary color and cannot be selected. Nodes follow the stored MQTT paths
  split on `/`; levels are never synthetic nodes.
- `Up` and `Down` move, `Right` expands, `Left` collapses or moves to the parent,
  `Space` toggles, `Enter` selects. Selecting loads the subtree from the store and
  marks it read, and is independent of expansion, as clicking a label in the GUI is. A
  mouse click on the marker toggles, on the label selects.
- The unread badge is ` (N)` after the label in the primary color, rolled up from the
  descendants, capped at `999+`. The selected label is bold in the primary color.
- The first tree a user sees is fully expanded until the user changes the expansion,
  as `TopicTree.tsx` does.

### Message view

*Phase 4.*

- The header line is the selected topic. Below it, the rows of the selected topic and
  all its recursive descendants, oldest at the top, newest at the bottom, following new
  messages unless the user has scrolled up. `PageUp` at the top asks the store for the
  previous page of 200 rows, the `before` cursor being the oldest row on screen, as
  `get_messages` pages. Only the rows in view are laid out, and heights are cached per
  row and wrap width.
- A bubble is a rounded block, at most 80 percent of the pane wide and at least 18
  cells, aligned left for incoming rows and right for outgoing rows. An incoming bubble
  is preceded by the sender's name, falling back to the sender id, never the
  application name; outgoing and senderless rows have no header.
- Tiers render as [gui.md](gui.md#message-view) lists them: an envelope shows a bold
  title, the body, and a collapsed `data` tree; raw JSON is a tree; raw text is shown
  as text; bytes show `N bytes` and grouped hex; an encrypted message shows a lock
  glyph and `encrypted (key <kid>)`, with `[enc]` where the glyph is unavailable; a
  newer `v` gets the `newer version` chip.
- Colors follow `payload.level`, independently of the topic or direction. `info` and
  `debug` use the regular fill; `success`, `warn`, and `error` use the MUI palette
  values (`#2e7d32`, `#ed6c02`, `#d32f2f`) for the border and the badge, with a tinted
  fill only when a display mode is forced, because a tint needs a known background. An
  unknown level renders as `info` while the badge shows the raw name with the fallback
  in parentheses. See [Theme](#theme).
- The metadata row below a bubble, aligned to its right edge, holds the topic path
  relative to the selected tree topic (secondary color, empty for a row on the selected
  topic itself), the level badge, `QoS n`, a retained marker, the newer-version chip,
  and the time in the selected language. It is drawn only for the focused row, and its
  one-row space is reserved for every row so focusing does not move its neighbors. This
  is the GUI's hover row without a pointer.
- A centered secondary-color line separates rows on different days.
- With a row focused: `c` copies the body, `r` copies the raw payload, `Space` expands
  or collapses every tree in the bubble, `Enter` opens the detail view in which `Up` and
  `Down` move between tree nodes, `Space` toggles one node, and `Esc` returns. The
  snackbar confirms a copy or reports why it failed.

### Composer

*Phase 4.*

- A bordered editor at the bottom of the message pane, three rows minimum, growing to
  six, with `composer.placeholder` or `composer.placeholderJson` as its placeholder.
  Below it, right-aligned: the Level select, More Options with `▸` or `▾`, and Send.
- `Enter` sends from every composer control. `Alt+Enter` and `Ctrl+J` insert a
  newline, and so does `Shift+Enter` where the terminal reports it. Inside an open
  Level popup `Enter` picks the highlighted level without sending. `Tab` and
  `Shift+Tab` move between the editor and its controls.
- The Level select offers Info, Error, Success, and Warn in that order, Info by
  default, colored as the GUI colors them, disabled in raw JSON mode while keeping its
  value.
- More Options shows Topic (relative to the selected tree topic, leading slashes
  stripped as typed), Title (disabled in raw JSON mode), and the QoS row with the
  Config, 0, 1, and 2 radios, Retain Message, and As Raw JSON. Collapsing hides the
  controls and keeps every value in effect.
- The draft, level, options, and expansion state are kept per selected tree topic in
  process memory. Success clears only the originating topic's text; failure keeps it
  and goes to the snackbar. The composer is disabled while not connected or without a
  selection, and the reason is shown as its placeholder.
- Sending calls the session's publish, the same path `hmg` and one-shot `hmc` use. The
  bubble appears once the broker has acknowledged, and the copy the broker echoes back
  collapses into it by row id.

### Settings

*Phase 5, except the Broker fields a first run needs, which phase 3 provides.*

What phase 3 builds: the category list, in a rounded block beside the panel, with
`Up` and `Down` and clicks; and the Broker panel with three text fields, URL, Username,
and Password, each a rounded block titled with its label, the password shown as `*`
until `Ctrl+H`, the `Ctrl+H` hint, and the TLS note. `Tab` or `Enter` on the list
enters the Broker fields; `Tab`, `Down`, and `Enter` move to the next field and `Up`
and `Shift+Tab` to the previous one, back to the list from the first; `Esc` returns
to the list. The URL is saved trimmed and as typed, since a URL without a scheme is
TLS MQTT to the core; the username and password are saved as typed. Edits are saved
500 ms after the last one, one write at a time, and `Enter` on the password field saves
at once. The other six categories show `tui.settingsLater` until phase 5.

What phase 5 builds:

- A vertical category list on the left (Appearance, Broker, Topics, Notifications,
  History, Update, Advanced) and a panel on the right, 96 columns at most, centered.
  Appearance opens first. `Up` and `Down` on the list change the category, `Tab`
  enters the panel, `Esc` returns to the list.
- Appearance: Mode as a three-way radio row (Auto Mode, Light Mode, Dark Mode), Theme
  and Language as select popups. A change applies to the whole screen at once,
  including the catalog swap, and is saved automatically.
- Broker: the Protocol select and the URL box with the same split and join as
  `src/lib/brokerUrl.ts`, ported into `hiveme_core::config::url` so both applications
  read a pasted scheme the same way; the line under the box says the URL that will be
  saved and the port; Username and Password, with `Ctrl+H` to show or hide the
  password; the TLS note; **Copy CLI setup**, disabled until the broker fields validate,
  which flushes a pending save and then copies the complete `hmc --init '<json>'`
  command with the language and the apostrophe escaping of
  [gui.md](gui.md#copy-cli-setup); the Connection and Reconnect groups with their
  number fields.
- Topics: the subscription rows, each a filter box and an Absolute checkbox, with Add
  and Remove.
- Notifications: the two switches and the rules table (Id, Topic filter, Level,
  Enabled, Title template, Body template, Remove) with Add.
- History, Update, Advanced: as [gui.md](gui.md#settings) lists them. Advanced is read
  only text.
- Saving follows the GUI store: 500 ms after the last edit, one writer at a time, the
  newest snapshot after a write already in progress, silent on success, the snackbar on
  failure with the edits kept on screen. The session validates, writes, recompiles the
  rules, and reconnects only when broker or subscription fields changed.

### About

*Phase 5.*

`HiveMe` in large text with the amber to orange gradient of the GUI spread over the
letters, the version chip, the tagline, the Author and GitHub cards that open their URL
on `Enter` or click, the table of device, config file, history database, and license,
and the copyright line. There is no icon image.

Phase 3 draws the same content as text in one rounded block: `HiveMe` bold in amber
beside the version, the tagline, a table of the author, the repository, the device,
the config file, the history database, and the license, and the copyright line.

### Footer

*Phase 3.*

The same content as `Footer.tsx`: the state with its color, the broker host and port
or `no broker configured`, the reconnect countdown driven by the tick timer, the
subscription count, the messages received this session, the database size, the
`notifications paused` marker, and the `config error` and `last error` entries, which
open the snackbar with the detail on `Enter` or click. The state words come from
`footer.state.<state>` as in the GUI.

The state is drawn as `● <state>`, green while connected, orange while connecting or
reconnecting, and muted otherwise. The error entries sit at the right edge, underlined
in the error color. Until the tree and the chat view take the focus in phase 4, `Tab`
and `Shift+Tab` in the Messages tab move the focus through them, drawn reversed, and
`Enter` opens the focused one. While the quit path waits for the broker, `tui.quitting`
comes first.

### Snackbar

*Phase 3.*

A top-center overlay one row high, info or error colored, shown for four seconds or
until a key is pressed, one at a time. It is driven by the same `notifyInfo` and
`notifyError` calls the GUI store has: command errors, copy confirmations, save
failures. It is in-app feedback and unrelated to OS notifications.

It is drawn on the top row, over the toolbar's title, white on the error color, at most
80 percent of the width, with a detail of several lines folded onto one. The key that
dismisses it still does its work, except `Esc`, which only dismisses it, and a click on
it dismisses it too. Phase 3 reports only failures; the info color arrives with the
copy confirmations of phase 4.

### Update notice

*Phase 3.*

A line above the tabs, `HiveMe vX is available.`, while the session's update check has
found a newer release. `o` opens the releases page, `s` toggles Skip this version, `x`
or a click closes it and applies the skip through the session, which writes
`update.ignoreVersion` as `hmg` does.

The line reads `HiveMe vX is available. (o)` in the primary color on the left and
`[ ] Skip this version (s)  ✕ (x)` on the right, and a click on each part does what its
key does. The letters are keys only while no text field has the focus. The session's
answer is read on the tick until there is one, once, as the GUI polls for it.

### Help overlay

*Phase 3.*

A centered popup listing the key map below for the current scope. `Esc` or any
listed key closes it.

It lists the keys that work everywhere, then those of the open tab, then those of the
update notice while it is shown. The key names are written as keyboards print them in
every language; what they do comes from `tui.help.*`. `?`, `Ctrl+/`, and `Esc` only
close it, a key that does something else closes it and does it, and a click anywhere
closes it.

## Keys and mouse

*Phase 3, with the Messages and Settings rows in phases 4 and 5.*

Terminals do not reliably deliver `Ctrl+1`..`Ctrl+9`, `Ctrl+Tab`, or `Shift+Enter`
as distinct keys, so every action has a binding a plain terminal delivers, and the
GUI's bindings are accepted in addition where the terminal reports them.

| Scope | Key | Action |
|-------|-----|--------|
| Global | `Ctrl+Q`, `Ctrl+C` | Quit, see [Leaving the terminal UI](#leaving-the-terminal-ui). Raw mode turns off the terminal's own `Ctrl+C` signal, so both arrive as keys in every focus, text fields included |
| Global | `F1`, `F2`, `F3`, `F4`, `F10` | About, Connect or Disconnect, Pause, Clear topic, Settings |
| Global | `Alt+1`..`Alt+9`, `Ctrl+1`..`Ctrl+9` | Select a tab |
| Global | `Alt+Left`, `Alt+Right`, `Ctrl+Shift+Tab`, `Ctrl+Tab` | Previous, next tab |
| Global | `Ctrl+W` | Close the current closable tab |
| Global | `Ctrl+/`, `?` outside text | Help overlay |
| Global | `Esc` | Close the topmost overlay, popup, or detail view; otherwise back to the pane list |
| Messages | `Tab`, `Shift+Tab` | Cycle focus: topic filter, tree, message list, composer |
| Messages | `Ctrl+Left`, `Ctrl+Right` | Move the pane divider |
| Tree | `Up`, `Down`, `Left`, `Right`, `Space`, `Enter`, `/` | Move, collapse or expand, toggle, select, focus the filter |
| Message list | `Up`, `Down`, `PageUp`, `PageDown`, `Home`, `End`, `c`, `r`, `Space`, `Enter` | Move focus, page, jump, copy body, copy raw, toggle trees, detail view |
| Composer | `Enter`; `Alt+Enter`, `Ctrl+J`, `Shift+Enter`; `Tab` | Send; newline; next control |
| Settings | `Up`, `Down`, `Tab`, `Shift+Tab`, `Enter`, `Space`, `Ctrl+H` | Category, field, open a select, toggle, show or hide the password |
| Mouse | click, wheel, drag | Select tabs, tools (Quit included), topics, rows, and controls; scroll the focused list; move the divider |

Every text field takes the usual editing keys (`Left`, `Right`, `Home`, `End`,
`Backspace`, `Delete`, `Ctrl+U`, `Ctrl+A`, `Ctrl+E`), and a paste arrives through
bracketed paste when the terminal supports it; a paste into a one-line field drops its
line breaks.

- A key acts when it is pressed or repeats, never when it is released, which Windows
  reports as well.
- Without the keyboard protocol a terminal sends `Ctrl+/` as the byte crossterm reads
  as `Ctrl+7`, so `Ctrl+7` opens the help there; under the protocol it selects tab 7.
- `Up` and `Down` move between the fields of a form, and `Ctrl+H` toggles the password
  from any Settings field.
- `Ctrl+Left`, `Ctrl+Right`, the wheel, and dragging arrive with the split pane in
  phase 4.

## Theme

*Phase 3.*

`gui.theme` and `gui.displayMode` are honored, so the two applications look alike and
the Appearance settings mean the same thing in both.

- The twenty palettes of [gui.md](gui.md#theme) map their primary and secondary hex
  colors to `Color::Rgb`. The primary color marks the selected topic, the active
  toolbar button, badges, and the focused control; the secondary color marks the
  filter matches and the divider while dragging.
- `Auto` keeps the terminal's own background and foreground and draws no fills, since
  the terminal's colors are unknown. `Light` and `Dark` set the background and the
  foreground and draw the bubble fills of the GUI: in `Light` an outgoing bubble is
  black with white text and an incoming one is the `action.hover` gray; in `Dark` an
  outgoing bubble is grey 800. Severity tints exist only in a forced mode.
- A terminal without truecolor receives the nearest ANSI color from crossterm. The
  design never relies on a fill or a tint alone to convey a meaning: the badge text
  carries the level.
- Text is never transformed. Labels read as they are written in the catalogs.
- The glyphs `✕`, `●`, `✓`, `…`, and `│` fall back to `x`, `*`, `x`, `...`, and `|` on
  the Linux console (`TERM=linux`) and on a Windows console that is not Windows
  Terminal (no `WT_SESSION`), whose fonts lack them. The choice is made once for the
  whole screen.

## Languages

*Phase 2 for the catalogs and the publish and init lines; phase 3 for the terminal
UI.*

`hmc` ships the same nine languages as `hmg`: German (`de`), US English (`en-US`),
Spanish (`es`), French (`fr`), Italian (`it`), Japanese (`ja`), Simplified Chinese
(`zh-CN`), and Traditional Chinese for Hong Kong (`zh-HK`) and Taiwan (`zh-TW`).

- The catalogs are one shared set, `locales/*.json` at the repository root; the
  frontend imports it from there and `hiveme_core::i18n`, behind the `i18n` feature
  that `hmc` turns on, embeds it with `include_str!`. Keys the terminal UI and the CLI
  add live in the same files, under the `tui`, `cli`, and `help` namespaces, so the
  frontend's catalog test covers them as well.
- The lookups are `t(locale, key)`, `t_with(locale, key, values)` for `{{name}}`
  placeholders, and `t_count(locale, key, count)` for the `_one`, `_many`, and `_other`
  keys, with `{{count, number}}` grouped. A key a locale lacks falls back to `en-US`,
  then to the key itself. `Locale::resolve` is `resolveLanguage` of
  `src/i18n/index.ts`.
- Language selection: `gui.language` of the config that was loaded; `en-US` when there
  is no config; and `--init` prefers the `language` inside the setup string when it
  carries one. Regional and script tags resolve as the GUI resolves them, and an
  unsupported tag falls back to English. A language change in Appearance applies at
  once and is saved.
- What is translated: every label, placeholder, chip, and overlay of the terminal UI;
  every line `hmc` itself prints in publish and init mode (the publish confirmation,
  the init outcomes, the usage errors); and the clap help, rendered from the catalog
  with the `Usage`, `Arguments`, and `Options` headings included. The test
  `cli_help_matches_spec` renders `en-US` and compares it with the block in
  [cli.md](cli.md#usage).
- What is not: diagnostics authored by `hiveme-core` (broker refusals, config
  validation messages) stay English, as `hmg` shows them; the category words in
  `hmc: <category>: <detail>` and the exit codes are a contract for scripts and never
  change; protocol values, topic names, JSON, message content, device names,
  identifiers, and URLs remain as received.
- Formatting is done in `hiveme_core::i18n` by hand, without an ICU dependency: CLDR
  plural categories for the nine locales (`one` and `other` for `de` and `en-US`; `one`,
  `many` for a nonzero multiple of a million, and `other` for `es` and `it`; the same
  for `fr` with `one` for 0 and 1; `other` only for `ja` and Chinese), digit grouping
  per locale (none below five digits in `es` and `it`, a narrow no-break space in
  `fr`), and `format::{integer, decimal, bytes, duration, time, date_time, day}`, the
  twins of `src/lib/format.ts`, with the month names and field orders `Intl` uses. The values the frontend's catalog test asserts (`1.000` in `de`,
  `1 000 000 octets` in `fr` with a narrow no-break space, `1 バイト` in `ja`) are
  the values Rust produces.
- A Rust test mirrors the frontend's: every locale covers every `en-US` key, the
  interpolation variables match, and every plural category of the locale exists. A
  missing key fails the test rather than falling back in a release.

## Notifications

*Phase 3.*

Interactive `hmc` raises OS notifications from the shared rules of
[gui.md](gui.md#notifications), through the session's notifier of
[session.md](session.md#notifications): the rule engine, the rate limiter with its
`and N more messages` summary, the `notifyOwnMessages` rule, and the pause toggle are
the same code `hmg` runs. `hmc` supplies only the final show call.

| Platform | How `hmc` shows a toast |
|----------|-------------------------|
| Linux | `notify-rust` over D-Bus. A desktop without a notification daemon is a reason for the message to be silent, not for it to be lost: the failure is logged and the message is still stored and shown. |
| macOS | `notify-rust`. An unbundled binary shows its toasts under the identity of the terminal application that runs it; phase 6 records the label that is seen. |
| Windows | `tauri-winrt-notification` against the same `HiveMe` `AppUserModelId` `hmg` registers, so the toast is labeled HiveMe rather than the console host. The registry entry is written by whichever application starts first. |

The manual checklist of [gui.md](gui.md#platform-notes) is run with interactive `hmc`
in place of `hmg` at the end of phase 4.

## Client identifier

*Phase 1.*

Interactive `hmc` connects as `Role::Tui`: `<broker.clientIdPrefix>-hmc-<first 8
alphanumerics of device.id>-<8 random characters>`, the shape one-shot `hmc` already
uses, with the connection behavior of `hmg`: reconnect with backoff, subscriptions sent
again when the broker has forgotten the session, `broker.sessionExpirySecs` for the
life of the process, and the session ended on quit. The random suffix means several
interactive `hmc` processes and a running `hmg` never collide on the broker. The role
table is in [hivemq-cloud.md](hivemq-cloud.md#role).

## Startup and shutdown

*Phase 3.*

1. Resolve the config path (`--config`, `HIVEME_CONFIG`, then the per-OS location of
   [config.md](config.md#location-and-precedence)). Load it, or write the default one
   with a fresh `device.id` and `en-US` when there is none; a fresh file, one that did
   not exist before the terminal UI started, opens the terminal UI on the Settings
   tab's Broker category with the URL field focused and stays disconnected until a
   broker is entered, as `hmg` behaves on a fresh install. A file that cannot be read is
   reported in the footer as `config error`, the terminal UI runs on defaults, and
   nothing is written until a setting is saved.
2. Open `HiveMe.db` beside the config, with the busy timeout of
   [session.md](session.md#two-processes-one-installation).
3. Enter the alternate screen and raw mode, capture the mouse, enable bracketed paste,
   and request the keyboard enhancement flags where supported. Install the panic hook.
4. Select `hiveme` and load its subtree, start the prune loop, connect when a broker is
   configured, and start the update check when it is due, through the session.
5. On quit, by any of the ways out below, stop accepting work, end the broker session
   with the ten second bound `hmg` uses, restore the terminal, and exit 0.

## Leaving the terminal UI

*Phase 3.*

A user always has a visible way out, and every way out runs one quit path, so that the
MQTT connection is closed and the broker session is discarded however the terminal UI
ends.

| Way out | How it arrives |
|---------|----------------|
| The Quit tool | A click, or its key |
| `Ctrl+Q`, `Ctrl+C` | Key events. Raw mode turns off the terminal's `Ctrl+C` signal, so neither can kill the process halfway through the cleanup |
| Closing the terminal window or tab, or a dropped SSH connection | `SIGHUP` on Linux and macOS, `CTRL_CLOSE_EVENT` on Windows |
| `kill`, or a service manager stopping the process | `SIGTERM` on Linux and macOS, `CTRL_BREAK_EVENT` on Windows |
| Logging off or shutting down | `SIGHUP` or `SIGTERM` from the session manager; `CTRL_LOGOFF_EVENT` and `CTRL_SHUTDOWN_EVENT` on Windows where the system delivers them to a console process |

The quit path:

1. Stop accepting input and call `Session::begin_shutdown`, so that no connection is
   opened and nothing is published behind the quit.
2. When the terminal is still there, draw `tui.quitting` in the footer, so that a slow
   broker is not mistaken for a hung program.
3. Call `Session::shutdown` under `SHUTDOWN_TIMEOUT`, ten seconds. It closes the MQTT
   connection and discards the broker session as
   [hivemq-cloud.md](hivemq-cloud.md#disconnect-and-quit) describes, within its own
   five second bound.
4. Restore the terminal. After `SIGHUP` or a Windows close event there is no terminal
   left, so the restore is attempted and its errors are ignored.
5. Exit 0.

- A second `Ctrl+Q` or `Ctrl+C` during steps 2 and 3 skips the rest of the cleanup,
  restores the terminal, and exits at once. The broker then keeps the session until
  `broker.sessionExpirySecs` runs out.
- Windows ends a console process about five seconds after it delivers a close, logoff,
  or shutdown event, whatever the handler is doing. On those events the cleanup starts
  at once, skips step 2, and relies on the five second bound of step 3.
- A process that is killed outright (`SIGKILL`, Task Manager) or that crashes runs no
  quit path. The panic hook still restores the terminal; the broker session expires
  after `broker.sessionExpirySecs`, as it does after a network loss.
- The signals are read with `tokio::signal` in the same `select!` as the key events
  and the session events.

## Logging

*Phase 3.*

stderr is the screen while the terminal UI is up, so nothing is written to it. With
`--verbose` or `RUST_LOG` set, `log` output is appended to `hmc.log` beside the config
file, at debug or the requested level, with timestamps. Without either, nothing is
logged. A log file that cannot be opened means no logging rather than a failure.
Publish mode keeps the stderr logging of [cli.md](cli.md#output).

## Terminal requirements

*Phase 3.*

- 80 columns by 24 rows at least.
- The crossterm backend, which is ratatui's default and runs on Linux, macOS, and
  Windows. On Windows `hmc.exe` stays a console application, virtual terminal
  processing is enabled, Windows Terminal renders truecolor, and the legacy console
  degrades to the nearest ANSI color.
- The kitty keyboard protocol is requested with `PushKeyboardEnhancementFlags`
  (`DISAMBIGUATE_ESCAPE_CODES`) only where crossterm reports that the terminal supports
  it, and popped on the way out. It is what makes `Ctrl+1`..`Ctrl+9`, `Ctrl+Tab`, and
  `Shift+Enter` arrive; without it the `Alt` and `F` key bindings apply. On Windows
  crossterm reads the console API, which reports those chords without the protocol, so
  nothing is requested there.
- Mouse capture and bracketed paste are enabled with the alternate screen and disabled
  when the terminal is given back, including from the panic hook.
- Layout is display-width aware, so CJK labels are truncated by width, never by byte
  or character count. Glyphs the design uses (`▾`, `▸`, `✕`, the lock and pin
  markers) have ASCII fallbacks behind one table.

## Build and run

*Phase 3.*

```sh
cargo build -r -p hmc        # the one binary, publish mode and terminal UI alike
hmc                          # opens the terminal UI when stdin is a terminal
hmc --tui --config ./HiveMe.json
```

`hmc` links SQLite, `ureq`, and ratatui, because it turns on the `session` feature of
`hiveme-core` for the terminal UI; the binary grows accordingly. Nothing changes in the
bundles: every installer already carries `hmc` beside `hmg`.

## Tests

*Phases 3 to 6.*

- ratatui's `TestBackend` renders every screen in process against an in-memory store
  and a scripted session, at 80 x 24 and 120 x 40, in `en-US`, `de`, `ja`, and
  `zh-CN`, and the tests assert the buffer: the frame in each connection state, every
  message fixture of `crates/hiveme-core/tests/fixtures/message/` as a bubble in both
  directions, the tree with a filter, the composer's key handling and drafts, every
  settings category, and the save timing with a fake clock.
- Built in phase 3, in `crates/hmc/src/tui/tests.rs` and beside each module: the frame
  in every connection state, size, and one of the four languages, and the too small
  line; the key map row by row; the tabs, the tools by key and by click, the pause
  toggle holding back a scripted notifier's toasts, connect and disconnect with a
  failure in the snackbar, the footer errors, the update notice, the help overlay, the
  first run's Broker fields and their one delayed save, clearing the topic, About, and
  a real `Session` on a scratch directory.
- `assert_cmd` proves the trigger: no arguments with piped stdin publishes; `--tui`
  with a redirected stdout exits 2; `--tui` conflicts with the publish and init
  options.
- `crates/hmc/tests/tui.rs` drives the real binary in a pseudo-terminal through
  `portable-pty` against the Docker broker of `publish.rs`: the frame comes up, a
  message published by one-shot `hmc` appears, a message typed and sent is
  acknowledged and stored, a language change re-renders the toolbar, and `Ctrl+Q`
  ends the session and restores the terminal. A second run closes the
  pseudo-terminal instead, which delivers `SIGHUP`, and proves that the broker session
  ended all the same. It is gated like `publish.rs`; the Linux workflow runs it.
- Quit tests against a scripted session: the Quit tool, `Ctrl+Q`, `Ctrl+C` in a
  focused text field, and a signal each end the session once and restore the terminal;
  a second press exits at once; a shutdown that hangs is cut off at
  `SHUTDOWN_TIMEOUT`.
