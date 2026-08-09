//! Measuring how long a sound takes to come back.
//!
//! Task M1.2, the measurement rig. The analysis lives here — pure, and tested
//! against signals whose delay is known by construction. The part that needs a
//! sound card and a cable is `examples/latencia.rs`, which uses this.
//!
//! # The method
//!
//! Emit a click. Record. Find where the click landed in the recording by
//! cross-correlation, and the offset is the round trip through output buffer,
//! DAC, cable or air, ADC and input buffer.
//!
//! Cross-correlation rather than "find the first loud sample" because a
//! threshold is fooled by three things that all happen in practice: room noise
//! above the threshold, a click attenuated below it, and the slow rise of a
//! band-limited system. Correlation uses the whole shape and degrades into a
//! lower confidence score instead of a wrong answer.
//!
//! # Two rigs, and why both
//!
//! - **By cable.** Output to input directly. Measures the machine: buffers,
//!   driver, and the resampling in [`crate::resample`]. Repeatable to a sample.
//! - **Acoustically.** Speaker to microphone. Measures what a person
//!   experiences, and includes about 3 ms per metre of air that no amount of
//!   software will remove.
//!
//! The cable number is the one to optimise. The acoustic number is the one to
//! report, because ADR 0009's budget is mouth-to-ear.

/// What a measurement found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Delay {
    /// How many samples late the echo was.
    pub samples: usize,
    /// The same, in milliseconds.
    pub millis: f64,
    /// How peaked the correlation was, as a ratio to its own mean.
    ///
    /// One means the correlation is flat — no click was found, only noise that
    /// happened to correlate somewhere. Anything below [`MIN_CONFIDENCE`] is
    /// reported as no measurement rather than as a number somebody might write
    /// down.
    pub confidence: f32,
}

/// Below this, a measurement is noise finding a shape in noise.
///
/// Chosen from the synthetic cases: a click at −20 dB under white noise scores
/// well above this, and pure noise against a click scores just above 1.
pub const MIN_CONFIDENCE: f32 = 4.0;

/// A click: a short burst with energy across the band.
///
/// Not a single impulse. One sample has almost no energy, so it disappears into
/// the noise floor of any real microphone; and a band-limited system smears it
/// into something a threshold cannot find. A few milliseconds of alternating
/// full-scale samples is loud, broadband, and still short enough that the
/// correlation peak stays sharp.
#[must_use]
pub fn click(samples: usize) -> Vec<f32> {
    (0..samples)
        .map(|index| {
            // A Hann window keeps the edges from ringing, which would spread
            // the correlation peak.
            #[allow(clippy::cast_precision_loss, reason = "a few hundred samples")]
            let phase = index as f32 / samples.max(1) as f32;
            let window = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
            let alternating = if index % 2 == 0 { 1.0 } else { -1.0 };
            alternating * window
        })
        .collect()
}

/// Finds where `reference` occurs inside `recorded`.
///
/// Returns `None` when nothing correlates well enough to be worth a number —
/// see [`MIN_CONFIDENCE`]. A rig that reports a confident-looking figure from a
/// disconnected cable is worse than one that reports nothing.
///
/// Normalised by the energy under each window, so a quiet echo scores the same
/// as a loud one. That matters: an acoustic rig at any sensible volume records
/// the click far below full scale.
#[must_use]
pub fn find_delay(reference: &[f32], recorded: &[f32], sample_rate_hz: u32) -> Option<Delay> {
    if reference.is_empty() || recorded.len() < reference.len() || sample_rate_hz == 0 {
        return None;
    }

    let last_offset = recorded.len() - reference.len();
    let mut scores = Vec::with_capacity(last_offset + 1);

    for offset in 0..=last_offset {
        let window = recorded.get(offset..offset + reference.len())?;
        let mut dot = 0.0_f64;
        let mut energy = 0.0_f64;
        for (a, b) in reference.iter().zip(window) {
            dot += f64::from(*a) * f64::from(*b);
            energy += f64::from(*b) * f64::from(*b);
        }
        // The absolute value: a cable or a speaker can invert polarity, and an
        // upside-down click is still the click.
        let score = if energy > f64::EPSILON {
            dot.abs() / energy.sqrt()
        } else {
            0.0
        };
        scores.push(score);
    }

    let (best, peak) = scores
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;

    #[allow(clippy::cast_precision_loss, reason = "a count of correlation offsets")]
    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
    if mean <= f64::EPSILON {
        return None;
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "a ratio, reported to a human"
    )]
    let confidence = (peak / mean) as f32;
    if confidence < MIN_CONFIDENCE {
        return None;
    }

    Some(Delay {
        samples: best,
        millis: best as f64 / f64::from(sample_rate_hz) * 1000.0,
        confidence,
    })
}

/// What a run of measurements came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Every delay that was measured, in order.
    pub measurements: Vec<f64>,
    /// Attempts that found nothing.
    pub failed: usize,
}

impl Report {
    /// Collects a run.
    #[must_use]
    pub fn new(measurements: Vec<f64>, failed: usize) -> Self {
        Self {
            measurements,
            failed,
        }
    }

    /// The middle measurement.
    ///
    /// The median rather than the mean, because a run of twenty includes the
    /// occasional outlier from a scheduling hiccup, and one 400 ms sample would
    /// move a mean somewhere no individual measurement ever was.
    #[must_use]
    pub fn median_ms(&self) -> Option<f64> {
        if self.measurements.is_empty() {
            return None;
        }
        let mut sorted = self.measurements.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted.get(sorted.len() / 2).copied()
    }

