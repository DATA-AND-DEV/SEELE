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
//! - **G1.** The spec says the client resolves `ssrc → person` "from the table
//!   received on the control channel", but no control message carried an `ssrc`,
//!   and a client had no way to learn its own. [`ServerMessage::Session`] and
//!   [`ServerMessage::PersonJoined`] now carry it.
//! - **G8.** "Isolamento total" (deafen) is defined in
//!   `specs/07-tema-evangelion.md` and bound to a key in `specs/05-cliente-tui.md`,
//!   but had no protocol representation, so a roster could not show who was not
//!   listening. [`PersonState`] carries it beside the A.T. Field.
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
    AttachmentId, ChannelId, ClientMessageId, MessageId, PersonId, RoleId, ScreenId, SessionId,
    Ssrc, VoiceRoomId,
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
/// channel and far less than anything that would strain a frame.
pub const MAX_BODY_LEN: usize = 4 * 1024;

/// Longest nickname, in bytes.
pub const MAX_NICKNAME_LEN: usize = 32;

/// Longest client name, in bytes.
pub const MAX_CLIENT_NAME_LEN: usize = 64;

/// Longest voice room or Channel name, in bytes.
///
/// A name is a label in a list, not a description. Long enough for
/// `SALA-01 CENTRAL` several times over, short enough that no shell has to
/// decide where to cut one off.
pub const MAX_CHANNEL_NAME_LEN: usize = 48;

/// Largest number of people a voice room may be created with.
///
/// `specs/04-servidor-seele.md` sizes the target at "50 sessões e 5 voice_rooms
/// ativos em 1 vCPU / 512 MB", so this is five times the whole server: generous
/// rather than tight, and there to stop a `u16` of 65 535 being written into a
/// room nobody could fill.
pub const MAX_VOICE_ROOM_LIMIT: u16 = 250;

/// Longest operator-supplied alert text, in bytes.
pub const MAX_ALERT_TEXT_LEN: usize = 512;

/// Largest server icon, in bytes.
///
/// An icon is the one thing in this enum a stranger's machine both **stores**
/// and **re-broadcasts**, so the number is a security limit and not a taste.
/// Three things fix it:
///
/// - **It travels in a frame of its own, never inside
///   [`ServerMessage::Session`]**, so the only thing it shares
///   [`MAX_FRAME_LEN`] with is the postcard header. At 8 KiB the message sits
///   at half the frame cap, which keeps the two refusals distinguishable: an
///   oversized icon is refused *as an icon* rather than as a frame nobody can
///   explain. ADR 0032 refused the icon inside the handshake for the same
///   reason it is not there now — a decoration must never cost the connection.
/// - **8 KiB accepts the picture and refuses the photograph.** A 256×256 PNG
///   of flat shapes with alpha — a badge, which is all this is ever drawn as —
///   lands between 2 and 6 KiB. Anything that needs more than 8 KiB at that
///   size is a photograph, and a photograph shrunk to a badge is unreadable
///   anyway.
/// - **Whoever hosts pays for it fifty times.** Changing the icon writes one
///   row and then sends it to every connected session; `specs/04-servidor-seele.md`
///   sizes a server at ~50 people, so one change costs 50 × 8 KiB ≈ 400 KiB of a
///   home upstream. That is a hiccup. With no ceiling at all the same act is an
///   upload channel with a fifty-fold amplifier pointed at a machine somebody
///   runs in their living room, which is the whole reason there is a number
///   here rather than a shrug.
///
/// ADR 0032 wrote 16 KiB into its "when it is wanted" section, before there was
/// a frame to put it in; halving it is what buys the property above — the icon
/// can never be the reason a control frame is refused.
pub const MAX_SERVER_ICON_LEN: usize = 8 * 1024;

/// Largest side of a server icon, in pixels.
///
/// Not redundant with [`MAX_SERVER_ICON_LEN`], and this is the half that is easy
/// to leave out: PNG compresses uniform colour to almost nothing, so 8 KiB is
/// enough to declare a 20 000 × 20 000 image that costs 1.6 GB the moment
/// anything decodes it. The bytes are small; the picture is not. Refusing the
/// **declared** size here is what stops a decoration from being an
/// out-of-memory kill on every machine that draws it.
///
/// 256 because the badge it is drawn in is 56 px in the v3 comp, and 256 covers
/// that at four times the density with nothing left over.
pub const MAX_SERVER_ICON_SIDE: u32 = 256;

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
    /// The bytes are on the server and may be fetched.
    Available,
    /// The bytes were evicted to keep the server under its ceiling.
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

/// Why a server would not take, or would not hand back, a file.
///
/// Every one of these is a refusal somebody is waiting on, so each says
/// something a shell can turn into a different sentence. A single
/// "attachment failed" would leave a person retrying a file that will never fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentRefusal {
    /// The person lacks [`Permission::AttachFile`].
    NotAllowed,
    /// Larger than this server's per-file limit.
    ///
    /// Carries the limit, because "too big" without a number sends somebody to
    /// try again with a file that is also too big.
    TooLarge {
        /// The largest file this server accepts, in bytes.
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
    /// The one question ADR 0027 says a server can answer about a file: did it
    /// arrive whole. It says nothing about whether the file is good.
    HashDidNotMatch,
    /// The person is sending bytes faster than their budget.
    RateLimited,
    /// This server is not storing attachments at all.
    Unavailable,
    /// No such attachment, or it belongs to o canal this person may not read.
    NotFound,
    /// The bytes were evicted to keep the server under its ceiling.
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

/// Presence, as announced by the person.
///
/// Deliberately short. `specs/00-visao-geral.md` names "published presence" as
/// one of the things that made the tools it is reacting against unpleasant, so
/// this stays a hint the person sets rather than anything inferred from activity.
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
    /// See that a voice room exists.
    ViewVoiceRoom,
    /// Enter a voice room.
    EnterVoiceRoom,
    /// Transmit voice.
    Speak,
    /// Read o canal.
    ReadChannel,
    /// Post to o canal.
    WriteChannel,
    /// Delete somebody else's message.
    RemoveMessage,
    /// Move a person between voice_rooms.
    MovePerson,
    /// Disconnect a person.
    Kick,
    /// Bar a person from returning.
    Ban,
    /// Create and configure voice_rooms.
    ManageVoiceRooms,
    /// Create and assign roles.
    ManageRoles,
    /// Everything else about the server.
    AdministerServer,

    /// Put a file on the server's disk. ADR 0027.
    ///
    /// **Not folded into [`Self::WriteChannel`].** "May write" and "may put a
    /// gigabyte on my laptop" are different questions, and the point of hosting
    /// for your own friends is being able to answer the second one separately.
    /// The permission is what the ceiling cannot do: the ceiling bounds the
    /// disk, and nothing bounds *whose* history gets pushed out of it — so the
    /// only real lever over that is deciding who may push.
    ///
    /// Appended last, for the reason [`AlertReason::RateLimited`] gives: a build
    /// one protocol version older refuses the frame rather than reading this as
    /// a permission it already understands.
    ///
    /// Migration 3 seeds it on Commander, Operator and Person, and **denies it
    /// explicitly on Observer** rather than merely leaving it out — the schema
    /// already writes why on the Observer's channel: denying on purpose makes
    /// granting Observer to somebody who is also a pessoa *silence* them,
    /// instead of quietly doing nothing.
    AttachFile,
}

/// A role and the permissions it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// Identifier.
    pub id: RoleId,
    /// Display name. One of Commander, Operator, Person, Observer for the four
    /// defaults in `specs/04-servidor-seele.md`, but operators may add more.
    pub name: String,
    /// What the role allows.
    ///
    /// `specs/04-servidor-seele.md`: denied beats granted, and there is no tree
    /// inheritance. A permission absent from every one of a person's roles is
    /// denied.
    pub permissions: Vec<Permission>,
}

/// A person as other people see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonProfile {
    /// Account identifier.
    pub id: PersonId,
    /// Display name.
    pub nickname: String,
    /// Roles held.
    pub roles: Vec<RoleId>,
}

/// A voice channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceRoomInfo {
    /// Identifier.
    pub id: VoiceRoomId,
    /// Display name.
    pub name: String,
    /// How many people may be inside at once.
    pub limit: u16,
    /// Whether entry needs a password.
    pub password_required: bool,
    /// A Channel bound to this voice room, if any. `specs/04-servidor-seele.md` makes the
    /// association optional.
    pub channel: Option<ChannelId>,
}

/// A text channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelInfo {
    /// Identifier.
    pub id: ChannelId,
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
    Permissions,
    /// Media routing: voice room subscriptions, datagram forwarding, bandwidth.
    Media,
    /// Persistent state: voice_rooms, Channels, history, configuration, migrations.
    Persistence,
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

