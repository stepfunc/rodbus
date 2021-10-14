use std::convert::TryFrom;
use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use tokio_rustls::rustls;
use tracing::Instrument;

use crate::client::{Channel, ReconnectStrategy};
use crate::common::phys::PhysLayer;
use crate::tcp::client::{TcpChannelTask, TcpTaskConnectionHandler};
use crate::tcp::tls::{load_certs, load_private_key, CertificateMode, MinTlsVersion, TlsError};
use crate::tokio;
use crate::tokio::net::TcpStream;
use crate::{DecodeLevel, PhysDecodeLevel};

/// TLS configuration
pub struct TlsClientConfig {
    dns_name: rustls::ServerName,
    config: Arc<rustls::ClientConfig>,
}

pub(crate) fn spawn_tls_channel(
    addr: SocketAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    tls_config: TlsClientConfig,
    decode: DecodeLevel,
) -> Channel {
    let (handle, task) =
        create_tls_channel(addr, max_queued_requests, connect_retry, tls_config, decode);
    tokio::spawn(task);
    handle
}

pub(crate) fn create_tls_channel(
    addr: SocketAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    tls_config: TlsClientConfig,
    decode: DecodeLevel,
) -> (Channel, impl std::future::Future<Output = ()>) {
    let (tx, rx) = tokio::sync::mpsc::channel(max_queued_requests);
    let task = async move {
        TcpChannelTask::new(
            addr,
            rx,
            TcpTaskConnectionHandler::Tls(tls_config),
            connect_retry,
            decode,
        )
        .run()
        .instrument(tracing::info_span!("Modbus-Client-TCP", endpoint = ?addr))
        .await
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
        min_tls_version: MinTlsVersion,
        certificate_mode: CertificateMode,
    ) -> Result<Self, TlsError> {
        let mut peer_certs = load_certs(peer_cert_path, false)?;
        let local_certs = load_certs(local_cert_path, true)?;
        let private_key = load_private_key(private_key_path)?;

        let builder = rustls::ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(min_tls_version.to_rustls())
            .expect("cipher suites or kx groups mismatch with TLS version");

        let config = match certificate_mode {
            CertificateMode::TrustChain => {
                let mut root_store = rustls::RootCertStore::empty();
                for cert in &peer_certs {
                    root_store.add(cert).map_err(|err| {
                        TlsError::InvalidPeerCertificate(io::Error::new(
                            ErrorKind::InvalidData,
                            err.to_string(),
                        ))
                    })?;
                }
                builder
                    .with_root_certificates(root_store)
                    .with_single_cert(local_certs, private_key)
            }
            CertificateMode::SelfSignedCertificate => {
                // Set the custom certificate verifier
                if let Some(peer_cert) = peer_certs.pop() {
                    if !peer_certs.is_empty() {
                        return Err(TlsError::InvalidPeerCertificate(io::Error::new(
                            ErrorKind::InvalidData,
                            "more than one peer certificate in self-signed mode",
                        )));
                    }

                    builder
                        .with_custom_certificate_verifier(Arc::new(
                            SelfSignedCertificateServerCertVerifier::new(peer_cert),
                        ))
                        .with_single_cert(local_certs, private_key)
                } else {
                    return Err(TlsError::InvalidPeerCertificate(io::Error::new(
                        ErrorKind::InvalidData,
                        "no peer certificate",
                    )));
                }
            }
        }
        .map_err(|err| {
            TlsError::InvalidLocalCertificate(io::Error::new(
                ErrorKind::InvalidData,
                err.to_string(),
            ))
        })?;

        let dns_name = rustls::ServerName::try_from(name).map_err(|_| TlsError::InvalidDnsName)?;

        Ok(Self {
            config: std::sync::Arc::new(config),
            dns_name,
        })
    }

    pub(crate) async fn handle_connection(
        &mut self,
        socket: TcpStream,
        endpoint: &SocketAddr,
        level: PhysDecodeLevel,
    ) -> Result<PhysLayer, String> {
        let connector = tokio_rustls::TlsConnector::from(self.config.clone());
        match connector.connect(self.dns_name.clone(), socket).await {
            Err(err) => Err(format!(
                "failed to establish TLS session with {}: {}",
                endpoint, err
            )),
            Ok(stream) => Ok(PhysLayer::new_tls(
                tokio_rustls::TlsStream::from(stream),
                level,
            )),
        }
    }
}

struct SelfSignedCertificateServerCertVerifier {
    cert: rustls::Certificate,
}

impl SelfSignedCertificateServerCertVerifier {
    fn new(cert: rustls::Certificate) -> Self {
        Self { cert }
    }
}

impl rustls::client::ServerCertVerifier for SelfSignedCertificateServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::Certificate,
        intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        // Check that no intermediate certificates are present
        if !intermediates.is_empty() {
            return Err(rustls::Error::General(format!(
                "client sent {} intermediate certificates, expected none",
                intermediates.len()
            )));
        }

        // Check that presented certificate matches byte-for-byte the expected certificate
        if end_entity != &self.cert {
            return Err(rustls::Error::InvalidCertificateData(
                "client certificate doesn't match the expected self-signed certificate".to_string(),
            ));
        }

        // Check that the certificate is still valid
        let parsed_cert = rasn::x509::Certificate::parse(&end_entity.0).map_err(|err| {
            rustls::Error::InvalidCertificateData(format!(
                "unable to parse cert with rasn: {:?}",
                err
            ))
        })?;

        let now = now
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| rustls::Error::FailedToGetCurrentTime)?;
        let now = rasn::types::UtcTime::from_seconds_since_epoch(now.as_secs());

        if !parsed_cert.tbs_certificate.value.validity.is_valid(now) {
            return Err(rustls::Error::InvalidCertificateData(
                "self-signed certificate is currently not valid".to_string(),
            ));
        }

        // We do not validate DNS name. Providing the exact same certificate is sufficient.

        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
