// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::convert::TryFrom;
use std::net::Ipv4Addr;

use sfio_rustls_config::ServerNameVerification;
use std::path::Path;
use std::sync::Arc;

use tokio_rustls::rustls;
use tokio_rustls::rustls::pki_types::InvalidDnsNameError;
use tracing::Instrument;

use crate::client::{Channel, ClientState, ClientTask, HostAddr, Listener, RetryStrategy};
use crate::common::phys::{PhysLayer, Socket};
use crate::tcp::client::{new_channel, TcpTaskConnectionHandler};
use crate::tcp::tls::{
    handshake, CertificateMode, HandshakeError, MinTlsVersion, TlsError, TLS_HANDSHAKE_TIMEOUT,
};

use crate::ClientOptions;

/// TLS configuration
pub struct TlsClientConfig {
    server_name: rustls::pki_types::ServerName<'static>,
    config: Arc<rustls::ClientConfig>,
}

impl std::fmt::Debug for TlsClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsClientConfig")
            .field("server_name", &self.server_name)
            .finish_non_exhaustive()
    }
}

pub(crate) fn spawn_tls_channel(
    host: HostAddr,
    connect_retry: Box<dyn RetryStrategy>,
    tls_config: TlsClientConfig,
    options: ClientOptions,
    listener: Box<dyn Listener<ClientState>>,
) -> Channel {
    let span = tracing::info_span!("Modbus-Client-TLS", endpoint = ?host);
    let (handle, task) = create_tls_channel(host, connect_retry, tls_config, options, listener);
    tokio::spawn(task.run().instrument(span));
    handle
}

pub(crate) fn create_tls_channel(
    host: HostAddr,
    connect_retry: Box<dyn RetryStrategy>,
    tls_config: TlsClientConfig,
    options: ClientOptions,
    listener: Box<dyn Listener<ClientState>>,
) -> (Channel, ClientTask) {
    let (channel, task, signal) = new_channel(
        host,
        TcpTaskConnectionHandler::Tls(tls_config),
        connect_retry,
        options,
        listener,
    );
    (channel, ClientTask::tcp(task, signal))
}

impl TlsClientConfig {
    /// Legacy method for creating a client TLS configuration
    #[deprecated(
        since = "1.3.0",
        note = "Please use `full_pki` or `self_signed` instead"
    )]
    pub fn new(
        server_name: &str,
        peer_cert_path: &Path,
        local_cert_path: &Path,
        private_key_path: &Path,
        password: Option<&str>,
        min_tls_version: MinTlsVersion,
        certificate_mode: CertificateMode,
    ) -> Result<Self, TlsError> {
        match certificate_mode {
            CertificateMode::AuthorityBased => Self::full_pki(
                Some(server_name.to_string()),
                peer_cert_path,
                local_cert_path,
                private_key_path,
                password,
                min_tls_version,
            ),
            CertificateMode::SelfSigned => Self::self_signed(
                peer_cert_path,
                local_cert_path,
                private_key_path,
                password,
                min_tls_version,
            ),
        }
    }

    /// Create a TLS client configuration that expects a full PKI with an authority, and possibly
    /// intermediate CA certificates.
    ///
    /// If `server_subject_name` is specified, than the client will verify that the name is present in the
    /// SAN extension or in the Common Name of the client certificate.
    ///
    /// If `server_subject_name` is set to None, then no server name validation is performed, and
    /// any authenticated server is allowed.
    pub fn full_pki(
        server_subject_name: Option<String>,
        peer_cert_path: &Path,
        local_cert_path: &Path,
        private_key_path: &Path,
        password: Option<&str>,
        min_tls_version: MinTlsVersion,
    ) -> Result<Self, TlsError> {
        let (name_verifier, server_name) = match server_subject_name {
            None => (
                ServerNameVerification::DisableNameVerification,
                rustls::pki_types::ServerName::IpAddress(rustls::pki_types::IpAddr::V4(
                    Ipv4Addr::UNSPECIFIED.into(),
                )),
            ),
            Some(x) => {
                let server_name = rustls::pki_types::ServerName::try_from(x)?;
                (ServerNameVerification::SanOrCommonName, server_name)
            }
        };

        let config = sfio_rustls_config::client::authority(
            min_tls_version.into(),
            name_verifier,
            peer_cert_path,
            local_cert_path,
            private_key_path,
            password,
        )?;

        Ok(Self {
            server_name,
            config: Arc::new(config),
        })
    }

    /// Create a TLS client configuration that expects the client to present a single certificate.
    ///
    /// In lieu of performing server subject name validation, the client validates:
    ///
    /// 1) That the server presents a single certificate
    /// 2) That the certificate is a byte-for-byte match with the one loaded in `peer_cert_path`.
    /// 3) That the certificate's Validity (not before / not after) is currently valid.
    ///
    pub fn self_signed(
        peer_cert_path: &Path,
        local_cert_path: &Path,
        private_key_path: &Path,
        password: Option<&str>,
        min_tls_version: MinTlsVersion,
    ) -> Result<Self, TlsError> {
        let config = sfio_rustls_config::client::self_signed(
            min_tls_version.into(),
            peer_cert_path,
            local_cert_path,
            private_key_path,
            password,
        )?;

        Ok(Self {
            //  it doesn't matter what we put here, it just needs to be an IP so that the client won't send an SNI extension
            server_name: rustls::pki_types::ServerName::IpAddress(rustls::pki_types::IpAddr::V4(
                Ipv4Addr::UNSPECIFIED.into(),
            )),
            config: Arc::new(config),
        })
    }

    pub(crate) async fn handle_connection<S: Socket>(
        &self,
        socket: S,
        endpoint: &impl std::fmt::Display,
    ) -> std::io::Result<PhysLayer> {
        match self.connect(socket).await {
            Err(HandshakeError::Timeout) => {
                tracing::warn!(
                    "TLS handshake with {} didn't complete within {} seconds",
                    endpoint,
                    TLS_HANDSHAKE_TIMEOUT.as_secs()
                );
                Err(std::io::ErrorKind::TimedOut.into())
            }
            Err(HandshakeError::Failed(err)) => {
                let msg = format!("TLS error ({endpoint}): {err}");
                Err(std::io::Error::new(err.kind(), msg))
            }
            Ok(stream) => Ok(S::into_tls_phys(tokio_rustls::TlsStream::from(stream))),
        }
    }

    /// Run the client side of the TLS handshake over a connected socket
    pub(super) async fn connect<S>(
        &self,
        socket: S,
    ) -> Result<tokio_rustls::client::TlsStream<S>, HandshakeError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let connector = tokio_rustls::TlsConnector::from(self.config.clone());
        handshake(connector.connect(self.server_name.clone(), socket)).await
    }
}

impl From<InvalidDnsNameError> for TlsError {
    fn from(_: InvalidDnsNameError) -> Self {
        TlsError::InvalidDnsName
    }
}
