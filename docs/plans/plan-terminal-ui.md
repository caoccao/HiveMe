# HiveMe Terminal UI Plan

Status: proposed (2026-09-13)
Inputs: [docs/plans/plan-initialization.md](plan-initialization.md) (the predecessor, whose phases are all done except phase 6), `docs/specs/*.md`, the built `hmg` frontend under `src/`, the `hmg` backend under `src-tauri/src/`, `crates/hmc`, `crates/hiveme-core`, and the sibling clone `../ratatui` (0.30.2 plus 90 unreleased commits) used as the API reference.
Output: `hmc` gains an interactive mode that opens a ratatui terminal UI when it is run with no arguments on a terminal. The terminal UI has every feature of `hmg` (toolbar, tabs, topic tree, chat view, composer, settings, about, status bar, snackbar, OS notifications, update notice) rendered in the terminal, in all nine languages, on top of a backend that `hmg` and `hmc` share in `hiveme-core`. The one-shot publish mode of `hmc` is unchanged.

---

## 1. Decisions

Answers gathered before writing this plan. They are binding for the phases below unless a later plan revises them. Rows marked *assumed* were not asked; they follow from the answered ones and can be overturned in review.

| # | Question | Decision |
|---|----------|----------|
| 1 | Where the shared backend lives | A new module `hiveme_core::session`, behind a new `session` feature that implies `storage`. `src-tauri/src/{mqtt,controller,notification,config,update}.rs` become thin adapters over it. `hmc` turns the feature on and therefore links SQLite from now on. |
| 2 | Database | Interactive `hmc` opens the same `HiveMe.db` as `hmg`. Both may run at once: SQLite WAL serializes writers, envelopes collapse by `(topic, msg_id)`, and a payload with no id (raw JSON, text, bytes) that both applications receive becomes two rows. Documented, not prevented. |
| 3 | ratatui dependency | `ratatui = "0.30"` from crates.io with the default crossterm backend. `../ratatui` is the reference for API and examples only. |
| 4 | When the TUI opens | No message argument, no publish option, and stdin is a terminal. Piped stdin still publishes the body, so `echo hi \| hmc` is unchanged. `--tui` forces the TUI regardless of stdin; it conflicts with every publish and init option. |
| 5 | Notifications | Kept. Interactive `hmc` raises OS notifications from the shared rules, has the pause toggle, and has the snackbar for in-app feedback. |
| 6 | Toolbar | Kept, exactly three rows: a top border, the row of tools, a bottom border. Every tool also has a key binding. |
| 7 | Update check | Kept: the GitHub release check, `update.checkInterval`, `ignoreVersion`, the notice line, and the skip action, through the shared session. |
| 8 | Theme and display mode | Honored. The twenty palettes map their primary and secondary hex colors to `Color::Rgb`; `Light` and `Dark` select bubble fills and severity tints; `Auto` keeps the terminal's own background and foreground. |
| 9 | Scope of translation | Every TUI label, every line `hmc` itself prints in publish and init mode, and the clap help. Diagnostics authored by `hiveme-core` (broker refusals, validation messages) stay English, as they do in `hmg`. The `hmc: <category>:` prefix words and the exit codes are a contract for scripts and never change. |
| 10 | Catalogs | One shared JSON set. `src/i18n/locales/*.json` moves to `locales/*.json` at the repository root; the frontend imports it from there and `hiveme_core::i18n` embeds it with `include_str!`. TUI and CLI keys are added to the same files. |
| 11 | Setup string | `BrokerInit` v1 gains an optional `language`, filled from `gui.language` by `hmg`. `hmc --init` writes it when creating the config and updates `gui.language` on an existing config. Absent means `en-US`. No version bump. |
| 12 | Tests | ratatui `TestBackend` renders every screen in process against an in-memory store and a scripted session. One end-to-end test drives the real `hmc` binary in a pseudo-terminal against the Docker broker. |
| 13 | OS notifications in `hmc` | The rule engine, rate limiter, and pause toggle are in the shared session; each application supplies the final show call through a `Toaster` trait. `hmg` keeps its plugin and its Windows toast path. `hmc` uses `notify-rust` on Linux and macOS and `tauri-winrt-notification` on Windows with the same `HiveMe` application identity. |
| 14 | Client identity | A new `Role::Tui`: client id `<prefix>-hmc-<device8>-<random>` as today, with `hmg`'s connection behavior: reconnect with backoff, resubscribe, `sessionExpirySecs` from the config for the life of the process, session ended on quit. |
| 15 | First run without a config | Interactive mode writes the default config (fresh `device.id`, `en-US`), opens on the Settings tab's Broker category, and stays disconnected until a broker is entered. Publish mode keeps its exit 3. |
| 16 | Keys and mouse | The key map in section 5.9, proposed here. Mouse clicks and the wheel are handled where crossterm reports them. |
| 17 | Spec files (*assumed*) | Two new specs: `docs/specs/tui.md` for the terminal UI and `docs/specs/session.md` for the shared backend. `gui.md` keeps its component and layout sections and points at `session.md` for what used to be its IPC semantics. |
| 18 | Clipboard (*assumed*) | `arboard` first, OSC 52 escape sequence as the fallback when no native clipboard is reachable, which is the case over SSH. |
| 19 | Locale formatting (*assumed*) | Hand-written per-locale rules in `hiveme_core::i18n`: CLDR plural categories for the nine locales, digit grouping, byte sizes, durations, and time and date patterns through `chrono`. No ICU dependency. The results match the values `src/i18n/index.test.ts` already asserts. |
| 20 | Logging in the TUI (*assumed*) | stderr is the screen, so interactive mode logs to `hmc.log` beside the config file, only when `--verbose` or `RUST_LOG` is set. Publish mode is unchanged. |
| 21 | Runtime (*assumed*) | The TUI runs on a multi-thread tokio runtime; publish mode keeps its current-thread runtime. |
| 22 | Pane split and drafts (*assumed*) | The topic pane width, composer drafts, expansion state, and the selected settings category live in process memory only, as the GUI keeps its drafts. Nothing new is written to the config. |
| 23 | Minimum terminal (*assumed*) | 80 columns by 24 rows. Below that the TUI draws one centered line asking for a larger terminal, the way `hmg` enforces 600 x 450. |
| 24 | End-to-end driver (*assumed*) | The pseudo-terminal test is a Rust integration test using `portable-pty`, not a Deno script: Deno has no PTY API. `scripts/ts/test-e2e.ts` stays the `hmg` driver. |
| 25 | No subcommands (answered in phase 0) | `hmc [OPTIONS] [MESSAGE]` stays the whole grammar. A bare word is the message, so `hmc config` publishes `config`; every mode is an option, which is why the trigger of decision 4 is "no arguments" and the forcing flag is `--tui`. The helpers the initialization plan once spelled as subcommands are options when they are built. |

