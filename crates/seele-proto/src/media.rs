//! The media datagram header.
//!
//! `specs/02-protocolo.md` fixes the layout:
//!
//! ```text
//! ┌─────────┬──────────┬────────────┬─────────┬──────────────┐
//! │ ver (1) │ ssrc (4) │ seq (2)    │ ts (4)  │ opus payload │
//! └─────────┴──────────┴────────────┴─────────┴──────────────┘
//! ```
//!
//! > **Overhead:** 11 bytes of header for ~80 bytes of payload at 32 kbps.
//! > Acceptable. Do not add fields without demonstrated need.
//!
//! This lives in `seele-proto` rather than in the M1 transport spike because
//! `specs/01-arquitetura.md` makes this crate the owner of every byte that
//! crosses the network. The sockets around it are throwaway; the layout is not.
//!
//! # Untrusted input
//!
//! `specs/08-seguranca.md` requires that every network input be size-limited
//! before allocating, and names this crate's parsers as the fuzzing target:
//! "it is the surface that receives untrusted network bytes". Decoding here
//! borrows rather than copies, allocates nothing, and returns an enumerated
//! error for every malformed case. See `fuzz/fuzz_targets/media_header.rs`.
//!
//! # Byte order
//!
//! Big-endian, which `specs/02-protocolo.md` does not state. Chosen because it
//! is what every other real-time media protocol on the wire uses, so a packet
//! capture opened in Wireshark reads the way an engineer expects.

use thiserror::Error;

/// O número destes onze bytes: **o número da voz**.
///
/// **Sobe só quando o layout mudar**, e não quando o protocolo sobe. São duas
/// perguntas, e a tela já tem o número dela (`screen::SCREEN_HEADER_VERSION`).
///
/// Até a 0.15 este byte carregava o `PROTOCOL_VERSION` de quem falava, e quem
/// ouvia exigia igualdade estrita: cada subida de protocolo derrubava a voz
/// entre versões, apesar de os onze bytes nunca terem mudado. O servidor repassa
/// o datagrama intacto, então quem fala e quem ouve têm de concordar neste
/// número, e não no do controle. Adendo de 08/10/2026, §7, decisão D-fio-5.
pub const MEDIA_HEADER_VERSION: u8 = 1;

/// Bytes of header before the Opus payload.
pub const HEADER_LEN: usize = 11;

/// Largest Opus payload this build will accept in one datagram.
///
/// libopus caps a single frame at 1275 bytes. Anything larger is malformed, and
/// rejecting it before allocating is the rule from `specs/08-seguranca.md`.
pub const MAX_PAYLOAD_LEN: usize = 1275;

/// Largest datagram this build will accept.
pub const MAX_DATAGRAM_LEN: usize = HEADER_LEN + MAX_PAYLOAD_LEN;

/// Why a datagram could not be decoded.
///
/// Enumerated, per `specs/02-protocolo.md`. A shell matches on the variant; the
/// `Display` text is for `tracing`. See ADR 0012.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MediaError {
    /// Fewer bytes than a header.
    #[error("datagram is {len} bytes, shorter than the {HEADER_LEN}-byte header")]
    TooShort {
        /// Length received.
        len: usize,
    },

    /// More bytes than any legitimate frame.
    #[error("datagram is {len} bytes, longer than the {MAX_DATAGRAM_LEN}-byte maximum")]
    TooLong {
        /// Length received.
        len: usize,
    },

    /// A version this build does not speak.
    #[error("datagram announces protocol {found}, this build implements {expected}")]
    UnsupportedVersion {
        /// Version in the datagram.
        found: u8,
        /// Version this build implements.
        expected: u8,
    },

    /// A header with no audio behind it.
    #[error("datagram carries a header but no payload")]
    EmptyPayload,
}

/// The fixed part of a media datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaHeader {
    /// O número do layout. Sempre [`MEDIA_HEADER_VERSION`] na saída.
    pub version: u8,
    /// Source identifier, assigned by the server on voice room entry.
    ///
    /// `specs/08-seguranca.md`: never accepted from the client. The server binds
    /// it to the connection and forwards the datagram intact, which is what
    /// keeps the payload untouched and E2EE an increment rather than a rewrite.
    pub ssrc: u32,
    /// Sequence number, wrapping. Counts **transmitted** packets, so a gap here
    /// is packet loss.
    pub seq: u16,
    /// Timestamp in samples at 48 kHz. Counts **elapsed** samples, so a jump
    /// here without a sequence gap is silence rather than loss — see
    /// `seele-audio`'s jitter buffer and task M1.9.
    pub timestamp: u32,
}

impl MediaHeader {
    /// Writes the header into the first [`HEADER_LEN`] bytes of `out`.
    ///
    /// Returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError::TooShort`] if `out` cannot hold a header.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, MediaError> {
        let Some(slot) = out.get_mut(..HEADER_LEN) else {
            return Err(MediaError::TooShort { len: out.len() });
        };
        let (version, rest) = slot.split_at_mut(1);
        let (ssrc, rest) = rest.split_at_mut(4);
        let (seq, timestamp) = rest.split_at_mut(2);

