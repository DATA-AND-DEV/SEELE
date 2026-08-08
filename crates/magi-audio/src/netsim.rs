//! Deterministic network impairment simulator.
//!
//! `specs/03-audio.md` requires the jitter buffer to be "a pure, deterministic
//! module, testable without real audio: a sequence of frames with timestamps
//! goes in, a sequence of decisions comes out. This allows property tests
//! against synthetic network patterns." This module produces those patterns.
//!
//! It is a test tool that lives in the library rather than in `tests/`, because
//! more than one test binary needs it and integration tests cannot reach into a
//! `#[cfg(test)]` module. It has no dependencies and dead-code elimination keeps
//! it out of shipped binaries.
//!
//! # Two modelling decisions worth knowing about
//!
//! **Loss is bursty, not a biased coin.** Real packet loss arrives in runs: a
//! Wi-Fi retransmission window closes, a queue overflows, a radio hands over.
//! A jitter buffer that copes with 5% independent loss can still fall apart on
//! 5% bursty loss, so the default model is a Gilbert-Elliott two-state chain
//! ([`LossModel`]). `specs/09-roadmap.md` accepts M1 on "induced loss of 5%
//! remains intelligible" — this module makes that sentence mean the harder thing.
//!
//! **Reordering is not modelled separately.** It emerges: every frame draws its
//! own delay, so when frame N draws more than frame N+1, N+1 arrives first. That
//! is also how reordering happens on a real path, and modelling it independently
//! would let the simulator produce reorderings that no delay distribution could
//! explain.
//!
//! # Determinism
//!
//! Everything is driven by a seeded [`Pcg32`] rather than `rand`, so a run is
//! reproducible from its seed forever. `rand`'s generators are allowed to change
//! between releases; a property test that fails today has to be replayable in two
//! years, and a dependency upgrade must not silently change what "seed 42" means.

/// A small, stable PRNG. PCG-XSH-RR with a 64-bit state and 32-bit output.
///
/// Hand-rolled on purpose — see the module docs. The constants are the reference
/// ones from the PCG paper; do not tune them, or every recorded seed changes
/// meaning.
#[derive(Debug, Clone)]
pub struct Pcg32 {
    state: u64,
    increment: u64,
}

impl Pcg32 {
    /// Seeds the generator. Any `u64` is a valid seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut generator = Self {
            state: 0,
            increment: (seed << 1) | 1,
        };
        let _ = generator.next_u32();
        generator.state = generator.state.wrapping_add(seed);
        let _ = generator.next_u32();
        generator
    }

    /// Next raw 32-bit output.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.increment);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rotation = (old >> 59) as u32;
        xorshifted.rotate_right(rotation)
    }

    /// Next value in `0.0..1.0`.
    pub fn next_unit(&mut self) -> f64 {
        f64::from(self.next_u32()) / (f64::from(u32::MAX) + 1.0)
    }

    /// Draws `true` with probability `chance`, clamped to `0.0..=1.0`.
    pub fn chance(&mut self, chance: f64) -> bool {
        self.next_unit() < chance.clamp(0.0, 1.0)
    }
}

/// How packet loss is distributed over time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LossModel {
    /// Probability per frame of entering the lossy state.
    pub good_to_bad: f64,
    /// Probability per frame of recovering.
    pub bad_to_good: f64,
    /// Loss probability while in the good state. Usually zero.
    pub loss_in_good: f64,
    /// Loss probability while in the bad state.
    pub loss_in_bad: f64,
}

impl LossModel {
    /// A path that never drops anything.
    #[must_use]
    pub const fn perfect() -> Self {
        Self {
            good_to_bad: 0.0,
            bad_to_good: 1.0,
            loss_in_good: 0.0,
            loss_in_bad: 0.0,
        }
    }

    /// Memoryless loss: every frame is an independent coin flip.
    ///
    /// This is the easy case, and mostly a control to compare against
    /// [`Self::bursty`]. Real paths do not behave like this.
    #[must_use]
    pub fn independent(loss_fraction: f64) -> Self {
        let loss = loss_fraction.clamp(0.0, 1.0);
        Self {
            good_to_bad: 0.0,
            bad_to_good: 1.0,
            loss_in_good: loss,
            loss_in_bad: loss,
        }
    }

