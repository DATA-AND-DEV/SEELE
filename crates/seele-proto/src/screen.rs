//! What opens a screen transmission's own stream.
//!
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` §3.1:
//! **one unidirectional QUIC stream per transmission, and never a datagram.**
//! The reason is measured rather than argued — `spikes/tela-no-transporte` put
//! video on `send_datagram` and lost 16,1% of the voice with 2,16 s of delay,
//! because `quinn-proto` puts voice and video in the same FIFO and drops the
//! **oldest** when it fills. Shrinking that buffer inverts the failure instead
//! of fixing it: 98,1% of the voice discarded before leaving the machine. With
//! the video on a stream the voice loses 0,1%, because `quinn-proto` writes
//! `DATAGRAM` frames before `STREAM` frames in every packet.
//!
//! # The opening header, and nothing else
//!
//! §3.6 asks for a header "saying what it is (monitor or window), the
//! resolution and the codec — versioned by the first byte, like every control
//! frame". That is this, and the frames follow it to the end of the stream.
//!
//! It is a **fixed layout read with [`slice::get`]**, not a `postcard` message
//! like [`crate::attachment::AttachmentHeader`], and the difference is what the
//! two carry: an attachment header holds strings and needs a length in front of
//! it, while every field here is a small number of known width. Eleven bytes
//! read into an array on the stack means a receiver allocates nothing at all
//! before it knows what it is being sent, which is the posture
//! `specs/08-seguranca.md` asks of every network input.
//!
//! # There is no frame framing here, and that is not an oversight
//!
//! §3.6 is deliberately short, and it stops at the opening header. What
//! separates one encoded picture from the next on this stream is not decided
//! yet and is not decided here — see the report of this task.
//!
//! # `ssrc` is not used here
//!
//! §3.6 again, and it is the one thing that section puts in bold: [`crate::ids::Ssrc`] is
//! the **audio** source identifier the server assigns on voice room entry, and every
//! client keeps a table of `ssrc` → person built out of it. A screen is not a
//! voice, so it gets [`ScreenId`], and nobody has to rewrite that table to make
//! room for something that was never a talker.
//!
//! # Byte order
//!
//! Big-endian, for the reason [`crate::media`] already gives: it is what every
//! other real-time media protocol writes, so a capture opened in Wireshark
//! reads the way an engineer expects.

use thiserror::Error;

use crate::ids::ScreenId;
use crate::version::{negotiate, PROTOCOL_VERSION};

/// Bytes of opening header before the first encoded picture.
pub const SCREEN_HEADER_LEN: usize = 11;

/// Longest side a transmission may declare, in pixels.
///
/// The spec's §6 item 10 puts "nothing above 1080p" among the things v1 does
/// not do, and §5 fixes the list the interface offers at 1080p, 720p and 540p.
/// 1920 is that ceiling expressed as a **side** rather than as a width, which
/// is what makes a monitor stood on end shareable at all: 1080 × 1920 is a
/// portrait screen, not an oversized one, and a rule written as "width ≤ 1920,
/// height ≤ 1080" would refuse exactly the person who bought the second
/// monitor to read code on.
pub const MAX_SCREEN_SIDE: u16 = 1920;

/// Most pixels a transmission may declare.
///
/// [`MAX_SCREEN_SIDE`] alone would admit 1920 × 1920, which is 1,8 times the
/// pixels of 1080p and outside everything §2 measured. Two limits rather than
/// one, because the pair says what a single number cannot: any shape, up to the
/// area the CPU measurements in `spikes/tela-no-codec` actually cover.
pub const MAX_SCREEN_PIXELS: u32 = 1920 * 1080;

/// What is being transmitted.
///
/// §4: the same choice for us and three different things for the three systems
/// — on Wayland the compositor picks and hands back only what the person chose,
/// so this is what the sender **learned** it got, not what it asked for.
///
/// The codes are append-only, like the variants of every enum on this wire: a
/// number is never reused and a new kind goes at the end, so a peer one version
/// older refuses what it does not know instead of drawing a window as a
/// monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenSource {
    /// A whole display.
    Monitor,
    /// A single window.
    Window,
}

impl ScreenSource {
    /// The byte this travels as.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Monitor => 0,
            Self::Window => 1,
        }
    }

    /// Reads the byte back, or `None` for a code this build does not know.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Monitor),
            1 => Some(Self::Window),
            _ => None,
        }
    }
}

/// What the pictures are encoded with.
///
/// One variant today, and the enum exists precisely because there will be a
/// second: §2 chooses H.264 baseline through `shiguredo_openh264` and names VP8
/// as **the successor**, for the day its binding leaves canary and somebody
/// accepts paying for nasm. A codec declared on the wire is what lets that day
/// arrive without a flag day — the receiver refuses a codec it cannot decode
/// instead of feeding a decoder bytes meant for another one.
///
/// Codes are append-only, for the reason [`ScreenSource`] gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenCodec {
    /// H.264 baseline profile. §2, and ADR 0008's family of bindings.
    H264Baseline,
}

impl ScreenCodec {
    /// The byte this travels as.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::H264Baseline => 0,
        }
    }

    /// Reads the byte back, or `None` for a codec this build cannot decode.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::H264Baseline),
            _ => None,
        }
    }
}

/// Why an opening header could not be read, or could not be written.
///
/// Enumerated, per `specs/02-protocolo.md`, and each one carries the numbers a
/// shell would need to write its own sentence — ADR 0012. The `Display` text is
/// for `tracing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ScreenError {
    /// Fewer bytes than a header.
    #[error("stream opened with {len} bytes, shorter than the {SCREEN_HEADER_LEN}-byte header")]
    TooShort {
        /// Length received.
        len: usize,
    },

    /// A version this build does not speak.
    #[error("stream announces protocol {found}, this build implements {expected}")]
    UnsupportedVersion {
        /// Version in the header.
        found: u8,
        /// Version this build implements.
        expected: u8,
    },

    /// A source kind this build does not know.
    #[error("stream announces source kind {code}, which this build does not know")]
    UnknownSource {
        /// The code received.
        code: u8,
    },

    /// A codec this build cannot decode.
    #[error("stream announces codec {code}, which this build cannot decode")]
    UnknownCodec {
        /// The code received.
        code: u8,
    },

    /// A picture size outside what this build will carry.
    ///
    /// Carries both sides rather than a flag, because the two ways to fail it
    /// are different sentences: a side too long is "that screen is bigger than
    /// this product carries", and too many pixels is "that shape is".
    #[error("stream announces {width}×{height}, outside the {MAX_SCREEN_SIDE}-pixel side and {MAX_SCREEN_PIXELS}-pixel area this build carries")]
    BadResolution {
        /// Declared width.
        width: u16,
        /// Declared height.
        height: u16,
    },
}

