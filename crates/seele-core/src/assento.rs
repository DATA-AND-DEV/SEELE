//! O assento desta pessoa numa sala de voz, e as entradas que ela pediu e o
//! servidor ainda não decidiu.
//!
//! # Por que existe
//!
//! O cliente se senta sozinho quando pede uma sala, sem esperar resposta: o
//! servidor confirma a entrada por silêncio, e uma tela que esperasse uma
//! mensagem que não vem mostraria a sala vazia a quem está nela. Quem desfaz o
//! assento especulativo é a recusa — `Alert` com `VoiceRoomEntryRefused` ou
//! `VoiceRoomFull`.
//!
//! **E o servidor manda esses alertas também quando mantém a pessoa onde ela
//! estava.** A revisão do lote I-a da F5 mediu três casos com a pessoa sentada
//! em A no servidor e vista em A por todos, e fora de toda sala na própria tela:
//! pedir B com a senha errada; a volta de uma queda numa sala com senha, em que
//! o servidor resgata o assento e recusa a entrada que o motor refaz sem a
//! senha; e escolher um apelido que já é de outra pessoa, que o servidor recusa
//! com o mesmo motivo. Com a pendência 52, em que o áudio segue o assento, isso
//! deixava a pessoa surda e muda numa sala em que todos a veem.
//!
//! # A regra, uma só
//!
//! Uma recusa desfaz só o pedido de entrada **pendente**, e devolve o assento de
//! antes dele. Sem pedido pendente, ela não move ninguém.
//!
//! O pedido deixa de estar pendente quando volta o `Pong` do `Ping` que o motor
//! manda logo atrás dele, com o carimbo [`MARCA_DO_PING_DE_ENTRADA`]. O servidor
//! lê o fluxo de controle em ordem e responde o `Ping` na mesma volta em que o
//! lê, então a resposta ao pedido — a recusa, quando há — chega **antes** daquele
//! `Pong`. Nada no fio muda: o `Ping` e o `Pong` são os de sempre, e o servidor
//! devolve o carimbo sem lê-lo.
//!
//! O `Room` da casca e o motor do enlace seguem esta regra pelo mesmo tipo,
//! [`EntradasPendentes`], e pela mesma porta, [`EntradasPendentes::ouvir`]. Duas
//! cópias da contabilidade do assento já divergiram uma vez neste arquivo — o
//! motor esquecia a sala por um caminho e o `Room` por outro.

use std::collections::VecDeque;

use seele_proto::control::{AlertReason, ServerMessage};
use seele_proto::ids::{Ssrc, VoiceRoomId};

/// O carimbo do `Ping` que o motor manda logo depois de cada pedido de entrada.
///
/// O bit mais alto de um `u64`, que o contador dos `Ping` de rotina
/// (`Client::send_ping`, de um em um a partir de 1) nunca alcança: um `Pong`
/// com este carimbo é sempre a resposta a um pedido de entrada, e nunca a de um
/// `Ping` de rotina. Ver o doc do módulo.
pub const MARCA_DO_PING_DE_ENTRADA: u64 = 1 << 63;

/// Um pedido de entrada que o servidor ainda não decidiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entrada {
    /// A sala pedida.
    sala: VoiceRoomId,
    /// O assento de antes do pedido: é para ele que uma recusa devolve.
    antes: Option<VoiceRoomId>,
    /// Se é este pedido que define o assento agora.
    ///
    /// Deixa de valer quando outra coisa muda o assento depois dele — um pedido
    /// novo, a saída, o operador que move, a sala apagada. A recusa de um
    /// pedido que já não vale não move ninguém: o assento de agora é de outra
    /// decisão.
    vale: bool,
    /// Se a recusa dele já chegou. O `Pong` que vem atrás o tira da fila.
    recusada: bool,
}

/// As entradas pedidas que o servidor ainda não decidiu, na ordem em que foram
/// pedidas — que é a ordem em que o servidor as decide.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntradasPendentes {
    fila: VecDeque<Entrada>,
}

