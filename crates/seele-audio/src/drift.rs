//! Measures and corrects clock drift between a sender and this machine.
//!
//! This is risk R3 in `docs/plano-m0-m1.md`, and it appears in no spec at all —
//! which is precisely why it is dangerous. Two devices both claiming 48 kHz
//! actually run at slightly different rates; consumer crystals are specified to
//! something like ±50 ppm and the two ends never agree. At 100 ppm the skew is
//! 60 ms over ten minutes, which is an entire jitter buffer. The buffer either
//! fills until it overflows or drains until it underruns, and either way it
//! surfaces as intermittent clicks after several minutes of a call that started
//! fine — the most expensive symptom to diagnose in audio.
//!
//! `specs/09-roadmap.md` asks for "no clicking in 10 continuous minutes". That
//! duration is not arbitrary; ten minutes is roughly when drift arrives.
//!
//! # Measuring it: the minimum, not the average
//!
//! Transit time is `arrival − timestamp`, and it is polluted by jitter. But
//! **jitter only ever adds delay** — a packet cannot arrive before the path
//! allows — so the *minimum* transit over a window approximates the true path
//! delay, and a trend in that minimum is drift rather than jitter. Averaging
//! instead would track the jitter distribution's mean, which moves for reasons
//! that have nothing to do with clocks.
//!
//! Sign convention: a **positive** drift means the sender runs **fast**. Its
//! timestamps advance faster than real time, so transit shrinks, and a receiver
//! accumulates audio it must consume faster.
//!
//! # Correcting it: resample, do not drop
//!
//! The correction is a ratio a few parts per million away from 1.0, applied
//! through [`crate::resample::RateConverter::adjust_ratio`]. That hook exists
//! for this; [`crate::resample::RateConverter::new_adjustable`] keeps a filter
//! even at matching rates so there is something to steer.
//!
//! The alternative — dropping or inserting whole frames — costs a 20 ms glitch
//! every time. Resampling by 100 ppm is a pitch change of roughly 0.002 of a
//! semitone, which nothing can hear.

/// How long a measurement window lasts, in milliseconds.
///
/// Long enough that the minimum is a good estimate of the path floor and that a
/// real drift moves it measurably: 100 ppm over ten seconds is 1 ms, against a
/// minimum whose own noise is far smaller.
const WINDOW_MS: f64 = 10_000.0;

/// Frames a window needs before it is trusted.
///
/// Ten seconds at 50 frames a second is 500; requiring most of them keeps a
/// window full of silence from producing a confident estimate from three
/// samples.
const MIN_WINDOW_FRAMES: u64 = 200;

/// Smoothing on the drift estimate, per completed window.
///
/// Drift is a physical property of two crystals and does not change; the only
/// reason to move slowly is measurement noise.
const SMOOTHING: f64 = 0.25;

/// Largest correction that will ever be applied, in parts per million.
///
/// Well past any real crystal. A larger apparent drift means something else is
/// wrong — a sender resetting its clock, a stream restarting — and steering
/// hard on that would make things worse.
const MAX_CORRECTION_PPM: f64 = 500.0;

/// Drift below which no correction is applied, in parts per million.
///
/// Ten ppm is 0.6 ms per minute, which the jitter buffer absorbs without
/// noticing. Correcting inside the noise floor just makes the resampler chase
/// its own measurement error.
const DEADBAND_PPM: f64 = 10.0;

/// What the tracker has worked out.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DriftMetrics {
    /// Estimated drift in parts per million. Positive means the sender is fast.
    pub drift_ppm: f64,
    /// Windows completed. No estimate exists until this reaches two.
    pub windows: u64,
    /// Whether the estimate is currently being acted on.
    pub correcting: bool,
    /// Total skew accumulated so far, in milliseconds, as observed.
    pub observed_skew_ms: f64,
}

/// One completed measurement window.
#[derive(Debug, Clone, Copy)]
struct Window {
    min_transit_ms: f64,
    at_ms: f64,
}

/// Tracks the clock offset between one sender and this machine.
///
/// One per source: every talker has their own crystal.
#[derive(Debug)]
pub struct DriftTracker {
    window_min_ms: f64,
    window_started_at_ms: Option<f64>,
    window_frames: u64,
    baseline: Option<Window>,
    drift_ppm: f64,
    windows: u64,
    first_transit_ms: Option<f64>,
    last_transit_ms: f64,
}

