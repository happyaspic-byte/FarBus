use farbus_core::{hash_pin, PairingPin, PeerFingerprint};
use std::time::Duration;

#[test]
fn pairing_pin_rejects_after_five_failures() {
    let fp = PeerFingerprint::new([9; 32]);
    let mut pin = PairingPin::issue(fp);
    let wrong = hash_pin("000000", fp);
    for _ in 0..5 {
        let _ = pin.is_valid(&wrong);
    }
    let correct = hash_pin(&pin.pin, fp);
    assert!(!pin.is_valid(&correct));
}

#[test]
fn pairing_pin_is_single_use_on_success() {
    let fp = PeerFingerprint::new([4; 32]);
    let mut pin = PairingPin::issue(fp);
    let correct = hash_pin(&pin.pin, fp);
    assert!(pin.is_valid(&correct));
    assert!(!pin.is_valid(&correct));
}

#[test]
fn expired_pin_reissues_and_new_pin_pairs() {
    let server = PeerFingerprint::new([7u8; 32]);
    let mut pin = PairingPin::issue_with_ttl(server, Duration::from_millis(30));
    std::thread::sleep(Duration::from_millis(60));

    let stale_hash = hash_pin(&pin.pin.clone(), server);
    assert!(!pin.is_valid(&stale_hash));

    let stale_pin = pin.pin.clone();
    let new_pin = pin
        .reissue_if_expired(server)
        .expect("expired pin reissues");
    assert_ne!(new_pin, stale_pin);
    assert!(pin.is_valid(&hash_pin(&new_pin, server)));
}

#[test]
fn active_pin_is_not_reissued() {
    let server = PeerFingerprint::new([9u8; 32]);
    let mut pin = PairingPin::issue(server);

    let reissued = pin.reissue_if_expired(server);
    assert!(reissued.is_none());
}

#[test]
fn pairing_pin_accepts_correct_hash_within_budget() {
    let fp = PeerFingerprint::new([3; 32]);
    let mut pin = PairingPin::issue(fp);
    let correct = hash_pin(&pin.pin, fp);
    assert!(pin.is_valid(&correct));
}
