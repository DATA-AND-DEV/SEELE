//! The half of M1.16 that runs without hardware.
//!
//! > **Aceite:** soak de 10 min sem estalo · perda induzida de 5% inteligível ·
//! > relatório de latência boca-a-ouvido
//!
//! The third one needs a microphone and a room, and `docs/teste-duas-maquinas.md`
//! is the procedure for it. The first two are about the pipeline holding up over
//! time and under loss, and that is testable here — in **simulated** time, so
//! ten minutes of audio takes a second of wall clock.
//!
//! # What a simulated soak can and cannot say
//!
//! It **can** say that the jitter buffer does not drift, that concealment
//! matches loss, that nothing panics over thirty thousand frames, and that the
//! output keeps tracking the input. Those are real failures and they are the
//! ones that only appear with time.
//!
//! It **cannot** say anything about a sound card. Buffer underruns, driver
//! hiccups, and the clicks that come from either are exactly what the two-machine
//! test exists for. A green run here is a precondition, not a substitute.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use seele_audio::codec::{VoiceDecoder, VoiceEncoder};
use seele_audio::jitter::{Decision, JitterBuffer, JitterConfig};
use seele_audio::netsim::{talker_stream, ArrivedFrame, Network, NetworkProfile};
use seele_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};

/// Frames in one minute at 20 ms each.
const FRAMES_PER_MINUTE: usize = 60_000 / FRAME_MS as usize;

/// How long the soak runs.
///
/// Ten minutes by default, because that is what the acceptance criterion says
/// and simulated time is cheap. `SEELE_SOAK_MINUTES` makes it longer when
/// somebody is chasing something that takes an hour to show up.
fn soak_minutes() -> usize {
    std::env::var("SEELE_SOAK_MINUTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10)
}

/// Speech-like audio: a voiced tone with a formant and an envelope.
///
/// Not a pure sine. Opus is a speech codec, and a single tone is both easier to
/// encode and less representative than anything a person produces.
fn speech(frame_index: usize) -> Vec<f32> {
    (0..FRAME_SAMPLES)
        .map(|sample| {
            let absolute = frame_index * FRAME_SAMPLES + sample;
            #[allow(
                clippy::cast_precision_loss,
                reason = "sample index in an f32 pipeline"
            )]
            let time = absolute as f32 / SAMPLE_RATE_HZ as f32;
            let fundamental = (time * 140.0 * std::f32::consts::TAU).sin();
            let formant = (time * 850.0 * std::f32::consts::TAU).sin() * 0.4;
            // A slow envelope, so the encoder sees syllables rather than a
            // drone — but never reaching zero. A frame quiet enough for DTX to
            // swallow would be dropped here *after* `talker_stream` already
            // spent a sequence number on it, and the receiver would correctly
            // read that as packet loss. Real DTX never consumes a sequence
            // number; that is the whole basis of telling silence from loss
            // (M1.9), and this harness must not counterfeit it.
            let envelope = 0.6 + 0.4 * (time * 3.0 * std::f32::consts::TAU).sin();
            (fundamental + formant) * envelope * 0.3
        })
        .collect()
}

/// Root-mean-square of a frame.
fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "one frame")]
    let mean = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    mean.sqrt()
}

/// What one run came to.
#[derive(Debug, Default)]
struct Outcome {
    played: u64,
    concealed: u64,
    silence: u64,
    starved: u64,
    /// Depth of the jitter buffer, sampled once a second.
    depths_ms: Vec<f64>,
    /// Output level, sampled once a second, for the frames that carried audio.
    levels: Vec<f32>,
    /// Frames the network actually delivered, so the assertions can compare
    /// what came out against what arrived rather than against an estimate.
    delivered: u64,
}

