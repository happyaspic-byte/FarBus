use clap::Parser;
use farbus_core::{
    discovery, make_server_config, scan_host_usb, serve_session, serve_usbip_loopback,
    simulated_lab_devices, ServerState,
};
use farbus_server::Cli;
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
    if cli.export_all {
        for device in &mut devices {
            device.info.exported = true;
        }
    }
    if !cli.export.is_empty() {
        for device in &mut devices {
            if cli.export.iter().any(|bus| bus == &device.info.bus_id) {
                device.info.exported = true;
            }
        }
    }
    let exported = devices.iter().filter(|d| d.info.exported).count();

    let hostname = hostname::get().map_or_else(
        |_| "farbus-server".into(),
        |h| h.to_string_lossy().into_owned(),
    );
    let state = Arc::new(ServerState::new(hostname, server_fp, devices.clone()));

    // Spawn loopback USB/IP 1.1 stub listener on 127.0.0.1:3240 (or 3241 if 3240 taken)
    let loopback_devices = devices;
    tokio::spawn(async move {
        let _ = serve_usbip_loopback(loopback_devices, "127.0.0.1:3240").await;
    });

    let pin = state.pin.lock().await.pin.clone();
    println!("==================================================");
    println!(" FarBus USB Server 0.1.0");
    println!(" Fingerprint : {server_fp}");
    println!(" Pairing PIN : {pin}  (valid for 2 minutes)");
    println!(" Listening   : {}", cli.listen);
    println!(" Discovered  : {} devices", state.devices.len());
    println!(" Exported    : {exported} devices (use --export-all to share physical USB)");
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