impl Default for DriftTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl DriftTracker {
    /// A tracker that has not seen anything yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            window_min_ms: f64::MAX,
            window_started_at_ms: None,
            window_frames: 0,
            baseline: None,
            drift_ppm: 0.0,
            windows: 0,
            first_transit_ms: None,
            last_transit_ms: 0.0,
        }
    }

    /// Current estimate and status.
    #[must_use]
    pub fn metrics(&self) -> DriftMetrics {
        DriftMetrics {
            drift_ppm: self.drift_ppm,
            windows: self.windows,
            correcting: self.drift_ppm.abs() >= DEADBAND_PPM,
            observed_skew_ms: self
                .first_transit_ms
                .map_or(0.0, |first| first - self.last_transit_ms),
        }
    }

    /// Estimated drift in parts per million. Positive means the sender is fast.
    #[must_use]
    pub fn drift_ppm(&self) -> f64 {
        self.drift_ppm
    }

    /// The ratio to hand to [`crate::resample::RateConverter::adjust_ratio`].
    ///
    /// Returns `None` while the drift is inside the deadband or not yet
    /// measured, so a caller can skip the adjustment entirely rather than apply
    /// a no-op.
    ///
    /// A sender running fast produces a ratio above 1.0: the receiver has to
    /// consume its stream faster than nominal or the buffer grows without bound.
    #[must_use]
    pub fn correction_ratio(&self) -> Option<f64> {
        if self.windows < 2 || self.drift_ppm.abs() < DEADBAND_PPM {
            return None;
        }
        let bounded = self
            .drift_ppm
            .clamp(-MAX_CORRECTION_PPM, MAX_CORRECTION_PPM);
        Some(1.0 + bounded / 1_000_000.0)
    }

    /// Records one frame's transit time.
    ///
    /// `transit_ms` is `arrival − timestamp`, the same quantity the jitter
    /// buffer already computes for its RFC 3550 estimate. `now_ms` is the
    /// receiver's clock.
    pub fn observe(&mut self, transit_ms: f64, now_ms: f64) {
        if self.first_transit_ms.is_none() {
            self.first_transit_ms = Some(transit_ms);
        }
        self.last_transit_ms = transit_ms;

        let started = *self.window_started_at_ms.get_or_insert(now_ms);
        self.window_min_ms = self.window_min_ms.min(transit_ms);
        self.window_frames += 1;

        if now_ms - started < WINDOW_MS {
            return;
        }

        // A window nobody spoke through says nothing about clocks.
        if self.window_frames >= MIN_WINDOW_FRAMES {
            self.close_window(now_ms);
        }

        self.window_started_at_ms = Some(now_ms);
        self.window_min_ms = f64::MAX;
        self.window_frames = 0;
    }

    fn close_window(&mut self, now_ms: f64) {
        let current = Window {
            min_transit_ms: self.window_min_ms,
            at_ms: now_ms,
        };

        if let Some(previous) = self.baseline {
            let elapsed_ms = current.at_ms - previous.at_ms;
            if elapsed_ms > 0.0 {
                // Transit shrinking means the sender is ahead, hence the sign.
                let slope = (current.min_transit_ms - previous.min_transit_ms) / elapsed_ms;
                let measured_ppm = -slope * 1_000_000.0;
                self.drift_ppm += (measured_ppm - self.drift_ppm) * SMOOTHING;
            }
        }

        self.baseline = Some(current);
        self.windows += 1;
    }

    /// Forgets everything. For a stream that restarted, where the old baseline
    /// describes a clock relationship that no longer exists.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netsim::{drifting_stream, Network, NetworkProfile};

    /// Feeds a drifting sender through a network into a fresh tracker.
    fn track(drift_ppm: f64, minutes: usize, profile: NetworkProfile, seed: u64) -> DriftTracker {
        let frames = drifting_stream(minutes * 3_000, drift_ppm);
        let mut network = Network::new(profile, seed);
        let mut tracker = DriftTracker::new();
        let mut arrivals = Vec::new();

        for frame in frames {
            arrivals.clear();
            network.transmit(frame, &mut arrivals);
            for arrival in &arrivals {
                let sent_ms =
                    f64::from(arrival.timestamp) / f64::from(crate::SAMPLE_RATE_HZ) * 1000.0;
                tracker.observe(arrival.arrived_at_ms - sent_ms, arrival.arrived_at_ms);
            }
        }
        tracker
    }

    #[test]
    fn a_perfect_clock_reads_as_no_drift() {
        let tracker = track(0.0, 5, NetworkProfile::perfect(), 1);
        assert!(
            tracker.drift_ppm().abs() < DEADBAND_PPM,
            "invented {} ppm of drift on a perfect clock",
            tracker.drift_ppm()
        );
        assert_eq!(tracker.correction_ratio(), None, "corrected nothing");
    }

    #[test]
    fn a_fast_sender_is_measured_as_positive_drift() {
        let tracker = track(100.0, 5, NetworkProfile::perfect(), 1);
        let measured = tracker.drift_ppm();
        assert!(
            (measured - 100.0).abs() < 20.0,
            "expected about +100 ppm, measured {measured:.1}"
        );
    }

    #[test]
    fn a_slow_sender_is_measured_as_negative_drift() {
        let tracker = track(-100.0, 5, NetworkProfile::perfect(), 1);
        let measured = tracker.drift_ppm();
        assert!(
            (measured + 100.0).abs() < 20.0,
            "expected about -100 ppm, measured {measured:.1}"
        );
    }

    #[test]
    fn jitter_does_not_masquerade_as_drift() {
        // The reason the estimator uses the minimum rather than the mean. A
        // jittery path with a perfect clock must still read as no drift.
        for profile in [
            NetworkProfile::wifi(),
            NetworkProfile::regional(),
            NetworkProfile::mobile_poor(),
        ] {
            let tracker = track(0.0, 5, profile, 7);
            assert!(
                tracker.drift_ppm().abs() < 25.0,
                "jitter produced {:.1} ppm of phantom drift",
                tracker.drift_ppm()
            );
        }
    }

    #[test]
    fn drift_is_still_found_underneath_heavy_jitter_and_loss() {
        // The realistic case: a bad mobile link *and* a bad crystal.
        let tracker = track(150.0, 8, NetworkProfile::mobile_poor(), 3);
        let measured = tracker.drift_ppm();
        assert!(
            measured > 60.0,
            "drift was lost under the jitter: measured {measured:.1} ppm"
        );
    }

    #[test]
    fn the_correction_ratio_points_the_right_way() {
        // A sender running fast fills the buffer, so the receiver must consume
        // faster: a ratio above one. Getting this backwards would double the
        // drift instead of cancelling it, and the symptom would look identical.
        let fast = track(120.0, 5, NetworkProfile::perfect(), 1);
        let ratio = fast.correction_ratio().unwrap_or(1.0);
        assert!(ratio > 1.0, "fast sender produced ratio {ratio}");
        assert!(ratio < 1.001, "correction is implausibly large: {ratio}");

        let slow = track(-120.0, 5, NetworkProfile::perfect(), 1);
        let ratio = slow.correction_ratio().unwrap_or(1.0);
        assert!(ratio < 1.0, "slow sender produced ratio {ratio}");
    }

    #[test]
    fn small_drift_is_left_alone() {
        // Correcting inside the noise floor makes the resampler chase its own
        // measurement error. Five ppm is 0.3 ms per minute; the buffer absorbs it.
        let tracker = track(5.0, 5, NetworkProfile::perfect(), 1);
        assert_eq!(
            tracker.correction_ratio(),
            None,
            "corrected {} ppm, which is inside the deadband",
            tracker.drift_ppm()
        );
    }

    #[test]
    fn an_absurd_drift_is_clamped_rather_than_obeyed() {
        // A sender that resets its clock looks like enormous drift for one
        // window. Steering hard on that makes the audio worse than the glitch.
        let mut tracker = DriftTracker::new();
        tracker.drift_ppm = 50_000.0;
        tracker.windows = 5;

        let ratio = tracker.correction_ratio().unwrap_or(1.0);
        assert!(
            (ratio - (1.0 + MAX_CORRECTION_PPM / 1_000_000.0)).abs() < 1e-12,
            "clamp did not hold: {ratio}"
        );
    }

    #[test]
    fn no_estimate_exists_before_two_windows() {
        // One window gives a minimum but no slope. Reporting a number from that
        // would be reporting the path delay as if it were drift.
        let mut tracker = DriftTracker::new();
        for index in 0..400_u32 {
            tracker.observe(20.0, f64::from(index) * 20.0);
        }
        assert!(tracker.metrics().windows < 2);
        assert_eq!(tracker.correction_ratio(), None);
    }

    #[test]
    fn a_silent_window_does_not_produce_an_estimate() {
        // With DTX a talker may send almost nothing for ten seconds. Three
        // frames must not be treated as a measurement.
        let mut tracker = DriftTracker::new();
        tracker.observe(20.0, 0.0);
        tracker.observe(20.0, 5_000.0);
        tracker.observe(20.0, 11_000.0);
        tracker.observe(20.0, 22_000.0);
        assert_eq!(
            tracker.metrics().windows,
            0,
            "estimated from a silent window"
        );
    }

    #[test]
    fn reset_forgets_a_stream_that_restarted() {
        let mut tracker = track(100.0, 5, NetworkProfile::perfect(), 1);
        assert!(tracker.drift_ppm().abs() > DEADBAND_PPM);

        tracker.reset();

        assert_eq!(tracker.drift_ppm(), 0.0);
        assert_eq!(tracker.metrics().windows, 0);
        assert_eq!(tracker.correction_ratio(), None);
    }

    /// Runs a drifting sender into a jitter buffer for nine minutes and reports
    /// how much the buffer depth moved between minute one and minute nine.
    ///
    /// Scaling the consumption clock by the correction ratio is exactly what the
    /// resampler does to the stream, so this measures the real effect rather
    /// than describing it.
    fn depth_creep_ms(drift_ppm: f64, correct: bool) -> f64 {
        use crate::jitter::{JitterBuffer, JitterConfig};

        let frames = crate::netsim::drifting_stream(30_000, drift_ppm);
        let mut network = Network::new(NetworkProfile::lan(), 5);
        let mut arrivals = Vec::new();
        for frame in frames {
            network.transmit(frame, &mut arrivals);
        }
        network.sort_arrivals(&mut arrivals);

        let mut buffer: JitterBuffer<u16> = JitterBuffer::new(JitterConfig::default());
        let mut tracker = DriftTracker::new();
        let mut clock_ms = 0.0_f64;
        let mut index = 0;
        let (mut early, mut late) = (0.0_f64, 0.0_f64);

        for tick in 0..=27_000_usize {
            while arrivals
                .get(index)
                .is_some_and(|frame| frame.arrived_at_ms <= clock_ms)
            {
                if let Some(frame) = arrivals.get(index) {
                    buffer.push(frame.seq, frame.timestamp, frame.arrived_at_ms, frame.seq);
                    let sent_ms =
                        f64::from(frame.timestamp) / f64::from(crate::SAMPLE_RATE_HZ) * 1000.0;
                    tracker.observe(frame.arrived_at_ms - sent_ms, frame.arrived_at_ms);
                }
                index += 1;
            }
            let _ = buffer.tick();
            if tick == 3_000 {
                early = buffer.depth_ms();
            }
            if tick == 27_000 {
                late = buffer.depth_ms();
            }
            let ratio = if correct {
                tracker.correction_ratio().unwrap_or(1.0)
            } else {
                1.0
            };
            clock_ms += f64::from(crate::FRAME_MS) / ratio;
        }
        late - early
    }

    #[test]
    fn uncorrected_drift_makes_latency_creep() {
        // The failure R3 describes. A fast sender fills the buffer and latency
        // climbs; a slow one drains it until playout starves. Both start sounding
        // wrong minutes into a call that began fine.
        assert!(
            depth_creep_ms(100.0, false) >= 20.0,
            "a fast sender should have accumulated audio"
        );
        assert!(
            depth_creep_ms(-100.0, false) <= -10.0,
            "a slow sender should have drained the buffer"
        );
    }

    #[test]
    fn correction_holds_the_buffer_steady() {
        // The fix, and the end-to-end guard on the sign convention: getting the
        // direction backwards would double the drift rather than cancel it, and
        // the symptom would look identical to having no correction at all.
        for drift_ppm in [50.0, 100.0, 200.0, -100.0, -200.0] {
            let creep = depth_creep_ms(drift_ppm, true);
            assert!(
                creep.abs() < 20.0,
                "{drift_ppm} ppm still crept {creep:.1} ms with correction on"
            );
        }
    }

    #[test]
    fn ten_minutes_at_a_realistic_drift_is_a_whole_jitter_buffer() {
        // Not a test of the tracker so much as of why it exists. specs/09 asks
        // for ten minutes without clicking, and at 100 ppm that is 60 ms of
        // accumulated skew against a buffer that settles at 20 ms on LAN.
        let tracker = track(100.0, 10, NetworkProfile::lan(), 5);
        let skew = tracker.metrics().observed_skew_ms;
        assert!(
            skew > 40.0,
            "ten minutes at 100 ppm should skew tens of ms, saw {skew:.1}"
        );
    }
}
