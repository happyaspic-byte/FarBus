use farbus_core::{
    make_self_signed, make_server_config, serve_session, simulated_lab_devices, DeviceId,
    FarBusClient, ServerState,
};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn unlink_requires_lease_and_succeeds_after_attach() {
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
        while let Ok((stream, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let state = Arc::clone(&state);
            tokio::spawn(async move {
                if let Ok(mut tls) = acceptor.accept(stream).await {
                    let _ = serve_session(&mut tls, state).await;
                }
            });
        }
    });

    let mut client = FarBusClient::connect(addr, server_fp).await.unwrap();
    client.pair(&pin, server_fp).await.unwrap();
    let err = client.unlink(DeviceId(1), 9).await.unwrap_err();
    assert!(matches!(err, farbus_core::ClientError::AttachRejected));
    client.attach(DeviceId(1)).await.unwrap();
    let status = client.unlink(DeviceId(1), 9).await.unwrap();
    assert_eq!(status, 0);
}
