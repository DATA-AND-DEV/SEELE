//! **Entry Plug**, as one object a graphical shell can hold.
//!
//! `specs/06-clientes-gui.md` specifies this surface and states the rule it
//! exists to enforce: "Nenhuma funcionalidade nasce aqui: se algo é útil, é
//! implementado em `seele-core` e aparece nas duas interfaces." Nothing in this
//! crate decides anything. It owns a thread, a connection and a
//! [`seele_core::Room`], and it turns method calls into commands and server
//! events into notifications.
//!
//! # The shape, and why it is this shape
//!
//! ADR 0018: this is what `uniffi` will annotate in M6 without a rewrite.
//! One opaque object behind an `Arc`, no generics, no borrowed references
//! crossing over, errors as a closed enum, events through a `Send + Sync`
//! trait, and value types built from `u64`/`String`/`bool`/`Vec` and enums —
//! never a `seele-proto` newtype.
//!
//! # Threading
//!
//! [`Plug::connect`] **blocks**. It opens a QUIC connection and completes a
//! handshake, and a shell must call it off whatever thread draws. Everything
//! afterwards returns immediately: commands are queued to the driver thread,
//! and [`Plug::snapshot`] reads a copy.
//!
//! Events arrive on the driver thread, not the shell's. A listener that touches
//! a UI must marshal — `specs/06-clientes-gui.md`: "a casca marshala para sua
//! thread de UI."

// A test that has to handle the impossible case stops reading as a statement
// about the code. The same allowance the other crates take.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

pub mod types;

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seele_core::enlace::Enlace;
use seele_core::{
    identity, CageId, ClientMessageId, FilePinStore, LineId, MediaChannel, MessageId, PilotId,
    Room, Ssrc, SyncBand, SyncInputs, SyncRatio, Voice,
};

pub use types::{
    Attachment, AttachmentRefusal, Cage, CageSync, CaptureDevice, EndReason, Event, Line,
    LineWeight, LinkState, Message, Notice, NoticeReason, Pattern, Pilot, PlaybackDevice,
    PlugError, Preview, PreviewRefusal, PreviewRules, Severity, Snapshot, SyncBand as Band,
    Telemetry, Transfer, Trust, VoiceMode,
};

/// O que a casca gráfica precisa do core além de um [`Plug`] vivo.
///
/// ADR 0002 deixa `seele-app` ver `seele-ffi` e mais nada, e as telas do app
/// precisam dos mesmos módulos que o `plug` usa direto: a lista de Dogmas
/// visitados, a busca no histórico, a leitura de um `seele://` e os ajustes
/// que ficam nesta máquina. Nenhum deles é lógica de casca — se fossem escritos
/// aqui, seriam escritos de novo no terminal e mais uma vez no cliente móvel.
/// `preferences` é o caso mais claro: um microfone escolhido no app e ignorado
/// pelo terminal é um microfone que parece ter sido esquecido.
///
/// Re-exportados um a um, e não com um `pub use seele_core::*`, pela mesma
/// doutrina que `seele-core` já aplica sobre `seele-proto`: acrescentar a esta
/// lista é a hora de perguntar se a casca precisa do valor ou da decisão por
/// trás dele.
pub use seele_core::{conhecidos, preferences, search, uri};

/// Every microphone this machine is offering, right now.
///
/// A free function and not a method on [`Plug`]: picking a microphone is a thing
/// a person does *before* connecting at least as often as during, and hanging
/// the list off a live session would put the control behind the door it exists
/// to open. It is also what makes the list answerable from the entry screen.
///
/// An empty list means the machine would not enumerate — **not** that there is
/// no microphone. The default device still opens when enumeration fails, which
/// is why [`ConnectConfig::capture_device`] of `None` never consults this. A
/// shell that draws "no audio" off an empty list is drawing the wrong sentence;
/// [`Snapshot::audio_available`] is the one that means it.
#[must_use]
pub fn capture_devices() -> Vec<CaptureDevice> {
    seele_core::capture_devices()
        .into_iter()
        .map(|found| CaptureDevice {
            id: found.id,
            name: found.name,
            default: found.default,
        })
        .collect()
}

/// Every place this machine will play sound, right now.
///
/// The twin of [`capture_devices`], a free function for the same reason, and
/// with the same warning about an empty list: the machine would not enumerate,
/// **not** that there is nowhere to play. [`Snapshot::audio_available`] is still
/// the one that means "no audio".
#[must_use]
pub fn playback_devices() -> Vec<PlaybackDevice> {
    seele_core::playback_devices()
        .into_iter()
        .map(|found| PlaybackDevice {
            id: found.id,
            name: found.name,
            default: found.default,
        })
        .collect()
}

/// A impressão digital da identidade desta máquina.
///
/// A mesma chave com que este computador entra em qualquer Dogma, e a mesma
/// impressão que o outro lado vê — `identity.key` sob o `home`, criada na
/// primeira vez se ainda não existir.
///
/// Existe porque quem hospeda precisa se reconhecer no próprio Dogma **antes**
/// de bater na própria porta, e o app não pode calcular isso sozinho: a regra de
/// dependência do ADR 0002 o deixa ver `seele-ffi` e `seele-server`, e nunca
/// `seele-core` nem `seele-proto`. A ponte é aqui.
///
/// # Errors
///
/// Falha se a identidade não puder ser lida nem criada.
pub fn impressao_desta_maquina(home: &str) -> Result<String, PlugError> {
    let chave =
        identity::load_or_create(&PathBuf::from(home).join("identity.key")).map_err(|error| {
            tracing::warn!(%error, "identity unavailable");
            PlugError::IdentityUnavailable
        })?;
    Ok(seele_core::key_fingerprint(
        chave.verifying_key().as_bytes(),
    ))
}

/// How often measurements are refreshed.
const TICK: Duration = Duration::from_millis(250);

/// How many messages of history to ask for when a Line opens.
const HISTORY_PAGE: u16 = 50;

/// What a shell needs to connect.
#[derive(Debug, Clone)]
pub struct ConnectConfig {
    /// `host` or `host:port`.
    pub server: String,
    /// The invite's other addresses for the same Dogma, in try order.
    ///
    /// Empty for an address typed by hand, and for every invite written before
    /// ADR 0006 grew the field. One address per entry, same syntax as
    /// [`ConnectConfig::server`]; one that does not resolve is dropped rather
    /// than refused, because losing a path is not a reason to lose the link.
    pub alternate_servers: Vec<String>,
    /// How to appear in the roster.
    pub nickname: String,
    /// Where the identity and the pins live. ADR 0017.
    ///
    /// A path rather than a key, because the shell knows where its platform
    /// keeps configuration and the core knows how to persist an identity.
    pub home: String,
    /// Convite de uso único ou senha, quando o Dogma exige um.
    ///
    /// `None` num Dogma aberto, que é o padrão.
    pub join_secret: Option<String>,
    /// A impressão digital que o convite prometeu, quando veio de um link.
    pub expected_fingerprint: Option<String>,
    /// O bilhete de encontro do link, quando ele trouxe um.
    ///
    /// Degrau 4 do ADR 0022: com ele, quem entra bate no ponto de encontro antes
    /// de tentar os endereços, e o anfitrião fura o NAT para cá. `None` é o
    /// caminho de sempre — e é o que um link sem `enc` produz.
    pub bilhete: Option<seele_core::uri::Bilhete>,
    /// Whether to open the microphone and speakers at all.
    ///
    /// False on a machine with no sound card, which is most servers and every
    /// CI box. The text half of the product needs none.
    pub audio: bool,
    /// Which microphone to open, as a [`CaptureDevice::id`].
    ///
    /// `None` is the machine's default, and is what every session took before
    /// there was a screen to choose on. A device that is gone by the time the
    /// session opens does **not** refuse the connection — see `drive`, which
    /// falls back to the default rather than turning a stale preference into a
    /// Dogma nobody can enter.
    pub capture_device: Option<String>,
    /// Where the sound comes out, as a [`PlaybackDevice::id`].
    ///
    /// `None` is the machine's default. Falls back the same way its twin does,
    /// and independently of it: a headset left in another room must not cost
    /// somebody the microphone they chose.
    pub playback_device: Option<String>,
}

/// What a shell implements to be told things changed.
///
/// Called on the driver thread. `specs/06-clientes-gui.md` puts the marshalling
/// on the shell, because only the shell knows what its UI thread is.
pub trait EventListener: Send + Sync {
    /// Something happened.
    fn on_event(&self, event: Event);
}

/// A command on its way to the driver thread.
enum Command {
    InsertPlug(CageId),
    EjectPlug,
    OpenLine(LineId),
    Send {
        line: LineId,
        body: String,
    },
    SetAtField(bool),
    SetTotalIsolation(bool),
    CreateCage {
        name: String,
        limit: u16,
        line: Option<LineId>,
    },
    CreateLine {
        name: String,
    },
    RenameCage {
        cage: CageId,
        name: String,
    },
    RenameLine {
        line: LineId,
        name: String,
    },
    KickPilot {
        pilot: PilotId,
    },
    BanPilot {
        pilot: PilotId,
        reason: Option<String>,
        expires_at: Option<i64>,
    },
    RemoveMessage {
        message: MessageId,
    },
    MovePilot {
        pilot: PilotId,
        cage: CageId,
    },
    DeleteCage {
        cage: CageId,
    },
    DeleteLine {
        line: LineId,
    },
    /// The one command that carries somewhere to answer.
    ///
    /// Everything else on this queue is a thing to do, confirmed — when it is
    /// confirmed at all — by the Dogma announcing it to everybody. This is a
    /// question, and its answer is only useful to the caller who asked, while
    /// the box it fills is still open.
    WeighLine {
        line: LineId,
        answer: tokio::sync::oneshot::Sender<LineWeight>,
    },
    /// A file, on a stream of its own. ADR 0027.
    ///
    /// Boxed because it is much larger than every other variant here, and an
    /// enum is as big as its largest arm — a path, three strings and two ids on
    /// every queued keystroke would be paid by all of them.
    Attach(Box<seele_core::enlace::Anexo>),
    SaveAttachment {
        attachment: seele_core::AttachmentId,
        destination: std::path::PathBuf,
    },
    /// A picture to look at, and where to put the verdict. ADR 0027.
    ///
    /// The second command here that carries somewhere to answer, and the same
    /// argument as [`Command::WeighLine`]: the answer is only useful to the
    /// caller who asked, while the box it fills is still open.
    PreviewAttachment {
        attachment: seele_core::AttachmentId,
        answer: tokio::sync::oneshot::Sender<Preview>,
    },
    Shutdown,
}

/// State the driver thread writes and the shell reads.
struct Shared {
    room: Mutex<Room>,
    listeners: Mutex<Vec<Arc<dyn EventListener>>>,
    /// The voice path, which is also where the chosen devices are remembered.
    ///
    /// Remembered there and not here, on purpose. This crate used to keep its
    /// own copy of the chosen microphone, and a second copy of a fact is a
    /// second copy that can disagree — a switch that took but was not recorded
    /// here would be undone by the next reconnection, silently. `Voice` keeps
    /// what it asked for and hands it back through `reopen`, so there is one
    /// answer and the layer that acts on it is the layer that holds it.
    voice: Mutex<Option<Voice>>,
    /// What it takes to open a voice path on the connection that is up now.
    ///
    /// Here, and not only on the driver thread, so that switching microphone can
    /// answer on the caller's thread with a real error. Sent through the command
    /// queue instead, a device that is gone would fail somewhere nobody is
    /// listening, and the screen would show a pick that never took.
    ///
    /// Rewritten on every reconnection: the ssrc and the channel are both new.
    media: Mutex<Option<(MediaChannel, Ssrc)>>,
    nickname: Mutex<String>,
    pattern: AtomicU8,
    /// Round trip in microseconds. Integer because atomics have no `f32`, and
    /// microseconds because milliseconds would round a fast local link to zero.
    rtt_micros: std::sync::atomic::AtomicU64,
    /// O jitter de chegada deste receptor, em microssegundos (RFC 3550).
    ///
    /// Guardado aqui porque quem o calcula é [`measure`], no laço de voz, e quem
    /// o mostra é o [`Plug::snapshot`], na casca — e antes disto ele era
    /// calculado, usado no Sync Ratio e jogado fora, enquanto a tela lia o zero
    /// que o Dogma manda de propósito (`session.rs` diz em comentário que o
    /// servidor não tem como medir jitter, porque jitter se mede no receptor).
    ///
    /// Microssegundos inteiros, e não os milissegundos em `f32` que a tela
    /// mostra, pelo mesmo motivo de [`Self::rtt_micros`] logo acima: não há
    /// átomo de `f32`, e um cadeado por quadro de interface seria contenção por
    /// nada. Um jitter de rede honesto vive na casa das unidades de
    /// milissegundo, então um milissegundo inteiro arredondaria a diferença
    /// entre um enlace bom e um ótimo para o mesmo número.
    jitter_de_chegada_micros: std::sync::atomic::AtomicU64,
    sync_ratio: AtomicU8,
    running: AtomicBool,
    /// Onde o enlace está, para o `Snapshot` contar à casca.
    ///
    /// Átomos e não um `Mutex`: isto é lido a cada quadro de interface e
    /// escrito raramente, e um cadeado por leitura seria contenção por nada.
    /// Zero segundos restantes significa "no ar".
    link_battery: AtomicBool,
    link_seconds: std::sync::atomic::AtomicU64,
    link_attempts: std::sync::atomic::AtomicU32,
    /// Quantas vezes o histórico mudou desde que esta sessão começou.
    ///
    /// Existe para o [`Snapshot`] poder dizer "mudou" sem carregar a conversa
    /// inteira. Antes ele carregava: cada quadro de interface clonava todo
    /// apelido e todo corpo já dito, serializava em JSON e atravessava a ponte
    /// — duas vezes por segundo, custando proporcionalmente ao tamanho da
    /// conversa. Numa sessão longa isso é atraso que só cresce, e foi assim que
    /// apareceu num teste entre duas máquinas.
    ///
    /// O número em si não significa nada; só a diferença significa. A casca
    /// guarda o último que desenhou e busca o histórico quando ele muda.
    messages_revision: std::sync::atomic::AtomicU64,
    /// Quem está esperando o peso de uma Linha, e por qual Linha.
    ///
    /// A única pergunta com resposta deste crate. Todo o resto que a casca pede
    /// é ordem — o Dogma confirma anunciando a mudança a todo mundo —, e este é
    /// o número de uma frase que só serve a quem perguntou, enquanto a caixa
    /// que ela enche estiver aberta.
    ///
    /// Uma lista e não um mapa por Linha: duas caixas sobre a mesma Linha ao
    /// mesmo tempo não acontecem numa janela só, e se acontecessem um mapa
    /// atenderia uma e deixaria a outra esperando para sempre. Aqui as duas
    /// recebem a mesma resposta.
    pending_weights: Mutex<Vec<(LineId, tokio::sync::oneshot::Sender<LineWeight>)>>,
}

