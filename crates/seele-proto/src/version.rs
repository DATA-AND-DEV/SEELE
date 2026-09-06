//! Protocol version negotiation.
//!
//! `specs/02-protocolo.md`:
//!
//! > The first byte of every control frame is the protocol version. The server
//! > refuses a version higher than its own with `Incompatible`. A lower version
//! > is accepted while it falls inside the declared compatibility window (N−1).
//!
//! This is the first thing that touches a byte off the wire, before any
//! authentication, so it is written to be total: every possible input byte
//! produces an answer and none of them panics.

use thiserror::Error;

/// Version of the wire protocol implemented by this build.
///
/// Versioned independently of the product version (`specs/10-convencoes.md`).
///
/// **2 desde o ADR 0036**, que acrescentou [`crate::control::ServerMessage::UplinkLoss`].
/// O postcard indexa variante por posição e não é autodescritivo, então um
/// cliente v1 não sabe decodificar a variante nova — e uma variante desconhecida
/// não é ignorada, ela desloca a leitura do fluxo para sempre. A janela de
/// compatibilidade abaixo é o que garante que ele continue conectando e
/// simplesmente não a receba; quem decide não mandá-la é a sessão do servidor.
/// **3 desde 04/09/2026**, e a subida tem uma razão que não é uma variante nova.
///
/// `WatchScreen` e `UnwatchScreen` entraram na lista do cliente com este número
/// parado em 2. O postcard indexa variante por posição, então dois builds
/// diferentes passaram a dizer «2» e a falar vocabulários diferentes — e a
/// negociação abaixo, que existe exatamente para pegar isso, deixou os dois
/// entrarem e quebrarem depois.
///
/// O que a subida compra, medido no código e não suposto:
///
/// - **um cliente v3 não alcança mais um servidor v2**: o servidor faz
///   `negotiate(3)`, vê 3 acima do que ele fala, e recusa com `Incompatible`.
///   A pessoa lê «versão incompatível com este servidor» **antes** de entrar, em
///   vez de descobrir pela sessão morrendo em três segundos. Isso torna
///   desnecessário gatear os dois verbos no cliente: ele nunca fala com quem não
///   os entende;
/// - **um cliente v2 continua entrando num servidor v3**, porque 2 está na
///   janela. Quem tem de se conter é o servidor, e é o que `session.rs` já faz
///   com o `UplinkLoss`.
///
/// O que ela **custa**: a janela desliza e a v1 deixa de ser aceita. Combinado
/// com quem hospeda: não há ninguém na 0.9.x, que é onde a v1 vivia.
///
/// O que ela **não** conserta é o compartilhamento de tela entre versões — esse
/// era outro defeito, e está em [`crate::screen::SCREEN_HEADER_VERSION`]: o
/// cabeçalho da tela carregava este número e herdava esta janela, apesar de os
/// onze bytes dele nunca terem mudado.
///
/// **4 desde 06/09/2026**, com o caminho entre pares.
///
/// A v3 **já tinha saído** — release `v0.10.5-1`, commit `12a6401a6` — quando as
/// quatro variantes abaixo entraram, então elas não puderam pegar carona numa
/// versão ainda não publicada como as do ADR 0036 pegaram na v2. Foram
/// acrescentadas [`crate::control::ClientMessage::EmprestarSubida`],
/// [`crate::control::ClientMessage::ParFalhou`],
/// [`crate::control::ServerMessage::SirvaTelaPara`] e
/// [`crate::control::ServerMessage::AssistaTelaPor`] — o cliente que empresta a
/// própria subida para outro par assistir, e o servidor que orquestra o
/// encontro entre os dois.
///
/// Um cliente v3 continua entrando num servidor v4, porque 3 está na janela; só
/// nunca recebe as variantes novas, e é servido pelo servidor como sempre foi
/// — o mesmo comportamento que um cliente v2 já tinha diante da v3.
pub const PROTOCOL_VERSION: u8 = 4;

/// How many past versions a peer accepts, beyond the current one.
///
/// `specs/10-convencoes.md`: "protocol compatibility: a window of N−1".
pub const COMPATIBILITY_WINDOW: u8 = 1;

/// Lowest protocol version this build can still talk to.
#[must_use]
pub const fn oldest_supported_version() -> u8 {
    PROTOCOL_VERSION.saturating_sub(COMPATIBILITY_WINDOW)
}

/// Why a peer's protocol version was refused.
///
/// `specs/02-protocolo.md`: "Every error reason is enumerated. No free-form
/// string reaches the interface — the shell decides how to present each variant."
/// A handshake failure must say something specific, "never generic".
///
/// The `Display` text below is for `tracing` and for developers. **Shells must
/// match on the variant, never render this string** — see ADR 0012. The numbers
/// are carried in the variants precisely so a shell can build its own localised
/// message without the core formatting anything for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Incompatible {
    /// The peer speaks a newer protocol than this build understands.
    #[error("peer speaks protocol {peer}, this build implements {ours}")]
    PeerTooNew {
        /// Version the peer announced.
        peer: u8,
        /// Version this build implements.
        ours: u8,
    },

    /// The peer speaks a protocol older than the compatibility window allows.
    #[error("peer speaks protocol {peer}, oldest supported is {oldest_supported}")]
    PeerTooOld {
        /// Version the peer announced.
        peer: u8,
        /// Oldest version still inside the compatibility window.
        oldest_supported: u8,
    },
}

