//! M1.11 — the whole audio pipeline, over plain UDP, between two processes.
//!
//! **Throwaway.** `specs/01-arquitetura.md` and `specs/08-seguranca.md` are
//! categorical that there is no unencrypted path and no flag to disable TLS.
//! This crate exists to prove the audio works before M2 puts QUIC underneath,
//! and it dies with M1. Nothing may depend on it.
//!
//! One consequence worth remembering: any latency measured here **excludes**
//! QUIC and TLS overhead, so the number is not the product's number.
//!
//! # What it wires together
//!
//! Everything M1 built, in the order `specs/03-audio.md` lays out:
//!
//! ```text
//! capture → ring → resample → gate → Opus encode → UDP
//! UDP → jitter buffer (per ssrc) → decode/PLC → mixer → resample → ring → playback
//! ```
//!
//! # Usage
//!
//! Two machines:
//!
//! ```text
//! # on 192.168.1.10
//! voice-link --listen 0.0.0.0:9000 --peer 192.168.1.11:9000
//! # on 192.168.1.11
//! voice-link --listen 0.0.0.0:9000 --peer 192.168.1.10:9000
//! ```
//!
//! One machine, sending to itself — **wear headphones**, or the speaker feeds
//! the microphone and it howls:
//!
//! ```text
//! voice-link --loopback
//! ```

use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use seele_audio::device::{self, AudioIo};
use seele_audio::drift::DriftTracker;
use seele_audio::gate::{GateMode, VoiceGate};
use seele_audio::jitter::{Decision, JitterBuffer, JitterConfig};
use seele_audio::mixer::Mixer;
use seele_audio::resample::RateConverter;
use seele_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use seele_proto::media::{MAX_DATAGRAM_LEN, MediaHeader};
use shiguredo_opus::{
    Application, Decoder, DecoderConfig, Encoder, EncoderConfig, FrameDuration, InbandFec,
};

const RING_MS: u32 = 100;
const BITRATE_BPS: u32 = 32_000;
/// How often telemetry is printed.
const REPORT_EVERY: Duration = Duration::from_secs(5);

struct Args {
    listen: SocketAddr,
    peer: SocketAddr,
    ssrc: u32,
    voice_activated: bool,
    deafen: bool,
    tone: bool,
}

fn parse_args() -> Option<Args> {
    let mut listen = None;
    let mut peer = None;
    let mut ssrc = 1_u32;
    let mut voice_activated = true;
    let mut deafen = false;
    let mut tone = false;
    let mut argv = std::env::args().skip(1);

    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--loopback" => {
                listen = "127.0.0.1:9000".parse().ok();
                peer = "127.0.0.1:9000".parse().ok();
            }
            "--listen" => listen = argv.next()?.parse().ok(),
            "--peer" => peer = argv.next()?.parse().ok(),
            "--ssrc" => ssrc = argv.next()?.parse().ok()?,
            "--push-to-talk" => voice_activated = false,
            // specs/07-tema-evangelion.md calls this "Isolamento total". Also the
            // only safe way to exercise the loop on one machine with built-in
            // speakers, which would otherwise howl.
            "--deafen" => deafen = true,
            // Replaces the microphone with a synthetic tone, so the whole
            // encode/UDP/jitter/decode/mix path can be proven without a second
            // person and without a quiet room closing the gate.
            "--tone" => tone = true,
            other => {
                eprintln!("unknown flag: {other}");
                return None;
            }
        }
    }

    Some(Args {
        listen: listen?,
        peer: peer?,
        ssrc,
        voice_activated,
        deafen,
        tone,
    })
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  voice-link --listen <bind> --peer <addr> [--ssrc N] [--push-to-talk]");
    eprintln!("  voice-link --loopback          # send to self; WEAR HEADPHONES");
    eprintln!("  --deafen                       # run everything, play nothing (no feedback)");
    eprintln!("  --tone                         # send a synthetic tone instead of the mic");
}

