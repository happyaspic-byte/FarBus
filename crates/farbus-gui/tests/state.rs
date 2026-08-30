use farbus_core::PeerFingerprint;
use farbus_gui::{apply, GuiEvent, GuiPhase, GuiState};
use farbus_protocol::DeviceId;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn fp(byte: u8) -> PeerFingerprint {
    PeerFingerprint::new([byte; 32])
}

fn loopback() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3240)
}

#[test]
fn starts_unpaired_with_loopback_usbip() {
    let state = GuiState::new();
    assert_eq!(state.phase, GuiPhase::Idle);
    assert!(state.servers.is_empty());
    assert!(state.devices.is_empty());
    assert!(state.session.is_none());
    assert!(state.usbip_listen.ip().is_loopback());
    assert_eq!(state.usbip_listen.port(), 3240);
    assert!(state.window_visible);
}

#[test]
fn scan_lists_discovered_servers() {
    let mut state = GuiState::new();
    apply(&mut state, GuiEvent::ScanStarted);
    assert_eq!(state.phase, GuiPhase::Scanning);

    apply(
        &mut state,
        GuiEvent::ServersFound(vec![farbus_gui::DiscoveredServer {
            hostname: "lab".into(),
            addr: "192.168.1.20:7420".parse().unwrap(),
            fingerprint: fp(7),
        }]),
    );
    assert_eq!(state.phase, GuiPhase::Idle);
    assert_eq!(state.servers.len(), 1);
    assert_eq!(state.servers[0].hostname, "lab");
}

#[test]
fn empty_scan_keeps_manual_server() {
    let mut state = GuiState::new();
    apply(
        &mut state,
        GuiEvent::ManualHostChanged("100.95.152.106:7420".into()),
    );
    apply(
        &mut state,
        GuiEvent::ManualFingerprintChanged("aa".repeat(32)),
    );
    apply(&mut state, GuiEvent::ManualServerAdded);
    apply(&mut state, GuiEvent::ScanStarted);
    apply(&mut state, GuiEvent::ServersFound(Vec::new()));
    assert_eq!(state.servers.len(), 1);
    assert_eq!(state.phase, GuiPhase::Idle);
}

#[test]
fn scan_merges_lan_and_manual_servers() {
    let mut state = GuiState::new();
    apply(
        &mut state,
        GuiEvent::ManualHostChanged("100.95.152.106:7420".into()),
    );
    apply(
        &mut state,
        GuiEvent::ManualFingerprintChanged("aa".repeat(32)),
    );
    apply(&mut state, GuiEvent::ManualServerAdded);
    apply(
        &mut state,
        GuiEvent::ServersFound(vec![farbus_gui::DiscoveredServer {
            hostname: "lab".into(),
            addr: "192.168.1.20:7420".parse().unwrap(),
            fingerprint: fp(7),
        }]),
    );
    assert_eq!(state.servers.len(), 2);
}

#[test]
fn manual_tailscale_host_is_selected_without_scan() {
    let mut state = GuiState::new();
    apply(
        &mut state,
        GuiEvent::ManualHostChanged("100.95.152.106:7420".into()),
    );
    apply(
        &mut state,
        GuiEvent::ManualFingerprintChanged("aa".repeat(32)),
    );
    apply(&mut state, GuiEvent::ManualServerAdded);
    assert_eq!(state.servers.len(), 1);
    assert_eq!(
        state.servers[0].addr,
        "100.95.152.106:7420".parse().unwrap()
    );
    assert_eq!(state.selected, Some(state.servers[0].fingerprint));
    assert_eq!(state.phase, GuiPhase::Idle);
}

#[test]
fn parse_manual_ip_defaults_to_7420() {
    let server = farbus_gui::parse_manual_server("127.0.0.1", &"ab".repeat(32)).unwrap();
    assert_eq!(server.addr, "127.0.0.1:7420".parse().unwrap());
}

#[test]
fn manual_server_rejects_bad_address() {
    let mut state = GuiState::new();
    apply(
        &mut state,
        GuiEvent::ManualHostChanged("not-an-addr".into()),
    );
    apply(&mut state, GuiEvent::ManualServerAdded);
    assert!(state.servers.is_empty());
    assert!(matches!(state.phase, GuiPhase::Error(_)));
}

