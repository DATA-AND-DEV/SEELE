//! Demonstrates risk R3: what clock drift does to a call, and what fixes it.
//!
//! `specs/09-roadmap.md` accepts M1 on "no clicking in 10 continuous minutes".
//! Nothing in the specs says why ten minutes rather than one, and this is the
//! answer: ten minutes is roughly when two disagreeing crystals have moved a
//! whole jitter buffer apart.
//!
//! The correction is simulated faithfully rather than described. Consuming at a
//! rate scaled by the correction ratio is exactly what
//! [`magi_audio::resample::RateConverter::adjust_ratio`] does to the stream, so
//! the depth trend below is the trend the real pipeline will show.
//!
//! Run with: `cargo run --release --package magi-audio --example clock_drift`

use magi_audio::drift::DriftTracker;
use magi_audio::jitter::{Decision, JitterBuffer, JitterConfig};
use magi_audio::netsim::{drifting_stream, Network, NetworkProfile};
use magi_audio::{FRAME_MS, SAMPLE_RATE_HZ};

/// Ten minutes at 50 frames a second.
const FRAMES: usize = 30_000;
const SEED: u64 = 20_260_807;

struct Outcome {
    measured_ppm: f64,
    depth_at_1min_ms: f64,
    depth_at_9min_ms: f64,
    concealed: u64,
    starved: u64,
}

/// One minute in, past the warm-up.
const SAMPLE_EARLY: usize = 3_000;
/// Nine minutes in, still well inside the stream.
const SAMPLE_LATE: usize = 27_000;

/// Runs a drifting sender into a jitter buffer, optionally correcting.
///
/// The loop stops with the stream rather than running past it: ticking after the
/// last arrival drains the buffer to zero no matter what the clocks were doing,
/// which measures the end of the recording instead of the drift.
fn run(drift_ppm: f64, profile: NetworkProfile, correct: bool) -> Outcome {
    let frames = drifting_stream(FRAMES, drift_ppm);
    let mut network = Network::new(profile, SEED);
    let mut arrivals = Vec::new();
    for frame in frames {
        network.transmit(frame, &mut arrivals);
    }
    network.sort_arrivals(&mut arrivals);

    let mut buffer: JitterBuffer<u16> = JitterBuffer::new(JitterConfig::default());
    let mut tracker = DriftTracker::new();
    let mut clock_ms = 0.0_f64;
    let mut index = 0;
    let mut early = 0.0_f64;
    let mut late = 0.0_f64;
    let mut starved = 0_u64;

    for tick in 0..SAMPLE_LATE + 1 {
        while arrivals
            .get(index)
            .is_some_and(|frame| frame.arrived_at_ms <= clock_ms)
        {
            if let Some(frame) = arrivals.get(index) {
                buffer.push(frame.seq, frame.timestamp, frame.arrived_at_ms, frame.seq);
                let sent_ms = f64::from(frame.timestamp) / f64::from(SAMPLE_RATE_HZ) * 1000.0;
                tracker.observe(frame.arrived_at_ms - sent_ms, frame.arrived_at_ms);
            }
            index += 1;
        }

        if matches!(buffer.tick(), Decision::Starved) {
            starved += 1;
        }
        if tick == SAMPLE_EARLY {
            early = buffer.depth_ms();
        }
        if tick == SAMPLE_LATE {
            late = buffer.depth_ms();
        }

        // Consuming at a rate scaled by the correction ratio is what the
        // resampler does to the stream: a ratio above one means fewer
        // milliseconds of real time per frame consumed. Without correction the
        // tick is exactly one frame of local time and the clocks pull apart.
        let ratio = if correct {
            tracker.correction_ratio().unwrap_or(1.0)
        } else {
            1.0
        };
        clock_ms += f64::from(FRAME_MS) / ratio;
    }

    Outcome {
        measured_ppm: tracker.drift_ppm(),
        depth_at_1min_ms: early,
        depth_at_9min_ms: late,
        concealed: buffer.metrics().frames_concealed,
        starved,
    }
}

fn main() {
    println!("{FRAMES} frames = 10 minutes, seed {SEED}, LAN profile\n");
    println!(
        "{:>10} {:>10} {:>11} {:>11} {:>11} {:>9} {:>8}",
        "drift", "corrected", "measured", "depth @1min", "depth @9min", "creep", "starved"
    );
    println!("{}", "-".repeat(78));

    for drift_ppm in [0.0, 50.0, 100.0, -100.0, 200.0] {
        for correct in [false, true] {
            let outcome = run(drift_ppm, NetworkProfile::lan(), correct);
            let creep = outcome.depth_at_9min_ms - outcome.depth_at_1min_ms;
            println!(
                "{:>9.0}p {:>10} {:>10.1}p {:>9.1}ms {:>9.1}ms {:>8.1}ms {:>8}",
                drift_ppm,
                if correct { "yes" } else { "no" },
                outcome.measured_ppm,
                outcome.depth_at_1min_ms,
                outcome.depth_at_9min_ms,
                creep,
                outcome.starved,
            );
        }
        println!();
    }

    println!("drift   = the sender's crystal error, in parts per million");
    println!("creep   = how much extra latency accumulated over ten minutes");
    println!("starved = playout slots with nothing to play (a slow sender drains)");
    println!();
    println!("A crystal specified to +/-50 ppm is ordinary. Two ends can be 100 ppm apart");
    println!("and both be within spec. Uncorrected, that is why specs/09 asks for a soak");
    println!("of ten minutes rather than one.");

    // The other half of R3: concealment caused by drift rather than by the
    // network. On a perfect LAN there is no loss at all, so anything concealed
    // here was manufactured by the clocks.
    let uncorrected = run(200.0, NetworkProfile::lan(), false);
    let corrected = run(200.0, NetworkProfile::lan(), true);
    println!();
    println!("at 200 ppm on a link that drops nothing:");
    println!(
        "  uncorrected: {} concealed, {} starved",
        uncorrected.concealed, uncorrected.starved
    );
    println!(
        "  corrected:   {} concealed, {} starved",
        corrected.concealed, corrected.starved
    );
}
