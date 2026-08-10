//! SEELE headless client.
//!
//! All session, protocol, audio and state logic lives here. This crate is
//! headless and testable without any interface (`specs/01-arquitetura.md`).
//!
//! # One core, three shells
//!
//! The TUI, the desktop app and the mobile app are presentation layers over the
//! same state machine. None of them contains business logic. This crate exposes
//! a state machine that consumes commands and emits events:
//!
//! ```text
//! Command  →  [ seele-core ]  →  Event
//! ```
//!
//! Each shell only translates events into pixels and input into commands.
//!
//! **This boundary is the most important contract in the project.** If a feature
//! has to be implemented twice in two different interfaces, it is in the wrong
//! place. If the boundary leaks, the project becomes three applications.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

pub mod battery;
pub mod client;
pub mod conhecidos;
pub mod enlace;
pub mod frame;
pub mod identity;
pub mod state;
pub mod tofu;
pub mod voice;

pub use battery::{Battery, Link};
pub use client::{Client, ConnectError, MediaChannel, Pattern, SessionInfo};
pub use ed25519_dalek::SigningKey;
pub use identity::FilePinStore;
pub use state::{Changed, Ended, Message, Notice, Pilot, Room};
pub use tofu::{MemoryPinStore, PinDecision, PinStore};
pub use voice::{DeviceRates, Voice, VoiceMode};

/// The surface a shell is allowed to see.
///
/// ADR 0002 keeps `seele-tui` and `seele-ffi` from depending on `seele-proto` or
/// `seele-audio` directly. That is not bureaucracy: a shell that can name an
/// `ssrc` has protocol knowledge in it, and the same knowledge would then have
/// to be written twice more for the desktop and mobile shells.
///
/// So everything a shell legitimately needs is re-exported here, deliberately
/// and one item at a time. Adding to this list is the moment to ask whether the
/// shell needs the value or the decision behind it — usually it is the decision,
/// and that belongs in the core.
pub use seele_audio::telemetry::{AudioTelemetry, LocalTelemetry, SourceTelemetry};
pub use seele_proto::control::{
    AlertReason, AlertSeverity, CageInfo, DisconnectReason, LineInfo, Permission, PilotProfile,
    PilotState, Presence, ServerMessage, Subsystem, SubsystemHealth, Telemetry,
};
pub use seele_proto::ids::{
    CageId, ClientMessageId, LineId, MessageId, PilotId, RoleId, SessionId, Ssrc,
};
pub use seele_proto::sync_ratio::{SyncBand, SyncInputs, SyncRatio};
pub use seele_proto::transport::DEFAULT_PORT;
pub use seele_proto::uri;
pub use seele_proto::PROTOCOL_VERSION;
