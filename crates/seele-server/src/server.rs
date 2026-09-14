//! The server's shared state: storage, the write batch, and the event bus.
//!
//! `specs/04-servidor-seele.md` puts voice room state in a task per voice room with no global
//! lock. Text is different: it is one durable log per Channel, and the thing worth
//! avoiding is not contention but `fsync` per message. So storage sits behind a
//! single mutex — SQLite in WAL mode has one writer anyway — and the batching
//! happens in [`spawn_writer`].
//!
//! # Confirmation order
//!
//! A message is broadcast **after** its batch commits, never before. The
//! acceptance criterion in `specs/04-servidor-seele.md` is "reinício não perde
//! mensagem confirmada ao cliente", and announcing before the commit is exactly
//! how that promise gets broken by a power cut nobody planned for.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_proto::control::{ChannelInfo, PersonProfile, PersonState, VoiceRoomInfo};
use seele_proto::ids::{ChannelId, MessageId, PersonId, ScreenId, SessionId, Ssrc, VoiceRoomId};
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::persistence::messages::{Messages, PendingMessage, StoredMessage};
use crate::persistence::Persistence;

/// How long the writer waits before committing what it has.
///
/// `specs/04-servidor-seele.md`: "flush por tempo (~200 ms)". Long enough that a
/// busy Channel commits once instead of fifty times; short enough that a message
/// still feels sent immediately.
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(200);

/// Events every connection may care about.
///
/// Broadcast to all, filtered per connection. `specs/04-servidor-seele.md` sizes
/// a server at ~50 people, so filtering at the edge costs nothing and keeps the
/// bus from needing to know who is subscribed to what.
#[derive(Debug, Clone)]
pub enum Event {
    /// A subida medida do servidor andou, e quem está conectado precisa saber.
    ///
    /// **O `HostUplink` deixou de ser dito uma vez só.** Ele saía na entrada da
    /// sessão com o comentário «é uma declaração de configuração e não uma
    /// medida: enquanto ninguém medir, ela não muda de faixa e não há segundo
    /// quadro a mandar». Agora alguém mede, e o segundo quadro existe.
    HostUplink {
        /// A subida estimada, em bits por segundo.
        bps: u32,
    },
    /// A message was committed and is now durable.
    MessagePosted(StoredMessage),
    /// A message was edited.
    MessageEdited {
        /// Which Channel.
        channel: ChannelId,
        /// Which message.
        id: MessageId,
        /// New body.
        body: String,
    },
    /// A message was removed.
    MessageRemoved {
        /// Which Channel.
        channel: ChannelId,
        /// Which message.
        id: MessageId,
    },
    /// A person entered a voice room.
    PersonJoined {
        /// Which voice room.
        voice_room: VoiceRoomId,
        /// Who.
        profile: PersonProfile,
        /// Their media source.
        ssrc: Ssrc,
    },
    /// A person left a voice room.
    PersonLeft {
        /// Which voice room.
        voice_room: VoiceRoomId,
        /// Who.
        person: PersonId,
    },
    /// A person connected to the server, whatever room they are in.
    PersonPresent {
        /// Who, and what they are called.
        quem: Occupant,
    },
    /// A person's connection ended.
    PersonGone {
        /// Who.
        person: PersonId,
    },
    /// A person's state changed, including their Sync Ratio.
    PersonState(PersonState),
    /// A voice room was created.
    ///
    /// Announced to **everybody**, the person who asked included, and this is the
    /// difference between a feature and a demonstration: a room that only shows
    /// up on the next handshake is a room whose maker has to tell their friends
    /// to reconnect before they can use it.
    VoiceRoomCreated {
        /// The voice room, as it now exists.
        voice_room: VoiceRoomInfo,
    },
    /// A Channel was created.
    ChannelCreated {
        /// The Channel, as it now exists.
        channel: ChannelInfo,
    },
    /// A voice room was renamed.
    VoiceRoomRenamed {
        /// Which voice room.
        voice_room: VoiceRoomId,
        /// Its new name.
        name: String,
    },
    /// A Channel was renamed.
    ChannelRenamed {
        /// Which Channel.
        channel: ChannelId,
        /// Its new name.
        name: String,
    },

    // ---- what the server calls itself ----
    //
    // The same shape as `VoiceRoomRenamed`, one level up: committed first, announced
    // after, forwarded to every connection including the one that asked.
    //
    // Announced to **everybody** for the reason `VoiceRoomCreated` gives, and here it
    // is sharper than for a room: a name is drawn in the header of every open
    // window, so a rename that only reached the next handshake would put the new
    // name on the screen of whoever typed it and the old one on everybody
    // else's — which is the exact failure ADR 0032 says not to ship.
    /// The server was renamed.
    ServerRenamed {
        /// What it is called now.
        name: String,
    },
    /// The server's icon changed, or was taken down.
    ///
    /// The bytes travel on the bus rather than a "go and read it again": the bus
    /// is what every connection is already draining, and telling fifty sessions
    /// to each take the PERSISTENCE lock and read the same 8 KiB row would be fifty
    /// reads of a value one reader already has in its hand.
    ServerIconChanged {
        /// The picture, or `None` when it was taken down.
        icon: Option<Vec<u8>>,
    },

    /// A imagem de alguém mudou.
    ///
    /// Difundida a todos, como a do servidor — mas com o dono junto, porque a
    /// figura é de uma pessoa e não do lugar. Quem recebe troca a imagem
    /// daquela linha do roster e de mais nenhuma.
    /// Alguém trocou de apelido.
    ///
    /// Difundido a todos. O histórico não é tocado: cada mensagem guarda o
    /// apelido de quando foi escrita, e é decisão de produto que continue
    /// mostrando aquele.
    PersonRenamed {
        /// Quem.
        person: PersonId,
        /// O nome novo.
        nickname: String,
    },
    /// A imagem de alguém mudou.
    ///
    /// Difundida a todos, como a do servidor — mas com o dono junto, porque a
    /// figura é de uma pessoa e não do lugar. Quem recebe troca a imagem
    /// daquela linha do roster e de mais nenhuma.
    PersonIconChanged {
        /// De quem.
        person: PersonId,
        /// A figura, ou `None` quando foi tirada.
        icon: Option<Vec<u8>>,
    },

    // ---- moderation ----
    //
    // These two are the odd ones on this bus: every other event is something a
    // connection **forwards** to its client, and these are something a
    // connection **does to itself**. They are here anyway because a server has no
    // other way for one session to reach another — there is no map of live
    // sessions, deliberately, since `specs/04-servidor-seele.md` puts voice room state
    // in a task per voice room with no global lock. The bus already reaches every
    // connection; adding a registry of sessions beside it would be a second way
    // to find somebody, and the two would disagree the first time one of them
    // leaked.
    //
    // Addressed to one person, delivered to all, acted on by the one. At fifty
    // sessions that is forty-nine cheap comparisons, once, when an operator
    // presses a button.
    /// An operator ended a person's session.
    SessionEnded {
        /// Whose.
        person: PersonId,
        /// Which of the enumerated reasons to send them.
        reason: seele_proto::control::DisconnectReason,
    },
    /// An operator moved a person into a voice room.
    PersonMoved {
        /// Who.
        person: PersonId,
        /// Where to.
        voice_room: VoiceRoomId,
    },

    // ---- unmaking a room ----
    //
    // Both halves of the bus at once, and they are the first events that are.
    // Everything above is either something every connection **forwards** to its
    // client (a room was made) or something one connection **does to itself**
    // (you were kicked). These are both: every client has to stop drawing the
    // room, and the ones who were standing in it have to be turned out and told.
    //
    // Which is why the sessions concerned act on them in the loop and then
    // `continue`, rather than letting `translate` write the same frame twice.
    /// A voice room was destroyed.
    VoiceRoomDeleted {
        /// Which voice room.
        voice_room: VoiceRoomId,
    },
    /// A Channel was destroyed, and everything written in it with it.
    ChannelDeleted {
        /// Which Channel.
        channel: ChannelId,
    },

    // ---- compartilhamento de tela ----
    //
    // Só o controle passa por aqui. **Os quadros não**, e é a decisão medida do
    // §3 de `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`:
    // eles vão num fluxo unidirecional QUIC, e este barramento é um
    // `broadcast::Sender<Event>` que toda conexão drena — pôr 150 kB/s de vídeo
    // num anel que cinquenta sessões copiam seria o oposto exato do desenho.
    /// Alguém começou a compartilhar tela.
    ScreenShareStarted {
        /// Em qual voice room.
        voice_room: VoiceRoomId,
        /// Quem.
        person: PersonId,
        /// Como a transmissão se chama daqui em diante.
        screen: ScreenId,
    },
    /// Uma transmissão acabou.
    ScreenShareStopped {
        /// Em qual voice room.
        voice_room: VoiceRoomId,
        /// Qual transmissão.
        screen: ScreenId,
    },
    /// Quantas pessoas estão recebendo uma transmissão, agora.
    ///
    /// **N**, e ele é um termo do teto do §5.1 —
    /// `caminho de quem hospeda × 60% ÷ N` — que até aqui só existia dentro do
    /// servidor. Sem ele no fio, quem compartilha aplica um `min` com uma perna
    /// que inventa; com ele, a mesma conta é feita nas duas pontas a partir do
    /// mesmo número.
    ///
    /// Mandado pelo [`crate::voice_room::VoiceRoom`], que é o único lugar deste servidor que
    /// sabe quem está na sala sem perguntar a ninguém, e no mesmo instante em
    /// que ele refaz o teto. Dois donos de N seriam duas contas discordando no
    /// primeiro dia ruim.
    ScreenViewers {
        /// Em qual voice room.
        voice_room: VoiceRoomId,
        /// Qual transmissão.
        screen: ScreenId,
        /// Quantos assistem. Não conta quem compartilha.
        quantos: u32,
    },
    /// Alguém que assiste não tem de que predizer e pediu um quadro-chave.
    ///
    /// Endereçado a uma pessoa e entregue a todos, como o [`Self::SessionEnded`]
    /// — um servidor não tem outra maneira de uma sessão alcançar outra, e o
    /// barramento já é o que toda conexão drena.
    KeyFrameRequested {
        /// Qual transmissão.
        screen: ScreenId,
        /// Quem pediu.
        person: PersonId,
        /// A quem entregar: quem está compartilhando.
        sharer: PersonId,
    },

