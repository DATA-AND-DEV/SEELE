//! Fuzzes the first decision made on bytes from an unauthenticated socket.
//!
//! `specs/08-seguranca.md`: "Fuzzing of the seele-proto parsers (cargo-fuzz) — it
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
    if let Ok(agreed) = seele_proto::version::confere_o_carimbo(version) {
        assert!(agreed <= seele_proto::PROTOCOL_VERSION);
        assert!(agreed >= seele_proto::oldest_supported_version());
    }
    let _ = seele_proto::version::linha_de(version);

    // Com dois bytes, a faixa de um `Hello` (ADR 0058): o acordo, quando há,
    // é um número que esta build fala e que está dentro da faixa do par.
    if let Some(&maxima) = data.get(1) {
        let do_par = seele_proto::version::Faixa {
            minima: version,
            maxima,
        };
        if let Ok(acordo) = seele_proto::version::negociar(do_par) {
            assert!(seele_proto::version::confere_o_carimbo(acordo).is_ok());
            assert!(acordo <= do_par.maxima);
        }
    }
});
