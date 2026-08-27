use farbus_core::simulated_lab_devices;
use farbus_protocol::usbip::{
    UsbipCmdSubmit, UsbipRetSubmit, OP_REP_DEVLIST, OP_REP_IMPORT, OP_REQ_DEVLIST, OP_REQ_IMPORT,
    USBIP_VERSION,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test]
async fn usbip_devlist_import_and_interrupt_urb() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let devices = simulated_lab_devices();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let devices = devices.clone();
            tokio::spawn(async move {
                let _ = farbus_core::usbip_proxy::handle_client(stream, devices).await;
            });
        }
    });

    let mut client = TcpStream::connect(addr).await.unwrap();
    let mut req = Vec::new();
    req.extend_from_slice(&USBIP_VERSION.to_be_bytes());
    req.extend_from_slice(&OP_REQ_DEVLIST.to_be_bytes());
    req.extend_from_slice(&0u32.to_be_bytes());
    client.write_all(&req).await.unwrap();

    let mut header = [0u8; 12];
    client.read_exact(&mut header).await.unwrap();
    assert_eq!(u16::from_be_bytes([header[0], header[1]]), USBIP_VERSION);
    assert_eq!(u16::from_be_bytes([header[2], header[3]]), OP_REP_DEVLIST);
    let ndev = u32::from_be_bytes(header[8..12].try_into().unwrap());
    assert_eq!(ndev, 4);
    let mut saw_composite = false;
    for _ in 0..4 {
        let mut udev = [0u8; 312];
        client.read_exact(&mut udev).await.unwrap();
        let niface = usize::from(udev[311]);
        let mut ifaces = vec![0u8; niface.max(1) * 4];
        client.read_exact(&mut ifaces).await.unwrap();
        if niface == 2 {
            saw_composite = true;
            assert_eq!(ifaces[0], 3);
            assert_eq!(ifaces[4], 3);
        }
    }
    assert!(saw_composite);

    let mut import = Vec::new();
    import.extend_from_slice(&USBIP_VERSION.to_be_bytes());
    import.extend_from_slice(&OP_REQ_IMPORT.to_be_bytes());
    import.extend_from_slice(&0u32.to_be_bytes());
    let mut busid = [0u8; 32];
    busid[..5].copy_from_slice(b"1-1.2");
    import.extend_from_slice(&busid);
    client.write_all(&import).await.unwrap();

    let mut import_hdr = [0u8; 8];
    client.read_exact(&mut import_hdr).await.unwrap();
    assert_eq!(
        u16::from_be_bytes([import_hdr[2], import_hdr[3]]),
        OP_REP_IMPORT
    );
    assert_eq!(u32::from_be_bytes(import_hdr[4..8].try_into().unwrap()), 0);
    let mut udev = [0u8; 312];
    client.read_exact(&mut udev).await.unwrap();

    let cmd = UsbipCmdSubmit {
        seqnum: 7,
        devid: 1,
        direction: 1,
        ep: 0x81,
        transfer_flags: 0,
        transfer_buffer_length: 8,
        start_frame: 0,
        number_of_packets: 0,
        interval: 1,
        setup: [0; 8],
    };
    client.write_all(&cmd.encode()).await.unwrap();
    let mut ret_hdr = [0u8; 48];
    client.read_exact(&mut ret_hdr).await.unwrap();
    let reply = UsbipRetSubmit::decode(&ret_hdr).unwrap();
    assert_eq!(reply.seqnum, 7);
    assert_eq!(reply.status, 0);
    assert!(reply.actual_length > 0);
    let mut payload = vec![0u8; reply.actual_length as usize];
    client.read_exact(&mut payload).await.unwrap();
    assert_eq!(payload[1], b'A');
}
