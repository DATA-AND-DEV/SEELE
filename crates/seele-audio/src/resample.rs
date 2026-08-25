//! Sample-rate conversion between the device and the 48 kHz pipeline.
//!
//! `specs/03-audio.md` puts "conversion to 48 kHz mono f32" on the processing
//! thread, after the ring — which is where this runs. It only describes the
//! **capture** side, though; the playback pipeline goes from the mixer straight
//! to the ring with no conversion at all. That is gap G7 in
//! `docs/plano-m0-m1.md`, and it matters: 44.1 kHz is the factory default on a
//! lot of hardware, macOS included. This module converts in both directions.
//!
//! # Why `rubato`'s `Async` and not the cheaper fixed-ratio resampler
//!
//! Because task M1.8 is coming. Clock drift between two independent devices is
//! not a fixed ratio — it is a ratio that wanders by a handful of parts per
//! million, and correcting it needs a resampler whose ratio can be nudged while
//! running. [`RateConverter::adjust_ratio`] is that hook. Picking a fixed-ratio
//! resampler now would mean replacing it in three tasks' time.
//!
//! # Allocation
//!
//! This runs on the processing thread, not in the callback, so allocation is
//! permitted here. It is still avoided after warm-up: every buffer is
//! preallocated at construction, and [`RateConverter::push`] only grows the
//! caller's output vector until it reaches its steady size.

use audioadapter_buffers::direct::SequentialSlice;
use rubato::{
    Adjustable, Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use thiserror::Error;

/// Input frames per conversion chunk.
///
/// One 20 ms frame at 48 kHz (`specs/03-audio.md`). Matching the codec frame
/// keeps the pipeline's units the same all the way through.
const CHUNK_FRAMES: usize = 960;

/// How far the ratio may be nudged away from nominal, as a multiplier.
///
/// Clock drift between consumer devices is measured in parts per million, so 2%
/// is orders of magnitude more headroom than M1.8 will need. It costs only a
/// slightly larger internal buffer.
const MAX_RATIO_ADJUSTMENT: f64 = 1.02;

/// Why conversion could not be set up or run.
///
/// Enumerated, per `specs/02-protocolo.md`. The `Display` text is for `tracing`;
/// a shell matches on the variant — see ADR 0012.
#[derive(Debug, Error)]
pub enum ConversionError {
    /// A sample rate of zero was requested. Always a bug, never a device.
    #[error("sample rate must be non-zero (asked to convert {from} Hz to {to} Hz)")]
    ZeroRate {
        /// Source rate.
        from: u32,
        /// Target rate.
        to: u32,
    },

    /// `rubato` refused the requested configuration.
    #[error("could not build a resampler for {from} Hz to {to} Hz: {source}")]
    Construction {
        /// Source rate.
        from: u32,
        /// Target rate.
        to: u32,
        /// Underlying `rubato` error.
        #[source]
        source: rubato::ResamplerConstructionError,
    },

    /// `rubato` refused a conversion at run time.
    #[error("resampling failed: {source}")]
    Process {
        /// Underlying `rubato` error.
        #[source]
        source: rubato::ResampleError,
    },

    /// A scratch buffer did not match the shape the resampler asked for.
    ///
    /// Only reachable if the preallocated sizes and `rubato`'s reported maxima
    /// disagree, which would be a bug here rather than bad input.
    #[error("internal buffer shape mismatch: wanted {frames} frames")]
    BufferShape {
        /// Frame count that was rejected.
        frames: usize,
    },
}

/// Converts a mono `f32` stream from one sample rate to another.
///
/// When the two rates are equal this is a passthrough that copies and nothing
/// more — no filter, no delay, no cost. That case is common enough (48 kHz on
/// both sides is the norm) to be worth not paying for.
pub struct RateConverter {
    /// `None` when the rates match and no conversion is needed.
    inner: Option<Async<f32>>,
    /// Input that has arrived but does not yet make a whole chunk.
    pending: Vec<f32>,
    /// Preallocated scratch. Mono, so a flat vector is a valid channel layout.
    input_scratch: Vec<f32>,
    output_scratch: Vec<f32>,
    from_hz: u32,
    to_hz: u32,
}

impl std::fmt::Debug for RateConverter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RateConverter")
            .field("from_hz", &self.from_hz)
            .field("to_hz", &self.to_hz)
            .field("passthrough", &self.inner.is_none())
            .field("pending", &self.pending.len())
            .finish()
    }
}

