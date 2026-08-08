//! The voice path, as one thing a shell can hold.
//!
//! `specs/01-arquitetura.md` puts all session, protocol and audio logic in this
//! crate, and ADR 0002 keeps `magi-tui` from depending on `magi-audio` at all.
//! Those two together mean the capture → encode → send → receive → jitter →
//! decode → mix → play loop cannot live in the interface, however convenient
//! that would be. The M2 spike put it in the client binary because a spike is
//! allowed to; this is the version that survives.
//!
//! What a shell gets is a handle with verbs from the product's own vocabulary —
//! A.T. Field, Isolamento total, push-to-talk — and a telemetry snapshot. What
//! it does not get is a sample, a sequence number, or an opinion about Opus.
//!
//! # Why its own thread
//!
//! `cpal`'s streams are not `Send` on every backend, so the pipeline cannot be
//! a `tokio::spawn`ed task on the interface's runtime. It gets a dedicated
//! thread with a current-thread runtime instead. That is not a workaround: the
//! real-time path should not be sharing a scheduler with a terminal redraw.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use magi_audio::device::{self, AudioIo};
use magi_audio::drift::DriftTracker;
use magi_audio::gate::{GateConfig, GateMode, VoiceGate};
use magi_audio::jitter::{Decision, JitterBuffer, JitterConfig};
use magi_audio::mixer::Mixer;
use magi_audio::resample::RateConverter;
use magi_audio::telemetry::{AudioTelemetry, LocalTelemetry, SourceTelemetry};
use magi_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use magi_proto::ids::Ssrc;
use magi_proto::MediaHeader;
use shiguredo_opus::{
    Application, Decoder, DecoderConfig, Encoder, EncoderConfig, FrameDuration, InbandFec,
};

use crate::client::MediaChannel;

/// Ring size between the device callbacks and the pipeline.
///
/// Absorbs scheduling jitter only. Not the jitter buffer.
const RING_MS: u32 = 100;

/// Default encoder bitrate. `specs/03-audio.md`, narrowed by ADR 0010.
const DEFAULT_BITRATE_BPS: u32 = 32_000;

/// How often the telemetry snapshot is refreshed.
///
/// The interface redraws far more often than this, and reassembling metrics on
/// every frame would burn the real-time thread on something nobody can read at
/// that rate.
const TELEMETRY_EVERY: Duration = Duration::from_millis(250);

/// How the microphone decides to open. `specs/03-audio.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceMode {
    /// A key is held. Never false-triggers, so it is the default.
    PushToTalk,
    /// The level decides, with hysteresis and hangover.
    VoiceActivated,
    /// Always open. A recording setup, never a default.
    Open,
}

impl VoiceMode {
    fn to_gate(self) -> GateMode {
        match self {
            Self::PushToTalk => GateMode::PushToTalk,
            Self::VoiceActivated => GateMode::VoiceActivated,
            Self::Open => GateMode::Open,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            1 => Self::VoiceActivated,
            2 => Self::Open,
            _ => Self::PushToTalk,
        }
    }

    fn as_byte(self) -> u8 {
        match self {
            Self::PushToTalk => 0,
            Self::VoiceActivated => 1,
            Self::Open => 2,
        }
    }
}

/// What the pilot has set, read by the audio thread every frame.
///
/// Atomics rather than a lock: this is read from the thread that must not
/// block, and every field is one machine word.
#[derive(Debug)]
struct Controls {
    at_field: AtomicBool,
    total_isolation: AtomicBool,
    key_held: AtomicBool,
    mode: AtomicU8,
    bitrate: AtomicU32,
    speaking: AtomicBool,
    stop: AtomicBool,
    /// Per-talker volume. Read once per 20 ms frame, so a lock is affordable
    /// here in a way it would not be inside a device callback.
    gains: Mutex<HashMap<u32, f32>>,
}

/// What the devices turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceRates {
    /// Native capture rate. Not necessarily 48 kHz — gap G7.
    pub capture_hz: u32,
    /// Native playback rate.
    pub playback_hz: u32,
}

/// A running voice path.
///
/// Dropping it stops the audio.
#[derive(Debug)]
pub struct Voice {
    controls: Arc<Controls>,
    telemetry: Arc<Mutex<AudioTelemetry>>,
    rates: DeviceRates,
}

