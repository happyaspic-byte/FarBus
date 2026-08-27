use crate::client::{ClientError, FarBusClient};
use crate::fingerprint::PeerFingerprint;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    pub initial: Duration,
    pub max: Duration,
    pub max_attempts: usize,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(100),
            max: Duration::from_secs(3),
            max_attempts: 10,
        }
    }
}

/// Connects to a server with bounded exponential backoff retries.
///
/// # Errors
///
/// Returns the last [`ClientError`] if all attempts fail.
pub async fn connect_with_retry(
    addr: SocketAddr,
    expected: PeerFingerprint,
    token: Option<[u8; 32]>,
    policy: &ReconnectPolicy,
) -> Result<FarBusClient, ClientError> {
    let mut backoff = policy.initial;
    let mut last_err = None;
    for _ in 0..policy.max_attempts {
        match FarBusClient::connect(addr, expected).await {
            Ok(client) => {
                return Ok(match token {
                    Some(tok) => client.with_auth_token(tok),
                    None => client,
                });
            }
            Err(err) => {
                last_err = Some(err);
                sleep(backoff).await;
                backoff = (backoff * 2).min(policy.max);
            }
        }
    }
    Err(last_err.unwrap_or(ClientError::Tls))
}