    /// The spread, as the gap between the fastest and slowest.
    ///
    /// Worth reporting beside the median: a machine whose measurements range
    /// over 30 ms has a scheduling problem that a single number would hide.
    #[must_use]
    pub fn spread_ms(&self) -> Option<f64> {
        let min = self
            .measurements
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let max = self
            .measurements
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        (min.is_finite() && max.is_finite()).then_some(max - min)
    }

    /// Whether the run is worth believing.
    ///
    /// A rig that half-works produces a plausible median from three good
    /// measurements and seventeen failures. Saying so is the point of the rig.
    #[must_use]
    pub fn trustworthy(&self) -> bool {
        self.measurements.len() >= 5 && self.failed <= self.measurements.len() / 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Deterministic pseudo-noise. `rand` is avoided across this workspace so
    /// that a version bump cannot change what a test measures.
    fn noise(count: usize, amplitude: f32, seed: u64) -> Vec<f32> {
        let mut state = seed | 1;
        (0..count)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                #[allow(clippy::cast_precision_loss, reason = "pseudo-noise, not arithmetic")]
                let unit = ((state >> 33) as f32 / (1_u64 << 31) as f32) - 0.5;
                unit * 2.0 * amplitude
            })
            .collect()
    }

    /// A recording with the click placed at a known offset.
    fn recording(delay: usize, gain: f32, noise_level: f32, invert: bool) -> (Vec<f32>, Vec<f32>) {
        let reference = click(240); // 5 ms at 48 kHz
        let mut recorded = noise(delay + reference.len() + 4_800, noise_level, 7);
        for (index, sample) in reference.iter().enumerate() {
            if let Some(slot) = recorded.get_mut(delay + index) {
                *slot += sample * gain * if invert { -1.0 } else { 1.0 };
            }
        }
        (reference, recorded)
    }

    #[test]
    fn a_click_is_found_where_it_was_put() {
        let (reference, recorded) = recording(1_234, 0.5, 0.001, false);
        let delay = find_delay(&reference, &recorded, RATE).expect("a delay");

        assert_eq!(delay.samples, 1_234);
        assert!((delay.millis - 25.708).abs() < 0.01, "{}", delay.millis);
    }

    #[test]
    fn an_inverted_click_is_still_the_click() {
        // A cable wired the other way, or a speaker with reversed polarity.
        // Reporting no measurement here would send somebody looking for a
        // software fault in their wiring.
        let (reference, recorded) = recording(900, 0.5, 0.001, true);
        assert_eq!(
            find_delay(&reference, &recorded, RATE).map(|d| d.samples),
            Some(900)
        );
    }

    #[test]
    fn a_quiet_echo_is_found_as_well_as_a_loud_one() {
        // An acoustic rig at any sensible volume records far below full scale.
        // Normalisation is what makes the two cases the same measurement.
        for gain in [1.0, 0.1, 0.01] {
            let (reference, recorded) = recording(2_000, gain, 0.0005, false);
            assert_eq!(
                find_delay(&reference, &recorded, RATE).map(|d| d.samples),
                Some(2_000),
                "gain {gain}"
            );
        }
    }

    #[test]
    fn noise_alone_produces_no_measurement() {
        // The failure that matters. A disconnected cable must report nothing,
        // not a confident number somebody writes into a document.
        let reference = click(240);
        let recorded = noise(48_000, 0.2, 99);

        assert_eq!(find_delay(&reference, &recorded, RATE), None);
    }

    #[test]
    fn a_click_buried_in_loud_noise_is_reported_with_lower_confidence() {
        let (reference, clean) = recording(1_500, 0.5, 0.0005, false);
        let (_, noisy) = recording(1_500, 0.5, 0.05, false);

        let clean = find_delay(&reference, &clean, RATE).expect("clean");
        let noisy = find_delay(&reference, &noisy, RATE).expect("noisy");

        assert_eq!(clean.samples, 1_500);
        assert_eq!(noisy.samples, 1_500);
        assert!(
            noisy.confidence < clean.confidence,
            "noise did not cost confidence: {} vs {}",
            noisy.confidence,
            clean.confidence
        );
    }

    #[test]
    fn a_recording_shorter_than_the_click_is_no_measurement() {
        assert_eq!(find_delay(&click(240), &[0.0; 10], RATE), None);
        assert_eq!(find_delay(&[], &[0.0; 1000], RATE), None);
    }

    #[test]
    fn the_median_ignores_one_wild_outlier() {
        // One scheduling hiccup in twenty is normal. A mean would move to
        // somewhere no individual measurement ever was.
        let report = Report::new(vec![20.0, 21.0, 20.5, 22.0, 400.0], 0);

        assert_eq!(report.median_ms(), Some(21.0));
        assert!(report.spread_ms().expect("spread") > 300.0);
    }

    #[test]
    fn a_run_that_mostly_failed_is_not_trustworthy() {
        // Three good measurements and seventeen failures produce a plausible
        // median. Saying so is what the rig is for.
        assert!(!Report::new(vec![20.0, 21.0, 20.5], 17).trustworthy());
        assert!(!Report::new(vec![20.0], 0).trustworthy());
        assert!(Report::new(vec![20.0, 21.0, 20.5, 22.0, 20.1], 1).trustworthy());
    }

    #[test]
    fn an_empty_run_reports_nothing_rather_than_zero() {
        let report = Report::new(Vec::new(), 10);
        assert_eq!(report.median_ms(), None);
        assert_eq!(report.spread_ms(), None);
        assert!(!report.trustworthy());
    }
}
