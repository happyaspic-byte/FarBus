use farbus_core::{
    make_self_signed, make_server_config, serve_session, DeviceBackend, DeviceId, FarBusClient,
    LocalDevice, ServerState, TransferType, UrbCompleter,
};
use farbus_protocol::{DeviceInfo, UrbComplete, UsbSpeed};
use std::sync::Arc;
use tokio::net::TcpListener;

fn host(bus_id: &str) -> LocalDevice {
    LocalDevice {
        info: DeviceInfo {
            id: DeviceId(0),
            bus_id: bus_id.into(),
            vid: 0x0403,
            pid: 0x6001,
            usb_class: 0xff,
            speed: UsbSpeed::Full,
            product: "Hotplug Serial".into(),
            exported: true,
            interfaces: Vec::new(),
        },
        backend: DeviceBackend::Host,
    }
}

#[tokio::test]
async fn paired_client_observes_hotplug_and_removal_revokes_lease() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(ServerState::new("server".into(), server_fp, Vec::new()));
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

    let mut client = FarBusClient::connect(addr, server_fp).await.unwrap();
    client.pair(&pin, server_fp).await.unwrap();
    assert!(client.devices().await.unwrap().devices.is_empty());

    let added = state.refresh_host_devices(vec![host("1-2")]).await;
    let id = added.added[0];
    assert_eq!(client.devices().await.unwrap().devices[0].id, id);
    client.attach(id).await.unwrap();
    assert!(state.leases.lock().await.owner(id).is_some());

    state.refresh_host_devices(Vec::new()).await;
    assert!(client.devices().await.unwrap().devices.is_empty());
    assert_eq!(state.leases.lock().await.owner(id), None);

    let readded = state.refresh_host_devices(vec![host("1-2")]).await;
    assert_eq!(readded.added.len(), 1);
    let new_id = readded.added[0];
    assert_ne!(new_id, id);
    assert_eq!(state.leases.lock().await.owner(new_id), None);
    client.attach(new_id).await.unwrap();
}

#[tokio::test]
async fn removal_turns_delayed_in_flight_urb_into_failure() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let completer: UrbCompleter = Arc::new(|urb| {
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            UrbComplete {
                seq: urb.seq,
                status: 0,
                data: vec![1],
            }
        })
    });
    let state = Arc::new(
        ServerState::new("server".into(), server_fp, vec![host("1-2")])
            .with_urb_completer(completer),
    );
    let pin = state.pin.lock().await.pin.clone();
    let id = state.devices_snapshot().await[0].info.id;
    tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(stream).await.unwrap();
            let _ = serve_session(&mut tls, state).await;
        }
    });

    let mut client = FarBusClient::connect(addr, server_fp).await.unwrap();
    client.pair(&pin, server_fp).await.unwrap();
    client.attach(id).await.unwrap();
    let urb_client = client.clone();
    let pending = tokio::spawn(async move {
        urb_client
            .urb(id, 7, 0x81, TransferType::Interrupt, vec![0; 8])
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    state.refresh_host_devices(Vec::new()).await;
    state.refresh_host_devices(vec![host("1-2")]).await;

    let result = pending.await.unwrap();
    assert!(result.is_ok());
    let complete = result.unwrap();
    assert_eq!(complete.status, -1);
    assert!(complete.data.is_empty());
}