    /// Gilbert-Elliott loss with a target average rate and burst length.
    ///
    /// `mean_burst_frames` is how many consecutive frames a loss episode lasts
    /// on average; at 50 frames per second, 5 frames is 100 ms of silence.
    ///
    /// The steady-state fraction of time spent in the bad state is
    /// `p / (p + r)`, and with total loss while bad that equals the average loss
    /// rate. Solving for `p` given `r = 1 / mean_burst` yields the transition
    /// probability below.
    #[must_use]
    pub fn bursty(loss_fraction: f64, mean_burst_frames: f64) -> Self {
        let loss = loss_fraction.clamp(0.0, 0.999);
        let burst = mean_burst_frames.max(1.0);
        let bad_to_good = 1.0 / burst;
        let good_to_bad = bad_to_good * loss / (1.0 - loss);
        Self {
            good_to_bad,
            bad_to_good,
            loss_in_good: 0.0,
            loss_in_bad: 1.0,
        }
    }
}

/// Everything that can happen to a frame in flight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetworkProfile {
    /// One-way delay before any jitter, in milliseconds.
    pub base_delay_ms: f64,
    /// Extra delay drawn per frame, uniform in `0..jitter_ms`.
    pub jitter_ms: f64,
    /// Occasional large delay, as seen when a queue fills or a radio hands over.
    pub spike_chance: f64,
    /// How large that spike is, in milliseconds.
    pub spike_ms: f64,
    /// How loss is distributed.
    pub loss: LossModel,
    /// Probability that a frame is delivered twice.
    pub duplication: f64,
}

impl NetworkProfile {
    /// A perfect link: fixed delay, no jitter, no loss. The control case.
    #[must_use]
    pub const fn perfect() -> Self {
        Self {
            base_delay_ms: 1.0,
            jitter_ms: 0.0,
            spike_chance: 0.0,
            spike_ms: 0.0,
            loss: LossModel::perfect(),
            duplication: 0.0,
        }
    }

    /// A quiet wired LAN. This is the condition `specs/00-visao-geral.md` sets
    /// the sub-60 ms target for.
    #[must_use]
    pub const fn lan() -> Self {
        Self {
            base_delay_ms: 1.0,
            jitter_ms: 1.0,
            spike_chance: 0.0,
            spike_ms: 0.0,
            loss: LossModel::perfect(),
            duplication: 0.0,
        }
    }

    /// Household Wi-Fi: mostly fine, with occasional retransmission stalls.
    #[must_use]
    pub fn wifi() -> Self {
        Self {
            base_delay_ms: 8.0,
            jitter_ms: 12.0,
            spike_chance: 0.01,
            spike_ms: 80.0,
            loss: LossModel::bursty(0.01, 3.0),
            duplication: 0.0,
        }
    }

    /// Regional internet, the second target in `specs/00-visao-geral.md`.
    #[must_use]
    pub fn regional() -> Self {
        Self {
            base_delay_ms: 30.0,
            jitter_ms: 15.0,
            spike_chance: 0.005,
            spike_ms: 60.0,
            loss: LossModel::bursty(0.005, 2.0),
            duplication: 0.001,
        }
    }

    /// A bad mobile link: high delay, wide jitter, long loss bursts.
    #[must_use]
    pub fn mobile_poor() -> Self {
        Self {
            base_delay_ms: 60.0,
            jitter_ms: 40.0,
            spike_chance: 0.03,
            spike_ms: 250.0,
            loss: LossModel::bursty(0.05, 8.0),
            duplication: 0.002,
        }
    }

    /// The exact condition `specs/09-roadmap.md` accepts M1 on: 5% induced loss.
    ///
    /// Bursty rather than independent, because that is the harder and more
    /// honest reading of the acceptance criterion.
    #[must_use]
    pub fn acceptance_five_percent_loss() -> Self {
        Self {
            base_delay_ms: 5.0,
            jitter_ms: 10.0,
            spike_chance: 0.0,
            spike_ms: 0.0,
            loss: LossModel::bursty(0.05, 4.0),
            duplication: 0.0,
        }
    }
}

