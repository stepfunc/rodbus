// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Handshakes over an in-memory pipe that pin how the client and server configurations apply
//! each certificate mode, the server's subject name and the minimum TLS version

use std::future::Future;
use std::net::Ipv4Addr;
use std::sync::Arc;

use sfio_rustls_config::{ClientNameVerification, ProtocolVersions, ServerNameVerification};
use tokio::io::DuplexStream;
use tokio_rustls::rustls;
use tokio_rustls::rustls::client::ResolvesClientCert;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::sign::CertifiedKey;
use tokio_rustls::rustls::{
    AlertDescription, CertificateError, PeerIncompatible, ProtocolVersion, SignatureScheme,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::client::HostAddr;
use crate::common::phys::PhysLayer;
use crate::common::test_support::watchdog;
use crate::server::task::AuthorizationType;
use crate::tcp::accept::test_util::PIPE;
use crate::tcp::tls::test_util::{
    ca_chain, cert, client_config, peer_client_config, server_config,
};
use crate::tcp::tls::{
    CertificateMode, HandshakeError, MinTlsVersion, Refused, TlsClientConfig, TlsServerConfig,
};

/// The subject name of the server certificate the authority issued
const NAME: &str = "test.com";

const MODES: [CertificateMode; 2] = [CertificateMode::AuthorityBased, CertificateMode::SelfSigned];

/// A client presenting the client certificate
fn client(mode: CertificateMode, min: MinTlsVersion) -> TlsClientConfig {
    match mode {
        CertificateMode::AuthorityBased => pki_client(Some(NAME), &ca_chain("ca_cert.pem"), min),
        CertificateMode::SelfSigned => client_config(min),
    }
}

/// A server presenting the server certificate
fn server(mode: CertificateMode, min: MinTlsVersion) -> TlsServerConfig {
    match mode {
        CertificateMode::AuthorityBased => pki_server(&ca_chain("ca_cert.pem"), min),
        CertificateMode::SelfSigned => server_config(min),
    }
}

/// A full-PKI client presenting the authority's client certificate and trusting `authority`
fn pki_client(
    name: Option<&str>,
    authority: &std::path::Path,
    min: MinTlsVersion,
) -> TlsClientConfig {
    TlsClientConfig::full_pki(
        name.map(str::to_string),
        authority,
        &ca_chain("client_cert.pem"),
        &ca_chain("client_key.pem"),
        None,
        min,
    )
    .unwrap()
}

/// A full-PKI server presenting the authority's server certificate and trusting `authority`
fn pki_server(authority: &std::path::Path, min: MinTlsVersion) -> TlsServerConfig {
    TlsServerConfig::new(
        authority,
        &ca_chain("server_cert.pem"),
        &ca_chain("server_key.pem"),
        None,
        min,
        CertificateMode::AuthorityBased,
    )
    .unwrap()
}

/// A peer that would complete a handshake with [`server`], built without this crate's configuration
fn peer_client(mode: CertificateMode, versions: ProtocolVersions) -> rustls::ClientConfig {
    match mode {
        CertificateMode::AuthorityBased => sfio_rustls_config::client::authority(
            versions,
            ServerNameVerification::SanOrCommonName,
            &ca_chain("ca_cert.pem"),
            &ca_chain("client_cert.pem"),
            &ca_chain("client_key.pem"),
            None,
        )
        .unwrap(),
        CertificateMode::SelfSigned => peer_client_config(versions),
    }
}

/// A peer that would complete a handshake with [`client`], built without this crate's configuration
fn peer_server(mode: CertificateMode, versions: ProtocolVersions) -> rustls::ServerConfig {
    match mode {
        CertificateMode::AuthorityBased => sfio_rustls_config::server::authority(
            versions,
            ClientNameVerification::None,
            &ca_chain("ca_cert.pem"),
            &ca_chain("server_cert.pem"),
            &ca_chain("server_key.pem"),
            None,
        ),
        CertificateMode::SelfSigned => sfio_rustls_config::server::self_signed(
            versions,
            &cert("entity2_cert.pem"),
            &cert("entity1_cert.pem"),
            &cert("entity1_key.pem"),
            None,
        ),
    }
    .unwrap()
}

fn both_versions() -> ProtocolVersions {
    ProtocolVersions::new().enable_v12().enable_v13()
}

/// Replaces the certificate chain a peer client presents
#[derive(Debug)]
enum ClientChain {
    /// Present no certificate
    Nothing,
    /// Present the configured certificate with a copy of itself as an intermediate
    WithIntermediate(Arc<dyn ResolvesClientCert>),
}

impl ResolvesClientCert for ClientChain {
    fn resolve(
        &self,
        root_hint_subjects: &[&[u8]],
        sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
        match self {
            Self::Nothing => None,
            Self::WithIntermediate(inner) => {
                let key = inner.resolve(root_hint_subjects, sigschemes)?;
                let chain = key.cert.iter().chain(key.cert.iter()).cloned().collect();
                Some(Arc::new(CertifiedKey::new(chain, key.key.clone())))
            }
        }
    }

    fn has_certs(&self) -> bool {
        matches!(self, Self::WithIntermediate(_))
    }
}

/// Run both sides of a handshake over a pipe
async fn handshake<C: Future, S: Future>(
    client: impl FnOnce(DuplexStream) -> C,
    server: impl FnOnce(DuplexStream) -> S,
) -> (C::Output, S::Output) {
    let (client_io, server_io) = tokio::io::duplex(PIPE);
    tokio::join!(client(client_io), server(server_io))
}

/// Accept a client on `io` as the server under test, without authorization
async fn accept(
    server: &TlsServerConfig,
    io: DuplexStream,
) -> Result<(PhysLayer, AuthorizationType), Refused> {
    server.handle_connection(io, None).await
}

/// Connect a peer client to `io`
async fn connect_peer(
    config: rustls::ClientConfig,
    io: DuplexStream,
) -> std::io::Result<tokio_rustls::client::TlsStream<DuplexStream>> {
    let name = ServerName::try_from(NAME).unwrap();
    TlsConnector::from(Arc::new(config)).connect(name, io).await
}

/// Accept a peer server's side of a handshake on `io`
async fn accept_peer(
    config: rustls::ServerConfig,
    io: DuplexStream,
) -> std::io::Result<tokio_rustls::server::TlsStream<DuplexStream>> {
    TlsAcceptor::from(Arc::new(config)).accept(io).await
}

/// The TLS error that ended a peer's handshake
fn peer_error<T>(result: std::io::Result<T>) -> rustls::Error {
    match result {
        Ok(_) => panic!("the peer completed the handshake"),
        Err(err) => rustls_error(err),
    }
}

/// The TLS error that ended the client's handshake
fn client_error<T>(result: Result<T, HandshakeError>) -> rustls::Error {
    match result {
        Ok(_) => panic!("the client completed the handshake"),
        Err(err) => handshake_error(err),
    }
}

/// The TLS error that ended the server's handshake
fn server_error<T>(result: Result<T, Refused>) -> rustls::Error {
    match result {
        Ok(_) => panic!("the server accepted the client"),
        Err(Refused::Handshake(err)) => handshake_error(err),
        Err(err) => panic!("refused after the handshake: {err:?}"),
    }
}

fn handshake_error(err: HandshakeError) -> rustls::Error {
    match err {
        HandshakeError::Timeout => panic!("the handshake timed out"),
        HandshakeError::Failed(err) => rustls_error(err),
    }
}

/// tokio-rustls reports TLS failures as an `io::Error` wrapping the `rustls::Error`
fn rustls_error(err: std::io::Error) -> rustls::Error {
    *err.into_inner()
        .expect("an error without a cause")
        .downcast::<rustls::Error>()
        .expect("not a TLS error")
}

/// The server under test accepted the client without authorization
fn accepted<T>(result: Result<(T, AuthorizationType), Refused>) -> bool {
    matches!(result, Ok((_, AuthorizationType::None)))
}

#[tokio::test]
async fn authority_based_peers_with_matching_names_complete_the_handshake() {
    watchdog(async {
        let client = client(CertificateMode::AuthorityBased, MinTlsVersion::V1_2);
        let server = server(CertificateMode::AuthorityBased, MinTlsVersion::V1_2);

        let (client, server) = handshake(|io| client.connect(io), |io| accept(&server, io)).await;

        assert_eq!(
            client.unwrap().get_ref().1.protocol_version(),
            Some(ProtocolVersion::TLSv1_3)
        );
        assert!(accepted(server));
    })
    .await;
}

#[tokio::test]
async fn self_signed_peers_with_each_others_certificates_complete_the_handshake() {
    watchdog(async {
        let client = client(CertificateMode::SelfSigned, MinTlsVersion::V1_2);
        let server = server(CertificateMode::SelfSigned, MinTlsVersion::V1_2);

        let (client, server) = handshake(|io| client.connect(io), |io| accept(&server, io)).await;

        assert_eq!(
            client.unwrap().get_ref().1.protocol_version(),
            Some(ProtocolVersion::TLSv1_3)
        );
        assert!(accepted(server));
    })
    .await;
}

#[tokio::test]
async fn without_a_subject_name_the_client_accepts_any_trusted_name() {
    watchdog(async {
        let client = pki_client(None, &ca_chain("ca_cert.pem"), MinTlsVersion::V1_2);
        let server = server(CertificateMode::AuthorityBased, MinTlsVersion::V1_2);

        let (client, server) = handshake(|io| client.connect(io), |io| accept(&server, io)).await;

        assert_eq!(
            client.unwrap().get_ref().1.protocol_version(),
            Some(ProtocolVersion::TLSv1_3)
        );
        assert!(accepted(server));
    })
    .await;
}

#[tokio::test]
async fn client_rejects_a_server_with_another_name() {
    watchdog(async {
        let client = pki_client(
            Some("other.com"),
            &ca_chain("ca_cert.pem"),
            MinTlsVersion::V1_2,
        );
        let server = server(CertificateMode::AuthorityBased, MinTlsVersion::V1_2);

        let (client, server) = handshake(|io| client.connect(io), |io| accept(&server, io)).await;

        assert_eq!(
            client_error(client),
            rustls::Error::InvalidCertificate(CertificateError::NotValidForName)
        );
        assert_eq!(
            server_error(server),
            rustls::Error::AlertReceived(AlertDescription::BadCertificate)
        );
    })
    .await;
}

#[tokio::test]
async fn client_rejects_a_server_from_another_authority() {
    watchdog(async {
        // a self-signed CA certificate that didn't issue the server's certificate; it has the
        // issuer's subject name, so verification fails on the signature
        let client = pki_client(None, &cert("entity1_cert.pem"), MinTlsVersion::V1_2);
        let server = server(CertificateMode::AuthorityBased, MinTlsVersion::V1_2);

        let (client, server) = handshake(|io| client.connect(io), |io| accept(&server, io)).await;

        assert_eq!(
            client_error(client),
            rustls::Error::InvalidCertificate(CertificateError::BadSignature)
        );
        assert_eq!(
            server_error(server),
            rustls::Error::AlertReceived(AlertDescription::DecryptError)
        );
    })
    .await;
}

#[tokio::test]
async fn server_rejects_a_client_from_another_authority() {
    watchdog(async {
        let client = client(CertificateMode::AuthorityBased, MinTlsVersion::V1_2);
        // a self-signed CA certificate that didn't issue the client's certificate; it has the
        // issuer's subject name, so verification fails on the signature
        let server = pki_server(&cert("entity1_cert.pem"), MinTlsVersion::V1_2);

        let (_, server) = handshake(|io| client.connect(io), |io| accept(&server, io)).await;

        assert_eq!(
            server_error(server),
            rustls::Error::InvalidCertificate(CertificateError::BadSignature)
        );
    })
    .await;
}

#[tokio::test]
async fn self_signed_client_rejects_a_server_certificate_it_was_not_given() {
    watchdog(async {
        // expects the client's own certificate instead of the server's
        let client = TlsClientConfig::self_signed(
            &cert("entity2_cert.pem"),
            &cert("entity2_cert.pem"),
            &cert("entity2_key.pem"),
            None,
            MinTlsVersion::V1_2,
        )
        .unwrap();
        let server = server(CertificateMode::SelfSigned, MinTlsVersion::V1_2);

        let (client, server) = handshake(|io| client.connect(io), |io| accept(&server, io)).await;

        assert_eq!(
            client_error(client),
            rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer)
        );
        assert_eq!(
            server_error(server),
            rustls::Error::AlertReceived(AlertDescription::UnknownCA)
        );
    })
    .await;
}

