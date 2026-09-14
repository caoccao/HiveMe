/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

//! Turning a [`Config`] into the options `rumqttc` connects with.
//!
//! The session settings differ between the applications, and that difference is the
//! whole of [`Role`]: one-shot `hmc` is one publish and a goodbye, `hmg` keeps a session
//! so that messages sent during a network interruption can arrive on recovery, and the
//! terminal UI of `hmc` behaves like `hmg` under an identifier of its own. The scheme of
//! the client identifiers is in `docs/specs/hivemq-cloud.md`.

use std::time::Duration;

use rumqttc::v5::MqttOptions;
use rumqttc::{NetworkOptions, TlsConfiguration, Transport};

use crate::config::{BrokerUrl, Config, MIN_KEEP_ALIVE_SECS, Scheme};
use crate::error::{Error, Result};

use super::tls;

/// Which application is connecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
  /// `hmc`: a fresh session per run, no reconnect, no subscriptions.
  Cli,
  /// `hmg`: a session that survives network interruptions while the app is open.
  Gui,
  /// Interactive `hmc`: the connection behavior of `hmg` under a fresh identifier.
  Tui,
}

impl Role {
  /// The segment of the client identifier that names the application.
  pub fn app(&self) -> &'static str {
    match self {
      Self::Cli | Self::Tui => "hmc",
      Self::Gui => "hmg",
    }
  }

  /// Whether each run gets its own client identifier.
  ///
  /// `hmc` runs, one-shot or interactive, can overlap with each other and with a
  /// running `hmg`, and a broker disconnects the older of two connections that share an
  /// identifier, so every run takes a fresh one. `hmg` keeps a stable identifier
  /// because that is what ties it to its session.
  pub fn unique_client_id(&self) -> bool {
    matches!(self, Self::Cli | Self::Tui)
  }

  /// Whether the broker discards whatever session the identifier already had.
  ///
  /// One-shot `hmc` asks for a clean start because it wants no session at all. The
  /// other two must not: the flag is on the CONNECT packet, and the client sends the
  /// same packet on every reconnect, so asking for a clean start is asking the broker
  /// to throw away the session on every recovery, along with the messages it held while
  /// the network was down. The terminal UI takes a fresh identifier per run, so the
  /// session there can only ever be the one this process left a moment ago, and
  /// [`crate::mqtt::MqttClient::end_session`] discards it on the way out.
  pub fn clean_start(&self) -> bool {
    matches!(self, Self::Cli)
  }

  /// Whether a lost connection is retried.
  pub fn reconnects(&self) -> bool {
    matches!(self, Self::Gui | Self::Tui)
  }

  /// How long the broker keeps the session after a disconnect.
  pub fn session_expiry_secs(&self, config: &Config) -> u32 {
    match self {
      Self::Cli => 0,
      Self::Gui | Self::Tui => config.broker.session_expiry_secs,
    }
  }
}

impl std::fmt::Display for Role {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(self.app())
  }
}

/// The identifier `role` connects with under `config`.
pub fn client_id(config: &Config, role: Role) -> String {
  config.client_id(role.app(), role.unique_client_id())
}

/// Everything about the connection that is decided before it is opened.
#[derive(Debug, Clone)]
pub struct Connection {
  pub options: MqttOptions,
  pub url: BrokerUrl,
  pub client_id: String,
  pub role: Role,
  pub connect_timeout: Duration,
}

