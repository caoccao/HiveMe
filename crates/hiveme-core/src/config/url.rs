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
//! HiveMe accepts the handful of schemes listed in `docs/specs/config.md`, so a small
//! parser is enough and the crate stays free of a URL dependency.

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

impl BrokerUrl {
  /// Reads a broker URL, describing the problem in a form fit for a user.
  pub fn parse(raw: &str) -> Result<Self, String> {
    let raw = raw.trim();
    if raw.is_empty() {
      return Err("the broker URL is empty".to_owned());
    }
    let Some((scheme, rest)) = raw.split_once("://") else {
      return Err(format!("'{raw}' has no scheme, expected one of mqtts, mqtt, wss, ws"));
    };
    let scheme = match scheme.to_ascii_lowercase().as_str() {
      "mqtts" | "ssl" | "mqtt+ssl" => Scheme::Mqtts,
      "mqtt" | "tcp" => Scheme::Mqtt,
      "wss" => Scheme::Wss,
      "ws" => Scheme::Ws,
      other => {
        return Err(format!(
          "'{other}' is not a broker scheme HiveMe speaks, expected one of mqtts, mqtt, wss, ws"
        ));
      }
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
  fn bad_urls_are_rejected_with_a_reason() {
    for raw in [
      "",
      "abc123.s1.eu.hivemq.cloud:8883",
      "http://host",
      "mqtts://",
      "mqtts://host:port",
      "mqtts://host:0",
      "mqtts://user:pass@host",
    ] {
      assert!(BrokerUrl::parse(raw).is_err(), "{raw} should not parse");
    }
  }

  #[test]
  fn display_round_trips_through_parse() {
    for raw in ["mqtts://host:8883", "wss://host:8884/mqtt", "mqtt://localhost:1883"] {
      let url = BrokerUrl::parse(raw).unwrap();
      assert_eq!(BrokerUrl::parse(&url.to_string()).unwrap(), url);
    }
  }
}