impl Voice {
    /// Opens the devices and starts the pipeline.
    ///
    /// # Errors
    ///
    /// Fails if no input or output device can be opened, or if the encoder
    /// refuses the configuration.
    pub fn start(media: MediaChannel, ssrc: Ssrc) -> Result<Self> {
        // Opened here rather than on the audio thread so that "there is no
        // microphone" is a return value the interface can show, instead of a
        // thread that quietly dies.
        let io = device::open_default(RING_MS)?;
        let rates = DeviceRates {
            capture_hz: io.capture_rate_hz,
            playback_hz: io.playback_rate_hz,
        };

        let controls = Arc::new(Controls {
            at_field: AtomicBool::new(false),
            total_isolation: AtomicBool::new(false),
            key_held: AtomicBool::new(false),
            // specs/03-audio.md makes push-to-talk the default because it never
            // false-triggers, and a client that transmits a room by accident is
            // worse than one that misses a word.
            mode: AtomicU8::new(VoiceMode::PushToTalk.as_byte()),
            bitrate: AtomicU32::new(DEFAULT_BITRATE_BPS),
            speaking: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            gains: Mutex::new(HashMap::new()),
        });
        let telemetry = Arc::new(Mutex::new(AudioTelemetry::default()));

        let thread_controls = Arc::clone(&controls);
        let thread_telemetry = Arc::clone(&telemetry);
        std::thread::Builder::new()
            .name("magi-voice".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => return,
                };
                runtime.block_on(pipeline(io, media, ssrc, thread_controls, thread_telemetry));
            })?;

        Ok(Self {
            controls,
            telemetry,
            rates,
        })
    }

    /// What the devices actually run at.
    #[must_use]
    pub fn rates(&self) -> DeviceRates {
        self.rates
    }

    /// Mutes the microphone — A.T. Field.
    pub fn set_at_field(&self, on: bool) {
        self.controls.at_field.store(on, Ordering::Relaxed);
    }

    /// Whether the microphone is muted.
    #[must_use]
    pub fn at_field(&self) -> bool {
        self.controls.at_field.load(Ordering::Relaxed)
    }

    /// Mutes the speakers — Isolamento total.
    pub fn set_total_isolation(&self, on: bool) {
        self.controls.total_isolation.store(on, Ordering::Relaxed);
    }

    /// Whether the speakers are muted.
    #[must_use]
    pub fn total_isolation(&self) -> bool {
        self.controls.total_isolation.load(Ordering::Relaxed)
    }

    /// Reports the push-to-talk key going down or coming up.
    pub fn set_key_held(&self, held: bool) {
        self.controls.key_held.store(held, Ordering::Relaxed);
    }

    /// Chooses how the microphone opens.
    pub fn set_mode(&self, mode: VoiceMode) {
        self.controls.mode.store(mode.as_byte(), Ordering::Relaxed);
    }

    /// How the microphone currently opens.
    #[must_use]
    pub fn mode(&self) -> VoiceMode {
        VoiceMode::from_byte(self.controls.mode.load(Ordering::Relaxed))
    }

    /// Sets one talker's volume. `1.0` is unchanged, `0.0` is muted.
    ///
    /// `specs/03-audio.md` asks for per-user volume, and this is where it lives:
    /// it is a property of the mix, not of the interface showing the mix.
    pub fn set_gain(&self, ssrc: u32, gain: f32) {
        if let Ok(mut gains) = self.controls.gains.lock() {
            gains.insert(ssrc, gain.clamp(0.0, 4.0));
        }
    }

    /// Whether this pilot is transmitting right now.
    #[must_use]
    pub fn speaking(&self) -> bool {
        self.controls.speaking.load(Ordering::Relaxed)
    }

    /// The latest measurements.
    #[must_use]
    pub fn telemetry(&self) -> AudioTelemetry {
        self.telemetry
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default()
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.controls.stop.store(true, Ordering::Relaxed);
    }
}

/// One talker being decoded.
struct Source {
    ssrc: u32,
    buffer: JitterBuffer<Vec<u8>>,
    drift: DriftTracker,
    decoder: Decoder,
}

