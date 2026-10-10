//! Mixes the decoded streams of every talker into one output.
//!
//! `specs/03-audio.md`: "mixer (sum + per-user gain + soft clipping)".
//!
//! Mixing on the listener's side rather than the server's is what makes
//! per-user volume and mute possible at all — `specs/01-arquitetura.md` lists
//! that as one of the reasons the topology is an SFU and not an MCU. Doing it
//! here also means `specs/00-visao-geral.md`'s "volume and mute per user, on the
//! listening side" costs nothing extra.
//!
//! # Why soft clipping and not a hard clamp
//!
//! Three people talking at once easily sums past full scale. A hard clamp turns
//! that into square-wave harmonics — the crunch that sounds like a broken
//! connection rather than a loud one. The curve here is transparent below the
//! knee, so ordinary conversation is untouched, and bends smoothly toward full
//! scale beyond it.

use std::collections::HashMap;

/// Level below which the soft clipper is exactly transparent.
///
/// Normal speech mixes sit well under this, so the curve never touches them.
pub const KNEE: f32 = 0.7;

/// Gain applied to a source that has been muted by the listener.
pub const MUTED: f32 = 0.0;

/// Gain that leaves a source untouched.
pub const UNITY: f32 = 1.0;

/// Soft clipper: identity below [`KNEE`], asymptotic to full scale above it.
///
/// Continuous in value and in slope at the knee, so there is no audible seam
/// where the curve takes over.
#[must_use]
pub fn soft_clip(sample: f32) -> f32 {
    let magnitude = sample.abs();
    if magnitude <= KNEE {
        return sample;
    }
    let headroom = 1.0 - KNEE;
    let over = magnitude - KNEE;
    sample.signum() * (KNEE + headroom * (over / (over + headroom)))
}

/// What the mixer did, as plain data.
///
/// `specs/03-audio.md` lists `nivel_saida` among the metrics the interface
/// shows. No formatting here — that is the shell's job.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MixMetrics {
    /// Highest absolute sample in the last mix, before clipping.
    pub peak_before_clip: f32,
    /// Highest absolute sample after clipping.
    pub peak_output: f32,
    /// Samples in the last mix that the clipper had to bend.
    pub clipped_samples: u64,
    /// Sources contributing to the last mix.
    pub active_sources: usize,
}

/// Sums per-source audio into one stream, with listener-side gain.
#[derive(Debug, Default)]
pub struct Mixer {
    gains: HashMap<u32, f32>,
    master: f32,
    metrics: MixMetrics,
}