impl Shared {
    /// Says the conversation moved: bumps the revision, then notifies.
    ///
    /// One call and not two, because the two must never happen apart. They did:
    /// `Command::OpenLine` clears the room's messages and emitted the event
    /// **without** bumping the revision, so a shell that refetches only when the
    /// number moves — which is the whole point of the number — swallowed it.
    /// Switching Line kept the previous Line's conversation on screen until
    /// somebody said something.
    ///
    /// That was a regression of the fix that stopped the snapshot carrying the
    /// whole history: before it, the shell re-read every message twice a second
    /// and a cleared list showed up on the next tick regardless. Making the
    /// shell smarter made this notification load-bearing, and it was not
    /// carrying its half.
    ///
    /// The bump comes first. A shell reacting to the event would otherwise read
    /// the old number and conclude there is nothing to fetch.
    fn messages_changed(&self) {
        self.messages_revision.fetch_add(1, Ordering::Relaxed);
        self.notify(&Event::MessagesChanged);
    }

    /// Guarda onde o enlace está, para o `Snapshot` contar à casca.
    fn gravar_enlace(&self, estado: seele_core::Link, restante: Option<std::time::Duration>) {
        let na_bateria = matches!(estado, seele_core::Link::InternalBattery { .. });
        self.link_battery.store(na_bateria, Ordering::Relaxed);
        self.link_seconds
            .store(restante.map_or(0, |q| q.as_secs()), Ordering::Relaxed);
        if let seele_core::Link::InternalBattery { attempts } = estado {
            self.link_attempts.store(attempts, Ordering::Relaxed);
        }
    }

    /// O estado do enlace como a casca o vê.
    fn enlace(&self) -> LinkState {
        if self.link_battery.load(Ordering::Relaxed) {
            LinkState::InternalBattery {
                remaining_seconds: self.link_seconds.load(Ordering::Relaxed),
                attempts: self.link_attempts.load(Ordering::Relaxed),
            }
        } else {
            LinkState::Online
        }
    }

    /// Guarda o jitter de chegada que o laço de voz acabou de medir.
    ///
    /// Só quem chama [`jitter_para_a_tela`] tem o que gravar aqui, e é de
    /// propósito: o número certo e o errado são ambos milissegundos em `f32`, e
    /// nada além daquela função separa um do outro.
    fn gravar_jitter_de_chegada(&self, ms: f32) {
        // Sem limitar a zero à mão: em Rust o `as` de ponto flutuante para
        // inteiro **satura**, e leva tanto o negativo quanto o `NaN` a zero
        // sozinho. Um `max` aqui seria uma guarda que nunca dispara, e uma
        // guarda que nunca dispara faz o teste que a exercita passar por si
        // mesmo em vez de guardar a propriedade.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "o `as` satura: negativo e NaN viram zero, e o teto de u64 está muito acima de qualquer jitter"
        )]
        let micros = (ms * 1000.0) as u64;
        self.jitter_de_chegada_micros
            .store(micros, Ordering::Relaxed);
    }

    /// O jitter de chegada como a casca o vê, em milissegundos.
    fn jitter_de_chegada_ms(&self) -> f32 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "um jitter em microssegundos está muito abaixo da faixa exata do f32"
        )]
        let ms = self.jitter_de_chegada_micros.load(Ordering::Relaxed) as f32 / 1000.0;
        ms
    }

    /// Entrega o peso a quem perguntou por aquela Linha.
    ///
    /// Quem sobrar na lista é de outra Linha e continua esperando. Ninguém fica
    /// esperando para sempre: o remetente é largado quando a tarefa do enlace
    /// termina, e um `oneshot` largado acorda quem espera com erro — que é como
    /// esta ponte já diz "não há sessão".
    /// Takes the transfer reasons the room collected, and empties the queue.
    ///
    /// Drained rather than read: two transfers can be in the air at once, and
    /// the second one failing must not erase the reason the first one did. The
    /// `Room` keeps them in order and this hands them on in order.
    fn drain_transfers(&self) -> Vec<Transfer> {
        let Ok(mut room) = self.room.lock() else {
            return Vec::new();
        };
        room.transfers
            .drain(..)
            .map(|aviso| match aviso {
                seele_core::TransferNotice::Refused {
                    client_message_id,
                    reason,
                } => Transfer::RefusedBecause {
                    client_message_id: client_message_id.get(),
                    reason: refusal_of(reason),
                },
                seele_core::TransferNotice::Unavailable { attachment, reason } => {
                    Transfer::Unavailable {
                        attachment: attachment.get(),
                        reason: refusal_of(reason),
                    }
                }
            })
            .collect()
    }

    fn answer_weight(&self, weight: LineWeight) {
        let Ok(mut pending) = self.pending_weights.lock() else {
            return;
        };
        let mut esperando = Vec::new();
        for (line, answer) in pending.drain(..) {
            if line.get() == weight.line {
                let _ = answer.send(weight);
            } else {
                esperando.push((line, answer));
            }
        }
        *pending = esperando;
    }

    fn notify(&self, event: &Event) {
        let listeners = match self.listeners.lock() {
            Ok(listeners) => listeners.clone(),
            Err(_) => return,
        };
        for listener in listeners {
            listener.on_event(event.clone());
        }
    }
}

/// A live session.
///
/// Dropping it disconnects.
pub struct Plug {
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Plug {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Plug")
            .field("running", &self.shared.running.load(Ordering::Relaxed))
            .finish()
    }
}

