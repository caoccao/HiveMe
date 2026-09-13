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

//! Parsing of the broker URL.
//!
//! HiveMe accepts the handful of schemes listed in `docs/specs/config.md`, and a URL
//! that names none, so a small parser is enough and the crate stays free of a URL
//! dependency.
//!
//! [`BrokerUrlParts`] is the other half: the scheme taken off the front of the URL for
//! a settings form and put back afterward, which is `src/lib/brokerUrl.ts` in Rust, so
//! that the Broker panels of `hmg` and of the terminal UI read a pasted URL the same way.
//! `tests/fixtures/broker_url.json` holds the cases both ports are tested against.

use std::fmt;

/// A broker URL broken into the parts the MQTT client needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerUrl {
  pub scheme: Scheme,
  pub host: String,
  pub port: u16,
  /// The WebSocket path, `/mqtt` by default. Empty for the TCP schemes.
  pub path: String,
}

/// A transport HiveMe can speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
  /// MQTT over TLS, the only transport HiveMQ Cloud accepts.
  Mqtts,
  /// MQTT over plain TCP, for a local test broker.
  Mqtt,
  /// MQTT over WebSocket with TLS.
  Wss,
  /// MQTT over plain WebSocket, for a local test broker.
  Ws,
}

impl Scheme {
  /// Every transport, in the order a protocol list offers them.
  pub const ALL: [Self; 4] = [Self::Mqtts, Self::Mqtt, Self::Wss, Self::Ws];

  /// Reads a scheme and the spellings the parser also accepts, in any case.
  pub fn from_alias(raw: &str) -> Option<Self> {
    match raw.to_ascii_lowercase().as_str() {
      "mqtts" | "ssl" | "mqtt+ssl" => Some(Self::Mqtts),
      "mqtt" | "tcp" => Some(Self::Mqtt),
      "wss" => Some(Self::Wss),
      "ws" => Some(Self::Ws),
      _ => None,
    }
  }

  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Mqtts => "mqtts",
      Self::Mqtt => "mqtt",
      Self::Wss => "wss",
      Self::Ws => "ws",
    }
  }

  /// The IANA port used when the URL leaves it out.
  pub fn default_port(&self) -> u16 {
    match self {
      Self::Mqtts => 8883,
      Self::Mqtt => 1883,
      Self::Wss => 8884,
      Self::Ws => 8083,
    }
  }

  /// Whether the transport encrypts the connection.
  pub fn is_secure(&self) -> bool {
    matches!(self, Self::Mqtts | Self::Wss)
  }

  /// Whether the transport is a WebSocket, which carries a path.
  pub fn is_websocket(&self) -> bool {
    matches!(self, Self::Wss | Self::Ws)
  }
}

impl fmt::Display for Scheme {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.as_str())
  }
}

/// The WebSocket path HiveMQ Cloud serves MQTT on.
pub const DEFAULT_WEBSOCKET_PATH: &str = "/mqtt";

/// What a URL with no scheme is read as, because a HiveMQ Cloud cluster accepts nothing else.
pub const DEFAULT_SCHEME: Scheme = Scheme::Mqtts;

impl BrokerUrl {
  /// Reads a broker URL, describing the problem in a form fit for a user.
  pub fn parse(raw: &str) -> Result<Self, String> {
    let raw = raw.trim();
    if raw.is_empty() {
      return Err("the broker URL is empty".to_owned());
    }
    // A URL with no scheme is the one the HiveMQ Cloud console shows, and it is read as
    // TLS MQTT: the transport the cluster it came from accepts, and the one the Settings
    // form of `hmg` starts on. So the same string works in the config file, in a setup
    // string, and in the form, and none of the three asks for a scheme to be typed in
    // front of what was copied. A scheme that is written is still the one that is used.
    let (scheme, rest) = match raw.split_once("://") {
      None => (DEFAULT_SCHEME, raw),
      Some((scheme, rest)) => match Scheme::from_alias(scheme) {
        Some(scheme) => (scheme, rest),
        None => {
          return Err(format!(
            "'{}' is not a broker scheme HiveMe speaks, expected one of mqtts, mqtt, wss, ws",
            scheme.to_ascii_lowercase()
          ));
        }
      },
    };

    // Credentials in the URL are refused rather than silently ignored, because a user
    // who writes them there expects them to be used.
    if rest.split('/').next().is_some_and(|authority| authority.contains('@')) {
      return Err("put the username and password in the broker block, not in the URL".to_owned());
    }

    let (authority, path) = match rest.find('/') {
      Some(index) => (&rest[..index], &rest[index..]),
      None => (rest, ""),
    };
    if authority.is_empty() {
      return Err(format!("'{raw}' has no host"));
    }

    let (host, port) = match authority.rsplit_once(':') {
      Some((host, port)) => {
        let port = port
          .parse::<u16>()
          .map_err(|_| format!("'{port}' is not a port number"))?;
        if port == 0 {
          return Err("port 0 is not a broker port".to_owned());
        }
        (host, port)
      }
      None => (authority, scheme.default_port()),
    };
    if host.is_empty() {
      return Err(format!("'{raw}' has no host"));
    }

    let path = if scheme.is_websocket() {
      if path.is_empty() {
        DEFAULT_WEBSOCKET_PATH.to_owned()
      } else {
        path.to_owned()
      }
    } else {
      String::new()
    };

    Ok(Self {
      scheme,
      host: host.to_owned(),
      port,
      path,
    })
  }