/// What a person's client is currently doing.
///
/// Carries both mute controls. `specs/07-tema-evangelion.md` names them
/// "A.T. Field" (microphone) and "Isolamento total" (speakers); the second had
/// no protocol representation before — gap G8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonState {
    /// Whose state this is.
    pub person: PersonId,
    /// Microphone muted — "A.T. Field" active.
    pub muted: bool,
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
    pub signal: u8,
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
    /// An operator disconnected this person.
    Kicked,
    /// An operator barred this person.
    Banned,
    /// The server is full.
    ServerFull,
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

    /// This connection fell so far behind the server's events that some were lost.
    ///
    /// The bus a server broadcasts on is a fixed ring. A connection that stops
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
    /// happened while this was silent instead: the person stayed connected with
    /// a gap in the conversation that neither end could name.
    ///
    /// Appended last, for the reason [`AlertReason::RateLimited`] gives.
    FellBehind,

    /// The knock reached the host and has not been decided yet. ADR 0030.
    ///
    /// The third admission layer is TOFU applied to people: a key nobody has
    /// approved yet does not get in, and does not get turned away either — the
    /// request is written down and the connection ends immediately.
    ///
    /// Nothing waits, and that is the design rather than a shortcut. Holding
    /// the connection open would force a deadline, and a deadline manufactures
    /// a third answer — "nobody was there" — that whoever knocked cannot act
    /// on. A standing request the host can grant hours later is a stronger
    /// promise than a bar that does not move, and it costs the server nothing to
    /// keep, which is the same argument [`Self::CredentialRejected`] makes for
    /// refusing before the signature is checked.
    ///
    /// **This one is deliberately distinguishable**, unlike the refusals
    /// `specs/08-seguranca.md` requires to be uniform. Those are uniform
    /// because a caller guessing a secret would learn which guess landed
    /// closer. Here nothing was guessed: the peer proved a key it holds, and
    /// the answer is about that peer alone. That a server has a doorkeeper is
    /// not a secret worth keeping — somebody who is not told to wait leaves
    /// believing the address was wrong.
    ///
    /// Appended last, for the reason [`AlertReason::RateLimited`] gives.
    AdmissionPending,

    /// The host looked at the knock and said no. ADR 0030.
    ///
    /// Distinct from [`Self::AdmissionPending`] for the reason that variant
    /// gives, and distinct from [`Self::Banned`] because it is milder: a
    /// refusal here is undone by the host approving the same key later, and it
    /// never ended a session that was already running.
    ///
    /// Appended last, for the reason [`AlertReason::RateLimited`] gives.
    AdmissionDenied,

    /// The nickname asked for belongs to a different key. ADR 0017.
    ///
    /// # Why this is not [`Self::CredentialRejected`]
    ///
    /// It used to be, and it cost somebody an evening. The report: "I host a
    /// server, somebody knocks, I approve them — and they still get credential
    /// rejected, even after closing the app." Four unrelated failures wore that
    /// one sentence, and this is the one that does not go away by trying again,
    /// by being approved, or by reinstalling: the name is simply somebody
    /// else's, and nothing but choosing another will move it.
    ///
    /// `specs/08-seguranca.md` requires login failures to be uniform so that
    /// somebody guessing a secret learns nothing from which guess landed
    /// closer. That rule does not reach here, and the position in the handshake
    /// is the argument: this refusal happens **after** the signature is checked
    /// and **after** the doorkeeper admitted the peer. Nothing was guessed. And
    /// on a server with no doorkeeper — the only case where an anonymous peer
    /// reaches this channel — the nicknames it could enumerate are the ones the
    /// roster hands to anybody who walks in.
    ///
    /// Appended last, for the reason [`AlertReason::RateLimited`] gives.
    NicknameTaken,
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

    /// An operator moved this person's connection into another voice room.
    ///
    /// Its own reason rather than [`Self::OperatorNotice`], because the shell
    /// has a specific sentence to write and `OperatorNotice` would have it
    /// write "the operator is saying something" beside a room that changed on
    /// its own. Being moved without being told is the case this exists to stop.
    ///
    /// Appended after `RateLimited`, for the reason that variant gives.
    MovedByOperator,

    /// The voice room this person's connection was in no longer exists.
    ///
    /// Its own reason rather than [`Self::OperatorNotice`], for the reason
    /// [`Self::MovedByOperator`] gives: the shell has a specific sentence to
    /// write, and being turned out of a room in the middle of speaking is the
    /// case this exists to explain. Sent only to the people who were inside —
    /// everybody else learns the room is gone from
    /// [`ServerMessage::VoiceRoomDeleted`] and has nothing to be told about it.
    ///
    /// Appended after `MovedByOperator`, for the reason that variant gives.
    VoiceRoomDeleted,

    /// A Channel this person had open no longer exists, and neither does anything
    /// written in it.
    ///
    /// Separate from [`Self::VoiceRoomDeleted`] because the two sentences are not
    /// the same sentence: one says the room you were speaking in is gone, the
    /// other says the conversation you were reading was destroyed. A shell
    /// given one reason for both would have to write the vaguer of the two.
    ChannelDeleted,

    /// The voice room asked about is the only one the server has, so it stays.
    ///
    /// A refusal with a sentence of its own, and that is the whole reason it
    /// exists: the nearest existing reason is [`Self::VoiceRoomEntryRefused`], which
    /// every shell writes as "entry refused" — a sentence about walking into a
    /// room, in front of somebody who was trying to destroy one. What this has
    /// to say is "make another room first", and no other variant says it.
    ///
    /// The app disables the control on the last voice room and says as much in a
    /// `title`, which is where the reader meets this first. This is the half
    /// that survives an older shell, and `specs/08-seguranca.md` puts the rule
    /// on the server for exactly that reason.
    LastVoiceRoom,

    /// Somebody is already sharing a screen in this voice room.
    ///
    /// §6 item 3 of the screen-sharing design puts one transmission per voice
    /// room in v1 — "two double the download of everybody watching and triple
    /// the interface" — so two people pressing the button within the same
    /// second is an ordinary race, not misuse, and the loser has to be told
    /// something true. The nearest existing reason is [`Self::PermissionDenied`],
    /// which every shell writes as "you may not do that": a sentence about the
    /// person, in front of somebody who may do it and simply has to wait.
    ///
    /// §2 asks that "o botão de compartilhar não pode falhar. Ou ele não está
    /// lá, ou ele explica o que falta"; this is the half of that promise the
    /// server owes, since only the server knows the room is taken.
    ///
    /// Appended after `LastVoiceRoom`, for the reason [`Self::RateLimited`] gives.
    ScreenShareTaken,

    /// The server stopped this person's transmission: the room outgrew its uplink.
    ///
    /// §5.1 made the host's upward path a term of the ceiling —
    /// `caminho de quem HOSPEDA × 60% ÷ N espectadores` — because the servidor is
    /// what lifts `N` copies. Past some `N` not even the floor of §2 fits, and
    /// §3.2 says what happens then: *"quando o sinal cai de faixa, quem baixa é
    /// o vídeo; se continuar caindo, quem para é o vídeo."* The alternative is
    /// the whole room stuttering because of a screen, which is the one thing
    /// that section calls the product broken.
    ///
    /// Its own reason, and sent only to the person who was sharing.
    /// [`ServerMessage::ScreenShareStopped`] goes to the whole voice room and carries
    /// no reason on purpose — the two ordinary endings tell themselves apart —
    /// and this is the third: somebody who pressed stop knows they pressed it,
    /// and somebody stopped by the server would otherwise learn nothing at all.
    /// The nearest existing reason is [`Self::SyncDegraded`], which every shell
    /// writes as "signal falling" — a sentence about this person's connection,
    /// in front of somebody whose connection is fine and whose audience grew.
    ///
    /// Appended after `ScreenShareTaken`, for the reason [`Self::RateLimited`]
    /// gives.
    ScreenShareOverHostUplink,
}

