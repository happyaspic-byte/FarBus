use farbus_core::fingerprint::PeerFingerprint;
use farbus_core::session::{serve_session, ServerState, UrbCompleter};
use farbus_core::{complete_urb, simulated_lab_devices};
use farbus_protocol::{
    AttachRequest, DeviceId, Hello, Message, PairRequest, TransferType, UrbSubmit, VERSION,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::duplex;

#[tokio::test]
async fn later_urb_completes_without_waiting_for_slow_earlier_urb() {
    let server_fp = PeerFingerprint::new([1u8; 32]);
    let client_fp = PeerFingerprint::new([2u8; 32]);
    let completer: UrbCompleter = Arc::new(|urb| {
        Box::pin(async move {
            if urb.seq == 1 {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            complete_urb(&urb)
        })
    });
    let state = Arc::new(
        ServerState::new("test-server".into(), server_fp, simulated_lab_devices())
            .with_urb_completer(completer),
    );
    let pin = state.pin.lock().await.pin.clone();

    let (mut client_io, mut server_io) = duplex(65_536);
    let server_state = Arc::clone(&state);
    let server = tokio::spawn(async move {
        let _ = serve_session(&mut server_io, server_state).await;
    });

    farbus_core::frame::write_message(
        &mut client_io,
        &Message::Hello(Hello {
            protocol_min: VERSION,
            protocol_max: VERSION,
            fingerprint: *client_fp.as_bytes(),
            hostname: "client".into(),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(
        farbus_core::frame::read_message(&mut client_io)
            .await
            .unwrap(),
        Message::Hello(_)
    ));

    farbus_core::frame::write_message(
        &mut client_io,
        &Message::PairRequest(PairRequest {
            client_fingerprint: *client_fp.as_bytes(),
            pin_hash: farbus_core::identity::hash_pin(&pin, server_fp),
            client_name: "client".into(),
        }),
    )
    .await
    .unwrap();
    let token = match farbus_core::frame::read_message(&mut client_io)
        .await
        .unwrap()
    {
        Message::PairResponse(response) if response.success => response.auth_token,
        other => panic!("pairing failed: {other:?}"),
    };

    farbus_core::frame::write_message(
        &mut client_io,
        &Message::AttachRequest(AttachRequest {
            device_id: DeviceId(1),
            auth_token: token,
        }),
    )
    .await
    .unwrap();
    assert!(matches!(
        farbus_core::frame::read_message(&mut client_io)
            .await
            .unwrap(),
        Message::AttachResponse(_)
    ));

    for seq in [1, 2] {
        farbus_core::frame::write_message(
            &mut client_io,
            &Message::UrbSubmit(UrbSubmit {
                seq,
                device_id: DeviceId(1),
                endpoint: 0x81,
                transfer: TransferType::Interrupt,
                data: vec![0; 8],
            }),
        )
        .await
        .unwrap();
    }

    let first = tokio::time::timeout(
        Duration::from_millis(75),
        farbus_core::frame::read_message(&mut client_io),
    )
    .await
    .expect("the second URB was blocked behind the first")
    .unwrap();
    assert!(matches!(first, Message::UrbComplete(complete) if complete.seq == 2));

    let second = farbus_core::frame::read_message(&mut client_io)
        .await
        .unwrap();
    assert!(matches!(second, Message::UrbComplete(complete) if complete.seq == 1));

    drop(client_io);
    server.await.unwrap();
}
