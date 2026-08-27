#![cfg(target_os = "linux")]

use crate::urb::complete_urb;
use crate::usb::{DeviceBackend, LocalDevice};
use farbus_protocol::{
    DeviceId, DeviceInfo, TransferType, UrbComplete, UrbSubmit, UsbInterfaceInfo, UsbSpeed,
};
use rusb::{Context, DeviceHandle, UsbContext};
use std::time::Duration;

#[must_use]
pub fn scan_libusb() -> Vec<LocalDevice> {
    try_scan_libusb().unwrap_or_default()
}

/// Enumerates libusb devices, distinguishing an empty bus from a backend failure.
///
/// # Errors
///
/// Returns libusb errors when context creation or enumeration fails.
pub fn try_scan_libusb() -> rusb::Result<Vec<LocalDevice>> {
    let ctx = Context::new()?;
    let list = ctx.devices()?;
    let mut out = Vec::new();
    let mut id = 1u32;
    for device in list.iter() {
        let desc = device.device_descriptor()?;
        let speed = match device.speed() {
            rusb::Speed::Low => UsbSpeed::Low,
            rusb::Speed::Full => UsbSpeed::Full,
            rusb::Speed::Super | rusb::Speed::SuperPlus => UsbSpeed::Super,
            _ => UsbSpeed::High,
        };
        let product = device
            .open()
            .ok()
            .and_then(|h| h.read_product_string_ascii(&desc).ok())
            .unwrap_or_else(|| format!("{:04x}:{:04x}", desc.vendor_id(), desc.product_id()));
        let ports = device.port_numbers()?;
        if ports.is_empty() {
            continue;
        }
        out.push(LocalDevice {
            info: DeviceInfo {
                id: DeviceId(id),
                bus_id: topology_bus_id(device.bus_number(), &ports),
                vid: desc.vendor_id(),
                pid: desc.product_id(),
                usb_class: desc.class_code(),
                speed,
                product,
                exported: false,
                interfaces: collect_interfaces(&device),
            },
            backend: DeviceBackend::Host,
        });
        id += 1;
    }
    Ok(out)
}

/// Completes an URB on a real USB device when possible; otherwise uses the emulator.
#[must_use]
pub fn complete_host_or_emulated(submit: &UrbSubmit, devices: &[LocalDevice]) -> UrbComplete {
    if let Some(complete) = try_host(submit, devices) {
        complete
    } else {
        let emulated = devices.iter().any(|device| {
            device.info.id == submit.device_id && device.backend == DeviceBackend::Emulated
        });
        if emulated {
            complete_urb(submit)
        } else {
            UrbComplete {
                seq: submit.seq,
                status: -1,
                data: Vec::new(),
            }
        }
    }
}

fn try_host(submit: &UrbSubmit, devices: &[LocalDevice]) -> Option<UrbComplete> {
    let device = devices.iter().find(|d| {
        d.info.id == submit.device_id && d.info.exported && d.backend == DeviceBackend::Host
    })?;
    let ctx = Context::new().ok()?;
    let list = ctx.devices().ok()?;
    let usb_dev = list.iter().find(|candidate| {
        let Ok(desc) = candidate.device_descriptor() else {
            return false;
        };
        let Ok(ports) = candidate.port_numbers() else {
            return false;
        };
        desc.vendor_id() == device.info.vid
            && desc.product_id() == device.info.pid
            && topology_bus_id(candidate.bus_number(), &ports) == device.info.bus_id
    })?;
    let handle = usb_dev.open().ok()?;
    let _ = handle.set_auto_detach_kernel_driver(true);
    let iface = interface_for_endpoint(&device.info.interfaces, submit.endpoint).unwrap_or(0);
    let _ = handle.claim_interface(iface);
    let timeout = Duration::from_millis(500);
    match submit.transfer {
        TransferType::Control => control(&handle, submit, timeout),
        TransferType::Bulk => Some(bulk(&handle, submit, timeout)),
        TransferType::Interrupt => Some(interrupt(&handle, submit, timeout)),
        TransferType::Isochronous => Some(UrbComplete {
            seq: submit.seq,
            status: -32,
            data: Vec::new(),
        }),
    }
}

fn collect_interfaces(device: &rusb::Device<Context>) -> Vec<UsbInterfaceInfo> {
    let Ok(config) = device.config_descriptor(0) else {
        return Vec::new();
    };
    config
        .interfaces()
        .filter_map(|iface| {
            let desc = iface.descriptors().next()?;
            let endpoints = desc.endpoint_descriptors().map(|ep| ep.address()).collect();
            Some(UsbInterfaceInfo {
                interface_number: desc.interface_number(),
                interface_class: desc.class_code(),
                interface_subclass: desc.sub_class_code(),
                interface_protocol: desc.protocol_code(),
                endpoints,
            })
        })
        .collect()
}

