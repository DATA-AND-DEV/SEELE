//! A sessão como o servidor a conta: o `Room`, que dobra as mensagens do
//! servidor, a porta do assento (`assento`) e o que chega na entrada
//! (`SessionInfo`).
//!
//! # Por que um crate à parte
//!
//! Para o navegador montar o mesmo retrato que o desktop (ADR 0057, Tarefa 2.2
//! da F4). O `Room` sempre foi puro (nenhum relógio, rede, disco, áudio ou
//! vídeo), mas morava no `seele-core`, que puxa o quinn, o tokio, o Opus e o
//! OpenH264 e não compila para o `wasm32-unknown-unknown`. Aqui ele depende só
//! do protocolo, e o `cargo xtask web` o compila para o navegador a cada vez.
//!
//! O `seele-core` reexporta os três (`seele_core::state`, `seele_core::assento`
//! e `seele_core::client::SessionInfo`): quem os usava não mudou. E o
//! `retrato` (a fase 2): os tipos de vista e a projeção do `Snapshot`, que o
//! desktop e o navegador montam com as mesmas funções. O que no
//! `Room` dependia do vídeo, o `teto_de_video`, ficou no `seele-core`, ao lado
//! do teto (`seele_core::tela::TetoDaSala`).
//!
//! A regra de quais despedidas acabam a sessão (`despedida`) mora aqui pelo
//! mesmo motivo, e o `seele-core` a reexporta também: o `Room` e o motor do
//! enlace a chamam, e uma regra só não tem como discordar de si mesma
//! (pendência 61, lote P61 da F5).

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

use seele_proto::ids::{PersonId, SessionId, Ssrc};

pub mod assento;
pub mod battery;
pub mod catalogo;
pub mod despedida;
pub mod mods;
pub mod nome_do_anexo;
pub mod preview;
pub mod retrato;
pub mod search;
pub mod state;
pub mod volta;

/// Everything the server told us on the way in.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: SessionId,
    /// Which person this connection is.
    pub person: PersonId,
    /// The media source the server assigned. Gap G1.
    pub ssrc: Ssrc,
    /// What the server is called.
    pub server: String,
    /// Voice channels visible to us.
    pub voice_rooms: Vec<seele_proto::control::VoiceRoomInfo>,
    /// Text channels visible to us.
    ///
    /// Carried through rather than dropped: an interface that knows the voice_rooms
    /// but not the Channels can only ever open whichever Channel it was started with.
    pub channels: Vec<seele_proto::control::ChannelInfo>,
    /// What this person may do, as PERMISSIONS resolved it.
    ///
    /// So a shell can decide whether to offer a control at all. **Convenience,
    /// never enforcement** — `specs/08-seguranca.md` puts the security in the
    /// server refusing, and it refuses again whatever this list says.
    pub permissions: Vec<seele_proto::control::Permission>,
}
