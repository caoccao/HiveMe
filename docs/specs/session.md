# Session (the shared backend)

`hiveme_core::session` is the backend both applications run on: the broker
connection, the history, the notification rules, the config in memory, and the update
check, behind one Rust API with an event stream. `hmg` wraps it in Tauri commands and
events; interactive `hmc` drives it from the terminal UI. Neither application holds
logic of its own between the broker and the screen.

**Status: built in phase 1** of [the terminal UI plan](../plans/plan-terminal-ui.md),
which moved what used to be `src-tauri/src/{mqtt,controller,notification,config,update,protocol}.rs`
into this module without changing what `hmg` does. `hmg` runs on it, and so does the
terminal UI of `hmc`, since phase 3. [gui.md](gui.md#ipc) stays the IPC
contract of `hmg` and points here for what each command does.

## Why

`hmc` and `hmg` share a config file and a message format so that a message from
either is indistinguishable at the other end. Once `hmc` also subscribes, stores,
notifies, and reconnects, the only way to keep the two from drifting is one
implementation. The Tauri handle and the event emitter were the one thing that kept
the `hmg` orchestration out of `hiveme-core`; a channel replaces them.

## Module layout

*Phase 1.*

Feature `session` of `hiveme-core`, which implies `storage` and brings `ureq`. `hmg`
turns it on, and so does `hmc` for its terminal UI, which links SQLite and `ureq` into
the one binary that also publishes.

```
crates/hiveme-core/src/session/
  mod.rs        Session and SessionApp: open, connect, disconnect, publish, set_config, status, shutdown
  config.rs     ConfigStore: the config in memory, the load error, save, save quietly, needs_reconnect
  mqtt.rs       the connection lifecycle and the pump that stores, emits, and notifies
  notify.rs     Notifier: the rule engine, the rate limiter, the pause toggle, the Toaster trait
  history.rs    the topic tree, paging, mark read, clear, the prune loop
  update.rs     the GitHub release check, its interval, skip version
  types.rs      About, Status, TopicNode, MessageRow, PublishOptions, UpdateCheckResult, SessionEvent
```

`ureq`, which the update check needs, lives in `hiveme-core` under the feature. The
rustls provider is still installed once per process by `mqtt::tls`, as
[hivemq-cloud.md](hivemq-cloud.md#transport-and-tls) explains; `hmc` needs that too
once it turns the feature on, because it then links `ureq`'s `ring` beside
`rumqttc`'s `aws-lc-rs`.

## Construction and lifetime

*Phase 1.*

```
Session::open(config_path: Option<&Path>, app: SessionApp, toaster: Arc<dyn Toaster>) -> Result<Arc<Session>>
Session::with_parts(app: SessionApp, config: Arc<ConfigStore>, store: Arc<Store>, toaster: Arc<dyn Toaster>) -> Arc<Session>
Session::subscribe(&self) -> broadcast::Receiver<SessionEvent>
Session::start_background_work(self: &Arc<Self>)
Session::begin_shutdown(&self)
Session::shutdown(&self) -> Result<()>
SHUTDOWN_TIMEOUT = 10 s
```

- `open` resolves the config path as [config.md](config.md#location-and-precedence)
  does, loads or creates the file, opens `HiveMe.db` beside it, and compiles the
  rules. A config that cannot be read is kept as a load error, the session runs on
  defaults, and nothing is written until `set_config` is called, which is the user
  saying to. A history that cannot be opened is the one failure that stops startup,
  as it is in `hmg` today. The failure is logged with the database path before it is
  returned.
- `with_parts` builds a session on a config store and a history the caller already
  has, which is how the tests run a session on an in-memory store.
- `SessionApp` is `Gui` or `Tui`. It selects the MQTT role (`Role::Gui` or
  `Role::Tui`, see [Roles](#roles-and-client-identity)) and the `sender.app` value of
  every message the session publishes (`hmg` or `hmc`).
- The session owns no runtime. Its `async` operations run on the caller's Tokio
  runtime, and `start_background_work` spawns onto the runtime it is called from.
- `start_background_work` starts the prune loop, the first connection when a broker is
  configured, and the update check when it is due. Each application calls it once its
  screen exists. The prune loop holds the session weakly and stops once it is gone.
- `begin_shutdown` refuses new connections and publishes and interrupts startup and
  subscription waits. `shutdown` closes the MQTT connection and discards the broker
  session as [hivemq-cloud.md](hivemq-cloud.md#disconnect-and-quit) describes. Both
  applications bound the whole quit with `SHUTDOWN_TIMEOUT`, ten seconds, and exit even
  when the broker is unreachable.
- Every way an application is left runs `begin_shutdown` and `shutdown`: closing the
  `hmg` window or quitting it from the system menu, and every quit path of the terminal
  UI in [tui.md](tui.md#leaving-the-terminal-ui), including closing the terminal it
  runs in. Only a process that is killed outright, or that crashes, skips them; the
  broker then keeps the session until `broker.sessionExpirySecs` runs out, as it does
  after a network loss.

## Operations

*Phase 1.*

The operations are the commands of [gui.md](gui.md#commands) with the same names,
inputs, and answers, minus the Tauri wrapping.

| Operation | Input | Answer |
|-----------|-------|--------|
| `about` | none | `About` |
| `config`, `config_path`, `database_path`, `load_error` | none | the config as held, the two paths, why the file could not be read |
| `set_config` | `Config` | the config as saved. Validates, writes, recompiles the rules, and reconnects only when something the CONNECT packet or the subscription list is built from changed |
| `set_config_quietly` | `Config` | none. A change the user did not ask for, such as window geometry or the update check's bookkeeping; silently skipped while the file on disk is unreadable |
| `broker_init` | none | the setup string for `hmc --init`, carrying `gui.language`. Fails while the broker fields are not usable |
| `status` | none | `Status` |
| `connect` | none | `Status`. Replaces a connection that is already up |
| `disconnect` | none | none |
| `topic_tree` | none | `TopicNode[]`, built by splitting every stored topic on `/`, with unread and message counts rolled up |
| `messages` | topic, `before`, `limit` | `MessageRow[]` for the topic and all descendants, oldest first; no cursor means the newest page, no limit means 200 |
| `mark_read` | topic | none; clears unread counts throughout the subtree |
| `clear_topic` | topic | how many rows were deleted; only rows stored directly on that topic |
| `publish` | topic, body, `PublishOptions` | the stored `MessageRow`; see [Publishing](#publishing) |
| `set_notifications_paused` | bool | `Status` |
| `update_result` | none | `UpdateCheckResult`, or nothing while the check is still running |
| `skip_version` | version | none; writes `update.ignoreVersion` quietly |

Errors are `hiveme_core::Error`, extended with the variants the session needs
(`NotConnected`, `Quitting`, `NothingToSend`, `NotJson`, and `InvalidTopic` for a
publish topic the MQTT rules refuse), rather than `anyhow`,
because `hmc` maps errors to exit codes through its `Failure::from` and `hmg` turns
them into the one line the snackbar shows.

## Events

*Phase 1.*

`SessionEvent` on a `tokio::sync::broadcast` channel holding 1,024 events. `hmg`
forwards each one to the frontend under the event names of [gui.md](gui.md#events);
the terminal UI applies each one to its state.

| Event | Payload | When |
|-------|---------|------|
| `Status` | `Status` | every connection state change, every reconnect countdown, every pause toggle, and the end of a `connect` or `disconnect` |
| `Message` | `MessageRow` | once per stored row, whether it arrived or was published here. The echo of a message this installation published raises no second event, because it collapses into the row that is already there |
| `TopicAdded` | topic | the first message ever stored on a topic |
| `NotificationFired` | rule id, message id, topic | a rule raised an OS notification |

A receiver that falls behind loses the oldest events, which is the broadcast
channel's rule; the applications refresh the tree and the status from the operations
after such a lag, as the GUI store already does after every message.

## Types

*Phase 1.*

The types keep the shape and the camelCase serialization of
[gui.md](gui.md#types), so that `hmg` passes them to the frontend unchanged and
`protocol.ts` does not move. `src-tauri/src/protocol.rs` re-exports them and keeps
only what is the GUI's own: the event names, the `topic-added` and
`notification-fired` payload structs, and the managed state. `check-spec-sync.ts`
pairs `session/types.rs` as well as `protocol.rs` with `protocol.ts`.

```
About          = { appVersion, configPath, databasePath, deviceId, deviceName, githubUrl }
Status         = { state, host, port, clientId, subscriptions, attempt, retryInMs,
                   lastError, messagesReceived, databaseBytes, notificationsPaused, configError }
TopicNode      = { id, label, topic, unread, messages, children: TopicNode[] }
MessageRow     = { rowId, topic, id, ts, receivedTs, senderId, senderName, app,
                   tier, level, title, body, raw, rawLength, qos, retain, outgoing }
PublishOptions = { topic?, json?, qos?, retain?, title?, level? }
UpdateCheckResult = { hasUpdate, latestVersion }
SessionEvent   = Status(Status) | Message(MessageRow) | TopicAdded { topic }
               | NotificationFired { rule_id, message_id, topic }
```

Their semantics are the ones [gui.md](gui.md#types) lists: `Status.state` is one of
the four names `hiveme_core::State::as_str` spells; `TopicNode.topic` is set on every
nonempty path; `MessageRow.raw` is text or hex; `PublishOptions.topic` is relative to
the selected topic.

## Config store

*Phase 1.*

`ConfigStore`: one copy of the config in memory behind a lock, so that an operation,
the pump, and a window handler all see the same thing.

- Loading never validates. `set_config` validates first and reports every problem at
  once; it is the one path that overwrites a file that could not be read.
- Writes go through `ConfigFile::save`, which merges into the document that was read
  so unknown keys survive.
- `needs_reconnect` is true when `broker` or `topics.subscriptions` differ. Changing a
  theme keeps the connection; changing a password drops it.
- Both applications keep the config in memory and neither watches the file, so a
  setting changed in one is seen by the other at its next start. See
  [Two processes, one installation](#two-processes-one-installation).

## Publishing

*Phase 1.*

The path one-shot `hmc`, the `hmg` composer, and the terminal UI composer all take,
so that a message from any of them is indistinguishable.

1. Resolve the topic: the given base joined with the relative input, leading slashes
   stripped, as [config.md](config.md#topic-resolution) says. Validate it.
2. Refuse while `encryption.mode` is not `Off`, with the message
   [message.md](message.md#encryption) prescribes.
3. Refuse an empty body. With `json`, refuse input that is not JSON and publish the
   bytes unchanged with content type `application/json` and no `hiveme-v` property.
   Otherwise build the envelope: `v`, a UUID v7 `id`, an RFC 3339 `ts`, `sender` from
   `device` with the application's `app`, the body, the trimmed title when there is
   one, and the level, `info` by default.
4. QoS is the option or `publish.qos`; retain is the option or `publish.retain`.
5. Publish through `MqttClient`, which returns once the broker has acknowledged.
6. Store the row, flagged outgoing, only then. A publish that failed leaves no row and
   no bubble. Emit `TopicAdded` when the topic is new and `Message` for the row.

## Receiving

*Phase 1.*

Every message the broker delivers is counted, parsed with the lenient reader of
[message.md](message.md#parse-tiers), and stored. A new row raises `Message` and is
offered to the notifier. A row that was already there raises nothing when this session
put it there, as the echo of its own publish or a second delivery, or when the message
is a retained copy; one that another process on the same database stored a moment
earlier is raised and offered to the notifier as new, see
[Two processes, one installation](#two-processes-one-installation). `TopicAdded` is
raised for a topic the store has not seen. Inserts run on the runtime in arrival order,
which is the order the chat views show.

## Notifications

*Phase 1 for the notifier, phase 3 for the `hmc` toaster.*

The evaluation of [gui.md](gui.md#notifications) is the session's: the compiled rules,
first match in config order on topic filter and payload level, the `notifyOwnMessages`
rule, the one per rule per second limiter with its `and N more messages` summary, and
the pause toggle that lasts for the process and is never written to the config.
Reloading the rules after a save resets the limiter; resuming after a pause does too.

What the session cannot do is show the toast, because that is a platform call each
application makes differently. It asks the `Toaster` given to `Session::open`:

```rust
pub trait Toaster: Send + Sync {
  fn show(&self, title: &str, body: &str) -> Result<(), String>;
}
```

| Application | Toaster |
|-------------|---------|
| `hmg` | `TauriToaster` in `src-tauri/src/notification.rs`: the Tauri notification plugin on Linux and macOS, `tauri-winrt-notification` with the registered `HiveMe` identity on Windows. The session exists before the Tauri application, so the plugin's handle is attached in the window setup, before the first connection starts. |
| `hmc` | `notify-rust` on Linux and macOS; `tauri-winrt-notification` with the same identity on Windows. See [tui.md](tui.md#notifications). |

A show that fails is logged and dropped: the message is stored and shown either way,
and `NotificationFired` is raised only for a toast that was shown.

## History

*Phase 1.*

The store of [gui.md](gui.md#storage) is unchanged. The session adds what used to be
in `controller.rs`: the tree built by splitting every stored topic on `/` with counts
rolled up into every parent, paging across a subtree with the row id as cursor,
marking a subtree read, clearing one topic, and the prune loop that runs at startup and
every ten minutes with `gui.history`.

## Update check

*Phase 1.*

The check `src-tauri/src/update.rs` used to hold, unchanged: due when `update.lastChecked` is
older than `update.checkInterval`; runs on its own thread against the GitHub releases
of the repository; writes `lastChecked` and `lastVersion` quietly; a version equal to
`ignoreVersion` is not reported; when not due, the last result still counts so the
notice survives a restart. Nothing is downloaded or installed.

## Roles and client identity

*Phase 1.*

| `SessionApp` | `Role` | Client id | Session |
|--------------|--------|-----------|---------|
| `Gui` | `Role::Gui` | `<prefix>-hmg-<8 of device.id>` | persistent across network interruptions, ended on quit |
| `Tui` | `Role::Tui` | `<prefix>-hmc-<8 of device.id>-<8 random>` | the same behavior; the suffix keeps several interactive `hmc` processes apart |

One-shot `hmc` does not use the session; it keeps `Role::Cli`. The full table is in
[hivemq-cloud.md](hivemq-cloud.md#role).

## Two processes, one installation

*Phase 1 for the rules, phase 6 for the verification.*

`hmg` and interactive `hmc` may run at once on the same config and the same
`HiveMe.db`.

- SQLite runs in WAL mode and the store sets a five second busy timeout, so a write
  that meets the other process's write waits instead of failing. A message is stored in
  one immediate transaction, the lookup and the insert together, so the two processes
  never both find a message missing and both insert it.
- An envelope received by both is one row: whichever stores it second finds the row
  and does not count it unread again. Both still show it live and offer it to the rules.
  The database cannot say who put a row there, so each session remembers the last 10,000
  messages it stored or sent itself: a row it did not put there was stored a moment ago
  by the other process and is raised as new, while the echo of its own publish, a second
  delivery, and a retained copy of a stored message raise nothing. So an envelope from
  another device is one row, counted unread once, one `Message` event in each process,
  and one notification in each.
- A payload with no id of its own (raw JSON, text, bytes) has an id generated on insert,
  so one received by both becomes two rows, each counted unread, and each process raises
  its own. This is documented, not prevented; only an envelope can claim to be the same
  message.
- Both processes prune; the second pass finds nothing.
- Both hold the config in memory and write it atomically. The last writer wins, and a
  change made in one is seen by the other at its next start. A file watcher is an open
  item in [todos.md](../todos.md).
- The client identifiers differ by construction, so the broker never disconnects one
  for the other.
- Verified in phase 6 by `hmg_and_the_terminal_ui_share_one_installation` in
  `crates/hmc/tests/tui.rs`: interactive `hmc` in a pseudo-terminal and `hmg`'s session,
  `SessionApp::Gui` in the test process, on one config and one database, with one-shot
  `hmc` publishing as another device. An envelope is one row, unread once, and one
  notification in each; a raw JSON payload is two rows, and the unread count is three.
  The check found that the second process took the first one's row for its own echo and
  showed and notified nothing, which is what the remembered messages and the immediate
  transaction above fixed.

## What each application adds

*Phase 1.*

| Concern | `hmg` (`src-tauri`) | `hmc` (`crates/hmc/src/tui`) |
|---------|---------------------|------------------------------|
| Entry | `lib.rs`: one `#[tauri::command]` per operation, alphabetized, `convert_error`, each calling one line of `controller.rs` | `mod.rs`: the event loop; `service.rs`: the `Service` trait `Session` implements, so the screens can be tested against a scripted session |
| Events | `events.rs`: a task forwarding `SessionEvent` to `app.emit` under the names of [gui.md](gui.md#events); a lag is logged and skipped | applied to the application state in the same `select!` as the keys; a lag is logged and the status read again |
| Toaster | `notification.rs` | `notify.rs` |
| Opening things | the opener plugin for URLs and the config file | the `open` crate |
| Update check | `MainContent.tsx` polls `get_update_result` every second until it has an answer | the tick polls `update_result` until it has an answer |
| Window or screen | `window.rs`: geometry in `gui.window`, the quit path, and the runtime `start_background_work` spawns onto | terminal setup and restore, the quit paths of [tui.md](tui.md#leaving-the-terminal-ui) |
| Logging | stderr through `env_logger` | `hmc.log` beside the config, see [tui.md](tui.md#logging) |

## Tests

*Phase 1.*

`crates/hiveme-core/tests/session.rs`, against the Docker broker of
`crates/hiveme-core/tests/mqtt.rs` and skipped the same way without Docker: connect
and subscribe emit `Status`; a published envelope is stored, emitted once, and its
echo is recognized; a raw JSON publish carries no `hiveme-v`; `set_config` with a
theme change keeps the connection and with a password change replaces it; the pause
toggle stops the toaster from being called while the message is still stored and
raised; `shutdown` ends the session within its bound, the broker keeps no session for
the client identifier, and later work is refused. A scripted `Toaster` records the
calls. The unit tests of the moved code moved with it, and `tests/storage.rs` proves
that two handles on one database store an envelope once and count it unread once, and
that two threads storing the same two hundred envelopes through two handles at once
store each one once, new to exactly one of them. Built in phase 6: two sessions, `Gui`
and `Tui`, on one config and database each raise a message from another device once and
show its notification once, and a message one of them sends is raised once by each, by
the other as this device's message.