---

## 2. Constraints that shape the design

### 2.1 What the code looks like today

- `crates/hmc` is a one-shot publisher: `cli.rs` (clap), `run.rs` (init, publish), `failure.rs` (exit codes), a current-thread runtime, no SQLite. No argument plus a terminal on stdin is exit 2.
- Everything `hmg` does between the broker and the screen lives in `src-tauri/src/`: `mqtt.rs` (session lifecycle, the pump that stores, emits, and evaluates rules, status forwarding), `controller.rs` (publish, tree building, paging, set_config with reconnect, prune loop), `notification.rs` (rule engine, limiter, pause, the Windows identity), `config.rs` (the in-memory holder, load error, quiet writes, `needs_reconnect`), `update.rs` (GitHub check), `protocol.rs` (`Status`, `TopicNode`, `MessageRow`, `PublishOptions`, `About`, `UpdateCheckResult`). All of it takes a Tauri `AppHandle` and emits through `Emitter`.
- The frontend store (`src/lib/store.tsx`) is the behavioral reference for the TUI's state: per-subtree message cache, paging with `before`, mark read on select, cached ancestor invalidation on clear, single-writer debounced config saves, echo collapse by row id.
- The frontend catalogs are nine JSON files with i18next keys, `{{var}}` interpolation, and `_one`/`_other` plural suffixes. `src/i18n/index.test.ts` enforces coverage, interpolation variables, plural categories per locale, and specific formatted outputs.

### 2.2 Terminal facts

- Terminals do not reliably deliver `Ctrl+1`..`Ctrl+9`, `Ctrl+Tab`, or `Shift+Enter` as distinct keys. crossterm can ask for the kitty keyboard protocol (`PushKeyboardEnhancementFlags`), which Windows Terminal, kitty, WezTerm, foot, and recent iTerm2 support; where it is unsupported the TUI must still be usable, so every binding in section 5.9 has a fallback that plain terminals deliver.
- ratatui 0.30 provides `ratatui::init()` and `ratatui::restore()` for raw mode and the alternate screen, `DefaultTerminal`, `TestBackend` for in-process rendering, and the widgets the TUI needs: `Block` with rounded borders, `Paragraph` with wrapping, `List`, `Table`, `Tabs`, `Scrollbar`, `Clear` for popups. Its `mouse-drawing`, `popup`, `user-input`, `input-form`, `tabs`, and `demo2` examples in `../ratatui/examples/apps/` are the models for the corresponding pieces.
- Async events need crossterm's `event-stream` feature (`EventStream`), selected in the same loop as the session's event channel and a tick timer.
- Windows: `hmc.exe` stays a console application; crossterm enables virtual terminal processing; Windows Terminal renders truecolor, legacy conhost degrades to the nearest ANSI color. ConPTY makes the pseudo-terminal test runnable there too.
- CJK catalogs need display-width aware layout. ratatui measures with `unicode-width`, so labels are truncated by width, never by byte or char count.
- Panics must restore the terminal, so a panic hook calls `ratatui::restore()` before the default hook prints.

### 2.3 Two processes, one config, one database

- SQLite: both applications open `HiveMe.db` in WAL mode. A `busy_timeout` (5 s) is set so a write that meets the other process's write waits instead of failing. Unread counts stay right because the second insert of an envelope is recognized as existing and does not count. Pruning may run in both; the second pass finds nothing.
- Config: both applications keep the config in memory and write it atomically. Neither watches the file, so a setting changed in one is seen by the other at its next start. Documented; a file watcher is an open item.
- Broker: `hmg` uses `<prefix>-hmg-<device8>`, one-shot `hmc` uses `<prefix>-hmc-<device8>-<random>`, and interactive `hmc` uses the same suffixed shape, so no two processes share an identifier.

---

## 3. Architecture

### 3.1 The shared session, `hiveme_core::session`

Feature `session = ["storage"]`. `hmg` and `hmc` both turn it on. The module is the Tauri-free core of today's `src-tauri` orchestration, with events on a channel instead of an `Emitter`.

```
crates/hiveme-core/src/session/
  mod.rs        Session: construct, connect, disconnect, publish, set_config, status, shutdown
  config.rs     ConfigStore: the in-memory holder, load error, save, save_quietly, needs_reconnect
  mqtt.rs       the connection lifecycle and the pump (store, event, notify), from src-tauri/src/mqtt.rs
  notify.rs     Notifier: RuleEngine + NotificationLimiter + pause, the Toaster trait
  history.rs    topic tree building, paging, mark read, clear, prune loop, from controller.rs
  update.rs     the GitHub release check, interval logic, skip version, from src-tauri/src/update.rs
  types.rs      About, Status, TopicNode, MessageRow, PublishOptions, UpdateCheckResult, SessionEvent
```

```rust
pub struct Session { /* Arc<Store>, ConfigStore, Mqtt, Notifier, counters, update slot, broadcast::Sender<SessionEvent> */ }

pub enum SessionEvent {
  Status(Status),
  Message(MessageRow),
  TopicAdded { topic: String },
  NotificationFired { rule_id: String, message_id: String, topic: String },
}

pub trait Toaster: Send + Sync {
  fn show(&self, title: &str, body: &str) -> Result<(), String>;
}

impl Session {
  pub fn open(config_path: Option<&Path>, app: SessionApp, toaster: Arc<dyn Toaster>) -> Result<Arc<Self>>;
  pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent>;
  pub fn about(&self) -> About;
  pub fn config(&self) -> Config;              // and config_path, database_path, load_error
  pub async fn set_config(&self, config: Config) -> Result<Config>;   // validate, write, reload rules, reconnect when needed
  pub fn set_config_quietly(&self, config: Config) -> Result<()>;
  pub fn broker_init(&self) -> Result<String>; // carries gui.language
  pub fn status(&self) -> Status;
  pub async fn connect(&self) -> Result<Status>;
  pub async fn disconnect(&self) -> Result<()>;
  pub fn topic_tree(&self) -> Result<Vec<TopicNode>>;
  pub fn messages(&self, topic: &str, before: Option<i64>, limit: u32) -> Result<Vec<MessageRow>>;
  pub fn mark_read(&self, topic: &str) -> Result<()>;
  pub fn clear_topic(&self, topic: &str) -> Result<u64>;
  pub async fn publish(&self, topic: &str, body: &str, options: PublishOptions) -> Result<MessageRow>;
  pub fn set_notifications_paused(&self, paused: bool) -> Status;
  pub fn update_result(&self) -> Option<UpdateCheckResult>;
  pub fn skip_version(&self, version: &str) -> Result<()>;
  pub fn start_background_work(self: &Arc<Self>);   // prune loop, first connection, update check
  pub fn begin_shutdown(&self);
  pub async fn shutdown(&self) -> Result<()>;
}

pub enum SessionApp { Gui, Tui }   // selects Role::Gui or Role::Tui and the sender.app value
```

