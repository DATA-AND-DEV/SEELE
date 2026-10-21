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

/// O nome que o certificado da sala pessoal leva no SAN, e o SNI de quem liga.
///
/// Mora no `seele-proto`, porque quem liga também o usa, e o `seele-core` não
/// pode importar daqui (ADR 0002). Ele entra no DER da sala, e o vetor de
/// `tests/identidade_da_sala.rs` o prende.
pub use seele_proto::transport::NOME_DA_SALA;

/// O PKCS#8 v1 de uma chave Ed25519, sem a chave pública: estes 16 bytes e a
/// semente (RFC 8410, §7).
///
/// À mão porque não há crate `pkcs8` na árvore, e o formato da `identity.key`
/// (32 bytes crus) não muda: o ADR 0017 marca como alto o custo de mudá-lo.
const PREFIXO_PKCS8_ED25519: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

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

    /// A identidade da sala pessoal, feita da semente da `identity.key`.
    ///
    /// **Determinística, byte a byte**, e é por isso que ela existe em vez de
    /// [`Self::self_signed`]. O nome, o número de série e a validade são fixos,
    /// e a assinatura Ed25519 é determinística. Então o DER sai igual a cada
    /// abertura, e o dono nunca lê «A CHAVE MUDOU» ao entrar na própria sala.
    /// O DER é formato congelado: ver o vetor de `tests/identidade_da_sala.rs`.
    ///
    /// A validade vai até 31/12/9999, a convenção do RFC 5280 (§4.1.2.5) para
    /// «sem data de expiração»: quem confere a sala é o código, e não a data.
    ///
    /// A semente fica só aqui, dentro do processo. Ela nunca passa pela webview
    /// e nunca vai para o banco: ver [`crate::ServerConfig::identidade`].
    ///
    /// # Errors
    ///
    /// Falha se o `rcgen` recusar a chave ou o certificado. Com 32 bytes
    /// quaisquer, isso não acontece.
    pub fn da_semente(semente: &[u8; 32]) -> Result<Self> {
        let mut pkcs8 = Vec::with_capacity(PREFIXO_PKCS8_ED25519.len() + semente.len());
        pkcs8.extend_from_slice(&PREFIXO_PKCS8_ED25519);
        pkcs8.extend_from_slice(semente);
        let der = rustls::pki_types::PrivatePkcs8KeyDer::from(pkcs8);

        let par = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&der, &rcgen::PKCS_ED25519)
            .context("o rcgen recusou a chave da sala")?;
        let mut params = rcgen::CertificateParams::new(vec![NOME_DA_SALA.to_owned()])
            .context("o nome da sala não serve de nome de certificado")?;
        params.serial_number = Some(rcgen::SerialNumber::from_slice(&[1]));
        params.not_before = rcgen::date_time_ymd(2026, 9, 23);
        params.not_after = rcgen::date_time_ymd(9999, 12, 31);
        let mut nome = rcgen::DistinguishedName::new();
        nome.push(rcgen::DnType::CommonName, NOME_DA_SALA);
        params.distinguished_name = nome;
        let certificado = params
            .self_signed(&par)
            .context("o rcgen não assinou o certificado da sala")?;

        Ok(Self {
            chain: vec![certificado.der().clone()],
            key: PrivateKeyDer::Pkcs8(der),
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

    /// A mesma impressão de [`Self::fingerprint`], em bytes: é o que a prova
    /// de identidade do cliente amarra (o S2).
    ///
    /// Sem certificado nenhum, zeros — e então nenhuma prova confere, que é a
    /// falha do lado seguro. Uma `Identity` sempre nasce com um.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        self.chain.first().map_or([0; 32], |certificate| {
            seele_proto::transport::certificate_digest(certificate.as_ref())
        })
    }
}

impl Clone for Identity {
    fn clone(&self) -> Self {
        Self {
            chain: self.chain.clone(),
            key: self.key.clone_key(),
        }
    }
}