impl EntradasPendentes {
    /// Um pedido de entrada saiu: senta a pessoa na sala pedida, e guarda de
    /// onde ela veio.
    pub fn pedir(&mut self, assento: &mut Option<VoiceRoomId>, sala: VoiceRoomId) {
        self.superar();
        self.fila.push_back(Entrada {
            sala,
            antes: *assento,
            vale: true,
            recusada: false,
        });
        *assento = Some(sala);
    }

    /// A pessoa saiu da sala. Os pedidos ainda no ar deixam de definir o
    /// assento, e as respostas deles continuam chegando e sendo descartadas.
    pub fn sair(&mut self, assento: &mut Option<VoiceRoomId>) {
        self.superar();
        *assento = None;
    }

    /// O que uma mensagem do servidor faz com o assento. Devolve se ele mudou.
    ///
    /// É a única porta do servidor para o assento, para o `Room` e para o
    /// motor: o operador que move, a sala apagada, a recusa e a confirmação.
    pub fn ouvir(&mut self, assento: &mut Option<VoiceRoomId>, mensagem: &ServerMessage) -> bool {
        match mensagem {
            ServerMessage::MovedToVoiceRoom { voice_room } => {
                self.superar();
                let mudou = *assento != Some(*voice_room);
                *assento = Some(*voice_room);
                mudou
            }
            ServerMessage::VoiceRoomDeleted { voice_room } if *assento == Some(*voice_room) => {
                self.superar();
                *assento = None;
                true
            }
            ServerMessage::Alert {
                reason: AlertReason::VoiceRoomEntryRefused | AlertReason::VoiceRoomFull,
                ..
            } => self.recusar(assento),
            ServerMessage::Pong { timestamp } if *timestamp == MARCA_DO_PING_DE_ENTRADA => {
                // Decidido: recusado, e a recusa já desfez o assento; ou aceito,
                // e o assento especulativo era o de verdade.
                self.fila.pop_front();
                false
            }
            _ => false,
        }
    }

    /// A recusa do pedido que o servidor está decidindo agora: o mais antigo.
    fn recusar(&mut self, assento: &mut Option<VoiceRoomId>) -> bool {
        let Some(recusada) = self.fila.front_mut() else {
            // Sem pedido no ar, a recusa não é de entrada nenhuma desta
            // pessoa — a troca de apelido recusada chega com este motivo — e
            // não move ninguém.
            return false;
        };
        if recusada.recusada {
            return false;
        }
        recusada.recusada = true;
        let (sala, antes, valia) = (recusada.sala, recusada.antes, recusada.vale);
        recusada.vale = false;
        // O pedido seguinte, se partia desta sala, parte do que havia antes
        // dela: a entrada recusada nunca aconteceu.
        if let Some(seguinte) = self.fila.get_mut(1) {
            if seguinte.antes == Some(sala) {
                seguinte.antes = antes;
            }
        }
        if !valia {
            return false;
        }
        let mudou = *assento != antes;
        *assento = antes;
        mudou
    }

    /// O assento que o servidor já confirmou.
    ///
    /// O mesmo de `assento` quando nenhum pedido está no ar; com um no ar, o de
    /// antes dele — e de antes de cada pedido anterior que também está no ar.
    /// É por este que o áudio abre: o microfone não abre numa sala que ainda
    /// pode recusar.
    #[must_use]
    pub fn confirmada(&self, assento: Option<VoiceRoomId>) -> Option<VoiceRoomId> {
        let Some(ultimo) = self.fila.back().filter(|entrada| entrada.vale) else {
            return assento;
        };
        let mut confirmada = ultimo.antes;
        for anterior in self.fila.iter().rev().skip(1) {
            if !anterior.recusada && Some(anterior.sala) == confirmada {
                confirmada = anterior.antes;
            }
        }
        confirmada
    }