- `types.rs` keeps the serde derives and the camelCase renames that `protocol.rs` has today, so `hmg` passes the values to the frontend unchanged and `protocol.ts` does not move. `src-tauri/src/protocol.rs` re-exports them and keeps only `AppState` and the event name constants.
- `hiveme_core::mqtt::Role` gains `Tui`: `app()` is `hmc`, `unique_client_id()` true, `clean_start()` true, `reconnects()` true, `session_expiry_secs()` from the config.
- `ureq` moves to `hiveme-core` under the `session` feature. The rustls provider situation is the one deviation 12 of the initialization plan already handles: `mqtt::tls` installs `aws-lc-rs` once per process, and `hmc` now needs that too because it links `ureq`'s `ring`.
- Errors stay `hiveme_core::Error`; the session adds variants it needs (`NotConnected`, `Quitting`, `NothingToSend`, `NotJson`) rather than `anyhow`, because `hmc` maps errors to exit codes through `Failure::from`.

### 3.2 What is left in `src-tauri`

| File | After phase 1 |
|------|---------------|
| `lib.rs` | Unchanged command list; each command delegates to `controller.rs`, which calls the session |
| `controller.rs` | One line per command plus `open_config_file`, which needs the opener plugin |
| `mqtt.rs` | Replaced by a task that forwards `SessionEvent` to `app.emit` under the existing event names |
| `notification.rs` | The `Toaster` implementation: the plugin on Linux and macOS, `tauri-winrt-notification` and the registry identity on Windows |
| `config.rs` | Removed; `window.rs` and `update` code call the session's config functions |
| `update.rs` | Removed; `window.rs` calls `session.start_background_work()` |
| `protocol.rs` | Re-exports the session types, keeps `AppState` and the event name constants |
| `window.rs` | Unchanged behavior; uses the session for geometry writes and shutdown |

The IPC contract (`gui.md` command table, `protocol.ts`) does not change. The native end-to-end test is what proves the refactor preserved behavior.

### 3.3 `hiveme_core::i18n`

Feature `i18n`, default off, on for `hmc`. `hmg` keeps react-i18next; the backend of `hmg` still authors English diagnostics.

```
locales/                       moved from src/i18n/locales/, imported by both sides
  de.json en-US.json es.json fr.json it.json ja.json zh-CN.json zh-HK.json zh-TW.json
crates/hiveme-core/src/i18n/
  mod.rs      Locale enum (De, EnUs, Es, Fr, It, Ja, ZhCn, ZhHk, ZhTw), resolve(tag) with the rules of src/i18n/index.ts
  catalog.rs  the flattened key table per locale, loaded from include_str! on first use
  plural.rs   CLDR categories: en/de/es/it one|other, fr one for 0 and 1, ja/zh other only
  format.rs   number grouping, bytes, duration, time, date, date-time per locale
```

- `t(locale, key)`, `t_with(locale, key, &[("var", value)])`, `t_count(locale, key, count)`. Missing keys fall back to `en-US`, then to the key itself, and a Rust test makes a missing key a failure so the fallback is never seen in a release.
- The frontend import becomes `../../locales/en-US.json` and the vitest coverage test reads `locales/`. Vite serves JSON outside `src/` because the repository root is its workspace root.
- Keys added for the TUI and the CLI go under `tui.*`, `cli.*`, and `help.*` namespaces in the same files, so the existing vitest coverage test also covers them.
- The clap help is rendered from the catalog: `Cli::command()` is mutated with translated `about`, argument help, and the `help`/`version` flag descriptions, and the help template's headings come from `help.usage`, `help.arguments`, `help.options`. `cli_help_matches_spec` renders `en-US` and still compares with the block in `cli.md`.
- Publish and init mode choose the language from `gui.language` of the config they loaded, `en-US` when there is none, and `--init` prefers the language inside the setup string when it carries one.

### 3.4 `crates/hmc` after this plan

```
crates/hmc/src/
  main.rs            parse, choose the mode, pick the runtime, install the panic hook for the TUI
  cli.rs             clap, --tui, translated help
  failure.rs         unchanged
  run.rs             publish and init, translated lines
  tui/
    mod.rs           run(): terminal setup, event loop (crossterm EventStream, session events, ticks), shutdown
    app.rs           App: the state of src/lib/store.tsx in Rust, plus focus, tabs, drafts, overlays
    keys.rs          the key map of section 5.9 and the kitty enhancement flags
    theme.rs         the twenty palettes, display mode, severity colors
    layout.rs        toolbar 3 rows, update notice, tabs, content, footer
    toolbar.rs       Toolbar (see gui.md Toolbar)
    tabs.rs          the tab strip and the close buttons
    footer.rs        the status bar
    snackbar.rs      the transient overlay
    help.rs          the key binding overlay
    messages/
      mod.rs         the split pane
      topic_tree.rs  filter, tree, badges
      message_view.rs bubbles, virtual rows, paging, day separators, focus row
      json_tree.rs   the collapsible tree used by data and raw JSON
      detail.rs      the per-node tree navigation of one message
      composer.rs    input, level, options, send
    settings/
      mod.rs         the category strip and panel
      appearance.rs broker.rs topics.rs notifications.rs history.rs update.rs advanced.rs
    about.rs
    update_notice.rs
    widgets/         text input, multi-line editor, select popup, checkbox, radio row, number field, editable table
    clipboard.rs     arboard, then OSC 52
    notify.rs        the Toaster: notify-rust, or tauri-winrt-notification on Windows
    open.rs          open URLs and the config directory with the `open` crate
```

The file split mirrors `src/components/` on purpose, as `gui.md`'s component table does, so that a feature has the same name in both applications.

New dependencies for `hmc`: `ratatui 0.30`, `crossterm 0.29` with `event-stream`, `arboard`, `open`, `notify-rust` (not Windows), `tauri-winrt-notification` and `windows-registry` (Windows), `portable-pty` (dev). Third party ratatui widgets (`tui-textarea`, `tui-tree-widget`) are used only if they support ratatui 0.30 at implementation time; otherwise the `widgets/` directory holds in-house ones. See section 10.

---

## 4. Config and setup string changes

### 4.1 Setup string

```json hiveme:broker-init
{
  "v": 1,
  "url": "mqtts://abc123.s1.eu.hivemq.cloud:8883",
  "username": "hiveme-sam",
  "password": "s3cret",
  "language": "en-US"
}
```

