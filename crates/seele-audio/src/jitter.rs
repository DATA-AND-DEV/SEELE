//! Adaptive jitter buffer, one per source.
//!
//! `specs/03-audio.md` calls this "the component that separates *works* from
//! *works well*", and specifies it as
//!
//! > a pure, deterministic module, testable without real audio: a sequence of
//! > frames with timestamps goes in, a sequence of decisions comes out.
//!
//! That shape is load-bearing, not stylistic. **Nothing here decodes Opus.** The
//! buffer emits [`Decision`]s and the caller runs the codec, which is why the
//! whole thing can be property-tested against synthetic networks
//! ([`crate::netsim`]) with no sound card and no libopus in the loop.
//!
//! # What it does
//!
//! - Reorders frames back into sequence, within the delay it is holding.
//! - Estimates arrival jitter and adapts how much delay to hold, between the
//!   20 ms and 200 ms bounds in `specs/03-audio.md`.
//! - Grows the target fast and shrinks it slowly — "intentional asymmetry,
//!   because cutting audio is worse than delaying it".
//! - Asks for one frame of Opus PLC on a gap, then silence, per `specs/03`.
//! - Discards frames that arrive after their moment has passed, and counts them
//!   as loss rather than pretending they were useful.
//!
//! # Telling silence apart from loss
//!
//! With DTX and voice activation on (`specs/03-audio.md`), a sender puts nothing
//! on the wire while nobody is speaking. A buffer that reads every gap as loss
//! reports heavy loss whenever the room is quiet — and `specs/07-estetica.md`
//! puts loss on screen as the product's signature metric, so that is not a
//! cosmetic bug. This was gap G5 in `docs/plano-m0-m1.md`.
//!
//! `specs/02-protocolo.md` already contains the answer, in the channel saying the
//! timestamp "detects silence gaps". The two counters move differently:
//!
//! - `seq` increments once per **transmitted** packet.
//! - `timestamp` advances by the samples that **elapsed**.
//!
//! So a gap whose `seq` is contiguous is a talker who stopped, and a gap that
//! skips sequence numbers is the network. Both can happen at once, and the
//! arithmetic separates them exactly: the timestamp gap gives the total slots to
//! fill, the sequence gap gives how many of those were lost, and the remainder
//! is silence.
//!
//! This is also why **playout is indexed by timestamp, not by sequence**.
//! Playing consecutive sequence numbers back to back would compress a two-second
//! silence into 20 ms.
//!
//! # Sequence wrap
//!
//! `specs/02-protocolo.md` gives the media header a 16-bit sequence number,
//! which wraps every 21 minutes at 50 frames per second. Every comparison here
//! goes through [`seq_delta`], never through `<`. A call longer than 21 minutes
//! is entirely ordinary and a naive comparison would break exactly once per
//! wrap, in a way that looks like a random glitch.

use std::collections::VecDeque;

/// Frames the buffer can hold at once.
///
/// 256 frames is 5.12 s at 20 ms — far beyond the 200 ms ceiling in
/// `specs/03-audio.md`, so the capacity never binds in normal operation. It
/// exists to bound memory against a sender that floods, which
/// `specs/04-servidor-seele.md` also guards against on its side.
const CAPACITY: usize = 256;

/// Smoothing factor for the arrival-jitter estimate.
///
/// RFC 3550's interarrival jitter, which uses 1/16. Keeping the standard value
/// means the number is comparable with every other VoIP tool an operator might
/// hold it next to.
const JITTER_SMOOTHING: f64 = 1.0 / 16.0;

/// How many jitter estimates of headroom the target aims to hold.
///
/// Three covers the great majority of a roughly normal arrival distribution.
/// Higher is safer and slower; `specs/00-visao-geral.md` is spending this
/// directly out of the latency budget.
const JITTER_HEADROOM: f64 = 3.0;

/// Fraction of the gap the target closes per frame while shrinking.
///
/// `specs/03-audio.md` asks for fast growth and slow shrink. At 50 frames per
/// second this closes ~10% of the gap per second, so a buffer that ballooned to
/// 200 ms takes roughly 20 s to come back to 20 ms once the network settles.
/// Slow enough that a second disturbance finds the buffer still deep.
const SHRINK_PER_FRAME: f64 = 0.002;

/// Gap size beyond which the buffer jumps rather than working through it.
///
/// Silence is walked slot by slot, one per playout tick, which is correct and
/// costs nothing: a ten-second silence is ten real seconds of ticks. This bound
/// exists for the other case — a receiver that fell far behind, or a sender that
/// jumped its clock. 3000 frames is a minute.
const MAX_GAP_FRAMES: u32 = 3_000;

/// Wrap-aware difference between two sequence numbers.
///
/// Returns how far `a` is ahead of `b`, treating the 16-bit space as circular.
/// Positive means `a` is newer.
#[must_use]
pub fn seq_delta(a: u16, b: u16) -> i32 {
    i32::from(a.wrapping_sub(b) as i16)
}

/// Wrap-aware difference between two RTP timestamps, in samples.
///
/// The timestamp is 32 bits of 48 kHz samples, so it wraps after about 24.8
/// hours. Rare, but a session that long is not forbidden and the failure would
/// be a single inexplicable glitch.
#[must_use]
pub fn ts_delta(a: u32, b: u32) -> i64 {
    i64::from(a.wrapping_sub(b) as i32)
}

