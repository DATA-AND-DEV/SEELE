//! Opus, configured the way `specs/03-audio.md` asks, and nothing else.
//!
//! Task M1.5. The configuration is small but every field in it is a decision
//! somebody argued about, and leaving it inline at the call site meant those
//! decisions were invisible and would be re-made — differently — by the second
//! caller.
//!
//! # The settings, and why each one
//!
//! | | | |
//! |---|---|---|
//! | 48 kHz, mono | `specs/03-audio.md` | the pipeline's rate; anything else resamples twice |
//! | `Application::Voip` | `specs/03-audio.md` | optimises for speech, not music |
//! | 20 ms frames | `specs/03-audio.md` | the latency budget in ADR 0009 is built on it |
//! | DTX on | `specs/03-audio.md` | silence costs nothing on the wire |
//! | in-band FEC **off** | ADR 0010 | measured; it cost bitrate and bought nothing at the loss rates M1.6 produces |
//!
//! # Why it lives here and not in the caller
//!
//! It did live in the caller — `magi-core::voice` — until M5, which meant
//! `magi-core` depended on `shiguredo_opus` directly while `magi-audio`, whose
//! whole job is audio, did not own the codec. Nothing broke, because there was
//! one caller. The second one is the mobile shell in M6.

use shiguredo_opus::{
    Application, Decoder, DecoderConfig, Encoder, EncoderConfig, FrameDuration, InbandFec,
};

use crate::{FRAME_SAMPLES, SAMPLE_RATE_HZ};

/// Default encoder bitrate. `specs/03-audio.md`, narrowed by ADR 0010.
pub const DEFAULT_BITRATE_BPS: u32 = 32_000;

/// The range `specs/03-audio.md` allows, after ADR 0010 narrowed it.
pub const MIN_BITRATE_BPS: u32 = 16_000;
/// Upper end of the allowed range.
pub const MAX_BITRATE_BPS: u32 = 64_000;

/// Why the codec refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// libopus would not create or configure the encoder or decoder.
    ///
    /// The library's own message goes to `tracing`: it names a C function and a
    /// numeric code, which helps a developer and tells a pilot nothing.
    Unavailable,
    /// The frame handed in was not one 20 ms frame of mono audio.
    ///
    /// A wrong length is a caller bug rather than a runtime condition, so it is
    /// worth telling apart from everything else.
    WrongFrameSize {
        /// What arrived.
        got: usize,
        /// What a frame is.
        expected: usize,
    },
    /// The payload was not decodable.
    Corrupt,
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CodecError {}

/// One outgoing voice stream.
#[derive(Debug)]
pub struct VoiceEncoder {
    inner: Encoder,
    bitrate_bps: u32,
}

impl VoiceEncoder {
    /// Builds an encoder at the given bitrate.
    ///
    /// # Errors
    ///
    /// [`CodecError::Unavailable`] if libopus refuses the configuration.
    pub fn new(bitrate_bps: u32) -> Result<Self, CodecError> {
        let bitrate_bps = bitrate_bps.clamp(MIN_BITRATE_BPS, MAX_BITRATE_BPS);
        Ok(Self {
            inner: build(bitrate_bps)?,
            bitrate_bps,
        })
    }

    /// An encoder at the default bitrate.
    ///
    /// # Errors
    ///
    /// [`CodecError::Unavailable`] if libopus refuses the configuration.
    pub fn with_defaults() -> Result<Self, CodecError> {
        Self::new(DEFAULT_BITRATE_BPS)
    }

    /// The bitrate in use.
    #[must_use]
    pub fn bitrate_bps(&self) -> u32 {
        self.bitrate_bps
    }

    /// Changes the bitrate.
    ///
    /// **This rebuilds the encoder**, because `shiguredo_opus` exposes
    /// `get_bitrate` and no setter — `OPUS_SET_BITRATE` is applied at
    /// construction and not reachable afterwards. A rebuild resets the
    /// encoder's internal state, so the first frame after a change is encoded
    /// without the prediction history of the ones before it. At 20 ms that is
    /// one frame of slightly worse quality, which is why this is acceptable for
    /// a pilot changing a setting and **would not be** for an automatic
    /// congestion response adjusting bitrate every few seconds. If that is ever
    /// wanted, this is the line that has to change first.
    ///
    /// Returns whether anything happened: asking for the current bitrate is not
    /// a reason to reset the encoder.
    ///
    /// # Errors
    ///
    /// [`CodecError::Unavailable`] if the rebuild fails. The old encoder is
    /// kept in that case, so a refused change leaves a working stream.
    pub fn set_bitrate(&mut self, bitrate_bps: u32) -> Result<bool, CodecError> {
        let wanted = bitrate_bps.clamp(MIN_BITRATE_BPS, MAX_BITRATE_BPS);
        if wanted == self.bitrate_bps {
            return Ok(false);
        }
        self.inner = build(wanted)?;
        self.bitrate_bps = wanted;
        Ok(true)
    }

