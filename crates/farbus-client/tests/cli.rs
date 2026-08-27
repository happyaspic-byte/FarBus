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
    let fingerprint = "01".repeat(32);
    let cli = Cli::try_parse_from(["farbus", "attach", &fingerprint, "42"]).unwrap();
    assert!(matches!(cli.command, Command::Attach { device_id: 42, .. }));
}
