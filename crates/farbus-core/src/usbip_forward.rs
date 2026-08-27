use crate::client::FarBusClient;
use crate::usb::LocalDevice;
use crate::usbip_proxy::encode_device_header;
use farbus_protocol::usbip::{
    UsbipCmdSubmit, UsbipRetSubmit, OP_REP_DEVLIST, OP_REP_IMPORT, OP_REQ_DEVLIST, OP_REQ_IMPORT,
    USBIP_VERSION,
};
use farbus_protocol::{DeviceId, TransferType, UrbSubmit};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

/// Serves USB/IP 1.1 on loopback and forwards URBs over an authenticated `FarBus` session.
///
/// # Errors
///
/// Returns bind or I/O errors.
pub async fn serve_usbip_forward(
    listen: &str,
    devices: Vec<LocalDevice>,
    client: Arc<Mutex<FarBusClient>>,
) -> std::io::Result<()> {
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
                    reply.extend_from_slice(&[device.info.usb_class, 0, 0, 0]);
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
                    reply.extend_from_slice(&0u32.to_be_bytes());
                    reply.extend_from_slice(&encode_device_header(&device));
                    stream.write_all(&reply).await?;
                    forward_urbs(&mut stream, device.info.id, &client).await?;
                } else {
                    reply.extend_from_slice(&4u32.to_be_bytes());
                    stream.write_all(&reply).await?;
                }
            }
            _ => break,
        }
    }
    Ok(())
}

async fn forward_urbs(
    stream: &mut TcpStream,
    device_id: DeviceId,
    client: &Arc<Mutex<FarBusClient>>,
) -> std::io::Result<()> {
    loop {
        let mut header = [0u8; 48];
        if stream.read_exact(&mut header).await.is_err() {
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
        let mut data = Vec::new();
        if cmd.direction == 0 && cmd.transfer_buffer_length > 0 {
            data.resize(cmd.transfer_buffer_length as usize, 0);
            stream.read_exact(&mut data).await?;
        } else if cmd.ep == 0 {
            data = cmd.setup.to_vec();
        }
        let complete = {
            let mut farbus = client.lock().await;
            farbus
                .urb(
                    device_id,
                    cmd.seqnum,
                    u8::try_from(cmd.ep).unwrap_or(0),
                    transfer,
                    data,
                )
                .await
                .map_err(|err| std::io::Error::other(err.to_string()))?
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
        stream.write_all(&ret.encode()).await?;
        if cmd.direction == 1 && !complete.data.is_empty() {
            stream.write_all(&complete.data).await?;
        }
    }
    Ok(())
}

// Keep unused import warning-free if UrbSubmit is not needed here.
#[allow(dead_code)]
fn _urb_ty(_: UrbSubmit) {}
