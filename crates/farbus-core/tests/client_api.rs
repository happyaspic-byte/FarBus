use farbus_core::{
    make_self_signed, make_server_config, serve_session, simulated_lab_devices, DeviceId,
    FarBusClient, ServerState,
};
use farbus_protocol::TransferType;
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn farbus_client_high_level_api_roundtrip() {
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

    let mut client = FarBusClient::connect(addr, server_fp).await.unwrap();
    client.pair(&pin, server_fp).await.unwrap();

    let devices = client.devices().await.unwrap();
    assert_eq!(devices.devices.len(), 4);
    let composite = devices
        .devices
        .iter()
        .find(|d| d.bus_id == "1-4")
        .expect("composite device");
    assert_eq!(composite.interfaces.len(), 2);

    let attached = client.attach(DeviceId(2)).await.unwrap();
    assert_eq!(attached.device_id, DeviceId(2));
    assert_eq!(attached.bus_id, "1-2");

    let urb = client
        .urb(DeviceId(2), 100, 0x81, TransferType::Bulk, vec![0u8; 16])
        .await
        .unwrap();
    assert_eq!(urb.seq, 100);
    assert_eq!(urb.status, 0);
    assert_eq!(urb.data.len(), 16);

    client.detach(DeviceId(2)).await.unwrap();
}
