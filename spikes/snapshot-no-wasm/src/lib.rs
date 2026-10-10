//! O `Room` do `seele-core` no navegador, e o custo de um retrato.
//!
//! Spike da Tarefa 2.2 da F4 (ADR 0057, «quanto do núcleo vai para `wasm`»).
//! Ver o `README.md`.

#[path = "../../../crates/seele-core/src/assento.rs"]
#[allow(dead_code, unused_imports, rustdoc::broken_intra_doc_links)]
pub mod assento;

/// O `SessionInfo` do `client.rs`, recortado pelo `build.rs`.
pub mod client {
    use seele_proto::ids::{PersonId, SessionId, Ssrc};
    include!(concat!(env!("OUT_DIR"), "/sessao.rs"));
}

/// O `state.rs` do `seele-core`, sem o `teto_de_video` e sem os testes.
#[allow(dead_code, unused_imports, rustdoc::broken_intra_doc_links, clippy::all)]
pub mod state {
    include!(concat!(env!("OUT_DIR"), "/state.rs"));
}

pub mod retrato;
pub mod sala;

#[cfg(target_arch = "wasm32")]
mod fronteira;
