use farbus_core::session::UrbCompleter;
use farbus_core::{
    complete_urb, make_self_signed, make_server_config, serve_session, simulated_lab_devices,
    DeviceId, FarBusClient, ServerState, TransferType,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::test]
async fn farbus_client_later_urb_completes_without_waiting_for_slow_earlier_urb() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let completer: UrbCompleter = Arc::new(|urb| {
        Box::pin(async move {
            if urb.seq == 1 {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            complete_urb(&urb)
        })
    });
    let state = Arc::new(
        ServerState::new("farbus-server".into(), server_fp, simulated_lab_devices())
            .with_urb_completer(completer),
    );
    let pin = state.pin.lock().await.pin.clone();

    tokio::spawn({
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

    let client = FarBusClient::connect(addr, server_fp).await.unwrap();
    let mut client = client;
    client.pair(&pin, server_fp).await.unwrap();
    client.attach(DeviceId(1)).await.unwrap();
    let client = Arc::new(client);

    let slow = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            client
                .urb(DeviceId(1), 1, 0x81, TransferType::Interrupt, vec![0; 8])
                .await
                .unwrap()
        })
    };
    let fast = {
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            client
                .urb(DeviceId(1), 2, 0x81, TransferType::Interrupt, vec![0; 8])
                .await
                .unwrap()
        })
    };

    let first = tokio::time::timeout(Duration::from_millis(75), fast)
        .await
        .expect("the second URB was blocked behind the first")
        .unwrap();
    assert_eq!(first.seq, 2);
    assert_eq!(first.status, 0);

    let second = slow.await.unwrap();
    assert_eq!(second.seq, 1);
    assert_eq!(second.status, 0);
}
