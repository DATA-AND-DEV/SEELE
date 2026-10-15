//! **Recusar os MODs não gasta o convite.**
//!
//! Hipótese da F8 (a tela do celular), medida em 09/10/2026: o servidor gastava
//! o convite de uso único na portaria, **antes** de perguntar dos MODs. Quem
//! recusava os MODs — o caso de todo app que ainda não os tinha aceitado: ele
//! recusa, devolve a lista à tela, e entra de novo depois do sim — ficava com o
//! convite gasto, e a volta, **mesmo com o mesmo token**, recebia
//! `CredentialRejected` com a portaria desligada. O caso irmão da P4, que
//! resolveu o mesmo para o apelido tomado.
//!
//! Desde o conserto, o convite é gasto só depois de os MODs serem aceitos: no
//! instante em que a pessoa de fato entra.
//!
//! # O que este arquivo não cobre
//!
//! A volta **sem** o token continua recusada com a portaria desligada: a
//! política de admissão não tem memória, e sem portaria nada registra quem já
//! passou pelo segredo. Fazer o app reenviar o token, ou o servidor lembrar de
//! quem só faltava aceitar os MODs, é outra decisão.
//!
//! # Por que um par cru
//!
//! O que está sob teste é o que o servidor faz com o convite; o par cru recusa
//! e aceita os MODs à mão, como o cliente de verdade faz.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_server::persistence::mods::{enable, EnabledMod};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

/// Um verificador que aceita qualquer certificado: o que está sob teste é a
/// recusa de um pedido, não o TOFU.
#[derive(Debug)]
struct AceitaQualquer(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AceitaQualquer {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon))
}

/// Como uma batida terminou.
#[derive(Debug, PartialEq, Eq)]
enum Fim {
    /// Entrou: chegou o `Session`.
    Entrou,
    /// O servidor recusou, antes ou depois dos MODs.
    Recusada(DisconnectReason),
    /// Chegou outro quadro no lugar.
    Outro(String),
}

/// Uma batida inteira: o `Hello` com `convite`, a prova, e então aceita os MODs
/// (com o conjunto anunciado) ou os recusa.
async fn bater(endereco: SocketAddr, semente: u8, convite: &str, aceita: bool) -> Result<Fim> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    )));
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    let (mut envio, mut recebe) = conexao.open_bi().await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: Some(convite.to_owned()),
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let fim = |quadro: ServerMessage| match quadro {
        ServerMessage::Session { .. } => Fim::Entrou,
        ServerMessage::Disconnecting { reason } => Fim::Recusada(reason),
        outro => Fim::Outro(format!("{outro:?}")),
    };
    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave
                .sign(&prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let depois = frame::read::<ServerMessage>(&mut recebe).await?;
    let ServerMessage::ModsExigidos { conjunto, .. } = depois else {
        return Ok(fim(depois));
    };
    let resposta = if aceita {
        ClientMessage::AceitarMods { conjunto }
    } else {
        ClientMessage::RecusarMods
    };
    frame::write(&mut envio, &resposta).await?;
    Ok(fim(frame::read::<ServerMessage>(&mut recebe).await?))
}

/// Um servidor fechado por convite, com a portaria desligada e um MOD exigido.
/// Devolve o convite.
async fn servidor_fechado() -> Result<(SocketAddr, Arc<Daemon>, String)> {
    let (endereco, daemon) = servidor().await?;
    let convite = {
        let mut banco = daemon.server().persistence.lock().await;
        enable(
            &banco,
            &EnabledMod {
                id: "fulano/exemplo".into(),
                version: "1.0.0".into(),
                hash: "ab".repeat(32),
                repo: "https://github.com/seele/exemplo".into(),
                reach: vec!["ler".into()],
                server_half: false,
                api: 0,
            },
        )?;
        seele_server::portaria::ligar(&mut banco, false)?;
        seele_server::admissao::criar_convite(&mut banco, "para quem ainda vai aceitar")?
    };
    Ok((endereco, daemon, convite))
}

/// **O caso da F8:** a primeira batida recusa os MODs, e a segunda, com o mesmo
/// convite, os aceita e entra.
#[tokio::test(flavor = "multi_thread")]
async fn recusar_os_mods_nao_gasta_o_convite() -> Result<()> {
    let (endereco, daemon, convite) = servidor_fechado().await?;

    let primeira = bater(endereco, 141, &convite, false).await?;
    assert_eq!(primeira, Fim::Recusada(DisconnectReason::ModsRecusados));

    let segunda = bater(endereco, 141, &convite, true).await?;
    assert_eq!(
        segunda,
        Fim::Entrou,
        "o convite foi gasto por quem só recusou os MODs: a volta com o mesmo \
         token não entra"
    );

    daemon.shutdown();
    Ok(())
}

/// O controle: entrar de fato gasta o convite, e outra chave com o mesmo token
/// não entra mais.
#[tokio::test(flavor = "multi_thread")]
async fn entrar_de_fato_gasta_o_convite() -> Result<()> {
    let (endereco, daemon, convite) = servidor_fechado().await?;

    assert_eq!(bater(endereco, 142, &convite, true).await?, Fim::Entrou);
    assert_eq!(
        bater(endereco, 143, &convite, true).await?,
        Fim::Recusada(DisconnectReason::CredentialRejected),
        "o convite de uso único entrou duas vezes"
    );

    daemon.shutdown();
    Ok(())
}

/// A mensagem que a prova assina: o rótulo, a impressão do certificado que o
/// TLS desta conexão conferiu e o nonce (o S2).
fn prova_amarrada(conexao: &quinn::Connection, nonce: &[u8]) -> Vec<u8> {
    let cadeia = conexao
        .peer_identity()
        .and_then(|identidade| {
            identidade
                .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
                .ok()
        })
        .expect("o TLS sempre entrega a cadeia do servidor");
    let impressao = seele_proto::transport::certificate_digest(
        cadeia
            .first()
            .expect("a cadeia tem um certificado")
            .as_ref(),
    );
    let nonce: [u8; 32] = nonce.try_into().expect("o desafio tem 32 bytes");
    seele_proto::assinatura::prova_de_cliente(&impressao, &nonce)
}
