# Release Notes

## 0.1.0 (unreleased)

The first release of HiveMe: get a message from any shell, script, or machine to your
desktop, through your own HiveMQ Cloud cluster.

### Connecting

* **Your own HiveMQ Cloud cluster.** Connect to a free Serverless cluster over MQTT 5
  with TLS. Paste the URL exactly as the HiveMQ Cloud console shows it; there are no
  certificates to set up.
* **Set up once, use everywhere.** **Copy CLI setup** turns the cluster settings into
  one command that sets up `hmc` on any machine, in your language.
* **Stays connected.** HiveMe reconnects on its own after a network drop, and the
  messages the broker held in the meantime arrive when the connection comes back.
* **Settings that save themselves.** Every change applies right away and is saved when
  you pause typing.

### Desktop app

* **Minimize to the system tray.** The new button at the right of the toolbar hides
  the window while messages and notifications keep running. The HiveMe tray icon
  appears at startup and stays available while the app runs. Double-click it to
  restore the window, or right-click to restore or exit. Menu labels follow your
  language. On Linux, a missing tray host leaves the window open with an explanation.

* **Topic tree.** Every topic appears as messages arrive, with unread badges and a
  filter. Selecting a topic shows its messages and those of all its subtopics.
* **Chat view.** Messages appear as bubbles colored by level, with their title, sender,
  and topic. JSON data opens as a collapsible tree, and plain text, other JSON, and
  binary payloads are shown too. Copy a message, or its raw JSON, in one click.
* **Composer.** Write multi-line messages and choose the level, title, topic, QoS, and
  retain flag, or send raw JSON. Drafts are kept per topic while the app runs.
* **Four message levels.** Info, Success, Warn, and Error are the only defined levels
  in the CLI, desktop app, and terminal UI, including notification rules.
* **Status bar.** Shows the connection, the subscriptions, the messages received, and
  the size of the history.
* **Remembers its window.** HiveMe opens at the size and position you left it.

### Command line

* **Send from anywhere.** `hmc "Build finished"` sends a message from a shell, a
  script, or a cron job. The message can come from an argument or from stdin.
* **Levels, titles, and topics.** Choose `info`, `success`, `warn`, or `error`, add a
  title, and pick a topic under `hiveme`. `--json` sends a raw JSON payload.
* **Made for scripts.** `hmc` confirms each message it sends and returns a different
  exit code for each kind of failure, so a script can tell a wrong password from a
  broker that did not answer.

### Terminal UI

* **Everything the desktop app does, in a terminal.** Run `hmc` on its own to open the
  topic tree, chat view, composer, settings, and notifications, on a server or over SSH.
* **Keyboard and mouse.** Press **?** to see every key. The mouse scrolls and resizes the
  panes, and copying works over SSH in terminals that support it.
* **Runs beside the desktop app.** The terminal UI and `hmg` can be open at the same
  time. Both show every message live and share one history.

### Notifications

* **Rules.** A rule matches a topic filter and a level. Four built-in rules cover
  `info`, `success`, `warn`, and `error`, and you can add your own, with templates for
  the title and the body.
* **OS notification or topmost window.** Each rule chooses a native OS notification, a
  window that stays above the others, or both. On macOS, HiveMe asks for permission the
  first time it raises an OS notification and appears in System Settings >
  Notifications.
* **Pause.** Silence notifications from the toolbar without losing any messages.
* **No spam.** A burst of messages is summarized as "and N more messages", and a
  message never notifies the app that sent it.

### History

* **Local history.** Topics and messages survive a restart, and are shared by the
  desktop app and the terminal UI.
* **You decide how much to keep.** Limit the messages kept per topic and the number of
  days they are kept, or clear a topic and all its subtopics from the toolbar.

### Appearance and languages

* **Light, dark, or auto,** with twenty color themes, in both the desktop app and the
  terminal UI.
* **Nine languages.** English, German, Spanish, French, Italian, Japanese, Simplified
  Chinese, and Traditional Chinese for Hong Kong and Taiwan, including the messages
  `hmc` prints.
* **Editor preferences.** Turn autocomplete, autocorrection, automatic capitalization,
  spellchecking, and writing suggestions on or off for every text field of the desktop
  app.

### Installation and updates

* **Linux, macOS, and Windows.** deb, rpm, and AppImage packages for Linux, dmg images
  for Intel and Apple silicon Macs, and msi, setup, and portable packages for Windows.
* **Two apps, one download.** Every installer includes both `hmg` and `hmc`, and `hmc`
  is also available on its own for machines without a desktop.
* **Update check.** HiveMe checks for new releases daily, weekly, or monthly, and lets
  you skip a version.
