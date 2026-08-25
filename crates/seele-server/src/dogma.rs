//! The Dogma's shared state: storage, the write batch, and the event bus.
//!
//! `specs/04-servidor-seele.md` puts Cage state in a task per Cage with no global
//! lock. Text is different: it is one durable log per Line, and the thing worth
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
use seele_proto::control::{CageInfo, LineInfo, PersonProfile, PersonState};
use seele_proto::ids::{CageId, LineId, MessageId, PersonId, ScreenId, Ssrc};
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::persistence::messages::{Messages, PendingMessage, StoredMessage};
use crate::persistence::Persistence;

/// How long the writer waits before committing what it has.
///
/// `specs/04-servidor-seele.md`: "flush por tempo (~200 ms)". Long enough that a
/// busy Line commits once instead of fifty times; short enough that a message
/// still feels sent immediately.
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(200);

/// Events every connection may care about.
///
/// Broadcast to all, filtered per connection. `specs/04-servidor-seele.md` sizes
/// a Dogma at ~50 people, so filtering at the edge costs nothing and keeps the
/// bus from needing to know who is subscribed to what.
#[derive(Debug, Clone)]
pub enum Event {
    /// A message was committed and is now durable.
    MessagePosted(StoredMessage),
    /// A message was edited.
    MessageEdited {
        /// Which Line.
        line: LineId,
        /// Which message.
        id: MessageId,
        /// New body.
        body: String,
    },
    /// A message was removed.
    MessageRemoved {
        /// Which Line.
        line: LineId,
        /// Which message.
        id: MessageId,
    },
    /// A person entered a Cage.
    PersonJoined {
        /// Which Cage.
        cage: CageId,
        /// Who.
        profile: PersonProfile,
        /// Their media source.
        ssrc: Ssrc,
    },
    /// A person left a Cage.
    PersonLeft {
        /// Which Cage.
        cage: CageId,
        /// Who.
        person: PersonId,
    },
    /// A person connected to the Dogma, whatever room they are in.
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
    /// A Cage was created.
    ///
    /// Announced to **everybody**, the person who asked included, and this is the
    /// difference between a feature and a demonstration: a room that only shows
    /// up on the next handshake is a room whose maker has to tell their friends
    /// to reconnect before they can use it.
    CageCreated {
        /// The Cage, as it now exists.
        cage: CageInfo,
    },
    /// A Line was created.
    LineCreated {
        /// The Line, as it now exists.
        line: LineInfo,
    },
    /// A Cage was renamed.
    CageRenamed {
        /// Which Cage.
        cage: CageId,
        /// Its new name.
        name: String,
    },
    /// A Line was renamed.
    LineRenamed {
        /// Which Line.
        line: LineId,
        /// Its new name.
        name: String,
    },

    // ---- what the Dogma calls itself ----
    //
    // The same shape as `CageRenamed`, one level up: committed first, announced
    // after, forwarded to every connection including the one that asked.
    //
    // Announced to **everybody** for the reason `CageCreated` gives, and here it
    // is sharper than for a room: a name is drawn in the header of every open
    // window, so a rename that only reached the next handshake would put the new
    // name on the screen of whoever typed it and the old one on everybody
    // else's — which is the exact failure ADR 0032 says not to ship.
    /// The Dogma was renamed.
    DogmaRenamed {
        /// What it is called now.
        name: String,
    },
    /// The Dogma's icon changed, or was taken down.
    ///
    /// The bytes travel on the bus rather than a "go and read it again": the bus
    /// is what every connection is already draining, and telling fifty sessions
    /// to each take the PERSISTENCE lock and read the same 8 KiB row would be fifty
    /// reads of a value one reader already has in its hand.
    DogmaIconChanged {
        /// The picture, or `None` when it was taken down.
        icon: Option<Vec<u8>>,
    },

