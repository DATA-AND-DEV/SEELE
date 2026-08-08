//! Mechanical enforcement of the first real-time rule in `specs/03-audio.md`.
//!
//! > **No heap allocation.** No `Vec::push`, `String`, `format!`, `Box`.
//! >
//! > Violating this produces audible clicks that are hard to diagnose later.
//! > Suggestion: a CI test that fails if the audio crate allocates on the
//! > critical path.
//!
//! This is that test. It installs a global allocator that panics if anything
//! allocates inside [`assert_no_alloc`], then drives every callback path —
//! including the error paths, which is where an allocation usually sneaks in
//! later as someone reaches for a `format!` to describe what went wrong.
//!
//! The last test is a negative control. Without it there is no evidence the
//! guard does anything at all, and a gate that cannot fail is decoration.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::num::NonZeroU16;
use std::sync::Arc;

use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use magi_audio::rt::{capture_path, playback_path, StreamCounters};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const MONO: NonZeroU16 = NonZeroU16::new(1).unwrap();
const STEREO: NonZeroU16 = NonZeroU16::new(2).unwrap();

/// One device buffer's worth, sized like a real CoreAudio callback (512 frames,
/// which M1.1 measured as this machine's native buffer).
const DEVICE_FRAMES: usize = 512;

#[test]
fn capture_callback_does_not_allocate() {
    let counters = StreamCounters::shared();
    let (mut sink, _consumer) = capture_path(DEVICE_FRAMES * 4, STEREO, Arc::clone(&counters));
    let buffer = vec![0.25_f32; DEVICE_FRAMES * 2];

    assert_no_alloc(|| {
        for _ in 0..8 {
            sink.on_capture(&buffer);
        }
    });

    assert!(counters.snapshot().frames_captured > 0);
}

#[test]
fn capture_overrun_path_does_not_allocate() {
    // A full ring is the interesting case: the branch that runs when the
    // processing thread has stalled must stay as quiet as the happy path.
    let counters = StreamCounters::shared();
    let (mut sink, _consumer) = capture_path(8, MONO, Arc::clone(&counters));
    let buffer = vec![0.25_f32; DEVICE_FRAMES];

    assert_no_alloc(|| {
        for _ in 0..4 {
            sink.on_capture(&buffer);
        }
    });

    assert!(
        counters.snapshot().capture_overruns > 0,
        "the test did not actually exercise the overrun branch"
    );
}

#[test]
fn playback_callback_does_not_allocate() {
    let counters = StreamCounters::shared();
    let (mut producer, mut source) =
        playback_path(DEVICE_FRAMES * 4, STEREO, Arc::clone(&counters));
    for _ in 0..DEVICE_FRAMES * 2 {
        let _ = producer.push(0.1);
    }
    let mut buffer = vec![0.0_f32; DEVICE_FRAMES * 2];

    assert_no_alloc(|| {
        source.on_playback(&mut buffer);
    });

    assert!(counters.snapshot().frames_played > 0);
}

#[test]
fn playback_underrun_path_does_not_allocate() {
    // Starved ring, so every frame takes the fade branch.
    let counters = StreamCounters::shared();
    let (_producer, mut source) = playback_path(8, STEREO, Arc::clone(&counters));
    let mut buffer = vec![0.0_f32; DEVICE_FRAMES * 2];

    assert_no_alloc(|| {
        for _ in 0..4 {
            source.on_playback(&mut buffer);
        }
    });

    assert!(
        counters.snapshot().playback_underruns > 0,
        "the test did not actually exercise the underrun branch"
    );
}

#[test]
fn reading_metrics_from_a_callback_does_not_allocate() {
    // Plausible future mistake: someone samples the counters inside the callback
    // to stamp a telemetry event. Prove the read itself is free, so that if it
    // ever does allocate the cause is whatever was added around it.
    let counters = StreamCounters::shared();

    assert_no_alloc(|| {
        let snapshot = counters.snapshot();
        std::hint::black_box(snapshot);
    });
}

#[test]
fn integer_format_conversion_does_not_allocate() {
    // Task M1.4 moved sample-format conversion into the callback. It is
    // per-sample arithmetic with no state, so it should cost nothing — but
    // "should" is what this file exists to replace.
    let counters = StreamCounters::shared();
    let (mut sink, _consumer) = capture_path(DEVICE_FRAMES * 4, STEREO, Arc::clone(&counters));
    let i16_buffer = vec![1234_i16; DEVICE_FRAMES * 2];
    let u16_buffer = vec![40_000_u16; DEVICE_FRAMES * 2];
    let i32_buffer = vec![70_000_i32; DEVICE_FRAMES * 2];

    assert_no_alloc(|| {
        sink.on_capture(&i16_buffer);
        sink.on_capture(&u16_buffer);
        sink.on_capture(&i32_buffer);
    });

    assert!(counters.snapshot().frames_captured > 0);
}

#[test]
fn integer_format_playback_does_not_allocate() {
    let counters = StreamCounters::shared();
    let (mut producer, mut source) =
        playback_path(DEVICE_FRAMES * 4, STEREO, Arc::clone(&counters));
    for _ in 0..DEVICE_FRAMES {
        let _ = producer.push(0.5);
    }
    let mut buffer = vec![0_i16; DEVICE_FRAMES * 2];

    assert_no_alloc(|| {
        source.on_playback(&mut buffer);
    });

    assert!(counters.snapshot().frames_played > 0);
}

/// Environment flag that turns this test binary into the canary child.
const CANARY: &str = "MAGI_ALLOC_CANARY";

#[test]
fn the_guard_actually_catches_allocation() {
    // Negative control. Without it there is no evidence the guard does anything,
    // and every other test in this file would keep passing after the gate went
    // inert.
    //
    // It has to run in a child process: `AllocDisabler` reacts to a violation by
    // aborting, not by unwinding, so `catch_unwind` cannot observe it and the
    // abort would take the whole test binary down with it.
    if std::env::var_os(CANARY).is_some() {
        assert_no_alloc(|| {
            let sneaky: Vec<f32> = Vec::with_capacity(1024);
            std::hint::black_box(sneaky);
        });
        // Reaching here means the allocation went unnoticed. Exit cleanly so the
        // parent sees success and fails the assertion below.
        eprintln!("canary allocated inside assert_no_alloc and survived");
        std::process::exit(0);
    }

    let exe = std::env::current_exe().expect("test binary should know its own path");
    let status = std::process::Command::new(exe)
        .args([
            "--exact",
            "the_guard_actually_catches_allocation",
            "--nocapture",
        ])
        .env(CANARY, "1")
        .output()
        .expect("should be able to re-run this test binary as a child");

    assert!(
        !status.status.success(),
        "allocating inside assert_no_alloc did not fail the child process — \
         the real-time gate is inert and the other tests in this file prove nothing"
    );
}
