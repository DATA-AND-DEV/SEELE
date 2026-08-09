//! The real-time boundary: lock-free transport between the audio callbacks and
//! the rest of the system.
//!
//! `specs/03-audio.md` states the rules for the `cpal` callback in full:
//!
//! > - **No heap allocation.** No `Vec::push`, `String`, `format!`, `Box`.
//! > - **No `Mutex`** that can block. Only lock-free structures.
//! > - **No I/O, no logging, no `println!`.**
//! > - **No `.await`.** The callback is not async.
//! >
//! > Violating this produces audible clicks that are hard to diagnose later.
//!
//! Everything in this module is written to hold those rules, and
//! `tests/realtime_safety.rs` enforces the first one mechanically rather than by
//! review.
//!
//! # Why this module has no `cpal` in it
//!
//! CI has no audio devices, so anything wired to hardware cannot be tested
//! there. The logic that runs inside the callback lives here, operating on plain
//! slices, and is fully testable without a sound card. The thin layer that hands
//! these types to `cpal` lives in [`crate::device`] and is deliberately kept as
//! small as possible, because it is the part no test can reach.

use std::num::NonZeroU16;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rtrb::{Consumer, Producer, RingBuffer};

/// Per-sample decay applied while the playback ring is starved.
///
/// `specs/03-audio.md` asks for "silence with fade" rather than an abrupt cut,
/// because dropping straight to zero from a non-zero sample is a click. At
/// 48 kHz this reaches inaudibility in about 10 ms.
const UNDERRUN_DECAY: f32 = 0.99;

/// How many samples of ring a given duration needs at a given rate.
///
/// The ring only has to absorb scheduling jitter between the device callback and
/// the processing thread. It is not the jitter buffer — that one is adaptive,
/// lives per source, and is a different component entirely.
#[must_use]
pub const fn capacity_for_ms(ms: u32, sample_rate_hz: u32) -> usize {
    (sample_rate_hz as usize * ms as usize) / 1000
}

/// A sample format a device might speak, convertible to and from the `f32` the
/// pipeline uses internally.
///
/// `specs/03-audio.md` makes the internal format 48 kHz mono `f32`, but devices
/// deliver whatever they like. **Format** conversion happens here, inside the
/// callback, because it is per-sample arithmetic with no state and no
/// allocation. **Rate** conversion is a different problem — it needs state and
/// buffers — and lives in [`crate::resample`], on the processing thread, exactly
/// where the pipeline diagram in `specs/03-audio.md` puts it.
///
/// Implemented here rather than reusing `cpal`'s trait so that this module stays
/// free of `cpal` and testable without a sound card.
pub trait RawSample: Copy + Send + 'static {
    /// Converts one device sample to the internal representation, nominally in
    /// `-1.0..=1.0`.
    fn to_f32(self) -> f32;

    /// Converts one internal sample back to the device format, clamping rather
    /// than wrapping. A wrap here is a full-scale click.
    fn from_f32(value: f32) -> Self;
}

impl RawSample for f32 {
    fn to_f32(self) -> f32 {
        self
    }
    fn from_f32(value: f32) -> Self {
        value
    }
}

// Both directions use the same scale, 2^15, and clamp at the ends rather than
// scaling by 32767 one way and 32768 the other. Mixing the two divisors looks
// harmless and costs a full quantisation step of round-trip error — which a test
// in this module caught. Rounding rather than truncating halves what remains and
// removes the bias toward zero that truncation introduces.

impl RawSample for i16 {
    fn to_f32(self) -> f32 {
        f32::from(self) / 32_768.0
    }
    fn from_f32(value: f32) -> Self {
        (value * 32_768.0).round().clamp(-32_768.0, 32_767.0) as Self
    }
}

impl RawSample for u16 {
    fn to_f32(self) -> f32 {
        (f32::from(self) - 32_768.0) / 32_768.0
    }
    fn from_f32(value: f32) -> Self {
        (value * 32_768.0 + 32_768.0).round().clamp(0.0, 65_535.0) as Self
    }
}

impl RawSample for i32 {
    fn to_f32(self) -> f32 {
        self as f32 / 2_147_483_648.0
    }
    fn from_f32(value: f32) -> Self {
        (f64::from(value) * 2_147_483_648.0)
            .round()
            .clamp(-2_147_483_648.0, 2_147_483_647.0) as Self
    }
}

/// Counters written by the real-time callbacks and read by everyone else.
///
/// Atomics with [`Ordering::Relaxed`]: these are statistics, and no other memory
/// is published through them. Relaxed loads and stores compile to plain moves on
/// every architecture we target, so reading them costs the callback nothing.
#[derive(Debug, Default)]
pub struct StreamCounters {
    frames_captured: AtomicU64,
    frames_played: AtomicU64,
    capture_overruns: AtomicU64,
    playback_underruns: AtomicU64,
    stream_errors: AtomicU64,
}

