//! Protocol version negotiation.
//!
//! `specs/02-protocolo.md`:
//!
//! > The first byte of every control frame is the protocol version. The server
//! > refuses a version higher than its own with `Incompatible`. A lower version
//! > is accepted while it falls inside the declared compatibility window (N−1).
//!
//! **Desde o Servidor 1.0 (protocolo 9), a regra é outra:** o `Hello` leva a
//! [`Faixa`] que cada lado fala, os dois conversam pelo menor dos máximos
//! ([`negociar`]), e toda 1.x fala o 9 (ADR 0058, «1.x nunca quebra o fio»). A
//! janela N−1 acima é a história de antes da 1.0. O desenho inteiro está no
//! adendo `docs/superpowers/specs/2026-10-08-0.16-adendo-negociacao-de-versao.md`.
//!
//! This is the first thing that touches a byte off the wire, before any
//! authentication, so it is written to be total: every possible input byte
//! produces an answer and none of them panics.

use serde::{Deserialize, Serialize};
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
///
/// **5 desde 14/09/2026, e é uma subida só para duas entregas.**
///
/// A malha e o anúncio de MODs acrescentaram variantes às **mesmas duas listas**
/// em paralelo. Se cada uma tivesse subido a versão por conta própria, as duas
/// teriam chamado «5» a vocabulários diferentes — o defeito exato que
/// `control.rs::o_ultimo_verbo_de_cada_lista_esta_onde_esta_versao_o_deixou`
/// existe para pegar, e que já custou uma tela preta sem mensagem nenhuma. Por
/// isso o anúncio nasceu com uma constante própria,
/// [`crate::mods::VERSAO_DO_ANUNCIO`], já em 5 e deliberadamente acima desta —
/// esperando a integração conjunta em vez de disputá-la.
///
/// **Esta é a integração conjunta, e o contrato dela se cumpre aqui:** as duas
/// listas já convivem numa ordem única — `EmprestarSubida`, `ParFalhou`,
/// `AceitarMods` e `RecusarMods` no cliente; `SirvaTelaPara`, `AssistaTelaPor` e
/// `ModsExigidos` no servidor —, os ordinais da v4 continuam onde estavam, e
/// este número passa a 5 uma vez só. `VERSAO_DO_ANUNCIO` continua 5 e agora é
/// alcançada, então o portão do anúncio liga sozinho: ver
/// [`crate::mods::VERSAO_DO_ANUNCIO`] e
/// `seele_server::mods::anuncio::o_anuncio_alcanca_alguem`.
///
/// **E uma correção que vale para todas as subidas acima, medida em
/// 14/09/2026.** Cada uma delas diz que quem ficou na versão anterior «continua
/// entrando». Isso é verdade sobre o que *esta* build aceita ouvir, e só: todo
/// quadro sai carimbado com este número, então a build mais velha recusa a
/// resposta do servidor antes de ler o corpo dela. A janela de compatibilidade
/// abaixo explica a medida. A consequência prática, que precisa ser dita antes
/// de publicar e não depois: **publicar a v5 tira do ar quem está na v3
/// publicada**, e a janela não muda isso.
/// v6 adds authenticated MOD requests and private, chunked replies.
/// **7 desde 18/09/2026**: a `Session` passa a carregar a identidade do
/// servidor.
///
/// Um campo novo numa variante existente, e o postcard não é autodescritivo:
/// um cliente v6 leria o campo novo como o começo do próximo e desalinharia o
/// quadro inteiro. É a mesma armadilha que a subida para 3 registra, e a razão
/// de este número existir.
///
/// Por que um campo e não uma mensagem à parte: a identidade tem de estar
/// **junto** do resto da sessão. Numa mensagem separada haveria um instante
/// em que a sessão existe sem se saber de quem ela é — e é justamente nesse
/// instante que o aceite de MODs e a escolha de raiz precisam decidir.
///
/// **8 desde 21/09/2026**: uma mensagem de texto recusada volta identificada
/// — `ServerMessage::MessageRejected`, com o `client_message_id` de quem
/// escreveu e um [`crate::control::MessageRefusal`].
///
/// Variante nova, e o portão de `session::entende_a_mensagem` a cala para quem
/// não a conhece: o postcard não é autodescritivo, e uma variante desconhecida
/// não é ignorada — ela desloca a leitura do fluxo de controle daquele par para
/// sempre.
///
/// Por que uma mensagem à parte e não um campo em algo existente: ela não é
/// resposta a nenhum quadro que o servidor já mande. O envio é assíncrono por
/// desenho — enfileira, agrupa, grava, difunde —, então a recusa chega depois,
/// sozinha, e precisa dizer de qual mensagem fala.
///
/// **9 desde 08/10/2026: o Servidor 1.0, e a última quebra.**
///
/// O `Hello` deixou de levar um número só e passou a levar a [`Faixa`] que o
/// cliente fala, e a 0.15 — que fala 8 — fica de fora. Isso é o decidido
/// (spec-mestre da 0.16, §1): a subida junta numa vez só tudo o que a linha 1
/// vai precisar em mensagens que já existem, porque o postcard lê campo por
/// posição e um campo novo não pode chegar numa 1.x.
///
/// Daqui em diante este número é **o maior que a build fala**, e não o único.
/// Um conserto não sobe nada. A regra é o ADR 0058, e a linha de cada número é
/// a tabela de [`linha_de`].
///
/// **10 desde 24/10/2026: o Servidor 2.0, a linha 2** (ADR 0063).
///
/// A 0.17 troca o transporte: o QUIC sai e entra o WebRTC, nos nativos e no
/// navegador. Não é acréscimo, e o ADR 0058 (`:41`) diz o que é uma mudança
/// que não é acréscimo: a 2.0. A linha 1 fica de pé pelo launcher (ADR 0046),
/// e **não recebe versão nova**: o 9 é o único número dela, e a linha 2
/// começa no 10 ([`PRIMEIRO_DA_LINHA_DOIS`]). A linha 1 inteira fica fora da
/// faixa desta build, e um par dela não chega nem a mandar um `Hello`, porque
/// não há transporte em comum — o ADR 0063 diz como as duas pontas ficam
/// sabendo disso antes de tentar.
pub const PROTOCOL_VERSION: u8 = 11;