    /// Quantos pedidos estão no ar.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fila.len()
    }

    /// Se não há pedido no ar.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fila.is_empty()
    }

    /// A conexão acabou: os pedidos dela não terão resposta.
    pub fn esquecer(&mut self) {
        self.fila.clear();
    }

    fn superar(&mut self) {
        for entrada in &mut self.fila {
            entrada.vale = false;
        }
    }
}

/// O assento que o servidor devolveu numa volta, se devolveu.
///
/// **Um `ssrc` igual na sessão nova prova que o servidor resgatou o assento.** O
/// servidor guarda o assento e o `ssrc` de quem cai por cinco minutos, e devolve
/// os dois a quem volta dentro deles (`Slots::reclaim`, no `seele-server`); uma
/// sessão nova ganha um `ssrc` novo. O assento resgatado é o confirmado de
/// antes da queda.
#[must_use]
pub fn assento_resgatado(
    ssrc_antes: Option<Ssrc>,
    ssrc_agora: Ssrc,
    confirmada: Option<VoiceRoomId>,
) -> Option<VoiceRoomId> {
    if ssrc_antes == Some(ssrc_agora) {
        confirmada
    } else {
        None
    }
}

#[cfg(test)]
mod testes {
    use super::{assento_resgatado, EntradasPendentes, MARCA_DO_PING_DE_ENTRADA};
    use seele_proto::control::{AlertReason, AlertSeverity, ServerMessage};
    use seele_proto::ids::{Ssrc, VoiceRoomId};

    const A: VoiceRoomId = VoiceRoomId(1);
    const B: VoiceRoomId = VoiceRoomId(2);
    const C: VoiceRoomId = VoiceRoomId(3);