/// The first eleven bytes of a screen transmission's stream.
///
/// ```text
/// ┌─────────┬────────────┬────────────┬───────────┬───────────┬────────────┐
/// │ ver (1) │ screen (4) │ source (1) │ codec (1) │ width (2) │ height (2) │
/// └─────────┴────────────┴────────────┴───────────┴───────────┴────────────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenHeader {
    /// Protocol version. Always [`PROTOCOL_VERSION`] on the way out.
    pub version: u8,
    /// Which transmission this stream carries.
    ///
    /// Assigned by the server when the transmission is announced
    /// ([`crate::control::ServerMessage::ScreenShareStarted`]) and never chosen
    /// by the sender, which is the rule [`crate::ids::Ssrc`] already follows in
    /// `specs/08-seguranca.md`: an identifier a client picks is an identifier a
    /// client can pick somebody else's. It is here so that a receiver can bind
    /// the stream it was just offered to the transmission it was told about,
    /// and refuse a stream that matches nothing.
    pub screen: ScreenId,
    /// Monitor or window.
    pub source: ScreenSource,
    /// What the pictures are encoded with.
    pub codec: ScreenCodec,
    /// Width in pixels, as the sender is actually encoding it.
    ///
    /// A **number, not a name.** §5 fixes the list the interface offers at
    /// 1080p, 720p and 540p, and that list is a list of buttons: it changes the
    /// day somebody adds a fourth. What crosses the wire is what the encoder is
    /// doing, so no peer has to hold the same list of pretty names to
    /// understand a picture.
    ///
    /// It is also **not a promise**. §5: "a tela não promete a escolha" — the
    /// resolution holds and the frame rate cedes, between 5 and 30 per second,
    /// so this is what a receiver will get and the frame rate is not declared
    /// here at all. Writing a rate into a header written once would be writing
    /// down the one number the design says will move.
    pub width: u16,
    /// Height in pixels, as the sender is actually encoding it.
    pub height: u16,
}

