//! The values that cross the boundary.
//!
//! Everything here is plain: `u64`, `u32`, `String`, `bool`, `Vec`, and closed
//! enums. **No `seele-proto` newtype appears in this module**, and that is the
//! point rather than an oversight.
//!
//! `specs/06-clientes-gui.md` states the rule in one sentence: "Se o frontend
//! precisa saber o que é um `ssrc`, algo está errado." A `PersonId` crossing
//! into a shell is protocol knowledge in the shell, and the third shell would
//! have to learn it too.
//!
//! ADR 0018 explains why the shapes are what they are: this is what `uniffi`
//! will be able to annotate in M6 without a rewrite.
//!
//! # Why these derive `Serialize`
//!
//! The Tauri shell talks to its webview over JSON. Without `serde` here, the
//! desktop crate would have to declare a mirror of every type in this file plus
//! the conversion between them — the same duplication `seele_core::state` was
//! moved to avoid, one layer up. The derive is invisible to `uniffi`, which
//! generates its own marshalling and does not care.
//!
//! Field names stay `snake_case` on the wire. A rename layer buys nothing and
//! costs one more place for a field to be spelled two ways.

/// How the microphone opens. `specs/03-audio.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VoiceMode {
    /// A key is held. Never false-triggers, so it is the default.
    PushToTalk,
    /// The level decides, with hysteresis and hangover.
    VoiceActivated,
    /// Always open.
    Open,
}

impl From<VoiceMode> for seele_core::VoiceMode {
    fn from(mode: VoiceMode) -> Self {
        match mode {
            VoiceMode::PushToTalk => Self::PushToTalk,
            VoiceMode::VoiceActivated => Self::VoiceActivated,
            VoiceMode::Open => Self::Open,
        }
    }
}

/// A volta, e ela **precisa morar junto da ida**.
///
/// Esta conversão existia como um `match` solto dentro de `snapshot`, e a de
/// cima aqui: o mesmo mapeamento escrito em dois lugares, que é como um ganha
/// um quarto modo e o outro não. É exatamente a forma do defeito que custou uma
/// versão inteira em campo — o `palco-imagem.js` declarava o perfil do vídeo
/// que o `codec.rs` decidia, e os dois deixaram de concordar sem que nada
/// avisasse.
impl From<seele_core::VoiceMode> for VoiceMode {
    fn from(mode: seele_core::VoiceMode) -> Self {
        match mode {
            seele_core::VoiceMode::PushToTalk => Self::PushToTalk,
            seele_core::VoiceMode::VoiceActivated => Self::VoiceActivated,
            seele_core::VoiceMode::Open => Self::Open,
        }
    }
}

/// How far the connection has got. `specs/07-estetica.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LinkTrust {
    /// Not connected.
    Offline,
    /// Connected, not verified.
    Unverified,
    /// Verified. Normal operation.
    Verified,
}

/// Which band a Sync Ratio falls into.
///
/// Carried as an enum beside the number so a shell never has to know the
/// thresholds. Two shells with two copies of "85 is nominal" is two shells that
/// disagree the day one of them is updated.
///
/// Three bands, from `comp v2`. The fourth — `Acceptable`
/// — is gone, and it is gone from the JSON the webview reads too: a shell that
/// still branches on `"Acceptable"` now falls through to whatever it does with
/// an unknown band, and never to a colour it drew last week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum SignalBand {
    /// 85 and above.
    Nominal,
    /// 60 to 84.
    Degraded,
    /// Below 60.
    ///
    /// The default, because an unmeasured ratio is zero and zero is critical by
    /// the thresholds the comp sets. Defaulting to nominal would be a
    /// reassurance nobody measured — worse than an alarm, because an alarm gets
    /// checked.
    #[default]
    Critical,
}

impl From<seele_core::SignalBand> for SignalBand {
    fn from(band: seele_core::SignalBand) -> Self {
        match band {
            seele_core::SignalBand::Nominal => Self::Nominal,
            seele_core::SignalBand::Degraded => Self::Degraded,
            seele_core::SignalBand::Critical => Self::Critical,
        }
    }
}

/// How loud a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    /// Worth knowing.
    Info,
    /// Something is degrading.
    Warning,
    /// Something is wrong.
    Critical,
}

impl From<seele_core::AlertSeverity> for Severity {
    fn from(severity: seele_core::AlertSeverity) -> Self {
        match severity {
            seele_core::AlertSeverity::Info => Self::Info,
            seele_core::AlertSeverity::Warning => Self::Warning,
            seele_core::AlertSeverity::Critical => Self::Critical,
        }
    }
}

/// What a notice is about. Enumerated so each shell writes its own sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum NoticeReason {
    /// The person was named in a message.
    Mentioned,
    /// A subsystem changed health.
    SubsystemChanged,
    /// The connection is struggling.
    SyncDegraded,
    /// Entry to a voice room was refused.
    VoiceRoomEntryRefused,
    /// The action needed a permission the person lacks.
    PermissionDenied,
    /// The voice room is at its limit.
    VoiceRoomFull,
    /// The operator is saying something.
    OperatorNotice,
    /// The client is sending control frames faster than its budget.
    RateLimited,
    /// An operator moved this person's connection into another voice room.
    MovedByOperator,
    /// The voice room this person's connection was in no longer exists.
    VoiceRoomDeleted,
    /// A Channel this person had open no longer exists.
    ChannelDeleted,
    /// The voice room asked about is the only one the server has, so it stays.
    LastVoiceRoom,
    /// Somebody else is already sharing their screen in this room.
    ///
    /// One share per voice room (§6.3 da spec do compartilhamento de tela), and
    /// this is what whoever lost the race is told. Not a permission problem —
    /// the person may share the moment the other one stops — and that is why it
    /// is its own reason instead of `PermissionDenied`, which would say the
    /// wrong thing and send somebody looking for a role they already have.
    ScreenShareTaken,
    /// O servidor parou a transmissão desta pessoa: a sala cresceu além da subida
    /// de quem hospeda.
    ///
    /// §5.1 pôs o caminho de quem hospeda dentro do teto — `caminho × 60% ÷ N` —
    /// porque é o servidor que sobe as N cópias. Passado certo N nem o piso do §2
    /// cabe, e o §3.2 diz o que acontece então: *quem para é o vídeo*.
    ///
    /// Razão própria, e não [`Self::SyncDegraded`], que toda casca escreve como
    /// «sinal em queda» — uma frase sobre a conexão de quem lê, na frente de
    /// alguém cuja conexão está boa e cuja plateia cresceu. E não um
    /// `ScreenShareStopped` mudo: quem apertou parar sabe que apertou, e quem
    /// foi parado pelo servidor não ficaria sabendo de nada.
    ScreenShareOverHostUplink,

    /// A sala cresceu além do que a subida medida desta máquina comporta.
    ///
    /// Os dois números viajam porque «a sala está grande» não é acionável e
    /// «precisa de 6,5 Mbps, e a medida é 4 Mbps» é. A casca escreve a frase;
    /// aqui só chegam os valores. Ver o ADR 0038.
    VoiceRoomOverHostUplink {
        /// Quanto a sala pede no pior caso, todos falando ao mesmo tempo.
        precisa_bps: u64,
        /// A subida que esta máquina mediu para si. Nunca zero.
        medido_bps: u32,
    },
}

impl From<seele_core::AlertReason> for NoticeReason {
    fn from(reason: seele_core::AlertReason) -> Self {
        match reason {
            seele_core::AlertReason::Mentioned => Self::Mentioned,
            seele_core::AlertReason::SubsystemChanged => Self::SubsystemChanged,
            seele_core::AlertReason::SyncDegraded => Self::SyncDegraded,
            seele_core::AlertReason::VoiceRoomEntryRefused => Self::VoiceRoomEntryRefused,
            seele_core::AlertReason::PermissionDenied => Self::PermissionDenied,
            seele_core::AlertReason::VoiceRoomFull => Self::VoiceRoomFull,
            seele_core::AlertReason::OperatorNotice => Self::OperatorNotice,
            seele_core::AlertReason::VoiceRoomOverHostUplink {
                precisa_bps,
                medido_bps,
            } => Self::VoiceRoomOverHostUplink {
                precisa_bps,
                medido_bps,
            },
            seele_core::AlertReason::RateLimited => Self::RateLimited,
            seele_core::AlertReason::MovedByOperator => Self::MovedByOperator,
            seele_core::AlertReason::VoiceRoomDeleted => Self::VoiceRoomDeleted,
            seele_core::AlertReason::ChannelDeleted => Self::ChannelDeleted,
            seele_core::AlertReason::LastVoiceRoom => Self::LastVoiceRoom,
            seele_core::AlertReason::ScreenShareTaken => Self::ScreenShareTaken,
            seele_core::AlertReason::ScreenShareOverHostUplink => Self::ScreenShareOverHostUplink,
        }
    }
}

/// What o canal holds, as the confirmation in front of destroying it needs it.
///
/// Counted in the server's database at the instant of asking, and carried across
/// the bridge unrounded. A shell cannot work these out for itself: it holds one
/// page of history and would guess low by whatever the Channel's whole past is,
/// and a number that is nearly right in a box promising to destroy 1.847
/// messages is worse than no number at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ChannelWeight {
    /// Which Channel was weighed.
    pub channel: u32,
    /// How many messages are in it that anybody can read.
    pub messages: u32,
    /// How many distinct people wrote them.
    pub authors: u32,
    /// When the oldest was written, in seconds since the Unix epoch.
    ///
    /// `None` when the Channel is empty — the one case where the sentence has no
    /// date to give and has to say something else instead. Turning it into a
    /// date somebody reads is a shell's job, like every other timestamp that
    /// crosses here.
    pub oldest_at_seconds: Option<i64>,
}