/// A frame as it left the sender.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SentFrame {
    /// Sequence number, wrapping. `specs/02-protocolo.md`.
    pub seq: u16,
    /// Timestamp in samples at 48 kHz. `specs/02-protocolo.md`.
    pub timestamp: u32,
    /// When the sender put it on the wire, in milliseconds.
    pub sent_at_ms: f64,
}

/// A frame as the receiver saw it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrivedFrame {
    /// Sequence number as sent.
    pub seq: u16,
    /// Timestamp as sent.
    pub timestamp: u32,
    /// When the sender put it on the wire.
    pub sent_at_ms: f64,
    /// When it reached the receiver.
    pub arrived_at_ms: f64,
    /// True if this is the second copy of a duplicated frame.
    pub is_duplicate: bool,
}

impl ArrivedFrame {
    /// One-way transit time for this frame.
    #[must_use]
    pub fn transit_ms(&self) -> f64 {
        self.arrived_at_ms - self.sent_at_ms
    }
}

/// What the simulator actually did, so a test can check it got the conditions
/// it asked for.
///
/// Without this, a test that intends 5% loss but accidentally configures 0%
/// passes for the wrong reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NetworkStats {
    /// Frames handed to [`Network::transmit`].
    pub sent: u64,
    /// Frames that reached the receiver, not counting duplicates.
    pub delivered: u64,
    /// Frames dropped.
    pub dropped: u64,
    /// Extra copies delivered.
    pub duplicated: u64,
    /// Frames that arrived after a frame with a later sequence number.
    pub reordered: u64,
    /// Longest run of consecutive drops.
    pub longest_loss_burst: u64,
}

impl NetworkStats {
    /// Observed loss as a fraction of frames sent.
    #[must_use]
    pub fn loss_fraction(&self) -> f64 {
        if self.sent == 0 {
            return 0.0;
        }
        self.dropped as f64 / self.sent as f64
    }
}

/// A simulated one-way network path.
#[derive(Debug, Clone)]
pub struct Network {
    profile: NetworkProfile,
    rng: Pcg32,
    in_bad_state: bool,
    stats: NetworkStats,
    current_burst: u64,
}

impl Network {
    /// Builds a path from a profile and a seed.
    ///
    /// The same profile and seed always produce exactly the same run.
    #[must_use]
    pub fn new(profile: NetworkProfile, seed: u64) -> Self {
        Self {
            profile,
            rng: Pcg32::new(seed),
            in_bad_state: false,
            stats: NetworkStats::default(),
            current_burst: 0,
        }
    }

    /// What the simulator has done so far.
    #[must_use]
    pub fn stats(&self) -> NetworkStats {
        self.stats
    }

    /// Sends one frame, appending zero, one or two arrivals to `out`.
    ///
    /// Arrivals are appended in send order; call [`Self::sort_arrivals`] to see
    /// them in the order a receiver would.
    pub fn transmit(&mut self, frame: SentFrame, out: &mut Vec<ArrivedFrame>) {
        self.stats.sent += 1;

        // Advance the Gilbert-Elliott chain before deciding, so a burst can
        // begin on this very frame.
        if self.in_bad_state {
            if self.rng.chance(self.profile.loss.bad_to_good) {
                self.in_bad_state = false;
            }
        } else if self.rng.chance(self.profile.loss.good_to_bad) {
            self.in_bad_state = true;
        }

        let loss_chance = if self.in_bad_state {
            self.profile.loss.loss_in_bad
        } else {
            self.profile.loss.loss_in_good
        };

        if self.rng.chance(loss_chance) {
            self.stats.dropped += 1;
            self.current_burst += 1;
            self.stats.longest_loss_burst = self.stats.longest_loss_burst.max(self.current_burst);
            return;
        }
        self.current_burst = 0;

        let arrival = frame.sent_at_ms + self.draw_delay();
        out.push(ArrivedFrame {
            seq: frame.seq,
            timestamp: frame.timestamp,
            sent_at_ms: frame.sent_at_ms,
            arrived_at_ms: arrival,
            is_duplicate: false,
        });
        self.stats.delivered += 1;

        if self.rng.chance(self.profile.duplication) {
            // A duplicate takes its own path and so has its own delay.
            let copy_arrival = frame.sent_at_ms + self.draw_delay();
            out.push(ArrivedFrame {
                seq: frame.seq,
                timestamp: frame.timestamp,
                sent_at_ms: frame.sent_at_ms,
                arrived_at_ms: copy_arrival,
                is_duplicate: true,
            });
            self.stats.duplicated += 1;
        }
    }

