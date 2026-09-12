# App

HiveMe is a rust project with multiple applications built on top of MQTT.

## Applications

1. HiveMe CLI - A command-line interface for interacting with the MQTT broker.
2. HiveMe GUI - A graphical user interface for interacting with the MQTT broker.

Both applications stay in the same folder, share the same config.

## HiveMe CLI

HiveMe CLI is a command-line interface for interacting with the MQTT broker. Its name is `hmc`.

### Usage

- `hmc --help` or `hmc -h` can show the help information for the HiveMe CLI.
- `hmc <message>` can be used to send a message to the MQTT broker.
- `hmc -t <topic> <message>` can be used to send a message to a topic on the MQTT broker.

## HiveMe GUI

HiveMe GUI is a graphical user interface for interacting with the MQTT broker. Its name is `hmg`.
It uses tauri + React as the GUI framework.

The main window has a toolbar on the top, a status bar on the footer, a tree view on the left, a message view on the right.

It has a rule based notification system. Here are the built-in rules:

- Topic `info` triggers an OS notification for info messages.
- Topic `error` triggers an OS notification for error messages.
- Topic `warn` triggers an OS notification for warning messages.

All topics are listed in the left tree view. The message view shows the messages for the selected topic.

## Authentication

HiveMQ broker's TLS MQTT URL is stored in the same config file.

## Reference

- [HiveMQ Cloud Documentation](https://docs.hivemq.com/hivemq-cloud/index.html)
