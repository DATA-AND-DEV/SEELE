//! Mede o que o anel de reprodução perde nesta máquina, e se a malha o segura.
//!
//! # A pergunta que isto responde
//!
//! O laço de voz produz 48 000 amostras por segundo **de `Instant`**. O
//! dispositivo consome no ritmo do cristal dele. Os dois não são o mesmo
//! relógio, e a diferença se acumula no único lugar onde cabe: o anel entre os
//! dois. Ele encosta no fundo — e a reprodução inventa silêncio para sempre — ou
//! no topo — e a latência de saída vira o anel inteiro, para sempre. É a
//! pendência 2, e o sintoma é o aviso "ÁUDIO LOCAL FALHANDO" acendendo sozinho
//! com o áudio audivelmente bom.
//!
//! O `device_smoke` não a responde: lá o laço empurra tantas amostras quantas
//! capturou, então ele anda no ritmo do cristal de **entrada** e se sincroniza
//! com a saída por acidente. O `cadencia` do `seele-core` também não: ele mede a
//! volta do laço, sem dispositivo nenhum. Este aqui tem a forma do laço de voz —
//! quadros vencidos pelo relógio, empurrados por um `PlayoutClock` — contra o
//! dispositivo de verdade, que é a única combinação em que a deriva aparece.
//!
//! # Como ler o que sai
//!
//! - `falta` é amostra que o dispositivo pediu e não tinha. **O critério da
//!   pendência 2 é ela parar de crescer**, e a coluna por intervalo é onde se
//!   olha isso.
//! - `anel cheio` é a outra parede: amostra que a mistura produziu e o anel
//!   recusou.
//! - `fundo` é a menor profundidade vista no intervalo. Abaixo de um bloco do
//!   dispositivo, a próxima chamada dele já inventa silêncio.
//! - `ppm` é a deriva medida, que é o que a malha está cancelando.
//! - `desvio do relógio` no fim é a mesma deriva medida por outro caminho — o
//!   dispositivo pediu tantos quadros em tantos segundos de `Instant` —, e serve
//!   de conferência independente da malha.
//!
//! Rodar com: `cargo run --release -p seele-audio --example ritmo`
//! Sem a malha, para comparar: `... --example ritmo -- --sem-malha`
//! Por mais tempo: `... --example ritmo -- --segundos 600`

use std::time::{Duration, Instant};

use seele_audio::device::{self, DeviceError};
use seele_audio::pacing::RingPacer;
use seele_audio::playout::PlayoutClock;
use seele_audio::resample::RateConverter;
use seele_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};

/// O mesmo anel do `seele-core`.
const RING_MS: u32 = 100;

/// Quanto tempo medir, por omissão.
///
/// Um minuto é o bastante para a malha aquecer e sobrar meia dúzia de intervalos
/// de regime. Dez minutos é o que `specs/09-roadmap.md` pede, e é o que a deriva
/// leva para virar um jitter buffer inteiro.
const SEGUNDOS: u64 = 60;

/// De quanto em quanto tempo relatar.
const RELATO: Duration = Duration::from_secs(10);

