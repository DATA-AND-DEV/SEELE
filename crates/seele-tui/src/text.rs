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
        AlertReason::SyncDegraded => "SINAL EM QUEDA",
        AlertReason::CageEntryRefused => "ENTRADA NA SALA DE VOZ RECUSADA",
        AlertReason::PermissionDenied => "PERMISSÃO NEGADA",
        AlertReason::CageFull => "SALA DE VOZ LOTADA",
        AlertReason::OperatorNotice => "AVISO DO OPERADOR",
        AlertReason::RateLimited => "VOCÊ ESTÁ FALANDO RÁPIDO DEMAIS PARA O SERVIDOR",
        AlertReason::MovedByOperator => "UM OPERADOR MOVEU VOCÊ DE SALA",
        // O plug já saiu e a conversa já saiu da tela quando isto chega. Sem a
        // frase, o que resta é uma sala que sumiu sozinha — que de onde se lê é
        // igualzinho a um cliente que perdeu a conta de onde estava.
        AlertReason::CageDeleted => "A SALA DE VOZ EM QUE VOCÊ ESTAVA FOI APAGADA",
        AlertReason::LineDeleted => {
            "O CANAL DE TEXTO QUE VOCÊ LIA FOI APAGADO, COM TUDO QUE HAVIA NELE"
        }
        // A única recusa desta lista, e a única que ensina o passo seguinte.
        AlertReason::LastCage => {
            "ESTA É A ÚNICA SALA DE VOZ DO SERVIDOR. FAÇA OUTRA ANTES DE APAGAR ESTA"
        }
        // Uma transmissão por sala. Diz **quem** ocupou, e não só que não deu:
        // sem o nome, quem lê não sabe se espera ou se pede. E não é
        // `PERMISSÃO NEGADA`, que mandaria a pessoa procurar um papel que ela
        // já tem — aqui é só a vez de outro.
        AlertReason::ScreenShareTaken => "ALGUÉM JÁ ESTÁ COMPARTILHANDO A TELA NESTA SALA",
        // Não é a sua conexão: é a de quem hospeda, dividida por quanta gente
        // está assistindo. Dizer «sua conexão caiu» mandaria a pessoa mexer no
        // roteador dela, que é o lugar errado — e o §5.1 escreve que a razão
        // aparece, e não só o efeito.
        AlertReason::ScreenShareOverHostUplink => {
            "A TELA PAROU: A CONEXÃO DE QUEM HOSPEDA NÃO CARREGA TANTA GENTE ASSISTINDO"
        }
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
        DisconnectReason::Incompatible => "VERSÃO INCOMPATÍVEL COM ESTE SERVIDOR",
        // specs/08-seguranca.md requires login failures to be uniform, so this
        // says nothing about whether the account exists. Wording that leaked
        // that difference would undo the property the protocol went to trouble
        // to have.
        DisconnectReason::CredentialRejected => "CREDENCIAL RECUSADA",
        DisconnectReason::HandshakeTimeout => "TEMPO ESGOTADO NA SINCRONIZAÇÃO INICIAL",
        DisconnectReason::Kicked => "DESCONECTADO POR UM OPERADOR",
        DisconnectReason::Banned => "ACESSO BARRADO POR UM OPERADOR",
        DisconnectReason::DogmaFull => "SERVIDOR LOTADO",
        DisconnectReason::ScheduledMaintenance => "MANUTENÇÃO PROGRAMADA",
        DisconnectReason::ServerShuttingDown => "O SERVIDOR ESTÁ ENCERRANDO",
        DisconnectReason::Timeout => "ENLACE PERDIDO",
        DisconnectReason::ProtocolViolation => "PROTOCOLO VIOLADO",
        DisconnectReason::RateLimited => "LIMITE DE MENSAGENS EXCEDIDO",
        // Diz o que houve com a conversa, e não o que houve com o barramento.
        // Quem lê isto quer saber se perdeu alguma coisa: perdeu, e voltar é o
        // que a traz de volta.
        DisconnectReason::FellBehind => {
            "ESTE ENLACE FICOU PARA TRÁS; RECONECTANDO PARA NÃO FALTAR MENSAGEM"
        }
        // ADR 0030. As duas únicas desta lista sobre uma entrada que ainda pode
        // dar certo, e por isso as duas únicas que dizem o que fazer em seguida.
        //
        // Nenhuma delas fala em aguardar: nada está aguardando. A conexão caiu
        // no mesmo instante e o que ficou de pé é o pedido, do outro lado.
        DisconnectReason::AdmissionPending => {
            "QUEM HOSPEDA AINDA NÃO DECIDIU SOBRE VOCÊ; O PEDIDO FICOU GUARDADO"
        }
        DisconnectReason::AdmissionDenied => "QUEM HOSPEDA RECUSOU A SUA ENTRADA",
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
        | DisconnectReason::RateLimited
        // ADR 0030, e a pendente é a que engana. Voltar sozinho *funcionaria*
        // — no minuto em que quem hospeda aprovasse —, e é exatamente por isso
        // que não deve: seria uma bateria batendo na porta de outra pessoa por
        // tempo indeterminado, que é a espera sem fim que o ADR recusa, com o
        // agravante de a máquina fazê-la sozinha. Quem foi mandado tentar de
        // novo tenta quando quiser.
        | DisconnectReason::AdmissionPending
        | DisconnectReason::AdmissionDenied => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALERTS: [AlertReason; 12] = [
        AlertReason::Mentioned,
        AlertReason::SubsystemChanged,
        AlertReason::SyncDegraded,
        AlertReason::CageEntryRefused,
        AlertReason::PermissionDenied,
        AlertReason::CageFull,
        AlertReason::OperatorNotice,
        AlertReason::RateLimited,
        AlertReason::MovedByOperator,
        AlertReason::CageDeleted,
        AlertReason::LineDeleted,
        // The one that reads closest to `CageEntryRefused`, and the reason it
        // is not that: "entry refused" is a sentence about walking into a room,
        // in front of somebody who was trying to destroy one.
        AlertReason::LastCage,
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
