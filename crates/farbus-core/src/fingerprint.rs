use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerFingerprint([u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FingerprintError {
    #[error("fingerprint must be 64 hex characters")]
    BadLength,
    #[error("fingerprint must be lowercase hex")]
    BadHex,
}

impl PeerFingerprint {
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for PeerFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for PeerFingerprint {
    type Err = FingerprintError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 {
            return Err(FingerprintError::BadLength);
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
            let hex = std::str::from_utf8(chunk).map_err(|_| FingerprintError::BadHex)?;
            bytes[i] = u8::from_str_radix(hex, 16).map_err(|_| FingerprintError::BadHex)?;
        }
        Ok(Self(bytes))
    }
}
