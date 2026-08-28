use farbus_core::{parse_sysfs_device, simulated_lab_devices, DeviceId};
use farbus_server::{apply_export_flags, apply_export_policy};
use std::fs;
use std::path::Path;

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

fn write_hex(path: &Path, name: &str, value: &str) {
    fs::write(path.join(name), format!("{value}\n")).unwrap();
}

#[test]
fn export_all_denies_sysfs_composite_hid() {
    let temp = std::env::temp_dir().join(format!(
        "farbus-export-sysfs-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let bus_id = "1-1.2";
    let dev = temp.join(bus_id);
    fs::create_dir_all(&dev).unwrap();
    write_hex(&dev, "idVendor", "046d");
    write_hex(&dev, "idProduct", "c52b");
    write_hex(&dev, "bDeviceClass", "00");
    write_hex(&dev, "speed", "12");
    fs::write(dev.join("product"), "Composite Receiver\n").unwrap();

    let iface0 = dev.join(format!("{bus_id}:1.0"));
    fs::create_dir_all(&iface0).unwrap();
    write_hex(&iface0, "bInterfaceNumber", "00");
    write_hex(&iface0, "bInterfaceClass", "03");
    write_hex(&iface0, "bInterfaceSubClass", "01");
    write_hex(&iface0, "bInterfaceProtocol", "01");

    let iface1 = dev.join(format!("{bus_id}:1.1"));
    fs::create_dir_all(&iface1).unwrap();
    write_hex(&iface1, "bInterfaceNumber", "01");
    write_hex(&iface1, "bInterfaceClass", "03");
    write_hex(&iface1, "bInterfaceSubClass", "01");
    write_hex(&iface1, "bInterfaceProtocol", "02");

    let parsed = parse_sysfs_device(&dev, DeviceId(4)).unwrap();
    let _ = fs::remove_dir_all(&temp);

    let mut devices = vec![parsed];
    apply_export_policy(&mut devices, true, &[]);
    assert!(!devices[0].info.exported);
}