/// O primeiro número da linha 1. **Congelado** (ADR 0058), e desde a linha 2 o
/// único número dela: a linha 1 não recebe versão nova (ADR 0063).
pub const PRIMEIRO_DA_LINHA_UM: u8 = 9;

/// O primeiro número da linha 2. **Congelado**: toda 2.x o fala (ADR 0063,
/// pela regra do ADR 0058).
pub const PRIMEIRO_DA_LINHA_DOIS: u8 = 10;

/// O menor número que esta build fala: o primeiro número da linha dela. Na
/// linha 1 era [`PRIMEIRO_DA_LINHA_UM`]; na linha 2, sempre
/// [`PRIMEIRO_DA_LINHA_DOIS`], e é isso que deixa o 9 de fora.
///
/// # A janela N−1, que esta constante substitui
///
/// Até a 0.15 havia `COMPATIBILITY_WINDOW = 1` aqui, e esta é a história
/// dela, guardada porque é o argumento da troca.
///
/// ## Ficou em 1, e a medida que fez o 2 voltar atrás
///
/// Esta janela esteve em 2 por um dia, junto com a subida para a v5, e a
/// justificativa era esta: a v4 nunca foi publicada, a última release —
/// `v0.10.5-1`, de 05/09/2026, commit `12a6401a6` — fala **protocolo 3**, e uma
/// janela de N−1 recusaria a única versão instalada em campo em nome de uma que
/// ninguém tem.
///
/// **A premissa está certa e a conclusão era inútil.** A janela governa só
/// metade do encontro: o que *esta* build aceita ouvir. O que a outra ponta
/// ouve é decidido por [`crate::control::encode`], que carimba
/// [`PROTOCOL_VERSION`] no primeiro byte de **todo** quadro — sempre o número
/// global, nunca o negociado. O `decode` daquele commit publicado é este mesmo
/// e chama `negotiate` nesse byte antes de ler o corpo, com
/// `PROTOCOL_VERSION = 3` e `COMPATIBILITY_WINDOW = 1`: qualquer carimbo acima
/// de 3 morre ali, com `PeerTooNew`. Um par v3 de verdade recebe o primeiro
/// quadro de um servidor v5 e fecha — num servidor sem MOD nenhum, e com esta
/// janela em 2 do mesmo jeito. Preso em
/// `control.rs::a_release_publicada_recusa_o_carimbo_desta_build`, que desde
/// 08/10/2026 é `a_publicada_ou_e_da_mesma_linha_ou_le_a_recusa_desta_build` e
/// mede contra a v0.15.1, e não contra o 3.
///
/// Então o 2 não comprava nada que o 1 não comprasse: só deixava o par v3 morrer
/// mais tarde, no meio do aperto de mão, em vez de receber `Incompatible` na
/// primeira leitura. Voltou para 1 — a regra da spec —, e a versão anterior viva
/// aqui é a v4.
///
/// **O que isso cobra, dito por inteiro:** publicar a v5 tira do ar quem está na
/// v3, e nenhum número escrito nesta linha evita isso. Quem evita é o seletor de
/// versão do ADR 0046 e, antes dele, carimbar o quadro com a versão negociada em
/// vez da global — que é mudança de protocolo com custo próprio, porque o
/// servidor passaria a ter de calar as variantes fora do vocabulário do par, sob
/// pena de o postcard deslocar a leitura dele para sempre. Registrado na
/// pendência #42.
///
/// Preso em `a_versao_anterior_continua_dentro_da_janela`.
/// **O que a faixa troca nessa história.** O carimbo deixa de ser o número
/// global: depois do `Hello`, cada lado carimba com o número negociado
/// (`control::encode_em`), e o par mais velho lê. E o piso deixa de deslizar:
/// toda 1.x fala o 9, então uma 1.0 conversa com qualquer 1.x, nos dois
/// sentidos. Da linha 2 em diante, toda 2.x fala o 10, pela mesma regra.
pub const VERSAO_MINIMA: u8 = PRIMEIRO_DA_LINHA_DOIS;

