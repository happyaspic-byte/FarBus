//! Linux kernel compatible USB/IP 1.1 wire protocol structures.

use thiserror::Error;

pub const USBIP_VERSION: u16 = 0x0111;
pub const OP_REQ_DEVLIST: u16 = 0x8005;
pub const OP_REP_DEVLIST: u16 = 0x0005;
pub const OP_REQ_IMPORT: u16 = 0x8003;
pub const OP_REP_IMPORT: u16 = 0x0003;

pub const USBIP_CMD_SUBMIT: u32 = 0x0000_0001;
pub const USBIP_RET_SUBMIT: u32 = 0x0000_0003;
pub const USBIP_CMD_UNLINK: u32 = 0x0000_0002;
pub const USBIP_RET_UNLINK: u32 = 0x0000_0004;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbipDeviceHeader {
    pub path: [u8; 256],
    pub busid: [u8; 32],
    pub busnum: u32,
    pub devnum: u32,
    pub speed: u32,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub configuration_value: u8,
    pub num_configurations: u8,
    pub num_interfaces: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbipInterface {
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub padding: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbipCmdSubmit {
    pub seqnum: u32,
    pub devid: u32,
    pub direction: u32,
    pub ep: u32,
    pub transfer_flags: u32,
    pub transfer_buffer_length: u32,
    pub start_frame: u32,
    pub number_of_packets: u32,
    pub interval: u32,
    pub setup: [u8; 8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbipRetSubmit {
    pub seqnum: u32,
    pub devid: u32,
    pub direction: u32,
    pub ep: u32,
    pub status: i32,
    pub actual_length: u32,
    pub start_frame: u32,
    pub number_of_packets: u32,
    pub error_count: i32,
    pub setup: [u8; 8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbipCmdUnlink {
    pub seqnum: u32,
    pub devid: u32,
    pub direction: u32,
    pub ep: u32,
    pub unlink_seqnum: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbipRetUnlink {
    pub seqnum: u32,
    pub devid: u32,
    pub direction: u32,
    pub ep: u32,
    pub status: i32,
}

#[derive(Debug, Error)]
pub enum UsbipError {
    #[error("truncated packet")]
    Truncated,
    #[error("unsupported version {0:#06x}")]
    Version(u16),
}

fn read_u32_be(slice: &[u8], offset: usize) -> Result<u32, UsbipError> {
    if slice.len() < offset + 4 {
        return Err(UsbipError::Truncated);
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&slice[offset..offset + 4]);
    Ok(u32::from_be_bytes(buf))
}

fn read_i32_be(slice: &[u8], offset: usize) -> Result<i32, UsbipError> {
    if slice.len() < offset + 4 {
        return Err(UsbipError::Truncated);
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&slice[offset..offset + 4]);
    Ok(i32::from_be_bytes(buf))
}

impl UsbipCmdSubmit {
    /// Encodes a `USBIP_CMD_SUBMIT` header (48 bytes).
    #[must_use]
    pub fn encode(&self) -> [u8; 48] {
        let mut buf = [0u8; 48];
        buf[0..4].copy_from_slice(&USBIP_CMD_SUBMIT.to_be_bytes());
        buf[4..8].copy_from_slice(&self.seqnum.to_be_bytes());
        buf[8..12].copy_from_slice(&self.devid.to_be_bytes());
        buf[12..16].copy_from_slice(&self.direction.to_be_bytes());
        buf[16..20].copy_from_slice(&self.ep.to_be_bytes());
        buf[20..24].copy_from_slice(&self.transfer_flags.to_be_bytes());
        buf[24..28].copy_from_slice(&self.transfer_buffer_length.to_be_bytes());
        buf[28..32].copy_from_slice(&self.start_frame.to_be_bytes());
        buf[32..36].copy_from_slice(&self.number_of_packets.to_be_bytes());
        buf[36..40].copy_from_slice(&self.interval.to_be_bytes());
        buf[40..48].copy_from_slice(&self.setup);
        buf
    }

    /// Decodes a `USBIP_CMD_SUBMIT` header.
    ///
    /// # Errors
    ///
    /// Returns [`UsbipError::Truncated`] if slice is smaller than 48 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, UsbipError> {
        if bytes.len() < 48 {
            return Err(UsbipError::Truncated);
        }
        let mut setup = [0u8; 8];
        setup.copy_from_slice(&bytes[40..48]);
        Ok(Self {
            seqnum: read_u32_be(bytes, 4)?,
            devid: read_u32_be(bytes, 8)?,
            direction: read_u32_be(bytes, 12)?,
            ep: read_u32_be(bytes, 16)?,
            transfer_flags: read_u32_be(bytes, 20)?,
            transfer_buffer_length: read_u32_be(bytes, 24)?,
            start_frame: read_u32_be(bytes, 28)?,
            number_of_packets: read_u32_be(bytes, 32)?,
            interval: read_u32_be(bytes, 36)?,
            setup,
        })
    }
}

impl UsbipRetSubmit {
    /// Encodes a `USBIP_RET_SUBMIT` header (48 bytes).
    #[must_use]
    pub fn encode(&self) -> [u8; 48] {
        let mut buf = [0u8; 48];
        buf[0..4].copy_from_slice(&USBIP_RET_SUBMIT.to_be_bytes());
        buf[4..8].copy_from_slice(&self.seqnum.to_be_bytes());
        buf[8..12].copy_from_slice(&self.devid.to_be_bytes());
        buf[12..16].copy_from_slice(&self.direction.to_be_bytes());
        buf[16..20].copy_from_slice(&self.ep.to_be_bytes());
        buf[20..24].copy_from_slice(&self.status.to_be_bytes());
        buf[24..28].copy_from_slice(&self.actual_length.to_be_bytes());
        buf[28..32].copy_from_slice(&self.start_frame.to_be_bytes());
        buf[32..36].copy_from_slice(&self.number_of_packets.to_be_bytes());
        buf[36..40].copy_from_slice(&self.error_count.to_be_bytes());
        buf[40..48].copy_from_slice(&self.setup);
        buf
    }

    /// Decodes a `USBIP_RET_SUBMIT` header.
    ///
    /// # Errors
    ///
    /// Returns [`UsbipError::Truncated`] if slice is smaller than 48 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, UsbipError> {
        if bytes.len() < 48 {
            return Err(UsbipError::Truncated);
        }
        let mut setup = [0u8; 8];
        setup.copy_from_slice(&bytes[40..48]);
        Ok(Self {
            seqnum: read_u32_be(bytes, 4)?,
            devid: read_u32_be(bytes, 8)?,
            direction: read_u32_be(bytes, 12)?,
            ep: read_u32_be(bytes, 16)?,
            status: read_i32_be(bytes, 20)?,
            actual_length: read_u32_be(bytes, 24)?,
            start_frame: read_u32_be(bytes, 28)?,
            number_of_packets: read_u32_be(bytes, 32)?,
            error_count: read_i32_be(bytes, 36)?,
            setup,
        })
    }
}

impl UsbipCmdUnlink {
    /// Decodes a `USBIP_CMD_UNLINK` header (48 bytes).
    ///
    /// # Errors
    ///
    /// Returns [`UsbipError::Truncated`] if slice is smaller than 48 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, UsbipError> {
        if bytes.len() < 48 {
            return Err(UsbipError::Truncated);
        }
        Ok(Self {
            seqnum: read_u32_be(bytes, 4)?,
            devid: read_u32_be(bytes, 8)?,
            direction: read_u32_be(bytes, 12)?,
            ep: read_u32_be(bytes, 16)?,
            unlink_seqnum: read_u32_be(bytes, 20)?,
        })
    }
}

impl UsbipRetUnlink {
    /// Encodes a `USBIP_RET_UNLINK` header (48 bytes).
    #[must_use]
    pub fn encode(&self) -> [u8; 48] {
        let mut buf = [0u8; 48];
        buf[0..4].copy_from_slice(&USBIP_RET_UNLINK.to_be_bytes());
        buf[4..8].copy_from_slice(&self.seqnum.to_be_bytes());
        buf[8..12].copy_from_slice(&self.devid.to_be_bytes());
        buf[12..16].copy_from_slice(&self.direction.to_be_bytes());
        buf[16..20].copy_from_slice(&self.ep.to_be_bytes());
        buf[20..24].copy_from_slice(&self.status.to_be_bytes());
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_usbip_cmd_and_ret_submit() {
        let cmd = UsbipCmdSubmit {
            seqnum: 1234,
            devid: 2,
            direction: 1,
            ep: 0x81,
            transfer_flags: 0,
            transfer_buffer_length: 512,
            start_frame: 0,
            number_of_packets: 0,
            interval: 1,
            setup: [0x80, 0x06, 0, 1, 0, 0, 18, 0],
        };
        let encoded = cmd.encode();
        let decoded = UsbipCmdSubmit::decode(&encoded).unwrap();
        assert_eq!(cmd, decoded);

        let ret = UsbipRetSubmit {
            seqnum: 1234,
            devid: 2,
            direction: 1,
            ep: 0x81,
            status: 0,
            actual_length: 18,
            start_frame: 0,
            number_of_packets: 0,
            error_count: 0,
            setup: [0; 8],
        };
        let encoded_ret = ret.encode();
        let decoded_ret = UsbipRetSubmit::decode(&encoded_ret).unwrap();
        assert_eq!(ret, decoded_ret);

        let unlink = UsbipCmdUnlink {
            seqnum: 55,
            devid: 2,
            direction: 0,
            ep: 1,
            unlink_seqnum: 54,
        };
        let mut raw_unlink = [0u8; 48];
        raw_unlink[0..4].copy_from_slice(&USBIP_CMD_UNLINK.to_be_bytes());
        raw_unlink[4..8].copy_from_slice(&55u32.to_be_bytes());
        raw_unlink[8..12].copy_from_slice(&2u32.to_be_bytes());
        raw_unlink[12..16].copy_from_slice(&0u32.to_be_bytes());
        raw_unlink[16..20].copy_from_slice(&1u32.to_be_bytes());
        raw_unlink[20..24].copy_from_slice(&54u32.to_be_bytes());
        assert_eq!(UsbipCmdUnlink::decode(&raw_unlink).unwrap(), unlink);

        let ret_unlink = UsbipRetUnlink {
            seqnum: 55,
            devid: 2,
            direction: 0,
            ep: 1,
            status: 0,
        };
        assert_eq!(
            u32::from_be_bytes(ret_unlink.encode()[0..4].try_into().unwrap()),
            USBIP_RET_UNLINK
        );
    }
}
