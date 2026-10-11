//! **A volta do enlace**: o buraco do histórico e o que ficou sem resposta.
//! Lote P61 da F5, pendência 61.
//!
//! # O que faltava
//!
//! Medido pela F1 (10/10): numa queda, o que esta máquina mandava no instante
//! ficava em `SemResposta` e esperava alguém apertar TENTAR DE NOVO — o texto da
//! pendência dizia que o cliente repetia sozinho, e o código não fazia. E nada
//! buscava o que passou durante a queda: o núcleo só reassina o último canal, e
//! o histórico só vinha quando a pessoa abria um canal, e só a página mais nova.
//! O que passou durante a queda e não cabia nas 50 mais novas não voltava.
//!
//! # As duas peças, puras
//!
//! [`Buraco`] e [`Repeticao`] não tocam em rede nem em relógio próprio: dizem o
//! que pedir e quando, e o laço da FFI (`drive`) faz. É o que deixa a ordem, o
//! espaçamento e a parada serem provados aqui, por teste de crate, e a prova de
//! ponta a ponta ficar para a conformidade.

use std::collections::{BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use seele_core::state::EstadoDoEnvio;
use seele_core::{ChannelId, ClientMessageId, MessageId, Room, ServerMessage};

/// A marca do `Ping` que fecha cada página do buraco: o bit 62.
///
/// **Disjunta das outras duas.** O `Ping` de rotina conta de um em um a partir
/// de 1 (`Client::send_ping`) e nunca chega a 2^62; o `Ping` marcado de entrada
/// usa o bit 63 (`seele_core::assento::MARCA_DO_PING_DE_ENTRADA`), e quem o lê
/// confere só aquele bit — um `Pong` com o 62 e sem o 63 é, para o assento, um
/// `Pong` de rotina, e não fecha entrada nenhuma.
///
/// O servidor devolve o carimbo inteiro sem lê-lo (`session.rs`, braço do
/// `Ping`), e lê o fluxo de controle em ordem: o `Pong` deste carimbo chega
/// depois da página inteira que o `FetchHistory` logo antes dele pediu.
pub(crate) const MARCA_DO_PING_DO_BURACO: u64 = 1 << 62;

/// Quantas mensagens cada página do buraco pede: o teto do servidor
/// (`MAX_PAGE`, em `seele-server`).
///
/// **É também o tamanho pelo qual a página é medida** (I1 da revisão do P61):
/// o que o servidor manda são as `PAGINA_DO_BURACO` maiores abaixo do cursor,
/// e é por elas que a busca decide até onde a página chegou. Ver
/// [`Buraco::ouvir`].
pub const PAGINA_DO_BURACO: u16 = 200;

/// Quantas páginas o buraco pede, no máximo: 2 000 mensagens.
///
/// Um teto e não «até fechar»: uma queda de cinco minutos num canal muito
/// movimentado não pode virar uma busca sem fim na volta.
///
/// **O que passa do teto não volta nesta sessão** (M1 da revisão do P61). A
/// janela não tem como pedir histórico mais antigo: abrir um canal traz só a
/// página mais nova, e não há pedido com cursor saindo da casca. O pedaço que
/// ficou de fora aparece numa sessão nova, se ela o buscar. Por isso alcançar o
/// teto vai ao `seele.log` como aviso, com o canal e o cursor de onde parou.
pub(crate) const TETO_DE_PAGINAS: u32 = 10;

/// Quanto a cerca de uma página tem para voltar (I2 da revisão do P61).
///
/// O limitador do servidor descarta quadros acima do balde, e o `Ping` da
/// cerca é um quadro como os outros. Sem prazo, uma cerca descartada deixava a
/// busca aberta e calada até a próxima queda. Cinco segundos é muito mais que
/// uma página leva numa rede de verdade, e é o bastante para o balde voltar a
/// ter fichas para o pedido de novo.
pub(crate) const PRAZO_DA_PAGINA: Duration = Duration::from_secs(5);

/// Quanto esperar antes de pedir de novo uma página que veio vazia.
///
/// Uma página vazia é o começo do canal **ou** o `FetchHistory` que o limitador
/// descartou deixando passar o `Ping`; daqui não se distingue. Pedir de novo
/// depois de um segundo — vinte fichas do balde — separa os dois: o servidor
/// que responde vazio de novo está no começo do canal.
pub(crate) const ESPERA_DA_PAGINA_VAZIA: Duration = Duration::from_secs(1);

/// Por que uma página está sendo pedida de novo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeNovo {
    /// A cerca não voltou no prazo: o `Ping` pode ter sido descartado.
    SemCerca,
    /// A página veio vazia: o `FetchHistory` pode ter sido descartado.
    Vazia,
}

/// O que pedir ao enlace: uma página atrás de `cursor`, e logo atrás o `Ping`
/// com `carimbo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pagina {
    pub(crate) canal: ChannelId,
    pub(crate) cursor: Option<MessageId>,
    pub(crate) limite: u16,
    pub(crate) carimbo: u64,
    /// `Some` quando é a mesma página pedida de novo, e por quê.
    pub(crate) de_novo: Option<DeNovo>,
}

/// Por que a busca do buraco parou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Parada {
    /// Chegou à última mensagem que esta sessão conhecia: o buraco fechou.
    Fechou,
    /// O começo do canal, **dito pelo servidor**: uma página que veio
    /// incompleta, ou vazia duas vezes, a segunda depois de
    /// [`ESPERA_DA_PAGINA_VAZIA`].
    Comeco,
    /// Esta sessão não conhecia mensagem nenhuma do canal: a página mais nova
    /// basta, como quando a pessoa abre o canal.
    SoAMaisNova,
    /// [`TETO_DE_PAGINAS`] páginas e o buraco ainda aberto.
    Teto,
    /// A cerca da mesma página não voltou duas vezes no prazo: a busca
    /// desiste, e diz isso.
    SemResposta,
}

/// O que o laço faz agora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Passo {
    /// Pede uma página.
    Pedir(Pagina),
    /// A busca acabou.
    Parou(Parada),
}

/// A página que está no ar, e o que chegou dela até agora.
#[derive(Debug, Clone)]
struct NoAr {
    cursor: Option<MessageId>,
    carimbo: u64,
    /// Até quando a cerca dela tem para voltar.
    prazo: Instant,
    /// Os ids deste canal, abaixo do cursor, que chegaram desde o pedido.
    ///
    /// **Todos, e não só o menor** (I1 da revisão do P61). Na janela de uma
    /// página chega a página e chega o que é ao vivo — e o ao vivo inclui o
    /// eco de uma linha **antiga**: quem repete uma mensagem que o servidor já
    /// tinha gravado faz o servidor anunciar a linha original, com o id dela, ao
    /// canal inteiro. Guardar só o menor o tomava pela «mais antiga da página»,
    /// e a busca pulava para trás dele e fechava com o buraco pela metade.
    ids: BTreeSet<MessageId>,
}

