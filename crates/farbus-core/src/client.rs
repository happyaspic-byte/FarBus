use crate::fingerprint::PeerFingerprint;
use crate::frame::{write_message, FrameError, FramedReader};
use crate::identity::{hash_pin, Identity};
use crate::tls::make_pinned_client_config;
use farbus_protocol::{
    AttachRequest, AttachResponse, DetachRequest, DeviceId, DeviceList, DeviceListRequest, Hello,
    Message, PairRequest, PairResponse, TransferType, UrbComplete, UrbSubmit, UrbUnlink,
    UrbUnlinked, VERSION,
};
use rustls::pki_types::ServerName;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::split;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_rustls::client::TlsStream;

const LEASE_DENIED: i32 = -13;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("TLS error")]
    Tls,
    #[error("pairing rejected")]
    PairRejected,
    #[error("unexpected message")]
    Unexpected,
    #[error("attach rejected")]
    AttachRejected,
    #[error("connection closed")]
    Closed,
}

enum ClientCmd {
    Control {
        msg: Message,
        reply: oneshot::Sender<Result<Message, ClientError>>,
    },
    Urb {
        urb: UrbSubmit,
        reply: oneshot::Sender<Result<UrbComplete, ClientError>>,
    },
    Unlink {
        unlink: UrbUnlink,
        reply: oneshot::Sender<Result<i32, ClientError>>,
    },
}

struct ClientInner {
    cmd_tx: Mutex<mpsc::Sender<ClientCmd>>,
    pump_task: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct FarBusClient {
    inner: Arc<ClientInner>,
    identity: Identity,
    auth_token: Option<[u8; 32]>,
    addr: SocketAddr,
    expected: PeerFingerprint,
}

impl FarBusClient {
    /// Connects to a pinned `FarBus` server over TLS 1.3 with concurrent pipelining.
    ///
    /// # Errors
    ///
    /// Returns I/O or TLS errors when the handshake fails.
    pub async fn connect(addr: SocketAddr, expected: PeerFingerprint) -> Result<Self, ClientError> {
        let connector = make_pinned_client_config(expected).map_err(|_| ClientError::Tls)?;
        let tcp = TcpStream::connect(addr).await?;
        let _ = tcp.set_nodelay(true);
        let name = ServerName::try_from("farbus.local").map_err(|_| ClientError::Tls)?;
        let stream = connector
            .connect(name, tcp)
            .await
            .map_err(|_| ClientError::Tls)?;
        let identity = Identity::generate();
        let client = Self::from_stream(stream, identity, None, addr, expected);
        client.hello().await?;
        Ok(client)
    }

    fn from_stream(
        stream: TlsStream<TcpStream>,
        identity: Identity,
        auth_token: Option<[u8; 32]>,
        addr: SocketAddr,
        expected: PeerFingerprint,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<ClientCmd>(256);
        let pump = tokio::spawn(async move {
            run_client_pump(stream, cmd_rx).await;
        });

        Self {
            inner: Arc::new(ClientInner {
                cmd_tx: Mutex::new(cmd_tx),
                pump_task: Mutex::new(Some(pump)),
            }),
            identity,
            auth_token,
            addr,
            expected,
        }
    }

    /// Reopens the TLS session, preserving the bearer token and client identity.
    ///
    /// All clones share the I/O pump, so reconnect replaces it for every handle.
    ///
    /// # Errors
    ///
    /// Returns I/O or TLS errors when the handshake fails.
    pub async fn reconnect(&mut self) -> Result<(), ClientError> {
        let connector = make_pinned_client_config(self.expected).map_err(|_| ClientError::Tls)?;
        let tcp = TcpStream::connect(self.addr).await?;
        let _ = tcp.set_nodelay(true);
        let name = ServerName::try_from("farbus.local").map_err(|_| ClientError::Tls)?;
        let stream = connector
            .connect(name, tcp)
            .await
            .map_err(|_| ClientError::Tls)?;

        let (cmd_tx, cmd_rx) = mpsc::channel::<ClientCmd>(256);
        let pump = tokio::spawn(async move {
            run_client_pump(stream, cmd_rx).await;
        });

        if let Some(old) = self.inner.pump_task.lock().await.replace(pump) {
            old.abort();
        }
        *self.inner.cmd_tx.lock().await = cmd_tx;
        self.hello().await
    }

