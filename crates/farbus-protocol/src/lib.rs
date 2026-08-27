//! `FarBus` control-plane framing. Fail closed on unknown versions and oversized frames.

pub mod usbip;

use thiserror::Error as ThisError;

pub const VERSION: u8 = 1;
pub const MAX_PAYLOAD: usize = 65_536;
const MAGIC: &[u8; 4] = b"FARB";
pub const HEADER_LEN: usize = 10;
const MAX_U8_STR: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbSpeed {
    Low,
    Full,
    High,
    Super,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Control,
    Interrupt,
    Bulk,
    Isochronous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Unauthorized,
    NotFound,
    Conflict,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub protocol_min: u8,
    pub protocol_max: u8,
    pub fingerprint: [u8; 32],
    pub hostname: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairRequest {
    pub client_fingerprint: [u8; 32],
    pub pin_hash: [u8; 32],
    pub client_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairResponse {
    pub success: bool,
    pub server_fingerprint: [u8; 32],
    pub auth_token: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachRequest {
    pub device_id: DeviceId,
    pub auth_token: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachResponse {
    pub device_id: DeviceId,
    pub success: bool,
    pub usbip_port: u16,
    pub bus_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrbSubmit {
    pub seq: u32,
    pub device_id: DeviceId,
    pub endpoint: u8,
    pub transfer: TransferType,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrbComplete {
    pub seq: u32,
    pub status: i32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub bus_id: String,
    pub vid: u16,
    pub pid: u16,
    pub usb_class: u8,
    pub speed: UsbSpeed,
    pub product: String,
    pub exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceList {
    pub devices: Vec<DeviceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Hello(Hello),
    Error { code: ErrorCode, detail: String },
    Detach { device_id: DeviceId },
    DeviceList(DeviceList),
    PairRequest(PairRequest),
    PairResponse(PairResponse),
    AttachRequest(AttachRequest),
    AttachResponse(AttachResponse),
    UrbSubmit(UrbSubmit),
    UrbComplete(UrbComplete),
}

impl Message {
    pub const HELLO_TYPE: u8 = 1;
    pub const ERROR_TYPE: u8 = 2;
    pub const DETACH_TYPE: u8 = 3;
    pub const DEVICE_LIST_TYPE: u8 = 4;
    pub const PAIR_REQUEST_TYPE: u8 = 5;
    pub const PAIR_RESPONSE_TYPE: u8 = 6;
    pub const ATTACH_REQUEST_TYPE: u8 = 7;
    pub const ATTACH_RESPONSE_TYPE: u8 = 8;
    pub const URB_SUBMIT_TYPE: u8 = 9;
    pub const URB_COMPLETE_TYPE: u8 = 10;

    fn ty(&self) -> u8 {
        match self {
            Self::Hello(_) => Self::HELLO_TYPE,
            Self::Error { .. } => Self::ERROR_TYPE,
            Self::Detach { .. } => Self::DETACH_TYPE,
            Self::DeviceList(_) => Self::DEVICE_LIST_TYPE,
            Self::PairRequest(_) => Self::PAIR_REQUEST_TYPE,
            Self::PairResponse(_) => Self::PAIR_RESPONSE_TYPE,
            Self::AttachRequest(_) => Self::ATTACH_REQUEST_TYPE,
            Self::AttachResponse(_) => Self::ATTACH_RESPONSE_TYPE,
            Self::UrbSubmit(_) => Self::URB_SUBMIT_TYPE,
            Self::UrbComplete(_) => Self::URB_COMPLETE_TYPE,
        }
    }
}

#[derive(Debug, ThisError, PartialEq, Eq)]
pub enum Error {
    #[error("truncated frame")]
    Truncated,
    #[error("bad magic")]
    BadMagic,
    #[error("unsupported version {0}")]
    UnsupportedVersion(u8),
    #[error("payload too large: {len} > {max}")]
    PayloadTooLarge { len: usize, max: usize },
    #[error("field {field} too long: {len} > {max}")]
    FieldTooLong {
        field: &'static str,
        len: usize,
        max: usize,
    },
    #[error("unknown message type {0}")]
    UnknownType(u8),
    #[error("invalid payload")]
    InvalidPayload,
}

/// Encodes one bounded control-plane message.
///
/// # Errors
///
/// Returns an error when a field or the complete payload exceeds its wire limit.
pub fn encode(msg: &Message) -> Result<Vec<u8>, Error> {
    let mut payload = Vec::new();
    match msg {
        Message::Hello(hello) => {
            payload.push(hello.protocol_min);
            payload.push(hello.protocol_max);
            payload.extend_from_slice(&hello.fingerprint);
            put_u8_str(&mut payload, "hostname", &hello.hostname)?;
        }
        Message::Error { code, detail } => {
            payload.extend_from_slice(&code_to_u16(*code).to_be_bytes());
            put_u16_str(&mut payload, "detail", detail)?;
        }
        Message::Detach { device_id } => {
            payload.extend_from_slice(&device_id.0.to_be_bytes());
        }
        Message::DeviceList(list) => {
            let count = u16::try_from(list.devices.len()).map_err(|_| Error::FieldTooLong {
                field: "devices",
                len: list.devices.len(),
                max: usize::from(u16::MAX),
            })?;
            payload.extend_from_slice(&count.to_be_bytes());
            for device in &list.devices {
                payload.extend_from_slice(&device.id.0.to_be_bytes());
                put_u8_str(&mut payload, "bus_id", &device.bus_id)?;
                payload.extend_from_slice(&device.vid.to_be_bytes());
                payload.extend_from_slice(&device.pid.to_be_bytes());
                payload.push(device.usb_class);
                payload.push(speed_to_u8(device.speed));
                put_u8_str(&mut payload, "product", &device.product)?;
                payload.push(u8::from(device.exported));
            }
        }
        Message::PairRequest(pair) => {
            payload.extend_from_slice(&pair.client_fingerprint);
            payload.extend_from_slice(&pair.pin_hash);
            put_u8_str(&mut payload, "client_name", &pair.client_name)?;
        }
        Message::PairResponse(pair) => {
            payload.push(u8::from(pair.success));
            payload.extend_from_slice(&pair.server_fingerprint);
            payload.extend_from_slice(&pair.auth_token);
        }
        Message::AttachRequest(attach) => {
            payload.extend_from_slice(&attach.device_id.0.to_be_bytes());
            payload.extend_from_slice(&attach.auth_token);
        }
        Message::AttachResponse(attach) => {
            payload.extend_from_slice(&attach.device_id.0.to_be_bytes());
            payload.push(u8::from(attach.success));
            payload.extend_from_slice(&attach.usbip_port.to_be_bytes());
            put_u8_str(&mut payload, "bus_id", &attach.bus_id)?;
        }
        Message::UrbSubmit(urb) => {
            payload.extend_from_slice(&urb.seq.to_be_bytes());
            payload.extend_from_slice(&urb.device_id.0.to_be_bytes());
            payload.push(urb.endpoint);
            payload.push(transfer_to_u8(urb.transfer));
            let data_len = u16::try_from(urb.data.len()).map_err(|_| Error::FieldTooLong {
                field: "urb.data",
                len: urb.data.len(),
                max: usize::from(u16::MAX),
            })?;
            payload.extend_from_slice(&data_len.to_be_bytes());
            payload.extend_from_slice(&urb.data);
        }
        Message::UrbComplete(urb) => {
            payload.extend_from_slice(&urb.seq.to_be_bytes());
            payload.extend_from_slice(&urb.status.to_be_bytes());
            let data_len = u16::try_from(urb.data.len()).map_err(|_| Error::FieldTooLong {
                field: "urb.data",
                len: urb.data.len(),
                max: usize::from(u16::MAX),
            })?;
            payload.extend_from_slice(&data_len.to_be_bytes());
            payload.extend_from_slice(&urb.data);
        }
    }
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::PayloadTooLarge {
            len: payload.len(),
            max: MAX_PAYLOAD,
        });
    }
    let len = u32::try_from(payload.len()).map_err(|_| Error::PayloadTooLarge {
        len: payload.len(),
        max: MAX_PAYLOAD,
    })?;
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(msg.ty());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Decodes one bounded control-plane message.
///
/// # Errors
///
/// Returns an error for truncated, oversized, malformed, or unsupported frames.
pub fn decode(bytes: &[u8]) -> Result<Message, Error> {
    if bytes.len() < HEADER_LEN {
        return Err(Error::Truncated);
    }
    if bytes[0..4] != MAGIC[..] {
        return Err(Error::BadMagic);
    }
    let version = bytes[4];
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let ty = bytes[5];
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&bytes[6..10]);
    let wire_len = u32::from_be_bytes(len_bytes);
    let len = usize::try_from(wire_len).map_err(|_| Error::PayloadTooLarge {
        len: usize::MAX,
        max: MAX_PAYLOAD,
    })?;
    if len > MAX_PAYLOAD {
        return Err(Error::PayloadTooLarge {
            len,
            max: MAX_PAYLOAD,
        });
    }
    if bytes.len() < HEADER_LEN + len {
        return Err(Error::Truncated);
    }
    parse_payload(ty, &bytes[HEADER_LEN..HEADER_LEN + len])
}

#[allow(clippy::too_many_lines)]
fn parse_payload(ty: u8, payload: &[u8]) -> Result<Message, Error> {
    let mut cur = Cursor::new(payload);
    match ty {
        Message::HELLO_TYPE => {
            let protocol_min = cur.u8()?;
            let protocol_max = cur.u8()?;
            let mut fingerprint = [0u8; 32];
            cur.read_exact(&mut fingerprint)?;
            let hostname = cur.u8_str()?;
            cur.finish()?;
            Ok(Message::Hello(Hello {
                protocol_min,
                protocol_max,
                fingerprint,
                hostname,
            }))
        }
        Message::ERROR_TYPE => {
            let code = u16_to_code(cur.u16()?)?;
            let detail = cur.u16_str()?;
            cur.finish()?;
            Ok(Message::Error { code, detail })
        }
        Message::DETACH_TYPE => {
            let device_id = DeviceId(cur.u32()?);
            cur.finish()?;
            Ok(Message::Detach { device_id })
        }
        Message::DEVICE_LIST_TYPE => {
            let count = usize::from(cur.u16()?);
            let mut devices = Vec::with_capacity(count);
            for _ in 0..count {
                devices.push(DeviceInfo {
                    id: DeviceId(cur.u32()?),
                    bus_id: cur.u8_str()?,
                    vid: cur.u16()?,
                    pid: cur.u16()?,
                    usb_class: cur.u8()?,
                    speed: u8_to_speed(cur.u8()?)?,
                    product: cur.u8_str()?,
                    exported: match cur.u8()? {
                        0 => false,
                        1 => true,
                        _ => return Err(Error::InvalidPayload),
                    },
                });
            }
            cur.finish()?;
            Ok(Message::DeviceList(DeviceList { devices }))
        }
        Message::PAIR_REQUEST_TYPE => {
            let mut client_fingerprint = [0u8; 32];
            cur.read_exact(&mut client_fingerprint)?;
            let mut pin_hash = [0u8; 32];
            cur.read_exact(&mut pin_hash)?;
            let client_name = cur.u8_str()?;
            cur.finish()?;
            Ok(Message::PairRequest(PairRequest {
                client_fingerprint,
                pin_hash,
                client_name,
            }))
        }
        Message::PAIR_RESPONSE_TYPE => {
            let success = match cur.u8()? {
                0 => false,
                1 => true,
                _ => return Err(Error::InvalidPayload),
            };
            let mut server_fingerprint = [0u8; 32];
            cur.read_exact(&mut server_fingerprint)?;
            let mut auth_token = [0u8; 32];
            cur.read_exact(&mut auth_token)?;
            cur.finish()?;
            Ok(Message::PairResponse(PairResponse {
                success,
                server_fingerprint,
                auth_token,
            }))
        }
        Message::ATTACH_REQUEST_TYPE => {
            let device_id = DeviceId(cur.u32()?);
            let mut auth_token = [0u8; 32];
            cur.read_exact(&mut auth_token)?;
            cur.finish()?;
            Ok(Message::AttachRequest(AttachRequest {
                device_id,
                auth_token,
            }))
        }
        Message::ATTACH_RESPONSE_TYPE => {
            let device_id = DeviceId(cur.u32()?);
            let success = match cur.u8()? {
                0 => false,
                1 => true,
                _ => return Err(Error::InvalidPayload),
            };
            let usbip_port = cur.u16()?;
            let bus_id = cur.u8_str()?;
            cur.finish()?;
            Ok(Message::AttachResponse(AttachResponse {
                device_id,
                success,
                usbip_port,
                bus_id,
            }))
        }
        Message::URB_SUBMIT_TYPE => {
            let seq = cur.u32()?;
            let device_id = DeviceId(cur.u32()?);
            let endpoint = cur.u8()?;
            let transfer = u8_to_transfer(cur.u8()?)?;
            let data_len = usize::from(cur.u16()?);
            let data = cur.take(data_len)?.to_vec();
            cur.finish()?;
            Ok(Message::UrbSubmit(UrbSubmit {
                seq,
                device_id,
                endpoint,
                transfer,
                data,
            }))
        }
        Message::URB_COMPLETE_TYPE => {
            let seq = cur.u32()?;
            let mut status_buf = [0u8; 4];
            cur.read_exact(&mut status_buf)?;
            let status = i32::from_be_bytes(status_buf);
            let data_len = usize::from(cur.u16()?);
            let data = cur.take(data_len)?.to_vec();
            cur.finish()?;
            Ok(Message::UrbComplete(UrbComplete { seq, status, data }))
        }
        other => Err(Error::UnknownType(other)),
    }
}

struct Cursor<'a> {
    rest: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn new(rest: &'a [u8]) -> Self {
        Self { rest }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.rest.len() < n {
            return Err(Error::Truncated);
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, Error> {
        let mut buf = [0u8; 2];
        buf.copy_from_slice(self.take(2)?);
        Ok(u16::from_be_bytes(buf))
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(buf))
    }

    fn read_exact(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        dest.copy_from_slice(self.take(dest.len())?);
        Ok(())
    }

    fn u8_str(&mut self) -> Result<String, Error> {
        let len = usize::from(self.u8()?);
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidPayload)
    }

    fn u16_str(&mut self) -> Result<String, Error> {
        let len = usize::from(self.u16()?);
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidPayload)
    }

    fn finish(self) -> Result<(), Error> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(Error::InvalidPayload)
        }
    }
}

fn put_u8_str(buf: &mut Vec<u8>, field: &'static str, value: &str) -> Result<(), Error> {
    let len = u8::try_from(value.len()).map_err(|_| Error::FieldTooLong {
        field,
        len: value.len(),
        max: MAX_U8_STR,
    })?;
    buf.push(len);
    buf.extend_from_slice(value.as_bytes());
    Ok(())
}

fn put_u16_str(buf: &mut Vec<u8>, field: &'static str, value: &str) -> Result<(), Error> {
    let len = u16::try_from(value.len()).map_err(|_| Error::FieldTooLong {
        field,
        len: value.len(),
        max: usize::from(u16::MAX),
    })?;
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(value.as_bytes());
    Ok(())
}

fn speed_to_u8(speed: UsbSpeed) -> u8 {
    match speed {
        UsbSpeed::Low => 1,
        UsbSpeed::Full => 2,
        UsbSpeed::High => 3,
        UsbSpeed::Super => 4,
    }
}

fn u8_to_speed(value: u8) -> Result<UsbSpeed, Error> {
    match value {
        1 => Ok(UsbSpeed::Low),
        2 => Ok(UsbSpeed::Full),
        3 => Ok(UsbSpeed::High),
        4 => Ok(UsbSpeed::Super),
        _ => Err(Error::InvalidPayload),
    }
}

fn transfer_to_u8(t: TransferType) -> u8 {
    match t {
        TransferType::Control => 1,
        TransferType::Interrupt => 2,
        TransferType::Bulk => 3,
        TransferType::Isochronous => 4,
    }
}

fn u8_to_transfer(value: u8) -> Result<TransferType, Error> {
    match value {
        1 => Ok(TransferType::Control),
        2 => Ok(TransferType::Interrupt),
        3 => Ok(TransferType::Bulk),
        4 => Ok(TransferType::Isochronous),
        _ => Err(Error::InvalidPayload),
    }
}

fn code_to_u16(code: ErrorCode) -> u16 {
    match code {
        ErrorCode::Unauthorized => 1,
        ErrorCode::NotFound => 2,
        ErrorCode::Conflict => 3,
        ErrorCode::Unsupported => 4,
    }
}

fn u16_to_code(value: u16) -> Result<ErrorCode, Error> {
    match value {
        1 => Ok(ErrorCode::Unauthorized),
        2 => Ok(ErrorCode::NotFound),
        3 => Ok(ErrorCode::Conflict),
        4 => Ok(ErrorCode::Unsupported),
        _ => Err(Error::InvalidPayload),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_hello_frame() {
        let msg = Message::Hello(Hello {
            protocol_min: 1,
            protocol_max: 1,
            fingerprint: [0xAB; 32],
            hostname: "lab-pi".into(),
        });
        let bytes = encode(&msg).expect("encode");
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn rejects_truncated_header() {
        let err = decode(b"FAR").unwrap_err();
        assert!(matches!(err, Error::Truncated));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = encode(&Message::Error {
            code: ErrorCode::Unauthorized,
            detail: "no".into(),
        })
        .unwrap();
        bytes[0] = b'X';
        let err = decode(&bytes).unwrap_err();
        assert!(matches!(err, Error::BadMagic));
    }

    #[test]
    fn rejects_unknown_version() {
        let mut bytes = encode(&Message::Detach {
            device_id: DeviceId(7),
        })
        .unwrap();
        bytes[4] = 99;
        let err = decode(&bytes).unwrap_err();
        assert!(matches!(err, Error::UnsupportedVersion(99)));
    }

    #[test]
    fn rejects_oversized_payload_length() {
        let mut header = Vec::from(*b"FARB");
        header.push(1);
        header.push(Message::ERROR_TYPE);
        let oversized = u32::try_from(MAX_PAYLOAD + 1).expect("payload limit fits u32");
        header.extend_from_slice(&oversized.to_be_bytes());
        header.extend_from_slice(&[0; 8]);
        let err = decode(&header).unwrap_err();
        assert!(matches!(err, Error::PayloadTooLarge { .. }));
    }

    #[test]
    fn rejects_hostname_longer_than_255() {
        let msg = Message::Hello(Hello {
            protocol_min: 1,
            protocol_max: 1,
            fingerprint: [1; 32],
            hostname: "n".repeat(256),
        });
        let err = encode(&msg).unwrap_err();
        assert!(matches!(err, Error::FieldTooLong { .. }));
    }

    #[test]
    fn roundtrips_device_list() {
        let msg = Message::DeviceList(DeviceList {
            devices: vec![
                DeviceInfo {
                    id: DeviceId(1),
                    bus_id: "1-1.2".into(),
                    vid: 0x046d,
                    pid: 0xc52b,
                    usb_class: 3,
                    speed: UsbSpeed::High,
                    product: "Unifying Receiver".into(),
                    exported: true,
                },
                DeviceInfo {
                    id: DeviceId(2),
                    bus_id: "1-2".into(),
                    vid: 0x0403,
                    pid: 0x6001,
                    usb_class: 255,
                    speed: UsbSpeed::Full,
                    product: "FT232".into(),
                    exported: false,
                },
            ],
        });
        let decoded = decode(&encode(&msg).unwrap()).unwrap();
        assert_eq!(msg, decoded);
    }
}