    // ---- moderation ----
    //
    // These two are the odd ones on this bus: every other event is something a
    // connection **forwards** to its client, and these are something a
    // connection **does to itself**. They are here anyway because a Dogma has no
    // other way for one session to reach another — there is no map of live
    // sessions, deliberately, since `specs/04-servidor-seele.md` puts Cage state
    // in a task per Cage with no global lock. The bus already reaches every
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
    /// An operator moved a person into a Cage.
    PersonMoved {
        /// Who.
        person: PersonId,
        /// Where to.
        cage: CageId,
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
    /// A Cage was destroyed.
    CageDeleted {
        /// Which Cage.
        cage: CageId,
    },
    /// A Line was destroyed, and everything written in it with it.
    LineDeleted {
        /// Which Line.
        line: LineId,
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
        /// Em qual Cage.
        cage: CageId,
        /// Quem.
        person: PersonId,
        /// Como a transmissão se chama daqui em diante.
        screen: ScreenId,
    },
    /// Uma transmissão acabou.
    ScreenShareStopped {
        /// Em qual Cage.
        cage: CageId,
        /// Qual transmissão.
        screen: ScreenId,
    },
    /// Quantas pessoas estão recebendo uma transmissão, agora.
    ///
    /// **N**, e ele é um termo do teto do §5.1 —
    /// `caminho de quem hospeda × 60% ÷ N` — que até aqui só existia dentro do
    /// Dogma. Sem ele no fio, quem compartilha aplica um `min` com uma perna
    /// que inventa; com ele, a mesma conta é feita nas duas pontas a partir do
    /// mesmo número.
    ///
    /// Mandado pelo [`crate::cage::Cage`], que é o único lugar deste Dogma que
    /// sabe quem está na sala sem perguntar a ninguém, e no mesmo instante em
    /// que ele refaz o teto. Dois donos de N seriam duas contas discordando no
    /// primeiro dia ruim.
    ScreenViewers {
        /// Em qual Cage.
        cage: CageId,
        /// Qual transmissão.
        screen: ScreenId,
        /// Quantos assistem. Não conta quem compartilha.
        quantos: u32,
    },
    /// Alguém que assiste não tem de que predizer e pediu um quadro-chave.
    ///
    /// Endereçado a um pessoa e entregue a todos, como o [`Self::SessionEnded`]
    /// — um Dogma não tem outra maneira de uma sessão alcançar outra, e o
    /// barramento já é o que toda conexão drena.
    KeyFrameRequested {
        /// Qual transmissão.
        screen: ScreenId,
        /// Quem pediu.
        person: PersonId,
        /// A quem entregar: quem está compartilhando.
        sharer: PersonId,
    },
}

/// Quem está compartilhando tela em cada Cage.
///
/// **Uma transmissão por Cage**, que é o §6 item 3 da spec de compartilhamento
/// de tela: *«uma transmissão por sala de voz na v1. Duas dobram a subida de
/// quem recebe e triplicam a interface»*. Quem chega depois perde a corrida e
/// **é avisado com nome** — `AlertReason::ScreenShareTaken` —, porque
/// `PermissionDenied` diria «você não pode» a quem pode.
///
/// Ao lado da [`Occupancy`] e não dentro dela: quem está sentado e quem está
/// transmitindo mudam por motivos diferentes e em momentos diferentes, e a
/// única coisa que os liga é a limpeza — sair do Cage encerra a transmissão,
/// que é o que [`Telas::encerrar_de`] existe para fazer numa chamada só.
#[derive(Debug, Default)]
pub struct Telas {
    por_cage: HashMap<CageId, (PersonId, ScreenId)>,
}

impl Telas {
    /// Registra uma transmissão, ou diz quem já está com a vaga.
    ///
    /// `Err(person)` é quem chegou primeiro. Devolver o nome e não um `bool`
    /// porque a frase que a interface tem de escrever é «fulano já está
    /// compartilhando», e um booleano obrigaria quem chama a procurar a resposta
    /// de novo em outro lugar.
    pub fn comecar(
        &mut self,
        cage: CageId,
        person: PersonId,
        screen: ScreenId,
    ) -> Result<(), PersonId> {
        match self.por_cage.get(&cage) {
            // Quem já está transmitindo pedindo de novo não é uma corrida
            // perdida: é um cliente que reabriu o botão, e devolver
            // `ScreenShareTaken` para a própria pessoa seria dizer que ela
            // perdeu para si mesma.
            Some((dono, _)) if *dono != person => Err(*dono),
            _ => {
                self.por_cage.insert(cage, (person, screen));
                Ok(())
            }
        }
    }