impl NoAr {
    /// Até onde a página chegou: a menor das [`PAGINA_DO_BURACO`] maiores.
    ///
    /// O servidor manda as `PAGINA_DO_BURACO` maiores abaixo do cursor. Com a
    /// página cheia, um id a mais na janela, abaixo delas, é um eco antigo e
    /// fica de fora; com ela incompleta, o servidor chegou ao começo do canal e
    /// mandou tudo o que havia abaixo do cursor — um eco antigo é uma delas.
    /// Nos dois casos o resultado nunca é **mais antigo** que a página de
    /// verdade, e um cursor mais novo que o devido só pede de novo o que já
    /// chegou: o `Room` não duplica.
    fn ate_onde(&self) -> Option<MessageId> {
        self.ids
            .iter()
            .rev()
            .nth(usize::from(PAGINA_DO_BURACO) - 1)
            .or_else(|| self.ids.first())
            .copied()
    }

    /// Se a página veio cheia: o servidor tinha mais abaixo do cursor.
    fn cheia(&self) -> bool {
        self.ids.len() >= usize::from(PAGINA_DO_BURACO)
    }
}

/// A busca do buraco de um canal, página por página, de trás para a frente.
///
/// O fio só pagina para trás (`FetchHistory` traz as mais antigas que o
/// cursor, a partir da mais nova), então o buraco é fechado da mais nova para a
/// última conhecida. O `Room` guarda as mensagens por id e as ordena, então a
/// ordem de chegada não importa, e o que já estava na tela não duplica.
///
/// # O relógio
///
/// A busca não tem relógio próprio: quem chama passa o `agora`, e pergunta a
/// [`Self::quando`] até quando dormir. Quando esse instante chega,
/// [`Self::vez`] diz o que fazer: pedir a página que esperava, ou tratar a
/// cerca que não voltou.
#[derive(Debug)]
pub(crate) struct Buraco {
    canal: ChannelId,
    /// A última mensagem deste canal que esta sessão conhecia na queda.
    conhecida: Option<MessageId>,
    /// Quantas páginas já foram pedidas, sem contar as pedidas de novo.
    paginas: u32,
    /// A página no ar, se há uma.
    no_ar: Option<NoAr>,
    /// A página que espera a vez de ser pedida de novo: quando, de que cursor,
    /// e por quê.
    agendada: Option<(Instant, Option<MessageId>, DeNovo)>,
    /// Se a página de agora já foi pedida de novo porque a cerca não voltou.
    /// Uma vez por página.
    ///
    /// **Separado das respostas vazias** (R1-M1 da revisão do P61): as duas
    /// razões de pedir de novo dividiam um indicador só, e uma cerca perdida
    /// seguida de uma página vazia contava como duas respostas vazias — a busca
    /// parava em «começo do canal» com uma resposta só.
    sem_cerca_de_novo: bool,
    /// Quantas vezes o servidor respondeu à página de agora sem nada abaixo do
    /// cursor. Duas são o começo do canal.
    vazias: u8,
    /// O próximo número de carimbo. Ver [`MARCA_DO_PING_DO_BURACO`].
    sequencia: u64,
}

impl Buraco {
    /// Começa a busca do canal aberto, a partir do que o `Room` sabe dele.
    ///
    /// `sequencia` é de onde os carimbos desta busca contam: quem chama a
    /// carrega de uma busca para a outra, para um `Pong` velho não fechar uma
    /// página nova.
    pub(crate) fn abrir(room: &Room, sequencia: u64, agora: Instant) -> Option<(Self, Pagina)> {
        let canal = room.current_channel?;
        let conhecida = room.mensagens_do_canal(canal).last().map(|m| m.id);
        let mut buraco = Self {
            canal,
            conhecida,
            paginas: 1,
            no_ar: None,
            agendada: None,
            sem_cerca_de_novo: false,
            vazias: 0,
            sequencia,
        };
        let pagina = buraco.pedir(None, None, agora);
        Some((buraco, pagina))
    }

    /// Põe uma página no ar.
    fn pedir(
        &mut self,
        cursor: Option<MessageId>,
        de_novo: Option<DeNovo>,
        agora: Instant,
    ) -> Pagina {
        let carimbo = MARCA_DO_PING_DO_BURACO | self.sequencia;
        self.sequencia += 1;
        self.no_ar = Some(NoAr {
            cursor,
            carimbo,
            prazo: agora + PRAZO_DA_PAGINA,
            ids: BTreeSet::new(),
        });
        Pagina {
            canal: self.canal,
            cursor,
            limite: PAGINA_DO_BURACO,
            carimbo,
            de_novo,
        }
    }

    /// De onde a próxima busca conta os carimbos.
    pub(crate) const fn sequencia(&self) -> u64 {
        self.sequencia
    }

    /// O canal desta busca.
    pub(crate) const fn canal(&self) -> ChannelId {
        self.canal
    }

    /// Quantas páginas já foram pedidas.
    pub(crate) const fn paginas(&self) -> u32 {
        self.paginas
    }

    /// O cursor da página no ar, ou da que espera: de onde a busca parou, se
    /// parar agora.
    pub(crate) fn cursor(&self) -> Option<MessageId> {
        self.no_ar
            .as_ref()
            .and_then(|no_ar| no_ar.cursor)
            .or_else(|| self.agendada.and_then(|(_, cursor, _)| cursor))
    }

    /// Até quando dormir: a vez da página que espera, ou o prazo da cerca da
    /// página no ar.
    pub(crate) fn quando(&self) -> Option<Instant> {
        self.agendada
            .map(|(quando, _, _)| quando)
            .or_else(|| self.no_ar.as_ref().map(|no_ar| no_ar.prazo))
    }

    /// O instante de [`Self::quando`] chegou: pede a página que esperava, ou
    /// trata a cerca que não voltou.
    pub(crate) fn vez(&mut self, agora: Instant) -> Option<Passo> {
        if let Some((quando, cursor, por_que)) = self.agendada {
            if agora < quando {
                return None;
            }
            self.agendada = None;
            return Some(Passo::Pedir(self.pedir(cursor, Some(por_que), agora)));
        }
        let no_ar = self.no_ar.as_ref()?;
        if agora < no_ar.prazo {
            return None;
        }
        // A cerca não voltou: o limitador pode ter descartado o `Ping`. Uma
        // vez a mesma página de novo; na segunda, a busca desiste e diz.
        let cursor = no_ar.cursor;
        self.no_ar = None;
        if self.sem_cerca_de_novo {
            return Some(Passo::Parou(Parada::SemResposta));
        }
        self.sem_cerca_de_novo = true;
        Some(Passo::Pedir(self.pedir(
            cursor,
            Some(DeNovo::SemCerca),
            agora,
        )))
    }

