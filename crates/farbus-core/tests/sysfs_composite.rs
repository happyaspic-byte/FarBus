use farbus_core::{parse_sysfs_device, DeviceId};
use std::fs;
use std::path::Path;

fn write_hex(path: &Path, name: &str, value: &str) {
    fs::write(path.join(name), format!("{value}\n")).unwrap();
}

fn mock_composite_hid(root: &Path) {
    let bus_id = "1-1.2";
    let dev = root.join(bus_id);
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
    let ep81 = iface0.join("ep_81");
    fs::create_dir_all(&ep81).unwrap();
    write_hex(&ep81, "bEndpointAddress", "81");

    let iface1 = dev.join(format!("{bus_id}:1.1"));
    fs::create_dir_all(&iface1).unwrap();
    write_hex(&iface1, "bInterfaceNumber", "01");
    write_hex(&iface1, "bInterfaceClass", "03");
    write_hex(&iface1, "bInterfaceSubClass", "01");
    write_hex(&iface1, "bInterfaceProtocol", "02");
    let ep82 = iface1.join("ep_82");
    fs::create_dir_all(&ep82).unwrap();
    write_hex(&ep82, "bEndpointAddress", "82");
}

#[test]
fn parses_sysfs_composite_hid_interfaces() {
    let temp = std::env::temp_dir().join(format!(
        "farbus-sysfs-composite-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp).unwrap();
    mock_composite_hid(&temp);

    let parsed = parse_sysfs_device(&temp.join("1-1.2"), DeviceId(4)).unwrap();
    let _ = fs::remove_dir_all(&temp);

    assert_eq!(parsed.info.usb_class, 0);
    assert_eq!(parsed.info.interfaces.len(), 2);
    assert_eq!(parsed.info.interfaces[0].interface_number, 0);
    assert_eq!(parsed.info.interfaces[0].interface_class, 3);
    assert_eq!(parsed.info.interfaces[0].interface_subclass, 1);
    assert_eq!(parsed.info.interfaces[0].interface_protocol, 1);
    assert_eq!(parsed.info.interfaces[0].endpoints, vec![0x81]);
    assert_eq!(parsed.info.interfaces[1].interface_number, 1);
    assert_eq!(parsed.info.interfaces[1].interface_class, 3);
    assert_eq!(parsed.info.interfaces[1].endpoints, vec![0x82]);
}

#[test]
fn class_zero_device_without_interface_dirs_keeps_empty_interfaces() {
    let temp = std::env::temp_dir().join(format!(
        "farbus-sysfs-class0-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let dev = temp.join("3-1");
    fs::create_dir_all(&dev).unwrap();
    write_hex(&dev, "idVendor", "1234");
    write_hex(&dev, "idProduct", "5678");
    write_hex(&dev, "bDeviceClass", "00");
    write_hex(&dev, "speed", "12");
    fs::write(dev.join("product"), "Unknown Composite\n").unwrap();

    let parsed = parse_sysfs_device(&dev, DeviceId(9)).unwrap();
    let _ = fs::remove_dir_all(&temp);

    assert_eq!(parsed.info.usb_class, 0);
    assert!(parsed.info.interfaces.is_empty());
}