#[tokio::test]
async fn self_signed_server_rejects_a_client_certificate_it_was_not_given() {
    watchdog(async {
        let client = client(CertificateMode::SelfSigned, MinTlsVersion::V1_2);
        // expects the server's own certificate instead of the client's
        let server = TlsServerConfig::new(
            &cert("entity1_cert.pem"),
            &cert("entity1_cert.pem"),
            &cert("entity1_key.pem"),
            None,
            MinTlsVersion::V1_2,
            CertificateMode::SelfSigned,
        )
        .unwrap();

        let (_, server) = handshake(|io| client.connect(io), |io| accept(&server, io)).await;

        assert_eq!(
            server_error(server),
            rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer)
        );
    })
    .await;
}

#[tokio::test]
async fn self_signed_server_rejects_a_client_that_sends_intermediates() {
    watchdog(async {
        let server = server(CertificateMode::SelfSigned, MinTlsVersion::V1_2);
        let mut peer = peer_client(CertificateMode::SelfSigned, both_versions());
        peer.client_auth_cert_resolver = Arc::new(ClientChain::WithIntermediate(
            peer.client_auth_cert_resolver.clone(),
        ));

        let (_, server) = handshake(|io| connect_peer(peer, io), |io| accept(&server, io)).await;

        assert!(matches!(server_error(server), rustls::Error::General(_)));
    })
    .await;
}

