//! Control-channel messages.
//!
//! `specs/02-protocolo.md` carries these on "bidirectional stream #0, long
//! lived": handshake, state, presence, commands. Text goes on its own ephemeral
//! streams so that fetching five thousand messages of history cannot delay a
//! presence event, and voice goes on datagrams.
//!
//! # Three gaps in `specs/02-protocolo.md` closed here
//!
//! Found while implementing M1 and M2; see `docs/plano-m0-m1.md`.
//!
//! - **G1.** The spec says the client resolves `ssrc → pilot` "from the table
//!   received on the control channel", but no control message carried an `ssrc`,
//!   and a client had no way to learn its own. [`ServerMessage::Session`] and
//!   [`ServerMessage::PilotJoined`] now carry it.
//! - **G8.** "Isolamento total" (deafen) is defined in
//!   `specs/07-tema-evangelion.md` and bound to a key in `specs/05-cliente-tui.md`,
//!   but had no protocol representation, so a roster could not show who was not
//!   listening. [`PilotState`] carries it beside the A.T. Field.
//! - **G9.** `EnviarMensagem` is documented as "idempotent by `client_msg_id`"
//!   while the field was missing from its payload. It is explicit here.
//!
//! # Every reason is an enum
//!
//! `specs/02-protocolo.md`: "All error reasons are enumerated. No free-form
//! string reaches the interface — the shell decides how to present each
//! variant." That is why [`DisconnectReason`] and [`AlertReason`] are enums and
//! why a failed handshake reports [`DisconnectReason::CredentialRejected`]
//! rather than a sentence.
//!
//! The one place text does cross is the `operator_text` field of
//! [`ServerMessage::Alert`], and it is not an exception to the rule: an
//! operator's own words about their own server are data, not an error reason. It
//! is `Option` so the interface can render the enumerated reason with or without
//! it.

use serde::{Deserialize, Serialize};

use crate::ids::{
    AttachmentId, CageId, ClientMessageId, LineId, MessageId, PilotId, RoleId, SessionId, Ssrc,
};
use crate::version::PROTOCOL_VERSION;

/// Largest control frame this build will accept, including the version byte.
///
/// `specs/08-seguranca.md`: every network input is size-limited before
/// allocating. Control frames are small by design — history and other bulk
/// transfers use their own streams — so this is generous rather than tight.
pub const MAX_FRAME_LEN: usize = 16 * 1024;

/// Longest text message body, in bytes.
///
/// `specs/02-protocolo.md` leaves the limit open; 4 KiB is far more than a chat
/// line and far less than anything that would strain a frame.
pub const MAX_BODY_LEN: usize = 4 * 1024;

/// Longest nickname, in bytes.
pub const MAX_NICKNAME_LEN: usize = 32;

/// Longest client name, in bytes.
pub const MAX_CLIENT_NAME_LEN: usize = 64;

/// Longest Cage or Line name, in bytes.
///
/// A name is a label in a list, not a description. Long enough for
/// `CAGE-01 CENTRAL` several times over, short enough that no shell has to
/// decide where to cut one off.
pub const MAX_CHANNEL_NAME_LEN: usize = 48;

/// Largest number of pilots a Cage may be created with.
///
/// `specs/04-servidor-seele.md` sizes the target at "50 sessões e 5 Cages
/// ativos em 1 vCPU / 512 MB", so this is five times the whole Dogma: generous
/// rather than tight, and there to stop a `u16` of 65 535 being written into a
/// room nobody could fill.
pub const MAX_CAGE_LIMIT: u16 = 250;

/// Longest operator-supplied alert text, in bytes.
pub const MAX_ALERT_TEXT_LEN: usize = 512;

/// Longest sender-chosen file name, in bytes.
///
/// The name never reaches the filesystem — ADR 0027 stores a blob under the
/// SHA-256 of its own content — so this bounds what a shell has to draw rather
/// than what a path may hold. Long enough for anything a camera or a phone
/// produces, short enough that no shell has to decide where to cut one off.
pub const MAX_FILE_NAME_LEN: usize = 255;

/// Longest declared content type, in bytes.
///
/// A claim, not a fact. `image/png` is ten bytes; the slack is for parameters
/// nobody here reads.
pub const MAX_DECLARED_TYPE_LEN: usize = 128;

/// Whether an attachment's bytes are still there.
///
/// Enumerated, and not a sentence: ADR 0027 makes expiry a state the shell
/// presents however it presents every other enumerated reason. The row survives
/// the bytes precisely so this can be said at all — a message whose attachment
/// row had been deleted would draw as a message with nothing in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentState {
    /// The bytes are on the Dogma and may be fetched.
    Available,
    /// The bytes were evicted to keep the Dogma under its ceiling.
    ///
    /// The name and the size are still here, which is the point.
    Expired,
}

/// A file hanging off a message, as a client sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentInfo {
    /// What to ask for when downloading.
    pub id: AttachmentId,
    /// The name the sender gave it.
    pub file_name: String,
    /// The type the sender claimed.
    ///
    /// **A claim.** ADR 0027: only a short list of image types is ever drawn
    /// inline, and only when the bytes agree with the claim. Everything else is
    /// a file with a name and a size. This is not about trusting the file; it
    /// is about which decoder the bytes go to.
    pub declared_type: String,
    /// How many bytes it was.
    pub byte_size: u64,
    /// Whether the bytes are still there.
    pub state: AttachmentState,
}

/// Why a Dogma would not take, or would not hand back, a file.
///
/// Every one of these is a refusal somebody is waiting on, so each says
/// something a shell can turn into a different sentence. A single
/// "attachment failed" would leave a person retrying a file that will never fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentRefusal {
    /// The pilot lacks [`Permission::AttachFile`].
    NotAllowed,
    /// Larger than this Dogma's per-file limit.
    ///
    /// Carries the limit, because "too big" without a number sends somebody to
    /// try again with a file that is also too big.
    TooLarge {
        /// The largest file this Dogma accepts, in bytes.
        limit: u64,
    },
    /// Every byte of the ceiling is held by transfers already under way.
    ///
    /// Distinct from [`Self::TooLarge`]: this one passes if it is tried again
    /// in a moment, and the other never will.
    NoRoom,
    /// The stream ended before the declared number of bytes arrived, or carried
    /// more.
    SizeMismatch,
    /// The bytes did not hash to what the header declared.
    ///
    /// The one question ADR 0027 says a Dogma can answer about a file: did it
    /// arrive whole. It says nothing about whether the file is good.
    HashDidNotMatch,
    /// The pilot is sending bytes faster than their budget.
    RateLimited,
    /// This Dogma is not storing attachments at all.
    Unavailable,
    /// No such attachment, or it belongs to a Line this pilot may not read.
    NotFound,
    /// The bytes were evicted to keep the Dogma under its ceiling.
    Expired,
    /// The header was not a header, or a field was outside its bounds.
    Malformed,
}

/// Length of an Ed25519 public key, in bytes. ADR 0004.
pub const PUBLIC_KEY_LEN: usize = 32;

/// Length of an Ed25519 signature, in bytes.
pub const SIGNATURE_LEN: usize = 64;

/// Longest authentication proof, in bytes.
///
/// An Ed25519 signature is 64 bytes (ADR 0004); the slack is for whatever a
/// password fallback needs.
pub const MAX_PROOF_LEN: usize = 256;

/// Why a control frame could not be handled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    /// An empty frame carries not even a version.
    #[error("control frame is empty")]
    Empty,

    /// Longer than [`MAX_FRAME_LEN`].
    #[error("control frame is {len} bytes, over the {MAX_FRAME_LEN}-byte limit")]
    TooLong {
        /// Length received.
        len: usize,
    },

    /// A version outside the compatibility window.
    #[error("frame announces protocol {found}, this build implements {expected}")]
    UnsupportedVersion {
        /// Version in the frame.
        found: u8,
        /// Version this build implements.
        expected: u8,
    },

    /// The bytes after the version byte are not a message.
    #[error("frame body is malformed")]
    Malformed,

    /// A field exceeded its documented limit.
    #[error("field `{field}` is {len} bytes, over its {limit}-byte limit")]
    FieldTooLong {
        /// Which field.
        field: &'static str,
        /// Length received.
        len: usize,
        /// Limit for that field.
        limit: usize,
    },

    /// A numeric field held a value that is not a number, or is out of range.
    ///
    /// Found by fuzzing on the first run. `specs/02-protocolo.md` derives the
    /// Sync Ratio from RTT, jitter and loss, so a `NaN` in any of them poisons
    /// the product's signature metric — and silently, because every comparison
    /// against `NaN` is false and the band logic in
    /// `specs/07-tema-evangelion.md` would fall through to the wrong branch
    /// rather than error.
    #[error("field `{field}` holds a value outside its allowed range")]
    FieldOutOfRange {
        /// Which field.
        field: &'static str,
    },
}

