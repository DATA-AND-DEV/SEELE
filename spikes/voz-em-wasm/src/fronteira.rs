//! Um quadro de voz de 20 ms, como o desktop o faria: o portão sobre a
//! captura, o buffer de jitter de cada fonte e o mixer das fontes. A página
//! (e o `AudioWorklet`) chama `preparar` uma vez e `quadro` a cada 20 ms.

use std::cell::RefCell;

use crate::gate::{GateConfig, GateMode, VoiceGate};
use crate::jitter::{Decision, JitterBuffer, JitterConfig};
use crate::mixer::Mixer;
use crate::FRAME_SAMPLES;

struct Estado {
    portao: VoiceGate,
    fontes: Vec<JitterBuffer<u32>>,
    mixer: Mixer,
    captura: Vec<f32>,
    pcm: Vec<Vec<f32>>,
    saida: Vec<f32>,
    para_mandar: Vec<Vec<f32>>,
    seq: u16,
}

thread_local! {
    static ESTADO: RefCell<Option<Estado>> = const { RefCell::new(None) };
}

#[unsafe(no_mangle)]
pub extern "C" fn preparar(fontes: u32) {
    let n = fontes.max(1) as usize;
    let mut mixer = Mixer::new();
    for i in 0..n {
        mixer.set_gain(i as u32 + 1, 0.8);
    }
    let estado = Estado {
        portao: VoiceGate::new(GateConfig::automatico(-60.0), GateMode::VoiceActivated),
        fontes: (0..n).map(|_| JitterBuffer::new(JitterConfig::default())).collect(),
        mixer,
        captura: (0..FRAME_SAMPLES).map(|i| (i as f32 * 0.05).sin() * 0.1).collect(),
        pcm: (0..n).map(|k| (0..FRAME_SAMPLES).map(|i| ((i + k) as f32 * 0.03).sin() * 0.2).collect()).collect(),
        saida: vec![0.0; FRAME_SAMPLES],
        para_mandar: Vec::new(),
        seq: 0,
    };
    ESTADO.with(|e| *e.borrow_mut() = Some(estado));
}

/// Um quadro de 20 ms: devolve um número que depende da mistura, para nada ser
/// tirado pelo otimizador.
#[unsafe(no_mangle)]
pub extern "C" fn quadro(chegada_ms: f64) -> f32 {
    ESTADO.with(|e| {
        let mut e = e.borrow_mut();
        let Some(e) = e.as_mut() else { return f32::NAN };
        e.para_mandar.clear();
        let Estado { portao, captura, para_mandar, .. } = e;
        portao.quadros_a_transmitir(captura, para_mandar);
        let seq = e.seq;
        e.seq = e.seq.wrapping_add(1);
        let carimbo = u32::from(seq).wrapping_mul(FRAME_SAMPLES as u32);
        let mut tocando = Vec::with_capacity(e.fontes.len());
        for (i, fonte) in e.fontes.iter_mut().enumerate() {
            fonte.push(seq, carimbo, chegada_ms, i as u32);
            if let Decision::Play(qual) = fonte.tick() {
                tocando.push(qual as usize);
            }
        }
        let fontes: Vec<(u32, &[f32])> = tocando
            .iter()
            .map(|&i| (i as u32 + 1, e.pcm[i].as_slice()))
            .collect();
        e.mixer.mix(&fontes, &mut e.saida);
        e.saida[FRAME_SAMPLES / 2] + e.para_mandar.len() as f32
    })
}
