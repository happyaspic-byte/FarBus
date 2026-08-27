use crate::fingerprint::PeerFingerprint;
use crate::frame::{read_message, write_message, FrameError};
use crate::identity::{issue_auth_token, PairingPin};
use crate::lease::LeaseBook;
#[cfg(not(target_os = "linux"))]
use crate::urb::complete_urb;
use crate::usb::LocalDevice;
use farbus_protocol::{
    AttachResponse, DeviceList, ErrorCode, Hello, Message, PairResponse, VERSION,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;

pub struct ServerState {
    pub hostname: String,
    pub fingerprint: PeerFingerprint,
    pub pin: Mutex<PairingPin>,
    pub leases: Mutex<LeaseBook>,
    pub tokens: Mutex<HashMap<[u8; 32], (PeerFingerprint, Instant)>>,
    pub devices: Vec<LocalDevice>,
}

impl ServerState {
    #[must_use]
    pub fn new(hostname: String, fingerprint: PeerFingerprint, devices: Vec<LocalDevice>) -> Self {
        Self {
            pin: Mutex::new(PairingPin::issue(fingerprint)),
            hostname,
            fingerprint,
            leases: Mutex::new(LeaseBook::default()),
            tokens: Mutex::new(HashMap::new()),
            devices,
        }
    }

    #[must_use]
    pub fn device_list(&self) -> DeviceList {
        DeviceList {
            devices: self
                .devices
                .iter()
                .filter(|d| d.info.exported)
                .map(|d| d.info.clone())
                .collect(),
        }
    }
}

/// Serves one authenticated `FarBus` session.
///
/// # Errors
///
/// Returns framing or I/O errors when the peer disconnects or sends invalid frames.
#[allow(clippy::too_many_lines)]
pub async fn serve_session<S>(stream: &mut S, state: Arc<ServerState>) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut peer = None;
    loop {
        let msg = match read_message(stream).await {
            Ok(msg) => msg,
            Err(FrameError::Io(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(err) => return Err(err),
        };
        match msg {
            Message::Hello(hello) => {
                peer = Some(PeerFingerprint::new(hello.fingerprint));
                write_message(
                    stream,
                    &Message::Hello(Hello {
                        protocol_min: VERSION,
                        protocol_max: VERSION,
                        fingerprint: *state.fingerprint.as_bytes(),
                        hostname: state.hostname.clone(),
                    }),
                )
                .await?;
            }
            Message::PairRequest(req) => {
                let mut pin = state.pin.lock().await;
                let success = pin.is_valid(&req.pin_hash);
                let token = if success {
                    let token = issue_auth_token();
                    state.tokens.lock().await.insert(
                        token,
                        (
                            PeerFingerprint::new(req.client_fingerprint),
                            Instant::now() + Duration::from_secs(24 * 60 * 60),
                        ),
                    );
                    token
                } else {
                    [0u8; 32]
                };
                write_message(
                    stream,
                    &Message::PairResponse(PairResponse {
                        success,
                        server_fingerprint: *state.fingerprint.as_bytes(),
                        auth_token: token,
                    }),
                )
                .await?;
            }
            Message::DeviceList(_) => {
                write_message(stream, &Message::DeviceList(state.device_list())).await?;
            }
            Message::AttachRequest(req) => {
                let tokens = state.tokens.lock().await;
                let Some((owner, expires)) = tokens.get(&req.auth_token).copied() else {
                    write_message(
                        stream,
                        &Message::Error {
                            code: ErrorCode::Unauthorized,
                            detail: "invalid token".into(),
                        },
                    )
                    .await?;
                    continue;
                };
                if Instant::now() > expires {
                    write_message(
                        stream,
                        &Message::Error {
                            code: ErrorCode::Unauthorized,
                            detail: "expired token".into(),
                        },
                    )
                    .await?;
                    continue;
                }
                drop(tokens);
                let device = state
                    .devices
                    .iter()
                    .find(|d| d.info.id == req.device_id && d.info.exported)
                    .cloned();
                let Some(device) = device else {
                    write_message(
                        stream,
                        &Message::Error {
                            code: ErrorCode::NotFound,
                            detail: "unknown device".into(),
                        },
                    )
                    .await?;
                    continue;
                };
                if device.info.usb_class == 3 {
                    eprintln!(
                        "warning: exporting HID device {} ({}); it can inject input",
                        device.info.bus_id, device.info.product
                    );
                }
                let success = state
                    .leases
                    .lock()
                    .await
                    .acquire(req.device_id, owner)
                    .is_ok();
                write_message(
                    stream,
                    &Message::AttachResponse(AttachResponse {
                        device_id: req.device_id,
                        success,
                        usbip_port: 3240,
                        bus_id: device.info.bus_id,
                    }),
                )
                .await?;
            }
            Message::Detach { device_id } => {
                if let Some(owner) = peer {
                    let _ = state.leases.lock().await.release(device_id, owner);
                }
                write_message(stream, &Message::Detach { device_id }).await?;
            }
            Message::UrbSubmit(urb) => {
                #[cfg(target_os = "linux")]
                let complete = crate::host_usb::complete_host_or_emulated(&urb, &state.devices);
                #[cfg(not(target_os = "linux"))]
                let complete = complete_urb(&urb);
                write_message(stream, &Message::UrbComplete(complete)).await?;
            }
            other => {
                write_message(
                    stream,
                    &Message::Error {
                        code: ErrorCode::Unsupported,
                        detail: format!("unsupported {other:?}"),
                    },
                )
                .await?;
            }
        }
    }
    Ok(())
}
