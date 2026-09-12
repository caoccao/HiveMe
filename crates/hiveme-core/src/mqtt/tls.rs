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

//! The TLS configuration of the broker connection.
//!
//! HiveMQ Cloud presents a public certificate chain, which every operating system
//! trust store already carries, so the roots are the native ones and
//! `broker.tls.caFile` only ever adds to them. See `docs/specs/hivemq-cloud.md`.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use rumqttc::tokio_rustls::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rumqttc::tokio_rustls::rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rumqttc::tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rumqttc::tokio_rustls::rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

use crate::config::{BrokerUrl, Tls};
use crate::error::{Error, Result};

/// The rustls configuration for a connection to `url`.
///
/// Server verification cannot be turned off for a HiveMQ Cloud host: the credentials
/// travel in the CONNECT packet, so an unverified connection would hand them to
/// whoever answered. The setting is honored for any other host, where a local broker
/// with a self-signed certificate is a reasonable thing to have, and logs a warning.
pub fn client_config(tls: &Tls, url: &BrokerUrl) -> Result<Arc<ClientConfig>> {
  install_crypto_provider();
  let roots = root_store(tls.ca_file.as_deref())?;
  let builder = ClientConfig::builder();

  let config = if tls.verify_server {
    builder.with_root_certificates(roots).with_no_client_auth()
  } else if url.is_hivemq_cloud() {
    log::warn!(
      "broker.tls.verifyServer is false, but {} is a HiveMQ Cloud host and the broker password \
       would be sent to whatever answered, so the certificate is verified anyway",
      url.host
    );
    builder.with_root_certificates(roots).with_no_client_auth()
  } else {
    log::warn!(
      "broker.tls.verifyServer is false, so the certificate of {} is not checked and the \
       connection is open to interception",
      url.host
    );
    let verifier = NoServerVerification::new(builder.crypto_provider().clone());
    builder
      .dangerous()
      .with_custom_certificate_verifier(Arc::new(verifier))
      .with_no_client_auth()
  };

  Ok(Arc::new(config))
}

/// Chooses the cryptography rustls uses, once per process.
///
/// rustls picks a provider on its own only when exactly one is compiled in, and panics
/// rather than guessing when there are two. `hmg` links a second one through `ureq`,
/// which brings its own rustls with `ring`, so the choice is made here instead of being
/// left to a guess. aws-lc-rs is the provider `rumqttc` is built against.
fn install_crypto_provider() {
  static ONCE: std::sync::Once = std::sync::Once::new();
  ONCE.call_once(|| {
    if CryptoProvider::get_default().is_some() {
      return;
    }
    if rumqttc::tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
      .install_default()
      .is_err()
    {
      log::debug!("another rustls crypto provider was installed first");
    }
  });
}

/// The native trust store, plus the certificates of `ca_file` when one is configured.
fn root_store(ca_file: Option<&Path>) -> Result<RootCertStore> {
  let mut roots = RootCertStore::empty();

  let native = rustls_native_certs::load_native_certs();
  for error in &native.errors {
    // A store that is only partly readable is common enough that this is not fatal on
    // its own; an empty result below is what actually stops the connection.
    log::debug!("a native certificate source could not be read: {error}");
  }
  let native_count = native.certs.len();
  let (added, ignored) = roots.add_parsable_certificates(native.certs);
  log::debug!("loaded {added} of {native_count} native root certificates, {ignored} were unusable");

  if let Some(path) = ca_file {
    let added = add_pem_file(&mut roots, path)?;
    log::debug!("loaded {added} root certificates from {}", path.display());
  }

  if roots.is_empty() {
    return Err(Error::Tls(match ca_file {
      Some(path) => format!(
        "neither the system trust store nor {} holds a usable certificate",
        path.display()
      ),
      None => "the system trust store holds no usable certificate, set broker.tls.caFile".to_owned(),
    }));
  }
  Ok(roots)
}

