use clap::{Parser, Subcommand};
use farbus_core::PeerFingerprint;

#[derive(Parser, Debug)]
#[command(name = "farbus", about = "Secure USB over the network")]
pub struct Cli {
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
    },
    /// Detach a remote USB device
    Detach {
        fingerprint: PeerFingerprint,
        device_id: u32,
    },
}