  /// Whether this points at a HiveMQ Cloud cluster, which is always TLS only.
  pub fn is_hivemq_cloud(&self) -> bool {
    self.host.eq_ignore_ascii_case("hivemq.cloud") || self.host.to_ascii_lowercase().ends_with(".hivemq.cloud")
  }
}

/// A broker URL as a settings form holds it: a protocol, and the rest of the URL exactly
/// as it was written.
///
/// The HiveMQ Cloud console shows a cluster as `host`, `host:8883`, or `host:8884/mqtt`,
/// and all three are pasted in and saved as they are. Only the scheme is ever taken off
/// the front; [`BrokerUrl::parse`] stays the one thing that reads a host, a port, and a
/// path out of what is left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerUrlParts {
  pub scheme: Scheme,
  /// The host, with the port and the path if the URL carries them, as written.
  pub address: String,
}

impl Default for BrokerUrlParts {
  /// What a setting that was never filled in starts from: TLS MQTT and no address.
  fn default() -> Self {
    Self {
      scheme: DEFAULT_SCHEME,
      address: String::new(),
    }
  }
}

impl BrokerUrlParts {
  /// Takes the scheme off a URL, and nothing else. `splitBrokerUrl` in the frontend.
  ///
  /// A URL without a scheme keeps `fallback`, the protocol already selected. One with a
  /// scheme moves the protocol to it, so pasting a whole URL does not leave `mqtts://`
  /// in the box; a scheme HiveMe does not know is still taken off and leaves the
  /// protocol on `fallback`, because the alternative is a URL with two schemes in it.
  pub fn split(raw: &str, fallback: Scheme) -> Self {
    let text = raw.trim();
    match text.split_once("://") {
      None => Self {
        scheme: fallback,
        address: text.to_owned(),
      },
      Some((scheme, address)) => Self {
        scheme: Scheme::from_alias(scheme).unwrap_or(fallback),
        address: address.to_owned(),
      },
    }
  }

  /// Writes the scheme back on. Empty while there is no address, so an empty form saves
  /// as empty. `joinBrokerUrl` in the frontend.
  pub fn join(&self) -> String {
    let address = self.address.trim();
    if address.is_empty() {
      String::new()
    } else {
      format!("{}://{address}", self.scheme)
    }
  }

  /// The port the connection will use, for the line under the box that says so: the one
  /// the address carries, or the protocol's own. `effectivePort` in the frontend.
  pub fn effective_port(&self) -> u16 {
    let authority = self.address.split('/').next().unwrap_or_default();
    authority
      .rsplit_once(':')
      .map(|(_, written)| written)
      .filter(|written| !written.is_empty() && written.bytes().all(|byte| byte.is_ascii_digit()))
      .filter(|written| !written.starts_with('0'))
      .and_then(|written| written.parse::<u16>().ok())
      .unwrap_or_else(|| self.scheme.default_port())
  }
}