| Field | Config path | Notes |
|-------|-------------|-------|
| `language` | `gui.language` | Optional. `hmg` fills it from `gui.language`. `hmc --init` writes it on create and updates an existing `gui.language` that differs. Absent means `en-US`. An unsupported tag is kept as written and resolves to English, as `gui.language` already does. |

`BrokerInit::apply_to` sets `gui.language` when the field is present. The unchanged, updated, and created outcomes keep their meaning: a string whose language matches the file changes nothing.

### 4.2 Config

- No new keys. `gui.language`, `gui.theme`, `gui.displayMode`, and `gui.history` are now read by both `hmg` and interactive `hmc`. The struct doc comment and `config.md` say so; the block keeps its name because renaming it would break every existing file for a word.
- `gui.window` stays `hmg` only.
- `broker.sessionExpirySecs` now applies to interactive `hmc` as well as `hmg`; one-shot `hmc` still uses 0.

### 4.3 Schemas and generated types

`cargo xtask schema` regenerates `schemas/broker-init.schema.json`; `pnpm gen:types` regenerates `src/generated/`. The `json hiveme:broker-init` example in `config.md` gains the field and is validated by `cargo xtask check-spec`.

---

## 5. Terminal UI design

Spec file: `docs/specs/tui.md`. The layout and the behaviors are those of `gui.md`, rendered in cells. Where the terminal cannot do what the GUI does, this section says what replaces it.

### 5.1 Layout

Rows: toolbar (3), update notice (0 or 1), tabs (1), content (the rest), footer (1).

```
┌ HiveMe v0.1.0 ──────────────────────────────────────────────────────────────┐
│ [F2 Connect] [F3 Pause] [F4 Clear] [F10 Settings] [F1 About]          [? Help]│
└──────────────────────────────────────────────────────────────────────────────┘
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

- The toolbar is a bordered block whose title is `HiveMe v<version>`, the window title of `hmg`. The tools are labeled buttons showing their key; the active one (connected, paused, an open tab) is drawn in the primary color, as `activeButtonSx` does.
- The pane split starts at 28 percent, is clamped to 15..60, and moves with `Ctrl+Left`/`Ctrl+Right` or by dragging the divider with the mouse.
- Below 80 x 24 the whole frame is replaced by one centered line: `tui.tooSmall`.

### 5.2 Toolbar

| Tool | Key | Behavior |
|------|-----|----------|
| Connect / Disconnect | `F2` | As `gui.md`: Disconnect is offered while Connecting, Connected, or Reconnecting |
| Pause notifications | `F3` | Toggle in the session; drawn active while paused; the footer says `notifications paused` |
| Clear selected topic | `F4` | Deletes the stored history of the selected topic, invalidates cached ancestor views, reloads |
| Settings | `F10` | Opens or focuses the Settings tab |
| About | `F1` | Opens or focuses the About tab |
| Help | `?` outside a text field, `Ctrl+/` anywhere | The key binding overlay; not a GUI feature, added because a terminal has no tooltips |

### 5.3 Tabs

Tab 0 Messages is fixed; Settings and About open as closable tabs, in the order they were opened, with `✕`. `Alt+1`..`Alt+9` select, `Ctrl+W` closes the current closable tab, `Alt+Right`/`Alt+Left` cycle. `Ctrl+1`..`Ctrl+9` and `Ctrl+Tab`/`Ctrl+Shift+Tab` are also accepted when the terminal reports them (kitty protocol). The Messages tab stays mounted, so its scroll position and drafts survive a switch.

### 5.4 Topic tree

- A filter field on top; `/` focuses it from anywhere in the Messages tab, `Esc` leaves it. Matching is on the whole path, case-insensitive, keeps a parent whose child matches, and expands what it found. `hiveme` stays visible even when it does not match.
- The tree is drawn with `▾`/`▸` markers and two-cell indentation. `hiveme` is always present, selected, and highlighted at startup, even with an empty database. Every nonempty path is selectable; an empty leading segment is an italic group in the secondary color.
- `Up`/`Down` move, `Right` expands, `Left` collapses or moves to the parent, `Space` toggles, `Enter` selects (loads the subtree from the store and marks it read). Selection and expansion are independent, as in the GUI. A mouse click on the marker toggles, on the label selects.
- The unread badge is ` (N)` after the label in the primary color, rolled up from descendants, capped at `999+`. The selected label is bold in the primary color.
- The first tree a user sees is fully expanded until the user touches the expansion, as `TopicTree.tsx` does.

### 5.5 Message view

- The header line is the selected topic. Below it, the rows of the selected subtree, oldest at the top, newest at the bottom, auto-following new messages unless the user scrolled up. `PageUp` at the top asks the store for the previous page of 200 rows, the `before` cursor being the oldest row on screen. Only the rows in view are laid out; heights are cached per row and wrap width.
- A bubble is a rounded block at most 80 percent of the pane wide and at least 18 cells, aligned left for incoming and right for outgoing rows (`outgoing` on the row, which the backend sets from `sender.id == device.id`). Incoming bubbles are preceded by the sender name, falling back to the sender id, and no app name; outgoing and senderless rows have no header.
- Tiers render as in `gui.md`: envelope shows a bold title, the body, and a collapsed `data` tree; raw JSON is a tree; raw text is shown in the monospace style; bytes show `N bytes` and grouped hex; encrypted shows `🔒 encrypted (key k)` with a plain `[enc]` fallback where the glyph is unavailable; a newer `v` gets the `newer version` chip.
- Colors follow `payload.level`: info and debug use the regular fill (`Auto`: no fill, `Light`: black fill with white text for outgoing, `action.hover` gray for incoming; `Dark`: grey 800 for outgoing); success, warn, and error use the MUI palette values (`#2e7d32`, `#ed6c02`, `#d32f2f`) for the border and the badge, with a tinted fill only when a mode is forced, since a tint needs a known background. An unknown level renders as info while the badge shows the raw name with the fallback in parentheses.
- The metadata row below a bubble, aligned to its right edge, holds the relative topic path (secondary color, empty for a row on the selected topic itself), the level badge, `QoS n`, a `📌`/`[R]` retained marker, the newer-version chip, and the time. It is drawn only for the focused row and its one-row space is reserved for every row, which is the hover behavior of the GUI without a pointer.
- The day separator between rows on different days is a centered secondary-color line.
- With a row focused: `c` copies the body, `r` copies the raw payload, `Space` expands or collapses every tree in the bubble, `Enter` opens the detail view where `Up`/`Down` move between tree nodes, `Space` toggles one node, and `Esc` returns. The snackbar confirms a copy or reports why it failed.

