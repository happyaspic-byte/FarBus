use clap::{Parser, Subcommand};
use farbus_core::PeerFingerprint;
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

#[derive(Parser, Debug)]
#[command(name = "farbus", about = "Secure USB over the network")]
pub struct Cli {
    /// Server TCP address used by pair/devices/attach/detach
    #[arg(long, global = true)]
    pub connect: Option<SocketAddr>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Discover `FarBus` servers on the local network
    Discover,
    /// Pair with a server; reads the PIN securely from the terminal
    Pair { fingerprint: PeerFingerprint },
    /// List exported devices on a paired server
    Devices { fingerprint: PeerFingerprint },
    /// Attach a remote USB device
    Attach {
        fingerprint: PeerFingerprint,
        device_id: u32,
        /// Loopback address exposed to the local USB/IP driver.
        #[arg(
            long,
            default_value = "127.0.0.1:3240",
            value_parser = parse_loopback_addr
        )]
        usbip_listen: SocketAddr,
    },
    /// Detach a remote USB device
    Detach {
        fingerprint: PeerFingerprint,
        device_id: u32,
    },
    /// Run connectivity and security diagnostics
    Diagnose { fingerprint: PeerFingerprint },
    /// Show saved pairing and connection status
    Status {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
}
