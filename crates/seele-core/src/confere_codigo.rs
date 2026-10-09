//! Quem liga confere o código **dentro do TLS**, antes do `Hello`.
//!
//! Spec de 23/09, «Quem liga confere o código dentro do TLS», e o ADR 0060. A
//! sala pessoal apresenta um certificado Ed25519 feito da chave da pessoa, e
//! quem liga só sabe o código. Este verificador lê a chave do certificado, exige
//! que seja Ed25519, confere o código contra ela e, quando já houve uma ligação
//! completa, confere o pino da chave inteira. A prova de posse (o
//! CertificateVerify) fica com o `ring`.
//!
//! Uma chave que não bate derruba o aperto de mão antes de qualquer fluxo: nada
//! do que quem liga diria chega a quem atendeu.
//!
//! # O que não é usado aqui
//!
//! O `PinStore` por `host:porta` (`crate::tofu`). Uma sala é achada pelo código,
//! em qualquer endereço, e o pino dela é a chave, sob `@CÓDIGO` — ver
//! [`chave_do_pino`].
//!
//! # O motivo tipado
//!
//! O `rustls` só aceita de volta um `rustls::Error`. O motivo de verdade
//! ([`FalhaDoCodigo`]) sai por um [`RelatoDoCodigo`] antes de virar texto, no
//! molde do `Relato` de `crate::par`, para quem disca dizer «QUEM ATENDEU NÃO É
//! O DONO DESTE CÓDIGO» e nunca um «a conexão segura falhou» genérico.

use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use seele_proto::codigo::Codigo;

/// O começo da SPKI de uma chave Ed25519 em DER (RFC 8410, §4): a `SEQUENCE`
/// de fora, o algoritmo `1.3.101.112` e o `BIT STRING` de 33 bytes.
const SPKI_ED25519: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// Por que quem atendeu não é a sala deste código.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FalhaDoCodigo {
    /// O certificado não traz uma chave Ed25519: não é uma sala pessoal.
    #[error("quem atendeu não apresentou a chave de uma pessoa")]
    NaoEhChaveDePessoa,
    /// A chave de quem atendeu tem outro código.
    #[error("quem atendeu não é o dono deste código: esperado {esperado}, veio {veio}")]
    CodigoNaoBate {
        /// O código que quem liga digitou.
        esperado: Codigo,
        /// O código da chave que quem atendeu apresentou.
        veio: Codigo,
    },
    /// O código bate, e a chave não é a que ficou pinada na primeira ligação.
    /// São 75 bits contra 256: é alguém que achou uma chave com o mesmo código.
    #[error("alguém tentou se passar pelo dono do código {codigo}")]
    SePassouPor {
        /// O código de quem se passou.
        codigo: Codigo,
    },
}

/// Onde o verificador deixa o [`FalhaDoCodigo`] que o `rustls` não sabe
/// carregar. Uma vaga por discagem.
pub type RelatoDoCodigo = Arc<Mutex<Option<FalhaDoCodigo>>>;

/// A chave sob a qual o pino de um código mora no arquivo de pins.
///
/// O `@` não aparece em `host:porta`, então um código nunca se confunde com um
/// endereço no mesmo arquivo.
#[must_use]
pub fn chave_do_pino(codigo: &Codigo) -> String {
    format!("@{codigo}")
}

/// A chave Ed25519 crua de um certificado, ou nada se ele não for Ed25519.
fn chave_ed25519(certificado: &CertificateDer<'_>) -> Option<[u8; 32]> {
    let analisado = rustls::server::ParsedCertificate::try_from(certificado).ok()?;
    let spki = analisado.subject_public_key_info();
    let bytes: &[u8] = spki.as_ref();
    // Com ou sem a SEQUENCE de fora: o que importa é o algoritmo e a chave.
    let resto = bytes
        .strip_prefix(&SPKI_ED25519[..])
        .or_else(|| bytes.strip_prefix(SPKI_ED25519.get(2..)?))?;
    resto.try_into().ok()
}

/// Aceita só a sala deste código.
#[derive(Debug)]
pub struct ConfereCodigo {
    esperado: Codigo,
    /// A impressão da chave crua pinada sob [`chave_do_pino`], quando já houve
    /// uma ligação completa: `seele_proto::transport::key_fingerprint`.
    pino: Option<String>,
    provedor: Arc<rustls::crypto::CryptoProvider>,
    relato: RelatoDoCodigo,
}