### 5.6 Composer

- A bordered editor at the bottom of the message pane, three rows minimum, growing to six, with the placeholder of `composer.placeholder` or `composer.placeholderJson`. Below it, right-aligned: the Level select, More Options with `▸`/`▾`, Send.
- `Enter` sends from every composer control, `Alt+Enter` and `Ctrl+J` insert a newline (also `Shift+Enter` where the terminal reports it). Inside an open Level popup `Enter` picks the highlighted level without sending. `Tab`/`Shift+Tab` move between the editor and its controls.
- The Level select offers Info, Error, Success, Warn in that order, Info by default, colored as the GUI colors them, disabled in raw JSON mode while keeping its value.
- More Options shows Topic (relative to the selected tree topic, leading slashes stripped as typed), Title (disabled in raw JSON mode), and the QoS row with Config/0/1/2 radios, Retain Message, and As Raw JSON. Collapsing hides the controls and keeps the values.
- The draft, level, options, and expansion state are kept per selected tree topic in memory. Success clears only the originating topic's text; failure keeps it and goes to the snackbar. The composer is disabled while not connected or without a selection, with the reason shown as the placeholder.
- Sending calls `Session::publish`, the same path `hmg` and one-shot `hmc` use; the bubble appears once the broker acknowledged and the echo collapses into it by row id.

### 5.7 Settings tab

- A vertical category list on the left (Appearance, Broker, Topics, Notifications, History, Update, Advanced), a panel on the right, 96 columns at most, centered. Appearance opens first. `Up`/`Down` on the list change the category, `Tab` enters the panel, `Esc` returns to the list.
- Appearance: Mode as a three-way radio row (Auto Mode, Light Mode, Dark Mode), Theme and Language as select popups; a change applies immediately to the whole screen, including the catalog swap, and is saved automatically.
- Broker: Protocol select and the URL box with the same split and join as `src/lib/brokerUrl.ts`, ported into `hiveme_core::config::url` so both applications read a pasted scheme the same way; the line under the box says the URL that will be saved and the port; Username and Password with `Ctrl+H` to show or hide; the TLS note; Copy CLI setup, disabled until the broker fields validate, which flushes a pending save, then copies `hmc --init '<json>'` with the language and apostrophes escaped as the GUI escapes them; Connection and Reconnect groups with their number fields.
- Topics: the subscription rows, each a filter box and an Absolute checkbox, with Add and Remove.
- Notifications: the two switches and the rules table (Id, Topic filter, Level, Enabled, Title template, Body template, Remove) with Add.
- History, Update, Advanced: as `gui.md` lists them; Advanced is read-only text.
- Saving follows the store: 500 ms after the last edit, one writer at a time, the newest snapshot after a write in progress, silent on success, the snackbar on failure with the edits kept on screen. The session validates, writes, recompiles the rules, and reconnects only when broker or subscription fields changed.

### 5.8 About tab, footer, snackbar, update notice

- About: `HiveMe` in large text with the amber to orange gradient spread over the letters, the version chip, the tagline, Author and GitHub cards that open their URL on `Enter` or click, the table of device, config file, history database, and license, and the copyright line. There is no icon image; `ratatui::widgets::logo` is not used because the brand is HiveMe's.
- Footer: identical content to `Footer.tsx`, including the reconnect countdown driven by the tick timer, the notifications paused marker, and `config error` and `last error` entries that open the snackbar with the detail on `Enter` or click.
- Snackbar: a top-center overlay one line high, info or error colored, shown for four seconds or until a key, one at a time, driven by the same `notifyInfo`/`notifyError` calls the store has.
- Update notice: a line above the tabs, `HiveMe vX is available.`, `o` opens the releases page, `s` toggles Skip this version, `x`/click closes it and applies the skip through `Session::skip_version`.

### 5.9 Key map

| Scope | Key | Action |
|-------|-----|--------|
| Global | `Ctrl+C`, `Ctrl+Q` | Quit: end the broker session, restore the terminal, exit 0 |
| Global | `F1`, `F2`, `F3`, `F4`, `F10` | About, Connect/Disconnect, Pause, Clear topic, Settings |
| Global | `Alt+1`..`Alt+9`, `Ctrl+1`..`Ctrl+9` | Select tab |
| Global | `Alt+Left`/`Alt+Right`, `Ctrl+Shift+Tab`/`Ctrl+Tab` | Previous, next tab |
| Global | `Ctrl+W` | Close the current closable tab |
| Global | `Ctrl+/`, `?` outside text | Help overlay |
| Global | `Esc` | Close the topmost overlay, popup, or detail view; otherwise back to the pane list |
| Messages | `Tab`/`Shift+Tab` | Cycle focus: topic filter, tree, message list, composer |
| Messages | `Ctrl+Left`/`Ctrl+Right` | Move the pane divider |
| Tree | `Up`/`Down`, `Left`/`Right`, `Space`, `Enter`, `/` | Move, collapse/expand, toggle, select, focus the filter |
| Message list | `Up`/`Down`, `PageUp`/`PageDown`, `Home`/`End`, `c`, `r`, `Space`, `Enter` | Move focus, page, jump, copy body, copy raw, toggle trees, detail view |
| Composer | `Enter`, `Alt+Enter`/`Ctrl+J`/`Shift+Enter`, `Tab` | Send, newline, next control |
| Settings | `Up`/`Down`, `Tab`/`Shift+Tab`, `Enter`, `Space`, `Ctrl+H` | Category, field, open a select, toggle, show/hide password |
| Mouse | click, wheel, drag | Select tabs, tools, topics, rows, controls; scroll the focused list; move the divider |

Every text field takes the usual editing keys (`Left`/`Right`, `Home`/`End`, `Backspace`, `Delete`, `Ctrl+U`, `Ctrl+A`/`Ctrl+E`), and paste arrives through crossterm's bracketed paste when the terminal supports it.

### 5.10 Startup, shutdown, logging

1. Resolve the config path (`--config`, `HIVEME_CONFIG`, per-OS). Load or create it; a fresh file opens the Settings tab on Broker. A file that cannot be read is reported in the footer as `config error`, the TUI runs on defaults, and nothing is written until a setting is saved, as `hmg` does.
2. Open `HiveMe.db` beside it, with a 5 s busy timeout.
3. Enter the alternate screen, raw mode, mouse capture, bracketed paste, and the kitty keyboard flags when supported. Install the panic hook.
4. Select `hiveme`, load its subtree, start the prune loop, connect when a broker is configured, start the update check when due.
5. On quit, stop accepting work, end the broker session with the ten second bound `hmg` uses, restore the terminal, exit 0. A second `Ctrl+C` during shutdown exits at once with the terminal restored.
6. Logging: with `--verbose` or `RUST_LOG`, `log` output goes to `hmc.log` beside the config file at debug or the requested level; otherwise nothing is logged. stderr is never written while the TUI is up.
7. `--tui` on a stdout that is not a terminal is a usage error, exit 2.

