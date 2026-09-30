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
//!
//! # A impressão esperada
//!
//! Quando quem conecta espera uma chave — a impressão de um link, colado agora
//! ou guardado na lista de servidores —, o verificador a confere **no primeiro
//! contato, dentro do aperto de mão**. A chave que não confere falha o TLS e não
//! é fixada, e o `Hello` não chega a sair: o convite ou a senha, o apelido e a
//! chave de identidade ficam nesta máquina. O que sai é o `ClientHello` do
//! próprio TLS, que não leva nada disso. Até a 0.15.0 essa conferência
//! acontecia depois do `Hello` (o S2b da análise de 22/09). Com pin
//! estabelecido onde ele prova o servidor (o endereço que a pessoa escolheu,
//! quando é de escopo público), a regra é a de sempre; num candidato que
//! ninguém escolheu, ou num alvo de escopo local, o pin não passa por cima da
//! impressão esperada — ver [`TofuVerifier::decide`].
//!
//! A frase que o TLS leva a quem atendeu quando recusa é uma só para os dois
//! motivos, a impressão que não confere e a chave fixada que mudou, e não conta
//! se esta máquina tinha um link ou um pin. O porquê fica em
//! [`TofuVerifier::last_decision`], para quem conectou.

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
    /// A chave não é a que a impressão esperada promete, e nenhum pin a
    /// sustenta: nada estava fixado, ou o pin daquele endereço não prova o
    /// servidor.
    ///
    /// **Recusada dentro do TLS, e nada foi fixado nem desfeito.** O aperto de
    /// mão falha aqui, antes de qualquer `Hello`: o convite, a senha e o
    /// apelido nunca saem para quem atendeu (o `ClientHello` do próprio TLS
    /// sai; o `Hello` do protocolo, não). Até a 0.15.0 este caso era um
    /// `FirstContact` que fixava a chave, mandava o `Hello` e só depois era
    /// recusado — o S2b da análise de 22/09, que a corrida de candidatos do
    /// ADR 0037 repetia em todo candidato que fechasse o TLS.
    ///
    /// Existe no primeiro contato, e mesmo com um pin que confere onde o pin
    /// não prova o servidor (um candidato que a pessoa não escolheu, ou um
    /// alvo de escopo local): ali o pin pode ser de outro servidor. Onde ele
    /// prova o servidor, com pin estabelecido, quem decide é o pin; ver
    /// [`TofuVerifier::decide`].
    InviteRefused {
        /// O que a impressão esperada prometia.
        expected: String,
        /// O que o servidor apresentou.
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

/// Se a impressão que se esperava é a que o servidor ofereceu.
///
/// **A única comparação de impressões deste módulo.** [`verdict`] e
/// [`TofuVerifier::decide`] perguntam a mesma coisa — uma ao fim do aperto de
/// mão, a outra dentro dele — e duas escritas dela divergiriam no primeiro
/// link com a impressão em maiúsculas. Não diferencia maiúsculas de minúsculas.
fn confere(esperada: &str, ofertada: &str) -> bool {
    esperada.eq_ignore_ascii_case(ofertada)
}

/// Turns what the TOFU verifier saw into what to do about it.
///
/// Pure on purpose: what to do to the pin store on a refusal belongs to the
/// caller, so this can be tested as the table it is. The verifier now refuses a
/// first contact the invite contradicts inside the TLS handshake, before
/// pinning anything, so `InviteRefused` no longer reaches here from a real
/// connection; the caller's cleanup stays as a second line.
#[must_use]
pub fn verdict(decision: &PinDecision, expected: Option<&str>) -> Verdict {
    let agrees = |offered: &str| expected.is_none_or(|expected| confere(expected, offered));

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
        // Também não chega aqui: o verificador recusa no TLS, antes do `Hello`,
        // e a falha sobe como `ConnectError::InviteMismatch`. O braço existe
        // para o `match` continuar exaustivo e dizer o mesmo que a recusa
        // disse, se alguém um dia o alcançar.
        PinDecision::InviteRefused { expected, offered } => Verdict::InviteRefused {
            expected: expected.clone(),
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
    /// Exists because the verifier pins during the TLS handshake, and the
    /// connection can still fail after that (the control stream, the deadline,
    /// the credential, the reply). A pin left behind by a handshake that did not
    /// finish would turn the next visit into a match and let it in without
    /// hesitation. A refusal by fingerprint no longer needs this: it happens
    /// inside the handshake, before anything is pinned. What callers undo is
    /// what their own handshake wrote, and, after a race of candidates, the pin
    /// nobody had before it when a candidate's deadline ran out mid-handshake.
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
    /// What this connection's pin is filed under. See [`TofuVerifier::new`].
    pin_key: String,
    /// A impressão que quem conecta espera, quando espera alguma.
    ///
    /// Decide o primeiro contato e, onde o pin não prova o servidor, também o
    /// candidato já fixado — ver [`TofuVerifier::decide`].
    esperada: Option<String>,
    /// Se o pino de `pin_key` prova o servidor: `pin_key` é o alvo (o endereço
    /// que a pessoa escolheu) e o endereço dele é de escopo público.
    ///
    /// Só pesa com pin e com esperada. O pin prova *este endereço*, e só ali
    /// este endereço é o servidor que a pessoa pediu — ver
    /// [`TofuVerifier::decide`] e `crate::enlace::Destino::o_pino_prova_o_servidor`.
    o_pino_prova_o_servidor: bool,
    /// The last decision, so the shell can report what happened.
    ///
    /// É também por onde o motivo de uma recusa chega a quem conectou: o
    /// `rustls` só leva texto adiante, e `client::classify_connection_error`
    /// lê a decisão daqui para devolver um erro com nome
    /// ([`PinDecision::InviteRefused`] vira `ConnectError::InviteMismatch`, e
    /// [`PinDecision::Changed`] vira `ConnectError::PinChanged`).
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
    ///
    /// `esperada` é a impressão que quem conecta espera encontrar — a do link
    /// colado agora, ou a que a lista de servidores guardou dele —, e `None`
    /// para um endereço digitado à mão, em que não há o que conferir.
    /// `Client::connect_por` a repassa do `Destino`; só `Client::connect`, o
    /// caminho público que nunca confere impressão, passa `None`.
    ///
    /// `o_pino_prova_o_servidor` diz se o pin de `pin_key` prova o servidor:
    /// `false` num candidato que entrou na corrida por outro caminho que não a
    /// escolha da pessoa (um alternativo do convite ou da lista, ou o endereço
    /// que o quarto devolveu) e num alvo de escopo local (um endereço de LAN,
    /// que é o mesmo de uma casa para outra). Vem do `Destino`, como a
    /// esperada, e quem o calcula é a FFI; `Client::connect`, que não confere
    /// impressão nenhuma, passa `true`, e sem esperada ele não pesa.
    #[must_use]
    pub fn new(
        store: Arc<dyn PinStore>,
        pin_key: String,
        esperada: Option<String>,
        o_pino_prova_o_servidor: bool,
    ) -> Self {
        Self {
            store,
            pin_key,
            esperada,
            o_pino_prova_o_servidor,
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
    ///
    /// # A impressão esperada, e onde ela **não** manda
    ///
    /// Sem pin, ela decide **antes de fixar**: a chave que não confere vira
    /// [`PinDecision::InviteRefused`] e não é fixada. É isso que permite ao TLS
    /// recusar antes do `Hello`.
    ///
    /// Com pin **onde ele prova o servidor**, ela não muda nada aqui. Um
    /// servidor já fixado cuja chave o link desmente continua `Matches`, e o
    /// veredito vira [`Verdict::InviteDisagrees`]: a conexão segue e avisa. É a
    /// decisão do ADR 0003 — o pin é a prova de continuidade, e quem discorda
    /// dele é o link —, e a tabela de [`verdict`] já a escrevia antes de a
    /// impressão chegar ao TLS. Recusar ali trancaria alguém para fora de um
    /// servidor que ele usa porque um amigo mandou um link velho.
    ///
    /// # Onde o pin não prova o servidor, ele não passa por cima dela
    ///
    /// O pin prova *este endereço*: este `IP:porta` já apresentou esta chave.
    /// Ele só prova **o servidor** no alvo (o endereço que a pessoa escolheu)
    /// cujo endereço é de escopo público, o mesmo em qualquer rede. Nos outros
    /// dois casos ele só diz que algum servidor já atendeu ali, e pode ser
    /// outro:
    ///
    /// - num candidato que ninguém escolheu — um alternativo do convite ou da
    ///   lista, ou o endereço que o quarto devolveu —, o quarto pode apontar
    ///   para um endereço que esta máquina fixou com a chave de quem ocupou a
    ///   marca, e um alternativo de LAN é o mesmo de uma casa para outra;
    /// - num alvo de escopo local — o endereço da rede de casa do anfitrião,
    ///   que é o primeiro de um link quando ele tem uma —, um servidor da rede
    ///   em que a pessoa está pode atender no mesmo `IP:porta`, já fixado. Ali
    ///   um pin que confere com a esperada discordando é, no caso comum,
    ///   colisão: um servidor que trocou de chave dá `Changed`, e não
    ///   `Matches`.
    ///
    /// Nos dois, a impressão prometida vale mais que o pin: a que não confere
    /// vira [`PinDecision::InviteRefused`], dentro do TLS e antes do `Hello`, e
    /// nada é fixado nem desfeito. Sem esperada, o pin decide como sempre. É o
    /// adendo de 2026-09-29 ao ADR 0003. Este verificador não decide onde o pin
    /// prova o servidor: recebe a resposta pronta
    /// (`crate::enlace::Destino::o_pino_prova_o_servidor`), calculada num lugar
    /// só.
    ///
    /// Uma chave **trocada** continua `Changed`, recusada com ou sem link, onde
    /// o pin prova o servidor e onde não prova.
    ///
    /// A comparação não diferencia maiúsculas de minúsculas, como a de
    /// [`verdict`]: é a mesma função, `confere`, nos dois lugares.
    pub fn decide(&self, host: &str, certificate: &[u8]) -> PinDecision {
        let offered = seele_proto::transport::certificate_fingerprint(certificate);
        match self.store.pinned(host) {
            None => {
                if let Some(expected) = self.desmente(&offered) {
                    return PinDecision::InviteRefused {
                        expected: expected.to_owned(),
                        offered,
                    };
                }
                self.store.pin(host, offered.clone());
                PinDecision::FirstContact {
                    fingerprint: offered,
                }
            }
            Some(pinned) if pinned == offered => {
                if !self.o_pino_prova_o_servidor {
                    if let Some(expected) = self.desmente(&offered) {
                        return PinDecision::InviteRefused {
                            expected: expected.to_owned(),
                            offered,
                        };
                    }
                }
                PinDecision::Matches {
                    fingerprint: offered,
                }
            }
            Some(pinned) => PinDecision::Changed { pinned, offered },
        }
    }

    /// A impressão esperada, quando há uma e ela não confere com a ofertada.
    fn desmente(&self, ofertada: &str) -> Option<&str> {
        self.esperada
            .as_deref()
            .filter(|esperada| !confere(esperada, ofertada))
    }
}

/// A frase que o TLS leva a quem atendeu quando o verificador recusa.
///
/// **É a única frase, para os dois casos, e não conta nada.** O texto do `Err`
/// de `verify_server_cert` vira o `reason` do `CONNECTION_CLOSE` que o servidor
/// (ou quem atendeu no lugar dele) recebe — `quinn-proto`, `crypto/rustls.rs`,
/// em `read_handshake`. Uma frase por caso ensinaria a quem atende se esta
/// máquina tinha um link na mão ou uma chave fixada dele, e essa é justamente a
/// informação que um impostor quer para saber o que forjar. O que distingue a
/// chave trocada da impressão que não confere fica em `last` e no log daqui,
/// que só quem usa esta máquina lê.
const RECUSA_NO_TLS: &str = "certificado não aceito";

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
            //
            // O aperto de mão falha **aqui** nos dois casos, e antes do `Hello`:
            // o convite, a senha e o apelido não saem para quem atendeu. O
            // porquê já está em `last`, gravado acima, para
            // `client::classify_connection_error` devolver com nome. A frase é
            // uma só, e neutra — ver [`RECUSA_NO_TLS`].
            PinDecision::Changed { .. } | PinDecision::InviteRefused { .. } => {
                Err(TlsError::General(RECUSA_NO_TLS.into()))
            }
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
        TofuVerifier::new(
            Arc::new(MemoryPinStore::new()),
            "seele.exemplo".to_owned(),
            None,
            true,
        )
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

    /// Um verificador que espera `esperada`, e a loja que ele usa, para o
    /// teste ler o que ficou fixado.
    fn esperando(esperada: &str) -> (Arc<MemoryPinStore>, TofuVerifier) {
        let loja = Arc::new(MemoryPinStore::new());
        let verificador = TofuVerifier::new(
            Arc::clone(&loja) as Arc<dyn PinStore>,
            "seele.exemplo".to_owned(),
            Some(esperada.to_owned()),
            true,
        );
        (loja, verificador)
    }

    #[test]
    fn um_primeiro_contato_que_a_esperada_confirma_e_fixado() {
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");
        let (loja, verificador) = esperando(&um);

        assert_eq!(
            verificador.decide("server.example", b"certificate-one"),
            PinDecision::FirstContact {
                fingerprint: um.clone()
            },
            "a impressão esperada conferia e o primeiro contato não seguiu"
        );
        assert_eq!(
            loja.pinned("server.example"),
            Some(um),
            "conferir não substitui fixar: o TOFU do ADR 0003 continua valendo"
        );
    }

    #[test]
    fn um_primeiro_contato_que_a_esperada_desmente_e_recusado_sem_fixar() {
        // O S2b da análise de 22/09. O verificador fixava toda chave no
        // primeiro contato, e a conferência contra o link só acontecia depois do
        // `Hello`, com o convite já entregue a quem atendeu.
        let (loja, verificador) = esperando(B);
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");

        assert_eq!(
            verificador.decide("server.example", b"certificate-one"),
            PinDecision::InviteRefused {
                expected: B.into(),
                offered: um
            },
            "o certificado que a impressão esperada desmente passou como primeiro contato"
        );
        assert_eq!(
            loja.pinned("server.example"),
            None,
            "a chave recusada ficou fixada, e a visita seguinte sem link entraria \
             calada no servidor que acabou de ser recusado"
        );
    }

    #[test]
    fn a_recusa_da_esperada_falha_o_aperto_de_mao_e_deixa_o_porque() {
        // `decide` sozinho não prova que o TLS para. Quem o `rustls` chama no
        // meio do aperto de mão é `verify_server_cert`, e um `Ok` dele manda o
        // `Hello` logo em seguida. Aqui ele é chamado direto, sem sessão TLS
        // nenhuma: só lê o certificado.
        let (_, verificador) = esperando(B);
        let certificado = CertificateDer::from(b"certificate-one".to_vec());
        let nome = ServerName::try_from("localhost").expect("nome TLS de teste");

        let resposta = verificador.verify_server_cert(
            &certificado,
            &[],
            &nome,
            &[],
            UnixTime::since_unix_epoch(std::time::Duration::ZERO),
        );

        assert!(
            resposta.is_err(),
            "o TLS aceitou o certificado que a impressão esperada recusa: o \
             `Hello` sairia com o convite dentro"
        );
        assert!(
            matches!(
                verificador.last_decision(),
                Some(PinDecision::InviteRefused { .. })
            ),
            "o TLS recusou sem deixar o porquê, e quem conecta só teria \
             `TlsRefused` genérico para mostrar"
        );
    }

    #[test]
    fn um_servidor_ja_fixado_com_link_velho_passa_no_tls_e_so_avisa() {
        // A decisão que este verificador **não** muda (ADR 0003). Com pin, quem
        // prova o servidor é o pin, e quem discorda dele é o link. Recusar aqui
        // trancaria alguém para fora de um servidor que ele usa porque um amigo
        // mandou um convite velho. A tabela de `verdict` já escreve essa regra
        // em `a_matching_pin_the_invite_contradicts_warns_and_does_not_refuse`.
        let loja = Arc::new(MemoryPinStore::new());
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");
        loja.pin("server.example", um.clone());
        let verificador = TofuVerifier::new(
            Arc::clone(&loja) as Arc<dyn PinStore>,
            "seele.exemplo".to_owned(),
            Some(B.to_owned()),
            true,
        );

        let decisao = verificador.decide("server.example", b"certificate-one");
        assert_eq!(
            decisao,
            PinDecision::Matches {
                fingerprint: um.clone()
            },
            "um servidor já fixado passou a ser recusado por causa do link"
        );
        assert_eq!(
            verdict(&decisao, Some(B)),
            Verdict::InviteDisagrees {
                expected: B.into(),
                offered: um
            },
            "o servidor já fixado que o link desmente deixou de só avisar"
        );
    }

    /// Um verificador com a loja já fixada em `fixada`, com um pino que prova o
    /// servidor ou não, e com a esperada que o teste quiser.
    fn candidato_fixado(
        o_pino_prova_o_servidor: bool,
        fixada: &str,
        esperada: Option<&str>,
    ) -> (Arc<MemoryPinStore>, TofuVerifier) {
        let loja = Arc::new(MemoryPinStore::new());
        loja.pin("server.example", fixada.to_owned());
        let verificador = TofuVerifier::new(
            Arc::clone(&loja) as Arc<dyn PinStore>,
            "seele.exemplo".to_owned(),
            esperada.map(str::to_owned),
            o_pino_prova_o_servidor,
        );
        (loja, verificador)
    }

    #[test]
    fn onde_o_pino_nao_prova_o_servidor_ele_nao_passa_por_cima_da_esperada() {
        // O pino é por endereço de candidato, e nem todo endereço é o servidor
        // que a pessoa pediu: entram na corrida a resposta do quarto e os
        // alternativos do convite e da lista, e o alvo de LAN é o mesmo de uma
        // casa para outra. Se esta máquina já fixou ali um servidor Y, o pino
        // confere com a chave de Y, e deixá-lo passar mandaria o `Hello` (o
        // convite, o apelido e a assinatura) a um servidor que a impressão
        // prometida desmente.
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");
        let (loja, verificador) = candidato_fixado(false, &um, Some(B));

        assert_eq!(
            verificador.decide("server.example", b"certificate-one"),
            PinDecision::InviteRefused {
                expected: B.into(),
                offered: um.clone()
            },
            "um endereço cujo pino não prova o servidor passou pelo pino de outro servidor, \
             e o `Hello` sairia para ele com o convite, o apelido e a assinatura"
        );
        assert_eq!(
            loja.pinned("server.example"),
            Some(um),
            "a recusa onde o pino não prova o servidor mexeu no pino daquele endereço: ele \
             continua sendo a prova de quem atendeu ali, e a recusa não fixa nem desfaz nada"
        );
    }

    #[test]
    fn onde_o_pino_prova_o_servidor_o_que_confere_continua_passando_e_so_avisa() {
        // A metade que a recusa de cima não pode levar junto (ADR 0003). No
        // alvo de escopo público, o pino é a prova de continuidade, e quem
        // discorda dele é o link.
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");
        let (_, verificador) = candidato_fixado(true, &um, Some(B));

        assert_eq!(
            verificador.decide("server.example", b"certificate-one"),
            PinDecision::Matches { fingerprint: um },
            "o alvo já fixado, onde o pino prova o servidor, passou a ser recusado pela \
             esperada: um link velho trancaria a pessoa para fora do servidor que ela usa, \
             contra o ADR 0003"
        );
    }

    #[test]
    fn onde_o_pino_nao_prova_o_servidor_ele_passa_quando_nada_o_desmente() {
        // Onde o pino não prova o servidor, quem recusa é a esperada que não
        // confere, e não a falta dela nem o próprio fato de o pino não provar.
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");

        let (_, sem_esperada) = candidato_fixado(false, &um, None);
        assert_eq!(
            sem_esperada.decide("server.example", b"certificate-one"),
            PinDecision::Matches {
                fingerprint: um.clone()
            },
            "um endereço fixado, sem impressão a conferir, deixou de ser reconhecido só \
             porque o pino dele não prova o servidor"
        );

        let (_, que_confere) = candidato_fixado(false, &um, Some(&um.to_uppercase()));
        assert_eq!(
            que_confere.decide("server.example", b"certificate-one"),
            PinDecision::Matches { fingerprint: um },
            "um endereço fixado com a chave que a impressão promete foi recusado onde o pino \
             não prova o servidor: a conferência deixou de ser a mesma `confere` do veredito"
        );
    }

    #[test]
    fn a_esperada_ignora_caixa_como_o_veredito_ignora() {
        // `verdict` compara sem diferenciar maiúsculas de minúsculas. Se o
        // verificador diferenciasse, um link com a impressão em maiúsculas seria
        // recusado no TLS e aceito no veredito: duas regras para a mesma
        // pergunta.
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");
        let (_, verificador) = esperando(&um.to_uppercase());

        assert!(
            matches!(
                verificador.decide("server.example", b"certificate-one"),
                PinDecision::FirstContact { .. }
            ),
            "a mesma impressão em maiúsculas foi recusada no TLS"
        );
    }

    #[test]
    fn uma_esperada_vazia_nao_confere_com_chave_nenhuma() {
        // A comparação é de igualdade, e não «a esperada é um começo da
        // ofertada»: um link com o campo vazio (`?fp=`) que chegasse até aqui
        // como `Some("")` seria prefixo de qualquer impressão, e recusaria
        // ninguém.
        let (loja, verificador) = esperando("");
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");

        assert_eq!(
            verificador.decide("server.example", b"certificate-one"),
            PinDecision::InviteRefused {
                expected: String::new(),
                offered: um
            },
            "uma impressão esperada vazia passou como se conferisse"
        );
        assert_eq!(
            loja.pinned("server.example"),
            None,
            "a chave que uma esperada vazia deveria ter recusado ficou fixada"
        );
    }

    #[test]
    fn uma_esperada_truncada_em_63_digitos_nao_confere() {
        // O mesmo pela outra ponta: uma impressão cortada um dígito antes do
        // fim é parecida demais com a verdadeira para o olho, e é justamente o
        // que uma comparação por prefixo aceitaria.
        let um = seele_proto::transport::certificate_fingerprint(b"certificate-one");
        let truncada = um
            .get(..63)
            .expect("uma impressão SHA-256 tem 64 dígitos hexadecimais");
        let (loja, verificador) = esperando(truncada);

        assert_eq!(
            verificador.decide("server.example", b"certificate-one"),
            PinDecision::InviteRefused {
                expected: truncada.to_owned(),
                offered: um
            },
            "uma impressão esperada com 63 dos 64 dígitos passou como se conferisse"
        );
        assert_eq!(
            loja.pinned("server.example"),
            None,
            "a chave que a esperada truncada deveria ter recusado ficou fixada"
        );
    }

    /// O que o `rustls` leva ao `CONNECTION_CLOSE` de quem atendeu, quando o
    /// verificador recusa este certificado.
    fn a_frase_que_vai_ao_fio(verificador: &TofuVerifier) -> String {
        let certificado = CertificateDer::from(b"certificate-one".to_vec());
        let nome = ServerName::try_from("localhost").expect("nome TLS de teste");
        verificador
            .verify_server_cert(
                &certificado,
                &[],
                &nome,
                &[],
                UnixTime::since_unix_epoch(std::time::Duration::ZERO),
            )
            .expect_err("o certificado devia ter sido recusado")
            .to_string()
    }

    #[test]
    fn a_frase_que_vai_ao_fio_nao_conta_se_havia_link_ou_pino() {
        // O texto do `Err` de `verify_server_cert` vira o `reason` do
        // `CONNECTION_CLOSE` que **quem atendeu** recebe (`quinn-proto`,
        // `crypto/rustls.rs`). Se as duas recusas dissessem coisas diferentes,
        // um servidor qualquer (ou um impostor) aprenderia, só de ser recusado,
        // se esta máquina tinha um link na mão ou uma chave fixada dele. A
        // distinção fica na decisão que o verificador guarda e no log daqui.
        let (_, com_link) = esperando(B);
        let por_link = a_frase_que_vai_ao_fio(&com_link);

        let loja = Arc::new(MemoryPinStore::new());
        loja.pin("seele.exemplo", A.into());
        let com_pin = TofuVerifier::new(
            Arc::clone(&loja) as Arc<dyn PinStore>,
            "seele.exemplo".to_owned(),
            None,
            true,
        );
        let por_pin = a_frase_que_vai_ao_fio(&com_pin);
        assert!(
            matches!(com_pin.last_decision(), Some(PinDecision::Changed { .. })),
            "o cenário do pin trocado não foi o que o teste montou"
        );

        assert_eq!(
            por_link, por_pin,
            "a recusa por link e a recusa por pin trocado dizem coisas diferentes a \
             quem atendeu, e ele aprende qual das duas defesas esta máquina tinha"
        );
        let minuscula = por_link.to_lowercase();
        // Sem «expect»: o `rustls` antepõe «unexpected error: » à frase.
        for palavra in [
            "pin", "fix", "link", "convite", "impress", "esperad", "invite", "chang", "mudou",
            "troc", "promete",
        ] {
            assert!(
                !minuscula.contains(palavra),
                "a frase que vai ao fio diz «{palavra}» e conta a quem atendeu \
                 como esta máquina decidiu: {por_link}"
            );
        }
    }

    #[test]
    fn a_recusa_no_tls_tem_o_veredito_da_recusa_de_depois() {
        let decisao = PinDecision::InviteRefused {
            expected: B.into(),
            offered: A.into(),
        };
        assert_eq!(
            verdict(&decisao, Some(B)),
            Verdict::InviteRefused {
                expected: B.into(),
                offered: A.into()
            },
            "a recusa do TLS deixou de virar a recusa do veredito: quem lê o \
             veredito trataria como conhecido um servidor que o link desmente"
        );
    }
}
