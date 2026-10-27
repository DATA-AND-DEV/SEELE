//! Certificate handling for the daemon.
//!
//! ADR 0003 makes TOFU the default: the server presents a self-signed
//! certificate, the client memorises the public key on first contact and shouts
//! if it ever changes. `specs/08-seguranca.md` gives the reasoning — it is the
//! SSH model, the audience already understands it, and ACME would demand a
//! domain plus ports 80/443, which contradicts the single-UDP-port simplicity of
//! `specs/01-arquitetura.md`.
//!
//! There is no plaintext path and no flag to disable encryption.
//! `specs/08-seguranca.md` is categorical about that, so this module has no
//! "insecure" branch to audit. Since 0.17 the certificate is presented by the
//! DTLS of `seele-enlace` ([`Identity::no_enlace`]), not by TLS-in-QUIC.

use anyhow::{Context, Result};
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

// ------------------------------------------------- a identidade no enlace
//
// Até a 0.16 aqui moravam o `quinn::ServerConfig`, o TLS do `seele/1` e os
// certificados curtos do `h3` com o resolvedor por ALPN (ADR 0057). A 0.17 não
// fala QUIC (contrato da fase 2, D1): o DTLS é do `seele-enlace`, o navegador
// confere o `a=fingerprint` sem limite de validade, e os curtos, as janelas e
// os `serverCertificateHashes` deixaram de existir.

/// Por que uma identidade não serve ao enlace.
#[derive(Debug, thiserror::Error)]
pub enum IdentidadeQueNaoServe {
    /// A chave guardada neste banco não é P-256 (R1 da revisão da 0.17).
    ///
    /// O DTLS do navegador e do `str0m` só negocia ECDSA P-256, e o `seeled`
    /// sempre gerou P-256 (`rcgen::generate_simple_self_signed`). Um banco com
    /// outra chave **não é regerado aqui**, de propósito: uma chave nova troca a
    /// impressão, e todo cliente que fixou a de antes leria «A CHAVE DO
    /// SERVIDOR MUDOU», o alerta reservado para ataque.
    #[error(
        "a identidade guardada neste banco não é uma chave P-256 ({0}), e a 0.17 só apresenta \
         P-256 no DTLS. Ela não foi trocada: uma chave nova mudaria a impressão digital, e quem \
         já entrou leria «A CHAVE DO SERVIDOR MUDOU». Para seguir, quem hospeda decide: \
         apague `tls_cert` e `tls_key` da tabela `configuracao` deste banco (e avise quem entra \
         que a impressão vai mudar), ou hospede este banco pela 0.16"
    )]
    NaoEP256(String),
    /// A chave da sala não assinou a apresentação, ou o certificado efêmero não
    /// saiu.
    #[error("a identidade da sala não se apresentou: {0}")]
    Sala(String),
}

/// O que um servidor apresenta no enlace.
#[derive(Debug, Clone)]
pub struct NoEnlace {
    /// O certificado DTLS: o longo do banco no `seeled`, efêmero na sala.
    pub certificado: seele_enlace::CertificadoDtls,
    /// O corpo da apresentação `0x03` no `ctl`: vazio no `seeled`; na sala, a
    /// chave Ed25519 (32) e a assinatura dela (64) sobre
    /// [`seele_proto::assinatura::apresentacao_da_sala`] da impressão DTLS.
    pub apresentacao: Vec<u8>,
    /// A impressão que a prova do cliente amarra (D5): a do certificado no
    /// `seeled`, a mesma do `fp=` e da 0.16; na sala, o SHA-256 dos 32 bytes
    /// da chave, que é o que quem liga por código calcula.
    pub impressao: [u8; 32],
}

impl Identity {
    /// A semente Ed25519, quando esta é a identidade de uma sala
    /// ([`Self::da_semente`]): o PKCS#8 v1 que ela monta, prefixo e semente.
    fn semente_da_sala(&self) -> Option<[u8; 32]> {
        let der = self.key.secret_der();
        let semente = der.strip_prefix(&PREFIXO_PKCS8_ED25519[..])?;
        semente.try_into().ok()
    }

    /// **O que esta identidade apresenta no enlace** (D3).
    ///
    /// - No `seeled`, o certificado longo P-256 **é** o certificado DTLS: o
    ///   `a=fingerprint` é o `fp=` do convite, e é a mesma impressão da 0.16
    ///   para o mesmo banco. Os pinos TOFU de quem atualiza continuam valendo.
    /// - Com `sala` e uma identidade de [`Self::da_semente`], o DTLS é P-256
    ///   efêmero (o DTLS não negocia Ed25519), e a identidade vai na
    ///   apresentação `0x03`, assinada pela chave da sala.
    ///
    /// `sala` diz se esta identidade pode ser a de uma sala: só a que veio na
    /// configuração da sala pessoal. Uma chave Ed25519 **guardada no banco**
    /// não é sala nenhuma, e é recusada como qualquer outra que não é P-256.
    ///
    /// # Errors
    ///
    /// [`IdentidadeQueNaoServe`], com a frase para quem hospeda.
    pub fn no_enlace(&self, sala: bool) -> Result<NoEnlace, IdentidadeQueNaoServe> {
        if sala {
            if let Some(semente) = self.semente_da_sala() {
                return apresentacao_da_sala(&semente);
            }
        }
        let der = self
            .chain
            .first()
            .map(|c| c.as_ref().to_vec())
            .unwrap_or_default();
        let certificado = seele_enlace::CertificadoDtls::novo(der, self.key.secret_der().to_vec())
            .map_err(|erro| IdentidadeQueNaoServe::NaoEP256(format!("{erro:#}")))?;
        Ok(NoEnlace {
            impressao: certificado.impressao(),
            certificado,
            apresentacao: Vec::new(),
        })
    }
}

