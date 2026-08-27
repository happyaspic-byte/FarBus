use clap::Parser;
use std::net::SocketAddr;

#[derive(Parser, Debug)]
#[command(name = "farbus-server", about = "Secure FarBus USB export server")]
pub struct Cli {
    /// Listen address; the default IPv6 socket also accepts IPv4 on supported systems.
    #[arg(long, default_value = "[::]:7420")]
    pub listen: SocketAddr,
    /// Loopback address for the local USB/IP compatibility listener.
    #[arg(long, default_value = "127.0.0.1:3240")]
    pub usbip_listen: SocketAddr,
    /// Export every discovered USB device. Off by default.
    #[arg(long, default_value_t = false)]
    pub export_all: bool,
    /// Export a specific bus id (repeatable), e.g. --export 1-1.2
    #[arg(long = "export")]
    pub export: Vec<String>,
}