    // ---- o caminho entre pares ----
    //
    // Os dois são endereçados a **uma** pessoa e entregues a todas, como o
    // [`Self::KeyFrameRequested`] logo acima e pela mesma razão: um servidor
    // não tem outra maneira de uma sessão alcançar outra, e o barramento já é
    // o que toda conexão drena. Quem estreita a audiência é `session::translate`.
    //
    // São dois eventos e não um porque as duas pontas recebem coisas
    // diferentes: quem empresta recebe o endereço de quem vai assistir, e quem
    // assiste recebe o de quem empresta. Um evento só carregaria os dois pares
    // de endereços para as duas pontas, e cada uma leria o endereço da outra
    // sem precisar dele.
    /// O servidor apontou esta pessoa para servir uma transmissão a outra.
    SirvaTelaPara {
        /// Qual transmissão.
        screen: ScreenId,
        /// A quem entregar: quem empresta a subida.
        quem_empresta: PersonId,
        /// Onde alcançar quem vai assistir.
        enderecos: Vec<std::net::SocketAddr>,
        /// A impressão digital que quem vai assistir apresenta.
        impressao: String,
    },
    /// O servidor mandou esta pessoa buscar a imagem num par, e não nele.
    AssistaTelaPor {
        /// Qual transmissão.
        screen: ScreenId,
        /// A quem entregar: quem pediu para assistir.
        quem_assiste: PersonId,
        /// Onde alcançar quem empresta a subida.
        enderecos: Vec<std::net::SocketAddr>,
        /// A impressão digital que quem empresta apresenta.
        impressao: String,
    },

    // ---- o bitrate adaptativo do ADR 0036 ----
    /// Quanto da voz de alguém não está chegando.
    ///
    /// Endereçado a uma pessoa e entregue a todas, como o
    /// [`Self::KeyFrameRequested`] acima e pelo mesmo motivo: um servidor não
    /// tem outra maneira de uma sessão alcançar outra, e a sala que mede não
    /// conhece sessão nenhuma. Quem estreita a audiência é `session::translate`.
    ///
    /// **A audiência é uma pessoa só**, e isso é promessa e não detalhe:
    /// difundir a perda de subida contaria a toda a sala a qualidade da rede de
    /// cada um.
    UplinkLoss {
        /// De quem é a subida medida.
        person: PersonId,
        /// A fração perdida, de zero a um.
        fraction: f32,
    },

    // ---- o teto contado do ADR 0038 ----
    /// Uma sala cresceu além do que a subida medida desta máquina comporta.
    ///
    /// Difundido, e entregue **só a quem tem `AdministerServer`** — o filtro
    /// mora no laço da sessão, e não em `translate`, porque conferir permissão é
    /// uma pergunta ao banco e `translate` é síncrona.
    ///
    /// Ninguém é impedido de nada por causa disto. É aviso, e o `limit` que quem
    /// hospeda escreveu continua sendo o único que barra alguém.
    VoiceRoomOverHostUplink {
        /// Qual sala cresceu.
        voice_room: VoiceRoomId,
        /// Quanto ela pede no pior caso, todos falando ao mesmo tempo.
        precisa_bps: u64,
        /// A subida medida desta máquina. Nunca zero: sem medida não há aviso.
        medido_bps: u32,
    },
}

/// Quem está compartilhando tela em cada sala de voz.
///
/// **Uma transmissão por sala de voz**, que é o §6 item 3 da spec de compartilhamento
/// de tela: *«uma transmissão por sala de voz na v1. Duas dobram a subida de
/// quem recebe e triplicam a interface»*. Quem chega depois perde a corrida e
/// **é avisado com nome** — `AlertReason::ScreenShareTaken` —, porque
/// `PermissionDenied` diria «você não pode» a quem pode.
///
/// Ao lado da [`Occupancy`] e não dentro dela: quem está sentado e quem está
/// transmitindo mudam por motivos diferentes e em momentos diferentes, e a
/// única coisa que os liga é a limpeza — sair da sala de voz encerra a transmissão,
/// que é o que [`Telas::encerrar_de`] existe para fazer numa chamada só.
#[derive(Debug, Default)]
pub struct Telas {
    /// Por sala: quem transmite, **de qual conexão**, e qual transmissão é.
    ///
    /// A sessão está aqui pelo mesmo motivo que está no [`Occupant`]: quem
    /// encerra precisa saber de qual conexão a transmissão é, ou a conexão velha
    /// de alguém, ao morrer, apaga a tela que a conexão nova dele acabou de
    /// abrir.
    por_voice_room: HashMap<VoiceRoomId, Vec<(PersonId, SessionId, ScreenId)>>,
}

// **Não há constante de quantas transmissões cabem numa sala, e isso é a
// decisão.**
//
// Ela já foi `1` e já foi `2`. Os dois números estavam errados pela mesma razão:
// o que uma transmissão a mais consome depende da subida da casa de quem hospeda
// e de quantas pessoas estão assistindo, e nenhuma constante sabe disso.
//
// Quem recusa é `VoiceRoom::tela_abriu`, quando o teto por cópia não cabe mais
// acima de `crate::tela::PISO_DE_BANDA_BPS` — e recusa com
// `AlemDoQueOHospedeiroCarrega`, que é uma frase que explica em vez de um número
// que ninguém entende. Numa casa com subida boa cabem várias; numa apertada, a
// segunda não cabe.

impl Telas {
    /// Registra uma transmissão.
    ///
    /// **Não recusa nada**, e é aí que a decisão mora: quem sabe se uma
    /// transmissão a mais cabe é o encaminhador, que mede a subida e conta as
    /// cópias — ver a nota acima e `VoiceRoom::tela_abriu`.
    ///
    /// Quem já transmite pedindo de novo **troca a própria tela** e não ocupa
    /// vaga nova: é um cliente que reabriu o botão, ou um `StartScreenShare`
    /// depois de reconectar. Mandar duas telas dobraria a subida de quem manda
    /// sem que ninguém tivesse pedido a segunda.
    ///
    /// # Por que devolve a tela trocada
    ///
    /// Porque a troca é o único ponto do desmonte em que a identidade da sessão é
    /// **sobrescrita** em vez de conferida — e sobrescrever calado deixa o
    /// `ScreenId` anterior desenhado para sempre em quem assiste: o cliente funde
    /// aditivamente, e só um `ScreenShareStopped` apaga um cabeçalho de
    /// transmissão. Devolvendo, quem chama anuncia o fim da antiga antes do começo
    /// da nova, e a substituição deixa de ser silenciosa. Achado por revisão
    /// independente; é a mesma família de «o produto sabe e não conta».
    #[must_use]
    pub fn comecar(
        &mut self,
        voice_room: VoiceRoomId,
        person: PersonId,
        sessao: SessionId,
        screen: ScreenId,
    ) -> Option<ScreenId> {
        let vagas = self.por_voice_room.entry(voice_room).or_default();

        if let Some(minha) = vagas.iter_mut().find(|(dono, _, _)| *dono == person) {
            let substituida = minha.2;
            minha.1 = sessao;
            minha.2 = screen;
            return Some(substituida);
        }
        vagas.push((person, sessao, screen));
        None
    }

    /// Encerra a transmissão desta pessoa nesta sala, se houver.
    ///
    /// Conferido, e não apagado às cegas: um `StopScreenShare` de quem não está
    /// transmitindo derrubaria a tela de quem está.
    ///
    /// Conferida também a **sessão**, pela mesma razão de [`Self::encerrar_de`]:
    /// numa queda silenciosa a conexão velha da pessoa segue viva por alguns
    /// segundos, e um `StopScreenShare` atrasado dela derrubaria a transmissão
    /// que a conexão nova acabou de abrir.
    pub fn parar(
        &mut self,
        voice_room: VoiceRoomId,
        person: PersonId,
        sessao: SessionId,
    ) -> Option<ScreenId> {
        let vagas = self.por_voice_room.get_mut(&voice_room)?;
        let onde = vagas
            .iter()
            .position(|(dono, de_quem, _)| *dono == person && *de_quem == sessao)?;
        let (_, _, screen) = vagas.remove(onde);
        if vagas.is_empty() {
            self.por_voice_room.remove(&voice_room);
        }
        Some(screen)
    }

    /// Encerra o que esta pessoa estivesse transmitindo, onde quer que fosse.
    ///
    /// Devolve as salas e as transmissões, porque alguém tem de anunciar o fim e
    /// quem chama nem sempre sabe a sala — uma sessão acaba em qualquer `?` do
    /// meio do laço dela. É o mesmo raciocínio de
    /// [`Occupancy::vacate_everywhere_da_sessao`].
    ///
    /// `sessao` é `Some` quando quem encerra é uma conexão, e aí só encerra o que
    /// **aquela** conexão abriu: sem isso, a conexão velha de alguém apagaria, ao
    /// morrer, a transmissão que a conexão nova dele acabou de abrir.
    pub fn encerrar_de(
        &mut self,
        person: PersonId,
        sessao: Option<SessionId>,
    ) -> Vec<(VoiceRoomId, ScreenId)> {
        let mut encerradas = Vec::new();
        for (voice_room, vagas) in &mut self.por_voice_room {
            if let Some(onde) = vagas.iter().position(|(dono, de_quem, _)| {
                *dono == person && sessao.is_none_or(|qual| *de_quem == qual)
            }) {
                let (_, _, screen) = vagas.remove(onde);
                encerradas.push((*voice_room, screen));
            }
        }
        self.por_voice_room.retain(|_, vagas| !vagas.is_empty());
        encerradas
    }

    /// Encerra tudo o que estivesse acontecendo numa sala que deixou de existir.
    pub fn encerrar_voice_room(&mut self, voice_room: VoiceRoomId) -> Vec<ScreenId> {
        self.por_voice_room
            .remove(&voice_room)
            .unwrap_or_default()
            .into_iter()
            .map(|(_, _, screen)| screen)
            .collect()
    }

