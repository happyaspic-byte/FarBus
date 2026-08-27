use farbus_core::persist::{identity_dir, load_or_create_server_identity};
use std::fs;

#[test]
fn persisted_server_identity_is_stable_across_loads() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = identity_dir().unwrap();
    let _ = fs::remove_dir_all(&dir);
    let first = load_or_create_server_identity("farbus.local").unwrap();
    let second = load_or_create_server_identity("farbus.local").unwrap();
    assert_eq!(first.2, second.2);
    let _ = fs::remove_dir_all(&dir);
}
