//! Repository automation for the SEELE workspace.
//!
//! Run with `cargo xtask <command>`. This crate is tooling — it is never shipped
//! and is not part of the product.

use std::process::ExitCode;

mod check_api;
mod check_deps;
mod check_versao;

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("check-api") => check_api::run(),
        Some("check-deps") => check_deps::run(),
        Some("check-versao") => check_versao::run(),
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`");
            usage();
            ExitCode::FAILURE
        }
        None => {
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!("usage: cargo xtask <command>");
    eprintln!();
    eprintln!("commands:");
    eprintln!("  check-api    enforce the MOD API façade from `api/` (ADR 0045)");
    eprintln!("  check-deps   enforce the dependency rule from specs/01-arquitetura.md");
    eprintln!("  check-versao prova que a versão do produto alcança tudo o que a carrega");
}
