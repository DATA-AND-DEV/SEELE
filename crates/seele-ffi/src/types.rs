//! The values that cross the boundary.
//!
//! Everything here is plain: `u64`, `u32`, `String`, `bool`, `Vec`, and closed
//! enums. **No `seele-proto` newtype appears in this module**, and that is the
//! point rather than an oversight.
//!
//! `specs/06-clientes-gui.md` states the rule in one sentence: "Se o frontend
//! precisa saber o que é um `ssrc`, algo está errado." A `PilotId` crossing
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

/// How far the connection has got. `specs/07-tema-evangelion.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Pattern {
    /// Not connected.
    Offline,
    /// Connected, not verified.
    Orange,
    /// Verified. Normal operation.
    Blue,
}

/// Which band a Sync Ratio falls into.
///
/// Carried as an enum beside the number so a shell never has to know the
/// thresholds. Two shells with two copies of "85 is nominal" is two shells that
/// disagree the day one of them is updated.
///
/// Three bands, from `design/Entry Plug v2.dc.html`. The fourth — `Acceptable`
/// — is gone, and it is gone from the JSON the webview reads too: a shell that
/// still branches on `"Acceptable"` now falls through to whatever it does with
/// an unknown band, and never to a colour it drew last week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum SyncBand {
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

impl From<seele_core::SyncBand> for SyncBand {
    fn from(band: seele_core::SyncBand) -> Self {
        match band {
            seele_core::SyncBand::Nominal => Self::Nominal,
            seele_core::SyncBand::Degraded => Self::Degraded,
            seele_core::SyncBand::Critical => Self::Critical,
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
    /// The pilot was named in a message.
    Mentioned,
    /// A subsystem changed health.
    SubsystemChanged,
    /// The connection is struggling.
    SyncDegraded,
    /// Entry to a Cage was refused.
    CageEntryRefused,
    /// The action needed a permission the pilot lacks.
    PermissionDenied,
    /// The Cage is at its limit.
    CageFull,
    /// The operator is saying something.
    OperatorNotice,
    /// The client is sending control frames faster than its budget.
    RateLimited,
    /// An operator moved this pilot's plug into another Cage.
    MovedByOperator,
    /// The Cage this pilot's plug was in no longer exists.
    CageDeleted,
    /// A Line this pilot had open no longer exists.
    LineDeleted,
    /// The Cage asked about is the only one the Dogma has, so it stays.
    LastCage,
}

impl From<seele_core::AlertReason> for NoticeReason {
    fn from(reason: seele_core::AlertReason) -> Self {
        match reason {
            seele_core::AlertReason::Mentioned => Self::Mentioned,
            seele_core::AlertReason::SubsystemChanged => Self::SubsystemChanged,
            seele_core::AlertReason::SyncDegraded => Self::SyncDegraded,
            seele_core::AlertReason::CageEntryRefused => Self::CageEntryRefused,
            seele_core::AlertReason::PermissionDenied => Self::PermissionDenied,
            seele_core::AlertReason::CageFull => Self::CageFull,
            seele_core::AlertReason::OperatorNotice => Self::OperatorNotice,
            seele_core::AlertReason::RateLimited => Self::RateLimited,
            seele_core::AlertReason::MovedByOperator => Self::MovedByOperator,
            seele_core::AlertReason::CageDeleted => Self::CageDeleted,
            seele_core::AlertReason::LineDeleted => Self::LineDeleted,
            seele_core::AlertReason::LastCage => Self::LastCage,
        }
    }
}

/// What a Line holds, as the confirmation in front of destroying it needs it.
///
/// Counted in the Dogma's database at the instant of asking, and carried across
/// the bridge unrounded. A shell cannot work these out for itself: it holds one
/// page of history and would guess low by whatever the Line's whole past is,
/// and a number that is nearly right in a box promising to destroy 1.847
/// messages is worse than no number at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct LineWeight {
    /// Which Line was weighed.
    pub line: u32,
    /// How many messages are in it that anybody can read.
    pub messages: u32,
    /// How many distinct pilots wrote them.
    pub authors: u32,
    /// When the oldest was written, in seconds since the Unix epoch.
    ///
    /// `None` when the Line is empty — the one case where the sentence has no
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
    /// An operator disconnected this pilot.
    Kicked,
    /// An operator barred this pilot.
    Banned,
    /// The Dogma is full.
    DogmaFull,
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
    /// This connection fell behind the Dogma's events and lost some.
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
            seele_core::DisconnectReason::DogmaFull => Self::DogmaFull,
            seele_core::DisconnectReason::ScheduledMaintenance => Self::ScheduledMaintenance,
            seele_core::DisconnectReason::ServerShuttingDown => Self::ServerShuttingDown,
            seele_core::DisconnectReason::Timeout => Self::Timeout,
            seele_core::DisconnectReason::ProtocolViolation => Self::ProtocolViolation,
            seele_core::DisconnectReason::RateLimited => Self::RateLimited,
            seele_core::DisconnectReason::FellBehind => Self::FellBehind,
            seele_core::DisconnectReason::AdmissionPending => Self::AdmissionPending,
            seele_core::DisconnectReason::AdmissionDenied => Self::AdmissionDenied,
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
    /// line it may ignore.
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

/// One pilot in a Cage.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Pilot {
    /// Stable identifier for this pilot on this Dogma.
    pub id: u64,
    /// Display name.
    pub nickname: String,
    /// Transmitting right now.
    pub speaking: bool,
    /// Microphone muted — A.T. Field.
    pub at_field: bool,
    /// Speakers muted — Isolamento total.
    pub total_isolation: bool,
    /// Sync Ratio, 0 to 100.
    pub sync_ratio: u8,
    /// Which band that falls into, decided once, in the core.
    pub sync_band: SyncBand,
    /// Whether this is the pilot holding the handle.
    pub is_self: bool,
}

/// The average Sync Ratio of a Cage, already banded — **MÉDIA DO CAGE**.
///
/// The comp computes this in the shell and colours it there. It is computed in
/// the core instead, for the reason at the top of this module: a shell that
/// knows the thresholds is a shell that will disagree with the other one. What
/// arrives here is the number, the band and the size of the sample, and drawing
/// is all that is left to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct CageSync {
    /// The average, 0 to 100, rounded to a whole point.
    ///
    /// The comp prints `82.4`; the datum is a `u8` at every point it exists, so
    /// this is `82`. A decimal here would be precision invented at the last
    /// step, not carried from the measurement.
    pub ratio: u8,
    /// Which band the average falls into.
    pub band: SyncBand,
    /// How many pilots it averages — the comp's `5 PLUGS`.
    pub pilots: u32,
}

