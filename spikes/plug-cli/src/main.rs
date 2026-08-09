//! The ugly command-line client from `specs/09-roadmap.md`.
//!
//! > Cliente de linha de comando feio, sem TUI, só para exercitar o protocolo.
//!
//! **Throwaway.** M4 builds `plug` properly on `ratatui`, against this same
//! `seele-core`. That is the point of the boundary in `specs/01-arquitetura.md`:
//! the shell changes and the core does not.
//!
//! # What it is actually for
//!
//! This is the first place M1 and M2 meet. Everything below the transport comes
//! from `seele-audio` — capture, resampling, the voice gate, Opus, the jitter
//! buffer, drift tracking, the mixer — and everything above it from `seele-core`.
//! If the two halves disagree about anything, this is where it shows.
//!
//! # Usage
//!
//! ```text
//! seeled 127.0.0.1:8383
//! plug-cli --server 127.0.0.1:8383 --nick ayanami
//! plug-cli --server 127.0.0.1:8383 --nick shinji --tone     # synthetic voice
//! plug-cli --server 127.0.0.1:8383 --nick asuka  --deafen   # listen only, silent
//! ```
//!
//! Wear headphones. ADR 0007 requires them, and without them a speaker feeds the
//! microphone straight back into the Cage.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use seele_audio::device::{self, AudioIo};
use seele_audio::drift::DriftTracker;
use seele_audio::gate::{GateConfig, GateMode, VoiceGate};
use seele_audio::jitter::{Decision, JitterBuffer, JitterConfig};
use seele_audio::mixer::Mixer;
use seele_audio::resample::RateConverter;
use seele_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use seele_core::{Client, MemoryPinStore, PinDecision};
use seele_proto::MediaHeader;
use seele_proto::ids::{CageId, ClientMessageId, LineId};
use seele_proto::ServerMessage;
use shiguredo_opus::{
    Application, Decoder, DecoderConfig, Encoder, EncoderConfig, FrameDuration, InbandFec,
};

const RING_MS: u32 = 100;
const BITRATE_BPS: u32 = 32_000;
const REPORT_EVERY: Duration = Duration::from_secs(5);

struct Args {
    server: SocketAddr,
    nickname: String,
    cage: CageId,
    tone: bool,
    deafen: bool,
    no_audio: bool,
}

fn parse_args() -> Option<Args> {
    let mut server = None;
    let mut nickname = "piloto".to_owned();
    let mut cage = CageId(1);
    let (mut tone, mut deafen, mut no_audio) = (false, false, false);
    let mut argv = std::env::args().skip(1);

    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--server" => server = argv.next()?.parse().ok(),
            "--nick" => nickname = argv.next()?,
            "--cage" => cage = CageId(argv.next()?.parse().ok()?),
            "--tone" => tone = true,
            "--deafen" => deafen = true,
            // Exercises the protocol with no sound card at all, which is what a
            // CI box or a headless VPS has.
            "--no-audio" => no_audio = true,
            other => {
                eprintln!("unknown flag: {other}");
                return None;
            }
        }
    }

    Some(Args {
        server: server?,
        nickname,
        cage,
        tone,
        deafen,
        no_audio,
    })
}

