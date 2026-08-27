use crate::fingerprint::PeerFingerprint;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};

pub const DISCOVERY_PORT: u16 = 7421;
const MAGIC: &[u8] = b"FARBD1";

#[must_use]
pub fn encode_beacon(fingerprint: PeerFingerprint, listen: SocketAddr, hostname: &str) -> Vec<u8> {
    let mut out = Vec::from(MAGIC);
    out.extend_from_slice(fingerprint.as_bytes());
    let addr = listen.to_string();
    let addr_len = u8::try_from(addr.len()).unwrap_or(0);
    out.push(addr_len);
    out.extend_from_slice(&addr.as_bytes()[..usize::from(addr_len)]);
    let host_len = u8::try_from(hostname.len().min(255)).unwrap_or(0);
    out.push(host_len);
    out.extend_from_slice(&hostname.as_bytes()[..usize::from(host_len)]);
    out
}

#[must_use]
pub fn decode_beacon(bytes: &[u8]) -> Option<(PeerFingerprint, SocketAddr, String)> {
    if bytes.len() < MAGIC.len() + 32 + 2 {
        return None;
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return None;
    }
    let mut fp = [0u8; 32];
    fp.copy_from_slice(&bytes[MAGIC.len()..MAGIC.len() + 32]);
    let mut cur = MAGIC.len() + 32;
    let addr_len = usize::from(*bytes.get(cur)?);
    cur += 1;
    let addr = std::str::from_utf8(bytes.get(cur..cur + addr_len)?).ok()?;
    let listen = addr.parse().ok()?;
    cur += addr_len;
    let host_len = usize::from(*bytes.get(cur)?);
    cur += 1;
    let hostname = std::str::from_utf8(bytes.get(cur..cur + host_len)?)
        .ok()?
        .to_string();
    Some((PeerFingerprint::new(fp), listen, hostname))
}

/// Broadcasts a discovery beacon once.
///
/// # Errors
///
/// Returns I/O errors from the UDP socket.
pub async fn announce(
    fingerprint: PeerFingerprint,
    listen: SocketAddr,
    hostname: &str,
) -> std::io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;
    let payload = encode_beacon(fingerprint, listen, hostname);
    let _ = socket
        .send_to(&payload, ("255.255.255.255", DISCOVERY_PORT))
        .await;
    let _ = socket
        .send_to(&payload, ("[ff02::1]", DISCOVERY_PORT))
        .await;
    Ok(())
}

/// Collects discovery beacons for `wait`.
///
/// # Errors
///
/// Returns I/O errors from the UDP socket.
pub async fn collect(
    wait: Duration,
) -> std::io::Result<Vec<(PeerFingerprint, SocketAddr, String)>> {
    let socket = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await {
        Ok(s) => s,
        Err(_) => UdpSocket::bind(("127.0.0.1", DISCOVERY_PORT)).await?,
    };
    let mut found = Vec::new();
    let deadline = timeout(wait, async {
        let mut buf = [0u8; 512];
        loop {
            if let Ok((n, _)) = socket.recv_from(&mut buf).await {
                if let Some(entry) = decode_beacon(&buf[..n]) {
                    if !found.iter().any(|(fp, _, _)| fp == &entry.0) {
                        found.push(entry);
                    }
                }
            }
        }
    });
    let _ = deadline.await;
    Ok(found)
}