#[tokio::test]
async fn servers_reject_a_client_without_a_certificate() {
    watchdog(async {
        for mode in MODES {
            let server = server(mode, MinTlsVersion::V1_2);
            let mut peer = peer_client(mode, both_versions());
            peer.client_auth_cert_resolver = Arc::new(ClientChain::Nothing);

            let (_, server) =
                handshake(|io| connect_peer(peer, io), |io| accept(&server, io)).await;

            assert_eq!(server_error(server), rustls::Error::NoCertificatesPresented);
        }
    })
    .await;
}

#[tokio::test]
async fn servers_with_minimum_version_1_3_reject_a_1_2_only_client() {
    watchdog(async {
        for mode in MODES {
            let server = server(mode, MinTlsVersion::V1_3);
            let peer = peer_client(mode, ProtocolVersions::v12_only());

            let (peer, server) =
                handshake(|io| connect_peer(peer, io), |io| accept(&server, io)).await;

            assert_eq!(
                server_error(server),
                rustls::Error::PeerIncompatible(PeerIncompatible::Tls12NotOfferedOrEnabled)
            );
            assert_eq!(
                peer_error(peer),
                rustls::Error::AlertReceived(AlertDescription::ProtocolVersion)
            );
        }
    })
    .await;
}

#[tokio::test]
async fn clients_with_minimum_version_1_3_reject_a_1_2_only_server() {
    watchdog(async {
        for mode in MODES {
            let client = client(mode, MinTlsVersion::V1_3);
            let peer = peer_server(mode, ProtocolVersions::v12_only());

            let (client, peer) =
                handshake(|io| client.connect(io), |io| accept_peer(peer, io)).await;

            assert_eq!(
                client_error(client),
                rustls::Error::AlertReceived(AlertDescription::ProtocolVersion)
            );
            assert_eq!(
                peer_error(peer),
                rustls::Error::PeerIncompatible(PeerIncompatible::Tls12NotOfferedOrEnabled)
            );
        }
    })
    .await;
}