fn usage() {
    eprintln!("usage: plug-cli --server <addr> [--nick <name>] [--cage N]");
    eprintln!("  --tone       send a synthetic voice instead of the microphone");
    eprintln!("  --deafen     run everything, play nothing (Isolamento total)");
    eprintln!("  --no-audio   protocol only, no sound card needed");
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(args) = parse_args() else {
        usage();
        std::process::exit(2);
    };

    // ADR 0004: identity is an Ed25519 key pair. A fresh one per run means a
    // fresh identity, which is all M2 can offer — there are no accounts until
    // M3 brings CASPER.
    let key = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let pins = Arc::new(MemoryPinStore::new());

    println!("PADRÃO: LARANJA — conectando a {}", args.server);
    let mut client = Client::connect(args.server, "localhost", &args.nickname, &key, pins)
        .await
        .context("could not reach PADRÃO: AZUL")?;

    // ADR 0003. specs/08-seguranca.md wants the first contact stated explicitly
    // rather than accepted in silence.
    match client.pin_decision() {
        PinDecision::FirstContact { fingerprint } => {
            println!("PRIMEIRO CONTATO — chave fixada");
            println!("  {fingerprint}");
            println!("  Confira por outro canal se você não confia nesta rede.");
        }
        PinDecision::Matches => println!("chave confere com a fixada"),
        PinDecision::Changed { pinned, offered } => {
            // Unreachable: the handshake fails first. Printed anyway, because a
            // silent impossible branch is how impossible branches stop being so.
            println!("ALERTA · 警告 — A CHAVE DO SERVIDOR MUDOU");
            println!("  fixada:  {pinned}");
            println!("  ofertada:{offered}");
        }
    }

    let session = client.session().clone();
    println!();
    println!("PADRÃO: AZUL");
    println!("  dogma   {}", session.dogma);
    println!("  piloto  {}", session.pilot);
    println!("  ssrc    {}", session.ssrc);
    for cage in &session.cages {
        println!("  cage    {} · {}", cage.id, cage.name);
    }

    client.insert_plug(args.cage).await?;
    client.join_line(LineId(1)).await?;
    client.fetch_history(LineId(1), None, 20).await?;
    println!("\nplug inserido no cage {} · linha 1\n", args.cage);

    if args.no_audio {
        return protocol_only(client).await;
    }
    full_pipeline(client, &args).await
}

/// Exercises the control channel with no sound card in sight.
///
/// Typing a line sends it to Line 1; anything anybody else says shows up here.
/// This is the half of the product that needs no hardware at all, which makes it
/// the half a headless VPS or a CI box can exercise.
async fn protocol_only(mut client: Client) -> Result<()> {
    println!("modo protocolo: sem áudio. digite para falar na linha 1.\n");
    // Media and control are independent (specs/02-protocolo.md), so they can be
    // awaited together without one borrow blocking the other.
    let media = client.media();
    let me = client.session().pilot;
    let mut received = 0_u64;
    let mut next_key = 1_u64;

    // stdin blocks, so it gets its own task and a channel.
    let (typed_tx, mut typed_rx) = tokio::sync::mpsc::channel::<String>(16);
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if typed_tx.blocking_send(line).is_err() {
                return;
            }
        }
    });
    loop {
        tokio::select! {
            typed = typed_rx.recv() => {
                let Some(body) = typed else { return Ok(()) };
                if body.trim().is_empty() {
                    continue;
                }
                // specs/02-protocolo.md: idempotent by client_msg_id, so a
                // resend after a lost acknowledgement does not post twice.
                client.send_message(LineId(1), body.trim(), ClientMessageId(next_key)).await?;
                next_key += 1;
            }
            event = client.next_event() => {
                match event {
                    Ok(ServerMessage::MessageReceived { author, body, .. }) => {
                        let who = if author == me { "você" } else { "outro" };
                        println!("  [{who} {author}] {body}");
                    }
                    Ok(ServerMessage::Telemetry(telemetry)) => {
                        let sync = seele_proto::sync_ratio::raw(seele_proto::SyncInputs {
                            rtt_ms: telemetry.rtt_ms,
                            jitter_ms: telemetry.jitter_ms,
                            loss_fraction: telemetry.loss_fraction,
                        });
                        if received % 20 == 0 {
                            println!(
                                "  · SYNC {sync:.0}% · RTT {:.1}ms · perda {:.2}%",
                                telemetry.rtt_ms,
                                telemetry.loss_fraction * 100.0
                            );
                        }
                        received += 1;
                    }
                    Ok(ServerMessage::PilotJoined { profile, .. }) => {
                        println!("  · {} inseriu o plug", profile.nickname);
                    }
                    Ok(ServerMessage::Alert { reason, .. }) => {
                        println!("  · ALERTA · {reason:?}");
                    }
                    Ok(_) => {}
                    Err(error) => {
                        println!("stream de controle encerrado: {error}");
                        return Ok(());
                    }
                }
            }
            datagram = media.next() => {
                match datagram {
                    Ok(bytes) => {
                        received += 1;
                        if let Ok((header, payload)) = MediaHeader::decode(&bytes) {
                            if received % 50 == 1 {
                                println!(
                                    "datagrama de ssrc {} seq {} ts {} ({} bytes de opus)",
                                    header.ssrc, header.seq, header.timestamp, payload.len()
                                );
                            }
                        }
                    }
                    Err(error) => {
                        println!("conexão encerrada: {error}");
                        return Ok(());
                    }
                }
            }
            () = tokio::time::sleep(Duration::from_secs(5)) => {
                // specs/02-protocolo.md: a Ping every 5 s. The Pong arrives
                // through the event stream, which seele-core uses to measure the
                // round trip — one reader on the control stream.
                if let Err(error) = client.send_ping().await {
                    println!("ping falhou: {error}");
                    return Ok(());
                }
                let _ = client.rtt();
            }
        }
    }
}

