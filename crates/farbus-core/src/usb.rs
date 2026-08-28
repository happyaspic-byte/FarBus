use farbus_protocol::{DeviceId, DeviceInfo, UsbInterfaceInfo, UsbSpeed};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceBackend {
    Emulated,
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDevice {
    pub info: DeviceInfo,
    pub backend: DeviceBackend,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DeviceKey {
    backend: DeviceBackend,
    bus_id: String,
    vid: u16,
    pid: u16,
}

impl From<&LocalDevice> for DeviceKey {
    fn from(device: &LocalDevice) -> Self {
        Self {
            backend: device.backend,
            bus_id: device.info.bus_id.clone(),
            vid: device.info.vid,
            pid: device.info.pid,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct InventoryDelta {
    pub added: Vec<DeviceId>,
    pub removed: Vec<DeviceId>,
}

#[derive(Debug)]
pub struct DeviceInventory {
    present: BTreeMap<u32, LocalDevice>,
    known_ids: HashMap<DeviceKey, DeviceId>,
    presence: HashMap<u32, Arc<AtomicBool>>,
    next_id: u32,
}

impl DeviceInventory {
    #[must_use]
    pub fn new(devices: Vec<LocalDevice>) -> Self {
        let mut inventory = Self {
            present: BTreeMap::new(),
            known_ids: HashMap::new(),
            presence: HashMap::new(),
            next_id: 1,
        };
        for mut device in devices {
            let key = DeviceKey::from(&device);
            let requested = device.info.id;
            let id = if requested.0 != 0 && !inventory.present.contains_key(&requested.0) {
                inventory.next_id = inventory.next_id.max(requested.0.saturating_add(1));
                requested
            } else {
                inventory.allocate_id()
            };
            device.info.id = id;
            inventory.known_ids.insert(key, id);
            inventory
                .presence
                .insert(id.0, Arc::new(AtomicBool::new(true)));
            inventory.present.insert(id.0, device);
        }
        inventory
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<LocalDevice> {
        self.present.values().cloned().collect()
    }

    #[must_use]
    pub fn get(&self, id: DeviceId) -> Option<LocalDevice> {
        self.present.get(&id.0).cloned()
    }

    #[must_use]
    pub fn presence_token(&self, id: DeviceId) -> Option<Arc<AtomicBool>> {
        self.presence.get(&id.0).cloned()
    }

    pub fn refresh_hosts(&mut self, scanned: Vec<LocalDevice>) -> InventoryDelta {
        let old_host_ids: Vec<DeviceId> = self
            .present
            .values()
            .filter(|device| device.backend == DeviceBackend::Host)
            .map(|device| device.info.id)
            .collect();
        let mut new_ids = Vec::new();
        let mut next_hosts = Vec::new();

        for mut device in scanned {
            let key = DeviceKey::from(&device);
            let id = if let Some(id) = self.known_ids.get(&key).copied() {
                id
            } else {
                let id = self.allocate_id();
                self.known_ids.insert(key, id);
                id
            };
            device.info.id = id;
            if !self.present.contains_key(&id.0) {
                new_ids.push(id);
            }
            next_hosts.push((id, device));
        }

        let next_set: std::collections::HashSet<u32> =
            next_hosts.iter().map(|(id, _)| id.0).collect();
        let removed: Vec<DeviceId> = old_host_ids
            .into_iter()
            .filter(|id| !next_set.contains(&id.0))
            .collect();
        for id in &removed {
            if let Some(device) = self.present.remove(&id.0) {
                self.known_ids.remove(&DeviceKey::from(&device));
            }
            if let Some(token) = self.presence.remove(&id.0) {
                token.store(false, Ordering::Release);
            }
        }
        for (id, device) in next_hosts {
            self.presence
                .entry(id.0)
                .or_insert_with(|| Arc::new(AtomicBool::new(true)));
            self.present.insert(id.0, device);
        }

        InventoryDelta {
            added: new_ids,
            removed,
        }
    }

    fn allocate_id(&mut self) -> DeviceId {
        let id = DeviceId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }
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
    let mut interfaces = parse_sysfs_interfaces(dir, &bus_id);
    if interfaces.is_empty() && usb_class != 0 {
        interfaces.push(UsbInterfaceInfo {
            interface_number: 0,
            interface_class: usb_class,
            interface_subclass: 0,
            interface_protocol: 0,
            endpoints: Vec::new(),
        });
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
            interfaces,
        },
        backend: DeviceBackend::Host,
    })
}

fn parse_sysfs_interfaces(device_dir: &Path, bus_id: &str) -> Vec<UsbInterfaceInfo> {
    let mut interfaces = Vec::new();
    let Ok(entries) = fs::read_dir(device_dir) else {
        return interfaces;
    };

    let prefix = format!("{bus_id}:");
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.contains('.') {
            continue;
        }

        let Some(interface_number) = read_hex_u8(&path.join("bInterfaceNumber")) else {
            continue;
        };
        let interface_class = read_hex_u8(&path.join("bInterfaceClass")).unwrap_or(0);
        let interface_subclass = read_hex_u8(&path.join("bInterfaceSubClass")).unwrap_or(0);
        let interface_protocol = read_hex_u8(&path.join("bInterfaceProtocol")).unwrap_or(0);
        let endpoints = parse_sysfs_endpoints(&path);

        interfaces.push(UsbInterfaceInfo {
            interface_number,
            interface_class,
            interface_subclass,
            interface_protocol,
            endpoints,
        });
    }

    interfaces.sort_by_key(|iface| iface.interface_number);
    interfaces
}

fn parse_sysfs_endpoints(interface_dir: &Path) -> Vec<u8> {
    let mut endpoints = Vec::new();
    let Ok(entries) = fs::read_dir(interface_dir) else {
        return endpoints;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with("ep_") {
            if let Some(addr) = read_hex_u8(&path.join("bEndpointAddress")) {
                endpoints.push(addr);
            }
        }
    }
    endpoints.sort_unstable();
    endpoints
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
    try_scan_host_usb().unwrap_or_default()
}

/// Scans host USB devices while distinguishing an empty bus from a scan failure.
///
/// # Errors
///
/// Returns an I/O error when neither libusb nor sysfs inventory can be read.
pub fn try_scan_host_usb() -> std::io::Result<Vec<LocalDevice>> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(libusb) = crate::host_usb::try_scan_libusb() {
            return Ok(libusb);
        }
    }
    scan_sysfs_result(&PathBuf::from("/sys/bus/usb/devices"))
}

fn scan_sysfs_result(root: &Path) -> std::io::Result<Vec<LocalDevice>> {
    let entries = fs::read_dir(root)?;
    let mut devices = Vec::new();
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
    Ok(devices)
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