/// Os números que um lado fala, do menor ao maior. Viaja no `Hello`.
///
/// O quadro do `Hello` sai carimbado com a [`Faixa::minima`] de quem liga, para
/// que qualquer servidor da mesma linha o leia; o resto da sessão sai com o
/// número que [`negociar`] escolheu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Faixa {
    /// O menor número.
    pub minima: u8,
    /// O maior número.
    pub maxima: u8,
}

impl Faixa {
    /// Uma faixa que começa em 1 ou mais e não termina antes de começar.
    ///
    /// O 0 nunca foi escrito por build nenhuma, e uma faixa ao contrário não
    /// tem número nenhum dentro: as duas são bytes de quem não está falando
    /// SEELE.
    #[must_use]
    pub const fn valida(self) -> bool {
        self.minima >= 1 && self.minima <= self.maxima
    }
}

/// A faixa desta build.
pub const FAIXA: Faixa = Faixa {
    minima: VERSAO_MINIMA,
    maxima: PROTOCOL_VERSION,
};

/// Lowest protocol version this build can still talk to: [`VERSAO_MINIMA`],
/// pelo nome de antes.
#[must_use]
pub const fn oldest_supported_version() -> u8 {
    VERSAO_MINIMA
}

/// A versão de servidor que esta build fala, como as pessoas a leem.
///
/// Duas numerações desde a 0.16 (spec-mestre, §1): a do app, que o atualizador
/// oferece, e a do servidor, que é o protocolo e a API de MODs. O app 0.16 sai
/// falando o **Servidor 1.0**, e a 0.17 o **Servidor 2.0** (ADR 0063). Um
/// conserto do `seeled` não muda este nome.
///
/// **A forma desta linha é contrato:** o `empacotar/manifesto.py` e o
/// `tools/carga-da-casca.py` a leem por expressão regular. Preso em
/// `a_versao_do_servidor_e_a_que_a_0_17_fala`.
pub const VERSAO_DO_SERVIDOR: &str = "2.0";