fn main() {
    let mut segundos = SEGUNDOS;
    let mut com_malha = true;
    let mut argumentos = std::env::args().skip(1);
    while let Some(argumento) = argumentos.next() {
        match argumento.as_str() {
            "--sem-malha" => com_malha = false,
            "--segundos" => {
                if let Some(quantos) = argumentos.next().and_then(|texto| texto.parse().ok()) {
                    segundos = quantos;
                }
            }
            outro => {
                eprintln!("argumento desconhecido: {outro}");
                std::process::exit(2);
            }
        }
    }

    match medir(segundos, com_malha) {
        Ok(()) => {}
        Err(DeviceError::NoOutputDevice) => {
            eprintln!("esta máquina não oferece saída de áudio.");
            std::process::exit(1);
        }
        Err(erro) => {
            eprintln!("não deu para abrir o áudio: {erro}");
            std::process::exit(1);
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "um laço de medição; separá-lo esconderia a ordem, que é o que se está medindo"
)]
fn medir(segundos: u64, com_malha: bool) -> Result<(), DeviceError> {
    let mut io = device::open_default(RING_MS)?;
    let mut to_device = match RateConverter::new_adjustable(SAMPLE_RATE_HZ, io.playback_rate_hz) {
        Ok(conversor) => conversor,
        Err(erro) => {
            eprintln!("não deu para construir o conversor de saída: {erro}");
            std::process::exit(1);
        }
    };

    let anel = io.to_device.buffer().capacity();
    let mut ritmo = RingPacer::new(io.playback_rate_hz, anel);
    println!(
        "reprodução {} Hz · anel {anel} amostras ({:.0} ms) · malha {}",
        io.playback_rate_hz,
        f64::from(RING_MS),
        if com_malha { "ligada" } else { "DESLIGADA" }
    );
    println!(
        "empurrando silêncio no compasso do relógio por {segundos} s, \
         que é a forma do laço de voz\n"
    );
    println!(
        "{:>5} {:>9} {:>11} {:>7} {:>7} {:>9} {:>7}",
        "t", "falta", "anel cheio", "fundo", "alvo", "ppm", "grampo"
    );

    let inicio = Instant::now();
    let mut relogio = PlayoutClock::new(inicio, FRAME_MS);
    let mut proximo_relato = inicio + RELATO;
    let mixed = vec![0.0_f32; FRAME_SAMPLES];
    let mut para_o_anel = Vec::new();
    let mut anel_cheio = 0_u64;
    let mut fundo = anel;
    let (mut falta_anterior, mut cheio_anterior) = (0_u64, 0_u64);

    while inicio.elapsed() < Duration::from_secs(segundos) {
        // A forma da volta do laço de voz: duas esperas de temporizador por
        // volta, que é o que a pendência 15 mediu em 5,65 ms de p50 aqui.
        std::thread::sleep(Duration::from_millis(1));

        let vencidos = relogio.due(Instant::now());
        if vencidos > 0 {
            let profundidade = anel.saturating_sub(io.to_device.slots());
            fundo = fundo.min(profundidade);

            if com_malha {
                let compasso = ritmo.observe(
                    profundidade,
                    io.counters.playback_burst_frames(),
                    Instant::now(),
                );
                for _ in 0..compasso.prime_samples {
                    if io.to_device.push(0.0).is_err() {
                        anel_cheio += 1;
                    }
                }
                if let Some(razao) = compasso.ratio {
                    if let Err(erro) = to_device.adjust_ratio(razao) {
                        eprintln!("o reamostrador recusou a razão {razao}: {erro}");
                    }
                }
            }

            for _ in 0..vencidos {
                para_o_anel.clear();
                if let Err(erro) = to_device.push(&mixed, &mut para_o_anel) {
                    eprintln!("conversão de saída falhou: {erro}");
                    break;
                }
                for amostra in para_o_anel.drain(..) {
                    if io.to_device.push(amostra).is_err() {
                        anel_cheio += 1;
                    }
                }
            }
        }

        if Instant::now() >= proximo_relato {
            proximo_relato += RELATO;
            let medido = io.counters.snapshot();
            let compasso = ritmo.metrics();
            println!(
                "{:>4.0}s {:>9} {:>11} {:>7} {:>6.1}ms {:>+8.0} {:>7}",
                inicio.elapsed().as_secs_f64(),
                medido.playback_underruns - falta_anterior,
                anel_cheio - cheio_anterior,
                fundo,
                compasso.target_ms,
                compasso.ppm,
                compasso.clamps,
            );
            falta_anterior = medido.playback_underruns;
            cheio_anterior = anel_cheio;
            fundo = anel;
        }

        std::thread::sleep(Duration::from_millis(2));
    }

    let decorrido = inicio.elapsed().as_secs_f64();
    let medido = io.counters.snapshot();
    let compasso = ritmo.metrics();
    let pedidas = medido.frames_played + medido.playback_underruns;
    #[allow(
        clippy::cast_precision_loss,
        reason = "uma contagem de quadros de áudio, muito abaixo do limite de f64"
    )]
    let pedidas_por_segundo = pedidas as f64 / decorrido;
    let desvio_ppm =
        (pedidas_por_segundo / f64::from(io.playback_rate_hz) - 1.0) * 1e6;

    println!(
        "\nbloco do dispositivo {} quadros · alvo da reserva {:.1} ms · reposição {}",
        medido.playback_burst_frames, compasso.target_ms, compasso.primes
    );
    println!(
        "faltaram {} amostras no total, o anel recusou {anel_cheio}",
        medido.playback_underruns
    );
    println!(
        "desvio do relógio: o dispositivo pediu {pedidas} quadros em {decorrido:.3} s \
         = {pedidas_por_segundo:.1} Hz ({desvio_ppm:+.0} ppm)"
    );
    // A conferência independente: a deriva medida pela malha e a deriva medida
    // pela contagem de quadros são dois caminhos para o mesmo número.
    if com_malha {
        println!("a malha estava pedindo {:+.0} ppm", compasso.ppm);
    }
    Ok(())
}
