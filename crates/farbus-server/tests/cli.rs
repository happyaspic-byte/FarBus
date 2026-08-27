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
fn export_all_is_off_by_default() {
    let cli = Cli::try_parse_from(["farbus-server"]).unwrap();
    assert!(!cli.export_all);
    let cli = Cli::try_parse_from(["farbus-server", "--export-all"]).unwrap();
    assert!(cli.export_all);
}