/// Why a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum EndReason {
    /// Protocol version outside the compatibility window.
    Incompatible,
    /// Authentication failed. Says nothing about whether the account exists —
    /// `specs/08-seguranca.md` requires login failures to be uniform.
    CredentialRejected,
    /// The handshake did not finish in time.
    HandshakeTimeout,
    /// An operator disconnected this person.
    Kicked,
    /// An operator barred this person.
    Banned,
    /// The server is full.
    ServerFull,
    /// Planned downtime.
    ScheduledMaintenance,
    /// The server is stopping.
    ServerShuttingDown,
    /// Keepalive lapsed.
    Timeout,
    /// The client sent something it should not have.
    ProtocolViolation,
    /// The client exceeded its frame budget.
    RateLimited,
    /// This connection fell behind the server's events and lost some.
    ///
    /// Reconnecting is the repair, not a refusal: the session's view of the
    /// conversation has a hole in it that only a full resync fills.
    FellBehind,
    /// The host has not decided about this key yet. ADR 0030.
    ///
    /// Not a refusal: the request is written down on the hosting machine and
    /// stays there. Trying again after the host has looked is what gets in, and
    /// nothing was lost by the attempt.
    AdmissionPending,
    /// The host decided this key does not come in. ADR 0030.
    ///
    /// Milder than [`Self::Banned`]: the host can approve the same key later,
    /// and this never ended a session that was already running.
    AdmissionDenied,
    /// O apelido pedido é de outra chave. ADR 0017.
    ///
    /// Separada de [`Self::CredentialRejected`] porque é a única deste conjunto
    /// que a pessoa conserta sozinha, e porque não passa com o tempo: tentar de
    /// novo, ser aprovado de novo e reinstalar o app dão todos o mesmo
    /// resultado. Enquanto ela vestia a frase da credencial, o conselho que a
    /// tela dava — «confira o convite» — mandava a pessoa mexer na única coisa
    /// que não era o problema.
    NicknameTaken,
    /// The link died without the server saying why.
    LinkLost,
}

impl From<seele_core::DisconnectReason> for EndReason {
    fn from(reason: seele_core::DisconnectReason) -> Self {
        match reason {
            seele_core::DisconnectReason::Incompatible => Self::Incompatible,
            seele_core::DisconnectReason::CredentialRejected => Self::CredentialRejected,
            seele_core::DisconnectReason::HandshakeTimeout => Self::HandshakeTimeout,
            seele_core::DisconnectReason::Kicked => Self::Kicked,
            seele_core::DisconnectReason::Banned => Self::Banned,
            seele_core::DisconnectReason::ServerFull => Self::ServerFull,
            seele_core::DisconnectReason::ScheduledMaintenance => Self::ScheduledMaintenance,
            seele_core::DisconnectReason::ServerShuttingDown => Self::ServerShuttingDown,
            seele_core::DisconnectReason::Timeout => Self::Timeout,
            seele_core::DisconnectReason::ProtocolViolation => Self::ProtocolViolation,
            seele_core::DisconnectReason::RateLimited => Self::RateLimited,
            seele_core::DisconnectReason::FellBehind => Self::FellBehind,
            seele_core::DisconnectReason::AdmissionPending => Self::AdmissionPending,
            seele_core::DisconnectReason::AdmissionDenied => Self::AdmissionDenied,
            seele_core::DisconnectReason::NicknameTaken => Self::NicknameTaken,
        }
    }
}

/// What the server's certificate turned out to be. ADR 0003, ADR 0006.
///
/// Mirrors `seele_core::tofu::Verdict` one variant at a time — see the `From`
/// impl below — so the shell sees every distinction the core makes and none
/// it doesn't.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Trust {
    /// Nothing was pinned and no invite vouched for anything. Pinned blind.
    ///
    /// `specs/08-seguranca.md` wants this stated rather than accepted in
    /// silence, which is why it is a value the shell must handle and not a log
    /// channel it may ignore.
    FirstContact {
        /// What was pinned. Show it — somebody may want to check it elsewhere.
        fingerprint: String,
    },
    /// Nothing was pinned, and the invite confirmed what the server offered.
    ///
    /// This is what ADR 0006 invented the link to produce.
    FirstContactVerified {
        /// What was pinned, now vouched for.
        fingerprint: String,
    },
    /// The pin matches and nothing contradicts it. Nothing to say.
    Known,
    /// First contact, and the invite named a different key. Refused.
    ///
    /// The core drops the connection when this happens, so in practice this
    /// arm never crosses the boundary — it exists to keep the match exhaustive.
    InviteRefused {
        /// What the link promised.
        expected: String,
        /// What the server offered.
        offered: String,
    },
    /// The pin is the usual one, but the invite names a different key.
    ///
    /// The connection stands: trust on first use already established that this
    /// is the same server as before, so the link is what is wrong.
    InviteDisagrees {
        /// What the link promised.
        expected: String,
        /// What the server offered, and what stays pinned.
        offered: String,
    },
}

impl From<seele_core::tofu::Verdict> for Trust {
    fn from(verdict: seele_core::tofu::Verdict) -> Self {
        use seele_core::tofu::Verdict;

        // An exhaustive match rather than a blanket conversion: when the core
        // grows a sixth verdict this stops compiling, instead of silently
        // mapping it onto one that already exists.
        match verdict {
            Verdict::FirstContact { fingerprint } => Self::FirstContact { fingerprint },
            Verdict::FirstContactVerified { fingerprint } => {
                Self::FirstContactVerified { fingerprint }
            }
            Verdict::Known => Self::Known,
            Verdict::InviteRefused { expected, offered } => {
                Self::InviteRefused { expected, offered }
            }
            Verdict::InviteDisagrees { expected, offered } => {
                Self::InviteDisagrees { expected, offered }
            }
        }
    }
}

/// One person in a voice room.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Person {
    /// Stable identifier for this person on this server.
    pub id: u64,
    /// Display name.
    pub nickname: String,
    /// Transmitting right now.
    pub speaking: bool,
    /// Microphone muted — mudo.
    pub muted: bool,
    /// Speakers muted — Isolamento total.
    pub total_isolation: bool,
    /// Sync Ratio, 0 to 100.
    pub signal: u8,
    /// Which band that falls into, decided once, in the core.
    pub sync_band: SignalBand,
    /// Whether this is the person holding the handle.
    pub is_self: bool,
}

/// The average Sync Ratio of a voice room, already banded — **MÉDIA DO VOICE_ROOM**.
///
/// The comp computes this in the shell and colours it there. It is computed in
/// the core instead, for the reason at the top of this module: a shell that
/// knows the thresholds is a shell that will disagree with the other one. What
/// arrives here is the number, the band and the size of the sample, and drawing
/// is all that is left to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct VoiceRoomSync {
    /// The average, 0 to 100, rounded to a whole point.
    ///
    /// The comp prints `82.4`; the datum is a `u8` at every point it exists, so
    /// this is `82`. A decimal here would be precision invented at the last
    /// step, not carried from the measurement.
    pub ratio: u8,
    /// Which band the average falls into.
    pub band: SignalBand,
    /// How many people it averages — the comp's `5 PLUGS`.
    pub people: u32,
}

impl From<seele_core::VoiceRoomSync> for VoiceRoomSync {
    fn from(sync: seele_core::VoiceRoomSync) -> Self {
        Self {
            ratio: sync.ratio,
            band: sync.band.into(),
            // A voice room holds at most `limit` people and `limit` is a `u16`. The
            // saturation is unreachable and is here so the conversion cannot
            // panic on a server that says otherwise.
            people: u32::try_from(sync.people).unwrap_or(u32::MAX),
        }
    }
}

/// A voice channel.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VoiceRoom {
    /// Identifier.
    pub id: u32,
    /// Display name.
    pub name: String,
    /// How many people fit.
    pub limit: u16,
    /// Whether entry needs a password.
    pub password_required: bool,
    /// Whether this person's connection is in it.
    pub occupied_by_us: bool,
    /// The Channel bound to it, if any. `specs/04-servidor-seele.md` makes the
    /// association optional.
    ///
    /// Carried so a shell can say what destroying that Channel would do to this
    /// room. The voice room survives it and comes out with no canal attached, which is
    /// a change nobody asked for — and a product whose confirmations name their
    /// consequences has to be able to name that one.
    pub channel: Option<u32>,
    /// Who is inside, in arrival order.
    pub people: Vec<Person>,
    /// The average Sync Ratio of everybody inside, or `None` if nobody is.
    ///
    /// `None` rather than zero: an empty voice room has nothing to average, and zero
    /// would draw every idle room in the critical colour.
    pub sync: Option<VoiceRoomSync>,
}

/// A text channel.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Channel {
    /// Identifier.
    pub id: u32,
    /// Display name.
    pub name: String,
    /// Whether this is the Channel being read.
    pub open: bool,
}

/// One thing somebody said.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Message {
    /// Server-assigned identifier. Ordered; the clock is not.
    pub id: u64,
    /// Which Channel.
    pub channel: u32,
    /// Who wrote it.
    pub author: u64,
    /// Their name.
    pub author_nickname: String,
    /// When the **server** accepted it, **seconds** since the Unix epoch.
    ///
    /// The unit is in the name deliberately — see `seele-proto`, where getting
    /// it wrong drew every real message as 1970 while the tests passed.
    ///
    /// Turning it into something a person reads is the shell's job: a locale, a
    /// time zone and a format are all things this layer must not decide.
    pub at_seconds: i64,
    /// The body.
    pub body: String,
    /// Whether we wrote it.
    pub own: bool,
    /// Whether it has been edited since.
    pub edited: bool,
    /// The file hanging off it, if any. ADR 0027.
    pub attachment: Option<Attachment>,
}

