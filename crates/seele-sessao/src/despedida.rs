//! **Esta despedida acaba a sessão?** Uma pergunta, e uma resposta só.
//!
//! # Por que aqui, e não no motor do enlace
//!
//! Porque eram duas respostas, e elas discordavam. O motor do enlace
//! (`seele_core::enlace`) tratava `FellBehind` como queda — repassava a
//! despedida e reconectava pela bateria —, e o [`crate::state::Room`] tratava
//! **toda** `Disconnecting` como fim. Medido pela F1 (pendência 61, 10/10): a
//! janela mostrava «ENLACE ENCERRADO · ESTE ENLACE FICOU PARA TRÁS DO
//! SERVIDOR», a FFI emitia `Ended` e o app revogava a geração, enquanto o motor
//! reconectava por baixo e o servidor tinha a pessoa sentada na sala quatro
//! segundos depois, em 11 de 11 rodadas. O que ela escrevia no instante ficava
//! em `SemResposta` debaixo da tela de fim.
//!
//! A pergunta passou a morar aqui, no crate que os dois já veem: o `Room` mora
//! neste crate, e o `seele-core` o reexporta. O motor e o `Room` chamam a
//! **mesma** função, e uma segunda lista não tem onde nascer sem que um teste a
//! veja: `toda_despedida_acaba_onde_o_motor_acaba`, no `state.rs`, e o par dele
//! no `enlace.rs`.
//!
//! Só o protocolo é preciso para responder, e é o que mantém o crate dentro da
//! linha dele no `check-deps` — e compilável para o navegador, que monta o mesmo
//! retrato pelo mesmo `Room`.

use seele_proto::control::DisconnectReason;