impl Plug {
    /// Connects, authenticates, and starts the driver thread.
    ///
    /// **Blocks** until the session reaches PADRÃO: AZUL or fails.
    ///
    /// # Errors
    ///
    /// Every failure is a [`PlugError`] variant, never a string: a shell has to
    /// be able to write its own sentence for each one.
    pub fn connect(config: ConnectConfig) -> Result<(Arc<Self>, Trust), PlugError> {
        let (address, server_name, pin_key) = resolve(&config.server)?;
        // The invite's other addresses, resolved here so the driver thread gets
        // values. An alternative that does not resolve is dropped: the first
        // address is the one the person is most likely on the same network as,
        // and refusing the whole link over a spare would trade one path fewer
        // for no path at all.
        let alternates: Vec<(SocketAddr, String, String)> = config
            .alternate_servers
            .iter()
            .filter_map(|alternate| resolve(alternate).ok())
            .collect();

        let home = PathBuf::from(&config.home);
        let key = identity::load_or_create(&home.join("identity.key")).map_err(|error| {
            tracing::warn!(%error, "identity unavailable");
            PlugError::IdentityUnavailable
        })?;
        let pins = Arc::new(FilePinStore::open(home.join("pins")).map_err(|error| {
            tracing::warn!(%error, "pin store unavailable");
            PlugError::IdentityUnavailable
        })?);

        let shared = Arc::new(Shared {
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            media: Mutex::new(None),
            nickname: Mutex::new(config.nickname.clone()),
            pattern: AtomicU8::new(pattern_byte(Pattern::Offline)),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
            sync_ratio: AtomicU8::new(0),
            running: AtomicBool::new(false),
            pending_weights: Mutex::new(Vec::new()),
        });

        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let thread_shared = Arc::clone(&shared);
        let thread_config = config.clone();
        std::thread::Builder::new()
            .name("seele-plug".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        tracing::error!(%error, "could not start the plug runtime");
                        let _ = ready_tx.send(Err(PlugError::Unreachable));
                        return;
                    }
                };
                runtime.block_on(drive(
                    thread_config,
                    address,
                    server_name,
                    pin_key,
                    alternates,
                    key,
                    pins,
                    thread_shared,
                    command_rx,
                    &ready_tx,
                ));
            })
            .map_err(|error| {
                tracing::error!(%error, "could not start the plug thread");
                PlugError::Unreachable
            })?;

        // The thread reports the outcome of the handshake, then keeps running.
        let trust = ready_rx.recv().map_err(|_| PlugError::Unreachable)??;

        let plug = Arc::new(Self {
            commands: command_tx,
            shared,
        });
        Ok((plug, trust))
    }

    /// Puts the plug into a Cage.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn insert_plug(&self, cage: u32) -> Result<(), PlugError> {
        self.command(Command::InsertPlug(CageId(cage)))
    }

    /// Takes the plug out.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn eject_plug(&self) -> Result<(), PlugError> {
        self.command(Command::EjectPlug)
    }

    /// Opens a Line and asks for the page of history behind it.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn open_line(&self, line: u32) -> Result<(), PlugError> {
        self.command(Command::OpenLine(LineId(line)))
    }

    /// Sends a file, on a stream of its own. ADR 0027.
    ///
    /// Asks, and does not wait: the bar comes back as
    /// [`Event::TransferChanged`], and the message appears on the Line only
    /// once the bytes have arrived whole. While it is going up, the only person
    /// who can see it is the sender — the cost ADR 0027 takes on purpose, so
    /// that "not arrived yet" and "expired" are never two similar absences on
    /// the same screen.
    ///
    /// `declared_type` is a **claim**, passed through as one.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn send_attachment(
        &self,
        line: u32,
        body: String,
        path: String,
        file_name: String,
        declared_type: String,
    ) -> Result<u64, PlugError> {
        // The key is taken here rather than on the queue, unlike `Send`: the
        // shell needs it **now**, to hang a bar on. Nothing else about a
        // message has ever had to be known before it was sent.
        let id = next_client_message_id();
        self.command(Command::Attach(Box::new(seele_core::enlace::Anexo {
            linha: LineId(line),
            id: ClientMessageId(id),
            corpo: body,
            caminho: std::path::PathBuf::from(path),
            nome: file_name,
            tipo: declared_type,
        })))?;
        Ok(id)
    }

    /// Saves an attachment where the person receiving it chose.
    ///
    /// **Where they chose, and nowhere else.** ADR 0027 gives no client of the
    /// SEELE a button that opens a file: saving is an act of the person who
    /// received it, in a place they picked. The file is marked with the
    /// operating system's own quarantine on the way down — which is not
    /// antivirus, and this product does not have one.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn save_attachment(&self, attachment: u64, destination: String) -> Result<(), PlugError> {
        self.command(Command::SaveAttachment {
            attachment: seele_core::AttachmentId(attachment),
            destination: std::path::PathBuf::from(destination),
        })
    }

    /// Fetches a small attachment and says whether a window may draw it.
    ///
    /// **On a press, never on a scroll**, and that is a decision this call
    /// leaves no room to get wrong: it downloads. The file lives on the Dogma,
    /// so looking at it costs the host's uplink, and a Line that previewed
    /// everything as it scrolled would turn a 1 GiB disk ceiling into a 1 GiB
    /// transfer every time somebody opened it.
    ///
    /// Nothing is written to disk on the way. Saving is a separate act with a
    /// separate confirmation, and this is not it.
    ///
    /// The claim the sender made is read from this client's own history rather
    /// than taken as an argument. The fewer hands a string somebody else chose
    /// passes through before it is compared to the bytes, the better.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over, and equally if it
    /// ends while the fetch is in flight.
    pub async fn preview_attachment(&self, attachment: u64) -> Result<Preview, PlugError> {
        let (answer, caixa) = tokio::sync::oneshot::channel();
        self.command(Command::PreviewAttachment {
            attachment: seele_core::AttachmentId(attachment),
            answer,
        })?;
        caixa.await.map_err(|_| PlugError::NotConnected)
    }

    /// What a screen needs to decide whether to offer a preview at all.
    ///
    /// Convenience, exactly like [`SessionInfo::permissions`] is: a window that
    /// asks anyway gets [`PreviewRefusal::TooBig`] or
    /// [`PreviewRefusal::NotAPicture`], so this saves a round trip and enforces
    /// nothing.
    ///
    /// The four types come from here rather than being written out again in a
    /// screen, because a second copy of the list is a second copy that can
    /// disagree — and it would disagree by offering to draw something the fetch
    /// then refuses.
    ///
    /// [`SessionInfo::permissions`]: seele_core::SessionInfo::permissions
    #[must_use]
    pub fn preview_rules() -> PreviewRules {
        PreviewRules {
            limit: seele_core::PREVIEW_LIMIT,
            types: seele_core::ImageFormat::ALL
                .iter()
                .map(|format| format.media_type().to_owned())
                .collect(),
        }
    }

    /// Says something in a Line.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn send_message(&self, line: u32, body: String) -> Result<(), PlugError> {
        if body.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::Send {
            line: LineId(line),
            body,
        })
    }

    /// Asks the Dogma to make a Cage.
    ///
    /// Asks. It does not decide, and it does not report back whether it worked
    /// — because it cannot: the answer comes from the Dogma, arrives on the
    /// event stream, and reaches the shell as [`Event::ChannelsChanged`] when
    /// the room exists or [`Event::NoticeRaised`] carrying
    /// [`NoticeReason::PermissionDenied`] when it does not. Returning a result
    /// here would mean this method waiting on a round trip, which is the one
    /// thing every other command on this object promises not to do.
    ///
    /// `line` binds a text channel to the room; `None` leaves it a voice room
    /// only, which `specs/04-servidor-seele.md` allows.
    ///
    /// The empty-name case returns `Ok` without sending anything, the same way
    /// [`Plug::send_message`] swallows an empty body: a person who pressed the
    /// button with nothing typed has not asked for anything, and answering that
    /// with an error would put a red message on a screen where nothing went
    /// wrong.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn create_cage(
        &self,
        name: String,
        limit: u16,
        line: Option<u32>,
    ) -> Result<(), PlugError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::CreateCage {
            name,
            limit,
            line: line.map(LineId),
        })
    }

    /// Asks the Dogma to make a Line.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn create_line(&self, name: String) -> Result<(), PlugError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::CreateLine { name })
    }

    /// Asks the Dogma to rename a Cage.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn rename_cage(&self, cage: u32, name: String) -> Result<(), PlugError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::RenameCage {
            cage: CageId(cage),
            name,
        })
    }

    /// Asks the Dogma to rename a Line.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn rename_line(&self, line: u32, name: String) -> Result<(), PlugError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::RenameLine {
            line: LineId(line),
            name,
        })
    }

    /// Asks the Dogma to end a pilot's session — `expulsar`.
    ///
    /// Asks, and reports nothing back, for the same reason [`Plug::create_cage`]
    /// gives: the answer comes from the Dogma. The roster losing them arrives as
    /// [`Event::RosterChanged`]; a refusal arrives as [`Event::NoticeRaised`]
    /// carrying [`NoticeReason::PermissionDenied`].
    ///
    /// A shell may read [`Snapshot::may_kick`] to decide whether to draw the
    /// control. That is **convenience, never enforcement** — pressing it
    /// without the permission removes nobody.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn kick_pilot(&self, pilot: u64) -> Result<(), PlugError> {
        self.command(Command::KickPilot {
            pilot: PilotId(pilot),
        })
    }

    /// Asks the Dogma to bar a pilot from returning — `banir`.
    ///
    /// `expires_at` is seconds since the Unix epoch; `None` is permanent. The
    /// `reason` is for whoever hosts, in their own records, and never reaches
    /// the person barred.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn ban_pilot(
        &self,
        pilot: u64,
        reason: Option<String>,
        expires_at: Option<i64>,
    ) -> Result<(), PlugError> {
        self.command(Command::BanPilot {
            pilot: PilotId(pilot),
            reason,
            expires_at,
        })
    }

    /// Asks the Dogma to take a message off its Line — `remover_mensagem`.
    ///
    /// It goes away for everybody, this client included, when the Dogma says so.
    /// An author removing their own needs no permission, which is why a shell
    /// drawing this control on one's own message may draw it for anybody.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn remove_message(&self, message: u64) -> Result<(), PlugError> {
        self.command(Command::RemoveMessage {
            message: MessageId(message),
        })
    }

    /// Asks the Dogma to move a pilot into a Cage — `mover_piloto`.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn move_pilot(&self, pilot: u64, cage: u32) -> Result<(), PlugError> {
        self.command(Command::MovePilot {
            pilot: PilotId(pilot),
            cage: CageId(cage),
        })
    }

    /// Asks the Dogma to destroy a Cage — `apagar_cage`.
    ///
    /// Everybody inside is turned out of it and told; the Line bound to it, if
    /// there is one, is left alone. The Dogma refuses the last Cage, and says so
    /// with [`NoticeReason::LastCage`] rather than with the sentence it uses for
    /// a refused entry.
    ///
    /// Asks, and nothing more. Nothing is removed from this client's own idea of
    /// the Dogma until the Dogma says the room is gone — a room removed
    /// optimistically would vanish off the screen of the person who asked
    /// whether or not it worked, and the refusal is the case they most need to
    /// see did not happen.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn delete_cage(&self, cage: u32) -> Result<(), PlugError> {
        self.command(Command::DeleteCage { cage: CageId(cage) })
    }

    /// Asks the Dogma to destroy a Line, and everything written in it —
    /// `apagar_linha`.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn delete_line(&self, line: u32) -> Result<(), PlugError> {
        self.command(Command::DeleteLine { line: LineId(line) })
    }

    /// Asks what destroying a Line would cost, and waits for the answer.
    ///
    /// The one call on this handle that waits, and the reason is the sentence it
    /// feeds: a confirmation promising to destroy 1.847 messages by 6 people
    /// written since a certain day has to have counted them, in the Dogma's own
    /// database, at the moment of asking. This client holds one page of history
    /// and would guess low by whatever the Line's whole past is — and a number
    /// that is nearly right in that box is worse than no number at all.
    ///
    /// So the caller waits, and a shell that cannot get an answer must not open
    /// the box: there is no honest version of it without these three numbers.
    ///
    /// Destroys nothing, and needs no permission — the Dogma answers about a
    /// Line the asker may already read.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over, and equally if it
    /// ends while the question is in flight: the driver drops what it was going
    /// to answer with, and this returns rather than waiting for a Dogma that is
    /// no longer there.
    pub async fn weigh_line(&self, line: u32) -> Result<LineWeight, PlugError> {
        let (answer, resposta) = tokio::sync::oneshot::channel();
        self.command(Command::WeighLine {
            line: LineId(line),
            answer,
        })?;
        resposta.await.map_err(|_| PlugError::NotConnected)
    }

    /// Mutes or unmutes the microphone — A.T. Field.
    ///
    /// Announced to the Dogma as well as applied locally: the roster shows it,
    /// and a mute nobody else can see is half a feature.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn set_at_field(&self, on: bool) -> Result<(), PlugError> {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                voice.set_at_field(on);
            }
        }
        self.command(Command::SetAtField(on))
    }

    /// Mutes or unmutes the speakers — Isolamento total.
    ///
    /// # Errors
    ///
    /// [`PlugError::NotConnected`] once the session is over.
    pub fn set_total_isolation(&self, on: bool) -> Result<(), PlugError> {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                voice.set_total_isolation(on);
            }
        }
        self.command(Command::SetTotalIsolation(on))
    }

    /// Reports the push-to-talk key going down or coming up.
    ///
    /// Not a command: it has to take effect on the next 20 ms frame, and a
    /// round trip through the driver thread would put a queue between a key and
    /// a microphone.
    pub fn set_talking(&self, talking: bool) {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                voice.set_key_held(talking);
            }
        }
    }

    /// Switches this session to another microphone.
    ///
    /// `device` is a [`CaptureDevice::id`] from [`capture_devices`]; `None` goes
    /// back to the machine's default. The choice is remembered, so a
    /// reconnection reopens the same microphone rather than quietly falling back
    /// to the default one. The chosen sound output is left where it is.
    ///
    /// Takes effect **now**, not on the next Cage. That is worth the extra
    /// mechanism: somebody opens this screen because the microphone they are
    /// speaking into is the wrong one, and telling them to leave the Dogma and
    /// come back is telling them to solve it themselves.
    ///
    /// Synchronous rather than queued, because the answer is the point: a device
    /// that has been unplugged since the list was drawn has to come back as an
    /// error the screen can put next to the row that was clicked.
    ///
    /// # Errors
    ///
    /// [`PlugError::CaptureDeviceGone`] when the machine is not offering that
    /// device any more, in which case **nothing changed** and the previous
    /// microphone is still live. [`PlugError::NoAudioDevice`] when this session
    /// has no audio at all — a session joined with the audio box unticked has no
    /// voice path to move.
    pub fn set_capture_device(&self, device: Option<String>) -> Result<(), PlugError> {
        // The new path opens before the old one is dropped, so a microphone that
        // turns out to be gone leaves the session speaking instead of silent.
        // `switch_capture` is what carries A.T. Field and the rest across — in
        // the core, so the terminal client gets the same list of what survives,
        // and the chosen output with it.
        self.switch_device(|running, media, ssrc| {
            running
                .switch_capture(device.as_deref(), media, ssrc)
                .map_err(|error| {
                    tracing::warn!(%error, "could not open the chosen microphone");
                    PlugError::CaptureDeviceGone
                })
        })
    }

    /// Switches this session to another sound output.
    ///
    /// `device` is a [`PlaybackDevice::id`] from [`playback_devices`]; `None`
    /// goes back to the machine's default. The microphone stays where it is.
    ///
    /// Takes effect **now**, for the reason its twin does and one more: somebody
    /// changes output in the middle of a conversation they cannot hear, and
    /// "leave the Dogma and come back" is not an instruction you can give
    /// somebody who is already unable to follow what is being said.
    ///
    /// Isolamento total survives the switch. That is decided in
    /// `Voice::switch_playback`, not here, so the terminal cannot decide it
    /// differently — and it matters because changing output is exactly what
    /// somebody does when they cannot hear anything, muted speakers included.
    ///
    /// # Errors
    ///
    /// [`PlugError::PlaybackDeviceGone`] when the machine is not offering that
    /// device any more, in which case **nothing changed** and the sound is still
    /// coming out of the old one. [`PlugError::NoAudioDevice`] when this session
    /// has no audio at all.
    pub fn set_playback_device(&self, device: Option<String>) -> Result<(), PlugError> {
        self.switch_device(|running, media, ssrc| {
            running
                .switch_playback(device.as_deref(), media, ssrc)
                .map_err(|error| {
                    tracing::warn!(%error, "could not open the chosen sound output");
                    PlugError::PlaybackDeviceGone
                })
        })
    }

    /// Replaces the live voice path with one the caller reopens.
    ///
    /// Shared by both switches so that the order stays one decision: open the
    /// new path, and only then let go of the old. Written twice, one copy would
    /// eventually drop first and leave somebody with no audio at all on the one
    /// path where the device turned out to be missing.
    fn switch_device(
        &self,
        reopen: impl FnOnce(&Voice, MediaChannel, Ssrc) -> Result<Voice, PlugError>,
    ) -> Result<(), PlugError> {
        let Ok(mut voice) = self.shared.voice.lock() else {
            return Err(PlugError::NoAudioDevice);
        };
        let Some(running) = voice.as_ref() else {
            return Err(PlugError::NoAudioDevice);
        };
        let Some((media, ssrc)) = self.shared.media.lock().ok().and_then(|slot| slot.clone())
        else {
            return Err(PlugError::NotConnected);
        };

        let fresh = reopen(running, media, ssrc)?;
        *voice = Some(fresh);
        Ok(())
    }

    /// Chooses how the microphone opens.
    pub fn set_voice_mode(&self, mode: VoiceMode) {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                voice.set_mode(mode.into());
            }
        }
    }

    /// Sets one talker's volume, as a percentage. 100 is unchanged.
    ///
    /// # Errors
    ///
    /// [`PlugError::UnknownPilot`] if nobody here is called that,
    /// [`PlugError::NoAudioDevice`] if this session has no audio.
    pub fn set_volume(&self, nickname: String, percent: u16) -> Result<(), PlugError> {
        let ssrc = self
            .shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.ssrc_of(&nickname))
            .ok_or(PlugError::UnknownPilot)?;

        let voice = self
            .shared
            .voice
            .lock()
            .map_err(|_| PlugError::NoAudioDevice)?;
        let voice = voice.as_ref().ok_or(PlugError::NoAudioDevice)?;
        voice.set_gain(ssrc.get(), f32::from(percent.min(400)) / 100.0);
        Ok(())
    }

    /// Subscribes to changes.
    ///
    /// Called back on the driver thread. The shell marshals.
    pub fn subscribe(&self, listener: Arc<dyn EventListener>) {
        if let Ok(mut listeners) = self.shared.listeners.lock() {
            listeners.push(listener);
        }
    }

    /// The conversation in the open Line, oldest first.
    ///
    /// Separate from [`Plug::snapshot`] because the two change at completely
    /// different rates. Telemetry moves on its own and wants reading twice a
    /// second; the history only moves when somebody says something. Carrying
    /// both in one value meant paying for the conversation on every frame —
    /// cloning each nickname and body, serialising the lot — so a session got
    /// slower the longer it went on. Ask for this when
    /// [`Snapshot::messages_revision`] changes, and not otherwise.
    #[must_use]
    pub fn messages(&self) -> Vec<Message> {
        self.shared
            .room
            .lock()
            .map(|room| messages_of(&room))
            .unwrap_or_default()
    }

    /// Everything the interface needs, in one value.
    ///
    /// Cheap enough to call on every frame of a redraw, and deliberately a copy:
    /// a shell holding a borrow into live state is a shell that can see the
    /// roster change halfway through drawing it.
    ///
    /// "Cheap" is now true. It used to carry the whole conversation, which made
    /// the cost grow with the session — see [`Snapshot::messages_revision`].
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let room = match self.shared.room.lock() {
            Ok(room) => room.clone(),
            Err(_) => Room::new(),
        };
        let nickname = self
            .shared
            .nickname
            .lock()
            .map(|name| name.clone())
            .unwrap_or_default();

        let audio = self.audio_state();

        let sync_ratio = self.shared.sync_ratio.load(Ordering::Relaxed);
        #[allow(
            clippy::cast_precision_loss,
            reason = "a round trip in microseconds is far below f32's exact range"
        )]
        let rtt_ms = self.shared.rtt_micros.load(Ordering::Relaxed) as f32 / 1000.0;

        Snapshot {
            link: self.shared.enlace(),
            pattern: pattern_from_byte(self.shared.pattern.load(Ordering::Relaxed)),
            dogma: room.dogma.clone(),
            me: room.me.map(|pilot| pilot.0),
            nickname,
            cages: cages_of(&room),
            lines: lines_of(&room),
            messages_revision: self.shared.messages_revision.load(Ordering::Relaxed),
            telemetry: Telemetry {
                rtt_ms,
                // O jitter que a pessoa quer saber é o de chegada, medido
                // aqui — e não o do relatório do Dogma, que é sempre `0.0`
                // porque o servidor não tem como medir jitter. Ver
                // [`Shared::jitter_de_chegada_micros`].
                jitter_ms: self.shared.jitter_de_chegada_ms(),
                loss_fraction: room.telemetry.as_ref().map_or(0.0, |t| t.loss_fraction),
                bitrate_bps: audio.bitrate_bps,
                sync_ratio,
                sync_band: SyncBand::of(sync_ratio).into(),
                input_level: audio.input_level,
                local_fault: audio.local_fault,
                frames_refused: audio.frames_refused,
            },
            notice: room.notice.as_ref().map(|notice| Notice {
                severity: notice.severity.into(),
                reason: notice.reason.into(),
                operator_text: notice.operator_text.clone(),
            }),
            at_field: audio.at_field,
            total_isolation: audio.total_isolation,
            speaking: audio.speaking,
            voice_mode: audio.mode,
            audio_available: audio.available,
            capture: audio.capture,
            playback: audio.playback,
            may_manage_cages: room
                .permissions
                .contains(&seele_core::Permission::ManageCages),
            may_kick: room.permissions.contains(&seele_core::Permission::Kick),
            may_ban: room.permissions.contains(&seele_core::Permission::Ban),
            may_remove_message: room
                .permissions
                .contains(&seele_core::Permission::RemoveMessage),
            may_move_pilot: room
                .permissions
                .contains(&seele_core::Permission::MovePilot),
            may_delete_rooms: room
                .permissions
                .contains(&seele_core::Permission::AdministerDogma),
            ended: room.ended.map(|end| end.reason.into()),
        }
    }

    /// Ends the session.
    pub fn disconnect(&self) {
        self.shared.running.store(false, Ordering::Relaxed);
        let _ = self.commands.send(Command::Shutdown);
    }

    fn command(&self, command: Command) -> Result<(), PlugError> {
        self.commands
            .send(command)
            .map_err(|_| PlugError::NotConnected)
    }

    fn audio_state(&self) -> AudioState {
        let Ok(voice) = self.shared.voice.lock() else {
            return AudioState::silent();
        };
        let Some(voice) = voice.as_ref() else {
            return AudioState::silent();
        };
        let telemetry = voice.telemetry();
        AudioState {
            available: true,
            mode: match voice.mode() {
                seele_core::VoiceMode::PushToTalk => VoiceMode::PushToTalk,
                seele_core::VoiceMode::VoiceActivated => VoiceMode::VoiceActivated,
                seele_core::VoiceMode::Open => VoiceMode::Open,
            },
            speaking: telemetry.local.speaking,
            at_field: voice.at_field(),
            total_isolation: voice.total_isolation(),
            input_level: telemetry.local.input_level,
            local_fault: voice.falha_local(),
            frames_refused: voice.quadros_recusados(),
            bitrate_bps: telemetry.local.bitrate_bps,
            capture: voice.capture().map(|device| CaptureDevice {
                id: device.id.clone(),
                name: device.name.clone(),
                default: device.default,
            }),
            playback: voice.playback().map(|device| PlaybackDevice {
                id: device.id.clone(),
                name: device.name.clone(),
                default: device.default,
            }),
        }
    }
}

/// What the voice path is doing, read once per [`Plug::snapshot`].
///
/// A struct and not the eight-tuple this was: the tuple had already reached the
/// point of needing `clippy::type_complexity` waved through, and its run of
/// consecutive `bool`s was three chances to swap two fields with nothing
/// anywhere to catch it.
struct AudioState {
    available: bool,
    mode: VoiceMode,
    speaking: bool,
    at_field: bool,
    total_isolation: bool,
    input_level: f32,
    local_fault: bool,
    frames_refused: u64,
    bitrate_bps: u32,
    capture: Option<CaptureDevice>,
    playback: Option<PlaybackDevice>,
}

