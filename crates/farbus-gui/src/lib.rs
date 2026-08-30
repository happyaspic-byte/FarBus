//! Windows client GUI state. Network I/O lives in the binary; this crate stays testable on Linux.

pub mod actions;
#[cfg(windows)]
mod app;
#[cfg(windows)]
pub use app::run;

use farbus_core::PeerFingerprint;
use farbus_protocol::DeviceId;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

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
    ManualHostChanged(String),
    ManualFingerprintChanged(String),
    ManualServerAdded,
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
    pub manual_host: String,
    pub manual_fingerprint: String,
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
            .field("manual_host", &self.manual_host)
            .field("manual_fingerprint", &self.manual_fingerprint)
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
            manual_host: String::new(),
            manual_fingerprint: String::new(),
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

/// Parses `host`, `host:port`, or a socket address for Tailscale/manual pairing.
///
/// # Errors
///
/// Returns a display string when the host or fingerprint is invalid.
pub fn parse_manual_server(host: &str, fingerprint: &str) -> Result<DiscoveredServer, String> {
    let host = host.trim();
    if host.is_empty() {
        return Err("enter the server Tailscale name or IP:7420".into());
    }
    let addr = resolve_server_addr(host)?;
    let fingerprint = fingerprint.trim();
    let fingerprint = if fingerprint.is_empty() {
        PeerFingerprint::new([0; 32])
    } else {
        fingerprint.parse().map_err(|_| {
            "fingerprint must be 64 hex characters from the Linux server".to_string()
        })?
    };
    Ok(DiscoveredServer {
        hostname: display_hostname(host),
        addr,
        fingerprint,
    })
}

fn resolve_server_addr(host: &str) -> Result<SocketAddr, String> {
    if let Ok(addr) = host.parse::<SocketAddr>() {
        return Ok(addr);
    }
    let candidate = if host_has_port(host) {
        host.to_string()
    } else {
        format!("{host}:7420")
    };
    candidate
        .to_socket_addrs()
        .map_err(|err| format!("could not resolve {candidate}: {err}"))?
        .next()
        .ok_or_else(|| format!("could not resolve {candidate}"))
}

fn host_has_port(host: &str) -> bool {
    if let Some(rest) = host.strip_prefix('[') {
        rest.contains("]:")
    } else {
        host.rfind(':').is_some()
    }
}

fn merge_servers(state: &mut GuiState, incoming: Vec<DiscoveredServer>) {
    for server in incoming {
        if let Some(existing) = state
            .servers
            .iter_mut()
            .find(|existing| existing.fingerprint == server.fingerprint)
        {
            *existing = server;
        } else {
            state.servers.push(server);
        }
    }
}

fn display_hostname(host: &str) -> String {
    host.split([':', ']'])
        .find(|part| !part.is_empty() && part.chars().any(|ch| ch.is_ascii_alphabetic()))
        .unwrap_or("manual")
        .to_string()
}

#[allow(clippy::too_many_lines)]
pub fn apply(state: &mut GuiState, event: GuiEvent) {
    match event {
        GuiEvent::ScanStarted => {
            state.phase = GuiPhase::Scanning;
        }
        GuiEvent::ServersFound(servers) => {
            merge_servers(state, servers);
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
        GuiEvent::ManualHostChanged(host) => {
            state.manual_host = host;
        }
        GuiEvent::ManualFingerprintChanged(fp) => {
            state.manual_fingerprint = fp
                .chars()
                .filter(char::is_ascii_hexdigit)
                .take(64)
                .collect();
        }
        GuiEvent::ManualServerAdded => {
            match parse_manual_server(&state.manual_host, &state.manual_fingerprint) {
                Ok(server) => {
                    let fingerprint = server.fingerprint;
                    if !state
                        .servers
                        .iter()
                        .any(|existing| existing.fingerprint == fingerprint)
                    {
                        state.servers.push(server);
                    }
                    state.selected = Some(fingerprint);
                    if matches!(
                        state.phase,
                        GuiPhase::Error(_) | GuiPhase::Idle | GuiPhase::Scanning
                    ) {
                        state.phase = GuiPhase::Idle;
                    }
                }
                Err(err) => {
                    state.phase = GuiPhase::Error(err);
                }
            }
        }
        GuiEvent::TrayHidden => {
            state.window_visible = false;
        }
        GuiEvent::TrayShown => {
            state.window_visible = true;
        }
    }
}