    /// Encerra a transmissão de um Cage, se for deste pessoa.
    ///
    /// Conferido, e não apagado às cegas: um `StopScreenShare` de quem não está
    /// transmitindo derrubaria a tela de quem está.
    pub fn parar(&mut self, cage: CageId, person: PersonId) -> Option<ScreenId> {
        let (dono, screen) = *self.por_cage.get(&cage)?;
        if dono != person {
            return None;
        }
        self.por_cage.remove(&cage);
        Some(screen)
    }

    /// Encerra o que este pessoa estivesse transmitindo, onde quer que fosse.
    ///
    /// Devolve os Cages e as transmissões, porque alguém tem de anunciar o fim
    /// e quem chama nem sempre sabe a sala — uma sessão acaba em qualquer `?` do
    /// meio do laço dela. É o mesmo raciocínio de
    /// [`Occupancy::vacate_everywhere`].
    pub fn encerrar_de(&mut self, person: PersonId) -> Vec<(CageId, ScreenId)> {
        let encerradas: Vec<_> = self
            .por_cage
            .iter()
            .filter(|(_, (dono, _))| *dono == person)
            .map(|(cage, (_, screen))| (*cage, *screen))
            .collect();
        for (cage, _) in &encerradas {
            self.por_cage.remove(cage);
        }
        encerradas
    }

    /// Encerra tudo o que estivesse acontecendo num Cage que deixou de existir.
    pub fn encerrar_cage(&mut self, cage: CageId) -> Option<ScreenId> {
        self.por_cage.remove(&cage).map(|(_, screen)| screen)
    }

    /// Quem está transmitindo neste Cage, se alguém está.
    #[must_use]
    pub fn em(&self, cage: CageId) -> Option<(PersonId, ScreenId)> {
        self.por_cage.get(&cage).copied()
    }

    /// Onde este pessoa está transmitindo, se está.
    ///
    /// A pergunta que o **encaminhamento** faz, e ela vem ao contrário de
    /// [`Self::em`] por um motivo concreto: a tarefa que aceita os fluxos
    /// unidirecionais de uma conexão vive fora do laço da sessão — é o que a
    /// impede de bloquear o controle — e por isso não enxerga em que Cage o
    /// plug está. Este registro é a única fonte que sabe as duas coisas ao
    /// mesmo tempo, e é a mesma que decidiu a corrida do §6 item 3. Perguntar a
    /// ela é o que garante que um fluxo de tela só é aceito de quem o controle
    /// já autorizou.
    #[must_use]
    pub fn de(&self, person: PersonId) -> Option<(CageId, ScreenId)> {
        self.por_cage
            .iter()
            .find(|(_, (dono, _))| *dono == person)
            .map(|(cage, (_, screen))| (*cage, *screen))
    }

    /// Tudo o que está acontecendo agora, achatado.
    ///
    /// Achatado e não devolvido como mapa porque o único chamador percorre a
    /// lista uma vez para escrever um quadro por transmissão — a mesma razão
    /// que [`Occupancy::everywhere`] dá.
    #[must_use]
    pub fn todas(&self) -> Vec<(CageId, PersonId, ScreenId)> {
        self.por_cage
            .iter()
            .map(|(cage, (person, screen))| (*cage, *person, *screen))
            .collect()
    }
}

/// A Cage seat held open for a person who dropped.
///
/// `specs/02-protocolo.md`: "O servidor guarda o slot pelo mesmo período" — the
/// five minutes of the internal battery. Without this a person whose train enters
/// a tunnel comes back to find their Cage full.
#[derive(Debug, Clone, Copy)]
struct ReservedSlot {
    cage: CageId,
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
    pub fn reserve(&mut self, person: PersonId, cage: CageId, ssrc: Ssrc, now: Instant) {
        self.reserved.insert(
            person,
            ReservedSlot {
                cage,
                ssrc,
                expires_at: now + seele_proto::transport::SESSION_GRACE,
            },
        );
    }