/// What the buffer wants the caller to do for one 20 ms slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision<T> {
    /// Decode and play this payload.
    Play(T),
    /// The frame is missing. Ask the decoder to conceal it — one frame of Opus
    /// PLC, per `specs/03-audio.md`.
    Conceal,
    /// The gap has outlasted concealment. Emit silence, faded rather than cut.
    Silence,
    /// The talker was silent for this slot — DTX or voice activation, not loss.
    ///
    /// Sounds identical to [`Self::Silence`], but must never be counted as loss:
    /// see the module docs.
    Comfort,
    /// Not enough depth yet to start, or nothing to play. The caller should emit
    /// silence without counting it as loss.
    Starved,
}

/// Tunables, all from `specs/03-audio.md`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JitterConfig {
    /// Where the target starts. `specs/03-audio.md`: 40 ms, two frames.
    pub initial_target_ms: f64,
    /// Lower bound on the adaptive target.
    pub min_target_ms: f64,
    /// Upper bound on the adaptive target.
    pub max_target_ms: f64,
    /// Frame duration, in milliseconds.
    pub frame_ms: f64,
}

impl Default for JitterConfig {
    fn default() -> Self {
        Self {
            initial_target_ms: 40.0,
            min_target_ms: 20.0,
            max_target_ms: 200.0,
            frame_ms: f64::from(crate::FRAME_MS),
        }
    }
}

/// Everything the buffer knows about itself, as plain data.
///
/// `specs/03-audio.md` lists exactly these and says they feed the Sync Ratio.
/// No formatting, no colour, no bands — those are the shell's job
/// (`specs/01-arquitetura.md`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JitterMetrics {
    /// Frames handed to the buffer, including ones it later discarded.
    pub frames_received: u64,
    /// Frames played out.
    pub frames_played: u64,
    /// Slots where a frame was missing and concealment was requested.
    pub frames_concealed: u64,
    /// Slots filled with silence because the gap outlasted concealment.
    pub frames_silenced: u64,
    /// Slots the talker was simply silent for. **Not loss.**
    pub frames_comfort: u64,
    /// Frames that arrived after their playout slot had passed.
    pub late_discards: u64,
    /// Frames dropped because the buffer was already full.
    pub overflow_discards: u64,
    /// Duplicate sequence numbers seen and ignored.
    pub duplicates: u64,
    /// Times the buffer gave up on a gap and resynchronised.
    pub resyncs: u64,
    /// Current target depth.
    pub target_ms: f64,
    /// Current actual depth.
    pub depth_ms: f64,
    /// Smoothed arrival jitter, RFC 3550 style.
    pub jitter_ms: f64,
}

impl JitterMetrics {
    /// Fraction of expected slots that could not be played from a real frame.
    ///
    /// This is what `specs/02-protocolo.md` feeds into the loss penalty of the
    /// Sync Ratio. Concealed and silenced slots count, because from the
    /// listener's side those are indistinguishable from loss.
    ///
    /// [`JitterMetrics::frames_comfort`] deliberately does **not** count, and is
    /// excluded from the denominator too. A quiet room is not a bad connection,
    /// and reporting it as one would make the signature metric of
    /// `specs/07-estetica.md` say the opposite of the truth.
    #[must_use]
    pub fn loss_fraction(&self) -> f64 {
        let slots = self.frames_played + self.frames_concealed + self.frames_silenced;
        if slots == 0 {
            return 0.0;
        }
        (self.frames_concealed + self.frames_silenced) as f64 / slots as f64
    }
}

/// One frame waiting its turn.
#[derive(Debug, Clone)]
struct Held<T> {
    seq: u16,
    timestamp: u32,
    payload: T,
}

/// An adaptive jitter buffer for a single source.
#[derive(Debug)]
pub struct JitterBuffer<T> {
    config: JitterConfig,
    queue: VecDeque<Held<T>>,
    /// Timestamp the next [`Self::tick`] will try to play. `None` until playout
    /// starts. This, not the sequence number, is the playout timeline — see the
    /// module docs.
    next_timestamp: Option<u32>,
    /// Sequence of the last frame actually played, for loss arithmetic.
    last_played_seq: Option<u16>,
    /// Loss slots still owed for the gap being worked through.
    gap_lost_left: u64,
    /// Silence slots still owed for the same gap.
    gap_comfort_left: u64,
    /// True once enough depth accumulated to begin playing.
    playing: bool,
    target_ms: f64,
    jitter_ms: f64,
    /// Previous frame's transit time, for the RFC 3550 jitter estimate.
    last_transit_ms: Option<f64>,
    /// Consecutive missing slots, for the conceal-once-then-silence rule.
    consecutive_missing: u32,
    /// Ticks spent starving since the last frame actually played.
    ///
    /// The gap a hole leaves is only discovered when the frame after it
    /// arrives, and by then part of it may already have gone out as nothing.
    /// See `plan_gap`.
    starved_since_play: u64,
    metrics: JitterMetrics,
}