impl AudioState {
    /// A session with no voice path — before audio opens, and after it stops.
    ///
    /// Push-to-talk rather than a derived `Default`, and that is the whole
    /// reason this is written out: `specs/03-audio.md` picks the mode that never
    /// false-triggers, and a placeholder that read as `Open` would draw an open
    /// microphone on a session that has none.
    fn silent() -> Self {
        Self {
            available: false,
            mode: VoiceMode::PushToTalk,
            speaking: false,
            at_field: false,
            total_isolation: false,
            input_level: 0.0,
            local_fault: false,
            frames_refused: 0,
            bitrate_bps: 0,
            capture: None,
            playback: None,
        }
    }
}

impl Drop for Plug {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn cages_of(room: &Room) -> Vec<Cage> {
    room.cages
        .iter()
        .map(|cage| Cage {
            id: cage.id.0,
            name: cage.name.clone(),
            limit: cage.limit,
            password_required: cage.password_required,
            occupied_by_us: room.current_cage == Some(cage.id),
            line: cage.line.map(|line| line.0),
            pilots: room
                .roster(cage.id)
                .map(|pilot| Pilot {
                    id: pilot.id.0,
                    nickname: pilot.nickname.clone(),
                    speaking: pilot.speaking,
                    at_field: pilot.at_field,
                    total_isolation: pilot.total_isolation,
                    sync_ratio: pilot.sync_ratio,
                    sync_band: SyncBand::of(pilot.sync_ratio).into(),
                    is_self: room.me == Some(pilot.id),
                })
                .collect(),
            // Off the core, not folded here: the terminal draws the same number
            // from the same method, and a mean computed in two shells is a mean
            // two shells will one day round differently.
            sync: room.cage_sync(cage.id).map(Into::into),
        })
        .collect()
}

fn lines_of(room: &Room) -> Vec<Line> {
    room.lines
        .iter()
        .map(|line| Line {
            id: line.id.0,
            name: line.name.clone(),
            open: room.current_line == Some(line.id),
        })
        .collect()
}

fn messages_of(room: &Room) -> Vec<Message> {
    room.messages
        .iter()
        .map(|message| Message {
            id: message.id.0,
            line: message.line.0,
            author: message.author.0,
            author_nickname: message.author_nickname.clone(),
            at_seconds: message.at_seconds,
            body: message.body.clone(),
            own: message.own,
            edited: message.edited,
            attachment: message.attachment.as_ref().map(|anexo| Attachment {
                id: anexo.id.get(),
                file_name: anexo.file_name.clone(),
                declared_type: anexo.declared_type.clone(),
                byte_size: anexo.byte_size,
                // Flattened to a boolean, and it is the whole state: the wire
                // enum has two variants and a third would be a state the design
                // does not have. ADR 0027 argues for exactly two — "this file
                // has not arrived yet" was deliberately never made a state, so
                // that it could not be confused with this one.
                expired: anexo.state == seele_core::AttachmentState::Expired,
            }),
        })
        .collect()
}

/// Maps a wire refusal onto this crate's own.
fn refusal_of(reason: seele_core::AttachmentRefusal) -> AttachmentRefusal {
    match reason {
        seele_core::AttachmentRefusal::NotAllowed => AttachmentRefusal::NotAllowed,
        seele_core::AttachmentRefusal::TooLarge { limit } => AttachmentRefusal::TooLarge { limit },
        seele_core::AttachmentRefusal::NoRoom => AttachmentRefusal::NoRoom,
        seele_core::AttachmentRefusal::SizeMismatch => AttachmentRefusal::SizeMismatch,
        seele_core::AttachmentRefusal::HashDidNotMatch => AttachmentRefusal::HashDidNotMatch,
        seele_core::AttachmentRefusal::RateLimited => AttachmentRefusal::RateLimited,
        seele_core::AttachmentRefusal::Unavailable => AttachmentRefusal::Unavailable,
        seele_core::AttachmentRefusal::NotFound => AttachmentRefusal::NotFound,
        seele_core::AttachmentRefusal::Expired => AttachmentRefusal::Expired,
        seele_core::AttachmentRefusal::Malformed => AttachmentRefusal::Malformed,
    }
}

/// Turns one step of a transfer into what a shell draws.
fn transfer_of(estado: &seele_core::enlace::Transferencia) -> Transfer {
    use seele_core::enlace::Transferencia;
    match estado {
        Transferencia::Subindo { id, feito, total } => Transfer::Sending {
            client_message_id: id.get(),
            done: *feito,
            total: *total,
        },
        Transferencia::Subiu { id } => Transfer::Sent {
            client_message_id: id.get(),
        },
        Transferencia::Recusada { id } => Transfer::Refused {
            client_message_id: id.get(),
        },
        Transferencia::Caiu { id } => Transfer::Fell {
            client_message_id: id.get(),
        },
        Transferencia::Baixando {
            anexo,
            feito,
            total,
        } => Transfer::Receiving {
            attachment: anexo.get(),
            done: *feito,
            total: *total,
        },
        Transferencia::Salvo { anexo, caminho } => Transfer::Saved {
            attachment: anexo.get(),
            path: caminho.display().to_string(),
        },
        Transferencia::NaoSalvou { anexo } => Transfer::NotSaved {
            attachment: anexo.get(),
        },
    }
}

fn pattern_byte(pattern: Pattern) -> u8 {
    match pattern {
        Pattern::Offline => 0,
        Pattern::Orange => 1,
        Pattern::Blue => 2,
    }
}

fn pattern_from_byte(byte: u8) -> Pattern {
    match byte {
        1 => Pattern::Orange,
        2 => Pattern::Blue,
        _ => Pattern::Offline,
    }
}

/// Resolves `host` or `host:port` into an address, a TLS label, and a pin key.
///
/// Three values because they are three things — see the same function in
/// `seele-tui`, and `TofuVerifier::new` for why keying the pin by the TLS label
/// was wrong.
/// The split is `seele_core::uri::separar` and not `rsplit_once(':')`: the port
/// separator and an IPv6's own separator are the same character, and doing it by
/// hand here made `[2001:db8::1]:8383` resolve to nothing. ADR 0022, step 2.
fn resolve(target: &str) -> Result<(SocketAddr, String, String), PlugError> {
    let alvo = seele_core::uri::separar(target).map_err(|_| PlugError::UnresolvableHost)?;
    let address = (alvo.maquina, alvo.porta)
        .to_socket_addrs()
        .map_err(|_| PlugError::UnresolvableHost)?
        .next()
        .ok_or(PlugError::UnresolvableHost)?;

    // TLS gets the name the M2 certificate carries; the pin gets the address,
    // which is what actually tells one server from another.
    let server_name = if alvo.maquina.parse::<std::net::IpAddr>().is_ok() {
        "localhost".to_owned()
    } else {
        alvo.maquina.to_owned()
    };
    // The pin key is written back in the canonical form rather than as typed, so
    // `[::1]:8383` and `[::1]` file under one entry instead of two. An IPv6 goes
    // back into its brackets: without them the key is ambiguous with `host:port`.
    let pin_key = if address.is_ipv6() && alvo.maquina.contains(':') {
        format!("[{}]:{}", alvo.maquina, alvo.porta)
    } else {
        format!("{}:{}", alvo.maquina, alvo.porta)
    };
    Ok((address, server_name, pin_key))
}

/// Turns a [`ConnectConfig`] and what [`resolve`] made of it into a
/// [`seele_core::enlace::Destino`].
///
/// Pulled apart from `drive` for the same reason `seele_core::enlace` pulls
/// `conferir` apart from `Enlace::conectar`: everything here is a decision on
/// values, and a decision on values does not need a QUIC socket to test.
/// Without this split, whether `expected_fingerprint` actually reaches
/// `impressao_esperada` could only be checked by a live handshake — which is
/// exactly the gap `every_expected_fingerprint_reaches_the_destino_it_promised`
/// below closes.
fn build_destino(
    config: &ConnectConfig,
    address: SocketAddr,
    server_name: &str,
    pin_key: &str,
) -> seele_core::enlace::Destino {
    seele_core::enlace::Destino {
        servidor: address,
        nome_tls: server_name.to_owned(),
        chave_do_pin: pin_key.to_owned(),
        apelido: config.nickname.clone(),
        segredo: config.join_secret.clone(),
        impressao_esperada: config.expected_fingerprint.clone(),
    }
}

/// The driver: connects, then pumps until told to stop.
#[allow(
    clippy::too_many_arguments,
    reason = "one private function, called once, with everything the thread owns"
)]
async fn drive(
    config: ConnectConfig,
    address: SocketAddr,
    server_name: String,
    pin_key: String,
    alternates: Vec<(SocketAddr, String, String)>,
    key: seele_core::SigningKey,
    pins: Arc<FilePinStore>,
    shared: Arc<Shared>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    ready: &std::sync::mpsc::Sender<Result<Trust, PlugError>>,
) {
    shared
        .pattern
        .store(pattern_byte(Pattern::Orange), Ordering::Relaxed);

    // `Enlace` e não `Client`: é a sessão que atravessa quedas, com a bateria
    // interna dentro. Antes disto, o app pulava de "conectado" para "encerrado"
    // no primeiro soluço de rede.
    let destinos = std::iter::once((address, server_name.clone(), pin_key.clone()))
        .chain(alternates)
        .map(|(address, server_name, pin_key)| {
            build_destino(&config, address, &server_name, &pin_key)
        })
        .collect();
    // `conectar_entre_com_bilhete` e não `conectar_entre`: com um bilhete no
    // link, quem entra bate no ponto de encontro antes de tentar endereço nenhum
    // — degrau 4 do ADR 0022. Sem bilhete os dois caminhos são o mesmo.
    let bilhete = config.bilhete.clone();
    let mut client =
        match seele_core::enlace::Enlace::conectar_entre_com_bilhete(destinos, bilhete, key, pins)
            .await
        {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!(%error, "could not reach the Dogma");
                let _ = ready.send(Err(classify_connect_failure(&error)));
                return;
            }
        };

    // `Enlace::conectar` already returned `Err` above for anything that would
    // have made this a `PinDecision::Changed` or a refused invite — see
    // `seele_core::tofu::verdict`'s own note that `Changed` never reaches a
    // caller as a verdict. What is left here is a verdict the shell is allowed
    // to see, not one that had to be turned back into a connection error.
    let trust = Trust::from(client.veredito().clone());

    if let Ok(mut room) = shared.room.lock() {
        room.adopt(client.sessao(), &config.nickname);
    }
    shared
        .pattern
        .store(pattern_byte(Pattern::Blue), Ordering::Relaxed);

    remember_media(&shared, client.media(), client.sessao().ssrc);

    if config.audio {
        // `start_preferring` and not `start_on`: it falls back to the machine's
        // own device, per side, rather than refusing the session. A preference
        // written down last week names a device that may be in another room by
        // now, and turning that into a Dogma nobody can enter would make the
        // picker the most dangerous control in the app. The screen shows what
        // actually opened, so the fallback is visible — and per side, so a
        // headset left behind does not also throw away the microphone.
        let opened = Voice::start_preferring(
            &chosen_devices(&config),
            client.media(),
            client.sessao().ssrc,
        );
        match opened {
            Ok(voice) => {
                if let Ok(mut slot) = shared.voice.lock() {
                    *slot = Some(voice);
                }
            }
            // No microphone is not a reason to have no session. The text half
            // works, and `audio_available` says which half this is.
            Err(error) => tracing::warn!(%error, "no audio device; text only"),
        }
    }

    shared.running.store(true, Ordering::Relaxed);
    let _ = ready.send(Ok(trust));

    let mut sync = SyncRatio::new();
    let mut next_tick = Instant::now() + TICK;

    while shared.running.load(Ordering::Relaxed) {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                if !run_command(&client, &shared, command).await {
                    break;
                }
            }

            aviso = client.proximo() => {
                match aviso {
                    seele_core::enlace::Aviso::Mensagem(message) => {
                        // A resposta a uma pergunta, e não um fato sobre a
                        // sala: ela vai para quem perguntou e não entra no
                        // `Room`. Antes do `fold` porque é onde ela para —
                        // `Room::apply` não tem arm que a guarde, de propósito.
                        if let seele_core::ServerMessage::LineWeighed {
                            line, messages, authors, oldest_at_seconds,
                        } = message.as_ref() {
                            shared.answer_weight(LineWeight {
                                line: line.get(),
                                messages: *messages,
                                authors: *authors,
                                oldest_at_seconds: *oldest_at_seconds,
                            });
                        }
                        fold(&shared, &message);
                        // A razão de uma recusa, que o `Room` guardou ao dobrar
                        // a mensagem. Drenada em vez de lida: duas
                        // transferências podem estar no ar, e a segunda falhar
                        // não pode apagar o motivo da primeira.
                        for aviso in shared.drain_transfers() {
                            shared.notify(&Event::TransferChanged { transfer: aviso });
                        }
                    }
                    seele_core::enlace::Aviso::Transferencia(estado) => {
                        shared.notify(&Event::TransferChanged {
                            transfer: transfer_of(&estado),
                        });
                    }
                    // Cair não encerra: começa a bateria interna, e a casca
                    // esmaece em vez de fechar.
                    seele_core::enlace::Aviso::Estado { estado, restante } => {
                        shared.gravar_enlace(estado, restante);
                        shared.notify(&Event::TelemetryChanged);
                    }
                    seele_core::enlace::Aviso::Reconectado { media, sessao } => {
                        shared.gravar_enlace(seele_core::Link::Online, None);
                        if let Ok(mut room) = shared.room.lock() {
                            room.adopt(&sessao, &config.nickname);
                        }
                        // Conexão nova, `ssrc` novo, canal de mídia novo: sem
                        // reabrir, a voz sairia por uma conexão morta.
                        remember_media(&shared, (*media).clone(), sessao.ssrc);
                        if let Ok(mut slot) = shared.voice.lock() {
                            if let Some(atual) = slot.as_ref() {
                                // `reopen` e não `start_on`, porque os controles
                                // têm que atravessar a reabertura. A lista mora
                                // em `Voice::switch_capture` — A.T. Field,
                                // Isolamento total, o modo, a tecla segura, cada
                                // ganho por interlocutor — e está lá justamente
                                // para que nenhuma casca esqueça um item. Esta
                                // esquecia todos.
                                //
                                // O que torna isto pior que "volta desmutado" é
                                // que `Enlace::tentar` **restaura** o A.T. Field
                                // no servidor: o roster continuava mostrando a
                                // pessoa muda enquanto o portão local voltava
                                // aberto, e o indicador que todo mundo lê
                                // passava a mentir.
                                //
                                // `reopen` também é quem sabe **o que** reabrir:
                                // a voz guarda os dois dispositivos que pediu, e
                                // volta pedindo os mesmos. Sem isso, voltar do ar
                                // trocaria o microfone e a saída de quem está no
                                // meio de uma conversa — e trocaria calado, que é
                                // a pior parte. O recuo por lado é o mesmo do
                                // caminho de conexão: uma interface que sumiu
                                // enquanto estávamos fora do ar não pode custar a
                                // voz do resto da sessão.
                                match atual.reopen((*media).clone(), sessao.ssrc) {
                                    Ok(voice) => *slot = Some(voice),
                                    // A mesma degradação do caminho de conexão:
                                    // a metade de texto continua funcionando, e
                                    // `audio_available` diz qual metade é esta.
                                    Err(error) => {
                                        tracing::warn!(%error, "no audio device after reconnecting; text only");
                                        *slot = None;
                                    }
                                }
                            }
                        }
                        shared.notify(&Event::TelemetryChanged);
                    }
                    seele_core::enlace::Aviso::Encerrado(motivo) => {
                        let reason = match motivo {
                            seele_core::enlace::Motivo::Descarregou => EndReason::LinkLost,
                            seele_core::enlace::Motivo::Recusado(_) => EndReason::CredentialRejected,
                            seele_core::enlace::Motivo::Pedido => EndReason::LinkLost,
                        };
                        shared.notify(&Event::Ended { reason });
                        break;
                    }
                }
            }

            () = tokio::time::sleep_until(next_tick.into()) => {
                next_tick = Instant::now() + TICK;
                if measure(&mut sync, &client, &shared) {
                    shared.notify(&Event::TelemetryChanged);
                }
            }
        }
    }

    shared.running.store(false, Ordering::Relaxed);
    shared
        .pattern
        .store(pattern_byte(Pattern::Offline), Ordering::Relaxed);
    // Dropping the voice stops the audio thread with the session.
    if let Ok(mut voice) = shared.voice.lock() {
        *voice = None;
    }
    // E o canal com ela: um `set_capture_device` que chegasse depois abriria um
    // caminho de voz sobre uma conexão morta, e ele ficaria de pé sem sessão
    // nenhuma por trás.
    if let Ok(mut media) = shared.media.lock() {
        *media = None;
    }
    // Quem estava esperando o peso de uma Linha é acordado, e não deixado
    // pendurado. Largar o remetente é o que faz `weigh_line` devolver
    // `NotConnected` em vez de esperar uma resposta que não vem mais — e o que
    // impede uma caixa de confirmação de ficar aberta e muda depois de a sessão
    // acabar debaixo dela.
    if let Ok(mut pending) = shared.pending_weights.lock() {
        pending.clear();
    }
    client.sair().await;
}

