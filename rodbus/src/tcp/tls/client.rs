use std::convert::TryFrom;
use std::net::Ipv4Addr;

use sfio_rustls_config::NameVerifier;
use std::path::Path;
use std::sync::Arc;

use tokio::net::TcpStream;
use tokio_rustls::rustls;
use tracing::Instrument;

use crate::client::{Channel, ClientState, HostAddr, Listener, RetryStrategy};
use crate::common::phys::PhysLayer;
use crate::tcp::client::{TcpChannelTask, TcpTaskConnectionHandler};
use crate::tcp::tls::{CertificateMode, MinTlsVersion, TlsError};

use crate::DecodeLevel;

/// TLS configuration
pub struct TlsClientConfig {
    server_name: rustls::ServerName,
    config: Arc<rustls::ClientConfig>,
}

pub(crate) fn spawn_tls_channel(
    host: HostAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn RetryStrategy>,
    tls_config: TlsClientConfig,
    decode: DecodeLevel,
    listener: Box<dyn Listener<ClientState>>,
) -> Channel {
    let (handle, task) = create_tls_channel(
        host,
        max_queued_requests,
        connect_retry,
        tls_config,
        decode,
        listener,
    );
    tokio::spawn(task);
    handle
}

pub(crate) fn create_tls_channel(
    host: HostAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn RetryStrategy>,
    tls_config: TlsClientConfig,
    decode: DecodeLevel,
    listener: Box<dyn Listener<ClientState>>,
) -> (Channel, impl std::future::Future<Output = ()>) {
    let (tx, rx) = tokio::sync::mpsc::channel(max_queued_requests);
    let task = async move {
        TcpChannelTask::new(
            host.clone(),
            rx.into(),
            TcpTaskConnectionHandler::Tls(tls_config),
            connect_retry,
            decode,
            listener,
        )
        .run()
        .instrument(tracing::info_span!("Modbus-Client-TCP", endpoint = ?host))
        .await;
    };
    (Channel { tx }, task)
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
                NameVerifier::any(),
                rustls::ServerName::IpAddress(Ipv4Addr::UNSPECIFIED.into()),
            ),
            Some(x) => {
                let server_name = rustls::ServerName::try_from(x.as_str())?;
                (NameVerifier::equal_to(x), server_name)
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
            server_name: rustls::ServerName::IpAddress(Ipv4Addr::UNSPECIFIED.into()),
            config: Arc::new(config),
        })
    }

    pub(crate) async fn handle_connection(
        &mut self,
        socket: TcpStream,
        endpoint: &HostAddr,
    ) -> Result<PhysLayer, String> {
        let connector = tokio_rustls::TlsConnector::from(self.config.clone());
        match connector.connect(self.server_name.clone(), socket).await {
            Err(err) => Err(format!(
                "failed to establish TLS session with {endpoint}: {err}"
            )),
            Ok(stream) => Ok(PhysLayer::new_tls(tokio_rustls::TlsStream::from(stream))),
        }
    }
}
