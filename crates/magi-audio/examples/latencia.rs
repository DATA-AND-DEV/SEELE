//! Measures round-trip audio latency on this machine. Task M1.2.
//!
//! ```text
//! cargo run --release --example latencia -p magi-audio
//! cargo run --release --example latencia -p magi-audio -- --repeticoes 40
//! ```
//!
//! # Two ways to run it
//!
//! **By cable** — a jack cable from the headphone output to the microphone
//! input. Measures the machine: output buffer, DAC, ADC, input buffer, and the
//! resampling in [`magi_audio::resample`]. Repeatable to a sample, and the
//! number to optimise.
//!
//! **Acoustically** — speaker and microphone, nothing connected. Measures what
//! a person experiences, and includes about 3 ms per metre of air. This is the
//! number ADR 0009's budget is about, because that budget is mouth-to-ear.
//!
//! Turn the volume up enough to be clearly audible and keep the room quiet. The
//! tool refuses to report a figure it cannot stand behind rather than printing
//! a plausible one — see `MIN_CONFIDENCE`.
//!
//! # What it does not measure
//!
//! Anything past the sound card. No codec, no network, no jitter buffer. Add
//! ADR 0009's budget for those. Keeping this measurement narrow is the point:
//! when the end-to-end figure moves, this says whether the machine moved.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool"
)]

use std::time::{Duration, Instant};

use magi_audio::latency::{click, find_delay, Report};
use magi_audio::{device, SAMPLE_RATE_HZ};

/// 5 ms of click. Long enough to carry energy, short enough to stay sharp.
const CLICK_SAMPLES: usize = 240;

/// How long to listen after each click before giving up on it.
const LISTEN: Duration = Duration::from_millis(400);

/// Silence between attempts, so one click's echo is not the next one's answer.
const GAP: Duration = Duration::from_millis(300);

fn main() {
    let repetitions = parse_repetitions();

    println!("MAGI · medição de latência de ida e volta");
    println!();
    println!("  Ligue um cabo da saída de fone para a entrada de microfone,");
    println!("  ou deixe no ar com o volume audível e a sala em silêncio.");
    println!("  {repetitions} tentativas.");
    println!();

    let mut io = match device::open_default(200) {
        Ok(io) => io,
        Err(error) => {
            eprintln!("não consegui abrir os dispositivos: {error}");
            std::process::exit(1);
        }
    };
    println!(
        "  captura {} Hz · reprodução {} Hz",
        io.capture_rate_hz, io.playback_rate_hz
    );
    println!();

    // The click is generated at the device's own playback rate and looked for
    // at the device's own capture rate. Resampling either one would put this
    // tool's own conversion into the number it is measuring.
    let outgoing = click(CLICK_SAMPLES);
    let reference = click(CLICK_SAMPLES);

    let mut measurements = Vec::new();
    let mut failed = 0_usize;

    for attempt in 1..=repetitions {
        // Drain whatever is sitting in the capture ring, or the previous
        // attempt's tail becomes this attempt's answer.
        while io.captured.pop().is_ok() {}

        for sample in &outgoing {
            let _ = io.to_device.push(*sample);
        }

        let mut recorded = Vec::with_capacity(io.capture_rate_hz as usize / 2);
        let deadline = Instant::now() + LISTEN;
        while Instant::now() < deadline {
            while let Ok(sample) = io.captured.pop() {
                recorded.push(sample);
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        match find_delay(&reference, &recorded, io.capture_rate_hz) {
            Some(delay) => {
                println!(
                    "  {attempt:>3}  {:>7.2} ms   confiança {:>5.1}",
                    delay.millis, delay.confidence
                );
                measurements.push(delay.millis);
            }
            None => {
                println!("  {attempt:>3}      —        nada encontrado");
                failed += 1;
            }
        }

        std::thread::sleep(GAP);
    }

    report(&Report::new(measurements, failed), &io);
}

fn report(report: &Report, io: &device::AudioIo) {
    println!();
    let Some(median) = report.median_ms() else {
        eprintln!("nenhuma medição. Confira o cabo, o volume e o dispositivo de entrada.");
        std::process::exit(2);
    };

    println!("  mediana   {median:.2} ms");
    if let Some(spread) = report.spread_ms() {
        println!("  dispersão {spread:.2} ms");
    }
    println!(
        "  falhas    {}/{}",
        report.failed,
        report.failed + report.measurements.len()
    );

    let counters = io.counters.snapshot();
    if counters.capture_overruns > 0 || counters.playback_underruns > 0 {
        println!();
        println!(
            "  atenção: {} overruns de captura, {} underruns de reprodução.",
            counters.capture_overruns, counters.playback_underruns
        );
        println!("  A máquina não acompanhou, e a medição carrega isso dentro.");
    }

    if !report.trustworthy() {
        println!();
        eprintln!("  Poucas medições boas. Não anote este número.");
        std::process::exit(2);
    }

    println!();
    println!("  Anote em docs/m1-medicoes.md, dizendo qual dos dois rigs foi:");
    println!("  por cabo mede a máquina, no ar mede a experiência.");
}

fn parse_repetitions() -> usize {
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        if flag == "--repeticoes" || flag == "-n" {
            if let Some(value) = argv.next().and_then(|value| value.parse().ok()) {
                return value;
            }
            eprintln!("--repeticoes precisa de um número");
            std::process::exit(2);
        }
    }
    20
}

// The pipeline's rate, referenced so a future change to it fails here rather
// than silently making the tool measure something else.
const _: () = assert!(SAMPLE_RATE_HZ == 48_000);