---

## 6. Specification and documentation sync

| Document | Change |
|----------|--------|
| `docs/specs/tui.md` (new) | Everything in section 5, the component table of section 3.4, the languages section for `hmc`, the client identity, the startup and shutdown rules, the logging rule |
| `docs/specs/session.md` (new) | The shared session: its operations, events, types, config store semantics, notification evaluation, update check, the `Toaster` contract, the two-process rules of section 2.3 |
| `docs/specs/cli.md` | `hmc` "publishes one message and exits, or opens the terminal UI"; the `text hiveme:help` block with `--tui` and the interactive sentence; the interactive mode trigger; `--init` and `language`; languages of the printed lines; the client identifier for `Role::Tui`; "Out of scope for phase 1" rewritten |
| `docs/specs/config.md` | The setup string table and example; the `gui` block wording; `sessionExpirySecs` note |
| `docs/specs/gui.md` | The IPC section becomes a short table pointing at `session.md`; Copy CLI setup carries the language; the languages section notes the shared `locales/` directory |
| `docs/specs/hivemq-cloud.md` | The client identifier table gains `Role::Tui` |
| `docs/specs/app.md` | Applications overview, the spec table with two new files, repository layout (`locales/`, `crates/hmc/src/tui/`, `hiveme-core/src/session/`, `i18n/`), the spec-sync table rows for the new rules, the status table rows of each phase, a deviations entry per phase where the tree departs from this plan |
| `scripts/ts/check-spec-sync.ts` | New rules: `crates/hiveme-core/src/session/**` → `session.md`; `crates/hiveme-core/src/i18n/**` and `crates/hmc/src/tui/**` → `tui.md`; the existing `crates/hmc/src/` rule excludes `tui/` |
| `docs/release_notes.md` | One entry per user-visible change, per phase |
| `docs/development.md` | Running the TUI, its log file, the pseudo-terminal test, the shared `locales/` directory, `hmc` now linking SQLite |
| `docs/installation.md` | `hmc` opens an interactive UI; nothing else changes |
| `docs/screenshots.md` | A `tui.png` row and its recipe |
| `docs/todos.md` | The non-interactive tail option is superseded by interactive mode; a config file watcher is a new open item |
| `README.md` | The two-sentence description of `hmc`, a quick start step for the terminal UI |
| `CLAUDE.md` | Project overview line for `hmc`, the workspace table (`locales/`, `session`), the build order note, the pitfall about stderr in the TUI. `AGENTS.md` is untouched |

Mechanisms that stay in force: `cli_help_matches_spec` (en-US), `cargo xtask check-spec` for the tagged examples, the schema freshness test, `pnpm gen:types`, the vitest catalog test, and the new Rust catalog test.

---

## 7. Implementation phases

Every phase ends the same way, which is the definition of done of the initialization plan plus the final verification rule of `CLAUDE.md`:

1. Code, tests, and the documents of section 6 that the phase touches are updated in the same change.
2. `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo xtask schema`, `cargo xtask check-spec`, `pnpm typecheck`, `pnpm test`, the three Deno checks.
3. The status table in `app.md` gains or updates its rows, and `release_notes.md` gains a line for anything a user would notice.
4. `pnpm build`, `cargo build --release -p hmc`, then `cargo build --release -p hmg --features tauri/custom-protocol`, into the repository `target/release`, without touching runtime config or data files.

Phases are sequential. Phase 2 does not depend on phase 1 and can be built in parallel with it.

### Phase 0: Specification

**Goals.** The design is written down before code moves, so the specs stay the description of the code as built rather than a description of intent.

**Tasks.**
- Write `docs/specs/tui.md` and `docs/specs/session.md` from sections 3, 4, and 5, marking each section with the phase that builds it.
- Update `cli.md`, `config.md`, `gui.md`, `hivemq-cloud.md`, and `app.md` as section 6 lists, with the new status rows as `planned`.
- Add the new `check-spec-sync.ts` rules.
- Add this plan to the `app.md` and `todos.md` pointers.

**Tests.** `cargo xtask check-spec` passes with the updated `broker-init` example once the type carries the field (the example is added in phase 2; until then the block is unchanged and this phase records that in `config.md`). `deno task -c scripts/ts/deno.json check` passes.

**Documentation.** This phase is the documentation.

**Done when.** Every behavior in sections 3 to 5 has a home in a spec file, and the status table lists every phase of this plan.

### Phase 1: The shared session

**Goals.** `hmg` runs on `hiveme_core::session` with no change a user or the native end-to-end test can see, and `hmc` can link the same code.

**Tasks.**
- Add the `session` feature to `hiveme-core`, implying `storage`; move `ureq` and the update check under it.
- Create the module of section 3.1, moving code out of `src-tauri/src/{mqtt,controller,notification,config,update,protocol}.rs` and replacing `AppHandle` and `Emitter` with `broadcast::Sender<SessionEvent>` and the `Toaster` trait. Keep every log line and every error message.
- Add `Role::Tui` to `hiveme_core::mqtt::options` and `SessionApp` to choose the role and `sender.app`.
- Add the SQLite busy timeout to `Store::open`.
- Reduce `src-tauri` to the adapters of section 3.2. The `#[tauri::command]` list, `protocol.ts`, and `service.ts` are untouched.
- Move the unit tests with the code they test.

**Tests.**
- Existing `src-tauri` unit tests pass where they remain; those that moved pass in `hiveme-core`.
- New `crates/hiveme-core/tests/session.rs` against the Docker broker: connect and subscribe emit `Status`; a published envelope is stored, emitted once, and its echo is recognized; a raw JSON publish carries no `hiveme-v`; `set_config` with a theme change keeps the connection and with a password change replaces it; the pause toggle stops the toaster from being called; `shutdown` ends the session within the bound. A scripted `Toaster` records calls.
- `pnpm test:e2e --release` passes unchanged on Linux CI, which is the proof that `hmg` behaves as before.

**Documentation.** `session.md` matches the module; `gui.md` IPC points at it; `app.md` status rows for the session and the adapters; `development.md` notes that `hmc` will link SQLite from phase 3. No release note, because nothing visible changed.

**Done when.** Both release builds succeed, the end-to-end test is green, and `src-tauri/src` contains no MQTT, rule, or history logic.

### Phase 2: Setup string language and shared catalogs

**Goals.** `hmc --init` learns the language, the catalogs are one set both applications read, and every line one-shot `hmc` prints, including `--help`, is in the config language.