impl fmt::Display for BrokerUrl {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}://{}:{}{}", self.scheme, self.host, self.port, self.path)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_cloud_url_parses() {
    let url = BrokerUrl::parse("mqtts://abc123.s1.eu.hivemq.cloud:8883").unwrap();
    assert_eq!(url.scheme, Scheme::Mqtts);
    assert_eq!(url.host, "abc123.s1.eu.hivemq.cloud");
    assert_eq!(url.port, 8883);
    assert!(url.path.is_empty());
    assert!(url.is_hivemq_cloud());
    assert!(url.scheme.is_secure());
  }

  #[test]
  fn the_port_defaults_per_scheme() {
    assert_eq!(BrokerUrl::parse("mqtts://host").unwrap().port, 8883);
    assert_eq!(BrokerUrl::parse("mqtt://host").unwrap().port, 1883);
    assert_eq!(BrokerUrl::parse("wss://host").unwrap().port, 8884);
    assert_eq!(BrokerUrl::parse("ws://host").unwrap().port, 8083);
  }

  #[test]
  fn a_websocket_url_defaults_to_the_mqtt_path() {
    assert_eq!(BrokerUrl::parse("wss://host:8884").unwrap().path, "/mqtt");
    assert_eq!(BrokerUrl::parse("wss://host:8884/ws").unwrap().path, "/ws");
  }

  #[test]
  fn a_tcp_url_has_no_path() {
    assert!(BrokerUrl::parse("mqtts://host:8883/ignored").unwrap().path.is_empty());
  }

  #[test]
  fn a_local_broker_is_not_hivemq_cloud() {
    assert!(!BrokerUrl::parse("mqtt://localhost:1883").unwrap().is_hivemq_cloud());
  }

  #[test]
  fn a_lookalike_host_is_not_hivemq_cloud() {
    assert!(!BrokerUrl::parse("mqtts://nothivemq.cloud").unwrap().is_hivemq_cloud());
    assert!(
      !BrokerUrl::parse("mqtts://hivemq.cloud.example.com")
        .unwrap()
        .is_hivemq_cloud()
    );
  }

  #[test]
  fn a_url_with_no_scheme_is_read_as_tls_mqtt() {
    // What the HiveMQ Cloud console shows, copied as it shows it. Nobody should have to
    // type a scheme in front of it to set the CLI up, any more than in the GUI form.
    let url = BrokerUrl::parse("abc123.s1.eu.hivemq.cloud:8883").unwrap();
    assert_eq!(url.scheme, DEFAULT_SCHEME);
    assert_eq!(url.scheme, Scheme::Mqtts);
    assert_eq!(url.host, "abc123.s1.eu.hivemq.cloud");
    assert_eq!(url.port, 8883);
    assert!(url.is_hivemq_cloud());

    // The port is still the scheme's own when the URL does not carry one.
    let bare = BrokerUrl::parse("abc123.s1.eu.hivemq.cloud").unwrap();
    assert_eq!(bare.port, 8883);

    // A transport that is not the default is the one thing a URL still has to say, so a
    // WebSocket is written with its scheme.
    assert_eq!(BrokerUrl::parse("wss://host:8884/mqtt").unwrap().scheme, Scheme::Wss);
  }

  #[test]
  fn bad_urls_are_rejected_with_a_reason() {
    for raw in [
      "",
      "http://host",
      "host:port",
      "user:pass@host",
      "mqtts://",
      "mqtts://host:port",
      "mqtts://host:0",
      "mqtts://user:pass@host",
    ] {
      assert!(BrokerUrl::parse(raw).is_err(), "{raw} should not parse");
    }
  }

  #[test]
  fn every_scheme_and_alias_is_read_in_any_case() {
    assert_eq!(Scheme::from_alias("MQTT+SSL"), Some(Scheme::Mqtts));
    assert_eq!(Scheme::from_alias("tcp"), Some(Scheme::Mqtt));
    assert_eq!(Scheme::from_alias("http"), None);
    for scheme in Scheme::ALL {
      assert_eq!(Scheme::from_alias(scheme.as_str()), Some(scheme));
    }
  }

  #[test]
  fn the_parts_round_trip_every_protocol() {
    for scheme in Scheme::ALL {
      let parts = BrokerUrlParts {
        scheme,
        address: "abc123.s1.eu.hivemq.cloud:8883".to_owned(),
      };
      assert_eq!(BrokerUrlParts::split(&parts.join(), Scheme::Ws), parts);
    }
    assert_eq!(BrokerUrlParts::split("", Scheme::Mqtts), BrokerUrlParts::default());
  }

  #[test]
  fn display_round_trips_through_parse() {
    for raw in ["mqtts://host:8883", "wss://host:8884/mqtt", "mqtt://localhost:1883"] {
      let url = BrokerUrl::parse(raw).unwrap();
      assert_eq!(BrokerUrl::parse(&url.to_string()).unwrap(), url);
    }
  }
}
