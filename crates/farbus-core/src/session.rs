use crate::fingerprint::PeerFingerprint;
use crate::frame::{write_message, FrameError, FramedReader};
use crate::identity::{issue_auth_token, PairingPin};
use crate::lease::LeaseBook;
use crate::urb::complete_urb;
#[cfg(target_os = "linux")]
use crate::usb::DeviceBackend;
use crate::usb::LocalDevice;
use farbus_protocol::{
    AttachResponse, DeviceList, ErrorCode, Hello, Message, PairResponse, UrbComplete, UrbSubmit,
    UrbUnlinked, VERSION,
};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{split, AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, Mutex};

pub type UrbCompleterFuture = Pin<Box<dyn Future<Output = UrbComplete> + Send>>;
pub type UrbCompleter = Arc<dyn Fn(UrbSubmit) -> UrbCompleterFuture + Send + Sync>;

pub struct ServerState {
    pub hostname: String,
    pub fingerprint: PeerFingerprint,
    pub pin: Mutex<PairingPin>,
    pub leases: Mutex<LeaseBook>,
    pub tokens: Mutex<HashMap<[u8; 32], (PeerFingerprint, Instant)>>,
    pub devices: Vec<LocalDevice>,
    pub urb_completer: Option<UrbCompleter>,
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
            urb_completer: None,
        }
    }

    #[must_use]
    pub fn with_urb_completer(mut self, completer: UrbCompleter) -> Self {
        self.urb_completer = Some(completer);
        self
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

/// Serves one authenticated `FarBus` session with pipelined, non-blocking URB processing.
///
/// # Errors
///
/// Returns framing or I/O errors when the peer disconnects or sends invalid frames.
#[allow(clippy::too_many_lines)]
pub async fn serve_session<S>(stream: &mut S, state: Arc<ServerState>) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut reader_half, mut writer_half) = split(stream);
    let mut framed_reader = FramedReader::new();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(128);

    let mut peer = None;
    let mut principal = None;

    loop {
        tokio::select! {
            biased;

            Some(msg_to_send) = out_rx.recv() => {
                write_message(&mut writer_half, &msg_to_send).await?;
            }

            read_res = framed_reader.read_message(&mut reader_half) => {
                let msg = match read_res {
                    Ok(msg) => msg,
                    Err(FrameError::Io(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                        break;
                    }
                    Err(err) => return Err(err),
                };

                match msg {
                    Message::Hello(hello) => {
                        peer = Some(PeerFingerprint::new(hello.fingerprint));
                        let _ = out_tx
                            .send(Message::Hello(Hello {
                                protocol_min: VERSION,
                                protocol_max: VERSION,
                                fingerprint: *state.fingerprint.as_bytes(),
                                hostname: state.hostname.clone(),
                            }))
                            .await;
                    }
                    Message::PairRequest(req) => {
                        let request_peer = PeerFingerprint::new(req.client_fingerprint);
                        let mut pin = state.pin.lock().await;
                        let success = peer == Some(request_peer) && pin.is_valid(&req.pin_hash);
                        let token = if success {
                            *pin = PairingPin::issue(state.fingerprint);
                            let token = issue_auth_token();
                            state.tokens.lock().await.insert(
                                token,
                                (
                                    request_peer,
                                    Instant::now() + Duration::from_secs(24 * 60 * 60),
                                ),
                            );
                            principal = Some(request_peer);
                            token
                        } else {
                            [0u8; 32]
                        };
                        let _ = out_tx
                            .send(Message::PairResponse(PairResponse {
                                success,
                                server_fingerprint: *state.fingerprint.as_bytes(),
                                auth_token: token,
                            }))
                            .await;
                    }
                    Message::DeviceListRequest(req) => {
                        let tokens = state.tokens.lock().await;
                        let Some((owner, expires)) = tokens.get(&req.auth_token).copied() else {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "invalid token".into(),
                                })
                                .await;
                            continue;
                        };
                        if peer != Some(owner) || Instant::now() > expires {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "expired token".into(),
                                })
                                .await;
                            continue;
                        }
                        drop(tokens);
                        principal = Some(owner);
                        let _ = out_tx
                            .send(Message::DeviceList(state.device_list()))
                            .await;
                    }
                    Message::DeviceList(_) => {
                        let _ = out_tx
                            .send(Message::Error {
                                code: ErrorCode::Unauthorized,
                                detail: "pairing required".into(),
                            })
                            .await;
                    }
                    Message::AttachRequest(req) => {
                        let tokens = state.tokens.lock().await;
                        let Some((owner, expires)) = tokens.get(&req.auth_token).copied() else {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "invalid token".into(),
                                })
                                .await;
                            continue;
                        };
                        if peer != Some(owner) || Instant::now() > expires {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "expired token".into(),
                                })
                                .await;
                            continue;
                        }
                        drop(tokens);
                        principal = Some(owner);
                        let device = state
                            .devices
                            .iter()
                            .find(|d| d.info.id == req.device_id && d.info.exported)
                            .cloned();
                        let Some(device) = device else {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::NotFound,
                                    detail: "unknown device".into(),
                                })
                                .await;
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
                        let _ = out_tx
                            .send(Message::AttachResponse(AttachResponse {
                                device_id: req.device_id,
                                success,
                                usbip_port: 3240,
                                bus_id: device.info.bus_id,
                            }))
                            .await;
                    }
                    Message::DetachRequest(req) => {
                        let tokens = state.tokens.lock().await;
                        let Some((owner, expires)) = tokens.get(&req.auth_token).copied() else {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "invalid token".into(),
                                })
                                .await;
                            continue;
                        };
                        if peer != Some(owner) || Instant::now() > expires {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "expired token".into(),
                                })
                                .await;
                            continue;
                        }
                        drop(tokens);
                        principal = Some(owner);
                        let _ = state.leases.lock().await.release(req.device_id, owner);
                        let _ = out_tx
                            .send(Message::Detach {
                                device_id: req.device_id,
                            })
                            .await;
                    }
                    Message::Detach { .. } => {
                        let _ = out_tx
                            .send(Message::Error {
                                code: ErrorCode::Unauthorized,
                                detail: "token required".into(),
                            })
                            .await;
                    }
                    Message::UrbSubmit(urb) => {
                        let Some(peer) = principal else {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "missing client identity".into(),
                                })
                                .await;
                            continue;
                        };
                        let tokens = state.tokens.lock().await;
                        let active = tokens
                            .values()
                            .any(|(owner, expires)| *owner == peer && Instant::now() <= *expires);
                        drop(tokens);
                        if !active {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "expired token".into(),
                                })
                                .await;
                            continue;
                        }
                        if state.leases.lock().await.owner(urb.device_id) != Some(peer) {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "device lease required".into(),
                                })
                                .await;
                            continue;
                        }

                        let tx = out_tx.clone();
                        let state = Arc::clone(&state);
                        tokio::spawn(async move {
                            let complete = if let Some(ref custom) = state.urb_completer {
                                custom(urb).await
                            } else {
                                #[cfg(target_os = "linux")]
                                {
                                    let host = state.devices.iter().any(|device| {
                                        device.info.id == urb.device_id
                                            && device.backend == DeviceBackend::Host
                                    });
                                    if host {
                                        let devices = state.devices.clone();
                                        let submitted = urb.clone();
                                        match tokio::task::spawn_blocking(move || {
                                            crate::host_usb::complete_host_or_emulated(
                                                &submitted, &devices,
                                            )
                                        })
                                        .await
                                        {
                                            Ok(res) => res,
                                            Err(_) => complete_urb(&urb),
                                        }
                                    } else {
                                        complete_urb(&urb)
                                    }
                                }
                                #[cfg(not(target_os = "linux"))]
                                {
                                    complete_urb(&urb)
                                }
                            };
                            let _ = tx.send(Message::UrbComplete(complete)).await;
                        });
                    }
                    Message::UrbUnlink(unlink) => {
                        let Some(peer) = principal else {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "missing client identity".into(),
                                })
                                .await;
                            continue;
                        };
                        if state.leases.lock().await.owner(unlink.device_id) != Some(peer) {
                            let _ = out_tx
                                .send(Message::Error {
                                    code: ErrorCode::Unauthorized,
                                    detail: "device lease required".into(),
                                })
                                .await;
                            continue;
                        }
                        let _ = out_tx
                            .send(Message::UrbUnlinked(UrbUnlinked {
                                seq: unlink.seq,
                                status: 0,
                            }))
                            .await;
                    }
                    other => {
                        let _ = out_tx
                            .send(Message::Error {
                                code: ErrorCode::Unsupported,
                                detail: format!("unsupported {other:?}"),
                            })
                            .await;
                    }
                }
            }
        }
    }

    // Flush any pending responses before ending
    while let Ok(msg) = out_rx.try_recv() {
        let _ = write_message(&mut writer_half, &msg).await;
    }

    if let Some(owner) = principal {
        state.leases.lock().await.release_all(owner);
    }
    Ok(())
}