/// Runs a talker through a network into a jitter buffer and out of a decoder.
///
/// One sender, one receiver, everything the real path does except the sound
/// card and the socket.
///
/// The receiver runs on a **fixed 20 ms clock** over the whole duration, not
/// one tick per frame that happened to be transmitted. That distinction is the
/// whole point of M1.9: while a talker is silent, DTX sends nothing and the
/// receiver keeps ticking — and a harness that stopped the clock during silence
/// would report the silence as a buffer five seconds deep.
fn run(profile: NetworkProfile, minutes: usize, seed: u64) -> Outcome {
    let ticks = FRAMES_PER_MINUTE * minutes;

    // A talker who speaks for two seconds and pauses for one, which is what DTX
    // and voice activation produce on the wire.
    let stream = talker_stream(100, 50, ticks / 150 + 2);

    let mut network = Network::new(profile, seed);
    let mut encoder = VoiceEncoder::with_defaults().expect("encoder");
    let mut decoder = VoiceDecoder::new().expect("decoder");
    let mut buffer: JitterBuffer<Vec<u8>> = JitterBuffer::new(JitterConfig::default());

    let mut outcome = Outcome::default();
    let mut scratch: Vec<ArrivedFrame> = Vec::new();
    // In flight: arrived_at_ms is in the future relative to the receiver clock.
    let mut flying: Vec<(ArrivedFrame, Vec<u8>)> = Vec::new();
    let mut next_to_send = 0_usize;

    for tick in 0..ticks {
        #[allow(clippy::cast_precision_loss, reason = "a tick count over ten minutes")]
        let now_ms = tick as f64 * f64::from(FRAME_MS);

        // ---- sender: everything whose turn has come ----
        while let Some(sent) = stream.get(next_to_send) {
            if sent.sent_at_ms > now_ms {
                break;
            }
            next_to_send += 1;

            let payload = encoder.encode(&speech(tick)).expect("encode");
            // Must not happen: the envelope above keeps every frame loud enough
            // that DTX stays out of it. If it ever does, the sequence number is
            // already spent and the receiver would read loss where there was
            // silence — so fail loudly rather than skew the measurement.
            assert!(
                !payload.is_empty(),
                "DTX swallowed a frame the harness had already numbered"
            );

            scratch.clear();
            network.transmit(*sent, &mut scratch);
            for arrival in &scratch {
                flying.push((*arrival, payload.clone()));
            }
        }

        // ---- receiver: whatever has landed by now ----
        let mut still_flying = Vec::with_capacity(flying.len());
        for (arrival, payload) in flying.drain(..) {
            if arrival.arrived_at_ms <= now_ms {
                buffer.push(
                    arrival.seq,
                    arrival.timestamp,
                    arrival.arrived_at_ms,
                    payload,
                );
            } else {
                still_flying.push((arrival, payload));
            }
        }
        flying = still_flying;

        // ---- playout, exactly one frame per tick ----
        match buffer.tick() {
            Decision::Play(bytes) => {
                let decoded = decoder.decode(&bytes).unwrap_or_default();
                outcome.played += 1;
                if tick % 50 == 0 {
                    outcome.levels.push(rms(&decoded));
                }
            }
            Decision::Conceal => {
                let _ = decoder.conceal();
                outcome.concealed += 1;
            }
            Decision::Silence | Decision::Comfort => outcome.silence += 1,
            Decision::Starved => outcome.starved += 1,
        }

        if tick % 50 == 0 {
            outcome.depths_ms.push(buffer.metrics().depth_ms);
        }
    }

    outcome.delivered = network.stats().delivered;
    outcome
}