/// One microphone this machine is offering.
///
/// Two strings and not one, because they answer two different questions. The id
/// is what a preference is written down as — the core documents it as stable
/// across runs, unplugs and reboots — and the name is what a person reads. Two
/// microphones of the same model report the same name, so a shell that stored
/// the name would leave the second one unpickable.
///
/// A shell shows `name` and sends back `id`. It never parses either.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CaptureDevice {
    /// The stable handle to send back. Not for a person to read.
    pub id: String,
    /// What the machine calls it.
    pub name: String,
    /// Whether this is the one a session with no preference would take.
    ///
    /// Carried rather than derived: "the default" is a moving target, and a
    /// shell cannot recompute which row is the machine's own choice.
    pub default: bool,
}

/// One place this machine will play sound.
///
/// The twin of [`CaptureDevice`], with the same two strings for the same two
/// questions, and its own type rather than a shared one carrying a direction:
/// both ids are strings, so only the type stops a shell sending a microphone
/// where an output belongs.
///
/// A shell shows `name` and sends back `id`. It never parses either.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlaybackDevice {
    /// The stable handle to send back. Not for a person to read.
    pub id: String,
    /// What the machine calls it.
    pub name: String,
    /// Whether this is the one a session with no preference would take.
    pub default: bool,
}

/// Connection quality.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize)]
pub struct Telemetry {
    /// Round trip, milliseconds.
    pub rtt_ms: f32,
    /// Arrival jitter, milliseconds.
    pub jitter_ms: f32,
    /// Packet loss, 0.0 to 1.0.
    pub loss_fraction: f32,
    /// Encoder bitrate, bits per second. Zero when there is no audio.
    pub bitrate_bps: u32,
    /// This person's Sync Ratio, 0 to 100.
    pub signal: u8,
    /// Which band that is.
    pub sync_band: SignalBand,
    /// Microphone level, 0.0 to 1.0.
    pub input_level: f32,
    /// Whether the machine, rather than the network, is dropping audio.
    ///
    /// The two sound identical to a listener and have opposite fixes, so the
    /// core separates them and the shell can say which one it is.
    pub local_fault: bool,
    /// Voice frames this machine produced and never managed to send.
    ///
    /// The third kind of loss, and the one that had no name. `loss_fraction`
    /// is the network's, reported by the server. `local_fault` is this machine's
    /// capture or playback stumbling. This is neither: the frame was encoded,
    /// it was ready, and the transport refused it.
    ///
    /// A QUIC datagram is not fragmented. Text travels on a stream and adapts
    /// to the path by itself; voice travels in datagrams, and a datagram that
    /// does not fit the path is refused whole. That is how a link can deliver
    /// every message and still chop the audio — and in one direction only,
    /// because the path is not the same both ways.
    ///
    /// Zero is normal. Anything else means the audio is being lost **before it
    /// leaves**, which sounds identical to network loss and has the opposite
    /// fix.
    pub frames_refused: u64,
}

/// Something worth surfacing.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Notice {
    /// How loud.
    pub severity: Severity,
    /// What about.
    pub reason: NoticeReason,
    /// The operator's own words, when they have any.
    ///
    /// Not a hole in the enumerated-reasons rule: an operator writing about
    /// their own server is data, not an error reason.
    pub operator_text: Option<String>,
}

/// Uma tela ou uma janela que esta máquina pode transmitir.
///
/// O gêmeo de [`CaptureDevice`] para o vídeo, com a mesma divisão de trabalho:
/// a casca mostra `nome`, devolve `id` em [`crate::Connection::compartilhar_tela`], e
/// não lê nem um nem o outro.
///
/// `largura` e `altura` são o tamanho da **fonte**, e não o da transmissão. Os
/// dois quase nunca são iguais: o que sai é o degrau que o teto compra (§5.1),
/// e quem quiser o número que está saindo lê [`TelaEmCurso::altura`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FonteDeTela {
    /// O identificador a devolver. Não é para uma pessoa ler.
    pub id: u64,
    /// Como o sistema chama esta fonte.
    pub nome: String,
    /// `true` para um monitor inteiro, `false` para uma janela.
    pub monitor: bool,
    /// Largura da fonte, em pixels.
    pub largura: u32,
    /// Altura da fonte, em pixels.
    pub altura: u32,
}

/// O que o sistema deixa este processo fazer com o microfone.
///
/// O gêmeo de [`PermissaoDeTela`], e a diferença entre os dois é o que a tela
/// pode oferecer: aquele tem um botão que **pergunta**, e este não — no Windows
/// não há a quem pedir para um app de área de trabalho. Aqui o botão só abre a
/// página certa dos Ajustes.
///
/// Ver `seele_audio::device::ConsentimentoDoMicrofone` para o porquê de cada
/// caso, e para a ordem em que o sistema os decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PermissaoDeMicrofone {
    /// O sistema deixa.
    Permitida,
    /// Desligado por política da máquina; quem conserta é quem a administra.
    NegadaNaMaquina,
    /// A pessoa desligou o microfone para todos os aplicativos.
    NegadaParaTudo,
    /// O microfone está ligado e **aplicativos de área de trabalho** não.
    NegadaParaAreaDeTrabalho,
    /// Não deu para saber. **Não é «permitida»** — ver o enum do núcleo.
    NaoSeSabe,
}

impl From<seele_core::ConsentimentoDoMicrofone> for PermissaoDeMicrofone {
    fn from(consentimento: seele_core::ConsentimentoDoMicrofone) -> Self {
        match consentimento {
            seele_core::ConsentimentoDoMicrofone::Permitido => Self::Permitida,
            seele_core::ConsentimentoDoMicrofone::NegadoNaMaquina => Self::NegadaNaMaquina,
            seele_core::ConsentimentoDoMicrofone::NegadoParaTudo => Self::NegadaParaTudo,
            seele_core::ConsentimentoDoMicrofone::NegadoParaAreaDeTrabalho => {
                Self::NegadaParaAreaDeTrabalho
            }
            seele_core::ConsentimentoDoMicrofone::NaoSeSabe => Self::NaoSeSabe,
        }
    }
}

/// O que o sistema operacional respondeu sobre gravar a tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PermissaoDeTela {
    /// Pode capturar.
    Concedida,
    /// Negada, e a pessoa precisa ir aos ajustes do sistema — no macOS o TCC
    /// não pergunta duas vezes.
    Negada,
    /// Ainda não perguntada.
    NaoPerguntada,
    /// Esta compilação não tem como perguntar ao sistema.
    ///
    /// **Não estava no contrato de 22/08, e existe porque as outras três
    /// mentiriam.** É o Linux da v1: não há captura de tela neste build — o
    /// portal XDG exige `ashpd` mais `pipewire`, e com eles o binário deixa de
    /// ser autocontido (decisão de 22/08/2026, §7 item 5) —, então não há a quem
    /// perguntar. Responder `NaoPerguntada` mandaria a pessoa apertar um botão
    /// que não pergunta nada, e responder `Negada` culparia um sistema que nunca
    /// foi consultado.
    ///
    /// Uma casca que recebe isto **não desenha o controle de compartilhar**. É a
    /// mesma resposta que [`Snapshot::caminho`] dá com `None`: sem informação, a
    /// tela não escreve nada.
    NaoSeSabe,
}

/// O que a pessoa escolheu, e **todos são teto** (§5 da spec de tela).
///
/// Teto e nunca piso: o sistema continua livre para ficar abaixo de cada um
/// destes números, e a §3.2 depende disso — *a voz nunca cede à tela*. Uma
/// escolha tratada como piso devolve os 225 ms de atraso na voz que o
/// `spikes/tela-no-transporte` mediu.
///
/// Nada aqui é conferido nesta travessia. Os valores da lista fechada do §5 são
/// 1080/720/540 e 30/15/8, e quem os converte em degrau de codificador é quem
/// abre o codificador — uma ponte que recusasse um número seria uma segunda
/// autoridade sobre a mesma lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LimitesDeTela {
    /// Teto de banda em bits por segundo. `None` = sem limite próprio; vale só
    /// o que o caminho permitir.
    pub banda_bps: Option<u32>,
    /// Altura máxima em pixels: 1080, 720 ou 540.
    pub altura_maxima: u32,
    /// Quadros por segundo, no máximo: 30, 15 ou 8.
    pub quadros_maximos: u32,
    /// O que cede primeiro quando o orçamento aperta.
    ///
    /// **Não é teto como os três acima**, e por isso não é um número: os outros
    /// dizem «no máximo isto», este diz o que sacrificar quando o máximo não
    /// couber.
    ///
    /// Ausente no JSON é [`Prioridade::Nitidez`], que é o padrão do §2 — uma
    /// casca antiga que não conheça este campo continua pedindo o que sempre
    /// pediu.
    #[serde(default)]
    pub prioridade: Prioridade,
}

/// O que cede primeiro quando o orçamento aperta.
///
/// `nitidez` é a regra do §2 — a resolução segura, o quadro cede —, e é certa
/// para texto: ele continua legível a 8 quadros e vira borrão se a resolução
/// baixar. `movimento` é o contrário, e é o que jogo pede: a 8 quadros um jogo
/// não é pior, é inutilizável.
///
/// Atravessa como texto minúsculo — `"nitidez"`, `"movimento"` — porque a casca
/// gráfica escreve este JSON à mão, e um número seria uma tabela a manter dos
/// dois lados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Prioridade {
    /// A resolução segura, o quadro cede. O padrão.
    #[default]
    Nitidez,
    /// O quadro segura, a resolução cede.
    Movimento,
}

