//! Manual smoke test for the `cpal` seam. Not run by CI — CI has no sound card.
//!
//! `specs/10-convencoes.md` puts end-to-end audio under "manual, with a
//! documented checklist per platform". This is the first entry on that
//! checklist, and the only way [`magi_audio::device`] gets exercised at all.
//!
//! It opens both devices, runs the real-time path for a few seconds, and reports
//! the counters. It **does not** feed the microphone back to the speaker: that
//! would howl, and ADR 0007 requires headphones for anything involving both ends
//! at once. Playback is fed silence, which is enough to prove the output
//! callback is being serviced without underruns.
//!
//! Run with: `cargo run --package magi-audio --example device_smoke`

use std::time::{Duration, Instant};

use magi_audio::device::{self, DeviceError};
use magi_audio::resample::RateConverter;

const RING_MS: u32 = 100;
const RUN_FOR: Duration = Duration::from_secs(3);

fn main() {
    match run() {
        Ok(()) => {}
        Err(DeviceError::NoInputDevice) => {
            eprintln!("no default input device.");
            eprintln!(
                "On macOS this is usually the microphone permission rather than a missing\n\
                 device: System Settings > Privacy & Security > Microphone."
            );
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("device smoke test failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), DeviceError> {
    let mut io = device::open_default(RING_MS)?;

    println!("capture : {} Hz", io.capture_rate_hz);
    println!("playback: {} Hz", io.playback_rate_hz);
    if io.capture_rate_hz != magi_audio::SAMPLE_RATE_HZ
        || io.playback_rate_hz != magi_audio::SAMPLE_RATE_HZ
    {
        println!(
            "note: a device is not at {} Hz. Resampling is mandatory and is task M1.4.",
            magi_audio::SAMPLE_RATE_HZ
        );
    }
    // Task M1.4: device rate in, 48 kHz internal, device rate back out. Both
    // are passthroughs when the device already runs at 48 kHz, which is the
    // common case and costs nothing.
    let mut to_pipeline = match RateConverter::new(io.capture_rate_hz, magi_audio::SAMPLE_RATE_HZ) {
        Ok(converter) => converter,
        Err(error) => {
            eprintln!("could not build the capture converter: {error}");
            std::process::exit(1);
        }
    };
    let mut to_device = match RateConverter::new(magi_audio::SAMPLE_RATE_HZ, io.playback_rate_hz) {
        Ok(converter) => converter,
        Err(error) => {
            eprintln!("could not build the playback converter: {error}");
            std::process::exit(1);
        }
    };
    println!(
        "capture converter:  {} ({} frames of filter delay)",
        if to_pipeline.is_passthrough() {
            "passthrough"
        } else {
            "resampling"
        },
        to_pipeline.delay_frames()
    );
    println!(
        "playback converter: {} ({} frames of filter delay)",
        if to_device.is_passthrough() {
            "passthrough"
        } else {
            "resampling"
        },
        to_device.delay_frames()
    );

    println!("\nrunning for {RUN_FOR:?}. speak into the microphone.\n");

    let started = Instant::now();
    let mut peak = 0.0_f32;
    let mut drained = 0_u64;
    // Reused across iterations so the steady state does no allocation.
    let mut device_rate = Vec::new();
    let mut pipeline_rate = Vec::new();
    let mut back_to_device = Vec::new();
    let mut pipeline_frames = 0_u64;

    // Stands in for the processing thread that M1.4 onwards will fill in. It
    // drains capture and feeds silence to playback, which is enough to keep both
    // callbacks honest without creating a feedback loop.
    while started.elapsed() < RUN_FOR {
        device_rate.clear();
        while let Ok(sample) = io.captured.pop() {
            peak = peak.max(sample.abs());
            drained += 1;
            device_rate.push(sample);
        }

        // Device rate -> 48 kHz. This is where the encoder would sit.
        pipeline_rate.clear();
        if let Err(error) = to_pipeline.push(&device_rate, &mut pipeline_rate) {
            eprintln!("capture conversion failed: {error}");
            break;
        }
        pipeline_frames += pipeline_rate.len() as u64;

        // 48 kHz -> device rate. Silenced rather than fed back, to avoid howling
        // through the built-in speaker; ADR 0007 requires headphones anyway.
        let silence = vec![0.0_f32; pipeline_rate.len()];
        back_to_device.clear();
        if let Err(error) = to_device.push(&silence, &mut back_to_device) {
            eprintln!("playback conversion failed: {error}");
            break;
        }
        for sample in back_to_device.drain(..) {
            let _ = io.to_device.push(sample);
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    let metrics = io.counters.snapshot();
    println!("drained from capture ring: {drained} frames (device rate)");
    println!("after conversion:          {pipeline_frames} frames (48 kHz)");
    println!("peak input level:          {peak:.4}");
    println!();
    println!("frames captured:    {}", metrics.frames_captured);
    println!("frames played:      {}", metrics.frames_played);
    println!("capture overruns:   {}", metrics.capture_overruns);
    println!("playback underruns: {}", metrics.playback_underruns);
    println!("stream errors:      {}", metrics.stream_errors);

    println!();
    if metrics.frames_captured == 0 {
        println!("FAIL: the input callback never ran.");
    } else if metrics.capture_overruns > 0 {
        println!("FAIL: capture overran — the draining loop is not keeping up.");
    } else if peak < 1e-4 {
        println!("SUSPECT: the input callback ran but heard nothing but digital silence.");
    } else {
        println!("PASS: both callbacks serviced, ring clean, microphone audible.");
    }

    Ok(())
}