    /// Quem está transmitindo nesta sala.
    #[must_use]
    pub fn em(&self, voice_room: VoiceRoomId) -> Vec<(PersonId, ScreenId)> {
        self.por_voice_room
            .get(&voice_room)
            .map(|vagas| {
                vagas
                    .iter()
                    .map(|(dono, _, screen)| (*dono, *screen))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Onde esta pessoa está transmitindo, se está.
    ///
    /// A pergunta que o **encaminhamento** faz, e ela vem ao contrário de
    /// [`Self::em`] por um motivo concreto: a tarefa que aceita os fluxos
    /// unidirecionais de uma conexão vive fora do laço da sessão — é o que a
    /// impede de bloquear o controle — e por isso não enxerga em que sala o
    /// connection está. Este registro é a única fonte que sabe as duas coisas ao
    /// mesmo tempo, e é a mesma que decidiu a vaga. Perguntar a ela é o que
    /// garante que um fluxo de tela só é aceito de quem o controle já autorizou.
    ///
    /// Uma pessoa transmite **uma** tela por vez, mesmo com mais de uma vaga na
    /// sala: o que a segunda vaga permite é outra pessoa, não outra janela da
    /// mesma.
    ///
    /// A `sessao` é conferida porque quem pergunta é **uma conexão**: um fluxo
    /// aberto pela conexão velha de alguém não pode ser aceito como se fosse da
    /// transmissão que a conexão nova registrou — o cabeçalho ainda casaria, e o
    /// que chegaria à sala seriam duas fontes escrevendo o mesmo `ScreenId`.
    #[must_use]
    pub fn de(&self, person: PersonId, sessao: SessionId) -> Option<(VoiceRoomId, ScreenId)> {
        self.por_voice_room.iter().find_map(|(voice_room, vagas)| {
            vagas
                .iter()
                .find(|(dono, de_quem, _)| *dono == person && *de_quem == sessao)
                .map(|(_, _, screen)| (*voice_room, *screen))
        })
    }

    /// Tudo o que está acontecendo agora, achatado.
    ///
    /// Achatado e não devolvido como mapa porque o único chamador percorre a
    /// lista uma vez para escrever um quadro por transmissão — a mesma razão
    /// que [`Occupancy::everywhere`] dá.
    #[must_use]
    pub fn todas(&self) -> Vec<(VoiceRoomId, PersonId, ScreenId)> {
        self.por_voice_room
            .iter()
            .flat_map(|(voice_room, vagas)| {
                vagas
                    .iter()
                    .map(move |(person, _, screen)| (*voice_room, *person, *screen))
            })
            .collect()
    }
}

/// A voice room seat held open for a person who dropped.
///
/// `specs/02-protocolo.md`: "O servidor guarda o slot pelo mesmo período" — the
/// five minutes of the internal battery. Without this a person whose train enters
/// a tunnel comes back to find their voice room full.
#[derive(Debug, Clone, Copy)]
struct ReservedSlot {
    /// A conexão que guardou o assento. É o que separa «a reserva que eu mesmo
    /// acabei de escrever» de «a reserva que uma conexão anterior deixou», e sem
    /// ela o descarte depende de uma ordem que nenhum tipo confere.
    sessao: SessionId,
    voice_room: VoiceRoomId,
    ssrc: Ssrc,
    expires_at: Instant,
}

/// Seats held for people who are expected back.
#[derive(Debug, Default)]
pub struct Slots {
    reserved: HashMap<PersonId, ReservedSlot>,
}

impl Slots {
    /// Holds a seat for the grace period.
    pub fn reserve(
        &mut self,
        person: PersonId,
        sessao: SessionId,
        voice_room: VoiceRoomId,
        ssrc: Ssrc,
        now: Instant,
    ) {
        self.reserved.insert(
            person,
            ReservedSlot {
                sessao,
                voice_room,
                ssrc,
                expires_at: now + seele_proto::transport::SESSION_GRACE,
            },
        );
    }

    /// Reclaims a seat, if one is still being held.
    ///
    /// Returns the voice room and the `ssrc` the person had, so a reconnection lands
    /// where it left off rather than looking like somebody new.
    pub fn reclaim(&mut self, person: PersonId, now: Instant) -> Option<(VoiceRoomId, Ssrc)> {
        let slot = self.reserved.get(&person).copied()?;
        if slot.expires_at <= now {
            self.reserved.remove(&person);
            return None;
        }
        self.reserved.remove(&person);
        Some((slot.voice_room, slot.ssrc))
    }

    /// Joga fora o assento guardado para esta pessoa **por outra conexão**, e diz
    /// se havia um. A reserva escrita pela própria sessão vigente fica onde está.
    ///
    /// Existe para a janela que [`reservar_o_assento_da_carencia`] não alcança:
    /// ver [`descartar_a_reserva_de_quem_ja_voltou`], que é quem chama.
    pub fn descartar(&mut self, person: PersonId, vigente: SessionId) -> bool {
        let Some(slot) = self.reserved.get(&person) else {
            return false;
        };
        if slot.sessao == vigente {
            return false;
        }
        self.reserved.remove(&person).is_some()
    }

    /// Drops seats whose grace period has passed.
    pub fn sweep(&mut self, now: Instant) -> usize {
        let before = self.reserved.len();
        self.reserved.retain(|_, slot| slot.expires_at > now);
        before - self.reserved.len()
    }

    /// How many seats are currently held.
    #[must_use]
    pub fn held(&self) -> usize {
        self.reserved.len()
    }
}

/// Somebody sitting in a voice room right now.
#[derive(Debug, Clone)]
pub struct Occupant {
    /// Who.
    pub person: PersonId,
    /// What they are called.
    pub nickname: String,
    /// Their media source.
    pub ssrc: Ssrc,
    /// **Qual conexão** desta pessoa pôs este registro aqui.
    ///
    /// A chave continua sendo a pessoa — duas linhas para ela seriam dois nomes
    /// na lista —, mas quem **apaga** precisa saber de qual sessão o registro é.
    /// Sem isto, a sessão que morre aos 20 s de silêncio apagava a que subiu aos
    /// 15 s: o desmonte era chaveado por pessoa e nunca perguntava se aquela
    /// pessoa ainda era dele. Ver `desassentar` em `crate::session`.
    pub sessao: SessionId,
}

/// Quem está conectado neste servidor agora, sentado numa sala ou não.
///
/// # Por que não bastava a ocupação
///
/// [`Occupancy`] responde «quem está em qual sala», e por muito tempo era a
/// única tabela de presença que existia — então quem entrava no servidor e ficava
/// fora das salas não existia para mais ninguém. `PersonJoined` carrega uma sala de voz
/// porque anuncia sentar-se num; não havia mensagem para estar aqui. O cliente
/// escreveu isso num comentário e seguiu em frente: «não há mensagem na fita
/// que diga quem entrou no servidor e ficou fora das salas».
///
/// Chaveado por pessoa e não por conexão: reconectar dentro da carência é a
/// mesma pessoa, e duas linhas para ela seriam dois nomes na lista.
#[derive(Debug, Default)]
pub struct Presentes {
    por_person: HashMap<PersonId, Occupant>,
}

impl Presentes {
    /// Marca alguém como presente, e diz se ele ainda não estava.
    ///
    /// O `bool` é o que evita anunciar duas vezes: uma reconexão dentro da
    /// carência passa por aqui de novo, e um segundo `PersonPresent` faria a
    /// lista de todo mundo piscar sem nada ter mudado.
    ///
    /// **Aqui a última gravação vence, e isso se apoia em ordem, não em
    /// identidade.** Todo o resto desta família ([`Presentes::saiu`],
    /// [`Presentes::e_a_vigente`], `Occupancy::vacate_da_sessao`) compara a
    /// sessão gravada antes de mexer; esta não compara, porque quem chega é,
    /// por construção, a conexão mais nova: o `handshake` da conexão velha
    /// terminou antes de a nova sequer existir. Se algum dia o `handshake`
    /// passar a acontecer fora dessa ordem — reaproveitado, repetido ou
    /// adiado —, esta linha volta a ser o buraco que o resto fechou, e passa a
    /// precisar do mesmo cuidado: só sobrescrever quando a sessão que chega for
    /// posterior à gravada.
    ///
    /// O que sobrescrever significa para o resto está preso em
    /// `quem_reconecta_passa_a_ser_a_vigente_e_a_anterior_perde_a_autoridade`:
    /// é daqui que sai a resposta de quem é a conexão vigente de uma pessoa, e
    /// sem isso os guardas continuariam passando enquanto apontam para a
    /// conexão errada.
    pub fn chegou(&mut self, quem: Occupant) -> bool {
        self.por_person.insert(quem.person, quem).is_none()
    }

    /// Tira alguém, e diz se havia o que tirar — **se a sessão for a dele**.
    ///
    /// O identificador de sessão não é decoração: numa queda silenciosa a
    /// conexão nova da mesma pessoa sobe por volta de 15 s e a velha só é
    /// desmontada aos 20 s. Chaveado só por pessoa, o desmonte da velha tirava
    /// dos presentes quem estava conectado agora, e o `PersonGone` que ele
    /// difundia apagava a pessoa da lista de todo mundo — **sem nunca avisar a
    /// própria**, porque `translate` não manda `PersonGone` para si.
    ///
    /// # O relato de 07/09/2026, e por que ele fica coberto aqui
    ///
    /// *«Ele entrava na sala, ficava alguns segundos e saía, e não aparecia pra
    /// mim — mas pra ele, ele tava dentro.»* A linha principal já havia
    /// consertado esse caso conferindo o `ssrc`: o cliente tenta vários caminhos
    /// ao mesmo tempo (ADR 0037), fica com o primeiro que abre, e os abandonados
    /// fecham uns 80 ms depois — e fechar roda a saída inteira, que apagava a
    /// ficha de quem estava vivo.
    ///
    /// **É o mesmo defeito, e a conferência de sessão o cobre inteiro.** O
    /// caminho abandonado e a conexão que morre 20 s depois de a pessoa já ter
    /// voltado são o mesmo caso visto de dois relógios: uma conexão que não é
    /// mais a vigente removendo estado de quem é. Conferir a sessão substitui a
    /// conferência de `ssrc` em vez de conviver com ela, porque [`SessionId`] é o
    /// identificador que todo o desmonte já carrega — inclusive onde não há
    /// `ssrc` à mão, como nas telas e nas reservas de assento.
    pub fn saiu(&mut self, person: PersonId, sessao: SessionId) -> bool {
        match self.por_person.get(&person) {
            Some(quem) if quem.sessao == sessao => {
                self.por_person.remove(&person);
                true
            }
            _ => false,
        }
    }

    /// Se esta sessão ainda é a conexão vigente desta pessoa.
    ///
    /// A autoridade mora aqui porque [`Presentes`] já responde «quem está
    /// conectado», e uma quarta tabela só para dizer «qual conexão» seria uma
    /// segunda cópia da mesma contabilidade — exatamente o que produziu este
    /// defeito.
    ///
    /// **Sem registro nenhum é recusa, e isso foi um conserto.** A primeira
    /// versão respondia «sim» quando não havia registro, com o argumento de que
    /// uma sessão morta antes de se anunciar presente não deve deixar assento de
    /// fantasma para trás. O argumento estava certo e a conclusão invertida: quem
    /// nunca se anunciou presente também nunca entrou em sala nenhuma, de modo
    /// que não chega a perguntar isto — e o ramo permissivo só disparava no caso
    /// oposto, o de o registro **já ter sido tirado por uma sessão posterior**.
    ///
    /// O encadeamento que ele deixava aberto: a sessão A cai calada, B reconecta,
    /// B sai da sala de propósito e se desconecta, e só então A morre. Sem
    /// registro a que se comparar, A gravava uma reserva de cinco minutos com a
    /// sala e o `ssrc` velhos, e o `handshake` seguinte re-sentava quem tinha
    /// saído por vontade própria — exatamente o sintoma que este conserto fecha.
    #[must_use]
    pub fn e_a_vigente(&self, person: PersonId, sessao: SessionId) -> bool {
        self.por_person
            .get(&person)
            .is_some_and(|quem| quem.sessao == sessao)
    }

    /// Se esta pessoa tem **alguma** conexão viva, seja ela qual for.
    ///
    /// Diferente de [`Presentes::e_a_vigente`], que pergunta por uma sessão
    /// nomeada. A distinção é o que deixa uma conexão que acabou saber por que
    /// não tinha nada seu para remover: porque a pessoa não está mais aqui, ou
    /// porque ela já voltou por outra conexão. Ver [`Desassentamentos`].
    #[must_use]
    pub fn esta_presente(&self, person: PersonId) -> bool {
        self.por_person.contains_key(&person)
    }

    /// Todo mundo que está aqui agora.
    pub fn todos(&self) -> Vec<Occupant> {
        self.por_person.values().cloned().collect()
    }
}

/// Guarda o assento da carência para uma conexão que acabou — **se ainda for a
/// desta pessoa** — e diz se guardou.
///
/// Mora aqui, e não solta no fim da sessão, porque um guarda dentro de uma função
/// de trezentas linhas que só roda com servidor, QUIC e relógio de verdade é um
/// guarda que nenhum teste alcança: era exatamente o achado da revisão. Com a
/// decisão num nome só, revertê-la faz um teste reprovar.
///
/// A reserva obsoleta é da mesma família do defeito que `session::desassentar`
/// fecha: quem saiu da sala de propósito e caiu em seguida era re-sentado na
/// conexão seguinte por uma reserva deixada por uma conexão anterior, e essa
/// reserva vale cinco minutos.
pub fn reservar_o_assento_da_carencia(
    presentes: &Presentes,
    slots: &mut Slots,
    person: PersonId,
    sessao: SessionId,
    voice_room: VoiceRoomId,
    ssrc: Ssrc,
    now: Instant,
) -> bool {
    if !presentes.e_a_vigente(person, sessao) {
        return false;
    }
    slots.reserve(person, sessao, voice_room, ssrc, now);
    true
}

/// Joga fora a reserva que uma conexão **anterior** tenha deixado depois de esta
/// já ter resgatado a sua — e diz se havia uma.
///
/// # A janela que o guarda da reserva não alcança
///
/// [`reservar_o_assento_da_carencia`] pergunta a [`Presentes`] quem é a sessão
/// vigente, e a conexão nova só entra em `Presentes` depois do `handshake`. Entre
/// uma coisa e outra há um intervalo em que a sessão velha ainda é a vigente: se
/// ela morrer exatamente aí, a pergunta responde «sim» e a reserva obsoleta é
/// gravada assim mesmo — com a sala e o `ssrc` velhos, valendo cinco minutos.
/// Achado por revisão independente, e não por defeito de campo, porque a janela é
/// de microssegundos.
///
/// O outro lado da mesma pergunta fecha-a sem cronômetro nenhum: quando uma
/// conexão se declara presente, o `handshake` dela **já resgatou** o que houvesse
/// para resgatar. Qualquer reserva que exista neste instante foi escrita depois
/// disso, quer dizer, por uma conexão que já não é a desta pessoa. Não há caso
/// legítimo a perder — e o que se perde, se este descarte não existir, é a pessoa
/// ser re-sentada numa sala de onde ela saiu de propósito.
///
/// # Por que este descarte também é chaveado por sessão
///
/// O parágrafo acima é um argumento de ordem: «o resgate já aconteceu, logo o que
/// existir agora é de outra conexão». O argumento se sustenta no fluxo de hoje,
/// mas era o único guarda desta família que não conferia sessão nenhuma — achado
/// de revisão independente —, e uma ordem que só o texto garante é uma ordem que
/// a próxima mudança pode quebrar sem nenhum teste reclamar. Com a sessão gravada
/// na reserva, a pergunta deixa de ser «quando isto aconteceu» e passa a ser
/// «quem escreveu isto»: só some a reserva de quem já não é esta conexão.
pub fn descartar_a_reserva_de_quem_ja_voltou(
    slots: &mut Slots,
    person: PersonId,
    vigente: SessionId,
) -> bool {
    slots.descartar(person, vigente)
}

/// O que aconteceu quando uma conexão pediu para transmitir a tela dela.
///
/// Existe para que a recusa tenha nome: quem chama precisa distinguir «a vaga é
/// sua» de «esta conexão já não é a desta pessoa», e um `Option` não diz qual das
/// duas foi.
#[derive(Debug, PartialEq, Eq)]
pub enum AberturaDeTela {
    /// A vaga é desta conexão. `substituida` é a tela que ela trocou, quando
    /// havia uma — quem chama anuncia o fim dela antes do começo da nova.
    Aberta {
        /// A tela que saiu do lugar, se saiu.
        substituida: Option<ScreenId>,
    },
    /// Esta conexão já não é a vigente da pessoa dela, e nada foi escrito.
    DeConexaoVelha,
}

/// Registra a transmissão desta conexão — **se ela ainda for a vigente desta
/// pessoa**.
///
/// # A última escrita por pessoa desta família
///
/// [`Telas::comecar`] é chaveada por pessoa e **sobrescreve** a sessão dona da
/// vaga, porque é assim que um `StartScreenShare` depois de reconectar toma a
/// vaga que a conexão anterior tinha: a troca é legítima e é o caso comum.
/// Sobrescrever sem conferir, no entanto, faz a mesma coisa ao contrário — um
/// `StartScreenShare` **atrasado** da conexão velha toma a vaga da nova e, de
/// lambuja, manda `ScreenShareStopped` da tela que a nova acabou de abrir, que é
/// exatamente a imagem parada que esta pendência existe para não deixar
/// acontecer. Achado por revisão independente; inalcançável pela queda silenciosa,
/// em que a conexão velha está muda, mas alcançável quando as duas conexões estão
/// vivas ao mesmo tempo — que é o que a janela de cinco segundos permite.
///
/// A conferência mora aqui, num nome só, e não dentro do laço da sessão, pela
/// mesma razão de [`reservar_o_assento_da_carencia`]: um guarda dentro de uma
/// função que só roda com servidor, QUIC e relógio de verdade é um guarda que
/// nenhum teste alcança.
pub fn comecar_a_tela_da_conexao_vigente(
    presentes: &Presentes,
    telas: &mut Telas,
    voice_room: VoiceRoomId,
    person: PersonId,
    sessao: SessionId,
    screen: ScreenId,
) -> AberturaDeTela {
    if !presentes.e_a_vigente(person, sessao) {
        return AberturaDeTela::DeConexaoVelha;
    }
    AberturaDeTela::Aberta {
        substituida: telas.comecar(voice_room, person, sessao, screen),
    }
}

/// Who is in which voice room at this moment.
///
/// Separate from [`Slots`], which holds seats for people who are *away*. This
/// is who is actually there, and it exists to answer one question the protocol
/// could not: **who was already here before I was watching.**
///
/// `specs/02-protocolo.md` announces arrivals going forward and nothing else,
/// so a person entering an occupied voice room saw an empty room until somebody moved.
/// Gap G15, found by running two clients where the second started after the
/// first had already sat down.
///
/// # Why the whole map, and not one voice room
///
/// G15 was closed for the voice room the person walked into, and only that one. The
/// screen `comp v3` draws occupants under **every** voice room,
/// and for the other four that data had never existed on the client at all:
/// they were drawn empty, always, however many people were in them. Reported
/// from a real session as "o sistema de voice_rooms não está bem implementado,
/// mostra que as salas de voz estão vazias quando não deveriam estar".
///
/// So [`Occupancy::everywhere`] hands back the entire picture, and a connection
/// is given it once, at the start of its session. Everything after that is the
/// unfiltered `PersonJoined` / `PersonLeft` broadcast.
#[derive(Debug, Default)]
pub struct Occupancy {
    by_voice_room: HashMap<VoiceRoomId, Vec<Occupant>>,
}

impl Occupancy {
    /// Seats a person, replacing any earlier seat they held, and says which
    /// voice rooms that emptied.
    ///
    /// Replacing rather than appending: a reconnection inside the grace period
    /// re-enters the same voice room, and a roster with the same person twice is a
    /// roster nobody trusts.
    ///
    /// As salas devolvidas são o que `assentar` precisa para contar a saída à sala
    /// anterior. Vêm daqui, e não de uma chamada separada, porque a única remoção
    /// por pessoa que este tipo ainda faz é esta — e ela é a única correta:
    /// sentar é o momento em que o assento anterior **deve** cair, de qual sessão
    /// for. Todo o resto passa pelas versões com sessão
    /// ([`Self::vacate_da_sessao`], [`Self::vacate_everywhere_da_sessao`]).
    pub fn seat(&mut self, voice_room: VoiceRoomId, occupant: Occupant) -> Vec<VoiceRoomId> {
        let vacated = self.vacate_everywhere(occupant.person);
        // Sem guarda de sessão, e de propósito: sentar alguém tem de tirar
        // **todo** assento anterior dessa pessoa, de qual sessão for, ou a
        // reconexão deixaria a mesma pessoa sentada duas vezes — um roster com o
        // mesmo nome duas vezes é um roster em que ninguém confia.
        self.by_voice_room
            .entry(voice_room)
            .or_default()
            .push(occupant);
        vacated
    }

    /// Quantas pessoas estão nesta sala.
    ///
    /// Existe para o teto contado do ADR 0038: a conta da subida cresce com o
    /// quadrado deste número, e quem a faz precisa dele **depois** de sentar a
    /// pessoa que acabou de entrar.
    #[must_use]
    pub fn quantos(&self, voice_room: VoiceRoomId) -> usize {
        self.by_voice_room
            .get(&voice_room)
            .map_or(0, std::vec::Vec::len)
    }

    /// Tira de uma sala o assento **desta sessão**, e diz se havia o que tirar.
    ///
    /// O `bool` é o que decide se o `PersonLeft` sai: anunciar uma saída que não
    /// aconteceu apaga da tela de todo mundo alguém que está na sala, e é esse o
    /// defeito que o identificador de sessão existe para fechar.
    pub fn vacate_da_sessao(
        &mut self,
        voice_room: VoiceRoomId,
        person: PersonId,
        sessao: SessionId,
    ) -> bool {
        let Some(seated) = self.by_voice_room.get_mut(&voice_room) else {
            return false;
        };
        let antes = seated.len();
        seated.retain(|occupant| occupant.person != person || occupant.sessao != sessao);
        seated.len() != antes
    }

    /// Removes a person from wherever they were, and says where that was.
    ///
    /// The voice_rooms come back because somebody has to announce the departure and
    /// the caller does not always know the room: a session can end at any `?`
    /// in the middle of its loop, and that path has no idea where the person was
    /// sitting. Returning the answer here is what lets one call at the end of a
    /// connection both clear the seat and tell everybody about it — the same
    /// reasoning `crate::voice_room::voice_rooms::leave_everywhere` gives for being
    /// broadcast rather than aimed.
    ///
    /// **Privada, e é a decisão.** Era pública, e enquanto era, cada caminho novo
    /// de desmonte podia chamá-la sem conferir sessão nenhuma — foi assim que o
    /// defeito de campo nasceu seis vezes no mesmo arquivo. O único chamador
    /// legítimo é [`Self::seat`], porque sentar tem de derrubar o assento
    /// anterior de qual sessão for; quem desmonta uma conexão chama
    /// [`Self::vacate_everywhere_da_sessao`], e agora não tem escolha.
    fn vacate_everywhere(&mut self, person: PersonId) -> Vec<VoiceRoomId> {
        let mut vacated = Vec::new();
        for (voice_room, seated) in &mut self.by_voice_room {
            let before = seated.len();
            seated.retain(|occupant| occupant.person != person);
            if seated.len() != before {
                vacated.push(*voice_room);
            }
        }
        vacated
    }

    /// O mesmo, mas só para os assentos **desta sessão**.
    ///
    /// É o que o fim de uma conexão tem de chamar. A versão por pessoa era o
    /// coração do defeito: uma queda silenciosa põe duas sessões da mesma pessoa
    /// vivas ao mesmo tempo, e a velha, ao morrer aos 20 s, desocupava o assento
    /// que a nova tinha tomado aos 15 s.
    pub fn vacate_everywhere_da_sessao(
        &mut self,
        person: PersonId,
        sessao: SessionId,
    ) -> Vec<VoiceRoomId> {
        let mut vacated = Vec::new();
        for (voice_room, seated) in &mut self.by_voice_room {
            let before = seated.len();
            seated.retain(|occupant| occupant.person != person || occupant.sessao != sessao);
            if seated.len() != before {
                vacated.push(*voice_room);
            }
        }
        vacated
    }

    /// Who is in a voice room, in the order they arrived.
    #[must_use]
    pub fn in_voice_room(&self, voice_room: VoiceRoomId) -> Vec<Occupant> {
        self.by_voice_room
            .get(&voice_room)
            .cloned()
            .unwrap_or_default()
    }

    /// Em que sala esta pessoa está sentada agora, se alguma.
    ///
    /// Quem precisa disto é a retirada de consentimento no caminho entre pares:
    /// quem deixa de emprestar encerra os repasses que fazia, e cada espectador
    /// órfão volta a ser servido **pela sala em que ele está**. A sala de quem
    /// retirou não serve: a declaração de par é global ao daemon, e pessoas
    /// trocam de sala sem redeclarar nada.
    #[must_use]
    pub fn onde_esta(&self, person: PersonId) -> Option<VoiceRoomId> {
        self.by_voice_room
            .iter()
            .find(|(_, seated)| seated.iter().any(|occupant| occupant.person == person))
            .map(|(voice_room, _)| *voice_room)
    }

    /// Everybody seated anywhere, with the voice room they are seated in.
    ///
    /// Flattened rather than handed back as a map, because the only caller
    /// walks it once to write a frame per occupant, and a map would make that
    /// caller nest two loops to say one thing.
    #[must_use]
    pub fn everywhere(&self) -> Vec<(VoiceRoomId, Occupant)> {
        self.by_voice_room
            .iter()
            .flat_map(|(voice_room, seated)| {
                seated
                    .iter()
                    .map(|occupant| (*voice_room, occupant.clone()))
            })
            .collect()
    }
}

/// How far behind the event bus a session was allowed to fall, counted.
///
/// The bus is a fixed ring ([`broadcast`]). A session that stops draining it —
/// because it is stuck writing to a peer that stopped reading — eventually falls
/// off the back of the ring, and `recv` reports `Lagged(n)`: **n events that
/// existed and no longer do**, for that connection. Committed messages among
/// them are gone from that person's view of the conversation for as long as the
/// session lasts.
///
/// It used to be swallowed by a `let Ok(event) = event else { continue }`, which
/// is why `docs/pendencias.md` #1 could be measured from the outside and never
/// explained: nothing anywhere counted it. A number that can be read is the
/// difference between "the burst lost messages" and "the burst lost 371 events
/// on this connection at this second".
///
/// Process-wide rather than per-session on purpose: an operator reading a log
/// wants to know whether this server has ever done it at all, and a counter that
/// dies with the connection that incremented it answers that with silence.
#[derive(Debug, Default)]
pub struct Atrasos {
    eventos: std::sync::atomic::AtomicU64,
    sessoes: std::sync::atomic::AtomicU64,
}

impl Atrasos {
    /// Records one session falling `quantos` events behind.
    pub fn registrar(&self, quantos: u64) {
        self.eventos
            .fetch_add(quantos, std::sync::atomic::Ordering::Relaxed);
        self.sessoes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// How many events the bus dropped before a session could read them.
    #[must_use]
    pub fn eventos(&self) -> u64 {
        self.eventos.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// How many sessions it happened to. Zero is the only good answer.
    #[must_use]
    pub fn sessoes(&self) -> u64 {
        self.sessoes.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Quantas conexões velhas morreram sem levar ninguém junto.
///
/// **Existe porque o conserto, funcionando, não deixa rastro nenhum.** O guarda
/// de sessão acerta calando-se: a sessão velha morre, não encontra nada seu para
/// remover, e todo mundo continua exatamente onde estava. Do lado de fora, isso é
/// indistinguível de a sessão velha nunca ter morrido — e um teste que não
/// distingue as duas coisas passa por ausência de evento, e não por defesa.
///
/// Este número é a diferença entre as duas. Cada unidade aqui é uma conexão que
/// chegou ao fim depois de a mesma pessoa já ter reconectado por outra: a queda
/// silenciosa de rede que o `docs/pendencias.md` #11 descreve. Zero é o normal
/// numa rede que não cai; um número que cresce durante uma chamada é a rede de
/// alguém piscando — e, antes do guarda, era essa pessoa ficando muda e invisível
/// para quem ficou, sem nenhum sinal para ela própria.
///
/// Do processo inteiro e não por sessão, pela mesma razão que [`Atrasos`]: um
/// contador que morre com a conexão que o incrementou responde com silêncio a
/// «este servidor já fez isto alguma vez?».
#[derive(Debug, Default)]
pub struct Desassentamentos {
    conexoes_velhas: std::sync::atomic::AtomicU64,
}

impl Desassentamentos {
    /// Registra uma conexão que morreu já não sendo a vigente da pessoa dela.
    pub fn conexao_velha(&self) {
        self.conexoes_velhas
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Quantas foram até agora.
    #[must_use]
    pub fn conexoes_velhas(&self) -> u64 {
        self.conexoes_velhas
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// One message on its way to the batch, with somewhere to report the outcome.
pub struct WriteRequest {
    /// What to write.
    pub message: PendingMessage,
}

/// Everything a connection needs from the server.
pub struct Server {
    /// Persistent state. One connection, one mutex — SQLite has one writer.
    pub persistence: Arc<Mutex<Persistence>>,
    /// The event bus.
    pub events: broadcast::Sender<Event>,
    /// Where messages go to be batched.
    pub writes: mpsc::Sender<WriteRequest>,
    /// Seats held for people who are expected back.
    pub slots: Arc<Mutex<Slots>>,
    /// Who is sitting in which voice room right now — gap G15.
    pub occupancy: Arc<Mutex<Occupancy>>,
    /// Quem está conectado, sentado ou não. Ver [`Presentes`].
    pub presentes: Arc<Mutex<Presentes>>,
    /// A subida deste servidor, medida enquanto ele empurra cópias.
    ///
    /// Compartilhada porque é do **cano**, e o cano é um só: cada sessão vê a
    /// fatia dela e nunca o total. Ver [`crate::tela::Subida`].
    pub subida: Arc<Mutex<crate::tela::Subida>>,
    /// Quantos apertos de mão cada endereço ainda pode gastar.
    ///
    /// Antes de autenticar, portanto sem identidade nenhuma para contar: a
    /// chave é o endereço de origem. Ver [`crate::taxa`].
    pub portaria: Arc<Mutex<crate::taxa::Portaria>>,
    /// How often the bus outran a session. See [`Atrasos`].
    pub atrasos: Arc<Atrasos>,
    /// Quantas conexões velhas o guarda de sessão já defendeu. Ver
    /// [`Desassentamentos`].
    pub desassentamentos: Arc<Desassentamentos>,
    /// Quem está compartilhando tela em cada sala de voz. Ver [`Telas`].
    pub telas: Arc<Mutex<Telas>>,
    /// Quem declarou que empresta a subida, e quem serve quem. Ver
    /// [`crate::pares::Pares`].
    pub pares: Arc<Mutex<crate::pares::Pares>>,
    /// The attachment store, its ceiling, and the byte budget. ADR 0027.
    ///
    /// `None` when this server has nowhere to keep blobs, which is the in-memory
    /// case. A transfer then meets `AttachmentRefusal::Unavailable` — a
    /// sentence — rather than a directory appearing wherever the process
    /// started.
    pub anexos: Option<Arc<crate::transfer::Vault>>,
    /// Quanto a subida desta máquina carrega, em bits por segundo, ou `None`.
    ///
    /// A cópia de `ServerConfig::caminho_bps` que o resto do daemon alcança, e
    /// ela mora aqui pela mesma razão que [`Self::telas`]: é um fato sobre
    /// **este servidor** que duas partes distantes precisam, e a alternativa era
    /// passar a configuração inteira por assinaturas que já estão cheias.
    ///
    /// Duas leituras saem daqui e elas discordam de propósito — a admissão de
    /// [`crate::voice_room::VoiceRoom`] cai numa hipótese quando isto é `None`, e o
    /// `HostUplink` que a sessão escreve manda **zero**, que pelo protocolo é
    /// «não medi». Ver `crate::tela::caminho_no_fio`.
    pub caminho_bps: Option<u32>,
    /// A partir de que versão do protocolo o anúncio de MODs sai.
    ///
    /// A cópia de [`crate::ServerConfig::versao_do_anuncio`] que o aperto de
    /// mão e o laço de sessão alcançam, e ela mora aqui pela mesma razão que
    /// [`Self::caminho_bps`]. O que ela decide, e onde: se o portão do ADR 0045
    /// está de pé ou dormente — [`crate::mods::anuncio::o_anuncio_alcanca_alguem`] —
    /// e, quando de pé, qual par é velho demais para ser perguntado.
    ///
    /// **No padrão ele está de pé desde 14/09/2026**, quando `PROTOCOL_VERSION`
    /// alcançou [`seele_proto::mods::VERSAO_DO_ANUNCIO`]. Quem é velho demais
    /// para ser perguntado, hoje, é o par da v4 — a única versão anterior que a
    /// janela de compatibilidade ainda alcança. O par da v3 da release
    /// `v0.10.5-1` não chega até este limiar: ele é recusado antes, no aperto de
    /// mão, com `PeerTooOld`, num servidor com MOD habilitado ou sem.
    pub versao_do_anuncio: u8,
}

/// Starts the batching writer.
///
/// Collects messages until [`FLUSH_INTERVAL`] elapses, writes them in one
/// transaction, and only then broadcasts them. See the module docs on why the
/// order is fixed.
pub fn spawn_writer(
    persistence: Arc<Mutex<Persistence>>,
    events: broadcast::Sender<Event>,
) -> mpsc::Sender<WriteRequest> {
    let (tx, mut rx) = mpsc::channel::<WriteRequest>(1024);

    tokio::spawn(async move {
        let mut pending: Vec<PendingMessage> = Vec::new();
        let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                request = rx.recv() => {
                    match request {
                        Some(request) => pending.push(request.message),
                        // The server is shutting down. Flush what is left rather
                        // than dropping messages the clients believe are queued.
                        None => {
                            flush(&persistence, &events, &mut pending).await;
                            return;
                        }
                    }
                }
                _ = ticker.tick() => {
                    flush(&persistence, &events, &mut pending).await;
                }
            }
        }
    });

    tx
}

async fn flush(
    persistence: &Arc<Mutex<Persistence>>,
    events: &broadcast::Sender<Event>,
    pending: &mut Vec<PendingMessage>,
) {
    if pending.is_empty() {
        return;
    }
    let batch = std::mem::take(pending);
    let stored = {
        let mut guard = persistence.lock().await;
        let mut messages = Messages::new(&mut guard);
        match messages.append_batch(&batch) {
            Ok(stored) => stored,
            Err(error) => {
                // Losing the batch is bad; losing it silently is worse. The
                // clients will not see their messages appear, which is the
                // honest outcome of a write that failed.
                tracing::error!(%error, count = batch.len(), "message batch failed");
                return;
            }
        }
    };

    // Committed, therefore durable, therefore safe to announce.
    for message in stored {
        let _ = events.send(Event::MessagePosted(message));
    }
}

impl Server {
    /// Queues a message for the next batch.
    ///
    /// Returns once it is queued, not once it is durable. The caller must not
    /// confirm anything to the client here; the broadcast after the commit is
    /// what does that.
    ///
    /// # Errors
    ///
    /// Fails if the writer task has stopped.
    pub async fn post(&self, message: PendingMessage) -> Result<()> {
        self.writes.send(WriteRequest { message }).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_seat_is_held_for_the_grace_period() {
        // specs/02-protocolo.md: the server holds the slot for the same five
        // minutes as the client's internal battery.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), SessionId(1), VoiceRoomId(1), Ssrc(7), now);

        let reclaimed = slots.reclaim(PersonId(1), now + Duration::from_secs(60));
        assert_eq!(reclaimed, Some((VoiceRoomId(1), Ssrc(7))));
    }

    #[test]
    fn a_reconnecting_person_gets_their_own_ssrc_back() {
        // Otherwise a sixty-second outage looks to everybody else like the person
        // left and a stranger arrived, and every listener's jitter buffer starts
        // from scratch.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), SessionId(1), VoiceRoomId(2), Ssrc(42), now);

        let (voice_room, ssrc) = slots.reclaim(PersonId(1), now).expect("seat held");
        assert_eq!(voice_room, VoiceRoomId(2));
        assert_eq!(ssrc, Ssrc(42));
    }

    #[test]
    fn an_expired_seat_is_not_reclaimable() {
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), SessionId(1), VoiceRoomId(1), Ssrc(7), now);

        let after = now + seele_proto::transport::SESSION_GRACE + Duration::from_secs(1);
        assert_eq!(slots.reclaim(PersonId(1), after), None);
    }

    #[test]
    fn reclaiming_twice_only_works_once() {
        // The seat is taken by the reconnection. A second claim would let one
        // person occupy two.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), SessionId(1), VoiceRoomId(1), Ssrc(7), now);

        assert!(slots.reclaim(PersonId(1), now).is_some());
        assert!(slots.reclaim(PersonId(1), now).is_none());
    }

    #[test]
    fn the_sweeper_frees_expired_seats() {
        // Without this a server slowly fills with seats held for people who are
        // never coming back, and specs/04 caps a voice room at a member limit.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), SessionId(1), VoiceRoomId(1), Ssrc(1), now);
        slots.reserve(PersonId(2), SessionId(2), VoiceRoomId(1), Ssrc(2), now);
        assert_eq!(slots.held(), 2);

        let after = now + seele_proto::transport::SESSION_GRACE + Duration::from_secs(1);
        assert_eq!(slots.sweep(after), 2);
        assert_eq!(slots.held(), 0);
    }

    #[test]
    fn the_sweeper_leaves_live_seats_alone() {
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), SessionId(1), VoiceRoomId(1), Ssrc(1), now);
        assert_eq!(slots.sweep(now + Duration::from_secs(30)), 0);
        assert_eq!(slots.held(), 1);
    }

    // ---- who is in which voice room ----

    fn occupant(person: u64, nickname: &str) -> Occupant {
        sentado(person, nickname, SessionId(person))
    }

    /// O mesmo, com a sessão escolhida: é o que separa duas conexões da mesma
    /// pessoa, que é o caso que o guarda existe para atender.
    fn sentado(person: u64, nickname: &str, sessao: SessionId) -> Occupant {
        Occupant {
            person: PersonId(person),
            nickname: nickname.to_owned(),
            ssrc: Ssrc(u32::try_from(person * 10).expect("ssrc")),
            sessao,
        }
    }

    #[test]
    fn the_whole_server_is_readable_at_once_and_not_one_room_at_a_time() {
        // The half of gap G15 that was missing. `in_voice_room` answered "who is in
        // the room I am walking into"; nothing answered "who is in the other
        // four", and the v3 layout draws those four with their occupants under
        // them. They were drawn empty however many people were in them.
        let mut occupancy = Occupancy::default();
        occupancy.seat(VoiceRoomId(1), occupant(1, "marcela"));
        occupancy.seat(VoiceRoomId(1), occupant(2, "rafael"));
        occupancy.seat(VoiceRoomId(2), occupant(3, "carla"));

        let mut everywhere: Vec<(u32, u64)> = occupancy
            .everywhere()
            .into_iter()
            .map(|(voice_room, seated)| (voice_room.0, seated.person.0))
            .collect();
        everywhere.sort_unstable();

        assert_eq!(everywhere, [(1, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn onde_uma_pessoa_esta_sentada_e_perguntavel_pelo_nome_dela() {
        // Quem precisa disto é a retirada de consentimento no caminho entre
        // pares: quem deixa de emprestar encerra os repasses que fazia, e cada
        // espectador órfão tem de voltar a ser servido **pela sala em que ele
        // está** — que não é necessariamente a de quem retirou, porque pessoas
        // trocam de sala sem redeclarar nada.
        let mut occupancy = Occupancy::default();
        occupancy.seat(VoiceRoomId(1), occupant(1, "marcela"));
        occupancy.seat(VoiceRoomId(2), occupant(3, "carla"));

        assert_eq!(occupancy.onde_esta(PersonId(1)), Some(VoiceRoomId(1)));
        assert_eq!(occupancy.onde_esta(PersonId(3)), Some(VoiceRoomId(2)));
        assert_eq!(occupancy.onde_esta(PersonId(9)), None);
    }

    #[test]
    fn leaving_says_which_rooms_were_left() {
        // The caller that needs this is the end of a connection, which does not
        // know where the person was sitting: a session can end at any `?`. If
        // this said nothing, the departure could not be announced, and the
        // person would stay on everybody's screen until they came back.
        let mut occupancy = Occupancy::default();
        occupancy.seat(VoiceRoomId(7), occupant(1, "marcela"));

        assert_eq!(
            occupancy.vacate_everywhere(PersonId(1)),
            vec![VoiceRoomId(7)]
        );
        assert!(occupancy.everywhere().is_empty());
    }

    #[test]
    fn a_sessao_velha_nao_desocupa_o_assento_que_a_nova_tomou() {
        // **O defeito de campo, em três linhas.** Numa queda silenciosa a conexão
        // nova da mesma pessoa sobe perto dos 15 s e a velha só é desmontada aos
        // 20 s, quando o tempo ocioso do transporte estoura. Chaveado só por
        // pessoa, o desmonte da velha apagava o assento da nova — e quem foi
        // apagado não recebe `PersonLeft`, então continua se vendo na sala
        // enquanto desapareceu da de todo mundo.
        let mut occupancy = Occupancy::default();
        occupancy.seat(VoiceRoomId(1), sentado(1, "marcela", SessionId(7)));
        // A reconexão: a mesma pessoa, outra conexão. Sentar substitui, e não
        // acumula — um roster com o mesmo nome duas vezes é um roster em que
        // ninguém confia.
        occupancy.seat(VoiceRoomId(1), sentado(1, "marcela", SessionId(8)));

        assert!(
            occupancy
                .vacate_everywhere_da_sessao(PersonId(1), SessionId(7))
                .is_empty(),
            "a conexão velha desocupou o assento da nova, e ainda anunciaria a saída"
        );
        assert_eq!(occupancy.in_voice_room(VoiceRoomId(1)).len(), 1);

        // E a conexão vigente continua podendo sair.
        assert_eq!(
            occupancy.vacate_everywhere_da_sessao(PersonId(1), SessionId(8)),
            vec![VoiceRoomId(1)]
        );
    }

    #[test]
    fn a_sessao_velha_nao_tira_da_sala_o_assento_que_a_nova_tomou() {
        // O mesmo, pelo caminho de uma sala só: é o que `SairDaVoiceRoom` e a
        // sala apagada usam, e eram duas cópias parciais desta contabilidade.
        let mut occupancy = Occupancy::default();
        occupancy.seat(VoiceRoomId(3), sentado(1, "marcela", SessionId(7)));
        occupancy.seat(VoiceRoomId(3), sentado(1, "marcela", SessionId(8)));

        assert!(
            !occupancy.vacate_da_sessao(VoiceRoomId(3), PersonId(1), SessionId(7)),
            "a saída da conexão velha valeu, e o `PersonLeft` dela apagaria da tela \
             de todo mundo alguém que está na sala"
        );
        assert!(occupancy.vacate_da_sessao(VoiceRoomId(3), PersonId(1), SessionId(8)));
    }

    #[test]
    fn a_sessao_velha_nao_tira_dos_presentes_quem_esta_conectado() {
        // Sem isto, o `PersonGone` da conexão velha apagava a pessoa da lista de
        // todo mundo — e `translate` não manda `PersonGone` para a própria
        // pessoa, de modo que ela nunca saberia que foi apagada.
        let mut presentes = Presentes::default();
        assert!(presentes.chegou(sentado(1, "marcela", SessionId(7))));
        // A reconexão não é uma chegada nova: é a mesma pessoa, e um segundo
        // anúncio faria a lista de todo mundo piscar sem nada ter mudado.
        assert!(!presentes.chegou(sentado(1, "marcela", SessionId(8))));

        assert!(
            !presentes.saiu(PersonId(1), SessionId(7)),
            "a conexão velha tirou dos presentes quem está conectado agora"
        );
        assert_eq!(presentes.todos().len(), 1);
        assert!(presentes.saiu(PersonId(1), SessionId(8)));
    }

    #[test]
    fn quem_reconecta_passa_a_ser_a_vigente_e_a_anterior_perde_a_autoridade() {
        // A invariante de que todo o resto desta família depende, e que uma
        // revisão independente apontou não estar presa por teste nenhum:
        // `chegou` não compara sessão, então é a **chegada** que decide quem é a
        // conexão vigente de uma pessoa. Se ela deixasse de sobrescrever, os
        // guardas continuariam «funcionando» e apontariam para a conexão errada:
        // a velha seguiria vigente, poderia desocupar e desistir de reservar
        // assento pela nova, e nenhum dos testes acima reclamaria — eles só
        // olham `saiu`.
        let mut presentes = Presentes::default();
        assert!(presentes.chegou(sentado(1, "marcela", SessionId(7))));
        assert!(presentes.e_a_vigente(PersonId(1), SessionId(7)));

        assert!(!presentes.chegou(sentado(1, "marcela", SessionId(8))));
        assert!(
            presentes.e_a_vigente(PersonId(1), SessionId(8)),
            "a conexão que acabou de reconectar não é a vigente desta pessoa, e \
             tudo o que ela fizer daqui para a frente será recusado como se \
             fosse de uma sessão morta"
        );
        assert!(
            !presentes.e_a_vigente(PersonId(1), SessionId(7)),
            "a conexão velha continua vigente depois de a nova chegar, e é ela \
             quem vai poder desocupar e anunciar a saída de quem está aqui"
        );
    }

    #[test]
    fn sem_registro_de_presenca_ninguem_e_a_vigente() {
        // O ramo que a primeira versão deixava permissivo. Ele não protege quem
        // morreu antes de se anunciar presente — essa conexão nunca entrou em
        // sala e nunca chega a perguntar isto —; ele só disparava quando o
        // registro já tinha sido tirado por uma sessão posterior.
        let presentes = Presentes::default();
        assert!(!presentes.e_a_vigente(PersonId(1), SessionId(7)));
    }

    #[test]
    fn a_sessao_velha_nao_reserva_assento_depois_de_a_nova_ter_saido_de_proposito() {
        // O encadeamento que sobrou da primeira versão deste guarda, e que uma
        // revisão encontrou: a sessão 7 cai calada; a 8 reconecta e toma o
        // registro de presença; a 8 sai da sala de propósito e se desconecta,
        // levando o registro embora; só então a 7 morre, até vinte segundos
        // depois. Sem registro a que se comparar, a 7 gravava uma reserva de
        // cinco minutos com a sala e o `ssrc` velhos, e a conexão seguinte
        // re-sentava na sala quem tinha saído por vontade própria.
        let mut presentes = Presentes::default();
        assert!(presentes.chegou(sentado(1, "marcela", SessionId(7))));
        assert!(!presentes.chegou(sentado(1, "marcela", SessionId(8))));
        assert!(presentes.saiu(PersonId(1), SessionId(8)));

        let mut slots = Slots::default();
        assert!(
            !reservar_o_assento_da_carencia(
                &presentes,
                &mut slots,
                PersonId(1),
                SessionId(7),
                VoiceRoomId(3),
                Ssrc(70),
                Instant::now(),
            ),
            "a conexão velha guardou assento para quem tinha acabado de sair de propósito"
        );
        assert_eq!(slots.held(), 0);
        assert_eq!(slots.reclaim(PersonId(1), Instant::now()), None);
    }

    #[test]
    fn a_sessao_velha_nao_reserva_o_assento_de_quem_ja_voltou() {
        // A reserva é da mesma família do resto: ela vale cinco minutos e
        // re-senta a pessoa na conexão seguinte. Gravada pela conexão velha que
        // morre depois da volta, ela põe de novo na sala quem tinha acabado de
        // sair de propósito — e com o `ssrc` errado.
        let mut presentes = Presentes::default();
        assert!(presentes.chegou(sentado(1, "marcela", SessionId(8))));
        let mut slots = Slots::default();

        assert!(
            !reservar_o_assento_da_carencia(
                &presentes,
                &mut slots,
                PersonId(1),
                SessionId(7),
                VoiceRoomId(3),
                Ssrc(70),
                Instant::now(),
            ),
            "a conexão velha guardou um assento para uma pessoa que já voltou por outra"
        );
        assert_eq!(slots.held(), 0);

        // E o outro lado, que é o que impede o guarda de cobrar caro de quem
        // caiu de verdade: a conexão vigente reserva.
        assert!(reservar_o_assento_da_carencia(
            &presentes,
            &mut slots,
            PersonId(1),
            SessionId(8),
            VoiceRoomId(3),
            Ssrc(10),
            Instant::now(),
        ));
        assert_eq!(
            slots.reclaim(PersonId(1), Instant::now()),
            Some((VoiceRoomId(3), Ssrc(10)))
        );
    }

    #[test]
    fn a_reserva_escrita_depois_da_volta_e_descartada_por_quem_voltou() {
        // A janela que o guarda da reserva não alcança: a conexão nova resgata o
        // assento no `handshake` e só depois entra em `Presentes`. Uma conexão
        // velha que morra no meio disso ainda é «a vigente» e grava a reserva —
        // com a sala e o `ssrc` velhos, valendo cinco minutos. Quem volta
        // descarta, porque o resgate dela já aconteceu e nada legítimo pode ter
        // sido escrito depois.
        const VELHA: SessionId = SessionId(1);
        const NOVA: SessionId = SessionId(2);

        let mut slots = Slots::default();
        slots.reserve(PersonId(1), VELHA, VoiceRoomId(3), Ssrc(70), Instant::now());

        assert!(
            descartar_a_reserva_de_quem_ja_voltou(&mut slots, PersonId(1), NOVA),
            "a reserva obsoleta da conexão velha sobreviveu à volta da pessoa, e \
             cinco minutos depois ela é re-sentada numa sala de onde saiu"
        );
        assert_eq!(slots.held(), 0);
        assert_eq!(slots.reclaim(PersonId(1), Instant::now()), None);

        // E o outro lado, para que o teste não passe por descartar sempre: sem
        // reserva nenhuma não há o que descartar, e ninguém mente dizendo que
        // descartou. A reserva de outra pessoa fica onde está.
        assert!(!descartar_a_reserva_de_quem_ja_voltou(
            &mut slots,
            PersonId(1),
            NOVA
        ));
        slots.reserve(PersonId(2), VELHA, VoiceRoomId(3), Ssrc(20), Instant::now());
        assert!(!descartar_a_reserva_de_quem_ja_voltou(
            &mut slots,
            PersonId(1),
            NOVA
        ));
        assert_eq!(slots.held(), 1);
    }

    #[test]
    fn a_reserva_da_propria_sessao_vigente_nao_e_descartada() {
        // O descarte era o único guarda desta família que se apoiava só na ordem
        // dos acontecimentos: «o resgate já foi, logo o que existe é de outra
        // conexão». Chaveado por sessão, ele responde à pergunta certa — quem
        // escreveu — e uma reserva escrita pela conexão vigente sobrevive, mesmo
        // que uma mudança futura passe a gravá-la antes deste ponto.
        const VIGENTE: SessionId = SessionId(9);

        let mut slots = Slots::default();
        slots.reserve(
            PersonId(1),
            VIGENTE,
            VoiceRoomId(3),
            Ssrc(70),
            Instant::now(),
        );

        assert!(
            !descartar_a_reserva_de_quem_ja_voltou(&mut slots, PersonId(1), VIGENTE),
            "o descarte comeu a reserva da própria conexão vigente, que é o \
             assento que ela espera resgatar quando cair"
        );
        assert_eq!(
            slots.reclaim(PersonId(1), Instant::now()),
            Some((VoiceRoomId(3), Ssrc(70)))
        );
    }

    #[test]
    fn a_tela_pedida_pela_conexao_velha_nao_toma_a_vaga_da_nova() {
        // A última escrita por pessoa desta família. A vaga de tela é
        // sobrescrita de propósito — é assim que quem reconecta toma a vaga da
        // conexão anterior —, e sem conferir a sessão a mesma porta serve ao
        // contrário: um `StartScreenShare` atrasado da conexão velha toma a vaga
        // da nova e ainda manda `ScreenShareStopped` da tela que a nova acabou
        // de abrir, apagando da tela de quem assiste a transmissão viva.
        let mut presentes = Presentes::default();
        assert!(presentes.chegou(sentado(10, "marcela", SessionId(7))));
        assert!(!presentes.chegou(sentado(10, "marcela", SessionId(8))));

        let mut telas = Telas::default();
        assert_eq!(
            comecar_a_tela_da_conexao_vigente(
                &presentes,
                &mut telas,
                VoiceRoomId(1),
                PersonId(10),
                SessionId(8),
                ScreenId(2),
            ),
            AberturaDeTela::Aberta { substituida: None }
        );

        assert_eq!(
            comecar_a_tela_da_conexao_vigente(
                &presentes,
                &mut telas,
                VoiceRoomId(1),
                PersonId(10),
                SessionId(7),
                ScreenId(1),
            ),
            AberturaDeTela::DeConexaoVelha,
            "a conexão velha tomou a vaga de tela da nova, e quem chama vai \
             anunciar o fim da transmissão que está no ar"
        );
        // E nada foi escrito: a vaga continua sendo da conexão nova, com a tela
        // dela.
        assert_eq!(
            telas.de(PersonId(10), SessionId(8)),
            Some((VoiceRoomId(1), ScreenId(2)))
        );
        assert_eq!(telas.em(VoiceRoomId(1)), vec![(PersonId(10), ScreenId(2))]);
        assert_eq!(telas.de(PersonId(10), SessionId(7)), None);

        // E o outro lado, para que o teste não passe por recusar sempre: a
        // conexão vigente troca a própria tela e recebe de volta a que saiu.
        assert_eq!(
            comecar_a_tela_da_conexao_vigente(
                &presentes,
                &mut telas,
                VoiceRoomId(1),
                PersonId(10),
                SessionId(8),
                ScreenId(3),
            ),
            AberturaDeTela::Aberta {
                substituida: Some(ScreenId(2))
            }
        );
    }

    #[test]
    fn quem_troca_a_propria_tela_recebe_de_volta_a_que_saiu() {
        // A vaga de tela é o único estado do desmonte cuja sessão é sobrescrita
        // em vez de conferida — e sobrescrever calado deixa o `ScreenId` antigo
        // desenhado para sempre em quem assiste, porque o cliente funde
        // aditivamente. Devolvendo a tela trocada, quem chama anuncia o fim dela.
        let mut telas = Telas::default();

        assert_eq!(
            telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(7), ScreenId(1)),
            None,
            "a primeira transmissão não substituiu transmissão nenhuma e disse que sim"
        );
        assert_eq!(
            telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(8), ScreenId(2)),
            Some(ScreenId(1)),
            "a conexão nova tomou a vaga da velha em silêncio: o cabeçalho da \
             transmissão antiga fica na tela de quem assiste prometendo um fluxo \
             que já não tem de onde vir"
        );
        // E a vaga é uma só, da sessão nova.
        assert_eq!(telas.em(VoiceRoomId(1)).len(), 1);
        assert_eq!(
            telas.de(PersonId(10), SessionId(8)),
            Some((VoiceRoomId(1), ScreenId(2)))
        );
    }

    #[test]
    fn leaving_a_room_nobody_was_in_announces_nothing() {
        // The other half, and the one that keeps a departure from being sent
        // twice: `serve` calls this after every session, including the ones that
        // already left through `LeaveVoiceRoom` and said so.
        let mut occupancy = Occupancy::default();
        occupancy.seat(VoiceRoomId(7), sentado(1, "marcela", SessionId(7)));
        assert!(
            occupancy.vacate_da_sessao(VoiceRoomId(7), PersonId(1), SessionId(7)),
            "havia o que desocupar e a saída disse que não"
        );
        assert!(
            !occupancy.vacate_da_sessao(VoiceRoomId(7), PersonId(1), SessionId(7)),
            "a segunda saída inventou uma despedida que já tinha sido dita"
        );

        assert!(
            occupancy.vacate_everywhere(PersonId(1)).is_empty(),
            "a person who had already left was announced as leaving again"
        );
    }

    #[test]
    fn walking_between_rooms_reports_the_room_that_was_left() {
        // What `EnterVoiceRoom` needs in order to tell the old room. Seating alone
        // clears the previous seat silently, and a silent clear is a person who
        // stays in the first voice room on every other client for ever.
        let mut occupancy = Occupancy::default();
        occupancy.seat(VoiceRoomId(1), occupant(1, "marcela"));

        assert_eq!(
            occupancy.vacate_everywhere(PersonId(1)),
            vec![VoiceRoomId(1)]
        );
        occupancy.seat(VoiceRoomId(2), occupant(1, "marcela"));

        assert_eq!(occupancy.in_voice_room(VoiceRoomId(1)).len(), 0);
        assert_eq!(occupancy.in_voice_room(VoiceRoomId(2)).len(), 1);
    }
    // ---- compartilhamento de tela ----

    #[test]
    fn o_registro_nao_limita_o_numero_de_transmissoes() {
        // **Quem limita é a medida, não este registro.**
        //
        // O número já foi 1 e já foi 2. Os dois estavam errados pela mesma razão:
        // a subida que uma transmissão a mais consome depende da casa de quem
        // hospeda e de quantas pessoas estão assistindo, e nenhuma constante
        // sabe disso.
        //
        // Quem recusa é `VoiceRoom::tela_abriu`, quando o teto por cópia não cabe
        // mais acima do piso — e ele recusa com `AlemDoQueOHospedeiroCarrega`,
        // que é uma frase que explica. Aqui só se guarda quem está transmitindo.
        let mut telas = Telas::default();
        for pessoa in 10_u32..20 {
            let _ = telas.comecar(
                VoiceRoomId(1),
                PersonId(u64::from(pessoa)),
                SessionId(u64::from(pessoa)),
                ScreenId(pessoa),
            );
        }
        assert_eq!(
            telas.em(VoiceRoomId(1)).len(),
            10,
            "o registro não é o lugar de dizer não"
        );

        // O que ele ainda recusa é a mesma pessoa ocupando duas vagas: uma
        // pessoa manda **uma** tela, e mandar duas dobraria a subida dela sem
        // que ninguém tivesse pedido a segunda.
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(10), ScreenId(99));
        assert_eq!(
            telas.em(VoiceRoomId(1)).len(),
            10,
            "trocar a própria não abre vaga nova"
        );
    }

    #[test]
    fn quem_ja_transmite_pedindo_de_novo_troca_a_propria_tela() {
        // Um cliente que reabriu o botão, ou um `StartScreenShare` depois de
        // reconectar. Devolver recusa para a própria pessoa seria dizer que ela
        // perdeu uma vaga para si mesma — e ocupar uma vaga nova gastaria a
        // segunda com a mesma pessoa, tirando-a de quem ainda não transmitiu.
        let mut telas = Telas::default();
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(10), ScreenId(1));
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(10), ScreenId(2));

        assert_eq!(
            telas.em(VoiceRoomId(1)),
            vec![(PersonId(10), ScreenId(2))],
            "trocou a tela e continua ocupando uma vaga só"
        );
    }

    #[test]
    fn parar_a_tela_de_outra_pessoa_nao_para_nada() {
        // Sem esta conferência, um `StopScreenShare` de qualquer pessoa da sala
        // derruba a tela de quem está transmitindo — e o verbo não carrega sala
        // nem `ScreenId` de propósito, então não há nada além do registro para
        // separar as duas.
        let mut telas = Telas::default();
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(10), ScreenId(1));
        let _ = telas.comecar(VoiceRoomId(2), PersonId(20), SessionId(20), ScreenId(2));

        assert_eq!(
            telas.parar(VoiceRoomId(1), PersonId(30), SessionId(30)),
            None
        );
        assert_eq!(telas.em(VoiceRoomId(1)).len(), 1, "ninguém saiu");

        assert_eq!(
            telas.parar(VoiceRoomId(1), PersonId(10), SessionId(10)),
            Some(ScreenId(1))
        );
        assert!(telas.em(VoiceRoomId(1)).is_empty());
        assert_eq!(
            telas.em(VoiceRoomId(2)),
            vec![(PersonId(20), ScreenId(2))],
            "parar uma não pode derrubar a de outra sala"
        );
    }

    #[test]
    fn a_tela_de_quem_sai_para_junto_com_ele_e_so_a_dele() {
        // O mesmo defeito que `Occupancy::vacate_everywhere_da_sessao` conserta para a
        // pessoa fantasma, com a diferença de que aqui a sala fica prometendo
        // imagem em movimento que não tem mais de onde vir: o fluxo morreu com a
        // conexão.
        let mut telas = Telas::default();
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(10), ScreenId(1));
        let _ = telas.comecar(VoiceRoomId(2), PersonId(10), SessionId(10), ScreenId(3));
        let _ = telas.comecar(VoiceRoomId(3), PersonId(20), SessionId(20), ScreenId(2));

        let encerradas = telas.encerrar_de(PersonId(10), None);
        assert_eq!(encerradas.len(), 2, "as duas salas em que ela transmitia");
        assert!(encerradas.contains(&(VoiceRoomId(1), ScreenId(1))));
        assert!(encerradas.contains(&(VoiceRoomId(2), ScreenId(3))));

        assert!(telas.em(VoiceRoomId(1)).is_empty());
        assert!(telas.em(VoiceRoomId(2)).is_empty());
        assert_eq!(
            telas.em(VoiceRoomId(3)),
            vec![(PersonId(20), ScreenId(2))],
            "a de quem ficou não pode cair junto"
        );
        assert!(telas.encerrar_de(PersonId(10), None).is_empty());
    }

    #[test]
    fn a_sessao_velha_nao_apaga_a_tela_que_a_nova_abriu() {
        // A quarta das quatro operações do desmonte, e a que explica a metade
        // «não vejo esse amigo» do relato. Quem reconecta e volta a compartilhar
        // ocupa a mesma vaga com outra sessão; a conexão velha, ao morrer,
        // encerrava a transmissão nova e a sala ficava com a tela apagada sem
        // ninguém saber por quê.
        let mut telas = Telas::default();
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(7), ScreenId(1));
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(8), ScreenId(2));

        assert!(
            telas
                .encerrar_de(PersonId(10), Some(SessionId(7)))
                .is_empty(),
            "a conexão velha encerrou a transmissão que a nova abriu"
        );
        assert_eq!(telas.em(VoiceRoomId(1)), vec![(PersonId(10), ScreenId(2))]);

        // E a sessão vigente continua podendo encerrar a dela.
        assert_eq!(
            telas.encerrar_de(PersonId(10), Some(SessionId(8))),
            vec![(VoiceRoomId(1), ScreenId(2))]
        );
    }

