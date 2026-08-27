//! Shared `FarBus` state machines, identity, and transport. Implementation follows tests.

pub mod client;
pub mod discovery;
pub mod fingerprint;
pub mod frame;
pub mod happy;
#[cfg(target_os = "linux")]
pub mod host_usb;
pub mod identity;
pub mod lease;
pub mod path;
pub mod persist;
pub mod reconnect;
pub mod session;
pub mod state;
pub mod store;
pub mod tls;
pub mod urb;
pub mod usb;
pub mod usbip_forward;
pub mod usbip_proxy;

pub use client::{ClientError, FarBusClient};
pub use discovery::{decode_beacon, encode_beacon};
pub use farbus_protocol::{
    AttachRequest, AttachResponse, DetachRequest, DeviceId, DeviceInfo, DeviceList,
    DeviceListRequest, ErrorCode, Hello, Message, PairRequest, PairResponse, TransferType,
    UrbComplete, UrbSubmit, UsbSpeed, VERSION,
};
pub use fingerprint::{FingerprintError, PeerFingerprint};
pub use frame::{read_message, write_message, FrameError};
pub use happy::happy_eyeballs_connect;
#[cfg(target_os = "linux")]
pub use host_usb::{complete_host_or_emulated, scan_libusb};
pub use identity::{
    constant_time_eq, fingerprint_from_secret, hash_pin, issue_auth_token, Identity, PairingPin,
};
pub use lease::{LeaseBook, LeaseError};
pub use path::connection_order;
pub use persist::load_or_create_server_identity;
pub use reconnect::{connect_with_retry, ReconnectPolicy};
pub use session::{serve_session, ServerState};
pub use state::{ConnectionEvent, ConnectionMachine, ConnectionState, TransitionError};
pub use store::{load_session, save_session, StoredSession};
pub use tls::{make_pinned_client_config, make_self_signed, make_server_config, TlsError};
pub use urb::complete_urb;
pub use usb::{
    parse_sysfs_device, scan_host_usb, scan_sysfs, simulated_lab_devices, DeviceBackend,
    LocalDevice,
};
pub use usbip_forward::serve_usbip_forward;
pub use usbip_proxy::{encode_device_header, handle_client, serve_usbip_loopback};
