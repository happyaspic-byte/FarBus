use crate::client::FarBusClient;
use crate::usb::LocalDevice;
use crate::usbip_proxy::encode_device_header;
use farbus_protocol::usbip::{
    UsbipCmdSubmit, UsbipCmdUnlink, UsbipRetSubmit, UsbipRetUnlink, OP_REP_DEVLIST, OP_REP_IMPORT,
    OP_REQ_DEVLIST, OP_REQ_IMPORT, USBIP_CMD_SUBMIT, USBIP_CMD_UNLINK, USBIP_VERSION,
};
use farbus_protocol::{DeviceId, TransferType};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

fn ensure_loopback(listen: &str) -> std::io::Result<()> {
    let addr: std::net::SocketAddr = listen
        .parse()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid address"))?;
    if !addr.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "USB/IP listener must use a loopback address",
        ));
    }
    Ok(())
}

/// Serves USB/IP 1.1 on loopback and forwards URBs concurrently over an authenticated `FarBus` session.
///
/// # Errors
///
/// Returns bind or I/O errors.
pub async fn serve_usbip_forward(
    listen: &str,
    devices: Vec<LocalDevice>,
    client: Arc<Mutex<FarBusClient>>,
) -> std::io::Result<()> {
    ensure_loopback(listen)?;
    let listener = TcpListener::bind(listen).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let devices = devices.clone();
        let client = Arc::clone(&client);
        tokio::spawn(async move {
            let _ = handle_forward(stream, devices, client).await;
        });
    }
}

/// Handles a single forwarding connection.
///
/// # Errors
///
/// Returns I/O errors when the peer disconnects.
pub async fn handle_forward_for_test(
    stream: TcpStream,
    devices: Vec<LocalDevice>,
    client: Arc<Mutex<FarBusClient>>,
) -> std::io::Result<()> {
    handle_forward(stream, devices, client).await
}

async fn handle_forward(
    mut stream: TcpStream,
    devices: Vec<LocalDevice>,
    client: Arc<Mutex<FarBusClient>>,
) -> std::io::Result<()> {
    let devices: Vec<_> = devices.into_iter().filter(|d| d.info.exported).collect();
    loop {
        let mut header = [0u8; 8];
        if stream.read_exact(&mut header).await.is_err() {
            break;
        }
        let version = u16::from_be_bytes([header[0], header[1]]);
        let command = u16::from_be_bytes([header[2], header[3]]);
        if version != USBIP_VERSION {
            break;
        }
        match command {
            OP_REQ_DEVLIST => {
                let mut reply = Vec::new();
                reply.extend_from_slice(&USBIP_VERSION.to_be_bytes());
                reply.extend_from_slice(&OP_REP_DEVLIST.to_be_bytes());
                reply.extend_from_slice(&0u32.to_be_bytes());
                reply.extend_from_slice(&u32::try_from(devices.len()).unwrap_or(0).to_be_bytes());
                for device in &devices {
                    reply.extend_from_slice(&encode_device_header(device));
                    if device.info.interfaces.is_empty() {
                        reply.extend_from_slice(&[device.info.usb_class, 0, 0, 0]);
                    } else {
                        for iface in &device.info.interfaces {
                            reply.extend_from_slice(&[
                                iface.interface_class,
                                iface.interface_subclass,
                                iface.interface_protocol,
                                0,
                            ]);
                        }
                    }
                }
                stream.write_all(&reply).await?;
            }
            OP_REQ_IMPORT => {
                let mut busid = [0u8; 32];
                stream.read_exact(&mut busid).await?;
                let requested = std::str::from_utf8(&busid)
                    .unwrap_or("")
                    .trim_end_matches('\0')
                    .to_string();
                let found = devices.iter().find(|d| d.info.bus_id == requested).cloned();
                let mut reply = Vec::new();
                reply.extend_from_slice(&USBIP_VERSION.to_be_bytes());
                reply.extend_from_slice(&OP_REP_IMPORT.to_be_bytes());
                if let Some(device) = found {
                    {
                        let farbus = client.lock().await;
                        if farbus.attach(device.info.id).await.is_err() {
                            reply.extend_from_slice(&2u32.to_be_bytes());
                            stream.write_all(&reply).await?;
                            continue;
                        }
                    }
                    reply.extend_from_slice(&0u32.to_be_bytes());
                    reply.extend_from_slice(&encode_device_header(&device));
                    stream.write_all(&reply).await?;
                    let result = forward_urbs(stream, device.info.id, &client).await;
                    let farbus = client.lock().await;
                    let _ = farbus.detach(device.info.id).await;
                    return result;
                }
                reply.extend_from_slice(&4u32.to_be_bytes());
                stream.write_all(&reply).await?;
            }
            _ => break,
        }
    }
    Ok(())
}

