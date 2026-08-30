use crate::fingerprint::PeerFingerprint;
use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, DigitallySignedStruct, Error as RustlsError, ServerConfig, SignatureScheme,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use thiserror::Error as ThisError;
use tokio_rustls::{TlsAcceptor, TlsConnector};

#[derive(Debug, ThisError)]
pub enum TlsError {
    #[error("certificate generation failed")]
    Gen,
    #[error("TLS configuration error")]
    Config,
    #[error(transparent)]
    Rustls(#[from] RustlsError),
}

#[must_use]
pub fn fingerprint_cert(cert_der: &[u8]) -> PeerFingerprint {
    let digest: [u8; 32] = Sha256::digest(cert_der).into();
    PeerFingerprint::new(digest)
}

/// Generates a self-signed P-256 certificate for `FarBus` servers.
///
/// # Errors
///
/// Returns [`TlsError::Gen`] if key or certificate generation fails.
pub fn make_self_signed(
    name: &str,
) -> Result<
    (
        Vec<CertificateDer<'static>>,
        PrivateKeyDer<'static>,
        PeerFingerprint,
    ),
    TlsError,
> {
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(|_| TlsError::Gen)?;
    let mut params = CertificateParams::new(vec![name.to_string()]).map_err(|_| TlsError::Gen)?;
    params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
    let cert = params.self_signed(&key).map_err(|_| TlsError::Gen)?;
    let der = cert.der().to_vec();
    let fingerprint = fingerprint_cert(&der);
    let cert_der = CertificateDer::from(der);
    let key_der = PrivateKeyDer::Pkcs8(key.serialized_der().to_vec().into());
    Ok((vec![cert_der], key_der, fingerprint))
}

/// Builds a server TLS acceptor with the single self-signed certificate.
///
/// # Errors
///
/// Returns [`TlsError::Rustls`] on invalid configuration.
pub fn make_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<TlsAcceptor, TlsError> {
    let mut cfg = ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(TlsError::Rustls)?;
    cfg.alpn_protocols = vec![b"farbus-v1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(cfg)))
}

#[derive(Debug)]
struct PinnedServerVerifier {
    expected: PeerFingerprint,
    supported_algs: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let actual = fingerprint_cert(end_entity.as_ref());
        if actual == self.expected {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(RustlsError::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.supported_algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.supported_algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported_algs.supported_schemes()
    }
}

#[derive(Debug)]
struct ObservingServerVerifier {
    seen: Arc<Mutex<Option<PeerFingerprint>>>,
    supported_algs: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for ObservingServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(fingerprint_cert(end_entity.as_ref()));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.supported_algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.supported_algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported_algs.supported_schemes()
    }
}

/// Builds a client TLS connector that records the server certificate fingerprint.
///
/// Pairing still requires the PIN. Use this only to learn the pin-target identity.
///
/// # Errors
///
/// Returns [`TlsError`] on configuration error.
type ObservedFingerprint = Arc<Mutex<Option<PeerFingerprint>>>;

#[allow(clippy::type_complexity)]
pub fn make_observing_client_config() -> Result<(TlsConnector, ObservedFingerprint), TlsError> {
    let supported_algs = rustls::crypto::ring::default_provider().signature_verification_algorithms;
    let seen = Arc::new(Mutex::new(None));
    let verifier = Arc::new(ObservingServerVerifier {
        seen: Arc::clone(&seen),
        supported_algs,
    });
    let mut cfg = ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"farbus-v1".to_vec()];
    Ok((TlsConnector::from(Arc::new(cfg)), seen))
}

/// Builds a client TLS connector that pins the server fingerprint.
///
/// # Errors
///
/// Returns [`TlsError`] on configuration error.
pub fn make_pinned_client_config(
    expected_server: PeerFingerprint,
) -> Result<TlsConnector, TlsError> {
    let supported_algs = rustls::crypto::ring::default_provider().signature_verification_algorithms;
    let verifier = Arc::new(PinnedServerVerifier {
        expected: expected_server,
        supported_algs,
    });
    let mut cfg = ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"farbus-v1".to_vec()];
    Ok(TlsConnector::from(Arc::new(cfg)))
}
