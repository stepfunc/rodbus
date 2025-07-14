pub(crate) mod client;
pub(crate) mod server;

pub(crate) use client::*;
pub(crate) use server::*;

/// Determines how the certificate(s) presented by the peer are validated
///
/// This validation always occurs **after** the handshake signature has been
/// verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serialization", derive(serde::Serialize, serde::Deserialize))]
pub enum CertificateMode {
    /// Validates the peer certificate against one or more configured trust anchors
    ///
    /// This mode uses the default certificate verifier in `rustls` to ensure that
    /// the chain of certificates presented by the peer is valid against one of
    /// the configured trust anchors.
    ///
    /// The name verification is relaxed to allow for certificates that do not contain
    /// the SAN extension. In these cases the name is verified using the Common Name instead.
    AuthorityBased,
    /// Validates that the peer presents a single certificate which is a byte-for-byte match
    /// against the configured peer certificate.
    ///
    /// The certificate is parsed only to ensure that the `NotBefore` and `NotAfter`
    /// are valid for the current system time.
    SelfSigned,
}

/// TLS-related errors
#[derive(Debug)]
pub enum TlsError {
    /// Invalid peer certificate
    InvalidPeerCertificate(std::io::Error),
    /// Invalid local certificate
    InvalidLocalCertificate(std::io::Error),
    /// Invalid private key
    InvalidPrivateKey(std::io::Error),
    /// DNS name is invalid
    InvalidDnsName,
    /// Error building TLS configuration
    BadConfig(String),
}

impl std::fmt::Display for TlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPeerCertificate(err) => {
                write!(f, "invalid peer certificate file: {err}")
            }
            Self::InvalidLocalCertificate(err) => {
                write!(f, "invalid local certificate file: {err}")
            }
            Self::InvalidPrivateKey(err) => write!(f, "invalid private key file: {err}"),
            Self::InvalidDnsName => write!(f, "invalid DNS name"),
            Self::BadConfig(err) => write!(f, "bad config: {err}"),
        }
    }
}

impl std::error::Error for TlsError {}

impl From<sfio_rustls_config::Error> for TlsError {
    fn from(err: sfio_rustls_config::Error) -> Self {
        Self::BadConfig(err.to_string())
    }
}

/// Minimum TLS version to allow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serialization", derive(serde::Serialize, serde::Deserialize))]
pub enum MinTlsVersion {
    /// TLS 1.2
    V1_2,
    /// TLS 1.3
    V1_3,
}

/*
impl From<MinTlsVersion> for sfio_rustls_config::MinProtocolVersion {
    fn from(value: MinTlsVersion) -> Self {
        match value {
            MinTlsVersion::V1_2 => sfio_rustls_config::MinProtocolVersion::V1_2,
            MinTlsVersion::V1_3 => sfio_rustls_config::MinProtocolVersion::V1_3,
        }
    }
}

impl From<InvalidDnsNameError> for TlsError {
    fn from(_: InvalidDnsNameError) -> Self {
        Self::InvalidDnsName
    }
}
*/