/// A linha de servidor de um número do fio, com o nome que o launcher usa:
/// `"0.15"` para o 8, `"1"` para o 9 e `"2"` do 10 em diante.
///
/// **Congelada:** um número nunca troca de linha. As linhas de antes da 0.15
/// (números 1 a 7) não são oferecidas pelo launcher, e por isso não têm nome.
///
/// Até 24/10/2026 a regra era «9 em diante é a linha 1», decidida em
/// 08/10/2026. Nenhum número acima de 9 foi publicado nela, e a linha 1 não
/// recebe versão nova (ADR 0063): é por isso que o 10 pôde entrar como o
/// primeiro da linha 2 sem que número nenhum trocasse de linha em campo. Uma
/// 0.16 lê um 10 como `Posterior` pelo `linha_de` dela, e nunca pede este
/// nome para ele (`seele-ffi`, `ConnectionError::OutraLinha`).
#[must_use]
pub const fn linha_do_protocolo(numero: u8) -> Option<&'static str> {
    match numero {
        8 => Some("0.15"),
        9 => Some("1"),
        10..=u8::MAX => Some("2"),
        _ => None,
    }
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

/// Se esta build lê um quadro carimbado com `carimbo`.
///
/// É a conferência do byte 0 de todo quadro, antes de qualquer outra leitura,
/// e por isso é total: todo byte tem resposta, e nenhum entra em pânico.
///
/// # Errors
///
/// [`Incompatible`], com os números de cada lado, para um carimbo fora de
/// [`FAIXA`].
///
/// # Examples
///
/// ```
/// use seele_proto::version::{confere_o_carimbo, PROTOCOL_VERSION};
///
/// assert_eq!(confere_o_carimbo(PROTOCOL_VERSION), Ok(PROTOCOL_VERSION));
/// assert!(confere_o_carimbo(PROTOCOL_VERSION.saturating_add(1)).is_err());
/// ```
pub fn confere_o_carimbo(carimbo: u8) -> Result<u8, Incompatible> {
    if carimbo > PROTOCOL_VERSION {
        return Err(Incompatible::PeerTooNew {
            peer: carimbo,
            ours: PROTOCOL_VERSION,
        });
    }
    if carimbo < VERSAO_MINIMA {
        return Err(Incompatible::PeerTooOld {
            peer: carimbo,
            oldest_supported: VERSAO_MINIMA,
        });
    }
    Ok(carimbo)
}

/// O número que os dois lados vão falar: o menor dos máximos, quando ele cabe
/// nas duas faixas.
///
/// **Quem decide é o servidor**, com esta função sobre a faixa do `Hello`; o
/// cliente adota o carimbo do primeiro quadro que recebe. Não há segunda
/// tentativa com outra faixa: a faixa já diz tudo o que o cliente fala.
///
/// # Errors
///
/// [`Incompatible::PeerTooNew`] se o par só fala acima desta build, e
/// [`Incompatible::PeerTooOld`] se só fala abaixo. Uma faixa inválida nunca
/// chega aqui: o `Hello` que a traz não passa da validação.
pub fn negociar(do_par: Faixa) -> Result<u8, Incompatible> {
    if do_par.minima > PROTOCOL_VERSION {
        return Err(Incompatible::PeerTooNew {
            peer: do_par.minima,
            ours: PROTOCOL_VERSION,
        });
    }
    if do_par.maxima < VERSAO_MINIMA {
        return Err(Incompatible::PeerTooOld {
            peer: do_par.maxima,
            oldest_supported: VERSAO_MINIMA,
        });
    }
    Ok(do_par.maxima.min(PROTOCOL_VERSION))
}

/// A linha de servidor de um número do fio.
///
/// **A tabela é congelada**: uma entrada nunca muda, e uma linha nova só se
/// acrescenta (ADR 0058). É ela que deixa «Servidor 1.0» ser o nome para
/// pessoas e «9» o que a máquina confere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linha {
    /// Antes do Servidor 1.0: do 1 ao 8. Cada subida quebrava, e o launcher só
    /// oferece a do 8, o Servidor 0.15.
    AntesDaUm {
        /// O número.
        numero: u8,
    },
    /// O Servidor 1.x: só o 9. A linha 1 não recebe versão nova (ADR 0063), e
    /// para esta build ela é anterior, como a [`Linha::AntesDaUm`]: o launcher
    /// a abre ao lado, e nenhum quadro desta build a alcança.
    Um,
    /// O Servidor 2.x: do 10 até o máximo desta build.
    Dois,
    /// Acima do que esta build fala.
    ///
    /// Como toda 2.x fala o 10, um par que **recusa** o 10 é de uma linha
    /// posterior à 2. O nome dela quem sabe é o manifesto do launcher, e não
    /// esta build.
    Posterior {
        /// O número.
        numero: u8,
    },
}

