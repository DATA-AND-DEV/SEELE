//! A identidade da sala pessoal: um certificado Ed25519 feito da chave da pessoa.
//!
//! Spec de 23/09, «Identidade da sala e o conserto S2»: a sala apresenta no TLS
//! a chave de identidade de quem recebe, e quem liga confere o **código**
//! contra essa chave antes do `Hello`. Para o dono nunca ler «A CHAVE MUDOU» na
//! própria sala, o certificado sai igual byte a byte em toda abertura.
//!
//! **O Ed25519 nunca tinha rodado no TLS deste repositório.** O primeiro teste
//! mede isso sem nada da sala: se o `rustls` com o `ring` e o `quinn` não
//! apertassem a mão com uma chave Ed25519, nada do resto valeria.
//!
//! O verificador daqui é o molde do `ConfereCodigo` que a etapa 6 põe no
//! `seele-core`. Ele mora no teste porque o `seele-core` não pode ser dependência
//! do servidor (ADR 0002).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::SigningKey;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use seele_proto::codigo::Codigo;
use seele_server::tls::Identity;

/// O começo da SPKI de uma chave Ed25519 em DER (RFC 8410, §4): a `SEQUENCE`
/// de fora, o algoritmo `1.3.101.112` e o `BIT STRING` de 33 bytes.
const SPKI_ED25519: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// A chave Ed25519 crua de um certificado, ou nada se ele não for Ed25519.
fn chave_ed25519(certificado: &CertificateDer<'_>) -> Option<[u8; 32]> {
    let analisado = rustls::server::ParsedCertificate::try_from(certificado).ok()?;
    let spki = analisado.subject_public_key_info();
    let bytes: &[u8] = spki.as_ref();
    // Com ou sem a SEQUENCE de fora: o que importa é o algoritmo e a chave.
    let resto = bytes
        .strip_prefix(&SPKI_ED25519[..])
        .or_else(|| bytes.strip_prefix(&SPKI_ED25519[2..]))?;
    resto.try_into().ok()
}

/// Confere que a chave do certificado é a do código esperado, e delega ao
/// `ring` a prova de posse (o CertificateVerify).
#[derive(Debug)]
struct ConfereCodigoDeTeste {
    esperado: Option<Codigo>,
    provedor: Arc<rustls::crypto::CryptoProvider>,
}

impl ConfereCodigoDeTeste {
    fn novo(esperado: Option<Codigo>) -> Arc<Self> {
        Arc::new(Self {
            esperado,
            provedor: Arc::new(rustls::crypto::ring::default_provider()),
        })
    }
}

impl ServerCertVerifier for ConfereCodigoDeTeste {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let Some(chave) = chave_ed25519(end_entity) else {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::BadEncoding,
            ));
        };
        match self.esperado {
            // Só a medida do passo 1: qualquer Ed25519 serve.
            None => Ok(ServerCertVerified::assertion()),
            Some(codigo) if codigo.confere_chave(&chave) => Ok(ServerCertVerified::assertion()),
            Some(_) => Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            )),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provedor.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provedor.signature_verification_algorithms,
        )
    }

    /// Só Ed25519: a sala não tem outra chave, e um esquema a mais aqui seria
    /// um caminho que nenhuma sala percorre.
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ED25519]
    }
}

fn cliente(verificador: Arc<ConfereCodigoDeTeste>) -> quinn::ClientConfig {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verificador)
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    ))
}

