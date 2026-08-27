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
use tokio::sync::{mpsc, Mutex, Semaphore};

const MAX_IN_FLIGHT_URBS: usize = 64;

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
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(128);
    let principal = Arc::new(Mutex::new(None));
    let urb_slots = Arc::new(Semaphore::new(MAX_IN_FLIGHT_URBS));

    let writer = async {
        while let Some(msg) = out_rx.recv().await {
            write_message(&mut writer_half, &msg).await?;
        }
        Ok::<(), FrameError>(())
    };

    let reader_principal = Arc::clone(&principal);
    let reader_state = Arc::clone(&state);
    let reader = async move {
        let mut framed_reader = FramedReader::new();
        let mut peer = None;
        loop {
            let msg = match framed_reader.read_message(&mut reader_half).await {
                Ok(msg) => msg,
                Err(FrameError::Io(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(());
                }
                Err(err) => return Err(err),
            };

            match msg {
                Message::Hello(hello) => {
                    peer = Some(PeerFingerprint::new(hello.fingerprint));
                    if out_tx
                        .send(Message::Hello(Hello {
                            protocol_min: VERSION,
                            protocol_max: VERSION,
                            fingerprint: *reader_state.fingerprint.as_bytes(),
                            hostname: reader_state.hostname.clone(),
                        }))
                        .await
                        .is_err()
                    {
                        return Ok(());
                    }
                }
                Message::PairRequest(req) => {
                    let request_peer = PeerFingerprint::new(req.client_fingerprint);
                    let mut pin = reader_state.pin.lock().await;
                    let success = peer == Some(request_peer) && pin.is_valid(&req.pin_hash);
                    let token = if success {
                        *pin = PairingPin::issue(reader_state.fingerprint);
                        let token = issue_auth_token();
                        reader_state.tokens.lock().await.insert(
                            token,
                            (
                                request_peer,
                                Instant::now() + Duration::from_secs(24 * 60 * 60),
                            ),
                        );
                        *reader_principal.lock().await = Some(request_peer);
                        token
                    } else {
                        [0u8; 32]
                    };
                    if out_tx
                        .send(Message::PairResponse(PairResponse {
                            success,
                            server_fingerprint: *reader_state.fingerprint.as_bytes(),
                            auth_token: token,
                        }))
                        .await
                        .is_err()
                    {
                        return Ok(());
                    }
                }
                Message::DeviceListRequest(req) => {
                    let tokens = reader_state.tokens.lock().await;
                    let Some((owner, expires)) = tokens.get(&req.auth_token).copied() else {
                        drop(tokens);
                        let _ = send_error(&out_tx, ErrorCode::Unauthorized, "invalid token").await;
                        continue;
                    };
                    if peer != Some(owner) || Instant::now() > expires {
                        drop(tokens);
                        let _ = send_error(&out_tx, ErrorCode::Unauthorized, "expired token").await;
                        continue;
                    }
                    drop(tokens);
                    *reader_principal.lock().await = Some(owner);
                    let _ = out_tx
                        .send(Message::DeviceList(reader_state.device_list()))
                        .await;
                }
                Message::DeviceList(_) => {
                    let _ = send_error(&out_tx, ErrorCode::Unauthorized, "pairing required").await;
                }
                Message::AttachRequest(req) => {
                    let tokens = reader_state.tokens.lock().await;
                    let Some((owner, expires)) = tokens.get(&req.auth_token).copied() else {
                        drop(tokens);
                        let _ = send_error(&out_tx, ErrorCode::Unauthorized, "invalid token").await;
                        continue;
                    };
                    if peer != Some(owner) || Instant::now() > expires {
                        drop(tokens);
                        let _ = send_error(&out_tx, ErrorCode::Unauthorized, "expired token").await;
                        continue;
                    }
                    drop(tokens);
                    *reader_principal.lock().await = Some(owner);
                    let device = reader_state
                        .devices
                        .iter()
                        .find(|d| d.info.id == req.device_id && d.info.exported)
                        .cloned();
                    let Some(device) = device else {
                        let _ = send_error(&out_tx, ErrorCode::NotFound, "unknown device").await;
                        continue;
                    };
                    if device.info.usb_class == 3 {
                        eprintln!(
                            "warning: exporting HID device {} ({}); it can inject input",
                            device.info.bus_id, device.info.product
                        );
                    }
                    let success = reader_state
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
                    let tokens = reader_state.tokens.lock().await;
                    let Some((owner, expires)) = tokens.get(&req.auth_token).copied() else {
                        drop(tokens);
                        let _ = send_error(&out_tx, ErrorCode::Unauthorized, "invalid token").await;
                        continue;
                    };
                    if peer != Some(owner) || Instant::now() > expires {
                        drop(tokens);
                        let _ = send_error(&out_tx, ErrorCode::Unauthorized, "expired token").await;
                        continue;
                    }
                    drop(tokens);
                    *reader_principal.lock().await = Some(owner);
                    let _ = reader_state
                        .leases
                        .lock()
                        .await
                        .release(req.device_id, owner);
                    let _ = out_tx
                        .send(Message::Detach {
                            device_id: req.device_id,
                        })
                        .await;
                }
                Message::Detach { .. } => {
                    let _ = send_error(&out_tx, ErrorCode::Unauthorized, "token required").await;
                }
                Message::UrbSubmit(urb) => {
                    let owner = *reader_principal.lock().await;
                    let Some(owner) = owner else {
                        let _ = out_tx
                            .send(Message::UrbComplete(UrbComplete {
                                seq: urb.seq,
                                status: -13,
                                data: Vec::new(),
                            }))
                            .await;
                        continue;
                    };
                    let tokens = reader_state.tokens.lock().await;
                    let active = tokens.values().any(|(token_owner, expires)| {
                        *token_owner == owner && Instant::now() <= *expires
                    });
                    drop(tokens);
                    let leased =
                        reader_state.leases.lock().await.owner(urb.device_id) == Some(owner);
                    if !active || !leased {
                        let _ = out_tx
                            .send(Message::UrbComplete(UrbComplete {
                                seq: urb.seq,
                                status: -13,
                                data: Vec::new(),
                            }))
                            .await;
                        continue;
                    }

                    let Ok(permit) = Arc::clone(&urb_slots).acquire_owned().await else {
                        let _ = out_tx
                            .send(Message::UrbComplete(UrbComplete {
                                seq: urb.seq,
                                status: -1,
                                data: Vec::new(),
                            }))
                            .await;
                        continue;
                    };
                    let tx = out_tx.clone();
                    let task_state = Arc::clone(&reader_state);
                    tokio::spawn(async move {
                        let complete = complete_session_urb(urb, task_state).await;
                        drop(permit);
                        let _ = tx.send(Message::UrbComplete(complete)).await;
                    });
                }
                Message::UrbUnlink(unlink) => {
                    let owner = *reader_principal.lock().await;
                    let leased = if let Some(owner) = owner {
                        reader_state.leases.lock().await.owner(unlink.device_id) == Some(owner)
                    } else {
                        false
                    };
                    let status = if leased { 0 } else { -13 };
                    let _ = out_tx
                        .send(Message::UrbUnlinked(UrbUnlinked {
                            seq: unlink.seq,
                            status,
                        }))
                        .await;
                }
                other => {
                    let _ = send_error(
                        &out_tx,
                        ErrorCode::Unsupported,
                        &format!("unsupported {other:?}"),
                    )
                    .await;
                }
            }
        }
    };

    let result = tokio::try_join!(reader, writer);

    if let Some(owner) = *principal.lock().await {
        state.leases.lock().await.release_all(owner);
    }
    result.map(|_| ())
}

async fn send_error(
    tx: &mpsc::Sender<Message>,
    code: ErrorCode,
    detail: &str,
) -> Result<(), mpsc::error::SendError<Message>> {
    tx.send(Message::Error {
        code,
        detail: detail.into(),
    })
    .await
}

async fn complete_session_urb(urb: UrbSubmit, state: Arc<ServerState>) -> UrbComplete {
    if let Some(ref custom) = state.urb_completer {
        return custom(urb).await;
    }

    #[cfg(target_os = "linux")]
    {
        let host = state
            .devices
            .iter()
            .any(|device| device.info.id == urb.device_id && device.backend == DeviceBackend::Host);
        if host {
            let devices = state.devices.clone();
            let submitted = urb.clone();
            return match tokio::task::spawn_blocking(move || {
                crate::host_usb::complete_host_or_emulated(&submitted, &devices)
            })
            .await
            {
                Ok(result) => result,
                Err(_) => UrbComplete {
                    seq: urb.seq,
                    status: -1,
                    data: Vec::new(),
                },
            };
        }
    }

    complete_urb(&urb)
}
