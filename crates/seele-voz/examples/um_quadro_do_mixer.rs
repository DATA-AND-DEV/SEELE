//! O custo de um quadro do mixer com oito fontes, no nativo.
//!
//! A thread de áudio do desktop chama o `Mixer::mix` a cada 20 ms, e uma
//! mudança no mixer não pode deixá-la mais lenta sem que alguém veja. É a
//! medida que a troca do `HashMap` pelo `BTreeMap` mostrou antes e depois
//! (Tarefa 3.3 da F4).
//!
//!     cargo run --release -p seele-voz --example um_quadro_do_mixer

use std::hint::black_box;
use std::time::Instant;

use seele_voz::mixer::Mixer;
use seele_voz::FRAME_SAMPLES;

const FONTES: u32 = 8;
const RODADAS: usize = 7;
const QUADROS_POR_RODADA: u32 = 200_000;

fn main() {
    // SSRCs espalhados, como o servidor os dá, e não 1, 2, 3.
    let fontes: Vec<(u32, Vec<f32>)> = (0..FONTES)
        .map(|i| {
            let ssrc = 1_000 + i.wrapping_mul(7_919);
            let pcm = (0..FRAME_SAMPLES)
                .map(|n| ((n as f32 + i as f32) * 0.01).sin() * 0.2)
                .collect();
            (ssrc, pcm)
        })
        .collect();
    let mut mixer = Mixer::new();
    for (ssrc, _) in &fontes {
        mixer.set_gain(*ssrc, 0.9);
    }
    let entradas: Vec<(u32, &[f32])> = fontes.iter().map(|(s, pcm)| (*s, pcm.as_slice())).collect();
    let mut saida = vec![0.0_f32; FRAME_SAMPLES];

    for _ in 0..10_000 {
        mixer.mix(black_box(&entradas), black_box(&mut saida));
    }
    let mut ns: Vec<f64> = (0..RODADAS)
        .map(|_| {
            let inicio = Instant::now();
            for _ in 0..QUADROS_POR_RODADA {
                mixer.mix(black_box(&entradas), black_box(&mut saida));
            }
            inicio.elapsed().as_nanos() as f64 / f64::from(QUADROS_POR_RODADA)
        })
        .collect();
    ns.sort_by(f64::total_cmp);
    let (menor, mediana, maior) = (
        ns.first().copied().unwrap_or_default(),
        ns.get(RODADAS / 2).copied().unwrap_or_default(),
        ns.last().copied().unwrap_or_default(),
    );
    println!(
        "um quadro do mixer, {FONTES} fontes: mediana {mediana:.0} ns (de {menor:.0} a {maior:.0}), \
         em {RODADAS} rodadas de {QUADROS_POR_RODADA} quadros"
    );
}
