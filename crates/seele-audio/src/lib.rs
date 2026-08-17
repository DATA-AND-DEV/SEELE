//! SEELE audio pipeline: capture, codec, jitter buffer and mixing.
//!
//! This is the hard part of the project (`specs/03-audio.md`) and the reason M1
//! comes before the protocol and the server in the roadmap.
//!
//! # Real-time thread rules
//!
//! The `cpal` callback runs on a high-priority thread with a deadline of a few
//! milliseconds. Inside it there is **no heap allocation, no blocking `Mutex`,
//! no I/O, no logging and no `.await`**. Communication with the async world goes
//! through a lock-free ring buffer.
//!
//! Violating this produces audible clicks that are very hard to diagnose after
//! the fact, so the rule is enforced mechanically rather than by review: the
//! critical path is covered by an allocation-failing test in CI (task M1.3).
//!
//! # Boundary
//!
//! This crate emits **plain data** — never formatted text, never colour, never a
//! `SyncRatio` band. Presentation belongs to the shells (`specs/01-arquitetura.md`).
//! A themed `to_string()` in here is an architecture error, not a detail.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod codec;
pub mod device;
pub mod drift;
pub mod gate;
pub mod jitter;
pub mod latency;
pub mod mixer;
pub mod netsim;
pub mod pacing;
pub mod playout;
pub mod resample;
pub mod rt;
pub mod supervisor;
pub mod telemetry;

/// Sample rate used everywhere inside the pipeline, in hertz.
///
/// `specs/03-audio.md`. Device rates that differ from this are resampled at the
/// edges, in both directions (task M1.4).
pub const SAMPLE_RATE_HZ: u32 = 48_000;

/// Opus frame duration, in milliseconds (`specs/03-audio.md`).
pub const FRAME_MS: u32 = 20;

/// Samples per frame, per channel, at [`SAMPLE_RATE_HZ`].
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE_HZ as usize * FRAME_MS as usize) / 1000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_960_samples() {
        // specs/03-audio.md states 20 ms = 960 samples at 48 kHz.
        assert_eq!(FRAME_SAMPLES, 960);
    }
}