/// Presence, as announced by the pilot.
///
/// Deliberately short. `specs/00-visao-geral.md` names "published presence" as
/// one of the things that made the tools it is reacting against unpleasant, so
/// this stays a hint the pilot sets rather than anything inferred from activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Presence {
    /// Present and available.
    Available,
    /// Connected but not paying attention.
    Away,
    /// Present and asking not to be interrupted.
    DoNotDisturb,
}

/// One permission. `specs/04-servidor-seele.md`, enumerated with no expression
/// system: "the complexity does not pay for itself at the target scale".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// See that a Cage exists.
    ViewCage,
    /// Enter a Cage.
    InsertPlug,
    /// Transmit voice.
    Speak,
    /// Read a Line.
    ReadLine,
    /// Post to a Line.
    WriteLine,
    /// Delete somebody else's message.
    RemoveMessage,
    /// Move a pilot between Cages.
    MovePilot,
    /// Disconnect a pilot.
    Kick,
    /// Bar a pilot from returning.
    Ban,
    /// Create and configure Cages.
    ManageCages,
    /// Create and assign roles.
    ManageRoles,
    /// Everything else about the Dogma.
    AdministerDogma,
}

/// A role and the permissions it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// Identifier.
    pub id: RoleId,
    /// Display name. One of Commander, Operator, Pilot, Observer for the four
    /// defaults in `specs/04-servidor-seele.md`, but operators may add more.
    pub name: String,
    /// What the role allows.
    ///
    /// `specs/04-servidor-seele.md`: denied beats granted, and there is no tree
    /// inheritance. A permission absent from every one of a pilot's roles is
    /// denied.
    pub permissions: Vec<Permission>,
}

/// A pilot as other pilots see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotProfile {
    /// Account identifier.
    pub id: PilotId,
    /// Display name.
    pub nickname: String,
    /// Roles held.
    pub roles: Vec<RoleId>,
}

/// A voice channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CageInfo {
    /// Identifier.
    pub id: CageId,
    /// Display name.
    pub name: String,
    /// How many pilots may be inside at once.
    pub limit: u16,
    /// Whether entry needs a password.
    pub password_required: bool,
    /// A Line bound to this Cage, if any. `specs/04-servidor-seele.md` makes the
    /// association optional.
    pub line: Option<LineId>,
}

/// A text channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineInfo {
    /// Identifier.
    pub id: LineId,
    /// Display name.
    pub name: String,
}

/// One of the three subsystems in `specs/04-servidor-seele.md`.
///
/// Not decoration: they are real module boundaries, and the client shows the
/// state of each. "The three agree" is the nominal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Subsystem {
    /// Identity, authentication, sessions, roles, permissions.
    Melchior,
    /// Media routing: Cage subscriptions, datagram forwarding, bandwidth.
    Balthasar,
    /// Persistent state: Cages, Lines, history, configuration, migrations.
    Casper,
}

/// How a subsystem is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubsystemHealth {
    /// Working.
    Nominal,
    /// Working, but not well.
    Degraded,
    /// Not working.
    Failed,
}

/// What a pilot's client is currently doing.
///
/// Carries both mute controls. `specs/07-tema-evangelion.md` names them
/// "A.T. Field" (microphone) and "Isolamento total" (speakers); the second had
/// no protocol representation before — gap G8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotState {
    /// Whose state this is.
    pub pilot: PilotId,
    /// Microphone muted — "A.T. Field" active.
    pub at_field: bool,
    /// Speakers muted — "Isolamento total".
    ///
    /// Local in effect, but announced so the roster can show who is not
    /// listening. Talking to somebody who cannot hear you is worth knowing.
    pub total_isolation: bool,
    /// Whether they are transmitting right now.
    pub speaking: bool,
    /// Presence hint.
    pub presence: Presence,
    /// Sync Ratio, 0 to 100. `specs/02-protocolo.md`.
    pub sync_ratio: u8,
}

/// Connection quality, as the server sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    /// Round trip, milliseconds.
    pub rtt_ms: f32,
    /// Arrival jitter, milliseconds.
    pub jitter_ms: f32,
    /// Packet loss, 0.0 to 1.0.
    pub loss_fraction: f32,
    /// State of each subsystem.
    pub subsystems: Vec<(Subsystem, SubsystemHealth)>,
}

/// Why a session is ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisconnectReason {
    /// Protocol version outside the compatibility window.
    Incompatible,
    /// Authentication failed.
    ///
    /// `specs/08-seguranca.md` requires login failures to be uniform, so this
    /// says nothing about whether the account exists.
    CredentialRejected,
    /// Handshake did not finish inside the 10 s budget.
    HandshakeTimeout,
    /// An operator disconnected this pilot.
    Kicked,
    /// An operator barred this pilot.
    Banned,
    /// The Dogma is full.
    DogmaFull,
    /// Planned downtime. `specs/04-servidor-seele.md` gives clients 3 s.
    ScheduledMaintenance,
    /// The server is stopping for another reason.
    ServerShuttingDown,
    /// Keepalive lapsed.
    Timeout,
    /// The client sent something it should not have.
    ProtocolViolation,
    /// The client exceeded its frame budget. `specs/04-servidor-seele.md`.
    RateLimited,

    /// This connection fell so far behind the Dogma's events that some were lost.
    ///
    /// The bus a Dogma broadcasts on is a fixed ring. A connection that stops
    /// draining it — because the peer stopped reading and the writes back to it
    /// blocked — eventually falls off the back, and the events that scrolled
    /// past **no longer exist** for that session. Committed messages are among
    /// them.
    ///
    /// Ending the session is the cure rather than the punishment. The hole
    /// cannot be patched in place: events are not addressable, so the server
    /// cannot say which ones were missed, and the client cannot ask for them.
    /// What it *can* do is reconnect and fetch history, which is a path that
    /// already exists and is already exercised — and which is exactly what the
    /// internal battery does on its own. `docs/pendencias.md` #1 is what
    /// happened while this was silent instead: the pilot stayed connected with
    /// a gap in the conversation that neither end could name.
    ///
    /// Appended last, for the reason [`AlertReason::RateLimited`] gives.
    FellBehind,
}

/// How loud an alert is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// Worth knowing.
    Info,
    /// Something is degrading.
    Warning,
    /// Something is wrong. `specs/07-tema-evangelion.md` reserves red for this.
    Critical,
}

/// What an alert is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertReason {
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
    ///
    /// Sent **before** [`DisconnectReason::RateLimited`], not instead of it: a
    /// client that is merely badly written gets told, and only one that keeps
    /// going after being told is disconnected. Dropping somebody with no
    /// explanation is how a product comes to look broken.
    ///
    /// Appended last on purpose. A build one protocol version older does not
    /// know this variant and fails to decode the frame — which costs a
    /// connection that was already exceeding its budget, and nothing else.
    RateLimited,

    /// An operator moved this pilot's plug into another Cage.
    ///
    /// Its own reason rather than [`Self::OperatorNotice`], because the shell
    /// has a specific sentence to write and `OperatorNotice` would have it
    /// write "the operator is saying something" beside a room that changed on
    /// its own. Being moved without being told is the case this exists to stop.
    ///
    /// Appended after `RateLimited`, for the reason that variant gives.
    MovedByOperator,

    /// The Cage this pilot's plug was in no longer exists.
    ///
    /// Its own reason rather than [`Self::OperatorNotice`], for the reason
    /// [`Self::MovedByOperator`] gives: the shell has a specific sentence to
    /// write, and being turned out of a room in the middle of speaking is the
    /// case this exists to explain. Sent only to the pilots who were inside —
    /// everybody else learns the room is gone from
    /// [`ServerMessage::CageDeleted`] and has nothing to be told about it.
    ///
    /// Appended after `MovedByOperator`, for the reason that variant gives.
    CageDeleted,

    /// A Line this pilot had open no longer exists, and neither does anything
    /// written in it.
    ///
    /// Separate from [`Self::CageDeleted`] because the two sentences are not
    /// the same sentence: one says the room you were speaking in is gone, the
    /// other says the conversation you were reading was destroyed. A shell
    /// given one reason for both would have to write the vaguer of the two.
    LineDeleted,

    /// The Cage asked about is the only one the Dogma has, so it stays.
    ///
    /// A refusal with a sentence of its own, and that is the whole reason it
    /// exists: the nearest existing reason is [`Self::CageEntryRefused`], which
    /// every shell writes as "entry refused" — a sentence about walking into a
    /// room, in front of somebody who was trying to destroy one. What this has
    /// to say is "make another room first", and no other variant says it.
    ///
    /// The app disables the control on the last Cage and says as much in a
    /// `title`, which is where the reader meets this first. This is the half
    /// that survives an older shell, and `specs/08-seguranca.md` puts the rule
    /// on the server for exactly that reason.
    LastCage,
}

