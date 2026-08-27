use farbus_core::{
    make_self_signed, make_server_config, serve_session, simulated_lab_devices, DeviceId,
    FarBusClient, ServerState, TransferType,
};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn urb_without_attach_is_rejected() {
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
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();
        let _ = serve_session(&mut tls, state).await;
    });

    let mut client = FarBusClient::connect(addr, server_fp).await.unwrap();
    client.pair(&pin, server_fp).await.unwrap();
    let err = client
        .urb(
            DeviceId(1),
            1,
            0,
            TransferType::Control,
            vec![0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x12, 0x00],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, farbus_core::ClientError::AttachRejected));
}
