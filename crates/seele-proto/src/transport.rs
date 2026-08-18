//! Transport constants shared by both ends.
//!
//! Plain data, no I/O. `specs/01-arquitetura.md` makes this crate depend on
//! nothing, and ADR 0002 keeps `seele-server` and `seele-core` from sharing a
//! transport crate — their QUIC setups are genuinely different, since one binds
//! and presents a certificate while the other connects and pins one. What they
//! do share is these numbers, and a number in two places drifts.

use std::time::Duration;

/// ALPN identifier negotiated during the TLS handshake.
///
/// Versioned separately from [`crate::PROTOCOL_VERSION`] on purpose: ALPN is
/// how two peers refuse each other *before* a single application byte is
/// exchanged, which is cheaper and safer than discovering it afterwards.
pub const ALPN: &[u8] = b"seele/1";

/// Default listening port, UDP.
///
/// `specs/01-arquitetura.md` proposes 8383 and marks it open; the design
/// prototype in `design/` shows 7743. Unresolved — see
/// `docs/adr/0005-porta-padrao.md`. This constant is the single place to change
/// when it is decided.
pub const DEFAULT_PORT: u16 = 8383;

/// How long a peer may be silent before the connection is dropped.
///
/// `specs/02-protocolo.md` sends a `Ping` every 5 s and declares
/// `Reconectando` after three are missed. QUIC's own idle timeout sits above
/// that so the application-level state machine is what notices first — two
/// layers racing to detect the same failure produce two different stories about
/// what happened.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(20);

/// How often QUIC sends its own keepalive.
///
/// Below [`IDLE_TIMEOUT`] so a connection carrying nothing but silence — which
/// with DTX is most of a call — stays up.
pub const KEEPALIVE: Duration = Duration::from_secs(5);

/// Longest a handshake may take before the server gives up.
///
/// `specs/02-protocolo.md`: "Handshake timeout: 10 s. Failure produces
/// `PadraoAzulNaoEstabelecido` with a specific reason, never a generic one."
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a disconnected client keeps its session alive locally.
///
/// `specs/07-tema-evangelion.md` calls this the "bateria interna" and gives it a
/// 04:59 countdown; `specs/02-protocolo.md` has the server hold the slot for the
/// same period.
pub const SESSION_GRACE: Duration = Duration::from_secs(5 * 60);

/// Frames per second an honest client sends.
///
/// One 20 ms frame at a time (`specs/03-audio.md`).
pub const NOMINAL_FRAMES_PER_SECOND: u32 = 50;

/// Frames per second above which a sender is dropped and logged.
///
/// `specs/04-servidor-seele.md`: "per-sender limit on frames per second (an
/// honest client sends 50/s). Above that, discard and log. Protects against a
/// malicious client saturating the Cage." The margin absorbs bursts after a
/// scheduling hiccup without admitting a flood.
pub const MAX_FRAMES_PER_SECOND: u32 = 60;

/// Participants above which a Cage starts forwarding only the loudest.
///
/// `specs/01-arquitetura.md` sets the threshold and leaves the policy open —
/// see decision D13 in `docs/plano-m0-m1.md`. Nothing implements the policy yet;
/// this is the number it will key off.
pub const CAGE_ACTIVE_SPEAKER_THRESHOLD: usize = 20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keepalive_fits_inside_the_idle_timeout() {
        // A keepalive at or above the idle timeout is a connection that drops
        // while both ends believe it is healthy.
        const { assert!(KEEPALIVE.as_secs() * 2 < IDLE_TIMEOUT.as_secs()) };
    }

    #[test]
    fn the_application_notices_a_dead_peer_before_quic_does() {
        // specs/02-protocolo.md: Ping every 5 s, three missed means
        // `Reconectando` — 15 s. QUIC's idle timeout has to sit above that, or
        // the transport tears the connection down while the state machine is
        // still deciding, and the user gets the wrong explanation.
        const { assert!(KEEPALIVE.as_secs() * 3 < IDLE_TIMEOUT.as_secs()) };
    }

    #[test]
    fn the_flood_limit_leaves_room_for_a_hiccup() {
        // specs/04-servidor-seele.md puts an honest client at 50/s. A limit at
        // exactly 50 would disconnect anybody whose scheduler stuttered and then
        // caught up.
        const { assert!(MAX_FRAMES_PER_SECOND > NOMINAL_FRAMES_PER_SECOND) };
        const { assert!(MAX_FRAMES_PER_SECOND < NOMINAL_FRAMES_PER_SECOND * 2) };
    }

    #[test]
    fn the_handshake_budget_is_shorter_than_the_idle_timeout() {
        // Otherwise a stalled handshake is collected by the idle timer with a
        // transport error instead of the specific reason specs/02 demands.
        const { assert!(HANDSHAKE_TIMEOUT.as_secs() < IDLE_TIMEOUT.as_secs()) };
    }
}

