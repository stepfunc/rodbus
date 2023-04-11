use std::convert::TryFrom;
use std::io::{self, ErrorKind};
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
    dns_name: rustls::ServerName,
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
            rx,
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
    /// Create a TLS master config
    pub fn new(
        name: &str,
        peer_cert_path: &Path,
        local_cert_path: &Path,
        private_key_path: &Path,
        password: Option<&str>,
        min_tls_version: MinTlsVersion,
        certificate_mode: CertificateMode,
    ) -> Result<Self, TlsError> {
        let peer_certs: Vec<rustls::Certificate> = {
            let bytes = std::fs::read(peer_cert_path)?;
            let certs = sfio_pem_util::read_certificates(bytes)?;
            certs.into_iter().map(rustls::Certificate).collect()
        };

        let local_certs = {
            let bytes = std::fs::read(local_cert_path)?;
            let certs = sfio_pem_util::read_certificates(bytes)?;
            certs.into_iter().map(rustls::Certificate).collect()
        };

        let private_key = {
            let bytes = std::fs::read(private_key_path)?;
            let key = match password {
                Some(x) => sfio_pem_util::PrivateKey::decrypt_from_pem(bytes, x),
                None => sfio_pem_util::PrivateKey::read_from_pem(bytes),
            }?;
            rustls::PrivateKey(key.bytes().to_vec())
        };

        let builder = rustls::ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(min_tls_version.to_rustls())
            .map_err(|err| TlsError::BadConfig(err.to_string()))?;

        let verifier: Arc<dyn rustls::client::ServerCertVerifier> = match certificate_mode {
            CertificateMode::AuthorityBased => {
                let verifier = sfio_rustls_util::ServerCertVerifier::new(
                    peer_certs,
                    sfio_rustls_util::NameVerifier::equal_to(name.to_string()),
                )?;
                Arc::new(verifier)
            }
            CertificateMode::SelfSigned => {
                let peer_cert = super::expect_single_peer_cert(peer_certs)?;
                let verifier = sfio_rustls_util::SelfSignedVerifier::create(peer_cert)?;
                Arc::new(verifier)
            }
        };

        let config = builder
            .with_custom_certificate_verifier(verifier)
            .with_single_cert(local_certs, private_key)
            .map_err(|err| {
                TlsError::InvalidLocalCertificate(io::Error::new(
                    ErrorKind::InvalidData,
                    err.to_string(),
                ))
            })?;

        let dns_name = rustls::ServerName::try_from(name).map_err(|_| TlsError::InvalidDnsName)?;

        Ok(Self {
            config: Arc::new(config),
            dns_name,
        })
    }

    pub(crate) async fn handle_connection(
        &mut self,
        socket: TcpStream,
        endpoint: &HostAddr,
    ) -> Result<PhysLayer, String> {
        let connector = tokio_rustls::TlsConnector::from(self.config.clone());
        match connector.connect(self.dns_name.clone(), socket).await {
            Err(err) => Err(format!(
                "failed to establish TLS session with {endpoint}: {err}"
            )),
            Ok(stream) => Ok(PhysLayer::new_tls(tokio_rustls::TlsStream::from(stream))),
        }
    }
}
