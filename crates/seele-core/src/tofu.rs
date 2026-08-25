//! Trust on first use.
//!
//! ADR 0003 makes this the default, and `specs/08-seguranca.md` describes it:
//!
//! > **TOFU (trust on first use)** with a self-signed certificate: the client
//! > memorises the public key on the first connection and warns loudly if it
//! > ever changes. Friendly for self-hosting, it is the SSH model, and the
//! > audience understands it. Requires explicit UX for acceptance and for key
//! > change.
//! >
//! > The key-change warning must be impossible to ignore — in the theme, it is
//! > literally a blocking `Alerta · 警告`.
//!
//! # What this module does and does not decide
//!
//! It decides whether a certificate **matches what was pinned**. It does not
//! decide what to show a user, because that is the shell's job
//! (`specs/01-arquitetura.md`) and [`PinDecision`] is plain data for a shell to
//! match on.
//!
//! What it does enforce is that a **changed** key fails the TLS handshake rather
//! than producing a warning somebody can click past. A warning that can be
//! dismissed protects nobody, and `specs/08-seguranca.md` calls the alert
//! blocking for exactly that reason.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};

/// What happened when a certificate was checked against the pin store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinDecision {
    /// Nothing was pinned for this host. The key has now been recorded.
    ///
    /// `specs/08-seguranca.md` wants explicit acceptance UX here. The connection
    /// proceeds because refusing every first contact would make the product
    /// unusable, but the shell should say what it just trusted.
    FirstContact {
        /// The fingerprint now pinned.
        fingerprint: String,
    },
    /// The certificate matches what was pinned.
    ///
    /// Carries the fingerprint because a caller comparing an invite against
    /// what the server offered needs something to compare *with*. Without it
    /// the terminal client ended up comparing the expected value with itself,
    /// which is a test that cannot fail.
    Matches {
        /// The fingerprint that both the pin and the certificate carry.
        fingerprint: String,
    },
    /// The certificate does **not** match. The connection was refused.
    Changed {
        /// What was pinned before.
        pinned: String,
        /// What the server offered now.
        offered: String,
    },
}

/// What the check concluded — already decided, for the shell to only draw.
///
/// Five variants because there are five distinct things to say. `PinDecision`
/// describes what the TOFU verifier saw; this describes what to do about it,
/// and the gap between the two is why this type exists at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing was pinned and no invite vouched for anything. Pinned blind.
    ///
    /// `specs/08-seguranca.md` wants this stated rather than accepted in
    /// silence — the shell must say what it just trusted.
    FirstContact {
        /// What was pinned.
        fingerprint: String,
    },
    /// Nothing was pinned, and the invite confirmed what the server offered.
    ///
    /// This is what ADR 0006 invented the link to produce.
    FirstContactVerified {
        /// What was pinned, now vouched for.
        fingerprint: String,
    },
    /// The pin matches and nothing contradicts it. Nothing to say.
    Known,
    /// First contact, and the invite named a different key. Refused.
    InviteRefused {
        /// What the link promised.
        expected: String,
        /// What the server offered.
        offered: String,
    },
    /// The pin is the usual one, but the invite names a different key.
    ///
    /// The connection stands: trust on first use already established that this
    /// is the same server as before, so the link is what is wrong.
    InviteDisagrees {
        /// What the link promised.
        expected: String,
        /// What the server offered, and what stays pinned.
        offered: String,
    },
}

