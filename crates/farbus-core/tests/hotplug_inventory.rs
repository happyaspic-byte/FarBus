use farbus_core::{
    DeviceBackend, DeviceId, DeviceInventory, LocalDevice, PeerFingerprint, ServerState,
};
use farbus_protocol::{DeviceInfo, UsbSpeed};
use std::sync::Arc;

fn host(bus_id: &str, vid: u16, pid: u16) -> LocalDevice {
    LocalDevice {
        info: DeviceInfo {
            id: DeviceId(0),
            bus_id: bus_id.into(),
            vid,
            pid,
            usb_class: 0xff,
            speed: UsbSpeed::High,
            product: bus_id.into(),
            exported: true,
            interfaces: Vec::new(),
        },
        backend: DeviceBackend::Host,
    }
}

#[test]
fn refresh_preserves_ids_across_scan_order_changes() {
    let mut inventory = DeviceInventory::new(vec![host("1-2", 1, 2), host("1-3", 3, 4)]);
    let first = inventory.snapshot();
    let id_12 = first
        .iter()
        .find(|d| d.info.bus_id == "1-2")
        .unwrap()
        .info
        .id;
    let id_13 = first
        .iter()
        .find(|d| d.info.bus_id == "1-3")
        .unwrap()
        .info
        .id;

    let delta = inventory.refresh_hosts(vec![host("1-3", 3, 4), host("1-2", 1, 2)]);

    assert!(delta.added.is_empty());
    assert!(delta.removed.is_empty());
    let refreshed = inventory.snapshot();
    assert_eq!(
        refreshed
            .iter()
            .find(|d| d.info.bus_id == "1-2")
            .unwrap()
            .info
            .id,
        id_12
    );
    assert_eq!(
        refreshed
            .iter()
            .find(|d| d.info.bus_id == "1-3")
            .unwrap()
            .info
            .id,
        id_13
    );
}

#[test]
fn refresh_reports_removal_and_replug_gets_new_identity() {
    let mut inventory = DeviceInventory::new(vec![host("1-2", 1, 2)]);
    let id = inventory.snapshot()[0].info.id;

    let removed = inventory.refresh_hosts(Vec::new());
    assert_eq!(removed.removed, vec![id]);
    assert!(inventory.snapshot().is_empty());

    let added = inventory.refresh_hosts(vec![host("1-2", 1, 2)]);
    assert_eq!(added.added.len(), 1);
    assert_ne!(added.added[0], id);
}

#[test]
fn initial_inventory_preserves_explicit_nonzero_ids() {
    let mut device = host("9-9", 1, 2);
    device.info.id = DeviceId(99);
    let inventory = DeviceInventory::new(vec![device]);
    assert_eq!(inventory.snapshot()[0].info.id, DeviceId(99));
}

#[test]
fn replacement_at_same_port_gets_a_new_id() {
    let mut inventory = DeviceInventory::new(vec![host("1-2", 1, 2)]);
    let old_id = inventory.snapshot()[0].info.id;

    let delta = inventory.refresh_hosts(vec![host("1-2", 9, 9)]);

    assert_eq!(delta.removed, vec![old_id]);
    assert_eq!(delta.added.len(), 1);
    assert_ne!(delta.added[0], old_id);
}

#[tokio::test]
async fn removing_device_revokes_lease_and_replug_requires_attach() {
    let state = Arc::new(ServerState::new(
        "test".into(),
        PeerFingerprint::new([1; 32]),
        vec![host("1-2", 1, 2)],
    ));
    let id = state.devices_snapshot().await[0].info.id;
    let owner = PeerFingerprint::new([2; 32]);
    state.leases.lock().await.acquire(id, owner).unwrap();

    let delta = state.refresh_host_devices(Vec::new()).await;
    assert_eq!(delta.removed, vec![id]);
    assert_eq!(state.leases.lock().await.owner(id), None);

    let added = state.refresh_host_devices(vec![host("1-2", 1, 2)]).await;
    assert_eq!(added.added.len(), 1);
    assert_ne!(added.added[0], id);
    assert_eq!(state.leases.lock().await.owner(added.added[0]), None);
}
