//! Um fluxo unidirecional diz o que é, e o servidor não adivinha mais.
//!
//! §5.2 de `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`:
//! um servidor recebe **dois** tipos de fluxo unidirecional — transferência de
//! anexo e tela — e até esta onda nada no fio dizia qual era qual. O que os
//! separava era aritmética sobre o primeiro byte: zero era transferência,
//! porque o quadro dela cabe em 16 KiB e o byte mais significativo do
//! comprimento nunca chegava a valer mais que isso; não-zero era tela, porque a
//! versão do cabeçalho dela nasceu em 1.
//!
//! As duas premissas eram emprestadas de outra seção, e no dia em que uma delas
//! mudasse o sintoma seria **um fluxo lido como o tipo errado** — o pior
//! formato de erro de protocolo que existe, porque ele não parece um erro.
//!
//! Estes dois testes prendem as duas metades do conserto: o byte reservado é
//! recusado alto, e o tipo conhecido leva o cabeçalho intacto para quem o lê.
//!
//! # Por que um par cru
//!
//! `seele-core` não pode ser dependência daqui (ADR 0002), e o que está sob
//! teste é justamente o primeiro byte de um fluxo — coisa que nenhum cliente
//! oferece controle sobre. `tests/par_lento.rs` monta o mesmo par pelo mesmo
//! motivo.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::attachment::AttachmentHeader;
use seele_proto::control::{AttachmentRefusal, ClientMessage, ServerMessage};
use seele_proto::ids::{ClientMessageId, ChannelId};
use seele_proto::stream::{StreamType, RESERVED_TYPE};
use seele_server::persistence::Location;
use seele_server::{frame, ServerConfig, Daemon};

/// Um verificador que aceita qualquer certificado.
///
/// O que está sob teste é o primeiro byte de um fluxo, não o TOFU — que tem os
/// testes dele em `seele-core/src/tofu.rs`.
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

/// Um servidor em memória, já atendendo.
///
/// **Sem diretório de anexos**, e é o que faz o segundo teste ser barato: a
/// resposta honesta a uma transferência é `Unavailable`, que só sai depois de o
/// cabeçalho ter sido lido inteiro — que é exatamente o que se quer provar.
async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
}

/// Um par cru: a conexão QUIC e o fluxo de controle, apertados à mão.
struct Par {
    conexao: quinn::Connection,
    _envio: quinn::SendStream,
    recebe: quinn::RecvStream,
}

async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];

    let config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    ));

    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(config);
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    // A ponta é dona do socket; sem isto ela cai no fim desta função e leva a
    // conexão junto.
    std::mem::forget(ponta);

    let (mut envio, mut recebe) = conexao.open_bi().await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            version: seele_proto::PROTOCOL_VERSION,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
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
    let ServerMessage::Session { .. } = frame::read::<ServerMessage>(&mut recebe).await? else {
        anyhow::bail!("o servidor não mandou Session");
    };

    Ok(Par {
        conexao,
        _envio: envio,
        recebe,
    })
}

/// O quadro de um cabeçalho de transferência: quatro bytes de tamanho e o corpo.
fn cabecalho_de_anexo(chave: u64) -> Vec<u8> {
    let header = AttachmentHeader {
        channel: ChannelId(1),
        client_message_id: ClientMessageId(chave),
        body: String::new(),
        replies_to: None,
        file_name: "retrato.png".into(),
        declared_type: "image/png".into(),
        declared_len: 4,
        content_hash: [0_u8; 32],
    };
    let quadro = seele_proto::control::encode(&header).expect("codificar o cabeçalho");
    let mut bytes = u32::try_from(quadro.len())
        .expect("cabe")
        .to_be_bytes()
        .to_vec();
    bytes.extend(quadro);
    bytes
}

