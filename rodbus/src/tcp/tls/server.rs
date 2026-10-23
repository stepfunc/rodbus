use sfio_rustls_config::ClientNameVerification;
use std::path::Path;
use std::sync::Arc;

use tokio_rustls::rustls;

use crate::common::phys::{PhysLayer, Socket};
use crate::server::task::AuthorizationType;
use crate::server::AuthorizationHandler;
use crate::tcp::tls::{
    handshake, CertificateMode, HandshakeError, MinTlsVersion, TlsError, TLS_HANDSHAKE_TIMEOUT,
};

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

    pub(crate) async fn handle_connection<S: Socket>(
        &mut self,
        socket: S,
        auth_handler: Option<Arc<dyn AuthorizationHandler>>,
    ) -> Result<(PhysLayer, AuthorizationType), String> {
        let connector = tokio_rustls::TlsAcceptor::from(self.inner.clone());
        // the server runs one handshake at a time, so one that stalls would stop it
        match handshake(connector.accept(socket)).await {
            Err(HandshakeError::Timeout) => Err(format!(
                "TLS handshake didn't complete within {} seconds",
                TLS_HANDSHAKE_TIMEOUT.as_secs()
            )),
            Err(HandshakeError::Failed(err)) => {
                Err(format!("failed to establish TLS session: {err}"))
            }
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

                let layer = S::into_tls_phys(tokio_rustls::TlsStream::from(stream));

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
    let roles = extensions.into_iter().filter_map(|ext| match ext.content {
        rx509::x509::ext::SpecificExtension::ModbusRole(role) => Some(role.role),
        _ => None,
    });

    match single_role(roles) {
        Ok(role) => Ok(role.to_string()),
        Err(RoleError::Missing) => Err("certificate doesn't have Modbus extension".to_string()),
        Err(RoleError::Multiple) => {
            Err("certificate has more than one Modbus extension".to_string())
        }
        Err(RoleError::ContainsNul) => {
            Err("certificate's Modbus role contains a NUL character".to_string())
        }
    }
}

/// Why a certificate's Modbus roles don't give one usable role
#[derive(Debug, PartialEq)]
enum RoleError {
    Missing,
    Multiple,
    ContainsNul,
}

/// The one role among a certificate's Modbus role extensions
///
/// The whole string is the role (Modbus/TCP Security R-65). A NUL would cut it short wherever the
/// role is handed on as a C string, so such a role is refused.
fn single_role<'a>(mut roles: impl Iterator<Item = &'a str>) -> Result<&'a str, RoleError> {
    let role = roles.next().ok_or(RoleError::Missing)?;
    if roles.next().is_some() {
        return Err(RoleError::Multiple);
    }
    if role.contains('\0') {
        return Err(RoleError::ContainsNul);
    }
    Ok(role)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_role_is_used_whole() {
        assert_eq!(single_role(["operator"].into_iter()), Ok("operator"));
    }

    #[test]
    fn a_role_containing_nul_is_refused() {
        assert_eq!(
            single_role(["admin\0operator"].into_iter()),
            Err(RoleError::ContainsNul)
        );
        assert_eq!(single_role(["\0"].into_iter()), Err(RoleError::ContainsNul));
    }

    #[test]
    fn exactly_one_role_is_required() {
        assert_eq!(single_role(std::iter::empty()), Err(RoleError::Missing));
        assert_eq!(
            single_role(["operator", "admin"].into_iter()),
            Err(RoleError::Multiple)
        );
    }
}