fn main() {
    let Some(args) = parse_args() else {
        usage();
        std::process::exit(2);
    };

    if let Err(error) = run(&args) {
        eprintln!("voice-link: {error}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), String> {
    let io = device::open_default(RING_MS).map_err(|error| error.to_string())?;
    let socket = UdpSocket::bind(args.listen).map_err(|error| format!("bind failed: {error}"))?;

    println!("listening on {}, sending to {}", args.listen, args.peer);
    println!(
        "capture {} Hz, playback {} Hz, ssrc {}",
        io.capture_rate_hz, io.playback_rate_hz, args.ssrc
    );
    if args.listen == args.peer {
        println!("\nLOOPBACK: your own voice comes back. Wear headphones.\n");
    }

    // Receiving blocks, so it gets its own thread. `cpal::Stream` is not `Send`
    // on every backend, so everything touching the devices stays on this one.
    let (tx, rx) = mpsc::channel::<(MediaHeader, Vec<u8>, Instant)>();
    let rx_socket = socket.try_clone().map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        let mut buffer = [0_u8; MAX_DATAGRAM_LEN];
        loop {
            let Ok((len, _from)) = rx_socket.recv_from(&mut buffer) else {
                continue;
            };
            let Some(datagram) = buffer.get(..len) else {
                continue;
            };
            // A malformed datagram is dropped and forgotten. specs/08 wants the
            // parser to be total, and seele-proto's is fuzzed for exactly this.
            if let Ok((header, payload)) = MediaHeader::decode(datagram) {
                let _ = tx.send((header, payload.to_vec(), Instant::now()));
            }
        }
    });

    pipeline(io, socket, args, &rx)
}

