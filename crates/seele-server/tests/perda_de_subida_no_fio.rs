//! O relato de perda de subida atravessa o fio, e só chega a quem produziu.
//!
//! As duas malhas do ADR 0036 já estão provadas isoladas: o controlador em
//! `seele_audio::bitrate` e o estimador em `seele_server::perda_de_subida`. O que
//! nenhuma delas prova é a **costura** — que a sala mede, que a sessão filtra por
//! destinatário, e que o quadro chega ao cliente certo e a mais ninguém.
//!
//! # O que este teste não tenta
//!
//! Induzir perda de verdade num enlace QUIC local não é confiável e não é o que
//! falta provar. A lacuna aqui é produzida do jeito honesto: um `seq` que não é
//! enviado. Pela definição do protocolo isso **é** um pacote que saiu e não
//! chegou, que é exatamente o que o estimador conta.

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
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{Ssrc, VoiceRoomId};
use seele_proto::media::MediaHeader;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

/// Um quadro de voz do tamanho que 32 kbps produz. O servidor não decodifica
/// nada — `specs/08-seguranca.md` proíbe tocar no payload —, então o conteúdo
/// não importa e o tamanho é o que faz o teste parecer com a vida.
const PAYLOAD: [u8; 80] = [7; 80];

/// Um verificador que aceita qualquer certificado.
///
/// O que está sob teste é o escalonamento da sessão, não o TOFU.
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

/// Um par cru: a conexão QUIC e o fluxo de controle, sem tarefa leitora.
struct Par {
    conexao: quinn::Connection,
    envio: quinn::SendStream,
    recebe: quinn::RecvStream,
    /// O que o servidor atribuiu a esta conexão. Nunca escolhido pelo cliente.
    ssrc: Ssrc,
    /// A primeira sala de voz que este par pode ver.
    sala: VoiceRoomId,
}

/// Abre uma conexão e faz o aperto de mão à mão.
///
/// `janela` é a janela de recepção de fluxo que este par anuncia. Uma janela
/// pequena não cria o defeito: só encurta o tempo até o par parar de aceitar
/// bytes, que é a condição do cenário.
async fn abrir(endereco: SocketAddr, semente: u8, janela: u32) -> Result<Par> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];

    let mut config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    ));
    let mut transporte = quinn::TransportConfig::default();
    transporte.stream_receive_window(janela.into());
    config.transport_config(Arc::new(transporte));

    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(config);
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    // A ponta é dona do socket; sem isto ela cai no fim desta função.
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
    let ServerMessage::Session {
        ssrc, voice_rooms, ..
    } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };
    let sala = voice_rooms
        .first()
        .map(|info| info.id)
        .ok_or_else(|| anyhow::anyhow!("o servidor não ofereceu sala de voz nenhuma"))?;

    Ok(Par {
        conexao,
        envio,
        recebe,
        ssrc,
        sala,
    })
}

/// Um quadro de voz pronto para o fio, com o ssrc que o servidor atribuiu.
fn quadro(ssrc: Ssrc, seq: u16) -> Vec<u8> {
    let cabecalho = MediaHeader {
        version: seele_proto::PROTOCOL_VERSION,
        ssrc: ssrc.get(),
        seq,
        // 20 ms a 48 kHz são 960 amostras. O servidor não lê isto; quem lê é o
        // buffer de jitter do outro lado.
        timestamp: u32::from(seq) * 960,
    };
    let mut fora = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = cabecalho
        .encode_datagram(&PAYLOAD, &mut fora)
        .expect("o quadro cabe num datagrama");
    fora.truncate(tamanho);
    fora
}

/// Quem perde pacote é avisado, e o vizinho não fica sabendo.
///
/// A segunda metade é a promessa de privacidade do ADR 0036, e é a que um
/// filtro por destinatário erra calado: difundir a medida contaria a toda a
/// sala a qualidade da rede de cada um, e nada na tela de ninguém diria que
/// isso aconteceu.
#[tokio::test(flavor = "multi_thread")]
async fn quem_perde_pacote_e_avisado_e_o_vizinho_nao() -> Result<()> {
    let (endereco, servidor) = server().await?;

    let mut falante = abrir(endereco, 1, 1024 * 1024).await?;
    let sala = falante.sala;
    frame::write(
        &mut falante.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;

    let mut vizinho = abrir(endereco, 2, 1024 * 1024).await?;
    frame::write(
        &mut vizinho.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Duzentos quadros com o de número cem nunca enviado. Vinte milissegundos
    // entre um e o seguinte: o ritmo de um microfone de verdade, sob o teto de
    // sessenta por segundo que o `taxa::Vigia` aplica, e tempo bastante para a
    // sala cruzar o intervalo de medida com amostra cheia.
    for seq in 0..200_u16 {
        if seq == 100 {
            continue;
        }
        let _ = falante
            .conexao
            .send_datagram(quadro(falante.ssrc, seq).into());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // **Todos** os relatos, e não o primeiro.
    //
    // A sala relata uma vez por segundo, e o primeiro relato sai por volta do
    // primeiro segundo — antes da lacuna, que está no quadro de número cem, aos
    // dois segundos. A primeira redação deste teste parava no primeiro `Some` e
    // reprovava com zero, acusando o código de um defeito que era do teste.
    let mut relatos = Vec::new();
    let prazo = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < prazo {
        match tokio::time::timeout(
            Duration::from_millis(500),
            frame::read::<ServerMessage>(&mut falante.recebe),
        )
        .await
        {
            Ok(Ok(ServerMessage::UplinkLoss { fraction })) => relatos.push(fraction),
            Ok(Ok(_)) => {}
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }
    println!("relatos de perda: {relatos:?}");
    assert!(
        !relatos.is_empty(),
        "quem falou não recebeu relato nenhum: o quadro não atravessou o fio"
    );
    assert!(
        relatos.iter().any(|fracao| *fracao > 0.0),
        "nenhum dos {} relatos viu a lacuna, e um pacote em duzentos nunca saiu: {relatos:?}",
        relatos.len()
    );

    // E o vizinho, que não mandou voz nenhuma, não recebe relato sobre a rede de
    // ninguém.
    while let Ok(Ok(quadro)) = tokio::time::timeout(
        Duration::from_millis(200),
        frame::read::<ServerMessage>(&mut vizinho.recebe),
    )
    .await
    {
        assert!(
            !matches!(quadro, ServerMessage::UplinkLoss { .. }),
            "o vizinho recebeu a medida de rede de outra pessoa"
        );
    }

    servidor.shutdown();
    Ok(())
}