/// Decides which protocol version to speak with a peer that announced `peer`.
///
/// On success, returns the version both sides will use: the peer's, since it is
/// the lower of the two and inside the window.
///
/// # Errors
///
/// Returns [`Incompatible`] when the peer is newer than this build, or older
/// than the compatibility window reaches.
///
/// # Examples
///
/// ```
/// use seele_proto::version::{negotiate, PROTOCOL_VERSION};
///
/// assert_eq!(negotiate(PROTOCOL_VERSION), Ok(PROTOCOL_VERSION));
/// assert!(negotiate(PROTOCOL_VERSION.saturating_add(1)).is_err());
/// ```
pub fn negotiate(peer: u8) -> Result<u8, Incompatible> {
    if peer > PROTOCOL_VERSION {
        return Err(Incompatible::PeerTooNew {
            peer,
            ours: PROTOCOL_VERSION,
        });
    }
    if peer < oldest_supported_version() {
        return Err(Incompatible::PeerTooOld {
            peer,
            oldest_supported: oldest_supported_version(),
        });
    }
    Ok(peer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn compatibility_window_never_underflows() {
        assert!(oldest_supported_version() <= PROTOCOL_VERSION);
    }

    /// Um cliente da versão anterior continua entrando.
    ///
    /// O ADR 0036 sobe a versão para carregar `ServerMessage::UplinkLoss`, e a
    /// promessa que acompanha a subida é esta: ninguém que já instalou perde o
    /// servidor. Um cliente v1 conecta, não recebe o quadro novo, e roda no
    /// bitrate fixo — que é exatamente o comportamento que ele já tinha.
    ///
    /// Escrito como teste e não como comentário porque a janela é a única coisa
    /// entre «subimos a versão» e «quebramos todo mundo que não atualizou no
    /// mesmo minuto».
    #[test]
    fn a_versao_anterior_continua_dentro_da_janela() {
        // **Os números mudaram de novo em 06/09/2026, com o caminho entre
        // pares, e a janela deslizou junto.**
        //
        // Desta vez quem sai da janela é a v2, e a diferença para a subida
        // anterior é esta: a v3 **já tinha saído** — release `v0.10.5-1`, commit
        // `12a6401a6` — então existe gente rodando exatamente essa versão, e é
        // ela que a janela precisa continuar cobrindo. O que fica preso aqui é a
        // forma — aceita-se a de agora e a anterior, recusa-se a seguinte —, e
        // ela vale para qualquer número.
        assert_eq!(PROTOCOL_VERSION, 4);
        assert_eq!(oldest_supported_version(), 3);
        assert!(negotiate(3).is_ok(), "a versão anterior foi recusada");
        assert!(negotiate(4).is_ok());
        assert!(
            negotiate(2).is_err(),
            "a v2 continuou aceita depois de a janela deslizar"
        );
        assert!(negotiate(5).is_err(), "um cliente do futuro foi aceito");
    }

    #[test]
    fn a_versao_subiu_para_a_do_caminho_entre_pares() {
        // A v3 **já saiu** no release `v0.10.5-1` (commit `12a6401a6`), então as
        // quatro mensagens novas não pegam carona como as do ADR 0036 pegaram na
        // v2. Um cliente v3 conecta pela janela, nunca recebe as mensagens novas, e
        // é servido pelo servidor — que é o comportamento de antes desta onda.
        assert_eq!(PROTOCOL_VERSION, 4);
        assert_eq!(oldest_supported_version(), 3);
    }

    #[test]
    fn own_version_is_always_accepted() {
        assert_eq!(negotiate(PROTOCOL_VERSION), Ok(PROTOCOL_VERSION));
    }

    #[test]
    fn newer_peer_is_refused_with_a_specific_reason() {
        assert_eq!(
            negotiate(PROTOCOL_VERSION.saturating_add(1)),
            Err(Incompatible::PeerTooNew {
                peer: PROTOCOL_VERSION + 1,
                ours: PROTOCOL_VERSION,
            })
        );
    }

    proptest! {
        /// The very first byte off an untrusted socket. Totality is the point.
        #[test]
        fn never_panics_for_any_byte(peer: u8) {
            let _ = negotiate(peer);
        }

        /// Acceptance is exactly the closed window, no wider and no narrower.
        #[test]
        fn accepts_exactly_the_window(peer: u8) {
            let inside = peer >= oldest_supported_version() && peer <= PROTOCOL_VERSION;
            prop_assert_eq!(negotiate(peer).is_ok(), inside);
        }

        /// A negotiated version is never one this build cannot speak.
        #[test]
        fn negotiated_version_is_always_speakable(peer: u8) {
            if let Ok(agreed) = negotiate(peer) {
                prop_assert!(agreed <= PROTOCOL_VERSION);
                prop_assert!(agreed >= oldest_supported_version());
                prop_assert_eq!(agreed, peer);
            }
        }

        /// The refusal always carries the numbers a shell needs to explain
        /// itself, so no layer below the shell has to format a message.
        #[test]
        fn refusal_carries_actionable_numbers(peer: u8) {
            match negotiate(peer) {
                Ok(_) => {}
                Err(Incompatible::PeerTooNew { peer: got, ours }) => {
                    prop_assert_eq!(got, peer);
                    prop_assert_eq!(ours, PROTOCOL_VERSION);
                    prop_assert!(got > ours);
                }
                Err(Incompatible::PeerTooOld { peer: got, oldest_supported }) => {
                    prop_assert_eq!(got, peer);
                    prop_assert_eq!(oldest_supported, oldest_supported_version());
                    prop_assert!(got < oldest_supported);
                }
            }
        }
    }
}