/// O byte que a leitura antiga produzia é recusado, e não entendido de novo.
///
/// Estes são **exatamente os bytes que um par velho põe no fio** para mandar um
/// anexo: sem marca de tipo, e com o byte mais significativo do comprimento —
/// zero, porque um quadro de controle cabe em 16 KiB — no lugar onde hoje vai o
/// tipo. A leitura antiga os aceitava; a nova tem de recusá-los.
///
/// Recusar e não aceitar é a decisão do §5.2, e ela é sobre o par velho: aceitar
/// zero deixaria um cliente de antes desta onda ser **entendido errado** — o
/// fluxo de tela dele lido como cabeçalho de anexo — em vez de falhar alto. Um
/// fluxo lido como o tipo errado é o pior formato de erro de protocolo que
/// existe, porque ele não parece um erro.
///
/// As duas asserções são as duas metades disso: nenhuma resposta de
/// transferência volta, e o fluxo é cortado de propósito e não por acidente.
#[tokio::test(flavor = "multi_thread")]
async fn o_byte_reservado_e_recusado_em_vez_de_ser_lido_como_anexo() -> Result<()> {
    let (endereco, _servidor) = server().await?;
    let mut par = abrir(endereco, 1).await?;

    let mut fluxo = par.conexao.open_uni().await?;
    // O quadro do jeito antigo: os quatro bytes do comprimento inteiros, com o
    // zero na frente fazendo as vezes do tipo. É o que a aritmética lia.
    fluxo.write_all(&cabecalho_de_anexo(1)).await?;
    fluxo.write_all(&[1, 2, 3, 4]).await?;
    fluxo.finish()?;

    // Nada volta. Se voltasse, o servidor teria lido o cabeçalho — quer dizer,
    // teria adivinhado o tipo do fluxo a partir do conteúdo dele.
    let resposta = tokio::time::timeout(
        Duration::from_millis(1500),
        frame::read::<ServerMessage>(&mut par.recebe),
    )
    .await;
    if let Ok(Ok(ServerMessage::AttachmentRefused { .. })) = resposta {
        panic!("o servidor leu um fluxo de tipo reservado como transferência");
    }

    // E o corte foi deliberado: `CODIGO_DE_CORTE`, e não o zero que um fluxo
    // largado no chão produziria. A diferença é entre o servidor ter recusado e o
    // server ter caído.
    let mut segundo = par.conexao.open_uni().await?;
    segundo.write_all(&[RESERVED_TYPE]).await?;
    let veredito = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Err(erro) = segundo.write_all(&[0_u8; 64]).await {
                return erro;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        matches!(
            veredito,
            Ok(quinn::WriteError::Stopped(codigo))
                if codigo == quinn::VarInt::from_u32(seele_server::tela::CODIGO_DE_CORTE)
        ),
        "o servidor não cortou o tipo reservado de propósito: {veredito:?}"
    );
    Ok(())
}

/// E o cabeçalho de quem se identifica chega inteiro do outro lado.
///
/// A outra metade do conserto, e a que apaga a dívida: com o tipo no fio, o
/// cabeçalho de transferência volta a começar no seu próprio primeiro byte.
/// Enquanto ele não vinha, `transfer::receive` e `quem_perguntou` carregavam um
/// parâmetro `primeiro` — o byte que a demultiplexação tinha consumido e
/// precisava devolver. Se esta resposta nomeia a chave de idempotência certa, o
/// cabeçalho foi lido do começo.
#[tokio::test(flavor = "multi_thread")]
async fn um_fluxo_que_diz_ser_anexo_e_lido_como_anexo() -> Result<()> {
    let (endereco, _servidor) = server().await?;
    let mut par = abrir(endereco, 2).await?;

    let mut fluxo = par.conexao.open_uni().await?;
    fluxo.write_all(&[StreamType::Attachment.byte()]).await?;
    fluxo.write_all(&cabecalho_de_anexo(77)).await?;
    fluxo.write_all(&[1, 2, 3, 4]).await?;
    fluxo.finish()?;

    // Este servidor não guarda arquivo, então a resposta honesta é `Unavailable` —
    // e ela só existe porque o cabeçalho foi lido: é dele que sai o nome a quem
    // responder.
    let resposta = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::AttachmentRefused {
                client_message_id,
                reason,
            } = frame::read::<ServerMessage>(&mut par.recebe).await?
            {
                return anyhow::Ok((client_message_id, reason));
            }
        }
    })
    .await??;

    assert_eq!(
        resposta,
        (ClientMessageId(77), AttachmentRefusal::Unavailable)
    );
    Ok(())
}