fn interface_for_endpoint(ifaces: &[UsbInterfaceInfo], endpoint: u8) -> Option<u8> {
    ifaces
        .iter()
        .find(|iface| iface.endpoints.contains(&endpoint) || endpoint == 0)
        .map(|iface| iface.interface_number)
        .or_else(|| ifaces.first().map(|iface| iface.interface_number))
}

fn topology_bus_id(bus: u8, ports: &[u8]) -> String {
    let path = ports
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(".");
    format!("{bus}-{path}")
}

fn control<T: UsbContext>(
    handle: &DeviceHandle<T>,
    submit: &UrbSubmit,
    timeout: Duration,
) -> Option<UrbComplete> {
    if submit.data.len() < 8 {
        return None;
    }
    let request_type = submit.data[0];
    let request = submit.data[1];
    let value = u16::from_le_bytes([submit.data[2], submit.data[3]]);
    let index = u16::from_le_bytes([submit.data[4], submit.data[5]]);
    let w_length = usize::from(u16::from_le_bytes([submit.data[6], submit.data[7]])).min(65_536);
    let dir_in = request_type & 0x80 != 0;
    if dir_in {
        let mut buf = vec![0u8; w_length.max(1)];
        match handle.read_control(request_type, request, value, index, &mut buf, timeout) {
            Ok(n) => Some(UrbComplete {
                seq: submit.seq,
                status: 0,
                data: buf[..n].to_vec(),
            }),
            Err(_) => Some(UrbComplete {
                seq: submit.seq,
                status: -1,
                data: Vec::new(),
            }),
        }
    } else {
        let payload = submit.data.get(8..).unwrap_or(&[]);
        match handle.write_control(request_type, request, value, index, payload, timeout) {
            Ok(_) => Some(UrbComplete {
                seq: submit.seq,
                status: 0,
                data: Vec::new(),
            }),
            Err(_) => Some(UrbComplete {
                seq: submit.seq,
                status: -1,
                data: Vec::new(),
            }),
        }
    }
}

fn bulk<T: UsbContext>(
    handle: &DeviceHandle<T>,
    submit: &UrbSubmit,
    timeout: Duration,
) -> UrbComplete {
    if submit.endpoint & 0x80 != 0 {
        let mut buf = vec![0u8; submit.data.len().clamp(1, 65_536)];
        match handle.read_bulk(submit.endpoint, &mut buf, timeout) {
            Ok(n) => UrbComplete {
                seq: submit.seq,
                status: 0,
                data: buf[..n].to_vec(),
            },
            Err(_) => UrbComplete {
                seq: submit.seq,
                status: -1,
                data: Vec::new(),
            },
        }
    } else {
        match handle.write_bulk(submit.endpoint, &submit.data, timeout) {
            Ok(_) => UrbComplete {
                seq: submit.seq,
                status: 0,
                data: Vec::new(),
            },
            Err(_) => UrbComplete {
                seq: submit.seq,
                status: -1,
                data: Vec::new(),
            },
        }
    }
}

fn interrupt<T: UsbContext>(
    handle: &DeviceHandle<T>,
    submit: &UrbSubmit,
    timeout: Duration,
) -> UrbComplete {
    if submit.endpoint & 0x80 != 0 {
        let mut buf = vec![0u8; 64];
        match handle.read_interrupt(submit.endpoint, &mut buf, timeout) {
            Ok(n) => UrbComplete {
                seq: submit.seq,
                status: 0,
                data: buf[..n].to_vec(),
            },
            Err(_) => UrbComplete {
                seq: submit.seq,
                status: -1,
                data: Vec::new(),
            },
        }
    } else {
        match handle.write_interrupt(submit.endpoint, &submit.data, timeout) {
            Ok(_) => UrbComplete {
                seq: submit.seq,
                status: 0,
                data: Vec::new(),
            },
            Err(_) => UrbComplete {
                seq: submit.seq,
                status: -1,
                data: Vec::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::topology_bus_id;

    #[test]
    fn formats_stable_usb_topology_path() {
        assert_eq!(topology_bus_id(1, &[2]), "1-2");
        assert_eq!(topology_bus_id(3, &[1, 4, 2]), "3-1.4.2");
    }
}