    #[must_use]
    pub fn with_auth_token(mut self, token: [u8; 32]) -> Self {
        self.auth_token = Some(token);
        self
    }

    #[must_use]
    pub fn auth_token(&self) -> Option<[u8; 32]> {
        self.auth_token
    }

    async fn send_cmd(&self, cmd: ClientCmd) -> Result<(), ClientError> {
        let tx = self.inner.cmd_tx.lock().await.clone();
        tx.send(cmd).await.map_err(|_| ClientError::Closed)
    }

    async fn hello(&self) -> Result<(), ClientError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_cmd(ClientCmd::Control {
            msg: Message::Hello(Hello {
                protocol_min: VERSION,
                protocol_max: VERSION,
                fingerprint: *self.identity.fingerprint.as_bytes(),
                hostname: "farbus-client".into(),
            }),
            reply: reply_tx,
        })
        .await?;

        match reply_rx.await.map_err(|_| ClientError::Closed)?? {
            Message::Hello(_) => Ok(()),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Completes PIN pairing and stores the issued auth token.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::PairRejected`] when the PIN is invalid.
    pub async fn pair(&mut self, pin: &str, server: PeerFingerprint) -> Result<(), ClientError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_cmd(ClientCmd::Control {
            msg: Message::PairRequest(PairRequest {
                client_fingerprint: *self.identity.fingerprint.as_bytes(),
                pin_hash: hash_pin(pin, server),
                client_name: "farbus-client".into(),
            }),
            reply: reply_tx,
        })
        .await?;

        match reply_rx.await.map_err(|_| ClientError::Closed)?? {
            Message::PairResponse(PairResponse {
                success: true,
                auth_token,
                ..
            }) => {
                self.auth_token = Some(auth_token);
                Ok(())
            }
            Message::PairResponse(_) => Err(ClientError::PairRejected),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Lists exported devices.
    ///
    /// # Errors
    ///
    /// Returns framing errors on disconnect.
    pub async fn devices(&self) -> Result<DeviceList, ClientError> {
        let token = self.auth_token.ok_or(ClientError::PairRejected)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_cmd(ClientCmd::Control {
            msg: Message::DeviceListRequest(DeviceListRequest { auth_token: token }),
            reply: reply_tx,
        })
        .await?;

        match reply_rx.await.map_err(|_| ClientError::Closed)?? {
            Message::DeviceList(list) => Ok(list),
            Message::Error { .. } => Err(ClientError::AttachRejected),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Attaches a remote device.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::AttachRejected`] when the lease is denied.
    pub async fn attach(&self, device_id: DeviceId) -> Result<AttachResponse, ClientError> {
        let token = self.auth_token.ok_or(ClientError::PairRejected)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_cmd(ClientCmd::Control {
            msg: Message::AttachRequest(AttachRequest {
                device_id,
                auth_token: token,
            }),
            reply: reply_tx,
        })
        .await?;

        match reply_rx.await.map_err(|_| ClientError::Closed)?? {
            Message::AttachResponse(res) if res.success => Ok(res),
            Message::AttachResponse(_) | Message::Error { .. } => Err(ClientError::AttachRejected),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Detaches a remote device.
    ///
    /// # Errors
    ///
    /// Returns framing errors on disconnect.
    pub async fn detach(&self, device_id: DeviceId) -> Result<(), ClientError> {
        let token = self.auth_token.ok_or(ClientError::PairRejected)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_cmd(ClientCmd::Control {
            msg: Message::DetachRequest(DetachRequest {
                device_id,
                auth_token: token,
            }),
            reply: reply_tx,
        })
        .await?;

        match reply_rx.await.map_err(|_| ClientError::Closed)?? {
            Message::Detach { .. } => Ok(()),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Requests cancellation of a previously submitted URB.
    ///
    /// # Errors
    ///
    /// Returns framing errors on disconnect.
    pub async fn unlink(&self, device_id: DeviceId, seq: u32) -> Result<i32, ClientError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_cmd(ClientCmd::Unlink {
            unlink: UrbUnlink { seq, device_id },
            reply: reply_tx,
        })
        .await?;

        let status = reply_rx.await.map_err(|_| ClientError::Closed)??;
        if status == LEASE_DENIED {
            Err(ClientError::AttachRejected)
        } else {
            Ok(status)
        }
    }

    /// Submits one URB and waits for completion in parallel with other in-flight requests.
    ///
    /// # Errors
    ///
    /// Returns framing errors on disconnect.
    pub async fn urb(
        &self,
        device_id: DeviceId,
        seq: u32,
        endpoint: u8,
        transfer: TransferType,
        data: Vec<u8>,
    ) -> Result<UrbComplete, ClientError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_cmd(ClientCmd::Urb {
            urb: UrbSubmit {
                seq,
                device_id,
                endpoint,
                transfer,
                data,
            },
            reply: reply_tx,
        })
        .await?;

        let complete = reply_rx.await.map_err(|_| ClientError::Closed)??;
        if complete.status == LEASE_DENIED {
            Err(ClientError::AttachRejected)
        } else {
            Ok(complete)
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn run_client_pump(stream: TlsStream<TcpStream>, mut cmd_rx: mpsc::Receiver<ClientCmd>) {
    let (mut reader_half, mut writer_half) = split(stream);
    let mut framed_reader = FramedReader::new();
    let mut pending_urbs: HashMap<u32, oneshot::Sender<Result<UrbComplete, ClientError>>> =
        HashMap::new();
    let mut pending_unlinks: HashMap<u32, oneshot::Sender<Result<i32, ClientError>>> =
        HashMap::new();
    let mut control_replies: VecDeque<oneshot::Sender<Result<Message, ClientError>>> =
        VecDeque::new();

    loop {
        tokio::select! {
            cmd_opt = cmd_rx.recv() => {
                let Some(cmd) = cmd_opt else {
                    break;
                };
                match cmd {
                    ClientCmd::Control { msg, reply } => {
                        control_replies.push_back(reply);
                        if let Err(err) = write_message(&mut writer_half, &msg).await {
                            if let Some(r) = control_replies.pop_front() {
                                let _ = r.send(Err(ClientError::Frame(err)));
                            }
                            break;
                        }
                    }
                    ClientCmd::Urb { urb, reply } => {
                        pending_urbs.insert(urb.seq, reply);
                        if write_message(&mut writer_half, &Message::UrbSubmit(urb)).await.is_err() {
                            break;
                        }
                    }
                    ClientCmd::Unlink { unlink, reply } => {
                        pending_unlinks.insert(unlink.seq, reply);
                        if write_message(&mut writer_half, &Message::UrbUnlink(unlink)).await.is_err() {
                            break;
                        }
                    }
                }
            }

            read_res = framed_reader.read_message(&mut reader_half) => {
                let Ok(msg) = read_res else {
                    break;
                };
                match msg {
                    Message::UrbComplete(complete) => {
                        if let Some(reply) = pending_urbs.remove(&complete.seq) {
                            let _ = reply.send(Ok(complete));
                        }
                    }
                    Message::UrbUnlinked(UrbUnlinked { seq, status }) => {
                        if let Some(reply) = pending_unlinks.remove(&seq) {
                            let _ = reply.send(Ok(status));
                        }
                    }
                    Message::Error { .. } => {
                        if let Some(reply) = control_replies.pop_front() {
                            let _ = reply.send(Ok(msg));
                        }
                    }
                    other => {
                        if let Some(reply) = control_replies.pop_front() {
                            let _ = reply.send(Ok(other));
                        }
                    }
                }
            }
        }
    }

    while let Some(r) = control_replies.pop_front() {
        let _ = r.send(Err(ClientError::Closed));
    }
    for (_, r) in pending_urbs.drain() {
        let _ = r.send(Err(ClientError::Closed));
    }
    for (_, r) in pending_unlinks.drain() {
        let _ = r.send(Err(ClientError::Closed));
    }
}
