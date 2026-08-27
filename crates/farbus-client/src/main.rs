use clap::Parser;
use farbus_client::{Cli, Command};
use farbus_core::{discovery, load_session, save_session, DeviceId, FarBusClient, StoredSession};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::time::Duration;

#[tokio::main]
#[allow(clippy::too_many_lines)]
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
            let mut client = farbus_core::happy_eyeballs_connect([addr], fingerprint).await?;
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
            let devices = client.devices().await?.devices;
            let locals = devices
                .into_iter()
                .map(|info| farbus_core::LocalDevice {
                    info,
                    backend: farbus_core::DeviceBackend::Emulated,
                })
                .collect();
            println!(
                "Attached device {} ({}) through TLS 1.3.",
                attached.device_id.0, attached.bus_id
            );
            println!("Local USB/IP proxy: 127.0.0.1:3240");
            println!(
                "Windows: usbip attach --remote=127.0.0.1 --busid={}",
                attached.bus_id
            );
            println!("Press Ctrl+C to stop forwarding.");
            let shared = std::sync::Arc::new(tokio::sync::Mutex::new(client));
            farbus_core::serve_usbip_forward("127.0.0.1:3240", locals, shared).await?;
        }
        Command::Detach {
            fingerprint,
            device_id,
        } => {
            let mut client = connect_saved(cli.connect, fingerprint).await?;
            client.detach(DeviceId(device_id)).await?;
            println!("Detached device {device_id} from {fingerprint}");
        }
        Command::Diagnose { fingerprint } => {
            let saved = load_session(Some(fingerprint)).ok_or("run 'farbus pair' first")?;
            println!("Server address  : {}", saved.addr);
            println!("Fingerprint     : {}", saved.fingerprint);
            let start = std::time::Instant::now();
            let mut client = farbus_core::happy_eyeballs_connect([saved.addr], fingerprint).await?;
            let latency = start.elapsed();
            println!("TLS 1.3         : OK ({latency:?})");
            client = client.with_auth_token(saved.auth_token);
            let devices = client.devices().await?;
            println!("Auth token      : OK");
            println!("Exported devices: {}", devices.devices.len());
            println!("USB/IP loopback : available after 'farbus attach'");
        }
        Command::Status { json } => match load_session(None) {
            Some(saved) => {
                if json {
                    println!(
                        "{{\"addr\":\"{}\",\"fingerprint\":\"{}\",\"tls\":\"1.3\",\"usbip\":\"127.0.0.1:3240\",\"token\":true}}",
                        saved.addr, saved.fingerprint
                    );
                } else {
                    println!("Last server     : {}", saved.addr);
                    println!("Fingerprint     : {}", saved.fingerprint);
                    println!("Auth token      : present (256-bit)");
                    println!("TLS             : 1.3, fingerprint-pinned");
                    println!("USB/IP loopback : 127.0.0.1:3240 after attach");
                }
            }
            None => {
                if json {
                    println!("{{\"session\":null}}");
                } else {
                    println!(
                        "No saved session. Run: farbus --connect HOST:7420 pair <fingerprint>"
                    );
                }
            }
        },
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
    Ok(farbus_core::connect_with_retry(
        addr,
        fingerprint,
        Some(saved.auth_token),
        &farbus_core::ReconnectPolicy::default(),
    )
    .await?)
}