    /// Encodes one 20 ms frame of mono audio.
    ///
    /// Takes `f32` directly. The pipeline is `f32` end to end, and the previous
    /// arrangement converted to `i16` first — a rounding step that existed only
    /// because the call site did not know `encode_f32` was there.
    ///
    /// An empty result is DTX deciding this frame is silence and needs no
    /// bytes. That is not an error, and the caller must not treat it as loss:
    /// telling the two apart is what M1.9 is about.
    ///
    /// # Errors
    ///
    /// [`CodecError::WrongFrameSize`] if the slice is not one frame,
    /// [`CodecError::Unavailable`] if libopus refuses it.
    pub fn encode(&mut self, frame: &[f32]) -> Result<Vec<u8>, CodecError> {
        if frame.len() != FRAME_SAMPLES {
            return Err(CodecError::WrongFrameSize {
                got: frame.len(),
                expected: FRAME_SAMPLES,
            });
        }
        self.inner.encode_f32(frame).map_err(|error| {
            tracing::warn!(%error, "opus encode failed");
            CodecError::Unavailable
        })
    }
}

/// One incoming voice stream. One per talker.
#[derive(Debug)]
pub struct VoiceDecoder {
    inner: Decoder,
}

impl VoiceDecoder {
    /// Builds a decoder.
    ///
    /// # Errors
    ///
    /// [`CodecError::Unavailable`] if libopus refuses the configuration.
    pub fn new() -> Result<Self, CodecError> {
        Decoder::new(DecoderConfig::new(SAMPLE_RATE_HZ, 1))
            .map(|inner| Self { inner })
            .map_err(|error| {
                tracing::warn!(%error, "could not create an opus decoder");
                CodecError::Unavailable
            })
    }

    /// Decodes one payload into 20 ms of mono audio.
    ///
    /// # Errors
    ///
    /// [`CodecError::Corrupt`] if the payload is not decodable. A corrupt
    /// payload is a normal event on a real network, so the caller should
    /// conceal rather than give up.
    pub fn decode(&mut self, payload: &[u8]) -> Result<Vec<f32>, CodecError> {
        self.inner.decode_f32(payload).map_err(|error| {
            tracing::debug!(%error, len = payload.len(), "undecodable opus payload");
            CodecError::Corrupt
        })
    }

    /// Invents 20 ms of audio to cover a frame that never arrived.
    ///
    /// `specs/03-audio.md` allows exactly one concealed frame before falling
    /// back to silence with a fade: packet loss concealment run for longer
    /// produces a robotic smear that is worse than a gap.
    ///
    /// # Errors
    ///
    /// [`CodecError::Unavailable`] if libopus refuses.
    pub fn conceal(&mut self) -> Result<Vec<f32>, CodecError> {
        self.inner.decode_plc_f32().map_err(|error| {
            tracing::debug!(%error, "packet loss concealment failed");
            CodecError::Unavailable
        })
    }
}