/// Turns what the TOFU verifier saw into what to do about it.
///
/// Pure on purpose: the refusal's side effect — removing the pin the verifier
/// already wrote — belongs to the caller, so this can be tested as the table
/// it is.
#[must_use]
pub fn verdict(decision: &PinDecision, expected: Option<&str>) -> Verdict {
    let agrees =
        |offered: &str| expected.is_none_or(|expected| expected.eq_ignore_ascii_case(offered));

    match decision {
        PinDecision::FirstContact { fingerprint } if agrees(fingerprint) => {
            if expected.is_some() {
                Verdict::FirstContactVerified {
                    fingerprint: fingerprint.clone(),
                }
            } else {
                Verdict::FirstContact {
                    fingerprint: fingerprint.clone(),
                }
            }
        }
        PinDecision::FirstContact { fingerprint } => Verdict::InviteRefused {
            expected: expected.unwrap_or_default().to_owned(),
            offered: fingerprint.clone(),
        },
        PinDecision::Matches { fingerprint } if agrees(fingerprint) => Verdict::Known,
        PinDecision::Matches { fingerprint } => Verdict::InviteDisagrees {
            expected: expected.unwrap_or_default().to_owned(),
            offered: fingerprint.clone(),
        },
        // `Changed` never reaches here: the verifier refuses it at the TLS
        // layer, with or without an invite, and it surfaces as a connection
        // error rather than a verdict.
        PinDecision::Changed { pinned, offered } => Verdict::InviteRefused {
            expected: pinned.clone(),
            offered: offered.clone(),
        },
    }
}

/// Where pinned fingerprints live.
///
/// A trait so the TUI can persist to disk while tests keep everything in memory.
pub trait PinStore: Send + Sync + std::fmt::Debug {
    /// The fingerprint pinned for a host, if any.
    fn pinned(&self, host: &str) -> Option<String>;
    /// Records a fingerprint for a host.
    fn pin(&self, host: &str, fingerprint: String);
    /// Forgets the fingerprint pinned for a host.
    ///
    /// Exists because refusing a connection is not enough on its own: the
    /// verifier pins before anyone can judge, so a refusal that left the pin
    /// behind would let the very next visit — without a link to check against
    /// — walk into the server that was just rejected.
    fn unpin(&self, host: &str);
}

/// A pin store that forgets everything when the process exits.
#[derive(Debug, Default)]
pub struct MemoryPinStore {
    pins: Mutex<HashMap<String, String>>,
}

impl MemoryPinStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl PinStore for MemoryPinStore {
    fn pinned(&self, host: &str) -> Option<String> {
        self.pins
            .lock()
            .ok()
            .and_then(|pins| pins.get(host).cloned())
    }

    fn pin(&self, host: &str, fingerprint: String) {
        if let Ok(mut pins) = self.pins.lock() {
            pins.insert(host.to_owned(), fingerprint);
        }
    }

    fn unpin(&self, host: &str) {
        if let Ok(mut pins) = self.pins.lock() {
            pins.remove(host);
        }
    }
}