/// Ten minutes on a good network, which is the "sem estalo" half.
///
/// A click, in this pipeline, comes from the buffer running dry or overflowing.
/// Neither is audible here — there is no sound card — but both are visible: a
/// starved tick is a gap, and a depth that grows without bound is the drift
/// that eventually forces one.
#[test]
fn ten_minutes_on_a_good_network_does_not_drift_or_starve() {
    let minutes = soak_minutes();
    let outcome = run(NetworkProfile::wifi(), minutes, 0xA71F);

    if std::env::var("SEELE_SOAK_TRACE").is_ok() {
        println!(
            "played {} conceal {} silence {} starved {}",
            outcome.played, outcome.concealed, outcome.silence, outcome.starved
        );
        for (index, depth) in outcome.depths_ms.iter().enumerate().take(40) {
            println!("  t={index:>4}s  depth={depth:>7.1}ms");
        }
    }

    assert!(outcome.played > 0, "nothing was ever played");

    // The talker speaks two seconds in every three, so a third of the ticks
    // have nothing to play by construction. The buffer reports those as
    // starved, which is honest — it cannot know a talker paused until the next
    // frame arrives — and it is not a fault.
    //
    // What would be a fault is starving *more* than the talker was silent.
    let ticks = outcome.played + outcome.concealed + outcome.silence + outcome.starved;
    let expected_silent = ticks / 3;
    assert!(
        outcome.starved < expected_silent + ticks / 20,
        "starved on {} of {ticks} ticks; the talker was only quiet for about {expected_silent}",
        outcome.starved
    );

    // And everything that was sent has to come out. A pipeline that stays shallow
    // by throwing audio away would pass every depth check.
    let expected_played = ticks * 2 / 3;
    assert!(
        outcome.played > expected_played - ticks / 20,
        "only {} of about {expected_played} transmitted frames were played",
        outcome.played
    );

    // The depth must not climb. An unbounded climb is latency growing silently
    // until something has to be thrown away — the failure M1.8 exists to stop,
    // and the one that only shows up after ten minutes.
    // Compared from after the warm-up, not from the first sample: sample zero
    // is taken before the buffer has prebuffered anything, so it is always
    // zero, and comparing a running buffer against "not started yet" measures
    // the start rather than the drift.
    let warm = outcome.depths_ms.get(5).copied().unwrap_or_default();
    let last = outcome.depths_ms.last().copied().unwrap_or_default();
    assert!(
        (last - warm).abs() < 100.0,
        "the jitter buffer drifted from {warm:.1} ms to {last:.1} ms over {minutes} min"
    );

    // The regression that this whole test exists for. Before the fix in
    // `plan_gap`, the depth climbed to a full second within four seconds and
    // stayed there: every pause was replayed after the fact, so playout fell
    // one pause behind and never caught up.
    let deep = outcome
        .depths_ms
        .iter()
        .filter(|depth| **depth > 200.0)
        .count();
    assert_eq!(
        deep,
        0,
        "the buffer went past 200 ms on {deep} of {} samples — playout is falling behind pauses",
        outcome.depths_ms.len()
    );

    let deepest = outcome.depths_ms.iter().copied().fold(0.0_f64, f64::max);
    assert!(
        deepest < 200.0,
        "the buffer reached {deepest:.1} ms, well past anything a conversation tolerates"
    );
}

/// Five percent loss, and the audio has to survive it.
///
/// `specs/09-roadmap.md` asks for "perda induzida de 5% inteligível".
/// Intelligibility needs an ear, so what is asserted here is the property that
/// makes it possible: concealment tracks the loss instead of the stream falling
/// apart, and the output keeps carrying level.
#[test]
fn five_percent_loss_is_concealed_rather_than_dropped() {
    let outcome = run(NetworkProfile::acceptance_five_percent_loss(), 2, 0x5105);

    let ticks = outcome.played + outcome.concealed + outcome.silence + outcome.starved;
    assert!(ticks > 1000, "the run was too short to say anything");

    // Measured against what the network delivered, not against the tick count.
    // A third of the ticks are the talker pausing, and counting those as
    // undelivered audio would make DTX look like packet loss — the exact
    // confusion M1.9 exists to prevent.
    #[allow(clippy::cast_precision_loss, reason = "counters, compared as a ratio")]
    let rendered = outcome.played as f64 / outcome.delivered.max(1) as f64;
    assert!(
        rendered > 0.90,
        "only {:.0}% of the frames the network delivered were played \
         ({} played of {} delivered; {} concealed, {} starved, {} silent)",
        rendered * 100.0,
        outcome.played,
        outcome.delivered,
        outcome.concealed,
        outcome.starved,
        outcome.silence
    );

    // Concealment must actually engage. If nothing was concealed at 5% loss,
    // the losses are being turned into silence, which is what sounds broken.
    assert!(
        outcome.concealed > 0,
        "5% loss produced no concealment at all"
    );

    // And the output must still carry signal. A pipeline that "survives" loss
    // by outputting silence passes every counter and fails every listener.
    let quiet = outcome
        .levels
        .iter()
        .filter(|level| **level < 0.001)
        .count();
    assert!(
        quiet * 4 < outcome.levels.len(),
        "{quiet} of {} sampled frames came out silent",
        outcome.levels.len()
    );
}

/// The simulator has to be doing what the test asked, or the test passes for
/// the wrong reason.
#[test]
fn the_five_percent_profile_really_loses_about_five_percent() {
    let mut network = Network::new(NetworkProfile::acceptance_five_percent_loss(), 0x5105);
    let mut arrivals = Vec::new();

    for frame in talker_stream(100, 50, 100) {
        network.transmit(frame, &mut arrivals);
        arrivals.clear();
    }

    let loss = network.stats().loss_fraction();
    assert!(
        (0.03..=0.08).contains(&loss),
        "the profile intended 5% loss and produced {:.1}%",
        loss * 100.0
    );
}