/// Fingerprint of a DER-encoded certificate: SHA-256, lowercase hex, colon-free.
///
/// ADR 0003 makes TOFU the default, so this string is what a client pins and
/// what it compares on every later connection. `specs/08-seguranca.md` requires
/// the change warning to be "impossible to ignore — literally a blocking
/// `Alerta · 警告`", and this is the value behind it.
///
/// It lives here rather than in either end because both must compute it
/// **identically**: a client that hashes differently from the server would warn
/// about a key change that never happened, and users who learn to dismiss that
/// warning are exactly the users TOFU cannot protect.
#[must_use]
pub fn certificate_fingerprint(der: &[u8]) -> String {
    hex_sha256(der)
}

/// Fingerprint of a person's Ed25519 public key: SHA-256, lowercase hex.
///
/// The mirror image of [`certificate_fingerprint`]. That one names the machine
/// so whoever arrives can pin it (ADR 0003); this one names the *person* so
/// whoever hosts can decide about them (ADR 0030). Same hash, same shape,
/// deliberately: the two strings are read by the same eyes, out loud, over the
/// same phone call, and one of them being formatted differently is one more
/// thing that can be compared wrongly.
///
/// It is not the public key itself for the reason every fingerprint exists:
/// what a person compares by eye has to be short enough that they finish, and
/// a digest that differs anywhere differs visibly at the start.
///
/// Distinct from [`certificate_fingerprint`] as a function rather than reused,
/// even though the bytes go through the same digest, because the two answer
/// different questions and a call site that picks the wrong one would be
/// reading a machine's identity as a person's.
#[must_use]
pub fn key_fingerprint(public_key: &[u8]) -> String {
    hex_sha256(public_key)
}

/// SHA-256, lowercase hex, colon-free.
///
/// One body under both names above: two hex loops would be two places for the
/// formatting to drift, and drift here reads as a key that changed.
fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        // Writing to a String cannot fail; the result is discarded rather than
        // unwrapped so this stays usable from anywhere.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    #[test]
    fn a_fingerprint_is_sixty_four_hex_characters() {
        let print = certificate_fingerprint(b"whatever");
        assert_eq!(print.len(), 64, "SHA-256 is 32 bytes");
        assert!(print.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(print.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn the_same_certificate_always_prints_the_same() {
        assert_eq!(
            certificate_fingerprint(b"seele"),
            certificate_fingerprint(b"seele")
        );
    }

    #[test]
    fn a_different_certificate_prints_differently() {
        // The property TOFU rests on. If this ever stops holding, a key change
        // goes unnoticed and the whole model is decoration.
        assert_ne!(
            certificate_fingerprint(b"seele"),
            certificate_fingerprint(b"magj")
        );
    }

    #[test]
    fn an_empty_certificate_still_prints() {
        // A malformed peer must not make the pinning code panic.
        assert_eq!(certificate_fingerprint(&[]).len(), 64);
    }

    #[test]
    fn a_person_prints_in_the_same_shape_as_a_machine() {
        // ADR 0030 puts the two side by side in front of the same person: the
        // Dogma's fingerprint on the entry screen, the knocker's on the host's
        // approval card. One of them formatted differently — uppercase, or with
        // colons — is one more way to compare two strings wrongly.
        let pessoa = key_fingerprint(&[7_u8; 32]);
        assert_eq!(pessoa.len(), 64);
        assert!(pessoa.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(pessoa.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn two_people_print_differently() {
        // The property the doorkeeper rests on, and the same one TOFU rests on
        // above: if two keys could print alike, approving one would admit the
        // other.
        assert_ne!(key_fingerprint(&[1_u8; 32]), key_fingerprint(&[2_u8; 32]));
    }

    #[test]
    fn an_empty_key_still_prints() {
        // `key_fingerprint` is called on whatever bytes arrived in the `Hello`,
        // and a peer that sends none must not make the doorkeeper panic.
        assert_eq!(key_fingerprint(&[]).len(), 64);
    }
}