/// A rustls verifier that pins instead of chasing a certificate authority.
#[derive(Debug)]
pub struct TofuVerifier {
    store: Arc<dyn PinStore>,
    /// The last decision, so the shell can report what happened.
    /// What this connection's pin is filed under. See [`TofuVerifier::new`].
    pin_key: String,
    last: Mutex<Option<PinDecision>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl TofuVerifier {
    /// A verifier backed by the given store, filing under `pin_key`.
    ///
    /// The key is given rather than taken from the TLS server name, because the
    /// two are different things and conflating them was a real bug: this
    /// verifier never checks the certificate's names — it compares
    /// fingerprints — so the TLS name is only a label, and both shells were
    /// labelling every IP address `localhost`. Two servers on a LAN then shared
    /// one pin entry, and the second one to be contacted looked like the first
    /// one's key had changed. That is the most alarming false positive this
    /// system can produce, and it would have fired the first time somebody
    /// tested between two machines.
    ///
    /// The key should be the target as the person typed it, port included: two
    /// servers on one host at different ports are two servers.
    #[must_use]
    pub fn new(store: Arc<dyn PinStore>, pin_key: String) -> Self {
        Self {
            store,
            pin_key,
            last: Mutex::new(None),
            provider: Arc::new(rustls::crypto::ring::default_provider()),
        }
    }

    /// What the most recent handshake decided.
    #[must_use]
    pub fn last_decision(&self) -> Option<PinDecision> {
        self.last.lock().ok().and_then(|last| last.clone())
    }

    /// The pinning decision for one certificate, without any TLS machinery.
    ///
    /// Split out so the rule can be tested directly: the rustls trait needs a
    /// full handshake to exercise, and this is where the actual policy lives.
    pub fn decide(&self, host: &str, certificate: &[u8]) -> PinDecision {
        let offered = seele_proto::transport::certificate_fingerprint(certificate);
        match self.store.pinned(host) {
            None => {
                self.store.pin(host, offered.clone());
                PinDecision::FirstContact {
                    fingerprint: offered,
                }
            }
            Some(pinned) if pinned == offered => PinDecision::Matches {
                fingerprint: offered,
            },
            Some(pinned) => PinDecision::Changed { pinned, offered },
        }
    }
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        // Deliberately not derived from `_server_name`: see `Self::new`.
        let decision = self.decide(&self.pin_key.clone(), end_entity.as_ref());
        if let Ok(mut last) = self.last.lock() {
            *last = Some(decision.clone());
        }

        match decision {
            PinDecision::FirstContact { .. } | PinDecision::Matches { .. } => {
                Ok(ServerCertVerified::assertion())
            }
            // The handshake fails. specs/08-seguranca.md makes this alert
            // blocking, and a warning a user can dismiss is not one.
            PinDecision::Changed { .. } => Err(TlsError::General(
                "the server's certificate has changed since it was pinned".into(),
            )),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verifier() -> TofuVerifier {
        TofuVerifier::new(Arc::new(MemoryPinStore::new()), "seele.exemplo".to_owned())
    }

    #[test]
    fn the_first_contact_is_recorded_and_allowed() {
        let verifier = verifier();
        let decision = verifier.decide("server.example", b"certificate-one");

        let PinDecision::FirstContact { fingerprint } = decision else {
            panic!("first contact should have been recorded");
        };
        assert_eq!(
            fingerprint,
            seele_proto::transport::certificate_fingerprint(b"certificate-one")
        );
    }

    #[test]
    fn the_same_certificate_matches_afterwards() {
        let verifier = verifier();
        verifier.decide("server.example", b"certificate-one");
        assert_eq!(
            verifier.decide("server.example", b"certificate-one"),
            PinDecision::Matches {
                fingerprint: seele_proto::transport::certificate_fingerprint(b"certificate-one")
            }
        );
    }

    #[test]
    fn a_changed_certificate_is_reported_with_both_fingerprints() {
        // specs/08-seguranca.md wants the warning impossible to ignore, and an
        // operator answering "was that you?" needs both values to compare.
        let verifier = verifier();
        verifier.decide("server.example", b"certificate-one");

        let PinDecision::Changed { pinned, offered } =
            verifier.decide("server.example", b"certificate-two")
        else {
            panic!("a changed certificate went unnoticed");
        };
        assert_eq!(
            pinned,
            seele_proto::transport::certificate_fingerprint(b"certificate-one")
        );
        assert_eq!(
            offered,
            seele_proto::transport::certificate_fingerprint(b"certificate-two")
        );
        assert_ne!(pinned, offered);
    }

    #[test]
    fn a_changed_certificate_does_not_overwrite_the_pin() {
        // Recording the new key would turn the second connection into a silent
        // acceptance, which is the failure mode TOFU exists to prevent.
        let verifier = verifier();
        verifier.decide("server.example", b"certificate-one");
        verifier.decide("server.example", b"certificate-two");

        assert!(
            matches!(
                verifier.decide("server.example", b"certificate-two"),
                PinDecision::Changed { .. }
            ),
            "the impostor's key was pinned"
        );
    }

    #[test]
    fn hosts_are_pinned_independently() {
        let verifier = verifier();
        verifier.decide("first.example", b"certificate-one");
        assert!(matches!(
            verifier.decide("second.example", b"certificate-two"),
            PinDecision::FirstContact { .. }
        ));
        assert_eq!(
            verifier.decide("first.example", b"certificate-one"),
            PinDecision::Matches {
                fingerprint: seele_proto::transport::certificate_fingerprint(b"certificate-one")
            }
        );
    }

    const A: &str = "aaaa1111";
    const B: &str = "bbbb2222";

    #[test]
    fn a_first_contact_with_no_invite_is_blind_and_says_so() {
        let decision = PinDecision::FirstContact {
            fingerprint: A.into(),
        };
        assert_eq!(
            verdict(&decision, None),
            Verdict::FirstContact {
                fingerprint: A.into()
            }
        );
    }

    #[test]
    fn a_first_contact_the_invite_confirms_is_verified() {
        // ADR 0006 exists to produce exactly this outcome, and until now
        // nothing could tell it apart from the blind one.
        let decision = PinDecision::FirstContact {
            fingerprint: A.into(),
        };
        assert_eq!(
            verdict(&decision, Some(A)),
            Verdict::FirstContactVerified {
                fingerprint: A.into()
            }
        );
    }

    #[test]
    fn a_first_contact_the_invite_contradicts_is_refused() {
        // No prior pin, so the invite was the only evidence, and it failed.
        let decision = PinDecision::FirstContact {
            fingerprint: A.into(),
        };
        assert_eq!(
            verdict(&decision, Some(B)),
            Verdict::InviteRefused {
                expected: B.into(),
                offered: A.into()
            }
        );
    }

    #[test]
    fn a_matching_pin_with_no_invite_has_nothing_to_say() {
        let decision = PinDecision::Matches {
            fingerprint: A.into(),
        };
        assert_eq!(verdict(&decision, None), Verdict::Known);
    }

    #[test]
    fn a_matching_pin_the_invite_confirms_has_nothing_to_say_either() {
        let decision = PinDecision::Matches {
            fingerprint: A.into(),
        };
        assert_eq!(verdict(&decision, Some(A)), Verdict::Known);
    }

    #[test]
    fn a_matching_pin_the_invite_contradicts_warns_and_does_not_refuse() {
        // This is the hole: `connection` compared the expected value with itself,
        // because `Matches` carried no fingerprint to compare against.
        // Trust on first use already proved this is yesterday's server, so
        // the link is what is wrong — refusing would lock somebody out of a
        // server they use because a friend sent a stale link.
        let decision = PinDecision::Matches {
            fingerprint: A.into(),
        };
        assert_eq!(
            verdict(&decision, Some(B)),
            Verdict::InviteDisagrees {
                expected: B.into(),
                offered: A.into()
            }
        );
    }

    #[test]
    fn the_comparison_ignores_case() {
        let decision = PinDecision::FirstContact {
            fingerprint: "abcdef".into(),
        };
        assert_eq!(
            verdict(&decision, Some("ABCDEF")),
            Verdict::FirstContactVerified {
                fingerprint: "abcdef".into()
            }
        );
    }

    #[test]
    fn the_comparison_ignores_case_for_a_matching_pin_too() {
        let decision = PinDecision::Matches {
            fingerprint: "abcdef".into(),
        };
        assert_eq!(verdict(&decision, Some("ABCDEF")), Verdict::Known);
    }

    #[test]
    fn unpinning_a_host_makes_the_next_visit_a_first_contact_again() {
        // The refusal has to undo the pin the verifier already wrote, or the
        // next visit without a link walks straight into the server that was
        // just rejected.
        let store = MemoryPinStore::new();
        store.pin("casa", A.into());
        assert_eq!(store.pinned("casa"), Some(A.into()));

        store.unpin("casa");
        assert_eq!(store.pinned("casa"), None);
    }

    #[test]
    fn unpinning_a_host_that_was_never_pinned_is_not_an_error() {
        let store = MemoryPinStore::new();
        store.unpin("nunca visto");
        assert_eq!(store.pinned("nunca visto"), None);
    }
}