#[allow(clippy::too_many_lines, reason = "throwaway harness; clarity over structure")]
fn pipeline(
    mut io: AudioIo,
    socket: UdpSocket,
    args: &Args,
    rx: &mpsc::Receiver<(MediaHeader, Vec<u8>, Instant)>,
) -> Result<(), String> {
    let mut encoder = {
        let mut config = EncoderConfig::new(SAMPLE_RATE_HZ, 1);
        config.application = Some(Application::Voip);
        config.frame_duration = Some(FrameDuration::Ms20);
        config.bitrate = Some(BITRATE_BPS);
        // ADR 0010: off in v1. It only helps if the decoder gets the packet
        // after the lost one, which costs the jitter buffer 20 ms.
        config.inband_fec = Some(InbandFec::Disabled);
        config.dtx = Some(true);
        Encoder::new(config).map_err(|error| error.to_string())?
    };

    let mut to_pipeline = RateConverter::new(io.capture_rate_hz, SAMPLE_RATE_HZ)
        .map_err(|error| error.to_string())?;
    let mut to_device = RateConverter::new(SAMPLE_RATE_HZ, io.playback_rate_hz)
        .map_err(|error| error.to_string())?;

    let mut gate = VoiceGate::new(
        seele_audio::gate::GateConfig::default(),
        if args.voice_activated {
            GateMode::VoiceActivated
        } else {
            // Without global key capture (open decision, task M1.13) there is
            // nothing to hold, so push-to-talk here means permanently open.
            GateMode::Open
        },
    );
    let mut mixer = Mixer::new();
    if args.deafen {
        mixer.set_master(0.0);
        println!("deafened: the pipeline runs, nothing reaches the speaker");
    }

    // One jitter buffer, one drift tracker and one decoder per talker.
    let mut sources: Vec<(u32, JitterBuffer<Vec<u8>>, DriftTracker, Decoder)> = Vec::new();

    let mut captured = Vec::new();
    let mut at_48k = Vec::new();
    let mut pending = Vec::<f32>::new();
    let mut frame_i16 = vec![0_i16; FRAME_SAMPLES];
    let mut datagram = vec![0_u8; MAX_DATAGRAM_LEN];
    let mut mixed = vec![0.0_f32; FRAME_SAMPLES];
    let mut for_device = Vec::new();

    let mut tone_phase = 0.0_f32;
    let mut seq = 0_u16;
    let mut timestamp = 0_u32;
    let mut sent = 0_u64;
    let started = Instant::now();
    let mut next_playout = Instant::now();
    let mut next_report = Instant::now() + REPORT_EVERY;

    println!("running. ctrl-c to stop.\n");

    loop {
        // ---- capture -> encode -> send ----
        captured.clear();
        while let Ok(sample) = io.captured.pop() {
            captured.push(if args.tone {
                let value =
                    (tone_phase * 440.0 * std::f32::consts::TAU).sin() * 0.3;
                tone_phase += 1.0 / SAMPLE_RATE_HZ as f32;
                value
            } else {
                sample
            });
        }
        at_48k.clear();
        to_pipeline
            .push(&captured, &mut at_48k)
            .map_err(|error| error.to_string())?;
        pending.extend_from_slice(&at_48k);

        while pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();
            let speaking = gate.update(&frame);

            // The timestamp counts elapsed samples whether or not anything was
            // sent; the sequence counts only what went on the wire. That is what
            // lets the receiver tell silence from loss — task M1.9.
            timestamp = timestamp.wrapping_add(FRAME_SAMPLES as u32);
            if !speaking {
                continue;
            }

            for (slot, sample) in frame_i16.iter_mut().zip(frame.iter()) {
                *slot = (sample.clamp(-1.0, 1.0) * 32_768.0).round().clamp(-32_768.0, 32_767.0)
                    as i16;
            }
            let Ok(payload) = encoder.encode(&frame_i16) else {
                continue;
            };
            seq = seq.wrapping_add(1);
            let header = MediaHeader {
                version: seele_proto::PROTOCOL_VERSION,
                ssrc: args.ssrc,
                seq,
                timestamp,
            };
            if let Ok(len) = header.encode_datagram(&payload, &mut datagram) {
                if let Some(bytes) = datagram.get(..len) {
                    let _ = socket.send_to(bytes, args.peer);
                    sent += 1;
                }
            }
        }

        // ---- receive -> jitter buffer ----
        while let Ok((header, payload, at)) = rx.try_recv() {
            let arrival_ms = at.duration_since(started).as_secs_f64() * 1000.0;
            let index = match sources.iter().position(|(ssrc, ..)| *ssrc == header.ssrc) {
                Some(index) => index,
                None => {
                    let Ok(decoder) = Decoder::new(DecoderConfig::new(SAMPLE_RATE_HZ, 1)) else {
                        continue;
                    };
                    println!("new talker: ssrc {}", header.ssrc);
                    sources.push((
                        header.ssrc,
                        JitterBuffer::new(JitterConfig::default()),
                        DriftTracker::new(),
                        decoder,
                    ));
                    sources.len() - 1
                }
            };
            if let Some((_, buffer, tracker, _)) = sources.get_mut(index) {
                let sent_ms = f64::from(header.timestamp) / f64::from(SAMPLE_RATE_HZ) * 1000.0;
                tracker.observe(arrival_ms - sent_ms, arrival_ms);
                buffer.push(header.seq, header.timestamp, arrival_ms, payload);
            }
        }

        // ---- playout clock: one frame every 20 ms ----
        if Instant::now() >= next_playout {
            next_playout += Duration::from_millis(u64::from(FRAME_MS));

            let mut decoded: Vec<(u32, Vec<f32>)> = Vec::new();
            for (ssrc, buffer, _, decoder) in &mut sources {
                let samples = match buffer.tick() {
                    Decision::Play(payload) => decoder.decode_f32(&payload).ok(),
                    // specs/03-audio.md: one frame of PLC, then silence.
                    Decision::Conceal => decoder.decode_plc_f32().ok(),
                    Decision::Silence | Decision::Comfort | Decision::Starved => None,
                };
                if let Some(samples) = samples {
                    decoded.push((*ssrc, samples));
                }
            }

            let borrowed: Vec<(u32, &[f32])> = decoded
                .iter()
                .map(|(ssrc, samples)| (*ssrc, samples.as_slice()))
                .collect();
            mixer.mix(&borrowed, &mut mixed);

            for_device.clear();
            to_device
                .push(&mixed, &mut for_device)
                .map_err(|error| error.to_string())?;
            for sample in for_device.drain(..) {
                let _ = io.to_device.push(sample);
            }
        }

        // ---- telemetry ----
        if Instant::now() >= next_report {
            next_report += REPORT_EVERY;
            let stream = io.counters.snapshot();
            println!(
                "sent {sent:>6} | in {:.3} | out {:.3} | over {} under {} err {}",
                gate.metrics().input_rms,
                mixer.metrics().peak_output,
                stream.capture_overruns,
                stream.playback_underruns,
                stream.stream_errors,
            );
            for (ssrc, buffer, tracker, _) in &sources {
                let metrics = buffer.metrics();
                println!(
                    "  ssrc {ssrc:>5} | played {:>6} | depth {:>5.1}ms | target {:>5.1}ms | \
                     loss {:>5.2}% | conceal {:>4} | drift {:>6.1}ppm",
                    metrics.frames_played,
                    metrics.depth_ms,
                    metrics.target_ms,
                    metrics.loss_fraction() * 100.0,
                    metrics.frames_concealed,
                    tracker.drift_ppm(),
                );
            }
        }

        std::thread::sleep(Duration::from_millis(2));
    }
}
