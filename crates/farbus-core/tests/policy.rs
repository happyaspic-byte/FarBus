use farbus_core::{
    connection_order, ConnectionEvent, ConnectionMachine, ConnectionState, LeaseBook, LeaseError,
    PeerFingerprint,
};
use farbus_protocol::DeviceId;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;

fn fingerprint(seed: u8) -> PeerFingerprint {
    PeerFingerprint::new([seed; 32])
}

#[test]
fn fingerprint_text_roundtrips() {
    let original = fingerprint(0xab);
    let text = original.to_string();
    assert_eq!(text.len(), 64);
    assert_eq!(PeerFingerprint::from_str(&text).unwrap(), original);
}

#[test]
fn fingerprint_rejects_non_hex_or_wrong_length() {
    assert!(PeerFingerprint::from_str("abcd").is_err());
    assert!(PeerFingerprint::from_str(&"z".repeat(64)).is_err());
}

#[test]
fn lease_is_exclusive_and_reentrant_for_same_peer() {
    let mut leases = LeaseBook::default();
    let device = DeviceId(9);
    let alice = fingerprint(1);
    let bob = fingerprint(2);

    assert!(leases.acquire(device, alice).is_ok());
    assert!(leases.acquire(device, alice).is_ok());
    assert_eq!(leases.owner(device), Some(alice));
    assert_eq!(
        leases.acquire(device, bob),
        Err(LeaseError::AlreadyLeased { owner: alice })
    );
}

#[test]
fn only_owner_can_release_lease() {
    let mut leases = LeaseBook::default();
    let device = DeviceId(3);
    let alice = fingerprint(1);
    let bob = fingerprint(2);
    leases.acquire(device, alice).unwrap();

    assert_eq!(
        leases.release(device, bob),
        Err(LeaseError::NotOwner { owner: alice })
    );
    assert!(leases.release(device, alice).is_ok());
    assert_eq!(leases.owner(device), None);
}

#[test]
fn connection_order_interleaves_ipv6_and_ipv4_with_ipv6_first() {
    let v4a = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 7420);
    let v4b = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)), 7420);
    let v6a = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 7420);
    let v6b = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 2)), 7420);

    assert_eq!(
        connection_order([v4a, v6a, v4b, v6b]),
        vec![v6a, v4a, v6b, v4b]
    );
}

#[test]
fn connection_machine_follows_pair_attach_disconnect_reconnect_flow() {
    let mut machine = ConnectionMachine::default();
    assert_eq!(machine.state(), ConnectionState::Discovered);

    machine.apply(ConnectionEvent::Pair).unwrap();
    assert_eq!(machine.state(), ConnectionState::Paired);
    machine.apply(ConnectionEvent::Attach).unwrap();
    assert_eq!(machine.state(), ConnectionState::Attached);
    machine.apply(ConnectionEvent::ConnectionLost).unwrap();
    assert_eq!(machine.state(), ConnectionState::Reconnecting);
    machine.apply(ConnectionEvent::ReconnectSucceeded).unwrap();
    assert_eq!(machine.state(), ConnectionState::Attached);
    machine.apply(ConnectionEvent::Detach).unwrap();
    assert_eq!(machine.state(), ConnectionState::Paired);
}

#[test]
fn connection_machine_rejects_attach_before_pairing() {
    let mut machine = ConnectionMachine::default();
    let error = machine.apply(ConnectionEvent::Attach).unwrap_err();
    assert_eq!(error.from, ConnectionState::Discovered);
    assert_eq!(error.event, ConnectionEvent::Attach);
    assert_eq!(machine.state(), ConnectionState::Discovered);
}