**Tasks.**
- `BrokerInit.language: Option<String>`, filled by `BrokerInit::from_config`, applied by `apply_to`; regenerate the schema and the TypeScript types; `hmg`'s `get_broker_init` needs no change beyond the shared type.
- Move `src/i18n/locales/*.json` to `locales/`; update `src/i18n/index.ts`, `src/i18n/index.test.ts`, and `tsconfig.json` includes if needed.
- Build `hiveme_core::i18n` of section 3.3 with the `cli.*` and `help.*` keys in all nine catalogs.
- `hmc`: pick the language as section 3.3 says; translate `run.rs` outcomes, `cli.rs` body errors, `Failure` details `hmc` authors, and the clap help. Keep `hmc: <category>:` fixed.

**Tests.**
- Rust: every locale covers every `en-US` key, interpolation variables match, plural categories exist per locale, and the formatted outputs asserted in `src/i18n/index.test.ts` (`1.000` in `de`, `1 000 000 octets` in `fr`, `1 バイト` in `ja`) come out the same from Rust.
- `--init` round trips the language: create writes `gui.language`, update changes only a differing language, a matching one is `Unchanged`, absent means untouched on update and `en-US` on create.
- `assert_cmd`: with a config whose `gui.language` is `de`, `hmc --help` and the publish confirmation are German; without a config they are English; the stderr prefix is still `hmc: usage:`.
- `cli_help_matches_spec` still compares `en-US`. vitest passes after the move.

**Documentation.** `config.md` (setup string, example), `cli.md` (init, languages), `gui.md` (Copy CLI setup, `locales/`), `tui.md` languages section, `app.md` layout and status, `development.md`, release notes.

**Done when.** Copy CLI setup in `hmg` produces a string with `language`, `hmc --init` applies it, and `hmc --help` renders in each of the nine languages.

### Phase 3: The TUI shell

**Goals.** `hmc` with no arguments on a terminal opens the frame of section 5.1: toolbar, tabs, empty Messages tab, footer, snackbar, help overlay, theme, key map, OS notifications, update notice, connection at startup, graceful quit.

**Tasks.**
- Mode selection in `main.rs` and `--tui` in `cli.rs` (conflicts with every publish and init option); the multi-thread runtime for the TUI; the panic hook; terminal setup and restore; the log file rule.
- `tui/app.rs` holding the state of `store.tsx` and consuming `SessionEvent`; the event loop selecting the crossterm stream, the session channel, and a 250 ms tick.
- `theme.rs` with the twenty palettes and the display modes; `toolbar.rs`, `tabs.rs`, `footer.rs`, `snackbar.rs`, `help.rs`, `update_notice.rs`; `keys.rs` with the kitty flags; `notify.rs` (the `Toaster`); `open.rs`; `clipboard.rs`.
- First-run behavior of decision 15, using a Settings tab that for this phase holds only the category strip and the Broker panel's URL, username, and password fields, so a fresh install can be completed without leaving the TUI. The rest of Settings is phase 5.
- The Messages tab shows `messages.selectTopic` or `messages.empty` and nothing else yet.

**Tests.**
- `TestBackend` renders the frame at 80 x 24 and 120 x 40 in each connection state and in `en-US`, `de`, `ja`, and `zh-CN`, asserting the toolbar rows, the tab strip, and the footer text; a 79 x 24 render shows the too-small line.
- Key map unit tests for every row of section 5.9, including the kitty and fallback variants.
- `assert_cmd`: no arguments with piped stdin publishes; `--tui` with a redirected stdout exits 2 with a usage line; `--tui --init '{}'` conflicts.
- A scripted session drives status changes, a notification event, and an update result through the app and the tests assert the footer, the toaster call, and the notice line.

**Documentation.** `tui.md` layout, toolbar, tabs, footer, snackbar, keys, startup and shutdown, logging; `cli.md` help block and trigger rule; `app.md` status; `development.md` running the TUI; release notes.

**Done when.** On all three platforms a user can run `hmc`, see the frame connect to their cluster, receive an OS notification for an `error` published by another terminal, pause it, and quit with the terminal restored.

### Phase 4: The Messages tab

**Goals.** The topic tree, the message view, and the composer behave as sections 5.4 to 5.6 and `gui.md` describe.

**Tasks.**
- `topic_tree.rs`: filter, expansion, selection, badges, the always-present `hiveme`, mark read and subtree load through the session, live updates from `TopicAdded` and `Message` events.
- `message_view.rs`, `json_tree.rs`, `detail.rs`: virtual rows with a height cache, tiers, colors, headers, the focus metadata row, day separators, paging, auto-follow, copy actions, encrypted and newer-version rendering, hex for bytes.
- `composer.rs` and the editor widget: the drafts map, Enter semantics, the level popup, More Options, publish through the session, success and failure paths.
- Clear selected topic with the ancestor cache invalidation of `clearSelectedTopic`.
- Mouse handling for the tree, the rows, the divider, and the composer controls.

**Tests.**
- `TestBackend` renders each fixture in `crates/hiveme-core/tests/fixtures/message/` as a bubble in both directions and in both forced modes, asserting the same facts `MessageView.test.tsx` asserts: title bold, tree collapsed, relative path, level badge, encrypted placeholder, newer-version chip, hex for bytes.
- Tree tests mirror `TopicTree.test.tsx`: filter keeps parents, `hiveme` survives a non-matching filter, labels select without expanding, badges roll up.
- Composer tests mirror `Composer.test.tsx`: Enter sends from every control, newline keys, per-topic drafts including after a tab switch and a reconnect, raw JSON disables Level and Title, success clears only the originating topic.
- Paging with an in-memory store of 450 rows on three topics: the first page is the newest 200, `PageUp` at the top loads the previous, `hasOlder` turns false at the end.
- Integration against the Docker broker: a message sent from the composer is acknowledged, stored once, and shown once after its echo; a message published by one-shot `hmc` appears live and raises the badge until selected.

**Documentation.** `tui.md` sections 5.4 to 5.6; `app.md` status; release notes.

**Done when.** The manual checklist of `gui.md` Notifications passes with interactive `hmc` in place of `hmg`, and `hmc -t error "Disk full"` from another terminal appears live in the TUI.

### Phase 5: Settings and About

**Goals.** All seven settings categories are editable with automatic saving; the About tab is complete; Copy CLI setup works from the terminal; appearance changes apply immediately.