/// A impressão, e nunca a chave: uma identidade aparece em toda
/// [`crate::ServerConfig`] que alguém imprime para depurar.
impl std::fmt::Debug for Identity {
    fn fmt(&self, formatador: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatador
            .debug_struct("Identity")
            .field("impressao", &self.fingerprint())
            .field("key", &"<escondida>")
            .finish()
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
    com_o_transporte(tls)
}

/// O TLS dentro do QUIC, com o transporte do `seeled`: o ocioso, o keepalive e
/// os datagramas da voz. Um lugar só, para o `seele/1` e o `h3` não divergirem.
fn com_o_transporte(tls: rustls::ServerConfig) -> Result<quinn::ServerConfig> {
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

// ------------------------------------------------- o `h3` do navegador
//
// ADR 0057 (emenda ao 0003): o `seeled` anuncia `seele/1` e `h3` no mesmo
// socket, e escolhe o certificado pelo ALPN do ClientHello. O nativo recebe o
// longo, como sempre. O navegador recebe um **curto**: ele só aceita, pelo
// hash (`serverCertificateHashes`), um certificado de no máximo 14 dias.
//
// A medição 3 no iPhone (`docs/evidencias/medicao-3-iphone/LEIA.md`, 18 e
// 20/10/2026) decidiu o caminho simples: **o curto tem a chave do longo**. O
// Safari abriu o curto da mesma chave, e recusou só o longo.

/// O começo da janela de índice 0: 01/01/2026, 00:00 UTC. As janelas andam de
/// [`seele_proto::transport::PASSO_DA_JANELA`] em diante a partir daqui, e por
/// isso são as mesmas em toda abertura do servidor.
pub const EPOCA_DAS_JANELAS: u64 = 1_767_225_600;

/// O nome que o certificado curto leva. O navegador confere o certificado pelo
/// hash, e não pelo nome: o curto não nomeia endereço nenhum.
const NOME_DO_CURTO: &str = "seele.invalid";

/// Um certificado curto do `h3`, com a validade dele em segundos Unix UTC.
#[derive(Clone)]
pub struct Curto {
    /// O DER, assinado pela chave do longo.
    pub der: CertificateDer<'static>,
    /// Desde quando ele vale: o começo da janela, menos
    /// [`seele_proto::transport::ANTES_DA_JANELA`].
    pub nao_antes: u64,
    /// Até quando: [`seele_proto::transport::VALIDADE_DO_CURTO`] depois.
    pub nao_depois: u64,
}

impl Curto {
    /// O que o navegador confere: o SHA-256 do DER.
    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        seele_proto::transport::certificate_digest(self.der.as_ref())
    }

    /// A janela como o `HashesDoNavegador` a leva.
    #[must_use]
    pub fn janela(&self) -> seele_proto::control::JanelaDoNavegador {
        seele_proto::control::JanelaDoNavegador {
            hash: self.hash(),
            nao_antes: self.nao_antes,
            nao_depois: self.nao_depois,
        }
    }
}

impl std::fmt::Debug for Curto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Curto")
            .field("nao_antes", &self.nao_antes)
            .field("nao_depois", &self.nao_depois)
            .finish_non_exhaustive()
    }
}

/// O índice da janela que começou por último até `agora`.
#[must_use]
pub fn janela_de(agora: u64) -> u64 {
    agora.saturating_sub(EPOCA_DAS_JANELAS) / seele_proto::transport::PASSO_DA_JANELA.as_secs()
}

/// O dia civil (ano, mês, dia) de um instante Unix UTC, pelo algoritmo de
/// Howard Hinnant (`civil_from_days`). As janelas começam à meia-noite, e o
/// `rcgen` monta a data por dia.
fn dia_civil(segundos: u64) -> (i32, u8, u8) {
    let dias = i64::try_from(segundos / 86_400).unwrap_or(i64::MAX) + 719_468;
    let era = dias.div_euclid(146_097);
    let do_ciclo = dias.rem_euclid(146_097);
    let do_ano = (do_ciclo - do_ciclo / 1_460 + do_ciclo / 36_524 - do_ciclo / 146_096) / 365;
    let ano = do_ano + era * 400;
    let dia_do_ano = do_ciclo - (365 * do_ano + do_ano / 4 - do_ano / 100);
    let mp = (5 * dia_do_ano + 2) / 153;
    let dia = dia_do_ano - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = if mes <= 2 { ano + 1 } else { ano };
    (
        i32::try_from(ano).unwrap_or(i32::MAX),
        u8::try_from(mes).unwrap_or(1),
        u8::try_from(dia).unwrap_or(1),
    )
}

