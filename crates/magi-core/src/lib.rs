//! MAGI headless client.
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
//! Command  →  [ magi-core ]  →  Event
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
pub mod frame;
pub mod tofu;

pub use battery::{Battery, Link};
pub use client::{Client, MediaChannel, Pattern, SessionInfo};
pub use tofu::{MemoryPinStore, PinDecision, PinStore};
