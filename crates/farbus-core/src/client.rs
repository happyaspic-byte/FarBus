use crate::fingerprint::PeerFingerprint;
use crate::frame::{read_message, write_message, FrameError};
use crate::identity::{hash_pin, Identity};
use crate::tls::make_pinned_client_config;
use farbus_protocol::{
    AttachRequest, AttachResponse, DeviceId, DeviceList, Hello, Message, PairRequest, PairResponse,
    TransferType, UrbComplete, UrbSubmit, VERSION,
};
use rustls::pki_types::ServerName;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

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
}

pub struct FarBusClient {
    stream: TlsStream<TcpStream>,
    identity: Identity,
    pub auth_token: Option<[u8; 32]>,
    addr: SocketAddr,
    expected: PeerFingerprint,
}

impl FarBusClient {
    /// Connects to a pinned `FarBus` server over TLS 1.3.
    ///
    /// # Errors
    ///
    /// Returns I/O or TLS errors when the handshake fails.
    pub async fn connect(addr: SocketAddr, expected: PeerFingerprint) -> Result<Self, ClientError> {
        let connector = make_pinned_client_config(expected).map_err(|_| ClientError::Tls)?;
        let tcp = TcpStream::connect(addr).await?;
        let name = ServerName::try_from("farbus.local").map_err(|_| ClientError::Tls)?;
        let stream = connector
            .connect(name, tcp)
            .await
            .map_err(|_| ClientError::Tls)?;
        let identity = Identity::generate();
        let mut client = Self {
            stream,
            identity,
            auth_token: None,
            addr,
            expected,
        };
        client.hello().await?;
        Ok(client)
    }

    /// Reopens the TLS session, preserving the bearer token and client identity.
    ///
    /// # Errors
    ///
    /// Returns I/O or TLS errors when the handshake fails.
    pub async fn reconnect(&mut self) -> Result<(), ClientError> {
        let connector = make_pinned_client_config(self.expected).map_err(|_| ClientError::Tls)?;
        let tcp = TcpStream::connect(self.addr).await?;
        let name = ServerName::try_from("farbus.local").map_err(|_| ClientError::Tls)?;
        self.stream = connector
            .connect(name, tcp)
            .await
            .map_err(|_| ClientError::Tls)?;
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

    async fn hello(&mut self) -> Result<(), ClientError> {
        write_message(
            &mut self.stream,
            &Message::Hello(Hello {
                protocol_min: VERSION,
                protocol_max: VERSION,
                fingerprint: *self.identity.fingerprint.as_bytes(),
                hostname: "farbus-client".into(),
            }),
        )
        .await?;
        match read_message(&mut self.stream).await? {
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
        write_message(
            &mut self.stream,
            &Message::PairRequest(PairRequest {
                client_fingerprint: *self.identity.fingerprint.as_bytes(),
                pin_hash: hash_pin(pin, server),
                client_name: "farbus-client".into(),
            }),
        )
        .await?;
        match read_message(&mut self.stream).await? {
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
    pub async fn devices(&mut self) -> Result<DeviceList, ClientError> {
        write_message(
            &mut self.stream,
            &Message::DeviceList(DeviceList {
                devices: Vec::new(),
            }),
        )
        .await?;
        match read_message(&mut self.stream).await? {
            Message::DeviceList(list) => Ok(list),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Attaches a remote device.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::AttachRejected`] when the lease is denied.
    pub async fn attach(&mut self, device_id: DeviceId) -> Result<AttachResponse, ClientError> {
        let token = self.auth_token.ok_or(ClientError::PairRejected)?;
        write_message(
            &mut self.stream,
            &Message::AttachRequest(AttachRequest {
                device_id,
                auth_token: token,
            }),
        )
        .await?;
        match read_message(&mut self.stream).await? {
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
    pub async fn detach(&mut self, device_id: DeviceId) -> Result<(), ClientError> {
        write_message(&mut self.stream, &Message::Detach { device_id }).await?;
        match read_message(&mut self.stream).await? {
            Message::Detach { .. } => Ok(()),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Submits one URB and waits for completion.
    ///
    /// # Errors
    ///
    /// Returns framing errors on disconnect.
    pub async fn urb(
        &mut self,
        device_id: DeviceId,
        seq: u32,
        endpoint: u8,
        transfer: TransferType,
        data: Vec<u8>,
    ) -> Result<UrbComplete, ClientError> {
        write_message(
            &mut self.stream,
            &Message::UrbSubmit(UrbSubmit {
                seq,
                device_id,
                endpoint,
                transfer,
                data,
            }),
        )
        .await?;
        match read_message(&mut self.stream).await? {
            Message::UrbComplete(complete) => Ok(complete),
            _ => Err(ClientError::Unexpected),
        }
    }
}