/// Os curtos das janelas `de` em diante, `quantas`, assinados pela chave de
/// `identity`.
///
/// **Vazio quando a chave não é P-256**: o navegador só aceita ECDSA P-256 pelo
/// hash, e a sala pessoal, com a chave Ed25519 dela, não serve ao navegador (o
/// celular não liga pelo código na 0.16; ADR 0057, emenda de 09/10/2026).
///
/// # Errors
///
/// Falha se o `rcgen` recusar os parâmetros ou não assinar.
pub fn curtos_de(identity: &Identity, de: u64, quantas: usize) -> Result<Vec<Curto>> {
    let Ok(par) = rcgen::KeyPair::try_from(identity.key.secret_der()) else {
        return Ok(Vec::new());
    };
    if par.algorithm() != &rcgen::PKCS_ECDSA_P256_SHA256 {
        return Ok(Vec::new());
    }
    let passo = seele_proto::transport::PASSO_DA_JANELA.as_secs();
    let antes = seele_proto::transport::ANTES_DA_JANELA.as_secs();
    let validade = seele_proto::transport::VALIDADE_DO_CURTO.as_secs();
    let mut curtos = Vec::with_capacity(quantas);
    for indice in de..de.saturating_add(u64::try_from(quantas).unwrap_or(u64::MAX)) {
        let nao_antes = EPOCA_DAS_JANELAS + indice * passo - antes;
        let nao_depois = nao_antes + validade;
        let mut params = rcgen::CertificateParams::new(vec![NOME_DO_CURTO.to_owned()])
            .context("o nome do curto não serve de nome de certificado")?;
        let (ano, mes, dia) = dia_civil(nao_antes);
        params.not_before = rcgen::date_time_ymd(ano, mes, dia);
        let (ano, mes, dia) = dia_civil(nao_depois);
        params.not_after = rcgen::date_time_ymd(ano, mes, dia);
        let certificado = params
            .self_signed(&par)
            .context("o rcgen não assinou o certificado curto")?;
        curtos.push(Curto {
            der: certificado.der().clone(),
            nao_antes,
            nao_depois,
        });
    }
    Ok(curtos)
}

/// A chave da tabela `configuracao` onde os curtos ficam, ao lado de
/// `tls_cert` e `tls_key`.
const CHAVE_DOS_CURTOS: &str = "tls_curtos";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn de_hex(texto: &str) -> Option<Vec<u8>> {
    texto
        .len()
        .is_multiple_of(2)
        .then(|| {
            (0..texto.len())
                .step_by(2)
                .map(|i| {
                    texto
                        .get(i..i + 2)
                        .and_then(|par| u8::from_str_radix(par, 16).ok())
                })
                .collect()
        })
        .flatten()
}