impl StreamCounters {
    /// A fresh set of counters, shared between the two callbacks.
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Reads the counters as plain data.
    ///
    /// `specs/01-arquitetura.md`: this crate emits data, never presentation. The
    /// shell decides what "1832 underruns" looks like on screen.
    #[must_use]
    pub fn snapshot(&self) -> StreamMetrics {
        StreamMetrics {
            frames_captured: self.frames_captured.load(Ordering::Relaxed),
            frames_played: self.frames_played.load(Ordering::Relaxed),
            capture_overruns: self.capture_overruns.load(Ordering::Relaxed),
            playback_underruns: self.playback_underruns.load(Ordering::Relaxed),
            stream_errors: self.stream_errors.load(Ordering::Relaxed),
        }
    }

    /// Records a device-level stream error.
    ///
    /// Called from `cpal`'s error callback, which is the seam device hot-swap
    /// (task M1.14) grows out of. Counting is all this does today; the point is
    /// that an unplugged headset is never silently swallowed.
    pub fn record_stream_error(&self) {
        self.stream_errors.fetch_add(1, Ordering::Relaxed);
    }
}

/// A point-in-time reading of [`StreamCounters`]. Plain data, no behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamMetrics {
    /// Frames the capture callback successfully handed to the ring.
    pub frames_captured: u64,
    /// Frames the playback callback wrote to the device.
    pub frames_played: u64,
    /// Frames dropped because the processing thread was not draining fast enough.
    pub capture_overruns: u64,
    /// Frames the playback callback had to invent because the ring was empty.
    pub playback_underruns: u64,
    /// Errors reported by the device itself, such as a disconnection.
    pub stream_errors: u64,
}

/// The capture half of the boundary. Lives inside the `cpal` input callback.
#[derive(Debug)]
pub struct CaptureSink {
    producer: Producer<f32>,
    counters: Arc<StreamCounters>,
    channels: NonZeroU16,
}

impl CaptureSink {
    /// Hands one device buffer to the processing thread.
    ///
    /// **Runs on the real-time thread.** Allocates nothing, locks nothing,
    /// blocks on nothing, logs nothing.
    ///
    /// `interleaved` is the device's native layout. `specs/03-audio.md` makes
    /// capture mono, and this takes **channel 0** rather than averaging: on a
    /// laptop microphone array the channels are physically separated, and
    /// summing them comb-filters the result.
    ///
    /// When the ring is full the sample is dropped and counted. Dropping is the
    /// only real-time-safe option — the alternative is blocking the device
    /// thread, which is worse than a gap.
    ///
    /// Generic over the device's sample format; conversion to `f32` happens here
    /// (see [`RawSample`]). Samples leave at the **device's** rate — resampling
    /// to 48 kHz is [`crate::resample`]'s job, on the processing thread.
    pub fn on_capture<T: RawSample>(&mut self, interleaved: &[T]) {
        let mut captured = 0_u64;
        let mut overruns = 0_u64;

        for frame in interleaved.chunks_exact(usize::from(self.channels.get())) {
            let Some(sample) = frame.first() else {
                continue;
            };
            if self.producer.push(sample.to_f32()).is_ok() {
                captured += 1;
            } else {
                overruns += 1;
            }
        }

        if captured > 0 {
            self.counters
                .frames_captured
                .fetch_add(captured, Ordering::Relaxed);
        }
        if overruns > 0 {
            self.counters
                .capture_overruns
                .fetch_add(overruns, Ordering::Relaxed);
        }
    }
}

/// The playback half of the boundary. Lives inside the `cpal` output callback.
#[derive(Debug)]
pub struct PlaybackSource {
    consumer: Consumer<f32>,
    counters: Arc<StreamCounters>,
    channels: NonZeroU16,
    /// Last sample written, kept so an underrun can fade instead of cutting.
    last: f32,
}

