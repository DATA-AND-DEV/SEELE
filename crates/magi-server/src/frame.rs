//! Length-prefixed framing over a QUIC stream.
//!
//! `specs/02-protocolo.md` puts control on "bidirectional stream #0, long
//! lived". A QUIC stream is a byte stream, so message boundaries have to be
//! written down: four bytes of big-endian length, then the frame.
//!
//! The length is checked against [`magi_proto::control::MAX_FRAME_LEN`] **before**
//! anything is allocated, per `specs/08-seguranca.md`. Reading a 4 GiB length
//! from an unauthenticated peer and reserving for it is the oldest denial of
//! service there is.

use anyhow::{bail, Result};
use magi_proto::control::{Validate, MAX_FRAME_LEN};
use serde::{Deserialize, Serialize};

/// Reads one frame and decodes it.
///
/// # Errors
///
/// Fails on a closed stream, an oversized length, or a malformed frame.
pub async fn read<T>(stream: &mut quinn::RecvStream) -> Result<T>
where
    T: for<'de> Deserialize<'de> + Validate,
{
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;

    // Before allocating. specs/08-seguranca.md.
    if length > MAX_FRAME_LEN {
        bail!("peer announced a {length}-byte control frame, over the {MAX_FRAME_LEN}-byte limit");
    }
    if length == 0 {
        bail!("peer announced an empty control frame");
    }

    let mut frame = vec![0_u8; length];
    stream.read_exact(&mut frame).await?;
    Ok(magi_proto::control::decode::<T>(&frame)?)
}

/// Encodes a message and writes it as one frame.
///
/// # Errors
///
/// Fails if the message is invalid or the stream is closed.
pub async fn write<T>(stream: &mut quinn::SendStream, message: &T) -> Result<()>
where
    T: Serialize + Validate,
{
    let frame = magi_proto::control::encode(message)?;
    let length = u32::try_from(frame.len())?;
    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(&frame).await?;
    Ok(())
}
