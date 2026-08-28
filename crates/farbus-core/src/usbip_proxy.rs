use crate::urb::complete_urb;
use crate::usb::LocalDevice;
use farbus_protocol::usbip::{
    UsbipCmdSubmit, UsbipCmdUnlink, UsbipRetSubmit, UsbipRetUnlink, OP_REP_DEVLIST, OP_REP_IMPORT,
    OP_REQ_DEVLIST, OP_REQ_IMPORT, USBIP_CMD_SUBMIT, USBIP_CMD_UNLINK, USBIP_VERSION,
};
use farbus_protocol::{DeviceId, TransferType, UrbSubmit};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

fn padded(src: &str, n: usize) -> Vec<u8> {
    let mut out = vec![0u8; n];
    let bytes = src.as_bytes();
    let copy = bytes.len().min(n.saturating_sub(1));
    out[..copy].copy_from_slice(&bytes[..copy]);
    out
}

/// Encodes a 312-byte `usbip_usb_device` structure.
#[must_use]
pub fn encode_device_header(device: &LocalDevice) -> Vec<u8> {
    let mut out = Vec::with_capacity(312);
    out.extend_from_slice(&padded(
        &format!("/sys/bus/usb/devices/{}", device.info.bus_id),
        256,
    ));
    out.extend_from_slice(&padded(&device.info.bus_id, 32));
    let busnum = device
        .info
        .bus_id
        .split('-')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    out.extend_from_slice(&busnum.to_be_bytes());
    out.extend_from_slice(&device.info.id.0.to_be_bytes());
    let speed = match device.info.speed {
        farbus_protocol::UsbSpeed::Low => 1u32,
        farbus_protocol::UsbSpeed::Full => 2,
        farbus_protocol::UsbSpeed::High => 3,
        farbus_protocol::UsbSpeed::Super => 5,
    };
    out.extend_from_slice(&speed.to_be_bytes());
    out.extend_from_slice(&device.info.vid.to_be_bytes());
    out.extend_from_slice(&device.info.pid.to_be_bytes());
    out.extend_from_slice(&0x0100u16.to_be_bytes());
    out.push(device.info.usb_class);
    out.push(0);
    out.push(0);
    out.push(1);
    out.push(1);
    let niface = u8::try_from(device.info.interfaces.len().max(1)).unwrap_or(1);
    out.push(niface);
    out
}

fn encode_interfaces(device: &LocalDevice) -> Vec<u8> {
    if device.info.interfaces.is_empty() {
        return vec![device.info.usb_class, 0, 0, 0];
    }
    let mut out = Vec::with_capacity(device.info.interfaces.len() * 4);
    for iface in &device.info.interfaces {
        out.push(iface.interface_class);
        out.push(iface.interface_subclass);
        out.push(iface.interface_protocol);
        out.push(0);
    }
    out
}

/// Handles one USB/IP 1.1 client connection.
///
/// # Errors
///
/// Returns I/O errors when the peer disconnects mid-frame.
pub async fn handle_client(stream: TcpStream, devices: Vec<LocalDevice>) -> std::io::Result<()> {
    let devices: Vec<_> = devices.into_iter().filter(|d| d.info.exported).collect();
    handle_client_filtered(stream, devices).await
}

async fn handle_client_filtered(
    mut stream: TcpStream,
    devices: Vec<LocalDevice>,
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
                    reply.extend_from_slice(&encode_interfaces(device));
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
                let found = devices.iter().find(|d| d.info.bus_id == requested);
                let mut reply = Vec::new();
                reply.extend_from_slice(&USBIP_VERSION.to_be_bytes());
                reply.extend_from_slice(&OP_REP_IMPORT.to_be_bytes());
                if let Some(device) = found {
                    reply.extend_from_slice(&0u32.to_be_bytes());
                    reply.extend_from_slice(&encode_device_header(device));
                    stream.write_all(&reply).await?;
                    return serve_urbs(stream, device.info.id).await;
                }
                reply.extend_from_slice(&4u32.to_be_bytes());
                stream.write_all(&reply).await?;
            }
            _ => break,
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn serve_urbs(stream: TcpStream, device_id: DeviceId) -> std::io::Result<()> {
    let (mut reader_half, mut writer_half) = tokio::io::split(stream);
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(256);

    let writer_task = tokio::spawn(async move {
        while let Some(bytes) = out_rx.recv().await {
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
                let ret = UsbipRetUnlink {
                    seqnum: cmd.seqnum,
                    devid: cmd.devid,
                    direction: cmd.direction,
                    ep: cmd.ep,
                    status: 0,
                };
                let _ = out_tx.send(ret.encode().to_vec()).await;
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
        let mut out_payload = Vec::new();
        if cmd.direction == 0 && cmd.transfer_buffer_length > 0 {
            out_payload.resize(cmd.transfer_buffer_length as usize, 0);
            if reader_half.read_exact(&mut out_payload).await.is_err() {
                break;
            }
        }
        let data = farbus_protocol::usbip::urb_submit_data(
            cmd.ep,
            cmd.direction,
            cmd.setup,
            cmd.transfer_buffer_length,
            &out_payload,
        );

        let out_tx = out_tx.clone();
        tokio::spawn(async move {
            let complete = complete_urb(&UrbSubmit {
                seq: cmd.seqnum,
                device_id,
                endpoint: u8::try_from(cmd.ep).unwrap_or(0),
                transfer,
                data,
            });
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
            let _ = out_tx.send(payload).await;
        });
    }

    drop(out_tx);
    let _ = writer_task.await;
    Ok(())
}

/// Starts a loopback USB/IP 1.1 server for Windows/Linux VHCI clients.
///
/// # Errors
///
/// Returns bind errors when port 3240 is already in use.
pub async fn serve_usbip_loopback(devices: Vec<LocalDevice>, addr: &str) -> std::io::Result<()> {
    let listener = bind_loopback(addr).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let devices = devices.clone();
        tokio::spawn(async move {
            let _ = handle_client(stream, devices).await;
        });
    }
}

/// Starts a loopback USB/IP listener backed by the server's live hotplug inventory.
///
/// # Errors
///
/// Returns bind errors or rejects non-loopback addresses.
pub async fn serve_usbip_loopback_state(
    state: std::sync::Arc<crate::session::ServerState>,
    addr: &str,
) -> std::io::Result<()> {
    let listener = bind_loopback(addr).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let devices = state.devices_snapshot().await;
        tokio::spawn(async move {
            let _ = handle_client(stream, devices).await;
        });
    }
}

async fn bind_loopback(addr: &str) -> std::io::Result<TcpListener> {
    let listen: std::net::SocketAddr = addr
        .parse()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid address"))?;
    if !listen.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "USB/IP listener must use a loopback address",
        ));
    }
    TcpListener::bind(listen).await
}
