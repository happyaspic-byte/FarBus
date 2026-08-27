use crate::client::{ClientError, FarBusClient};
use crate::fingerprint::PeerFingerprint;
use crate::path::connection_order;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Races IPv6-first candidate addresses with a 250 ms stagger.
///
/// # Errors
///
/// Returns the last connection error if every candidate fails.
pub async fn happy_eyeballs_connect(
    addrs: impl IntoIterator<Item = SocketAddr>,
    expected: PeerFingerprint,
) -> Result<FarBusClient, ClientError> {
    let ordered = connection_order(addrs);
    if ordered.is_empty() {
        return Err(ClientError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no server addresses",
        )));
    }
    let mut tasks = Vec::new();
    for (i, addr) in ordered.into_iter().enumerate() {
        let delay = Duration::from_millis(250 * u64::try_from(i).unwrap_or(0));
        tasks.push(tokio::spawn(async move {
            if !delay.is_zero() {
                sleep(delay).await;
            }
            timeout(
                Duration::from_millis(1500),
                FarBusClient::connect(addr, expected),
            )
            .await
        }));
    }
    let mut last = None;
    for task in tasks {
        match task.await {
            Ok(Ok(Ok(client))) => return Ok(client),
            Ok(Ok(Err(err))) => last = Some(err),
            Ok(Err(_)) => {
                last = Some(ClientError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "connect timeout",
                )));
            }
            Err(_) => {}
        }
    }
    Err(last.unwrap_or(ClientError::Tls))
}