impl From<seele_core::CageSync> for CageSync {
    fn from(sync: seele_core::CageSync) -> Self {
        Self {
            ratio: sync.ratio,
            band: sync.band.into(),
            // A Cage holds at most `limit` pilots and `limit` is a `u16`. The
            // saturation is unreachable and is here so the conversion cannot
            // panic on a server that says otherwise.
            pilots: u32::try_from(sync.pilots).unwrap_or(u32::MAX),
        }
    }
}

/// A voice channel.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Cage {
    /// Identifier.
    pub id: u32,
    /// Display name.
    pub name: String,
    /// How many pilots fit.
    pub limit: u16,
    /// Whether entry needs a password.
    pub password_required: bool,
    /// Whether this pilot's plug is in it.
    pub occupied_by_us: bool,
    /// The Line bound to it, if any. `specs/04-servidor-seele.md` makes the
    /// association optional.
    ///
    /// Carried so a shell can say what destroying that Line would do to this
    /// room. The Cage survives it and comes out with no Line attached, which is
    /// a change nobody asked for — and a product whose confirmations name their
    /// consequences has to be able to name that one.
    pub line: Option<u32>,
    /// Who is inside, in arrival order.
    pub pilots: Vec<Pilot>,
    /// The average Sync Ratio of everybody inside, or `None` if nobody is.
    ///
    /// `None` rather than zero: an empty Cage has nothing to average, and zero
    /// would draw every idle room in the critical colour.
    pub sync: Option<CageSync>,
}

/// A text channel.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Line {
    /// Identifier.
    pub id: u32,
    /// Display name.
    pub name: String,
    /// Whether this is the Line being read.
    pub open: bool,
}

