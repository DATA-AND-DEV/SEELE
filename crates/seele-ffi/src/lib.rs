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
    identity, CageId, ClientMessageId, FilePinStore, LineId, PinDecision, Room, SyncBand,
    SyncInputs, SyncRatio, Voice,
};

pub use types::{
    Cage, EndReason, Event, Line, LinkState, Message, Notice, NoticeReason, Pattern, Pilot,
    PlugError, Severity, Snapshot, SyncBand as Band, Telemetry, Trust, VoiceMode,
};

/// O que a casca gráfica precisa do core além de um [`Plug`] vivo.
///
/// ADR 0002 deixa `seele-app` ver `seele-ffi` e mais nada, e a tela de entrada
/// do app precisa dos mesmos três módulos que o `plug` usa direto: a lista de
/// Dogmas visitados, a busca no histórico e a leitura de um `seele://`. Nenhum
/// dos três é lógica de casca — se fossem escritos aqui, seriam escritos de
/// novo no terminal e mais uma vez no cliente móvel.
///
/// Re-exportados um a um, e não com um `pub use seele_core::*`, pela mesma
/// doutrina que `seele-core` já aplica sobre `seele-proto`: acrescentar a esta
/// lista é a hora de perguntar se a casca precisa do valor ou da decisão por
/// trás dele.
pub use seele_core::{conhecidos, search, uri};

/// How often measurements are refreshed.
const TICK: Duration = Duration::from_millis(250);

/// How many messages of history to ask for when a Line opens.
const HISTORY_PAGE: u16 = 50;

/// What a shell needs to connect.
#[derive(Debug, Clone)]
pub struct ConnectConfig {
    /// `host` or `host:port`.
    pub server: String,
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
    /// Whether to open the microphone and speakers at all.
    ///
    /// False on a machine with no sound card, which is most servers and every
    /// CI box. The text half of the product needs none.
    pub audio: bool,
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
    Send { line: LineId, body: String },
    SetAtField(bool),
    SetTotalIsolation(bool),
    Shutdown,
}

/// State the driver thread writes and the shell reads.
struct Shared {
    room: Mutex<Room>,
    listeners: Mutex<Vec<Arc<dyn EventListener>>>,
    voice: Mutex<Option<Voice>>,
    nickname: Mutex<String>,
    pattern: AtomicU8,
    /// Round trip in microseconds. Integer because atomics have no `f32`, and
    /// microseconds because milliseconds would round a fast local link to zero.
    rtt_micros: std::sync::atomic::AtomicU64,
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
}

impl Shared {
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
    pub fn connect(config: ConnectConfig) -> Result<Arc<Self>, PlugError> {
        let (address, server_name, pin_key) = resolve(&config.server)?;

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
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            nickname: Mutex::new(config.nickname.clone()),
            pattern: AtomicU8::new(pattern_byte(Pattern::Offline)),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            sync_ratio: AtomicU8::new(0),
            running: AtomicBool::new(false),
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
        plug.shared.notify(&Event::Connected { trust });
        Ok(plug)
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

    /// Everything the interface needs, in one value.
    ///
    /// Cheap enough to call on every frame of a redraw, and deliberately a copy:
    /// a shell holding a borrow into live state is a shell that can see the
    /// roster change halfway through drawing it.
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

        let (
            audio,
            voice_mode,
            speaking,
            at_field,
            total_isolation,
            input_level,
            local_fault,
            bitrate,
        ) = self.audio_state();

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
            messages: messages_of(&room),
            telemetry: Telemetry {
                rtt_ms,
                jitter_ms: room.telemetry.as_ref().map_or(0.0, |t| t.jitter_ms),
                loss_fraction: room.telemetry.as_ref().map_or(0.0, |t| t.loss_fraction),
                bitrate_bps: bitrate,
                sync_ratio,
                sync_band: SyncBand::of(sync_ratio).into(),
                input_level,
                local_fault,
            },
            notice: room.notice.as_ref().map(|notice| Notice {
                severity: notice.severity.into(),
                reason: notice.reason.into(),
                operator_text: notice.operator_text.clone(),
            }),
            at_field,
            total_isolation,
            speaking,
            voice_mode,
            audio_available: audio,
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

    #[allow(clippy::type_complexity, reason = "read once, into one struct literal")]
    fn audio_state(&self) -> (bool, VoiceMode, bool, bool, bool, f32, bool, u32) {
        let Ok(voice) = self.shared.voice.lock() else {
            return (
                false,
                VoiceMode::PushToTalk,
                false,
                false,
                false,
                0.0,
                false,
                0,
            );
        };
        let Some(voice) = voice.as_ref() else {
            return (
                false,
                VoiceMode::PushToTalk,
                false,
                false,
                false,
                0.0,
                false,
                0,
            );
        };
        let telemetry = voice.telemetry();
        let mode = match voice.mode() {
            seele_core::VoiceMode::PushToTalk => VoiceMode::PushToTalk,
            seele_core::VoiceMode::VoiceActivated => VoiceMode::VoiceActivated,
            seele_core::VoiceMode::Open => VoiceMode::Open,
        };
        (
            true,
            mode,
            telemetry.local.speaking,
            voice.at_field(),
            voice.total_isolation(),
            telemetry.local.input_level,
            voice.falha_local(),
            telemetry.local.bitrate_bps,
        )
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
        })
        .collect()
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
fn resolve(target: &str) -> Result<(SocketAddr, String, String), PlugError> {
    let with_port = if target.contains(':') {
        target.to_owned()
    } else {
        format!("{target}:{}", seele_core::DEFAULT_PORT)
    };
    let host = with_port
        .rsplit_once(':')
        .map_or_else(|| with_port.clone(), |(host, _)| host.to_owned());
    let address = with_port
        .to_socket_addrs()
        .map_err(|_| PlugError::UnresolvableHost)?
        .next()
        .ok_or(PlugError::UnresolvableHost)?;

    // TLS gets the name the M2 certificate carries; the pin gets the address,
    // which is what actually tells one server from another.
    let server_name = if host.parse::<std::net::IpAddr>().is_ok() {
        "localhost".to_owned()
    } else {
        host
    };
    Ok((address, server_name, with_port))
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
    let destino = seele_core::enlace::Destino {
        servidor: address,
        nome_tls: server_name.clone(),
        chave_do_pin: pin_key.clone(),
        apelido: config.nickname.clone(),
        segredo: config.join_secret.clone(),
    };
    let mut client = match seele_core::enlace::Enlace::conectar(destino, key, pins).await {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(%error, "could not reach the Dogma");
            let _ = ready.send(Err(classify_connect_failure(&error)));
            return;
        }
    };