/// Client to server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Opens the handshake. `specs/02-protocolo.md`.
    Hello {
        /// Convite de uso único ou senha do Dogma, quando ele exige um.
        ///
        /// Viaja no `Hello`, portanto **antes** do desafio-resposta. Isso é
        /// deliberado: o segredo diz se esta conexão tem direito de existir, e
        /// gastar um desafio criptográfico com quem nem devia estar batendo à
        /// porta é trabalho de graça para quem varre a internet. O canal já é
        /// TLS 1.3 desde o primeiro byte, então o segredo nunca vai em claro.
        ///
        /// `None` num Dogma aberto é o caso normal.
        join_secret: Option<String>,
        /// Protocol version the client speaks.
        version: u8,
        /// Client software name, for logs.
        client: String,
        /// Nickname the client would like.
        nickname: String,
        /// Ed25519 public key claiming an identity (ADR 0004).
        ///
        /// Not in the payload column of `specs/02-protocolo.md`, and it has to
        /// be: the server answers with a nonce and then verifies a signature,
        /// which needs a key to verify against. In M3, MELCHIOR looks this key
        /// up against known accounts; in M2 there is no persistence, so proving
        /// possession of the key is all the handshake can establish.
        public_key: Vec<u8>,
    },
    /// Answers the server's challenge.
    Response {
        /// Proof of identity. ADR 0004 makes this an Ed25519 signature.
        proof: Vec<u8>,
    },
    /// Enters a Cage. "Inserir plug" in `docs/glossario.md`.
    InsertPlug {
        /// Which Cage.
        cage: CageId,
        /// Password, if the Cage needs one.
        password: Option<String>,
    },
    /// Leaves the current Cage. "Ejetar".
    EjectPlug,
    /// Subscribes to a text channel.
    JoinLine {
        /// Which Line.
        line: LineId,
    },
    /// Posts a message.
    SendMessage {
        /// Which Line.
        line: LineId,
        /// Body.
        body: String,
        /// Message being replied to.
        replies_to: Option<MessageId>,
        /// Client-chosen identifier, making the send idempotent — gap G9.
        client_message_id: ClientMessageId,
    },
    /// Fetches history. Cursor-paged, never by offset.
    FetchHistory {
        /// Which Line.
        line: LineId,
        /// Where to continue from. `None` starts at the newest.
        cursor: Option<MessageId>,
        /// How many messages.
        limit: u16,
    },
    /// Mutes or unmutes the microphone.
    SetAtField(bool),
    /// Mutes or unmutes the speakers — gap G8.
    SetTotalIsolation(bool),
    /// Announces presence.
    SetPresence(Presence),
    /// Keepalive. `specs/02-protocolo.md` sends one every 5 s.
    Ping {
        /// Client timestamp, echoed back.
        timestamp: u64,
    },

    // ---- rooms, made by whoever hosts ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives: a build
    // one protocol version older does not know these variants and refuses the
    // frame rather than misreading it as something it does understand.
    //
    // All four need [`Permission::ManageCages`]. `specs/04-servidor-seele.md`
    // enumerates one permission for channels and not two — there is no
    // `gerenciar_linhas` — so the one it names covers both kinds, and the
    // server checks it on every one of these.
    /// Creates a Cage.
    CreateCage {
        /// What to call it.
        name: String,
        /// How many pilots may be inside at once.
        limit: u16,
        /// A Line to bind to it, if any. `specs/04-servidor-seele.md` makes the
        /// association optional.
        line: Option<LineId>,
    },
    /// Creates a Line.
    CreateLine {
        /// What to call it.
        name: String,
    },
    /// Renames a Cage.
    RenameCage {
        /// Which Cage.
        cage: CageId,
        /// The new name.
        name: String,
    },
    /// Renames a Line.
    RenameLine {
        /// Which Line.
        line: LineId,
        /// The new name.
        name: String,
    },

    // ---- moderation ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // `specs/04-servidor-seele.md` enumerates `expulsar`, `banir`,
    // `remover_mensagem` and `mover_piloto`, migration 1 seeds all four on the
    // Comandante and the Operador, and until now **no message carried any of
    // them**. The permissions existed and there was nothing to ask for: the
    // app's `EJETAR PLUG DO OPERADOR` has been drawn and disabled since v2 for
    // exactly that reason.
    //
    // One permission each, checked by MELCHIOR at the instant the verb is used
    // — not from anything the handshake cached. `specs/08-seguranca.md`: "Toda
    // ação é verificada no servidor, sempre, mesmo que o cliente já esconda o
    // botão."
    /// Ends a pilot's session. `expulsar` — [`Permission::Kick`].
    ///
    /// This session, and nothing beyond it: the pilot may reconnect at once.
    /// Barring a return is [`Self::BanPilot`], and the two are separate verbs
    /// because they are separate decisions — "leave the room" is not "never
    /// come back", and an operator who wanted the first and got the second has
    /// no way to take it back except by finding the row.
    KickPilot {
        /// Who.
        pilot: PilotId,
    },
    /// Bars a pilot from returning. `banir` — [`Permission::Ban`].
    ///
    /// Ends their session too: a ban that let the offender stay until they
    /// chose to leave would be a ban that does nothing to the thing that
    /// prompted it.
    BanPilot {
        /// Who.
        pilot: PilotId,
        /// The operator's own words, for the operator's own record.
        ///
        /// Not a hole in the enumerated-reasons rule, and the same exception
        /// [`ServerMessage::Alert::operator_text`] documents: an operator
        /// writing about their own Dogma is data, not an error reason. It is
        /// **not** sent to the person banned — `specs/08-seguranca.md` wants
        /// uniform failures, and the refusal they meet on the way back in is
        /// the same [`DisconnectReason::Banned`] whatever this says.
        reason: Option<String>,
        /// When the ban lifts, in seconds since the Unix epoch. `None` is
        /// permanent.
        expires_at: Option<i64>,
    },
    /// Takes a message off the Line. `remover_mensagem` —
    /// [`Permission::RemoveMessage`].
    ///
    /// The permission is worded "delete somebody **else's** message", so an
    /// author removing their own does not need it. That is not a courtesy: a
    /// Dogma where taking back your own typo requires an operator is a Dogma
    /// where people ask an operator about typos.
    ///
    /// Carries no Line. The identifier is the server's own and globally
    /// unique, so the Line to announce the removal on is read out of the stored
    /// row rather than taken from the asker — a field the client fills in is a
    /// field the client can fill in wrong, and this one would aim somebody
    /// else's announcement.
    RemoveMessage {
        /// Which message.
        message: MessageId,
    },
    /// Moves a pilot into a Cage. `mover_piloto` — [`Permission::MovePilot`].
    ///
    /// The pilot is told, and told *what happened*: they get
    /// [`ServerMessage::MovedToCage`] so their client follows, and an
    /// [`ServerMessage::Alert`] carrying [`AlertReason::MovedByOperator`] so
    /// they read a sentence rather than finding themselves somewhere else with
    /// no explanation. Being moved silently is indistinguishable from a client
    /// that lost track of which room it was in.
    MovePilot {
        /// Who.
        pilot: PilotId,
        /// Where to.
        cage: CageId,
    },

    // ---- unmaking a room ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // # Why these are not [`Permission::ManageCages`]
    //
    // The four verbs above them are: creating a room and renaming one are both
    // things a mistake survives — the wrong name is renamed again, the room
    // nobody wanted is destroyed. Destroying is the one room verb that ends
    // somebody **else's** writing, and no screen of this product brings it
    // back.
    //
    // `specs/04-servidor-seele.md` enumerates `gerenciar_cages` as "criar e
    // configurar Cages" and `administrar_dogma` as "todo o resto sobre o
    // Dogma". Destroying every message six people wrote is not configuration.
    // So it sits on [`Permission::AdministerDogma`], which migration 1 seeds on
    // the Comandante alone — and the separation is real rather than notional
    // the moment an operator makes a role that may build rooms without being
    // able to unmake them. It is deliberately **not** the moderation
    // permissions either: `specs/04` gives the Operador `expulsar` and `banir`,
    // and somebody trusted to remove a person for the evening is not thereby
    // trusted to destroy the Dogma's history.
    /// Destroys a Cage. `apagar_cage` — [`Permission::AdministerDogma`].
    ///
    /// Everybody inside is turned out of it: a Cage does not vanish from under
    /// the feet of the people speaking in it. They are told, with
    /// [`AlertReason::CageDeleted`], for the reason
    /// [`AlertReason::MovedByOperator`] gives.
    ///
    /// A Line bound to the Cage is **not** destroyed with it.
    /// `specs/04-servidor-seele.md` makes Cages and Lines independent, and the
    /// association optional; destroying a voice room is no statement about the
    /// writing that happened to hang off it.
    ///
    /// The server refuses the last one. A Dogma with no Cage has nowhere to
    /// speak, which is the product's first sentence — and the refusal is a
    /// refusal rather than a silence, so a shell can say why.
    DeleteCage {
        /// Which Cage.
        cage: CageId,
    },
    /// Destroys a Line and everything written in it. `apagar_linha` —
    /// [`Permission::AdministerDogma`].
    ///
    /// Not a soft delete, unlike [`Self::RemoveMessage`]. That one keeps a row
    /// so replies do not dangle and an operator can answer "what was removed";
    /// this destroys the Line those rows hang from, so there is nothing left
    /// for either to be about.
    ///
    /// Any Cage bound to this Line keeps existing and loses the binding.
    DeleteLine {
        /// Which Line.
        line: LineId,
    },
    /// Asks what destroying a Line would cost, without destroying anything.
    ///
    /// A read, and the only verb in this enum that exists for a **sentence**: a
    /// confirmation that says "this destroys 1.847 messages by 6 people,
    /// written since 12/03" needs those three numbers counted in the database
    /// at the instant of asking. A client cannot count them for itself — it
    /// holds one page of history and would guess low by whatever the Line's
    /// whole past is — and a number that is nearly right in a box promising
    /// destruction is worse than no number at all.
    ///
    /// Needs no permission. Answering it tells a pilot how much is in a Line
    /// they may already read, and refusing it would only mean the confirmation
    /// they see is the vaguer one.
    WeighLine {
        /// Which Line.
        line: LineId,
    },
}