#[test]
fn pin_keeps_six_digits_and_never_enters_status() {
    let mut state = GuiState::new();
    apply(&mut state, GuiEvent::PinChanged("12ab34-56".into()));
    assert_eq!(state.pin, "123456");

    apply(&mut state, GuiEvent::PinChanged("9999999".into()));
    assert_eq!(state.pin, "999999");

    let status = state.public_status();
    assert!(!status.contains("999999"));
}

#[test]
fn pair_requires_six_digit_pin_without_calling_network() {
    let mut state = GuiState::new();
    apply(
        &mut state,
        GuiEvent::ServersFound(vec![farbus_gui::DiscoveredServer {
            hostname: "lab".into(),
            addr: "127.0.0.1:7420".parse().unwrap(),
            fingerprint: fp(1),
        }]),
    );
    apply(&mut state, GuiEvent::ServerSelected(fp(1)));
    apply(&mut state, GuiEvent::PinChanged("123".into()));
    apply(&mut state, GuiEvent::PairStarted);
    assert_eq!(
        state.phase,
        GuiPhase::Error("enter the 6-digit PIN from the server".into())
    );
    assert_eq!(state.pin, "123");
}

#[test]
fn successful_pair_clears_pin_and_stores_session_without_token() {
    let mut state = GuiState::new();
    apply(&mut state, GuiEvent::PinChanged("482910".into()));
    apply(
        &mut state,
        GuiEvent::PairSucceeded {
            addr: "127.0.0.1:7420".parse().unwrap(),
            fingerprint: fp(9),
        },
    );
    assert!(state.pin.is_empty());
    let session = state.session.expect("paired");
    assert_eq!(session.fingerprint, fp(9));
    assert_eq!(state.phase, GuiPhase::Ready);
    assert!(!state.public_status().contains("482910"));
}

#[test]
fn attach_uses_loopback_and_marks_device() {
    let mut state = GuiState::new();
    apply(
        &mut state,
        GuiEvent::DevicesLoaded(vec![farbus_gui::GuiDevice {
            id: DeviceId(1),
            bus_id: "1-1.2".into(),
            product: "UART".into(),
            vid: 0x0403,
            pid: 0x6001,
            attached: false,
        }]),
    );
    apply(
        &mut state,
        GuiEvent::AttachSucceeded {
            id: DeviceId(1),
            bus_id: "1-1.2".into(),
        },
    );
    assert!(state.devices[0].attached);
    assert_eq!(state.phase, GuiPhase::Forwarding);
    assert_eq!(state.usbip_listen, loopback());
    assert!(state.public_status().contains("127.0.0.1:3240"));
}

#[test]
fn rejects_non_loopback_usbip_listen() {
    let mut state = GuiState::new();
    apply(
        &mut state,
        GuiEvent::UsbipListenRejected("192.168.1.10:3240".into()),
    );
    assert!(matches!(state.phase, GuiPhase::Error(_)));
    assert!(state.usbip_listen.ip().is_loopback());
}

#[test]
fn tray_hide_and_show_toggle_window() {
    let mut state = GuiState::new();
    assert!(!state.take_focus_request());
    apply(&mut state, GuiEvent::TrayHidden);
    assert!(!state.window_visible);
    assert!(!state.take_focus_request());
    apply(&mut state, GuiEvent::TrayShown);
    assert!(state.window_visible);
    assert!(state.take_focus_request());
    assert!(!state.take_focus_request());
}

#[test]
fn detach_clears_forwarding() {
    let mut state = GuiState::new();
    apply(
        &mut state,
        GuiEvent::DevicesLoaded(vec![farbus_gui::GuiDevice {
            id: DeviceId(4),
            bus_id: "1-4".into(),
            product: "Composite".into(),
            vid: 1,
            pid: 2,
            attached: true,
        }]),
    );
    apply(
        &mut state,
        GuiEvent::AttachSucceeded {
            id: DeviceId(4),
            bus_id: "1-4".into(),
        },
    );
    apply(&mut state, GuiEvent::DetachSucceeded(DeviceId(4)));
    assert!(!state.devices[0].attached);
    assert_eq!(state.phase, GuiPhase::Ready);
}