/// A apresentação da sala desta semente, sobre um certificado DTLS efêmero.
fn apresentacao_da_sala(semente: &[u8; 32]) -> Result<NoEnlace, IdentidadeQueNaoServe> {
    use ed25519_dalek::Signer as _;
    let certificado = seele_enlace::CertificadoDtls::efemero()
        .map_err(|erro| IdentidadeQueNaoServe::Sala(format!("{erro:#}")))?;
    let chave = ed25519_dalek::SigningKey::from_bytes(semente);
    let publica = chave.verifying_key().to_bytes();
    let assinatura = chave.sign(&seele_proto::assinatura::apresentacao_da_sala(
        &certificado.impressao(),
    ));
    let mut apresentacao = Vec::with_capacity(32 + 64);
    apresentacao.extend_from_slice(&publica);
    apresentacao.extend_from_slice(&assinatura.to_bytes());
    Ok(NoEnlace {
        certificado,
        apresentacao,
        impressao: seele_proto::transport::certificate_digest(&publica),
    })
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
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]
mod a_identidade_no_enlace {
    use super::{guardar_identidade, identidade_guardada, IdentidadeQueNaoServe, Identity};
    use crate::persistence::{Location, Persistence};

    /// **A impressão da 0.17 é a da 0.16 para o mesmo banco** (R1): o
    /// certificado DTLS do `seeled` é o longo guardado, e o `a=fingerprint` é o
    /// `fp=` de sempre. Sem isto, todo pino TOFU de quem atualiza dispararia
    /// «A CHAVE MUDOU».
    #[test]
    fn o_seeled_apresenta_no_dtls_a_mesma_impressao_do_banco() {
        let banco = Persistence::open(&Location::Memory).unwrap();
        let identidade = Identity::load_or_create(&banco, vec!["localhost".into()]).unwrap();
        let no_enlace = identidade.no_enlace(false).unwrap();
        assert_eq!(no_enlace.impressao, identidade.digest());
        assert_eq!(no_enlace.certificado.impressao(), identidade.digest());
        assert!(
            no_enlace.apresentacao.is_empty(),
            "o seeled se apresenta vazio"
        );
        let (cert, _) = identidade_guardada(&banco).unwrap();
        assert_eq!(no_enlace.certificado.der(), cert.as_slice());
    }

    /// **R1: uma chave guardada que não é P-256 é recusada com o nome, e o
    /// banco fica como estava.** Regerar trocaria a chave em silêncio.
    #[test]
    fn uma_identidade_ed25519_guardada_e_recusada_sem_regerar() {
        let banco = Persistence::open(&Location::Memory).unwrap();
        let ed = Identity::da_semente(&[9; 32]).unwrap();
        let cert = ed.chain.first().unwrap().as_ref().to_vec();
        let chave = ed.key.secret_der().to_vec();
        guardar_identidade(&banco, &cert, &chave).unwrap();

        let lida = Identity::load_or_create(&banco, vec!["localhost".into()]).unwrap();
        let recusa = lida.no_enlace(false).unwrap_err();
        assert!(
            matches!(recusa, IdentidadeQueNaoServe::NaoEP256(_)),
            "{recusa}"
        );
        let frase = recusa.to_string();
        assert!(
            frase.contains("P-256") && frase.contains("A CHAVE DO SERVIDOR MUDOU"),
            "{frase}"
        );
        assert_eq!(
            identidade_guardada(&banco),
            Some((cert, chave)),
            "a recusa regerou a identidade guardada"
        );
    }

    /// **A sala se apresenta com a chave dela sobre o DTLS efêmero** (D3), e
    /// a impressão da prova é o SHA-256 da chave (D5), a que quem liga por
    /// código calcula.
    #[test]
    fn a_sala_assina_a_impressao_do_dtls_efemero() {
        let semente = [7_u8; 32];
        let sala = Identity::da_semente(&semente).unwrap();
        let no_enlace = sala.no_enlace(true).unwrap();
        let chave = ed25519_dalek::SigningKey::from_bytes(&semente).verifying_key();
        assert_eq!(no_enlace.apresentacao.len(), 96);
        assert_eq!(no_enlace.apresentacao[..32], chave.to_bytes());
        let assinatura =
            ed25519_dalek::Signature::from_slice(&no_enlace.apresentacao[32..]).unwrap();
        chave
            .verify_strict(
                &seele_proto::assinatura::apresentacao_da_sala(&no_enlace.certificado.impressao()),
                &assinatura,
            )
            .expect("a apresentação da sala não confere com o DTLS dela");
        assert_eq!(
            no_enlace.impressao,
            seele_proto::transport::certificate_digest(&chave.to_bytes())
        );
        // Efêmero: duas subidas, dois DTLS, a mesma identidade.
        let outra = sala.no_enlace(true).unwrap();
        assert_ne!(
            outra.certificado.impressao(),
            no_enlace.certificado.impressao()
        );
        assert_eq!(outra.impressao, no_enlace.impressao);
    }

    /// Fora da sala, a identidade Ed25519 da semente não vira sala: é uma
    /// chave que não é P-256, e é recusada.
    #[test]
    fn a_semente_fora_da_sala_e_recusada() {
        let sala = Identity::da_semente(&[1; 32]).unwrap();
        assert!(sala.no_enlace(false).is_err());
    }
}
