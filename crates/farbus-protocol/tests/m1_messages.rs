use farbus_protocol::{
    decode, encode, AttachRequest, AttachResponse, DetachRequest, DeviceId, DeviceListRequest,
    Message, PairRequest, PairResponse, TransferType, UrbComplete, UrbSubmit,
};

#[test]
fn roundtrips_pair_handshake() {
    let req = Message::PairRequest(PairRequest {
        client_fingerprint: [2; 32],
        pin_hash: [0x5A; 32],
        client_name: "win-workstation".into(),
    });
    let res = Message::PairResponse(PairResponse {
        success: true,
        server_fingerprint: [1; 32],
        auth_token: [0xAA; 32],
    });
    assert_eq!(decode(&encode(&req).unwrap()).unwrap(), req);
    assert_eq!(decode(&encode(&res).unwrap()).unwrap(), res);
}

#[test]
fn roundtrips_attach_handshake() {
    let req = Message::AttachRequest(AttachRequest {
        device_id: DeviceId(42),
        auth_token: [0xBB; 32],
    });
    let res = Message::AttachResponse(AttachResponse {
        device_id: DeviceId(42),
        success: true,
        usbip_port: 3240,
        bus_id: "1-1.4".into(),
    });
    assert_eq!(decode(&encode(&req).unwrap()).unwrap(), req);
    assert_eq!(decode(&encode(&res).unwrap()).unwrap(), res);
}

#[test]
fn roundtrips_urb_submit_and_complete() {
    let submit = Message::UrbSubmit(UrbSubmit {
        seq: 9,
        device_id: DeviceId(1),
        endpoint: 0x81,
        transfer: TransferType::Interrupt,
        data: vec![0x01, 0x02, 0x03],
    });
    let complete = Message::UrbComplete(UrbComplete {
        seq: 9,
        status: 0,
        data: vec![0xAA, 0xBB],
    });
    assert_eq!(decode(&encode(&submit).unwrap()).unwrap(), submit);
    assert_eq!(decode(&encode(&complete).unwrap()).unwrap(), complete);
}

#[test]
fn roundtrips_device_list_request() {
    let req = Message::DeviceListRequest(DeviceListRequest {
        auth_token: [0xCC; 32],
    });
    assert_eq!(decode(&encode(&req).unwrap()).unwrap(), req);
}

#[test]
fn roundtrips_detach_request() {
    let req = Message::DetachRequest(DetachRequest {
        device_id: DeviceId(7),
        auth_token: [0xDD; 32],
    });
    assert_eq!(decode(&encode(&req).unwrap()).unwrap(), req);
}