        version.copy_from_slice(&[self.version]);
        ssrc.copy_from_slice(&self.ssrc.to_be_bytes());
        seq.copy_from_slice(&self.seq.to_be_bytes());
        timestamp.copy_from_slice(&self.timestamp.to_be_bytes());
        Ok(HEADER_LEN)
    }

    /// Writes header and payload into `out`, returning the datagram length.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError::TooLong`] if the payload exceeds
    /// [`MAX_PAYLOAD_LEN`], [`MediaError::EmptyPayload`] if there is none, or
    /// [`MediaError::TooShort`] if `out` is too small.
    pub fn encode_datagram(&self, payload: &[u8], out: &mut [u8]) -> Result<usize, MediaError> {
        if payload.is_empty() {
            return Err(MediaError::EmptyPayload);
        }
        if payload.len() > MAX_PAYLOAD_LEN {
            return Err(MediaError::TooLong {
                len: HEADER_LEN + payload.len(),
            });
        }
        let total = HEADER_LEN + payload.len();
        let Some(slot) = out.get_mut(..total) else {
            return Err(MediaError::TooShort { len: out.len() });
        };
        self.encode(slot)?;
        let Some(tail) = slot.get_mut(HEADER_LEN..) else {
            return Err(MediaError::TooShort { len: out.len() });
        };
        tail.copy_from_slice(payload);
        Ok(total)
    }

    /// Splits a received datagram into header and payload.
    ///
    /// The payload is **borrowed**, not copied: this is on the receive path of
    /// every frame from every talker, fifty times a second each.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] for every malformed case. Never panics, whatever
    /// the bytes are.
    pub fn decode(datagram: &[u8]) -> Result<(Self, &[u8]), MediaError> {
        if datagram.len() > MAX_DATAGRAM_LEN {
            return Err(MediaError::TooLong {
                len: datagram.len(),
            });
        }
        let Some(header) = datagram.get(..HEADER_LEN) else {
            return Err(MediaError::TooShort {
                len: datagram.len(),
            });
        };
        let Some(payload) = datagram.get(HEADER_LEN..) else {
            return Err(MediaError::TooShort {
                len: datagram.len(),
            });
        };
        if payload.is_empty() {
            return Err(MediaError::EmptyPayload);
        }

        // Read the version before anything else, so a future layout change is
        // rejected rather than misparsed.
        let version = header.first().copied().unwrap_or_default();
        if version != MEDIA_HEADER_VERSION {
            return Err(MediaError::UnsupportedVersion {
                found: version,
                expected: MEDIA_HEADER_VERSION,
            });
        }

        let ssrc = read_u32(header, 1);
        let seq = read_u16(header, 5);
        let timestamp = read_u32(header, 7);

        Ok((
            Self {
                version,
                ssrc,
                seq,
                timestamp,
            },
            payload,
        ))
    }
}