fn build(bitrate_bps: u32) -> Result<Encoder, CodecError> {
    let mut config = EncoderConfig::new(SAMPLE_RATE_HZ, 1);
    config.application = Some(Application::Voip);
    config.frame_duration = Some(FrameDuration::Ms20);
    config.bitrate = Some(bitrate_bps);
    // ADR 0010: measured, and it bought nothing at the loss rates M1.6 produces.
    config.inband_fec = Some(InbandFec::Disabled);
    config.dtx = Some(true);

    Encoder::new(config).map_err(|error| {
        tracing::warn!(%error, bitrate_bps, "could not create an opus encoder");
        CodecError::Unavailable
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of speech-like audio. Silence would be swallowed by DTX, which
    /// is correct behaviour and useless for testing the encode path.
    fn tone(amplitude: f32) -> Vec<f32> {
        (0..FRAME_SAMPLES)
            .map(|index| {
                #[allow(clippy::cast_precision_loss, reason = "one frame; well inside f32")]
                let time = index as f32 / SAMPLE_RATE_HZ as f32;
                (time * 440.0 * std::f32::consts::TAU).sin() * amplitude
            })
            .collect()
    }

    #[test]
    fn a_frame_survives_the_round_trip_with_its_shape() {
        // Not sample-exact: Opus is lossy by construction. What has to survive
        // is that something with energy went in and something with energy came
        // out, at the right length.
        let mut encoder = VoiceEncoder::with_defaults().expect("encoder");
        let mut decoder = VoiceDecoder::new().expect("decoder");

        let input = tone(0.5);
        // The first frames prime the encoder; judging quality on frame one
        // measures the warm-up, not the codec.
        for _ in 0..5 {
            let payload = encoder.encode(&input).expect("encode");
            let _ = decoder.decode(&payload);
        }

        let payload = encoder.encode(&input).expect("encode");
        assert!(!payload.is_empty(), "a tone was encoded to nothing");

        let output = decoder.decode(&payload).expect("decode");
        assert_eq!(output.len(), FRAME_SAMPLES);

        let energy: f32 = output.iter().map(|sample| sample * sample).sum();
        assert!(energy > 0.1, "the decoded frame is silent: energy {energy}");
    }

    #[test]
    fn a_frame_of_the_wrong_length_is_refused_by_name() {
        // A caller bug, and one that would otherwise surface as a libopus error
        // code that says nothing about which side got it wrong.
        let mut encoder = VoiceEncoder::with_defaults().expect("encoder");

        assert_eq!(
            encoder.encode(&[0.0; 100]),
            Err(CodecError::WrongFrameSize {
                got: 100,
                expected: FRAME_SAMPLES
            })
        );
    }

    #[test]
    fn concealment_produces_a_full_frame() {
        let mut decoder = VoiceDecoder::new().expect("decoder");
        let mut encoder = VoiceEncoder::with_defaults().expect("encoder");

        // Concealment predicts from history, so there has to be history.
        let payload = encoder.encode(&tone(0.5)).expect("encode");
        decoder.decode(&payload).expect("decode");

        let concealed = decoder.conceal().expect("conceal");
        assert_eq!(concealed.len(), FRAME_SAMPLES);
    }

    #[test]
    fn a_corrupt_payload_is_an_error_and_not_a_panic() {
        // Corruption is a normal event on a real network. The caller conceals;
        // it must never be given the chance to unwind instead.
        let mut decoder = VoiceDecoder::new().expect("decoder");

        assert_eq!(decoder.decode(&[0xFF; 16]), Err(CodecError::Corrupt));
        assert_eq!(decoder.decode(&[]), Err(CodecError::Corrupt));
    }

    #[test]
    fn the_bitrate_is_clamped_to_the_allowed_range() {
        // specs/03-audio.md gives a range and ADR 0010 narrowed it. Somebody
        // asking for 1 bit per second should get the floor, not a codec that
        // refuses to exist.
        let encoder = VoiceEncoder::new(1).expect("encoder");
        assert_eq!(encoder.bitrate_bps(), MIN_BITRATE_BPS);

        let encoder = VoiceEncoder::new(u32::MAX).expect("encoder");
        assert_eq!(encoder.bitrate_bps(), MAX_BITRATE_BPS);
    }

    #[test]
    fn setting_the_bitrate_it_already_has_changes_nothing() {
        // Every change rebuilds the encoder and resets its prediction state.
        // A no-op that resets it anyway would put a glitch in the stream for
        // free, every time a settings screen re-applied its values.
        let mut encoder = VoiceEncoder::new(24_000).expect("encoder");

        assert_eq!(encoder.set_bitrate(24_000), Ok(false));
        assert_eq!(encoder.set_bitrate(48_000), Ok(true));
        assert_eq!(encoder.bitrate_bps(), 48_000);
    }

    #[test]
    fn a_bitrate_change_keeps_the_stream_encodable() {
        let mut encoder = VoiceEncoder::with_defaults().expect("encoder");
        let input = tone(0.5);

        encoder.encode(&input).expect("before");
        encoder.set_bitrate(16_000).expect("change");
        let after = encoder.encode(&input).expect("after");

        assert!(!after.is_empty());
        assert_eq!(encoder.bitrate_bps(), 16_000);
    }

    #[test]
    fn silence_costs_nothing_on_the_wire() {
        // DTX, from specs/03-audio.md. An empty payload here is the feature
        // working — and the receiver must read it as silence, not as loss.
        // That distinction is M1.9's whole subject.
        let mut encoder = VoiceEncoder::with_defaults().expect("encoder");

        let mut smallest = usize::MAX;
        for _ in 0..25 {
            let payload = encoder.encode(&[0.0; FRAME_SAMPLES]).expect("encode");
            smallest = smallest.min(payload.len());
        }

        assert!(
            smallest <= 3,
            "DTX is not engaging on pure silence: smallest frame {smallest} bytes"
        );
    }
}
