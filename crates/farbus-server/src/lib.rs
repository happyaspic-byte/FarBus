use clap::Parser;
use std::net::SocketAddr;

fn parse_loopback_addr(value: &str) -> Result<SocketAddr, String> {
    let addr: SocketAddr = value
        .parse()
        .map_err(|_| "expected a socket address".to_string())?;
    if !addr.ip().is_loopback() {
        return Err("USB/IP listener must use a loopback address".into());
    }
    Ok(addr)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExportPolicyError {
    #[error("requested USB bus id not found: {0}")]
    NotFound(String),
}

/// Applies explicit export selectors after a conservative broad-export pass.
///
/// # Errors
///
/// Returns [`ExportPolicyError::NotFound`] when an exact selector matches no device.
pub fn apply_export_policy(
    devices: &mut [farbus_core::LocalDevice],
    export_all: bool,
    exact_bus_ids: &[String],
) -> Result<(), ExportPolicyError> {
    for device in devices.iter_mut() {
        device.info.exported = false;
        if export_all && !is_sensitive(device) {
            device.info.exported = true;
        }
    }

    for bus_id in exact_bus_ids {
        let Some(device) = devices
            .iter_mut()
            .find(|device| device.info.bus_id == *bus_id)
        else {
            return Err(ExportPolicyError::NotFound(bus_id.clone()));
        };
        device.info.exported = true;
    }
    Ok(())
}

fn is_sensitive(device: &farbus_core::LocalDevice) -> bool {
    const HID: u8 = 3;
    const MASS_STORAGE: u8 = 8;
    const HUB: u8 = 9;
    let sensitive = |class| matches!(class, HID | MASS_STORAGE | HUB);
    sensitive(device.info.usb_class)
        || device
            .info
            .interfaces
            .iter()
            .any(|interface| sensitive(interface.interface_class))
}

#[derive(Parser, Debug)]
#[command(name = "farbus-server", about = "Secure FarBus USB export server")]
pub struct Cli {
    /// Listen address; the default IPv6 socket also accepts IPv4 on supported systems.
    #[arg(long, default_value = "[::]:7420")]
    pub listen: SocketAddr,
    /// Loopback address for the local USB/IP compatibility listener.
    #[arg(
        long,
        default_value = "127.0.0.1:3240",
        value_parser = parse_loopback_addr
    )]
    pub usbip_listen: SocketAddr,
    /// Export all non-sensitive devices. HID, storage, hubs, and related composites stay denied.
    #[arg(long, default_value_t = false)]
    pub export_all: bool,
    /// Export a specific bus id (repeatable), e.g. --export 1-1.2
    #[arg(long = "export")]
    pub export: Vec<String>,
}