impl ScreenHeader {
    /// Refuses a header this build will not carry.
    ///
    /// Asked on the way out as well as on the way in, so a size that came from
    /// a capture nobody bounded cannot travel further than one a receiver would
    /// accept.
    ///
    /// # Errors
    ///
    /// Returns [`ScreenError::BadResolution`] for a side of zero, a side over
    /// [`MAX_SCREEN_SIDE`], or an area over [`MAX_SCREEN_PIXELS`].
    pub fn check(&self) -> Result<(), ScreenError> {
        let bad = ScreenError::BadResolution {
            width: self.width,
            height: self.height,
        };
        // Zero is refused with the same breath as too large, for the reason
        // `check_server_icon` gives about a PNG declaring a side of nothing: it
        // is far more often a capture that failed than a deliberate choice, and
        // there is no screen behind it either way.
        if self.width == 0 || self.height == 0 {
            return Err(bad);
        }
        if self.width > MAX_SCREEN_SIDE || self.height > MAX_SCREEN_SIDE {
            return Err(bad);
        }
        if u32::from(self.width) * u32::from(self.height) > MAX_SCREEN_PIXELS {
            return Err(bad);
        }
        Ok(())
    }

    /// Writes the header into the first [`SCREEN_HEADER_LEN`] bytes of `out`.
    ///
    /// Returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// Returns [`ScreenError::TooShort`] if `out` cannot hold a header, or
    /// [`ScreenError::BadResolution`] if [`Self::check`] refuses it.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ScreenError> {
        self.check()?;
        let Some(slot) = out.get_mut(..SCREEN_HEADER_LEN) else {
            return Err(ScreenError::TooShort { len: out.len() });
        };
        let (version, rest) = slot.split_at_mut(1);
        let (screen, rest) = rest.split_at_mut(4);
        let (source, rest) = rest.split_at_mut(1);
        let (codec, rest) = rest.split_at_mut(1);
        let (width, height) = rest.split_at_mut(2);

        version.copy_from_slice(&[self.version]);
        screen.copy_from_slice(&self.screen.get().to_be_bytes());
        source.copy_from_slice(&[self.source.code()]);
        codec.copy_from_slice(&[self.codec.code()]);
        width.copy_from_slice(&self.width.to_be_bytes());
        height.copy_from_slice(&self.height.to_be_bytes());
        Ok(SCREEN_HEADER_LEN)
    }

    /// Reads the opening header off the front of a stream.
    ///
    /// Returns the header and whatever followed it in the same read, which is
    /// the first bytes of the first picture — a QUIC stream is a byte stream,
    /// so a receiver that read more than eleven bytes must not be made to
    /// forget the rest.
    ///
    /// Every read is [`slice::get`]. `specs/08-seguranca.md` names this crate
    /// as the surface that receives untrusted bytes, and a panicking indexation
    /// here would turn a truncated stream — which is just a laptop closing —
    /// into a dead process.
    ///
    /// # Errors
    ///
    /// Returns [`ScreenError`] for every malformed case. Never panics, whatever
    /// the bytes are.
    pub fn decode(bytes: &[u8]) -> Result<(Self, &[u8]), ScreenError> {
        let Some(header) = bytes.get(..SCREEN_HEADER_LEN) else {
            return Err(ScreenError::TooShort { len: bytes.len() });
        };
        let rest = bytes.get(SCREEN_HEADER_LEN..).unwrap_or_default();

        // The version before anything else, so a layout this build has never
        // seen is refused rather than read as one it has.
        let version = header.first().copied().unwrap_or_default();
        negotiate(version).map_err(|_| ScreenError::UnsupportedVersion {
            found: version,
            expected: PROTOCOL_VERSION,
        })?;

        let screen = ScreenId(read_u32(header, 1));
        let source_code = header.get(5).copied().unwrap_or_default();
        let Some(source) = ScreenSource::from_code(source_code) else {
            return Err(ScreenError::UnknownSource { code: source_code });
        };
        let codec_code = header.get(6).copied().unwrap_or_default();
        let Some(codec) = ScreenCodec::from_code(codec_code) else {
            return Err(ScreenError::UnknownCodec { code: codec_code });
        };

        let decoded = Self {
            version,
            screen,
            source,
            codec,
            width: read_u16(header, 7),
            height: read_u16(header, 9),
        };
        decoded.check()?;
        Ok((decoded, rest))
    }
}