    let trust = match client.pin_decision() {
        PinDecision::FirstContact { fingerprint } => Trust::FirstContact {
            fingerprint: fingerprint.clone(),
        },
        PinDecision::Matches => Trust::Known,
        PinDecision::Changed { pinned, offered } => {
            let error = PlugError::PinChanged {
                pinned: pinned.clone(),
                offered: offered.clone(),
            };
            let _ = ready.send(Err(error));
            return;
        }
    };

    if let Ok(mut room) = shared.room.lock() {
        room.adopt(client.sessao(), &config.nickname);
    }
    shared
        .pattern
        .store(pattern_byte(Pattern::Blue), Ordering::Relaxed);

    if config.audio {
        match Voice::start(client.media(), client.sessao().ssrc) {
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
                    seele_core::enlace::Aviso::Mensagem(message) => fold(&shared, &message),
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
                        if let Ok(mut slot) = shared.voice.lock() {
                            if slot.is_some() {
                                *slot = Voice::start(*media, sessao.ssrc).ok();
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
    client.sair().await;
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
        shared.notify(&Event::MessagesChanged);
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

/// Refreshes what the shell reads without asking the server.
///
/// Returns whether anything moved enough to be worth telling a shell about.
fn measure(sync: &mut SyncRatio, client: &Enlace, shared: &Arc<Shared>) -> bool {
    let mut jitter_ms = 0.0;
    let mut loss = 0.0;
    if let Ok(voice) = shared.voice.lock() {
        if let Some(voice) = voice.as_ref() {
            let telemetry = voice.telemetry();
            // Jitter and loss are only observable at the receiver, which is why
            // the server's own numbers are not the ones used here.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "milliseconds and a fraction; f32 is what the protocol carries"
            )]
            {
                jitter_ms = telemetry.worst_jitter_depth_ms() as f32;
                loss = telemetry.worst_loss_fraction() as f32;
            }
        }
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
            shared.notify(&Event::MessagesChanged);
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
        Command::Shutdown => return false,
    }
    true
}

/// A monotonic identifier for outgoing messages.
///
/// Process-wide rather than per-`Plug`: two handles in one process sending the
/// same number would collide in the server's idempotency check, and the second
/// message would be silently dropped as a resend of the first.
fn next_client_message_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
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
        seele_core::ConnectError::HandshakeTimeout => PlugError::HandshakeTimeout,
        seele_core::ConnectError::Refused { reason } => PlugError::Refused {
            reason: (*reason).into(),
        },
    }
}

#[cfg(test)]
mod tests {
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
            sync_ratio: 42,
        }));

        let pilot = &cages_of(&room)[0].pilots[0];
        assert_eq!(pilot.sync_ratio, 42);
        assert_eq!(pilot.sync_band, types::SyncBand::Degraded);
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

        let shared = Arc::new(Shared {
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            nickname: Mutex::new("ayanami".into()),
            pattern: AtomicU8::new(0),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            sync_ratio: AtomicU8::new(0),
            running: AtomicBool::new(true),
        });
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
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            nickname: Mutex::new("ayanami".into()),
            pattern: AtomicU8::new(0),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            sync_ratio: AtomicU8::new(0),
            running: AtomicBool::new(true),
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