impl ConfereCodigo {
    /// Um verificador para a sala de `esperado`, com o pino lido por quem disca.
    #[must_use]
    pub fn novo(esperado: Codigo, pino: Option<String>) -> Self {
        Self {
            esperado,
            pino,
            provedor: Arc::new(rustls::crypto::ring::default_provider()),
            relato: RelatoDoCodigo::default(),
        }
    }

    /// A vaga em que o motivo tipado aparece quando o aperto de mão falha.
    #[must_use]
    pub fn relato(&self) -> RelatoDoCodigo {
        Arc::clone(&self.relato)
    }

    /// A conferência em si, fora do `trait`. Devolve a chave de quem atendeu,
    /// que é o que quem disca pina depois de uma ligação completa.
    ///
    /// # Errors
    ///
    /// [`FalhaDoCodigo`], dizendo qual dos três casos foi.
    pub fn confere(&self, certificado: &CertificateDer<'_>) -> Result<[u8; 32], FalhaDoCodigo> {
        let chave = chave_ed25519(certificado).ok_or(FalhaDoCodigo::NaoEhChaveDePessoa)?;
        let veio = Codigo::da_chave(&chave);
        if veio != self.esperado {
            return Err(FalhaDoCodigo::CodigoNaoBate {
                esperado: self.esperado,
                veio,
            });
        }
        // Depois da primeira ligação completa, a conferência é a chave
        // inteira, e não os 75 bits do código (ADR 0060, «Força»).
        if let Some(pino) = &self.pino {
            if *pino != seele_proto::transport::key_fingerprint(&chave) {
                return Err(FalhaDoCodigo::SePassouPor { codigo: veio });
            }
        }
        Ok(chave)
    }
}