impl<T> JitterBuffer<T> {
    /// Builds a buffer with the given configuration.
    #[must_use]
    pub fn new(config: JitterConfig) -> Self {
        Self {
            config,
            queue: VecDeque::with_capacity(CAPACITY),
            next_timestamp: None,
            last_played_seq: None,
            gap_lost_left: 0,
            gap_comfort_left: 0,
            playing: false,
            target_ms: config.initial_target_ms,
            jitter_ms: 0.0,
            last_transit_ms: None,
            consecutive_missing: 0,
            starved_since_play: 0,
            metrics: JitterMetrics {
                target_ms: config.initial_target_ms,
                ..JitterMetrics::default()
            },
        }
    }

    /// Current metrics.
    #[must_use]
    pub fn metrics(&self) -> JitterMetrics {
        JitterMetrics {
            target_ms: self.target_ms,
            depth_ms: self.depth_ms(),
            jitter_ms: self.jitter_ms,
            ..self.metrics
        }
    }

    /// How much audio is currently held, in milliseconds.
    #[must_use]
    pub fn depth_ms(&self) -> f64 {
        self.queue.len() as f64 * self.config.frame_ms
    }

    /// Accepts a frame that has just arrived.
    ///
    /// `timestamp` is in samples at 48 kHz, per `specs/02-protocolo.md`.
    /// `arrival_ms` is the receiver's clock.
    pub fn push(&mut self, seq: u16, timestamp: u32, arrival_ms: f64, payload: T) {
        self.metrics.frames_received += 1;
        self.update_jitter(timestamp, arrival_ms);

        // Too late: its slot has already been played or concealed.
        if let Some(next) = self.next_timestamp {
            if self.playing && ts_delta(timestamp, next) < 0 {
                self.metrics.late_discards += 1;
                return;
            }
        }

        if self.queue.iter().any(|held| held.seq == seq) {
            self.metrics.duplicates += 1;
            return;
        }

        if self.queue.len() >= CAPACITY {
            self.metrics.overflow_discards += 1;
            return;
        }

        // Insert in timestamp order. Late-but-usable frames land in the middle,
        // which is the entire point of holding a buffer at all.
        let position = self
            .queue
            .iter()
            .position(|held| ts_delta(held.timestamp, timestamp) > 0)
            .unwrap_or(self.queue.len());
        self.queue.insert(
            position,
            Held {
                seq,
                timestamp,
                payload,
            },
        );
    }

    /// Produces the decision for one 20 ms playout slot.
    ///
    /// Call this on the playout clock, once per frame period.
    pub fn tick(&mut self) -> Decision<T> {
        self.adapt_target();

        // Prebuffering: hold until the target depth is reached, so the first
        // frames do not immediately underrun.
        if !self.playing {
            if self.depth_ms() < self.target_ms {
                return Decision::Starved;
            }
            self.playing = true;
            // Start at the earliest frame held, not at whichever arrived first.
            // With reordering those differ, and starting at the wrong one
            // conceals frames that are sitting right there.
            self.next_timestamp = self.queue.front().map(|held| held.timestamp);
        }

        // Still paying off a gap worked out on an earlier tick.
        if let Some(decision) = self.drain_gap() {
            return decision;
        }

        let Some(next) = self.next_timestamp else {
            return Decision::Starved;
        };

        let Some(front) = self.queue.front() else {
            // Nothing waiting at all: starved rather than lost. A talker who
            // stopped is not a network problem.
            //
            // Counted, because this tick is real time going by with nothing
            // played. When the next frame finally arrives, `plan_gap` uses the
            // count to know how much of the hole has already been heard.
            self.starved_since_play = self.starved_since_play.saturating_add(1);
            return Decision::Starved;
        };

        if front.timestamp == next {
            let played = self.queue.pop_front();
            self.next_timestamp = Some(next.wrapping_add(self.frame_samples()));
            self.consecutive_missing = 0;
            self.starved_since_play = 0;
            self.metrics.frames_played += 1;
            if let Some(held) = played {
                self.last_played_seq = Some(held.seq);
                return Decision::Play(held.payload);
            }
            return Decision::Starved;
        }

        // There is a hole between here and the next frame. Work out how much of
        // it the network took and how much of it the talker simply did not fill.
        let (front_ts, front_seq) = (front.timestamp, front.seq);
        self.plan_gap(front_ts, front_seq, next);

        if let Some(decision) = self.drain_gap() {
            return decision;
        }

        // `plan_gap` resynchronised rather than scheduling slots, so the frame at
        // the front is playable right now. Without this the tick is wasted and
        // playout stalls one slot per resync.
        if self.next_timestamp == Some(front_ts) {
            if let Some(held) = self.queue.pop_front() {
                self.next_timestamp = Some(front_ts.wrapping_add(self.frame_samples()));
                self.last_played_seq = Some(held.seq);
                self.consecutive_missing = 0;
                self.starved_since_play = 0;
                self.metrics.frames_played += 1;
                return Decision::Play(held.payload);
            }
        }
        Decision::Starved
    }

    /// Frame duration in samples, from the configured frame length.
    fn frame_samples(&self) -> u32 {
        ((self.config.frame_ms * f64::from(crate::SAMPLE_RATE_HZ)) / 1000.0) as u32
    }

