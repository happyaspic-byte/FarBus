use farbus_core::{
    decode_beacon, encode_beacon, parse_sysfs_device, DeviceId, PeerFingerprint, UsbSpeed,
};
use std::fs;
use std::net::SocketAddr;

#[test]
fn parses_linux_sysfs_usb_device() {
    let temp = std::env::temp_dir().join(format!("farbus-usb-{}", std::process::id()));
    let dev = temp.join("1-2.3");
    fs::create_dir_all(&dev).unwrap();
    fs::write(dev.join("idVendor"), "046d\n").unwrap();
    fs::write(dev.join("idProduct"), "c31c\n").unwrap();
    fs::write(dev.join("bDeviceClass"), "03\n").unwrap();
    fs::write(dev.join("speed"), "480\n").unwrap();
    fs::write(dev.join("product"), "USB Keyboard\n").unwrap();

    let parsed = parse_sysfs_device(&dev, DeviceId(7)).unwrap();
    assert_eq!(parsed.info.id, DeviceId(7));
    assert_eq!(parsed.info.bus_id, "1-2.3");
    assert_eq!(parsed.info.vid, 0x046d);
    assert_eq!(parsed.info.pid, 0xc31c);
    assert_eq!(parsed.info.usb_class, 3);
    assert_eq!(parsed.info.speed, UsbSpeed::High);
    assert_eq!(parsed.info.product, "USB Keyboard");
    assert!(!parsed.info.exported);
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn discovery_beacon_roundtrips_ipv6() {
    let fp = PeerFingerprint::new([0x42; 32]);
    let addr: SocketAddr = "[fe80::1234]:7420".parse().unwrap();
    let bytes = encode_beacon(fp, addr, "farbus-pi");
    let (decoded_fp, decoded_addr, host) = decode_beacon(&bytes).unwrap();
    assert_eq!(decoded_fp, fp);
    assert_eq!(decoded_addr, addr);
    assert_eq!(host, "farbus-pi");
}

#[test]
fn discovery_rejects_malformed_beacons() {
    assert!(decode_beacon(b"bad").is_none());
    let fp = PeerFingerprint::new([1; 32]);
    let bytes = encode_beacon(fp, "127.0.0.1:7420".parse().unwrap(), "lab");
    assert!(decode_beacon(&bytes[..bytes.len() - 1]).is_none());
}
