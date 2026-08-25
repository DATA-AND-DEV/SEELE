//! Cost of one 20 ms frame through the codec, in both directions.
//!
//! `specs/10-convencoes.md`: "measure before optimising" and "a performance
//! regression on the audio path is a bug, not a matter of taste". This is the
//! baseline that statement needs in order to mean anything.
//!
//! It exists for one concrete reason. ADR 0008 chose `shiguredo_opus` for being
//! maintained and buildable everywhere, knowing its `encode`/`decode`/`decode_plc`
//! return `Vec` and therefore allocate on every call. That is legal — the codec
//! runs on the processing thread, not in the `cpal` callback, so the no-allocation
//! rule of `specs/03-audio.md` does not apply — but it is 2 allocations per frame
//! per source, 50 times a second. This benchmark is what decides whether the
//! plan B in ADR 0008 (our own zero-allocation wrapper) is worth 3 points of M1.
//!
//! Run with `cargo bench --package seele-audio`.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use seele_audio::resample::RateConverter;
use seele_audio::{FRAME_SAMPLES, SAMPLE_RATE_HZ};
use shiguredo_opus::{
    Application, Decoder, DecoderConfig, Encoder, EncoderConfig, FrameDuration, InbandFec,
};

fn spec_encoder() -> Encoder {
    let mut config = EncoderConfig::new(SAMPLE_RATE_HZ, 1);
    config.application = Some(Application::Voip);
    config.frame_duration = Some(FrameDuration::Ms20);
    config.bitrate = Some(32_000);
    config.inband_fec = Some(InbandFec::Disabled);
    config.dtx = Some(true);
    #[allow(clippy::expect_used, reason = "a benchmark that cannot start is a bug")]
    Encoder::new(config).expect("encoder should initialise")
}

fn spec_decoder() -> Decoder {
    #[allow(clippy::expect_used, reason = "a benchmark that cannot start is a bug")]
    Decoder::new(DecoderConfig::new(SAMPLE_RATE_HZ, 1)).expect("decoder should initialise")
}

fn tone_frame() -> Vec<i16> {
    (0..FRAME_SAMPLES)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE_HZ as f32;
            ((t * 440.0 * std::f32::consts::TAU).sin() * 8000.0) as i16
        })
        .collect()
}

fn codec_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("opus 20 ms frame");

    // Real time per frame is 20 ms. Anything approaching that on one source is a
    // fire; with N sources the budget is 20 ms divided by N.
    group.bench_function("encode", |b| {
        let mut encoder = spec_encoder();
        let frame = tone_frame();
        b.iter(|| black_box(encoder.encode(black_box(&frame))));
    });

    group.bench_function("decode", |b| {
        let mut encoder = spec_encoder();
        let mut decoder = spec_decoder();
        #[allow(clippy::unwrap_used, reason = "setup outside the measured region")]
        let payload = encoder.encode(&tone_frame()).unwrap();
        b.iter(|| black_box(decoder.decode(black_box(&payload))));
    });

    // The jitter buffer calls this on every missing frame, so its cost is paid
    // exactly when the network is already misbehaving.
    group.bench_function("decode_plc", |b| {
        let mut encoder = spec_encoder();
        let mut decoder = spec_decoder();
        #[allow(clippy::unwrap_used, reason = "setup outside the measured region")]
        let payload = encoder.encode(&tone_frame()).unwrap();
        #[allow(clippy::unwrap_used, reason = "setup outside the measured region")]
        let _ = decoder.decode(&payload).unwrap();
        b.iter(|| black_box(decoder.decode_plc()));
    });

    group.finish();
}

fn resampling(c: &mut Criterion) {
    // Task M1.4. The question is whether a sinc resampler is affordable per
    // source: a client in a 15-person voice room runs one on capture and, once mixing
    // lands, potentially more on playback. Budget is 20 ms of real time per
    // frame.
    let mut group = c.benchmark_group("resample 20 ms frame");
    let frame = tone_frame_f32();

    group.bench_function("44100->48000", |b| {
        #[allow(clippy::unwrap_used, reason = "setup outside the measured region")]
        let mut converter = RateConverter::new(44_100, SAMPLE_RATE_HZ).unwrap();
        let mut out = Vec::with_capacity(FRAME_SAMPLES * 2);
        b.iter(|| {
            out.clear();
            let _ = converter.push(black_box(&frame), &mut out);
            black_box(out.len())
        });
    });

    group.bench_function("48000->44100", |b| {
        #[allow(clippy::unwrap_used, reason = "setup outside the measured region")]
        let mut converter = RateConverter::new(SAMPLE_RATE_HZ, 44_100).unwrap();
        let mut out = Vec::with_capacity(FRAME_SAMPLES * 2);
        b.iter(|| {
            out.clear();
            let _ = converter.push(black_box(&frame), &mut out);
            black_box(out.len())
        });
    });

    // The common case: matching rates must cost effectively nothing, or the
    // passthrough shortcut is not earning its complexity.
    group.bench_function("passthrough", |b| {
        #[allow(clippy::unwrap_used, reason = "setup outside the measured region")]
        let mut converter = RateConverter::new(SAMPLE_RATE_HZ, SAMPLE_RATE_HZ).unwrap();
        let mut out = Vec::with_capacity(FRAME_SAMPLES * 2);
        b.iter(|| {
            out.clear();
            let _ = converter.push(black_box(&frame), &mut out);
            black_box(out.len())
        });
    });

    group.finish();
}

fn tone_frame_f32() -> Vec<f32> {
    (0..FRAME_SAMPLES)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE_HZ as f32;
            (t * 440.0 * std::f32::consts::TAU).sin() * 0.25
        })
        .collect()
}

criterion_group!(benches, codec_path, resampling);
criterion_main!(benches);
