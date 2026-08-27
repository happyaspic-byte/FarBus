use crate::urb::complete_urb;
use crate::usb::LocalDevice;
use farbus_protocol::usbip::{
    UsbipCmdSubmit, UsbipRetSubmit, OP_REP_DEVLIST, OP_REP_IMPORT, OP_REQ_DEVLIST, OP_REQ_IMPORT,
    USBIP_CMD_SUBMIT, USBIP_CMD_UNLINK, USBIP_RET_UNLINK, USBIP_VERSION,
};
use farbus_protocol::{DeviceId, TransferType, UrbSubmit};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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
    out.push(1);
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
                let found = devices.iter().find(|d| d.info.bus_id == requested);
                let mut reply = Vec::new();
                reply.extend_from_slice(&USBIP_VERSION.to_be_bytes());
                reply.extend_from_slice(&OP_REP_IMPORT.to_be_bytes());
                if let Some(device) = found {
                    reply.extend_from_slice(&0u32.to_be_bytes());
                    reply.extend_from_slice(&encode_device_header(device));
                    stream.write_all(&reply).await?;
                    serve_urbs(&mut stream, device.info.id).await?;
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

async fn serve_urbs(stream: &mut TcpStream, device_id: DeviceId) -> std::io::Result<()> {
    loop {
        let mut header = [0u8; 48];
        if stream.read_exact(&mut header).await.is_err() {
            break;
        }
        let command = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if command == USBIP_CMD_UNLINK {
            header[0..4].copy_from_slice(&USBIP_RET_UNLINK.to_be_bytes());
            stream.write_all(&header).await?;
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
            stream.read_exact(&mut data).await?;
        } else if cmd.ep == 0 {
            data = cmd.setup.to_vec();
        } else {
            data.resize(cmd.transfer_buffer_length as usize, 0);
        }
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
        stream.write_all(&ret.encode()).await?;
        if cmd.direction == 1 && !complete.data.is_empty() {
            stream.write_all(&complete.data).await?;
        }
    }
    Ok(())
}

/// Starts a loopback USB/IP 1.1 server for Windows/Linux VHCI clients.
///
/// # Errors
///
/// Returns bind errors when port 3240 is already in use.
pub async fn serve_usbip_loopback(devices: Vec<LocalDevice>, addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let devices = devices.clone();
        tokio::spawn(async move {
            let _ = handle_client(stream, devices).await;
        });
    }
}
