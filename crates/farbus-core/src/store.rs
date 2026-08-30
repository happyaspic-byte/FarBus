use crate::fingerprint::PeerFingerprint;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct StoredSession {
    pub addr: SocketAddr,
    pub fingerprint: PeerFingerprint,
    pub auth_token: [u8; 32],
    pub client_secret: Option<[u8; 32]>,
}

/// Saves the session connection details to disk.
///
/// # Errors
///
/// Returns I/O errors when the configuration directory cannot be written.
pub fn save_session(session: &StoredSession) -> std::io::Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    let mut token_hex = String::new();
    for b in session.auth_token {
        use std::fmt::Write;
        let _ = write!(token_hex, "{b:02x}");
    }
    let mut content = format!("{}\n{}\n{}\n", session.addr, session.fingerprint, token_hex);
    if let Some(secret) = session.client_secret {
        let mut secret_hex = String::new();
        for b in secret {
            use std::fmt::Write;
            let _ = write!(secret_hex, "{b:02x}");
        }
        content.push_str(&secret_hex);
        content.push('\n');
    }
    let session_path = dir.join(format!("{}.session", session.fingerprint));
    write_private_atomic(&session_path, content.as_bytes())?;
    let latest = dir.join("latest");
    write_private_atomic(&latest, session.fingerprint.to_string().as_bytes())?;
    Ok(())
}

fn write_private_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o600);
        let mut file = opts.open(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        fs::write(&tmp, data)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
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
    let token = parse_hex32(lines.next()?)?;
    let client_secret = lines.next().and_then(parse_hex32);
    Some(StoredSession {
        addr,
        fingerprint: fp_parsed,
        auth_token: token,
        client_secret,
    })
}

fn parse_hex32(token_hex: &str) -> Option<[u8; 32]> {
    if token_hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in token_hex.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(hex, 16).ok()?;
    }
    Some(out)
}

fn config_dir() -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
    Ok(PathBuf::from(home).join(".config/farbus"))
}