/// Server to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Asks the client to prove who it is.
    ///
    /// Present in the handshake diagram of `specs/02-protocolo.md` but missing
    /// from its message table.
    Challenge {
        /// Random nonce to sign.
        nonce: Vec<u8>,
    },
    /// Authentication succeeded. The session is now PATTERN: BLUE.
    Session {
        /// Session identifier.
        id: SessionId,
        /// Which pilot this connection is.
        pilot: PilotId,
        /// The media source assigned to this connection — gap G1.
        ///
        /// `specs/08-seguranca.md`: the server assigns it and never accepts one
        /// from the client. A client needs to know its own in order to read its
        /// own telemetry back.
        ssrc: Ssrc,
        /// Name of the Dogma.
        dogma: String,
        /// Voice channels visible to this pilot.
        cages: Vec<CageInfo>,
        /// Text channels visible to this pilot.
        lines: Vec<LineInfo>,
        /// Roles defined on this Dogma.
        roles: Vec<Role>,
        /// What **this** pilot may do, as MELCHIOR resolved it.
        ///
        /// `roles` above is the Dogma's catalogue of roles; nothing on the wire
        /// ever told a client which of them it holds, so no shell could tell
        /// whether to offer a control at all. Resolved server-side rather than
        /// sent as role identifiers for the shell to intersect, because
        /// "negadas vencem concedidas" (`specs/04-servidor-seele.md`) is a rule
        /// that would then be re-implemented in every shell, and wrongly in at
        /// least one of them.
        ///
        /// This is **convenience, never enforcement**. `specs/08-seguranca.md`:
        /// "A interface esconder é conveniência; o servidor negar é a
        /// segurança." Every action is checked again when it is asked for.
        permissions: Vec<Permission>,
    },
    /// A pilot entered a Cage.
    PilotJoined {
        /// Which Cage.
        cage: CageId,
        /// Who.
        profile: PilotProfile,
        /// Their media source — gap G1. This is the mapping
        /// `specs/02-protocolo.md` says the client resolves from the control
        /// channel, and which nothing previously carried.
        ssrc: Ssrc,
    },
    /// A pilot left a Cage.
    PilotLeft {
        /// Which Cage.
        cage: CageId,
        /// Who.
        pilot: PilotId,
    },
    /// A pilot's state changed.
    PilotState(PilotState),
    /// A message was posted.
    MessageReceived {
        /// Which Line.
        line: LineId,
        /// Server-assigned identifier.
        id: MessageId,
        /// Who wrote it.
        author: PilotId,
        /// When the server accepted it, in **seconds** since the Unix epoch.
        ///
        /// The unit is in the name because it was wrong once: CASPER stores
        /// seconds, this field was declared in milliseconds, and every real
        /// message would have been drawn as 1970 while the tests — which used
        /// synthetic milliseconds — passed.
        ///
        /// The server's clock, not the client's, and not the arrival time.
        /// Without it a page of history has no time on it at all: the receiving
        /// client only knows when the *page* arrived, which is now.
        /// `specs/06-clientes-gui.md` requires a session to be resumable in
        /// another client "sem perda de histórico", and history whose lines all
        /// claim to have been written the moment the app opened has lost
        /// something.
        at_seconds: i64,
        /// What the author is called.
        ///
        /// Carried with the message rather than looked up, because a client
        /// reading history has never seen most of these pilots arrive and has
        /// no other way to learn their names. Without it a resumed session
        /// attributes everything written before you got there to "piloto 1".
        author_nickname: String,
        /// Body.
        body: String,
        /// What it replies to.
        replies_to: Option<MessageId>,
        /// Echo of the sender's identifier, so a client can match its own
        /// pending send instead of showing it twice.
        client_message_id: Option<ClientMessageId>,
    },
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
    /// Connection quality and subsystem health.
    Telemetry(Telemetry),
    /// Something the interface should surface.
    Alert {
        /// How loud.
        severity: AlertSeverity,
        /// What about. Enumerated, so the shell can localise it.
        reason: AlertReason,
        /// The operator's own words, when they have any.
        ///
        /// Not a hole in the enumerated-reasons rule: an operator writing about
        /// their own server is data, not an error reason.
        operator_text: Option<String>,
    },
    /// Echo of a [`ClientMessage::Ping`].
    Pong {
        /// The client's timestamp, unchanged.
        timestamp: u64,
    },
    /// The session is ending, and why.
    Disconnecting {
        /// Enumerated reason. Never generic — `specs/02-protocolo.md`.
        reason: DisconnectReason,
    },

    // ---- rooms, made by whoever hosts ----
    //
    // Appended last, for the same reason as their client-side counterparts.
    //
    // Sent to **everybody connected**, the pilot who asked included. Without
    // that, a Cage made at nine o'clock is a Cage nobody sees until they
    // reconnect — and "reconnect to see the room I just told you about" is the
    // kind of instruction that makes a product feel broken rather than new.
    /// A Cage was created.
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

    // ---- moderation ----
    //
    // Three of the four verbs need no announcement of their own: a kick and a
    // ban both end with [`Self::Disconnecting`], which already enumerates
    // `Kicked` and `Banned`; a removal is [`Self::MessageRemoved`], which every
    // shell already folds in. Only being moved had nothing that could say it.
    /// This pilot's plug is now in a different Cage, by somebody else's hand.
    ///
    /// Sent only to the pilot who was moved. Everybody else learns it the
    /// ordinary way, as a [`Self::PilotLeft`] from the old Cage and a
    /// [`Self::PilotJoined`] in the new one — there is nothing special about a
    /// move from outside, and inventing a second way to say "somebody is in
    /// that room now" would mean every shell learning both.
    ///
    /// What makes this its own message is that the moved client has to change
    /// **its own** idea of where it is, and that is a fact no `PilotJoined` has
    /// ever carried: a client sets its current Cage on the way *out*, when it
    /// asks. Without this it would keep sending voice into the room it thought
    /// it was in and drawing that room's roster around itself.
    MovedToCage {
        /// Where the plug is now.
        cage: CageId,
    },

    // ---- unmaking a room ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // Sent to **everybody connected**, the pilot who asked included, exactly
    // like the four announcements above: a room that goes on being drawn until
    // the next handshake is a room people keep trying to walk into.
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
    /// What destroying a Line would cost, counted now.
    ///
    /// The answer to [`ClientMessage::WeighLine`], and the numbers a
    /// confirmation is built out of. Counted at the moment of asking rather
    /// than carried on [`LineInfo`], because a count on the room list would be
    /// stale by every message sent since it was drawn — and stale is the one
    /// thing a number in this particular sentence may not be.
    LineWeighed {
        /// Which Line.
        line: LineId,
        /// How many messages are in it that anybody can read.
        ///
        /// Messages already taken off the Line by [`ClientMessage::RemoveMessage`]
        /// are not counted. They are gone from every screen already, so
        /// counting them would inflate what the reader is told they are about
        /// to lose by a number only the database can see.
        messages: u32,
        /// How many distinct pilots wrote them.
        authors: u32,
        /// When the oldest one was written, in seconds since the Unix epoch.
        ///
        /// `None` when the Line is empty, which is the one case where the
        /// sentence has no date to give and must say something else instead.
        oldest_at_seconds: Option<i64>,
    },
}