    /// Splits a gap into lost slots and silent slots.
    ///
    /// The timestamp gap says how many slots are missing in total; the sequence
    /// gap says how many packets the network actually dropped. The difference is
    /// silence the talker never sent. See the module docs.
    fn plan_gap(&mut self, front_ts: u32, front_seq: u16, next_ts: u32) {
        let frame_samples = i64::from(self.frame_samples().max(1));
        let total = (ts_delta(front_ts, next_ts) / frame_samples).max(0) as u64;
        if total == 0 {
            // Timestamps that do not land on a frame boundary. Resynchronise
            // rather than loop; a sender doing this is out of spec anyway.
            self.next_timestamp = Some(front_ts);
            self.metrics.resyncs += 1;
            return;
        }

        // A gap far beyond anything plausible means we fell behind rather than
        // lost audio — jump instead of grinding through it slot by slot.
        if total > u64::from(MAX_GAP_FRAMES) {
            self.metrics.resyncs += 1;
            self.next_timestamp = Some(front_ts);
            self.consecutive_missing = 0;
            return;
        }

        let lost = match self.last_played_seq {
            // seq counts transmitted packets, so the hole in it is exactly what
            // the network dropped, no matter how long the silence around it was.
            Some(last) => i64::from(seq_delta(front_seq, last))
                .saturating_sub(1)
                .max(0) as u64,
            // Nothing played yet, so there is no baseline to compare against.
            // Treat it as silence rather than inventing loss.
            None => 0,
        };

        // How much of this gap has already been lived through.
        //
        // A hole is only discovered when the frame *after* it arrives. If the
        // buffer ran dry in the meantime, those ticks already went out as
        // nothing — the listener has heard that silence. Scheduling slots to
        // play it again would be double-counting, and the delay it adds is
        // permanent: every pause in a conversation would push playout further
        // behind, pause length by pause length, until the queue hit its cap.
        //
        // That is why this subtraction exists, and it is the difference between
        // a call that stays at 90 ms and one that is two seconds behind after a
        // minute of ordinary back-and-forth.
        //
        // A gap found while audio was still queued is the other case: there the
        // ticks have not been spent, the buffer has depth to pay with, and
        // concealment does its job without adding delay.
        let spent = std::mem::take(&mut self.starved_since_play);
        let skipped = spent.min(total);
        let remaining = total - skipped;

        // Move the playout pointer past what has already been heard.
        //
        // This channel is the fix, and leaving it out was the first attempt: the
        // counters were reduced but the pointer was not moved, so the next tick
        // found the same hole with the counter already spent and scheduled the
        // whole thing again. The skip has to happen in the pointer or it does
        // not happen at all.
        let advance = u32::try_from(skipped)
            .unwrap_or(0)
            .wrapping_mul(self.frame_samples());
        self.next_timestamp = Some(next_ts.wrapping_add(advance));

        // What is left is spent on the lost frames first. Concealment needs a
        // slot to happen in; silence does not, because silence is what already
        // went out while the buffer was dry.
        self.gap_lost_left = lost.min(remaining);
        self.gap_comfort_left = remaining - self.gap_lost_left;

        // The skipped slots still happened, and the metrics have to describe
        // what a listener heard. Loss that fell inside them stays counted as
        // loss — understating it would make the Sync Ratio flatter the link
        // than it deserves — and the rest is the talker's pause.
        let lost_skipped = lost.saturating_sub(self.gap_lost_left);
        self.metrics.frames_silenced += lost_skipped;
        self.metrics.frames_comfort += skipped.saturating_sub(lost_skipped);

        if remaining == 0 {
            self.consecutive_missing = 0;
        }
    }

    /// Emits one slot of a planned gap, if any remain.
    fn drain_gap(&mut self) -> Option<Decision<T>> {
        if self.gap_lost_left > 0 {
            self.gap_lost_left -= 1;
            self.next_timestamp = self
                .next_timestamp
                .map(|ts| ts.wrapping_add(self.frame_samples()));
            self.consecutive_missing += 1;

            // specs/03-audio.md: PLC for one frame, silence from the second on.
            if self.consecutive_missing == 1 {
                self.metrics.frames_concealed += 1;
                return Some(Decision::Conceal);
            }
            self.metrics.frames_silenced += 1;
            return Some(Decision::Silence);
        }

        if self.gap_comfort_left > 0 {
            self.gap_comfort_left -= 1;
            self.next_timestamp = self
                .next_timestamp
                .map(|ts| ts.wrapping_add(self.frame_samples()));
            self.consecutive_missing = 0;
            self.metrics.frames_comfort += 1;
            return Some(Decision::Comfort);
        }

        None
    }

    /// RFC 3550 interarrival jitter.
    ///
    /// `D` is the difference in relative transit time between consecutive
    /// frames; the estimate follows it with a 1/16 gain. Using the RTP timestamp
    /// as the send clock means no clock synchronisation between the two ends is
    /// needed — only the difference matters.
    fn update_jitter(&mut self, timestamp: u32, arrival_ms: f64) {
        let sent_ms = f64::from(timestamp) / f64::from(crate::SAMPLE_RATE_HZ) * 1000.0;
        let transit = arrival_ms - sent_ms;

        if let Some(previous) = self.last_transit_ms {
            let difference = (transit - previous).abs();
            self.jitter_ms += (difference - self.jitter_ms) * JITTER_SMOOTHING;
        }
        self.last_transit_ms = Some(transit);
    }

