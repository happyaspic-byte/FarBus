use farbus_core::{
    make_self_signed, make_server_config, serve_session, simulated_lab_devices, FarBusClient,
    ServerState,
};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn device_list_requires_pairing() {
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
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();
        let _ = serve_session(&mut tls, state).await;
    });

    let mut client = FarBusClient::connect(addr, server_fp).await.unwrap();
    let err = client.devices().await.unwrap_err();
    assert!(matches!(err, farbus_core::ClientError::PairRejected));
}
