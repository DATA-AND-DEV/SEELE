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
    /// The certificate matches what was pinned. Nothing to say.
    Matches,
    /// The certificate does **not** match. The connection was refused.
    Changed {
        /// What was pinned before.
        pinned: String,
        /// What the server offered now.
        offered: String,
    },
}

/// Where pinned fingerprints live.
///
/// A trait so the TUI can persist to disk while tests keep everything in memory.
pub trait PinStore: Send + Sync + std::fmt::Debug {
    /// The fingerprint pinned for a host, if any.
    fn pinned(&self, host: &str) -> Option<String>;
    /// Records a fingerprint for a host.
    fn pin(&self, host: &str, fingerprint: String);
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
}

/// A rustls verifier that pins instead of chasing a certificate authority.
#[derive(Debug)]
pub struct TofuVerifier {
    store: Arc<dyn PinStore>,
    /// The last decision, so the shell can report what happened.
    last: Mutex<Option<PinDecision>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl TofuVerifier {
    /// A verifier backed by the given store.
    #[must_use]
    pub fn new(store: Arc<dyn PinStore>) -> Self {
        Self {
            store,
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
        let offered = magi_proto::transport::certificate_fingerprint(certificate);
        match self.store.pinned(host) {
            None => {
                self.store.pin(host, offered.clone());
                PinDecision::FirstContact {
                    fingerprint: offered,
                }
            }
            Some(pinned) if pinned == offered => PinDecision::Matches,
            Some(pinned) => PinDecision::Changed { pinned, offered },
        }
    }
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let host = match server_name {
            ServerName::DnsName(name) => name.as_ref().to_owned(),
            ServerName::IpAddress(address) => format!("{address:?}"),
            _ => "unknown".to_owned(),
        };

        let decision = self.decide(&host, end_entity.as_ref());
        if let Ok(mut last) = self.last.lock() {
            *last = Some(decision.clone());
        }

        match decision {
            PinDecision::FirstContact { .. } | PinDecision::Matches => {
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
        TofuVerifier::new(Arc::new(MemoryPinStore::new()))
    }

    #[test]
    fn the_first_contact_is_recorded_and_allowed() {
        let verifier = verifier();
        let decision = verifier.decide("dogma.example", b"certificate-one");

        let PinDecision::FirstContact { fingerprint } = decision else {
            panic!("first contact should have been recorded");
        };
        assert_eq!(
            fingerprint,
            magi_proto::transport::certificate_fingerprint(b"certificate-one")
        );
    }

    #[test]
    fn the_same_certificate_matches_afterwards() {
        let verifier = verifier();
        verifier.decide("dogma.example", b"certificate-one");
        assert_eq!(
            verifier.decide("dogma.example", b"certificate-one"),
            PinDecision::Matches
        );
    }

    #[test]
    fn a_changed_certificate_is_reported_with_both_fingerprints() {
        // specs/08-seguranca.md wants the warning impossible to ignore, and an
        // operator answering "was that you?" needs both values to compare.
        let verifier = verifier();
        verifier.decide("dogma.example", b"certificate-one");

        let PinDecision::Changed { pinned, offered } =
            verifier.decide("dogma.example", b"certificate-two")
        else {
            panic!("a changed certificate went unnoticed");
        };
        assert_eq!(
            pinned,
            magi_proto::transport::certificate_fingerprint(b"certificate-one")
        );
        assert_eq!(
            offered,
            magi_proto::transport::certificate_fingerprint(b"certificate-two")
        );
        assert_ne!(pinned, offered);
    }

    #[test]
    fn a_changed_certificate_does_not_overwrite_the_pin() {
        // Recording the new key would turn the second connection into a silent
        // acceptance, which is the failure mode TOFU exists to prevent.
        let verifier = verifier();
        verifier.decide("dogma.example", b"certificate-one");
        verifier.decide("dogma.example", b"certificate-two");

        assert!(
            matches!(
                verifier.decide("dogma.example", b"certificate-two"),
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
            PinDecision::Matches
        );
    }
}
