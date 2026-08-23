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
pub mod chegada;
pub mod client;
pub mod conhecidos;
pub mod encontro;
pub mod enlace;
pub mod frame;
pub mod identity;
pub mod preferences;
pub mod preview;
pub mod search;
pub mod state;
pub mod tela;
pub mod tofu;
pub mod video;
pub mod voice;

pub use battery::{Battery, Link};
pub use client::{
    AttachmentRequest, Client, ConnectError, FlowControl, MediaChannel, Pattern, Previewed, Sent,
    SessionInfo, Transfers,
};
pub use ed25519_dalek::SigningKey;
pub use identity::FilePinStore;
pub use preview::{
    check_dogma_icon, IconRefusal, ImageFormat, Verdict as PreviewVerdict, PREVIEW_LIMIT,
};
pub use state::{
    CageSync, Changed, ChavePedida, Ended, Message, Notice, Pilot, Room, Tela, TransferNotice,
};
pub use tela::{
    menor_resolucao, resolucao_estimada_para, Envio, ErroDeTela, MotivoDeDescarte, MotivoDeParada,
    PernaQueAperta, QuadroRecebido, Recepcao, Teto, TetoDeVideo, Transmissao,
};
pub use tofu::{MemoryPinStore, PinDecision, PinStore, Verdict};
pub use video::{
    Ajuste, Compartilhamento, ErroDeCompartilhamento, FonteDeQuadros, Passo as PassoDeVideo,
};
pub use voice::{
    capture_devices, playback_devices, CaptureDevice, DeviceChoice, DeviceRates, PlaybackDevice,
    Voice, VoiceMode,
};

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
    AlertReason, AlertSeverity, AttachmentInfo, AttachmentRefusal, AttachmentState, CageInfo,
    DisconnectReason, LineInfo, Permission, PilotProfile, PilotState, Presence, ServerMessage,
    Subsystem, SubsystemHealth, Telemetry,
};
/// A impressão digital de uma chave pública, no formato que o produto mostra.
///
/// Reexportada porque a casca precisa dela para se reconhecer no próprio Dogma
/// — quem hospeda tem de estar admitido antes de bater na própria porta — e a
/// regra de dependência do ADR 0002 não deixa a casca ver `seele-proto`.
pub use seele_proto::transport::key_fingerprint;

pub use seele_proto::ids::{
    AttachmentId, CageId, ClientMessageId, LineId, MessageId, PilotId, RoleId, ScreenId, SessionId,
    Ssrc,
};
/// O cabeçalho de abertura de uma transmissão de tela, e o que ele carrega.
///
/// Reexportado porque a casca é quem sabe **o que** a pessoa escolheu
/// compartilhar — um monitor ou uma janela — e em que resolução, e a regra de
/// dependência do ADR 0002 não deixa a casca ver `seele-proto`. Note o que
/// **não** está aqui: quadros por segundo. §5, e é decisão e não esquecimento —
/// a tela não promete a escolha, e um número que a casca pudesse escrever no
/// cabeçalho seria a casca prometendo.
pub use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource};
pub use seele_proto::sync_ratio::{SyncBand, SyncInputs, SyncRatio};
pub use seele_proto::transport::DEFAULT_PORT;
pub use seele_proto::uri;
pub use seele_proto::PROTOCOL_VERSION;
