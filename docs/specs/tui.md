# HiveMe terminal UI (`hmc` interactive mode)

`hmc` run with no arguments on a terminal opens a terminal UI that has every feature
of `hmg`: toolbar, tabs, topic tree, chat view, composer, settings, about, status bar,
snackbar, OS notifications, and the update notice, rendered with
[ratatui](https://ratatui.rs/) in the terminal, in all nine languages, on the same
backend `hmg` uses. The one-shot publish mode of [cli.md](cli.md) is unchanged.

**Status: built.** [The terminal UI plan](../plans/plan-terminal-ui.md) built it in
phases 0 to 6. Every section below names the phase that built it, and the status table
in [app.md](app.md#status) records each one.

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

*Phase 3 for the shell, 4 for `messages/`, 5 for `settings/` and `about.rs`, 6 for the
end-to-end test and the performance tests.*

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
| `crates/hmc/src/tui/messages/mod.rs` | Tab 0: the split pane, its divider, the focus ring, and which pane a key goes to |
| `crates/hmc/src/tui/messages/topic_tree.rs` | The topic hierarchy, its filter, and the unread badges |
| `crates/hmc/src/tui/messages/message_view.rs` | The chat view: bubbles, virtual rows and their height cache, paging, the metadata row, the copy actions |
| `crates/hmc/src/tui/messages/json_tree.rs` | The collapsible tree used by `data` and raw JSON, read in the key order of the payload |
| `crates/hmc/src/tui/messages/detail.rs` | Per-node navigation of one message's trees |
| `crates/hmc/src/tui/messages/composer.rs` | The message box, the Level select, More Options, Send, and the drafts per topic |
| `crates/hmc/src/tui/settings/mod.rs` | The Settings tab: the category list, the panel with its scrolling and its select popup, the keys, and what an edit does to the config |
| `crates/hmc/src/tui/settings/form.rs` | A panel as rows of labels and controls: text and number fields, selects, checkboxes, radio rows, buttons, table rows, headings, and hints, with their layout, drawing, and focus order |
| `crates/hmc/src/tui/settings/{appearance,broker,topics,notifications,history,update,advanced}.rs` | One panel per category: its form, and how its controls read and write the config |
| `crates/hmc/src/tui/about.rs` | The About tab: the gradient letters, the cards and the links, and the table |
| `crates/hmc/src/tui/widgets/` | The one-line text input, the multi-line editor, `truncate`, and `wrap`. The select popup, checkbox, radio row, number field, and editable table are rows of `settings/form.rs`, the one place that uses them |
| `crates/hmc/src/tui/clipboard.rs` | `arboard`, then the OSC 52 escape sequence |
| `crates/hmc/src/tui/notify.rs` | The shared `DesktopToaster` of [session.md](session.md#notifications), using HiveMe's application identity and notification host on macOS |
| `crates/hmc/src/tui/open.rs` | Opens URLs and the config directory |
| `crates/hiveme-core/src/i18n/` | Locale resolution, the catalogs, plural rules, formatting; see [Languages](#languages). Built in phase 2 |
| `crates/hmc/src/tui/tests/` | The in-process tests: `mod.rs` holds the scripted session and the shell, `messages.rs` the Messages tab, `settings.rs` the Settings and About tabs, `performance.rs` a large history and a terminal that changes size |
| `crates/hmc/tests/tui.rs` | The end-to-end test: the real binary in a pseudo-terminal, against the Docker broker |
| `locales/*.json` | The nine catalogs, shared with the frontend |

Third party ratatui widgets (`tui-textarea`, `tui-tree-widget`) are used only if they
support the ratatui release in use. When phase 3 started, `tui-textarea` 0.7 still
depended on ratatui 0.29, so `widgets/` holds in-house controls, and the topic tree of
phase 4 is drawn by `messages/topic_tree.rs` itself; the choices are recorded in the
deviations of [app.md](app.md).

## Layout

*Phase 3, with the split pane of the Messages tab in phase 4.*

Rows, top to bottom: the toolbar (3), the update notice (0 or 1), the tabs (1), the
content (the rest), the footer (1).

```
 HiveMe v0.1.0 ─────────────────────────────────────────────────────────────────
 [F2 Disconn…] [F3 Pause] [F4 Clear] [F10 Settin…] [F1 About] [? Help] [^Q Quit]
────────────────────────────────────────────────────────────────────────────────
 Messages │ Settings ✕ │ About ✕
╭ Filter topics ─────╮╭ hiveme ────────────────────────────────────────────────╮
│>                   ││ ╰───────────────────────────╯                          │
│▾ hiveme (2)        ││                                                        │
│  ▾ build (2)       ││   ci-runner                                            │
│      ci (2)        ││ ╭────────────────────────────╮                         │
│    deploy          ││ │ CI                         │                         │
│                    ││ │ Nightly build 482 finished │                         │
│                    ││ ╰────────────────────────────╯                         │
│                    ││                                                        │
│                    ││                         ╭────────────────────────────╮ │
│                    ││                         │ Deployed hiveme to staging │ │
│                    ││                         ╰────────────────────────────╯ │
│                    ││                        deploy  Success  QoS 1  9:41 AM │
│                    │├────────────────────────────────────────────────────────┤
│                    ││ Write a message. Enter sends, Alt+Enter adds a line.   │
│                    ││                                                        │
│                    ││                                                        │
│                    ││                       [Info ▾] [More Options ▸] [Send] │
╰────────────────────╯╰────────────────────────────────────────────────────────╯
 ● connected  abc123.s1.eu.hivemq.cloud:8883  1 subscription  128 messages this
```

- The frame above is an 80 by 24 terminal with the newest message focused, which is
  why its metadata row shows and the older bubble at the top is cut.
- The pane split starts at 28 percent of the width, is clamped to 15..60, and moves by
  two percent with `Ctrl+Left` and `Ctrl+Right` or by dragging the divider with the
  mouse. The divider is the two borders where the panes meet; while it is dragged the
  message pane's border takes the secondary color. The split is remembered for the
  process only, as the GUI remembers its divider in the window.
- The topic pane is a rounded block titled `topics.filter`, the message pane a rounded
  block titled with the selected topic, and the pane with the focus has its border in
  the primary color. In the message pane, the chat view takes the rows above the
  composer, which is separated from it by a rule joined to the pane's borders and takes
  the rows it needs, at most 70 percent of the pane.
- Below 80 columns by 24 rows the whole frame is replaced by one centered line,
  `tui.tooSmall`, the way `hmg` enforces its 600 x 450 minimum.
- The Messages tab stays mounted while another tab is shown, so its scroll position,
  focus, and drafts survive a switch.
- The chat view holds `messages.empty` while the selected subtree has no stored rows
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

- A filter field is the first row of the topic pane, after `> `. `/` focuses it from
  anywhere in the Messages tab outside a text field, `Down` or `Enter` goes on to the
  tree, and `Esc` leaves it. Matching is on the whole path, case-insensitive, keeps a
  parent whose child matches, and opens what it found. `hiveme` stays visible even when
  it does not match, and a filter that matches nothing says `topics.noMatch` below the
  field, as in the GUI.
- The tree is drawn with `▾` and `▸` markers and two-cell indentation. `hiveme` is
  always present, selected, and highlighted at startup, even with an empty database.
  Every nonempty path is selectable, including a parent such as `hiveme/build` whose
  messages are all on `hiveme/build/ci`; an empty leading segment is an italic `/` in
  the secondary color and cannot be selected. Nodes follow the stored MQTT paths
  split on `/`; levels are never synthetic nodes.
- A cursor, drawn reversed while the tree has the focus, is what the keys move. `Up` and
  `Down` move it, `PageUp`, `PageDown`, `Home`, and `End` jump, `Right` opens a node or
  moves into an open one, `Left` closes a node or moves to its parent, `Space` toggles,
  and `Enter` selects. Selecting loads the subtree from the store, marks it read, and
  asks the session for the tree again, and is independent of expansion, as clicking a
  label in the GUI is. A mouse click on the marker toggles, on the label selects, and
  the wheel over the tree moves the cursor three rows.
- The unread badge is ` (N)` after the label in the primary color, rolled up from the
  descendants, grouped as the selected language groups digits, and capped at `999+`.
  The selected label is bold in the primary color.
- The first tree a user sees is fully open until the user opens or closes a node, after
  which new topics arrive closed, as `TopicTree.tsx` does. The tree is read again from
  the session whenever a row is stored or a topic is added.

### Message view

*Phase 4.*

- The message pane's title is the selected topic. Below it, the rows of the selected
  topic and all its recursive descendants, oldest at the top, newest at the bottom,
  following new messages unless the user has scrolled away from the newest. Where the
  view is when it does not follow is a row id and how many of its lines are above the
  view, so a page of older rows loading above never moves what is being read. `PageUp`
  at the top, `Up` on the oldest row, or the wheel at the top asks the store for the
  previous page of 200 rows, the `before` cursor being the oldest row loaded, until a
  page comes back short, as `get_messages` pages. Only the rows in view are laid out,
  and heights are cached per row, width, and expansion.
- A bubble is a rounded block, at most 80 percent of the list wide and at least 18
  cells, aligned left for incoming rows and right for outgoing rows, one cell from the
  pane's border. Outgoing means the terminal UI sent it, so what `hmg` sent from the
  same device is an incoming row here; [session.md](session.md#which-side-a-message-is-on)
  has the rule. An incoming bubble is preceded by the sender's name in bold, falling
  back to the sender id, never the application name; outgoing and senderless rows have
  no header.
- Tiers render as [gui.md](gui.md#message-view) lists them: an envelope shows a bold
  title, the body, and its `data` tree; raw JSON is a tree; raw text is shown as text;
  bytes show `N bytes` and the hex, sixteen pairs to a line; an encrypted message shows
  `🔒 encrypted (key <kid>)`, with `[enc]` where the glyph is unavailable; a newer `v`
  gets the `newer version` chip. A tree opens its top level and closes every branch
  below it, as `JsonTree` does, and lists the keys in the order the payload wrote them.
  Text wraps at spaces, and inside a word only when the word is wider than the bubble.
- Colors follow `payload.level`, independently of the topic or direction. `info` and
  `debug` use the regular fill and a muted border; `success`, `warn`, and `error` use
  the MUI palette values (`#2e7d32`, `#ed6c02`, `#d32f2f`) for the border and the badge,
  with a tinted fill only when a display mode is forced, because a tint needs a known
  background. An unknown level renders as `info` while the badge shows the raw name
  with the fallback in parentheses. See [Theme](#theme).
- The metadata row below a bubble, aligned to its right edge and running on to the
  right when the bubble is narrower than the row, holds the topic path relative to the
  selected tree topic (muted, as the GUI's `text.disabled`; empty for a row on the
  selected topic itself), the level badge in the level's color, `QoS n`, the retained
  marker `📌` (`[R]`), the newer-version chip, and the time in the selected language. It
  is drawn only for the focused row while the list has the focus, and its one-row space
  is reserved for every row so focusing does not move its neighbors. This is the GUI's
  hover row without a pointer. The focused bubble's border is bold, and in the primary
  color when the level gives it none.
- A centered muted line with the day in the selected language separates rows on
  different days.
- While the list has the focus, `Up` and `Down` move the focus from row to row, starting
  from the newest, and scroll just enough to show it; `PageUp` and `PageDown` scroll a
  page and focus the first or the last row fully shown; `Home` goes to the oldest loaded
  row and `End` to the newest, following again. A click focuses a row, and the wheel
  scrolls three lines without moving the focus.
- With a row focused: `c` copies the body, `r` copies the raw payload, `Space` opens
  every tree in the bubble or closes them all when all are open, and `Enter` opens the
  detail view. The snackbar confirms a copy or reports why it failed. The copy goes to
  the native clipboard through `arboard`; where none is reachable, as over SSH, the text
  is handed to the terminal in the OSC 52 escape sequence, which most terminals put on
  the system clipboard.
- The detail view takes the list's place: a rounded block titled with the row's full
  topic, with its date and time on the bottom border, the metadata line, and the
  content the width of the pane. A reversed cursor moves from tree node to tree node
  with `Up` and `Down`, `Space` or `Enter` opens or closes the node under it, `PageUp`
  and `PageDown` scroll, `c` and `r` copy, and `Esc` returns to the list. What is opened
  there stays open in the bubble.

### Composer

*Phase 4.*

- The message box sits below the rule, three rows minimum, growing to six, and the rule
  takes the primary color while the box has the focus. Its placeholder is
  `tui.composer.placeholder` or `tui.composer.placeholderJson`: the GUI's sentences with
  the newline key every terminal delivers, `Alt+Enter`, in place of `Shift+Enter`. Below
  it, right-aligned, `[Info ▾] [More Options ▸] [Send]`: the Level select in its level's
  color, More Options with `▸` or `▾`, and Send in the primary color. The focused control
  is drawn reversed and a disabled one dimmed; the More Options label gives up letters
  first when the row does not fit.
- `Enter` sends from every composer control without also activating it. `Alt+Enter` and
  `Ctrl+J` insert a newline, and so does `Shift+Enter` where the terminal reports it.
  `Space` does what a click does: it opens the Level popup, opens or closes More
  Options, sends from Send, and checks a checkbox; `Left` and `Right` pick the QoS
  radio. The Level popup is drawn above its button: `Up` and `Down` move, `Enter` or
  `Space` picks the highlighted level without sending, and `Esc` or a click elsewhere
  closes it. `Tab` and `Shift+Tab` move between the message box and the controls that
  can be used right now, as part of the tab's focus ring.
- The Level select offers Info, Error, Success, and Warn in that order, Info by
  default, colored as the GUI colors them, disabled in raw JSON mode while keeping its
  value.
- More Options shows Topic and Title as underlined one-line fields with their labels
  right-aligned: Topic relative to the selected tree topic, leading slashes stripped as
  typed, and Title disabled in raw JSON mode. Then the QoS row, `(●) Config ( ) 0 ( ) 1
  ( ) 2` followed by `[ ] Retain Message` and `[ ] As Raw JSON`, which wrap onto the next
  row when the pane is too narrow. Collapsing hides the controls and keeps every value
  in effect.
- The draft, level, options, and expansion state are kept per selected tree topic in
  process memory. Success clears only the originating topic's text, even when the
  selection moved while the message was on its way, and nothing else is sent until the
  broker has answered; failure keeps the text and goes to the snackbar. The composer is
  disabled while not connected or without a selection, and the reason is shown as its
  placeholder.
- Sending calls the session's publish, the same path `hmg` and one-shot `hmc` use. The
  bubble appears once the broker has acknowledged, and the copy the broker echoes back
  collapses into it by row id.

### Settings

*Phase 5, which replaced the Broker fields phase 3 built for a first run.*

A vertical category list (Appearance, Broker, History, Notifications, Topics, Update,
Advanced) and the panel of the selected category beside it, centered together with the
panel 96 columns at most, as the GUI centers its sidebar and panel. Both are rounded
blocks, and the one with the focus has its border in the primary color. The selected
category is bold in the primary color, and reversed while the list has the focus.
Appearance opens first; a first run opens on Broker with the URL focused.
The order matches `hmg` with its Editor category omitted. Browser text assistance
is available only in `hmg`; `hmc` preserves `gui.editor` when saving other settings.

Every panel is one description of rows, `settings/form.rs`, which gives its drawing, its
focus order, and its scrolling alike. A labeled row has a label column as wide as the
panel's longest label, at most two fifths of the panel, in the muted color, or bold in the
primary color while one of its controls has the focus.

- A text field is an underlined row. It shows the start of its text without the focus
  and follows the cursor with it. A number field is twelve cells wide and leaves the
  config alone until its text is a whole number that fits, as the GUI's `NumberField`
  does, keeping the text on screen meanwhile.
- A select is drawn `[value ▾]`, reversed while focused, and opens a bordered popup of its
  choices below it, or above it when there is more room there, scrolled to keep the
  highlighted choice in view.
- A checkbox, and a switch of the GUI, is `[✓] label`; a radio row is
  `(●) choice  ( ) choice` and moves its choices to a second row when they do not fit; a
  button is `[label]` in the primary color, and muted while it cannot be used.
- A group after the first is a heading, `─ Title ───`, with its action, such as Add a
  rule, at the right end. A hint sits under the controls when it fits there on one line
  and wraps across the panel otherwise.
- A panel taller than the screen scrolls, with a scrollbar on its right border: to keep
  the focused control in view, with `PageUp` and `PageDown` outside a text field, and
  with the wheel.

| Category | Rows |
|----------|------|
| Appearance | Mode, a radio row of Auto Mode, Light Mode, and Dark Mode; Theme, a select listing each palette in its own primary color; Language, a select of the names of the languages in themselves, `LANGUAGE_LABELS` |
| Broker | Protocol, a select of the four transports; URL; the line that says the URL that will be saved and its port, or `settings.urlHint` while the box is empty; Username; Password, masked until `Ctrl+H`, and the `Ctrl+H` hint; the TLS note; **Copy CLI setup** and `settings.copyCliSetupHint`, the GUI's tooltip; **Connection** with the client id prefix, keep alive, session expiry, and connect timeout; **Reconnect** with the first and the longest retry |
| History | `settings.historyHint`, Messages per topic, and Retention (days) |
| Notifications | Raise OS Notifications (checked) and Raise Topmost Window Notifications (unchecked) as separate checkboxes; **Rules** with Add at the right end, followed by four rows per rule: Name with Enabled and `[✕]`; Topic filter with Level; Title template with OS Notification; Body template with Topmost Window |
| Topics | **Subscriptions** with Add at the right end, then a row per subscription: the filter, the Absolute checkbox, and `[✕]` to remove it |
| Update | Check for updates, a select of Daily, Weekly, and Monthly |
| Advanced | `settings.advancedHint`, then **Encryption** and **Cloud API**, each saying `settings.notImplemented`. Nothing takes the focus |

The Broker URL follows `src/lib/brokerUrl.ts` through its Rust twin,
`hiveme_core::config::BrokerUrlParts`. The box holds the rest of the URL exactly as it was
pasted and the protocol list holds the scheme; the config is saved with the scheme in
front, `mqtts://abc123.s1.eu.hivemq.cloud:8883`, and empty while the box is. A scheme typed
or pasted into the box moves the list and leaves the rest in the box, and a protocol
chosen before a URL is typed stays chosen. Both ports are tested against the cases of
`crates/hiveme-core/tests/fixtures/broker_url.json`.

A subscription row is written back trimmed, as a bare string when it is relative and as
`{ "filter", "absolute": true }` when it is not, and a cleared filter keeps its row. Add
appends `#`, or the rule the GUI adds (`rule-<n>`, `info`, Info, enabled, Topmost Window unchecked, and the default
templates), and moves the focus to its first field. Remove deletes its row and leaves the
focus on the Remove of the row that took its place, or on Add when no row is left.

Keys:

- On the list, `Up` and `Down` change the category; `Tab`, `Enter`, and `Right` enter the
  panel at its first control and `Shift+Tab` at its last.
- In the panel, `Tab` and `Down` move to the next control and `Shift+Tab` and `Up` to the
  previous one. `Tab` from the last control, `Shift+Tab` or `Up` from the first, and `Esc`
  from any go back to the list. A button that cannot be used is skipped.
- `Enter` in a text field saves at once and moves to the next control. `Space` and `Enter`
  on a select open its popup, on a checkbox toggle it, on a button press it, and on a
  radio row move it to its next choice; `Left` and `Right` move a radio row either way.
- In an open popup, `Up`, `Down`, `PageUp`, `PageDown`, `Home`, and `End` move the
  highlight, `Enter` and `Space` pick, and `Esc` closes it and keeps the focus on its
  select. Any other key or click closes it first.
- `Ctrl+H` deletes a character while typing, including terminals that send it for Backspace. Outside text fields it shows or hides the password in the Broker category.
- A click focuses a control and does what `Space` does. A click on a radio choice or on a
  popup entry picks it, and a click on the select whose popup is open only closes it.

Saving follows the GUI store. An edit changes the config the screen is drawn from at
once, so a language, a theme, or a display mode applies to the whole screen in the same
frame, and the config goes to the session 500 ms after the last edit. Writes run one at a
time: edits made during a write are written as soon as it finishes, with the newest
values, and the answer to an older write never replaces a newer edit on screen. Success
is silent. A refused config goes to the snackbar, the edits stay on screen, and the next
edit retries the whole config. The session validates, writes, recompiles the rules, and
reconnects only when broker or subscription fields changed.

Copy CLI setup can be used while the URL, the username, and a password or a password
reference are filled in, `isBrokerUsable` of the GUI. It first writes the pending edits
and waits for the writes in progress, then, when the last write succeeded, asks the
session for the setup string of [config.md](config.md#the-setup-string), writes its
apostrophes and the quotes that look like one as JSON escapes, and copies
`hmc --init '<json>'` through `arboard` or OSC 52, as the GUI escapes and copies it. The
snackbar says `settings.cliSetupCopied`; a write that failed copies nothing.

### About

*Phase 5.*

`About.tsx` in cells, scrolled when it is taller than the screen:

- `HiveMe` three rows high in block letters, colored column by column from `#ffb300` to
  `#e65100`, the GUI's gradient, or one row of bold letters where the column is too
  narrow.
- The version chip, `(v<version>)`, and the tagline, centered.
- The Author and GitHub cards, rounded blocks four rows high in a column of at most 72
  cells, each with its caption and its value. The focused card has its border and its
  value in the primary color.
- The table of the device, the config file, the history database, and the license, whose
  values wrap.
- The copyright line: `about.copyright`, the author's name, and `caoccao.com`, the two
  names underlined in the primary color, as the GUI's links are.

`Tab`, `Shift+Tab`, and the arrows move the focus through the two cards and the two
names; `Enter`, `Space`, or a click opens the page: the author's for the card and the
name, the repository for the GitHub card, and `https://www.caoccao.com/`. `PageUp`,
`PageDown`, `Home`, `End`, and the wheel scroll. There is no icon image.

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
in the error color. They end the focus ring of the Messages tab, after the composer's
controls, so `Tab` and `Shift+Tab` reach them, drawn reversed, and `Enter` opens the
focused one. While the quit path waits for the broker, `tui.quitting` comes first.

### Snackbar

*Phase 3.*

A top-center overlay one row high, success or error colored, shown for four seconds or
until a key is pressed, one at a time. It is driven by the same `notifyInfo` and
`notifyError` calls the GUI store has: command errors, copy confirmations, save
failures. It is in-app feedback and unrelated to OS notifications.

It is drawn on the top row, over the toolbar's title, white on the error color for a
failure and on the success color for a confirmation, the two severities the GUI's filled
`Alert` uses, at most 80 percent of the width, with a detail of several lines folded
onto one. The key that dismisses it still does its work, except `Esc`, which only
dismisses it, and a click on it dismisses it too.

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
update notice while it is shown. On a terminal too short for the list the blank lines
between the sections go, and when it is still too tall the keys that work everywhere
are drawn beside the rest. The key names are written as keyboards print them in
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
| Messages | `Tab`, `Shift+Tab` | Cycle focus: topic filter, tree, message list, composer, footer errors |
| Messages | `Ctrl+Left`, `Ctrl+Right` | Move the pane divider |
| Tree | `Up`, `Down`, `PageUp`, `PageDown`, `Home`, `End`, `Left`, `Right`, `Space`, `Enter`, `/` | Move, jump, collapse or expand, toggle, select, focus the filter |
| Message list | `Up`, `Down`, `PageUp`, `PageDown`, `Home`, `End`, `c`, `r`, `Space`, `Enter` | Move focus, page, jump, copy body, copy raw, toggle trees, detail view |
| Detail view | `Up`, `Down`, `Space`, `Enter`, `PageUp`, `PageDown`, `c`, `r`, `Esc` | Move between nodes, toggle a node, scroll, copy, back to the list |
| Composer | `Enter`; `Alt+Enter`, `Ctrl+J`, `Shift+Enter`; `Space`; `Left`, `Right`; `Tab` | Send; newline; what a click does; QoS; next control |
| Settings | `Up`, `Down`, `Tab`, `Shift+Tab`, `Enter`, `Space`, `Left`, `Right`, `PageUp`, `PageDown`, `Ctrl+H` | Category or control, open a select, toggle, or press, a radio choice, scroll, show or hide the password |
| About | `Tab`, `Shift+Tab`, the arrows, `Enter`, `Space`, `PageUp`, `PageDown`, `Home`, `End` | Move between the cards and the links, open one, scroll |
| Mouse | click, wheel, drag | Select tabs, tools (Quit included), topics, rows, controls, choices, and links; scroll the pane under the pointer; move the divider |

Every text field takes the usual editing keys (`Left`, `Right`, `Home`, `End`,
`Backspace`, `Delete`, `Ctrl+U`, `Ctrl+A`, `Ctrl+E`), and a paste arrives through
bracketed paste when the terminal supports it; a paste into a one-line field drops its
line breaks.

- A key acts when it is pressed or repeats, never when it is released, which Windows
  reports as well.
- Without the keyboard protocol a terminal sends `Ctrl+/` as the byte crossterm reads
  as `Ctrl+7`, so `Ctrl+7` opens the help there; under the protocol it selects tab 7.
- `Up` and `Down` move between the controls of a settings panel, and `Ctrl+H` outside
  text fields shows or hides the password in the Broker category.
- `Ctrl+Left` and `Ctrl+Right` move the divider in every focus of the Messages tab,
  text fields included, and a drag on the divider moves it too.
- The wheel scrolls the pane under the pointer: the list by three lines, the tree by
  moving its cursor three rows.
- `/`, `c`, `r`, and `Space` are keys only outside a text field; in one they are text.
- A plain terminal sends `Ctrl+J` as the line feed byte, which raw mode reports as
  `Ctrl+J` rather than as `Enter`, so it is a newline key every terminal has.

## Theme

*Phase 3.*

`gui.theme` and `gui.displayMode` are honored, so the two applications look alike and
the Appearance settings mean the same thing in both.

- The twenty palettes of [gui.md](gui.md#theme) map their primary and secondary hex
  colors to `Color::Rgb`. The primary color marks the selected topic, the active
  toolbar button, badges, and the focused control; the secondary color marks the
  empty leading segment of an absolute topic and the divider while it is dragged.
- `Auto` keeps the terminal's own background and foreground and draws no fills, since
  the terminal's colors are unknown. `Light` and `Dark` set the background and the
  foreground and draw the bubble fills of the GUI: in `Light` an outgoing bubble is
  black with white text and an incoming one is the `action.hover` gray; in `Dark` an
  outgoing bubble is grey 800. Severity tints exist only in a forced mode.
- A terminal without truecolor receives the nearest ANSI color from crossterm. The
  design never relies on a fill or a tint alone to convey a meaning: the badge text
  carries the level.
- Text is never transformed. Labels read as they are written in the catalogs.
- The glyphs `✕`, `●`, `✓`, `…`, `│`, `▾`, `▸`, `🔒`, `📌`, and `(●)` fall back to
  `x`, `*`, `x`, `...`, `|`, `v`, `>`, `[enc]`, `[R]`, and `(*)` on the Linux console
  (`TERM=linux`) and on a Windows console without a modern terminal marker (`WT_SESSION`,
  `TERM_PROGRAM`, `WEZTERM_EXECUTABLE`, `ALACRITTY_LOG`, `ConEmuANSI`, or `SSH_TTY`), whose fonts lack them. The choice is made once for the whole screen.
- The block letters of the About tab (`█`, `▀`, `▄`) and the scrollbar of a settings
  panel (`│`, `┃`), composer joins (`├`, `┤`), and rounded bubble borders have no ASCII
  fallback. Their appearance depends on the console font; the fallback set above
  covers the explicit control markers.

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

*Phase 3, and phase 6 for the macOS identity.*

Interactive `hmc` raises both OS notifications and topmost window notifications from
the shared rules of [gui.md](gui.md#notifications). The independent channel flags,
OS rate limiter, current-session publish exclusion, and pause toggle use the same
code as hmg. Each rule has separate OS Notification and Topmost Window checkboxes,
both unchecked by default and gated by the corresponding global switch. F3 pauses
both channels and discards queued notifications; resuming only admits new messages.
Messages from every other MQTT session run through the rules, including
another hmc process or hmg sharing the same config. Sender metadata does not suppress
notifications, and there is no device-based checkbox.

Both applications use `hiveme_core::desktop::DesktopToaster`: native D-Bus notifications
on Linux, the registered HiveMe toast identity on Windows, and the modern macOS
notification API under HiveMe's `com.caoccao.hiveme` application identity. Packaged
hmc reuses its sibling hmg's signed application bundle; unbundled hmc uses a cached
bundle with the same identity and HiveMe icon. Topmost notifications use the single
`hmg --notification-host` window on every desktop OS. `hmg` must be installed beside
`hmc` or on PATH; both executables ship together. The host opens no config or database.
OS notifications and the topmost window display the HiveMe app icon. Each topmost
notification carries its Close label translated into the session's saved language,
so changing languages also works when the same host serves both applications.
See [GUI platform notes](gui.md#platform-notes) for authorization, failure handling,
and the per-OS manual checklist, which also applies to interactive hmc.

## Client identifier

*Phase 1.*

Interactive `hmc` connects as `Role::Tui`: `<broker.clientIdPrefix>-hmc-<device.id alphanumeric>-<8 random characters>`, the shape one-shot `hmc` already
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
- Built in phase 3, in `crates/hmc/src/tui/tests/mod.rs` and beside each module: the frame
  in every connection state, size, and one of the four languages, and the too small
  line; the key map row by row; the tabs, the tools by key and by click, the pause
  toggle holding back a scripted notifier's toasts, connect and disconnect with a
  failure in the snackbar, the footer errors, the update notice, the help overlay, the
  first run's Broker fields and their one delayed save, clearing the topic, About, and
  a real `Session` on a scratch directory.
- Built in phase 4, in `crates/hmc/src/tui/tests/messages.rs` and beside each module:
  every fixture of `crates/hiveme-core/tests/fixtures/message/` as a bubble in both
  directions and both forced modes, asserting the side, the fill, the level's border,
  the bold title, the collapsed tree, the relative topic, the level badge, the encrypted
  placeholder, the newer-version chip, the hex of bytes, and the sender header; a `data`
  tree opened with `Space` and node by node in the detail view; the tree's rolled-up
  badges, label and marker clicks, keys, and filter; a live message raising a badge until
  its topic is selected; `Enter` from every composer control, the newline keys, the focus
  ring, drafts per topic across a tab switch and a reconnect, raw JSON, the Level popup,
  the relative topic and the options, and a send that finishes after the selection
  moved; paging through 450 rows on three topics; the copy actions and their snackbar;
  the divider; and the tab in German, Japanese, and Chinese at 80 x 24. The scripted
  session keeps its rows in an in-memory store. One test runs against the Docker broker
  of `publish.rs` and is skipped the same way: a message sent from the composer is
  acknowledged, stored once, and shown once after its echo, and a message published by
  the one-shot mode appears live and raises the badge until its topic is selected.
- Built in phase 5, in `crates/hmc/src/tui/tests/settings.rs` and beside each module,
  mirroring `Config.test.tsx`: every category at both sizes in the four languages; the
  focus ring of a panel; the Broker panel's split URL, masked password, and `Ctrl+H`; the
  three URLs of the console saved with their protocol in front; a pasted scheme moving
  the list; a protocol chosen before the URL; the save timing against given instants, two
  edits and one write, an edit during a write written next with the newest values, and a
  refused config kept on screen with the snackbar; Copy CLI setup writing the edits first,
  copying the whole command with the language and the quotes escaped, copying nothing
  after a failed write, and skipped while the broker is not usable; subscriptions written
  back as strings or objects with Add and Remove; the switches, a rule's level, Add, and
  Remove; number fields that leave the config alone; a language change re-rendering
  every label in the same frame; a theme change recoloring the selected topic; the
  display mode's radio row painting the screen; the select popup by key and by click;
  the Advanced sections; scrolling; and the About tab's content and links at both sizes.
  `crates/hiveme-core/tests/config.rs` and `src/lib/brokerUrl.test.ts` read the same
  broker URL cases.
- `assert_cmd` proves the trigger: no arguments with piped stdin publishes; `--tui`
  with a redirected stdout exits 2; `--tui` conflicts with the publish and init
  options.
- Built in phase 6, in `crates/hmc/src/tui/tests/performance.rs`: ten thousand rows over
  two hundred topics, envelopes, raw JSON, and long text in both directions, opened at
  200 x 60, paged through to the oldest row, moved through with the list keys, joined by
  fifty live messages, and walked node by node through the tree, with every frame and
  the work before it within 150 ms in a release build and 750 ms in a debug one; and a
  terminal resized between two frames on each tab, to sizes above, at, and below
  80 x 24, with a key, the wheel, and a click arriving while the application still knows
  the old size, drawn whole at the new one with no click target outside it.
- Built in phase 6, `crates/hmc/tests/tui.rs` runs the real binary in a pseudo-terminal
  from `portable-pty`, openpty on Linux and macOS and ConPTY on Windows, against the
  Docker broker of `publish.rs`. `vt100` reads what `hmc` writes back into a screen, and
  the test answers the cursor position and device attribute queries a terminal answers.
  Every run waits for the status bar to read `connected` with one subscription, which
  the session reports only once the broker has acknowledged the subscription; the
  Disconnect tool appears as soon as a connection starts, and a message published
  before then has no subscriber to reach. One run sees a message that one-shot `hmc`
  published as another device appear under that device's name; types a message, presses `Enter`,
  and finds the bubble above an empty message box and one outgoing row in `HiveMe.db`;
  changes the language with `F10`, `Tab`, and the Language select, and finds the German
  toolbar and `gui.language` saved; and leaves with `Ctrl+Q`: exit 0, the frame and every
  terminal mode gone, raw mode off again on Unix, `the MQTT connection and session have
  ended` in `hmc.log`, and no session left on the broker for the client identifier the
  log names. A second run closes the pseudo-terminal instead, which is `SIGHUP` on Unix
  and `CTRL_CLOSE_EVENT` on Windows, and finds the same end in the log and on the broker,
  and exit 0 on Unix. A third runs `hmg`'s session in the test process on the same config
  and database, see [session.md](session.md#two-processes-one-installation). The file is
  skipped like `publish.rs`; the Linux workflow runs it, and it runs on Windows through
  ConPTY wherever Docker is.
- Quit tests against a scripted session: the Quit tool, `Ctrl+Q`, `Ctrl+C` in a
  focused text field, and a signal each end the session once and restore the terminal;
  a second press exits at once; a shutdown that hangs is cut off at
  `SHUTDOWN_TIMEOUT`.

Idle ticks redraw only after a visible change or during a reconnect
countdown. Bubble parsing/wrapping and day labels are cached until invalidated. The
unfiltered topic tree is traversed directly. Forms are reused during input handling
and drawing; long single-line fields scroll in linear time. Split arithmetic uses
32-bit intermediates. After event lag the tree, status, and selected page are reloaded.
Mark-read and clear operations run off the event loop, with completion applied on a tick.
Rule IDs remain unique after deletion; blank subscription drafts defer autosave.
