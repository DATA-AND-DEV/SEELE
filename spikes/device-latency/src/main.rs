//! M1.1 — gate spike: how much latency do the audio devices cost on their own?
//!
//! **Throwaway.** This crate dies with M1. It is outside the workspace, it is not
//! held to the conventions in `specs/10-convencoes.md`, and nothing may depend on
//! it.
//!
//! # What it answers
//!
//! ADR 0009 accepts a two-tier latency budget and assumes ~10 ms for the capture
//! buffer and ~10 ms for the playback buffer. Those two numbers were guesses. If
//! the real device round-trip alone exceeds ~35 ms with default buffers, the
//! 60 ms LAN target of `specs/00-visao-geral.md` is unreachable without
//! per-backend low-latency configuration, and the scope of M1 changes.
//!
//! # Subcommands
//!
//! - `devices` — enumerate and report configs and buffer ranges. Silent, and
//!   needs no microphone permission.
//! - `acoustic` — **plays a short chirp through the speaker and records it**,
//!   then cross-correlates to recover the true round-trip. Needs microphone
//!   permission. Use headphones off, speaker on, in a quiet room.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, InputCallbackInfo, OutputCallbackInfo, StreamInstant};

/// Chirp length. Long enough for decent energy through a laptop speaker, short
/// enough that the correlation peak stays sharp.
const CHIRP_MS: u32 = 5;
/// Sweep range. Above room rumble, below the speaker's rolloff.
const CHIRP_LOW_HZ: f32 = 1_000.0;
const CHIRP_HIGH_HZ: f32 = 8_000.0;
/// How long to record before giving up on hearing the chirp.
const LISTEN_SECS: u32 = 2;
/// Silence before the chirp, so the streams settle and any startup glitch is
/// not mistaken for the signal.
const SETTLE_SECS: u32 = 1;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("devices") => report_devices(),
        Some("acoustic") => acoustic(std::env::args().nth(2).and_then(|a| a.parse().ok())),
        _ => {
            eprintln!("usage: spike-device-latency <devices|acoustic>");
            std::process::exit(2);
        }
    }
}

fn ms(frames: u32, rate: u32) -> f64 {
    f64::from(frames) / f64::from(rate) * 1000.0
}

fn report_devices() {
    let host = cpal::default_host();
    println!("host: {}\n", host.id().name());

    if let Some(device) = host.default_input_device() {
        describe(&device, true);
    } else {
        println!("no default input device");
    }
    println!();
    if let Some(device) = host.default_output_device() {
        describe(&device, false);
    } else {
        println!("no default output device");
    }
}

fn describe(device: &cpal::Device, input: bool) {
    let kind = if input { "INPUT" } else { "OUTPUT" };
    let name = device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| "<unnamed>".into());
    println!("{kind}: {name}");

    let config = if input {
        device.default_input_config()
    } else {
        device.default_output_config()
    };

    match config {
        Ok(config) => {
            let rate = config.sample_rate();
            println!("  default: {} ch @ {} Hz, {:?}", config.channels(), rate, config.sample_format());
            match config.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => {
                    println!(
                        "  buffer range: {min}..{max} frames  ({:.2} ms .. {:.2} ms)",
                        ms(*min, rate),
                        ms(*max, rate)
                    );
                    println!(
                        "  -> a floor of {min} frames is {:.2} ms of unavoidable buffering on this side",
                        ms(*min, rate)
                    );
                }
                cpal::SupportedBufferSize::Unknown => {
                    println!("  buffer range: unknown (the backend will not say)");
                }
            }
        }
        Err(error) => println!("  could not read default config: {error}"),
    }
}

/// Linear chirp, normalised, with a raised-cosine envelope so the ends do not
/// click — a click would smear the correlation peak.
fn chirp(rate: u32) -> Vec<f32> {
    let n = (rate * CHIRP_MS / 1000) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let progress = i as f32 / n as f32;
            let freq = CHIRP_LOW_HZ + (CHIRP_HIGH_HZ - CHIRP_LOW_HZ) * progress;
            let envelope = 0.5 - 0.5 * (progress * std::f32::consts::TAU).cos();
            (t * freq * std::f32::consts::TAU).sin() * envelope * 0.6
        })
        .collect()
}

