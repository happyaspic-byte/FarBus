use farbus_core::{
    connect_with_retry, make_self_signed, make_server_config, serve_session, simulated_lab_devices,
    ReconnectPolicy, ServerState,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::test]
async fn reconnect_succeeds_after_server_comes_up() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let delayed = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let listener = TcpListener::bind(addr).await.unwrap();
        let state = Arc::new(ServerState::new(
            "farbus-server".into(),
            server_fp,
            simulated_lab_devices(),
        ));
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();
        let _ = serve_session(&mut tls, state).await;
    });

    let policy = ReconnectPolicy {
        initial: Duration::from_millis(50),
        max: Duration::from_millis(200),
        max_attempts: 8,
    };
    let client = connect_with_retry(addr, server_fp, None, &policy)
        .await
        .expect("reconnect after server start");
    drop(client);
    let _ = delayed.await;
}
