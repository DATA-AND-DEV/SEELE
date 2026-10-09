//! O assento desta pessoa numa sala de voz, e as entradas que ela pediu e o
//! servidor ainda não decidiu.
//!
//! # Por que existe
//!
//! O cliente se senta sozinho quando pede uma sala, sem esperar resposta: o
//! servidor confirma a entrada por silêncio, e uma tela que esperasse uma
//! mensagem que não vem mostraria a sala vazia a quem está nela. Quem desfaz o
//! assento especulativo é a recusa — `Alert` com `VoiceRoomEntryRefused` ou
//! `VoiceRoomFull`, ou um `PedidoRecusado` que nomeia a entrada.
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

use seele_proto::control::{nome_do_pedido, AlertReason, ServerMessage};
use seele_proto::ids::{SessionId, Ssrc, VoiceRoomId};

/// A marca do `Ping` que o motor manda logo depois de cada pedido de entrada.
///
/// O bit mais alto de um `u64`, que o contador dos `Ping` de rotina
/// (`Client::send_ping`, de um em um a partir de 1) nunca alcança: um `Pong`
/// com esta marca é sempre a resposta a um pedido de entrada, e nunca a de um
/// `Ping` de rotina. Ver o doc do módulo e [`carimbo_da_entrada`].
pub const MARCA_DO_PING_DE_ENTRADA: u64 = 1 << 63;

/// O carimbo do `Ping` do pedido de entrada de número `numero`: a marca e o
/// número.
///
/// **Numerado por causa do limitador do servidor** (N2 da rodada 2 do lote
/// I-a). Ele limita o `Ping` como qualquer quadro de controle, e acima da
/// rajada descarta calado: um `Ping` marcado que não volta deixava o pedido
/// dele na frente da fila, e o `Pong` do pedido seguinte o tirava no lugar do
/// seu. Com o número, o `Pong` do pedido `n` fecha o pedido `n` e todos os mais
/// velhos: o servidor os decidiu antes, porque lê o fluxo de controle em ordem.
///
/// Os pedidos são contados por conexão, a partir de 1, na ordem em que saem —
/// o `Room` da casca e o motor contam os mesmos pedidos na mesma ordem.
#[must_use]
pub const fn carimbo_da_entrada(numero: u64) -> u64 {
    MARCA_DO_PING_DE_ENTRADA | numero
}

/// O número do pedido de entrada que este carimbo de `Pong` fecha, se ele é de
/// um `Ping` marcado.
#[must_use]
pub const fn numero_do_carimbo(carimbo: u64) -> Option<u64> {
    if carimbo & MARCA_DO_PING_DE_ENTRADA == 0 {
        None
    } else {
        Some(carimbo & !MARCA_DO_PING_DE_ENTRADA)
    }
}

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
    /// O número dele nesta conexão, que vai no carimbo do `Ping` marcado.
    numero: u64,
}

/// As entradas pedidas que o servidor ainda não decidiu, na ordem em que foram
/// pedidas — que é a ordem em que o servidor as decide.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntradasPendentes {
    fila: VecDeque<Entrada>,
    /// O número do último pedido desta conexão. Ver [`carimbo_da_entrada`].
    ultimo: u64,
}

impl EntradasPendentes {
    /// Um pedido de entrada saiu: senta a pessoa na sala pedida, e guarda de
    /// onde ela veio. Devolve o número dele, para o carimbo do `Ping` marcado.
    pub fn pedir(&mut self, assento: &mut Option<VoiceRoomId>, sala: VoiceRoomId) -> u64 {
        self.superar();
        self.ultimo += 1;
        self.fila.push_back(Entrada {
            sala,
            antes: *assento,
            vale: true,
            recusada: false,
            numero: self.ultimo,
        });
        *assento = Some(sala);
        self.ultimo
    }