/// Um servidor QUIC cru com esta identidade, e o que ele viu: `true` se uma
/// conexão chegou a abrir um fluxo.
async fn servidor_cru(identidade: Identity) -> (SocketAddr, tokio::task::JoinHandle<bool>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = seele_server::tls::server_config(identidade).expect("config do servidor");
    let ponta = quinn::Endpoint::server(config, SocketAddr::from(([127, 0, 0, 1], 0)))
        .expect("subir o servidor cru");
    let endereco = ponta.local_addr().expect("o endereço do servidor cru");
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

/// Liga para `endereco` com este verificador e manda «oi» num fluxo.
async fn ligar(endereco: SocketAddr, verificador: Arc<ConfereCodigoDeTeste>) -> anyhow::Result<()> {
    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(cliente(verificador));
    let conexao = ponta
        .connect(endereco, seele_server::tls::NOME_DA_SALA)?
        .await?;
    let (mut envio, _) = conexao.open_bi().await?;
    envio.write_all(b"oi").await?;
    envio.finish()?;
    // O servidor lê antes de a conexão cair.
    tokio::time::sleep(Duration::from_millis(200)).await;
    Ok(())
}

#[tokio::test]
async fn o_ed25519_aperta_a_mao_no_quic_deste_repositorio() {
    let par = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).expect("um par Ed25519");
    let certificado =
        rcgen::CertificateParams::new(vec![seele_proto::transport::NOME_DA_SALA.to_owned()])
            .unwrap()
            .self_signed(&par)
            .unwrap();
    let identidade = Identity {
        chain: vec![certificado.der().clone()],
        key: rustls::pki_types::PrivateKeyDer::try_from(par.serialize_der()).unwrap(),
    };

    let (endereco, visto) = servidor_cru(identidade).await;
    ligar(endereco, ConfereCodigoDeTeste::novo(None))
        .await
        .expect("o aperto de mão com Ed25519 não fechou: o rustls com o ring não serve à sala");
    assert!(
        tokio::time::timeout(Duration::from_secs(5), visto)
            .await
            .unwrap()
            .unwrap(),
        "a conexão com Ed25519 abriu e o servidor não viu o fluxo"
    );
}

/// A semente de zeros: a do vetor congelado.
const ZEROS: [u8; 32] = [0; 32];

/// O SHA-256 do DER da sala para a semente de zeros. **Formato congelado**: o
/// dono entra na própria sala por `127.0.0.1:8385`, o pino do TOFU é por
/// endereço, e um DER que mudasse entre versões daria «A CHAVE MUDOU» na
/// própria sala. Mudar o SAN, a validade, o número de série ou o nome muda
/// este número, e este teste reprova antes.
const DER_DA_SEMENTE_DE_ZEROS: &str =
    "82547b3cfe6c9b6f4a2eb295889c4b079b4d5550fbf48f9149cfa6afdc491d02";

#[test]
fn a_mesma_semente_da_o_mesmo_der() {
    let um = Identity::da_semente(&[42; 32]).expect("a primeira identidade");
    let outro = Identity::da_semente(&[42; 32]).expect("a segunda identidade");
    assert_eq!(
        um.chain, outro.chain,
        "o certificado da sala mudou entre duas aberturas com a mesma chave: \
         o dono leria «A CHAVE MUDOU» na própria sala"
    );
    assert_ne!(
        Identity::da_semente(&[43; 32]).unwrap().chain,
        um.chain,
        "duas chaves deram o mesmo certificado"
    );
}

#[test]
fn a_chave_do_certificado_e_a_da_identidade_da_pessoa() {
    let semente = [42_u8; 32];
    let identidade = Identity::da_semente(&semente).unwrap();
    let do_certificado =
        chave_ed25519(identidade.chain.first().unwrap()).expect("o certificado da sala é Ed25519");
    assert_eq!(
        do_certificado,
        SigningKey::from_bytes(&semente).verifying_key().to_bytes(),
        "a sala apresenta uma chave que não é a da pessoa: o código não bateria"
    );
}

#[test]
fn o_der_da_semente_de_zeros_e_o_congelado() {
    let identidade = Identity::da_semente(&ZEROS).unwrap();
    let der = identidade.chain.first().unwrap();
    let hex: String = seele_proto::transport::certificate_digest(der.as_ref())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        hex, DER_DA_SEMENTE_DE_ZEROS,
        "o DER da sala mudou: todo dono leria «A CHAVE MUDOU» na própria sala depois de atualizar"
    );
}

#[test]
fn o_debug_da_configuracao_nao_mostra_a_chave() {
    // Bytes que não aparecem por acaso num texto de depuração.
    let semente: [u8; 32] = core::array::from_fn(|i| 0x9c ^ u8::try_from(i).unwrap());
    let config = seele_server::ServerConfig {
        identidade: Some(Identity::da_semente(&semente).unwrap()),
        ..seele_server::ServerConfig::default()
    };
    let texto = format!("{config:?}");
    // Sem colchetes: num vazamento, a semente aparece no meio de uma lista mais
    // longa, depois do prefixo do PKCS#8. A primeira versão deste teste
    // procurava `[156, 157, …` e passou com a chave inteira no texto.
    let em_lista = semente[..6]
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let em_hex: String = semente[..6].iter().map(|b| format!("{b:02x}")).collect();
    let em_hex_maiusculo = em_hex.to_uppercase();
    assert!(
        !texto.contains(&em_lista)
            && !texto.contains(&em_hex)
            && !texto.contains(&em_hex_maiusculo),
        "o Debug da configuração mostrou a chave da pessoa: {texto}"
    );
}

