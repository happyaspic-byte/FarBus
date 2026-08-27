use clap::Parser;
use farbus_core::{
    discovery, make_server_config, scan_host_usb, serve_session, serve_usbip_loopback_state,
    LocalDevice, ServerState,
};
use farbus_server::{apply_export_flags, apply_export_policy, Cli};
use std::sync::Arc;
use tokio::net::TcpListener;

#[cfg(target_os = "linux")]
fn scan_hotplug_usb() -> std::io::Result<Vec<LocalDevice>> {
    farbus_core::try_scan_libusb().map_err(|error| std::io::Error::other(error.to_string()))
}

#[cfg(not(target_os = "linux"))]
fn scan_hotplug_usb() -> std::io::Result<Vec<LocalDevice>> {
    farbus_core::try_scan_host_usb()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    let (certs, key, server_fp) = farbus_core::load_or_create_server_identity("farbus.local")?;
    let acceptor = make_server_config(certs, key)?;

    let mut devices = scan_host_usb();
    if devices.is_empty() {
        println!("No physical USB devices found; waiting for hotplug events.");
    }
    apply_export_policy(&mut devices, cli.export_all, &cli.export);
    let exported = devices.iter().filter(|d| d.info.exported).count();

    let hostname = hostname::get().map_or_else(
        |_| "farbus-server".into(),
        |h| h.to_string_lossy().into_owned(),
    );
    let state = Arc::new(ServerState::new(hostname, server_fp, devices));

    let usbip_state = Arc::clone(&state);
    let usbip_listen = cli.usbip_listen;
    tokio::spawn(async move {
        let _ = serve_usbip_loopback_state(usbip_state, &usbip_listen.to_string()).await;
    });

    let hotplug_state = Arc::clone(&state);
    let export_all = cli.export_all;
    let exact_exports = cli.export.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            let Ok(Ok(mut scanned)) = tokio::task::spawn_blocking(scan_hotplug_usb).await else {
                continue;
            };
            apply_export_flags(&mut scanned, export_all, &exact_exports);
            let delta = hotplug_state.refresh_host_devices(scanned).await;
            for id in delta.added {
                println!("USB hotplug added device {}", id.0);
            }
            for id in delta.removed {
                println!("USB hotplug removed device {}", id.0);
            }
        }
    });

    let pin = state.pin.lock().await.pin.clone();
    println!("==================================================");
    println!(" FarBus USB Server 0.1.0");
    println!(" Fingerprint : {server_fp}");
    println!(" Pairing PIN : {pin}  (valid for 2 minutes)");
    println!(" Listening   : {}", cli.listen);
    println!(
        " Discovered  : {} devices",
        state.devices_snapshot().await.len()
    );
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
