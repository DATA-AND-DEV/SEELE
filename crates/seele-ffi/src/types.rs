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
        }
    }
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
    pub dogma: String,
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
    /// The named pilot is not in this Cage.
    UnknownPilot,
    /// No Cage or Line by that name or number.
    UnknownChannel,
    /// The control stream broke.
    LinkLost,
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
        }

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