/// Builds the `rumqttc` options for `role` from `config`.
///
/// The config is not validated here. Both applications call
/// [`Config::validate`](crate::config::Config::validate) first, and this reports only
/// what would stop the options being built at all.
pub fn build(config: &Config, role: Role) -> Result<Connection> {
  let url = config
    .broker
    .parsed_url()
    .map_err(|reason| Error::ConfigInvalid(vec![format!("broker.url: {reason}")]))?;
  let client_id = client_id(config, role);
  let connect_timeout = Duration::from_secs(config.broker.connect_timeout_secs.max(1));

  // A WebSocket transport reads the host and the port back out of the address, so it
  // has to be handed the whole URL, while the TCP transports take them apart.
  let address = if url.scheme.is_websocket() {
    url.to_string()
  } else {
    url.host.clone()
  };
  let mut options = MqttOptions::new(client_id.clone(), address, url.port);

  options.set_transport(transport(config, &url)?);
  // Clamped rather than passed on: `set_keep_alive` asserts, and a config value that
  // has not been through the validator, or a caller that skipped it, must not be able
  // to turn one line of JSON into a panic. See [`MIN_KEEP_ALIVE_SECS`].
  options.set_keep_alive(Duration::from_secs(u64::from(
    config.broker.keep_alive_secs.max(MIN_KEEP_ALIVE_SECS),
  )));
  options.set_clean_start(role.clean_start());
  options.set_session_expiry_interval(Some(role.session_expiry_secs(config)));
  options.set_connection_timeout(connect_timeout.as_secs());

  // Credentials are optional so that a local broker with anonymous access, which is
  // what the integration tests run against, needs no fake username.
  let username = config.broker.username.trim();
  if !username.is_empty() {
    options.set_credentials(username, config.broker.resolved_password()?);
  }

  let mut network = NetworkOptions::new();
  network.set_connection_timeout(connect_timeout.as_secs());
  options.set_network_options(network);

  Ok(Connection {
    options,
    url,
    client_id,
    role,
    connect_timeout,
  })
}

/// The `rumqttc` transport for a broker URL.
fn transport(config: &Config, url: &BrokerUrl) -> Result<Transport> {
  match url.scheme {
    Scheme::Mqtts => Ok(Transport::tls_with_config(TlsConfiguration::Rustls(
      tls::client_config(&config.broker.tls, url)?,
    ))),
    Scheme::Mqtt => {
      warn_about_plaintext(url);
      Ok(Transport::tcp())
    }
    #[cfg(feature = "websocket")]
    Scheme::Wss => Ok(Transport::wss_with_config(TlsConfiguration::Rustls(
      tls::client_config(&config.broker.tls, url)?,
    ))),
    #[cfg(feature = "websocket")]
    Scheme::Ws => {
      warn_about_plaintext(url);
      Ok(Transport::Ws)
    }
    #[cfg(not(feature = "websocket"))]
    Scheme::Wss | Scheme::Ws => Err(Error::NotImplemented(format!(
      "broker.url uses {}, which needs the 'websocket' feature of hiveme-core; \
       HiveMQ Cloud also serves plain MQTT over TLS on port 8883, so mqtts:// works today",
      url.scheme
    ))),
  }
}