#[tokio::test]
async fn o_codigo_certo_aperta_a_mao_e_o_errado_e_recusado_antes_de_qualquer_fluxo() {
    let semente = [42_u8; 32];
    let chave = SigningKey::from_bytes(&semente).verifying_key().to_bytes();
    let certo = Codigo::da_chave(&chave);
    let errado = Codigo::da_chave(&[7; 32]);

    let (endereco, visto) = servidor_cru(Identity::da_semente(&semente).unwrap()).await;
    ligar(endereco, ConfereCodigoDeTeste::novo(Some(certo)))
        .await
        .expect("o código certo não apertou a mão");
    assert!(
        tokio::time::timeout(Duration::from_secs(5), visto)
            .await
            .unwrap()
            .unwrap(),
        "com o código certo, o servidor não viu o fluxo"
    );

    let (endereco, visto) = servidor_cru(Identity::da_semente(&semente).unwrap()).await;
    assert!(
        ligar(endereco, ConfereCodigoDeTeste::novo(Some(errado)))
            .await
            .is_err(),
        "o código errado apertou a mão: quem liga entraria na sala de outra pessoa"
    );
    assert!(
        !tokio::time::timeout(Duration::from_secs(5), visto)
            .await
            .unwrap()
            .unwrap(),
        "com o código errado, o servidor chegou a ver um fluxo: algo foi enviado antes da recusa"
    );
}

#[tokio::test]
async fn o_servidor_com_a_identidade_da_sala_aceita_a_prova_amarrada_a_ela() {
    use ed25519_dalek::Signer;
    use seele_proto::control::{ClientMessage, ServerMessage};
    use seele_server::frame;

    let semente = [42_u8; 32];
    let pessoa = SigningKey::from_bytes(&semente);
    let pasta = tempfile::tempdir().unwrap();
    let config = seele_server::ServerConfig {
        identidade: Some(Identity::da_semente(&semente).unwrap()),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: seele_server::persistence::Location::File(pasta.path().join("seele.db")),
        ..seele_server::ServerConfig::default()
    };
    let servidor = Arc::new(
        seele_server::Daemon::bind(config)
            .await
            .expect("subir a sala"),
    );
    assert_eq!(
        servidor.fingerprint(),
        Identity::da_semente(&semente).unwrap().fingerprint(),
        "o servidor não apresentou a identidade que recebeu"
    );
    {
        let banco = servidor.server().persistence.lock().await;
        assert!(
            seele_server::tls::identidade_guardada(&banco).is_none(),
            "a chave da pessoa foi gravada no banco do servidor"
        );
    }
    let endereco = servidor.local_addr().unwrap();
    let rodando = Arc::clone(&servidor);
    tokio::spawn(async move { rodando.run().await });

    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
    ponta.set_default_client_config(cliente(ConfereCodigoDeTeste::novo(Some(Codigo::da_chave(
        &pessoa.verifying_key().to_bytes(),
    )))));
    let conexao = ponta
        .connect(endereco, seele_server::tls::NOME_DA_SALA)
        .unwrap()
        .await
        .unwrap();
    let (mut envio, mut recebe) = conexao.open_bi().await.unwrap();
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: "dona".into(),
            public_key: pessoa.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: Vec::new(),
        },
    )
    .await
    .unwrap();
    let ServerMessage::Challenge { nonce } =
        frame::read::<ServerMessage>(&mut recebe).await.unwrap()
    else {
        panic!("a sala não mandou Challenge");
    };
    let cadeia = conexao
        .peer_identity()
        .unwrap()
        .downcast::<Vec<CertificateDer<'static>>>()
        .unwrap();
    let impressao = seele_proto::transport::certificate_digest(cadeia.first().unwrap().as_ref());
    let nonce: [u8; 32] = nonce.as_slice().try_into().unwrap();
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: pessoa
                .sign(&seele_proto::assinatura::prova_de_cliente(
                    &impressao, &nonce,
                ))
                .to_bytes()
                .to_vec(),
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(
            frame::read::<ServerMessage>(&mut recebe).await.unwrap(),
            ServerMessage::Session { .. }
        ),
        "a prova amarrada à impressão da sala não foi aceita pela sala"
    );
    servidor.shutdown();
}