    fn recusa(reason: AlertReason) -> ServerMessage {
        ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason,
            operator_text: None,
        }
    }

    fn pong_da_entrada() -> ServerMessage {
        ServerMessage::Pong {
            timestamp: MARCA_DO_PING_DE_ENTRADA,
        }
    }

    /// Sentada em A, confirmada.
    fn em_a() -> (EntradasPendentes, Option<VoiceRoomId>) {
        let mut entradas = EntradasPendentes::default();
        let mut assento = None;
        entradas.pedir(&mut assento, A);
        entradas.ouvir(&mut assento, &pong_da_entrada());
        (entradas, assento)
    }

    #[test]
    fn a_recusa_de_outra_sala_devolve_a_de_antes() {
        let (mut entradas, mut assento) = em_a();
        entradas.pedir(&mut assento, B);
        assert_eq!(assento, Some(B), "o assento especulativo não foi para B");
        assert_eq!(
            entradas.confirmada(assento),
            Some(A),
            "com B no ar, o confirmado é A"
        );

        assert!(entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused)));
        assert_eq!(
            assento,
            Some(A),
            "a recusa de B tirou de toda sala quem o servidor manteve em A"
        );
        entradas.ouvir(&mut assento, &pong_da_entrada());
        assert!(entradas.is_empty());
        assert_eq!(entradas.confirmada(assento), Some(A));
    }

    #[test]
    fn uma_recusa_sem_pedido_no_ar_nao_move_ninguem() {
        let (mut entradas, mut assento) = em_a();
        for motivo in [
            AlertReason::VoiceRoomEntryRefused,
            AlertReason::VoiceRoomFull,
        ] {
            assert!(!entradas.ouvir(&mut assento, &recusa(motivo)));
            assert_eq!(
                assento,
                Some(A),
                "uma recusa sem pedido no ar ({motivo:?}) tirou a pessoa da sala"
            );
        }
    }

    #[test]
    fn a_recusa_de_quem_estava_fora_devolve_para_fora() {
        let mut entradas = EntradasPendentes::default();
        let mut assento = None;
        entradas.pedir(&mut assento, A);
        assert_eq!(entradas.confirmada(assento), None);
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomFull));
        entradas.ouvir(&mut assento, &pong_da_entrada());
        assert_eq!(assento, None, "a recusa deixou sentado quem nunca entrou");
        assert!(entradas.is_empty());
    }

    #[test]
    fn o_pong_da_entrada_confirma_e_o_de_rotina_nao() {
        let mut entradas = EntradasPendentes::default();
        let mut assento = None;
        entradas.pedir(&mut assento, A);
        entradas.ouvir(&mut assento, &ServerMessage::Pong { timestamp: 4 });
        assert_eq!(
            entradas.confirmada(assento),
            None,
            "o `Pong` de um `Ping` de rotina, que pode ser de antes do pedido, confirmou a entrada"
        );
        entradas.ouvir(&mut assento, &pong_da_entrada());
        assert_eq!(entradas.confirmada(assento), Some(A));
        // E uma recusa depois disso já não é desta entrada.
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        assert_eq!(assento, Some(A));
    }

    #[test]
    fn dois_pedidos_no_ar_cada_resposta_e_do_seu() {
        // Em A, pede B e logo C. O servidor recusa B e aceita C.
        let (mut entradas, mut assento) = em_a();
        entradas.pedir(&mut assento, B);
        entradas.pedir(&mut assento, C);
        assert_eq!(assento, Some(C));
        assert_eq!(entradas.confirmada(assento), Some(A));

        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        assert_eq!(
            assento,
            Some(C),
            "a recusa de B, que já não definia o assento, tirou a pessoa de C"
        );
        assert_eq!(entradas.confirmada(assento), Some(A));
        entradas.ouvir(&mut assento, &pong_da_entrada());
        entradas.ouvir(&mut assento, &pong_da_entrada());
        assert_eq!(assento, Some(C));
        assert_eq!(entradas.confirmada(assento), Some(C));
        assert!(entradas.is_empty());
    }

    #[test]
    fn a_recusa_do_segundo_pedido_volta_ao_que_havia_antes_do_primeiro_recusado() {
        // Em A, pede B e logo C. O servidor recusa os dois: a pessoa fica em A.
        let (mut entradas, mut assento) = em_a();
        entradas.pedir(&mut assento, B);
        entradas.pedir(&mut assento, C);
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        entradas.ouvir(&mut assento, &pong_da_entrada());
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomFull));
        entradas.ouvir(&mut assento, &pong_da_entrada());
        assert_eq!(assento, Some(A));
    }

    #[test]
    fn sair_ou_ser_movido_com_um_pedido_no_ar_nao_e_desfeito_pela_recusa() {
        let (mut entradas, mut assento) = em_a();
        entradas.pedir(&mut assento, B);
        entradas.sair(&mut assento);
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        assert_eq!(assento, None, "a recusa de B pôs de volta em A quem saiu");

        let (mut entradas, mut assento) = em_a();
        entradas.pedir(&mut assento, B);
        entradas.ouvir(
            &mut assento,
            &ServerMessage::MovedToVoiceRoom { voice_room: C },
        );
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        assert_eq!(
            assento,
            Some(C),
            "a recusa de B desfez a mudança do operador"
        );
        assert_eq!(entradas.confirmada(assento), Some(C));
    }

    #[test]
    fn a_sala_apagada_tira_quem_estava_nela_e_so_ela() {
        let (mut entradas, mut assento) = em_a();
        assert!(!entradas.ouvir(
            &mut assento,
            &ServerMessage::VoiceRoomDeleted { voice_room: B }
        ));
        assert_eq!(assento, Some(A));
        assert!(entradas.ouvir(
            &mut assento,
            &ServerMessage::VoiceRoomDeleted { voice_room: A }
        ));
        assert_eq!(assento, None);
    }

    #[test]
    fn o_ssrc_igual_e_o_assento_resgatado() {
        assert_eq!(assento_resgatado(Some(Ssrc(2)), Ssrc(2), Some(A)), Some(A));
        assert_eq!(
            assento_resgatado(Some(Ssrc(2)), Ssrc(3), Some(A)),
            None,
            "uma sessão nova, com `ssrc` novo, não trouxe assento nenhum"
        );
        assert_eq!(assento_resgatado(None, Ssrc(2), Some(A)), None);
    }
}
