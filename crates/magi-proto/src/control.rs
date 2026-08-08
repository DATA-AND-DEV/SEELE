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

use crate::ids::{CageId, ClientMessageId, LineId, MessageId, PilotId, RoleId, SessionId, Ssrc};
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

/// Longest operator-supplied alert text, in bytes.
pub const MAX_ALERT_TEXT_LEN: usize = 512;

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

/// One permission. `specs/04-servidor-magi.md`, enumerated with no expression
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
    /// defaults in `specs/04-servidor-magi.md`, but operators may add more.
    pub name: String,
    /// What the role allows.
    ///
    /// `specs/04-servidor-magi.md`: denied beats granted, and there is no tree
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
    /// A Line bound to this Cage, if any. `specs/04-servidor-magi.md` makes the
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

/// One of the three subsystems in `specs/04-servidor-magi.md`.
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
    /// Planned downtime. `specs/04-servidor-magi.md` gives clients 3 s.
    ScheduledMaintenance,
    /// The server is stopping for another reason.
    ServerShuttingDown,
    /// Keepalive lapsed.
    Timeout,
    /// The client sent something it should not have.
    ProtocolViolation,
    /// The client exceeded its frame budget. `specs/04-servidor-magi.md`.
    RateLimited,
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
}

/// Client to server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Opens the handshake. `specs/02-protocolo.md`.
    Hello {
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
            Self::EjectPlug
            | Self::JoinLine { .. }
            | Self::FetchHistory { .. }
            | Self::SetAtField(_)
            | Self::SetTotalIsolation(_)
            | Self::SetPresence(_)
            | Self::Ping { .. } => Ok(()),
        }
    }
}

impl Validate for ServerMessage {
    fn validate(&self) -> Result<(), ControlError> {
        match self {
            Self::Challenge { nonce } => check("nonce", nonce.len(), MAX_PROOF_LEN),
            Self::Session { dogma, .. } => check("dogma", dogma.len(), MAX_CLIENT_NAME_LEN),
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
            | Self::Disconnecting { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn hello() -> ClientMessage {
        ClientMessage::Hello {
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
            version: PROTOCOL_VERSION,
            client: "plug".into(),
            nickname: "ayanami".into(),
            public_key: vec![9; PUBLIC_KEY_LEN],
        };
        let frame = encode(&hello).unwrap();
        assert_eq!(decode::<ClientMessage>(&frame).unwrap(), hello);
    }
}