/// Says once, loudly, that the credentials are about to cross the network in the clear.
fn warn_about_plaintext(url: &BrokerUrl) {
  log::warn!(
    "broker.url uses {}, so the connection to {} is not encrypted and the broker \
     password travels in the clear; this is only meant for a local test broker",
    url.scheme,
    url.host
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  fn config() -> Config {
    let mut config = Config::default();
    config.device.id = "0f9c2d1e-1111-7000-8000-aaaabbbbcccc".to_owned();
    config.broker.url = "mqtts://abc123.s1.eu.hivemq.cloud:8883".to_owned();
    config.broker.username = "hiveme".to_owned();
    config.broker.password = "secret".to_owned();
    config
  }

  #[test]
  fn the_two_roles_do_not_share_a_client_id() {
    let config = config();
    assert_ne!(client_id(&config, Role::Cli), client_id(&config, Role::Gui));
  }

  #[test]
  fn the_terminal_ui_does_not_share_a_client_id_with_either_application() {
    let config = config();
    let tui = client_id(&config, Role::Tui);
    assert!(tui.starts_with("hiveme-hmc-0f9c2d1e-"), "{tui}");
    assert_eq!(tui.len(), "hiveme-hmc-0f9c2d1e-".len() + 8);
    assert_ne!(tui, client_id(&config, Role::Tui), "every terminal UI takes a fresh id");
    assert_ne!(tui, client_id(&config, Role::Gui));
  }

  #[test]
  fn the_terminal_ui_reconnects_and_keeps_its_session_for_the_process() {
    let mut config = config();
    config.broker.session_expiry_secs = 7_200;
    let connection = build(&config, Role::Tui).unwrap();
    assert!(
      !connection.options.clean_start(),
      "a reconnect that asked for a clean start would throw away the messages the broker \
       held while the network was down"
    );
    assert_eq!(connection.options.session_expiry_interval(), Some(7_200));
    assert!(Role::Tui.reconnects());
  }

  #[test]
  fn only_the_one_shot_run_asks_the_broker_to_forget_its_session() {
    assert!(Role::Cli.clean_start());
    assert!(!Role::Gui.clean_start());
    assert!(!Role::Tui.clean_start());
  }

  #[test]
  fn a_keep_alive_the_client_would_panic_on_is_clamped() {
    let mut config = config();
    for seconds in [0, 1, MIN_KEEP_ALIVE_SECS - 1] {
      config.broker.keep_alive_secs = seconds;
      // `build` is reached by `connect_unvalidated` too, which never sees the validator.
      let connection = build(&config, Role::Cli).unwrap();
      assert_eq!(
        connection.options.keep_alive(),
        Duration::from_secs(u64::from(MIN_KEEP_ALIVE_SECS))
      );
    }
    config.broker.keep_alive_secs = MIN_KEEP_ALIVE_SECS;
    assert_eq!(
      build(&config, Role::Cli).unwrap().options.keep_alive(),
      Duration::from_secs(u64::from(MIN_KEEP_ALIVE_SECS))
    );
  }

  #[test]
  fn the_gui_client_id_is_stable_across_runs() {
    let config = config();
    assert_eq!(client_id(&config, Role::Gui), client_id(&config, Role::Gui));
    assert_eq!(client_id(&config, Role::Gui), "hiveme-hmg-0f9c2d1e");
  }

  #[test]
  fn every_cli_run_takes_a_fresh_client_id() {
    let config = config();
    let one = client_id(&config, Role::Cli);
    let other = client_id(&config, Role::Cli);
    assert_ne!(one, other);
    assert!(one.starts_with("hiveme-hmc-0f9c2d1e-"), "{one}");
    assert_eq!(one.len(), "hiveme-hmc-0f9c2d1e-".len() + 8);
  }

  #[test]
  fn the_client_id_prefix_is_configurable() {
    let mut config = config();
    config.broker.client_id_prefix = "build-farm".to_owned();
    assert_eq!(client_id(&config, Role::Gui), "build-farm-hmg-0f9c2d1e");
  }

  #[test]
  fn the_cli_starts_clean_and_keeps_no_session() {
    let config = config();
    let connection = build(&config, Role::Cli).unwrap();
    assert!(connection.options.clean_start());
    assert_eq!(connection.options.session_expiry_interval(), Some(0));
    assert!(!Role::Cli.reconnects());
  }

  #[test]
  fn the_gui_resumes_its_session_and_reconnects() {
    let mut config = config();
    config.broker.session_expiry_secs = 7_200;
    let connection = build(&config, Role::Gui).unwrap();
    assert!(!connection.options.clean_start());
    assert_eq!(connection.options.session_expiry_interval(), Some(7_200));
    assert!(Role::Gui.reconnects());
  }

  #[test]
  fn the_broker_address_comes_from_the_url() {
    let connection = build(&config(), Role::Gui).unwrap();
    assert_eq!(
      connection.options.broker_address(),
      ("abc123.s1.eu.hivemq.cloud".to_owned(), 8883)
    );
    assert_eq!(connection.url.port, 8883);
  }

  #[test]
  fn the_keep_alive_and_timeout_come_from_the_config() {
    let mut config = config();
    config.broker.keep_alive_secs = 45;
    config.broker.connect_timeout_secs = 20;
    let connection = build(&config, Role::Gui).unwrap();
    assert_eq!(connection.options.keep_alive(), Duration::from_secs(45));
    assert_eq!(connection.connect_timeout, Duration::from_secs(20));
    assert_eq!(connection.options.connection_timeout(), 20);
  }

  #[test]
  fn a_zero_connect_timeout_still_leaves_a_second() {
    let mut config = config();
    config.broker.connect_timeout_secs = 0;
    assert_eq!(
      build(&config, Role::Cli).unwrap().connect_timeout,
      Duration::from_secs(1)
    );
  }

  #[test]
  fn credentials_are_sent_when_a_username_is_set() {
    let connection = build(&config(), Role::Cli).unwrap();
    let login = connection.options.credentials().unwrap();
    assert_eq!(login.username, "hiveme");
    assert_eq!(login.password, "secret");
  }

  #[test]
  fn an_anonymous_broker_needs_no_credentials() {
    let mut config = config();
    config.broker.url = "mqtt://localhost:1883".to_owned();
    config.broker.username = String::new();
    config.broker.password = String::new();
    let connection = build(&config, Role::Cli).unwrap();
    assert!(connection.options.credentials().is_none());
  }

  #[test]
  fn a_plain_tcp_url_uses_the_tcp_transport() {
    let mut config = config();
    config.broker.url = "mqtt://localhost:1883".to_owned();
    let connection = build(&config, Role::Cli).unwrap();
    assert!(matches!(connection.options.transport(), Transport::Tcp));
    assert_eq!(connection.url.port, 1883);
  }

  #[test]
  fn a_tls_url_uses_the_tls_transport() {
    let connection = build(&config(), Role::Cli).unwrap();
    assert!(matches!(connection.options.transport(), Transport::Tls(_)));
  }

  #[test]
  fn a_url_that_does_not_parse_is_a_config_error() {
    // A URL with no scheme parses now, as TLS MQTT, so the one that does not is a URL
    // with something wrong inside it.
    let mut config = config();
    config.broker.url = "abc123.s1.eu.hivemq.cloud:not-a-port".to_owned();
    let error = build(&config, Role::Cli).unwrap_err();
    assert!(error.is_config(), "{error}");
    assert!(error.to_string().contains("broker.url"), "{error}");
  }

  #[cfg(not(feature = "websocket"))]
  #[test]
  fn a_websocket_url_says_which_feature_it_needs() {
    let mut config = config();
    config.broker.url = "wss://abc123.s1.eu.hivemq.cloud:8884/mqtt".to_owned();
    let error = build(&config, Role::Cli).unwrap_err();
    assert!(error.is_config(), "{error}");
    assert!(error.to_string().contains("websocket"), "{error}");
  }

  #[cfg(feature = "websocket")]
  #[test]
  fn a_websocket_url_is_handed_over_whole() {
    let mut config = config();
    config.broker.url = "wss://abc123.s1.eu.hivemq.cloud:8884/mqtt".to_owned();
    let connection = build(&config, Role::Cli).unwrap();
    assert!(matches!(connection.options.transport(), Transport::Wss(_)));
    assert_eq!(
      connection.options.broker_address().0,
      "wss://abc123.s1.eu.hivemq.cloud:8884/mqtt"
    );
  }

  #[test]
  fn the_password_can_come_from_the_environment() {
    let mut config = config();
    config.broker.password = String::new();
    config.broker.password_ref = Some(crate::config::SecretRef::Env {
      name: "HIVEME_TEST_OPTIONS_PASSWORD".to_owned(),
    });
    let error = build(&config, Role::Cli).unwrap_err();
    assert!(error.to_string().contains("HIVEME_TEST_OPTIONS_PASSWORD"), "{error}");
  }

  #[test]
  fn the_role_names_the_application() {
    assert_eq!(Role::Cli.to_string(), "hmc");
    assert_eq!(Role::Gui.to_string(), "hmg");
    assert_eq!(Role::Tui.to_string(), "hmc");
  }
}
