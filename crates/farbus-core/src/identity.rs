use crate::fingerprint::PeerFingerprint;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const PIN_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct Identity {
    pub fingerprint: PeerFingerprint,
    secret: [u8; 32],
}

impl Identity {
    #[must_use]
    pub fn generate() -> Self {
        let mut secret = [0u8; 32];
        rand::thread_rng().fill(&mut secret);
        let fingerprint = fingerprint_from_secret(&secret);
        Self {
            fingerprint,
            secret,
        }
    }

    #[must_use]
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self {
            fingerprint: fingerprint_from_secret(&secret),
            secret,
        }
    }

    #[must_use]
    pub fn secret(&self) -> [u8; 32] {
        self.secret
    }
}

#[must_use]
pub fn fingerprint_from_secret(secret: &[u8; 32]) -> PeerFingerprint {
    let digest: [u8; 32] = Sha256::digest(secret).into();
    PeerFingerprint::new(digest)
}

#[must_use]
pub fn hash_pin(pin: &str, server: PeerFingerprint) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"farbus-pin-v1");
    hasher.update(server.as_bytes());
    hasher.update(pin.as_bytes());
    hasher.finalize().into()
}

#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (left, right) in a.iter().zip(b) {
        diff |= left ^ right;
    }
    diff == 0
}

#[derive(Debug, Clone)]
pub struct PairingPin {
    pub pin: String,
    pub hash: [u8; 32],
    expires_at: Instant,
    attempts: u8,
    consumed: bool,
}

impl PairingPin {
    #[must_use]
    pub fn issue(server: PeerFingerprint) -> Self {
        let pin = format!("{:06}", rand::thread_rng().gen_range(100_000..1_000_000));
        Self {
            hash: hash_pin(&pin, server),
            pin,
            expires_at: Instant::now() + PIN_TTL,
            attempts: 0,
            consumed: false,
        }
    }

    /// Constant-time PIN check with a five-attempt lockout.
    #[must_use]
    pub fn is_valid(&mut self, candidate_hash: &[u8; 32]) -> bool {
        if self.consumed || self.attempts >= 5 || Instant::now() > self.expires_at {
            return false;
        }
        if constant_time_eq(&self.hash, candidate_hash) {
            self.consumed = true;
            true
        } else {
            self.attempts = self.attempts.saturating_add(1);
            false
        }
    }
}

#[must_use]
pub fn issue_auth_token() -> [u8; 32] {
    let mut token = [0u8; 32];
    rand::thread_rng().fill(&mut token);
    token
}
