// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

pub(crate) mod client;
pub(crate) mod server;

pub(crate) use client::*;
pub(crate) use server::*;

/// Determines how the certificate(s) presented by the peer are validated
///
/// This validation always occurs **after** the handshake signature has been
/// verified.
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
    /// The name verification is relaxed to allow for certificates that do not contain
    /// the SAN extension. In these cases the name is verified using the Common Name instead.
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
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use sfio_rustls_config::ProtocolVersions;
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio::net::TcpStream;
    use tokio_rustls::rustls;
    use tokio_rustls::rustls::pki_types::ServerName;

    use super::*;

    /// One of the repository's self-signed test certificates or keys
    pub(crate) fn cert(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../certs/self_signed")
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

    /// Connect to `addr` and run a handshake as a client offering `versions`
    pub(crate) async fn connect(
        addr: SocketAddr,
        versions: ProtocolVersions,
    ) -> std::io::Result<tokio_rustls::client::TlsStream<TcpStream>> {
        handshake_as_client(TcpStream::connect(addr).await?, versions).await
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
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::time::Duration;

    use sfio_rustls_config::ProtocolVersions;
    use tokio::net::{TcpListener, TcpStream};
    use tokio_rustls::rustls;
    use tokio_rustls::rustls::{AlertDescription, PeerIncompatible, ProtocolVersion};

    use super::test_util::*;
    use super::*;
    use crate::client::HostAddr;

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test(start_paused = true)]
    async fn a_handshake_that_never_completes_times_out() {
        let start = tokio::time::Instant::now();
        let result = handshake(std::future::pending::<std::io::Result<()>>()).await;
        assert!(matches!(result, Err(HandshakeError::Timeout)));
        assert_eq!(start.elapsed(), TLS_HANDSHAKE_TIMEOUT);
    }

    #[tokio::test]
    async fn a_failed_handshake_is_reported_at_once() {
        let start = tokio::time::Instant::now();
        let failure = std::io::ErrorKind::ConnectionReset;
        let result = handshake(async { std::io::Result::<()>::Err(failure.into()) }).await;
        assert!(matches!(result, Err(HandshakeError::Failed(err)) if err.kind() == failure));
        assert!(start.elapsed() < TLS_HANDSHAKE_TIMEOUT);
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

    #[tokio::test]
    async fn server_refuses_a_client_below_its_minimum() {
        let (server_ok, client) =
            server_handshake(MinTlsVersion::V1_3, ProtocolVersions::v12_only()).await;
        assert!(!server_ok);
        assert_eq!(
            client,
            Err(rustls::Error::AlertReceived(
                AlertDescription::ProtocolVersion
            ))
        );
    }

    #[tokio::test]
    async fn client_refuses_a_server_below_its_minimum() {
        let (client_ok, server) =
            client_handshake(MinTlsVersion::V1_3, ProtocolVersions::v12_only()).await;
        assert!(!client_ok);
        assert_eq!(
            server,
            Err(rustls::Error::PeerIncompatible(
                PeerIncompatible::Tls12NotOfferedOrEnabled
            ))
        );
    }

    #[tokio::test]
    async fn server_with_minimum_v1_2_negotiates_v1_3_with_a_capable_client() {
        let (server_ok, client) = server_handshake(MinTlsVersion::V1_2, both_versions()).await;
        assert!(server_ok);
        assert_eq!(client, Ok(ProtocolVersion::TLSv1_3));
    }

    #[tokio::test]
    async fn client_with_minimum_v1_2_negotiates_v1_3_with_a_capable_server() {
        let (client_ok, server) = client_handshake(MinTlsVersion::V1_2, both_versions()).await;
        assert!(client_ok);
        assert_eq!(server, Ok(ProtocolVersion::TLSv1_3));
    }

    /// Handshake between the rodbus server configured with `min` and a client offering `versions`.
    /// Returns whether the server completed it, and the client's outcome
    async fn server_handshake(
        min: MinTlsVersion,
        versions: ProtocolVersions,
    ) -> (bool, Result<ProtocolVersion, rustls::Error>) {
        within(async {
            let (listener, addr) = bind().await;
            let client = tokio::spawn(peer_client(addr, versions));
            let (socket, _) = listener.accept().await.unwrap();
            let server = server_config(min).handle_connection(socket, None).await;
            (server.is_ok(), client.await.unwrap())
        })
        .await
    }

    /// Handshake between the rodbus client configured with `min` and a server offering `versions`.
    /// Returns whether the client completed it, and the server's outcome
    async fn client_handshake(
        min: MinTlsVersion,
        versions: ProtocolVersions,
    ) -> (bool, Result<ProtocolVersion, rustls::Error>) {
        within(async {
            let (listener, addr) = bind().await;
            let server = tokio::spawn(peer_server(listener, versions));
            let socket = TcpStream::connect(addr).await.unwrap();
            let client = client_config(min)
                .handle_connection(socket, &HostAddr::ip(addr.ip(), addr.port()))
                .await;
            (client.is_ok(), server.await.unwrap())
        })
        .await
    }

    /// A client configured directly with `versions`, so the mapping under test isn't involved
    async fn peer_client(
        addr: SocketAddr,
        versions: ProtocolVersions,
    ) -> Result<ProtocolVersion, rustls::Error> {
        let stream = connect(addr, versions).await.map_err(tls_error)?;
        Ok(stream.get_ref().1.protocol_version().unwrap())
    }

    /// A server configured directly with `versions`, so the mapping under test isn't involved
    async fn peer_server(
        listener: TcpListener,
        versions: ProtocolVersions,
    ) -> Result<ProtocolVersion, rustls::Error> {
        let config = sfio_rustls_config::server::self_signed(
            versions,
            &cert("entity2_cert.pem"),
            &cert("entity1_cert.pem"),
            &cert("entity1_key.pem"),
            None,
        )
        .unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let stream = tokio_rustls::TlsAcceptor::from(Arc::new(config))
            .accept(socket)
            .await
            .map_err(tls_error)?;
        Ok(stream.get_ref().1.protocol_version().unwrap())
    }

    /// tokio-rustls reports TLS failures as an `io::Error` wrapping the `rustls::Error`
    fn tls_error(err: std::io::Error) -> rustls::Error {
        *err.into_inner()
            .and_then(|inner| inner.downcast::<rustls::Error>().ok())
            .expect("a TLS error")
    }

    async fn bind() -> (TcpListener, SocketAddr) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        (listener, addr)
    }

    async fn within<T>(future: impl Future<Output = T>) -> T {
        tokio::time::timeout(TIMEOUT, future)
            .await
            .expect("the handshake didn't finish in time")
    }
}
