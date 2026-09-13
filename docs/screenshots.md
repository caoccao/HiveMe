# Screenshots

> None yet. They have to be taken by hand on a desktop session, so they are the one
> piece of step 5.2 that is still open. The recipe below produces the state to shoot.

Wanted, to be saved under `docs/screenshots/` and linked from here and from the
[README](../README.md):

| File | Shot |
|------|------|
| `messages.png` | The main window: toolbar, topic tree with unread badges, chat view of a selected topic, status bar |
| `settings.png` | The Settings tab, Broker category, with the category strip and **Copy CLI setup** |
| `notification.png` | A desktop notification raised by the built-in `error` rule |

## Setting up the shot

A local broker gives a repeatable set of messages without touching a real cluster:

```sh
docker run -d --rm --name hiveme-shots -p 21883:1883 hivemq/hivemq-ce
```

Point a throwaway config at it, so the screenshots do not carry real credentials:

```json
{
  "version": 1,
  "device": { "id": "0f7a1c2e-5d4b-4a6e-9c1d-2b3e4f5a6b7c", "name": "sams-desktop" },
  "broker": { "url": "mqtt://127.0.0.1:21883", "username": "demo", "password": "demo" },
  "topics": { "prefix": "hiveme", "default": "", "subscriptions": ["#"] },
  "gui": { "displayMode": "Light", "theme": "Amber" }
}
```

```sh
HIVEME_CONFIG=/tmp/HiveMe.json pnpm tauri dev
```

Then fill the tree from another terminal, with a config pointing at the same broker:

```sh
hmc -c /tmp/hmc.json --level info  --title CI     "Nightly build 482 finished in 6m 12s"
hmc -c /tmp/hmc.json --level info  --title Deploy "Deployed hiveme 0.1.0 to staging"
hmc -c /tmp/hmc.json --level warn  --title Disk   "Disk usage on build-02 is at 87%"
hmc -c /tmp/hmc.json --level error --title CI     "Nightly build 483 failed: 2 tests red"
hmc -c /tmp/hmc.json -t build/ci --json '{"build":483,"branch":"main","failed":["config::migrate"]}'
```

Use the already selected `hiveme` topic for the main shot, and `docker stop hiveme-shots` afterward.
