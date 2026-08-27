use farbus_protocol::{decode, encode, Error as ProtoError, Message, HEADER_LEN, MAX_PAYLOAD};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// A stateful reader that buffers partial frames and is safe against `tokio::select!` cancellation.
#[derive(Debug, Default)]
pub struct FramedReader {
    buffer: Vec<u8>,
}

impl FramedReader {
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(1024),
        }
    }

    /// Reads bytes from the underlying reader into the internal buffer until a complete frame is available.
    ///
    /// # Errors
    ///
    /// Returns protocol errors when frame payload exceeds maximum allowed size or framing is invalid.
    pub async fn read_message<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
    ) -> Result<Message, FrameError> {
        loop {
            if let Some(msg) = self.try_decode_frame()? {
                return Ok(msg);
            }
            let needed = self.bytes_needed()?;
            let mut chunk = vec![0u8; needed];
            let n = reader.read(&mut chunk).await?;
            if n == 0 {
                return Err(FrameError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "stream ended while reading frame",
                )));
            }
            self.buffer.extend_from_slice(&chunk[..n]);
        }
    }

    fn bytes_needed(&self) -> Result<usize, FrameError> {
        if self.buffer.len() < HEADER_LEN {
            return Ok(HEADER_LEN - self.buffer.len());
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&self.buffer[6..10]);
        let payload_len = usize::try_from(u32::from_be_bytes(len_bytes)).unwrap_or(usize::MAX);
        if payload_len > MAX_PAYLOAD {
            return Err(FrameError::Protocol(ProtoError::PayloadTooLarge {
                len: payload_len,
                max: MAX_PAYLOAD,
            }));
        }
        let frame_len = HEADER_LEN + payload_len;
        Ok(frame_len.saturating_sub(self.buffer.len()).max(1))
    }

    fn try_decode_frame(&mut self) -> Result<Option<Message>, FrameError> {
        if self.buffer.len() < HEADER_LEN {
            return Ok(None);
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&self.buffer[6..10]);
        let payload_len = usize::try_from(u32::from_be_bytes(len_bytes)).unwrap_or(usize::MAX);
        if payload_len > MAX_PAYLOAD {
            return Err(FrameError::Protocol(ProtoError::PayloadTooLarge {
                len: payload_len,
                max: MAX_PAYLOAD,
            }));
        }
        let frame_len = HEADER_LEN + payload_len;
        if self.buffer.len() < frame_len {
            return Ok(None);
        }

        let frame_bytes = self.buffer.drain(..frame_len).collect::<Vec<u8>>();
        let msg = decode(&frame_bytes).map_err(FrameError::Protocol)?;
        Ok(Some(msg))
    }
}

/// Reads one framed `FarBus` message.
///
/// # Errors
///
/// Returns protocol or I/O errors when the stream is truncated or malformed.
pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Message, FrameError> {
    let mut framed = FramedReader::new();
    framed.read_message(reader).await
}

/// Writes one framed `FarBus` message.
///
/// # Errors
///
/// Returns encode or I/O errors.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Message,
) -> Result<(), FrameError> {
    let bytes = encode(message)?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error(transparent)]
    Protocol(#[from] ProtoError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