    /// Reclaims a seat, if one is still being held.
    ///
    /// Returns the Cage and the `ssrc` the person had, so a reconnection lands
    /// where it left off rather than looking like somebody new.
    pub fn reclaim(&mut self, person: PersonId, now: Instant) -> Option<(CageId, Ssrc)> {
        let slot = self.reserved.get(&person).copied()?;
        if slot.expires_at <= now {
            self.reserved.remove(&person);
            return None;
        }
        self.reserved.remove(&person);
        Some((slot.cage, slot.ssrc))
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

/// Somebody sitting in a Cage right now.
#[derive(Debug, Clone)]
pub struct Occupant {
    /// Who.
    pub person: PersonId,
    /// What they are called.
    pub nickname: String,
    /// Their media source.
    pub ssrc: Ssrc,
}

/// Quem está conectado neste Dogma agora, sentado numa sala ou não.
///
/// # Por que não bastava a ocupação
///
/// [`Occupancy`] responde «quem está em qual sala», e por muito tempo era a
/// única tabela de presença que existia — então quem entrava no Dogma e ficava
/// fora das salas não existia para mais ninguém. `PersonJoined` carrega um Cage
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
    pub fn chegou(&mut self, quem: Occupant) -> bool {
        self.por_person.insert(quem.person, quem).is_none()
    }

    /// Tira alguém, e diz se havia o que tirar.
    pub fn saiu(&mut self, person: PersonId) -> bool {
        self.por_person.remove(&person).is_some()
    }

    /// Todo mundo que está aqui agora.
    pub fn todos(&self) -> Vec<Occupant> {
        self.por_person.values().cloned().collect()
    }
}

/// Who is in which Cage at this moment.
///
/// Separate from [`Slots`], which holds seats for people who are *away*. This
/// is who is actually there, and it exists to answer one question the protocol
/// could not: **who was already here before I was watching.**
///
/// `specs/02-protocolo.md` announces arrivals going forward and nothing else,
/// so a person entering an occupied Cage saw an empty room until somebody moved.
/// Gap G15, found by running two clients where the second started after the
/// first had already sat down.
///
/// # Why the whole map, and not one Cage
///
/// G15 was closed for the Cage the person walked into, and only that one. The
/// screen `design/Entry Plug v3.dc.html` draws occupants under **every** Cage,
/// and for the other four that data had never existed on the client at all:
/// they were drawn empty, always, however many people were in them. Reported
/// from a real session as "o sistema de cages não está bem implementado,
/// mostra que as cages estão vazias quando não deveriam estar".
///
/// So [`Occupancy::everywhere`] hands back the entire picture, and a connection
/// is given it once, at the start of its session. Everything after that is the
/// unfiltered `PersonJoined` / `PersonLeft` broadcast.
#[derive(Debug, Default)]
pub struct Occupancy {
    by_cage: HashMap<CageId, Vec<Occupant>>,
}

impl Occupancy {
    /// Seats a person, replacing any earlier seat they held.
    ///
    /// Replacing rather than appending: a reconnection inside the grace period
    /// re-enters the same Cage, and a roster with the same person twice is a
    /// roster nobody trusts.
    pub fn seat(&mut self, cage: CageId, occupant: Occupant) {
        let _ = self.vacate_everywhere(occupant.person);
        self.by_cage.entry(cage).or_default().push(occupant);
    }

    /// Removes a person from one Cage.
    pub fn vacate(&mut self, cage: CageId, person: PersonId) {
        if let Some(seated) = self.by_cage.get_mut(&cage) {
            seated.retain(|occupant| occupant.person != person);
        }
    }

    /// Removes a person from wherever they were, and says where that was.
    ///
    /// The Cages come back because somebody has to announce the departure and
    /// the caller does not always know the room: a session can end at any `?`
    /// in the middle of its loop, and that path has no idea where the person was
    /// sitting. Returning the answer here is what lets one call at the end of a
    /// connection both clear the seat and tell everybody about it — the same
    /// reasoning `crate::cage::Cages::leave_everywhere` gives for being
    /// broadcast rather than aimed.
    pub fn vacate_everywhere(&mut self, person: PersonId) -> Vec<CageId> {
        let mut vacated = Vec::new();
        for (cage, seated) in &mut self.by_cage {
            let before = seated.len();
            seated.retain(|occupant| occupant.person != person);
            if seated.len() != before {
                vacated.push(*cage);
            }
        }
        vacated
    }

    /// Who is in a Cage, in the order they arrived.
    #[must_use]
    pub fn in_cage(&self, cage: CageId) -> Vec<Occupant> {
        self.by_cage.get(&cage).cloned().unwrap_or_default()
    }