/// A transmissão de tela desta sala de voz, quando há uma.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TelaEmCurso {
    /// Quem está compartilhando.
    pub de: u64,
    /// Se é esta pessoa.
    pub e_minha: bool,
    /// A altura que está saindo **agora** — não a que foi pedida.
    ///
    /// Zero enquanto [`Self::medida`] for `false`. A tela mostra este número ao
    /// lado do que foi pedido (§5): receber menos do que se escolheu não é
    /// defeito, esconder que aconteceu é.
    pub altura: u32,
    /// Os quadros por segundo que estão saindo agora. Zero enquanto
    /// [`Self::medida`] for `false`.
    pub quadros: u32,
    /// Os kbps que estão saindo agora. Zero enquanto [`Self::medida`] for
    /// `false`.
    pub kbps: u32,
    /// Quantas pessoas estão na sala de voz além de quem compartilha.
    ///
    /// Contadas no roster **desta** máquina, que é a única contagem que existe
    /// aqui. É a razão que a tela escreve ao lado da resolução — `720p · 6
    /// pessoas assistindo`, §5.1 — e **não** é, hoje, o N pelo qual o servidor
    /// dividiu o teto: esse número é calculado em `VoiceRoom::reconferir_o_teto` e
    /// nenhum quadro de controle o carrega de volta ao cliente. Os dois
    /// coincidem sempre que o roster estiver em dia, e divergem no intervalo
    /// entre alguém entrar e o `PersonJoined` chegar.
    pub espectadores: u32,
    /// `Some` quando a transmissão está parada, com o nome estável do motivo.
    ///
    /// Um nome e não uma frase, como [`Snapshot::caminho`]: a lista
    /// [`crate::motivos_de_parada_da_tela`] é derivada deste mesmo mapeamento,
    /// para a casca poder cobrar cobertura de frase sem repetir os nomes. Uma
    /// frase pronta em português atravessando aqui seria a única sentença que a
    /// casca não escreve — e a que o guarda de vocabulário da interface não vê.
    pub parada: Option<String>,
    /// Se [`Self::altura`], [`Self::quadros`] e [`Self::kbps`] foram medidos.
    ///
    /// **Não estava no contrato de 22/08.** Está aqui porque os três são `u32`
    /// e hoje ninguém os mede: nada nesta ponte alcança o codificador de quem
    /// compartilha, e do lado de quem assiste nada abre a recepção. Sem este
    /// campo os três sairiam zerados e a tela escreveria `0p · 0 quadros`, que
    /// é a mentira confiante — o mesmo defeito do jitter que o servidor manda como
    /// `0.0` porque não tem como medi-lo.
    ///
    /// `false` significa **não sei**, e a casca não escreve nada.
    pub medida: bool,
    /// O que foi **pedido** para esta transmissão, quando ela é desta pessoa.
    ///
    /// O outro lado do §5: *a tela não promete a escolha*, e mostra o que está
    /// saindo **ao lado** do que foi pedido. Os campos acima são a primeira
    /// metade; sem esta, a segunda não atravessa e a comparação que a regra
    /// obriga não tem com o que ser feita.
    ///
    /// **Estava faltando no contrato de 22/08, e a falta tinha dono.** A casca
    /// gráfica cobriu o buraco guardando os limites que ela mesma mandou numa
    /// variável de JavaScript — e uma janela recarregada, ou uma segunda janela
    /// sobre a mesma sessão, perdia a memória de uma transmissão que continuava
    /// saindo. Aqui o pedido mora do lado que sobrevive à janela, que é o mesmo
    /// lado que o mandou ao codificador.
    ///
    /// `None` em dois casos, e os dois são a mesma resposta honesta — «não sei»:
    ///
    /// - **quem só assiste**, sempre. O teto é escolha de quem compartilha e não
    ///   atravessa o fio em lugar nenhum (`ScreenHeader` carrega resolução e
    ///   codec, nunca o que a pessoa pediu). Inventá-lo a partir do que chega
    ///   seria devolver a medida como se fosse a escolha, que é exatamente a
    ///   promessa que o §5 proíbe;
    /// - **uma transmissão própria que este processo não começou** — a sessão
    ///   caiu e voltou, ou o servidor reanunciou uma tela de antes.
    pub pedido: Option<LimitesDeTela>,
}

/// Everything the interface needs, in one value.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Snapshot {
    /// Por qual caminho esta conversa saiu, como nome estável.
    ///
    /// Um de `RedeLocal`, `Ipv6Direto`, `EnderecoPublico` ou `FuroDeNat` — os
    /// quatro de `seele_core::chegada::Caminho`, que a lista
    /// [`crate::caminhos`] deriva do enum para a casca poder cobrar cobertura
    /// de frase sem repetir os nomes.
    ///
    /// **`None` quando não se sabe, e a casca então não escreve nada.** Não
    /// existe um quinto nome para «direto»: a distinção que essa palavra
    /// apagaria é justamente a que importa — em `FuroDeNat` a conversa é direta
    /// **e** alguém soube que ela existe. Inventar um nome quando não se sabe é
    /// a mentira confiante que o ADR 0022 existe para não produzir.
    ///
    /// O grau de certeza de `FuroDeNat` está escrito onde ele nasce, em
    /// `seele_core::chegada::Caminho`: um `LEVE` que saiu é evidência forte de
    /// que o furo abriu, e não é prova.
    pub caminho: Option<&'static str>,
    /// Onde o enlace está: no ar, na bateria interna, ou acabado.
    ///
    /// `specs/07-estetica.md` manda a interface não fechar quando a
    /// conexão cai — ela esmaece, conta cinco minutos e continua legível. Sem
    /// este campo a casca gráfica não tinha como saber disso e pulava direto
    /// para "encerrado", que é a simplificação que a spec proíbe.
    pub link: LinkState,
    /// How far the connection has got.
    pub link_state: LinkTrust,
    /// What the server is called.
    ///
    /// Follows a rename **inside the session** now: the server announces it to
    /// everybody connected, so a shell that redraws its header on
    /// [`Event::ServerChanged`] never shows a name the server stopped using.
    pub server: String,
    /// How many times the server's picture has changed, this session.
    ///
    /// Not the picture. The same reasoning as [`Snapshot::messages_revision`],
    /// with the numbers a size smaller: this value is read on every interface
    /// frame — twice a second — and carrying the bytes would mean cloning them,
    /// turning them into JSON, and pushing them across the bridge each time,
    /// for a value that changes when somebody presses a button and never
    /// otherwise. At the protocol ceiling that is 8 KiB of PNG becoming rather
    /// more than that as text, sixty times a minute, for ever.
    ///
    /// The number itself means nothing — only the difference does. A shell
    /// keeps the last one it drew and calls [`crate::Connection::server_icon`] when it
    /// moves. Zero, and never moving, is the ordinary server: it has no picture.
    pub icon_revision: u64,
    /// Sobe quando **qualquer** retrato de pessoa muda.
    ///
    /// A casca guarda o último que desenhou e rebusca os retratos que estão na
    /// tela quando ele muda — o mesmo acordo de [`Snapshot::icon_revision`], e
    /// pela mesma razão: os bytes não atravessam a ponte duas vezes por segundo
    /// por causa de uma imagem que muda quando alguém aperta um botão.
    pub person_icons_revision: u64,
    /// This person's identifier, once known.
    pub me: Option<u64>,
    /// This person's name.
    pub nickname: String,
    /// Voice channels, each carrying who is in it.
    pub voice_rooms: Vec<VoiceRoom>,
    /// Quem está conectado neste servidor, em sala ou fora dela.
    ///
    /// **Não é a soma de [`VoiceRoom::people`]**, e essa diferença é a razão de o
    /// campo existir: quem entra no servidor e fica fora das salas não aparece
    /// em nenhum sala de voz, e por muito tempo não aparecia em lugar nenhum — a
    /// interface listava os sentados e chamava aquilo de «pessoas». A lista de
    /// quem está fora das salas é esta menos aquelas, e a subtração é uma linha
    /// de quem desenha.
    pub presentes: Vec<Person>,
    /// Text channels.
    pub channels: Vec<Channel>,
    /// How many times the conversation has changed, this session.
    ///
    /// Not the conversation itself. This snapshot is read on every interface
    /// frame — twice a second, plus once per event — and carrying the history
    /// meant cloning every nickname and every body already said, serialising
    /// the lot to JSON, and pushing it across the bridge each time. The cost
    /// grows with the conversation, so a long session gets steadily slower to
    /// type into. That is not a tuning problem; it is the shape of the loop,
    /// and it showed up the first time two machines talked for a while.
    ///
    /// The number itself means nothing — only the difference does. A shell
    /// keeps the last one it drew and calls [`Connection::messages`] when it moves.
    /// `seele_core::Changed` already said when that was, and this is the shell
    /// finally being able to act on it.
    pub messages_revision: u64,
    /// Measurements.
    pub telemetry: Telemetry,
    /// The last thing worth surfacing.
    pub notice: Option<Notice>,
    /// Whether this person's microphone is muted.
    pub muted: bool,
    /// Whether this person's speakers are muted.
    pub total_isolation: bool,
    /// Whether this person is transmitting.
    pub speaking: bool,
    /// How the microphone opens.
    pub voice_mode: VoiceMode,
    /// Whether audio is running at all.
    pub audio_available: bool,
    /// The microphone this session is actually capturing from.
    ///
    /// The device that **opened**, not the one that was asked for — those are
    /// different whenever nothing was asked for, which is most of the time. A
    /// screen that drew the request would tell a person their microphone is
    /// called "default".
    ///
    /// `None` when there is no audio, or when the machine opened a device and
    /// then would not describe it. The second is not a failure: audio is
    /// running, and a shell must draw an unnamed device rather than a missing
    /// one. [`Snapshot::audio_available`] is what tells the two apart.
    pub capture: Option<CaptureDevice>,
    /// Where this session is actually playing.
    ///
    /// The device that **opened**, read the same way [`Snapshot::capture`] is
    /// read, and needed more than that one: falling back to the machine's own
    /// speakers makes no sound of its own. A person who picked a headset and
    /// hears nothing has this channel and nothing else to tell them the pick did
    /// not take.
    pub playback: Option<PlaybackDevice>,
    /// Whether this person may make and rename voice_rooms and Channels.
    ///
    /// So a shell can decide whether the control exists at all. `ManageVoiceRooms`
    /// as PERMISSIONS resolved it, sent down in the handshake — a single boolean
    /// and not the permission list, because this is the one a screen asks
    /// about, and a list would invite each shell to start deciding things out
    /// of it.
    ///
    /// **Convenience, never enforcement.** `specs/08-seguranca.md`: "A
    /// interface esconder é conveniência; o servidor negar é a segurança." A
    /// shell that ignores this and asks anyway gets a `NoticeRaised` carrying
    /// `PermissionDenied`, and nothing is created.
    pub may_manage_voice_rooms: bool,
    /// Whether this person may end somebody else's session — `expulsar`.
    ///
    /// One boolean per moderation verb rather than the permission list, for the
    /// reason [`Snapshot::may_manage_voice_rooms`] gives: a list invites each shell to
    /// start deciding things out of it, and four separate controls ask four
    /// separate questions. They are separate on the wire too —
    /// `specs/04-servidor-seele.md` enumerates four permissions and a role may
    /// carry any subset.
    ///
    /// **Convenience, never enforcement.** The app's `EJETAR PLUG DO OPERADOR`
    /// has been drawn and disabled since v2; this is what may enable it, and
    /// the server refusing is still what makes it safe.
    pub may_kick: bool,
    /// Whether this person may bar somebody from returning — `banir`.
    pub may_ban: bool,
    /// Whether this person may take somebody else's message off o canal.
    ///
    /// Only somebody else's: removing one's own needs no permission, so a shell
    /// offering the control on a message the reader wrote does not consult this.
    pub may_remove_message: bool,
    /// Whether this person may move somebody between voice_rooms — `mover_pessoa`.
    pub may_move_person: bool,
    /// Whether this person may name the server and give it a picture.
    ///
    /// `AdministerServer`, which is the same permission behind
    /// [`Snapshot::may_delete_rooms`] today — and a separate field anyway,
    /// because they are separate questions. A shell that read the destroy flag
    /// to decide whether to offer a rename box would be leaning on a
    /// coincidence between two verbs that the roles table can pull apart the
    /// moment somebody makes a role that may dress the server without being able
    /// to destroy what people wrote in it.
    ///
    /// **Convenience, never enforcement**, like every flag beside it.
    pub may_customise_server: bool,
    /// Whether this person may destroy voice_rooms and Channels — `administrar_server`.
    ///
    /// Its own boolean, and deliberately not [`Snapshot::may_manage_voice_rooms`].
    /// Making a room and renaming one are mistakes a server survives; destroying
    /// one ends what other people wrote, and no screen of this product brings
    /// it back. `specs/04-servidor-seele.md` calls `gerenciar_voice_rooms` "criar e
    /// configurar salas de voz" and `administrar_server` "todo o resto sobre o servidor",
    /// so a role that may build rooms without being able to unmake them is a
    /// role somebody can actually write — and a single boolean for both would
    /// make that role impossible to offer correctly.
    ///
    /// **Convenience, never enforcement**, like the five above it.
    pub may_delete_rooms: bool,
    /// A transmissão de tela desta sala de voz, quando há uma.
    ///
    /// `None` quando ninguém está compartilhando **na sala onde esta pessoa
    /// está**. Uma transmissão noutra sala não aparece aqui: o servidor só a
    /// anuncia a quem está lá dentro, e desenhá-la fora seria a casca contando
    /// algo que a sessão não viu.
    pub tela: Option<TelaEmCurso>,
    /// Tudo o que está sendo transmitido na sala em que esta pessoa está.
    ///
    /// **A lista existe porque a sala passou a caber mais de uma.** Com uma só,
    /// [`Self::tela`] respondia tudo: quem transmite, quanto sai, se parou. Com
    /// duas, alguém precisa escolher qual assistir — e para escolher é preciso
    /// saber o que há.
    ///
    /// Vazia quando ninguém transmite, e com **um** elemento no caso comum, que
    /// é quando a casca não desenha lista nenhuma: escolher entre uma coisa não
    /// é escolher.
    pub transmissoes: Vec<TransmissaoNaSala>,
    /// Set once the session is over.
    pub ended: Option<EndReason>,
}

