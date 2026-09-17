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
        if let Some((cert, key)) = identidade_guardada(persistence) {
            return Ok(Self {
                chain: vec![CertificateDer::from(cert)],
                key: PrivateKeyDer::try_from(key)
                    .map_err(|erro| anyhow::anyhow!("a chave guardada não é uma chave: {erro}"))?,
            });
        }

        let identidade = Self::self_signed(subject_alt_names)?;
        let cert = identidade
            .chain
            .first()
            .map(|c| c.as_ref().to_vec())
            .unwrap_or_default();
        let key = identidade.key.secret_der().to_vec();

        guardar_identidade(persistence, &cert, &key)?;

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

/// O par guardado neste banco, quando há um e os dois lados estão cheios.
#[must_use]
pub fn identidade_guardada(
    persistence: &crate::persistence::Persistence,
) -> Option<(Vec<u8>, Vec<u8>)> {
    let (cert, key): (Vec<u8>, Vec<u8>) = persistence
        .connection()
        .query_row(
            "SELECT
               (SELECT valor FROM configuracao WHERE chave = 'tls_cert'),
               (SELECT valor FROM configuracao WHERE chave = 'tls_key')",
            [],
            |linha| Ok((linha.get(0)?, linha.get(1)?)),
        )
        .ok()?;
    (!cert.is_empty() && !key.is_empty()).then_some((cert, key))
}

/// Grava o par neste banco, substituindo o que houver.
///
/// # Errors
///
/// Falha se o banco não aceitar a escrita.
pub fn guardar_identidade(
    persistence: &crate::persistence::Persistence,
    cert: &[u8],
    key: &[u8],
) -> Result<()> {
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
    Ok(())
}

/// Faz `para` apresentar a mesma chave que `de`, se ele ainda não tem uma.
///
/// **O defeito que isto existe para impedir é o mesmo de sempre, numa forma
/// nova.** O comentário de `load_or_create` já conta a primeira: reiniciar o
/// `seeled` trocava a chave, e todo cliente via «A CHAVE DO SERVIDOR MUDOU» —
/// o alerta bloqueante do ADR 0003 — num reinício de rotina.
///
/// A segunda veio com os servidores guardados. Cada um ganhou banco próprio, e
/// com ele uma identidade própria; a chave do pino do TOFU, porém, é **o texto
/// do alvo** — o mesmo endereço para todos eles. Sair de um servidor desta
/// máquina e entrar em outro passou a disparar o aviso reservado para ataque.
///
/// A resposta não é afrouxar o TOFU: é reconhecer o que o pino sempre quis
/// dizer. Ele diz «a máquina neste endereço», e dois servidores guardados aqui
/// **são** a mesma máquina e o mesmo endereço. Uma chave por máquina é a
/// verdade que o pino já afirmava.
///
/// **Sobrescreve, e esta é a segunda metade da lição.** A primeira versão
/// recusava sobrescrever, com o argumento de que um banco com identidade já tem
/// clientes que a fixaram. O argumento é bom em geral e **não vale aqui**: o
/// pino é por endereço, e dois servidores guardados nesta máquina dividem o
/// mesmo. É impossível alguém ter fixado os dois — quem fixou o segundo já
/// tinha perdido o pino do primeiro.
///
/// A recusa deixava sem conserto justamente as máquinas onde o defeito já
/// aconteceu: um servidor criado antes desta correção fica com a chave própria
/// para sempre, e continua disparando o alerta a cada troca. Medido na máquina
/// de quem relatou — dois bancos, duas chaves, um endereço.
///
/// A chave de `de` é a do banco de sempre, e é a que as pessoas têm fixada.
/// Fazer todo servidor desta máquina falar com essa voz é o que zera os
/// alertas, não o que os causa.
///
/// Devolve se mudou alguma coisa.
///
/// # Errors
///
/// Falha se a escrita no destino falhar.
pub fn herdar_identidade(
    de: &crate::persistence::Persistence,
    para: &crate::persistence::Persistence,
) -> Result<bool> {
    let Some((cert, key)) = identidade_guardada(de) else {
        return Ok(false);
    };
    if identidade_guardada(para).as_ref() == Some(&(cert.clone(), key.clone())) {
        return Ok(false);
    }
    guardar_identidade(para, &cert, &key)?;
    Ok(true)
}

