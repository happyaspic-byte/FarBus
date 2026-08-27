use farbus_protocol::{
    decode, encode, AttachRequest, AttachResponse, DeviceId, DeviceInfo, DeviceList,
    DeviceListRequest, Message, PairRequest, PairResponse, TransferType, UrbComplete, UrbSubmit,
    UrbUnlink, UrbUnlinked, UsbInterfaceInfo, UsbSpeed,
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
fn roundtrips_composite_device_list() {
    let msg = Message::DeviceList(DeviceList {
        devices: vec![DeviceInfo {
            id: DeviceId(10),
            bus_id: "1-4".into(),
            vid: 0x046d,
            pid: 0xc52b,
            usb_class: 0,
            speed: UsbSpeed::High,
            product: "Composite Receiver".into(),
            exported: true,
            interfaces: vec![
                UsbInterfaceInfo {
                    interface_number: 0,
                    interface_class: 3,
                    interface_subclass: 1,
                    interface_protocol: 1,
                    endpoints: vec![0x81],
                },
                UsbInterfaceInfo {
                    interface_number: 1,
                    interface_class: 3,
                    interface_subclass: 1,
                    interface_protocol: 2,
                    endpoints: vec![0x82],
                },
            ],
        }],
    });
    assert_eq!(decode(&encode(&msg).unwrap()).unwrap(), msg);
}

#[test]
fn roundtrips_urb_unlink_and_unlinked() {
    let unlink = Message::UrbUnlink(UrbUnlink {
        seq: 55,
        device_id: DeviceId(3),
    });
    let unlinked = Message::UrbUnlinked(UrbUnlinked { seq: 55, status: 0 });
    assert_eq!(decode(&encode(&unlink).unwrap()).unwrap(), unlink);
    assert_eq!(decode(&encode(&unlinked).unwrap()).unwrap(), unlinked);
}
