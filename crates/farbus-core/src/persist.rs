use crate::fingerprint::PeerFingerprint;
use crate::tls::{fingerprint_cert, TlsError};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::fs;
use std::path::PathBuf;

/// Returns the directory used for persisted server identity.
///
/// # Errors
///
/// Returns an I/O error when no home directory is available.
pub fn identity_dir() -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
    Ok(PathBuf::from(home).join(".config/farbus/server"))
}

/// Loads a persisted server certificate or creates one.
///
/// # Errors
///
/// Returns I/O or TLS generation errors.
pub fn load_or_create_server_identity(
    name: &str,
) -> Result<
    (
        Vec<CertificateDer<'static>>,
        PrivateKeyDer<'static>,
        PeerFingerprint,
    ),
    TlsError,
> {
    if let Ok(loaded) = load_identity() {
        return Ok(loaded);
    }
    let generated = crate::tls::make_self_signed(name)?;
    let _ = save_identity(&generated.0[0], &generated.1);
    Ok(generated)
}

fn load_identity() -> Result<
    (
        Vec<CertificateDer<'static>>,
        PrivateKeyDer<'static>,
        PeerFingerprint,
    ),
    TlsError,
> {
    let dir = identity_dir().map_err(|_| TlsError::Gen)?;
    let cert = fs::read(dir.join("cert.der")).map_err(|_| TlsError::Gen)?;
    let key = fs::read(dir.join("key.der")).map_err(|_| TlsError::Gen)?;
    let fingerprint = fingerprint_cert(&cert);
    Ok((
        vec![CertificateDer::from(cert)],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        fingerprint,
    ))
}

fn save_identity(
    cert: &CertificateDer<'static>,
    key: &PrivateKeyDer<'static>,
) -> std::io::Result<()> {
    let dir = identity_dir()?;
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        if let Some(parent) = dir.parent() {
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    let cert_path = dir.join("cert.der");
    let key_path = dir.join("key.der");
    fs::write(&cert_path, cert.as_ref())?;
    fs::write(&key_path, key.secret_der())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600));
        let _ = fs::set_permissions(&cert_path, fs::Permissions::from_mode(0o644));
    }
    Ok(())
}
