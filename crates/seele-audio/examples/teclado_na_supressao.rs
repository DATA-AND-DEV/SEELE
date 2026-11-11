//! Quanto do som de uma batida de teclado atravessa a supressão de ruído.
//!
//! Sinal sintético: um piso de sala a −55 dBFS e, a cada 180 ms, um estalo de
//! 6 ms de banda larga a −18 dBFS com queda exponencial — a forma de uma tecla
//! mecânica gravada de perto. Mede a energia dos estalos antes e depois.
//!
//! ```sh
//! cargo run --release -p seele-audio --example teclado_na_supressao
//! ```

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::similar_names,
    missing_docs
)]

use seele_audio::supressao::Supressao;

const QUADRO: usize = seele_audio::FRAME_SAMPLES;

/// Um gerador simples e repetível: o mesmo sinal a cada execução.
struct Sorteio(u64);
impl Sorteio {
    fn proximo(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}

fn db(x: f64) -> f64 {
    10.0 * x.max(1e-20).log10()
}

fn main() {
    let taxa = 48_000_usize;
    let segundos = 10;
    let total = taxa * segundos;
    let piso = 10f32.powf(-55.0 / 20.0);
    let estalo = 10f32.powf(-18.0 / 20.0);
    let periodo = taxa * 180 / 1000;
    let duracao = taxa * 6 / 1000;
    let mut sorteio = Sorteio(7);
    let mut sinal = vec![0f32; total];
    let mut e_estalo = vec![false; total];
    for (i, amostra) in sinal.iter_mut().enumerate() {
        *amostra = piso * sorteio.proximo();
        let fase = i % periodo;
        // Os dois primeiros segundos são só sala: o filtro aprende o piso.
        if i > 2 * taxa && fase < duracao * 4 {
            let queda = (-(fase as f32) / (duracao as f32 / 3.0)).exp();
            *amostra += estalo * queda * sorteio.proximo();
            e_estalo[i] = fase < duracao * 2;
        }
    }

    // Um sinal de «voz» para medir o que cada filtro preserva: harmônicos de
    // 140 Hz com envelope de sílaba, a −20 dBFS, sobre o mesmo piso de sala.
    let mut voz = vec![0f32; total];
    let amplitude = 10f32.powf(-20.0 / 20.0);
    for (i, amostra) in voz.iter_mut().enumerate() {
        let t = i as f32 / taxa as f32;
        let silaba = (0.5 - 0.5 * (std::f32::consts::TAU * 3.0 * t).cos()).powi(2);
        let mut soma = 0.0;
        for h in 1..=12 {
            soma += (std::f32::consts::TAU * 140.0 * h as f32 * t).sin() / h as f32;
        }
        *amostra = piso * sorteio.proximo()
            + if i > 2 * taxa {
                amplitude * 0.4 * silaba * soma
            } else {
                0.0
            };
    }

    let medir = |nome: &str, saida: &[f32], atraso: usize| {
        let (mut antes, mut depois, mut sala_antes, mut sala_depois) = (0f64, 0f64, 0f64, 0f64);
        let (mut n_estalo, mut n_sala) = (0usize, 0usize);
        for i in (3 * taxa)..(total - QUADRO * 2) {
            let fora = f64::from(saida.get(i + atraso).copied().unwrap_or(0.0));
            let dentro = f64::from(sinal[i]);
            if e_estalo[i] {
                antes += dentro * dentro;
                depois += fora * fora;
                n_estalo += 1;
            } else if i % periodo > duracao * 8 {
                sala_antes += dentro * dentro;
                sala_depois += fora * fora;
                n_sala += 1;
            }
        }
        println!(
            "{nome}: o estalo sai {:.1} dB abaixo do que entrou; a sala entre estalos, {:.1} dB abaixo",
            db(antes / n_estalo as f64) - db(depois / n_estalo as f64),
            db(sala_antes / n_sala as f64) - db(sala_depois / n_sala as f64),
        );
    };
    let energia = |x: &[f32]| {
        x[3 * taxa..total - QUADRO * 2]
            .iter()
            .map(|a| f64::from(*a) * f64::from(*a))
            .sum::<f64>()
    };

    let de_hoje = |entrada: &[f32], forca: f32| {
        let mut supressao = Supressao::nova(forca);
        let mut saida = Vec::with_capacity(total);
        let mut bloco = Vec::new();
        for quadro in entrada.chunks_exact(QUADRO) {
            bloco.clear();
            supressao.processar(quadro, &mut bloco);
            saida.extend_from_slice(&bloco);
        }
        saida
    };
    // O RNNoise trabalha em blocos de 480 amostras, na escala de 16 bits.
    let rnnoise = |entrada: &[f32]| {
        let mut estado = nnnoiseless::DenoiseState::new();
        let mut saida = Vec::with_capacity(total);
        let mut dentro = [0f32; nnnoiseless::DenoiseState::FRAME_SIZE];
        let mut fora = [0f32; nnnoiseless::DenoiseState::FRAME_SIZE];
        for bloco in entrada.chunks_exact(nnnoiseless::DenoiseState::FRAME_SIZE) {
            for (d, a) in dentro.iter_mut().zip(bloco) {
                *d = a * 32768.0;
            }
            estado.process_frame(&mut fora, &dentro);
            saida.extend(fora.iter().map(|a| a / 32768.0));
        }
        saida
    };

    println!("-- só teclado sobre a sala");
    medir(
        "hoje, força 1,0",
        &de_hoje(&sinal, 1.0),
        Supressao::atraso_em_amostras(),
    );
    let inicio = std::time::Instant::now();
    let pelo_rnnoise = rnnoise(&sinal);
    let gasto = inicio.elapsed();
    medir(
        "RNNoise",
        &pelo_rnnoise,
        nnnoiseless::DenoiseState::FRAME_SIZE,
    );
    // Os dois em série: o RNNoise tira o que varia, a subtração espectral, o que fica.
    medir(
        "RNNoise e depois o de hoje",
        &de_hoje(&pelo_rnnoise, 1.0),
        nnnoiseless::DenoiseState::FRAME_SIZE + Supressao::atraso_em_amostras(),
    );
    println!(
        "RNNoise: {:.2}% de um núcleo ({} s de áudio em {:.0} ms)",
        100.0 * gasto.as_secs_f64() / segundos as f64,
        segundos,
        gasto.as_secs_f64() * 1000.0
    );
    // O atraso do RNNoise: o deslocamento em que a saída mais se parece com a entrada.
    let limpa = rnnoise(&voz);
    let (mut melhor, mut onde) = (f64::MIN, 0usize);
    for atraso in 0..1500 {
        let soma: f64 = (3 * taxa..5 * taxa)
            .map(|i| f64::from(voz[i]) * f64::from(limpa[i + atraso]))
            .sum();
        if soma > melhor {
            melhor = soma;
            onde = atraso;
        }
    }
    println!(
        "RNNoise: atraso de {onde} amostras ({:.1} ms)",
        onde as f64 / 48.0
    );
    // O RNNoise também diz, a cada bloco, a chance de haver voz. É o que um
    // portão de voz usaria para não abrir com uma batida de teclado.
    let chance_de_voz = |entrada: &[f32]| {
        let mut estado = nnnoiseless::DenoiseState::new();
        let mut dentro = [0f32; nnnoiseless::DenoiseState::FRAME_SIZE];
        let mut fora = [0f32; nnnoiseless::DenoiseState::FRAME_SIZE];
        let mut chances = Vec::new();
        for (n, bloco) in entrada
            .chunks_exact(nnnoiseless::DenoiseState::FRAME_SIZE)
            .enumerate()
        {
            for (d, a) in dentro.iter_mut().zip(bloco) {
                *d = a * 32768.0;
            }
            let chance = estado.process_frame(&mut fora, &dentro);
            if n >= 300 {
                chances.push(chance);
            }
        }
        chances.sort_by(f32::total_cmp);
        (
            chances[chances.len() / 2],
            chances[chances.len() * 95 / 100],
            chances[chances.len() - 1],
        )
    };
    let (mediana, p95, maxima) = chance_de_voz(&sinal);
    println!("chance de voz, só teclado: mediana {mediana:.2}, p95 {p95:.2}, máxima {maxima:.2}");
    let (mediana, p95, maxima) = chance_de_voz(&voz);
    println!("chance de voz, só voz: mediana {mediana:.2}, p95 {p95:.2}, máxima {maxima:.2}");
    println!("-- só voz sobre a sala (quanto da energia sobra)");
    let e = energia(&voz);
    println!(
        "hoje, força 1,0: sobra {:.0}%",
        100.0 * energia(&de_hoje(&voz, 1.0)) / e
    );
    println!("RNNoise: sobra {:.0}%", 100.0 * energia(&rnnoise(&voz)) / e);
    println!(
        "RNNoise e depois o de hoje: sobra {:.0}%",
        100.0 * energia(&de_hoje(&rnnoise(&voz), 1.0)) / e
    );
}