#[cfg(test)]
mod uma_chave_por_maquina {
    use super::{herdar_identidade, identidade_guardada, Identity};
    use crate::persistence::{Location, Persistence};

    fn banco() -> Persistence {
        Persistence::open(&Location::Memory).expect("um banco em memória")
    }

    fn com_identidade() -> Persistence {
        let p = banco();
        Identity::load_or_create(&p, vec!["localhost".to_owned()]).expect("gerar identidade");
        p
    }

    /// **O defeito relatado, na forma de teste.**
    ///
    /// Sair de um servidor desta máquina e entrar em outro dava «A CHAVE DO
    /// SERVIDOR MUDOU», porque o pino do TOFU é o endereço e cada servidor
    /// guardado tinha a própria chave. Herdar é o que faz os dois apresentarem
    /// a mesma — e a mesma quer dizer, literalmente, os mesmos bytes.
    #[test]
    fn um_servidor_novo_apresenta_a_chave_que_esta_maquina_ja_apresentava() {
        let velho = com_identidade();
        let novo = banco();

        assert!(herdar_identidade(&velho, &novo).expect("herdar"));

        assert_eq!(
            identidade_guardada(&novo),
            identidade_guardada(&velho),
            "dois servidores no mesmo endereço, a mesma chave"
        );
    }

    /// **O caso que o primeiro conserto deixou de fora, e era o do relato.**
    ///
    /// Um servidor criado antes da correção já tem chave própria. A primeira
    /// versão recusava sobrescrever — e com isso deixava sem conserto
    /// justamente a máquina onde o defeito já tinha acontecido: dois bancos,
    /// duas chaves, um endereço, e o alerta a cada troca, para sempre.
    #[test]
    fn um_servidor_que_ja_tem_chave_propria_passa_a_usar_a_da_maquina() {
        let velho = com_identidade();
        let outro = com_identidade();
        assert_ne!(
            identidade_guardada(&outro),
            identidade_guardada(&velho),
            "antes, cada um com a sua"
        );

        assert!(herdar_identidade(&velho, &outro).expect("herdar"));

        assert_eq!(
            identidade_guardada(&outro),
            identidade_guardada(&velho),
            "depois, a máquina fala com uma voz só"
        );
    }

    /// E herdar duas vezes não é uma segunda escrita: sem isto, toda vez que
    /// alguém pedisse o banco haveria uma gravação no disco sem nada mudar.
    #[test]
    fn herdar_de_novo_nao_muda_nada_e_diz_que_nao_mudou() {
        let velho = com_identidade();
        let outro = banco();
        assert!(herdar_identidade(&velho, &outro).expect("primeira"));
        assert!(!herdar_identidade(&velho, &outro).expect("segunda"));
    }

    #[test]
    fn sem_nada_de_onde_herdar_o_destino_segue_vazio() {
        let vazio = banco();
        let novo = banco();

        assert!(!herdar_identidade(&vazio, &novo).expect("herdar"));
        assert!(identidade_guardada(&novo).is_none());
    }

    /// E o que herdou continua servindo para subir um servidor: `load_or_create`
    /// tem de **encontrar** o que foi plantado, e não gerar por cima.
    #[test]
    fn a_chave_herdada_e_a_que_o_servidor_usa_ao_subir() {
        let velho = com_identidade();
        let novo = banco();
        herdar_identidade(&velho, &novo).expect("herdar");

        let identidade =
            Identity::load_or_create(&novo, vec!["localhost".to_owned()]).expect("carregar");

        let (cert, _) = identidade_guardada(&velho).expect("o velho tem identidade");
        assert_eq!(
            identidade.chain.first().map(|c| c.as_ref().to_vec()),
            Some(cert),
            "subir usa a chave herdada, e não uma recém-gerada"
        );
    }
}