/// One thing somebody said.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Message {
    /// Server-assigned identifier. Ordered; the clock is not.
    pub id: u64,
    /// Which Line.
    pub line: u32,
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
    /// This pilot's Sync Ratio, 0 to 100.
    pub sync_ratio: u8,
    /// Which band that is.
    pub sync_band: SyncBand,
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
    /// is the network's, reported by the Dogma. `local_fault` is this machine's
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
    /// `specs/07-tema-evangelion.md` manda a interface não fechar quando a
    /// conexão cai — ela esmaece, conta cinco minutos e continua legível. Sem
    /// este campo a casca gráfica não tinha como saber disso e pulava direto
    /// para "encerrado", que é a simplificação que a spec proíbe.
    pub link: LinkState,
    /// How far the connection has got.
    pub pattern: Pattern,
    /// What the Dogma is called.
    ///
    /// Follows a rename **inside the session** now: the Dogma announces it to
    /// everybody connected, so a shell that redraws its header on
    /// [`Event::DogmaChanged`] never shows a name the server stopped using.
    pub dogma: String,
    /// How many times the Dogma's picture has changed, this session.
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
    /// keeps the last one it drew and calls [`crate::Plug::dogma_icon`] when it
    /// moves. Zero, and never moving, is the ordinary Dogma: it has no picture.
    pub icon_revision: u64,
    /// This pilot's identifier, once known.
    pub me: Option<u64>,
    /// This pilot's name.
    pub nickname: String,
    /// Voice channels, each carrying who is in it.
    pub cages: Vec<Cage>,
    /// Text channels.
    pub lines: Vec<Line>,
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
    /// keeps the last one it drew and calls [`Plug::messages`] when it moves.
    /// `seele_core::Changed` already said when that was, and this is the shell
    /// finally being able to act on it.
    pub messages_revision: u64,
    /// Measurements.
    pub telemetry: Telemetry,
    /// The last thing worth surfacing.
    pub notice: Option<Notice>,
    /// Whether this pilot's microphone is muted.
    pub at_field: bool,
    /// Whether this pilot's speakers are muted.
    pub total_isolation: bool,
    /// Whether this pilot is transmitting.
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
    /// hears nothing has this line and nothing else to tell them the pick did
    /// not take.
    pub playback: Option<PlaybackDevice>,
    /// Whether this pilot may make and rename Cages and Lines.
    ///
    /// So a shell can decide whether the control exists at all. `ManageCages`
    /// as MELCHIOR resolved it, sent down in the handshake — a single boolean
    /// and not the permission list, because this is the one a screen asks
    /// about, and a list would invite each shell to start deciding things out
    /// of it.
    ///
    /// **Convenience, never enforcement.** `specs/08-seguranca.md`: "A
    /// interface esconder é conveniência; o servidor negar é a segurança." A
    /// shell that ignores this and asks anyway gets a `NoticeRaised` carrying
    /// `PermissionDenied`, and nothing is created.
    pub may_manage_cages: bool,
    /// Whether this pilot may end somebody else's session — `expulsar`.
    ///
    /// One boolean per moderation verb rather than the permission list, for the
    /// reason [`Snapshot::may_manage_cages`] gives: a list invites each shell to
    /// start deciding things out of it, and four separate controls ask four
    /// separate questions. They are separate on the wire too —
    /// `specs/04-servidor-seele.md` enumerates four permissions and a role may
    /// carry any subset.
    ///
    /// **Convenience, never enforcement.** The app's `EJETAR PLUG DO OPERADOR`
    /// has been drawn and disabled since v2; this is what may enable it, and
    /// the server refusing is still what makes it safe.
    pub may_kick: bool,
    /// Whether this pilot may bar somebody from returning — `banir`.
    pub may_ban: bool,
    /// Whether this pilot may take somebody else's message off a Line.
    ///
    /// Only somebody else's: removing one's own needs no permission, so a shell
    /// offering the control on a message the reader wrote does not consult this.
    pub may_remove_message: bool,
    /// Whether this pilot may move somebody between Cages — `mover_piloto`.
    pub may_move_pilot: bool,
    /// Whether this pilot may name the Dogma and give it a picture.
    ///
    /// `AdministerDogma`, which is the same permission behind
    /// [`Snapshot::may_delete_rooms`] today — and a separate field anyway,
    /// because they are separate questions. A shell that read the destroy flag
    /// to decide whether to offer a rename box would be leaning on a
    /// coincidence between two verbs that the roles table can pull apart the
    /// moment somebody makes a role that may dress the Dogma without being able
    /// to destroy what people wrote in it.
    ///
    /// **Convenience, never enforcement**, like every flag beside it.
    pub may_customise_dogma: bool,
    /// Whether this pilot may destroy Cages and Lines — `administrar_dogma`.
    ///
    /// Its own boolean, and deliberately not [`Snapshot::may_manage_cages`].
    /// Making a room and renaming one are mistakes a Dogma survives; destroying
    /// one ends what other people wrote, and no screen of this product brings
    /// it back. `specs/04-servidor-seele.md` calls `gerenciar_cages` "criar e
    /// configurar Cages" and `administrar_dogma` "todo o resto sobre o Dogma",
    /// so a role that may build rooms without being able to unmake them is a
    /// role somebody can actually write — and a single boolean for both would
    /// make that role impossible to offer correctly.
    ///
    /// **Convenience, never enforcement**, like the five above it.
    pub may_delete_rooms: bool,
    /// Set once the session is over.
    pub ended: Option<EndReason>,
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
    /// Cages or Lines changed.
    ChannelsChanged,
    /// The Dogma renamed itself, or changed its picture.
    ///
    /// Separate from [`Self::ChannelsChanged`] for the reason
    /// `seele_core::Changed` gives: what moved is the header and the badge, and
    /// a shell that redrew every room because the server was renamed would be
    /// redrawing the one part of the screen that did not change.
    ///
    /// The new name is on the next [`Snapshot`]. The picture is not — read
    /// [`Snapshot::icon_revision`] and fetch it with
    /// [`crate::Plug::dogma_icon`] when the number moves.
    DogmaChanged,
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
    /// isso que ele existe: `Plug::connect` bloqueia, e quem se inscrevesse
    /// depois dela só teria o `Arc<Plug>` com a travessia inteira já terminada.
    /// Quem quiser este evento entra por `Plug::connect_watching`, que recebe o
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
    /// file is going up there is no message yet — the Dogma publishes it only
    /// once the bytes have arrived whole — so there is nothing for a message
    /// event to be about. What the screen has is a bar.
    TransferChanged {
        /// Where it is.
        transfer: Transfer,
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
    /// Every byte went out. The message appears on the Line next.
    Sent {
        /// Which message.
        client_message_id: u64,
    },
    /// The Dogma cut the stream: it refused. The reason is still travelling.
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
    /// The Dogma said **why** it refused.
    RefusedBecause {
        /// Which message.
        client_message_id: u64,
        /// The enumerated reason.
        reason: AttachmentRefusal,
    },
    /// A file that was asked for is not coming, and why.
    ///
    /// The expected reason is [`AttachmentRefusal::Expired`]: the bytes were
    /// evicted to keep the Dogma under its ceiling, the row survived, and this
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

/// Why a Dogma would not take, or would not hand back, a file.
///
/// Mirrored here rather than re-exported from the wire, like every other
/// enumeration that crosses this boundary: the shape a shell matches on is this
/// crate's promise, and a variant renamed on the wire should break a build here
/// rather than silently change what a screen writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AttachmentRefusal {
    /// The pilot lacks the permission to attach.
    NotAllowed,
    /// Larger than this Dogma's per-file limit.
    TooLarge {
        /// The largest file this Dogma accepts, in bytes. Carried because "too
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
    /// This Dogma is not storing attachments at all.
    Unavailable,
    /// No such attachment, or it is in a Line this pilot may not read.
    NotFound,
    /// The bytes were evicted to keep the Dogma under its ceiling.
    Expired,
    /// The header was not a header.
    Malformed,
}

/// A file hanging off a message, as a screen sees it.
///
/// ADR 0027. Present **even when the bytes are gone**, which is the whole
/// reason the Dogma keeps the row after deleting the blob: a message that had a
/// picture and now draws as an empty line leaves nobody able to tell there had
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
    /// Whether the bytes are still on the Dogma.
    pub expired: bool,
}