    #[test]
    fn uma_sala_destruida_leva_todas_as_transmissoes_dela() {
        // **Todas**, e o plural é o ponto: a sessão anuncia o fim de cada uma, e
        // avisar só a primeira deixaria as outras desenhadas para sempre na tela
        // de quem assiste. Hoje a sala guarda uma; a forma devolve lista para
        // que o dia em que guardar duas não passe por aqui sem ninguém notar.
        let mut telas = Telas::default();
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(10), ScreenId(1));

        assert_eq!(telas.encerrar_voice_room(VoiceRoomId(1)), vec![ScreenId(1)]);
        assert!(telas.encerrar_voice_room(VoiceRoomId(1)).is_empty());
        assert!(telas.todas().is_empty());
    }

    #[test]
    fn o_encaminhamento_acha_a_tela_de_cada_pessoa() {
        // `de` é a pergunta que a tarefa dos fluxos faz, e ela vive fora do laço
        // da sessão: é a única fonte que sabe sala e tela ao mesmo tempo. Com
        // duas transmissões na sala, achar «a da sala» deixou de bastar.
        let mut telas = Telas::default();
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(10), ScreenId(1));
        let _ = telas.comecar(VoiceRoomId(2), PersonId(20), SessionId(20), ScreenId(2));

        assert_eq!(
            telas.de(PersonId(10), SessionId(10)),
            Some((VoiceRoomId(1), ScreenId(1)))
        );
        assert_eq!(
            telas.de(PersonId(20), SessionId(20)),
            Some((VoiceRoomId(2), ScreenId(2)))
        );
        assert_eq!(telas.de(PersonId(30), SessionId(30)), None);
        assert_eq!(telas.todas().len(), 2);
    }

    #[test]
    fn a_sessao_velha_nao_para_nem_assina_a_tela_que_a_nova_abriu() {
        // As duas perguntas que sobraram chaveadas só por pessoa, e a mesma
        // queda silenciosa das outras: a conexão velha segue viva até o tempo
        // ocioso do transporte estourar, e nesse meio-tempo a nova já registrou
        // a transmissão.
        //
        // `parar` sem sessão: um `StopScreenShare` atrasado da conexão velha
        // derrubava a tela que a nova acabou de abrir — e quem transmite não
        // recebe aviso nenhum, então fica mandando quadros para uma sala que
        // já não os encaminha.
        //
        // `de` sem sessão: um fluxo aberto pela conexão velha era aceito como se
        // fosse o da nova, e a sala passava a receber duas fontes escrevendo o
        // mesmo `ScreenId`.
        let mut telas = Telas::default();
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(7), ScreenId(1));
        // A reconexão: mesma pessoa, outra conexão, tela nova. `comecar` troca a
        // tela da pessoa em vez de ocupar outra vaga.
        let _ = telas.comecar(VoiceRoomId(1), PersonId(10), SessionId(8), ScreenId(2));

        assert_eq!(
            telas.parar(VoiceRoomId(1), PersonId(10), SessionId(7)),
            None,
            "a conexão velha parou a transmissão da nova"
        );
        assert_eq!(
            telas.em(VoiceRoomId(1)),
            vec![(PersonId(10), ScreenId(2))],
            "a transmissão vigente saiu do registro"
        );

        assert_eq!(
            telas.de(PersonId(10), SessionId(7)),
            None,
            "um fluxo da conexão velha seria aceito como o da nova"
        );
        assert_eq!(
            telas.de(PersonId(10), SessionId(8)),
            Some((VoiceRoomId(1), ScreenId(2))),
            "a conexão vigente deixou de ser encontrada"
        );

        // E a conexão vigente continua podendo parar a própria tela.
        assert_eq!(
            telas.parar(VoiceRoomId(1), PersonId(10), SessionId(8)),
            Some(ScreenId(2))
        );
        assert!(telas.em(VoiceRoomId(1)).is_empty());
    }
}