/// Serialises a message into a frame, version byte first.
///
/// `specs/02-protocolo.md`: "The first byte of every control frame is the
/// protocol version."
///
/// # Errors
///
/// Returns [`ControlError::FieldTooLong`] if a field exceeds its limit, or
/// [`ControlError::TooLong`] if the whole frame does.
pub fn encode<T: Serialize + Validate>(message: &T) -> Result<Vec<u8>, ControlError> {
    message.validate()?;
    let mut frame = Vec::with_capacity(64);
    frame.push(PROTOCOL_VERSION);
    frame = postcard::to_extend(message, frame).map_err(|_| ControlError::Malformed)?;
    if frame.len() > MAX_FRAME_LEN {
        return Err(ControlError::TooLong { len: frame.len() });
    }
    Ok(frame)
}

/// Parses a frame back into a message.
///
/// Rejects before allocating, per `specs/08-seguranca.md`, and never panics
/// whatever the bytes are — this is the surface named for fuzzing.
///
/// # Errors
///
/// Returns [`ControlError`] for every malformed or oversized case.
pub fn decode<T: for<'de> Deserialize<'de> + Validate>(frame: &[u8]) -> Result<T, ControlError> {
    if frame.len() > MAX_FRAME_LEN {
        return Err(ControlError::TooLong { len: frame.len() });
    }
    let Some((version, body)) = frame.split_first() else {
        return Err(ControlError::Empty);
    };
    // Version before anything else, so a future layout is refused rather than
    // misparsed into something plausible.
    crate::version::negotiate(*version).map_err(|_| ControlError::UnsupportedVersion {
        found: *version,
        expected: PROTOCOL_VERSION,
    })?;

    let message: T = postcard::from_bytes(body).map_err(|_| ControlError::Malformed)?;
    message.validate()?;
    Ok(message)
}

/// Bounds checking that serialisation cannot express.
///
/// `postcard` will happily carry a four-thousand-character nickname; the limits
/// in `specs/08-seguranca.md` are a separate concern from the wire format, and
/// they are enforced on the way in as well as the way out so a peer cannot skip
/// them by hand-rolling a frame.
pub trait Validate {
    /// Checks every bounded field.
    ///
    /// # Errors
    ///
    /// Returns [`ControlError::FieldTooLong`] for the first field over its limit.
    fn validate(&self) -> Result<(), ControlError>;
}

fn check(field: &'static str, len: usize, limit: usize) -> Result<(), ControlError> {
    if len > limit {
        return Err(ControlError::FieldTooLong { field, len, limit });
    }
    Ok(())
}

/// Bounds a Cage or Line name at both ends.
///
/// The upper bound is [`MAX_CHANNEL_NAME_LEN`], like every other text field.
/// The lower one is the half that is easy to leave out: a name that is empty,
/// or only spaces, produces a row in the channel list with nothing in it — a
/// thing the shell draws, the person clicks, and nobody can refer to out loud.
/// Refused here rather than trimmed here, because trimming on the way *in*
/// would mean the sender and the receiver disagree about what was sent.
fn check_name(field: &'static str, name: &str) -> Result<(), ControlError> {
    if name.trim().is_empty() {
        return Err(ControlError::FieldOutOfRange { field });
    }
    check(field, name.len(), MAX_CHANNEL_NAME_LEN)
}

/// Bounds how many pilots a Cage may hold.
///
/// Zero is refused: a Cage nobody may enter is not a Cage, and a limit of zero
/// is far more often a field left at its default than a deliberate choice.
fn check_cage_limit(limit: u16) -> Result<(), ControlError> {
    if limit == 0 || limit > MAX_CAGE_LIMIT {
        return Err(ControlError::FieldOutOfRange { field: "limit" });
    }
    Ok(())
}

/// Rejects `NaN`, infinities and values outside a sane range.
fn check_range(field: &'static str, value: f32, min: f32, max: f32) -> Result<(), ControlError> {
    if !value.is_finite() || value < min || value > max {
        return Err(ControlError::FieldOutOfRange { field });
    }
    Ok(())
}

/// Plausible upper bound for a latency or jitter reading, in milliseconds.
///
/// Ten seconds is far past the five-minute grace period of
/// `specs/02-protocolo.md` being useful; anything larger is a broken sender
/// rather than a slow one.
const MAX_TIMING_MS: f32 = 10_000.0;

impl Validate for Telemetry {
    fn validate(&self) -> Result<(), ControlError> {
        check_range("rtt_ms", self.rtt_ms, 0.0, MAX_TIMING_MS)?;
        check_range("jitter_ms", self.jitter_ms, 0.0, MAX_TIMING_MS)?;
        check_range("loss_fraction", self.loss_fraction, 0.0, 1.0)
    }
}

impl Validate for PilotState {
    fn validate(&self) -> Result<(), ControlError> {
        // specs/02-protocolo.md puts the Sync Ratio on a 0-100 scale. A u8 can
        // hold 200, and a shell matching the bands of specs/07 would find no
        // band for it.
        if self.sync_ratio > 100 {
            return Err(ControlError::FieldOutOfRange {
                field: "sync_ratio",
            });
        }
        Ok(())
    }
}

impl Validate for ClientMessage {
    fn validate(&self) -> Result<(), ControlError> {
        match self {
            Self::Hello {
                client,
                nickname,
                public_key,
                ..
            } => {
                check("client", client.len(), MAX_CLIENT_NAME_LEN)?;
                check("nickname", nickname.len(), MAX_NICKNAME_LEN)?;
                // An Ed25519 public key is exactly 32 bytes. Anything else is
                // not a key, and length is the cheapest place to say so.
                if public_key.len() != PUBLIC_KEY_LEN {
                    return Err(ControlError::FieldOutOfRange {
                        field: "public_key",
                    });
                }
                Ok(())
            }
            Self::Response { proof } => check("proof", proof.len(), MAX_PROOF_LEN),
            Self::InsertPlug { password, .. } => check(
                "password",
                password.as_ref().map_or(0, String::len),
                MAX_NICKNAME_LEN,
            ),
            Self::SendMessage { body, .. } => check("body", body.len(), MAX_BODY_LEN),
            Self::CreateCage { name, limit, .. } => {
                check_name("name", name)?;
                check_cage_limit(*limit)
            }
            Self::CreateLine { name }
            | Self::RenameCage { name, .. }
            | Self::RenameLine { name, .. } => check_name("name", name),
            // The operator's own words about their own Dogma, bounded like the
            // other place they cross the wire.
            Self::BanPilot { reason, .. } => check(
                "reason",
                reason.as_ref().map_or(0, String::len),
                MAX_ALERT_TEXT_LEN,
            ),
            Self::EjectPlug
            | Self::JoinLine { .. }
            | Self::FetchHistory { .. }
            | Self::SetAtField(_)
            | Self::SetTotalIsolation(_)
            | Self::SetPresence(_)
            | Self::Ping { .. }
            | Self::KickPilot { .. }
            | Self::RemoveMessage { .. }
            | Self::MovePilot { .. }
            | Self::DeleteCage { .. }
            | Self::DeleteLine { .. }
            | Self::WeighLine { .. } => Ok(()),
        }
    }
}

