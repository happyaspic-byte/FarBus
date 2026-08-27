use clap::Parser;
use farbus_core::{
    discovery, make_server_config, scan_host_usb, serve_session, serve_usbip_loopback,
    simulated_lab_devices, ServerState,
};
use farbus_server::{apply_export_policy, Cli};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    let (certs, key, server_fp) = farbus_core::load_or_create_server_identity("farbus.local")?;
    let acceptor = make_server_config(certs, key)?;

    let mut devices = scan_host_usb();
    if devices.is_empty() {
        println!("No physical USB devices found; using simulated lab devices.");
        devices = simulated_lab_devices();
    }
    apply_export_policy(&mut devices, cli.export_all, &cli.export)?;
    let exported = devices.iter().filter(|d| d.info.exported).count();

    let hostname = hostname::get().map_or_else(
        |_| "farbus-server".into(),
        |h| h.to_string_lossy().into_owned(),
    );
    let state = Arc::new(ServerState::new(hostname, server_fp, devices.clone()));

    let loopback_devices = devices;
    let usbip_listen = cli.usbip_listen;
    tokio::spawn(async move {
        let _ = serve_usbip_loopback(loopback_devices, &usbip_listen.to_string()).await;
    });

    let pin = state.pin.lock().await.pin.clone();
    println!("==================================================");
    println!(" FarBus USB Server 0.1.0");
    println!(" Fingerprint : {server_fp}");
    println!(" Pairing PIN : {pin}  (valid for 2 minutes)");
    println!(" Listening   : {}", cli.listen);
    println!(" Discovered  : {} devices", state.devices.len());
    println!(" Exported    : {exported} devices (prefer exact --export BUS-ID selectors)");
    println!("==================================================");

    let listener = TcpListener::bind(cli.listen).await?;

    let announce_addr = cli.listen;
    let announce_fp = server_fp;
    let announce_host = state.hostname.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            let _ = discovery::announce(announce_fp, announce_addr, &announce_host).await;
        }
    });

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let _ = stream.set_nodelay(true);
        let acceptor = acceptor.clone();
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Ok(mut tls) = acceptor.accept(stream).await {
                let _ = serve_session(&mut tls, state).await;
            }
            println!("Disconnected from {peer_addr}");
        });
    }
}
