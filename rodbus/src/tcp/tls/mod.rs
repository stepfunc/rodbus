// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

pub(crate) mod client;
#[cfg(test)]
mod handshake_tests;
pub(crate) mod server;

pub(crate) use client::*;
pub(crate) use server::*;

/// Determines how the certificate(s) presented by the peer are validated
///
/// The certificate is validated when the peer presents it during the handshake, before the peer's
/// handshake signature is verified. The handshake fails if either check fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum CertificateMode {
    /// Validates the peer certificate against one or more configured trust anchors
    ///
    /// This mode uses the default certificate verifier in `rustls` to ensure that
    /// the chain of certificates presented by the peer is valid against one of
    /// the configured trust anchors.
    ///
    /// A client configured with a server name checks it against the SAN extension of the server's
    /// certificate or, if the certificate has no SAN extension, against its Common Name. A client
    /// configured without one skips the name check. A server doesn't check the client's name.
    AuthorityBased,
    /// Validates that the peer presents a single certificate which is a byte-for-byte match
    /// against the configured peer certificate.
    ///
    /// The certificate is parsed only to ensure that the `NotBefore` and `NotAfter`
    /// are valid for the current system time.
    SelfSigned,
}

/// TLS-related errors
#[derive(Debug)]
pub enum TlsError {
    /// Invalid peer certificate
    InvalidPeerCertificate(std::io::Error),
    /// Invalid local certificate
    InvalidLocalCertificate(std::io::Error),
    /// Invalid private key
    InvalidPrivateKey(std::io::Error),
    /// DNS name is invalid
    InvalidDnsName,
    /// Error building TLS configuration
    BadConfig(String),
}

impl std::fmt::Display for TlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPeerCertificate(err) => {
                write!(f, "invalid peer certificate file: {err}")
            }
            Self::InvalidLocalCertificate(err) => {
                write!(f, "invalid local certificate file: {err}")
            }
            Self::InvalidPrivateKey(err) => write!(f, "invalid private key file: {err}"),
            Self::InvalidDnsName => write!(f, "invalid DNS name"),
            Self::BadConfig(err) => write!(f, "bad config: {err}"),
        }
    }
}

impl std::error::Error for TlsError {}

impl From<sfio_rustls_config::Error> for TlsError {
    fn from(err: sfio_rustls_config::Error) -> Self {
        Self::BadConfig(err.to_string())
    }
}

/// Minimum TLS version to allow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum MinTlsVersion {
    /// Allow TLS 1.2 and TLS 1.3
    V1_2,
    /// Allow only TLS 1.3
    ///
    /// A peer that doesn't support TLS 1.3 fails the handshake.
    V1_3,
}

impl From<MinTlsVersion> for sfio_rustls_config::ProtocolVersions {
    fn from(value: MinTlsVersion) -> Self {
        match value {
            MinTlsVersion::V1_2 => sfio_rustls_config::ProtocolVersions::new()
                .enable_v12()
                .enable_v13(),
            MinTlsVersion::V1_3 => sfio_rustls_config::ProtocolVersions::v13_only(),
        }
    }
}

/// How long a TLS handshake may take, so that a peer that stalls its handshake can't hold up
/// whatever is waiting for it
pub(crate) const TLS_HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Why a TLS handshake didn't produce a session
#[derive(Debug)]
pub(crate) enum HandshakeError {
    /// The peer didn't complete the handshake within [`TLS_HANDSHAKE_TIMEOUT`]
    Timeout,
    /// The handshake failed
    Failed(std::io::Error),
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(
                f,
                "TLS handshake didn't complete within {} seconds",
                TLS_HANDSHAKE_TIMEOUT.as_secs()
            ),
            Self::Failed(err) => write!(f, "failed to establish TLS session: {err}"),
        }
    }
}