/// Normalised cross-correlation. Returns the offset of the best match and its
/// score in 0..1, so a weak match can be reported as weak instead of guessed at.
fn best_offset(haystack: &[f32], needle: &[f32]) -> (usize, f32) {
    let needle_energy: f32 = needle.iter().map(|s| s * s).sum::<f32>().sqrt();
    let mut best = (0_usize, 0.0_f32);

    for offset in 0..haystack.len().saturating_sub(needle.len()) {
        let window = &haystack[offset..offset + needle.len()];
        let dot: f32 = window.iter().zip(needle).map(|(a, b)| a * b).sum();
        let window_energy: f32 = window.iter().map(|s| s * s).sum::<f32>().sqrt();
        if window_energy > 1e-9 {
            let score = dot / (window_energy * needle_energy);
            if score > best.1 {
                best = (offset, score);
            }
        }
    }
    best
}

fn acoustic(forced_frames: Option<u32>) {
    let host = cpal::default_host();
    let Some(input_device) = host.default_input_device() else {
        eprintln!("no default input device");
        std::process::exit(1);
    };
    let Some(output_device) = host.default_output_device() else {
        eprintln!("no default output device");
        std::process::exit(1);
    };

    let in_config = match input_device.default_input_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("cannot read input config: {error}");
            std::process::exit(1);
        }
    };
    let out_config = match output_device.default_output_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("cannot read output config: {error}");
            std::process::exit(1);
        }
    };

    let mut in_stream_config: cpal::StreamConfig = in_config.clone().into();
    let mut out_stream_config: cpal::StreamConfig = out_config.clone().into();
    if let Some(frames) = forced_frames {
        in_stream_config.buffer_size = BufferSize::Fixed(frames);
        out_stream_config.buffer_size = BufferSize::Fixed(frames);
        println!("forcing buffer size to {frames} frames on both sides");
    }

    let in_rate = in_config.sample_rate();
    let out_rate = out_config.sample_rate();
    let in_channels = in_config.channels() as usize;
    let out_channels = out_config.channels() as usize;

    println!(
        "input : {} ch @ {} Hz\noutput: {} ch @ {} Hz",
        in_channels, in_rate, out_channels, out_rate
    );
    if in_rate != 48_000 || out_rate != 48_000 {
        println!(
            "note: a device is not at 48 kHz — resampling is mandatory, which is \
             exactly gap G7 in docs/plano-m0-m1.md"
        );
    }
    println!("\nplaying a {CHIRP_MS} ms chirp in {SETTLE_SECS}s. keep the room quiet.\n");

    // Anchors, each written exactly once from a callback. A mutex in a real-time
    // callback is against the rules of specs/03-audio.md — acceptable here only
    // because this is throwaway probe code and the lock is uncontended after the
    // single write. Nothing in seele-audio may copy this link_state.
    let emitted_at: Arc<Mutex<Option<StreamInstant>>> = Arc::new(Mutex::new(None));
    let captured_at: Arc<Mutex<Option<StreamInstant>>> = Arc::new(Mutex::new(None));

    let recording: Arc<Mutex<Vec<f32>>> =
        Arc::new(Mutex::new(Vec::with_capacity((in_rate * LISTEN_SECS) as usize)));
    let overruns = Arc::new(AtomicUsize::new(0));

    // ---- input ----
    let rec = Arc::clone(&recording);
    let cap_at = Arc::clone(&captured_at);
    let over = Arc::clone(&overruns);
    let limit = (in_rate * LISTEN_SECS) as usize;

    let input_stream = input_device.build_input_stream(
        in_stream_config.clone(),
        move |data: &[f32], info: &InputCallbackInfo| {
            if let Ok(mut anchor) = cap_at.lock() {
                if anchor.is_none() {
                    *anchor = Some(info.timestamp().capture);
                }
            }
            if let Ok(mut buffer) = rec.lock() {
                if buffer.len() >= limit {
                    over.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                // Take channel 0 only; interleaved input.
                buffer.extend(data.iter().step_by(in_channels).copied());
            }
        },
        |error: cpal::Error| eprintln!("input stream error: {error}"),
        None,
    );

    let input_stream = match input_stream {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("could not open the input stream: {error}");
            eprintln!(
                "\nOn macOS this is usually the microphone permission (TCC). Grant it to the\n\
                 terminal application in System Settings > Privacy & Security > Microphone,\n\
                 then run again."
            );
            std::process::exit(1);
        }
    };

    // ---- output ----
    let tone = chirp(out_rate);
    let emit_at = Arc::clone(&emitted_at);
    let armed = Arc::new(AtomicBool::new(false));
    let armed_cb = Arc::clone(&armed);
    let mut cursor = 0_usize;

    let output_stream = output_device.build_output_stream(
        out_stream_config.clone(),
        move |data: &mut [f32], info: &OutputCallbackInfo| {
            data.fill(0.0);
            if !armed_cb.load(Ordering::Relaxed) || cursor >= tone.len() {
                return;
            }
            let frames = data.len() / out_channels;
            for frame in 0..frames {
                let Some(sample) = tone.get(cursor) else { break };
                if cursor == 0 {
                    // Playback instant of the very first chirp sample, offset by
                    // where in this buffer it lands.
                    if let Ok(mut anchor) = emit_at.lock() {
                        let offset = Duration::from_secs_f64(frame as f64 / f64::from(out_rate));
                        *anchor = info.timestamp().playback.checked_add(offset);
                    }
                }
                for channel in 0..out_channels {
                    if let Some(slot) = data.get_mut(frame * out_channels + channel) {
                        *slot = *sample;
                    }
                }
                cursor += 1;
            }
        },
        |error: cpal::Error| eprintln!("output stream error: {error}"),
        None,
    );

    let output_stream = match output_stream {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("could not open the output stream: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = input_stream.play() {
        eprintln!("input play failed: {error}");
        std::process::exit(1);
    }
    if let Err(error) = output_stream.play() {
        eprintln!("output play failed: {error}");
        std::process::exit(1);
    }

    std::thread::sleep(Duration::from_secs(u64::from(SETTLE_SECS)));
    armed.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_secs(u64::from(LISTEN_SECS)));

    drop(input_stream);
    drop(output_stream);

    // ---- analysis ----
    let Ok(recorded) = recording.lock() else {
        eprintln!("recording buffer poisoned");
        std::process::exit(1);
    };
    let Ok(emitted) = emitted_at.lock() else { return };
    let Ok(captured) = captured_at.lock() else { return };

    let (Some(emitted), Some(captured)) = (emitted.as_ref(), captured.as_ref()) else {
        eprintln!("never got both stream anchors — chirp may not have been emitted");
        std::process::exit(1);
    };

    let needle = chirp(in_rate);
    let (offset, score) = best_offset(&recorded, &needle);

    println!("recorded {} frames ({:.2} s)", recorded.len(), recorded.len() as f64 / f64::from(in_rate));
    if overruns.load(Ordering::Relaxed) > 0 {
        println!("note: {} input callbacks dropped after the buffer filled", overruns.load(Ordering::Relaxed));
    }
    println!("correlation peak: {score:.3} at frame {offset}");

    if score < 0.3 {
        println!(
            "\nWEAK MATCH. The chirp was probably not heard. Check that output is the\n\
             built-in speaker (not headphones), the volume is up, and the room is quiet."
        );
        std::process::exit(1);
    }

    // Time from the first captured frame to the frame where the chirp starts.
    let into_recording = Duration::from_secs_f64(offset as f64 / f64::from(in_rate));

    // Both anchors come from the same cpal clock domain, so they are comparable.
    let Some(round_trip) = captured
        .checked_add(into_recording)
        .and_then(|heard_at| heard_at.checked_duration_since(*emitted))
    else {
        eprintln!("could not difference the stream instants");
        std::process::exit(1);
    };

    let total_ms = round_trip.as_secs_f64() * 1000.0;

    println!("\n================ M1.1 RESULT ================");
    println!("device round-trip (speaker -> air -> mic): {total_ms:.2} ms");
    println!("  includes: output buffer, DAC, ~1 ms of air, ADC, input buffer");
    println!("  excludes: codec, network, jitter buffer");
    println!();
    println!("ADR 0009 assumed ~10 ms capture + ~10 ms playback = 20 ms.");
    if total_ms > 35.0 {
        println!("VERDICT: OVER THE 35 ms GATE. Stop and revisit scope before M1.7.");
    } else {
        println!("VERDICT: within the 35 ms gate. The ADR 0009 budget holds.");
    }
    println!("============================================");
}
