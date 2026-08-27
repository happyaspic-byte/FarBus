use farbus_core::{
    hash_pin, make_pinned_client_config, make_self_signed, make_server_config, read_message,
    serve_session, simulated_lab_devices, write_message, DeviceId, Identity, Message, PairRequest,
    ServerState,
};
use farbus_protocol::{
    AttachRequest, AttachResponse, Hello, PairResponse, TransferType, UrbSubmit, VERSION,
};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn server_session_e2e_full_lifecycle() {
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

    // Server loop
    let server_task = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(stream).await.unwrap();
            serve_session(&mut tls, state).await.unwrap();
        }
    });

    // Client
    let connector = make_pinned_client_config(server_fp).unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("farbus.local").unwrap();
    let mut client = connector.connect(server_name, stream).await.unwrap();

    let client_id = Identity::generate();

    // 1. Hello
    write_message(
        &mut client,
        &Message::Hello(Hello {
            protocol_min: VERSION,
            protocol_max: VERSION,
            fingerprint: *client_id.fingerprint.as_bytes(),
            hostname: "client-host".into(),
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut client).await.unwrap();
    assert!(matches!(res, Message::Hello(_)));

    // 2. Pair
    let pin_hash = hash_pin(&pin, server_fp);
    write_message(
        &mut client,
        &Message::PairRequest(PairRequest {
            client_fingerprint: *client_id.fingerprint.as_bytes(),
            pin_hash,
            client_name: "client-box".into(),
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut client).await.unwrap();
    let token = match res {
        Message::PairResponse(PairResponse {
            success: true,
            auth_token,
            ..
        }) => auth_token,
        other => panic!("expected PairResponse, got {other:?}"),
    };

    // 3. List Devices
    write_message(
        &mut client,
        &Message::DeviceListRequest(farbus_protocol::DeviceListRequest { auth_token: token }),
    )
    .await
    .unwrap();
    let res = read_message(&mut client).await.unwrap();
    match res {
        Message::DeviceList(list) => {
            assert_eq!(list.devices.len(), 3);
            assert_eq!(list.devices[0].product, "USB Keyboard");
        }
        other => panic!("expected DeviceList, got {other:?}"),
    }

    // 4. Attach Keyboard
    write_message(
        &mut client,
        &Message::AttachRequest(AttachRequest {
            device_id: DeviceId(1),
            auth_token: token,
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut client).await.unwrap();
    match res {
        Message::AttachResponse(AttachResponse {
            success: true,
            bus_id,
            ..
        }) => {
            assert_eq!(bus_id, "1-1.2");
        }
        other => panic!("expected AttachResponse, got {other:?}"),
    }

    // 5. Submit URB and read response
    write_message(
        &mut client,
        &Message::UrbSubmit(UrbSubmit {
            seq: 1,
            device_id: DeviceId(1),
            endpoint: 0x81,
            transfer: TransferType::Interrupt,
            data: Vec::new(),
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut client).await.unwrap();
    match res {
        Message::UrbComplete(urb) => {
            assert_eq!(urb.seq, 1);
            assert_eq!(urb.status, 0);
            assert_eq!(urb.data[1], b'A');
        }
        other => panic!("expected UrbComplete, got {other:?}"),
    }

    // 6. Detach
    write_message(
        &mut client,
        &Message::DetachRequest(farbus_protocol::DetachRequest {
            device_id: DeviceId(1),
            auth_token: token,
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut client).await.unwrap();
    assert!(matches!(res, Message::Detach { .. }));

    drop(client);
    server_task.await.unwrap();
}
