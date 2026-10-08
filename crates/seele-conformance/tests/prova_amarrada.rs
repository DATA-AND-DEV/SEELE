//! **A prova de identidade vale só para o servidor que o cliente conferiu.**
//!
//! Até a 0.15, o cliente assinava o nonce cru que o servidor mandasse, e o
//! servidor conferia a assinatura sobre esse nonce. Uma prova não dizia **para
//! quem** ela era (análise de 22/09, §3.1, o S2).
//!
//! Desde o Servidor 1.0, a mensagem assinada é o rótulo, a impressão do
//! certificado do servidor e o nonce de 32 bytes, montada por
//! `seele_proto::assinatura::prova_de_cliente`. Este arquivo prova os dois
//! lados disso, cada um por conta própria:
//!
//! - **o servidor** só aceita uma prova amarrada à impressão **dele** — uma
//!   prova amarrada a outra impressão é recusada com `CredentialRejected`;
//! - **o cliente de verdade** (`seele_core::Client::connect`) assina a impressão
//!   do certificado que o TLS daquela conexão conferiu, e nunca outra.
//!
//! As duas metades juntas são o portão do Plano 2 (spec-mestre da 0.16, §2,
//! F1), como a orquestradora o fixou em 08/10/2026.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_core::frame;
use seele_proto::assinatura::prova_de_cliente;
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

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

/// A impressão do certificado que o TLS desta conexão aceitou, em bytes.
fn impressao_de(conexao: &quinn::Connection) -> [u8; 32] {
    let cadeia = conexao
        .peer_identity()
        .and_then(|identidade| {
            identidade
                .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
                .ok()
        })
        .expect("o TLS sempre entrega a cadeia do servidor");
    seele_proto::transport::certificate_digest(
        cadeia
            .first()
            .expect("a cadeia tem um certificado")
            .as_ref(),
    )
}

/// O que o servidor respondeu à prova: `Session` ou o quadro que veio no
/// lugar dele.
#[derive(Debug)]
enum Resposta {
    Entrou,
    Recusou(ServerMessage),
}

/// Faz o aperto de mão à mão e assina a prova **com a impressão que a função
/// `impressao` escolher**, a partir da impressão de verdade.
async fn bater_com(
    endereco: SocketAddr,
    semente: u8,
    impressao: impl FnOnce([u8; 32]) -> [u8; 32],
) -> Result<Resposta> {
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
            join_secret: None,
            material_e2ee: None,
        },
    )
    .await?;

    let primeiro = frame::read::<ServerMessage>(&mut recebe).await?;
    let ServerMessage::Challenge { nonce } = primeiro else {
        return Ok(Resposta::Recusou(primeiro));
    };
    let nonce: [u8; 32] = nonce.as_slice().try_into()?;
    let assinada = prova_de_cliente(&impressao(impressao_de(&conexao)), &nonce);
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave.sign(&assinada).to_bytes().to_vec(),
        },
    )
    .await?;

    let resposta = frame::read::<ServerMessage>(&mut recebe).await?;
    drop(ponta);
    Ok(match resposta {
        ServerMessage::Session { .. } => Resposta::Entrou,
        outro => Resposta::Recusou(outro),
    })
}

/// O caminho certo: a prova amarrada à impressão deste servidor entra.
#[tokio::test(flavor = "multi_thread")]
async fn a_prova_amarrada_a_impressao_deste_servidor_entra() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let resposta = bater_com(endereco, 61, |de_verdade| de_verdade).await?;
    assert!(
        matches!(resposta, Resposta::Entrou),
        "a prova amarrada à impressão deste servidor foi recusada: {resposta:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// **O lado do servidor:** uma prova feita para outro servidor — amarrada a
/// outra impressão — é recusada, com a mesma razão uniforme de toda assinatura
/// que não confere (`specs/08-seguranca.md`).
#[tokio::test(flavor = "multi_thread")]
async fn uma_prova_amarrada_a_outra_impressao_e_recusada() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let resposta = bater_com(endereco, 62, |de_verdade| {
        let mut outra = de_verdade;
        outra[0] ^= 0xFF;
        outra
    })
    .await?;
    assert!(
        matches!(
            resposta,
            Resposta::Recusou(ServerMessage::Disconnecting {
                reason: DisconnectReason::CredentialRejected
            })
        ),
        "uma prova amarrada a outra impressão não foi recusada: {resposta:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// E o nonce cru, como a 0.15 assinava, também é recusado: sem rótulo e sem
/// impressão, a assinatura não confere.
#[tokio::test(flavor = "multi_thread")]
async fn a_prova_da_0_15_sobre_o_nonce_cru_e_recusada() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

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
    let chave = SigningKey::from_bytes(&[63; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: "pessoa063".into(),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
        },
    )
    .await?;
    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave.sign(&nonce).to_bytes().to_vec(),
        },
    )
    .await?;
    let resposta = frame::read::<ServerMessage>(&mut recebe).await?;
    assert_eq!(
        resposta,
        ServerMessage::Disconnecting {
            reason: DisconnectReason::CredentialRejected
        },
        "a prova sobre o nonce cru ainda entra"
    );

    drop(ponta);
    daemon.shutdown();
    Ok(())
}

// ------------------------------------------------------------- o par cru

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