/// Client to server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Opens the handshake. `specs/02-protocolo.md`.
    Hello {
        /// Convite de uso único ou senha do servidor, quando ele exige um.
        ///
        /// Viaja no `Hello`, portanto **antes** do desafio-resposta. Isso é
        /// deliberado: o segredo diz se esta conexão tem direito de existir, e
        /// gastar um desafio criptográfico com quem nem devia estar batendo à
        /// porta é trabalho de graça para quem varre a internet. O canal já é
        /// TLS 1.3 desde o primeiro byte, então o segredo nunca vai em claro.
        ///
        /// `None` num servidor aberto é o caso normal.
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
        /// which needs a key to verify against. In M3, PERMISSIONS looks this key
        /// up against known accounts; in M2 there is no persistence, so proving
        /// possession of the key is all the handshake can establish.
        public_key: Vec<u8>,
    },
    /// Answers the server's challenge.
    Response {
        /// Proof of identity. ADR 0004 makes this an Ed25519 signature.
        proof: Vec<u8>,
    },
    /// Enters a voice room. "Inserir connection" in `docs/glossario.md`.
    EnterVoiceRoom {
        /// Which voice room.
        voice_room: VoiceRoomId,
        /// Password, if the voice room needs one.
        password: Option<String>,
    },
    /// Leaves the current voice room. "Ejetar".
    LeaveVoiceRoom,
    /// Subscribes to a text channel.
    JoinChannel {
        /// Which Channel.
        channel: ChannelId,
    },
    /// Posts a message.
    SendMessage {
        /// Which Channel.
        channel: ChannelId,
        /// Body.
        body: String,
        /// Message being replied to.
        replies_to: Option<MessageId>,
        /// Client-chosen identifier, making the send idempotent — gap G9.
        client_message_id: ClientMessageId,
    },
    /// Fetches history. Cursor-paged, never by offset.
    FetchHistory {
        /// Which Channel.
        channel: ChannelId,
        /// Where to continue from. `None` starts at the newest.
        cursor: Option<MessageId>,
        /// How many messages.
        limit: u16,
    },
    /// Mutes or unmutes the microphone.
    SetMuted(bool),
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
    // All four need [`Permission::ManageVoiceRooms`]. `specs/04-servidor-seele.md`
    // enumerates one permission for channels and not two — there is no
    // `gerenciar_linhas` — so the one it names covers both kinds, and the
    // server checks it on every one of these.
    /// Creates a voice room.
    CreateVoiceRoom {
        /// What to call it.
        name: String,
        /// How many people may be inside at once.
        limit: u16,
        /// A Channel to bind to it, if any. `specs/04-servidor-seele.md` makes the
        /// association optional.
        channel: Option<ChannelId>,
    },
    /// Creates o canal.
    CreateChannel {
        /// What to call it.
        name: String,
    },
    /// Renames a voice room.
    RenameVoiceRoom {
        /// Which voice room.
        voice_room: VoiceRoomId,
        /// The new name.
        name: String,
    },
    /// Renames o canal.
    RenameChannel {
        /// Which Channel.
        channel: ChannelId,
        /// The new name.
        name: String,
    },

    // ---- moderation ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // `specs/04-servidor-seele.md` enumerates `expulsar`, `banir`,
    // `remover_mensagem` and `mover_pessoa`, migration 1 seeds all four on the
    // Comandante and the Operador, and until now **no message carried any of
    // them**. The permissions existed and there was nothing to ask for: the
    // app's `EJETAR PLUG DO OPERADOR` has been drawn and disabled since v2 for
    // exactly that reason.
    //
    // One permission each, checked by PERMISSIONS at the instant the verb is used
    // — not from anything the handshake cached. `specs/08-seguranca.md`: "Toda
    // ação é verificada no servidor, sempre, mesmo que o cliente já esconda o
    // botão."
    /// Ends a person's session. `expulsar` — [`Permission::Kick`].
    ///
    /// This session, and nothing beyond it: the person may reconnect at once.
    /// Barring a return is [`Self::BanPerson`], and the two are separate verbs
    /// because they are separate decisions — "leave the room" is not "never
    /// come back", and an operator who wanted the first and got the second has
    /// no way to take it back except by finding the row.
    KickPerson {
        /// Who.
        person: PersonId,
    },
    /// Bars a person from returning. `banir` — [`Permission::Ban`].
    ///
    /// Ends their session too: a ban that let the offender stay until they
    /// chose to leave would be a ban that does nothing to the thing that
    /// prompted it.
    BanPerson {
        /// Who.
        person: PersonId,
        /// The operator's own words, for the operator's own record.
        ///
        /// Not a hole in the enumerated-reasons rule, and the same exception
        /// [`ServerMessage::Alert::operator_text`] documents: an operator
        /// writing about their own server is data, not an error reason. It is
        /// **not** sent to the person banned — `specs/08-seguranca.md` wants
        /// uniform failures, and the refusal they meet on the way back in is
        /// the same [`DisconnectReason::Banned`] whatever this says.
        reason: Option<String>,
        /// When the ban lifts, in seconds since the Unix epoch. `None` is
        /// permanent.
        expires_at: Option<i64>,
    },
    /// Takes a message off the Channel. `remover_mensagem` —
    /// [`Permission::RemoveMessage`].
    ///
    /// The permission is worded "delete somebody **else's** message", so an
    /// author removing their own does not need it. That is not a courtesy: a
    /// server where taking back your own typo requires an operator is a server
    /// where people ask an operator about typos.
    ///
    /// Carries no canal. The identifier is the server's own and globally
    /// unique, so the Channel to announce the removal on is read out of the stored
    /// row rather than taken from the asker — a field the client fills in is a
    /// field the client can fill in wrong, and this one would aim somebody
    /// else's announcement.
    RemoveMessage {
        /// Which message.
        message: MessageId,
    },
    /// Moves a person into a voice room. `mover_pessoa` — [`Permission::MovePerson`].
    ///
    /// The person is told, and told *what happened*: they get
    /// [`ServerMessage::MovedToVoiceRoom`] so their client follows, and an
    /// [`ServerMessage::Alert`] carrying [`AlertReason::MovedByOperator`] so
    /// they read a sentence rather than finding themselves somewhere else with
    /// no explanation. Being moved silently is indistinguishable from a client
    /// that lost track of which room it was in.
    MovePerson {
        /// Who.
        person: PersonId,
        /// Where to.
        voice_room: VoiceRoomId,
    },

    // ---- unmaking a room ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // # Why these are not [`Permission::ManageVoiceRooms`]
    //
    // The four verbs above them are: creating a room and renaming one are both
    // things a mistake survives — the wrong name is renamed again, the room
    // nobody wanted is destroyed. Destroying is the one room verb that ends
    // somebody **else's** writing, and no screen of this product brings it
    // back.
    //
    // `specs/04-servidor-seele.md` enumerates `gerenciar_voice_rooms` as "criar e
    // configurar salas de voz" and `administrar_server` as "todo o resto sobre o
    // server". Destroying every message six people wrote is not configuration.
    // So it sits on [`Permission::AdministerServer`], which migration 1 seeds on
    // the Comandante alone — and the separation is real rather than notional
    // the moment an operator makes a role that may build rooms without being
    // able to unmake them. It is deliberately **not** the moderation
    // permissions either: `specs/04` gives the Operador `expulsar` and `banir`,
    // and somebody trusted to remove a person for the evening is not thereby
    // trusted to destroy the server's history.
    /// Destroys a voice room. `apagar_voice_room` — [`Permission::AdministerServer`].
    ///
    /// Everybody inside is turned out of it: a voice room does not vanish from under
    /// the feet of the people speaking in it. They are told, with
    /// [`AlertReason::VoiceRoomDeleted`], for the reason
    /// [`AlertReason::MovedByOperator`] gives.
    ///
    /// A Channel bound to the voice room is **not** destroyed with it.
    /// `specs/04-servidor-seele.md` makes voice_rooms and Channels independent, and the
    /// association optional; destroying a voice room is no statement about the
    /// writing that happened to hang off it.
    ///
    /// The server refuses the last one. A server with na sala de voz has nowhere to
    /// speak, which is the product's first sentence — and the refusal is a
    /// refusal rather than a silence, so a shell can say why.
    DeleteVoiceRoom {
        /// Which voice room.
        voice_room: VoiceRoomId,
    },
    /// Destroys o canal and everything written in it. `apagar_linha` —
    /// [`Permission::AdministerServer`].
    ///
    /// Not a soft delete, unlike [`Self::RemoveMessage`]. That one keeps a row
    /// so replies do not dangle and an operator can answer "what was removed";
    /// this destroys the Channel those rows hang from, so there is nothing left
    /// for either to be about.
    ///
    /// Any voice room bound to this Channel keeps existing and loses the binding.
    DeleteChannel {
        /// Which Channel.
        channel: ChannelId,
    },
    /// Asks what destroying o canal would cost, without destroying anything.
    ///
    /// A read, and the only verb in this enum that exists for a **sentence**: a
    /// confirmation that says "this destroys 1.847 messages by 6 people,
    /// written since 12/03" needs those three numbers counted in the database
    /// at the instant of asking. A client cannot count them for itself — it
    /// holds one page of history and would guess low by whatever the Channel's
    /// whole past is — and a number that is nearly right in a box promising
    /// destruction is worse than no number at all.
    ///
    /// Needs no permission. Answering it tells a person how much is in o canal
    /// they may already read, and refusing it would only mean the confirmation
    /// they see is the vaguer one.
    WeighChannel {
        /// Which Channel.
        channel: ChannelId,
    },

    // ---- attachments ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // Only one verb, and that is the shape of ADR 0027: **sending** a file does
    // not cross the control stream at all. It opens its own unidirectional
    // stream, because twenty megabytes on the ordered control stream stop every
    // presence event and every channel of text behind them. What comes back here
    // is the answer.
    /// Asks for an attachment's bytes.
    ///
    /// The server opens a unidirectional stream back and writes an
    /// `attachment::AttachmentDelivery` followed by the file. When it will not,
    /// it answers [`ServerMessage::AttachmentUnavailable`] here, with an
    /// enumerated reason — which is how «this file expired» reaches a screen at
    /// all.
    ///
    /// Needs [`Permission::ReadChannel`] and nothing more: a file hanging off a
    /// message somebody may read is part of that message.
    FetchAttachment {
        /// Which attachment.
        attachment: AttachmentId,
    },

    // ---- what the server calls itself ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // Both need [`Permission::AdministerServer`], and **not** the
    // [`Permission::ManageVoiceRooms`] the four room verbs use. The channel
    // `specs/04-servidor-seele.md` draws is between "criar e configurar voice_rooms"
    // and "todo o resto sobre o servidor"; the name and the picture of the servidor
    // itself are not a room, and whoever may build rooms is not thereby the
    // person whose server it is. Migration 1 seeds `AdministerServer` on the
    // Comandante alone, and `Permissions::seat_the_arrival` gives the Comandante to
    // whoever connects to their own server first — so this reaches exactly the
    // person who pressed the button, plus anybody they deliberately promoted.
    //
    // ADR 0032 wanted the name written through an accessor on `Hospedagem`,
    // straight into the PERSISTENCE on the machine, with no protocol verb at all —
    // the argument being that naming a server is a decision for whoever holds
    // the file. That argument is sound and it answers a different question:
    // it covers the app that is hosting **in this process**, and leaves a server
    // run by `seeled` on a VPS with no way to be named by the person who
    // administers it. Both paths write the same row, and the permission above
    // is what makes the wire path no wider than the local one.
    /// Renames the server. [`Permission::AdministerServer`].
    ///
    /// Bounded by [`MAX_CLIENT_NAME_LEN`], which is what
    /// [`ServerMessage::Session`] already carries the name under, and refused
    /// blank for the reason `check_name` gives about a voice room: a header with
    /// nothing in it is a thing nobody can refer to out loud.
    RenameServer {
        /// The new name.
        name: String,
    },
    /// Sets or clears the server's icon. [`Permission::AdministerServer`].
    ///
    /// `None` takes the icon off, and that is a verb rather than a gap: an
    /// operator who wants no picture must be able to say so after having said
    /// otherwise, and the alternative — writing a blank one — leaves every
    /// client drawing an empty square.
    ///
    /// **PNG, and only PNG**, checked against the file's own first bytes by
    /// [`Validate`]. The type is fixed by the message rather than declared
    /// beside the bytes, so there is no claim for the content to disagree with
    /// — which is the failure ADR 0027 names when it says nothing decides what
    /// to decode from a sender's word alone. PNG because a badge on a dark
    /// surface needs alpha and must not lose it to a re-encode, because every
    /// platform this product draws on decodes it without a codec being chosen,
    /// and because fixing one format is what lets the two formats that would
    /// hurt be refused by construction: a GIF, which `specs/07-tema-evangelion.md`
    /// would have animating in a badge, and an SVG, which is a document with
    /// script and network fetches in it rather than a picture.
    SetServerIcon {
        /// The picture, or `None` to have none.
        icon: Option<Vec<u8>>,
    },

    // ---- screen sharing ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // Three verbs, and §3.6 of
    // `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`
    // asks for exactly three: the beginning, the end, and the key frame. The
    // pictures themselves never come near this stream — they go on a
    // unidirectional stream of their own, opened by whoever is sharing, headed
    // by a `screen::ScreenHeader`. That split is the same one ADR 0027 made for
    // attachments, and here it is measured: `spikes/tela-no-transporte` shows
    // what sharing a queue with the voice costs.
    /// Starts sharing a screen in the voice room this connection's connection is in.
    ///
    /// Carries nothing, like [`Self::LeaveVoiceRoom`]: the voice room is the one the
    /// server already has this connection in, and a voice room taken from the asker
    /// is a voice room the asker can aim somewhere else. What comes back is
    /// [`ServerMessage::ScreenShareStarted`], and the [`ScreenId`] in it is
    /// what the sender then writes at the head of the stream it opens — a
    /// sender cannot open the stream first, because until the server answers
    /// there is nothing to call the transmission.
    ///
    /// It carries no resolution and no codec either. Those describe what is
    /// coming out of the encoder, which is not decided when the button is
    /// pressed and moves afterwards (§5: "a tela não promete a escolha"), so
    /// they are declared where they are true — at the head of the stream.
    StartScreenShare,
    /// Stops the transmission this connection is sending.
    ///
    /// Also carries nothing, for the same reason. Closing the stream says the
    /// same thing and is what happens when a machine goes away, but a verb that
    /// only exists as an absence gives the room no way to tell "she stopped
    /// sharing" from "her link fell" — and those are different sentences.
    StopScreenShare,
    /// Asks whoever is sharing for a key frame.
    ///
    /// §3.3, and it is a measurement rather than a preference: a key frame is
    /// four times the bytes of an ordinary picture — 65 KiB in 1080p, 446 ms of
    /// the whole budget — so sending them on a timer spends the link on
    /// pictures nobody needed. Between two peers there is nobody joining
    /// mid-transmission, so the only party who knows a key frame is needed is
    /// the receiver that has nothing to predict from.
    RequestKeyFrame {
        /// Which transmission.
        screen: ScreenId,
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
        /// Which person this connection is.
        person: PersonId,
        /// The media source assigned to this connection — gap G1.
        ///
        /// `specs/08-seguranca.md`: the server assigns it and never accepts one
        /// from the client. A client needs to know its own in order to read its
        /// own telemetry back.
        ssrc: Ssrc,
        /// Name of the server.
        server: String,
        /// Voice channels visible to this person.
        voice_rooms: Vec<VoiceRoomInfo>,
        /// Text channels visible to this person.
        channels: Vec<ChannelInfo>,
        /// Roles defined on this server.
        roles: Vec<Role>,
        /// What **this** person may do, as PERMISSIONS resolved it.
        ///
        /// `roles` above is the server's catalogue of roles; nothing on the wire
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
    /// A person entered a voice room.
    PersonJoined {
        /// Which voice room.
        voice_room: VoiceRoomId,
        /// Who.
        profile: PersonProfile,
        /// Their media source — gap G1. This is the mapping
        /// `specs/02-protocolo.md` says the client resolves from the control
        /// channel, and which nothing previously carried.
        ssrc: Ssrc,
    },
    /// A person left a voice room.
    PersonLeft {
        /// Which voice room.
        voice_room: VoiceRoomId,
        /// Who.
        person: PersonId,
    },
    /// A person's state changed.
    PersonState(PersonState),
    /// A message was posted.
    MessageReceived {
        /// Which Channel.
        channel: ChannelId,
        /// Server-assigned identifier.
        id: MessageId,
        /// Who wrote it.
        author: PersonId,
        /// When the server accepted it, in **seconds** since the Unix epoch.
        ///
        /// The unit is in the name because it was wrong once: PERSISTENCE stores
        /// seconds, this field was declared in milliseconds, and every real
        /// message would have been drawn as 1970 while the tests — which used
        /// synthetic milliseconds — passed.
        ///
        /// The server's clock, not the client's, and not the arrival time.
        /// Without it a page of history has no time on it at all: the receiving
        /// client only knows when the *page* arrived, which is now.
        /// `specs/06-clientes-gui.md` requires a session to be resumable in
        /// another client "sem perda de histórico", and history whose channels all
        /// claim to have been written the moment the app opened has lost
        /// something.
        at_seconds: i64,
        /// What the author is called.
        ///
        /// Carried with the message rather than looked up, because a client
        /// reading history has never seen most of these people arrive and has
        /// no other way to learn their names. Without it a resumed session
        /// attributes everything written before you got there to "pessoa 1".
        author_nickname: String,
        /// Body.
        body: String,
        /// What it replies to.
        replies_to: Option<MessageId>,
        /// Echo of the sender's identifier, so a client can match its own
        /// pending send instead of showing it twice.
        client_message_id: Option<ClientMessageId>,
        /// The file hanging off it, if any. ADR 0027.
        ///
        /// Carried with the message rather than fetched per message, for the
        /// reason `author_nickname` is: a client reading history would
        /// otherwise need one round trip per channel to find out whether there is
        /// a file at all.
        ///
        /// **The text survives the file.** When the bytes have been evicted
        /// this is still `Some`, with the name, the size, and
        /// [`AttachmentState::Expired`] — which is the entire reason the row
        /// outlives the blob. A `None` here would draw as a message with
        /// nothing in it and nobody would learn a file had been there.
        attachment: Option<AttachmentInfo>,
    },
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
    // Sent to **everybody connected**, the person who asked included. Without
    // that, a voice room made at nine o'clock is a voice room nobody sees until they
    // reconnect — and "reconnect to see the room I just told you about" is the
    // kind of instruction that makes a product feel broken rather than new.
    /// A voice room was created.
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

    // ---- moderation ----
    //
    // Three of the four verbs need no announcement of their own: a kick and a
    // ban both end with [`Self::Disconnecting`], which already enumerates
    // `Kicked` and `Banned`; a removal is [`Self::MessageRemoved`], which every
    // shell already folds in. Only being moved had nothing that could say it.
    /// This person's connection is now in a different voice room, by somebody else's hand.
    ///
    /// Sent only to the person who was moved. Everybody else learns it the
    /// ordinary way, as a [`Self::PersonLeft`] from the old voice room and a
    /// [`Self::PersonJoined`] in the new one — there is nothing special about a
    /// move from outside, and inventing a second way to say "somebody is in
    /// that room now" would mean every shell learning both.
    ///
    /// What makes this its own message is that the moved client has to change
    /// **its own** idea of where it is, and that is a fact no `PersonJoined` has
    /// ever carried: a client sets its current voice room on the way *out*, when it
    /// asks. Without this it would keep sending voice into the room it thought
    /// it was in and drawing that room's roster around itself.
    MovedToVoiceRoom {
        /// Where the connection is now.
        voice_room: VoiceRoomId,
    },

    // ---- unmaking a room ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // Sent to **everybody connected**, the person who asked included, exactly
    // like the four announcements above: a room that goes on being drawn until
    // the next handshake is a room people keep trying to walk into.
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
    /// What destroying o canal would cost, counted now.
    ///
    /// The answer to [`ClientMessage::WeighChannel`], and the numbers a
    /// confirmation is built out of. Counted at the moment of asking rather
    /// than carried on [`ChannelInfo`], because a count on the room list would be
    /// stale by every message sent since it was drawn — and stale is the one
    /// thing a number in this particular sentence may not be.
    ChannelWeighed {
        /// Which Channel.
        channel: ChannelId,
        /// How many messages are in it that anybody can read.
        ///
        /// Messages already taken off the Channel by [`ClientMessage::RemoveMessage`]
        /// are not counted. They are gone from every screen already, so
        /// counting them would inflate what the reader is told they are about
        /// to lose by a number only the database can see.
        messages: u32,
        /// How many distinct people wrote them.
        authors: u32,
        /// When the oldest one was written, in seconds since the Unix epoch.
        ///
        /// `None` when the Channel is empty, which is the one case where the
        /// sentence has no date to give and must say something else instead.
        oldest_at_seconds: Option<i64>,
    },

    // ---- attachments ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // Neither of these carries bytes. A file goes on its own unidirectional
    // stream in both directions; what crosses control is the **reason**, which
    // is where `specs/02-protocolo.md` says every reason already lives.
    /// A transfer was not taken, and why.
    ///
    /// Sent only to the person who was sending, and nothing is published: no
    /// half message, and no message pointing at a file that does not exist.
    /// Keyed by `client_message_id` rather than by anything the server assigned,
    /// because at the moment of refusal the server has assigned nothing — the
    /// sender's own key is the only name the two ends share.
    AttachmentRefused {
        /// Which of the sender's messages this was.
        client_message_id: ClientMessageId,
        /// Why. Enumerated, so the shell writes a different sentence for a file
        /// that will never fit and one that would fit in a minute.
        reason: AttachmentRefusal,
    },
    /// A file that was asked for is not coming.
    ///
    /// The expected case is [`AttachmentRefusal::Expired`]: the bytes were
    /// evicted to keep the server under its ceiling, the row survived, and this
    /// is what turns that row into a sentence on somebody's screen.
    AttachmentUnavailable {
        /// Which attachment.
        attachment: AttachmentId,
        /// Why.
        reason: AttachmentRefusal,
    },

    // ---- what the server calls itself ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // Sent to **everybody connected**, the person who asked included, exactly
    // like the four room announcements: a server that goes on being drawn under
    // its old name until the next handshake is the failure ADR 0032 names —
    // the screen of whoever renamed it showing one thing and everybody else's
    // showing another.
    /// The server has a new name.
    ServerRenamed {
        /// What it is called now.
        name: String,
    },
    /// The server has a new icon, or none.
    ///
    /// Also sent **once per session, straight after
    /// [`ServerMessage::Session`], when the server has one**. Silence there
    /// means there is none: `Session` describes the server from scratch — the
    /// name and both channel lists come out of it — so a client that reconnects
    /// into a server whose picture was taken down while it was away stops
    /// drawing the old one by having been introduced to the server again, not by
    /// being handed a `None`. A server with no icon, which is every server that
    /// exists today, therefore exchanges exactly the frames it did before this
    /// message was added.
    ///
    /// `None` **in a session** is the other thing: an operator taking the
    /// picture down while people are connected, which has to reach them.
    ///
    /// Its own frame rather than a field on `Session`, for the third reason ADR
    /// 0032 gives against the icon: `Session` already carries the voice_rooms, the
    /// Channels, the roles and the permissions inside [`MAX_FRAME_LEN`], and a
    /// picture sharing that budget would make a big server fail to admit anybody
    /// because of a decoration.
    ///
    /// What this costs, said plainly: the bytes cross once per session instead
    /// of once per machine. Addressing the picture by the hash of its content
    /// and letting a client say it already has that one is the cheaper design
    /// and needs a stream of its own to fetch it on; at
    /// [`MAX_SERVER_ICON_LEN`] the saving is 8 KiB per reconnection, which does
    /// not yet pay for a second transfer path.
    ServerIconChanged {
        /// The picture, or `None` when the server has none.
        icon: Option<Vec<u8>>,
    },

    // ---- screen sharing ----
    //
    // Appended last, for the reason [`AlertReason::RateLimited`] gives.
    //
    // Sent to everybody in the voice room, the sharer included — the sharer needs the
    // [`ScreenId`] before it can open a stream, and everybody else needs to
    // know a stream is about to arrive rather than discovering it by being
    // handed one.
    /// Somebody started sharing a screen.
    ///
    /// Also sent to a person who **enters** a voice room where a transmission is
    /// already running, straight after their [`Self::PersonJoined`]. That is a
    /// rule for the server rather than a message of its own, and it is the
    /// reason [`VoiceRoomInfo`] gained no field: a client learns about a
    /// transmission the same way whether it was there when it began or not,
    /// and there is only one frame to understand instead of two.
    ScreenShareStarted {
        /// Which voice room it is happening in.
        voice_room: VoiceRoomId,
        /// Who is sharing.
        person: PersonId,
        /// What to call the transmission from now on.
        ///
        /// Assigned here, by the server, and never taken from the sender — the
        /// rule `specs/08-seguranca.md` applies to [`Ssrc`], and a **different**
        /// identifier from it for the reason §3.6 gives: a screen is not a
        /// talker, and the table of `ssrc` → person must not have to grow a
        /// second kind of row.
        screen: ScreenId,
    },
    /// A transmission ended.
    ///
    /// Carries no reason. The two ways it ends — somebody pressed stop, or the
    /// sender went away — are already told apart by everything else that
    /// happens: a person who left produces [`Self::PersonLeft`], and one who is
    /// still in the room stopped on purpose.
    ScreenShareStopped {
        /// Which voice room it was happening in.
        voice_room: VoiceRoomId,
        /// Which transmission ended.
        screen: ScreenId,
    },
    /// Somebody watching has nothing to predict from and needs a key frame.
    ///
    /// Sent only to the person who is sharing, and it carries who asked because
    /// the sender may hold one stream per watcher: without a name it would have
    /// to spend a key frame on everybody to answer one person, and §3.3 counts
    /// what a key frame costs. It is also the only way a sender can tell one
    /// watcher asking twice a second from the room genuinely losing pictures.
    KeyFrameRequested {
        /// Which transmission.
        screen: ScreenId,
        /// Who asked.
        person: PersonId,
    },
    /// How many people are receiving a transmission.
    ///
    /// The `N` of §5.1, and it travels because the ceiling cannot be computed
    /// without it: the server forwards the pictures, so what it has to lift is
    /// `N × ceiling`, and the correction §5.1 makes mandatory divides the
    /// host's measured path by this number. Without it a sharer applies a
    /// `min(...)` with a leg it invented, which is the "measure one leg and
    /// burst the other" that section names as the most expensive defect in it.
    ///
    /// On control rather than in the stream, because it is not about a picture:
    /// it changes when somebody enters or leaves the voice room, not when a frame is
    /// encoded, and a number carried by the pictures would stop arriving
    /// exactly when the transmission stalls.
    ///
    /// Sent to the whole voice room whenever the count moves, and once straight after
    /// [`Self::ScreenShareStarted`] — a transmission that never learns its own
    /// audience would sit on the ceiling for a single watcher while four are
    /// listening.
    ///
    /// It is also the sentence the interface owes the person: §5.1 puts
    /// "720p · 6 pessoas assistindo" on the screen, because "more than four
    /// people" is something one can plan around and a number of kbps is not.
    /// The trigger is still the ceiling, and this is the reason shown beside
    /// it.
    ScreenViewers {
        /// Which transmission.
        tela: ScreenId,
        /// How many are receiving it, the sharer excluded.
        quantos: u32,
    },
    /// What the server measured its own upward path to be, in bits per second.
    ///
    /// The first channel of the ceiling in §5.1 — `caminho de quem HOSPEDA × 60% ÷
    /// N espectadores` — and it is the leg the sharer cannot see: the bytes
    /// leave the host's machine, not the sharer's, and until now the number
    /// standing in for it was the 2000 kbps pipe of `spikes/tela-no-transporte`.
    /// A constant borrowed from a laboratory is exactly the "second measurer
    /// disagreeing with the first" that §3.2 rule 2 refuses.
    ///
    /// Sent on entering the session, and again when the measurement changes
    /// band. Band rather than value, for the reason [`Signal`] already
    /// exists: a number that moves every second would have the encoder chasing
    /// it, and a transmission that renegotiates its ceiling fifty times a
    /// minute is worse than one that is slightly wrong.
    ///
    /// **Zero means "not measured", and never zero bits per second.** Whoever
    /// receives it treats zero as absence — the same contract as the `——` the
    /// rest of the product shows where there is no measurement. An `Option`
    /// would say it better in Rust and would cost a byte on every send of a
    /// field that is present nearly always; the sentinel is written down here
    /// instead, and it is the one thing about this message a reader must not
    /// guess.
    HostUplink {
        /// Bits per second, or zero for "not measured".
        bps: u32,
    },
    /// Somebody is connected to this server, whether or not they are in a voice room.
    ///
    /// # Why this exists at all
    ///
    /// [`Self::PersonJoined`] carries a voice room, because it announces sitting down
    /// in one. There was nothing that announced being *here* — so a person who
    /// connected and stayed out of every room was invisible to everybody else,
    /// and a client's own comment said so: "there is no message on the wire
    /// that says who entered the server and stayed out of the rooms". The
    /// people list showed whoever was seated and called itself the roster.
    ///
    /// Sent once per connected person when a session opens — the whole picture,
    /// like the occupancy sweep beside it — and again to everybody whenever
    /// somebody new arrives.
    ///
    /// Idempotent by construction: a person who is announced twice is one entry,
    /// because whoever receives it keys on [`PersonProfile::id`].
    ///
    /// **At the end of the enum, and this is not stylistic.** Postcard writes a
    /// variant's index and nothing else; a variant inserted in the middle
    /// renumbers every one after it, and two builds of this product would then
    /// disagree about what every later message means.
    PersonPresent {
        /// Who.
        profile: PersonProfile,
        /// Their media source, for the ssrc-to-person table every client keeps.
        ssrc: Ssrc,
    },
    /// Somebody's connection to this server ended.
    ///
    /// The twin of [`Self::PersonPresent`], and the half that hurts to leave
    /// out: without it every client accumulates the names of everyone who has
    /// ever connected and draws them as present. [`Self::PersonLeft`] does not
    /// cover it — that one says a voice room was vacated, and somebody who never sat
    /// down never produces one.
    PersonGone {
        /// Who.
        person: PersonId,
    },

    // ---- o bitrate adaptativo do ADR 0036 ----
    //
    // No fim do enum, e pela razão que [`Self::PersonPresent`] já escreve: o
    // postcard grava o índice da variante e nada mais.
    /// Quanto da voz **desta** conexão não está chegando ao servidor.
    ///
    /// Escrito para uma sessão só, e nunca difundido: a perda de subida de
    /// alguém não é assunto de mais ninguém, e espalhá-la contaria a toda a sala
    /// a qualidade da rede de cada um. O filtro está em `session::translate`,
    /// porque a sala que mede não conhece sessão nenhuma.
    ///
    /// # Por que não o `loss_fraction` de [`Telemetry`]
    ///
    /// Porque aquele não serve para isto, por duas razões que não se corrigem
    /// uma à outra. Ele vem de `stats.path` do quinn, então mede a direção
    /// **servidor→cliente** — encolher o microfone de alguém porque o download
    /// dele está ruim é o oposto do que `specs/03-audio.md` pede. E é
    /// **cumulativo desde o início da conexão**: uma razão monótona que só decai
    /// assintoticamente, o que torna o «sobe de volta gradualmente» da spec
    /// aritmeticamente impossível.
    ///
    /// Este é medido por quem recebe, contando lacunas de `seq` numa janela que
    /// desliza. Lacuna de `seq` é perda e nunca silêncio, porque o DTX não
    /// incrementa a sequência — ver `seele_server::perda_de_subida`.
    ///
    /// **A variante que fez a versão do protocolo subir para 2.** Um cliente v1
    /// não a conhece e não a recebe.
    UplinkLoss {
        /// A fração perdida na janela mais recente, de zero a um.
        fraction: f32,
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
    check_bounds(field, len, limit)
}

/// Refuses a field longer than its documented limit.
///
/// Public because [`crate::attachment`] validates the header of a transfer with
/// the same rules, on a different stream. One implementation rather than two
/// that drift.
///
/// # Errors
///
/// Returns [`ControlError::FieldTooLong`] when `len` exceeds `limit`.
pub fn check_bounds(field: &'static str, len: usize, limit: usize) -> Result<(), ControlError> {
    if len > limit {
        return Err(ControlError::FieldTooLong { field, len, limit });
    }
    Ok(())
}

/// Bounds a voice room or Channel name at both ends.
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

/// Bounds the server's own name at both ends.
///
/// The upper bound is [`MAX_CLIENT_NAME_LEN`], because that is what
/// [`ServerMessage::Session`] has always carried this string under and a rename
/// that accepted more would produce a server whose own handshake refuses to
/// describe it. The lower one is [`check_name`]'s, word for word: a header with
/// nothing in it is a thing nobody can refer to out loud.
fn check_server_name(name: &str) -> Result<(), ControlError> {
    if name.trim().is_empty() {
        return Err(ControlError::FieldOutOfRange { field: "name" });
    }
    check("name", name.len(), MAX_CLIENT_NAME_LEN)
}

/// Refuses anything that is not a small PNG.
///
/// Three questions, and all three are asked here rather than in the server,
/// because both ends have to agree about what an icon is: the server refuses
/// what it is sent, and refuses again on the way out, so a picture that arrived
/// from an older build or straight from somebody's `sqlite3` prompt cannot
/// travel further than one a client could have asked for. It is the rule
/// [`ServerMessage::Session`] already applies to a voice room's name.
///
/// **Public**, and that is what keeps it the only copy. A shell has to be able
/// to tell somebody their picture will not do *before* the frame is built:
/// `encode` refusing at the last moment is indistinguishable, from where the
/// client sits, from the link having fallen — the send fails, and everything
/// above it treats a failed send as a dead connection. So the client asks this
/// first, with the same function, rather than keeping a second list of rules
/// that would be the one to fall behind.
///
/// # Errors
///
/// [`ControlError::FieldTooLong`] over [`MAX_SERVER_ICON_LEN`];
/// [`ControlError::FieldOutOfRange`] for anything that is not a PNG within
/// [`MAX_SERVER_ICON_SIDE`].
pub fn check_server_icon(icon: Option<&[u8]>) -> Result<(), ControlError> {
    let Some(bytes) = icon else {
        return Ok(());
    };
    check("icon", bytes.len(), MAX_SERVER_ICON_LEN)?;
    let Some((width, height)) = png_header(bytes) else {
        return Err(ControlError::FieldOutOfRange { field: "icon" });
    };
    // Zero is refused with the same breath as too large: a PNG declaring a side
    // of nothing is not a picture, and it is far more often a truncated file
    // than a deliberate one.
    if width == 0 || height == 0 || width > MAX_SERVER_ICON_SIDE || height > MAX_SERVER_ICON_SIDE {
        return Err(ControlError::FieldOutOfRange { field: "icon" });
    }
    Ok(())
}

/// The same question, asked of the field as the messages carry it.
fn check_icon(icon: Option<&Vec<u8>>) -> Result<(), ControlError> {
    check_server_icon(icon.map(Vec::as_slice))
}

/// The size a PNG declares about itself, if it is a PNG at all.
///
/// Reads the eight-byte signature and then the `IHDR` chunk, which the format
/// requires to come first. `None` for anything else, which is the whole of
/// "only PNG" — no list of magic numbers to keep in step with a list of names,
/// because there is exactly one accepted format.
///
/// Every read is [`slice::get`]: this parses bytes from the network, and
/// `specs/08-seguranca.md` names that as the surface where a panicking
/// indexation turns a malformed frame into a dead server.
fn png_header(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.get(..8)? != SIGNATURE {
        return None;
    }
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

/// Bounds how many people a voice room may hold.
///
/// Zero is refused: a voice room nobody may enter is not a voice room, and a limit of zero
/// is far more often a field left at its default than a deliberate choice.
fn check_voice_room_limit(limit: u16) -> Result<(), ControlError> {
    if limit == 0 || limit > MAX_VOICE_ROOM_LIMIT {
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

impl Validate for PersonState {
    fn validate(&self) -> Result<(), ControlError> {
        // specs/02-protocolo.md puts the Sync Ratio on a 0-100 scale. A u8 can
        // hold 200, and a shell matching the bands of specs/07 would find no
        // band for it.
        if self.signal > 100 {
            return Err(ControlError::FieldOutOfRange { field: "signal" });
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
            Self::EnterVoiceRoom { password, .. } => check(
                "password",
                password.as_ref().map_or(0, String::len),
                MAX_NICKNAME_LEN,
            ),
            Self::SendMessage { body, .. } => check("body", body.len(), MAX_BODY_LEN),
            Self::CreateVoiceRoom { name, limit, .. } => {
                check_name("name", name)?;
                check_voice_room_limit(*limit)
            }
            Self::CreateChannel { name }
            | Self::RenameVoiceRoom { name, .. }
            | Self::RenameChannel { name, .. } => check_name("name", name),
            // The operator's own words about their own server, bounded like the
            // other place they cross the wire.
            Self::BanPerson { reason, .. } => check(
                "reason",
                reason.as_ref().map_or(0, String::len),
                MAX_ALERT_TEXT_LEN,
            ),
            Self::RenameServer { name } => check_server_name(name),
            Self::SetServerIcon { icon } => check_icon(icon.as_ref()),
            Self::LeaveVoiceRoom
            | Self::JoinChannel { .. }
            | Self::FetchHistory { .. }
            | Self::SetMuted(_)
            | Self::SetTotalIsolation(_)
            | Self::SetPresence(_)
            | Self::Ping { .. }
            | Self::KickPerson { .. }
            | Self::RemoveMessage { .. }
            | Self::MovePerson { .. }
            | Self::DeleteVoiceRoom { .. }
            | Self::DeleteChannel { .. }
            | Self::WeighChannel { .. }
            | Self::FetchAttachment { .. }
            | Self::StartScreenShare
            | Self::StopScreenShare
            | Self::RequestKeyFrame { .. } => Ok(()),
        }
    }
}

impl Validate for ServerMessage {
    fn validate(&self) -> Result<(), ControlError> {
        match self {
            Self::Challenge { nonce } => check("nonce", nonce.len(), MAX_PROOF_LEN),
            Self::Session {
                server,
                voice_rooms,
                channels,
                ..
            } => {
                check("server", server.len(), MAX_CLIENT_NAME_LEN)?;
                // The same bound the creating verb enforces, applied to the
                // tree on the way out. A voice room whose name came from an older
                // build, or straight from somebody's `sqlite3` prompt, must not
                // travel further than one a client could have asked for.
                for voice_room in voice_rooms {
                    check_name("name", &voice_room.name)?;
                }
                for channel in channels {
                    check_name("name", &channel.name)?;
                }
                Ok(())
            }
            Self::VoiceRoomCreated { voice_room } => check_name("name", &voice_room.name),
            Self::ChannelCreated { channel } => check_name("name", &channel.name),
            Self::VoiceRoomRenamed { name, .. } | Self::ChannelRenamed { name, .. } => {
                check_name("name", name)
            }
            Self::PersonJoined { profile, .. } | Self::PersonPresent { profile, .. } => {
                check("nickname", profile.nickname.len(), MAX_NICKNAME_LEN)
            }
            Self::MessageReceived {
                body, attachment, ..
            } => {
                check("body", body.len(), MAX_BODY_LEN)?;
                // The same bounds the transfer header enforces, applied on the
                // way out. A name that came from an older build, or straight
                // from somebody's `sqlite3` prompt, must not travel further
                // than one a sender could have declared.
                match attachment {
                    Some(attachment) => {
                        check("file_name", attachment.file_name.len(), MAX_FILE_NAME_LEN)?;
                        check(
                            "declared_type",
                            attachment.declared_type.len(),
                            MAX_DECLARED_TYPE_LEN,
                        )
                    }
                    None => Ok(()),
                }
            }
            Self::MessageEdited { body, .. } => check("body", body.len(), MAX_BODY_LEN),
            Self::Alert { operator_text, .. } => check(
                "operator_text",
                operator_text.as_ref().map_or(0, String::len),
                MAX_ALERT_TEXT_LEN,
            ),
            Self::Telemetry(telemetry) => telemetry.validate(),
            Self::PersonState(state) => state.validate(),
            // Conferido como a perda do `Telemetry`, e não posto no braço de
            // `Ok(())` junto com os quadros sem campo a validar: este número
            // atravessa o fio e vai direto para uma malha que escolhe o bitrate.
            // Um `NaN` — que `check_range` recusa, porque toda comparação com
            // ele é falsa — atravessaria os dois limiares e prenderia a faixa
            // onde estivesse, calada.
            Self::UplinkLoss { fraction } => check_range("fraction", *fraction, 0.0, 1.0),
            Self::ServerRenamed { name } => check_server_name(name),
            Self::ServerIconChanged { icon } => check_icon(icon.as_ref()),
            Self::PersonLeft { .. }
            | Self::PersonGone { .. }
            | Self::MessageRemoved { .. }
            | Self::Pong { .. }
            | Self::Disconnecting { .. }
            | Self::MovedToVoiceRoom { .. }
            | Self::VoiceRoomDeleted { .. }
            | Self::ChannelDeleted { .. }
            | Self::ChannelWeighed { .. }
            | Self::AttachmentRefused { .. }
            | Self::AttachmentUnavailable { .. }
            | Self::ScreenShareStarted { .. }
            | Self::ScreenShareStopped { .. }
            | Self::KeyFrameRequested { .. }
            | Self::ScreenViewers { .. }
            | Self::HostUplink { .. } => Ok(()),
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
            client: "connection/0.0.0".into(),
            nickname: "ayanami".into(),
            public_key: vec![7; PUBLIC_KEY_LEN],
        }
    }

    fn session() -> ServerMessage {
        ServerMessage::Session {
            id: SessionId(7),
            person: PersonId(42),
            ssrc: Ssrc(0xABCD),
            server: "Terceira Tóquio".into(),
            voice_rooms: vec![VoiceRoomInfo {
                id: VoiceRoomId(1),
                name: "SALA-01 CENTRAL".into(),
                limit: 15,
                password_required: false,
                channel: Some(ChannelId(1)),
            }],
            channels: vec![ChannelInfo {
                id: ChannelId(1),
                name: "geral".into(),
            }],
            roles: vec![Role {
                id: RoleId(1),
                name: "Person".into(),
                permissions: vec![Permission::EnterVoiceRoom, Permission::Speak],
            }],
            permissions: vec![Permission::EnterVoiceRoom, Permission::Speak],
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
            channel: ChannelId(1),
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
            client: "connection".into(),
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
        // specs/02-protocolo.md's "resolve ssrc to person from the control
        // channel" has nothing to resolve from.
        let frame = encode(&session()).unwrap();
        let ServerMessage::Session { ssrc, .. } = decode::<ServerMessage>(&frame).unwrap() else {
            panic!("not a session");
        };
        assert_eq!(ssrc, Ssrc(0xABCD));
    }

    #[test]
    fn a_joining_person_carries_their_ssrc() {
        // The other half of gap G1: the mapping for everybody else.
        let joined = ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(1),
            profile: PersonProfile {
                id: PersonId(2),
                nickname: "shinji".into(),
                roles: vec![RoleId(1)],
            },
            ssrc: Ssrc(99),
        };
        let frame = encode(&joined).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), joined);
    }

    #[test]
    fn person_state_carries_both_mute_controls() {
        // Gap G8. specs/07-tema-evangelion.md defines "Isolamento total" and
        // specs/05-cliente-tui.md binds it to a key, but nothing carried it, so
        // a roster could not show who was not listening.
        let state = ServerMessage::PersonState(PersonState {
            person: PersonId(1),
            muted: true,
            total_isolation: true,
            speaking: false,
            presence: Presence::Available,
            signal: 94,
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
            channel: ChannelId(1),
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
    fn the_session_says_what_this_person_may_do() {
        // `roles` is the server's catalogue; nothing ever said which of them this
        // connection holds. Without this field a shell has no honest way to
        // decide whether to offer a control at all, and the only alternative is
        // to offer everything and let the server refuse — which teaches people
        // that half the buttons do nothing.
        let frame = encode(&session()).unwrap();
        let ServerMessage::Session { permissions, .. } = decode::<ServerMessage>(&frame).unwrap()
        else {
            panic!("not a session");
        };
        assert_eq!(
            permissions,
            vec![Permission::EnterVoiceRoom, Permission::Speak]
        );
    }

    #[test]
    fn the_room_making_verbs_round_trip() {
        for message in [
            ClientMessage::CreateVoiceRoom {
                name: "SALA-02 SALA DOS FUNDOS".into(),
                limit: 8,
                channel: Some(ChannelId(1)),
            },
            ClientMessage::CreateChannel {
                name: "planejamento".into(),
            },
            ClientMessage::RenameVoiceRoom {
                voice_room: VoiceRoomId(2),
                name: "SALA-02 CENTRAL".into(),
            },
            ClientMessage::RenameChannel {
                channel: ChannelId(3),
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
            ServerMessage::VoiceRoomCreated {
                voice_room: VoiceRoomInfo {
                    id: VoiceRoomId(2),
                    name: "SALA-02 SALA DOS FUNDOS".into(),
                    limit: 8,
                    password_required: false,
                    channel: Some(ChannelId(1)),
                },
            },
            ServerMessage::ChannelCreated {
                channel: ChannelInfo {
                    id: ChannelId(3),
                    name: "planejamento".into(),
                },
            },
            ServerMessage::VoiceRoomRenamed {
                voice_room: VoiceRoomId(2),
                name: "SALA-02 CENTRAL".into(),
            },
            ServerMessage::ChannelRenamed {
                channel: ChannelId(3),
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
            let ask = ClientMessage::CreateVoiceRoom {
                name: blank.into(),
                limit: 8,
                channel: None,
            };
            assert!(
                matches!(
                    encode(&ask),
                    Err(ControlError::FieldOutOfRange { field: "name" })
                ),
                "accepted a voice room named {blank:?}"
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
                "accepted a hand-rolled voice room named {blank:?}"
            );
        }
    }

    #[test]
    fn an_oversized_room_name_is_refused() {
        let long = ClientMessage::CreateChannel {
            name: "n".repeat(MAX_CHANNEL_NAME_LEN + 1),
        };
        assert!(matches!(
            encode(&long),
            Err(ControlError::FieldTooLong { field: "name", .. })
        ));
        // Exactly at the limit is a name, not an overflow.
        assert!(encode(&ClientMessage::CreateChannel {
            name: "n".repeat(MAX_CHANNEL_NAME_LEN),
        })
        .is_ok());
    }

    #[test]
    fn a_voice_room_that_holds_nobody_or_everybody_is_refused() {
        // Zero is far more often a field left at its default than a decision,
        // and a room nobody may enter is not a room. The ceiling stops a u16 of
        // 65 535 being written into a server sized for fifty.
        for limit in [0, MAX_VOICE_ROOM_LIMIT + 1, u16::MAX] {
            let ask = ClientMessage::CreateVoiceRoom {
                name: "SALA-02".into(),
                limit,
                channel: None,
            };
            assert!(
                matches!(
                    encode(&ask),
                    Err(ControlError::FieldOutOfRange { field: "limit" })
                ),
                "accepted a voice room for {limit} people"
            );

            let mut frame = vec![PROTOCOL_VERSION];
            frame = postcard::to_extend(&ask, frame).unwrap();
            assert!(
                matches!(
                    decode::<ClientMessage>(&frame),
                    Err(ControlError::FieldOutOfRange { field: "limit" })
                ),
                "accepted a hand-rolled voice room for {limit} people"
            );
        }

        for limit in [1, MAX_VOICE_ROOM_LIMIT] {
            assert!(
                encode(&ClientMessage::CreateVoiceRoom {
                    name: "SALA-02".into(),
                    limit,
                    channel: None,
                })
                .is_ok(),
                "refused a voice room for {limit} people"
            );
        }
    }

    #[test]
    fn a_blank_name_never_leaves_the_server_inside_a_session() {
        // The tree on the way out gets the same bound the verb enforces on the
        // way in. A row written straight into the database by hand must not
        // reach a shell that has no sentence for it.
        let ServerMessage::Session {
            id,
            person,
            ssrc,
            server,
            channels,
            roles,
            permissions,
            ..
        } = session()
        else {
            panic!("not a session");
        };
        let blank = ServerMessage::Session {
            id,
            person,
            ssrc,
            server,
            voice_rooms: vec![VoiceRoomInfo {
                id: VoiceRoomId(9),
                name: "   ".into(),
                limit: 4,
                password_required: false,
                channel: None,
            }],
            channels,
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
            ClientMessage::KickPerson {
                person: PersonId(42),
            },
            ClientMessage::BanPerson {
                person: PersonId(42),
                reason: Some("inundou a Linha".into()),
                expires_at: Some(1_700_000_000),
            },
            ClientMessage::BanPerson {
                person: PersonId(42),
                reason: None,
                expires_at: None,
            },
            ClientMessage::RemoveMessage {
                message: MessageId(9),
            },
            ClientMessage::MovePerson {
                person: PersonId(42),
                voice_room: VoiceRoomId(2),
            },
        ] {
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ClientMessage>(&frame).unwrap(), message);
        }
    }

    #[test]
    fn the_doorkeeper_reasons_round_trip_and_do_not_read_as_an_older_one() {
        // ADR 0030. The round trip is the cheap half; the ordinals below are the
        // half that matters.
        //
        // `postcard` writes an enum variant as its position, so a build one
        // protocol version older reads byte 12 as whatever *it* has at 12. The
        // two new reasons are appended, so an older build has nothing there and
        // refuses the frame — which is the contract. What would break it is
        // somebody inserting a variant above them to keep the list tidy: the
        // reason a peer was told is «you fell behind» would silently become
        // «the host said no», and neither end would report anything.
        //
        // Asserting the numbers rather than only the round trip is what makes
        // that visible, because a round trip is self-referential: encode and
        // decode move together, and a shifted list round-trips perfectly.
        for (reason, ordinal) in [
            (DisconnectReason::AdmissionPending, 12_u8),
            (DisconnectReason::AdmissionDenied, 13_u8),
        ] {
            let message = ServerMessage::Disconnecting { reason };
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ServerMessage>(&frame).unwrap(), message);

            let bare: Vec<u8> = postcard::to_extend(&reason, Vec::new()).unwrap();
            assert_eq!(
                bare.as_slice(),
                &[ordinal],
                "{reason:?} no longer sits where it was appended, so every peer \
                 one version older now reads it as a different reason"
            );
        }

        // And the neighbour that was last before them has not moved either.
        assert_eq!(
            postcard::to_extend(&DisconnectReason::FellBehind, Vec::new()).unwrap(),
            vec![11_u8]
        );
    }

    #[test]
    fn being_moved_round_trips() {
        // The one moderation verb that needed an announcement of its own. A
        // kick and a ban end in `Disconnecting`, a removal is
        // `MessageRemoved`; only "you are somewhere else now" had nothing that
        // could say it, because a client sets its own voice room on the way out.
        let moved = ServerMessage::MovedToVoiceRoom {
            voice_room: VoiceRoomId(3),
        };
        let frame = encode(&moved).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), moved);
    }

    #[test]
    fn an_oversized_ban_reason_is_refused_in_both_directions() {
        // The operator's own words are still a bounded field. Enforced on the
        // way out so we cannot send one, and on the way in so a peer cannot
        // skip the check by hand-rolling a frame — the same pair every other
        // text field gets.
        let long = ClientMessage::BanPerson {
            person: PersonId(1),
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
    fn moderation_asks_for_a_person_and_never_for_a_sentence() {
        // specs/02-protocolo.md: no free-form string reaches the interface. A
        // kick that carried "why" as text would be a second error language
        // growing beside the enumerated one, written by whoever is angriest.
        // The one string here is the ban's operator note, which never leaves
        // the server — the person barred meets `DisconnectReason::Banned` and
        // nothing else.
        let frame = encode(&ClientMessage::KickPerson {
            person: PersonId(42),
        })
        .unwrap();
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
            ClientMessage::DeleteVoiceRoom {
                voice_room: VoiceRoomId(2),
            },
            ClientMessage::DeleteChannel {
                channel: ChannelId(7),
            },
            ClientMessage::WeighChannel {
                channel: ChannelId(7),
            },
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
            ServerMessage::ChannelWeighed {
                channel: ChannelId(7),
                messages: 1_847,
                authors: 6,
                oldest_at_seconds: Some(1_678_600_000),
            },
            // The empty Channel, which is the case with no date to give.
            ServerMessage::ChannelWeighed {
                channel: ChannelId(7),
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
        // Unlike `VoiceRoomCreated`, which carries the whole row: there is no row
        // any more, and everything a client needs to stop drawing the room it
        // already has. A frame carrying the name of a room that no longer
        // exists would be inviting some shell to keep it.
        for gone in [
            ServerMessage::VoiceRoomDeleted {
                voice_room: VoiceRoomId(2),
            },
            ServerMessage::ChannelDeleted {
                channel: ChannelId(7),
            },
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
        let voice_room = ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::VoiceRoomDeleted,
            operator_text: None,
        };
        let channel = ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::ChannelDeleted,
            operator_text: None,
        };
        // And the refusal of the last voice room is a third sentence, not either of
        // these two: "make another room first" is not "the room is gone".
        let ultimo = ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::LastVoiceRoom,
            operator_text: None,
        };
        assert_ne!(voice_room, channel);
        assert_ne!(voice_room, ultimo);
        assert_ne!(channel, ultimo);
        for alert in [voice_room, channel, ultimo] {
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
            channel: u32,
            id: u64,
        ) {
            let send = ClientMessage::SendMessage {
                channel: ChannelId(channel),
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
            subsystems: vec![(Subsystem::Permissions, SubsystemHealth::Nominal)],
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
        let state = ServerMessage::PersonState(PersonState {
            person: PersonId(1),
            muted: false,
            total_isolation: false,
            speaking: true,
            presence: Presence::Available,
            signal: 200,
        });
        assert!(matches!(
            encode(&state),
            Err(ControlError::FieldOutOfRange { field: "signal" })
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
                client: "connection".into(),
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
            client: "connection".into(),
            nickname: "ayanami".into(),
            public_key: vec![9; PUBLIC_KEY_LEN],
        };
        let frame = encode(&hello).unwrap();
        assert_eq!(decode::<ClientMessage>(&frame).unwrap(), hello);
    }
}

/// What an icon has to be to cross this protocol.
///
/// Its own module because these are the bounds that decide whether a stranger
/// may put bytes on a machine somebody runs at home, and a bound with no test
/// is a comment.
#[cfg(test)]
mod icon_tests {
    use super::*;

    /// A PNG of `side` × `side`, with `filler` bytes of body after the header.
    ///
    /// Hand-built rather than encoded by a library: what is under test is the
    /// **header check**, and a real encoder would refuse to produce the two
    /// cases that matter most — the twenty-thousand-pixel side and the file
    /// that stops in the middle of `IHDR`.
    fn png(side: u32, filler: usize) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13_u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&side.to_be_bytes());
        bytes.extend_from_slice(&side.to_be_bytes());
        bytes.extend(std::iter::repeat_n(0_u8, filler));
        bytes
    }

    #[test]
    fn an_icon_round_trips_in_both_directions() {
        let set = ClientMessage::SetServerIcon {
            icon: Some(png(128, 64)),
        };
        let frame = encode(&set).unwrap();
        assert_eq!(decode::<ClientMessage>(&frame).unwrap(), set);

        let announced = ServerMessage::ServerIconChanged {
            icon: Some(png(128, 64)),
        };
        let frame = encode(&announced).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), announced);
    }

    #[test]
    fn having_no_icon_is_a_value_and_not_a_refusal() {
        // `None` is how an operator takes the picture down, and it is also what
        // every session is told when the server has none. If this were refused,
        // a client reconnecting into a server whose icon was removed while it was
        // away would go on drawing the old one until it restarted.
        let none = ServerMessage::ServerIconChanged { icon: None };
        let frame = encode(&none).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), none);
    }

    #[test]
    fn an_icon_over_the_ceiling_is_refused_in_both_directions() {
        // On the way out so we cannot send one, and on the way in so a peer
        // cannot skip the check by hand-rolling a frame — the rule
        // `an_oversized_body_is_refused_in_both_directions` already states.
        let fat = ClientMessage::SetServerIcon {
            icon: Some(png(128, MAX_SERVER_ICON_LEN)),
        };
        assert!(matches!(
            encode(&fat),
            Err(ControlError::FieldTooLong { field: "icon", .. })
        ));

        let mut frame = vec![PROTOCOL_VERSION];
        frame = postcard::to_extend(&fat, frame).unwrap();
        assert!(matches!(
            decode::<ClientMessage>(&frame),
            Err(ControlError::FieldTooLong { field: "icon", .. })
        ));
    }

    #[test]
    fn an_icon_at_the_ceiling_still_leaves_the_frame_with_room_to_spare() {
        // The reason the ceiling is 8 KiB and not the 16 KiB of ADR 0032: at
        // the cap the two refusals would be the same event, and an icon would
        // be able to be the thing that makes a control frame unsendable. A
        // kibibyte is far more than the handful of bytes postcard adds — it is
        // there so that raising the ceiling towards the cap trips this test
        // rather than a stranger's connection.
        let biggest = ClientMessage::SetServerIcon {
            icon: Some(png(256, MAX_SERVER_ICON_LEN - 24)),
        };
        let frame = encode(&biggest).unwrap();
        assert!(
            frame.len() + 1024 <= MAX_FRAME_LEN,
            "an icon at the ceiling fills {} of the {MAX_FRAME_LEN}-byte frame; \
             the headroom that keeps «icon too big» and «frame too big» apart is gone",
            frame.len()
        );
    }

    #[test]
    fn a_picture_that_is_not_a_png_is_refused() {
        // The two that would hurt, by their own first bytes: a GIF, which
        // `specs/07-tema-evangelion.md` would have animating in a badge, and an
        // SVG, which is a document with script in it rather than a picture.
        // Plus an empty one, which is neither and arrives as a truncated file.
        for pretender in [
            b"GIF89a".to_vec(),
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec(),
            b"\xff\xd8\xff\xe0JFIF".to_vec(),
            Vec::new(),
        ] {
            let set = ClientMessage::SetServerIcon {
                icon: Some(pretender.clone()),
            };
            assert!(
                matches!(
                    encode(&set),
                    Err(ControlError::FieldOutOfRange { field: "icon" })
                ),
                "accepted {} bytes that are not a PNG",
                pretender.len()
            );
        }
    }

    #[test]
    fn a_small_file_declaring_an_enormous_picture_is_refused() {
        // The check the byte ceiling cannot make. PNG compresses uniform colour
        // to almost nothing, so a few hundred bytes can declare 20 000 × 20 000
        // — 1.6 GB the moment anything decodes it. The bytes are small; the
        // picture is not, and it is the picture that kills the machine drawing
        // it.
        let bomb = ClientMessage::SetServerIcon {
            icon: Some(png(20_000, 128)),
        };
        assert!(matches!(
            encode(&bomb),
            Err(ControlError::FieldOutOfRange { field: "icon" })
        ));
    }

    #[test]
    fn a_side_of_zero_is_refused() {
        let flat = ClientMessage::SetServerIcon {
            icon: Some(png(0, 128)),
        };
        assert!(matches!(
            encode(&flat),
            Err(ControlError::FieldOutOfRange { field: "icon" })
        ));
    }

    #[test]
    fn a_png_that_stops_inside_its_own_header_is_refused_and_does_not_panic() {
        // `specs/08-seguranca.md` names this surface for fuzzing: a malformed
        // frame must be a refusal, never a panicking indexation, or a truncated
        // upload is a way to stop a server.
        let whole = png(64, 0);
        for cut in 0..whole.len() {
            let set = ClientMessage::SetServerIcon {
                icon: Some(whole.get(..cut).unwrap_or_default().to_vec()),
            };
            assert!(
                matches!(
                    encode(&set),
                    Err(ControlError::FieldOutOfRange { field: "icon" })
                ),
                "a PNG cut at {cut} bytes was accepted"
            );
        }
    }

    #[test]
    fn a_server_name_is_bounded_at_both_ends() {
        // The upper bound is what `Session` already carries the name under; the
        // lower one is `check_name`'s, and it is the half that is easy to leave
        // out — a header with nothing in it is a thing nobody can refer to out
        // loud.
        for blank in ["", "   ", "\t\n"] {
            assert!(
                matches!(
                    encode(&ClientMessage::RenameServer { name: blank.into() }),
                    Err(ControlError::FieldOutOfRange { field: "name" })
                ),
                "a blank name was accepted: {blank:?}"
            );
        }

        assert!(matches!(
            encode(&ClientMessage::RenameServer {
                name: "n".repeat(MAX_CLIENT_NAME_LEN + 1),
            }),
            Err(ControlError::FieldTooLong { field: "name", .. })
        ));

        let ok = ClientMessage::RenameServer {
            name: "Terceira Tóquio".into(),
        };
        let frame = encode(&ok).unwrap();
        assert_eq!(decode::<ClientMessage>(&frame).unwrap(), ok);
    }

    #[test]
    fn a_blank_name_never_leaves_the_server_either() {
        // The same bound applied on the way out. A name that came from an older
        // build, or straight from somebody's `sqlite3` prompt, must not travel
        // further than one a client could have asked for.
        assert!(matches!(
            encode(&ServerMessage::ServerRenamed { name: "  ".into() }),
            Err(ControlError::FieldOutOfRange { field: "name" })
        ));
    }
}

/// What a screen transmission says on the control stream.
///
/// Its own module for the reason `icon_tests` has one: these three verbs are
/// the whole of §3.6 of
/// `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`, and
/// what they must never do — move, or borrow the audio identifier — is easier
/// to see when it is all in one place.
#[cfg(test)]
mod screen_tests {
    use super::*;

    #[test]
    fn the_screen_verbs_round_trip_and_sit_where_they_were_appended() {
        // §3.6 of the screen-sharing design asks for three things on control:
        // the beginning, the end and the key frame. The round trip is the cheap
        // half; the ordinals are the half that matters.
        //
        // `postcard` writes a variant as its position, so a build one protocol
        // version older reads byte 25 as whatever *it* has at 25 — which is
        // nothing, so it refuses the frame, which is the contract. What would
        // break it is somebody inserting a variant above these to keep the list
        // tidy: `SetServerIcon` would arrive as `StartScreenShare` and a picture
        // would become a room full of people watching nothing.
        //
        // Byte 0 is the protocol version, byte 1 the variant, so `frame[1]` is
        // where a shifted list shows up.
        for (message, ordinal) in [
            (ClientMessage::StartScreenShare, 25_u8),
            (ClientMessage::StopScreenShare, 26),
            (
                ClientMessage::RequestKeyFrame {
                    screen: ScreenId(0x00C0_FFEE),
                },
                27,
            ),
        ] {
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ClientMessage>(&frame).unwrap(), message);
            assert_eq!(frame.first(), Some(&PROTOCOL_VERSION));
            assert_eq!(
                frame.get(1),
                Some(&ordinal),
                "{message:?} no longer sits where it was appended, so every peer \
                 one version older now reads it as a different verb"
            );
        }

        for (message, ordinal) in [
            (
                ServerMessage::ScreenShareStarted {
                    voice_room: VoiceRoomId(2),
                    person: PersonId(42),
                    screen: ScreenId(0x00C0_FFEE),
                },
                24_u8,
            ),
            (
                ServerMessage::ScreenShareStopped {
                    voice_room: VoiceRoomId(2),
                    screen: ScreenId(0x00C0_FFEE),
                },
                25,
            ),
            (
                ServerMessage::KeyFrameRequested {
                    screen: ScreenId(0x00C0_FFEE),
                    person: PersonId(43),
                },
                26,
            ),
        ] {
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ServerMessage>(&frame).unwrap(), message);
            assert_eq!(
                frame.get(1),
                Some(&ordinal),
                "{message:?} no longer sits where it was appended"
            );
        }

        // And the neighbours that were last before them have not moved either.
        assert_eq!(
            postcard::to_extend(&ClientMessage::SetServerIcon { icon: None }, Vec::new()).unwrap(),
            vec![24_u8, 0]
        );
        assert_eq!(
            postcard::to_extend(&ServerMessage::ServerIconChanged { icon: None }, Vec::new())
                .unwrap(),
            vec![23_u8, 0]
        );
    }

    #[test]
    fn a_transmission_is_named_by_a_screen_id_and_never_by_an_ssrc() {
        // §3.6 puts this in bold, and it is the one decision in that section
        // that is about somebody else's code: `ssrc` is the audio source
        // assigned on voice room entry, and every client keeps a table of
        // `ssrc` → person built out of it. Reusing it for a screen would mean
        // one person holding two rows in that table, and every shell that reads
        // it learning the difference.
        //
        // The compiler is what enforces this — `ScreenId` and `Ssrc` do not
        // interchange — so the test records the shape instead: the
        // announcement carries a `ScreenId`, and `Session` still carries the
        // `ssrc` it always did, untouched by any of this.
        let started = ServerMessage::ScreenShareStarted {
            voice_room: VoiceRoomId(2),
            person: PersonId(42),
            screen: ScreenId(7),
        };
        let frame = encode(&started).unwrap();
        let ServerMessage::ScreenShareStarted { screen, .. } =
            decode::<ServerMessage>(&frame).unwrap()
        else {
            panic!("not a transmission");
        };
        assert_eq!(screen, ScreenId(7));
    }

    #[test]
    fn a_room_that_is_already_being_shared_says_so_in_its_own_words() {
        // §6 item 3 allows one transmission per voice room in v1, so two people
        // pressing the button in the same second is an ordinary race and the
        // loser has to be told something true. `PermissionDenied` would have
        // every shell write "you may not do that" in front of somebody who may,
        // and who only has to wait.
        //
        // The ordinal is asserted for the reason
        // `the_doorkeeper_reasons_round_trip_and_do_not_read_as_an_older_one`
        // gives: a round trip is self-referential and a shifted list passes it
        // perfectly.
        let alert = ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::ScreenShareTaken,
            operator_text: None,
        };
        let frame = encode(&alert).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), alert);

        assert_eq!(
            postcard::to_extend(&AlertReason::ScreenShareTaken, Vec::new()).unwrap(),
            vec![12_u8],
            "the reason moved, so every peer one version older now reads it as another one"
        );
        assert_eq!(
            postcard::to_extend(&AlertReason::LastVoiceRoom, Vec::new()).unwrap(),
            vec![11_u8]
        );
    }
    #[test]
    fn the_two_frames_the_ceiling_needs_sit_after_the_verbs_and_shift_nothing() {
        // The wave-3 additions of `fio-onda3.md`: the audience count and the
        // host's measured path, both appended, both on control.
        //
        // The round trip is the cheap half. The ordinals are the half that
        // matters, for the reason the test above gives: `postcard` writes a
        // variant as its position, so a variant inserted among the screen verbs
        // to keep the list tidy would have a peer one version older read
        // `ScreenViewers` as `KeyFrameRequested` — and answer an audience count
        // with a key frame, which is 65 KiB spent on nothing.
        for (message, ordinal) in [
            (
                ServerMessage::ScreenViewers {
                    tela: ScreenId(0x00C0_FFEE),
                    quantos: 6,
                },
                27_u8,
            ),
            (ServerMessage::HostUplink { bps: 12_000_000 }, 28),
        ] {
            let frame = encode(&message).unwrap();
            assert_eq!(decode::<ServerMessage>(&frame).unwrap(), message);
            assert_eq!(frame.first(), Some(&PROTOCOL_VERSION));
            assert_eq!(
                frame.get(1),
                Some(&ordinal),
                "{message:?} no longer sits where it was appended, so every peer \
                 one version older now reads it as a different frame"
            );
        }

        // And the verb that was last before them has not moved.
        assert_eq!(
            postcard::to_extend(
                &ServerMessage::KeyFrameRequested {
                    screen: ScreenId(1),
                    person: PersonId(1),
                },
                Vec::new()
            )
            .unwrap()
            .first(),
            Some(&26_u8)
        );
    }

    #[test]
    fn an_unmeasured_uplink_travels_as_zero_rather_than_being_refused() {
        // The contract written on `HostUplink`: zero is **absence**, the same
        // `——` the rest of the product shows where nothing was measured, and
        // not a link of no bits. A server that has not measured yet still has to
        // say so on entry — refusing zero here would leave it silent, and a
        // sharer with no first channel for the ceiling of §5.1 falls back to
        // guessing, which is what this frame exists to end.
        let quieto = ServerMessage::HostUplink { bps: 0 };
        let frame = encode(&quieto).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), quieto);

        // And a room that emptied says zero too, for the same reason: the
        // count is news whether it went up or down.
        let ninguem = ServerMessage::ScreenViewers {
            tela: ScreenId(7),
            quantos: 0,
        };
        let frame = encode(&ninguem).unwrap();
        assert_eq!(decode::<ServerMessage>(&frame).unwrap(), ninguem);
    }
}
