use farbus_core::{
    make_self_signed, make_server_config, serve_session, simulated_lab_devices, ServerState,
};
use farbus_gui::actions;
use farbus_gui::{apply, loopback_usbip, sanitize_pin, GuiEvent, GuiPhase, GuiState};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;

fn tempfile_dir() -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "farbus-gui-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn sanitize_pin_strips_non_digits() {
    assert_eq!(sanitize_pin("12ab34-56"), "123456");
    assert_eq!(sanitize_pin("1"), "1");
}

#[test]
fn loopback_usbip_rejects_lan_addresses() {
    let lan: SocketAddr = "192.168.1.10:3240".parse().unwrap();
    assert!(loopback_usbip(lan).is_none());
    assert!(loopback_usbip(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3240)).is_some());
}

#[tokio::test]
async fn probe_reads_tls_certificate_fingerprint() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(ServerState::new(
        "farbus-server".into(),
        server_fp,
        simulated_lab_devices(),
    ));
    let _server = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            while let Ok((stream, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(stream).await {
                        let _ = serve_session(&mut tls, state).await;
                    }
                });
            }
        }
    });

    let found = actions::probe_server(&addr.to_string())
        .await
        .expect("probe");
    assert_eq!(found.fingerprint, server_fp);
    assert_eq!(found.addr, addr);
}

#[tokio::test]
async fn pair_rejects_short_pin_without_network() {
    let err = actions::pair_server(
        "127.0.0.1:7420".parse().unwrap(),
        farbus_core::PeerFingerprint::new([1; 32]),
        "123",
    )
    .await
    .expect_err("short pin");
    assert!(err.contains("6-digit"));
}

#[tokio::test]
async fn pair_and_list_devices_over_tls() {
    let tmp = tempfile_dir();
    std::env::set_var("HOME", &tmp);
    std::env::set_var("USERPROFILE", &tmp);
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(ServerState::new(
        "farbus-server".into(),
        server_fp,
        simulated_lab_devices(),
    ));
    let pin = state.pin.lock().await.pin.clone();
    let _server = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            while let Ok((stream, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(stream).await {
                        let _ = serve_session(&mut tls, state).await;
                    }
                });
            }
        }
    });

    let session = actions::pair_server(addr, server_fp, &pin)
        .await
        .expect("gui pair");
    let devices = actions::load_devices(session, None)
        .await
        .expect("gui devices");
    assert!(!devices.is_empty());

    let mut gui = GuiState::new();
    apply(
        &mut gui,
        GuiEvent::PairSucceeded {
            addr: session.addr,
            fingerprint: session.fingerprint,
        },
    );
    apply(&mut gui, GuiEvent::DevicesLoaded(devices));
    assert_eq!(gui.phase, GuiPhase::Ready);
}