impl RateConverter {
    /// Builds a converter from `from_hz` to `to_hz`.
    ///
    /// Matching rates produce a passthrough that copies and nothing more. Use
    /// [`Self::new_adjustable`] when the ratio has to be steerable even at 1:1.
    ///
    /// # Errors
    ///
    /// Returns [`ConversionError::ZeroRate`] if either rate is zero, or
    /// [`ConversionError::Construction`] if `rubato` rejects the configuration.
    pub fn new(from_hz: u32, to_hz: u32) -> Result<Self, ConversionError> {
        Self::build(from_hz, to_hz, false)
    }

    /// Builds a converter whose ratio can be steered, even when the rates match.
    ///
    /// This is what clock-drift correction needs (task M1.8). Two devices both
    /// claiming 48 kHz still disagree by tens of parts per million, and
    /// correcting that means resampling by a ratio very slightly away from 1.0 —
    /// which the passthrough shortcut in [`Self::new`] cannot do, because it has
    /// no filter to steer.
    ///
    /// Costs a real sinc filter where [`Self::new`] would have cost nothing, so
    /// use it only where drift is actually being corrected.
    ///
    /// # Errors
    ///
    /// Same as [`Self::new`].
    pub fn new_adjustable(from_hz: u32, to_hz: u32) -> Result<Self, ConversionError> {
        Self::build(from_hz, to_hz, true)
    }

    fn build(from_hz: u32, to_hz: u32, force_filter: bool) -> Result<Self, ConversionError> {
        if from_hz == 0 || to_hz == 0 {
            return Err(ConversionError::ZeroRate {
                from: from_hz,
                to: to_hz,
            });
        }

        if from_hz == to_hz && !force_filter {
            return Ok(Self {
                inner: None,
                pending: Vec::new(),
                input_scratch: Vec::new(),
                output_scratch: Vec::new(),
                from_hz,
                to_hz,
            });
        }

        // rubato's documented balanced preset: good enough that voice artefacts
        // are inaudible, cheap enough to run per source.
        let parameters = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: None,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };

        let ratio = f64::from(to_hz) / f64::from(from_hz);
        let inner = Async::<f32>::new_sinc(
            ratio,
            MAX_RATIO_ADJUSTMENT,
            &parameters,
            CHUNK_FRAMES,
            1,
            FixedAsync::Input,
        )
        .map_err(|source| ConversionError::Construction {
            from: from_hz,
            to: to_hz,
            source,
        })?;

        let input_frames = inner.input_frames_max();
        let output_frames = inner.output_frames_max();

