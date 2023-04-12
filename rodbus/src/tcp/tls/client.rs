use std::convert::TryFrom;

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
        let dns_name = rustls::ServerName::try_from(name).map_err(|_| TlsError::InvalidDnsName)?;

        let config = match certificate_mode {
            CertificateMode::SelfSigned => sfio_rustls_config::client::self_signed(
                min_tls_version.into(),
                peer_cert_path,
                local_cert_path,
                private_key_path,
                password,
            )?,
            CertificateMode::AuthorityBased => sfio_rustls_config::client::authority(
                min_tls_version.into(),
                NameVerifier::equal_to(name.to_string()),
                peer_cert_path,
                local_cert_path,
                private_key_path,
                password,
            )?,
        };

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
