use farbus_core::{
    make_self_signed, make_server_config, serve_session, simulated_lab_devices, FarBusClient,
    ServerState, TransferType,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};

#[tokio::test]
async fn forward_reconnects_reattaches_and_retries_once() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.reconnect").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(ServerState::new(
        "farbus-server".into(),
        server_fp,
        simulated_lab_devices(),
    ));
    let pin = state.pin.lock().await.pin.clone();

    let (disconnect_tx, disconnect_rx) = oneshot::channel();
    let (disconnected_tx, disconnected_rx) = oneshot::channel();
    let server = {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(stream).await.unwrap();
            tokio::select! {
                _ = serve_session(&mut tls, Arc::clone(&state)) => {}
                _ = disconnect_rx => {}
            }
            drop(tls);
            let _ = disconnected_tx.send(());

            let (stream, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(stream).await.unwrap();
            let _ = serve_session(&mut tls, state).await;
        })
    };

    let mut client = FarBusClient::connect(addr, server_fp).await.unwrap();
    client.pair(&pin, server_fp).await.unwrap();
    let device = client.devices().await.unwrap().devices[0].id;
    client.attach(device).await.unwrap();
    let shared = Arc::new(Mutex::new(client));

    disconnect_tx.send(()).unwrap();
    disconnected_rx.await.unwrap();

    let complete = farbus_core::usbip_forward::urb_with_recovery(
        Arc::clone(&shared),
        device,
        7,
        0x02,
        TransferType::Bulk,
        8,
        vec![0u8; 8],
    )
    .await
    .expect("forward should reconnect, reattach, and retry");
    assert_eq!(complete.seq, 7);
    assert_eq!(complete.status, 0);

    server.abort();
}