    /// Everybody seated anywhere, with the Cage they are seated in.
    ///
    /// Flattened rather than handed back as a map, because the only caller
    /// walks it once to write a frame per occupant, and a map would make that
    /// caller nest two loops to say one thing.
    #[must_use]
    pub fn everywhere(&self) -> Vec<(CageId, Occupant)> {
        self.by_cage
            .iter()
            .flat_map(|(cage, seated)| seated.iter().map(|occupant| (*cage, occupant.clone())))
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
/// wants to know whether this Dogma has ever done it at all, and a counter that
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

/// One message on its way to the batch, with somewhere to report the outcome.
pub struct WriteRequest {
    /// What to write.
    pub message: PendingMessage,
}

/// Everything a connection needs from the Dogma.
pub struct Dogma {
    /// Persistent state. One connection, one mutex — SQLite has one writer.
    pub persistence: Arc<Mutex<Persistence>>,
    /// The event bus.
    pub events: broadcast::Sender<Event>,
    /// Where messages go to be batched.
    pub writes: mpsc::Sender<WriteRequest>,
    /// Seats held for people who are expected back.
    pub slots: Arc<Mutex<Slots>>,
    /// Who is sitting in which Cage right now — gap G15.
    pub occupancy: Arc<Mutex<Occupancy>>,
    /// Quem está conectado, sentado ou não. Ver [`Presentes`].
    pub presentes: Arc<Mutex<Presentes>>,
    /// Quantos apertos de mão cada endereço ainda pode gastar.
    ///
    /// Antes de autenticar, portanto sem identidade nenhuma para contar: a
    /// chave é o endereço de origem. Ver [`crate::taxa`].
    pub portaria: Arc<Mutex<crate::taxa::Portaria>>,
    /// How often the bus outran a session. See [`Atrasos`].
    pub atrasos: Arc<Atrasos>,
    /// Quem está compartilhando tela em cada Cage. Ver [`Telas`].
    pub telas: Arc<Mutex<Telas>>,
    /// The attachment store, its ceiling, and the byte budget. ADR 0027.
    ///
    /// `None` when this Dogma has nowhere to keep blobs, which is the in-memory
    /// case. A transfer then meets `AttachmentRefusal::Unavailable` — a
    /// sentence — rather than a directory appearing wherever the process
    /// started.
    pub anexos: Option<Arc<crate::transfer::Vault>>,
    /// Quanto a subida desta máquina carrega, em bits por segundo, ou `None`.
    ///
    /// A cópia de `DogmaConfig::caminho_bps` que o resto do daemon alcança, e
    /// ela mora aqui pela mesma razão que [`Self::telas`]: é um fato sobre
    /// **este Dogma** que duas partes distantes precisam, e a alternativa era
    /// passar a configuração inteira por assinaturas que já estão cheias.
    ///
    /// Duas leituras saem daqui e elas discordam de propósito — a admissão de
    /// [`crate::cage::Cage`] cai numa hipótese quando isto é `None`, e o
    /// `HostUplink` que a sessão escreve manda **zero**, que pelo protocolo é
    /// «não medi». Ver `crate::tela::caminho_no_fio`.
    pub caminho_bps: Option<u32>,
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
                        // The Dogma is shutting down. Flush what is left rather
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

impl Dogma {
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
        slots.reserve(PersonId(1), CageId(1), Ssrc(7), now);

        let reclaimed = slots.reclaim(PersonId(1), now + Duration::from_secs(60));
        assert_eq!(reclaimed, Some((CageId(1), Ssrc(7))));
    }

    #[test]
    fn a_reconnecting_person_gets_their_own_ssrc_back() {
        // Otherwise a sixty-second outage looks to everybody else like the person
        // left and a stranger arrived, and every listener's jitter buffer starts
        // from scratch.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), CageId(2), Ssrc(42), now);

        let (cage, ssrc) = slots.reclaim(PersonId(1), now).expect("seat held");
        assert_eq!(cage, CageId(2));
        assert_eq!(ssrc, Ssrc(42));
    }

    #[test]
    fn an_expired_seat_is_not_reclaimable() {
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), CageId(1), Ssrc(7), now);

        let after = now + seele_proto::transport::SESSION_GRACE + Duration::from_secs(1);
        assert_eq!(slots.reclaim(PersonId(1), after), None);
    }

