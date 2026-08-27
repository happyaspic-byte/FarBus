use farbus_core::{hash_pin, PairingPin, PeerFingerprint};

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
fn pairing_pin_accepts_correct_hash_within_budget() {
    let fp = PeerFingerprint::new([3; 32]);
    let mut pin = PairingPin::issue(fp);
    let correct = hash_pin(&pin.pin, fp);
    assert!(pin.is_valid(&correct));
}