/// What a window may draw for one attachment, and why. ADR 0027.
///
/// The answer to one press of one button, and never anything a screen gets by
/// scrolling: the file lives on the Dogma, so looking at it is downloading it,
/// and a Line that fetched every picture as it scrolled past would turn the
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
pub enum PlugError {
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
    /// ADR 0006, and deliberately not [`PlugError::PinChanged`]: nothing was
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
    /// Its own variant rather than [`PlugError::NoAudioDevice`] because the two
    /// ask different things of the person reading them: "this machine has no
    /// microphone" and "the one you picked was unplugged" have different next
    /// steps, and the second one has a list to pick from again.
    CaptureDeviceGone,
    /// The chosen output is not one this machine is offering any more.
    ///
    /// Its own variant rather than [`PlugError::CaptureDeviceGone`] because a
    /// shell writes one sentence per variant, and "the microphone you picked was
    /// unplugged" is the wrong sentence to read after choosing a headset. The
    /// two point at opposite halves of the screen.
    PlaybackDeviceGone,
    /// The named pilot is not in this Cage.
    UnknownPilot,
    /// No Cage or Line by that name or number.
    UnknownChannel,
    /// The control stream broke.
    LinkLost,
    /// The file offered as the Dogma's icon is not a picture that fits there.
    ///
    /// Answered **before** anything is sent, by
    /// `seele_core::check_dogma_icon`, and that is the whole reason this
    /// variant exists: the rule lives on the wire, and a picture that breaks it
    /// makes the frame unbuildable — which everything above the link reads as a
    /// dropped connection. Without this, choosing a PDF would look exactly like
    /// the network falling over, five-minute internal battery included.
    IconNotAPicture,
    /// The picture offered as the Dogma's icon is heavier than a Dogma takes.
    ///
    /// Separate from [`PlugError::IconNotAPicture`] because the next step
    /// differs: a photograph can be shrunk, and a PDF cannot be made into an
    /// icon. The ceiling travels with it so the sentence can carry the number
    /// — a shell has no other way to name a limit that lives in the protocol.
    IconTooBig {
        /// The most bytes a Dogma accepts.
        limit_bytes: u64,
    },
}