#[tokio::test]
async fn minimum_version_1_2_negotiates_the_highest_version_the_peer_offers() {
    watchdog(async {
        let cases = [
            (ProtocolVersions::v12_only(), ProtocolVersion::TLSv1_2),
            (both_versions(), ProtocolVersion::TLSv1_3),
        ];
        for mode in MODES {
            for (versions, expected) in cases {
                let client = client(mode, MinTlsVersion::V1_2);
                let peer = peer_server(mode, versions);

                let (client, peer) =
                    handshake(|io| client.connect(io), |io| accept_peer(peer, io)).await;

                assert_eq!(
                    client.unwrap().get_ref().1.protocol_version(),
                    Some(expected)
                );
                assert_eq!(peer.unwrap().get_ref().1.protocol_version(), Some(expected));

                let server = server(mode, MinTlsVersion::V1_2);
                let peer = peer_client(mode, versions);

                let (peer, server) =
                    handshake(|io| connect_peer(peer, io), |io| accept(&server, io)).await;

                assert!(accepted(server));
                assert_eq!(peer.unwrap().get_ref().1.protocol_version(), Some(expected));
            }
        }
    })
    .await;
}

#[tokio::test]
async fn client_connection_reports_a_failed_handshake_as_an_io_error() {
    watchdog(async {
        let client = client(CertificateMode::SelfSigned, MinTlsVersion::V1_3);
        let peer = peer_server(CertificateMode::SelfSigned, ProtocolVersions::v12_only());
        let endpoint = HostAddr::ip(Ipv4Addr::LOCALHOST.into(), 502);

        let (client, _) = handshake(
            |io| client.handle_connection(io, &endpoint),
            |io| accept_peer(peer, io),
        )
        .await;

        assert!(matches!(client, Err(err) if err.kind() == std::io::ErrorKind::InvalidData));
    })
    .await;
}
