use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tokio_rustls::rustls;
use tokio_rustls::rustls::server::AllowAnyAuthenticatedClient;

use crate::common::phys::PhysLayer;
use crate::server::task::SessionAuthentication;
use crate::server::AuthorizationHandlerType;
use crate::tcp::tls::{load_certs, load_private_key, CertificateMode, MinTlsVersion, TlsError};
use crate::tokio::net::TcpStream;
use crate::PhysDecodeLevel;

type RoleContainer = Arc<Mutex<Option<String>>>;
type ConfigBuilderCallback =
    Arc<dyn Fn(RoleContainer) -> rustls::ServerConfig + Send + Sync + 'static>;

/// TLS configuration
#[derive(Clone)]
pub struct TlsServerConfig {
    config_builder: ConfigBuilderCallback,
}

impl TlsServerConfig {
    /// Create a TLS server config
    pub fn new(
        peer_cert_path: &Path,
        local_cert_path: &Path,
        private_key_path: &Path,
        min_tls_version: MinTlsVersion,
        certificate_mode: CertificateMode,
    ) -> Result<Self, TlsError> {
        let mut peer_certs = load_certs(peer_cert_path, false)?;
        let local_certs = load_certs(local_cert_path, true)?;
        let private_key = load_private_key(private_key_path)?;

        let config_builder: ConfigBuilderCallback = match certificate_mode {
            CertificateMode::TrustChain => {
                // Build root certificate store
                let mut roots = rustls::RootCertStore::empty();
                for cert in &peer_certs {
                    roots.add(cert).map_err(|err| {
                        TlsError::InvalidPeerCertificate(io::Error::new(
                            ErrorKind::InvalidData,
                            err.to_string(),
                        ))
                    })?;
                }

                Arc::new(move |role_container| {
                    let verifier = CaChainClientCertVerifier::new(roots.clone(), role_container);

                    rustls::ServerConfig::builder()
                        .with_safe_default_cipher_suites()
                        .with_safe_default_kx_groups()
                        .with_protocol_versions(min_tls_version.to_rustls())
                        .expect("cipher suites or kx groups mismatch with TLS version")
                        .with_client_cert_verifier(verifier)
                        .with_single_cert(local_certs.clone(), private_key.clone())
                        .map_err(|err| {
                            TlsError::InvalidLocalCertificate(io::Error::new(
                                ErrorKind::InvalidData,
                                err.to_string(),
                            ))
                        })
                        .unwrap()
                })
            }
            CertificateMode::SelfSignedCertificate => {
                if let Some(peer_cert) = peer_certs.pop() {
                    if !peer_certs.is_empty() {
                        return Err(TlsError::InvalidPeerCertificate(io::Error::new(
                            ErrorKind::InvalidData,
                            "more than one peer certificate in self-signed mode",
                        )));
                    }

                    Arc::new(move |role_container| {
                        let verifier = SelfSignedCertificateClientCertVerifier::new(
                            peer_cert.clone(),
                            role_container,
                        );

                        rustls::ServerConfig::builder()
                            .with_safe_default_cipher_suites()
                            .with_safe_default_kx_groups()
                            .with_protocol_versions(min_tls_version.to_rustls())
                            .expect("cipher suites or kx groups mismatch with TLS version")
                            .with_client_cert_verifier(verifier)
                            .with_single_cert(local_certs.clone(), private_key.clone())
                            .map_err(|err| {
                                TlsError::InvalidLocalCertificate(io::Error::new(
                                    ErrorKind::InvalidData,
                                    err.to_string(),
                                ))
                            })
                            .unwrap()
                    })
                } else {
                    return Err(TlsError::InvalidPeerCertificate(io::Error::new(
                        ErrorKind::InvalidData,
                        "no peer certificate",
                    )));
                }
            }
        };

        Ok(TlsServerConfig { config_builder })
    }

    fn build(&self, role_container: RoleContainer) -> Arc<rustls::ServerConfig> {
        Arc::new((self.config_builder)(role_container))
    }

    pub(crate) async fn handle_connection(
        &mut self,
        socket: TcpStream,
        level: PhysDecodeLevel,
        auth_handler: AuthorizationHandlerType,
    ) -> Result<(PhysLayer, SessionAuthentication), String> {
        let role_container = Arc::new(Mutex::new(None));
        let tls_config = self.build(role_container.clone());

        let connector = tokio_rustls::TlsAcceptor::from(tls_config);
        match connector.accept(socket).await {
            Err(err) => Err(format!("failed to establish TLS session: {}", err)),
            Ok(stream) => {
                let layer = PhysLayer::new_tls(tokio_rustls::TlsStream::from(stream), level);
                let role = role_container
                    .lock()
                    .unwrap()
                    .clone()
                    .ok_or_else(|| "client did not present Modbus role".to_string())?;

                Ok((
                    layer,
                    SessionAuthentication::Authenticated(auth_handler, role),
                ))
            }
        }
    }
}

struct CaChainClientCertVerifier {
    inner: Arc<dyn rustls::server::ClientCertVerifier>,
    role_container: RoleContainer,
}