/// Os dois dispositivos que a casca pediu, como o core os lê.
///
/// Uma função e não dois campos soltos no ponto de chamada: os dois são
/// `Option<String>`, e trocá-los compilaria — a pessoa acabaria com a voz
/// saindo pelo microfone.
fn chosen_devices(config: &ConnectConfig) -> seele_core::DeviceChoice {
    seele_core::DeviceChoice {
        capture: config.capture_device.clone(),
        playback: config.playback_device.clone(),
    }
}

/// Guarda por onde a voz sai, para quem for reabri-la.
fn remember_media(shared: &Arc<Shared>, media: MediaChannel, ssrc: Ssrc) {
    if let Ok(mut slot) = shared.media.lock() {
        *slot = Some((media, ssrc));
    }
}

/// Folds a server message into the room and tells the shell what moved.
fn fold(shared: &Arc<Shared>, message: &seele_core::ServerMessage) {
    let changed = match shared.room.lock() {
        Ok(mut room) => room.apply(message),
        Err(_) => return,
    };

    if changed.roster {
        shared.notify(&Event::RosterChanged);
    }
    if changed.messages {
        shared.messages_changed();
    }
    if changed.channels {
        shared.notify(&Event::ChannelsChanged);
    }
    if changed.telemetry {
        shared.notify(&Event::TelemetryChanged);
    }
    if changed.notice {
        let notice = shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.notice.clone())
            .map(|notice| Notice {
                severity: notice.severity.into(),
                reason: notice.reason.into(),
                operator_text: notice.operator_text,
            });
        if let Some(notice) = notice {
            shared.notify(&Event::NoticeRaised { notice });
        }
    }
    if changed.ended {
        let reason = shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.ended)
            .map_or(EndReason::LinkLost, |end| end.reason.into());
        shared.notify(&Event::Ended { reason });
    }
}

/// Qual das duas grandezas chamadas "jitter" vai para a tela.
///
/// São duas, com o mesmo nome, e mostrar a errada é pior que não mostrar nada:
///
/// - `chegada_ms` é o **jitter de chegada** da RFC 3550 — a variação do
///   intervalo entre pacotes, medida neste receptor por
///   `seele_audio::jitter`, que alisa por `J += (|D(i-1,i)| - J) / 16`. É ruído
///   da rede, e é o que a pessoa quer saber: quanto menor, melhor;
/// - `profundidade_do_anel_ms` é a **profundidade do anel de reprodução** — a
///   nossa própria reserva contra esse ruído, que o ADR 0028 acabou de dotar de
///   alvo. Quanto maior, mais folga o anel teve. Mostrá-la como "jitter" exibiria
///   a reserva como se fosse o problema, e uma conexão saudável apareceria na
///   tela como ruim.
///
/// A resposta é sempre a primeira, e a segunda é ignorada de propósito. Um
/// parâmetro ignorado parece bobo e não é: ele existe para o segundo número ter
/// de ser **escrito** por quem chama, e para quem trocar os dois de lugar
/// reprovar aqui, num teste desta casa, em vez de na tela de outra pessoa.
///
/// O que a tela lia antes deste conserto não era nenhum dos dois: era o jitter
/// do relatório do Dogma, que é `0.0` fixo porque o servidor não tem como medir
/// uma grandeza que só existe no receptor.
fn jitter_para_a_tela(chegada_ms: f32, profundidade_do_anel_ms: f32) -> f32 {
    let _ = profundidade_do_anel_ms;
    chegada_ms
}

/// As duas grandezas chamadas "jitter" que uma volta de telemetria produz, já
/// separadas e com destino escrito no nome.
///
/// Existe como estrutura, e não como uma tupla de dois `f32`, porque uma tupla
/// de dois `f32` na mesma unidade é exatamente a forma que deixa trocar os dois
/// sem nada notar — que é o defeito 3.3 outra vez, com outra roupa.
struct JitterDaVolta {
    /// Vai para o Sync Ratio: quanta reserva o anel de reprodução teve.
    profundidade_do_anel_ms: f32,
    /// Vai para a tela: o jitter de chegada da RFC 3550, medido neste receptor.
    ///
    /// Vazio quando não há fonte nenhuma sendo recebida. `None` e não zero de
    /// propósito: o pior de uma lista vazia seria zero, e zero é o valor que
    /// este conserto existe para tirar da tela.
    chegada_ms: Option<f32>,
}

/// Separa as duas, a partir do que a camada de áudio mediu.
///
/// Apartada de [`measure`] para poder ser afirmada por um teste: `measure`
/// precisa de um [`Enlace`] vivo e de um dispositivo de áudio aberto, e sem esta
/// separação a única coisa que sobrava para testar era a função pura dos dois
/// lados da fiação — que passava verde com a fiação desfeita.
fn jitter_da_volta(telemetria: &seele_core::AudioTelemetry) -> JitterDaVolta {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "milissegundos; f32 é o que a ponte para a casca carrega"
    )]
    let volta = JitterDaVolta {
        profundidade_do_anel_ms: telemetria.worst_jitter_depth_ms() as f32,
        // O pior entre as fontes, como já vale para a perda e para a
        // profundidade: um único interlocutor ruim tem de ficar visível sem a
        // casca ter de varrer a lista.
        chegada_ms: telemetria
            .sources
            .iter()
            .map(|fonte| fonte.jitter_ms)
            .reduce(f64::max)
            .map(|chegada| chegada as f32),
    };
    volta
}

/// Refreshes what the shell reads without asking the server.
///
/// Returns whether anything moved enough to be worth telling a shell about.
fn measure(sync: &mut SyncRatio, client: &Enlace, shared: &Arc<Shared>) -> bool {
    let mut jitter_ms = 0.0;
    let mut loss = 0.0;
    let mut chegada_ms = None;
    if let Ok(voice) = shared.voice.lock() {
        if let Some(voice) = voice.as_ref() {
            let telemetry = voice.telemetry();
            // Jitter and loss are only observable at the receiver, which is why
            // the server's own numbers are not the ones used here.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "a fraction; f32 is what the protocol carries"
            )]
            {
                loss = telemetry.worst_loss_fraction() as f32;
            }
            // Duas grandezas, dois destinos. A profundidade do anel continua
            // indo para o Sync Ratio, que é o que ela mede: quanta reserva o
            // anel teve. O de chegada vai para a tela, que é o que a pessoa quer
            // saber.
            let volta = jitter_da_volta(&telemetry);
            jitter_ms = volta.profundidade_do_anel_ms;
            chegada_ms = volta.chegada_ms;
        }
    }

    if let Some(chegada) = chegada_ms {
        shared.gravar_jitter_de_chegada(jitter_para_a_tela(chegada, jitter_ms));
    }

    let rtt = client.rtt().unwrap_or_default();
    let rtt_micros = u64::try_from(rtt.as_micros()).unwrap_or(u64::MAX);
    #[allow(
        clippy::cast_precision_loss,
        reason = "a round trip in microseconds is far below f32's exact range"
    )]
    let ratio = sync.update(SyncInputs {
        rtt_ms: rtt_micros as f32 / 1000.0,
        jitter_ms,
        loss_fraction: loss,
    });

    let previous_ratio = shared.sync_ratio.swap(ratio, Ordering::Relaxed);
    let previous_rtt = shared.rtt_micros.swap(rtt_micros, Ordering::Relaxed);

    // A shell redrawing because the round trip moved by a microsecond is a
    // shell redrawing thirty times a second for nothing.
    previous_ratio != ratio || previous_rtt.abs_diff(rtt_micros) > 1_000
}