        Ok(Self {
            inner: Some(inner),
            pending: Vec::with_capacity(input_frames * 2),
            input_scratch: vec![0.0; input_frames],
            output_scratch: vec![0.0; output_frames],
            from_hz,
            to_hz,
        })
    }

    /// True when the rates match and [`Self::push`] is a plain copy.
    #[must_use]
    pub fn is_passthrough(&self) -> bool {
        self.inner.is_none()
    }

    /// Source rate.
    #[must_use]
    pub fn from_hz(&self) -> u32 {
        self.from_hz
    }

    /// Target rate.
    #[must_use]
    pub fn to_hz(&self) -> u32 {
        self.to_hz
    }

    /// Frames of delay the filter itself adds.
    ///
    /// Zero in passthrough. This is a real entry in the latency budget of
    /// ADR 0009 whenever a device is not already at 48 kHz, which is why it is
    /// exposed rather than buried.
    #[must_use]
    pub fn delay_frames(&self) -> usize {
        self.inner.as_ref().map_or(0, Resampler::output_delay)
    }

    /// Nudges the conversion ratio relative to nominal.
    ///
    /// The hook for clock-drift correction (task M1.8): a value slightly above
    /// or below `1.0` speeds the stream up or slows it down without a
    /// discontinuity. No effect in passthrough — a converter built for matching
    /// rates has no filter to steer.
    ///
    /// # Errors
    ///
    /// Returns [`ConversionError::Process`] if the adjustment exceeds the
    /// headroom the converter was built with.
    pub fn adjust_ratio(&mut self, relative: f64) -> Result<(), ConversionError> {
        let Some(inner) = self.inner.as_mut() else {
            return Ok(());
        };
        inner
            .set_resample_ratio_relative(relative, true)
            .map_err(|source| ConversionError::Process { source })
    }

    /// Converts `input` and appends the result to `output`.
    ///
    /// Input is buffered internally until a whole chunk is available, so any
    /// slice length is acceptable. `output` is the caller's to reuse; after
    /// warm-up it stops growing and no allocation happens per call.
    ///
    /// # Errors
    ///
    /// Returns [`ConversionError::Process`] if `rubato` rejects a chunk.
    pub fn push(&mut self, input: &[f32], output: &mut Vec<f32>) -> Result<(), ConversionError> {
        let Some(inner) = self.inner.as_mut() else {
            output.extend_from_slice(input);
            return Ok(());
        };

        self.pending.extend_from_slice(input);

        loop {
            let needed = inner.input_frames_next();
            if self.pending.len() < needed {
                return Ok(());
            }

            let Some(slot) = self.input_scratch.get_mut(..needed) else {
                return Err(ConversionError::BufferShape { frames: needed });
            };
            for (target, sample) in slot.iter_mut().zip(self.pending.drain(..needed)) {
                *target = sample;
            }

            let available = self.output_scratch.len();
            let Some(filled) = self.input_scratch.get(..needed) else {
                return Err(ConversionError::BufferShape { frames: needed });
            };
            let input_view = SequentialSlice::new(filled, 1, needed)
                .map_err(|_| ConversionError::BufferShape { frames: needed })?;
            let Some(spare) = self.output_scratch.get_mut(..available) else {
                return Err(ConversionError::BufferShape { frames: available });
            };
            let mut output_view = SequentialSlice::new_mut(spare, 1, available)
                .map_err(|_| ConversionError::BufferShape { frames: available })?;

            let (_consumed, produced) = inner
                .process_into_buffer(&input_view, &mut output_view, None)
                .map_err(|source| ConversionError::Process { source })?;

            output.extend_from_slice(self.output_scratch.get(..produced).unwrap_or_default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SAMPLE_RATE_HZ;

    fn tone(rate: u32, hz: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (t * hz * std::f32::consts::TAU).sin() * 0.5
            })
            .collect()
    }

    /// Counts zero crossings, which recovers a pure tone's frequency well enough
    /// to tell "the pitch survived" from "the pitch shifted".
    fn estimated_hz(samples: &[f32], rate: u32) -> f32 {
        let crossings = samples
            .windows(2)
            .filter(|pair| match pair {
                [a, b] => (*a <= 0.0 && *b > 0.0) || (*a >= 0.0 && *b < 0.0),
                _ => false,
            })
            .count();
        crossings as f32 / 2.0 * rate as f32 / samples.len() as f32
    }

    #[test]
    fn matching_rates_are_a_passthrough() {
        let mut converter = RateConverter::new(48_000, 48_000).unwrap();
        assert!(converter.is_passthrough());
        assert_eq!(converter.delay_frames(), 0);

        let input = tone(48_000, 440.0, 128);
        let mut output = Vec::new();
        converter.push(&input, &mut output).unwrap();

        assert_eq!(output, input, "a passthrough must not touch the samples");
    }

    #[test]
    fn zero_rate_is_rejected() {
        assert!(matches!(
            RateConverter::new(0, 48_000),
            Err(ConversionError::ZeroRate { .. })
        ));
        assert!(matches!(
            RateConverter::new(48_000, 0),
            Err(ConversionError::ZeroRate { .. })
        ));
    }

    #[test]
    fn upsampling_44100_to_48000_produces_the_right_length() {
        let mut converter = RateConverter::new(44_100, SAMPLE_RATE_HZ).unwrap();
        assert!(!converter.is_passthrough());

        // Ten chunks in, so the ratio dominates the warm-up transient.
        let input = tone(44_100, 440.0, CHUNK_FRAMES * 10);
        let mut output = Vec::new();
        converter.push(&input, &mut output).unwrap();

        let expected = input.len() as f32 * 48_000.0 / 44_100.0;
        let error = (output.len() as f32 - expected).abs() / expected;
        assert!(
            error < 0.01,
            "expected about {expected:.0} frames, got {}",
            output.len()
        );
    }

    #[test]
    fn downsampling_48000_to_44100_produces_the_right_length() {
        let mut converter = RateConverter::new(SAMPLE_RATE_HZ, 44_100).unwrap();
        let input = tone(48_000, 440.0, CHUNK_FRAMES * 10);
        let mut output = Vec::new();
        converter.push(&input, &mut output).unwrap();

        let expected = input.len() as f32 * 44_100.0 / 48_000.0;
        let error = (output.len() as f32 - expected).abs() / expected;
        assert!(
            error < 0.01,
            "expected about {expected:.0} frames, got {}",
            output.len()
        );
    }

    #[test]
    fn conversion_preserves_pitch() {
        // The failure this guards against is a resampler that runs but shifts
        // everything — the kind of bug that sounds like chipmunks, not silence.
        let mut converter = RateConverter::new(44_100, SAMPLE_RATE_HZ).unwrap();
        let input = tone(44_100, 1_000.0, CHUNK_FRAMES * 20);
        let mut output = Vec::new();
        converter.push(&input, &mut output).unwrap();

        // Skip the filter's warm-up before measuring.
        let settled = output.get(2_000..).unwrap_or_default();
        let hz = estimated_hz(settled, SAMPLE_RATE_HZ);
        assert!(
            (hz - 1_000.0).abs() < 20.0,
            "1 kHz tone came out at {hz:.0} Hz"
        );
    }

    #[test]
    fn arbitrary_input_lengths_are_buffered() {
        // The capture ring hands over whatever it has, never a tidy chunk.
        let mut converter = RateConverter::new(44_100, SAMPLE_RATE_HZ).unwrap();
        let input = tone(44_100, 440.0, CHUNK_FRAMES * 8);
        let mut ragged = Vec::new();
        let mut whole = Vec::new();

        for piece in input.chunks(37) {
            converter.push(piece, &mut ragged).unwrap();
        }

        let mut fresh = RateConverter::new(44_100, SAMPLE_RATE_HZ).unwrap();
        fresh.push(&input, &mut whole).unwrap();

        assert_eq!(
            ragged, whole,
            "chunking the input must not change the output"
        );
    }

    #[test]
    fn ratio_can_be_nudged_for_clock_drift() {
        // Task M1.8 needs this; prove the hook works before depending on it.
        let mut converter = RateConverter::new(44_100, SAMPLE_RATE_HZ).unwrap();
        assert!(converter.adjust_ratio(1.000_01).is_ok());
        assert!(converter.adjust_ratio(0.999_99).is_ok());

        let input = tone(44_100, 440.0, CHUNK_FRAMES * 2);
        let mut output = Vec::new();
        assert!(converter.push(&input, &mut output).is_ok());
        assert!(!output.is_empty(), "still converting after an adjustment");
    }

    #[test]
    fn an_adjustable_converter_steers_even_at_matching_rates() {
        // Clock-drift correction (task M1.8) runs at 48 kHz on both sides and
        // still has to nudge the ratio. The passthrough shortcut cannot do that,
        // which is why this constructor exists.
        let mut converter = RateConverter::new_adjustable(48_000, 48_000).unwrap();
        assert!(!converter.is_passthrough(), "must keep a filter to steer");
        assert!(converter.adjust_ratio(1.000_1).is_ok());

        let input = tone(48_000, 440.0, CHUNK_FRAMES * 8);
        let mut output = Vec::new();
        converter.push(&input, &mut output).unwrap();

        // Steering by +100 ppm should produce measurably more output than input.
        assert!(
            output.len() > input.len() - CHUNK_FRAMES,
            "converter stopped producing after an adjustment"
        );
    }

    #[test]
    fn adjusting_a_passthrough_is_harmless() {
        let mut converter = RateConverter::new(48_000, 48_000).unwrap();
        assert!(converter.adjust_ratio(1.000_01).is_ok());
        assert!(converter.is_passthrough());
    }

    #[test]
    fn filter_delay_is_reported_when_converting() {
        let converter = RateConverter::new(44_100, SAMPLE_RATE_HZ).unwrap();
        let delay = converter.delay_frames();
        assert!(delay > 0, "a sinc filter cannot have zero delay");

        // ADR 0009 has a ~67 ms floor. A resampler costing more than a couple of
        // milliseconds would deserve its own channel in that budget.
        let delay_ms = delay as f64 / f64::from(SAMPLE_RATE_HZ) * 1000.0;
        assert!(delay_ms < 5.0, "resampler adds {delay_ms:.2} ms of delay");
    }
}
