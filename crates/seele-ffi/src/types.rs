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
/// thresholds. Two shells with two copies of "90 is nominal" is two shells that
/// disagree the day one of them is updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum SyncBand {
    /// 90 and above.
    Nominal,
    /// 70 to 89.
    Acceptable,
    /// 40 to 69.
    Degraded,
    /// Below 40.
    ///
    /// The default, because an unmeasured ratio is zero and zero is critical by
    /// the thresholds in `specs/07-tema-evangelion.md`. Defaulting to nominal
    /// would be a reassurance nobody measured — worse than an alarm, because an
    /// alarm gets checked.
    #[default]
    Critical,
}

impl From<seele_core::SyncBand> for SyncBand {
    fn from(band: seele_core::SyncBand) -> Self {
        match band {
            seele_core::SyncBand::Nominal => Self::Nominal,
            seele_core::SyncBand::Acceptable => Self::Acceptable,
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

/// What the server's certificate turned out to be. ADR 0003.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Trust {
    /// Never seen before. The fingerprint was pinned.
    ///
    /// `specs/08-seguranca.md` wants this stated rather than accepted in
    /// silence, which is why it is a value the shell must handle and not a log
    /// line it may ignore.
    FirstContact {
        /// What was pinned. Show it — somebody may want to check it elsewhere.
        fingerprint: String,
    },
    /// Matches what was pinned before.
    Known,
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
    /// The conversation in the open Line, oldest first.
    pub messages: Vec<Message>,
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
    /// The connection reached PADRÃO: AZUL.
    Connected {
        /// What the certificate turned out to be.
        trust: Trust,
    },
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
    /// The server refused the session, and said why.
    Refused {
        /// The enumerated reason.
        reason: EndReason,
    },
    /// The identity on disk could not be read or written.
    IdentityUnavailable,
    /// No usable microphone or speaker.
    NoAudioDevice,
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
        ];
        let mapped: std::collections::HashSet<NoticeReason> =
            all.into_iter().map(NoticeReason::from).collect();
        assert_eq!(mapped.len(), all.len());
    }

    #[test]
    fn the_bands_agree_with_the_core() {
        // Two shells with two copies of "90 is nominal" is two shells that
        // disagree the day one of them is updated. The core decides; this only
        // relabels.
        for ratio in 0..=100u8 {
            let band: SyncBand = seele_core::SyncBand::of(ratio).into();
            let expected = match ratio {
                90..=255 => SyncBand::Nominal,
                70..=89 => SyncBand::Acceptable,
                40..=69 => SyncBand::Degraded,
                _ => SyncBand::Critical,
            };
            assert_eq!(band, expected, "ratio {ratio}");
        }
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
}
