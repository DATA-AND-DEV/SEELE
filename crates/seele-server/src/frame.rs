//! Length-prefixed framing over a QUIC stream.
//!
//! `specs/02-protocolo.md` puts control on "bidirectional stream #0, long
//! lived". A QUIC stream is a byte stream, so message boundaries have to be
//! written down: four bytes of big-endian length, then the frame.
//!
//! The length is checked against [`seele_proto::control::MAX_FRAME_LEN`] **before**
//! anything is allocated, per `specs/08-seguranca.md`. Reading a 4 GiB length
//! from an unauthenticated peer and reserving for it is the oldest denial of
//! service there is.

use anyhow::{bail, Result};
use seele_proto::control::{Validate, MAX_FRAME_LEN};
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
    let mut primeiro = [0_u8; 1];
    stream.read_exact(&mut primeiro).await?;
    read_apos(stream, primeiro.first().copied().unwrap_or_default()).await
}

/// O mesmo [`read`], num fluxo cujo primeiro byte já foi lido.
///
/// Existe por causa de uma multiplexação que o fio não carrega: uma conexão tem
/// um `accept_uni` só e dois usos para fluxo unidirecional — uma transferência
/// e uma transmissão de tela —, e **nada no primeiro byte diz qual é**. O que
/// diz é a aritmética: um quadro deste enquadramento tem no máximo
/// [`MAX_FRAME_LEN`] bytes, 16 KiB, então o byte mais significativo do
/// comprimento é **sempre zero**; o cabeçalho de tela abre com a versão do
/// protocolo, que nasceu em 1 e nunca foi 0 nesta feature. Então zero é
/// transferência e qualquer outra coisa é tela — e quem demultiplexa consome
/// esse byte antes de saber o que fazer com ele.
///
/// **É uma leitura do formato, e não uma marca dele**, e isso é dívida: o lugar
/// certo de um discriminante de fluxo é um byte de tipo em `seele-proto`, na
/// frente dos dois cabeçalhos. Está no relatório desta tarefa.
///
/// # Errors
///
/// As mesmas de [`read`].
pub async fn read_apos<T>(stream: &mut quinn::RecvStream, primeiro: u8) -> Result<T>
where
    T: for<'de> Deserialize<'de> + Validate,
{
    let mut resto = [0_u8; 3];
    stream.read_exact(&mut resto).await?;
    let length = u32::from_be_bytes([
        primeiro,
        resto.first().copied().unwrap_or_default(),
        resto.get(1).copied().unwrap_or_default(),
        resto.get(2).copied().unwrap_or_default(),
    ]) as usize;

    // Before allocating. specs/08-seguranca.md.
    if length > MAX_FRAME_LEN {
        bail!("peer announced a {length}-byte control frame, over the {MAX_FRAME_LEN}-byte limit");
    }
    if length == 0 {
        bail!("peer announced an empty control frame");
    }

    let mut frame = vec![0_u8; length];
    stream.read_exact(&mut frame).await?;
    Ok(seele_proto::control::decode::<T>(&frame)?)
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
    let frame = seele_proto::control::encode(message)?;
    let length = u32::try_from(frame.len())?;
    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(&frame).await?;
    Ok(())
}