/// Run a handshake, giving up if it doesn't complete within [`TLS_HANDSHAKE_TIMEOUT`]
pub(crate) async fn handshake<S>(
    handshake: impl std::future::Future<Output = std::io::Result<S>>,
) -> Result<S, HandshakeError> {
    match tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, handshake).await {
        Err(_) => Err(HandshakeError::Timeout),
        Ok(Err(err)) => Err(HandshakeError::Failed(err)),
        Ok(Ok(stream)) => Ok(stream),
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use sfio_rustls_config::{ProtocolVersions, ServerNameVerification};
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio_rustls::rustls;
    use tokio_rustls::rustls::pki_types::ServerName;

    use super::*;

    /// One of the repository's self-signed test certificates or keys
    pub(crate) fn cert(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../certs/self_signed")
            .join(name)
    }

    /// One of the repository's test certificates or keys issued by its test authority
    pub(crate) fn ca_chain(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../certs/ca_chain")
            .join(name)
    }

    /// The rodbus server under test presents entity1 and expects entity2
    pub(crate) fn server_config(min: MinTlsVersion) -> TlsServerConfig {
        TlsServerConfig::new(
            &cert("entity2_cert.pem"),
            &cert("entity1_cert.pem"),
            &cert("entity1_key.pem"),
            None,
            min,
            CertificateMode::SelfSigned,
        )
        .unwrap()
    }

    /// The rodbus client under test presents entity2 and expects entity1
    pub(crate) fn client_config(min: MinTlsVersion) -> TlsClientConfig {
        TlsClientConfig::self_signed(
            &cert("entity1_cert.pem"),
            &cert("entity2_cert.pem"),
            &cert("entity2_key.pem"),
            None,
            min,
        )
        .unwrap()
    }

    /// A client that the server under test accepts, configured directly with `versions` so that
    /// rodbus's version mapping isn't involved
    pub(crate) fn peer_client_config(versions: ProtocolVersions) -> rustls::ClientConfig {
        sfio_rustls_config::client::self_signed(
            versions,
            &cert("entity1_cert.pem"),
            &cert("entity2_cert.pem"),
            &cert("entity2_key.pem"),
            None,
        )
        .unwrap()
    }

    /// Run a handshake over `socket` as a client offering `versions`
    pub(crate) async fn handshake_as_client<S: AsyncRead + AsyncWrite + Unpin>(
        socket: S,
        versions: ProtocolVersions,
    ) -> std::io::Result<tokio_rustls::client::TlsStream<S>> {
        // an IP address so that the client doesn't send SNI, as the rodbus client does
        let name = ServerName::IpAddress(IpAddr::V4(Ipv4Addr::UNSPECIFIED).into());
        tokio_rustls::TlsConnector::from(Arc::new(peer_client_config(versions)))
            .connect(name, socket)
            .await
    }

    pub(crate) fn both_versions() -> ProtocolVersions {
        ProtocolVersions::new().enable_v12().enable_v13()
    }

    /// The name in the certificate of [`authority_server`]
    const SERVER_NAME: &str = "test.com";

    /// The role in the certificate of [`operator`]
    pub(crate) const ROLE: &str = "operator";

    /// A server presenting the test authority's server certificate and accepting the clients it
    /// issued certificates to
    pub(crate) fn authority_server() -> TlsServerConfig {
        TlsServerConfig::new(
            &ca_chain("ca_cert.pem"),
            &ca_chain("server_cert.pem"),
            &ca_chain("server_key.pem"),
            None,
            MinTlsVersion::V1_2,
            CertificateMode::AuthorityBased,
        )
        .unwrap()
    }

    /// A client of [`authority_server`] presenting the certificate `cert` with key `key`
    fn authority_client(cert: &str, key: &str) -> rustls::ClientConfig {
        sfio_rustls_config::client::authority(
            both_versions(),
            ServerNameVerification::SanOrCommonName,
            &ca_chain("ca_cert.pem"),
            &ca_chain(cert),
            &ca_chain(key),
            None,
        )
        .unwrap()
    }

    /// A client whose certificate, issued by the test authority, has the role [`ROLE`]
    pub(crate) fn operator() -> rustls::ClientConfig {
        authority_client("client_cert.pem", "client_key.pem")
    }

    /// A client whose certificate, issued by the test authority, has no role
    pub(crate) fn without_a_role() -> rustls::ClientConfig {
        authority_client("server_cert.pem", "server_key.pem")
    }

    /// Run a handshake over `socket` as a client configured by `client`
    pub(crate) async fn handshake_with<S: AsyncRead + AsyncWrite + Unpin>(
        client: rustls::ClientConfig,
        socket: S,
    ) -> std::io::Result<tokio_rustls::client::TlsStream<S>> {
        let name = ServerName::try_from(SERVER_NAME).unwrap();
        tokio_rustls::TlsConnector::from(Arc::new(client))
            .connect(name, socket)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use sfio_rustls_config::ProtocolVersions;

    use super::test_util::{ca_chain, cert};
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_handshake_that_never_completes_times_out() {
        let start = tokio::time::Instant::now();
        let result = handshake(std::future::pending::<std::io::Result<()>>()).await;
        assert!(matches!(result, Err(HandshakeError::Timeout)));
        assert_eq!(start.elapsed(), TLS_HANDSHAKE_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_handshake_is_reported_at_once() {
        let start = tokio::time::Instant::now();
        let failure = std::io::ErrorKind::ConnectionReset;
        let result = handshake(async { std::io::Result::<()>::Err(failure.into()) }).await;
        assert!(matches!(result, Err(HandshakeError::Failed(err)) if err.kind() == failure));
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[test]
    fn minimum_v1_2_allows_both_versions() {
        assert_eq!(
            ProtocolVersions::from(MinTlsVersion::V1_2),
            ProtocolVersions::new().enable_v12().enable_v13()
        );
    }

    #[test]
    fn minimum_v1_3_allows_only_v1_3() {
        assert_eq!(
            ProtocolVersions::from(MinTlsVersion::V1_3),
            ProtocolVersions::v13_only()
        );
    }

    /// Each way to build a TLS configuration
    #[derive(Debug, Clone, Copy)]
    enum Constructor {
        ClientFullPki,
        ClientSelfSigned,
        Server(CertificateMode),
    }

    /// Each constructor with files it accepts: the peer certificate, the local certificate and
    /// the private key
    fn constructors() -> [(Constructor, [PathBuf; 3]); 4] {
        let pki_client = [
            ca_chain("ca_cert.pem"),
            ca_chain("client_cert.pem"),
            ca_chain("client_key.pem"),
        ];
        let pki_server = [
            ca_chain("ca_cert.pem"),
            ca_chain("server_cert.pem"),
            ca_chain("server_key.pem"),
        ];
        let self_signed = [
            cert("entity1_cert.pem"),
            cert("entity2_cert.pem"),
            cert("entity2_key.pem"),
        ];
        [
            (Constructor::ClientFullPki, pki_client),
            (Constructor::ClientSelfSigned, self_signed.clone()),
            (
                Constructor::Server(CertificateMode::AuthorityBased),
                pki_server,
            ),
            (
                Constructor::Server(CertificateMode::SelfSigned),
                self_signed,
            ),
        ]
    }

    fn build(constructor: Constructor, [peer, local, key]: &[PathBuf; 3]) -> Result<(), TlsError> {
        let min = MinTlsVersion::V1_2;
        match constructor {
            Constructor::ClientFullPki => {
                TlsClientConfig::full_pki(None, peer, local, key, None, min).map(drop)
            }
            Constructor::ClientSelfSigned => {
                TlsClientConfig::self_signed(peer, local, key, None, min).map(drop)
            }
            Constructor::Server(mode) => {
                TlsServerConfig::new(peer, local, key, None, min, mode).map(drop)
            }
        }
    }

    #[test]
    fn a_missing_file_is_a_bad_config_for_every_constructor() {
        for (constructor, files) in constructors() {
            assert!(build(constructor, &files).is_ok(), "{constructor:?}");
            for missing in 0..files.len() {
                let mut files = files.clone();
                files[missing] = cert("missing.pem");
                assert!(
                    matches!(build(constructor, &files), Err(TlsError::BadConfig(_))),
                    "{constructor:?} without file {missing}"
                );
            }
        }
    }

    #[test]
    fn a_server_name_that_isnt_a_dns_name_is_rejected() {
        let result = TlsClientConfig::full_pki(
            Some("not a name".to_string()),
            &ca_chain("ca_cert.pem"),
            &ca_chain("client_cert.pem"),
            &ca_chain("client_key.pem"),
            None,
            MinTlsVersion::V1_2,
        );
        assert!(matches!(result, Err(TlsError::InvalidDnsName)));
    }
}
