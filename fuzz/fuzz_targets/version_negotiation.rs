//! Fuzzes the first decision made on bytes from an unauthenticated socket.
//!
//! `specs/08-seguranca.md`: "Fuzzing of the magi-proto parsers (cargo-fuzz) — it
//! is the surface that receives untrusted network bytes."
//!
//! Right now that surface is one byte wide: `specs/02-protocolo.md` puts the
//! protocol version in the first byte of every control frame, and version
//! negotiation runs before authentication. The target is deliberately trivial
//! today and becomes real in M2, when the wire decoder lands — the point of
//! having it in M0 is that the harness is proven to build and run, so M2 only
//! has to add cases.
//!
//! Run with: cargo +nightly fuzz run version_negotiation

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // A frame with no bytes at all is a real thing to receive.
    let Some(version) = data.first().copied() else {
        return;
    };

    // Must be total: every byte produces an answer, none of them panics.
    if let Ok(agreed) = magi_proto::version::negotiate(version) {
        assert!(agreed <= magi_proto::PROTOCOL_VERSION);
        assert!(agreed >= magi_proto::oldest_supported_version());
    }
});