/// Reads four big-endian bytes, or zero if they are not there.
///
/// The bounds can only fail if [`SCREEN_HEADER_LEN`] and the offsets disagree,
/// which would be a bug here rather than bad input — but this is the
/// untrusted-input surface, so it returns a value instead of panicking either
/// way. The same shape [`crate::media`] uses, and for the same reason.
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map_or(0, u32::from_be_bytes)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    bytes
        .get(offset..offset + 2)
        .and_then(|slice| slice.try_into().ok())
        .map_or(0, u16::from_be_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> ScreenHeader {
        ScreenHeader {
            version: PROTOCOL_VERSION,
            screen: ScreenId(0x00C0_FFEE),
            source: ScreenSource::Window,
            codec: ScreenCodec::H264Baseline,
            width: 1280,
            height: 720,
        }
    }

    #[test]
    fn the_opening_header_is_exactly_eleven_bytes() {
        // Once per transmission rather than fifty times a second, so the number
        // is not a budget the way `media::HEADER_LEN` is. It is pinned because
        // the receiver reads a fixed array off the stream: a field added here
        // without moving this line reads the first bytes of the first picture
        // as a resolution.
        assert_eq!(SCREEN_HEADER_LEN, 1 + 4 + 1 + 1 + 2 + 2);

        let mut buffer = [0_u8; 32];
        assert_eq!(header().encode(&mut buffer), Ok(SCREEN_HEADER_LEN));
    }

    #[test]
    fn a_header_round_trips_and_keeps_what_followed_it() {
        // The tail matters: a QUIC stream is a byte stream, and a receiver that
        // read the header and the start of the first picture in one syscall
        // must not be made to forget the second half.
        let mut buffer = [0_u8; 32];
        header().encode(&mut buffer).unwrap();
        buffer[SCREEN_HEADER_LEN] = 0x77;

        let (decoded, rest) = ScreenHeader::decode(&buffer).unwrap();
        assert_eq!(decoded, header());
        assert_eq!(rest.first(), Some(&0x77));
    }

    #[test]
    fn the_wire_layout_is_big_endian_and_in_the_documented_order() {
        // Pinned against the byte level, not just against the decoder. A round
        // trip alone would pass with width and height swapped, and the first
        // person to notice would be somebody watching a stretched screen.
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut buffer).unwrap();

        assert_eq!(buffer[0], PROTOCOL_VERSION, "version first");
        assert_eq!(
            &buffer[1..5],
            &[0x00, 0xC0, 0xFF, 0xEE],
            "screen, big-endian"
        );
        assert_eq!(buffer[5], 1, "source: window");
        assert_eq!(buffer[6], 0, "codec: H.264 baseline");
        assert_eq!(&buffer[7..9], &[0x05, 0x00], "width");
        assert_eq!(&buffer[9..11], &[0x02, 0xD0], "height");
    }

    #[test]
    fn a_truncated_header_is_rejected_not_misread() {
        // The laptop that closed mid-transmission. `specs/08-seguranca.md`
        // names this surface for fuzzing: every one of these must be a refusal
        // and none of them a panic.
        let mut whole = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut whole).unwrap();
        for len in 0..SCREEN_HEADER_LEN {
            assert_eq!(
                ScreenHeader::decode(&whole[..len]),
                Err(ScreenError::TooShort { len }),
                "accepted a {len}-byte opening"
            );
        }
    }

    #[test]
    fn a_foreign_version_is_refused_with_the_numbers() {
        // Versioned by the first byte like every control frame, and through the
        // same `negotiate`, so the compatibility window is decided in one place
        // rather than twice with a difference nobody notices until a release.
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut buffer).unwrap();
        buffer[0] = PROTOCOL_VERSION.wrapping_add(7);

        assert_eq!(
            ScreenHeader::decode(&buffer),
            Err(ScreenError::UnsupportedVersion {
                found: PROTOCOL_VERSION.wrapping_add(7),
                expected: PROTOCOL_VERSION,
            })
        );
    }

    #[test]
    fn an_unknown_source_or_codec_is_refused_rather_than_guessed() {
        // The whole reason both travel as codes with an append-only meaning. A
        // build that met a codec it does not have and started its H.264 decoder
        // anyway would draw garbage and blame the network; §2 names VP8 as the
        // successor, so the second codec is a matter of when.
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut buffer).unwrap();

        let mut strange_source = buffer;
        strange_source[5] = 9;
        assert_eq!(
            ScreenHeader::decode(&strange_source),
            Err(ScreenError::UnknownSource { code: 9 })
        );

        let mut strange_codec = buffer;
        strange_codec[6] = 4;
        assert_eq!(
            ScreenHeader::decode(&strange_codec),
            Err(ScreenError::UnknownCodec { code: 4 })
        );
    }

    #[test]
    fn a_screen_stood_on_end_is_carried_and_not_refused() {
        // The trap in writing the ceiling as a width. 1080 × 1920 is a portrait
        // monitor — the second screen somebody bought to read code on — and a
        // rule of "width ≤ 1920, height ≤ 1080" refuses exactly that person
        // while accepting every landscape screen, which is a bug that looks
        // like a policy.
        let portrait = ScreenHeader {
            width: 1080,
            height: 1920,
            ..header()
        };
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        portrait.encode(&mut buffer).unwrap();
        assert_eq!(ScreenHeader::decode(&buffer).unwrap().0, portrait);
    }

    #[test]
    fn a_resolution_outside_the_ceiling_is_refused_in_both_directions() {
        // §6 item 10 puts anything above 1080p outside v1, and §2 measured the
        // CPU only up to it. Refused on the way out so we cannot send one, and
        // on the way in so a peer cannot skip the check by writing the eleven
        // bytes by hand — the same pair every bounded field in `control` gets.
        for (width, height) in [
            (0, 720),
            (1280, 0),
            (1921, 1080),
            (1080, 1921),
            // Both sides inside the ceiling and 1,8 times the pixels of 1080p:
            // the case the side limit alone cannot refuse.
            (1920, 1920),
        ] {
            let outside = ScreenHeader {
                width,
                height,
                ..header()
            };
            let mut buffer = [0_u8; SCREEN_HEADER_LEN];
            assert_eq!(
                outside.encode(&mut buffer),
                Err(ScreenError::BadResolution { width, height }),
                "encoded {width}×{height}"
            );

            // And now the way a hostile peer would build it, skipping the
            // encoder entirely.
            let mut frame = [0_u8; SCREEN_HEADER_LEN];
            header().encode(&mut frame).unwrap();
            frame[7..9].copy_from_slice(&width.to_be_bytes());
            frame[9..11].copy_from_slice(&height.to_be_bytes());
            assert_eq!(
                ScreenHeader::decode(&frame),
                Err(ScreenError::BadResolution { width, height }),
                "decoded a hand-rolled {width}×{height}"
            );
        }
    }

    #[test]
    fn every_offered_resolution_fits_the_header() {
        // §5 fixes the list at 1080p, 720p and 540p, and it travels as numbers
        // precisely so this list can change without the wire changing. This
        // test is the list of today meeting the wire of today; a fourth entry
        // added tomorrow needs no byte moved, which is the property being
        // recorded.
        for (width, height) in [(1920, 1080), (1280, 720), (960, 540)] {
            let offered = ScreenHeader {
                width,
                height,
                ..header()
            };
            let mut buffer = [0_u8; SCREEN_HEADER_LEN];
            offered.encode(&mut buffer).unwrap();
            assert_eq!(ScreenHeader::decode(&buffer).unwrap().0, offered);
        }
    }

    #[test]
    fn encoding_into_a_small_buffer_fails_instead_of_truncating() {
        let mut buffer = [0_u8; 4];
        assert_eq!(
            header().encode(&mut buffer),
            Err(ScreenError::TooShort { len: 4 })
        );
    }

    #[test]
    fn a_screen_is_not_an_audio_source() {
        // §3.6, and the one thing it puts in bold. `Ssrc` is assigned on voice room
        // entry and every client keeps a table of `ssrc` → person built out of
        // it; passing one where the other belongs is a bug the compiler catches
        // for free, and this test documents the guarantee — `ScreenId(1)` does
        // not compile where an `Ssrc` is wanted.
        fn takes_screen(id: ScreenId) -> u32 {
            id.get()
        }
        assert_eq!(takes_screen(header().screen), 0x00C0_FFEE);
    }
}