    /// O número do último pedido desta conexão, se já houve um.
    #[must_use]
    pub const fn ultimo_numero(&self) -> Option<u64> {
        if self.ultimo == 0 {
            None
        } else {
            Some(self.ultimo)
        }
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
            ServerMessage::VoiceRoomDeleted { voice_room } => {
                // Um pedido no ar que partia da sala apagada já não tem para
                // onde voltar. Se o servidor apagou a sala antes de ler o
                // pedido, ele tirou a pessoa dela (`desassentar`, no braço de
                // `Event::VoiceRoomDeleted` de `session.rs`), e a recusa do
                // pedido a deixa fora de sala. Se leu o pedido antes, a recusa
                // chegou antes deste aviso e devolveu a pessoa à sala ainda de
                // pé, e as linhas de baixo a tiram dela.
                //
                // Menos quando a sala apagada é ela mesma um pedido ainda no
                // ar: aí o servidor pode tê-la recusado sem nunca sentar a
                // pessoa nela, e quem conserta o `antes` dos pedidos seguintes
                // é a recusa dela, pela corrente de `recusar`.
                let mut pedida_no_ar = false;
                for entrada in &mut self.fila {
                    if entrada.antes == Some(*voice_room) && !pedida_no_ar {
                        entrada.antes = None;
                    }
                    if entrada.sala == *voice_room && !entrada.recusada {
                        pedida_no_ar = true;
                    }
                }
                if *assento != Some(*voice_room) {
                    return false;
                }
                self.superar();
                *assento = None;
                true
            }
            ServerMessage::Alert {
                reason: AlertReason::VoiceRoomEntryRefused | AlertReason::VoiceRoomFull,
                ..
            } => self.recusar(assento),
            // **A recusa que diz qual pedido foi** (F1, `PedidoRecusado`). Só a
            // da entrada desfaz a entrada, e pela mesma regra das duas de cima:
            // a F1 escreve que, com dois pedidos do mesmo tipo em voo, a recusa
            // é do mais antigo sem resposta — que é a frente desta fila. A de
            // qualquer outro pedido — o apelido tomado, o canal que sumiu — não
            // é de entrada nenhuma e cai no `_` lá embaixo, sem mover ninguém.
            ServerMessage::Alert {
                reason: AlertReason::PedidoRecusado { pedido, .. },
                ..
            } if e_a_entrada(*pedido) => self.recusar(assento),
            ServerMessage::Pong { timestamp } => {
                // O pedido deste número está decidido, e todos os mais velhos
                // também: recusados, e a recusa já desfez o assento; aceitos, e
                // o assento especulativo era o de verdade; ou perdidos no
                // limitador do servidor, sem resposta que ainda vá chegar.
                if let Some(numero) = numero_do_carimbo(*timestamp) {
                    while self
                        .fila
                        .front()
                        .is_some_and(|entrada| entrada.numero <= numero)
                    {
                        self.fila.pop_front();
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// A recusa do pedido que o servidor está decidindo agora: o mais antigo.
    fn recusar(&mut self, assento: &mut Option<VoiceRoomId>) -> bool {
        let Some(recusada) = self.fila.front_mut() else {
            // Sem pedido no ar, a recusa não é de entrada nenhuma desta
            // pessoa — de um host 0.15.x, a troca de apelido recusada chega com
            // este motivo — e não move ninguém.
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

    /// A conexão acabou: os pedidos dela não terão resposta, e a conexão nova
    /// conta os dela do 1.
    pub fn esquecer(&mut self) {
        self.fila.clear();
        self.ultimo = 0;
    }

    fn superar(&mut self) {
        for entrada in &mut self.fila {
            entrada.vale = false;
        }
    }
}

/// Se o pedido de ordinal `pedido`, em `ClientMessage`, é a entrada numa sala de
/// voz.
///
/// Pelo nome, com `nome_do_pedido`, que a F1 deixou para isto: «é por nome que a
/// casca escreve a frase de um `PedidoRecusado` e acha o pedido a desfazer». O
/// ordinal é congelado na linha 1 (`tests/fio_1_0.rs`); o nome não, e o teste
/// `a_recusa_que_nomeia_a_entrada_a_desfaz_como_as_recusas_de_sala` lê o
/// ordinal de um quadro de entrada de verdade, então um nome trocado reprova
/// lá em vez de calar aqui.
fn e_a_entrada(pedido: u32) -> bool {
    nome_do_pedido(pedido) == Some("EnterVoiceRoom")
}

/// Uma conexão, pelo que a volta compara: o `ssrc` e o número da sessão.
pub type Conexao = (Ssrc, SessionId);

/// O assento que o servidor devolveu numa volta, se devolveu.
///
/// **Resgate é o mesmo `ssrc` com uma sessão de número maior.** O servidor
/// guarda o assento e o `ssrc` de quem cai por cinco minutos, e devolve os dois
/// a quem volta dentro deles, com uma sessão nova (`Slots::reclaim`; em
/// `seele-server/src/session.rs`, linhas 1188–1200 de `8c62d559`: o `ssrc`
/// resgatado no lugar do novo, e o `SessionId` sempre o recém-emitido). O
/// assento resgatado é o confirmado de antes da queda.
///
/// **Só o `ssrc` não basta.** Os dois contadores recomeçam em 1 a cada subida
/// do servidor e andam juntos (`Registry::new` e `Registry::issue`, linhas
/// 202–208 e 221–226): depois de um reinício, quem volta pode ganhar o mesmo
/// `ssrc` de antes, sem assento nenhum, e com uma sessão de número igual ou
/// menor que a antiga. Tomado por resgate, ele deixava a pessoa sentada na
/// própria tela e fora de toda sala no servidor — N1 da rodada 2 do lote I-a,
/// medido com o servidor caindo e subindo de novo na mesma porta. Num resgate,
/// a sessão nova é sempre maior: o contador de sessões não volta enquanto o
/// processo vive.
///
/// Isto se apoia em como o servidor numera, e não em algo que o fio promete.
/// Dizer o resgate no fio — um campo da sessão, ou uma mensagem — seria da F1.
#[must_use]
pub fn assento_resgatado(
    antes: Option<Conexao>,
    agora: Conexao,
    confirmada: Option<VoiceRoomId>,
) -> Option<VoiceRoomId> {
    let (ssrc_agora, sessao_agora) = agora;
    match antes {
        Some((ssrc_antes, sessao_antes))
            if ssrc_antes == ssrc_agora && sessao_agora > sessao_antes =>
        {
            confirmada
        }
        _ => None,
    }
}

#[cfg(test)]
mod testes {
    use super::{assento_resgatado, EntradasPendentes};
    use seele_proto::control::{
        AlertReason, AlertSeverity, CausaDaRecusa, ClientMessage, ServerMessage,
    };
    use seele_proto::ids::{MessageId, SessionId, Ssrc, VoiceRoomId};

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

    /// O `Pong` do `Ping` marcado do pedido de número `n` desta conexão, como o
    /// cliente o carimba.
    fn pong_do_pedido(n: u64) -> ServerMessage {
        ServerMessage::Pong {
            timestamp: super::carimbo_da_entrada(n),
        }
    }

    /// Sentada em A, confirmada.
    fn em_a() -> (EntradasPendentes, Option<VoiceRoomId>) {
        let mut entradas = EntradasPendentes::default();
        let mut assento = None;
        let a = entradas.pedir(&mut assento, A);
        entradas.ouvir(&mut assento, &pong_do_pedido(a));
        (entradas, assento)
    }

    #[test]
    fn a_recusa_de_outra_sala_devolve_a_de_antes() {
        let (mut entradas, mut assento) = em_a();
        let b = entradas.pedir(&mut assento, B);
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
        entradas.ouvir(&mut assento, &pong_do_pedido(b));
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
        let a = entradas.pedir(&mut assento, A);
        assert_eq!(entradas.confirmada(assento), None);
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomFull));
        entradas.ouvir(&mut assento, &pong_do_pedido(a));
        assert_eq!(assento, None, "a recusa deixou sentado quem nunca entrou");
        assert!(entradas.is_empty());
    }

    #[test]
    fn o_pong_da_entrada_confirma_e_o_de_rotina_nao() {
        let mut entradas = EntradasPendentes::default();
        let mut assento = None;
        let a = entradas.pedir(&mut assento, A);
        entradas.ouvir(&mut assento, &ServerMessage::Pong { timestamp: 4 });
        assert_eq!(
            entradas.confirmada(assento),
            None,
            "o `Pong` de um `Ping` de rotina, que pode ser de antes do pedido, confirmou a entrada"
        );
        entradas.ouvir(&mut assento, &pong_do_pedido(a));
        assert_eq!(entradas.confirmada(assento), Some(A));
        // E uma recusa depois disso já não é desta entrada.
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        assert_eq!(assento, Some(A));
    }

    #[test]
    fn dois_pedidos_no_ar_cada_resposta_e_do_seu() {
        // Em A, pede B e logo C. O servidor recusa B e aceita C.
        let (mut entradas, mut assento) = em_a();
        let b = entradas.pedir(&mut assento, B);
        let c = entradas.pedir(&mut assento, C);
        assert_eq!(assento, Some(C));
        assert_eq!(entradas.confirmada(assento), Some(A));

        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        assert_eq!(
            assento,
            Some(C),
            "a recusa de B, que já não definia o assento, tirou a pessoa de C"
        );
        assert_eq!(entradas.confirmada(assento), Some(A));
        entradas.ouvir(&mut assento, &pong_do_pedido(b));
        entradas.ouvir(&mut assento, &pong_do_pedido(c));
        assert_eq!(assento, Some(C));
        assert_eq!(entradas.confirmada(assento), Some(C));
        assert!(entradas.is_empty());
    }

    #[test]
    fn a_recusa_do_segundo_pedido_volta_ao_que_havia_antes_do_primeiro_recusado() {
        // Em A, pede B e logo C. O servidor recusa os dois: a pessoa fica em A.
        let (mut entradas, mut assento) = em_a();
        let b = entradas.pedir(&mut assento, B);
        let c = entradas.pedir(&mut assento, C);
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        entradas.ouvir(&mut assento, &pong_do_pedido(b));
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomFull));
        entradas.ouvir(&mut assento, &pong_do_pedido(c));
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

    /// N2 da rodada 2 do lote I-a: um `Ping` marcado que o limitador do
    /// servidor descarta não desalinha a fila.
    ///
    /// O servidor limita o `Ping` como qualquer quadro de controle, e acima da
    /// rajada descarta calado. O `Pong` que não vem deixava o pedido dele na
    /// frente da fila, e o `Pong` do pedido seguinte o tirava no lugar do seu:
    /// o pedido seguinte ficava no ar para sempre, o áudio nunca abria nele, e
    /// a primeira recusa sem pedido tirava a pessoa da sala.
    #[test]
    fn um_ping_marcado_perdido_no_limitador_nao_desalinha_a_fila() {
        let mut entradas = EntradasPendentes::default();
        let mut assento = None;
        // O pedido de A, e o `Ping` marcado dele se perde no limitador.
        entradas.pedir(&mut assento, A);
        // A pessoa sai, pede B, e o servidor aceita.
        entradas.sair(&mut assento);
        entradas.pedir(&mut assento, B);
        entradas.ouvir(&mut assento, &pong_do_pedido(2));

        assert_eq!(
            entradas.confirmada(assento),
            Some(B),
            "o `Pong` de B tirou da fila o pedido velho de A, e B ficou no ar para sempre: \
             o áudio nunca abre em B"
        );
        assert!(
            entradas.is_empty(),
            "o pedido de A, sem `Pong`, ficou na fila"
        );

        // E uma recusa sem pedido no ar não tira a pessoa de B.
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        assert_eq!(
            assento,
            Some(B),
            "uma recusa sem pedido tirou a pessoa de B"
        );
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

    /// Em A, pedir B; o servidor apaga A antes de ler o pedido — e tira a
    /// pessoa de A ao apagar — e depois recusa B. A pessoa fica fora de sala no
    /// servidor, e a recusa não pode devolvê-la a uma sala que não existe.
    #[test]
    fn a_recusa_nao_devolve_a_sala_de_antes_que_foi_apagada_com_o_pedido_no_ar() {
        let (mut entradas, mut assento) = em_a();
        let b = entradas.pedir(&mut assento, B);
        entradas.ouvir(
            &mut assento,
            &ServerMessage::VoiceRoomDeleted { voice_room: A },
        );
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        entradas.ouvir(&mut assento, &pong_do_pedido(b));

        assert_eq!(
            assento, None,
            "a recusa de B devolveu a pessoa a A, que o servidor apagou antes de ler o \
             pedido: a tela a mostra numa sala que não existe"
        );
        assert_eq!(
            entradas.confirmada(assento),
            None,
            "o áudio abriria em A, que não existe"
        );
    }

    /// O mesmo conserto não vale para a sala apagada que é ela mesma um pedido
    /// ainda no ar. Em A, pedir B e logo C; o servidor apaga B antes de ler o
    /// pedido de B, recusa B (a sala não existe) e recusa C: a pessoa nunca
    /// saiu de A. Quem devolve C para A é a recusa de B, pela corrente.
    #[test]
    fn a_sala_apagada_que_ainda_e_um_pedido_no_ar_fica_para_a_resposta_dele() {
        let (mut entradas, mut assento) = em_a();
        let b = entradas.pedir(&mut assento, B);
        let c = entradas.pedir(&mut assento, C);
        entradas.ouvir(
            &mut assento,
            &ServerMessage::VoiceRoomDeleted { voice_room: B },
        );
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        entradas.ouvir(&mut assento, &pong_do_pedido(b));
        entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
        entradas.ouvir(&mut assento, &pong_do_pedido(c));

        assert_eq!(
            assento,
            Some(A),
            "o servidor recusou B e C e manteve a pessoa em A, e a tela a tirou de toda sala"
        );
    }

    /// O ordinal de um pedido em `ClientMessage`, lido de um quadro de verdade,
    /// do jeito que o servidor o lê para dizer qual pedido recusou
    /// (`ordinal_do_quadro`, no braço de cada pedido de `session.rs`).
    fn ordinal_de(pedido: &ClientMessage) -> u32 {
        let quadro = seele_proto::control::encode(pedido).expect("o quadro do pedido");
        seele_proto::control::ordinal_do_quadro(&quadro).expect("o ordinal do pedido")
    }

    fn pedido_recusado(pedido: &ClientMessage, causa: CausaDaRecusa) -> ServerMessage {
        recusa(AlertReason::PedidoRecusado {
            pedido: ordinal_de(pedido),
            causa,
        })
    }

    /// Acréscimo D do lote I-b: a recusa de um pedido que **não** é a entrada
    /// não desfaz a entrada no ar.
    ///
    /// Era o caso que o I-a deixou: até a sexta integração o servidor recusava
    /// o apelido tomado com `VoiceRoomEntryRefused`, e uma troca de apelido
    /// recusada no mesmo instante de um pedido de sala tirava a pessoa da sala
    /// pedida. Com o `PedidoRecusado`, a recusa diz qual pedido foi, e só o da
    /// entrada desfaz a entrada.
    #[test]
    fn a_recusa_de_outro_pedido_com_uma_entrada_no_ar_nao_desfaz_a_entrada() {
        for (pedido, causa) in [
            (
                ClientMessage::SetNickname { name: "lia".into() },
                CausaDaRecusa::NomeTomado,
            ),
            (
                ClientMessage::RenameVoiceRoom {
                    voice_room: C,
                    name: "Outra".into(),
                },
                CausaDaRecusa::NaoExiste,
            ),
            (
                ClientMessage::RemoveMessage {
                    message: MessageId(9),
                },
                CausaDaRecusa::FalhaDoServidor,
            ),
        ] {
            let (mut entradas, mut assento) = em_a();
            let b = entradas.pedir(&mut assento, B);

            let mudou = entradas.ouvir(&mut assento, &pedido_recusado(&pedido, causa));
            assert!(
                !mudou && assento == Some(B),
                "a recusa de {pedido:?} ({causa:?}) desfez a entrada em B que estava no ar: \
                 a tela devolve a pessoa a A enquanto o servidor a senta em B (assento: \
                 {assento:?})"
            );

            // E a recusa de verdade da entrada, que chega depois, continua sendo
            // dela: a recusa do outro pedido não tomou o lugar da resposta da
            // entrada na fila.
            entradas.ouvir(&mut assento, &recusa(AlertReason::VoiceRoomEntryRefused));
            entradas.ouvir(&mut assento, &pong_do_pedido(b));
            assert_eq!(
                assento,
                Some(A),
                "depois da recusa de {pedido:?}, a recusa de B não devolveu a pessoa a A: a \
                 primeira recusa ficou com o lugar da resposta da entrada"
            );
        }
    }

    /// Acréscimo D: o `PedidoRecusado` que nomeia a própria entrada desfaz a
    /// entrada como as recusas de sala.
    ///
    /// A F1 diz do ordinal (`AlertReason::PedidoRecusado`, em `control.rs`):
    /// com dois pedidos do mesmo tipo em voo, a recusa é do mais antigo, e quem
    /// recebe desfaz o mais antigo dos pedidos daquele ordinal que ainda não
    /// teve resposta. É a mesma regra que `recusar` já segue para
    /// `VoiceRoomEntryRefused` e `VoiceRoomFull`. O servidor da 0.16 ainda não
    /// manda esta recusa para a entrada — as quatro recusas de entrada dele são
    /// as duas de sala —, mas o fio a permite, e uma 1.x que a mande não pode
    /// deixar a pessoa sentada numa sala que disse não.
    #[test]
    fn a_recusa_que_nomeia_a_entrada_a_desfaz_como_as_recusas_de_sala() {
        let entrada = ClientMessage::EnterVoiceRoom {
            voice_room: B,
            password: None,
        };
        for causa in [
            CausaDaRecusa::NaoExiste,
            CausaDaRecusa::FalhaDoServidor,
            CausaDaRecusa::NomeTomado,
        ] {
            let (mut entradas, mut assento) = em_a();
            let b = entradas.pedir(&mut assento, B);

            let mudou = entradas.ouvir(&mut assento, &pedido_recusado(&entrada, causa));
            assert_eq!(
                assento,
                Some(A),
                "o servidor recusou a entrada em B ({causa:?}) e a tela deixou a pessoa em B, \
                 numa sala em que ninguém a vê, com o áudio abrindo lá"
            );
            assert!(
                mudou,
                "a recusa da entrada desfez o assento e não disse que ele mudou: o roster \
                 não redesenha"
            );
            entradas.ouvir(&mut assento, &pong_do_pedido(b));
            assert_eq!(entradas.confirmada(assento), Some(A));
        }

        // E, como as recusas de sala, sem pedido no ar não move ninguém.
        let (mut entradas, mut assento) = em_a();
        assert!(!entradas.ouvir(
            &mut assento,
            &pedido_recusado(&entrada, CausaDaRecusa::NaoExiste)
        ));
        assert_eq!(
            assento,
            Some(A),
            "uma recusa de entrada sem pedido no ar tirou a pessoa da sala"
        );
    }

    #[test]
    fn o_ssrc_igual_com_sessao_maior_e_o_assento_resgatado() {
        assert_eq!(
            assento_resgatado(
                Some((Ssrc(2), SessionId(2))),
                (Ssrc(2), SessionId(5)),
                Some(A)
            ),
            Some(A),
            "o resgate — o mesmo `ssrc`, numa sessão nova — não devolveu o assento"
        );
        assert_eq!(
            assento_resgatado(
                Some((Ssrc(2), SessionId(2))),
                (Ssrc(3), SessionId(5)),
                Some(A)
            ),
            None,
            "uma sessão nova, com `ssrc` novo, não trouxe assento nenhum"
        );
        assert_eq!(
            assento_resgatado(None, (Ssrc(2), SessionId(5)), Some(A)),
            None
        );
    }

    /// N1: depois de um reinício do servidor, os contadores recomeçam juntos, e
    /// o mesmo `ssrc` vem numa sessão de número igual ou menor.
    #[test]
    fn o_ssrc_igual_com_sessao_igual_ou_menor_e_um_servidor_que_subiu_de_novo() {
        for sessao_agora in [SessionId(2), SessionId(1)] {
            assert_eq!(
                assento_resgatado(
                    Some((Ssrc(2), SessionId(2))),
                    (Ssrc(2), sessao_agora),
                    Some(A)
                ),
                None,
                "um `ssrc` repetido numa sessão {sessao_agora:?} — o servidor subiu de novo — foi \
                 tomado por resgate"
            );
        }
    }
}
