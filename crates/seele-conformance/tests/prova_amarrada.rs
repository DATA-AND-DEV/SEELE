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
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
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
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
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

/// **Uma chave de ordem pequena é recusada no registro**, antes do desafio.
///
/// Uma chave dessas não prova posse de nada: a conferência comum do
/// `ed25519-dalek` aceita com ela assinaturas que não exigem chave privada
/// nenhuma. O servidor a recusa ao ler o `Hello`, com a mesma razão uniforme de
/// toda credencial que não serve, e confere a prova com `verify_strict`. Achado
/// da F2, pedido pela orquestradora em 08/10/2026.
#[tokio::test(flavor = "multi_thread")]
async fn uma_chave_de_ordem_pequena_e_recusada_antes_do_desafio() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    // O elemento neutro da curva, comprimido: y = 1, x positivo.
    let mut identidade = [0u8; 32];
    identidade[0] = 1;
    assert!(
        ed25519_dalek::VerifyingKey::from_bytes(&identidade)?.is_weak(),
        "a chave do teste deixou de ser de ordem pequena"
    );

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
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: "pessoa064".into(),
            public_key: identidade.to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let resposta = frame::read::<ServerMessage>(&mut recebe).await?;
    assert_eq!(
        resposta,
        ServerMessage::Disconnecting {
            reason: DisconnectReason::CredentialRejected
        },
        "uma chave de ordem pequena chegou ao desafio"
    );

    drop(ponta);
    daemon.shutdown();
    Ok(())
}

/// **O lado do cliente:** o `Client::connect` de verdade assina a impressão do
/// certificado que o TLS daquela conexão conferiu, e nenhuma outra.
///
/// Quem confere é um servidor de teste, com um certificado próprio: ele lê o
/// `Hello`, manda um desafio conhecido e confere a assinatura que volta, com a
/// chave do `Hello`, contra a mensagem montada com a impressão **do certificado
/// que ele mesmo apresentou**. O conteúdo assinado não precisa de gancho no
/// cliente: ele sai inteiro na assinatura, e quem tem a chave pública o
/// confere. Pedido pela orquestradora em 08/10/2026 (a Tarefa 13b do plano).
#[tokio::test(flavor = "multi_thread")]
async fn o_cliente_assina_a_impressao_do_certificado_que_o_tls_conferiu() -> Result<()> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let _vaga = vaga::minha();
    let _ = rustls::crypto::ring::default_provider().install_default();

    let identidade = seele_server::tls::Identity::self_signed(vec!["localhost".into()])?;
    let apresentada = seele_proto::transport::certificate_digest(
        identidade
            .chain
            .first()
            .expect("a identidade tem certificado")
            .as_ref(),
    );
    let escuta = quinn::Endpoint::server(
        seele_server::tls::server_config(identidade)?,
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )?;
    let endereco = escuta.local_addr()?;

    let nonce = [0x5C_u8; 32];
    let conferindo = tokio::spawn(async move {
        let conexao = escuta
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("a escuta fechou antes de o cliente chegar"))?
            .await?;
        let (mut envio, mut recebe) = conexao.accept_bi().await?;
        let ClientMessage::Hello { public_key, .. } =
            frame::read::<ClientMessage>(&mut recebe).await?
        else {
            anyhow::bail!("o primeiro quadro não foi Hello");
        };
        frame::write(
            &mut envio,
            &ServerMessage::Challenge {
                nonce: nonce.to_vec(),
            },
        )
        .await?;
        let ClientMessage::Response { proof } = frame::read::<ClientMessage>(&mut recebe).await?
        else {
            anyhow::bail!("o segundo quadro não foi Response");
        };
        // Recusa, para o cliente sair do aperto de mão; o que importa já
        // chegou.
        let _ = frame::write(
            &mut envio,
            &ServerMessage::Disconnecting {
                reason: DisconnectReason::CredentialRejected,
            },
        )
        .await;
        let _ = envio.finish();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), envio.stopped()).await;

        let chave = VerifyingKey::from_bytes(&public_key.as_slice().try_into()?)?;
        let assinatura = Signature::from_slice(&proof)?;
        let com_a_apresentada = chave
            .verify(&prova_de_cliente(&apresentada, &nonce), &assinatura)
            .is_ok();
        let com_outra = chave
            .verify(&prova_de_cliente(&[0; 32], &nonce), &assinatura)
            .is_ok();
        let com_o_nonce_cru = chave.verify(&nonce, &assinatura).is_ok();
        drop(escuta);
        Ok((com_a_apresentada, com_outra, com_o_nonce_cru))
    });

    let resultado = seele_core::Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "marcela",
        &SigningKey::from_bytes(&[64; 32]),
        Arc::new(seele_core::MemoryPinStore::new()),
        None,
        None,
    )
    .await;
    assert!(
        resultado.is_err(),
        "o servidor de teste recusa sempre, e o cliente entrou"
    );

    let (com_a_apresentada, com_outra, com_o_nonce_cru) = conferindo.await??;
    assert!(
        com_a_apresentada,
        "a assinatura do cliente não confere com a impressão do certificado que o TLS \
         apresentou: ele assinou outra impressão"
    );
    assert!(
        !com_outra,
        "a assinatura confere com uma impressão que ninguém apresentou"
    );
    assert!(
        !com_o_nonce_cru,
        "o cliente voltou a assinar o nonce cru, como a 0.15"
    );
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
