use farbus_protocol::{decode, encode, DeviceId, Error, Message};

#[test]
fn rejects_trailing_bytes_after_valid_frame() {
    let mut bytes = encode(&Message::Detach {
        device_id: DeviceId(1),
    })
    .unwrap();
    bytes.push(0xFF);
    let err = decode(&bytes).unwrap_err();
    assert!(matches!(err, Error::InvalidPayload));
}

#[test]
fn rejects_unknown_message_type() {
    let mut bytes = encode(&Message::Detach {
        device_id: DeviceId(1),
    })
    .unwrap();
    bytes[5] = 99;
    bytes[6..10].copy_from_slice(&0u32.to_be_bytes());
    let err = decode(&bytes[..10]).unwrap_err();
    assert!(matches!(err, Error::UnknownType(99)));
}