impl Linha {
    /// Se é uma linha anterior à desta build: uma que o launcher abre ao lado,
    /// pelo nome de [`linha_do_protocolo`], e não esta.
    ///
    /// É o que a casca pergunta para decidir entre «abra a versão antiga» e
    /// «atualize»: até a 0.16 só a [`Linha::AntesDaUm`] era anterior; da
    /// linha 2 em diante, a [`Linha::Um`] também é.
    #[must_use]
    pub const fn e_anterior(self) -> bool {
        matches!(self, Self::AntesDaUm { .. } | Self::Um)
    }
}

/// A linha de `numero`, ou `None` para o 0, que nenhuma build escreveu.
#[must_use]
pub const fn linha_de(numero: u8) -> Option<Linha> {
    match numero {
        0 => None,
        n if n < PRIMEIRO_DA_LINHA_UM => Some(Linha::AntesDaUm { numero: n }),
        n if n < PRIMEIRO_DA_LINHA_DOIS => Some(Linha::Um),
        n if n <= PROTOCOL_VERSION => Some(Linha::Dois),
        n => Some(Linha::Posterior { numero: n }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// **O Servidor 2.0 fala só o 10, e a 0.16 fica de fora.**
    ///
    /// É a quebra decidida em 24/10/2026 (ADR 0063): o transporte trocou, e a
    /// linha 1 inteira — o 9 — fica fora da faixa. A 0.15 (8) continua fora.
    ///
    /// Substituiu `o_servidor_1_0_fala_so_o_9_e_a_0_15_fica_de_fora`, que
    /// prendia a 1.0. Reverter `VERSAO_MINIMA` para o 9 reprova o segundo
    /// `assert!` abaixo.
    #[test]
    fn a_linha_dois_fala_dez_e_onze_e_a_0_16_fica_de_fora() {
        assert_eq!(PROTOCOL_VERSION, 11);
        assert_eq!(VERSAO_MINIMA, 10);
        assert!(
            confere_o_carimbo(8).is_err(),
            "a 0.15 foi aceita, e o decidido é que ela fique de fora"
        );
        assert!(
            confere_o_carimbo(9).is_err(),
            "a 0.16 foi aceita pelo carimbo, e a linha 2 não fala o 9 (ADR 0063)"
        );
        assert!(confere_o_carimbo(10).is_ok());
        assert!(
            confere_o_carimbo(12).is_err(),
            "um par do futuro foi aceito pelo carimbo"
        );
        assert_eq!(oldest_supported_version(), VERSAO_MINIMA);
        assert!(
            negociar(Faixa {
                minima: 9,
                maxima: 9
            })
            .is_err(),
            "a faixa da 0.16 negociou com esta build"
        );
    }

    /// **A linha 1 não recebe versão nova** (ADR 0063): o 9 é o único número
    /// dela, e o primeiro da linha 2 vem logo depois. Uma constante que
    /// deixasse um buraco entre as duas linhas daria um número sem linha.
    #[test]
    fn a_linha_1_e_so_o_9_e_a_linha_2_vem_logo_depois() {
        assert_eq!(PRIMEIRO_DA_LINHA_UM, 9, "congelado pelo ADR 0058");
        assert_eq!(PRIMEIRO_DA_LINHA_DOIS, PRIMEIRO_DA_LINHA_UM + 1);
        assert_eq!(linha_de(9), Some(Linha::Um));
        assert_eq!(linha_de(10), Some(Linha::Dois));
    }

    /// **Toda 2.x fala o primeiro número da linha.** A frase do ADR 0058, na
    /// linha 2: uma 2.x que largue o 10 deixa de conversar com a 2.0.
    #[test]
    fn toda_2x_fala_o_primeiro_numero_da_linha() {
        assert_eq!(
            VERSAO_MINIMA, PRIMEIRO_DA_LINHA_DOIS,
            "ADR 0063: uma 2.x que larga o 10 quebra o fio"
        );
        assert_eq!(linha_de(VERSAO_MINIMA), Some(Linha::Dois));
        assert_eq!(linha_de(PROTOCOL_VERSION), Some(Linha::Dois));
        assert_eq!(
            FAIXA,
            Faixa {
                minima: VERSAO_MINIMA,
                maxima: PROTOCOL_VERSION
            }
        );
        assert!(FAIXA.valida());
    }

    /// A tabela congelada: cada número tem uma linha, e a de um número acima
    /// do que esta build fala é «posterior», sem nome.
    #[test]
    fn a_linha_de_cada_numero() {
        assert_eq!(linha_de(0), None, "nenhuma build escreveu 0");
        assert_eq!(linha_de(1), Some(Linha::AntesDaUm { numero: 1 }));
        assert_eq!(
            linha_de(8),
            Some(Linha::AntesDaUm { numero: 8 }),
            "o 8 é o Servidor 0.15, a linha legada"
        );
        assert_eq!(
            linha_de(9),
            Some(Linha::Um),
            "o 9 é o Servidor 1.0, a linha legada da 0.17"
        );
        assert_eq!(linha_de(10), Some(Linha::Dois));
        assert!(Linha::AntesDaUm { numero: 8 }.e_anterior());
        assert!(Linha::Um.e_anterior(), "a linha 1 é anterior à 2");
        assert!(
            !Linha::Dois.e_anterior(),
            "a linha desta build não é anterior"
        );
        assert!(!Linha::Posterior { numero: 30 }.e_anterior());
        assert_eq!(
            linha_de(PROTOCOL_VERSION + 1),
            Some(Linha::Posterior {
                numero: PROTOCOL_VERSION + 1
            })
        );
        assert_eq!(
            linha_de(u8::MAX),
            Some(Linha::Posterior { numero: u8::MAX })
        );
    }

    /// Quem fala mais desce ao máximo do outro; faixas sem número em comum não
    /// conversam, e a recusa diz para que lado.
    #[test]
    fn quem_fala_mais_desce_ao_maximo_do_outro() {
        assert_eq!(negociar(FAIXA), Ok(PROTOCOL_VERSION));
        assert_eq!(
            negociar(Faixa {
                minima: 10,
                maxima: 13
            }),
            Ok(PROTOCOL_VERSION),
            "uma 2.3 que encontra esta build desce até ela"
        );
        assert_eq!(
            negociar(Faixa {
                minima: 9,
                maxima: 9
            }),
            Err(Incompatible::PeerTooOld {
                peer: 9,
                oldest_supported: VERSAO_MINIMA
            }),
            "a 0.16 não negocia com a linha 2"
        );
        assert_eq!(
            negociar(Faixa {
                minima: PROTOCOL_VERSION + 1,
                maxima: PROTOCOL_VERSION + 3
            }),
            Err(Incompatible::PeerTooNew {
                peer: PROTOCOL_VERSION + 1,
                ours: PROTOCOL_VERSION
            })
        );
    }

    #[test]
    fn uma_faixa_ao_contrario_ou_que_comeca_no_zero_nao_e_faixa() {
        assert!(!Faixa {
            minima: 10,
            maxima: 9
        }
        .valida());
        assert!(!Faixa {
            minima: 0,
            maxima: 9
        }
        .valida());
        assert!(Faixa {
            minima: 9,
            maxima: 9
        }
        .valida());
    }

    /// A prova de que a subida é **uma só** para as duas entregas — a malha e o
    /// anúncio de MODs —, e de que o anúncio continua alcançado pela global.
    #[test]
    fn a_api_de_pedidos_preserva_o_limiar_do_anuncio_original() {
        assert_eq!(PROTOCOL_VERSION, 11);
        assert_eq!(
            crate::mods::VERSAO_DO_ANUNCIO,
            5,
            "o anúncio de MODs deixou de pedir o limiar de sempre: ou ele \
             voltou a ficar dormente, ou passou a pedir mais do que a versão \
             global entrega"
        );
    }

    /// O nome do servidor que esta build fala, lido pelo `empacotar/manifesto.py`
    /// por expressão regular: a forma da linha é contrato com ele.
    #[test]
    fn a_versao_do_servidor_e_a_que_a_0_17_fala() {
        assert_eq!(VERSAO_DO_SERVIDOR, "2.0");
        let fonte = include_str!("version.rs");
        assert!(
            fonte.contains("pub const VERSAO_DO_SERVIDOR: &str = \"2.0\";"),
            "o manifesto.py lê esta linha por regex, e ela mudou de forma"
        );
        assert_eq!(
            linha_do_protocolo(PROTOCOL_VERSION),
            VERSAO_DO_SERVIDOR.split('.').next(),
            "o nome do servidor e a linha do número desta build discordam"
        );
    }

    /// A linha de cada número, como o launcher a nomeia.
    #[test]
    fn a_linha_de_cada_protocolo_para_o_launcher() {
        assert_eq!(linha_do_protocolo(0), None);
        assert_eq!(
            linha_do_protocolo(7),
            None,
            "as linhas 0.11 a 0.14 não são oferecidas"
        );
        assert_eq!(linha_do_protocolo(8), Some("0.15"));
        assert_eq!(linha_do_protocolo(9), Some("1"));
        assert_eq!(linha_do_protocolo(10), Some("2"));
        assert_eq!(linha_do_protocolo(u8::MAX), Some("2"));
    }

    proptest! {
        /// O primeiro byte de um socket não confiável. Ser total é o ponto.
        #[test]
        fn never_panics_for_any_byte(peer: u8) {
            let _ = confere_o_carimbo(peer);
            let _ = linha_de(peer);
        }

        /// O carimbo aceito é exatamente a faixa desta build.
        #[test]
        fn accepts_exactly_the_range(peer: u8) {
            let dentro = (VERSAO_MINIMA..=PROTOCOL_VERSION).contains(&peer);
            prop_assert_eq!(confere_o_carimbo(peer).is_ok(), dentro);
        }

        /// A recusa sempre carrega os números de que uma casca precisa para se
        /// explicar, para que nada abaixo da casca formate frase.
        #[test]
        fn refusal_carries_actionable_numbers(peer: u8) {
            match confere_o_carimbo(peer) {
                Ok(aceito) => prop_assert_eq!(aceito, peer),
                Err(Incompatible::PeerTooNew { peer: got, ours }) => {
                    prop_assert_eq!(got, peer);
                    prop_assert_eq!(ours, PROTOCOL_VERSION);
                    prop_assert!(got > ours);
                }
                Err(Incompatible::PeerTooOld { peer: got, oldest_supported }) => {
                    prop_assert_eq!(got, peer);
                    prop_assert_eq!(oldest_supported, VERSAO_MINIMA);
                    prop_assert!(got < oldest_supported);
                }
            }
        }

        /// O acordo é o maior número que as duas faixas têm em comum, ou
        /// nenhum.
        #[test]
        fn o_acordo_e_o_maior_numero_comum(minima in 1_u8.., largura in 0_u8..8) {
            let do_par = Faixa { minima, maxima: minima.saturating_add(largura) };
            let comum = (do_par.minima.max(VERSAO_MINIMA)..=do_par.maxima.min(PROTOCOL_VERSION)).last();
            prop_assert_eq!(negociar(do_par).ok(), comum);
        }

        /// Um número negociado é sempre um que esta build fala e lê no carimbo.
        #[test]
        fn o_numero_negociado_e_sempre_falado(minima in 1_u8.., largura in 0_u8..8) {
            let do_par = Faixa { minima, maxima: minima.saturating_add(largura) };
            if let Ok(acordo) = negociar(do_par) {
                prop_assert!(confere_o_carimbo(acordo).is_ok());
                prop_assert!(acordo >= do_par.minima && acordo <= do_par.maxima);
            }
        }
    }
}
