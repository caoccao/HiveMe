# TODOs

Tracked work lives in [the initialization plan](plans/plan-initialization.md). This
page collects what the plan deliberately left open.

## Open questions from the plan

* Whether the `wss://` transport ships with the MQTT client in phase 2 or waits for
  phase 6 depends on the quality of `rumqttc`'s WebSocket support with rustls.
* The `$id` URLs in the generated schemas are placeholders until a documentation site
  exists.
* Whether `pnpm tauri build` inside a Cargo workspace needs extra configuration for
  the Windows portable archive.

## Deferred to phase 6

* Message encryption: `A256GCM`, `hmc key generate`, and the `Opportunistic` and
  `Required` modes.
* The HiveMQ Cloud REST client and the GUI Cluster tab, which need the Starter plan.
* OS keychain storage for the broker password, the encryption secrets, and the REST
  token.
* Named broker profiles.
* The remaining locales: de, es, fr, it, ja, zh-CN, zh-HK, zh-TW.
* Payload based notification rule matching.
* The `hmc sub` and `hmc config` subcommands.