impl Validate for ServerMessage {
    fn validate(&self) -> Result<(), ControlError> {
        match self {
            Self::Challenge { nonce } => check("nonce", nonce.len(), MAX_PROOF_LEN),
            Self::Session {
                dogma,
                cages,
                lines,
                ..
            } => {
                check("dogma", dogma.len(), MAX_CLIENT_NAME_LEN)?;
                // The same bound the creating verb enforces, applied to the
                // tree on the way out. A Cage whose name came from an older
                // build, or straight from somebody's `sqlite3` prompt, must not
                // travel further than one a client could have asked for.
                for cage in cages {
                    check_name("name", &cage.name)?;
                }
                for line in lines {
                    check_name("name", &line.name)?;
                }
                Ok(())
            }
            Self::CageCreated { cage } => check_name("name", &cage.name),
            Self::LineCreated { line } => check_name("name", &line.name),
            Self::CageRenamed { name, .. } | Self::LineRenamed { name, .. } => {
                check_name("name", name)
            }
            Self::PilotJoined { profile, .. } => {
                check("nickname", profile.nickname.len(), MAX_NICKNAME_LEN)
            }
            Self::MessageReceived { body, .. } | Self::MessageEdited { body, .. } => {
                check("body", body.len(), MAX_BODY_LEN)
            }
            Self::Alert { operator_text, .. } => check(
                "operator_text",
                operator_text.as_ref().map_or(0, String::len),
                MAX_ALERT_TEXT_LEN,
            ),
            Self::Telemetry(telemetry) => telemetry.validate(),
            Self::PilotState(state) => state.validate(),
            Self::PilotLeft { .. }
            | Self::MessageRemoved { .. }
            | Self::Pong { .. }
            | Self::Disconnecting { .. }
            | Self::MovedToCage { .. }
            | Self::CageDeleted { .. }
            | Self::LineDeleted { .. }
            | Self::LineWeighed { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn hello() -> ClientMessage {
        ClientMessage::Hello {
            join_secret: None,
            version: PROTOCOL_VERSION,
            client: "plug/0.0.0".into(),
            nickname: "ayanami".into(),
            public_key: vec![7; PUBLIC_KEY_LEN],
        }
    }

    fn session() -> ServerMessage {
        ServerMessage::Session {
            id: SessionId(7),
            pilot: PilotId(42),
            ssrc: Ssrc(0xABCD),
            dogma: "Terceira Tóquio".into(),
            cages: vec![CageInfo {
                id: CageId(1),
                name: "CAGE-01 CENTRAL".into(),
                limit: 15,
                password_required: false,
                line: Some(LineId(1)),
            }],
            lines: vec![LineInfo {
                id: LineId(1),
                name: "geral".into(),
            }],
            roles: vec![Role {
                id: RoleId(1),
                name: "Pilot".into(),
                permissions: vec![Permission::InsertPlug, Permission::Speak],
            }],
            permissions: vec![Permission::InsertPlug, Permission::Speak],
        }
    }

    #[test]
    fn a_client_message_round_trips() {
        let frame = encode(&hello()).unwrap();
        assert_eq!(decode::<ClientMessage>(&frame).unwrap(), hello());
    }

    #[test]
    fn a_server_message_round_trips() {
        let frame = encode(&session()).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), session());
    }

    #[test]
    fn the_version_byte_comes_first() {
        // specs/02-protocolo.md: "the first byte of every control frame is the
        // protocol version". Asserted at the byte level, because a round-trip
        // test alone would pass with the version anywhere.
        let frame = encode(&hello()).unwrap();
        assert_eq!(frame.first(), Some(&PROTOCOL_VERSION));
    }

    #[test]
    fn a_foreign_version_is_refused_before_the_body_is_read() {
        let mut frame = encode(&hello()).unwrap();
        frame[0] = PROTOCOL_VERSION.wrapping_add(9);
        assert!(matches!(
            decode::<ClientMessage>(&frame),
            Err(ControlError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn an_empty_frame_is_rejected() {
        assert_eq!(decode::<ClientMessage>(&[]), Err(ControlError::Empty));
    }

    #[test]
    fn an_oversized_frame_is_rejected_before_allocating() {
        // specs/08-seguranca.md: size-limited before allocating.
        let frame = vec![PROTOCOL_VERSION; MAX_FRAME_LEN + 1];
        assert!(matches!(
            decode::<ClientMessage>(&frame),
            Err(ControlError::TooLong { .. })
        ));
    }

    #[test]
    fn an_oversized_body_is_refused_in_both_directions() {
        // Enforced on the way out so we cannot send one, and on the way in so a
        // peer cannot skip the check by hand-rolling a frame.
        let long = ClientMessage::SendMessage {
            line: LineId(1),
            body: "x".repeat(MAX_BODY_LEN + 1),
            replies_to: None,
            client_message_id: ClientMessageId(1),
        };
        assert!(matches!(
            encode(&long),
            Err(ControlError::FieldTooLong { field: "body", .. })
        ));

        // Now build the frame anyway, the way a hostile peer would.
        let mut frame = vec![PROTOCOL_VERSION];
        frame = postcard::to_extend(&long, frame).unwrap();
        assert!(matches!(
            decode::<ClientMessage>(&frame),
            Err(ControlError::FieldTooLong { field: "body", .. })
        ));
    }

    #[test]
    fn an_oversized_nickname_is_refused() {
        let long = ClientMessage::Hello {
            join_secret: None,
            version: PROTOCOL_VERSION,
            client: "plug".into(),
            nickname: "n".repeat(MAX_NICKNAME_LEN + 1),
            public_key: vec![7; PUBLIC_KEY_LEN],
        };
        assert!(matches!(
            encode(&long),
            Err(ControlError::FieldTooLong {
                field: "nickname",
                ..
            })
        ));
    }

    #[test]
    fn the_session_carries_the_ssrc() {
        // Gap G1. Without this a client cannot learn its own media source, and
        // specs/02-protocolo.md's "resolve ssrc to pilot from the control
        // channel" has nothing to resolve from.
        let frame = encode(&session()).unwrap();
        let ServerMessage::Session { ssrc, .. } = decode::<ServerMessage>(&frame).unwrap() else {
            panic!("not a session");
        };
        assert_eq!(ssrc, Ssrc(0xABCD));
    }

    #[test]
    fn a_joining_pilot_carries_their_ssrc() {
        // The other half of gap G1: the mapping for everybody else.
        let joined = ServerMessage::PilotJoined {
            cage: CageId(1),
            profile: PilotProfile {
                id: PilotId(2),
                nickname: "shinji".into(),
                roles: vec![RoleId(1)],
            },
            ssrc: Ssrc(99),
        };
        let frame = encode(&joined).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), joined);
    }

    #[test]
    fn pilot_state_carries_both_mute_controls() {
        // Gap G8. specs/07-tema-evangelion.md defines "Isolamento total" and
        // specs/05-cliente-tui.md binds it to a key, but nothing carried it, so
        // a roster could not show who was not listening.
        let state = ServerMessage::PilotState(PilotState {
            pilot: PilotId(1),
            at_field: true,
            total_isolation: true,
            speaking: false,
            presence: Presence::Available,
            sync_ratio: 94,
        });
        let frame = encode(&state).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), state);
    }

    #[test]
    fn sending_a_message_carries_its_idempotency_key() {
        // Gap G9. specs/02-protocolo.md calls the send idempotent by
        // client_msg_id while leaving the field out of the payload. A retry
        // after a lost acknowledgement posts twice without it.
        let send = ClientMessage::SendMessage {
            line: LineId(1),
            body: "verificando harmônicos".into(),
            replies_to: None,
            client_message_id: ClientMessageId(0xFEED),
        };
        let frame = encode(&send).unwrap();
        assert_eq!(decode::<ClientMessage>(&frame).unwrap(), send);
    }

    #[test]
    fn a_control_frame_stays_small() {
        // The control stream shares a connection with voice datagrams. A
        // handshake that needed several kilobytes would be a design smell.
        assert!(encode(&hello()).unwrap().len() < 64);
        assert!(encode(&session()).unwrap().len() < 256);
    }

    #[test]
    fn the_session_says_what_this_pilot_may_do() {
        // `roles` is the Dogma's catalogue; nothing ever said which of them this
        // connection holds. Without this field a shell has no honest way to
        // decide whether to offer a control at all, and the only alternative is
        // to offer everything and let the server refuse — which teaches people
        // that half the buttons do nothing.
        let frame = encode(&session()).unwrap();
        let ServerMessage::Session { permissions, .. } = decode::<ServerMessage>(&frame).unwrap()
        else {
            panic!("not a session");
        };
        assert_eq!(permissions, vec![Permission::InsertPlug, Permission::Speak]);
    }

    #[test]
    fn the_room_making_verbs_round_trip() {
        for message in [
            ClientMessage::CreateCage {
                name: "CAGE-02 SALA DOS FUNDOS".into(),
                limit: 8,
                line: Some(LineId(1)),
            },
            ClientMessage::CreateLine {
                name: "planejamento".into(),
            },
            ClientMessage::RenameCage {
                cage: CageId(2),
                name: "CAGE-02 CENTRAL".into(),
            },
            ClientMessage::RenameLine {
                line: LineId(3),
                name: "avisos".into(),
            },
        ] {
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ClientMessage>(&frame).unwrap(), message);
        }
    }

