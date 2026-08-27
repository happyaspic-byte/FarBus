use farbus_core::{
    make_pinned_client_config, make_self_signed, make_server_config, read_message, serve_session,
    simulated_lab_devices, write_message, DetachRequest, DeviceId, Hello, Identity, Message,
    ServerState, VERSION,
};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn spoofed_hello_cannot_detach_another_clients_lease() {
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
    let server = tokio::spawn({
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

    let mut owner = farbus_core::FarBusClient::connect(addr, server_fp)
        .await
        .unwrap();
    owner.pair(&pin, server_fp).await.unwrap();
    owner.attach(DeviceId(1)).await.unwrap();

    let connector = make_pinned_client_config(server_fp).unwrap();
    let tcp = TcpStream::connect(addr).await.unwrap();
    let name = rustls::pki_types::ServerName::try_from("farbus.local").unwrap();
    let mut attacker = connector.connect(name, tcp).await.unwrap();
    let claimed = Identity::generate();
    write_message(
        &mut attacker,
        &Message::Hello(Hello {
            protocol_min: VERSION,
            protocol_max: VERSION,
            fingerprint: *claimed.fingerprint.as_bytes(),
            hostname: "attacker".into(),
        }),
    )
    .await
    .unwrap();
    let _ = read_message(&mut attacker).await.unwrap();
    write_message(
        &mut attacker,
        &Message::DetachRequest(DetachRequest {
            device_id: DeviceId(1),
            auth_token: [0; 32],
        }),
    )
    .await
    .unwrap();
    assert!(matches!(
        read_message(&mut attacker).await.unwrap(),
        Message::Error { .. }
    ));
    assert!(state.leases.lock().await.owner(DeviceId(1)).is_some());
    drop(attacker);
    drop(owner);
    server.abort();
}
