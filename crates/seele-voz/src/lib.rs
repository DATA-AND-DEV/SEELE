//! O caminho da voz que não toca em nada: o portão, o buffer de jitter, o
//! mixer, o relógio de mídia e o simulador de rede dos testes, com as três
//! constantes do quadro.
//!
//! # Por que um crate à parte
//!
//! Para o navegador tocar a mesma voz que o desktop (ADR 0057, Tarefa 3.3 da
//! F4). Estas peças sempre foram puras, mas moravam no `seele-audio`, que puxa
//! o cpal e a libopus em C e não compila para o `wasm32-unknown-unknown`. Aqui
//! elas não dependem de nada, e o `cargo xtask web` as compila para o
//! navegador a cada vez; dentro de um `AudioWorklet`, um quadro de 20 ms com
//! oito fontes custa cerca de 9 µs (`spikes/voz-em-wasm/`).
//!
//! O `seele-audio` as reexporta (`seele_audio::gate`, `::jitter`, `::mixer`,
//! `::netsim` e as constantes): quem as usava não mudou.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod gate;
pub mod jitter;
pub mod mixer;
pub mod netsim;
pub mod relogio;

/// Sample rate used everywhere inside the pipeline, in hertz.
///
/// `specs/03-audio.md`. Device rates that differ from this are resampled at the
/// edges, in both directions (task M1.4).
pub const SAMPLE_RATE_HZ: u32 = 48_000;

/// Opus frame duration, in milliseconds (`specs/03-audio.md`).
pub const FRAME_MS: u32 = 20;

/// Samples per frame, per channel, at [`SAMPLE_RATE_HZ`].
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE_HZ as usize * FRAME_MS as usize) / 1000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_960_samples() {
        // specs/03-audio.md states 20 ms = 960 samples at 48 kHz.
        assert_eq!(FRAME_SAMPLES, 960);
    }
}