/// Uma transmissão em curso, do ponto de vista de quem pode assisti-la.
///
/// O mínimo para desenhar uma linha de lista e para pedir a imagem: o nome da
/// transmissão e de quem ela é. O que ela está entregando — resolução, quadros,
/// kbps — está em [`TelaEmCurso`], e só para a que está no palco: medir as
/// outras seria medir o que ninguém está recebendo.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TransmissaoNaSala {
    /// Como o servidor batizou a transmissão. É o que se passa a `assistir`.
    pub tela: u32,
    /// Quem está mandando.
    pub de: u64,
    /// Se é esta pessoa — quem transmite não assiste a si mesmo pelo servidor.
    pub e_minha: bool,
}

/// What the shell subscribes to.
///
/// Deliberately coarse: a shell is told *that* the roster moved, then reads
/// [`Snapshot`]. Delivering the change itself would mean every shell
/// reimplementing the fold that `seele_core::state` already does.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Event {
    /// Somebody joined, left, or changed state.
    RosterChanged,
    /// A message arrived, changed, or went away.
    MessagesChanged,
    /// voice_rooms or Channels changed.
    ChannelsChanged,
    /// The server renamed itself, or changed its picture.
    ///
    /// Separate from [`Self::ChannelsChanged`] for the reason
    /// `seele_core::Changed` gives: what moved is the header and the badge, and
    /// a shell that redrew every room because the server was renamed would be
    /// redrawing the one part of the screen that did not change.
    ///
    /// The new name is on the next [`Snapshot`]. The picture is not — read
    /// [`Snapshot::icon_revision`] and fetch it with
    /// [`crate::Connection::server_icon`] when the number moves.
    ServerChanged,
    /// New measurements.
    TelemetryChanged,
    /// Something to surface.
    NoticeRaised {
        /// What it is.
        notice: Notice,
    },
    /// The session is over.
    Ended {
        /// Why.
        reason: EndReason,
    },
    /// Uma chegada mudou de etapa, enquanto ela acontece.
    ///
    /// O único evento desta lista que chega **antes** de haver sessão, e é para
    /// isso que ele existe: `Connection::connect` bloqueia, e quem se inscrevesse
    /// depois dela só teria o `Arc<Connection>` com a travessia inteira já terminada.
    /// Quem quiser este evento entra por `Connection::connect_watching`, que recebe o
    /// ouvinte antes de bloquear.
    ///
    /// Grosso como os outros? Não, e é a exceção: um `Snapshot` não carrega
    /// etapa nenhuma — não há sessão para carregá-lo — então a etapa viaja
    /// dentro do evento ou não viaja.
    ConnectStageChanged {
        /// Onde a chegada está agora.
        stage: crate::ConnectStage,
    },
    /// A file moved, finished moving, or stopped moving. ADR 0027.
    ///
    /// Its own event and not folded into [`Self::MessagesChanged`]: while a
    /// file is going up there is no message yet — the server publishes it only
    /// once the bytes have arrived whole — so there is nothing for a message
    /// event to be about. What the screen has is a bar.
    TransferChanged {
        /// Where it is.
        transfer: Transfer,
    },
    /// A transmissão de tela desta sala começou, acabou, ou mudou de número.
    ///
    /// Grosso como os outros: o que mudou está no próximo [`Snapshot::tela`].
    ///
    /// Acende também quando **só o roster** anda e há uma transmissão em curso,
    /// e isso não é generosidade — é o §5.1. A contagem de espectadores é um
    /// campo de [`TelaEmCurso`], ela é o N que divide o teto, e uma quinta
    /// pessoa entrando muda o que está saindo sem que nenhuma mensagem de tela
    /// tenha chegado. Uma casca que só redesenhasse a tela em
    /// `ScreenShareStarted` mostraria `720p · 4 pessoas assistindo` para uma
    /// sala de seis.
    ScreenChanged,
    /// Chegou uma transmissão de tela e esta versão não sabe lê-la.
    ///
    /// **É o evento que apagava a tela preta.** Quando o cabeçalho de um fluxo
    /// de tela não decodifica, este lado não sabe nem o número da transmissão —
    /// então não há `ScreenClosed` a mandar, e não havia nada. A casca não ficava
    /// sabendo que existiu alguma coisa: nenhum desenho, nenhuma frase, e a
    /// pessoa olhando um retângulo escuro.
    ///
    /// Relatado assim: «quem assiste com uma versão mais velha vê tela preta,
    /// sem mensagem nenhuma».
    ///
    /// O motivo vai como texto porque é para uma pessoa ler, e porque o que o
    /// causou é um formato que este build não conhece — enumerá-lo exigiria
    /// conhecer de antemão o que ainda não foi inventado.
    ScreenUnreadable {
        /// O que o decodificador do cabeçalho respondeu.
        reason: String,
    },
    /// Uma transmissão alheia começou a chegar, e este é o seu tamanho.
    ///
    /// Separado de [`Self::ScreenChanged`], que é grosso e manda reler o
    /// `Snapshot`: este carrega o que **não** está no snapshot, porque é do
    /// fluxo e não da sala — a largura e a altura com que o cabeçalho abriu.
    /// Sem elas a casca não tem como armar o decodificador, e o primeiro quadro
    /// chegaria a uma tela que ainda não sabe de que tamanho é a imagem.
    ScreenOpened {
        /// Qual transmissão.
        screen: u32,
        /// Largura em pixels.
        width: u16,
        /// Altura em pixels.
        height: u16,
    },
    /// Um quadro comprimido de uma transmissão alheia.
    ///
    /// **A exceção à regra de que os eventos são grossos**, e ela se paga: um
    /// quadro não é uma mudança que se lê do `Snapshot` depois, é o conteúdo. A
    /// alternativa seria a casca vir buscar quadro a quadro por comando, o que
    /// custa uma travessia de ida e volta por quadro em vez de meia.
    ///
    /// Os bytes vão em base64 e não como lista de números: o JSON de um `Vec<u8>`
    /// gasta umas quatro vezes mais para dizer a mesma coisa, e um quadro-chave
    /// de 1080p tem 65 KiB.
    ///
    /// Quem decodifica é a janela, com o decodificador do sistema. É o que faz
    /// **assistir não exigir o módulo do Cisco** — só transmitir exige.
    ScreenFrame {
        /// Qual transmissão.
        screen: u32,
        /// Se dá para começar a decodificar por este.
        key: bool,
        /// O quadro em Annex-B, em base64.
        data: String,
    },
    /// A transmissão que estava chegando acabou.
    ScreenClosed {
        /// Qual transmissão.
        screen: u32,
    },
}

