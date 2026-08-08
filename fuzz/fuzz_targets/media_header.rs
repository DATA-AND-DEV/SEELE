//! Fuzzes the media datagram parser.
//!
//! `specs/08-seguranca.md`: "Fuzzing of the magi-proto parsers (cargo-fuzz) — it
//! is the surface that receives untrusted network bytes." This is that surface
//! for real: every voice frame from every talker arrives here, before any
//! authentication has been checked on the datagram itself.
//!
//! Run with: cargo +nightly fuzz run media_header

#![no_main]

use libfuzzer_sys::fuzz_target;
use magi_proto::media::{MediaHeader, HEADER_LEN, MAX_DATAGRAM_LEN};

fuzz_target!(|data: &[u8]| {
    match MediaHeader::decode(data) {
        Ok((header, payload)) => {
            // Anything accepted must be internally consistent, or a later stage
            // will trust a value this one invented.
            assert!(data.len() >= HEADER_LEN);
            assert!(data.len() <= MAX_DATAGRAM_LEN);
            assert!(!payload.is_empty());
            assert_eq!(payload.len(), data.len() - HEADER_LEN);
            assert_eq!(header.version, magi_proto::PROTOCOL_VERSION);

            // Re-encoding an accepted datagram must reproduce it exactly.
            // A mismatch means encode and decode disagree about the layout,
            // which on the wire is two peers hearing different audio.
            let mut round_trip = vec![0_u8; data.len()];
            let len = header
                .encode_datagram(payload, &mut round_trip)
                .expect("an accepted datagram must re-encode");
            assert_eq!(&round_trip[..len], data);
        }
        Err(_) => {
            // Rejection is always acceptable. What is not acceptable is a panic,
            // and reaching this arm at all proves there was not one.
        }
    }
});
