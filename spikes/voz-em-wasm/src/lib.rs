//! O portão, o buffer de jitter e o mixer do desktop no navegador.
//!
//! Spike da Tarefa 3.3 da F4. Ver o `README.md`.

/// As três constantes do `seele-audio` (`lib.rs`) de que as três peças precisam.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
/// O quadro de voz.
pub const FRAME_MS: u32 = 20;
/// Amostras por quadro.
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE_HZ as usize * FRAME_MS as usize) / 1000;

#[path = "../../../crates/seele-audio/src/gate.rs"]
#[allow(dead_code, rustdoc::broken_intra_doc_links, clippy::all)]
pub mod gate;
#[path = "../../../crates/seele-audio/src/jitter.rs"]
#[allow(dead_code, rustdoc::broken_intra_doc_links, clippy::all)]
pub mod jitter;
#[path = "../../../crates/seele-audio/src/mixer.rs"]
#[allow(dead_code, rustdoc::broken_intra_doc_links, clippy::all)]
pub mod mixer;

#[cfg(target_arch = "wasm32")]
mod fronteira;