    fn draw_delay(&mut self) -> f64 {
        let mut delay = self.profile.base_delay_ms + self.rng.next_unit() * self.profile.jitter_ms;
        if self.rng.chance(self.profile.spike_chance) {
            delay += self.profile.spike_ms;
        }
        delay
    }

    /// Puts arrivals into the order a receiver observes them, and counts
    /// reordering as a side effect.
    ///
    /// Reordering is not injected anywhere — it falls out of per-frame delays,
    /// which is how it happens on a real path.
    pub fn sort_arrivals(&mut self, arrivals: &mut [ArrivedFrame]) {
        arrivals.sort_by(|a, b| {
            a.arrived_at_ms
                .partial_cmp(&b.arrived_at_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut highest_seen: Option<u16> = None;
        let mut reordered = 0_u64;
        for frame in arrivals.iter().filter(|f| !f.is_duplicate) {
            match highest_seen {
                Some(highest) if frame.seq < highest => reordered += 1,
                _ => highest_seen = Some(frame.seq.max(highest_seen.unwrap_or(frame.seq))),
            }
        }
        self.stats.reordered = reordered;
    }
}

/// Generates the frame stream a sender would produce.
///
/// One frame every 20 ms carrying 960 samples, per `specs/03-audio.md`.
#[must_use]
pub fn sender_stream(frames: usize) -> Vec<SentFrame> {
    (0..frames)
        .map(|index| SentFrame {
            seq: (index % usize::from(u16::MAX) + 1) as u16,
            timestamp: (index as u32).wrapping_mul(crate::FRAME_SAMPLES as u32),
            sent_at_ms: index as f64 * f64::from(crate::FRAME_MS),
        })
        .collect()
}

/// Generates a talker who alternates speech and silence.
///
/// This is what a real sender produces once DTX and voice activation are on
/// (`specs/03-audio.md`): during silence nothing goes on the wire at all.
///
/// The two counters follow RTP semantics, and the difference between them is
/// what lets a receiver tell silence from loss (`specs/02-protocolo.md`, task
/// M1.9):
///
/// - `seq` increments once per **transmitted** packet, so it stays contiguous
///   across a silence.
/// - `timestamp` advances by the samples that **elapsed**, so it jumps.
///
/// A gap with contiguous `seq` is somebody who stopped talking. A gap with a
/// jump in `seq` is the network.
#[must_use]
pub fn talker_stream(talk_frames: usize, silence_frames: usize, cycles: usize) -> Vec<SentFrame> {
    let mut frames = Vec::with_capacity(talk_frames * cycles);
    let mut seq = 1_u16;
    let mut elapsed = 0_usize;

    for _ in 0..cycles {
        for _ in 0..talk_frames {
            frames.push(SentFrame {
                seq,
                timestamp: (elapsed as u32).wrapping_mul(crate::FRAME_SAMPLES as u32),
                sent_at_ms: elapsed as f64 * f64::from(crate::FRAME_MS),
            });
            seq = seq.wrapping_add(1);
            elapsed += 1;
        }
        // Silence: the clock keeps running, the sequence does not.
        elapsed += silence_frames;
    }
    frames
}

/// Generates a sender whose clock runs fast or slow.
///
/// Two independent devices nominally at 48 kHz actually run at slightly
/// different rates — consumer crystals are specified to something like ±50 ppm,
/// and the two ends never agree exactly. Over ten minutes 100 ppm is 60 ms of
/// accumulated skew, which is an entire jitter buffer. This is risk R3 in
/// `docs/plano-m0-m1.md`, and it is why `specs/09-roadmap.md` asks for a ten
/// minute soak specifically.
///
/// The model is exact about where the error lives:
///
/// - `timestamp` advances by exactly one frame each time, because it counts the
///   sender's **own** samples and the sender believes its clock.
/// - `sent_at_ms` advances by slightly less or more than a frame of real time,
///   because the sender's clock is wrong about how long a frame takes.
///
/// A positive `drift_ppm` means the sender runs **fast**: it emits frames a
/// little sooner than real time, so a receiver accumulates audio.
#[must_use]
pub fn drifting_stream(frames: usize, drift_ppm: f64) -> Vec<SentFrame> {
    let interval_ms = f64::from(crate::FRAME_MS) * (1.0 - drift_ppm / 1_000_000.0);
    (0..frames)
        .map(|index| SentFrame {
            seq: (index % usize::from(u16::MAX) + 1) as u16,
            timestamp: (index as u32).wrapping_mul(crate::FRAME_SAMPLES as u32),
            sent_at_ms: index as f64 * interval_ms,
        })
        .collect()
}

/// Runs a whole stream through a path and returns arrivals in receive order.
#[must_use]
pub fn run(profile: NetworkProfile, seed: u64, frames: usize) -> (Vec<ArrivedFrame>, NetworkStats) {
    let mut network = Network::new(profile, seed);
    let mut arrivals = Vec::with_capacity(frames);
    for frame in sender_stream(frames) {
        network.transmit(frame, &mut arrivals);
    }
    network.sort_arrivals(&mut arrivals);
    (arrivals, network.stats())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Longest run of consecutive missing sequence numbers in a delivery.
    fn longest_gap(arrivals: &[ArrivedFrame], sent: usize) -> usize {
        let delivered: std::collections::HashSet<u16> =
            arrivals.iter().map(|frame| frame.seq).collect();
        let mut longest = 0;
        let mut current = 0;
        for frame in sender_stream(sent) {
            if delivered.contains(&frame.seq) {
                current = 0;
            } else {
                current += 1;
                longest = longest.max(current);
            }
        }
        longest
    }

    #[test]
    fn same_seed_gives_an_identical_run() {
        // The whole point. A property test that fails must be replayable.
        let (first, first_stats) = run(NetworkProfile::mobile_poor(), 42, 2_000);
        let (second, second_stats) = run(NetworkProfile::mobile_poor(), 42, 2_000);
        assert_eq!(first, second);
        assert_eq!(first_stats, second_stats);
    }

    #[test]
    fn different_seeds_give_different_runs() {
        let (first, _) = run(NetworkProfile::mobile_poor(), 1, 2_000);
        let (second, _) = run(NetworkProfile::mobile_poor(), 2, 2_000);
        assert_ne!(first, second);
    }

    #[test]
    fn a_perfect_path_delivers_everything_in_order() {
        let (arrivals, stats) = run(NetworkProfile::perfect(), 7, 500);
        assert_eq!(stats.sent, 500);
        assert_eq!(stats.delivered, 500);
        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.reordered, 0);
        assert_eq!(stats.duplicated, 0);

        let sequences: Vec<u16> = arrivals.iter().map(|frame| frame.seq).collect();
        let mut sorted = sequences.clone();
        sorted.sort_unstable();
        assert_eq!(sequences, sorted);
    }

    #[test]
    fn total_loss_delivers_nothing() {
        let profile = NetworkProfile {
            loss: LossModel::independent(1.0),
            ..NetworkProfile::lan()
        };
        let (arrivals, stats) = run(profile, 3, 200);
        assert!(arrivals.is_empty());
        assert_eq!(stats.dropped, 200);
    }

    #[test]
    fn observed_loss_tracks_the_configured_rate() {
        // Guards against a test that asks for 5% loss and silently gets none.
        for target in [0.01, 0.05, 0.20] {
            let profile = NetworkProfile {
                loss: LossModel::bursty(target, 4.0),
                ..NetworkProfile::lan()
            };
            let (_, stats) = run(profile, 99, 20_000);
            let observed = stats.loss_fraction();
            assert!(
                (observed - target).abs() < target * 0.25,
                "asked for {target}, observed {observed:.4}"
            );
        }
    }

    #[test]
    fn bursty_loss_clusters_and_independent_loss_does_not() {
        // The reason this module exists. At the same average rate, the bursty
        // model must produce visibly longer runs of consecutive loss — otherwise
        // it is an expensive way to flip a biased coin, and the jitter buffer
        // would be tested against a network that does not exist.
        const FRAMES: usize = 20_000;
        const RATE: f64 = 0.05;

        let (independent, independent_stats) = run(
            NetworkProfile {
                loss: LossModel::independent(RATE),
                ..NetworkProfile::lan()
            },
            11,
            FRAMES,
        );
        let (bursty, bursty_stats) = run(
            NetworkProfile {
                loss: LossModel::bursty(RATE, 8.0),
                ..NetworkProfile::lan()
            },
            11,
            FRAMES,
        );

        // Comparable overall loss...
        assert!((independent_stats.loss_fraction() - RATE).abs() < 0.02);
        assert!((bursty_stats.loss_fraction() - RATE).abs() < 0.02);

        // ...but very different shape.
        let independent_gap = longest_gap(&independent, FRAMES);
        let bursty_gap = longest_gap(&bursty, FRAMES);
        assert!(
            bursty_gap > independent_gap * 2,
            "bursty longest gap {bursty_gap} should dwarf independent {independent_gap}"
        );
        assert!(
            bursty_stats.longest_loss_burst > independent_stats.longest_loss_burst * 2,
            "bursty burst {} vs independent {}",
            bursty_stats.longest_loss_burst,
            independent_stats.longest_loss_burst
        );
    }

    #[test]
    fn mean_burst_length_is_honoured() {
        let short = run(
            NetworkProfile {
                loss: LossModel::bursty(0.05, 2.0),
                ..NetworkProfile::lan()
            },
            5,
            20_000,
        )
        .1;
        let long = run(
            NetworkProfile {
                loss: LossModel::bursty(0.05, 16.0),
                ..NetworkProfile::lan()
            },
            5,
            20_000,
        )
        .1;
        assert!(
            long.longest_loss_burst > short.longest_loss_burst,
            "a longer mean burst must produce longer bursts: {} vs {}",
            long.longest_loss_burst,
            short.longest_loss_burst
        );
    }

    #[test]
    fn jitter_causes_reordering_without_being_injected() {
        // Reordering is never generated directly — it must emerge from per-frame
        // delay draws, which is how a real path produces it.
        let profile = NetworkProfile {
            base_delay_ms: 20.0,
            // Jitter wider than the 20 ms frame interval guarantees overtaking.
            jitter_ms: 60.0,
            ..NetworkProfile::lan()
        };
        let (_, stats) = run(profile, 4, 2_000);
        assert!(stats.reordered > 0, "wide jitter should reorder frames");
    }

    #[test]
    fn no_jitter_means_no_reordering() {
        let (_, stats) = run(NetworkProfile::perfect(), 4, 2_000);
        assert_eq!(stats.reordered, 0);
    }

    #[test]
    fn duplicates_are_delivered_and_flagged() {
        let profile = NetworkProfile {
            duplication: 0.5,
            ..NetworkProfile::lan()
        };
        let (arrivals, stats) = run(profile, 8, 1_000);
        assert!(stats.duplicated > 300, "expected many duplicates");
        let flagged = arrivals.iter().filter(|frame| frame.is_duplicate).count();
        assert_eq!(flagged as u64, stats.duplicated);
    }

    #[test]
    fn arrivals_come_out_in_receive_order() {
        let (arrivals, _) = run(NetworkProfile::mobile_poor(), 13, 2_000);
        assert!(
            arrivals.windows(2).all(|pair| match pair {
                [a, b] => a.arrived_at_ms <= b.arrived_at_ms,
                _ => true,
            }),
            "the receiver sees frames in arrival order, so the output must be sorted"
        );
    }

    #[test]
    fn transit_time_respects_the_configured_floor() {
        let profile = NetworkProfile::regional();
        let (arrivals, _) = run(profile, 21, 2_000);
        assert!(
            arrivals
                .iter()
                .all(|frame| frame.transit_ms() >= profile.base_delay_ms),
            "nothing may arrive faster than the base delay"
        );
    }

    #[test]
    fn spikes_produce_a_long_tail() {
        let profile = NetworkProfile {
            base_delay_ms: 10.0,
            jitter_ms: 5.0,
            spike_chance: 0.05,
            spike_ms: 200.0,
            ..NetworkProfile::lan()
        };
        let (arrivals, _) = run(profile, 17, 4_000);
        let worst = arrivals
            .iter()
            .map(ArrivedFrame::transit_ms)
            .fold(0.0_f64, f64::max);
        assert!(worst > 200.0, "expected a spike in the tail, worst {worst}");
    }

    #[test]
    fn the_generator_is_uniform_enough_to_trust() {
        // A biased PRNG would quietly distort every loss and jitter figure above.
        let mut rng = Pcg32::new(2024);
        let mut buckets = [0_u32; 10];
        for _ in 0..100_000 {
            let index = (rng.next_unit() * 10.0) as usize;
            if let Some(bucket) = buckets.get_mut(index.min(9)) {
                *bucket += 1;
            }
        }
        for (index, count) in buckets.iter().enumerate() {
            assert!(
                (*count as i64 - 10_000).abs() < 600,
                "bucket {index} had {count}, expected about 10000"
            );
        }
    }

    #[test]
    fn a_drifting_sender_skews_wall_clock_but_not_its_timestamps() {
        // The property the drift tracker keys off: the timestamp is what the
        // sender believes, the send time is what actually happened, and the gap
        // between them opens linearly.
        let frames = drifting_stream(30_000, 100.0);

        let first = frames.first().copied().unwrap_or(SentFrame {
            seq: 0,
            timestamp: 0,
            sent_at_ms: 0.0,
        });
        let last = frames.last().copied().unwrap_or(first);

        let believed_ms =
            f64::from(last.timestamp - first.timestamp) / f64::from(crate::SAMPLE_RATE_HZ) * 1000.0;
        let actual_ms = last.sent_at_ms - first.sent_at_ms;

        // Ten minutes at 100 ppm is 60 ms of skew.
        let skew = believed_ms - actual_ms;
        assert!(
            (skew - 60.0).abs() < 1.0,
            "expected about 60 ms of skew over ten minutes, got {skew:.1}"
        );
    }

    #[test]
    fn zero_drift_is_the_ordinary_stream() {
        let drifting = drifting_stream(100, 0.0);
        let plain = sender_stream(100);
        assert_eq!(drifting.len(), plain.len());
        assert_eq!(
            drifting.last().map(|f| f.sent_at_ms),
            plain.last().map(|f| f.sent_at_ms)
        );
    }

    #[test]
    fn a_talker_keeps_sequence_contiguous_across_silence() {
        // The property the whole of M1.9 rests on. If this ever stops holding,
        // silence becomes indistinguishable from loss and the Sync Ratio of
        // specs/07-tema-evangelion.md drops every time nobody speaks.
        let frames = talker_stream(3, 10, 2);
        assert_eq!(frames.len(), 6);

        let sequences: Vec<u16> = frames.iter().map(|f| f.seq).collect();
        assert_eq!(sequences, vec![1, 2, 3, 4, 5, 6], "sequence must not skip");

        // The timestamp, by contrast, must jump across the silence.
        let before = frames.get(2).map(|f| f.timestamp).unwrap_or_default();
        let after = frames.get(3).map(|f| f.timestamp).unwrap_or_default();
        let frame_samples = crate::FRAME_SAMPLES as u32;
        assert_eq!(
            after - before,
            frame_samples * 11,
            "timestamp must account for the silent frames"
        );
    }

    #[test]
    fn sender_stream_matches_the_spec_frame_layout() {
        let frames = sender_stream(3);
        assert_eq!(frames.len(), 3);
        // 20 ms apart, 960 samples apart. specs/02-protocolo.md and specs/03.
        assert_eq!(frames.get(1).map(|f| f.sent_at_ms), Some(20.0));
        assert_eq!(frames.get(1).map(|f| f.timestamp), Some(960));
        assert_eq!(frames.get(2).map(|f| f.timestamp), Some(1_920));
    }
}