impl CaChainClientCertVerifier {
    #[allow(clippy::new_ret_no_self)]
    fn new(
        roots: rustls::RootCertStore,
        role_container: RoleContainer,
    ) -> Arc<dyn rustls::server::ClientCertVerifier> {
        let inner = AllowAnyAuthenticatedClient::new(roots);
        Arc::new(CaChainClientCertVerifier {
            inner,
            role_container,
        })
    }
}

impl rustls::server::ClientCertVerifier for CaChainClientCertVerifier {
    fn offer_client_auth(&self) -> bool {
        // Client must authenticate itself, so we better offer the authentication!
        true
    }

    fn client_auth_mandatory(&self) -> Option<bool> {
        // Client must authenticate itself
        Some(true)
    }

    fn client_auth_root_subjects(&self) -> Option<rustls::DistinguishedNames> {
        self.inner.client_auth_root_subjects()
    }

    fn verify_client_cert(
        &self,
        end_entity: &rustls::Certificate,
        intermediates: &[rustls::Certificate],
        now: std::time::SystemTime,
    ) -> Result<rustls::server::ClientCertVerified, rustls::Error> {
        self.inner
            .verify_client_cert(end_entity, intermediates, now)?;

        // Extract Modbus Role ID
        let parsed_cert = rasn::x509::Certificate::parse(&end_entity.0).map_err(|err| {
            rustls::Error::InvalidCertificateData(format!(
                "unable to parse cert with rasn: {:?}",
                err
            ))
        })?;
        let role = extract_modbus_role(&parsed_cert)?;
        self.role_container.lock().unwrap().replace(role);

        Ok(rustls::server::ClientCertVerified::assertion())
    }
}

struct SelfSignedCertificateClientCertVerifier {
    cert: rustls::Certificate,
    role_container: RoleContainer,
}

impl SelfSignedCertificateClientCertVerifier {
    #[allow(clippy::new_ret_no_self)]
    fn new(
        cert: rustls::Certificate,
        role_container: RoleContainer,
    ) -> Arc<dyn rustls::server::ClientCertVerifier> {
        Arc::new(SelfSignedCertificateClientCertVerifier {
            cert,
            role_container,
        })
    }
}

impl rustls::server::ClientCertVerifier for SelfSignedCertificateClientCertVerifier {
    fn offer_client_auth(&self) -> bool {
        // Client must authenticate itself, so we better offer the authentication!
        true
    }

    fn client_auth_mandatory(&self) -> Option<bool> {
        // Client must authenticate itself
        Some(true)
    }

    fn client_auth_root_subjects(&self) -> Option<rustls::DistinguishedNames> {
        // Let rustls extract the subjects
        let mut store = rustls::RootCertStore::empty();
        let _ = store.add(&self.cert);
        Some(store.subjects())
    }

    fn verify_client_cert(
        &self,
        end_entity: &rustls::Certificate,
        intermediates: &[rustls::Certificate],
        now: std::time::SystemTime,
    ) -> Result<rustls::server::ClientCertVerified, rustls::Error> {
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

        let parsed_cert = rasn::x509::Certificate::parse(&end_entity.0).map_err(|err| {
            rustls::Error::InvalidCertificateData(format!(
                "unable to parse cert with rasn: {:?}",
                err
            ))
        })?;

        // Extract Modbus Role ID
        let role = extract_modbus_role(&parsed_cert)?;
        self.role_container.lock().unwrap().replace(role);

        // Check that the certificate is still valid
        let now = now
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| rustls::Error::FailedToGetCurrentTime)?;
        let now = rasn::types::UtcTime::from_seconds_since_epoch(now.as_secs());

        if !parsed_cert.tbs_certificate.value.validity.is_valid(now) {
            return Err(rustls::Error::InvalidCertificateData(
                "self-signed certificate is currently not valid".to_string(),
            ));
        }

        Ok(rustls::server::ClientCertVerified::assertion())
    }
}

fn extract_modbus_role(cert: &rasn::x509::Certificate) -> Result<String, rustls::Error> {
    // Parse the extensions
    let extensions = cert
        .tbs_certificate
        .value
        .extensions
        .as_ref()
        .ok_or_else(|| {
            rustls::Error::InvalidCertificateData(
                "certificate doesn't have Modbus extension".to_string(),
            )
        })?;
    let extensions = extensions.parse().map_err(|err| {
        rustls::Error::InvalidCertificateData(format!(
            "unable to parse cert extensions with rasn: {:?}",
            err
        ))
    })?;

    // Extract the ModbusRole extensions
    let mut it = extensions.into_iter().filter_map(|ext| match ext.content {
        rasn::extensions::SpecificExtension::ModbusRole(role) => Some(role.role),
        _ => None,
    });

    // Extract the first ModbusRole extension
    let role = it.next().ok_or_else(|| {
        rustls::Error::InvalidCertificateData(
            "certificate doesn't have Modbus extension".to_string(),
        )
    })?;

    // Check that there is only one extension
    if it.next().is_some() {
        return Err(rustls::Error::InvalidCertificateData(
            "certificate has more than one Modbus extension".to_string(),
        ));
    }

    Ok(role.to_string())
}