impl PlaybackSource {
    /// Fills one device buffer from the processing thread's output.
    ///
    /// **Runs on the real-time thread.** Allocates nothing, locks nothing,
    /// blocks on nothing, logs nothing.
    ///
    /// The mono stream is duplicated across every device channel. When the ring
    /// is empty the last sample decays toward zero rather than cutting, per
    /// `specs/03-audio.md`.
    ///
    /// Generic over the device's sample format. Samples are expected to already
    /// be at the **device's** rate — [`crate::resample`] converts before they
    /// reach the ring.
    pub fn on_playback<T: RawSample>(&mut self, interleaved: &mut [T]) {
        let channels = usize::from(self.channels.get());
        let mut played = 0_u64;
        let mut underruns = 0_u64;

        let mut frames = interleaved.chunks_exact_mut(channels);
        for frame in frames.by_ref() {
            let sample = match self.consumer.pop() {
                Ok(sample) => {
                    played += 1;
                    self.last = sample;
                    sample
                }
                Err(_) => {
                    underruns += 1;
                    self.last *= UNDERRUN_DECAY;
                    self.last
                }
            };
            frame.fill(T::from_f32(sample));
        }

        // A device buffer whose length is not a whole number of frames should not
        // happen, but leaving stale memory audible if it did is not acceptable.
        frames.into_remainder().fill(T::from_f32(0.0));

        if played > 0 {
            self.counters
                .frames_played
                .fetch_add(played, Ordering::Relaxed);
        }
        if underruns > 0 {
            self.counters
                .playback_underruns
                .fetch_add(underruns, Ordering::Relaxed);
        }
    }
}

/// Builds the capture ring.
///
/// Returns the sink for the real-time callback and the consumer the processing
/// thread drains. All allocation happens here, at setup, never in the callback.
#[must_use]
pub fn capture_path(
    capacity_samples: usize,
    channels: NonZeroU16,
    counters: Arc<StreamCounters>,
) -> (CaptureSink, Consumer<f32>) {
    let (producer, consumer) = RingBuffer::new(capacity_samples);
    (
        CaptureSink {
            producer,
            counters,
            channels,
        },
        consumer,
    )
}

