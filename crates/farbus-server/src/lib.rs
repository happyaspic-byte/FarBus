use clap::Parser;
use std::net::SocketAddr;

#[derive(Parser, Debug)]
#[command(name = "farbus-server", about = "Secure FarBus USB export server")]
pub struct Cli {
    /// Listen address; the default IPv6 socket also accepts IPv4 on supported systems.
    #[arg(long, default_value = "[::]:7420")]
    pub listen: SocketAddr,
}