/// The whole product path, for the first time.
#[allow(clippy::too_many_lines, reason = "throwaway harness; clarity over structure")]
async fn full_pipeline(client: Client, args: &Args) -> Result<()> {
    let io: AudioIo = device::open_default(RING_MS)?;
    println!(
        "áudio: captura {} Hz, reprodução {} Hz",
        io.capture_rate_hz, io.playback_rate_hz
    );

    let mut encoder = {
        let mut config = EncoderConfig::new(SAMPLE_RATE_HZ, 1);
        config.application = Some(Application::Voip);
        config.frame_duration = Some(FrameDuration::Ms20);
        config.bitrate = Some(BITRATE_BPS);
        config.inband_fec = Some(InbandFec::Disabled); // ADR 0010
        config.dtx = Some(true);
        Encoder::new(config).map_err(|error| anyhow::anyhow!("{error}"))?
    };
    let mut to_pipeline = RateConverter::new(io.capture_rate_hz, SAMPLE_RATE_HZ)?;
    let mut to_device = RateConverter::new(SAMPLE_RATE_HZ, io.playback_rate_hz)?;
    let mut gate = VoiceGate::new(GateConfig::default(), GateMode::VoiceActivated);
    let mut mixer = Mixer::new();
    if args.deafen {
        mixer.set_master(0.0);
        println!("ISOLAMENTO TOTAL: nada chega ao alto-falante");
    }

    // One jitter buffer, drift tracker and decoder per talker.
    let mut sources: Vec<(u32, JitterBuffer<Vec<u8>>, DriftTracker, Decoder)> = Vec::new();

    let mut io = io;
    let (mut captured, mut at_48k, mut pending) = (Vec::new(), Vec::new(), Vec::<f32>::new());
    let mut frame_i16 = vec![0_i16; FRAME_SAMPLES];
    let mut datagram = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let mut mixed = vec![0.0_f32; FRAME_SAMPLES];
    let mut for_device = Vec::new();

    let (mut seq, mut timestamp, mut sent) = (0_u16, 0_u32, 0_u64);
    let started = Instant::now();
    let mut tone_phase = 0.0_f32;
    let mut next_playout = Instant::now();
    let mut next_report = Instant::now() + REPORT_EVERY;
    let ssrc = client.session().ssrc;

    println!("\nrodando. ctrl-c para sair.\n");

    loop {
        // ---- receive ----
        while let Ok(Ok(bytes)) =
            tokio::time::timeout(Duration::from_millis(1), client.next_media()).await
        {
            let arrival_ms = started.elapsed().as_secs_f64() * 1000.0;
            let Ok((header, payload)) = MediaHeader::decode(&bytes) else {
                continue;
            };
            let index = match sources.iter().position(|(s, ..)| *s == header.ssrc) {
                Some(index) => index,
                None => {
                    let Ok(decoder) = Decoder::new(DecoderConfig::new(SAMPLE_RATE_HZ, 1)) else {
                        continue;
                    };
                    println!("novo falante: ssrc {}", header.ssrc);
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
                buffer.push(header.seq, header.timestamp, arrival_ms, payload.to_vec());
            }
        }

        // ---- capture -> encode -> send ----
        captured.clear();
        while let Ok(sample) = io.captured.pop() {
            captured.push(if args.tone {
                let value = (tone_phase * 440.0 * std::f32::consts::TAU).sin() * 0.3;
                tone_phase += 1.0 / SAMPLE_RATE_HZ as f32;
                value
            } else {
                sample
            });
        }
        at_48k.clear();
        to_pipeline.push(&captured, &mut at_48k)?;
        pending.extend_from_slice(&at_48k);

        while pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();
            let speaking = gate.update(&frame);
            // The timestamp counts elapsed samples whether or not anything is
            // sent; the sequence counts only what goes on the wire. That is what
            // lets the receiver tell silence from loss — M1.9.
            timestamp = timestamp.wrapping_add(FRAME_SAMPLES as u32);
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
                version: seele_proto::PROTOCOL_VERSION,
                // The server refuses anything but the ssrc it assigned — G2.
                ssrc: ssrc.get(),
                seq,
                timestamp,
            };
            if let Ok(len) = header.encode_datagram(&payload, &mut datagram) {
                if let Some(bytes) = datagram.get(..len) {
                    let _ = client.send_media(bytes.to_vec());
                    sent += 1;
                }
            }
        }

        // ---- playout ----
        if Instant::now() >= next_playout {
            next_playout += Duration::from_millis(u64::from(FRAME_MS));
            let mut decoded: Vec<(u32, Vec<f32>)> = Vec::new();
            for (source, buffer, _, decoder) in &mut sources {
                let samples = match buffer.tick() {
                    Decision::Play(payload) => decoder.decode_f32(&payload).ok(),
                    Decision::Conceal => decoder.decode_plc_f32().ok(),
                    Decision::Silence | Decision::Comfort | Decision::Starved => None,
                };
                if let Some(samples) = samples {
                    decoded.push((*source, samples));
                }
            }
            let borrowed: Vec<(u32, &[f32])> = decoded
                .iter()
                .map(|(source, samples)| (*source, samples.as_slice()))
                .collect();
            mixer.mix(&borrowed, &mut mixed);

            for_device.clear();
            to_device.push(&mixed, &mut for_device)?;
            for sample in for_device.drain(..) {
                let _ = io.to_device.push(sample);
            }
        }

        // ---- telemetry ----
        if Instant::now() >= next_report {
            next_report += REPORT_EVERY;
            let stream = io.counters.snapshot();
            println!(
                "enviados {sent:>6} | nível {:.3} | saída {:.3} | over {} under {}",
                gate.metrics().input_rms,
                mixer.metrics().peak_output,
                stream.capture_overruns,
                stream.playback_underruns,
            );
            for (source, buffer, tracker, _) in &sources {
                let metrics = buffer.metrics();
                println!(
                    "  ssrc {source:>5} | tocados {:>6} | prof {:>5.1}ms | alvo {:>5.1}ms | \
                     perda {:>5.2}% | silêncio {:>5} | deriva {:>6.1}ppm",
                    metrics.frames_played,
                    metrics.depth_ms,
                    metrics.target_ms,
                    metrics.loss_fraction() * 100.0,
                    metrics.frames_comfort,
                    tracker.drift_ppm(),
                );
            }
        }

        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
