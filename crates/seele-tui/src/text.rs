//! Enumerated reasons, in Portuguese.
//!
//! `specs/02-protocolo.md` refuses generic error strings on the wire and carries
//! reasons as enums instead, so that every interface can say the same thing in
//! its own language. `crates/seele-proto/src/lib.rs` states the intent: nothing
//! below this layer decides what a human reads.
//!
//! This module is that decision for the terminal client. It is deliberately a
//! flat `match` and not a lookup table: adding a variant to the protocol should
//! break this file, and a table with a fallback would not.
//!
//! # Not a translation layer yet
//!
//! One language, because the product has one. When a second arrives, the shape
//! that changes is the return type, not the call sites — which is the reason
//! every string in the interface routes through here rather than being written
//! where it is shown.

use seele_core::{AlertReason, DisconnectReason};

/// What an alert is about.
#[must_use]
pub fn alert(reason: AlertReason) -> &'static str {
    match reason {
        AlertReason::Mentioned => "VOCÊ FOI CHAMADO",
        AlertReason::SubsystemChanged => "UM SUBSISTEMA MUDOU DE ESTADO",
        AlertReason::SyncDegraded => "TAXA DE SINCRONIZAÇÃO EM QUEDA",
        AlertReason::CageEntryRefused => "ENTRADA NO CAGE RECUSADA",
        AlertReason::PermissionDenied => "PERMISSÃO NEGADA",
        AlertReason::CageFull => "CAGE LOTADO",
        AlertReason::OperatorNotice => "AVISO DO OPERADOR",
        AlertReason::RateLimited => "VOCÊ ESTÁ FALANDO RÁPIDO DEMAIS PARA O DOGMA",
        AlertReason::MovedByOperator => "UM OPERADOR MOVEU O SEU PLUG",
    }
}

/// Why the session ended.
///
/// Every one of these is written to be read by somebody who is now staring at a
/// dead client and wants to know whether to try again. So they say what
/// happened, not what the enum is called.
#[must_use]
pub fn disconnect(reason: DisconnectReason) -> &'static str {
    match reason {
        DisconnectReason::Incompatible => "VERSÃO INCOMPATÍVEL COM ESTE DOGMA",
        // specs/08-seguranca.md requires login failures to be uniform, so this
        // says nothing about whether the account exists. Wording that leaked
        // that difference would undo the property the protocol went to trouble
        // to have.
        DisconnectReason::CredentialRejected => "CREDENCIAL RECUSADA",
        DisconnectReason::HandshakeTimeout => "TEMPO ESGOTADO NA SINCRONIZAÇÃO INICIAL",
        DisconnectReason::Kicked => "DESCONECTADO POR UM OPERADOR",
        DisconnectReason::Banned => "ACESSO BARRADO POR UM OPERADOR",
        DisconnectReason::DogmaFull => "DOGMA LOTADO",
        DisconnectReason::ScheduledMaintenance => "MANUTENÇÃO PROGRAMADA",
        DisconnectReason::ServerShuttingDown => "O DOGMA ESTÁ ENCERRANDO",
        DisconnectReason::Timeout => "ENLACE PERDIDO",
        DisconnectReason::ProtocolViolation => "PROTOCOLO VIOLADO",
        DisconnectReason::RateLimited => "LIMITE DE MENSAGENS EXCEDIDO",
        // Diz o que houve com a conversa, e não o que houve com o barramento.
        // Quem lê isto quer saber se perdeu alguma coisa: perdeu, e voltar é o
        // que a traz de volta.
        DisconnectReason::FellBehind => {
            "ESTE ENLACE FICOU PARA TRÁS; RECONECTANDO PARA NÃO FALTAR MENSAGEM"
        }
    }
}

/// Whether trying again could plausibly work.
///
/// The internal battery reconnects on its own (`specs/04-servidor-seele.md`), and
/// retrying into a ban is both futile and rude to the server. So the client has
/// to be able to tell the two apart, and this is where it is decided.
#[must_use]
pub fn worth_retrying(reason: DisconnectReason) -> bool {
    match reason {
        DisconnectReason::Timeout
        | DisconnectReason::ScheduledMaintenance
        | DisconnectReason::ServerShuttingDown
        | DisconnectReason::DogmaFull
        | DisconnectReason::HandshakeTimeout
        // Reconectar **é** o conserto aqui: a sessão perdeu evento e só uma
        // sincronização inteira a repõe. Tratar isto como recusa deixaria o
        // piloto de fora justamente do caso em que voltar resolve.
        | DisconnectReason::FellBehind => true,

        DisconnectReason::Banned
        | DisconnectReason::Kicked
        | DisconnectReason::CredentialRejected
        | DisconnectReason::Incompatible
        | DisconnectReason::ProtocolViolation
        | DisconnectReason::RateLimited => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALERTS: [AlertReason; 9] = [
        AlertReason::Mentioned,
        AlertReason::SubsystemChanged,
        AlertReason::SyncDegraded,
        AlertReason::CageEntryRefused,
        AlertReason::PermissionDenied,
        AlertReason::CageFull,
        AlertReason::OperatorNotice,
        AlertReason::RateLimited,
        AlertReason::MovedByOperator,
    ];

    const DISCONNECTS: [DisconnectReason; 12] = [
        DisconnectReason::Incompatible,
        DisconnectReason::CredentialRejected,
        DisconnectReason::HandshakeTimeout,
        DisconnectReason::Kicked,
        DisconnectReason::Banned,
        DisconnectReason::DogmaFull,
        DisconnectReason::ScheduledMaintenance,
        DisconnectReason::ServerShuttingDown,
        DisconnectReason::Timeout,
        DisconnectReason::ProtocolViolation,
        DisconnectReason::RateLimited,
        DisconnectReason::FellBehind,
    ];

    #[test]
    fn every_reason_says_something_and_no_two_say_the_same() {
        // Two reasons sharing a sentence means one of them is unreportable, and
        // the pilot cannot tell which of two situations they are in.
        let alerts: std::collections::HashSet<&str> = ALERTS.iter().map(|r| alert(*r)).collect();
        assert_eq!(alerts.len(), ALERTS.len());

        let ends: std::collections::HashSet<&str> =
            DISCONNECTS.iter().map(|r| disconnect(*r)).collect();
        assert_eq!(ends.len(), DISCONNECTS.len());
    }

    #[test]
    fn no_message_leaks_a_variant_name() {
        // The point of the enum-on-the-wire rule is that the interface writes
        // the sentence. `Debug` output on screen means it did not.
        for reason in DISCONNECTS {
            let text = disconnect(reason);
            assert!(!text.contains(&format!("{reason:?}")), "{text}");
        }
    }

    #[test]
    fn a_rejected_credential_is_not_retried() {
        // Reconnecting into a refusal is futile, and doing it on a timer looks
        // like an attack from the server's side.
        assert!(!worth_retrying(DisconnectReason::CredentialRejected));
        assert!(!worth_retrying(DisconnectReason::Banned));
        assert!(!worth_retrying(DisconnectReason::RateLimited));
    }

    #[test]
    fn a_lost_link_is_retried() {
        // The internal battery exists precisely for this case.
        assert!(worth_retrying(DisconnectReason::Timeout));
        assert!(worth_retrying(DisconnectReason::ServerShuttingDown));
    }
}
