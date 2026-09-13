# TODOs

Tracked work lives in [the initialization plan](plans/plan-initialization.md), whose
phases 0 to 5 are done, and in [the terminal UI plan](plans/plan-terminal-ui.md),
which builds the interactive mode of `hmc` and the shared session. This page collects
what the plans deliberately left open.

## Open items from the terminal UI plan

* Whether the in-house text controls and topic tree should give way to `tui-textarea`
  and `tui-tree-widget` once those support ratatui 0.30. Neither did when phases 3 and 4
  were built.
* Whether the two applications should watch the config file for changes the other
  made. Today each keeps the config in memory and sees the other's change at its next
  start. If a watcher is wanted, it belongs to the session and lands in a later plan.
* Whether a non-interactive tail of a topic filter, an option the initialization plan
  left to its phase 6, is still wanted now that interactive mode tails every
  configured subscription. It would be an option, never a subcommand: a bare word is
  a message.
* Whether the block letters of the About tab and the scrollbar of a settings panel need
  ASCII fallbacks on the Linux console and a Windows console outside Windows Terminal,
  whose fonts phase 5 assumed draw them.

## Open questions from the plan

* Whether the `wss://` transport ships with the MQTT client in phase 2 or waits for
  phase 6 depends on the quality of `rumqttc`'s WebSocket support with rustls.
* The `$id` URLs in the generated schemas are placeholders until a documentation site
  exists.
* Whether `pnpm tauri build` inside a Cargo workspace needs extra configuration for
  the Windows portable archive.

## Deferred to phase 6

* Message encryption: `A256GCM`, an option that generates a key, and the
  `Opportunistic` and `Required` modes.
* The HiveMQ Cloud REST client and the GUI Cluster tab, which need the Starter plan.
* OS keychain storage for the broker password, the encryption secrets, and the REST
  token.
* Named broker profiles.
* Payload based notification rule matching.
* Options that show the config, print its path, and store the password in the OS
  keychain. `hmc` has no subcommands, because `hmc config` is the message `config`;
  the non-interactive tail is superseded by the interactive mode of the terminal UI
  plan, see above.
