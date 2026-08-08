//! What the jitter buffer actually achieves on each simulated network.
//!
//! The unit tests assert invariants; this reports behaviour. It answers the two
//! questions the latency budget in ADR 0009 depends on: how much delay does the
//! buffer settle at on each path, and how much of the network's loss reaches the
//! listener.
//!
//! Run with: `cargo run --release --package magi-audio --example jitter_profiles`

use magi_audio::jitter::{Decision, JitterBuffer, JitterConfig};
use magi_audio::netsim::{run, NetworkProfile};
use magi_audio::FRAME_MS;

/// Ten minutes, matching the soak in `specs/09-roadmap.md`.
const FRAMES: usize = 30_000;
const SEED: u64 = 20_260_807;

fn main() {
    let profiles: [(&str, NetworkProfile); 6] = [
        ("perfect", NetworkProfile::perfect()),
        ("lan", NetworkProfile::lan()),
        ("wifi", NetworkProfile::wifi()),
        ("regional", NetworkProfile::regional()),
        ("mobile_poor", NetworkProfile::mobile_poor()),
        (
            "acceptance 5%",
            NetworkProfile::acceptance_five_percent_loss(),
        ),
    ];

    println!("{FRAMES} frames, {} s, seed {SEED}\n", FRAMES / 50);
    println!(
        "{:<15} {:>9} {:>9} {:>10} {:>9} {:>8} {:>7}",
        "profile", "net loss", "heard", "target ms", "jitter ms", "conceal", "late"
    );
    println!("{}", "-".repeat(74));

    for (name, profile) in profiles {
        let (arrivals, net) = run(profile, SEED, FRAMES);
        let mut buffer: JitterBuffer<u16> = JitterBuffer::new(JitterConfig::default());

        let mut clock_ms = 0.0_f64;
        let mut index = 0;
        let mut played = 0_u64;

        for _ in 0..(FRAMES + 100) {
            while arrivals
                .get(index)
                .is_some_and(|frame| frame.arrived_at_ms <= clock_ms)
            {
                if let Some(frame) = arrivals.get(index) {
                    buffer.push(frame.seq, frame.timestamp, frame.arrived_at_ms, frame.seq);
                }
                index += 1;
            }
            if matches!(buffer.tick(), Decision::Play(_)) {
                played += 1;
            }
            clock_ms += f64::from(FRAME_MS);
        }

        let metrics = buffer.metrics();
        let _ = played;

        println!(
            "{:<15} {:>8.2}% {:>8.2}% {:>10.1} {:>9.1} {:>8} {:>7}",
            name,
            net.loss_fraction() * 100.0,
            metrics.loss_fraction() * 100.0,
            metrics.target_ms,
            metrics.jitter_ms,
            metrics.frames_concealed,
            metrics.late_discards,
        );
    }

    // Task M1.9. A talker who pauses sends nothing, and a buffer that reads
    // every gap as loss reports a broken connection every time the room goes
    // quiet. On a perfect link the loss column below must read exactly zero.
    println!();
    println!("--- bursty talker on a perfect link (M1.9) ---");
    let frames = magi_audio::netsim::talker_stream(25, 50, 40);
    let mut buffer: JitterBuffer<u16> = JitterBuffer::new(JitterConfig::default());
    let mut clock_ms = 0.0_f64;
    let mut index = 0;
    for _ in 0..3_200 {
        while frames
            .get(index)
            .is_some_and(|frame| frame.sent_at_ms <= clock_ms)
        {
            if let Some(frame) = frames.get(index) {
                buffer.push(frame.seq, frame.timestamp, frame.sent_at_ms, frame.seq);
            }
            index += 1;
        }
        let _ = buffer.tick();
        clock_ms += f64::from(FRAME_MS);
    }
    let talker = buffer.metrics();
    println!("  frames played:   {}", talker.frames_played);
    println!("  silence (comfort): {}", talker.frames_comfort);
    println!("  concealed:       {}", talker.frames_concealed);
    println!("  reported loss:   {:.2}%", talker.loss_fraction() * 100.0);
    println!();
    println!("  Counting that silence as loss would report");
    println!(
        "  {:.1}% loss on a link that dropped nothing.",
        talker.frames_comfort as f64 / (talker.frames_played + talker.frames_comfort) as f64
            * 100.0
    );

    println!();
    println!("net loss = what the network dropped");
    println!("heard    = slots the listener got concealment or silence for");
    println!("target   = adaptive depth the buffer settled at (specs/03: 20-200 ms)");
    println!();
    println!("The gap between the two loss columns is what the buffer costs or saves:");
    println!("reordering it recovers lowers 'heard'; late arrivals it must drop raise it.");
}
