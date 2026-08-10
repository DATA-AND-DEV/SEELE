//! **Entry Plug** — the terminal client, `plug`.
//!
//! `specs/00-visao-geral.md` puts this first:
//!
//! > **Terminal como cidadão de primeira classe.** A TUI não é uma versão
//! > reduzida — é a referência. Os clientes gráficos é que imitam ela.
//!
//! Everything here translates events into cells and input into commands.
//! `specs/01-arquitetura.md` calls that boundary the most important contract in
//! the project, and ADR 0002 enforces it mechanically: this crate can see
//! `seele-core` and nothing else. If something here needs to know what an `ssrc`
//! is, the logic that needs it belongs in the core.

// A test that has to handle the impossible case stops reading as a statement
// about the code. The same allowance the other crates take.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

pub mod app;
pub mod command;
pub mod selecao;
pub mod text;
pub mod theme;
pub mod ui;
pub mod view;
