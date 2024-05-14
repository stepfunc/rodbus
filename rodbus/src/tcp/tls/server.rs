use sfio_rustls_config::ClientNameVerification;
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
        let config = match certificate_mode {
            CertificateMode::SelfSigned => sfio_rustls_config::server::self_signed(
                min_tls_version.into(),
                peer_cert_path,
                local_cert_path,
                private_key_path,
                password,
            )?,
            CertificateMode::AuthorityBased => sfio_rustls_config::server::authority(
                min_tls_version.into(),
                ClientNameVerification::None,
                peer_cert_path,
                local_cert_path,
                private_key_path,
                password,
            )?,
        };

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
                            .ok_or_else(|| "No peer certificate".to_string())?;

                        let parsed = rx509::x509::Certificate::parse(peer_cert)
                            .map_err(|err| format!("ASNError: {err}"))?;
                        let role = extract_modbus_role(&parsed)?;

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

fn extract_modbus_role(cert: &rx509::x509::Certificate) -> Result<String, String> {
    // Parse the extensions
    let extensions = cert
        .tbs_certificate
        .value
        .extensions
        .as_ref()
        .ok_or_else(|| "certificate doesn't contain Modbus role extension".to_string())?;

    let extensions = extensions
        .parse()
        .map_err(|err| format!("unable to parse cert extensions with rasn: {err:?}"))?;

    // Extract the ModbusRole extensions
    let mut it = extensions.into_iter().filter_map(|ext| match ext.content {
        rx509::x509::ext::SpecificExtension::ModbusRole(role) => Some(role.role),
        _ => None,
    });

    // Extract the first ModbusRole extension
    let role = it
        .next()
        .ok_or_else(|| "certificate doesn't have Modbus extension".to_string())?;

    // Check that there is only one role extension
    if it.next().is_some() {
        return Err("certificate has more than one Modbus extension".to_string());
    }

    Ok(role.to_string())
}