    #[test]
    fn reclaiming_twice_only_works_once() {
        // The seat is taken by the reconnection. A second claim would let one
        // person occupy two.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), CageId(1), Ssrc(7), now);

        assert!(slots.reclaim(PersonId(1), now).is_some());
        assert!(slots.reclaim(PersonId(1), now).is_none());
    }

    #[test]
    fn the_sweeper_frees_expired_seats() {
        // Without this a Dogma slowly fills with seats held for people who are
        // never coming back, and specs/04 caps a Cage at a member limit.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), CageId(1), Ssrc(1), now);
        slots.reserve(PersonId(2), CageId(1), Ssrc(2), now);
        assert_eq!(slots.held(), 2);

        let after = now + seele_proto::transport::SESSION_GRACE + Duration::from_secs(1);
        assert_eq!(slots.sweep(after), 2);
        assert_eq!(slots.held(), 0);
    }

    #[test]
    fn the_sweeper_leaves_live_seats_alone() {
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PersonId(1), CageId(1), Ssrc(1), now);
        assert_eq!(slots.sweep(now + Duration::from_secs(30)), 0);
        assert_eq!(slots.held(), 1);
    }

    // ---- who is in which Cage ----

    fn occupant(person: u64, nickname: &str) -> Occupant {
        Occupant {
            person: PersonId(person),
            nickname: nickname.to_owned(),
            ssrc: Ssrc(u32::try_from(person * 10).expect("ssrc")),
        }
    }

