use farbus_protocol::{decode, encode, DeviceId, Hello, Message, TransferType, UrbSubmit};
use proptest::prelude::*;

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic_decoder(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = decode(&bytes);
    }

    #[test]
    fn roundtrip_hello_proptest(
        min in 1u8..=5,
        max in 1u8..=5,
        fp in proptest::array::uniform32(any::<u8>()),
        host in "[a-zA-Z0-9_-]{1,64}"
    ) {
        let msg = Message::Hello(Hello {
            protocol_min: min,
            protocol_max: max,
            fingerprint: fp,
            hostname: host,
        });
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();
        prop_assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_urb_submit_proptest(
        seq in any::<u32>(),
        dev in any::<u32>(),
        ep in any::<u8>(),
        data in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        let msg = Message::UrbSubmit(UrbSubmit {
            seq,
            device_id: DeviceId(dev),
            endpoint: ep,
            transfer: TransferType::Bulk,
            requested_length: u32::try_from(data.len()).unwrap_or(0),
            data,
        });
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();
        prop_assert_eq!(msg, decoded);
    }
}
