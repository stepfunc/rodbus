// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use sfio_rustls_config::ClientNameVerification;
use std::path::Path;
use std::sync::Arc;

use tokio_rustls::rustls;

use crate::common::phys::{PhysLayer, Socket};
use crate::server::request::{Request, WriteRequest};
use crate::server::task::AuthorizationType;
use crate::server::{Authorization, AuthorizationHandler};
use crate::tcp::tls::{handshake, CertificateMode, HandshakeError, MinTlsVersion, TlsError};
use crate::UnitId;

/// TLS configuration
#[derive(Clone)]
pub struct TlsServerConfig {
    inner: Arc<rustls::ServerConfig>,
}

impl std::fmt::Debug for TlsServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsServerConfig").finish_non_exhaustive()
    }
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
        &self,
        socket: S,
        auth_handler: Option<Arc<dyn AuthorizationHandler>>,
    ) -> Result<(PhysLayer, AuthorizationType), Refused> {
        let connector = tokio_rustls::TlsAcceptor::from(self.inner.clone());
        // the server runs one handshake at a time, so one that stalls would stop it
        let stream = handshake(connector.accept(socket))
            .await
            .map_err(Refused::Handshake)?;

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
                    .ok_or(Refused::NoPeerCertificate)?;

                let parsed =
                    rx509::x509::Certificate::parse(peer_cert).map_err(Refused::Certificate)?;
                let role = extract_modbus_role(&parsed)?;

                tracing::info!("client role: {}", role);
                AuthorizationType::Handler(RoleAuthorizer::new(handler, role))
            }
        };

        let layer = S::into_tls_phys(tokio_rustls::TlsStream::from(stream));

        Ok((layer, auth_type))
    }
}

/// Authorizes a session's requests for the role in its client's certificate
pub(crate) struct RoleAuthorizer {
    handler: Arc<dyn AuthorizationHandler>,
    role: String,
}

impl RoleAuthorizer {
    pub(crate) fn new(handler: Arc<dyn AuthorizationHandler>, role: String) -> Self {
        Self { handler, role }
    }

    /// Asks the handler whether the role may make `request` to `unit_id`, logging a denial
    pub(crate) fn is_authorized(&self, unit_id: UnitId, request: &Request) -> Authorization {
        let result = self.check(unit_id, request);
        if let Authorization::Deny = result {
            tracing::warn!(
                "Role \"{}\" not authorized for request: {:?}",
                self.role,
                request.get_function()
            );
        }
        result
    }

    fn check(&self, unit_id: UnitId, request: &Request) -> Authorization {
        let handler = self.handler.as_ref();
        let role = self.role.as_str();
        match request {
            Request::ReadCoils(x) => handler.read_coils(unit_id, x.get(), role),
            Request::ReadDiscreteInputs(x) => handler.read_discrete_inputs(unit_id, x.get(), role),
            Request::ReadHoldingRegisters(x) => {
                handler.read_holding_registers(unit_id, x.get(), role)
            }
            Request::ReadInputRegisters(x) => handler.read_input_registers(unit_id, x.get(), role),
            Request::Write(write) => match write {
                WriteRequest::SingleCoil(x) => handler.write_single_coil(unit_id, x.index, role),
                WriteRequest::SingleRegister(x) => {
                    handler.write_single_register(unit_id, x.index, role)
                }
                WriteRequest::MultipleCoils(x) => {
                    handler.write_multiple_coils(unit_id, x.range, role)
                }
                WriteRequest::MultipleRegisters(x) => {
                    handler.write_multiple_registers(unit_id, x.range, role)
                }
            },
        }
    }
}

/// Why the server refused a TLS connection
#[derive(Debug)]
pub(crate) enum Refused {
    /// The handshake didn't produce a session
    Handshake(HandshakeError),
    /// The client presented no certificate to take its role from
    NoPeerCertificate,
    /// The client's certificate, or its extensions, couldn't be parsed
    Certificate(rx509::der::ASNError),
    /// The client's certificate doesn't give one usable role
    Role(RoleError),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handshake(err) => write!(f, "{err}"),
            Self::NoPeerCertificate => f.write_str("no peer certificate"),
            Self::Certificate(err) => write!(f, "unable to parse the peer certificate: {err}"),
            Self::Role(err) => write!(f, "{err}"),
        }
    }
}

fn extract_modbus_role(cert: &rx509::x509::Certificate) -> Result<String, Refused> {
    let Some(extensions) = cert.tbs_certificate.value.extensions.as_ref() else {
        return Err(Refused::Role(RoleError::Missing));
    };

    let extensions = extensions.parse().map_err(Refused::Certificate)?;

    // Extract the ModbusRole extensions
    let roles = extensions.into_iter().filter_map(|ext| match ext.content {
        rx509::x509::ext::SpecificExtension::ModbusRole(role) => Some(role.role),
        _ => None,
    });

    single_role(roles)
        .map(str::to_string)
        .map_err(Refused::Role)
}

/// Why a certificate's Modbus roles don't give one usable role
#[derive(Debug, PartialEq)]
pub(crate) enum RoleError {
    /// The certificate has no Modbus role extension
    Missing,
    /// The certificate has more than one Modbus role extension
    Multiple,
    /// The role contains a NUL character
    ContainsNul,
}

impl std::fmt::Display for RoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Missing => "certificate doesn't have a Modbus role extension",
            Self::Multiple => "certificate has more than one Modbus role extension",
            Self::ContainsNul => "certificate's Modbus role contains a NUL character",
        })
    }
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
    use crate::common::test_support::watchdog;
    use crate::server::handler::test_util::Recorder;
    use crate::tcp::tls::test_util::{
        authority_server, handshake_with, operator, without_a_role, ROLE,
    };

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

    /// What the server made of a connection
    #[derive(Debug, PartialEq)]
    enum Admitted {
        /// A session whose requests aren't checked
        Unchecked,
        /// A session whose requests are checked for the role
        Checked(String),
        /// Refused for the certificate's role
        RefusedRole(RoleError),
    }

    /// Run the handshake of `client` with `server` and `authorizer`, and return what the server
    /// made of it
    async fn admit(
        server: TlsServerConfig,
        authorizer: Option<Arc<dyn AuthorizationHandler>>,
        client: rustls::ClientConfig,
    ) -> Admitted {
        let (client_io, server_io) = tokio::io::duplex(crate::tcp::accept::test_util::PIPE);
        let (connected, admitted) = tokio::join!(
            handshake_with(client, client_io),
            server.handle_connection(server_io, authorizer)
        );
        connected.unwrap();
        match admitted {
            Ok((_, AuthorizationType::None)) => Admitted::Unchecked,
            Ok((_, AuthorizationType::Handler(authorizer))) => Admitted::Checked(authorizer.role),
            Err(Refused::Role(err)) => Admitted::RefusedRole(err),
            Err(err) => panic!("refused: {err}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn only_an_authorizing_server_takes_the_client_role_and_requires_one() {
        watchdog(async {
            let (authorizer, _) = Recorder::denying(&[]);
            let cases = [
                (
                    Some(authorizer.clone()),
                    operator(),
                    Admitted::Checked(ROLE.to_string()),
                ),
                (
                    Some(authorizer),
                    without_a_role(),
                    Admitted::RefusedRole(RoleError::Missing),
                ),
                (None, operator(), Admitted::Unchecked),
                (None, without_a_role(), Admitted::Unchecked),
            ];
            for (authorizer, client, expected) in cases {
                assert_eq!(
                    admit(authority_server(), authorizer, client).await,
                    expected
                );
            }
        })
        .await;
    }
}
