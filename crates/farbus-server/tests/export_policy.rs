use farbus_core::simulated_lab_devices;
use farbus_server::{apply_export_flags, apply_export_policy};

#[test]
fn export_all_excludes_sensitive_classes() {
    let mut devices = simulated_lab_devices();
    for device in &mut devices {
        device.info.exported = false;
    }

    apply_export_policy(&mut devices, true, &[]);

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
fn export_all_denies_class_zero_device_with_unknown_interfaces() {
    let mut devices = simulated_lab_devices();
    let device = &mut devices[0];
    device.info.usb_class = 0;
    device.info.interfaces.clear();

    apply_export_policy(std::slice::from_mut(device), true, &[]);

    assert!(!device.info.exported);
}

#[test]
fn exact_export_allows_sensitive_device() {
    let mut devices = simulated_lab_devices();
    for device in &mut devices {
        device.info.exported = false;
    }

    apply_export_policy(&mut devices, false, &["1-1.2".into()]);

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
fn absent_exact_export_is_applied_when_device_arrives_later() {
    let exact = vec!["5-2".to_string()];
    let mut devices = Vec::new();
    apply_export_policy(&mut devices, false, &exact);

    let mut arrived = simulated_lab_devices();
    arrived[1].info.bus_id = "5-2".into();
    apply_export_flags(&mut arrived, false, &exact);

    assert!(arrived[1].info.exported);
    assert!(arrived
        .iter()
        .enumerate()
        .all(|(index, device)| index == 1 || !device.info.exported));
}
