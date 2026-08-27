use crate::fingerprint::PeerFingerprint;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct StoredSession {
    pub addr: SocketAddr,
    pub fingerprint: PeerFingerprint,
    pub auth_token: [u8; 32],
}

/// Saves the session connection details to disk.
///
/// # Errors
///
/// Returns I/O errors when the configuration directory cannot be written.
pub fn save_session(session: &StoredSession) -> std::io::Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)?;
    let mut token_hex = String::new();
    for b in session.auth_token {
        use std::fmt::Write;
        let _ = write!(token_hex, "{b:02x}");
    }
    let content = format!("{}\n{}\n{}\n", session.addr, session.fingerprint, token_hex);
    let session_path = dir.join(format!("{}.session", session.fingerprint));
    fs::write(&session_path, content)?;
    restrict_file_mode(&session_path);
    let latest = dir.join("latest");
    fs::write(&latest, session.fingerprint.to_string())?;
    restrict_file_mode(&latest);
    Ok(())
}

fn restrict_file_mode(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

#[must_use]
pub fn load_session(fingerprint: Option<PeerFingerprint>) -> Option<StoredSession> {
    let dir = config_dir().ok()?;
    let fp = if let Some(fp) = fingerprint {
        fp
    } else {
        let latest = fs::read_to_string(dir.join("latest")).ok()?;
        latest.trim().parse().ok()?
    };
    let path = dir.join(format!("{fp}.session"));
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let addr: SocketAddr = lines.next()?.parse().ok()?;
    let fp_parsed: PeerFingerprint = lines.next()?.parse().ok()?;
    let token_hex = lines.next()?;
    if token_hex.len() != 64 {
        return None;
    }
    let mut token = [0u8; 32];
    for (i, chunk) in token_hex.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(chunk).ok()?;
        token[i] = u8::from_str_radix(hex, 16).ok()?;
    }
    Some(StoredSession {
        addr,
        fingerprint: fp_parsed,
        auth_token: token,
    })
}

fn config_dir() -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
    Ok(PathBuf::from(home).join(".config/farbus"))
}
