use farbus_protocol::{DeviceId, DeviceInfo, UsbInterfaceInfo, UsbSpeed};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceBackend {
    Emulated,
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDevice {
    pub info: DeviceInfo,
    pub backend: DeviceBackend,
}

pub fn parse_sysfs_device(dir: &Path, id: DeviceId) -> Option<LocalDevice> {
    let vid = read_hex_u16(&dir.join("idVendor"))?;
    let pid = read_hex_u16(&dir.join("idProduct"))?;
    let usb_class = read_hex_u8(&dir.join("bDeviceClass")).unwrap_or(0);
    let product = fs::read_to_string(dir.join("product"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{vid:04x}:{pid:04x}"));
    let speed = match fs::read_to_string(dir.join("speed"))
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some("1.5") => UsbSpeed::Low,
        Some("12") => UsbSpeed::Full,
        Some("5000" | "10000" | "20000") => UsbSpeed::Super,
        _ => UsbSpeed::High,
    };
    let bus_id = dir.file_name()?.to_string_lossy().into_owned();
    if !bus_id.contains('-') {
        return None;
    }
    Some(LocalDevice {
        info: DeviceInfo {
            id,
            bus_id,
            vid,
            pid,
            usb_class,
            speed,
            product,
            exported: false,
            interfaces: vec![UsbInterfaceInfo {
                interface_number: 0,
                interface_class: usb_class,
                interface_subclass: 0,
                interface_protocol: 0,
                endpoints: Vec::new(),
            }],
        },
        backend: DeviceBackend::Host,
    })
}

#[must_use]
pub fn scan_sysfs(root: &Path) -> Vec<LocalDevice> {
    let mut devices = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return devices;
    };
    let mut next_id = 1u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(device) = parse_sysfs_device(&path, DeviceId(next_id)) {
            devices.push(device);
            next_id += 1;
        }
    }
    devices
}

#[must_use]
pub fn scan_host_usb() -> Vec<LocalDevice> {
    #[cfg(target_os = "linux")]
    {
        let libusb = crate::host_usb::scan_libusb();
        if !libusb.is_empty() {
            return libusb;
        }
    }
    scan_sysfs(&PathBuf::from("/sys/bus/usb/devices"))
}

#[must_use]
pub fn simulated_lab_devices() -> Vec<LocalDevice> {
    vec![
        LocalDevice {
            info: DeviceInfo {
                id: DeviceId(1),
                bus_id: "1-1.2".into(),
                vid: 0x046d,
                pid: 0xc31c,
                usb_class: 3,
                speed: UsbSpeed::Full,
                product: "USB Keyboard".into(),
                exported: true,
                interfaces: vec![UsbInterfaceInfo {
                    interface_number: 0,
                    interface_class: 3,
                    interface_subclass: 1,
                    interface_protocol: 1,
                    endpoints: vec![0x81],
                }],
            },
            backend: DeviceBackend::Emulated,
        },
        LocalDevice {
            info: DeviceInfo {
                id: DeviceId(2),
                bus_id: "1-2".into(),
                vid: 0x0403,
                pid: 0x6001,
                usb_class: 255,
                speed: UsbSpeed::Full,
                product: "FT232 Serial".into(),
                exported: true,
                interfaces: vec![UsbInterfaceInfo {
                    interface_number: 0,
                    interface_class: 255,
                    interface_subclass: 255,
                    interface_protocol: 255,
                    endpoints: vec![0x81, 0x02],
                }],
            },
            backend: DeviceBackend::Emulated,
        },
        LocalDevice {
            info: DeviceInfo {
                id: DeviceId(3),
                bus_id: "2-1".into(),
                vid: 0x0781,
                pid: 0x5567,
                usb_class: 8,
                speed: UsbSpeed::High,
                product: "USB Disk".into(),
                exported: true,
                interfaces: vec![UsbInterfaceInfo {
                    interface_number: 0,
                    interface_class: 8,
                    interface_subclass: 6,
                    interface_protocol: 0x50,
                    endpoints: vec![0x81, 0x02],
                }],
            },
            backend: DeviceBackend::Emulated,
        },
        LocalDevice {
            info: DeviceInfo {
                id: DeviceId(4),
                bus_id: "1-4".into(),
                vid: 0x046d,
                pid: 0xc52b,
                usb_class: 0,
                speed: UsbSpeed::Full,
                product: "Composite Receiver".into(),
                exported: true,
                interfaces: vec![
                    UsbInterfaceInfo {
                        interface_number: 0,
                        interface_class: 3,
                        interface_subclass: 1,
                        interface_protocol: 1,
                        endpoints: vec![0x81],
                    },
                    UsbInterfaceInfo {
                        interface_number: 1,
                        interface_class: 3,
                        interface_subclass: 1,
                        interface_protocol: 2,
                        endpoints: vec![0x82],
                    },
                ],
            },
            backend: DeviceBackend::Emulated,
        },
    ]
}

fn read_hex_u16(path: &Path) -> Option<u16> {
    let text = fs::read_to_string(path).ok()?;
    u16::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

fn read_hex_u8(path: &Path) -> Option<u8> {
    let text = fs::read_to_string(path).ok()?;
    u8::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}
