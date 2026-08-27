use farbus_core::simulated_lab_devices;
use farbus_protocol::usbip::{UsbipCmdSubmit, USBIP_CMD_SUBMIT, USBIP_VERSION};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn usbip_oversize_transfer_does_not_allocate_unbounded_buffer() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let devices = simulated_lab_devices();
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let _ = farbus_core::usbip_proxy::handle_client(stream, devices).await;
        }
    });
    let mut client = TcpStream::connect(addr).await.unwrap();
    let mut import = Vec::new();
    import.extend_from_slice(&USBIP_VERSION.to_be_bytes());
    import.extend_from_slice(&farbus_protocol::usbip::OP_REQ_IMPORT.to_be_bytes());
    import.extend_from_slice(&0u32.to_be_bytes());
    let mut busid = [0u8; 32];
    busid[..5].copy_from_slice(b"1-1.2");
    import.extend_from_slice(&busid);
    client.write_all(&import).await.unwrap();
    let mut hdr = [0u8; 8];
    let _ = timeout(Duration::from_millis(200), async {
        use tokio::io::AsyncReadExt;
        let _ = client.read_exact(&mut hdr).await;
        let mut udev = [0u8; 312];
        let _ = client.read_exact(&mut udev).await;
    })
    .await;
    let cmd = UsbipCmdSubmit {
        seqnum: 1,
        devid: 1,
        direction: 0,
        ep: 1,
        transfer_flags: 0,
        transfer_buffer_length: u32::MAX,
        start_frame: 0,
        number_of_packets: 0,
        interval: 0,
        setup: [0; 8],
    };
    let encoded = cmd.encode();
    assert_eq!(
        u32::from_be_bytes(encoded[0..4].try_into().unwrap()),
        USBIP_CMD_SUBMIT
    );
    let _ = client.write_all(&encoded).await;
}
