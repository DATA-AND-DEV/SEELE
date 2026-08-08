//! Fuzzes the control-frame parser.
//!
//! `specs/09-roadmap.md` makes this an explicit acceptance criterion for M2:
//! "fuzzing of the parser without a crash". `specs/08-seguranca.md` names the
//! same surface: it is where untrusted network bytes land, and on the control
//! stream they land *before* the handshake has finished.
//!
//! Run with: cargo +nightly fuzz run control_frame

#![no_main]

use libfuzzer_sys::fuzz_target;
use magi_proto::control::{decode, encode, ClientMessage, ServerMessage, MAX_FRAME_LEN};

fuzz_target!(|data: &[u8]| {
    // Both directions: a client parses server frames and a server parses client
    // frames, and only one of those is behind authentication.
    if let Ok(message) = decode::<ClientMessage>(data) {
        assert!(data.len() <= MAX_FRAME_LEN);
        // Anything accepted must re-encode and re-decode to itself. A mismatch
        // means two peers can read the same bytes as different messages.
        let frame = encode(&message).expect("an accepted message must re-encode");
        let again = decode::<ClientMessage>(&frame).expect("a re-encoded frame must decode");
        assert_eq!(again, message);
    }

    if let Ok(message) = decode::<ServerMessage>(data) {
        assert!(data.len() <= MAX_FRAME_LEN);
        let frame = encode(&message).expect("an accepted message must re-encode");
        let again = decode::<ServerMessage>(&frame).expect("a re-encoded frame must decode");
        assert_eq!(again, message);
    }
});
