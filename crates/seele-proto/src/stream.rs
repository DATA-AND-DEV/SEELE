//! Which of the two uses a unidirectional stream is put to.
//!
//! One byte, written before anything else, and it is the whole of §5.2 of
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`.
//!
//! # Why a byte exists where arithmetic used to do the job
//!
//! A server accepts unidirectional streams for two different things — an
//! attachment transfer (ADR 0027) and a screen transmission (§3.1) — and until
//! now nothing on the wire said which. What told them apart was a sum over the
//! first byte: an attachment opens with a four-byte big-endian length whose
//! ceiling is [`crate::control::MAX_FRAME_LEN`] — 16 KiB — so its most
//! significant byte is **always zero**, while a screen header opens with the
//! protocol version, which was born at 1.
//!
//! That works and it is fragile in a specific way: **both premises belong to
//! other sections.** The day the frame ceiling rises past 16 MiB, or the
//! protocol version passes 255, or a third kind of stream appears, the sum
//! stops closing — and the symptom is a stream read as the wrong kind, which is
//! the worst shape a protocol error takes. Nothing crashes, nothing is
//! refused; a file is decoded as pictures.
//!
//! # Zero is reserved, and refused
//!
//! Zero is not spare room. It is **exactly what the old reading produced** for
//! an attachment, so a peer built before this byte existed opens its transfers
//! with it. Refusing zero by name is what makes that peer fail loudly — one
//! enumerated refusal, at the first byte, before anything is allocated —
//! instead of having its length read as a type and its header read as
//! pictures. An old peer that cannot share a screen is a version mismatch; an
//! old peer whose files are fed to a decoder is a defect nobody can diagnose
//! from either end.
//!
//! # What it does not change
//!
//! The two headers that follow are **untouched**: an
//! [`crate::attachment::AttachmentHeader`] is still a length-prefixed
//! `postcard` frame and a [`crate::screen::ScreenHeader`] is still eleven fixed
//! bytes. This byte sits in front of them and nothing moves inside.

use thiserror::Error;

/// Bytes of stream type, before either header.
pub const STREAM_TYPE_LEN: usize = 1;

/// The value a build older than this byte wrote first, and which is refused.
///
/// Named rather than spelled `0` at the two places that mention it, because
/// what matters about it is not the number: it is that the number is already
/// taken by history. See the module documentation.
pub const RESERVED_TYPE: u8 = 0;

/// What a unidirectional stream carries.
///
/// The codes are append-only, like every other enum on this wire: a number is
/// never reused and a new type goes at the end. Here that rule has teeth
/// beyond compatibility — a code that changed meaning would hand a receiver
/// the wrong reader for the whole stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    /// An attachment transfer: ADR 0027, and
    /// [`crate::attachment::AttachmentHeader`] behind this byte.
    Attachment,
    /// A screen transmission: §3.1, and [`crate::screen::ScreenHeader`] behind
    /// this byte.
    Screen,
}

/// Why a stream's first byte was refused.
///
/// Enumerated, per `specs/02-protocolo.md`, and each variant carries what a
/// shell would need to write its own sentence — ADR 0012.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum StreamTypeError {
    /// The stream ended before its first byte.
    #[error("stream closed before its type byte")]
    Missing,

    /// [`RESERVED_TYPE`], which is what a peer older than this byte opens an
    /// attachment with.
    #[error(
        "stream opened with the reserved type byte {RESERVED_TYPE}, which is what a build older \
         than the stream type byte writes"
    )]
    Reserved,

    /// A type this build does not know.
    #[error("stream opened with type byte {code}, which this build does not know")]
    Unknown {
        /// The code received.
        code: u8,
    },
}

impl StreamType {
    /// The byte this travels as.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            // Deliberately not starting at zero: see `RESERVED_TYPE`.
            Self::Attachment => 1,
            Self::Screen => 2,
        }
    }

    /// Reads the byte back.
    ///
    /// # Errors
    ///
    /// Returns [`StreamTypeError::Reserved`] for [`RESERVED_TYPE`] and
    /// [`StreamTypeError::Unknown`] for anything else this build does not know.
    pub const fn decode(code: u8) -> Result<Self, StreamTypeError> {
        match code {
            RESERVED_TYPE => Err(StreamTypeError::Reserved),
            1 => Ok(Self::Attachment),
            2 => Ok(Self::Screen),
            _ => Err(StreamTypeError::Unknown { code }),
        }
    }

    /// Takes the type off the front of a stream, returning what followed it.
    ///
    /// The tail is returned rather than dropped for the reason
    /// [`crate::screen::ScreenHeader::decode`] gives: a QUIC stream is a byte
    /// stream, and a receiver that read the type byte and the head of the
    /// header in one syscall must not be made to forget the second half.
    ///
    /// # Errors
    ///
    /// Returns [`StreamTypeError`] for an empty stream or a byte this build
    /// refuses. Never panics, whatever the bytes are.
    pub fn split(bytes: &[u8]) -> Result<(Self, &[u8]), StreamTypeError> {
        let Some(code) = bytes.first().copied() else {
            return Err(StreamTypeError::Missing);
        };
        Ok((
            Self::decode(code)?,
            bytes.get(STREAM_TYPE_LEN..).unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachment::{hash, AttachmentHeader};
    use crate::control::{decode, encode};
    use crate::ids::{ClientMessageId, LineId, ScreenId};
    use crate::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
    use crate::version::PROTOCOL_VERSION;

    #[test]
    fn the_two_types_travel_as_one_and_two_and_stay_there() {
        // Pinned at the byte level, not through a round trip: a round trip is
        // self-referential and passes perfectly with both codes renumbered,
        // while a peer of the other build reads a transfer as a screen.
        assert_eq!(StreamType::Attachment.byte(), 1);
        assert_eq!(StreamType::Screen.byte(), 2);
        for stream_type in [StreamType::Attachment, StreamType::Screen] {
            assert_eq!(StreamType::decode(stream_type.byte()), Ok(stream_type));
        }
    }

    #[test]
    fn zero_is_refused_by_name_so_an_older_peer_fails_loudly() {
        // The reason zero is reserved rather than spent on the first type. A
        // build older than this byte opens an attachment with the most
        // significant byte of a length that cannot exceed 16 KiB — always zero
        // — so its stream arrives here as a zero. Refusing it is one
        // enumerated failure at the first byte; accepting it as `Attachment`
        // would work by accident today and read a length as a type the day a
        // third kind appears.
        assert_eq!(RESERVED_TYPE, 0);
        assert_eq!(
            StreamType::decode(RESERVED_TYPE),
            Err(StreamTypeError::Reserved)
        );
        assert!(
            ![StreamType::Attachment.byte(), StreamType::Screen.byte()].contains(&RESERVED_TYPE),
            "a type took the byte an older peer already writes"
        );

        // And the whole opening of an old transfer, byte for byte, is refused
        // rather than read: four bytes of big-endian length and then the frame.
        let old = [0_u8, 0, 0x40, 0x00, PROTOCOL_VERSION];
        assert_eq!(StreamType::split(&old), Err(StreamTypeError::Reserved));
    }

    #[test]
    fn an_unknown_type_is_refused_rather_than_guessed() {
        // §5.2 names the third kind of stream as the thing the old arithmetic
        // could not survive. This is what meeting it looks like: a refusal that
        // says the number, not a reader chosen at random.
        for code in [3_u8, 4, 200, 255] {
            assert_eq!(
                StreamType::decode(code),
                Err(StreamTypeError::Unknown { code }),
                "accepted an unknown type {code}"
            );
        }
    }

    #[test]
    fn a_stream_that_closed_before_its_first_byte_is_refused_not_misread() {
        // The laptop that closed between `open_uni` and the first write.
        // `specs/08-seguranca.md` names this crate as the untrusted-input
        // surface: every one of these is a refusal and none of them a panic.
        assert_eq!(StreamType::split(&[]), Err(StreamTypeError::Missing));
    }

    #[test]
    fn the_screen_header_is_unchanged_and_merely_sits_one_byte_later() {
        // What §5.2 promises: the byte goes in front and nothing moves inside.
        // This is the receiver's whole job — take one byte, hand the rest to
        // the reader that byte named.
        let header = ScreenHeader {
            version: PROTOCOL_VERSION,
            screen: ScreenId(0x00C0_FFEE),
            source: ScreenSource::Window,
            codec: ScreenCodec::H264Baseline,
            width: 1280,
            height: 720,
        };
        let mut stream = vec![StreamType::Screen.byte()];
        let mut head = [0_u8; SCREEN_HEADER_LEN];
        header.encode(&mut head).unwrap();
        stream.extend_from_slice(&head);
        // A picture starting in the same read, so the tail is exercised.
        stream.push(0x77);

        let (kind, rest) = StreamType::split(&stream).unwrap();
        assert_eq!(kind, StreamType::Screen);
        let (decoded, pictures) = ScreenHeader::decode(rest).unwrap();
        assert_eq!(decoded, header);
        assert_eq!(pictures, &[0x77]);
    }

    #[test]
    fn the_attachment_header_is_unchanged_and_merely_sits_one_byte_later() {
        // The other half of the same promise, and the one that matters most:
        // this is the stream that already existed, so the byte in front of it
        // changes how a stream in the field is read.
        let header = AttachmentHeader {
            line: LineId(1),
            client_message_id: ClientMessageId(0xFEED),
            body: String::new(),
            replies_to: None,
            file_name: "harmônicos.png".into(),
            declared_type: "image/png".into(),
            declared_len: 4096,
            content_hash: hash("padrão azul".as_bytes()),
        };
        let frame = encode(&header).unwrap();
        let mut stream = vec![StreamType::Attachment.byte()];
        stream.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        stream.extend_from_slice(&frame);

        let (kind, rest) = StreamType::split(&stream).unwrap();
        assert_eq!(kind, StreamType::Attachment);
        // The length prefix the framing reads is exactly where it was, only one
        // byte further along, and the frame behind it still opens with the
        // protocol version.
        assert_eq!(rest.get(..4), Some(&(frame.len() as u32).to_be_bytes()[..]));
        assert_eq!(rest.get(4), Some(&PROTOCOL_VERSION));
        assert_eq!(
            decode::<AttachmentHeader>(rest.get(4..).unwrap()).unwrap(),
            header
        );
    }
}