/// Os curtos guardados neste banco, se são da identidade de impressão
/// `longo`. Os de outra identidade não servem: a chave deles é outra.
fn curtos_guardados(persistence: &crate::persistence::Persistence, longo: &str) -> Vec<Curto> {
    let Ok(texto) = persistence.connection().query_row(
        "SELECT valor FROM configuracao WHERE chave = ?1",
        rusqlite::params![CHAVE_DOS_CURTOS],
        |linha| linha.get::<_, String>(0),
    ) else {
        return Vec::new();
    };
    let Ok(guardado) = serde_json::from_str::<serde_json::Value>(&texto) else {
        return Vec::new();
    };
    if guardado.get("longo").and_then(serde_json::Value::as_str) != Some(longo) {
        return Vec::new();
    }
    guardado
        .get("curtos")
        .and_then(serde_json::Value::as_array)
        .map(|lista| {
            lista
                .iter()
                .filter_map(|curto| {
                    Some(Curto {
                        der: CertificateDer::from(de_hex(curto.get("der")?.as_str()?)?),
                        nao_antes: curto.get("nao_antes")?.as_u64()?,
                        nao_depois: curto.get("nao_depois")?.as_u64()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn guardar_curtos(
    persistence: &crate::persistence::Persistence,
    longo: &str,
    curtos: &[Curto],
) -> Result<()> {
    let lista: Vec<serde_json::Value> = curtos
        .iter()
        .map(|curto| {
            serde_json::json!({
                "nao_antes": curto.nao_antes,
                "nao_depois": curto.nao_depois,
                "der": hex(curto.der.as_ref()),
            })
        })
        .collect();
    let texto = serde_json::json!({ "longo": longo, "curtos": lista }).to_string();
    persistence.connection().execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
         ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
        rusqlite::params![CHAVE_DOS_CURTOS, texto],
    )?;
    Ok(())
}

/// **Os curtos deste servidor, guardados no banco** (Tarefa 2.5 da F4): da
/// janela de agora até as [`seele_proto::transport::JANELAS_ANUNCIADAS`]
/// seguintes.
///
/// **Um curto anunciado nunca é regenerado.** O ECDSA do `ring` assina com um
/// número aleatório, e o mesmo certificado assinado de novo sai com outro DER e
/// outro hash: sem o banco, reabrir o `seeled` trocava todos os hashes, e todo
/// link já mandado deixava de abrir. Os guardados que ainda valem ficam como
/// estão; só as janelas que faltam são geradas, e os vencidos saem.
///
/// Guardados com a impressão do longo: uma identidade nova descarta os curtos
/// da antiga. Vazio, e nada guardado, para uma chave que não é P-256.
///
/// # Errors
///
/// Falha se o `rcgen` não assinar ou o banco não aceitar a escrita.
pub fn curtos_do_banco(
    persistence: &crate::persistence::Persistence,
    identity: &Identity,
    agora: u64,
) -> Result<Vec<Curto>> {
    let longo = identity.fingerprint();
    let de_agora = janela_de(agora);
    let antes = seele_proto::transport::ANTES_DA_JANELA.as_secs();
    let guardados = curtos_guardados(persistence, &longo);
    let mut curtos: Vec<Curto> = guardados
        .iter()
        .filter(|curto| janela_de(curto.nao_antes + antes) >= de_agora && agora < curto.nao_depois)
        .cloned()
        .collect();
    let quantas = u64::try_from(seele_proto::transport::JANELAS_ANUNCIADAS).unwrap_or(u64::MAX);
    for indice in de_agora..de_agora.saturating_add(quantas) {
        if curtos
            .iter()
            .any(|curto| janela_de(curto.nao_antes + antes) == indice)
        {
            continue;
        }
        let novo = curtos_de(identity, indice, 1)?;
        if novo.is_empty() {
            return Ok(Vec::new());
        }
        curtos.extend(novo);
    }
    curtos.sort_by_key(|curto| curto.nao_antes);
    let mudou = curtos.len() != guardados.len()
        || curtos
            .iter()
            .zip(&guardados)
            .any(|(agora, antes)| agora.der != antes.der);
    if mudou {
        guardar_curtos(persistence, &longo, &curtos)?;
    }
    Ok(curtos)
}

/// O certificado de cada ALPN: o longo para `seele/1`, o curto vigente para
/// `h3`. O molde é o de `spikes/alpn-no-mesmo-socket`.
#[derive(Debug)]
struct PorAlpn {
    longo: Arc<rustls::sign::CertifiedKey>,
    curtos: Vec<(Curto, Arc<rustls::sign::CertifiedKey>)>,
}

impl PorAlpn {
    /// O curto da janela de agora, pelo índice ([`janela_de`]), e não o que
    /// começou por último.
    ///
    /// O `not_before` de cada curto recua um dia
    /// ([`seele_proto::transport::ANTES_DA_JANELA`]), para o relógio atrasado
    /// do celular; com isso, no último dia de uma janela a seguinte já vale.
    /// Mas um link feito antes leva só os hashes de algumas janelas a partir da
    /// dele, e a seguinte pode não estar entre eles: apresentar a seguinte
    /// nesse dia seria recusado por um link que a de agora ainda atende.
    fn vigente(&self, agora: u64) -> Option<Arc<rustls::sign::CertifiedKey>> {
        indice_vigente(self.curtos.iter().map(|(curto, _)| curto), agora)
            .and_then(|indice| self.curtos.get(indice))
            .map(|(_, certificado)| Arc::clone(certificado))
    }
}

/// A posição, em `curtos`, do curto da janela de `agora`, se ele está lá e
/// ainda vale.
fn indice_vigente<'a>(curtos: impl Iterator<Item = &'a Curto>, agora: u64) -> Option<usize> {
    let antes = seele_proto::transport::ANTES_DA_JANELA.as_secs();
    let de_agora = janela_de(agora);
    curtos
        .enumerate()
        .find(|(_, curto)| {
            janela_de(curto.nao_antes + antes) == de_agora && agora < curto.nao_depois
        })
        .map(|(posicao, _)| posicao)
}

impl rustls::server::ResolvesServerCert for PorAlpn {
    fn resolve(
        &self,
        hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        let pede = |alpn: &[u8]| {
            hello
                .alpn()
                .is_some_and(|mut oferecidos| oferecidos.any(|o| o == alpn))
        };
        if pede(seele_proto::transport::ALPN) || !pede(seele_proto::transport::ALPN_DO_NAVEGADOR) {
            return Some(Arc::clone(&self.longo));
        }
        let agora = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let vigente = self.vigente(agora);
        if vigente.is_none() {
            tracing::warn!(
                agora,
                "um navegador pediu o `h3`, e nenhum certificado curto vale agora: o aperto de mão falha"
            );
        }
        vigente
    }
}

/// A configuração QUIC do `seeled`: a de [`server_config`] e, com `curtos`,
/// também o `h3` do navegador, com o certificado escolhido pelo ALPN.
///
/// Sem curtos (a sala pessoal, ou uma chave que não é P-256), é a mesma de
/// [`server_config`]: só `seele/1`.
///
/// # Errors
///
/// Falha se o rustls recusar uma chave ou um certificado.
pub fn server_config_do_seeled(
    identity: Identity,
    curtos: &[Curto],
) -> Result<quinn::ServerConfig> {
    if curtos.is_empty() {
        return server_config(identity);
    }
    let assina = rustls::crypto::ring::sign::any_supported_type(&identity.key)
        .context("o rustls recusou a chave do servidor")?;
    let longo = Arc::new(rustls::sign::CertifiedKey::new(
        identity.chain,
        Arc::clone(&assina),
    ));
    let curtos = curtos
        .iter()
        .map(|curto| {
            let certificado =
                rustls::sign::CertifiedKey::new(vec![curto.der.clone()], Arc::clone(&assina));
            (curto.clone(), Arc::new(certificado))
        })
        .collect();
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(PorAlpn { longo, curtos }));
    tls.alpn_protocols = vec![
        seele_proto::transport::ALPN.to_vec(),
        seele_proto::transport::ALPN_DO_NAVEGADOR.to_vec(),
    ];
    com_o_transporte(tls)
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
    let herdou = identidade_guardada(para).as_ref() != Some(&(cert.clone(), key.clone()));
    if herdou {
        guardar_identidade(para, &cert, &key)?;
    }
    // **E os curtos do `h3`, sempre da máquina** (Tarefa 2.5 da F4): os
    // servidores guardados desta máquina atendem no mesmo endereço com a mesma
    // chave, e o navegador confere pelo hash que o link levou. Um curto gerado
    // por cada servidor teria outro DER e outro hash, e o link feito enquanto
    // um rodava falharia no outro. Por isso eles nascem e se completam no banco
    // da máquina (`de`), e vão iguais para `para` — também quando a chave já
    // era a mesma, porque um servidor que herdou antes da 2.5 tem os próprios.
    let identidade = Identity {
        chain: vec![CertificateDer::from(cert)],
        key: PrivateKeyDer::try_from(key)
            .map_err(|erro| anyhow::anyhow!("a chave guardada não é uma chave: {erro}"))?,
    };
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let curtos = curtos_do_banco(de, &identidade, agora)?;
    let longo = identidade.fingerprint();
    let ja_tem: Vec<Vec<u8>> = curtos_guardados(para, &longo)
        .iter()
        .map(|curto| curto.der.as_ref().to_vec())
        .collect();
    let da_maquina: Vec<Vec<u8>> = curtos
        .iter()
        .map(|curto| curto.der.as_ref().to_vec())
        .collect();
    if !curtos.is_empty() && ja_tem != da_maquina {
        guardar_curtos(para, &longo, &curtos)?;
    }
    Ok(herdou)
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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]
mod os_curtos_do_h3 {
    use super::{curtos_de, dia_civil, indice_vigente, janela_de, Identity, EPOCA_DAS_JANELAS};
    use seele_proto::transport::{ANTES_DA_JANELA, PASSO_DA_JANELA, VALIDADE_DO_CURTO};

    const DIA: u64 = 86_400;

    /// O dia civil de instantes conhecidos, conferidos à parte (o `calendar` do
    /// Python): o começo do Unix, a época das janelas, o mês depois de um
    /// fevereiro comum, um 29 de fevereiro, e o ano secular que não é bissexto.
    #[test]
    fn o_dia_civil_bate_com_o_calendario() {
        for (segundos, dia) in [
            (0, (1970, 1, 1)),
            (EPOCA_DAS_JANELAS, (2026, 1, 1)),
            (1_772_323_200, (2026, 3, 1)),
            (1_835_395_200, (2028, 2, 29)),
            (4_107_542_400, (2100, 3, 1)),
        ] {
            assert_eq!(dia_civil(segundos), dia, "{segundos}");
        }
    }

    #[test]
    fn a_chave_ed25519_da_sala_nao_tem_curtos() {
        let sala = Identity::da_semente(&[3; 32]).unwrap();
        assert!(curtos_de(&sala, 0, 4).unwrap().is_empty());
    }

    /// Cada curto começa à meia-noite (o `rcgen` monta a data por dia, e o
    /// DER diz exatamente o `nao_antes` anunciado), vale 13 dias, e a janela
    /// `k` começa `k` passos depois da época, um dia antes.
    #[test]
    fn os_curtos_comecam_a_meia_noite_e_valem_treze_dias() {
        let longo = Identity::self_signed(vec!["localhost".into()]).unwrap();
        let curtos = curtos_de(&longo, 5, 3).unwrap();
        assert_eq!(curtos.len(), 3);
        for (k, curto) in (5_u64..).zip(&curtos) {
            assert_eq!(
                curto.nao_antes,
                EPOCA_DAS_JANELAS + k * PASSO_DA_JANELA.as_secs() - ANTES_DA_JANELA.as_secs()
            );
            assert_eq!(curto.nao_antes % DIA, 0);
            assert_eq!(
                curto.nao_depois - curto.nao_antes,
                VALIDADE_DO_CURTO.as_secs()
            );
        }
    }

    /// **No último dia de uma janela, o vigente é ela, e não a seguinte** —
    /// mesmo com a seguinte já valendo pelo `not_before`, que recua um dia.
    #[test]
    fn no_ultimo_dia_da_janela_o_vigente_e_ela_e_nao_a_seguinte() {
        let longo = Identity::self_signed(vec!["localhost".into()]).unwrap();
        let curtos = curtos_de(&longo, 3, 4).unwrap();
        let inicio_da_3 = EPOCA_DAS_JANELAS + 3 * PASSO_DA_JANELA.as_secs();
        let ultimo_dia = inicio_da_3 + PASSO_DA_JANELA.as_secs() - DIA / 2;
        assert_eq!(janela_de(ultimo_dia), 3);
        assert!(
            curtos[1].nao_antes <= ultimo_dia,
            "a armadilha não existe: a janela 4 ainda não vale no último dia da 3"
        );
        assert_eq!(indice_vigente(curtos.iter(), ultimo_dia), Some(0));
        assert_eq!(
            indice_vigente(curtos.iter(), inicio_da_3 + PASSO_DA_JANELA.as_secs()),
            Some(1)
        );
        // Antes da primeira gerada, nenhuma é a de agora.
        assert_eq!(indice_vigente(curtos.iter(), inicio_da_3 - 1), None);
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]
mod os_curtos_no_banco {
    use super::{
        curtos_do_banco, curtos_guardados, guardar_identidade, herdar_identidade,
        identidade_guardada, Curto, Identity, EPOCA_DAS_JANELAS,
    };
    use crate::persistence::{Location, Persistence};
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use seele_proto::transport::{JANELAS_ANUNCIADAS, PASSO_DA_JANELA};

    fn banco() -> Persistence {
        Persistence::open(&Location::Memory).expect("um banco em memória")
    }

    fn ders(curtos: &[Curto]) -> Vec<Vec<u8>> {
        curtos
            .iter()
            .map(|curto| curto.der.as_ref().to_vec())
            .collect()
    }

    fn identidade_de(banco: &Persistence) -> Identity {
        let (cert, key) = identidade_guardada(banco).expect("uma identidade");
        Identity {
            chain: vec![CertificateDer::from(cert)],
            key: PrivateKeyDer::try_from(key).unwrap(),
        }
    }

    fn agora() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    const EM: u64 = EPOCA_DAS_JANELAS + 10 * 6 * 86_400 + 3_600;

    /// **Um curto anunciado não é regenerado**: o ECDSA assina com um número
    /// aleatório, e regenerar trocaria o hash que um link já levou.
    #[test]
    fn reabrir_nao_regenera_o_que_ja_foi_anunciado() {
        let p = banco();
        let id = Identity::load_or_create(&p, vec!["localhost".into()]).unwrap();
        let primeira = curtos_do_banco(&p, &id, EM).unwrap();
        let segunda = curtos_do_banco(&p, &id, EM).unwrap();
        assert_eq!(primeira.len(), JANELAS_ANUNCIADAS);
        assert_eq!(
            ders(&primeira),
            ders(&segunda),
            "reabrir regenerou os curtos"
        );
    }

    #[test]
    fn com_o_tempo_os_vencidos_saem_e_os_que_ficam_sao_os_mesmos() {
        let p = banco();
        let id = Identity::load_or_create(&p, vec!["localhost".into()]).unwrap();
        let antes = curtos_do_banco(&p, &id, EM).unwrap();
        let depois = curtos_do_banco(&p, &id, EM + 3 * PASSO_DA_JANELA.as_secs()).unwrap();
        assert_eq!(depois.len(), JANELAS_ANUNCIADAS);
        assert_eq!(
            ders(&depois[..JANELAS_ANUNCIADAS - 3]),
            ders(&antes[3..]),
            "as janelas que continuam valendo foram regeneradas"
        );
    }

    #[test]
    fn uma_identidade_nova_descarta_os_curtos_da_velha() {
        let p = banco();
        let velha = Identity::load_or_create(&p, vec!["localhost".into()]).unwrap();
        let da_velha = curtos_do_banco(&p, &velha, EM).unwrap();
        let nova = Identity::self_signed(vec!["localhost".into()]).unwrap();
        let da_nova = curtos_do_banco(&p, &nova, EM).unwrap();
        assert!(
            ders(&da_nova)
                .iter()
                .all(|der| !ders(&da_velha).contains(der)),
            "a identidade nova anunciou um curto da velha, de outra chave"
        );
    }

    /// **Os servidores guardados desta máquina anunciam os mesmos curtos**: eles
    /// atendem no mesmo endereço, e o link de um tem de abrir no outro.
    #[test]
    fn os_servidores_da_maquina_anunciam_os_mesmos_curtos() {
        let maquina = banco();
        Identity::load_or_create(&maquina, vec!["localhost".into()]).unwrap();
        let (a, b) = (banco(), banco());
        assert!(herdar_identidade(&maquina, &a).unwrap());
        assert!(herdar_identidade(&maquina, &b).unwrap());
        let de_a = curtos_do_banco(&a, &identidade_de(&a), agora()).unwrap();
        let de_b = curtos_do_banco(&b, &identidade_de(&b), agora()).unwrap();
        assert_eq!(de_a.len(), JANELAS_ANUNCIADAS);
        assert_eq!(
            ders(&de_a),
            ders(&de_b),
            "dois servidores da máquina anunciaram curtos diferentes"
        );
    }

    /// Um servidor que herdou a chave antes da 2.5 gerou os próprios curtos; a
    /// herança seguinte troca os dele pelos da máquina, mesmo com a chave igual.
    #[test]
    fn um_servidor_que_herdou_antes_passa_a_ter_os_da_maquina() {
        let maquina = banco();
        Identity::load_or_create(&maquina, vec!["localhost".into()]).unwrap();
        let a = banco();
        let (cert, key) = identidade_guardada(&maquina).unwrap();
        guardar_identidade(&a, &cert, &key).unwrap();
        let proprios = curtos_do_banco(&a, &identidade_de(&a), agora()).unwrap();
        assert!(
            !herdar_identidade(&maquina, &a).unwrap(),
            "a chave já era a mesma"
        );
        let longo = identidade_de(&a).fingerprint();
        let agora_em_a = curtos_guardados(&a, &longo);
        let da_maquina = curtos_guardados(&maquina, &longo);
        assert_eq!(ders(&agora_em_a), ders(&da_maquina));
        assert_ne!(ders(&agora_em_a), ders(&proprios));
    }

    #[test]
    fn a_sala_com_a_chave_ed25519_nao_guarda_curtos() {
        let p = banco();
        let sala = Identity::da_semente(&[5; 32]).unwrap();
        assert!(curtos_do_banco(&p, &sala, EM).unwrap().is_empty());
        assert!(curtos_guardados(&p, &sala.fingerprint()).is_empty());
    }
}
