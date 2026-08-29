use farbus_core::{hash_pin, simulated_lab_devices, PairingPin, PeerFingerprint, ServerState};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn expired_and_consumed_pins_publish_replacements() {
    let server = PeerFingerprint::new([7u8; 32]);
    let state = Arc::new(ServerState::new(
        "farbus-server".into(),
        server,
        simulated_lab_devices(),
    ));
    *state.pin.lock().await = PairingPin::issue_with_ttl(server, Duration::from_millis(20));
    let stale_pin = state.pin.lock().await.pin.clone();
    let mut updates = state.subscribe_pairing_pins();

    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(state.renew_expired_pin().await.is_some());

    let renewed = updates.recv().await.unwrap();
    assert!(
        !state
            .validate_pairing_pin(&hash_pin(&stale_pin, server))
            .await
    );
    assert_ne!(renewed, stale_pin);
    assert!(
        state
            .validate_pairing_pin(&hash_pin(&renewed, server))
            .await
    );

    let rotated = updates.recv().await.unwrap();
    assert_ne!(rotated, renewed);
}