/// The whole loop, on its own thread.
#[allow(
    clippy::too_many_lines,
    reason = "one real-time loop; splitting it would hide the ordering that matters"
)]
async fn pipeline(
    mut io: AudioIo,
    media: MediaChannel,
    ssrc: Ssrc,
    controls: Arc<Controls>,
    telemetry: Arc<Mutex<AudioTelemetry>>,
) {
    let Ok(mut encoder) = build_encoder(DEFAULT_BITRATE_BPS) else {
        return;
    };
    let (Ok(mut to_pipeline), Ok(mut to_device)) = (
        RateConverter::new(io.capture_rate_hz, SAMPLE_RATE_HZ),
        RateConverter::new(SAMPLE_RATE_HZ, io.playback_rate_hz),
    ) else {
        return;
    };

    let mut gate = VoiceGate::new(GateConfig::default(), GateMode::PushToTalk);
    let mut mixer = Mixer::new();
    let mut sources: Vec<Source> = Vec::new();

    let (mut captured, mut at_48k, mut pending) = (Vec::new(), Vec::new(), Vec::<f32>::new());
    let mut frame_i16 = vec![0_i16; FRAME_SAMPLES];
    let mut datagram = vec![0_u8; magi_proto::MAX_DATAGRAM_LEN];
    let mut mixed = vec![0.0_f32; FRAME_SAMPLES];
    let mut for_device = Vec::new();

    let (mut seq, mut timestamp) = (0_u16, 0_u32);
    let started = Instant::now();
    let mut next_playout = Instant::now();
    let mut next_telemetry = Instant::now() + TELEMETRY_EVERY;
    let mut bitrate = DEFAULT_BITRATE_BPS;

    while !controls.stop.load(Ordering::Relaxed) {
        // ---- receive ----
        while let Ok(Ok(bytes)) = tokio::time::timeout(Duration::from_millis(1), media.next()).await
        {
            let arrival_ms = started.elapsed().as_secs_f64() * 1000.0;
            let Ok((header, payload)) = MediaHeader::decode(&bytes) else {
                continue;
            };
            // Our own audio coming back is not something to play.
            if header.ssrc == ssrc.get() {
                continue;
            }
            let index = match sources.iter().position(|s| s.ssrc == header.ssrc) {
                Some(index) => index,
                None => {
                    let Ok(decoder) = Decoder::new(DecoderConfig::new(SAMPLE_RATE_HZ, 1)) else {
                        continue;
                    };
                    sources.push(Source {
                        ssrc: header.ssrc,
                        buffer: JitterBuffer::new(JitterConfig::default()),
                        drift: DriftTracker::new(),
                        decoder,
                    });
                    sources.len() - 1
                }
            };
            if let Some(source) = sources.get_mut(index) {
                let sent_ms = f64::from(header.timestamp) / f64::from(SAMPLE_RATE_HZ) * 1000.0;
                source.drift.observe(arrival_ms - sent_ms, arrival_ms);
                source
                    .buffer
                    .push(header.seq, header.timestamp, arrival_ms, payload.to_vec());
            }
        }

        // ---- capture, encode, send ----
        gate.set_mode(VoiceMode::from_byte(controls.mode.load(Ordering::Relaxed)).to_gate());
        gate.set_key_held(controls.key_held.load(Ordering::Relaxed));

        let wanted_bitrate = controls.bitrate.load(Ordering::Relaxed);
        if wanted_bitrate != bitrate {
            if let Ok(fresh) = build_encoder(wanted_bitrate) {
                encoder = fresh;
                bitrate = wanted_bitrate;
            }
        }

        captured.clear();
        while let Ok(sample) = io.captured.pop() {
            captured.push(sample);
        }
        at_48k.clear();
        if to_pipeline.push(&captured, &mut at_48k).is_ok() {
            pending.extend_from_slice(&at_48k);
        }

        let at_field = controls.at_field.load(Ordering::Relaxed);
        while pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();
            // The gate still runs while muted, so the level meter keeps moving
            // and somebody talking into a muted microphone can see that they
            // are. Not showing that is how people give whole speeches to nobody.
            let open = gate.update(&frame);
            let speaking = open && !at_field;
            controls.speaking.store(speaking, Ordering::Relaxed);

            // The timestamp counts elapsed samples whether or not anything goes
            // out; the sequence counts only what does. That difference is what
            // lets the receiver tell DTX silence from real loss — M1.9.
            timestamp = timestamp.wrapping_add(u32::try_from(FRAME_SAMPLES).unwrap_or(FRAME_MS));
            if !speaking {
                continue;
            }

            for (slot, sample) in frame_i16.iter_mut().zip(frame.iter()) {
                *slot = (sample.clamp(-1.0, 1.0) * 32_768.0)
                    .round()
                    .clamp(-32_768.0, 32_767.0) as i16;
            }
            let Ok(payload) = encoder.encode(&frame_i16) else {
                continue;
            };
            seq = seq.wrapping_add(1);
            let header = MediaHeader {
                version: magi_proto::PROTOCOL_VERSION,
                // The server refuses anything but the ssrc it assigned — G2.
                ssrc: ssrc.get(),
                seq,
                timestamp,
            };
            if let Ok(len) = header.encode_datagram(&payload, &mut datagram) {
                if let Some(bytes) = datagram.get(..len) {
                    let _ = media.send(bytes.to_vec());
                }
            }
        }

        // ---- playout ----
        if Instant::now() >= next_playout {
            next_playout += Duration::from_millis(u64::from(FRAME_MS));

            // Isolamento total silences the mix rather than stopping the
            // pipeline: the jitter buffers keep draining, so unmuting lands the
            // pilot in the present instead of replaying the last ten seconds.
            mixer.set_master(if controls.total_isolation.load(Ordering::Relaxed) {
                0.0
            } else {
                1.0
            });
            if let Ok(gains) = controls.gains.lock() {
                for (talker, gain) in gains.iter() {
                    mixer.set_gain(*talker, *gain);
                }
            }

            let mut decoded: Vec<(u32, Vec<f32>)> = Vec::new();
            for source in &mut sources {
                let samples = match source.buffer.tick() {
                    Decision::Play(payload) => source.decoder.decode_f32(&payload).ok(),
                    Decision::Conceal => source.decoder.decode_plc_f32().ok(),
                    Decision::Silence | Decision::Comfort | Decision::Starved => None,
                };
                if let Some(samples) = samples {
                    decoded.push((source.ssrc, samples));
                }
            }
            let borrowed: Vec<(u32, &[f32])> = decoded
                .iter()
                .map(|(talker, samples)| (*talker, samples.as_slice()))
                .collect();
            mixer.mix(&borrowed, &mut mixed);

            for_device.clear();
            if to_device.push(&mixed, &mut for_device).is_ok() {
                for sample in for_device.drain(..) {
                    let _ = io.to_device.push(sample);
                }
            }
        }

        // ---- telemetry ----
        if Instant::now() >= next_telemetry {
            next_telemetry += TELEMETRY_EVERY;
            let snapshot = AudioTelemetry {
                local: LocalTelemetry::assemble(
                    io.counters.snapshot(),
                    gate.metrics(),
                    mixer.metrics(),
                    bitrate,
                    controls.speaking.load(Ordering::Relaxed),
                ),
                sources: sources
                    .iter()
                    .map(|source| SourceTelemetry::assemble(source.ssrc, source.buffer.metrics()))
                    .collect(),
            };
            if let Ok(mut slot) = telemetry.lock() {
                *slot = snapshot;
            }
        }

        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

fn build_encoder(bitrate_bps: u32) -> Result<Encoder> {
    let mut config = EncoderConfig::new(SAMPLE_RATE_HZ, 1);
    config.application = Some(Application::Voip);
    config.frame_duration = Some(FrameDuration::Ms20);
    config.bitrate = Some(bitrate_bps);
    // ADR 0010: in-band FEC off, with the measurements behind it.
    config.inband_fec = Some(InbandFec::Disabled);
    config.dtx = Some(true);
    Encoder::new(config).map_err(|error| anyhow!("{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_push_to_talk() {
        // specs/03-audio.md picks it because it never false-triggers, and a
        // client that broadcasts a room by accident is the worse failure.
        assert_eq!(VoiceMode::from_byte(0), VoiceMode::PushToTalk);
    }

    #[test]
    fn modes_survive_the_round_trip_through_an_atomic() {
        for mode in [
            VoiceMode::PushToTalk,
            VoiceMode::VoiceActivated,
            VoiceMode::Open,
        ] {
            assert_eq!(VoiceMode::from_byte(mode.as_byte()), mode);
        }
    }

    #[test]
    fn an_unknown_byte_falls_back_to_the_safe_mode() {
        // Whatever goes wrong, it must not end with an open microphone.
        assert_eq!(VoiceMode::from_byte(200), VoiceMode::PushToTalk);
    }
}