    /// Uma mensagem do servidor chegou. Devolve o passo seguinte quando ela é
    /// o `Pong` da página no ar e há o que fazer agora.
    ///
    /// Uma mensagem do canal abaixo do cursor entra na conta da página, venha
    /// ela da página ou ao vivo: daqui não se distingue uma da outra. Quem
    /// separa é [`NoAr::ate_onde`], pelo tamanho da página.
    pub(crate) fn ouvir(&mut self, mensagem: &ServerMessage, agora: Instant) -> Option<Passo> {
        let no_ar = self.no_ar.as_mut()?;
        match *mensagem {
            ServerMessage::MessageReceived { channel, id, .. } if channel == self.canal => {
                if no_ar.cursor.is_none_or(|cursor| id < cursor) {
                    no_ar.ids.insert(id);
                }
                None
            }
            ServerMessage::Pong { timestamp } if timestamp == no_ar.carimbo => {
                let no_ar = self.no_ar.take()?;
                self.decidir(&no_ar, agora)
            }
            _ => None,
        }
    }

    /// A página no ar chegou inteira: fechou, pede a seguinte, ou espera para
    /// pedir a mesma de novo.
    fn decidir(&mut self, no_ar: &NoAr, agora: Instant) -> Option<Passo> {
        let Some(menor) = no_ar.ate_onde() else {
            // **Vazia não é começo, sozinha** (I2 da revisão do P61). O começo
            // do canal e o `FetchHistory` descartado pelo limitador, com o
            // `Ping` passando, dão a mesma página vazia. Pede de novo depois de
            // uma espera; o servidor que responde vazio de novo está no começo.
            self.vazias += 1;
            if self.vazias >= 2 {
                return Some(Passo::Parou(Parada::Comeco));
            }
            self.agendada = Some((agora + ESPERA_DA_PAGINA_VAZIA, no_ar.cursor, DeNovo::Vazia));
            return None;
        };
        self.sem_cerca_de_novo = false;
        self.vazias = 0;
        let Some(conhecida) = self.conhecida else {
            return Some(Passo::Parou(Parada::SoAMaisNova));
        };
        if menor <= conhecida {
            return Some(Passo::Parou(Parada::Fechou));
        }
        // Incompleta: o servidor respondeu, e não tinha mais abaixo do cursor.
        if !no_ar.cheia() {
            return Some(Passo::Parou(Parada::Comeco));
        }
        if self.paginas >= TETO_DE_PAGINAS {
            return Some(Passo::Parou(Parada::Teto));
        }
        self.paginas += 1;
        Some(Passo::Pedir(self.pedir(Some(menor), None, agora)))
    }
}

/// Faz o que a busca mandou: pede a página, ou registra a parada.
///
/// Aqui e não no laço da FFI, para os dois braços que acordam a busca — a
/// mensagem que chegou e o relógio dela — fazerem a mesma coisa.
pub(crate) async fn seguir(
    enlace: &seele_core::enlace::Enlace,
    buraco: &mut Option<Buraco>,
    carimbos: &mut u64,
    passo: Passo,
) {
    match passo {
        Passo::Pedir(pagina) => {
            if let Some(por_que) = pagina.de_novo {
                tracing::warn!(
                    canal = pagina.canal.get(),
                    cursor = pagina.cursor.map(MessageId::get),
                    ?por_que,
                    "a página da busca do buraco não teve resposta; pedindo de novo \
                     (o limitador do servidor pode ter descartado o pedido ou a cerca)"
                );
            }
            let _ = enlace
                .historico_com_cerca(pagina.canal, pagina.cursor, pagina.limite, pagina.carimbo)
                .await;
        }
        Passo::Parou(parada) => {
            let Some(parado) = buraco.take() else {
                return;
            };
            *carimbos = parado.sequencia();
            let (canal, paginas, cursor) = (
                parado.canal().get(),
                parado.paginas(),
                parado.cursor().map(MessageId::get),
            );
            match parada {
                Parada::Teto => tracing::warn!(
                    canal,
                    paginas,
                    cursor,
                    "a busca do buraco alcançou o teto de páginas; o que é mais antigo que o \
                     cursor não volta nesta sessão"
                ),
                Parada::SemResposta => tracing::warn!(
                    canal,
                    paginas,
                    cursor,
                    "a busca do buraco desistiu: a cerca da mesma página não voltou duas vezes \
                     no prazo; o que é mais antigo que o cursor não volta nesta sessão"
                ),
                // **O começo do canal sem a última conhecida** (R1-M3 da revisão
                // do P61): ou ela foi apagada durante a queda, ou o servidor
                // mandou páginas menores que `PAGINA_DO_BURACO` e a busca as
                // leu como o começo. O segundo caso é o que a conformidade
                // prende (`a_pagina_da_busca_e_a_pagina_do_servidor`); se um dia
                // acontecer, é aqui que aparece.
                Parada::Comeco => tracing::warn!(
                    canal,
                    paginas,
                    cursor,
                    "a busca do buraco chegou ao começo do canal sem achar a última mensagem que \
                     esta sessão conhecia: ela foi apagada, ou o servidor mandou páginas menores \
                     que as pedidas"
                ),
                Parada::Fechou | Parada::SoAMaisNova => {
                    tracing::info!(canal, paginas, ?parada, "o buraco da volta fechou");
                }
            }
        }
    }
}

/// Quantas repetições saem de uma vez na volta.
///
/// O balde do servidor deixa 60 quadros de rajada numa conexão nova
/// (`QUADROS_DE_RAJADA`, em `seele-server`), e acima dele **descarta calado**:
/// uma repetição descartada seria a mensagem sumindo de novo, por outra porta.
/// A volta já gasta alguns — a entrada na sala, o `Ping` dela, o canal, o mudo,
/// e duas por página do buraco —, e 20 deixa folga para isso e para quem
/// escreve na hora.
pub(crate) const RAJADA_DA_VOLTA: usize = 20;

/// O intervalo entre as repetições depois da rajada: 8 por segundo, menos da
/// metade dos 20 sustentados que o balde deixa (`QUADROS_POR_SEGUNDO`).
pub(crate) const INTERVALO_DA_REPETICAO: Duration = Duration::from_millis(125);

/// As pendentes que a volta repete, na ordem em que foram escritas, e o
/// compasso em que saem.
#[derive(Debug, Default)]
pub(crate) struct Repeticao {
    fila: VecDeque<ClientMessageId>,
    /// Quantas ainda saem sem esperar.
    rajada: usize,
    /// Quando a próxima sai, depois da rajada.
    proxima: Option<Instant>,
}