enum OutgoingUsbip {
    Ret(Vec<u8>),
}

#[allow(clippy::too_many_lines)]
async fn forward_urbs(
    stream: TcpStream,
    device_id: DeviceId,
    client: &Arc<Mutex<FarBusClient>>,
) -> std::io::Result<()> {
    let (mut reader_half, mut writer_half) = tokio::io::split(stream);
    let (out_tx, mut out_rx) = mpsc::channel::<OutgoingUsbip>(256);

    let writer_task = tokio::spawn(async move {
        while let Some(OutgoingUsbip::Ret(bytes)) = out_rx.recv().await {
            if writer_half.write_all(&bytes).await.is_err() {
                break;
            }
        }
    });

    loop {
        let mut header = [0u8; 48];
        if reader_half.read_exact(&mut header).await.is_err() {
            break;
        }
        let command = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if command == USBIP_CMD_UNLINK {
            if let Ok(cmd) = UsbipCmdUnlink::decode(&header) {
                let client = Arc::clone(client);
                let out_tx = out_tx.clone();
                tokio::spawn(async move {
                    let status = {
                        let farbus = client.lock().await;
                        farbus
                            .unlink(device_id, cmd.unlink_seqnum)
                            .await
                            .unwrap_or(0)
                    };
                    let ret = UsbipRetUnlink {
                        seqnum: cmd.seqnum,
                        devid: cmd.devid,
                        direction: cmd.direction,
                        ep: cmd.ep,
                        status,
                    };
                    let _ = out_tx.send(OutgoingUsbip::Ret(ret.encode().to_vec())).await;
                });
            }
            continue;
        }
        if command != USBIP_CMD_SUBMIT {
            break;
        }
        let Ok(cmd) = UsbipCmdSubmit::decode(&header) else {
            break;
        };
        let transfer = match cmd.ep {
            0 => TransferType::Control,
            ep if ep & 0x80 != 0 && cmd.transfer_buffer_length <= 64 => TransferType::Interrupt,
            _ => TransferType::Bulk,
        };
        if cmd.transfer_buffer_length as usize > 65_536 {
            break;
        }
        let mut data = Vec::new();
        if cmd.direction == 0 && cmd.transfer_buffer_length > 0 {
            data.resize(cmd.transfer_buffer_length as usize, 0);
            if reader_half.read_exact(&mut data).await.is_err() {
                break;
            }
        } else if cmd.ep == 0 {
            data = cmd.setup.to_vec();
        } else {
            data.resize(cmd.transfer_buffer_length as usize, 0);
        }

        let client = Arc::clone(client);
        let out_tx = out_tx.clone();
        tokio::spawn(async move {
            let complete = {
                let farbus = client.lock().await.clone();
                let first = farbus
                    .urb(
                        device_id,
                        cmd.seqnum,
                        u8::try_from(cmd.ep).unwrap_or(0),
                        transfer,
                        data.clone(),
                    )
                    .await;
                if let Ok(complete) = first {
                    complete
                } else {
                    let mut lock = client.lock().await;
                    if lock.reconnect().await.is_err() || lock.attach(device_id).await.is_err() {
                        return;
                    }
                    let reconnected = lock.clone();
                    drop(lock);
                    match reconnected
                        .urb(
                            device_id,
                            cmd.seqnum,
                            u8::try_from(cmd.ep).unwrap_or(0),
                            transfer,
                            data,
                        )
                        .await
                    {
                        Ok(comp) => comp,
                        Err(_) => return,
                    }
                }
            };
            let ret = UsbipRetSubmit {
                seqnum: cmd.seqnum,
                devid: cmd.devid,
                direction: cmd.direction,
                ep: cmd.ep,
                status: complete.status,
                actual_length: u32::try_from(complete.data.len()).unwrap_or(0),
                start_frame: 0,
                number_of_packets: 0,
                error_count: 0,
                setup: [0; 8],
            };
            let mut payload = ret.encode().to_vec();
            if cmd.direction == 1 && !complete.data.is_empty() {
                payload.extend_from_slice(&complete.data);
            }
            let _ = out_tx.send(OutgoingUsbip::Ret(payload)).await;
        });
    }

    drop(out_tx);
    let _ = writer_task.await;
    Ok(())
}
