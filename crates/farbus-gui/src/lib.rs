//! Windows client GUI state. Network I/O lives in the binary; this crate stays testable on Linux.

pub mod actions;
#[cfg(windows)]
mod app;
#[cfg(windows)]
pub use app::run;

use farbus_core::PeerFingerprint;
use farbus_protocol::DeviceId;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const DEFAULT_USBIP: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3240);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredServer {
    pub hostname: String,
    pub addr: SocketAddr,
    pub fingerprint: PeerFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiDevice {
    pub id: DeviceId,
    pub bus_id: String,
    pub product: String,
    pub vid: u16,
    pub pid: u16,
    pub attached: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuiSession {
    pub addr: SocketAddr,
    pub fingerprint: PeerFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuiPhase {
    Idle,
    Scanning,
    Pairing,
    Ready,
    Forwarding,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuiEvent {
    ScanStarted,
    ServersFound(Vec<DiscoveredServer>),
    ServerSelected(PeerFingerprint),
    PinChanged(String),
    PairStarted,
    PairSucceeded {
        addr: SocketAddr,
        fingerprint: PeerFingerprint,
    },
    PairRejected(String),
    DevicesLoaded(Vec<GuiDevice>),
    AttachSucceeded {
        id: DeviceId,
        bus_id: String,
    },
    DetachSucceeded(DeviceId),
    UsbipListenRejected(String),
    Failed(String),
    TrayHidden,
    TrayShown,
}

pub struct GuiState {
    pub phase: GuiPhase,
    pub servers: Vec<DiscoveredServer>,
    pub selected: Option<PeerFingerprint>,
    pub devices: Vec<GuiDevice>,
    pub session: Option<GuiSession>,
    pub pin: String,
    pub usbip_listen: SocketAddr,
    pub window_visible: bool,
}

impl fmt::Debug for GuiState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GuiState")
            .field("phase", &self.phase)
            .field("servers", &self.servers)
            .field("selected", &self.selected)
            .field("devices", &self.devices)
            .field("session", &self.session)
            .field("pin", &"[redacted]")
            .field("usbip_listen", &self.usbip_listen)
            .field("window_visible", &self.window_visible)
            .finish()
    }
}

impl GuiState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: GuiPhase::Idle,
            servers: Vec::new(),
            selected: None,
            devices: Vec::new(),
            session: None,
            pin: String::new(),
            usbip_listen: DEFAULT_USBIP,
            window_visible: true,
        }
    }

    #[must_use]
    pub fn public_status(&self) -> String {
        match &self.phase {
            GuiPhase::Idle if self.session.is_none() => "Not paired".into(),
            GuiPhase::Idle | GuiPhase::Ready => format!("Paired · USB/IP {}", self.usbip_listen),
            GuiPhase::Scanning => "Scanning LAN…".into(),
            GuiPhase::Pairing => "Pairing…".into(),
            GuiPhase::Forwarding => format!("Forwarding · {}", self.usbip_listen),
            GuiPhase::Error(err) => err.clone(),
        }
    }
}

impl Default for GuiState {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub fn sanitize_pin(raw: &str) -> String {
    raw.chars().filter(char::is_ascii_digit).take(6).collect()
}

#[must_use]
pub fn loopback_usbip(addr: SocketAddr) -> Option<SocketAddr> {
    addr.ip().is_loopback().then_some(addr)
}

pub fn apply(state: &mut GuiState, event: GuiEvent) {
    match event {
        GuiEvent::ScanStarted => {
            state.phase = GuiPhase::Scanning;
        }
        GuiEvent::ServersFound(servers) => {
            state.servers = servers;
            if state.phase == GuiPhase::Scanning {
                state.phase = GuiPhase::Idle;
            }
        }
        GuiEvent::ServerSelected(fingerprint) => {
            state.selected = Some(fingerprint);
        }
        GuiEvent::PinChanged(raw) => {
            state.pin = sanitize_pin(&raw);
        }
        GuiEvent::PairStarted => {
            if state.pin.len() == 6 {
                state.phase = GuiPhase::Pairing;
            } else {
                state.phase = GuiPhase::Error("enter the 6-digit PIN from the server".into());
            }
        }
        GuiEvent::PairSucceeded { addr, fingerprint } => {
            state.pin.clear();
            state.selected = Some(fingerprint);
            state.session = Some(GuiSession { addr, fingerprint });
            state.phase = GuiPhase::Ready;
        }
        GuiEvent::PairRejected(reason) | GuiEvent::Failed(reason) => {
            state.phase = GuiPhase::Error(reason);
        }
        GuiEvent::DevicesLoaded(devices) => {
            state.devices = devices;
            if state.session.is_some() && state.phase != GuiPhase::Forwarding {
                state.phase = GuiPhase::Ready;
            }
        }
        GuiEvent::AttachSucceeded { id, bus_id } => {
            for device in &mut state.devices {
                device.attached = device.id == id;
                if device.id == id {
                    device.bus_id.clone_from(&bus_id);
                }
            }
            state.phase = GuiPhase::Forwarding;
        }
        GuiEvent::DetachSucceeded(id) => {
            for device in &mut state.devices {
                if device.id == id {
                    device.attached = false;
                }
            }
            state.phase = if state.devices.iter().any(|device| device.attached) {
                GuiPhase::Forwarding
            } else {
                GuiPhase::Ready
            };
        }
        GuiEvent::UsbipListenRejected(_) => {
            state.phase = GuiPhase::Error("USB/IP listener must use a loopback address".into());
        }
        GuiEvent::TrayHidden => {
            state.window_visible = false;
        }
        GuiEvent::TrayShown => {
            state.window_visible = true;
        }
    }
}