impl ServerCertVerifier for ConfereCodigo {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.confere(end_entity)
            .map(|_| ServerCertVerified::assertion())
            .map_err(|erro| {
                // O motivo tipado sai por aqui antes de ser achatado em texto.
                if let Ok(mut vaga) = self.relato.lock() {
                    *vaga = Some(erro.clone());
                }
                rustls::Error::General(erro.to_string())
            })
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provedor.signature_verification_algorithms,
        )
    }

    /// A prova de posse da chave: delegada ao `ring`, que é quem sabe.
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provedor.signature_verification_algorithms,
        )
    }

    /// Só Ed25519: uma sala não tem outra chave, e um esquema a mais aqui seria
    /// um caminho que nenhuma sala percorre.
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Duration;

    /// O PKCS#8 v1 de uma chave Ed25519 sem a pública (RFC 8410, §7). O mesmo
    /// de `seele_server::tls::Identity::da_semente`, que o `seele-core` não pode
    /// importar (ADR 0002).
    const PREFIXO_PKCS8_ED25519: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];

    fn chave_publica(semente: [u8; 32]) -> [u8; 32] {
        ed25519_dalek::SigningKey::from_bytes(&semente)
            .verifying_key()
            .to_bytes()
    }

    /// O certificado de uma sala, como a sala o monta.
    fn certificado_da_sala(
        semente: [u8; 32],
    ) -> (
        CertificateDer<'static>,
        rustls::pki_types::PrivateKeyDer<'static>,
    ) {
        let mut pkcs8 = PREFIXO_PKCS8_ED25519.to_vec();
        pkcs8.extend_from_slice(&semente);
        let der = rustls::pki_types::PrivatePkcs8KeyDer::from(pkcs8);
        let par = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&der, &rcgen::PKCS_ED25519).unwrap();
        let certificado = rcgen::CertificateParams::new(vec!["seele-sala".to_owned()])
            .unwrap()
            .self_signed(&par)
            .unwrap();
        (
            certificado.der().clone(),
            rustls::pki_types::PrivateKeyDer::Pkcs8(der),
        )
    }

    /// Um servidor QUIC cru com este certificado, e o que ele viu: `true` se
    /// uma conexão chegou a abrir um fluxo.
    fn servidor(
        certificado: CertificateDer<'static>,
        chave: rustls::pki_types::PrivateKeyDer<'static>,
    ) -> (SocketAddr, tokio::task::JoinHandle<bool>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificado], chave)
            .unwrap();
        tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
        let config = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap(),
        ));
        let ponta = quinn::Endpoint::server(config, SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
        let endereco = ponta.local_addr().unwrap();
        let visto = tokio::spawn(async move {
            let Some(chegando) = ponta.accept().await else {
                return false;
            };
            let Ok(conexao) = chegando.await else {
                return false;
            };
            let Ok((_, mut recebe)) = conexao.accept_bi().await else {
                return false;
            };
            let mut oi = [0_u8; 2];
            recebe.read_exact(&mut oi).await.is_ok() && &oi == b"oi"
        });
        (endereco, visto)
    }

    /// Liga para `endereco` conferindo com `confere`, e manda «oi» num fluxo.
    async fn ligar(endereco: SocketAddr, confere: ConfereCodigo) -> anyhow::Result<()> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(confere))
            .with_no_client_auth();
        tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
        let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        ponta.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls)?,
        )));
        let conexao = ponta.connect(endereco, "seele-sala")?.await?;
        let (mut envio, _) = conexao.open_bi().await?;
        envio.write_all(b"oi").await?;
        envio.finish()?;
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(())
    }

    const SEMENTE: [u8; 32] = [42; 32];

    #[tokio::test]
    async fn o_codigo_certo_aperta_a_mao_num_quic_real() {
        let (certificado, chave) = certificado_da_sala(SEMENTE);
        let (endereco, visto) = servidor(certificado, chave);
        let codigo = Codigo::da_chave(&chave_publica(SEMENTE));

        ligar(endereco, ConfereCodigo::novo(codigo, None))
            .await
            .expect("o código certo não apertou a mão");
        assert!(
            tokio::time::timeout(Duration::from_secs(5), visto)
                .await
                .unwrap()
                .unwrap(),
            "com o código certo, a sala não viu o fluxo"
        );
    }

    #[tokio::test]
    async fn o_codigo_errado_e_recusado_antes_de_qualquer_fluxo_e_o_relato_diz_os_dois() {
        let (certificado, chave) = certificado_da_sala(SEMENTE);
        let (endereco, visto) = servidor(certificado, chave);
        let esperado = Codigo::da_chave(&[7; 32]);
        let confere = ConfereCodigo::novo(esperado, None);
        let relato = confere.relato();

        assert!(
            ligar(endereco, confere).await.is_err(),
            "o código errado apertou a mão: quem liga entraria na sala de outra pessoa"
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(5), visto)
                .await
                .unwrap()
                .unwrap(),
            "com o código errado, a sala chegou a ver um fluxo"
        );
        assert_eq!(
            relato.lock().unwrap().clone(),
            Some(FalhaDoCodigo::CodigoNaoBate {
                esperado,
                veio: Codigo::da_chave(&chave_publica(SEMENTE)),
            }),
            "o motivo tipado não chegou ao relato, e quem liga leria uma falha genérica"
        );
    }

    #[test]
    fn com_o_pino_da_mesma_chave_passa_e_com_o_de_outra_chave_se_passou_por() {
        let (certificado, _) = certificado_da_sala(SEMENTE);
        let codigo = Codigo::da_chave(&chave_publica(SEMENTE));

        let pino_certo = seele_proto::transport::key_fingerprint(&chave_publica(SEMENTE));
        assert_eq!(
            ConfereCodigo::novo(codigo, Some(pino_certo)).confere(&certificado),
            Ok(chave_publica(SEMENTE))
        );

        // A mesma situação de uma chave achada com o mesmo código: o código
        // bate, e a chave não é a pinada.
        let pino_de_outra = seele_proto::transport::key_fingerprint(&chave_publica([9; 32]));
        assert_eq!(
            ConfereCodigo::novo(codigo, Some(pino_de_outra)).confere(&certificado),
            Err(FalhaDoCodigo::SePassouPor { codigo }),
            "uma chave diferente da pinada passou só porque o código batia"
        );
    }

    #[test]
    fn um_certificado_que_nao_e_ed25519_nao_e_uma_sala() {
        let comum = rcgen::generate_simple_self_signed(vec!["seele-sala".to_owned()]).unwrap();
        let codigo = Codigo::da_chave(&chave_publica(SEMENTE));
        assert_eq!(
            ConfereCodigo::novo(codigo, None).confere(comum.cert.der()),
            Err(FalhaDoCodigo::NaoEhChaveDePessoa)
        );
    }

    #[test]
    fn so_o_esquema_ed25519_e_oferecido() {
        let codigo = Codigo::da_chave(&chave_publica(SEMENTE));
        assert_eq!(
            ConfereCodigo::novo(codigo, None).supported_verify_schemes(),
            vec![SignatureScheme::ED25519]
        );
    }

    #[test]
    fn o_pino_de_um_codigo_nao_se_confunde_com_um_endereco() {
        let codigo = Codigo::da_chave(&chave_publica(SEMENTE));
        let chave = chave_do_pino(&codigo);
        assert!(chave.starts_with('@'), "{chave}");
        assert!(chave.ends_with(&codigo.to_string()), "{chave}");
    }
}