    /// Moves the target toward what the measured jitter demands.
    ///
    /// Growth is immediate, shrinking is gradual — `specs/03-audio.md` is
    /// explicit that this asymmetry is intentional, "because cutting audio is
    /// worse than delaying it".
    fn adapt_target(&mut self) {
        let needed = (self.jitter_ms * JITTER_HEADROOM)
            .max(self.config.frame_ms)
            .clamp(self.config.min_target_ms, self.config.max_target_ms);

        if needed > self.target_ms {
            self.target_ms = needed;
        } else {
            self.target_ms += (needed - self.target_ms) * SHRINK_PER_FRAME;
        }
        self.target_ms = self
            .target_ms
            .clamp(self.config.min_target_ms, self.config.max_target_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netsim::{run, NetworkProfile};

    fn buffer() -> JitterBuffer<u16> {
        JitterBuffer::new(JitterConfig::default())
    }

    /// Feeds a whole simulated run through a buffer, ticking on the playout
    /// clock between arrivals, and returns every decision in order.
    fn drive(
        profile: NetworkProfile,
        seed: u64,
        frames: usize,
    ) -> (Vec<Decision<u16>>, JitterMetrics) {
        let (arrivals, _) = run(profile, seed, frames);
        let mut buffer = buffer();
        let mut decisions = Vec::new();
        let mut clock_ms = 0.0_f64;
        let mut arrival_index = 0;

        // Run the playout clock past the end so the buffer drains.
        let total_ticks = frames + 50;
        for _ in 0..total_ticks {
            while arrivals
                .get(arrival_index)
                .is_some_and(|frame| frame.arrived_at_ms <= clock_ms)
            {
                if let Some(frame) = arrivals.get(arrival_index) {
                    buffer.push(frame.seq, frame.timestamp, frame.arrived_at_ms, frame.seq);
                }
                arrival_index += 1;
            }
            decisions.push(buffer.tick());
            clock_ms += f64::from(crate::FRAME_MS);
        }
        let metrics = buffer.metrics();
        (decisions, metrics)
    }

    fn count_played(decisions: &[Decision<u16>]) -> usize {
        decisions
            .iter()
            .filter(|d| matches!(d, Decision::Play(_)))
            .count()
    }

    #[test]
    fn sequence_delta_handles_wrap() {
        // 21 minutes into a call the sequence wraps. A naive `<` breaks here
        // exactly once, and it looks like a random glitch.
        assert_eq!(seq_delta(5, 3), 2);
        assert_eq!(seq_delta(3, 5), -2);
        assert_eq!(seq_delta(0, u16::MAX), 1, "0 comes after 65535");
        assert_eq!(seq_delta(u16::MAX, 0), -1);
        assert_eq!(seq_delta(10, u16::MAX - 10), 21);
        assert_eq!(seq_delta(7, 7), 0);
    }

    #[test]
    fn a_perfect_network_plays_every_frame_in_order() {
        let (decisions, metrics) = drive(NetworkProfile::perfect(), 1, 500);

        assert_eq!(metrics.frames_concealed, 0);
        assert_eq!(metrics.frames_silenced, 0);
        assert_eq!(metrics.late_discards, 0);
        assert_eq!(count_played(&decisions), 500);

        let played: Vec<u16> = decisions
            .iter()
            .filter_map(|d| match d {
                Decision::Play(seq) => Some(*seq),
                _ => None,
            })
            .collect();
        let mut sorted = played.clone();
        sorted.sort_unstable();
        assert_eq!(played, sorted, "playout must be in sequence order");
    }

    #[test]
    fn reordered_frames_are_put_back_in_order() {
        // The buffer's whole reason to exist. Push out of order, expect in order.
        let mut buffer = buffer();
        buffer.push(3, 2_880, 60.0, 3);
        buffer.push(1, 960, 61.0, 1);
        buffer.push(2, 1_920, 62.0, 2);
        buffer.push(4, 3_840, 63.0, 4);

        let mut played = Vec::new();
        for _ in 0..4 {
            if let Decision::Play(seq) = buffer.tick() {
                played.push(seq);
            }
        }
        assert_eq!(played, vec![1, 2, 3, 4]);
        assert_eq!(buffer.metrics().frames_concealed, 0);
    }

    #[test]
    fn a_gap_conceals_once_then_falls_back_to_silence() {
        // specs/03-audio.md: "missing frame -> Opus PLC for one frame; from the
        // second on, silence with fade".
        let mut buffer = buffer();
        buffer.push(1, 960, 0.0, 1);
        buffer.push(2, 1_920, 20.0, 2);
        // 3, 4 and 5 never arrive.
        buffer.push(6, 5_760, 100.0, 6);

        let decisions: Vec<Decision<u16>> = (0..6).map(|_| buffer.tick()).collect();

        assert_eq!(decisions.first(), Some(&Decision::Play(1)));
        assert_eq!(decisions.get(1), Some(&Decision::Play(2)));
        assert_eq!(
            decisions.get(2),
            Some(&Decision::Conceal),
            "first gap frame"
        );
        assert_eq!(decisions.get(3), Some(&Decision::Silence), "second onwards");
        assert_eq!(decisions.get(4), Some(&Decision::Silence));
        assert_eq!(decisions.get(5), Some(&Decision::Play(6)));

        let metrics = buffer.metrics();
        assert_eq!(metrics.frames_concealed, 1);
        assert_eq!(metrics.frames_silenced, 2);
    }

    #[test]
    fn a_frame_that_arrives_after_its_slot_is_discarded_and_counted() {
        let mut buffer = buffer();
        buffer.push(1, 960, 0.0, 1);
        buffer.push(2, 1_920, 20.0, 2);
        buffer.push(4, 3_840, 60.0, 4);

        // Play through slot 3, concealing it.
        let _ = buffer.tick();
        let _ = buffer.tick();
        let _ = buffer.tick();

        // Frame 3 finally shows up. Too late to be useful.
        buffer.push(3, 2_880, 200.0, 3);

        assert_eq!(buffer.metrics().late_discards, 1);
        assert_eq!(buffer.metrics().frames_concealed, 1);
    }

    #[test]
    fn duplicates_are_ignored() {
        let mut buffer = buffer();
        // Two distinct frames, so the 40 ms prebuffer target is met and playout
        // can begin; the repeats of frame 1 are the thing under test.
        buffer.push(1, 960, 0.0, 1);
        buffer.push(1, 960, 1.0, 1);
        buffer.push(1, 960, 2.0, 1);
        buffer.push(2, 1_920, 20.0, 2);

        assert_eq!(buffer.metrics().duplicates, 2);
        assert_eq!(buffer.tick(), Decision::Play(1));
        assert_eq!(buffer.tick(), Decision::Play(2));
        assert!(matches!(buffer.tick(), Decision::Starved));
    }

    #[test]
    fn target_grows_fast_and_shrinks_slowly() {
        // specs/03-audio.md calls this asymmetry intentional. Prove both halves,
        // because getting only the growth right still sounds fine in testing and
        // wastes latency forever afterwards.
        let mut buffer = buffer();
        let start = buffer.metrics().target_ms;

        // A burst of wildly varying transit times.
        for index in 0..40_u32 {
            let wobble = if index % 2 == 0 { 0.0 } else { 90.0 };
            buffer.push(
                index as u16 + 1,
                index * 960,
                f64::from(index) * 20.0 + wobble,
                index as u16,
            );
            let _ = buffer.tick();
        }
        let grown = buffer.metrics().target_ms;
        assert!(
            grown > start * 2.0,
            "target should have grown quickly: {start} -> {grown}"
        );

        // Now perfectly regular arrivals for the same number of frames.
        for index in 100..140_u32 {
            buffer.push(
                index as u16,
                index * 960,
                f64::from(index) * 20.0,
                index as u16,
            );
            let _ = buffer.tick();
        }
        let after = buffer.metrics().target_ms;
        assert!(
            after > grown * 0.9,
            "shrinking must be slow: {grown} -> {after} in 40 frames"
        );
    }

    #[test]
    fn target_stays_inside_the_spec_bounds() {
        // specs/03-audio.md: adjusts the target between 20 ms and 200 ms.
        let config = JitterConfig::default();
        for profile in [
            NetworkProfile::perfect(),
            NetworkProfile::lan(),
            NetworkProfile::wifi(),
            NetworkProfile::regional(),
            NetworkProfile::mobile_poor(),
        ] {
            let (_, metrics) = drive(profile, 5, 3_000);
            assert!(
                metrics.target_ms >= config.min_target_ms - f64::EPSILON
                    && metrics.target_ms <= config.max_target_ms + f64::EPSILON,
                "target {} outside [{}, {}]",
                metrics.target_ms,
                config.min_target_ms,
                config.max_target_ms
            );
        }
    }

    #[test]
    fn a_wide_jitter_path_holds_more_delay_than_a_quiet_one() {
        let (_, lan) = drive(NetworkProfile::lan(), 9, 3_000);
        let (_, mobile) = drive(NetworkProfile::mobile_poor(), 9, 3_000);
        assert!(
            mobile.target_ms > lan.target_ms,
            "a jittery path must buy more headroom: mobile {} vs lan {}",
            mobile.target_ms,
            lan.target_ms
        );
    }

    #[test]
    fn the_acceptance_profile_stays_mostly_intelligible() {
        // specs/09-roadmap.md accepts M1 on "induced loss of 5% remains
        // intelligible". This cannot judge intelligibility, but it can hold the
        // channel that the buffer is not making it worse: what the network dropped
        // should be roughly what the listener loses, no more.
        let (decisions, metrics) =
            drive(NetworkProfile::acceptance_five_percent_loss(), 77, 10_000);

        let played = count_played(&decisions) as f64;
        let lost = (metrics.frames_concealed + metrics.frames_silenced) as f64;
        let observed = lost / (played + lost);

        assert!(
            observed < 0.08,
            "buffer lost {observed:.3} of slots on a 5% path — it is adding loss"
        );
        assert_eq!(
            metrics.overflow_discards, 0,
            "a 5% path should never overflow a 5 s buffer"
        );
    }

    #[test]
    fn no_profile_makes_the_buffer_panic_or_lose_track() {
        // The closest thing to a property test over the whole space: every
        // profile, several seeds, checking the invariants that must hold no
        // matter what arrives.
        for profile in [
            NetworkProfile::perfect(),
            NetworkProfile::lan(),
            NetworkProfile::wifi(),
            NetworkProfile::regional(),
            NetworkProfile::mobile_poor(),
            NetworkProfile::acceptance_five_percent_loss(),
        ] {
            for seed in [1_u64, 7, 42, 1_000] {
                let (decisions, metrics) = drive(profile, seed, 2_000);

                assert!(!decisions.is_empty());
                assert!(
                    metrics.frames_played <= metrics.frames_received,
                    "played more frames than arrived"
                );
                assert!(metrics.loss_fraction() >= 0.0 && metrics.loss_fraction() <= 1.0);
                assert!(metrics.target_ms.is_finite());
                assert!(metrics.jitter_ms.is_finite() && metrics.jitter_ms >= 0.0);
                assert!(metrics.depth_ms.is_finite() && metrics.depth_ms >= 0.0);
            }
        }
    }

    #[test]
    fn the_buffer_is_deterministic() {
        let first = drive(NetworkProfile::mobile_poor(), 314, 2_000);
        let second = drive(NetworkProfile::mobile_poor(), 314, 2_000);
        assert_eq!(first.0, second.0);
        assert_eq!(first.1, second.1);
    }

    /// Pushes a stream of frames onto a buffer with no network impairment, one
    /// per playout tick, and collects the decisions.
    fn play_stream(
        frames: &[crate::netsim::SentFrame],
        ticks: usize,
    ) -> (Vec<Decision<u16>>, JitterMetrics) {
        let mut buffer = buffer();
        let mut decisions = Vec::new();
        let mut index = 0;
        let mut clock_ms = 0.0_f64;

        for _ in 0..ticks {
            while frames
                .get(index)
                .is_some_and(|frame| frame.sent_at_ms <= clock_ms)
            {
                if let Some(frame) = frames.get(index) {
                    buffer.push(frame.seq, frame.timestamp, frame.sent_at_ms, frame.seq);
                }
                index += 1;
            }
            decisions.push(buffer.tick());
            clock_ms += f64::from(crate::FRAME_MS);
        }
        let metrics = buffer.metrics();
        (decisions, metrics)
    }

    #[test]
    fn dtx_silence_is_not_counted_as_loss() {
        // Gap G5. A talker who stops sends nothing, so the sequence stays
        // contiguous while the timestamp jumps. Reading that as loss would sink
        // the Sync Ratio every time the room goes quiet.
        let mut buffer = buffer();
        let frame_samples = crate::FRAME_SAMPLES as u32;

        buffer.push(1, frame_samples, 0.0, 1);
        buffer.push(2, frame_samples * 2, 20.0, 2);
        // Frame 2 sits at slot 2 and frame 3 at slot 12, so nine slots are
        // silent. The sequence stays contiguous across all of them.
        buffer.push(3, frame_samples * 12, 220.0, 3);

        let decisions: Vec<Decision<u16>> = (0..12).map(|_| buffer.tick()).collect();

        assert_eq!(decisions.first(), Some(&Decision::Play(1)));
        assert_eq!(decisions.get(1), Some(&Decision::Play(2)));
        assert!(
            decisions
                .get(2..11)
                .is_some_and(|slots| slots.iter().all(|decision| *decision == Decision::Comfort)),
            "the silence should be comfort, not concealment: {decisions:?}"
        );
        assert_eq!(decisions.get(11), Some(&Decision::Play(3)));

        let metrics = buffer.metrics();
        assert_eq!(metrics.frames_comfort, 9);
        assert_eq!(metrics.frames_concealed, 0, "silence must not conceal");
        assert_eq!(metrics.frames_silenced, 0);
        assert_eq!(
            metrics.loss_fraction(),
            0.0,
            "a quiet room reported as packet loss"
        );
    }

    #[test]
    fn real_loss_is_still_counted_as_loss() {
        // The control for the test above. Here the sequence skips too, which is
        // the network rather than the talker.
        let mut buffer = buffer();
        let frame_samples = crate::FRAME_SAMPLES as u32;

        buffer.push(1, frame_samples, 0.0, 1);
        buffer.push(2, frame_samples * 2, 20.0, 2);
        // Frames 3, 4, 5 were dropped: sequence jumps to 6 and the timestamp
        // moves by the same three slots.
        buffer.push(6, frame_samples * 6, 100.0, 6);

        let decisions: Vec<Decision<u16>> = (0..6).map(|_| buffer.tick()).collect();

        assert_eq!(decisions.get(2), Some(&Decision::Conceal));
        assert_eq!(decisions.get(3), Some(&Decision::Silence));
        assert_eq!(decisions.get(4), Some(&Decision::Silence));
        assert_eq!(decisions.get(5), Some(&Decision::Play(6)));

        let metrics = buffer.metrics();
        assert_eq!(metrics.frames_comfort, 0);
        assert_eq!(metrics.frames_concealed, 1);
        assert_eq!(metrics.frames_silenced, 2);
        assert!(metrics.loss_fraction() > 0.0);
    }

    #[test]
    fn loss_inside_a_silence_is_separated_exactly() {
        // Both at once, which is the case a simpler rule gets wrong. The
        // timestamp gap gives the total slots; the sequence gap gives how many
        // were lost; the remainder is silence.
        let mut buffer = buffer();
        let frame_samples = crate::FRAME_SAMPLES as u32;

        buffer.push(1, frame_samples, 0.0, 1);
        buffer.push(2, frame_samples * 2, 20.0, 2);
        // Nineteen slots between frame 2 and frame 5, but the sequence only
        // skipped two packets: the talker was quiet for the other seventeen.
        buffer.push(5, frame_samples * 22, 440.0, 5);

        let _decisions: Vec<Decision<u16>> = (0..22).map(|_| buffer.tick()).collect();

        let metrics = buffer.metrics();
        assert_eq!(
            metrics.frames_concealed + metrics.frames_silenced,
            2,
            "lost"
        );
        assert_eq!(metrics.frames_comfort, 17, "silent");
    }

    #[test]
    fn a_pause_does_not_push_playout_permanently_behind() {
        // The failure this is here to stop, found by the M1.16 soak: a hole is
        // only discovered when the frame after it arrives, and by then the
        // buffer has already spent that time starving. Scheduling slots to play
        // the silence *again* costs the length of the pause, every pause, and
        // never gives it back — a conversation with a breath every few seconds
        // ends up seconds behind.
        let mut buffer = buffer();
        let frame = crate::FRAME_SAMPLES as u32;

        // Ten frames of speech, played out.
        for index in 0..10_u16 {
            buffer.push(
                index + 1,
                u32::from(index) * frame,
                f64::from(index) * 20.0,
                index,
            );
        }
        for _ in 0..10 {
            buffer.tick();
        }

        // Fifty ticks of nothing: the talker paused. The listener hears silence
        // as it happens, because there is nothing to play.
        let during: Vec<Decision<u16>> = (0..50).map(|_| buffer.tick()).collect();
        assert!(
            during.iter().all(|d| matches!(d, Decision::Starved)),
            "the buffer had audio it was not playing during the pause"
        );

        // The talker resumes. Sequence is contiguous — DTX sent nothing — and
        // the timestamp has jumped by the whole pause.
        buffer.push(11, 60 * frame, 1_200.0, 99);

        // This tick must play. Anything else is the pause being replayed.
        match buffer.tick() {
            Decision::Play(payload) => assert_eq!(payload, 99),
            other => panic!("the pause was replayed instead of resuming: {other:?}"),
        }
    }

    #[test]
    fn a_quiet_conversation_reports_no_loss_at_all() {
        // The headline case. A talker who speaks in bursts, on a perfect link,
        // must report exactly zero loss — otherwise specs/07-estetica.md's
        // signature metric says the opposite of the truth every time somebody
        // pauses for breath.
        let frames = crate::netsim::talker_stream(25, 50, 20);
        let (_, metrics) = play_stream(&frames, 1_600);

        assert!(metrics.frames_played > 400, "the talker never got heard");
        assert!(
            metrics.frames_comfort > 400,
            "the silence was not recognised"
        );
        assert_eq!(
            metrics.frames_concealed, 0,
            "concealed {} frames on a perfect link",
            metrics.frames_concealed
        );
        assert_eq!(metrics.loss_fraction(), 0.0);
    }

    #[test]
    fn silence_is_not_compressed_into_a_moment() {
        // Playing consecutive sequence numbers back to back would turn a second
        // of silence into 20 ms, which is why playout is indexed by timestamp.
        let frames = crate::netsim::talker_stream(10, 40, 3);
        let (decisions, _) = play_stream(&frames, 200);

        // Counted rather than sliced, because prebuffering shifts playout by a
        // tick and a hard-coded window would test the offset, not the silence.
        let plays_in_first_50 = decisions
            .get(..50)
            .unwrap_or_default()
            .iter()
            .filter(|decision| matches!(decision, Decision::Play(_)))
            .count();
        assert_eq!(
            plays_in_first_50, 10,
            "the first burst is ten frames; more means the silence was compressed"
        );
    }

    #[test]
    fn an_implausible_gap_resyncs_instead_of_grinding() {
        // Not silence and not loss: a receiver that fell badly behind, or a
        // sender whose clock jumped. Walking a million slots would hang playout.
        let mut buffer = buffer();
        let frame_samples = crate::FRAME_SAMPLES as u32;

        buffer.push(1, frame_samples, 0.0, 1);
        buffer.push(2, frame_samples * 2, 20.0, 2);
        let _ = buffer.tick();
        let _ = buffer.tick();

        // An hour into the future.
        buffer.push(3, frame_samples * 180_000, 3_600_000.0, 3);

        let decision = buffer.tick();
        assert_eq!(decision, Decision::Play(3), "should have jumped the gap");
        assert_eq!(buffer.metrics().resyncs, 1);
        assert_eq!(buffer.metrics().frames_comfort, 0);
        assert_eq!(buffer.metrics().frames_concealed, 0);
    }

    #[test]
    fn an_empty_buffer_reports_starved_not_loss() {
        // Starvation before anyone has spoken is not a network fault, and must
        // not show up in the metric that specs/07 puts on screen.
        let mut buffer = buffer();
        for _ in 0..10 {
            assert!(matches!(buffer.tick(), Decision::Starved));
        }
        assert_eq!(buffer.metrics().loss_fraction(), 0.0);
        assert_eq!(buffer.metrics().frames_concealed, 0);
    }

    #[test]
    fn overflow_is_bounded_and_counted() {
        // specs/04-servidor-seele.md guards the server against a flooding sender;
        // the client must not fall over either.
        let mut buffer = buffer();
        for index in 0..(CAPACITY as u32 * 2) {
            buffer.push(
                index as u16 + 1,
                index * 960,
                f64::from(index),
                index as u16,
            );
        }
        assert!(buffer.metrics().overflow_discards > 0);
        assert!(buffer.depth_ms() <= CAPACITY as f64 * f64::from(crate::FRAME_MS));
    }
}