impl Repeticao {
    /// A volta: toda pendente que ficou sem resposta — ou que saiu por um
    /// enlace que já tinha caído — entra na fila, na ordem do `Room`, que é a
    /// ordem em que foram escritas.
    ///
    /// `Enviando` entra junto com `SemResposta`: uma mensagem escrita durante a
    /// bateria foi entregue a um motor sem conexão, que não a guarda, e só vira
    /// `SemResposta` na tica seguinte. Repetir uma que na verdade saiu pela
    /// conexão nova é seguro — a chave é a mesma, e o servidor devolve a
    /// original.
    pub(crate) fn na_volta(&mut self, room: &Room, agora: Instant) {
        self.fila = room
            .pendentes
            .iter()
            .filter(|pendente| {
                matches!(
                    pendente.estado,
                    EstadoDoEnvio::SemResposta | EstadoDoEnvio::Enviando
                )
            })
            .map(|pendente| pendente.client_message_id)
            .collect();
        self.rajada = RAJADA_DA_VOLTA;
        self.proxima = Some(agora);
    }

    /// O enlace caiu de novo: o que estava na fila espera a próxima volta, e
    /// sai de lá a partir do `Room`, como desta vez.
    pub(crate) fn esquecer(&mut self) {
        *self = Self::default();
    }

    /// Quando a próxima vez chega, se há alguma na fila.
    pub(crate) fn quando(&self) -> Option<Instant> {
        if self.fila.is_empty() {
            None
        } else {
            self.proxima
        }
    }

    /// As que saem agora, na ordem.
    pub(crate) fn vez(&mut self, agora: Instant) -> Vec<ClientMessageId> {
        if self.proxima.is_none_or(|proxima| agora < proxima) {
            return Vec::new();
        }
        let quantas = self.rajada.max(1).min(self.fila.len());
        self.rajada = 0;
        self.proxima = Some(agora + INTERVALO_DA_REPETICAO);
        self.fila.drain(..quantas).collect()
    }
}

/// Dorme até `quando`. Sem ele, nunca volta: é o braço do `select!` de
/// `drive` que nunca é escolhido, como `PortaDaVoz::chegada`.
pub(crate) async fn esperar_ate(quando: Option<Instant>) {
    match quando {
        Some(quando) => tokio::time::sleep_until(quando.into()).await,
        None => std::future::pending().await,
    }
}

/// Se um corpo passa do teto do fio, a recusa que ele recebe.
///
/// **Uma conferência só**, para o envio, para o TENTAR DE NOVO e para a
/// repetição da volta. O teto é de **bytes** (`seele_core::MAX_BODY_LEN`), e um
/// corpo acima dele não é recusado pelo servidor: o quadro se recusa a ser
/// codificado, e o núcleo trata isso como enlace caído. Uma pendente grande
/// demais que chegasse à repetição derrubaria o enlace, voltaria a
/// `SemResposta`, seria repetida na volta seguinte e derrubaria de novo — para
/// sempre.
pub(crate) fn grande_demais(corpo: &str) -> Option<seele_core::MessageRefusal> {
    if corpo.len() <= seele_core::MAX_BODY_LEN {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "o teto é 4 KiB e cabe folgado num u32"
    )]
    let limite = seele_core::MAX_BODY_LEN as u32;
    Some(seele_core::MessageRefusal::TooLong { limit: limite })
}

/// O que fazer com uma pendente que a volta repete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Repetir {
    /// Manda, com **a mesma chave**, por este canal e com este corpo. Ela já
    /// está de novo em `Enviando`.
    Mandar {
        canal: ChannelId,
        corpo: String,
        chave: ClientMessageId,
    },
    /// Passa do teto do fio: foi recusada aqui, com a frase dela, e não sai.
    Recusada,
    /// Não há o que repetir: o servidor já a confirmou (o eco, ou o buraco,
    /// trouxe a original), ou recusou, ou quem escreveu desistiu dela.
    Nada,
}