/// Reads four big-endian bytes, or zero if they are not there.
///
/// The bounds can only fail if [`HEADER_LEN`] and the offsets disagree, which
/// would be a bug here rather than bad input — but this is the untrusted-input
/// surface, so it returns a value instead of panicking either way.
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

    fn header() -> MediaHeader {
        MediaHeader {
            version: MEDIA_HEADER_VERSION,
            ssrc: 0xDEAD_BEEF,
            seq: 0x1234,
            timestamp: 0x0009_6000,
        }
    }

    /// **A voz carrega o número dos onze bytes, e não o do protocolo.**
    ///
    /// Até a 0.15, o primeiro byte do datagrama era o `PROTOCOL_VERSION` de
    /// quem falava, conferido com igualdade estrita: cada subida de protocolo
    /// derrubava a voz entre versões, apesar de estes onze bytes nunca terem
    /// mudado. A tela já tinha passado por isso (`screen::SCREEN_HEADER_VERSION`).
    /// Adendo de 08/10/2026, §7, decisão D-fio-5.
    #[test]
    fn a_voz_carrega_o_numero_dos_onze_bytes_e_nao_o_do_protocolo() {
        let mut saida = [0_u8; HEADER_LEN + 1];
        let cabecalho = MediaHeader {
            version: MEDIA_HEADER_VERSION,
            ssrc: 1,
            seq: 2,
            timestamp: 3,
        };
        cabecalho.encode_datagram(&[0xAB], &mut saida).unwrap();
        assert_eq!(
            saida[0], 1,
            "o byte 0 da voz voltou a ser a versão do protocolo"
        );
        assert!(MediaHeader::decode(&saida).is_ok());

        let mut do_protocolo = saida;
        do_protocolo[0] = crate::PROTOCOL_VERSION;
        assert!(
            matches!(
                MediaHeader::decode(&do_protocolo),
                Err(MediaError::UnsupportedVersion { found, .. }) if found == crate::PROTOCOL_VERSION
            ),
            "a voz passou a aceitar o número do protocolo, e cada subida voltaria a derrubá-la"
        );
    }

    #[test]
    fn the_header_is_exactly_eleven_bytes() {
        // specs/02-protocolo.md budgets 11 bytes against ~80 of payload and says
        // not to add fields without demonstrated need. A silent growth here is a
        // silent tax on every frame from every talker.
        assert_eq!(HEADER_LEN, 1 + 4 + 2 + 4);

        let mut buffer = [0_u8; 64];
        assert_eq!(header().encode(&mut buffer), Ok(11));
    }

    #[test]
    fn a_datagram_round_trips() {
        let payload = [1_u8, 2, 3, 4, 5];
        let mut buffer = [0_u8; 64];
        let len = header().encode_datagram(&payload, &mut buffer).unwrap();

        let (decoded, decoded_payload) = MediaHeader::decode(&buffer[..len]).unwrap();
        assert_eq!(decoded, header());
        assert_eq!(decoded_payload, &payload);
    }

    #[test]
    fn the_wire_layout_is_big_endian_and_in_the_documented_order() {
        // Pinned against the byte level, not just against the decoder. A
        // round-trip test alone would happily pass with the fields swapped.
        let mut buffer = [0_u8; HEADER_LEN];
        header().encode(&mut buffer).unwrap();

        assert_eq!(buffer[0], MEDIA_HEADER_VERSION, "version first");
        assert_eq!(&buffer[1..5], &[0xDE, 0xAD, 0xBE, 0xEF], "ssrc, big-endian");
        assert_eq!(&buffer[5..7], &[0x12, 0x34], "seq");
        assert_eq!(&buffer[7..11], &[0x00, 0x09, 0x60, 0x00], "timestamp");
    }

    #[test]
    fn overhead_matches_what_the_spec_budgeted() {
        // 32 kbps at 20 ms is 80 bytes. specs/02-protocolo.md calls 11 over 80
        // acceptable; this records the actual ratio so a future field addition
        // has to argue with a number.
        let payload_bytes = 32_000 / 8 * 20 / 1000;
        assert_eq!(payload_bytes, 80);
        let overhead = HEADER_LEN as f64 / (HEADER_LEN + payload_bytes) as f64;
        assert!(
            overhead < 0.13,
            "header overhead is {:.1}%",
            overhead * 100.0
        );
    }

    #[test]
    fn a_truncated_datagram_is_rejected_not_misread() {
        for len in 0..HEADER_LEN {
            let datagram = vec![0_u8; len];
            assert_eq!(
                MediaHeader::decode(&datagram),
                Err(MediaError::TooShort { len }),
                "accepted a {len}-byte datagram"
            );
        }
    }

    #[test]
    fn a_header_with_no_audio_is_rejected() {
        let mut buffer = [0_u8; HEADER_LEN];
        header().encode(&mut buffer).unwrap();
        assert_eq!(
            MediaHeader::decode(&buffer),
            Err(MediaError::EmptyPayload),
            "a header alone carries nothing to play"
        );
    }

    #[test]
    fn an_oversized_datagram_is_rejected_before_anything_is_allocated() {
        // specs/08-seguranca.md: every network input is size-limited before
        // allocating. libopus caps a frame at 1275 bytes; more is malformed.
        let datagram = vec![0_u8; MAX_DATAGRAM_LEN + 1];
        assert_eq!(
            MediaHeader::decode(&datagram),
            Err(MediaError::TooLong {
                len: MAX_DATAGRAM_LEN + 1
            })
        );
    }

    #[test]
    fn a_foreign_version_is_refused_with_the_numbers() {
        let mut buffer = [0_u8; 32];
        let len = header().encode_datagram(&[9], &mut buffer).unwrap();
        buffer[0] = MEDIA_HEADER_VERSION.wrapping_add(7);

        assert_eq!(
            MediaHeader::decode(&buffer[..len]),
            Err(MediaError::UnsupportedVersion {
                found: MEDIA_HEADER_VERSION.wrapping_add(7),
                expected: MEDIA_HEADER_VERSION,
            })
        );
    }

    #[test]
    fn encoding_into_a_small_buffer_fails_instead_of_truncating() {
        let mut buffer = [0_u8; 4];
        assert!(matches!(
            header().encode(&mut buffer),
            Err(MediaError::TooShort { .. })
        ));
        assert!(matches!(
            header().encode_datagram(&[1, 2, 3], &mut buffer),
            Err(MediaError::TooShort { .. })
        ));
    }

    #[test]
    fn an_empty_payload_is_refused_on_the_way_out_too() {
        let mut buffer = [0_u8; 64];
        assert_eq!(
            header().encode_datagram(&[], &mut buffer),
            Err(MediaError::EmptyPayload)
        );
    }

    #[test]
    fn extreme_field_values_survive_a_round_trip() {
        // Sequence wraps every 21 minutes and the timestamp every 24.8 hours,
        // so the ends of both ranges are ordinary values, not edge cases.
        let extremes = MediaHeader {
            version: MEDIA_HEADER_VERSION,
            ssrc: u32::MAX,
            seq: u16::MAX,
            timestamp: u32::MAX,
        };
        let mut buffer = [0_u8; 64];
        let len = extremes.encode_datagram(&[7], &mut buffer).unwrap();
        let (decoded, _) = MediaHeader::decode(&buffer[..len]).unwrap();
        assert_eq!(decoded, extremes);
    }
}