impl std::fmt::Display for PlugError {
    /// For logs and `Error`, never for a user.
    ///
    /// A shell that prints this is a shell that skipped writing its own
    /// sentence, which is what the enum exists to make it do.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PlugError {}

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
            seele_core::DisconnectReason::DogmaFull,
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
            seele_core::AlertReason::CageEntryRefused,
            seele_core::AlertReason::PermissionDenied,
            seele_core::AlertReason::CageFull,
            seele_core::AlertReason::OperatorNotice,
            seele_core::AlertReason::RateLimited,
            seele_core::AlertReason::MovedByOperator,
            seele_core::AlertReason::CageDeleted,
            seele_core::AlertReason::LineDeleted,
            seele_core::AlertReason::LastCage,
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
            let band: SyncBand = seele_core::SyncBand::of(ratio).into();
            let expected = match ratio {
                85..=255 => SyncBand::Nominal,
                60..=84 => SyncBand::Degraded,
                _ => SyncBand::Critical,
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
            let band: SyncBand = seele_core::SyncBand::of(ratio).into();
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
        let pin = PlugError::PinChanged {
            pinned: "aaa".into(),
            offered: "bbb".into(),
        };
        let PlugError::PinChanged { pinned, offered } = &pin else {
            panic!("shape changed");
        };
        assert_ne!(pinned, offered);

        // Everything else is a bare variant, which is what makes it localisable.
        assert_eq!(format!("{}", PlugError::Unreachable), "Unreachable");
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
            .filter(|line| !line.starts_with("//") && line.contains(known_field_marker()))
            .collect();

        for campo in &campos {
            assert!(
                !campo.contains("Vec<Message>"),
                "`Snapshot` carries the conversation again: `{campo}`\n\
                 Reading this costs one clone of every nickname and body already \
                 said, on every interface frame — twice a second — so a long \
                 session gets steadily slower. Use `messages_revision` and \
                 `Plug::messages`, which exist for exactly this."
            );
            assert!(
                !campo.contains("Vec<u8>"),
                "`Snapshot` carries raw bytes: `{campo}`\n\
                 The one thing that would be, today, is the Dogma\u{2019}s icon, and it \
                 is bounded rather than unbounded — but it is still kilobytes \
                 cloned and turned into JSON twice a second for a value that \
                 changes when somebody presses a button. Use `icon_revision` and \
                 `Plug::dogma_icon`, which are the same answer one size smaller."
            );
        }

        assert!(
            campos.iter().any(|campo| campo.contains("icon_revision")),
            "`icon_revision` is gone, and with it the only way a shell can tell \
             that the Dogma\u{2019}s picture moved without being handed it on every frame"
        );

        assert!(
            campos
                .iter()
                .any(|campo| campo.contains("messages_revision")),
            "`messages_revision` is gone, and with it the only way a shell can \
             tell that the history moved without being handed all of it"
        );
    }

    /// What a field declaration looks like, so a doc line is not mistaken for one.
    fn known_field_marker() -> &'static str {
        "pub "
    }
}