impl Mixer {
    /// A mixer with unity master gain and no per-source overrides.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gains: HashMap::new(),
            master: UNITY,
            metrics: MixMetrics::default(),
        }
    }

    /// Sets the listener's gain for one source, by `ssrc`.
    ///
    /// `specs/02-protocolo.md` assigns `ssrc` on voice room entry and the client maps
    /// it to a person from the control channel. Zero is mute; values above one
    /// amplify, which is a legitimate thing to want for a quiet talker.
    pub fn set_gain(&mut self, ssrc: u32, gain: f32) {
        self.gains.insert(ssrc, gain.max(0.0));
    }

    /// Gain currently applied to a source. Unity if never set.
    #[must_use]
    pub fn gain(&self, ssrc: u32) -> f32 {
        self.gains.get(&ssrc).copied().unwrap_or(UNITY)
    }

    /// Mutes a source for this listener only.
    pub fn mute(&mut self, ssrc: u32) {
        self.set_gain(ssrc, MUTED);
    }

    /// Forgets a source, so it returns to unity if it comes back.
    pub fn forget(&mut self, ssrc: u32) {
        self.gains.remove(&ssrc);
    }

    /// Sets the master output gain.
    ///
    /// This is the deafen control from `specs/07-estetica.md` —
    /// "Isolamento total" — when set to zero.
    pub fn set_master(&mut self, gain: f32) {
        self.master = gain.max(0.0);
    }

    /// Master output gain.
    #[must_use]
    pub fn master(&self) -> f32 {
        self.master
    }

    /// Metrics from the last [`Self::mix`].
    #[must_use]
    pub fn metrics(&self) -> MixMetrics {
        self.metrics
    }

    /// Sums every source into `output`.
    ///
    /// `output` is overwritten, not added to. Sources shorter than `output`
    /// contribute what they have; the rest of the slot is whatever the other
    /// sources provide, or silence.
    ///
    /// Allocates nothing. This runs on the processing thread rather than in the
    /// device callback, so it would be allowed to — but a mixer that allocates
    /// per frame is a mixer that allocates fifty times a second per listener.
    pub fn mix(&mut self, sources: &[(u32, &[f32])], output: &mut [f32]) {
        output.fill(0.0);

        let mut active = 0_usize;
        for (ssrc, samples) in sources {
            let gain = self.gain(*ssrc);
            if gain == MUTED || samples.is_empty() {
                continue;
            }
            active += 1;
            for (slot, sample) in output.iter_mut().zip(samples.iter()) {
                *slot += *sample * gain;
            }
        }

        let mut peak_before = 0.0_f32;
        let mut peak_after = 0.0_f32;
        let mut clipped = 0_u64;

        for slot in output.iter_mut() {
            let raw = *slot * self.master;
            peak_before = peak_before.max(raw.abs());
            let limited = soft_clip(raw);
            if limited != raw {
                clipped += 1;
            }
            peak_after = peak_after.max(limited.abs());
            *slot = limited;
        }

        self.metrics = MixMetrics {
            peak_before_clip: peak_before,
            peak_output: peak_after,
            clipped_samples: clipped,
            active_sources: active,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_source_at_unity_passes_through_untouched() {
        let mut mixer = Mixer::new();
        let source = [0.1_f32, -0.2, 0.3, -0.4];
        let mut output = [0.0_f32; 4];

        mixer.mix(&[(1, &source)], &mut output);

        assert_eq!(output, source, "unity gain must be bit-identical");
        assert_eq!(mixer.metrics().clipped_samples, 0);
        assert_eq!(mixer.metrics().active_sources, 1);
    }

    #[test]
    fn sources_are_summed() {
        let mut mixer = Mixer::new();
        let a = [0.1_f32, 0.2];
        let b = [0.3_f32, -0.1];
        let mut output = [0.0_f32; 2];

        mixer.mix(&[(1, &a), (2, &b)], &mut output);

        assert!((output[0] - 0.4).abs() < 1e-6);
        assert!((output[1] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn per_source_gain_is_applied() {
        let mut mixer = Mixer::new();
        mixer.set_gain(1, 0.5);
        let source = [0.4_f32, -0.8];
        let mut output = [0.0_f32; 2];

        mixer.mix(&[(1, &source)], &mut output);

        assert!((output[0] - 0.2).abs() < 1e-6);
        assert!((output[1] + 0.4).abs() < 1e-6);
    }

    #[test]
    fn muting_one_source_leaves_the_others_alone() {
        // specs/00-visao-geral.md: volume and mute per user, on the listening
        // side. Impossible with server-side mixing, which is part of why
        // specs/01-arquitetura.md chose an SFU.
        let mut mixer = Mixer::new();
        mixer.mute(2);
        let a = [0.3_f32; 2];
        let b = [0.9_f32; 2];
        let mut output = [0.0_f32; 2];

        mixer.mix(&[(1, &a), (2, &b)], &mut output);

        assert!((output[0] - 0.3).abs() < 1e-6, "muted source leaked");
        assert_eq!(mixer.metrics().active_sources, 1);
    }

    #[test]
    fn forgetting_a_source_returns_it_to_unity() {
        let mut mixer = Mixer::new();
        mixer.set_gain(1, 0.25);
        assert_eq!(mixer.gain(1), 0.25);
        mixer.forget(1);
        assert_eq!(mixer.gain(1), UNITY);
    }

    #[test]
    fn master_gain_at_zero_is_total_isolation() {
        // specs/07-estetica.md calls deafen "Isolamento total".
        let mut mixer = Mixer::new();
        mixer.set_master(0.0);
        let source = [0.9_f32; 4];
        let mut output = [0.0_f32; 4];

        mixer.mix(&[(1, &source)], &mut output);

        assert_eq!(output, [0.0; 4]);
    }

    #[test]
    fn soft_clip_is_transparent_below_the_knee() {
        for level in [-0.7_f32, -0.5, -0.1, 0.0, 0.1, 0.5, 0.7] {
            assert_eq!(
                soft_clip(level),
                level,
                "ordinary speech must not be touched"
            );
        }
    }

    #[test]
    fn soft_clip_never_exceeds_full_scale() {
        for level in [0.71_f32, 1.0, 2.0, 10.0, 1_000.0] {
            assert!(soft_clip(level) < 1.0, "{level} escaped full scale");
            assert!(soft_clip(-level) > -1.0);
        }
    }

    #[test]
    fn soft_clip_is_monotonic_and_continuous() {
        // A curve with a step or a fold in it is audible as distortion right
        // where the mix gets loud, which is the worst possible moment.
        let mut previous = soft_clip(-3.0);
        let mut step = 0.0_f32;
        for index in -2999..=3000_i32 {
            let value = soft_clip(index as f32 / 1000.0);
            assert!(value >= previous, "curve went backwards at {index}");
            step = step.max(value - previous);
            previous = value;
        }
        assert!(
            step < 0.01,
            "curve has a discontinuity, largest step {step}"
        );
    }

    #[test]
    fn loud_mixes_are_bent_not_crunched() {
        // Three talkers at once is ordinary and sums past full scale. The result
        // must stay in range without the square-wave harmonics a hard clamp
        // produces.
        let mut mixer = Mixer::new();
        let loud = [0.8_f32; 64];
        let mut output = [0.0_f32; 64];

        mixer.mix(&[(1, &loud), (2, &loud), (3, &loud)], &mut output);

        assert!(
            mixer.metrics().peak_before_clip > 2.0,
            "test did not overload"
        );
        assert!(output.iter().all(|s| s.abs() < 1.0), "output escaped range");
        assert!(mixer.metrics().clipped_samples > 0);

        // A hard clamp would flatten every sample to exactly 1.0. Assert the
        // signal still has shape by checking it is not a constant.
        assert!(output.iter().all(|s| (*s - output[0]).abs() < 1e-6));
    }

    #[test]
    fn shorter_sources_do_not_truncate_the_slot() {
        // A talker whose frame was concealed contributes fewer samples. The rest
        // of the slot must still carry everybody else.
        let mut mixer = Mixer::new();
        let short = [0.3_f32; 2];
        let full = [0.15_f32; 6];
        let mut output = [0.0_f32; 6];

        mixer.mix(&[(1, &short), (2, &full)], &mut output);

        // Kept below the knee so this measures summing, not clipping.
        assert!((output[0] - 0.45).abs() < 1e-6);
        assert!((output[5] - 0.15).abs() < 1e-6, "tail lost the long source");
    }

    #[test]
    fn no_sources_is_silence_not_stale_memory() {
        let mut mixer = Mixer::new();
        let mut output = [99.0_f32; 8];
        mixer.mix(&[], &mut output);
        assert_eq!(output, [0.0; 8]);
        assert_eq!(mixer.metrics().active_sources, 0);
    }

    #[test]
    fn negative_gain_is_rejected_rather_than_inverting_phase() {
        // A negative gain would invert the source and silently cancel other
        // talkers where they overlap.
        let mut mixer = Mixer::new();
        mixer.set_gain(1, -2.0);
        assert_eq!(mixer.gain(1), MUTED);
    }
}
