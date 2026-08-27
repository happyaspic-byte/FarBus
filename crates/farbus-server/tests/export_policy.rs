use farbus_core::simulated_lab_devices;
use farbus_server::{apply_export_policy, ExportPolicyError};

#[test]
fn export_all_excludes_sensitive_classes() {
    let mut devices = simulated_lab_devices();
    for device in &mut devices {
        device.info.exported = false;
    }

    apply_export_policy(&mut devices, true, &[]).unwrap();

    assert!(
        !devices
            .iter()
            .find(|d| d.info.bus_id == "1-1.2")
            .unwrap()
            .info
            .exported
    );
    assert!(
        devices
            .iter()
            .find(|d| d.info.bus_id == "1-2")
            .unwrap()
            .info
            .exported
    );
    assert!(
        !devices
            .iter()
            .find(|d| d.info.bus_id == "2-1")
            .unwrap()
            .info
            .exported
    );
    assert!(
        !devices
            .iter()
            .find(|d| d.info.bus_id == "1-4")
            .unwrap()
            .info
            .exported
    );
}

#[test]
fn exact_export_allows_sensitive_device() {
    let mut devices = simulated_lab_devices();
    for device in &mut devices {
        device.info.exported = false;
    }

    apply_export_policy(&mut devices, false, &["1-1.2".into()]).unwrap();

    assert!(
        devices
            .iter()
            .find(|d| d.info.bus_id == "1-1.2")
            .unwrap()
            .info
            .exported
    );
    assert_eq!(devices.iter().filter(|d| d.info.exported).count(), 1);
}

#[test]
fn unmatched_exact_export_fails_closed() {
    let mut devices = simulated_lab_devices();
    for device in &mut devices {
        device.info.exported = false;
    }

    let err = apply_export_policy(&mut devices, false, &["missing".into()]).unwrap_err();
    assert!(matches!(err, ExportPolicyError::NotFound(bus) if bus == "missing"));
    assert!(devices.iter().all(|d| !d.info.exported));
}