/// Where one file is on its way.
///
/// Enumerated, and the shell writes the sentence. The one that has to be
/// written and cannot be inferred is [`Transfer::Fell`]: ADR 0027 has no
/// resumption, so trying again starts from zero, and whoever is waiting has to
/// be told rather than left to discover it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
pub enum Transfer {
    /// Going up.
    Sending {
        /// The message's idempotency key, which is how a screen finds its own.
        client_message_id: u64,
        /// Bytes gone.
        done: u64,
        /// Bytes in total. **Always known** — whoever chose the file knows how
        /// big it is — so this is always a bar and never a bar pretending to
        /// measure something nobody measured.
        total: u64,
    },
    /// Every byte went out. The message appears on the Channel next.
    Sent {
        /// Which message.
        client_message_id: u64,
    },
    /// The server cut the stream: it refused. The reason is still travelling.
    ///
    /// Two variants for one refusal, and it is not a duplication: this one is
    /// what the *sending* end observes — the stream stopped — and it arrives
    /// first, because it is a fact about a socket. [`Self::RefusedBecause`] is
    /// the reason, which comes back on the control stream as an enumerated
    /// value. A screen that waited for the second would show nothing while the
    /// first was already true.
    Refused {
        /// Which message.
        client_message_id: u64,
    },
    /// The server said **why** it refused.
    RefusedBecause {
        /// Which message.
        client_message_id: u64,
        /// The enumerated reason.
        reason: AttachmentRefusal,
    },
    /// A file that was asked for is not coming, and why.
    ///
    /// The expected reason is [`AttachmentRefusal::Expired`]: the bytes were
    /// evicted to keep the server under its ceiling, the row survived, and this
    /// is what turns that row into a sentence on somebody's screen.
    Unavailable {
        /// Which attachment.
        attachment: u64,
        /// The enumerated reason.
        reason: AttachmentRefusal,
    },
    /// The link fell in the middle. **Trying again starts from zero.**
    Fell {
        /// Which message.
        client_message_id: u64,
    },
    /// Coming down.
    Receiving {
        /// Which attachment.
        attachment: u64,
        /// Bytes arrived.
        done: u64,
        /// Bytes in total.
        total: u64,
    },
    /// On the receiver's disk, where they chose.
    Saved {
        /// Which attachment.
        attachment: u64,
        /// Where it went.
        path: String,
    },
    /// It did not save.
    NotSaved {
        /// Which attachment.
        attachment: u64,
    },
}

/// Why a server would not take, or would not hand back, a file.
///
/// Mirrored here rather than re-exported from the wire, like every other
/// enumeration that crosses this boundary: the shape a shell matches on is this
/// crate's promise, and a variant renamed on the wire should break a build here
/// rather than silently change what a screen writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AttachmentRefusal {
    /// The person lacks the permission to attach.
    NotAllowed,
    /// Larger than this server's per-file limit.
    TooLarge {
        /// The largest file this server accepts, in bytes. Carried because "too
        /// big" with no number sends somebody to try again with a file that is
        /// also too big.
        limit: u64,
    },
    /// Every byte of the ceiling is held by transfers already under way.
    NoRoom,
    /// The stream ended before the declared number of bytes arrived.
    SizeMismatch,
    /// The bytes did not hash to what was declared.
    HashDidNotMatch,
    /// Bytes are going up faster than the budget allows.
    RateLimited,
    /// This server is not storing attachments at all.
    Unavailable,
    /// No such attachment, or it is in o canal this person may not read.
    NotFound,
    /// The bytes were evicted to keep the server under its ceiling.
    Expired,
    /// The header was not a header.
    Malformed,
}

/// A file hanging off a message, as a screen sees it.
///
/// ADR 0027. Present **even when the bytes are gone**, which is the whole
/// reason the server keeps the row after deleting the blob: a message that had a
/// picture and now draws as an empty channel leaves nobody able to tell there had
/// been one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Attachment {
    /// What to ask for when saving it.
    pub id: u64,
    /// The name the sender gave it.
    pub file_name: String,
    /// The type the sender claimed. **A claim**, and the shell treats it as one:
    /// only a short list of image types is ever drawn, and everything else is a
    /// name and a size with no preview and no way to open it.
    pub declared_type: String,
    /// How many bytes it was.
    pub byte_size: u64,
    /// Whether the bytes are still on the server.
    pub expired: bool,
}

/// What a window may draw for one attachment, and why. ADR 0027.
///
/// The answer to one press of one button, and never anything a screen gets by
/// scrolling: the file lives on the server, so looking at it is downloading it,
/// and o canal that fetched every picture as it scrolled past would turn the
/// host's disk ceiling into everybody's uplink.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Preview {
    /// Which attachment this is about.
    pub attachment: u64,
    /// The picture, whole, as a `data:` URI — or nothing.
    ///
    /// **Composed on this side of the boundary, media type included, and the
    /// media type came from the bytes.** A shell handed the bytes and a type to
    /// join up would be a shell that could join them up with the sender's
    /// claim, which is the one thing the whole design refuses.
    pub image: Option<String>,
    /// The type the sender claimed, so a sentence can quote it back.
    pub claimed: String,
    /// What the leading bytes turned out to be, when they are a picture this
    /// product knows. `None` means they are not one.
    pub found: Option<String>,
    /// Why there is no picture. `None` exactly when [`Self::image`] is `Some`.
    pub refusal: Option<PreviewRefusal>,
}

/// What a screen may offer a preview for, before it asks for one.
///
/// Both halves are advice and neither is the rule: the rule is applied where
/// the bytes are, on the way down. A window that offers nothing here still
/// cannot draw anything it should not, and a window that offers too much gets
/// a refusal instead of a picture.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PreviewRules {
    /// The most a preview will pull down, in bytes.
    pub limit: u64,
    /// The media types that could be drawn, if their bytes agree.
    pub types: Vec<String>,
}

/// Why a file was not drawn.
///
/// Four, and they are four because they ask different things of the person
/// reading. One says the file is fine and this window will not spend the memory;
/// another says the file is not what it claimed to be, which is not a transfer
/// problem and not something to retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
pub enum PreviewRefusal {
    /// The bytes are not what the file said it was.
    ///
    /// `NOTAS-DE-RELEASE.md` separates "did it arrive whole" from "is it what
    /// it says it is". The hash answered the first, with a yes. This is the
    /// second, with a no — and not drawing is not the same as hiding: the file
    /// is still on the screen, with its name, its size and its save button.
    Disagrees,
    /// Larger than this window will decode.
    TooBig {
        /// The most a preview will pull down, in bytes. Carried for the same
        /// reason [`AttachmentRefusal::TooLarge`] carries one: "too big" with
        /// no number is a sentence nobody can act on.
        limit: u64,
    },
    /// Never a picture in the first place — a document, a build, a text file.
    ///
    /// A window should not have asked, and this is what it gets if it did.
    NotAPicture,
    /// The bytes never came: expired, gone, or they did not arrive whole.
    DidNotArrive,
}

/// Onde o enlace está.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LinkState {
    /// Conectado e respondendo.
    Online,
    /// Bateria interna: caiu, e a sessão está sendo segurada.
    InternalBattery {
        /// Segundos que faltam dos cinco minutos.
        remaining_seconds: u64,
        /// Tentativas de reconexão até agora.
        attempts: u32,
    },
}

