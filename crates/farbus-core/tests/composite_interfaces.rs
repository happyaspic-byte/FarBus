use farbus_core::{encode_device_header, simulated_lab_devices};

#[test]
fn usbip_device_header_reports_real_interface_count() {
    let keyboard = &simulated_lab_devices()[0];
    let header = encode_device_header(keyboard);
    assert_eq!(header.len(), 312);
    assert_eq!(header[311], 1);
}

#[test]
fn simulated_mass_storage_exposes_bulk_endpoints() {
    let disk = &simulated_lab_devices()[2];
    assert_eq!(disk.info.interfaces.len(), 1);
    assert_eq!(disk.info.interfaces[0].interface_class, 8);
    assert!(disk.info.interfaces[0].endpoints.contains(&0x81));
    assert!(disk.info.interfaces[0].endpoints.contains(&0x02));
}
