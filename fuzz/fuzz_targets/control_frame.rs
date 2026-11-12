//! Fuzzes the control-frame parser.
//!
//! `specs/09-roadmap.md` makes this an explicit acceptance criterion for M2:
//! "fuzzing of the parser without a crash". `specs/08-seguranca.md` names the
//! same surface: it is where untrusted network bytes land, and on the control
//! stream they land *before* the handshake has finished.
//!
//! Run with: cargo +nightly fuzz run control_frame
//!
//! A propriedade mora em `fuzz/propriedades/control_frame.rs`, e não aqui: o
//! `cargo test -p seele-proto --test sementes_do_fuzz` passa as sementes pelo
//! mesmo código, sem o nightly. Ver lá por que ela compara os bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../propriedades/control_frame.rs"]
mod propriedade;

fuzz_target!(|data: &[u8]| {
    propriedade::conferir(data);
});