/// Everything that can go wrong, enumerated.
///
/// `specs/02-protocolo.md` refuses generic error strings on the wire, and the
/// same reasoning applies here: a variant carrying free text is a variant the
/// shell cannot localise and must print verbatim. Detail that helps a developer
/// goes to `tracing`, not across this boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ConnectionError {
    /// No connection, or it is already gone.
    NotConnected,
    /// A connection is already open on this handle.
    AlreadyConnected,
    /// The address could not be resolved.
    UnresolvableHost,
    /// Nothing answered.
    Unreachable,
    /// The handshake did not finish in time.
    HandshakeTimeout,
    /// The server's key is not the one that was pinned.
    ///
    /// ADR 0003. The two fingerprints are carried because the whole point is
    /// that a human compares them; they are data, not an error message.
    PinChanged {
        /// What was pinned before.
        pinned: String,
        /// What was offered now.
        offered: String,
    },
    /// The invite named one key and the server offered another.
    ///
    /// ADR 0006, and deliberately not [`ConnectionError::PinChanged`]: nothing was
    /// ever pinned here, so the shell's key-change alarm would name the wrong
    /// culprit. What failed is the link, not the server's continuity.
    InviteMismatch {
        /// What the link promised.
        expected: String,
        /// What the server offered.
        offered: String,
    },
    /// The server refused the session, and said why.
    Refused {
        /// The enumerated reason.
        reason: EndReason,
    },
    /// The identity on disk could not be read or written.
    IdentityUnavailable,
    /// No usable microphone or speaker.
    NoAudioDevice,
    /// The chosen microphone is not one this machine is offering any more.
    ///
    /// Its own variant rather than [`ConnectionError::NoAudioDevice`] because the two
    /// ask different things of the person reading them: "this machine has no
    /// microphone" and "the one you picked was unplugged" have different next
    /// steps, and the second one has a list to pick from again.
    CaptureDeviceGone,
    /// The chosen output is not one this machine is offering any more.
    ///
    /// Its own variant rather than [`ConnectionError::CaptureDeviceGone`] because a
    /// shell writes one sentence per variant, and "the microphone you picked was
    /// unplugged" is the wrong sentence to read after choosing a headset. The
    /// two point at opposite halves of the screen.
    PlaybackDeviceGone,
    /// The named person is not in this voice room.
    UnknownPerson,
    /// No voice room or Channel by that name or number.
    UnknownChannel,
    /// The control stream broke.
    LinkLost,
    /// The file offered as the server's icon is not a picture that fits there.
    ///
    /// Answered **before** anything is sent, by
    /// `seele_core::check_server_icon`, and that is the whole reason this
    /// variant exists: the rule lives on the wire, and a picture that breaks it
    /// makes the frame unbuildable — which everything above the link reads as a
    /// dropped connection. Without this, choosing a PDF would look exactly like
    /// the network falling over, five-minute internal battery included.
    IconNotAPicture,
    /// The picture offered as the server's icon is heavier than a server takes.
    ///
    /// Separate from [`ConnectionError::IconNotAPicture`] because the next step
    /// differs: a photograph can be shrunk, and a PDF cannot be made into an
    /// icon. The ceiling travels with it so the sentence can carry the number
    /// — a shell has no other way to name a limit that lives in the protocol.
    IconTooBig {
        /// The most bytes a server accepts.
        limit_bytes: u64,
    },
    /// Alguém já está compartilhando a tela nesta sala de voz.
    ///
    /// Uma transmissão por sala (§6.3 da spec de tela). **Não é permissão**: a
    /// pessoa pode compartilhar assim que a outra parar, e dizer
    /// [`ConnectionError::Refused`] ou o `PermissionDenied` de um aviso a mandaria
    /// procurar um papel que ela já tem.
    ///
    /// Quem decide é o servidor, e por isso este veredito chega **também** — hoje,
    /// só — pelo caminho assíncrono, como [`NoticeReason::ScreenShareTaken`]
    /// num [`Event::NoticeRaised`]. A variante existe aqui para o dia em que o
    /// pedido tiver resposta síncrona; esta ponte nunca a devolve por conta
    /// própria, porque decidir localmente quem perdeu a corrida seria a casca
    /// julgando no lugar do servidor.
    ScreenShareTaken,
    /// Esta máquina não tem como começar a compartilhar.
    ///
    /// **Não é uma recusa do servidor e não é rede.** É uma das quatro coisas que
    /// faltam **aqui**, e três delas a pessoa consegue mudar:
    ///
    /// 1. o módulo do Cisco não está em disco. O produto não vem com codec, e é
    ///    a licença que impõe isso — há um endereço de download, e o log da
    ///    ponte o escreve;
    /// 2. o sistema não concedeu gravação de tela. Aí quem responde é
    ///    [`PermissaoDeTela`], e o botão a oferecer é o de pedir;
    /// 3. a fonte escolhida não está na última lista — a janela fechou entre o
    ///    menu e o clique. Listar de novo resolve;
    /// 4. este build não tem captura de tela (o Linux da v1). Só este último não
    ///    tem conserto do lado de quem usa, e é o que
    ///    [`PermissaoDeTela::NaoSeSabe`] diz.
    ///
    /// Também é o que `Connection::ajustar_limites_da_tela` devolve, e ali o motivo
    /// é outro: a bomba não tem ordem que troque a escolha de resolução ou de
    /// cadência depois de armada. Parar e recomeçar aplica as três.
    ScreenShareUnavailable,
    /// O módulo de vídeo não está nesta máquina.
    ///
    /// **Não é o mesmo que [`Self::ScreenShareUnavailable`]**, e confundir os
    /// dois já custou dois testes de campo: aquele diz «esta compilação não
    /// captura tela», e este diz «captura, e falta um arquivo».
    ///
    /// O módulo do OpenH264 não vem no pacote por decisão de licença — a
    /// cobertura de patente do Cisco acompanha o binário que o Cisco entrega,
    /// então embrulhá-lo junto seria trocar uma patente respondida por uma
    /// contornada. O que resta é buscá-lo, uma vez, com consentimento e com o
    /// hash conferido.
    ///
    /// Quem lê a frase errada conclui que o recurso não existe e para de
    /// tentar. Quem lê esta sabe que falta um megabyte e o que fazer com isso.
    ScreenModuleMissing,
    /// O download veio, e o módulo não ficou em disco.
    ///
    /// Hash que não bate, bz2 que não abre, pasta que não aceita gravação. São
    /// três causas e uma frase só, porque a ação de quem clicou é a mesma nas
    /// três — tentar de novo — e o que as separa está no log de quem depura.
    ///
    /// Um hash que não bate quase nunca é corrupção: é uma página de erro de
    /// proxy chegando no lugar do arquivo.
    ScreenModuleRefused,
}

impl std::fmt::Display for ConnectionError {
    /// For logs and `Error`, never for a user.
    ///
    /// A shell that prints this is a shell that skipped writing its own
    /// sentence, which is what the enum exists to make it do.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ConnectionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_disconnect_reason_the_protocol_has_maps_to_one_here() {
        // If the protocol grows a reason, this fails to compile rather than
        // quietly folding the new one into something else.
        let all = [
            seele_core::DisconnectReason::Incompatible,
            seele_core::DisconnectReason::CredentialRejected,
            seele_core::DisconnectReason::HandshakeTimeout,
            seele_core::DisconnectReason::Kicked,
            seele_core::DisconnectReason::Banned,
            seele_core::DisconnectReason::ServerFull,
            seele_core::DisconnectReason::ScheduledMaintenance,
            seele_core::DisconnectReason::ServerShuttingDown,
            seele_core::DisconnectReason::Timeout,
            seele_core::DisconnectReason::ProtocolViolation,
            seele_core::DisconnectReason::RateLimited,
            seele_core::DisconnectReason::FellBehind,
            seele_core::DisconnectReason::AdmissionPending,
            seele_core::DisconnectReason::AdmissionDenied,
        ];
        let mapped: std::collections::HashSet<EndReason> =
            all.into_iter().map(EndReason::from).collect();

