//! Certificate handling for the daemon.
//!
//! ADR 0003 makes TOFU the default: the server presents a self-signed
//! certificate, the client memorises the public key on first contact and shouts
//! if it ever changes. `specs/08-seguranca.md` gives the reasoning — it is the
//! SSH model, the audience already understands it, and ACME would demand a
//! domain plus ports 80/443, which contradicts the single-UDP-port simplicity of
//! `specs/01-arquitetura.md`.
//!
//! There is no plaintext path and no flag to disable TLS. `specs/08-seguranca.md`
//! is categorical about that, so this module has no "insecure" branch to audit.

use std::sync::Arc;

use anyhow::{Context, Result};
use quinn::crypto::rustls::QuicServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// A certificate and the key that signs for it.
pub struct Identity {
    /// DER-encoded certificate chain.
    pub chain: Vec<CertificateDer<'static>>,
    /// DER-encoded private key.
    pub key: PrivateKeyDer<'static>,
}

impl Identity {
    /// Generates a fresh self-signed identity.
    ///
    /// `subject_alt_names` should include every name and address clients will
    /// use to reach this server. With TOFU the names matter less than the key —
    /// a client pins the key, not the name — but a certificate with no matching
    /// name still fails before pinning ever happens.
    ///
    /// # Errors
    ///
    /// Fails if key generation or certificate signing fails.
    pub fn self_signed(subject_alt_names: Vec<String>) -> Result<Self> {
        let generated = rcgen::generate_simple_self_signed(subject_alt_names)
            .context("could not generate a self-signed certificate")?;
        let key = PrivateKeyDer::try_from(generated.signing_key.serialize_der())
            .map_err(|error| anyhow::anyhow!("could not encode the private key: {error}"))?;
        Ok(Self {
            chain: vec![generated.cert.der().clone()],
            key,
        })
    }

    /// Lê a identidade guardada no banco, ou gera e guarda uma.
    ///
    /// **Sem isto, reiniciar o `seeled` trocava a chave do servidor.** Todo
    /// cliente que já tinha se conectado via `A CHAVE DO SERVIDOR MUDOU` — o
    /// alerta bloqueante do ADR 0003 — e era recusado. Um reinício de rotina
    /// disparando o aviso reservado para ataque é pior que não ter o aviso:
    /// ensina a ignorá-lo.
    ///
    /// A chave privada fica no mesmo banco que o resto. Quem consegue lê-lo já
    /// tem as mensagens todas; o que se protege é o arquivo, não uma camada a
    /// mais dentro dele — e é por isso que o PERSISTENCE cria o banco com permissão
    /// restrita ao dono.
    ///
    /// # Errors
    ///
    /// Falha se o banco não responder ou se o que está guardado não for uma
    /// identidade válida.
    pub fn load_or_create(
        persistence: &crate::persistence::Persistence,
        subject_alt_names: Vec<String>,
    ) -> Result<Self> {
        let guardada: Option<(Vec<u8>, Vec<u8>)> = persistence
            .connection()
            .query_row(
                "SELECT
                   (SELECT valor FROM configuracao WHERE chave = 'tls_cert'),
                   (SELECT valor FROM configuracao WHERE chave = 'tls_key')",
                [],
                |linha| Ok((linha.get(0)?, linha.get(1)?)),
            )
            .ok();

        if let Some((cert, key)) = guardada {
            if !cert.is_empty() && !key.is_empty() {
                return Ok(Self {
                    chain: vec![CertificateDer::from(cert)],
                    key: PrivateKeyDer::try_from(key).map_err(|erro| {
                        anyhow::anyhow!("a chave guardada não é uma chave: {erro}")
                    })?,
                });
            }
        }

        let identidade = Self::self_signed(subject_alt_names)?;
        let cert = identidade
            .chain
            .first()
            .map(|c| c.as_ref().to_vec())
            .unwrap_or_default();
        let key = identidade.key.secret_der().to_vec();

        let conexao = persistence.connection();
        conexao.execute(
            "INSERT INTO configuracao (chave, valor) VALUES ('tls_cert', ?1)
             ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
            rusqlite::params![cert],
        )?;
        conexao.execute(
            "INSERT INTO configuracao (chave, valor) VALUES ('tls_key', ?1)
             ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
            rusqlite::params![key],
        )?;

        Ok(identidade)
    }

    /// The fingerprint a client pins, as lowercase hex of the SHA-256 of the
    /// certificate.
    ///
    /// `specs/08-seguranca.md` requires the key-change warning to be
    /// "impossible to ignore — literally a blocking `Alerta · 警告`". This is the
    /// value both ends compare, and the one an operator reads out over another
    /// channel when a person asks whether the change was real.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        self.chain.first().map_or_else(String::new, |certificate| {
            seele_proto::transport::certificate_fingerprint(certificate.as_ref())
        })
    }
}

/// Builds the QUIC server configuration.
///
/// # Errors
///
/// Fails if rustls rejects the certificate or key.
pub fn server_config(identity: Identity) -> Result<quinn::ServerConfig> {
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(identity.chain, identity.key)
        .context("rustls rejected the certificate")?;
    // Refuse a mismatched peer during the TLS handshake, before a single
    // application byte is exchanged.
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];

    let quic = QuicServerConfig::try_from(tls).context("could not build the QUIC TLS config")?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(quic));

    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        seele_proto::transport::IDLE_TIMEOUT
            .try_into()
            .context("idle timeout out of range")?,
    ));
    transport.keep_alive_interval(Some(seele_proto::transport::KEEPALIVE));
    // specs/02-protocolo.md puts voice on datagrams so a history fetch cannot
    // block a presence event. Without this, quinn negotiates them off.
    transport.datagram_receive_buffer_size(Some(1024 * 1024));
    config.transport_config(Arc::new(transport));

    Ok(config)
}
