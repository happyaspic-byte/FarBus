use std::net::SocketAddr;

/// IPv6-first Happy Eyeballs order: first IPv6, first IPv4, remaining IPv6, remaining IPv4.
#[must_use]
pub fn connection_order(addrs: impl IntoIterator<Item = SocketAddr>) -> Vec<SocketAddr> {
    let mut v6 = Vec::new();
    let mut v4 = Vec::new();
    for addr in addrs {
        if addr.is_ipv6() {
            v6.push(addr);
        } else {
            v4.push(addr);
        }
    }

    let mut out = Vec::with_capacity(v6.len() + v4.len());
    let mut v6 = v6.into_iter();
    let mut v4 = v4.into_iter();
    if let Some(addr) = v6.next() {
        out.push(addr);
    }
    if let Some(addr) = v4.next() {
        out.push(addr);
    }
    out.extend(v6);
    out.extend(v4);
    out
}