/// As despedidas que **acabam** com a sessão, em vez de começarem a bateria.
///
/// Quase toda `Disconnecting` é uma queda com nome: manutenção, desligamento,
/// keepalive vencido, ficar para trás no barramento. Para todas elas reconectar
/// é o conserto, e é o que a bateria interna existe para fazer — o próprio
/// [`DisconnectReason::FellBehind`] tem isso escrito no doc dele.
///
/// Estas duas são o contrário: alguém do outro lado decidiu que esta pessoa não
/// fica. Reconectar as desfaz — e não em teoria. Medido em
/// `expulsar_acaba_com_a_sessao_e_deixa_voltar`, com sonda no servidor: a
/// conexão expulsa fecha, a bateria sobe outra em seguida, e ela **redeclara a
/// sala de voz guardada**. A pessoa expulsa reaparece sentada onde estava,
/// segundos depois, sem que ninguém tenha apertado nada; e como o assento agora
/// pertence à conexão nova, a saída da conexão expulsa não esvazia nada e
/// ninguém na sala é informado de que ela saiu. O verbo inteiro desfeito por um
/// recurso feito para túnel de trem.
///
/// `Banned` está aqui pelo mesmo motivo, e não porque a volta funcione: a
/// portaria a recusa. O que ela produz hoje é uma reconexão inútil a cada
/// batida da bateria por cinco minutos, e uma casca que diz «reconectando» a
/// quem foi banido.
///
/// [`DisconnectReason::ModsMudaram`] entrou pela mesma porta e por um motivo
/// próprio: **reconectar não pode dar certo sozinho.** O servidor passou a
/// exigir um conjunto que esta máquina não aceitou, e o anúncio da reconexão
/// vai encontrar exatamente o mesmo não. Uma bateria batendo por cinco minutos
/// contra uma decisão que só uma pessoa pode tomar é a casca dizendo
/// «reconectando» para quem precisa ler uma lista e responder — ADR 0045, «um
/// servidor que troca de MOD pergunta de novo», e perguntar exige alguém.
///
/// As três da sala pessoal — [`DisconnectReason::ChamadaEncerrada`],
/// [`DisconnectReason::NinguemAtendeu`] e [`DisconnectReason::ChamadaRecusada`]
/// — são terminais pela spec de 23/09/2026 («Protocolo»): reconectar abriria a
/// sala de quem acabou de desligar, ou tocaria de novo para quem não atendeu.
///
/// `ChamadaEncerrada` é a que chega aqui: quem recebe desliga, e
/// `Recepcao::desligar` (`seele-server`, `recepcao.rs`, desde `9890e4eb`) a
/// manda a cada convidado lá dentro, no meio da sessão, pelo mesmo
/// `Event::ServidorEncerrando` com que quem hospeda fecha avisando. Sem ela
/// aqui, a bateria ligaria de novo para a sala de quem acabou de desligar.
///
/// As outras duas o servidor manda na fase do toque, antes de a sessão
/// existir (`tocar` e `recusar_o_aperto`, em `session.rs`): chegam como recusa
/// do aperto de mão, `seele_core::ConnectError::Refused`, e não por aqui. Ficam na lista
/// porque são terminais pela mesma spec, e uma despedida com elas no meio de
/// uma sessão não pode começar uma bateria que toca de novo.
///
/// [`DisconnectReason::ServerShuttingDown`] é **quem hospeda fechando o
/// servidor**, e o servidor o manda a cada sessão antes de cair
/// (`Daemon::encerrar_avisando`, lote G-servidor da F5). Reconectar bate por
/// cinco minutos numa porta que ninguém vai atender, com a casca dizendo
/// «reconectando» a quem precisa ler «o servidor está encerrando». A mesma
/// razão também é a recusa do aperto de mão de um servidor que não lê o próprio
/// banco, e essa nunca chega aqui: a portaria a devolve como
/// `seele_core::ConnectError::Refused`.
///
/// [`DisconnectReason::ScheduledMaintenance`] fica de fora de propósito: é o
/// «volto já», e a bateria existe para esperar o servidor voltar.
#[must_use]
pub const fn a_sessao_acabou_aqui(motivo: DisconnectReason) -> bool {
    matches!(
        motivo,
        DisconnectReason::Kicked
            | DisconnectReason::Banned
            | DisconnectReason::ModsMudaram
            | DisconnectReason::ChamadaEncerrada
            | DisconnectReason::NinguemAtendeu
            | DisconnectReason::ChamadaRecusada
            | DisconnectReason::ServerShuttingDown
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toda despedida do protocolo tem de escolher um lado, e escolher **aqui**.
    ///
    /// # O que este teste prova, e o que ele não prova
    ///
    /// Ele **não** prova comportamento: o que segura a expulsão é
    /// `expulsar_acaba_com_a_sessao_e_deixa_voltar`, em `seele-conformance`, e
    /// esse teste foi provado por reversão.
    ///
    /// O que ele prova é contra deriva: o `match` abaixo é exaustivo sem braço
    /// `_`, então uma variante nova de [`DisconnectReason`] **não compila** até
    /// alguém dizer de que lado ela cai. Sem isto ela cairia calada no lado de
    /// reconectar, que é onde estava o defeito que este conserto fechou.
    ///
    /// # Retificação de 10/09
    ///
    /// A versão anterior deste doc dizia que, com esta função devolvendo `true`
    /// para tudo, «a suíte inteira do workspace continua verde». **É falso, e
    /// este próprio teste é quem desmente**: ele não confere a tabela contra si
    /// mesma — escreve a decisão esperada variante a variante, à parte da
    /// função, e compara. Sob aquela mutação a primeira variante da lista já
    /// derruba o `assert_eq!` abaixo: `Incompatible`, esperado `false`, volta
    /// `true`. Medido às 05:51 de 10/09 e remedido na candidata de 496ba8c5.
    ///
    /// A lacuna real era outra, e mais estreita: **nenhum teste de
    /// comportamento** segurava o lado de reconectar da fronteira, com um
    /// servidor de verdade escrevendo a despedida no fio. Quem a fechou foi
    /// `uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao`, em
    /// `seele-conformance/tests/bateria_interna.rs`. O relatório histórico de
    /// 10/09 não foi reescrito; a retificação está no complemento de 496ba8c5.
    #[test]
    fn toda_despedida_do_protocolo_escolhe_um_lado() {
        // `DisconnectReason::` escrito por extenso e sem `_`: é o `match` que
        // fica vermelho, e não uma lista que alguém esqueceria de atualizar.
        for motivo in [
            DisconnectReason::Incompatible,
            DisconnectReason::CredentialRejected,
            DisconnectReason::HandshakeTimeout,
            DisconnectReason::Kicked,
            DisconnectReason::Banned,
            DisconnectReason::ServerFull,
            DisconnectReason::ScheduledMaintenance,
            DisconnectReason::ServerShuttingDown,
            DisconnectReason::Timeout,
            DisconnectReason::ProtocolViolation,
            DisconnectReason::RateLimited,
            DisconnectReason::FellBehind,
            DisconnectReason::AdmissionPending,
            DisconnectReason::AdmissionDenied,
            DisconnectReason::NicknameTaken,
            DisconnectReason::ModsRecusados,
            DisconnectReason::ModsMudaram,
            DisconnectReason::ModsIndisponiveis,
            DisconnectReason::ChamadaEncerrada,
            DisconnectReason::NinguemAtendeu,
            DisconnectReason::ChamadaRecusada,
            DisconnectReason::ModsDeOutraApi { apis: 1 << 7 },
        ] {
            let acaba = match motivo {
                // Alguém decidiu que esta pessoa não fica. Reconectar desfaz.
                DisconnectReason::Kicked | DisconnectReason::Banned => true,
                // O servidor passou a exigir um conjunto de MODs que esta
                // máquina não aceitou. Reconectar encontra o mesmo não, e quem
                // desfaz é uma pessoa lendo a lista nova.
                DisconnectReason::ModsMudaram => true,
                // A sala pessoal: quem recebia encerrou, não atendeu ou
                // recusou. Reconectar abriria ou tocaria de novo uma ligação
                // que alguém decidiu acabar (spec de 23/09).
                DisconnectReason::ChamadaEncerrada
                | DisconnectReason::NinguemAtendeu
                | DisconnectReason::ChamadaRecusada => true,
                // Quem hospeda fechou o servidor, e avisou. Reconectar bate
                // numa porta que ninguém atende por cinco minutos, com a casca
                // dizendo «reconectando» a quem precisa ler «o servidor está
                // encerrando» (lote G-servidor da F5).
                DisconnectReason::ServerShuttingDown => true,
                // A bateria é o conserto: o servidor volta, ou o cliente
                // reconecta e busca o histórico que faltou. O doc de
                // `FellBehind` diz isso com todas as letras. A manutenção
                // programada é o «volto já»: o servidor volta, e a bateria
                // existe para esperá-lo.
                DisconnectReason::ScheduledMaintenance
                | DisconnectReason::Timeout
                | DisconnectReason::FellBehind
                | DisconnectReason::RateLimited
                | DisconnectReason::ProtocolViolation => false,
                // Nunca chegam ao motor: a portaria as devolve como
                // `ConnectError::Refused`, e não há sessão para acabar.
                DisconnectReason::Incompatible
                | DisconnectReason::CredentialRejected
                | DisconnectReason::HandshakeTimeout
                | DisconnectReason::ServerFull
                | DisconnectReason::AdmissionPending
                | DisconnectReason::AdmissionDenied
                | DisconnectReason::NicknameTaken
                // Nascem no aperto de mão, como as de cima: quem recusa os MODs
                // ou encontra um servidor que não consegue anunciá-los nunca
                // chegou a ter sessão.
                | DisconnectReason::ModsRecusados
                | DisconnectReason::ModsIndisponiveis
                // A API de MOD que o cliente não roda: nasce no `Hello`, antes
                // do convite, e não há sessão para acabar.
                | DisconnectReason::ModsDeOutraApi { .. } => false,
            };
            assert_eq!(
                a_sessao_acabou_aqui(motivo),
                acaba,
                "{motivo:?} mudou de lado sem que este teste mudasse junto"
            );
        }
    }
}