/// Runs one command. Returns false when the driver should stop.
async fn run_command(client: &Enlace, shared: &Arc<Shared>, command: Command) -> bool {
    match command {
        Command::InsertPlug(cage) => {
            if client.inserir_plug(cage).await.is_err() {
                return false;
            }
            if let Ok(mut room) = shared.room.lock() {
                room.enter_cage(cage);
            }
            shared.notify(&Event::RosterChanged);
        }
        Command::EjectPlug => {
            if client.ejetar_plug().await.is_err() {
                return false;
            }
        }
        Command::OpenLine(line) => {
            if client.abrir_linha(line).await.is_err() {
                return false;
            }
            if let Ok(mut room) = shared.room.lock() {
                room.open_line(line);
            }
            // The fetch is what makes "sem perda de histórico" true: a client
            // arriving late reads what was said instead of an empty room.
            if client.historico(line, HISTORY_PAGE).await.is_err() {
                return false;
            }
            // A Linha trocou, então a conversa trocou — mesmo que nenhuma
            // mensagem nova tenha chegado. `Room::open_line` limpou a lista, e
            // sem isto a tela continuaria mostrando a conversa da Linha
            // anterior sob o nome da nova.
            shared.messages_changed();
        }
        Command::Send { line, body } => {
            // specs/02-protocolo.md: idempotent by client_msg_id, so a resend
            // after a lost acknowledgement does not post twice.
            let id = ClientMessageId(next_client_message_id());
            if client
                .dizer(line, body.trim().to_owned(), id)
                .await
                .is_err()
            {
                return false;
            }
        }
        Command::Attach(anexo) => {
            if client.anexar(*anexo).await.is_err() {
                return false;
            }
        }
        Command::SaveAttachment {
            attachment,
            destination,
        } => {
            if client.salvar_anexo(attachment, destination).await.is_err() {
                return false;
            }
        }
        Command::SetAtField(on) => {
            if client.at_field(on).await.is_err() {
                return false;
            }
        }
        Command::SetTotalIsolation(on) => {
            if client.isolamento(on).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` here, unlike entering a Cage
        // or opening a Line. Those two are facts about this client, which the
        // server confirms by silence; a room is a fact about the Dogma, and the
        // only honest source for it is the Dogma saying it exists. Writing it in
        // optimistically would draw a room for the person who asked even when
        // the server refused them.
        Command::CreateCage { name, limit, line } => {
            if client.criar_cage(name, limit, line).await.is_err() {
                return false;
            }
        }
        Command::CreateLine { name } => {
            if client.criar_linha(name).await.is_err() {
                return false;
            }
        }
        Command::RenameCage { cage, name } => {
            if client.renomear_cage(cage, name).await.is_err() {
                return false;
            }
        }
        Command::RenameLine { line, name } => {
            if client.renomear_linha(line, name).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` for these either, and for a
        // sharper version of the reason above: what a moderation verb changes
        // is somebody **else's** session. The only honest source for "they are
        // gone" is the `PilotLeft` the Dogma sends when it is true. Marking it
        // here would draw a roster the person who pressed the button is alone
        // in believing — and draw it identically whether the server did it or
        // refused, which is the exact difference the button exists to expose.
        Command::KickPilot { pilot } => {
            if client.expulsar(pilot).await.is_err() {
                return false;
            }
        }
        Command::BanPilot {
            pilot,
            reason,
            expires_at,
        } => {
            if client.banir(pilot, reason, expires_at).await.is_err() {
                return false;
            }
        }
        Command::RemoveMessage { message } => {
            if client.remover_mensagem(message).await.is_err() {
                return false;
            }
        }
        Command::MovePilot { pilot, cage } => {
            if client.mover_piloto(pilot, cage).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` for these either, and here
        // the reason is at its sharpest: a room removed optimistically would
        // vanish off the screen of the person who asked whether or not the
        // Dogma agreed — and the one case where it refuses, the last Cage, is
        // exactly the case they most need to see did not happen.
        Command::DeleteCage { cage } => {
            if client.apagar_cage(cage).await.is_err() {
                return false;
            }
        }
        Command::DeleteLine { line } => {
            if client.apagar_linha(line).await.is_err() {
                return false;
            }
        }
        // The question, and where to put the answer. Registered **before** the
        // ask goes out: the Dogma is on the other side of a socket that can be
        // faster than this thread's next line, and a reply arriving before its
        // slot exists is a reply with nowhere to go.
        Command::WeighLine { line, answer } => {
            if let Ok(mut pending) = shared.pending_weights.lock() {
                pending.push((line, answer));
            }
            if client.pesar_linha(line).await.is_err() {
                return false;
            }
        }
        // The claim is read out of this client's own history here, before
        // anything is asked of the Dogma, and it never travels through the
        // window: a page that handed it back could hand back a different one,
        // and the whole of ADR 0027's rule is that the claim does not get to
        // choose the decoder.
        Command::PreviewAttachment { attachment, answer } => {
            let claimed = declared_type_of(shared, attachment).unwrap_or_default();
            let Ok(caixa) = client.prever_anexo(attachment).await else {
                return false;
            };
            // In a task of its own, and not awaited here. This queue is what
            // carries every keystroke of this session, and a download parked on
            // it would stop anybody saying anything until the picture came
            // down — the head-of-line block that ADR 0027 gave each transfer
            // its own stream to avoid.
            tokio::spawn(async move {
                let _ = answer.send(preview_of(attachment.get(), &claimed, caixa.await.ok()));
            });
        }
        Command::Shutdown => return false,
    }
    true
}

/// The type the sender claimed for one attachment, out of the local history.
fn declared_type_of(shared: &Shared, attachment: seele_core::AttachmentId) -> Option<String> {
    let room = shared.room.lock().ok()?;
    room.messages.iter().find_map(|message| {
        message
            .attachment
            .as_ref()
            .filter(|anexo| anexo.id == attachment)
            .map(|anexo| anexo.declared_type.clone())
    })
}

/// Turns fetched bytes into the verdict a window acts on.
///
/// Every branch that produces a picture goes through
/// [`seele_core::preview::judge`], and the media type in the URI comes out of
/// what that function found in the bytes. `claimed` is only ever quoted back
/// into a sentence.
fn preview_of(
    attachment: u64,
    claimed: &str,
    fetched: Option<seele_core::enlace::Previa>,
) -> Preview {
    use seele_core::enlace::Previa;
    use seele_core::preview::{data_uri, judge, Verdict};

    let refused = |refusal: PreviewRefusal, found: Option<String>| Preview {
        attachment,
        image: None,
        claimed: claimed.to_owned(),
        found,
        refusal: Some(refusal),
    };

    match fetched {
        None | Some(Previa::NaoVeio) => refused(PreviewRefusal::DidNotArrive, None),
        Some(Previa::GrandeDemais { .. }) => refused(
            PreviewRefusal::TooBig {
                limit: seele_core::PREVIEW_LIMIT,
            },
            None,
        ),
        Some(Previa::Bytes(bytes)) => match judge(claimed, &bytes) {
            Verdict::Draw(format) => Preview {
                attachment,
                image: Some(data_uri(format, &bytes)),
                claimed: claimed.to_owned(),
                found: Some(format.media_type().to_owned()),
                refusal: None,
            },
            Verdict::Disagrees { found, .. } => refused(
                PreviewRefusal::Disagrees,
                found.map(|format| format.media_type().to_owned()),
            ),
            Verdict::NotAPicture => refused(PreviewRefusal::NotAPicture, None),
        },
    }
}

/// A monotonic identifier for outgoing messages.
///
/// Process-wide rather than per-`Plug`: two handles in one process sending the
/// same number would collide in the server's idempotency check, and the second
/// message would be silently dropped as a resend of the first.
fn next_client_message_id() -> u64 {
    // Drawn at process start, not counted from one — and the difference is a
    // defect, not tidiness.
    //
    // `Messages::append_batch` deduplicates on `(author_id, client_message_id)`.
    // `author_id` comes from the Ed25519 key on disk (ADR 0004) and never
    // changes; this counter used to restart at 1 on every launch. So the first
    // message after reopening the app was read as a retry of the first message
    // of the previous launch and **was never written**, with neither end told
    // (pendency 19). Process-wide was never the scope that mattered: the reuse
    // happened *between* processes.
    //
    // The high half is random and the low half counts, which leaves four
    // billion messages before the two halves could meet.
    static NEXT: std::sync::LazyLock<std::sync::atomic::AtomicU64> =
        std::sync::LazyLock::new(|| {
            std::sync::atomic::AtomicU64::new((u64::from(rand::random::<u32>()) << 32) | 1)
        });
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Relabels a core connection failure for the shell.
///
/// A relabel and nothing more: the core already decided *what* went wrong, and
/// it decided it from typed causes rather than from the text of somebody else's
/// error message.
fn classify_connect_failure(error: &seele_core::ConnectError) -> PlugError {
    match error {
        seele_core::ConnectError::LocalEndpoint | seele_core::ConnectError::Unreachable => {
            PlugError::Unreachable
        }
        seele_core::ConnectError::TlsRefused | seele_core::ConnectError::ProtocolViolation => {
            PlugError::Refused {
                reason: EndReason::Incompatible,
            }
        }
        seele_core::ConnectError::PinChanged { pinned, offered } => PlugError::PinChanged {
            pinned: pinned.clone(),
            offered: offered.clone(),
        },
        seele_core::ConnectError::InviteMismatch { expected, offered } => {
            PlugError::InviteMismatch {
                expected: expected.clone(),
                offered: offered.clone(),
            }
        }
        seele_core::ConnectError::HandshakeTimeout => PlugError::HandshakeTimeout,
        seele_core::ConnectError::Refused { reason } => PlugError::Refused {
            reason: (*reason).into(),
        },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_chave_de_mensagem_nao_comeca_num_valor_fixo() {
        // Pendência 19, a metade do app. Aqui o contador é do processo, e era
        // *entre* processos que a repetição acontecia: reabrir o SEELE fazia a
        // primeira mensagem ser lida como reenvio da primeira mensagem da
        // abertura anterior, e ela não era gravada.
        //
        // Só dá para observar o sorteio uma vez por processo, então o que se
        // afirma é a propriedade que sobrevive a isso: o valor não é pequeno, e
        // o contador anda.
        let primeira = super::next_client_message_id();
        let segunda = super::next_client_message_id();

        assert!(
            primeira > u64::from(u32::MAX),
            "a metade alta não foi sorteada: {primeira}"
        );
        assert_eq!(segunda, primeira + 1, "o contador não anda de um em um");
    }

    use super::*;

    #[test]
    fn a_bare_host_gets_the_default_port() {
        let (address, name, _) = resolve("localhost").expect("resolve");
        assert_eq!(address.port(), seele_core::DEFAULT_PORT);
        assert_eq!(name, "localhost");
    }

    #[test]
    fn an_address_is_pinned_under_a_name_a_certificate_can_carry() {
        // ADR 0003 pins per name. An IP is not a name a certificate is issued
        // for, so pinning under one would file the pin somewhere nothing ever
        // looks for it again.
        let (_, name, pin) = resolve("127.0.0.1:8383").expect("resolve");
        assert_eq!(
            name, "localhost",
            "TLS gets the name the certificate carries"
        );
        assert_eq!(pin, "127.0.0.1:8383", "the pin is filed under the address");
    }

    #[test]
    fn every_expected_fingerprint_reaches_the_destino_it_promised() {
        // `drive` builds this immediately before a real QUIC connection, so
        // this is the only way to check the wiring without a live server —
        // and the only thing standing between an invite's `fp=` and the
        // refusal ADR 0006 exists to produce.
        let (address, name, pin) = resolve("localhost:8383").expect("resolve");

        let with_fingerprint = ConnectConfig {
            server: "localhost:8383".into(),
            alternate_servers: Vec::new(),
            nickname: "shinji".into(),
            home: "/tmp/does-not-matter".into(),
            join_secret: None,
            expected_fingerprint: Some("aaaa1111".into()),
            bilhete: None,
            audio: false,
            capture_device: None,
            playback_device: None,
        };
        let destino = build_destino(&with_fingerprint, address, &name, &pin);
        assert_eq!(destino.impressao_esperada, Some("aaaa1111".into()));

        let without = ConnectConfig {
            expected_fingerprint: None,
            bilhete: None,
            ..with_fingerprint
        };
        let destino = build_destino(&without, address, &name, &pin);
        assert_eq!(destino.impressao_esperada, None);
    }

    #[test]
    fn a_host_that_does_not_exist_is_an_enum_and_not_a_panic() {
        assert_eq!(
            resolve("nao-existe.invalid:8383"),
            Err(PlugError::UnresolvableHost)
        );
    }

    #[test]
    fn the_pattern_survives_the_round_trip_through_an_atomic() {
        for pattern in [Pattern::Offline, Pattern::Orange, Pattern::Blue] {
            assert_eq!(pattern_from_byte(pattern_byte(pattern)), pattern);
        }
    }

    #[test]
    fn an_unknown_pattern_byte_reads_as_offline() {
        // Whatever goes wrong, it must not claim a verified session.
        assert_eq!(pattern_from_byte(200), Pattern::Offline);
    }

    #[test]
    fn message_identifiers_never_repeat_within_a_process() {
        // Two handles in one process sending the same number would collide in
        // the server's idempotency check, and the second message would vanish
        // as a resend of the first.
        let ids: std::collections::HashSet<u64> =
            (0..1000).map(|_| next_client_message_id()).collect();
        assert_eq!(ids.len(), 1000);
    }

    #[test]
    fn a_snapshot_of_an_empty_room_is_empty_and_not_a_panic() {
        let room = Room::new();
        assert!(cages_of(&room).is_empty());
        assert!(lines_of(&room).is_empty());
        assert!(messages_of(&room).is_empty());
    }

    #[test]
    fn the_snapshot_marks_which_pilot_is_us() {
        // Without this the shell has to compare ids, which means the shell has
        // to know what a `PilotId` is.
        use seele_core::{CageInfo, PilotId, PilotProfile, ServerMessage, SessionId, Ssrc};

        let mut room = Room::new();
        room.apply(&ServerMessage::Session {
            id: SessionId(1),
            pilot: PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: vec![CageInfo {
                id: CageId(1),
                name: "CAGE-01".into(),
                limit: 20,
                password_required: false,
                line: None,
            }],
            lines: Vec::new(),
            roles: Vec::new(),
            permissions: Vec::new(),
        });
        room.enter_cage(CageId(1));
        room.apply(&ServerMessage::PilotJoined {
            cage: CageId(1),
            profile: PilotProfile {
                id: PilotId(3),
                nickname: "ayanami".into(),
                roles: Vec::new(),
            },
            ssrc: Ssrc(30),
        });

        let cages = cages_of(&room);
        let pilots = &cages[0].pilots;
        assert!(pilots.iter().any(|pilot| pilot.is_self));
        assert_eq!(
            pilots.iter().filter(|pilot| pilot.is_self).count(),
            1,
            "more than one pilot claims to be us"
        );
        assert!(cages[0].occupied_by_us);
    }

    #[test]
    fn a_sync_band_travels_beside_its_number() {
        // specs/05-cliente-tui.md forbids carrying it by colour alone, and the
        // same applies here: a shell that only got a hue could not print a
        // number, and one that only got a number would have to know the
        // thresholds.
        use seele_core::{
            CageInfo, PilotId, PilotProfile, PilotState, Presence, ServerMessage, Ssrc,
        };

        let mut room = Room::new();
        room.cages = vec![CageInfo {
            id: CageId(1),
            name: "CAGE-01".into(),
            limit: 20,
            password_required: false,
            line: None,
        }];
        room.apply(&ServerMessage::PilotJoined {
            cage: CageId(1),
            profile: PilotProfile {
                id: PilotId(3),
                nickname: "ayanami".into(),
                roles: Vec::new(),
            },
            ssrc: Ssrc(30),
        });
        room.apply(&ServerMessage::PilotState(PilotState {
            pilot: PilotId(3),
            at_field: false,
            total_isolation: false,
            speaking: false,
            presence: Presence::Available,
            sync_ratio: 72,
        }));

        // 72 rather than a critical number on purpose: `SyncBand::Critical` is
        // the `Default`, so a shell that received it could not tell a banded
        // ratio from a field nobody filled in.
        let pilot = &cages_of(&room)[0].pilots[0];
        assert_eq!(pilot.sync_ratio, 72);
        assert_eq!(pilot.sync_band, types::SyncBand::Degraded);
    }

    #[test]
    fn the_cages_average_crosses_already_banded() {
        // MÉDIA DO CAGE. The comp computes it in the shell; here the shell gets
        // the number, the band and the sample size and has nothing left to
        // decide. A Cage nobody is in carries `None`, not a critical zero.
        use seele_core::{
            CageInfo, PilotId, PilotProfile, PilotState, Presence, ServerMessage, Ssrc,
        };

        let mut room = Room::new();
        room.cages = vec![CageInfo {
            id: CageId(1),
            name: "CAGE-01".into(),
            limit: 20,
            password_required: false,
            line: None,
        }];
        assert_eq!(cages_of(&room)[0].sync, None, "an empty Cage");

        for (id, sync) in [(3_u64, 84_u8), (4, 85)] {
            room.apply(&ServerMessage::PilotJoined {
                cage: CageId(1),
                profile: PilotProfile {
                    id: PilotId(id),
                    nickname: format!("piloto {id}"),
                    roles: Vec::new(),
                },
                ssrc: Ssrc(u32::try_from(id * 10).expect("ssrc")),
            });
            room.apply(&ServerMessage::PilotState(PilotState {
                pilot: PilotId(id),
                at_field: false,
                total_isolation: false,
                speaking: false,
                presence: Presence::Available,
                sync_ratio: sync,
            }));
        }

        let average = cages_of(&room)[0].sync.expect("two pilots are seated");
        assert_eq!(average.ratio, 85);
        assert_eq!(average.band, types::SyncBand::Nominal);
        assert_eq!(average.pilots, 2);
    }

    #[test]
    fn a_listener_hears_what_the_room_folded() {
        use seele_core::{AlertReason, AlertSeverity, ServerMessage};

        #[derive(Default)]
        struct Recorder(Mutex<Vec<Event>>);
        impl EventListener for Recorder {
            fn on_event(&self, event: Event) {
                if let Ok(mut seen) = self.0.lock() {
                    seen.push(event);
                }
            }
        }

        let shared = bare_shared();
        let recorder = Arc::new(Recorder::default());
        shared
            .listeners
            .lock()
            .unwrap()
            .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

        fold(
            &shared,
            &ServerMessage::Alert {
                severity: AlertSeverity::Critical,
                reason: AlertReason::PermissionDenied,
                operator_text: None,
            },
        );

        let seen = recorder.0.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let Event::NoticeRaised { notice } = &seen[0] else {
            panic!("expected a notice, got {:?}", seen[0]);
        };
        assert_eq!(notice.severity, Severity::Critical);
        assert_eq!(notice.reason, NoticeReason::PermissionDenied);
    }

    /// A `Shared` with nothing in it, for the folds below.
    fn bare_shared() -> Arc<Shared> {
        Arc::new(Shared {
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            media: Mutex::new(None),
            nickname: Mutex::new("ayanami".into()),
            pattern: AtomicU8::new(0),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
            sync_ratio: AtomicU8::new(0),
            running: AtomicBool::new(true),
            pending_weights: Mutex::new(Vec::new()),
        })
    }

    #[test]
    fn o_jitter_da_tela_nao_e_a_profundidade_do_anel() {
        // Duas grandezas com o mesmo nome, e mostrar a errada é pior que não
        // mostrar nada:
        //
        // - `worst_jitter_depth_ms` é **profundidade do anel de reprodução**, e o
        //   ADR 0028 acabou de dar um alvo a ele. Mostrá-lo como "jitter" exibiria a
        //   nossa própria reserva como ruído da rede — e uma reserva saudável
        //   apareceria na tela como problema;
        // - `SourceTelemetry::jitter_ms` é o de **chegada** (RFC 3550), medido aqui,
        //   e é o que a pessoa quer saber.
        //
        // E o que a tela lia antes deste conserto era um terceiro valor: o do
        // relatório do Dogma, que é sempre `0.0` porque o servidor não tem como
        // saber — `session.rs` diz isso em comentário desde sempre.
        // Uma reserva de anel saudável (42 ms) ao lado de um jitter de rede baixo
        // (7,5 ms): mostrar a primeira faria uma conexão boa parecer ruim.
        assert!(
            (jitter_para_a_tela(7.5, 42.0) - 7.5).abs() < 0.01,
            "a tela mostra o jitter de chegada, não a profundidade do anel"
        );
        // E nunca o zero que o Dogma manda de propósito, que era o que a tela lia.
        assert!(jitter_para_a_tela(7.5, 42.0) > 0.0);
    }

    #[test]
    fn o_jitter_que_o_laco_de_voz_grava_e_o_que_a_casca_le() {
        // A travessia inteira do número, sem o laço de voz: se o campo do
        // `Shared` perdesse a escrita ou a leitura, a tela voltaria a mostrar um
        // zero — que é exatamente o defeito de antes, com outra causa.
        let compartilhado = bare_shared();
        assert_eq!(
            compartilhado.jitter_de_chegada_ms(),
            0.0,
            "sem medida nenhuma ainda não há jitter para mostrar"
        );

        compartilhado.gravar_jitter_de_chegada(jitter_para_a_tela(7.5, 42.0));

        assert!(
            (compartilhado.jitter_de_chegada_ms() - 7.5).abs() < 0.01,
            "a casca leu {}, e o laço de voz gravou 7,5",
            compartilhado.jitter_de_chegada_ms()
        );
    }

    #[test]
    fn a_profundidade_do_anel_vai_para_o_sync_ratio_e_a_chegada_para_a_tela() {
        // A outra metade da fiação. `measure` em si precisa de um `Enlace` vivo
        // e de um dispositivo de áudio aberto, então o que se afirma é a
        // separação que ele faz, apartada nele para isto: qual das duas
        // grandezas sai por qual porta.
        //
        // Os números são escolhidos para não poderem ser confundidos, e são os
        // mesmos do teste da função pura: 42 ms de reserva de anel — saudável,
        // que é o alvo que o ADR 0028 deu a ele — ao lado de 7,5 ms de jitter de
        // rede. Trocar os dois de lugar em `measure` mostraria 42 na tela e uma
        // conexão boa pareceria ruim.
        let telemetria = seele_core::AudioTelemetry {
            local: seele_core::LocalTelemetry::default(),
            sources: vec![seele_core::SourceTelemetry {
                ssrc: 700,
                jitter_depth_ms: 42.0,
                jitter_ms: 7.5,
                ..Default::default()
            }],
        };

        let volta = jitter_da_volta(&telemetria);

        assert!(
            (volta.profundidade_do_anel_ms - 42.0).abs() < 0.01,
            "o Sync Ratio recebeu {}, e a reserva do anel era 42 ms",
            volta.profundidade_do_anel_ms
        );
        let Some(chegada) = volta.chegada_ms else {
            panic!("havia uma fonte sendo recebida e nenhum jitter de chegada saiu");
        };
        assert!(
            (chegada - 7.5).abs() < 0.01,
            "a tela recebeu {chegada}, e o jitter de chegada era 7,5 ms"
        );
    }

    #[test]
    fn sem_fonte_nenhuma_a_tela_nao_recebe_um_zero_inventado() {
        // O pior de uma lista vazia seria zero, e zero é justamente o valor que
        // esta tarefa existe para tirar da tela. Sem ninguém falando não há
        // jitter de chegada nenhum a mostrar, e o último medido continua valendo
        // até haver outro.
        let telemetria = seele_core::AudioTelemetry {
            local: seele_core::LocalTelemetry::default(),
            sources: Vec::new(),
        };

        assert!(
            jitter_da_volta(&telemetria).chegada_ms.is_none(),
            "sem fonte nenhuma saiu um jitter de chegada, e ele só pode ser zero"
        );
    }

    #[test]
    fn o_snapshot_le_o_jitter_que_o_laco_de_voz_gravou_e_nao_o_do_dogma() {
        // As duas linhas que consertam o defeito são de fiação, e fiação não se
        // afirma testando a função pura dos dois lados dela: `jitter_para_a_tela`
        // podia estar perfeita enquanto o `snapshot` continuava lendo o relatório
        // do Dogma, e a suíte continuaria verde. Isto aqui é o teste que fica
        // vermelho quando alguém desfaz o conserto.
        //
        // Os dois números são plantados de propósito para não poderem ser
        // confundidos: 12,75 ms é o de chegada, medido neste receptor, e é o que
        // a tela tem de mostrar; 99,0 ms é o que o relatório do Dogma diz, e é o
        // caminho errado. Na vida real esse campo é `0.0` fixo — o servidor não
        // tem como medir jitter, que só existe no receptor —, mas plantar um 99
        // aqui faz o teste distinguir "leu o certo" de "leu um zero que calhou de
        // bater", que um `0.0` de verdade não distinguiria.
        let compartilhado = bare_shared();
        if let Ok(mut room) = compartilhado.room.lock() {
            room.telemetry = Some(seele_core::Telemetry {
                rtt_ms: 20.0,
                jitter_ms: 99.0,
                loss_fraction: 0.0,
                subsystems: Vec::new(),
            });
        }
        compartilhado.gravar_jitter_de_chegada(jitter_para_a_tela(12.75, 42.0));

        let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
        let plug = Plug {
            commands,
            shared: Arc::clone(&compartilhado),
        };
        let mostrado = plug.snapshot().telemetry.jitter_ms;

        assert!(
            (mostrado - 12.75).abs() < 0.01,
            "a tela mostrou {mostrado}, e o laço de voz mediu 12,75 ms de chegada"
        );
        assert!(
            (mostrado - 99.0).abs() > 0.01,
            "a tela voltou a ler o jitter do relatório do Dogma, que é o defeito 3.3"
        );
    }

    #[test]
    fn um_jitter_de_fracao_de_milissegundo_nao_e_arredondado_para_zero() {
        // O campo guarda microssegundos inteiros justamente para isto: num
        // enlace local o jitter fica abaixo de um milissegundo, e guardá-lo em
        // milissegundos inteiros faria a tela dizer zero num caminho excelente —
        // indistinguível do zero que o Dogma manda.
        let compartilhado = bare_shared();
        compartilhado.gravar_jitter_de_chegada(0.25);

        assert!(
            (compartilhado.jitter_de_chegada_ms() - 0.25).abs() < 0.001,
            "0,25 ms virou {}",
            compartilhado.jitter_de_chegada_ms()
        );
    }

    #[test]
    fn um_jitter_impossivel_nao_atravessa_a_ponte() {
        // `NaN` e negativo não têm como sair da RFC 3550, mas atravessariam a
        // ponte para uma casca que os desenha — e um `NaN` numa tela vira texto
        // que ninguém sabe ler. Zero é a resposta honesta para "não há medida".
        //
        // Quem garante isto é o `as u64` de `gravar_jitter_de_chegada`, que em
        // Rust satura, e não uma guarda escrita à mão — havia um `max(0.0)` ali
        // e ele foi tirado justamente porque nunca disparava. Este teste então
        // não vigia código nosso: ele **fixa a linguagem**, e reprovaria se
        // alguém trocasse aquele cast por uma conversão que embrulha ou por um
        // `unsafe` de arredondamento sem verificação.
        let compartilhado = bare_shared();

        compartilhado.gravar_jitter_de_chegada(f32::NAN);
        assert_eq!(compartilhado.jitter_de_chegada_ms(), 0.0);

        compartilhado.gravar_jitter_de_chegada(-3.0);
        assert_eq!(compartilhado.jitter_de_chegada_ms(), 0.0);
    }

    #[test]
    fn a_room_made_now_reaches_the_shell_as_a_channel_change() {
        // The bridge invents nothing: the Dogma says a Cage exists, the room
        // folds it in, and the shell is told to redraw the list it already
        // knows how to draw. If this stopped firing, the person who made the
        // room would be looking at the old list with no way to know.
        use seele_core::{CageInfo, ServerMessage};

        #[derive(Default)]
        struct Recorder(Mutex<Vec<Event>>);
        impl EventListener for Recorder {
            fn on_event(&self, event: Event) {
                if let Ok(mut seen) = self.0.lock() {
                    seen.push(event);
                }
            }
        }

        let shared = bare_shared();
        let recorder = Arc::new(Recorder::default());
        shared
            .listeners
            .lock()
            .unwrap()
            .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

        fold(
            &shared,
            &ServerMessage::CageCreated {
                cage: CageInfo {
                    id: CageId(2),
                    name: "CAGE-02 SALA DOS FUNDOS".into(),
                    limit: 8,
                    password_required: false,
                    line: None,
                },
            },
        );

        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec![Event::ChannelsChanged],
            "the shell was not told the channel list moved"
        );
        let room = shared.room.lock().unwrap();
        assert_eq!(cages_of(&room).len(), 1);
        assert_eq!(cages_of(&room)[0].name, "CAGE-02 SALA DOS FUNDOS");
    }

    #[test]
    fn the_snapshot_says_whether_this_pilot_may_make_rooms() {
        // What a screen asks before drawing the control. Convenience, never
        // enforcement — but a shell with no way to ask has only two options,
        // and both are bad: hide the feature from the host, or offer everybody
        // a button that mostly answers "no".
        use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

        let shared = bare_shared();
        fold(
            &shared,
            &ServerMessage::Session {
                id: SessionId(1),
                pilot: seele_core::PilotId(7),
                ssrc: Ssrc(700),
                dogma: "Terceira Tóquio".into(),
                cages: Vec::new(),
                lines: Vec::new(),
                roles: Vec::new(),
                permissions: vec![Permission::Speak],
            },
        );
        assert!(
            !shared
                .room
                .lock()
                .unwrap()
                .permissions
                .contains(&Permission::ManageCages),
            "a pilot who may only speak was told they may manage Cages"
        );

        fold(
            &shared,
            &ServerMessage::Session {
                id: SessionId(1),
                pilot: seele_core::PilotId(7),
                ssrc: Ssrc(700),
                dogma: "Terceira Tóquio".into(),
                cages: Vec::new(),
                lines: Vec::new(),
                roles: Vec::new(),
                permissions: vec![Permission::Speak, Permission::ManageCages],
            },
        );
        // And the field the screen actually reads, not only the list behind it.
        // Asserting on `room.permissions` alone would pass with
        // `may_manage_cages` hardcoded either way — measured, and it did.
        let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
        let plug = Plug {
            commands,
            shared: Arc::clone(&shared),
        };
        assert!(plug.snapshot().may_manage_cages);

        fold(
            &shared,
            &ServerMessage::Session {
                id: SessionId(1),
                pilot: seele_core::PilotId(7),
                ssrc: Ssrc(700),
                dogma: "Terceira Tóquio".into(),
                cages: Vec::new(),
                lines: Vec::new(),
                roles: Vec::new(),
                permissions: vec![Permission::Speak],
            },
        );
        assert!(
            !plug.snapshot().may_manage_cages,
            "the snapshot went on offering the control after the permission went away"
        );
    }

    #[test]
    fn the_snapshot_answers_each_moderation_verb_on_its_own() {
        // Four permissions, four questions, four booleans — and not one
        // `may_moderate`. `specs/04-servidor-seele.md` enumerates them
        // separately and a role may carry any subset, so collapsing them here
        // would draw the ban control for somebody who may only kick, and put a
        // shell in the position of teaching people that half its buttons say no.
        use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

        let shared = bare_shared();
        let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
        let plug = Plug {
            commands,
            shared: Arc::clone(&shared),
        };

        let sessao = |permissions: Vec<Permission>| ServerMessage::Session {
            id: SessionId(1),
            pilot: seele_core::PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: Vec::new(),
            lines: Vec::new(),
            roles: Vec::new(),
            permissions,
        };

        fold(&shared, &sessao(vec![Permission::Speak]));
        let nada = plug.snapshot();
        assert!(!nada.may_kick);
        assert!(!nada.may_ban);
        assert!(!nada.may_remove_message);
        assert!(!nada.may_move_pilot);

        // An Operador holding exactly one of the four. The assertion that
        // matters is the three `false`s beside the one `true`.
        fold(&shared, &sessao(vec![Permission::Speak, Permission::Kick]));
        let so_expulsa = plug.snapshot();
        assert!(so_expulsa.may_kick);
        assert!(
            !so_expulsa.may_ban && !so_expulsa.may_remove_message && !so_expulsa.may_move_pilot,
            "one moderation permission lit up the other three"
        );

        fold(
            &shared,
            &sessao(vec![
                Permission::Kick,
                Permission::Ban,
                Permission::RemoveMessage,
                Permission::MovePilot,
            ]),
        );
        let tudo = plug.snapshot();
        assert!(tudo.may_kick && tudo.may_ban && tudo.may_remove_message && tudo.may_move_pilot);

        // And they go away again. A snapshot that latched would go on offering
        // a control after a Comandante revoked it.
        fold(&shared, &sessao(Vec::new()));
        let depois = plug.snapshot();
        assert!(
            !depois.may_kick && !depois.may_ban && !depois.may_remove_message,
            "the snapshot went on offering the controls after the permissions went away"
        );
    }

    #[test]
    fn destroying_a_room_is_a_permission_of_its_own_and_not_the_one_that_makes_them() {
        // The decision this whole path turns on, asserted where a shell reads
        // it. Making and renaming a room are mistakes a Dogma survives;
        // destroying one ends what other people wrote. A role that may build
        // rooms without being able to unmake them is a role somebody can
        // actually write — `specs/04-servidor-seele.md` enumerates
        // `gerenciar_cages` and `administrar_dogma` separately — and a single
        // boolean for both would make it impossible to offer correctly.
        use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

        let shared = bare_shared();
        let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
        let plug = Plug {
            commands,
            shared: Arc::clone(&shared),
        };

        let sessao = |permissions: Vec<Permission>| ServerMessage::Session {
            id: SessionId(1),
            pilot: seele_core::PilotId(7),
            ssrc: Ssrc(700),
            dogma: "Terceira Tóquio".into(),
            cages: Vec::new(),
            lines: Vec::new(),
            roles: Vec::new(),
            permissions,
        };

        // The role that builds and does not destroy. This is the pair the
        // separation exists for, and the one a single boolean would get wrong.
        fold(&shared, &sessao(vec![Permission::ManageCages]));
        let constroi = plug.snapshot();
        assert!(constroi.may_manage_cages);
        assert!(
            !constroi.may_delete_rooms,
            "the permission to make a room was read as the permission to destroy one"
        );

        // And the reverse, so the two are not simply the same field read twice.
        fold(&shared, &sessao(vec![Permission::AdministerDogma]));
        let administra = plug.snapshot();
        assert!(administra.may_delete_rooms);
        assert!(!administra.may_manage_cages);

        // Never the moderation permissions either: somebody trusted to remove a
        // person for the evening is not thereby trusted with the Dogma's past.
        fold(
            &shared,
            &sessao(vec![
                Permission::Kick,
                Permission::Ban,
                Permission::RemoveMessage,
                Permission::MovePilot,
            ]),
        );
        assert!(
            !plug.snapshot().may_delete_rooms,
            "a moderation permission lit up the one that destroys rooms"
        );

        // And it goes away again, like the five beside it.
        fold(&shared, &sessao(Vec::new()));
        assert!(!plug.snapshot().may_delete_rooms);
    }

    #[test]
    fn the_weight_of_a_line_reaches_the_caller_who_asked_for_it() {
        // The number in the confirmation, and the one call on this bridge that
        // waits for an answer. A shell that cannot get these three numbers must
        // not open the box at all, so the wiring that carries them is worth
        // pinning: the question registers a slot, the Dogma's reply fills it,
        // and the caller wakes with the counts unrounded.
        let shared = bare_shared();
        let (commands, mut queue) = tokio::sync::mpsc::unbounded_channel();
        let plug = Plug {
            commands,
            shared: Arc::clone(&shared),
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let pergunta = tokio::spawn({
                let shared = Arc::clone(&shared);
                async move {
                    // The driver's half, by hand: take the command off the
                    // queue, park the sender, then let the answer arrive.
                    let Some(Command::WeighLine { line, answer }) = queue.recv().await else {
                        panic!("nothing asked the Dogma to weigh anything");
                    };
                    shared
                        .pending_weights
                        .lock()
                        .expect("pending")
                        .push((line, answer));
                    shared.answer_weight(LineWeight {
                        line: 7,
                        messages: 1_847,
                        authors: 6,
                        oldest_at_seconds: Some(1_678_600_000),
                    });
                }
            });

            let peso = plug.weigh_line(7).await.expect("weight");
            pergunta.await.expect("driver");

            assert_eq!(peso.messages, 1_847);
            assert_eq!(peso.authors, 6);
            assert_eq!(peso.oldest_at_seconds, Some(1_678_600_000));
        });

        // And nothing was kept: the weight's whole value is being fresh, so
        // asking for it left the room exactly as it was.
        assert!(shared.room.lock().expect("room").lines.is_empty());
        assert!(shared.pending_weights.lock().expect("pending").is_empty());
    }

    #[test]
    fn a_weight_for_another_line_leaves_this_caller_waiting() {
        // Two boxes cannot be open at once in one window, but a reply landing
        // against the wrong slot would be the kind of mistake that fills a
        // confirmation with somebody else's numbers — which is the exact
        // failure the counted number exists to rule out.
        let shared = bare_shared();
        let (answer, mut resposta) = tokio::sync::oneshot::channel();
        shared
            .pending_weights
            .lock()
            .expect("pending")
            .push((seele_core::LineId(7), answer));

        shared.answer_weight(LineWeight {
            line: 9,
            messages: 3,
            authors: 1,
            oldest_at_seconds: None,
        });

        assert!(
            resposta.try_recv().is_err(),
            "a count for another Line was handed to the box asking about this one"
        );
        assert_eq!(
            shared.pending_weights.lock().expect("pending").len(),
            1,
            "the question was dropped without being answered"
        );
    }

    #[test]
    fn asking_for_a_room_with_no_name_asks_for_nothing() {
        // The same swallow `send_message` does for an empty body. Somebody who
        // pressed the button with nothing typed has not asked for anything, and
        // answering that with a red message puts an error on a screen where
        // nothing went wrong. The check is here as well as in `seele-proto`
        // because the proto one would arrive as a dropped connection rather
        // than as nothing happening.
        let (commands, mut queue) = tokio::sync::mpsc::unbounded_channel();
        let plug = Plug {
            commands,
            shared: bare_shared(),
        };

        for blank in ["", "   ", "\t\n"] {
            plug.create_cage(blank.into(), 8, None).unwrap();
            plug.create_line(blank.into()).unwrap();
            plug.rename_cage(1, blank.into()).unwrap();
            plug.rename_line(1, blank.into()).unwrap();
        }
        assert!(
            queue.try_recv().is_err(),
            "a blank name was sent to the Dogma"
        );

        plug.create_cage("CAGE-02".into(), 8, None).unwrap();
        assert!(
            matches!(queue.try_recv(), Ok(Command::CreateCage { .. })),
            "a real name was swallowed too"
        );
    }

    #[test]
    fn a_capture_device_crosses_with_both_halves_of_its_identity() {
        // A shell shows the name and sends back the id, and the two are not
        // interchangeable: two microphones of the same model report the same
        // name, so a shell that stored the name would leave the second one
        // unpickable — and one that *showed* the id would put a backend string
        // on screen where a person expects "Scarlett Solo".
        //
        // Skips out loud rather than passing on a machine with no microphone. A
        // test that succeeds whether or not the feature works is not a test, and
        // CI has no sound card.
        let found = capture_devices();
        if found.is_empty() {
            eprintln!("skipped: this machine lists no capture device");
            return;
        }

        for device in &found {
            assert!(
                !device.id.is_empty(),
                "a device with no id cannot be chosen"
            );
            assert!(
                !device.name.is_empty(),
                "a device with no name cannot be labelled"
            );
        }
        assert!(
            found.iter().filter(|device| device.default).count() <= 1,
            "two devices both claim to be the machine's default"
        );
    }

    #[test]
    fn a_playback_device_crosses_with_both_halves_of_its_identity() {
        // The twin of the check above, for the same reason and with the same
        // skip: CI has no sound card, and a test that passes on a machine with
        // no speakers passes on every machine.
        let found = playback_devices();
        if found.is_empty() {
            eprintln!("skipped: this machine lists no playback device");
            return;
        }

        for device in &found {
            assert!(
                !device.id.is_empty(),
                "a device with no id cannot be chosen"
            );
            assert!(
                !device.name.is_empty(),
                "a device with no name cannot be labelled"
            );
        }
        assert!(
            found.iter().filter(|device| device.default).count() <= 1,
            "two outputs both claim to be the machine's default"
        );
    }

    #[test]
    fn the_two_device_lists_are_not_the_same_list() {
        // Both are `Vec<_>` of three strings, and the two functions differ by one
        // word. A `playback_devices` that forwarded to the capture side would
        // compile, serialise, and draw a list of microphones under SAÍDA DE SOM
        // — and every pick from it would be refused by a machine that has no
        // such output.
        let (microphones, speakers) = (capture_devices(), playback_devices());
        if microphones.is_empty() || speakers.is_empty() {
            eprintln!(
                "skipped: this machine lists {} microphones and {} outputs, and telling the \
                 two lists apart needs one of each",
                microphones.len(),
                speakers.len()
            );
            return;
        }

        let ids: Vec<&str> = microphones
            .iter()
            .map(|device| device.id.as_str())
            .collect();
        assert!(
            speakers
                .iter()
                .any(|device| !ids.contains(&device.id.as_str())),
            "every listed output is also a listed microphone, so one list is the other: \
             {ids:?}"
        );
    }

    #[test]
    fn a_capture_device_serialises_the_field_names_a_shell_reads() {
        // The desktop shell is untyped JavaScript reading these three names off
        // the wire. A `serde(rename)` added here would leave every row drawn
        // `undefined`, with nothing failing anywhere — the same defect class
        // `apps/seele-app/tests/frontend.rs` guards for `Match`.
        let device = CaptureDevice {
            id: "coreaudio:alguma-coisa".into(),
            name: "Scarlett Solo".into(),
            default: true,
        };
        let Ok(json) = serde_json::to_string(&device) else {
            panic!("CaptureDevice does not serialise, so no shell can read it at all");
        };
        for field in ["\"id\"", "\"name\"", "\"default\""] {
            assert!(
                json.contains(field),
                "a capture device no longer carries {field}: {json}"
            );
        }
    }

    #[test]
    fn a_playback_device_serialises_the_field_names_a_shell_reads() {
        // Its own test and not an addition to the one above, because the two
        // types are separate declarations: a `serde(rename)` on this one would
        // leave every row under SAÍDA DE SOM drawn `undefined` while the
        // microphone list next to it kept working, which is the version of this
        // failure that takes longest to believe.
        let device = PlaybackDevice {
            id: "coreaudio:alguma-saida".into(),
            name: "Fones de ouvido".into(),
            default: true,
        };
        let Ok(json) = serde_json::to_string(&device) else {
            panic!("PlaybackDevice does not serialise, so no shell can read it at all");
        };
        for field in ["\"id\"", "\"name\"", "\"default\""] {
            assert!(
                json.contains(field),
                "a playback device no longer carries {field}: {json}"
            );
        }
    }

    #[test]
    fn a_snapshot_with_no_audio_names_no_microphone() {
        // The two have to agree, always: a screen that read a device off a
        // session with no voice path would draw a microphone that is not open.
        // `AudioState::silent` is the one place that pairing is decided.
        let quiet = AudioState::silent();
        assert!(!quiet.available);
        assert_eq!(quiet.capture, None);
        assert_eq!(
            quiet.playback, None,
            "a session with no audio must not name an output it is not playing through"
        );
        assert_eq!(
            quiet.mode,
            VoiceMode::PushToTalk,
            "a session with no audio must not read as an open microphone"
        );
    }

    #[test]
    fn a_pong_wakes_nobody() {
        // Every event delivered is a redraw somewhere. The round-trip
        // measurement is consumed by the core and is not news.
        use seele_core::ServerMessage;

        struct Counter(std::sync::atomic::AtomicUsize);
        impl EventListener for Counter {
            fn on_event(&self, _: Event) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let shared = Arc::new(Shared {
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            media: Mutex::new(None),
            nickname: Mutex::new("ayanami".into()),
            pattern: AtomicU8::new(0),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
            sync_ratio: AtomicU8::new(0),
            running: AtomicBool::new(true),
            pending_weights: Mutex::new(Vec::new()),
        });
        let counter = Arc::new(Counter(std::sync::atomic::AtomicUsize::new(0)));
        shared
            .listeners
            .lock()
            .unwrap()
            .push(Arc::clone(&counter) as Arc<dyn EventListener>);

        fold(&shared, &ServerMessage::Pong { timestamp: 1 });

        assert_eq!(counter.0.load(Ordering::Relaxed), 0);
    }
}

#[cfg(test)]
mod aviso_de_mensagens {
    /// `Event::MessagesChanged` may only be raised by `Shared::messages_changed`.
    ///
    /// The two halves — bump the revision, then tell the shell — have to move
    /// together, and they did not: `Command::OpenLine` clears the room's
    /// messages and raised the event **without** the bump. A shell that
    /// refetches only when the number moves, which is the entire point of the
    /// number, swallowed it, and switching Line left the previous Line's
    /// conversation on screen under the new heading.
    ///
    /// Nothing about that failed. The event fired, the listener ran, the guard
    /// compared two equal numbers and correctly concluded there was nothing to
    /// do. Every part behaved; the pair was wrong.
    ///
    /// Reading the source is the only way to state "one door and no other" —
    /// a behavioural test would have to know every future caller in advance.
    #[test]
    fn nobody_raises_the_event_without_moving_the_revision() {
        let fonte = include_str!("lib.rs");
        let Some(depois) = fonte.split("fn messages_changed(&self)").nth(1) else {
            panic!(
                "`messages_changed` is gone, and with it the only place the two halves are tied"
            );
        };
        let Some(corpo) = depois.split("\n    }").next() else {
            panic!("`messages_changed` is never closed");
        };
        assert!(
            corpo.contains("fetch_add") && corpo.contains("Event::MessagesChanged"),
            "the one function that ties the revision to the notice no longer does \
             both:\n{corpo}"
        );

        // Everything else may only reach it through that function.
        //
        // Scanned up to the first `#[cfg(test)]` and no further, and comments
        // dropped. Both exclusions are load-bearing rather than tidy: this very
        // test names the event three times in its own assertions, and the
        // paragraphs above name it because that is what they are about. A guard
        // its own text can trip is as broken as one its own text can satisfy.
        let producao = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
        let sem_comentarios: String = producao
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let mencoes = sem_comentarios.matches("Event::MessagesChanged").count();
        assert_eq!(
            mencoes, 1,
            "`Event::MessagesChanged` is raised somewhere other than \
             `messages_changed`, which is how the revision and the notice came \
             apart the first time"
        );
    }
}

#[cfg(test)]
mod previa {
    //! What a window is handed for one attachment. ADR 0027.
    //!
    //! This is the last place the bytes and the claim are both in scope, so it
    //! is the last place the wrong one could be picked. The window past this
    //! point gets a string and no choice.

    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];

    #[test]
    fn bytes_that_agree_with_the_claim_come_back_as_a_picture() {
        // The claim is **shouted**, and the URI comes out lower case. A media
        // type is case-insensitive, so this file agrees with itself and is
        // drawn — and the spelling of the string a window receives is proof of
        // which side of the agreement wrote it. With `image/png` on both sides
        // the two are indistinguishable, and a URI spliced together from the
        // claim would pass unnoticed.
        let previa = preview_of(
            7,
            "IMAGE/PNG",
            Some(seele_core::enlace::Previa::Bytes(PNG.to_vec())),
        );
        assert_eq!(previa.attachment, 7);
        assert_eq!(previa.refusal, None);
        assert_eq!(previa.found.as_deref(), Some("image/png"));
        assert_eq!(
            previa.image.as_deref(),
            Some("data:image/png;base64,iVBORw0KGgoAAAAN"),
            "the URI a window draws from was not written from the bytes"
        );
    }

    #[test]
    fn bytes_that_disagree_with_the_claim_come_back_with_no_picture_at_all() {
        // The one that matters. A JPEG that called itself a PNG is refused, and
        // the refusal carries both halves so a sentence can name them — but
        // `image` is `None`, which is what a window acts on.
        let previa = preview_of(
            7,
            "image/png",
            Some(seele_core::enlace::Previa::Bytes(JPEG.to_vec())),
        );
        assert_eq!(previa.image, None, "a lying file was drawn anyway");
        assert_eq!(previa.refusal, Some(PreviewRefusal::Disagrees));
        assert_eq!(previa.claimed, "image/png");
        assert_eq!(previa.found.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn a_claim_never_reaches_the_string_a_window_draws_from() {
        // Whatever the sender wrote, it is quoted into `claimed` and nowhere
        // else. Here the claim is a whole `data:` URI of somebody's choosing,
        // and the only thing it produces is a refusal.
        let mentira = "data:image/png;base64,AAAA";
        let previa = preview_of(
            7,
            mentira,
            Some(seele_core::enlace::Previa::Bytes(PNG.to_vec())),
        );
        assert_eq!(previa.image, None);
        assert_eq!(previa.refusal, Some(PreviewRefusal::NotAPicture));
        assert_eq!(previa.claimed, mentira);
    }

    #[test]
    fn a_file_over_the_limit_says_so_and_says_what_the_limit_is() {
        let previa = preview_of(
            7,
            "image/png",
            Some(seele_core::enlace::Previa::GrandeDemais { tamanho: 99 }),
        );
        assert_eq!(previa.image, None);
        assert_eq!(
            previa.refusal,
            Some(PreviewRefusal::TooBig {
                limit: seele_core::PREVIEW_LIMIT
            }),
            "«too big» with no number sends somebody nowhere"
        );
    }

    #[test]
    fn a_session_that_ended_mid_fetch_is_not_a_picture_and_not_a_panic() {
        // The dropped channel. It reads as "the bytes did not come", which is
        // true, and not as any of the three that say something about the file.
        let previa = preview_of(7, "image/png", None);
        assert_eq!(previa.image, None);
        assert_eq!(previa.refusal, Some(PreviewRefusal::DidNotArrive));
    }
}
