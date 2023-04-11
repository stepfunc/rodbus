use sfio_rustls_util::NameVerifier;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::Arc;

use tokio::net::TcpStream;
use tokio_rustls::rustls;

use crate::common::phys::PhysLayer;
use crate::server::task::AuthorizationType;
use crate::server::AuthorizationHandler;
use crate::tcp::tls::{CertificateMode, MinTlsVersion, TlsError};

/// TLS configuration
#[derive(Clone)]
pub struct TlsServerConfig {
    inner: Arc<rustls::ServerConfig>,
}

impl TlsServerConfig {
    /// Create a TLS server config
    pub fn new(
        peer_cert_path: &Path,
        local_cert_path: &Path,
        private_key_path: &Path,
        password: Option<&str>,
        min_tls_version: MinTlsVersion,
        certificate_mode: CertificateMode,
    ) -> Result<Self, TlsError> {
        let mut peer_certs: Vec<rustls::Certificate> = {
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

        let verifier: Arc<dyn rustls::server::ClientCertVerifier> = match certificate_mode {
            CertificateMode::AuthorityBased => {
                // Build root certificate store
                let mut roots = rustls::RootCertStore::empty();
                for cert in peer_certs.as_slice() {
                    roots.add(cert).map_err(|err| {
                        TlsError::InvalidPeerCertificate(io::Error::new(
                            ErrorKind::InvalidData,
                            err.to_string(),
                        ))
                    })?;
                }
                Arc::new(sfio_rustls_util::ClientCertVerifier::new(
                    roots,
                    NameVerifier::any(),
                ))
            }
            CertificateMode::SelfSigned => {
                if let Some(peer_cert) = peer_certs.pop() {
                    if !peer_certs.is_empty() {
                        return Err(TlsError::InvalidPeerCertificate(io::Error::new(
                            ErrorKind::InvalidData,
                            "more than one peer certificate in self-signed mode",
                        )));
                    }
                    let verifier = sfio_rustls_util::SelfSignedVerifier::create(peer_cert)?;
                    Arc::new(verifier)
                } else {
                    return Err(TlsError::InvalidPeerCertificate(io::Error::new(
                        ErrorKind::InvalidData,
                        "no peer certificate",
                    )));
                }
            }
        };

        let config = build_server_config(verifier, min_tls_version, local_certs, private_key)?;

        Ok(TlsServerConfig {
            inner: Arc::new(config),
        })
    }

    pub(crate) async fn handle_connection(
        &mut self,
        socket: TcpStream,
        auth_handler: Option<Arc<dyn AuthorizationHandler>>,
    ) -> Result<(PhysLayer, AuthorizationType), String> {
        let connector = tokio_rustls::TlsAcceptor::from(self.inner.clone());
        match connector.accept(socket).await {
            Err(err) => Err(format!("failed to establish TLS session: {err}")),
            Ok(stream) => {
                let auth_type = match auth_handler {
                    // bare TLS mode without authz
                    None => AuthorizationType::None,
                    // full secure modbus requires the client certificate contain a role
                    Some(handler) => {
                        // get the peer cert data
                        let peer_cert = stream
                            .get_ref()
                            .1
                            .peer_certificates()
                            .and_then(|x| x.first())
                            .ok_or_else(|| "No peer certificate".to_string())?
                            .0
                            .as_slice();

                        let parsed = rx509::x509::Certificate::parse(peer_cert)
                            .map_err(|err| format!("ASNError: {err}"))?;
                        let role = extract_modbus_role(&parsed).map_err(|err| format!("{err}"))?;

                        tracing::info!("client role: {}", role);
                        AuthorizationType::Handler(handler, role)
                    }
                };

                let layer = PhysLayer::new_tls(tokio_rustls::TlsStream::from(stream));

                Ok((layer, auth_type))
            }
        }
    }
}

fn build_server_config(
    verifier: Arc<dyn rustls::server::ClientCertVerifier>,
    min_tls_version: MinTlsVersion,
    local_certs: Vec<rustls::Certificate>,
    private_key: rustls::PrivateKey,
) -> Result<rustls::ServerConfig, TlsError> {
    let config = rustls::ServerConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(min_tls_version.to_rustls())
        .map_err(|err| TlsError::BadConfig(err.to_string()))?
        .with_client_cert_verifier(verifier)
        .with_single_cert(local_certs, private_key)
        .map_err(|err| TlsError::BadConfig(err.to_string()))?;

    Ok(config)
}

fn extract_modbus_role(cert: &rx509::x509::Certificate) -> Result<String, rustls::Error> {
    // Parse the extensions
    let extensions = cert
        .tbs_certificate
        .value
        .extensions
        .as_ref()
        .ok_or_else(|| {
            rustls::Error::General("certificate doesn't contain Modbus role extension".to_string())
        })?;

    let extensions = extensions.parse().map_err(|err| {
        rustls::Error::General(format!(
            "unable to parse cert extensions with rasn: {err:?}"
        ))
    })?;

    // Extract the ModbusRole extensions
    let mut it = extensions.into_iter().filter_map(|ext| match ext.content {
        rx509::x509::ext::SpecificExtension::ModbusRole(role) => Some(role.role),
        _ => None,
    });

    // Extract the first ModbusRole extension
    let role = it.next().ok_or_else(|| {
        rustls::Error::General("certificate doesn't have Modbus extension".to_string())
    })?;

    // Check that there is only one role extension
    if it.next().is_some() {
        return Err(rustls::Error::General(
            "certificate has more than one Modbus extension".to_string(),
        ));
    }

    Ok(role.to_string())
}