/// Builds the playback ring.
///
/// Returns the producer the processing thread feeds and the source for the
/// real-time callback. All allocation happens here, at setup.
#[must_use]
pub fn playback_path(
    capacity_samples: usize,
    channels: NonZeroU16,
    counters: Arc<StreamCounters>,
) -> (Producer<f32>, PlaybackSource) {
    let (producer, consumer) = RingBuffer::new(capacity_samples);
    (
        producer,
        PlaybackSource {
            consumer,
            counters,
            channels,
            last: 0.0,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONO: NonZeroU16 = NonZeroU16::new(1).unwrap();
    const STEREO: NonZeroU16 = NonZeroU16::new(2).unwrap();

    #[test]
    fn capacity_matches_the_spec_frame() {
        // 20 ms at 48 kHz is the frame size in specs/03-audio.md.
        assert_eq!(capacity_for_ms(20, SAMPLE_RATE), 960);
    }

    const SAMPLE_RATE: u32 = 48_000;

    #[test]
    fn format_conversion_round_trips_within_one_quantisation_step() {
        // A sign or scale error here is a full-scale click, not a subtle one.
        for original in [-1.0_f32, -0.5, 0.0, 0.5, 0.999] {
            let via_i16 = i16::from_f32(original).to_f32();
            assert!(
                (via_i16 - original).abs() < 1.0 / 32_767.0 + f32::EPSILON,
                "i16 round trip moved {original} to {via_i16}"
            );

            let via_u16 = u16::from_f32(original).to_f32();
            assert!(
                (via_u16 - original).abs() < 1.0 / 32_767.0 + f32::EPSILON,
                "u16 round trip moved {original} to {via_u16}"
            );
        }
    }

    #[test]
    fn format_conversion_clamps_instead_of_wrapping() {
        // Wrapping turns an overshoot into full-scale noise of the opposite
        // sign, which is the loudest possible way to fail.
        assert_eq!(i16::from_f32(2.0), i16::MAX);
        assert_eq!(i16::from_f32(-2.0), i16::MIN);
        assert_eq!(u16::from_f32(2.0), u16::MAX);
        assert_eq!(u16::from_f32(-2.0), u16::MIN);
        assert_eq!(i32::from_f32(2.0), i32::MAX);
        assert_eq!(i32::from_f32(-2.0), i32::MIN);
    }

    #[test]
    fn capture_converts_integer_input() {
        let counters = StreamCounters::shared();
        let (mut sink, mut consumer) = capture_path(16, MONO, Arc::clone(&counters));

        sink.on_capture(&[i16::MAX, 0, i16::MIN]);

        assert!(consumer.pop().is_ok_and(|s| (s - 1.0).abs() < 0.001));
        assert_eq!(consumer.pop(), Ok(0.0));
        assert!(consumer.pop().is_ok_and(|s| (s + 1.0).abs() < 0.001));
    }

    #[test]
    fn capture_takes_channel_zero_of_each_frame() {
        let counters = StreamCounters::shared();
        let (mut sink, mut consumer) = capture_path(16, STEREO, Arc::clone(&counters));

        // Interleaved stereo: left is the signal, right is a decoy.
        sink.on_capture(&[1.0, -9.0, 2.0, -9.0, 3.0, -9.0]);

        assert_eq!(consumer.pop(), Ok(1.0));
        assert_eq!(consumer.pop(), Ok(2.0));
        assert_eq!(consumer.pop(), Ok(3.0));
        assert_eq!(counters.snapshot().frames_captured, 3);
        assert_eq!(counters.snapshot().capture_overruns, 0);
    }

    #[test]
    fn capture_counts_overruns_instead_of_blocking() {
        let counters = StreamCounters::shared();
        let (mut sink, _consumer) = capture_path(2, MONO, Arc::clone(&counters));

        sink.on_capture(&[1.0, 2.0, 3.0, 4.0, 5.0]);

        let metrics = counters.snapshot();
        assert_eq!(metrics.frames_captured, 2, "only the ring's worth fits");
        assert_eq!(
            metrics.capture_overruns, 3,
            "the rest is dropped and counted"
        );
    }

    #[test]
    fn playback_duplicates_mono_across_channels() {
        let counters = StreamCounters::shared();
        let (mut producer, mut source) = playback_path(16, STEREO, Arc::clone(&counters));

        for sample in [0.5_f32, -0.25] {
            assert!(producer.push(sample).is_ok());
        }

        let mut buffer = [99.0_f32; 4];
        source.on_playback(&mut buffer);

        assert_eq!(buffer, [0.5, 0.5, -0.25, -0.25]);
        assert_eq!(counters.snapshot().frames_played, 2);
        assert_eq!(counters.snapshot().playback_underruns, 0);
    }

    #[test]
    fn underrun_fades_rather_than_cutting() {
        let counters = StreamCounters::shared();
        let (mut producer, mut source) = playback_path(16, MONO, Arc::clone(&counters));
        assert!(producer.push(1.0).is_ok());

        let mut buffer = [0.0_f32; 4];
        source.on_playback(&mut buffer);

        assert_eq!(buffer.first(), Some(&1.0));
        // A hard cut would put 0.0 here and click. Each starved frame decays.
        assert_eq!(buffer.get(1), Some(&UNDERRUN_DECAY));
        assert!(buffer
            .get(2)
            .is_some_and(|s| *s < UNDERRUN_DECAY && *s > 0.0));
        assert!(buffer.get(3).is_some_and(|s| *s > 0.0));
        assert_eq!(counters.snapshot().playback_underruns, 3);
    }

    #[test]
    fn sustained_underrun_reaches_silence() {
        let counters = StreamCounters::shared();
        let (mut producer, mut source) = playback_path(16, MONO, Arc::clone(&counters));
        assert!(producer.push(1.0).is_ok());

        // 10 ms at 48 kHz. specs/03-audio.md wants the fade inaudible by then.
        let mut buffer = [0.0_f32; 480];
        source.on_playback(&mut buffer);

        let tail = buffer.last().copied().unwrap_or(1.0);
        assert!(
            tail < 0.01,
            "fade should be inaudible after 10 ms, got {tail}"
        );
    }

    #[test]
    fn playback_on_empty_ring_is_silence_not_garbage() {
        let counters = StreamCounters::shared();
        let (_producer, mut source) = playback_path(16, MONO, Arc::clone(&counters));

        let mut buffer = [99.0_f32; 4];
        source.on_playback(&mut buffer);

        assert_eq!(buffer, [0.0; 4], "stale memory must never reach the device");
        assert_eq!(counters.snapshot().playback_underruns, 4);
    }

    #[test]
    fn ragged_device_buffer_leaves_no_stale_memory() {
        let counters = StreamCounters::shared();
        let (mut producer, mut source) = playback_path(16, STEREO, Arc::clone(&counters));
        assert!(producer.push(0.5).is_ok());

        // Five slots is two stereo frames plus a stray sample.
        let mut buffer = [99.0_f32; 5];
        source.on_playback(&mut buffer);

        assert_eq!(buffer.last(), Some(&0.0), "the remainder must be silenced");
    }

    #[test]
    fn round_trip_through_both_rings_preserves_the_signal() {
        let counters = StreamCounters::shared();
        let (mut sink, mut captured) = capture_path(64, STEREO, Arc::clone(&counters));
        let (mut to_device, mut source) = playback_path(64, MONO, Arc::clone(&counters));

        sink.on_capture(&[0.1, 0.0, 0.2, 0.0, 0.3, 0.0]);

        // Stands in for the processing thread.
        while let Ok(sample) = captured.pop() {
            assert!(to_device.push(sample).is_ok());
        }

        let mut buffer = [0.0_f32; 3];
        source.on_playback(&mut buffer);

        assert_eq!(buffer, [0.1, 0.2, 0.3]);
        let metrics = counters.snapshot();
        assert_eq!(metrics.capture_overruns, 0);
        assert_eq!(metrics.playback_underruns, 0);
    }
}
