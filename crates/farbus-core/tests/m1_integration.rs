use farbus_core::{
    complete_urb, issue_auth_token, make_pinned_client_config, make_self_signed,
    make_server_config, read_message, write_message, Identity, LeaseBook, PairingPin,
};
use farbus_protocol::{
    AttachRequest, AttachResponse, DeviceId, Hello, Message, PairRequest, PairResponse,
    TransferType, UrbSubmit,
};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn full_tls_handshake_pairing_and_urb_roundtrip() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").expect("cert gen");
    let acceptor = make_server_config(certs, key).expect("server tls");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server_identity = Identity::generate();
    let current_pin = Arc::new(Mutex::new(PairingPin::issue(server_fp)));
    let pin_value = current_pin.lock().await.pin.clone();
    let lease_book = Arc::new(Mutex::new(LeaseBook::default()));

    // Server Task
    let server_task = tokio::spawn({
        let current_pin = Arc::clone(&current_pin);
        let lease_book = Arc::clone(&lease_book);
        async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(stream).await.unwrap();

            // 1. Hello
            let msg = read_message(&mut tls).await.unwrap();
            assert!(matches!(msg, Message::Hello(_)));
            write_message(
                &mut tls,
                &Message::Hello(Hello {
                    protocol_min: 1,
                    protocol_max: 1,
                    fingerprint: *server_identity.fingerprint.as_bytes(),
                    hostname: "farbus-server".into(),
                }),
            )
            .await
            .unwrap();

            // 2. Pair Request
            let msg = read_message(&mut tls).await.unwrap();
            let mut issued_token = [0u8; 32];
            if let Message::PairRequest(pair) = msg {
                let pin_guard = current_pin.lock().await;
                let ok = pin_guard.is_valid(&pair.pin_hash);
                if ok {
                    issued_token = issue_auth_token();
                }
                write_message(
                    &mut tls,
                    &Message::PairResponse(PairResponse {
                        success: ok,
                        server_fingerprint: *server_fp.as_bytes(),
                        auth_token: issued_token,
                    }),
                )
                .await
                .unwrap();
            } else {
                panic!("expected PairRequest");
            }

            // 3. Attach Request
            let msg = read_message(&mut tls).await.unwrap();
            if let Message::AttachRequest(attach) = msg {
                let mut leases = lease_book.lock().await;
                let client_fp = server_identity.fingerprint;
                let success = leases.acquire(attach.device_id, client_fp).is_ok();
                write_message(
                    &mut tls,
                    &Message::AttachResponse(AttachResponse {
                        device_id: attach.device_id,
                        success,
                        usbip_port: 3240,
                        bus_id: "1-1.2".into(),
                    }),
                )
                .await
                .unwrap();
            }

            // 4. URB Submit -> Complete
            let msg = read_message(&mut tls).await.unwrap();
            if let Message::UrbSubmit(urb) = msg {
                let completed = complete_urb(&urb);
                write_message(&mut tls, &Message::UrbComplete(completed))
                    .await
                    .unwrap();
            }
        }
    });

    // Client
    let connector = make_pinned_client_config(server_fp).expect("client tls");
    let stream = TcpStream::connect(addr).await.expect("tcp connect");
    let server_name = rustls::pki_types::ServerName::try_from("farbus.local").unwrap();
    let mut tls = connector
        .connect(server_name, stream)
        .await
        .expect("tls connect");

    // 1. Send Hello
    let client_identity = Identity::generate();
    write_message(
        &mut tls,
        &Message::Hello(Hello {
            protocol_min: 1,
            protocol_max: 1,
            fingerprint: *client_identity.fingerprint.as_bytes(),
            hostname: "client-box".into(),
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut tls).await.unwrap();
    assert!(matches!(res, Message::Hello(_)));

    // 2. Pair
    let pin_hash = farbus_core::hash_pin(&pin_value, server_fp);
    write_message(
        &mut tls,
        &Message::PairRequest(PairRequest {
            client_fingerprint: *client_identity.fingerprint.as_bytes(),
            pin_hash,
            client_name: "test-client".into(),
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut tls).await.unwrap();
    let token = match res {
        Message::PairResponse(p) => {
            assert!(p.success);
            assert_ne!(p.auth_token, [0u8; 32]);
            p.auth_token
        }
        other => panic!("expected PairResponse, got {other:?}"),
    };

    // 3. Attach
    write_message(
        &mut tls,
        &Message::AttachRequest(AttachRequest {
            device_id: DeviceId(1),
            auth_token: token,
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut tls).await.unwrap();
    match res {
        Message::AttachResponse(a) => {
            assert!(a.success);
            assert_eq!(a.device_id, DeviceId(1));
            assert_eq!(a.bus_id, "1-1.2");
        }
        other => panic!("expected AttachResponse, got {other:?}"),
    }

    // 4. Send URB Control Submit -> Read Complete
    write_message(
        &mut tls,
        &Message::UrbSubmit(UrbSubmit {
            seq: 101,
            device_id: DeviceId(1),
            endpoint: 0,
            transfer: TransferType::Control,
            data: vec![0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x12, 0x00],
        }),
    )
    .await
    .unwrap();
    let res = read_message(&mut tls).await.unwrap();
    match res {
        Message::UrbComplete(urb) => {
            assert_eq!(urb.seq, 101);
            assert_eq!(urb.status, 0);
            assert!(!urb.data.is_empty());
        }
        other => panic!("expected UrbComplete, got {other:?}"),
    }

    server_task.await.unwrap();
}