    #[test]
    fn the_room_making_announcements_round_trip() {
        for message in [
            ServerMessage::CageCreated {
                cage: CageInfo {
                    id: CageId(2),
                    name: "CAGE-02 SALA DOS FUNDOS".into(),
                    limit: 8,
                    password_required: false,
                    line: Some(LineId(1)),
                },
            },
            ServerMessage::LineCreated {
                line: LineInfo {
                    id: LineId(3),
                    name: "planejamento".into(),
                },
            },
            ServerMessage::CageRenamed {
                cage: CageId(2),
                name: "CAGE-02 CENTRAL".into(),
            },
            ServerMessage::LineRenamed {
                line: LineId(3),
                name: "avisos".into(),
            },
        ] {
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ServerMessage>(&frame).unwrap(), message);
        }
    }

    #[test]
    fn a_room_with_no_name_is_refused_in_both_directions() {
        // A blank name draws a row in the channel list with nothing in it: a
        // thing the shell paints, the person clicks, and nobody can name out
        // loud. Whitespace counts as blank — " " is not a name, it is a name
        // somebody forgot to type.
        for blank in ["", " ", "\t\n  "] {
            let ask = ClientMessage::CreateCage {
                name: blank.into(),
                limit: 8,
                line: None,
            };
            assert!(
                matches!(
                    encode(&ask),
                    Err(ControlError::FieldOutOfRange { field: "name" })
                ),
                "accepted a Cage named {blank:?}"
            );

            // And now the way a hostile peer would build it, skipping the
            // encoder entirely.
            let mut frame = vec![PROTOCOL_VERSION];
            frame = postcard::to_extend(&ask, frame).unwrap();
            assert!(
                matches!(
                    decode::<ClientMessage>(&frame),
                    Err(ControlError::FieldOutOfRange { field: "name" })
                ),
                "accepted a hand-rolled Cage named {blank:?}"
            );
        }
    }

    #[test]
    fn an_oversized_room_name_is_refused() {
        let long = ClientMessage::CreateLine {
            name: "n".repeat(MAX_CHANNEL_NAME_LEN + 1),
        };
        assert!(matches!(
            encode(&long),
            Err(ControlError::FieldTooLong { field: "name", .. })
        ));
        // Exactly at the limit is a name, not an overflow.
        assert!(encode(&ClientMessage::CreateLine {
            name: "n".repeat(MAX_CHANNEL_NAME_LEN),
        })
        .is_ok());
    }

    #[test]
    fn a_cage_that_holds_nobody_or_everybody_is_refused() {
        // Zero is far more often a field left at its default than a decision,
        // and a room nobody may enter is not a room. The ceiling stops a u16 of
        // 65 535 being written into a Dogma sized for fifty.
        for limit in [0, MAX_CAGE_LIMIT + 1, u16::MAX] {
            let ask = ClientMessage::CreateCage {
                name: "CAGE-02".into(),
                limit,
                line: None,
            };
            assert!(
                matches!(
                    encode(&ask),
                    Err(ControlError::FieldOutOfRange { field: "limit" })
                ),
                "accepted a Cage for {limit} pilots"
            );

            let mut frame = vec![PROTOCOL_VERSION];
            frame = postcard::to_extend(&ask, frame).unwrap();
            assert!(
                matches!(
                    decode::<ClientMessage>(&frame),
                    Err(ControlError::FieldOutOfRange { field: "limit" })
                ),
                "accepted a hand-rolled Cage for {limit} pilots"
            );
        }

        for limit in [1, MAX_CAGE_LIMIT] {
            assert!(
                encode(&ClientMessage::CreateCage {
                    name: "CAGE-02".into(),
                    limit,
                    line: None,
                })
                .is_ok(),
                "refused a Cage for {limit} pilots"
            );
        }
    }

    #[test]
    fn a_blank_name_never_leaves_the_dogma_inside_a_session() {
        // The tree on the way out gets the same bound the verb enforces on the
        // way in. A row written straight into the database by hand must not
        // reach a shell that has no sentence for it.
        let ServerMessage::Session {
            id,
            pilot,
            ssrc,
            dogma,
            lines,
            roles,
            permissions,
            ..
        } = session()
        else {
            panic!("not a session");
        };
        let blank = ServerMessage::Session {
            id,
            pilot,
            ssrc,
            dogma,
            cages: vec![CageInfo {
                id: CageId(9),
                name: "   ".into(),
                limit: 4,
                password_required: false,
                line: None,
            }],
            lines,
            roles,
            permissions,
        };
        assert!(matches!(
            encode(&blank),
            Err(ControlError::FieldOutOfRange { field: "name" })
        ));
    }

    // ---- moderation ----

    #[test]
    fn the_moderation_verbs_round_trip() {
        for message in [
            ClientMessage::KickPilot { pilot: PilotId(42) },
            ClientMessage::BanPilot {
                pilot: PilotId(42),
                reason: Some("inundou a Linha".into()),
                expires_at: Some(1_700_000_000),
            },
            ClientMessage::BanPilot {
                pilot: PilotId(42),
                reason: None,
                expires_at: None,
            },
            ClientMessage::RemoveMessage {
                message: MessageId(9),
            },
            ClientMessage::MovePilot {
                pilot: PilotId(42),
                cage: CageId(2),
            },
        ] {
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ClientMessage>(&frame).unwrap(), message);
        }
    }

    #[test]
    fn being_moved_round_trips() {
        // The one moderation verb that needed an announcement of its own. A
        // kick and a ban end in `Disconnecting`, a removal is
        // `MessageRemoved`; only "you are somewhere else now" had nothing that
        // could say it, because a client sets its own Cage on the way out.
        let moved = ServerMessage::MovedToCage { cage: CageId(3) };
        let frame = encode(&moved).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), moved);
    }

    #[test]
    fn an_oversized_ban_reason_is_refused_in_both_directions() {
        // The operator's own words are still a bounded field. Enforced on the
        // way out so we cannot send one, and on the way in so a peer cannot
        // skip the check by hand-rolling a frame — the same pair every other
        // text field gets.
        let long = ClientMessage::BanPilot {
            pilot: PilotId(1),
            reason: Some("x".repeat(MAX_ALERT_TEXT_LEN + 1)),
            expires_at: None,
        };
        assert!(matches!(
            encode(&long),
            Err(ControlError::FieldTooLong {
                field: "reason",
                ..
            })
        ));

        let mut frame = vec![PROTOCOL_VERSION];
        frame = postcard::to_extend(&long, frame).unwrap();
        assert!(matches!(
            decode::<ClientMessage>(&frame),
            Err(ControlError::FieldTooLong {
                field: "reason",
                ..
            })
        ));
    }

    #[test]
    fn moderation_asks_for_a_pilot_and_never_for_a_sentence() {
        // specs/02-protocolo.md: no free-form string reaches the interface. A
        // kick that carried "why" as text would be a second error language
        // growing beside the enumerated one, written by whoever is angriest.
        // The one string here is the ban's operator note, which never leaves
        // the Dogma — the person barred meets `DisconnectReason::Banned` and
        // nothing else.
        let frame = encode(&ClientMessage::KickPilot { pilot: PilotId(42) }).unwrap();
        assert!(
            frame.len() < 32,
            "a kick got big enough to be carrying prose: {} bytes",
            frame.len()
        );
    }

    // ---- unmaking a room ----

    #[test]
    fn the_deleting_verbs_round_trip() {
        for message in [
            ClientMessage::DeleteCage { cage: CageId(2) },
            ClientMessage::DeleteLine { line: LineId(7) },
            ClientMessage::WeighLine { line: LineId(7) },
        ] {
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ClientMessage>(&frame).unwrap(), message);
        }
    }

    #[test]
    fn the_weight_of_a_line_survives_the_wire_exactly() {
        // Exactly, and that is the whole test: these three numbers are the
        // sentence a person reads before destroying something that no screen of
        // this product brings back. A count that arrives one off, or a date that
        // arrives as a default because `None` was flattened somewhere, produces
        // a confirmation that lies with total confidence.
        for weighed in [
            ServerMessage::LineWeighed {
                line: LineId(7),
                messages: 1_847,
                authors: 6,
                oldest_at_seconds: Some(1_678_600_000),
            },
            // The empty Line, which is the case with no date to give.
            ServerMessage::LineWeighed {
                line: LineId(7),
                messages: 0,
                authors: 0,
                oldest_at_seconds: None,
            },
        ] {
            let frame = encode(&weighed).unwrap();
            assert_eq!(decode::<ServerMessage>(&frame).unwrap(), weighed);
        }
    }

    #[test]
    fn a_destroyed_room_is_announced_by_identifier_and_nothing_else() {
        // Unlike `CageCreated`, which carries the whole row: there is no row
        // any more, and everything a client needs to stop drawing the room it
        // already has. A frame carrying the name of a room that no longer
        // exists would be inviting some shell to keep it.
        for gone in [
            ServerMessage::CageDeleted { cage: CageId(2) },
            ServerMessage::LineDeleted { line: LineId(7) },
        ] {
            let frame = encode(&gone).unwrap();
            assert_eq!(decode::<ServerMessage>(&frame).unwrap(), gone);
            assert!(
                frame.len() < 16,
                "a deletion announcement got big enough to be carrying a room: {} bytes",
                frame.len()
            );
        }
    }

    #[test]
    fn being_turned_out_of_a_room_has_its_own_reason_for_each_kind() {
        // Two reasons and not one, because they are two sentences: the room you
        // were speaking in is gone, and the conversation you were reading was
        // destroyed. A shell handed one reason for both writes the vaguer of the
        // two, which is the shape `OperatorNotice` already has and the reason
        // neither of these is it.
        let cage = ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::CageDeleted,
            operator_text: None,
        };
        let line = ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::LineDeleted,
            operator_text: None,
        };
        // And the refusal of the last Cage is a third sentence, not either of
        // these two: "make another room first" is not "the room is gone".
        let ultimo = ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::LastCage,
            operator_text: None,
        };
        assert_ne!(cage, line);
        assert_ne!(cage, ultimo);
        assert_ne!(line, ultimo);
        for alert in [cage, line, ultimo] {
            let frame = encode(&alert).unwrap();
            assert_eq!(decode::<ServerMessage>(&frame).unwrap(), alert);
        }
    }

    proptest! {
        /// The parser is the untrusted-input surface. Totality first.
        #[test]
        fn decoding_arbitrary_bytes_never_panics(bytes: Vec<u8>) {
            let _ = decode::<ClientMessage>(&bytes);
            let _ = decode::<ServerMessage>(&bytes);
        }

        /// A frame with the right version but random body must be rejected
        /// cleanly rather than producing a plausible message.
        #[test]
        fn a_correct_version_does_not_excuse_a_bad_body(body: Vec<u8>) {
            let mut frame = vec![PROTOCOL_VERSION];
            frame.extend_from_slice(&body);
            if let Ok(message) = decode::<ClientMessage>(&frame) {
                // Anything accepted must survive a re-encode unchanged.
                prop_assert_eq!(decode::<ClientMessage>(&encode(&message).unwrap()).unwrap(), message);
            }
        }

        /// Round-trip over the whole shape of a message that carries text.
        #[test]
        fn text_messages_round_trip(
            body in ".{0,200}",
            line: u32,
            id: u64,
        ) {
            let send = ClientMessage::SendMessage {
                line: LineId(line),
                body,
                replies_to: None,
                client_message_id: ClientMessageId(id),
            };
            let frame = encode(&send).unwrap();
            prop_assert_eq!(decode::<ClientMessage>(&frame).unwrap(), send);
        }
    }
}

