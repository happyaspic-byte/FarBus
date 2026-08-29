use farbus_protocol::{DeviceId, TransferType, UrbComplete, UrbSubmit};

/// Completes a submitted URB against the in-process USB emulator.
#[must_use]
pub fn complete_urb(submit: &UrbSubmit) -> UrbComplete {
    match submit.transfer {
        TransferType::Control => control_complete(submit),
        TransferType::Interrupt => interrupt_complete(submit),
        TransferType::Bulk => bulk_complete(submit),
        TransferType::Isochronous => UrbComplete {
            seq: submit.seq,
            status: -32,
            data: Vec::new(),
        },
    }
}

fn control_complete(submit: &UrbSubmit) -> UrbComplete {
    if submit.data.len() >= 8 && submit.data[1] == 0x06 {
        let descriptor = match submit.device_id {
            DeviceId(1) => hid_keyboard_descriptor(),
            DeviceId(2) => serial_descriptor(),
            _ => mass_storage_descriptor(),
        };
        return UrbComplete {
            seq: submit.seq,
            status: 0,
            data: descriptor,
        };
    }
    UrbComplete {
        seq: submit.seq,
        status: 0,
        data: Vec::new(),
    }
}

fn interrupt_complete(submit: &UrbSubmit) -> UrbComplete {
    if submit.endpoint & 0x80 != 0 {
        UrbComplete {
            seq: submit.seq,
            status: 0,
            data: vec![0x00, b'A', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        }
    } else {
        UrbComplete {
            seq: submit.seq,
            status: 0,
            data: Vec::new(),
        }
    }
}

fn bulk_complete(submit: &UrbSubmit) -> UrbComplete {
    if submit.endpoint & 0x80 != 0 {
        let requested = usize::try_from(submit.requested_length.clamp(1, 65_536)).unwrap_or(1);
        let mut data = vec![0u8; requested];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = u8::try_from(i % 251).unwrap_or(0);
        }
        UrbComplete {
            seq: submit.seq,
            status: 0,
            data,
        }
    } else {
        UrbComplete {
            seq: submit.seq,
            status: 0,
            data: Vec::new(),
        }
    }
}

fn hid_keyboard_descriptor() -> Vec<u8> {
    vec![
        18, 1, 0x00, 0x02, 0, 0, 0, 8, 0x6d, 0x04, 0x1c, 0xc3, 0x00, 0x01, 1, 2, 0, 1,
    ]
}

fn serial_descriptor() -> Vec<u8> {
    vec![
        18, 1, 0x00, 0x02, 0xff, 0, 0, 8, 0x03, 0x04, 0x01, 0x60, 0x00, 0x06, 1, 2, 0, 1,
    ]
}

fn mass_storage_descriptor() -> Vec<u8> {
    vec![
        18, 1, 0x00, 0x02, 8, 6, 80, 64, 0x81, 0x07, 0x67, 0x55, 0x00, 0x01, 1, 2, 0, 1,
    ]
}
