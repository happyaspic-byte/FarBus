use farbus_core::{
    happy_eyeballs_connect, make_self_signed, make_server_config, serve_session,
    simulated_lab_devices, ServerState,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn happy_eyeballs_skips_dead_address_and_connects() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let live = listener.local_addr().unwrap();
    let dead = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);
    let state = Arc::new(ServerState::new(
        "farbus-server".into(),
        server_fp,
        simulated_lab_devices(),
    ));
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();
        let _ = serve_session(&mut tls, state).await;
    });

    let client = happy_eyeballs_connect([dead, live], server_fp)
        .await
        .expect("connect via live address");
    drop(client);
}
