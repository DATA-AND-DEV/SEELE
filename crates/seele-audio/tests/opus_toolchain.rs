//! Proof that the Opus toolchain links and works on this platform.
//!
//! This test exists for a build reason, not an audio reason. `specs/09-roadmap.md`
//! puts the codec in M1, but ADR 0008 moved the *toolchain* to M0: discovering in
//! week two of the riskiest milestone that libopus does not compile on Windows
//! would burn M1 on a problem that teaches nothing about audio.
//!
//! It covers what M1 actually depends on:
//!
//! - encode and decode round-trip at the exact parameters in `specs/03-audio.md`;
//! - packet loss concealment, which the jitter buffer calls on a missing frame;
//! - the encoder's algorithmic lookahead, which is an input to the latency
//!   budget in ADR 0009.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use seele_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use shiguredo_opus::{
    Application, Decoder, DecoderConfig, Encoder, EncoderConfig, FrameDuration, InbandFec,
};

/// The codec parameters from the table in `specs/03-audio.md`.
///
/// In-band FEC is deliberately disabled: it only helps if the decoder is handed
/// packet N+1 when N is lost, which forces the jitter buffer to hold one extra
/// frame — 20 ms out of a 60 ms LAN budget. See ADR 0010.
fn spec_encoder(bitrate_bps: u32) -> Encoder {
    let mut config = EncoderConfig::new(SAMPLE_RATE_HZ, 1);
    config.application = Some(Application::Voip);
    config.frame_duration = Some(FrameDuration::Ms20);
    config.bitrate = Some(bitrate_bps);
    config.inband_fec = Some(InbandFec::Disabled);
    config.dtx = Some(true);
    Encoder::new(config).expect("libopus encoder should initialise at the parameters in specs/03")
}

fn spec_decoder() -> Decoder {
    let config = DecoderConfig::new(SAMPLE_RATE_HZ, 1);
    Decoder::new(config).expect("libopus decoder should initialise")
}

/// One 20 ms frame of a 440 Hz tone, as signed 16-bit mono at 48 kHz.
fn tone_frame() -> Vec<i16> {
    (0..FRAME_SAMPLES)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE_HZ as f32;
            ((t * 440.0 * std::f32::consts::TAU).sin() * 8000.0) as i16
        })
        .collect()
}

/// Rough energy measure, used only to tell signal from silence.
fn rms(samples: &[i16]) -> f64 {
    let sum: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    (sum / samples.len() as f64).sqrt()
}

#[test]
fn encode_decode_round_trip_preserves_signal() {
    let input = tone_frame();
    let mut encoder = spec_encoder(32_000);
    let mut decoder = spec_decoder();

    let payload = encoder
        .encode(&input)
        .expect("encoding one 20 ms frame should succeed");

    // specs/02-protocolo.md budgets ~80 bytes of payload at 32 kbps. A frame that
    // encodes to almost nothing means the encoder emitted comfort noise instead of
    // the tone, and the assertion below would then pass for the wrong reason.
    assert!(
        (10..=400).contains(&payload.len()),
        "unexpected packet size for one voice frame: {} bytes",
        payload.len()
    );

    let output = decoder
        .decode(&payload)
        .expect("decoding the frame we just encoded should succeed");

    assert_eq!(
        output.len(),
        FRAME_SAMPLES,
        "a {FRAME_MS} ms frame must decode back to {FRAME_SAMPLES} samples at 48 kHz"
    );

    // Opus is lossy, so this is not a sample-for-sample comparison. It asserts the
    // codec carried the signal rather than returning silence.
    assert!(
        rms(&output) > rms(&input) * 0.5,
        "decoded frame lost most of its energy: in {:.0}, out {:.0}",
        rms(&input),
        rms(&output)
    );
}

#[test]
fn packet_loss_concealment_produces_a_full_frame() {
    // specs/03-audio.md: "missing frame → Opus PLC for one frame; from the second
    // on, silence with fade". The jitter buffer depends on this path, so prove it
    // links and returns a whole frame rather than a gap.
    let mut encoder = spec_encoder(32_000);
    let mut decoder = spec_decoder();

    let payload = encoder.encode(&tone_frame()).unwrap();
    decoder.decode(&payload).unwrap();

    let concealed = decoder
        .decode_plc()
        .expect("PLC decode should succeed after a decoded frame");

    assert_eq!(
        concealed.len(),
        FRAME_SAMPLES,
        "concealment must yield a full frame, or the output stream gains a hole"
    );
}

#[test]
fn encoder_honours_the_bitrate_range_from_the_spec() {
    // specs/03-audio.md gives 24–48 kbps adaptive, and separately says the rate
    // drops to 16 kbps under >5% loss — 16 being outside its own stated range
    // (contradiction C5 in docs/plano-m0-m1.md). All four are exercised here so
    // that whichever way C5 is resolved, the codec is known to accept it.
    //
    // Note: shiguredo_opus sets bitrate at construction and exposes no setter, so
    // adaptive bitrate will need a rebuilt encoder or an upstream patch. That is
    // an M2 problem — ADR 0010 defers the adaptation policy to M2 anyway.
    for bitrate in [16_000, 24_000, 32_000, 48_000] {
        let mut encoder = spec_encoder(bitrate);

        assert_eq!(
            encoder.get_bitrate().unwrap(),
            bitrate,
            "encoder did not take the configured bitrate"
        );

        let payload = encoder.encode(&tone_frame()).unwrap();
        assert!(!payload.is_empty(), "encoding failed at {bitrate} bps");
    }
}

#[test]
fn encoder_lookahead_is_within_the_latency_budget() {
    // ADR 0009 accepts a two-tier budget: < 60 ms with the jitter buffer at its
    // 20 ms floor, < 90 ms with the 40 ms default. Encoder lookahead is a fixed
    // cost inside that budget, so record it rather than assume it.
    let encoder = spec_encoder(32_000);
    let lookahead_samples = encoder.get_lookahead().unwrap();
    let lookahead_ms = f64::from(lookahead_samples) / f64::from(SAMPLE_RATE_HZ) * 1000.0;

    println!("opus encoder lookahead: {lookahead_samples} samples ({lookahead_ms:.2} ms)");

    assert!(
        lookahead_ms < 10.0,
        "lookahead of {lookahead_ms:.2} ms would eat a sixth of the LAN budget"
    );
}
