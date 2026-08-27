use farbus_protocol::{decode, encode, Error as ProtoError, Message, HEADER_LEN, MAX_PAYLOAD};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Reads one framed `FarBus` message.
///
/// # Errors
///
/// Returns protocol or I/O errors when the stream is truncated or malformed.
pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Message, FrameError> {
    let mut header = [0u8; HEADER_LEN];
    reader.read_exact(&mut header).await?;
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&header[6..10]);
    let len = usize::try_from(u32::from_be_bytes(len_bytes)).unwrap_or(usize::MAX);
    if len > MAX_PAYLOAD {
        return Err(FrameError::Protocol(ProtoError::PayloadTooLarge {
            len,
            max: MAX_PAYLOAD,
        }));
    }
    let mut payload = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut payload).await?;
    }
    let mut frame = Vec::with_capacity(HEADER_LEN + len);
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&payload);
    decode(&frame).map_err(FrameError::Protocol)
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