/// Prepara a repetição de uma pendente da fila, no `Room`.
pub(crate) fn repetir(room: &mut Room, chave: ClientMessageId, agora_em_segundos: i64) -> Repetir {
    let Some(pendente) = room
        .pendentes
        .iter()
        .find(|pendente| pendente.client_message_id == chave)
    else {
        return Repetir::Nada;
    };
    if !matches!(
        pendente.estado,
        EstadoDoEnvio::SemResposta | EstadoDoEnvio::Enviando
    ) {
        return Repetir::Nada;
    }
    let (canal, corpo) = (pendente.channel, pendente.body.clone());
    if let Some(recusa) = grande_demais(&corpo) {
        room.recusar_envio(chave, recusa);
        return Repetir::Recusada;
    }
    room.anotar_envio(canal, chave, corpo.clone(), agora_em_segundos);
    Repetir::Mandar {
        canal,
        corpo,
        chave,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_core::{PersonId, SessionId, Ssrc};

    const CANAL: ChannelId = ChannelId(1);
    const EU: PersonId = PersonId(7);

    fn room() -> Room {
        let mut room = Room::new();
        room.adopt(
            &seele_core::SessionInfo {
                id: SessionId(1),
                person: EU,
                ssrc: Ssrc(70),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                permissions: Vec::new(),
            },
            "ana",
        );
        room.open_channel(CANAL);
        room
    }

    fn mensagem(id: u64, autor: PersonId, chave: Option<ClientMessageId>) -> ServerMessage {
        ServerMessage::MessageReceived {
            channel: CANAL,
            id: MessageId(id),
            author: autor,
            at_seconds: 0,
            author_nickname: "alguém".into(),
            body: format!("mensagem {id}"),
            replies_to: None,
            client_message_id: chave,
            attachment: None,
        }
    }

    fn pong(carimbo: u64) -> ServerMessage {
        ServerMessage::Pong { timestamp: carimbo }
    }

    /// O servidor de mentira destes testes: as mensagens do canal, e a página
    /// que o `FetchHistory` daria — as mais antigas que o cursor, até o
    /// limite, da mais antiga para a mais nova.
    fn pagina(gravadas: &[u64], cursor: Option<MessageId>, limite: u16) -> Vec<u64> {
        let mut antes: Vec<u64> = gravadas
            .iter()
            .copied()
            .filter(|id| cursor.is_none_or(|cursor| *id < cursor.get()))
            .collect();
        antes.sort_unstable();
        let corte = antes.len().saturating_sub(usize::from(limite));
        antes.split_off(corte)
    }

    /// Roda uma busca inteira contra o servidor de mentira, com `ao_vivo`
    /// chegando no meio da primeira página. Devolve a parada e as páginas.
    fn buscar(room: &mut Room, gravadas: &[u64], ao_vivo: &[u64]) -> (Parada, u32) {
        buscar_com_ecos(room, gravadas, ao_vivo, &[])
    }

    /// [`buscar`], com `ecos`: `(pedido, id)` é um eco ao vivo de uma linha
    /// **antiga** — a repetição de alguém, com a mesma chave, que o servidor
    /// devolve com o id original a todo o canal — chegando na janela daquele
    /// pedido (o primeiro é o 1), depois do bloco dele e antes da cerca.
    fn buscar_com_ecos(
        room: &mut Room,
        gravadas: &[u64],
        ao_vivo: &[u64],
        ecos: &[(u32, u64)],
    ) -> (Parada, u32) {
        let (parada, paginas, _) = simular(room, gravadas, ao_vivo, ecos, &[]);
        (parada, paginas)
    }

    /// O que o limitador do servidor descarta de um pedido da busca.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Descarte {
        /// O `FetchHistory`: a cerca volta sem página nenhuma.
        Pedido,
        /// O `Ping`: a página chega, e a cerca nunca.
        Cerca,
    }

    /// A busca inteira contra o servidor de mentira, com o relógio andando só
    /// quando ela espera ([`Buraco::quando`]). `descartes` diz, por número de
    /// pedido (o primeiro é o 1, e um pedido de novo conta), o que o limitador
    /// descarta. Devolve a parada, as páginas e cada pedido que saiu.
    fn simular(
        room: &mut Room,
        gravadas: &[u64],
        ao_vivo: &[u64],
        ecos: &[(u32, u64)],
        descartes: &[(u32, Descarte)],
    ) -> (Parada, u32, Vec<Pagina>) {
        let mut agora = Instant::now();
        let (mut buraco, mut pedido) = Buraco::abrir(room, 1, agora).expect("há canal aberto");
        let mut pedidos = vec![pedido];
        let mut numero = 0_u32;
        loop {
            numero += 1;
            let descarte = descartes
                .iter()
                .find(|(qual, _)| *qual == numero)
                .map(|(_, descarte)| *descarte);
            let mut chegam: Vec<ServerMessage> = Vec::new();
            if numero == 1 {
                chegam.extend(ao_vivo.iter().map(|id| mensagem(*id, PersonId(9), None)));
            }
            if descarte != Some(Descarte::Pedido) {
                chegam.extend(
                    pagina(gravadas, pedido.cursor, pedido.limite)
                        .into_iter()
                        .map(|id| mensagem(id, PersonId(9), None)),
                );
            }
            chegam.extend(
                ecos.iter()
                    .filter(|(qual, _)| *qual == numero)
                    .map(|(_, id)| mensagem(*id, PersonId(9), None)),
            );
            if descarte != Some(Descarte::Cerca) {
                chegam.push(pong(pedido.carimbo));
            }
            let mut passo = None;
            for chegou in &chegam {
                room.apply(chegou);
                if let Some(depois) = buraco.ouvir(chegou, agora) {
                    passo = Some(depois);
                }
            }
            // Nada a fazer agora: a busca espera. O relógio anda até a vez dela.
            let passo = match passo {
                Some(passo) => passo,
                None => {
                    agora = buraco
                        .quando()
                        .expect("a busca não pediu, não parou e não espera nada: ficou calada");
                    buraco
                        .vez(agora)
                        .expect("a vez da busca chegou e ela não disse nada")
                }
            };
            match passo {
                Passo::Pedir(proximo) => {
                    assert_eq!(proximo.canal, CANAL);
                    assert_ne!(proximo.carimbo & MARCA_DO_PING_DO_BURACO, 0);
                    assert_eq!(
                        proximo.carimbo >> 63,
                        0,
                        "o carimbo do buraco pegou o bit da entrada"
                    );
                    pedidos.push(proximo);
                    pedido = proximo;
                }
                Passo::Parou(parada) => return (parada, buraco.paginas(), pedidos),
            }
        }
    }

    fn ids(room: &Room) -> Vec<u64> {
        room.mensagens_do_canal(CANAL)
            .iter()
            .map(|m| m.id.get())
            .collect()
    }

    #[test]
    fn o_buraco_fecha_na_ultima_que_esta_sessao_conhecia() {
        // Esta sessão conhecia até a 100; durante a queda passaram 450.
        let mut room = room();
        for id in 51..=100 {
            room.apply(&mensagem(id, PersonId(9), None));
        }
        let gravadas: Vec<u64> = (1..=550).collect();
        let (parada, paginas) = buscar(&mut room, &gravadas, &[]);

        assert_eq!(parada, Parada::Fechou);
        assert_eq!(paginas, 3, "450 que faltavam são três páginas de 200");
        // A última página passa da última conhecida e traz as de antes dela
        // também: o que já estava na tela chega de novo, e não duplica.
        assert_eq!(
            ids(&room),
            (1..=550).collect::<Vec<_>>(),
            "o buraco não fechou, ou o que já estava na tela duplicou"
        );
    }

    /// **O eco de uma linha antiga no meio da busca não a fecha antes do
    /// buraco** (I1 da revisão do P61).
    ///
    /// Quem repete uma mensagem que o servidor já tinha gravado recebe — e o
    /// canal inteiro recebe — a linha original, com o **id antigo**. Esse eco
    /// chega ao vivo, na janela de uma página, e era tomado pela «mais antiga
    /// da página»: a busca pulava para trás dele e dizia que fechou. Medido
    /// pela revisão: faltavam 50 aqui, e 300 de 1 500 pela FFI.
    #[test]
    fn o_eco_de_uma_linha_antiga_nao_fecha_a_busca_antes_do_buraco() {
        // Ela conhecia até a 100; a dela foi gravada como 101 e o eco se
        // perdeu; 450 de outra pessoa passaram durante a queda. O eco da
        // repetição dela (id 101) chega na janela da segunda página.
        let mut room = room();
        for id in 51..=100 {
            room.apply(&mensagem(id, PersonId(9), None));
        }
        let gravadas: Vec<u64> = (1..=551).collect();
        let (parada, _) = buscar_com_ecos(&mut room, &gravadas, &[], &[(2, 101)]);

        assert_eq!(parada, Parada::Fechou);
        let faltam: Vec<u64> = (51..=551).filter(|id| !ids(&room).contains(id)).collect();
        assert!(
            faltam.is_empty(),
            "a busca disse que fechou e faltam {} do buraco: {:?}…",
            faltam.len(),
            faltam.iter().take(5).collect::<Vec<_>>()
        );

        // E um eco antigo na primeira página, abaixo da última conhecida —
        // fecharia na hora.
        let mut room = self::room();
        for id in 51..=100 {
            room.apply(&mensagem(id, PersonId(9), None));
        }
        let (parada, _) = buscar_com_ecos(&mut room, &gravadas, &[], &[(1, 60)]);
        assert_eq!(parada, Parada::Fechou);
        let faltam: Vec<u64> = (51..=551).filter(|id| !ids(&room).contains(id)).collect();
        assert!(
            faltam.is_empty(),
            "um eco de id abaixo da última conhecida fechou a busca na primeira página, \
             e faltam {}",
            faltam.len()
        );
    }

    #[test]
    fn a_que_chega_ao_vivo_no_meio_nao_engana_a_parada() {
        // Um buraco de duas páginas, e mensagens novas chegando ao vivo junto
        // com a primeira. Elas são mais novas que tudo da página e não podem
        // passar por «a mais antiga que chegou».
        let mut room = room();
        room.apply(&mensagem(10, PersonId(9), None));
        let gravadas: Vec<u64> = (1..=300).collect();
        let (parada, paginas) = buscar(&mut room, &gravadas, &[301, 302]);

        assert_eq!(parada, Parada::Fechou);
        assert_eq!(paginas, 2);
        assert_eq!(ids(&room), (1..=302).collect::<Vec<_>>());
    }

    #[test]
    fn a_busca_para_no_comeco_do_canal_e_no_teto() {
        // A última conhecida foi apagada durante a queda, e o canal tem menos
        // que uma página: a página seguinte vem vazia, e é o começo.
        let mut room = room();
        room.apply(&mensagem(5, PersonId(9), None));
        let gravadas: Vec<u64> = (6..=150).collect();
        assert_eq!(buscar(&mut room, &gravadas, &[]).0, Parada::Comeco);

        // Um buraco maior que o teto para no teto, com as 2 000 mais novas.
        let mut room = self::room();
        room.apply(&mensagem(1, PersonId(9), None));
        let gravadas: Vec<u64> = (1..=5_000).collect();
        let (parada, paginas) = buscar(&mut room, &gravadas, &[]);
        assert_eq!(parada, Parada::Teto);
        assert_eq!(paginas, TETO_DE_PAGINAS);
        assert_eq!(
            room.mensagens_do_canal(CANAL).len(),
            1 + usize::from(PAGINA_DO_BURACO) * usize::try_from(TETO_DE_PAGINAS).unwrap()
        );

        // Quem não conhecia nada do canal recebe a página mais nova e para.
        let mut room = self::room();
        assert_eq!(buscar(&mut room, &gravadas, &[]), (Parada::SoAMaisNova, 1));
    }

    /// **Uma página vazia, sozinha, não é o começo do canal** (I2 da revisão do
    /// P61).
    ///
    /// O limitador do servidor descarta quadros acima do balde. Se ele descarta
    /// o `FetchHistory` e deixa passar o `Ping`, a cerca volta sem página
    /// nenhuma — e a busca lia «começo do canal» e parava, com o buraco inteiro
    /// de fora e o `seele.log` dizendo só «fechou».
    #[test]
    fn uma_pagina_vazia_sozinha_nao_e_o_comeco_do_canal() {
        let mut room = room();
        room.apply(&mensagem(1, PersonId(9), None));
        let agora = Instant::now();
        let (mut buraco, pedido) = Buraco::abrir(&room, 1, agora).expect("canal aberto");
        // O pedido não chegou ao servidor; a cerca, sim.
        let passo = buraco.ouvir(&pong(pedido.carimbo), agora);
        assert_ne!(
            passo,
            Some(Passo::Parou(Parada::Comeco)),
            "a busca leu «começo do canal» numa página que o servidor não respondeu"
        );
        // Ela espera, e pede a mesma página de novo, dizendo por quê.
        assert_eq!(buraco.quando(), Some(agora + ESPERA_DA_PAGINA_VAZIA));
        assert_eq!(buraco.vez(agora), None, "pediu de novo antes da espera");
        let Some(Passo::Pedir(de_novo)) = buraco.vez(agora + ESPERA_DA_PAGINA_VAZIA) else {
            panic!("a busca não pediu de novo a página vazia");
        };
        assert_eq!(
            (de_novo.cursor, de_novo.de_novo),
            (pedido.cursor, Some(DeNovo::Vazia))
        );
    }

    /// **O `FetchHistory` descartado, com o `Ping` passando, não deixa o
    /// buraco pela metade** (I2): a página vazia é pedida de novo depois da
    /// espera, e a busca fecha o buraco inteiro.
    #[test]
    fn o_pedido_descartado_e_pedido_de_novo_e_o_buraco_fecha_inteiro() {
        let mut room = room();
        for id in 51..=100 {
            room.apply(&mensagem(id, PersonId(9), None));
        }
        let gravadas: Vec<u64> = (1..=551).collect();
        let (parada, paginas, pedidos) =
            simular(&mut room, &gravadas, &[], &[], &[(2, Descarte::Pedido)]);

        assert_eq!(parada, Parada::Fechou);
        assert_eq!(
            paginas, 3,
            "a página pedida de novo contou como página nova"
        );
        assert_eq!(
            pedidos
                .iter()
                .filter(|p| p.de_novo == Some(DeNovo::Vazia))
                .count(),
            1
        );
        let faltam: Vec<u64> = (51..=551).filter(|id| !ids(&room).contains(id)).collect();
        assert!(faltam.is_empty(), "faltam {} do buraco", faltam.len());
    }

    /// **A cerca descartada tem prazo** (I2): sem o `Pong`, a busca ficava
    /// aberta e calada até a próxima queda. Agora, no prazo, ela pede a mesma
    /// página de novo; e se a cerca não volta de novo, desiste dizendo.
    #[test]
    fn a_cerca_descartada_tem_prazo_e_a_pagina_e_pedida_de_novo() {
        let mut room = room();
        for id in 51..=100 {
            room.apply(&mensagem(id, PersonId(9), None));
        }
        let gravadas: Vec<u64> = (1..=551).collect();
        let (parada, paginas, pedidos) =
            simular(&mut room, &gravadas, &[], &[], &[(2, Descarte::Cerca)]);
        assert_eq!(parada, Parada::Fechou);
        assert_eq!(paginas, 3);
        assert_eq!(
            pedidos
                .iter()
                .filter(|p| p.de_novo == Some(DeNovo::SemCerca))
                .count(),
            1
        );
        let faltam: Vec<u64> = (51..=551).filter(|id| !ids(&room).contains(id)).collect();
        assert!(faltam.is_empty(), "faltam {} do buraco", faltam.len());

        // Duas vezes a mesma cerca: a busca desiste, e diz que desistiu.
        let mut room = self::room();
        for id in 51..=100 {
            room.apply(&mensagem(id, PersonId(9), None));
        }
        let (parada, _, _) = simular(
            &mut room,
            &gravadas,
            &[],
            &[],
            &[(2, Descarte::Cerca), (3, Descarte::Cerca)],
        );
        assert_eq!(parada, Parada::SemResposta);
    }

    /// **Uma cerca perdida não conta como a primeira resposta vazia** (R1-M1
    /// da revisão do P61).
    ///
    /// A sequência do revisor: a cerca da página 2 se perde, a página é pedida
    /// de novo, e esse pedido de novo é o que o limitador descarta — a cerca
    /// volta, sem página. Era a primeira resposta vazia do servidor, e a busca
    /// a contava como a segunda, porque o mesmo indicador marcava as duas
    /// razões de pedir de novo: parava em «começo do canal», com 51 faltando.
    #[test]
    fn uma_cerca_perdida_nao_conta_como_resposta_vazia() {
        let mut room = room();
        for id in 51..=100 {
            room.apply(&mensagem(id, PersonId(9), None));
        }
        let gravadas: Vec<u64> = (1..=551).collect();
        let (parada, _, pedidos) = simular(
            &mut room,
            &gravadas,
            &[],
            &[],
            &[(2, Descarte::Cerca), (3, Descarte::Pedido)],
        );
        let faltam: Vec<u64> = (51..=551).filter(|id| !ids(&room).contains(id)).collect();
        assert_eq!(
            (parada, faltam.len()),
            (Parada::Fechou, 0),
            "uma cerca perdida e uma página vazia pararam a busca: {:?}",
            pedidos
                .iter()
                .map(|p| (p.cursor, p.de_novo))
                .collect::<Vec<_>>()
        );
    }

    /// O começo do canal de verdade, numa divisa de página: a última página
    /// cheia é seguida de uma vazia, e a vazia, pedida de novo, vem vazia de
    /// novo — o servidor disse duas vezes.
    #[test]
    fn o_comeco_numa_divisa_de_pagina_e_dito_duas_vezes() {
        let mut room = room();
        // A última conhecida foi apagada durante a queda; o canal tem duas
        // páginas inteiras depois dela.
        room.apply(&mensagem(5, PersonId(9), None));
        let gravadas: Vec<u64> = (6..=405).collect();
        let (parada, paginas, pedidos) = simular(&mut room, &gravadas, &[], &[], &[]);
        assert_eq!(parada, Parada::Comeco);
        assert_eq!(paginas, 3);
        assert_eq!(
            pedidos
                .iter()
                .filter(|p| p.de_novo == Some(DeNovo::Vazia))
                .count(),
            1
        );
    }

    #[test]
    fn o_pong_de_outra_cerca_nao_fecha_a_pagina() {
        // O `Pong` de rotina, o do `Ping` de entrada e o de uma busca velha
        // chegam no meio, e nenhum fecha a página no ar.
        let mut room = room();
        room.apply(&mensagem(1, PersonId(9), None));
        let agora = Instant::now();
        let (mut buraco, pedido) = Buraco::abrir(&room, 40, agora).expect("canal aberto");
        for outro in [
            3,
            seele_core::assento::carimbo_da_entrada(3),
            MARCA_DO_PING_DO_BURACO | 39,
        ] {
            assert_eq!(
                buraco.ouvir(&pong(outro), agora),
                None,
                "o carimbo {outro:#x} fechou a página"
            );
        }
        assert_eq!(buraco.ouvir(&mensagem(1, PersonId(9), None), agora), None);
        assert_eq!(
            buraco.ouvir(&pong(pedido.carimbo), agora),
            Some(Passo::Parou(Parada::Fechou))
        );
        assert_eq!(buraco.sequencia(), 41);
        assert_eq!(buraco.canal(), CANAL);
    }

    fn pendente(room: &mut Room, chave: u64, estado: EstadoDoEnvio) {
        room.anotar_envio(CANAL, ClientMessageId(chave), format!("minha {chave}"), 0);
        match estado {
            EstadoDoEnvio::Enviando => {}
            EstadoDoEnvio::SemResposta => {
                // Só esta: as anteriores já estavam no estado delas.
                let mut so_esta = room.pendentes.clone();
                for outra in &mut so_esta {
                    if outra.client_message_id == ClientMessageId(chave) {
                        outra.estado = EstadoDoEnvio::SemResposta;
                    }
                }
                room.pendentes = so_esta;
            }
            EstadoDoEnvio::Falhou(motivo) => {
                room.recusar_envio(ClientMessageId(chave), motivo);
            }
        }
    }

    #[test]
    fn a_volta_repete_na_ordem_em_que_foram_escritas_e_so_as_sem_resposta() {
        let mut room = room();
        pendente(&mut room, 30, EstadoDoEnvio::SemResposta);
        pendente(
            &mut room,
            10,
            EstadoDoEnvio::Falhou(seele_core::MessageRefusal::PermissionDenied),
        );
        pendente(&mut room, 20, EstadoDoEnvio::SemResposta);
        pendente(&mut room, 40, EstadoDoEnvio::Enviando);
        let mut repeticao = Repeticao::default();
        let agora = Instant::now();
        repeticao.na_volta(&room, agora);

        assert_eq!(
            repeticao.vez(agora),
            vec![
                ClientMessageId(30),
                ClientMessageId(20),
                ClientMessageId(40)
            ],
            "fora da ordem em que foram escritas, ou repetindo uma que o servidor recusou"
        );
    }

    #[test]
    fn a_repeticao_sai_em_rajada_e_depois_espacada_abaixo_do_balde() {
        let mut room = room();
        for chave in 1..=58 {
            pendente(&mut room, chave, EstadoDoEnvio::SemResposta);
        }
        let mut repeticao = Repeticao::default();
        let comeco = Instant::now();
        repeticao.na_volta(&room, comeco);

        let mut saidas: Vec<(Duration, ClientMessageId)> = Vec::new();
        let mut agora = comeco;
        while let Some(quando) = repeticao.quando() {
            agora = agora.max(quando);
            for chave in repeticao.vez(agora) {
                saidas.push((agora - comeco, chave));
            }
        }

        assert_eq!(
            saidas
                .iter()
                .map(|(_, chave)| chave.get())
                .collect::<Vec<_>>(),
            (1..=58).collect::<Vec<_>>(),
            "a ordem não é a da escrita"
        );
        // Nunca mais que a rajada de uma vez, e nunca mais que a metade do
        // sustentado do balde em qualquer segundo depois dela.
        let de_saida = saidas.iter().filter(|(quando, _)| quando.is_zero()).count();
        assert_eq!(de_saida, RAJADA_DA_VOLTA);
        for janela in saidas[RAJADA_DA_VOLTA..].windows(2) {
            assert!(
                janela[1].0 - janela[0].0 >= INTERVALO_DA_REPETICAO,
                "duas repetições saíram mais perto que o intervalo: {janela:?}"
            );
        }
        // E a vez que não chegou não solta nada.
        let mut cedo = Repeticao::default();
        cedo.na_volta(&room, comeco);
        let _ = cedo.vez(comeco);
        assert!(cedo.vez(comeco + INTERVALO_DA_REPETICAO / 2).is_empty());
    }

    #[test]
    fn a_pendente_grande_demais_e_recusada_e_nao_sai() {
        let mut room = room();
        room.anotar_envio(CANAL, ClientMessageId(1), "á".repeat(3_000), 0);
        room.envios_sem_resposta();

        assert_eq!(repetir(&mut room, ClientMessageId(1), 0), Repetir::Recusada);
        assert_eq!(
            room.pendentes.first().map(|pendente| pendente.estado),
            Some(EstadoDoEnvio::Falhou(seele_core::MessageRefusal::TooLong {
                limit: 4096
            })),
            "a pendente grande demais não voltou a dizer por quê"
        );
        // E a volta seguinte não a põe na fila: ela é recusada, não sem resposta.
        let mut repeticao = Repeticao::default();
        repeticao.na_volta(&room, Instant::now());
        assert_eq!(repeticao.quando(), None);
    }

    /// **TENTAR DE NOVO numa recusada por tamanho não manda nada** (fase D).
    ///
    /// Medido na fase A: `reenviar_mensagem` mandava o corpo de 6 000 bytes ao
    /// enlace, o quadro se recusava a ser codificado, o núcleo chamava `cair()`
    /// e o enlace caía e voltava; a pendente passava de `Recusada(TooLong)` a
    /// `SemResposta`, com a tela oferecendo de novo um reenvio que derruba a
    /// conexão. Agora a conferência é a do envio, `grande_demais`, e a recusa
    /// volta a ser frase.
    #[test]
    fn tentar_de_novo_uma_grande_demais_nao_manda_nada_ao_enlace() {
        let shared = crate::tests::compartilhado_de_teste();
        let (commands, mut fila) = tokio::sync::mpsc::unbounded_channel();
        let conexao = crate::Connection {
            commands,
            shared: std::sync::Arc::clone(&shared),
            encerrada: std::sync::Mutex::new(None),
        };
        let chave = conexao
            .send_message(CANAL.get(), "á".repeat(3_000))
            .unwrap();
        assert!(
            fila.try_recv().is_err(),
            "o envio mandou um corpo grande demais"
        );

        assert_eq!(conexao.reenviar_mensagem(&chave), Ok(true));
        assert!(
            fila.try_recv().is_err(),
            "TENTAR DE NOVO mandou ao enlace um corpo que o derruba"
        );
        let estado = shared
            .room
            .lock()
            .unwrap()
            .pendentes
            .first()
            .map(|pendente| pendente.estado);
        assert_eq!(
            estado,
            Some(EstadoDoEnvio::Falhou(seele_core::MessageRefusal::TooLong {
                limit: 4096
            })),
            "a pendente deixou de dizer por que não sai"
        );
    }

    /// **Uma senha que não cabe no fio é recusada antes de pedir** (R1-M4 da
    /// revisão do P61): nada vai ao enlace, o `Room` não senta a pessoa na sala
    /// pedida — e então não há entrada no ar para desfazer —, e a recusa é a
    /// que o servidor daria a uma senha errada.
    #[test]
    fn uma_senha_grande_demais_nao_pede_a_entrada() {
        let shared = crate::tests::compartilhado_de_teste();
        let (commands, mut fila) = tokio::sync::mpsc::unbounded_channel();
        let conexao = crate::Connection {
            commands,
            shared: std::sync::Arc::clone(&shared),
            encerrada: std::sync::Mutex::new(None),
        };
        let antes = shared.room.lock().unwrap().current_voice_room;

        conexao
            .enter_voice_room(3, Some("á".repeat(17)))
            .expect("a recusa é local, e não erro de conexão");
        assert!(
            fila.try_recv().is_err(),
            "a entrada com a senha grande demais foi pedida ao enlace"
        );
        let room = shared.room.lock().unwrap();
        assert_eq!(room.current_voice_room, antes, "o `Room` sentou a pessoa");
        assert_eq!(
            room.notice.as_ref().map(|aviso| aviso.reason),
            Some(seele_core::AlertReason::VoiceRoomEntryRefused)
        );
        drop(room);

        // No teto, ela sai.
        conexao
            .enter_voice_room(3, Some("x".repeat(seele_core::MAX_NICKNAME_LEN)))
            .unwrap();
        assert!(fila.try_recv().is_ok(), "uma senha no teto não foi pedida");
    }

    #[test]
    fn a_conferencia_do_tamanho_e_a_do_teto_do_fio() {
        assert_eq!(grande_demais(&"a".repeat(seele_core::MAX_BODY_LEN)), None);
        assert!(grande_demais(&"a".repeat(seele_core::MAX_BODY_LEN + 1)).is_some());
        // Bytes, e não letras: 2 049 `á` são 4 098 bytes.
        assert!(grande_demais(&"á".repeat(2_049)).is_some());
    }

    /// O servidor de mentira da idempotência: a mesma chave da mesma pessoa
    /// devolve a linha original (`Messages::append_batch`), e cada envio
    /// volta como eco — duas vezes, para quem repetiu (medido pela F1).
    #[derive(Default)]
    struct Gravador {
        linhas: Vec<(ClientMessageId, u64)>,
    }

    impl Gravador {
        fn gravar(&mut self, chave: ClientMessageId) -> u64 {
            if let Some((_, id)) = self.linhas.iter().find(|(ja, _)| *ja == chave) {
                return *id;
            }
            let id = 1_000 + u64::try_from(self.linhas.len()).unwrap();
            self.linhas.push((chave, id));
            id
        }
    }

    #[test]
    fn repetir_com_a_mesma_chave_nao_duplica_na_tela() {
        // O servidor gravou antes de a conexão cair, e o eco se perdeu: a
        // pendente ficou sem resposta.
        let mut room = room();
        let chave = ClientMessageId(77);
        room.anotar_envio(CANAL, chave, "oi".into(), 0);
        let mut servidor = Gravador::default();
        let _ = servidor.gravar(chave);
        room.envios_sem_resposta();

        // A volta repete, e o servidor ecoa duas vezes.
        let Repetir::Mandar { chave: mandada, .. } = repetir(&mut room, chave, 0) else {
            panic!("a pendente sem resposta não foi repetida");
        };
        for _ in 0..2 {
            let id = servidor.gravar(mandada);
            room.apply(&mensagem(id, EU, Some(mandada)));
        }
        // E o buraco traz a linha original, com a chave dela.
        let (original, id) = servidor.linhas[0];
        room.apply(&mensagem(id, EU, Some(original)));

        assert_eq!(ids(&room).len(), 1, "a mensagem saiu duplicada na tela");
        assert_eq!(
            servidor.linhas.len(),
            1,
            "a repetição gravou uma segunda linha"
        );
        assert!(
            room.pendentes.is_empty(),
            "a pendente não foi confirmada pelo eco"
        );
    }
}
