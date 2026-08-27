use farbus_core::frame::{FrameError, FramedReader};
use farbus_protocol::{encode, Hello, Message, VERSION};
use std::io::Cursor;

#[tokio::test]
async fn framed_reader_recovers_after_interrupted_read() {
    let msg = Message::Hello(Hello {
        protocol_min: VERSION,
        protocol_max: VERSION,
        fingerprint: [7u8; 32],
        hostname: "resilient-client".into(),
    });
    let bytes = encode(&msg).unwrap();
    assert!(bytes.len() > 10);

    // Split stream into two chunks
    let (part1, part2) = bytes.split_at(5);
    let mut reader = FramedReader::new();

    // First attempt with only 5 bytes: returns Pending / WouldBlock or reads partially
    let mut cursor1 = Cursor::new(part1);
    let res = reader.read_message(&mut cursor1).await;
    assert!(matches!(res, Err(FrameError::Io(_))));

    // Second attempt with the rest of bytes
    let mut cursor2 = Cursor::new(part2);
    let parsed = reader.read_message(&mut cursor2).await.unwrap();
    assert_eq!(parsed, msg);
}
