//! Mede o que o anel de reprodução perde nesta máquina, e se o ritmo o segura.
//!
//! Rodar com: `cargo run --release -p seele-audio --example ritmo`

use std::time::{Duration, Instant};

use seele_audio::device;
use seele_audio::playout::PlayoutClock;
use seele_audio::resample::RateConverter;
use seele_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};

const RING_MS: u32 = 100;
const DURACAO: Duration = Duration::from_secs(60);
const RELATO: Duration = Duration::from_secs(10);

fn main() {
    let Ok(mut io) = device::open_default(RING_MS) else {
        eprintln!("sem dispositivo");
        return;
    };
    let Ok(mut to_device) = RateConverter::new(SAMPLE_RATE_HZ, io.playback_rate_hz) else {
        return;
    };
    println!("reprodução: {} Hz", io.playback_rate_hz);

    let inicio = Instant::now();
    let mut relogio = PlayoutClock::new(inicio, FRAME_MS);
    let mut proximo_relato = inicio + RELATO;
    let mixed = vec![0.0_f32; FRAME_SAMPLES];
    let mut for_device = Vec::new();
    let mut anel_cheio = 0_u64;
    let capacidade = io.to_device.buffer().capacity();
    let mut fundo = capacidade;
    let mut topo = 0;
    let mut anterior = inicio;
    let mut voltas: Vec<f64> = Vec::with_capacity(64_000);

    while inicio.elapsed() < DURACAO {
        // A forma da volta do laço de voz: duas esperas de temporizador.
        std::thread::sleep(Duration::from_millis(1));

        let agora = Instant::now();
        voltas.push((agora - anterior).as_secs_f64() * 1000.0);
        anterior = agora;

        let vencidos = relogio.due(agora);
        let profundidade = capacidade - io.to_device.slots();
        if vencidos > 0 {
            fundo = fundo.min(profundidade);
            topo = topo.max(profundidade);
        }
        for _ in 0..vencidos {
            for_device.clear();
            if to_device.push(&mixed, &mut for_device).is_ok() {
                for sample in for_device.drain(..) {
                    if io.to_device.push(sample).is_err() {
                        anel_cheio += 1;
                    }
                }
            }
        }

        if Instant::now() >= proximo_relato {
            proximo_relato += RELATO;
            let m = io.counters.snapshot();
            println!(
                "{:>3.0}s  falta {:>7}  anel cheio {:>7}  fundo {:>5}  topo {:>5}  agora {:>5}",
                inicio.elapsed().as_secs_f64(),
                m.playback_underruns,
                anel_cheio,
                fundo,
                topo,
                profundidade,
            );
            fundo = capacidade;
            topo = 0;
        }

        std::thread::sleep(Duration::from_millis(2));
    }

    let decorrido = inicio.elapsed().as_secs_f64();
    let m = io.counters.snapshot();
    let pedidas = m.frames_played + m.playback_underruns;
    voltas.sort_by(f64::total_cmp);
    let meio = voltas.get(voltas.len() / 2).copied().unwrap_or(0.0);
    let pior = voltas.last().copied().unwrap_or(0.0);
    println!(
        "\nvolta p50 {meio:.2} ms  pior {pior:.2} ms\n\
         reproduzidas {}  falta {}\n\
         o dispositivo pediu {pedidas} quadros em {decorrido:.3} s = {:.2} Hz ({:+.0} ppm)",
        m.frames_played,
        m.playback_underruns,
        pedidas as f64 / decorrido,
        (pedidas as f64 / decorrido / f64::from(io.playback_rate_hz) - 1.0) * 1e6,
    );
}