        assert_eq!(
            mapped.len(),
            all.len(),
            "two protocol reasons collapsed into one, and a shell can no longer tell them apart"
        );
        assert!(
            !mapped.contains(&EndReason::LinkLost),
            "LinkLost is for a link that died without the server saying why"
        );
    }

    #[test]
    fn every_alert_reason_maps_to_one_of_its_own() {
        let all = [
            seele_core::AlertReason::Mentioned,
            seele_core::AlertReason::SubsystemChanged,
            seele_core::AlertReason::SyncDegraded,
            seele_core::AlertReason::VoiceRoomEntryRefused,
            seele_core::AlertReason::PermissionDenied,
            seele_core::AlertReason::VoiceRoomFull,
            seele_core::AlertReason::OperatorNotice,
            seele_core::AlertReason::RateLimited,
            seele_core::AlertReason::MovedByOperator,
            seele_core::AlertReason::VoiceRoomDeleted,
            seele_core::AlertReason::ChannelDeleted,
            seele_core::AlertReason::LastVoiceRoom,
        ];
        let mapped: std::collections::HashSet<NoticeReason> =
            all.into_iter().map(NoticeReason::from).collect();
        assert_eq!(mapped.len(), all.len());
    }

    #[test]
    fn the_bands_agree_with_the_core() {
        // Two shells with two copies of "85 is nominal" is two shells that
        // disagree the day one of them is updated. The core decides; this only
        // relabels.
        for ratio in 0..=100u8 {
            let band: SignalBand = seele_core::SignalBand::of(ratio).into();
            let expected = match ratio {
                85..=255 => SignalBand::Nominal,
                60..=84 => SignalBand::Degraded,
                _ => SignalBand::Critical,
            };
            assert_eq!(band, expected, "ratio {ratio}");
        }
    }

    #[test]
    fn the_band_a_shell_reads_is_spelled_the_way_the_comp_bands_it() {
        // The desktop shell branches on this string. The edges are what a
        // shell would get wrong if it kept its own thresholds, so the edges
        // are what is pinned — 84 and 85, 59 and 60.
        let json = |ratio: u8| {
            let band: SignalBand = seele_core::SignalBand::of(ratio).into();
            serde_json::to_string(&band).expect("a bare enum always serialises")
        };

        assert_eq!(json(85), "\"Nominal\"");
        assert_eq!(json(84), "\"Degraded\"");
        assert_eq!(json(60), "\"Degraded\"");
        assert_eq!(json(59), "\"Critical\"");
        assert_eq!(json(0), "\"Critical\"");
        assert_eq!(json(100), "\"Nominal\"");

        // And nothing anywhere in the range still says the fourth band's name.
        for ratio in 0..=100u8 {
            assert_ne!(json(ratio), "\"Acceptable\"", "ratio {ratio}");
        }
    }

    #[test]
    fn every_verdict_the_core_produces_has_a_shell_facing_twin() {
        // Trust used to have two variants where the core now has five, and
        // folding five into two would throw away exactly the information this
        // work exists to create.
        use seele_core::tofu::Verdict;

        let cases = [
            Verdict::FirstContact {
                fingerprint: "a".into(),
            },
            Verdict::FirstContactVerified {
                fingerprint: "a".into(),
            },
            Verdict::Known,
            Verdict::InviteRefused {
                expected: "b".into(),
                offered: "a".into(),
            },
            Verdict::InviteDisagrees {
                expected: "b".into(),
                offered: "a".into(),
            },
        ];

        let seen: std::collections::BTreeSet<String> = cases
            .iter()
            .map(|verdict| format!("{:?}", Trust::from(verdict.clone())))
            .collect();

        assert_eq!(
            seen.len(),
            cases.len(),
            "two verdicts collapsed into one Trust"
        );
    }

    #[test]
    fn no_error_variant_carries_free_text() {
        // The two that carry strings carry *fingerprints*, which are data a
        // human compares — not a sentence a shell would have to print.
        let pin = ConnectionError::PinChanged {
            pinned: "aaa".into(),
            offered: "bbb".into(),
        };
        let ConnectionError::PinChanged { pinned, offered } = &pin else {
            panic!("shape changed");
        };
        assert_ne!(pinned, offered);

        // Everything else is a bare variant, which is what makes it localisable.
        assert_eq!(format!("{}", ConnectionError::Unreachable), "Unreachable");
    }

    /// The snapshot must stay cheap, and "cheap" means it carries no list that
    /// grows with the session.
    ///
    /// This reads the source because the property is invisible to the compiler
    /// and to any assertion about one value: a `Snapshot` carrying the whole
    /// conversation type-checks, serialises, and passes every other test here.
    /// What it does *not* do is stay the same size as a session goes on — and
    /// that only shows up as a person noticing the app got slower to type into,
    /// after a long conversation, on two machines. It took a real test between
    /// two machines to find it the first time, and the cost of finding it that
    /// way again is another release.
    ///
    /// The narrow rule: no `Vec<…>` field here may hold [`Message`]. Anything
    /// else that grows without bound belongs to the same family, but a rule
    /// written wider than the thing it guards is a rule people argue with.
    #[test]
    fn the_snapshot_does_not_carry_the_conversation() {
        let source = include_str!("types.rs");
        let Some(after) = source.split("pub struct Snapshot {").nth(1) else {
            panic!("`Snapshot` is no longer declared here; this guard has to move with it");
        };
        let Some(body) = after.split("\n}").next() else {
            panic!("`Snapshot` is never closed");
        };

        let campos: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|channel| !channel.starts_with("//") && channel.contains(known_field_marker()))
            .collect();

        for campo in &campos {
            assert!(
                !campo.contains("Vec<Message>"),
                "`Snapshot` carries the conversation again: `{campo}`\n\
                 Reading this costs one clone of every nickname and body already \
                 said, on every interface frame — twice a second — so a long \
                 session gets steadily slower. Use `messages_revision` and \
                 `Connection::messages`, which exist for exactly this."
            );
            assert!(
                !campo.contains("Vec<u8>"),
                "`Snapshot` carries raw bytes: `{campo}`\n\
                 The one thing that would be, today, is the Server\u{2019}s icon, and it \
                 is bounded rather than unbounded — but it is still kilobytes \
                 cloned and turned into JSON twice a second for a value that \
                 changes when somebody presses a button. Use `icon_revision` and \
                 `Connection::server_icon`, which are the same answer one size smaller."
            );
        }

        assert!(
            campos.iter().any(|campo| campo.contains("icon_revision")),
            "`icon_revision` is gone, and with it the only way a shell can tell \
             that the Server\u{2019}s picture moved without being handed it on every frame"
        );

        assert!(
            campos
                .iter()
                .any(|campo| campo.contains("messages_revision")),
            "`messages_revision` is gone, and with it the only way a shell can \
             tell that the history moved without being handed all of it"
        );
    }

    #[test]
    fn os_limites_da_tela_atravessam_pelo_nome() {
        // Este é o único valor deste módulo que viaja **para dentro**: a casca
        // gráfica o escreve em JSON e o Tauri o desserializa aqui. Um campo
        // renomeado de um lado só compila nos dois lados e falha em silêncio —
        // e o que a pessoa vê é um teto de banda que não pega.
        let escrito = r#"{"banda_bps":1200000,"altura_maxima":720,"quadros_maximos":15}"#;
        let limites: LimitesDeTela =
            serde_json::from_str(escrito).expect("a casca escreve exatamente estes três nomes");
        assert_eq!(limites.banda_bps, Some(1_200_000));
        assert_eq!(limites.altura_maxima, 720);
        assert_eq!(limites.quadros_maximos, 15);

        // «Sem limite próprio» é `null`, e não zero: zero seria um teto de zero
        // bits por segundo, que é a mesma conta que parar.
        let sem_teto: LimitesDeTela =
            serde_json::from_str(r#"{"banda_bps":null,"altura_maxima":1080,"quadros_maximos":30}"#)
                .expect("`null` é como a casca diz «sem limite próprio»");
        assert_eq!(sem_teto.banda_bps, None);
    }

    #[test]
    fn a_tela_em_curso_atravessa_pelo_nome_e_diz_o_que_nao_sabe() {
        let tela = TelaEmCurso {
            de: 3,
            e_minha: false,
            altura: 0,
            quadros: 0,
            kbps: 0,
            espectadores: 6,
            parada: None,
            medida: false,
            pedido: Some(LimitesDeTela {
                banda_bps: Some(1_200_000),
                altura_maxima: 1080,
                quadros_maximos: 30,
                prioridade: Prioridade::Nitidez,
            }),
        };
        let json = serde_json::to_string(&tela).expect("uma estrutura simples sempre serializa");

        for nome in [
            "\"de\"",
            "\"e_minha\"",
            "\"altura\"",
            "\"quadros\"",
            "\"kbps\"",
            "\"espectadores\"",
            "\"parada\"",
            "\"medida\"",
            "\"pedido\"",
        ] {
            assert!(json.contains(nome), "{nome} não atravessa: {json}");
        }

        // O campo que impede a casca de escrever `0p · 0 quadros` sobre uma
        // transmissão que ninguém mediu. Ver o doc de `TelaEmCurso::medida`.
        assert!(
            json.contains("\"medida\":false"),
            "a ponte diz ter medido o que não mediu: {json}"
        );

        // E o pedido atravessa com os **mesmos três nomes** com que a casca o
        // escreveu ao mandá-lo (`LimitesDeTela`, logo acima). Renomear um deles
        // aqui deixaria a coluna do que foi pedido vazia sem nada falhar: os
        // dois lados continuariam compilando e a comparação do §5 sumiria da
        // tela.
        for nome in ["\"banda_bps\"", "\"altura_maxima\"", "\"quadros_maximos\""] {
            assert!(
                json.contains(nome),
                "{nome} não atravessa dentro do pedido: {json}"
            );
        }
    }

    #[test]
    fn quem_so_assiste_nao_recebe_um_pedido_que_nao_e_dele() {
        // O teto é escolha de quem compartilha, e ela não viaja: o
        // `ScreenHeader` carrega resolução e codec, nunca o que a pessoa pediu.
        // Preencher esta coluna para quem assiste só seria possível copiando a
        // medida para o lugar da escolha — e aí as duas metades que o §5 manda
        // pôr lado a lado passariam a ser o mesmo número, sempre iguais, sempre
        // dizendo que o teto nunca apertou.
        let alheia = TelaEmCurso {
            de: 3,
            e_minha: false,
            altura: 0,
            quadros: 0,
            kbps: 0,
            espectadores: 6,
            parada: None,
            medida: false,
            pedido: None,
        };
        let json = serde_json::to_string(&alheia).expect("uma estrutura simples sempre serializa");
        assert!(
            json.contains("\"pedido\":null"),
            "o pedido de outra pessoa foi inventado deste lado: {json}"
        );
    }

    #[test]
    fn a_permissao_de_tela_atravessa_com_os_nomes_que_a_casca_ramifica() {
        let nome = |permissao: PermissaoDeTela| {
            serde_json::to_string(&permissao).expect("um enum simples sempre serializa")
        };
        assert_eq!(nome(PermissaoDeTela::Concedida), "\"Concedida\"");
        assert_eq!(nome(PermissaoDeTela::Negada), "\"Negada\"");
        assert_eq!(nome(PermissaoDeTela::NaoPerguntada), "\"NaoPerguntada\"");
        // A quarta, que não estava no contrato de 22/08. Uma casca que a
        // tratasse como `NaoPerguntada` desenharia um botão de pedir permissão
        // que não tem a quem pedir.
        assert_eq!(nome(PermissaoDeTela::NaoSeSabe), "\"NaoSeSabe\"");
    }

    #[test]
    fn a_tela_tem_evento_proprio_no_barramento() {
        // Separado de `RosterChanged` pelo motivo que `seele_core::Changed` já
        // dá: o que se mexe é um painel, e uma casca que redesenhasse a lista de
        // pessoas inteira por causa de um número de kbps estaria redesenhando a
        // parte da tela que não mudou.
        let json = serde_json::to_string(&Event::ScreenChanged)
            .expect("uma variante sem campo sempre serializa");
        assert_eq!(json, "\"ScreenChanged\"");
    }

    #[test]
    fn a_tela_tomada_nao_e_uma_recusa_de_permissao() {
        // Duas variantes, e a distinção é o produto inteiro: quem perde a
        // corrida pode compartilhar assim que o outro parar, e `PermissionDenied`
        // a mandaria procurar um papel que ela já tem.
        assert_ne!(
            serde_json::to_string(&ConnectionError::ScreenShareTaken).ok(),
            serde_json::to_string(&ConnectionError::ScreenShareUnavailable).ok(),
            "as duas respostas de tela viraram a mesma, e as saídas delas são opostas"
        );
        assert_ne!(
            NoticeReason::ScreenShareTaken,
            NoticeReason::PermissionDenied
        );
    }

    /// What a field declaration looks like, so a doc channel is not mistaken for one.
    fn known_field_marker() -> &'static str {
        "pub "
    }
}
