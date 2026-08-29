use clap::Parser;
use farbus_client::{Cli, Command};

#[test]
fn parses_discover_command() {
    let cli = Cli::try_parse_from(["farbus", "discover"]).unwrap();
    assert!(matches!(cli.command, Command::Discover));
}

#[test]
fn pairing_pin_is_not_accepted_on_command_line() {
    let fingerprint = "ab".repeat(32);
    let cli = Cli::try_parse_from(["farbus", "pair", &fingerprint]).unwrap();
    assert!(matches!(cli.command, Command::Pair { .. }));
    assert!(Cli::try_parse_from(["farbus", "pair", &fingerprint, "123456"]).is_err());
}

#[test]
fn parses_attach_with_numeric_device_id() {
    let cli = Cli::try_parse_from(["farbus", "attach", "42"]).unwrap();
    assert!(matches!(
        cli.command,
        Command::Attach {
            fingerprint: None,
            device_id: 42,
            usbip_listen,
        } if usbip_listen.to_string() == "127.0.0.1:3240"
    ));
}

#[test]
fn parses_devices_and_detach_without_fingerprint() {
    let cli = Cli::try_parse_from(["farbus", "devices"]).unwrap();
    assert!(matches!(
        cli.command,
        Command::Devices { fingerprint: None }
    ));

    let cli = Cli::try_parse_from(["farbus", "detach", "42"]).unwrap();
    assert!(matches!(
        cli.command,
        Command::Detach {
            fingerprint: None,
            device_id: 42,
        }
    ));
}

#[test]
fn parses_custom_usbip_listen_address() {
    let cli = Cli::try_parse_from([
        "farbus",
        "attach",
        "42",
        "--usbip-listen",
        "127.0.0.1:33240",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Command::Attach { usbip_listen, .. }
        if usbip_listen.port() == 33240
    ));
}

#[test]
fn rejects_non_loopback_usbip_listener() {
    for addr in ["0.0.0.0:3240", "[::]:3240", "192.168.1.10:3240"] {
        assert!(Cli::try_parse_from(["farbus", "attach", "42", "--usbip-listen", addr,]).is_err());
    }
}

#[test]
fn accepts_ipv6_loopback_usbip_listener() {
    let cli =
        Cli::try_parse_from(["farbus", "attach", "42", "--usbip-listen", "[::1]:3240"]).unwrap();
    assert!(matches!(
        cli.command,
        Command::Attach { usbip_listen, .. } if usbip_listen.ip().is_loopback()
    ));
}

#[test]
fn parses_diagnose_command() {
    let fingerprint = "22".repeat(32);
    let cli = Cli::try_parse_from(["farbus", "diagnose", &fingerprint]).unwrap();
    assert!(matches!(cli.command, Command::Diagnose { .. }));
}

#[test]
fn parses_status_command() {
    let cli = Cli::try_parse_from(["farbus", "status"]).unwrap();
    assert!(matches!(cli.command, Command::Status { json: false }));
    let cli = Cli::try_parse_from(["farbus", "status", "--json"]).unwrap();
    assert!(matches!(cli.command, Command::Status { json: true }));
}
