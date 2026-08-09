//! Characterises the network profiles in [`seele_audio::netsim`].
//!
//! A simulator nobody has calibrated is worse than no simulator: the jitter
//! buffer would be tuned against conditions that do not exist. This prints what
//! each profile actually produces over a long run, so the numbers in
//! `docs/m1-medicoes.md` can be checked rather than trusted.
//!
//! Run with: `cargo run --release --package seele-audio --example netsim_profiles`

use seele_audio::netsim::{run, ArrivedFrame, NetworkProfile};

/// Ten minutes of speech at 50 frames per second — the same duration as the
/// soak test in `specs/09-roadmap.md`.
///
/// Sixty seconds is not enough: a bursty profile with a mean burst of 8 frames
/// produces only about twenty loss episodes in a minute, so the observed rate
/// swings by tens of percent around the configured one. Sizing test runs by
/// episode count rather than by frame count is a real lesson for M1.7.
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

    println!(
        "{FRAMES} frames, {} s of speech, seed {SEED}\n",
        FRAMES / 50
    );
    println!(
        "{:<15} {:>7} {:>8} {:>7} {:>9} {:>9} {:>9}",
        "profile", "loss%", "burst", "reord", "transit p50", "p95", "worst"
    );
    println!("{}", "-".repeat(70));

    for (name, profile) in profiles {
        let (arrivals, stats) = run(profile, SEED, FRAMES);

        let mut transits: Vec<f64> = arrivals.iter().map(ArrivedFrame::transit_ms).collect();
        transits.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let percentile = |fraction: f64| -> f64 {
            if transits.is_empty() {
                return 0.0;
            }
            let index = ((transits.len() - 1) as f64 * fraction) as usize;
            transits.get(index).copied().unwrap_or_default()
        };

        println!(
            "{:<15} {:>6.2}% {:>8} {:>7} {:>8.1}ms {:>7.1}ms {:>7.1}ms",
            name,
            stats.loss_fraction() * 100.0,
            stats.longest_loss_burst,
            stats.reordered,
            percentile(0.50),
            percentile(0.95),
            transits.last().copied().unwrap_or_default(),
        );
    }

    println!();
    println!("burst = longest run of consecutive dropped frames (1 frame = 20 ms)");
    println!("reord = frames that arrived after a higher sequence number");
    println!("transit = one-way delay, sender to receiver");
}
