use clap::Parser;
use farbus_server::Cli;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

#[test]
fn defaults_to_dual_stack_ipv6_unspecified_listener() {
    let cli = Cli::try_parse_from(["farbus-server"]).unwrap();
    assert_eq!(
        cli.listen,
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 7420)
    );
}

#[test]
fn accepts_explicit_ipv4_listener() {
    let cli = Cli::try_parse_from(["farbus-server", "--listen", "0.0.0.0:9000"]).unwrap();
    assert_eq!(cli.listen.to_string(), "0.0.0.0:9000");
}

#[test]
fn parses_custom_usbip_listener() {
    let cli = Cli::try_parse_from(["farbus-server", "--usbip-listen", "127.0.0.1:33240"]).unwrap();
    assert_eq!(cli.usbip_listen.to_string(), "127.0.0.1:33240");
}

#[test]
fn rejects_non_loopback_usbip_listener() {
    for addr in ["0.0.0.0:3240", "[::]:3240", "192.168.1.10:3240"] {
        assert!(Cli::try_parse_from(["farbus-server", "--usbip-listen", addr]).is_err());
    }
}

#[test]
fn accepts_ipv6_loopback_usbip_listener() {
    let cli = Cli::try_parse_from(["farbus-server", "--usbip-listen", "[::1]:3240"]).unwrap();
    assert!(cli.usbip_listen.ip().is_loopback());
}

#[test]
fn export_all_is_off_by_default() {
    let cli = Cli::try_parse_from(["farbus-server"]).unwrap();
    assert!(!cli.export_all);
    let cli = Cli::try_parse_from(["farbus-server", "--export-all"]).unwrap();
    assert!(cli.export_all);
}

#[test]
fn parses_selective_export_bus_ids() {
    let cli =
        Cli::try_parse_from(["farbus-server", "--export", "1-1.2", "--export", "1-2"]).unwrap();
    assert_eq!(cli.export, vec!["1-1.2", "1-2"]);
}
