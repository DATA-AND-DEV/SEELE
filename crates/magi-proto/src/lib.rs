//! MAGI wire protocol.
//!
//! This crate owns every byte that crosses the network: control frames, text
//! frames and the media datagram header. It is the only place where the wire
//! format is defined, and it depends on no other crate in the workspace
//! (see `specs/01-arquitetura.md`).
//!
//! Two rules shape everything here:
//!
//! - **Every error reason is an enum.** No free-form string ever reaches an
//!   interface; the shell decides how to present each variant
//!   (`specs/02-protocolo.md`). Error types carry the numbers and identifiers a
//!   shell needs to build its own localised message, so that nothing below the
//!   shell has to format text — see ADR 0012. `thiserror` `Display` output
//!   exists for `tracing` and for developers, not for the interface.
//! - **Every network input is size-limited before allocating**
//!   (`specs/08-seguranca.md`). This crate is the untrusted-input surface of the
//!   whole system and is fuzzed accordingly — see `fuzz/`.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

pub mod control;
pub mod ids;
pub mod media;
pub mod transport;
pub mod version;

pub use control::{ClientMessage, ControlError, ServerMessage};
pub use ids::{CageId, LineId, PilotId, SessionId, Ssrc};
pub use media::{MediaError, MediaHeader, HEADER_LEN, MAX_DATAGRAM_LEN, MAX_PAYLOAD_LEN};
pub use version::{oldest_supported_version, Incompatible, COMPATIBILITY_WINDOW, PROTOCOL_VERSION};
