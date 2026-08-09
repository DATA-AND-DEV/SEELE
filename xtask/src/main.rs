//! Repository automation for the SEELE workspace.
//!
//! Run with `cargo xtask <command>`. This crate is tooling — it is never shipped
//! and is not part of the product.

use std::process::ExitCode;

mod check_deps;

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("check-deps") => check_deps::run(),
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
    eprintln!("  check-deps   enforce the dependency rule from specs/01-arquitetura.md");
}