#[cfg(test)]
mod numeric_tests {
    use super::*;

    fn telemetry(rtt: f32, jitter: f32, loss: f32) -> ServerMessage {
        ServerMessage::Telemetry(Telemetry {
            rtt_ms: rtt,
            jitter_ms: jitter,
            loss_fraction: loss,
            subsystems: vec![(Subsystem::Melchior, SubsystemHealth::Nominal)],
        })
    }

    #[test]
    fn sane_telemetry_round_trips() {
        let good = telemetry(38.0, 12.0, 0.002);
        let frame = encode(&good).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), good);
    }

    #[test]
    fn a_nan_never_reaches_the_sync_ratio() {
        // Found by fuzzing on the first run. specs/02-protocolo.md derives the
        // Sync Ratio from these three numbers, and every comparison against NaN
        // is false — so the bands in specs/07-tema-evangelion.md would not error,
        // they would quietly pick the wrong colour.
        for message in [
            telemetry(f32::NAN, 12.0, 0.0),
            telemetry(38.0, f32::NAN, 0.0),
            telemetry(38.0, 12.0, f32::NAN),
        ] {
            assert!(matches!(
                encode(&message),
                Err(ControlError::FieldOutOfRange { .. })
            ));
        }
    }

    #[test]
    fn infinities_are_refused_too() {
        for value in [f32::INFINITY, f32::NEG_INFINITY] {
            assert!(matches!(
                encode(&telemetry(value, 0.0, 0.0)),
                Err(ControlError::FieldOutOfRange { .. })
            ));
        }
    }

    #[test]
    fn negative_and_impossible_readings_are_refused() {
        assert!(encode(&telemetry(-1.0, 0.0, 0.0)).is_err(), "negative rtt");
        assert!(
            encode(&telemetry(0.0, -1.0, 0.0)).is_err(),
            "negative jitter"
        );
        assert!(
            encode(&telemetry(0.0, 0.0, 1.5)).is_err(),
            "loss above 100%"
        );
        assert!(encode(&telemetry(0.0, 0.0, -0.1)).is_err(), "negative loss");
    }

    #[test]
    fn a_hostile_peer_cannot_smuggle_a_nan_past_the_encoder() {
        // The check has to be on the way in as well, or a hand-rolled frame
        // walks straight through.
        let hostile = Telemetry {
            rtt_ms: f32::NAN,
            jitter_ms: 0.0,
            loss_fraction: 0.0,
            subsystems: Vec::new(),
        };
        let mut frame = vec![PROTOCOL_VERSION];
        frame = postcard::to_extend(&ServerMessage::Telemetry(hostile), frame).unwrap();
        assert!(matches!(
            decode::<ServerMessage>(&frame),
            Err(ControlError::FieldOutOfRange { .. })
        ));
    }

    #[test]
    fn a_sync_ratio_above_one_hundred_is_refused() {
        // specs/02-protocolo.md puts it on a 0-100 scale; a u8 holds more, and
        // no band in specs/07 covers 200.
        let state = ServerMessage::PilotState(PilotState {
            pilot: PilotId(1),
            at_field: false,
            total_isolation: false,
            speaking: true,
            presence: Presence::Available,
            sync_ratio: 200,
        });
        assert!(matches!(
            encode(&state),
            Err(ControlError::FieldOutOfRange {
                field: "sync_ratio"
            })
        ));
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn a_hello_without_a_real_key_is_refused() {
        // The handshake answers Hello with a nonce and then verifies a signature.
        // A key of the wrong length cannot verify anything, and finding that out
        // at signature-check time means the server has already done work for a
        // peer that was never going to succeed.
        for len in [0, 16, 31, 33, 64] {
            let hello = ClientMessage::Hello {
                join_secret: None,
                version: PROTOCOL_VERSION,
                client: "plug".into(),
                nickname: "ayanami".into(),
                public_key: vec![0; len],
            };
            assert!(
                matches!(
                    encode(&hello),
                    Err(ControlError::FieldOutOfRange {
                        field: "public_key"
                    })
                ),
                "accepted a {len}-byte public key"
            );
        }
    }

    #[test]
    fn a_thirty_two_byte_key_is_accepted() {
        let hello = ClientMessage::Hello {
            join_secret: None,
            version: PROTOCOL_VERSION,
            client: "plug".into(),
            nickname: "ayanami".into(),
            public_key: vec![9; PUBLIC_KEY_LEN],
        };
        let frame = encode(&hello).unwrap();
        assert_eq!(decode::<ClientMessage>(&frame).unwrap(), hello);
    }
}
