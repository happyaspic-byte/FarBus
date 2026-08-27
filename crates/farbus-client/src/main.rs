use clap::Parser;
use farbus_client::{Cli, Command};
use farbus_core::{discovery, load_session, save_session, DeviceId, FarBusClient, StoredSession};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    match cli.command {
        Command::Discover => {
            println!("Scanning LAN for FarBus servers (3s)...");
            let found = discovery::collect(Duration::from_secs(3)).await?;
            if found.is_empty() {
                println!("No servers discovered via UDP broadcast.");
            } else {
                for (fp, addr, host) in found {
                    println!("Found server '{host}' at {addr} (fingerprint: {fp})");
                }
            }
        }
        Command::Pair { fingerprint } => {
            let addr = require_connect(cli.connect)?;
            print!("Enter 6-digit PIN from server: ");
            io::stdout().flush()?;
            let pin = rpassword::read_password()?.trim().to_string();
            let mut client = FarBusClient::connect(addr, fingerprint).await?;
            client.pair(&pin, fingerprint).await?;
            let token = client.auth_token().ok_or("server did not issue token")?;
            save_session(&StoredSession {
                addr,
                fingerprint,
                auth_token: token,
            })?;
            println!("Paired with {fingerprint}. Token saved locally.");
        }
        Command::Devices { fingerprint } => {
            let mut client = connect_saved(cli.connect, fingerprint).await?;
            let list = client.devices().await?;
            if list.devices.is_empty() {
                println!("No exported devices.");
            } else {
                for device in list.devices {
                    println!(
                        "  [{}] {:<8} {:04x}:{:04x}  {}{}",
                        device.id.0,
                        device.bus_id,
                        device.vid,
                        device.pid,
                        device.product,
                        if device.exported { " (exported)" } else { "" }
                    );
                }
            }
        }
        Command::Attach {
            fingerprint,
            device_id,
        } => {
            let mut client = connect_saved(cli.connect, fingerprint).await?;
            let attached = client.attach(DeviceId(device_id)).await?;
            println!(
                "Attached device {} ({}) remote-usbip-port={}",
                attached.device_id.0, attached.bus_id, attached.usbip_port
            );
            println!(
                "Windows: usbip attach --remote=127.0.0.1 --busid={}",
                attached.bus_id
            );
        }
        Command::Detach {
            fingerprint,
            device_id,
        } => {
            let mut client = connect_saved(cli.connect, fingerprint).await?;
            client.detach(DeviceId(device_id)).await?;
            println!("Detached device {device_id} from {fingerprint}");
        }
    }
    Ok(())
}

fn require_connect(connect: Option<SocketAddr>) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    connect.ok_or_else(|| "pass --connect <addr:port>".into())
}

async fn connect_saved(
    connect: Option<SocketAddr>,
    fingerprint: farbus_core::PeerFingerprint,
) -> Result<FarBusClient, Box<dyn std::error::Error>> {
    let saved = load_session(Some(fingerprint)).ok_or("run 'farbus pair' first")?;
    let addr = connect.unwrap_or(saved.addr);
    Ok(FarBusClient::connect(addr, fingerprint)
        .await?
        .with_auth_token(saved.auth_token))
}
