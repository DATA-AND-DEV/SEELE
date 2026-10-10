//! O `Room` do `seele-core` no navegador, e o custo de um retrato.
//!
//! Spike da Tarefa 2.2 da F4 (ADR 0057, «quanto do núcleo vai para `wasm`»).
//! Ver o `README.md`.

/// O `Room` e o `SessionInfo` do `seele-sessao` (até `4e1139fc`, o texto do
/// `seele-core`, recortado pelo `build.rs`).
pub use seele_sessao::{state, SessionInfo};

/// O nome de antes do corte, para o `sala.rs` não mudar.
pub mod client {
    pub use seele_sessao::SessionInfo;
}

pub mod retrato;
pub mod sala;

#[cfg(target_arch = "wasm32")]
mod fronteira;