/// Adds every certificate of a PEM file to `roots`, reporting how many were added.
fn add_pem_file(roots: &mut RootCertStore, path: &Path) -> Result<usize> {
  let file = File::open(path)
    .map_err(|source| Error::Tls(format!("cannot read broker.tls.caFile {}: {source}", path.display())))?;
  let certificates = rustls_pemfile::certs(&mut BufReader::new(file))
    .collect::<std::result::Result<Vec<CertificateDer<'static>>, _>>()
    .map_err(|source| {
      Error::Tls(format!(
        "broker.tls.caFile {} is not a PEM certificate file: {source}",
        path.display()
      ))
    })?;
  if certificates.is_empty() {
    return Err(Error::Tls(format!(
      "broker.tls.caFile {} holds no CERTIFICATE block",
      path.display()
    )));
  }
  let (added, ignored) = roots.add_parsable_certificates(certificates);
  if added == 0 {
    return Err(Error::Tls(format!(
      "none of the {ignored} certificates in broker.tls.caFile {} can be used as a root",
      path.display()
    )));
  }
  Ok(added)
}

/// Accepts any server certificate, for `broker.tls.verifyServer = false`.
///
/// Signatures are still checked, so the handshake still proves that the peer holds the
/// key of the certificate it presented. What is skipped is the question of whether that
/// certificate is the right one.
#[derive(Debug)]
struct NoServerVerification {
  provider: Arc<CryptoProvider>,
}

impl NoServerVerification {
  fn new(provider: Arc<CryptoProvider>) -> Self {
    Self { provider }
  }
}

impl ServerCertVerifier for NoServerVerification {
  fn verify_server_cert(
    &self,
    _end_entity: &CertificateDer<'_>,
    _intermediates: &[CertificateDer<'_>],
    _server_name: &ServerName<'_>,
    _ocsp_response: &[u8],
    _now: UnixTime,
  ) -> std::result::Result<ServerCertVerified, rumqttc::tokio_rustls::rustls::Error> {
    Ok(ServerCertVerified::assertion())
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> std::result::Result<HandshakeSignatureValid, rumqttc::tokio_rustls::rustls::Error> {
    verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> std::result::Result<HandshakeSignatureValid, rumqttc::tokio_rustls::rustls::Error> {
    verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.provider.signature_verification_algorithms.supported_schemes()
  }
}

#[cfg(test)]
mod tests {
  use std::io::Write;

  use super::*;

  fn cloud_url() -> BrokerUrl {
    BrokerUrl::parse("mqtts://abc123.s1.eu.hivemq.cloud:8883").unwrap()
  }

  fn local_url() -> BrokerUrl {
    BrokerUrl::parse("mqtts://localhost:8883").unwrap()
  }

  #[test]
  fn the_crypto_provider_is_chosen_rather_than_guessed_at() {
    // A build that links two providers panics inside rustls when it has to pick one,
    // so this is what keeps every TLS connection in `hmg` working.
    install_crypto_provider();
    assert!(CryptoProvider::get_default().is_some());
    install_crypto_provider();
  }

  #[test]
  fn the_native_roots_are_enough_on_their_own() {
    assert!(client_config(&Tls::default(), &cloud_url()).is_ok());
  }

  #[test]
  fn a_missing_ca_file_is_reported_with_its_path() {
    let tls = Tls {
      verify_server: true,
      ca_file: Some(Path::new("no/such/roots.pem").to_path_buf()),
    };
    let message = client_config(&tls, &local_url()).unwrap_err().to_string();
    assert!(message.contains("roots.pem"), "{message}");
  }

  #[test]
  fn a_ca_file_that_is_not_pem_is_reported() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("roots.pem");
    let mut file = File::create(&path).unwrap();
    file.write_all(b"this is not a certificate\n").unwrap();
    drop(file);

    let tls = Tls {
      verify_server: true,
      ca_file: Some(path),
    };
    let message = client_config(&tls, &local_url()).unwrap_err().to_string();
    assert!(message.contains("no CERTIFICATE block"), "{message}");
  }

  #[test]
  fn a_ca_file_is_added_to_the_native_roots() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("roots.pem");
    std::fs::write(&path, super::super::SELF_SIGNED_ROOT_PEM).unwrap();

    let tls = Tls {
      verify_server: true,
      ca_file: Some(path),
    };
    assert!(client_config(&tls, &local_url()).is_ok());
  }

  #[test]
  fn verification_can_be_turned_off_for_a_local_broker() {
    let tls = Tls {
      verify_server: false,
      ca_file: None,
    };
    // Nothing outside rustls distinguishes the two configurations, so the test asserts
    // only that the dangerous path builds and that a cloud host still takes the safe
    // one, which the warning in client_config explains.
    assert!(client_config(&tls, &local_url()).is_ok());
    assert!(client_config(&tls, &cloud_url()).is_ok());
  }
}
