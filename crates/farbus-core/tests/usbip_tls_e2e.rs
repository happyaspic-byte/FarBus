use farbus_core::{
    make_self_signed, make_server_config, serve_session, simulated_lab_devices, FarBusClient,
    ServerState,
};
use farbus_protocol::usbip::{
    UsbipCmdSubmit, UsbipRetSubmit, OP_REP_DEVLIST, OP_REP_IMPORT, OP_REQ_DEVLIST, OP_REQ_IMPORT,
    USBIP_VERSION,
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn full_end_to_end_usbip_over_tls_proxy() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // 1. Remote FarBus Server (TLS 1.3)
    let (certs, key, server_fp) = make_self_signed("farbus.remote").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_listener.local_addr().unwrap();

    let server_devices = simulated_lab_devices();
    let server_state = Arc::new(ServerState::new(
        "farbus-remote".into(),
        server_fp,
        server_devices.clone(),
    ));
    let pin = server_state.pin.lock().await.pin.clone();

    let _server_task = tokio::spawn({
        let server_state = Arc::clone(&server_state);
        async move {
            while let Ok((stream, _)) = server_listener.accept().await {
                let acceptor = acceptor.clone();
                let state = Arc::clone(&server_state);
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(stream).await {
                        let _ = serve_session(&mut tls, state).await;
                    }
                });
            }
        }
    });

    // 2. FarBus Client (TLS 1.3 connected to remote server)
    let mut farbus_client = FarBusClient::connect(server_addr, server_fp).await.unwrap();
    farbus_client.pair(&pin, server_fp).await.unwrap();
    let client_shared = Arc::new(Mutex::new(farbus_client));

    // 3. Local Loopback USB/IP proxy (listening for Windows/Linux usbip tool)
    let local_usbip_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_usbip_addr = local_usbip_listener.local_addr().unwrap();
    let forward_devices = server_devices;

    tokio::spawn(async move {
        while let Ok((stream, _)) = local_usbip_listener.accept().await {
            let devices = forward_devices.clone();
            let client = Arc::clone(&client_shared);
            tokio::spawn(async move {
                let _ =
                    farbus_core::usbip_forward::handle_forward_for_test(stream, devices, client)
                        .await;
            });
        }
    });

    // 4. Standard USB/IP tool simulation connecting to local loopback proxy
    let mut tool = TcpStream::connect(local_usbip_addr).await.unwrap();

    // Query Device List
    let mut req = Vec::new();
    req.extend_from_slice(&USBIP_VERSION.to_be_bytes());
    req.extend_from_slice(&OP_REQ_DEVLIST.to_be_bytes());
    req.extend_from_slice(&0u32.to_be_bytes());
    tool.write_all(&req).await.unwrap();

    let mut devlist_hdr = [0u8; 12];
    tool.read_exact(&mut devlist_hdr).await.unwrap();
    assert_eq!(
        u16::from_be_bytes([devlist_hdr[0], devlist_hdr[1]]),
        USBIP_VERSION
    );
    assert_eq!(
        u16::from_be_bytes([devlist_hdr[2], devlist_hdr[3]]),
        OP_REP_DEVLIST
    );
    let ndev = u32::from_be_bytes(devlist_hdr[8..12].try_into().unwrap());
    assert_eq!(ndev, 3);
    let mut devlist_body = vec![0u8; (312 + 4) * 3];
    tool.read_exact(&mut devlist_body).await.unwrap();

    // Import Device 1-1.2 (Keyboard)
    let mut import = Vec::new();
    import.extend_from_slice(&USBIP_VERSION.to_be_bytes());
    import.extend_from_slice(&OP_REQ_IMPORT.to_be_bytes());
    import.extend_from_slice(&0u32.to_be_bytes());
    let mut busid = [0u8; 32];
    busid[..5].copy_from_slice(b"1-1.2");
    import.extend_from_slice(&busid);
    tool.write_all(&import).await.unwrap();

    let mut import_hdr = [0u8; 8];
    tool.read_exact(&mut import_hdr).await.unwrap();
    assert_eq!(
        u16::from_be_bytes([import_hdr[2], import_hdr[3]]),
        OP_REP_IMPORT
    );
    assert_eq!(u32::from_be_bytes(import_hdr[4..8].try_into().unwrap()), 0);
    let mut udev = [0u8; 312];
    tool.read_exact(&mut udev).await.unwrap();

    // Send USB Descriptor request (Control transfer over TLS tunnel)
    let cmd = UsbipCmdSubmit {
        seqnum: 99,
        devid: 1,
        direction: 1,
        ep: 0,
        transfer_flags: 0,
        transfer_buffer_length: 18,
        start_frame: 0,
        number_of_packets: 0,
        interval: 0,
        setup: [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x12, 0x00],
    };
    tool.write_all(&cmd.encode()).await.unwrap();

    let mut ret_hdr = [0u8; 48];
    tool.read_exact(&mut ret_hdr).await.unwrap();
    let reply = UsbipRetSubmit::decode(&ret_hdr).unwrap();
    assert_eq!(reply.seqnum, 99);
    assert_eq!(reply.status, 0);
    assert_eq!(reply.actual_length, 18);
    let mut desc = vec![0u8; 18];
    tool.read_exact(&mut desc).await.unwrap();
    assert_eq!(desc[0], 18);
    assert_eq!(desc[1], 1); // Device Descriptor
    assert_eq!(desc[8], 0x6d); // Logitech VID
    assert_eq!(desc[9], 0x04);
}