    #[test]
    fn the_whole_dogma_is_readable_at_once_and_not_one_room_at_a_time() {
        // The half of gap G15 that was missing. `in_cage` answered "who is in
        // the room I am walking into"; nothing answered "who is in the other
        // four", and the v3 layout draws those four with their occupants under
        // them. They were drawn empty however many people were in them.
        let mut occupancy = Occupancy::default();
        occupancy.seat(CageId(1), occupant(1, "ayanami"));
        occupancy.seat(CageId(1), occupant(2, "shinji"));
        occupancy.seat(CageId(2), occupant(3, "asuka"));

        let mut everywhere: Vec<(u32, u64)> = occupancy
            .everywhere()
            .into_iter()
            .map(|(cage, seated)| (cage.0, seated.person.0))
            .collect();
        everywhere.sort_unstable();

        assert_eq!(everywhere, [(1, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn leaving_says_which_rooms_were_left() {
        // The caller that needs this is the end of a connection, which does not
        // know where the person was sitting: a session can end at any `?`. If
        // this said nothing, the departure could not be announced, and the
        // person would stay on everybody's screen until they came back.
        let mut occupancy = Occupancy::default();
        occupancy.seat(CageId(7), occupant(1, "ayanami"));

        assert_eq!(occupancy.vacate_everywhere(PersonId(1)), vec![CageId(7)]);
        assert!(occupancy.everywhere().is_empty());
    }

    #[test]
    fn leaving_a_room_nobody_was_in_announces_nothing() {
        // The other half, and the one that keeps a departure from being sent
        // twice: `serve` calls this after every session, including the ones that
        // already left through `EjectPlug` and said so.
        let mut occupancy = Occupancy::default();
        occupancy.seat(CageId(7), occupant(1, "ayanami"));
        occupancy.vacate(CageId(7), PersonId(1));

        assert!(
            occupancy.vacate_everywhere(PersonId(1)).is_empty(),
            "a person who had already left was announced as leaving again"
        );
    }

    #[test]
    fn walking_between_rooms_reports_the_room_that_was_left() {
        // What `InsertPlug` needs in order to tell the old room. Seating alone
        // clears the previous seat silently, and a silent clear is a person who
        // stays in the first Cage on every other client for ever.
        let mut occupancy = Occupancy::default();
        occupancy.seat(CageId(1), occupant(1, "ayanami"));

        assert_eq!(occupancy.vacate_everywhere(PersonId(1)), vec![CageId(1)]);
        occupancy.seat(CageId(2), occupant(1, "ayanami"));

        assert_eq!(occupancy.in_cage(CageId(1)).len(), 0);
        assert_eq!(occupancy.in_cage(CageId(2)).len(), 1);
    }
    // ---- compartilhamento de tela ----

    #[test]
    fn uma_transmissao_por_cage_e_quem_perde_a_corrida_tem_nome() {
        // §6 item 3 da spec de compartilhamento de tela: uma transmissão por
        // sala de voz na v1, porque «duas dobram a subida de quem recebe e
        // triplicam a interface».
        //
        // O que este teste prende junto com a regra é **quem** o `Err` carrega:
        // é com esse nome que a sessão escreve `AlertReason::ScreenShareTaken`,
        // e trocá-lo por um `bool` obrigaria quem chama a procurar a resposta
        // de novo em outro lugar — onde ela já pode ter mudado.
        let mut telas = Telas::default();
        assert_eq!(telas.comecar(CageId(1), PersonId(10), ScreenId(1)), Ok(()));
        assert_eq!(
            telas.comecar(CageId(1), PersonId(20), ScreenId(2)),
            Err(PersonId(10)),
            "duas telas na mesma sala"
        );
        // E a corrida perdida não derruba quem ganhou.
        assert_eq!(telas.em(CageId(1)), Some((PersonId(10), ScreenId(1))));

        // Outra sala é outra corrida.
        assert_eq!(telas.comecar(CageId(2), PersonId(20), ScreenId(3)), Ok(()));
    }

    #[test]
    fn quem_ja_transmite_pedindo_de_novo_nao_perde_para_si_mesmo() {
        // Um cliente que reabriu o botão, ou um `StartScreenShare` repetido
        // depois de uma reconexão. Devolver `ScreenShareTaken` para a própria
        // pessoa seria dizer que ela perdeu uma corrida contra ela mesma, e a
        // interface escreveria uma frase que não faz sentido nenhum na tela de
        // quem está compartilhando naquele instante.
        let mut telas = Telas::default();
        telas
            .comecar(CageId(1), PersonId(10), ScreenId(1))
            .expect("primeira");
        assert_eq!(telas.comecar(CageId(1), PersonId(10), ScreenId(2)), Ok(()));
        assert_eq!(telas.em(CageId(1)), Some((PersonId(10), ScreenId(2))));
    }

    #[test]
    fn parar_a_tela_de_outra_pessoa_nao_para_nada() {
        // Sem esta conferência, um `StopScreenShare` de qualquer pessoa da sala
        // derruba a tela de quem está compartilhando — e o verbo não carrega
        // Cage nem `ScreenId` de propósito, então não há nada além do registro
        // para separar as duas.
        let mut telas = Telas::default();
        telas
            .comecar(CageId(1), PersonId(10), ScreenId(1))
            .expect("começa");

        assert_eq!(telas.parar(CageId(1), PersonId(20)), None);
        assert_eq!(telas.em(CageId(1)), Some((PersonId(10), ScreenId(1))));

        assert_eq!(telas.parar(CageId(1), PersonId(10)), Some(ScreenId(1)));
        assert_eq!(telas.em(CageId(1)), None);
    }

    #[test]
    fn a_tela_de_quem_sai_para_junto_com_ele() {
        // O mesmo defeito que `Occupancy::vacate_everywhere` conserta para o
        // pessoa fantasma, com a diferença de que aqui a sala fica prometendo
        // imagem em movimento que não tem mais de onde vir: o fluxo morreu com
        // a conexão.
        let mut telas = Telas::default();
        telas
            .comecar(CageId(1), PersonId(10), ScreenId(1))
            .expect("começa");
        telas
            .comecar(CageId(2), PersonId(20), ScreenId(2))
            .expect("começa");

        assert_eq!(
            telas.encerrar_de(PersonId(10)),
            vec![(CageId(1), ScreenId(1))]
        );
        assert_eq!(telas.em(CageId(1)), None);
        // E não encosta na de mais ninguém.
        assert_eq!(telas.em(CageId(2)), Some((PersonId(20), ScreenId(2))));
        assert!(telas.encerrar_de(PersonId(10)).is_empty());
    }

    #[test]
    fn uma_sala_destruida_leva_a_transmissao_dela() {
        let mut telas = Telas::default();
        telas
            .comecar(CageId(1), PersonId(10), ScreenId(1))
            .expect("começa");
        assert_eq!(telas.encerrar_cage(CageId(1)), Some(ScreenId(1)));
        assert_eq!(telas.encerrar_cage(CageId(1)), None);
        assert!(telas.todas().is_empty());
    }
}