**Tasks.**
- The form widgets of section 3.4: text, password, select popup, checkbox, radio row, number field, editable table.
- `settings/*.rs` for every category of section 5.7; the debounced single-writer save; the reconnect path through the session; Copy CLI setup with the language and the apostrophe escaping.
- Port `splitBrokerUrl`, `joinBrokerUrl`, and `effectivePort` into `hiveme_core::config::url` so the Broker panel and the GUI agree; the frontend keeps `brokerUrl.ts` because it is display code, and a shared fixture proves both ports agree.
- `about.rs` and the URL opening.
- Replace the phase 3 placeholder Settings tab.

**Tests.**
- `TestBackend` renders each category; form-to-config mapping tests mirror `Config.test.tsx`, including subscription rows written back as strings or `{filter, absolute}` objects, rule add and remove, and number fields that leave the draft alone until the text is a number.
- Save timing with a fake clock: an edit, a second edit within 500 ms, one write; an edit during a write, a second write with the newest snapshot; a rejected config keeps the edits and shows the snackbar.
- A language change re-renders every label of the visible screen in the new language within the same frame; a theme change recolors the selected topic.
- The broker URL twin: a table of console strings (`host`, `host:8883`, `host:8884/mqtt`, `mqtts://host`) gives the same split and join in Rust and TypeScript.

**Documentation.** `tui.md` settings and about sections; `config.md` wording; `gui.md` note on the shared URL helper; `app.md` status; release notes.

**Done when.** A fresh install can be configured entirely inside interactive `hmc`, the connection comes up after the automatic save, and Copy CLI setup from the TUI yields a string that `hmc --init` on another machine applies, language included.

### Phase 6: End to end, hardening, and onboarding

**Goals.** The TUI is exercised as a real process against a real broker on CI, behaves under load and resize, and the documentation lets a user go from nothing to a terminal session.

**Tasks.**
- `crates/hmc/tests/tui.rs` with `portable-pty`: start the Docker broker as `publish.rs` does, spawn `hmc --tui --config <scratch>` in a pseudo-terminal, wait for the frame, publish with one-shot `hmc`, assert the bubble text in the screen capture, type a message and `Enter`, assert the acknowledgement bubble and the row in `HiveMe.db`, press `F10`, change the language, assert the German toolbar, press `Ctrl+Q`, assert the terminal is restored and the broker session ended. Gated like `publish.rs`; the Linux workflow runs it, the others set `HIVEME_SKIP_DOCKER`.
- Performance: 10,000 rows across 200 topics render at 60 x 200 without a visible stall (a benchmark test with a budget); resize during a render; the too-small path.
- Windows: verify Windows Terminal and conhost, the ConPTY run of the test, the toast label; macOS: verify the toast appears from an unbundled binary and record its label in `tui.md`; Linux: the notification daemon absent case is silent, not fatal.
- Two-process check: run `hmg` and interactive `hmc` on one config and one database, publish from a third terminal, confirm one row and one notification each, and record the raw JSON duplication in `session.md`.
- The documents of section 6 that are not yet touched: README, `installation.md`, `screenshots.md`, `todos.md`, `CLAUDE.md`.

**Tests.** The pseudo-terminal test above; the benchmark test; the existing suites.

**Documentation.** Everything in section 6 is current; the status table marks every row of this plan `done`; `app.md` deviations record whatever the tree ended up doing differently from this plan.

**Done when.** The three workflows are green with the new test in the Linux one, and a reader of the README can reach a live terminal session without opening `hmg`.

---

## 8. Testing and CI

| Layer | Tool | Where |
|-------|------|-------|
| Session | `cargo test -p hiveme-core --test session` with the Docker broker | Linux workflow, local with Docker |
| i18n | `cargo test -p hiveme-core` catalog and format tests, vitest catalog test | all three workflows |
| TUI rendering and keys | `TestBackend` tests in `crates/hmc` | all three workflows |
| CLI behavior | `assert_cmd` in `crates/hmc/tests/cli.rs` | all three workflows |
| TUI end to end | `crates/hmc/tests/tui.rs` with `portable-pty` and the Docker broker | Linux workflow, local with Docker |
| GUI end to end | `scripts/ts/test-e2e.ts`, unchanged | Linux workflow |
| Spec sync | `cli_help_matches_spec`, `cargo xtask check-spec`, `check-spec-sync.ts`, schema and generated type freshness | all three workflows |

The workflows need no new steps: `cargo test -r --workspace` picks up the new tests, and `cargo build -r -p hmc` already precedes anything that compiles `hmg`.

---

## 9. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Terminals that do not report `Shift+Enter`, `Ctrl+digit`, or `Ctrl+Tab` | Every action has a plain-terminal binding; the kitty flags are requested and ignored where unsupported; the help overlay shows what applies |
| `tui-textarea` or `tui-tree-widget` lag ratatui 0.30 | The `widgets/` directory holds in-house editor and tree widgets if the crates do not build; decided in phase 3 and recorded in `app.md` deviations |
| Two processes on one SQLite file | WAL plus a 5 s busy timeout; the raw-payload duplication is documented; unread counts are proven right by a two-process test in phase 6 |
| A setting changed in one application is stale in the other | Documented; a file watcher is an open item in `todos.md` |
| `ureq` brings `ring` into `hmc` beside `aws-lc-rs` | `mqtt::tls` already installs the provider explicitly once per process; the session test runs in `hmc` too |
| macOS toasts from an unbundled binary | `notify-rust` shows them under the terminal application's identity; phase 6 verifies and `tui.md` says so |
| Windows toast label | `hmc` registers the same `AppUserModelId` `hmg` registers, so the label reads HiveMe |
| Truecolor unavailable in a legacy console | crossterm maps `Color::Rgb` to the nearest ANSI color; the design never relies on a tint to convey meaning, the badge text carries the level |
| CJK width and layout | ratatui's width-aware truncation; the `TestBackend` tests render `ja` and `zh-CN` |
| Translated clap help drifting from the spec | `cli_help_matches_spec` compares `en-US`; the catalog test proves every other language has every `help.*` key |
| Large histories | Only visible rows are laid out; heights are cached; the phase 6 benchmark holds a budget |
| `hmc` binary size grows with SQLite and ratatui | Accepted; `installation.md` mentions it |

---

## 10. Open items to confirm during implementation

- Whether `tui-textarea` and `tui-tree-widget` support ratatui 0.30 at the time of phase 3; if not, the in-house widgets stay.
- Whether the kitty keyboard protocol is worth enabling on Windows Terminal by default, or only when detected.
- Whether the two applications should watch the config file for changes made by the other; if so, it belongs to the session and lands in a later plan.
- Whether a non-interactive tail option, which the initialization plan left to its phase 6, is still wanted now that interactive mode tails every subscription. It would be an option, never a subcommand.
- The exact glyph fallbacks (`🔒`, `📌`, `▾`, `✕`) on terminals without those code points; phase 3 picks ASCII fallbacks behind one table.
