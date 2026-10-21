//! **A sessão pelo navegador, sobre a WebTransport** (Tarefa 2.6 da F4; ADR
//! 0057).
//!
//! O cliente daqui é o do `wtransport`, que confere o certificado como o
//! navegador: pelo hash (`serverCertificateHashes`), só dentro da validade, com
//! no máximo duas semanas e só ECDSA P-256. Ele faz o aperto de mão do SEELE
//! pelo fluxo de controle, com os mesmos quadros do nativo, e assina a prova
//! sobre a impressão do **longo** — a do `fp=` do link —, porque a página não
//! vê o certificado que a TLS apresentou.
//!
//! O que se prova:
//! - o navegador entra, e a sessão responde a um `Ping`;
//! - **a voz atravessa os dois transportes**: uma pessoa pelo navegador e outra
//!   pelo nativo, na mesma sala, e o datagrama de cada uma chega à outra;
//! - um hash que não é do servidor é recusado, e o servidor conta a recusa.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
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

/// A impressão do longo, que o link leva no `fp=` e a prova amarra.
fn impressao_do_longo(daemon: &Daemon) -> [u8; 32] {
    let hex = daemon.fingerprint();
    let mut bytes = [0_u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    bytes
}

fn hello(semente: u8, chave: &SigningKey) -> ClientMessage {
    ClientMessage::Hello {
        faixa: seele_proto::FAIXA,
        client: "navegador-de-teste".into(),
        nickname: format!("pessoa{semente:03}"),
        public_key: chave.verifying_key().to_bytes().to_vec(),
        join_secret: None,
        material_e2ee: None,
        apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
    }
}

/// O que o aperto de mão devolve: o `ssrc` e a primeira sala.
async fn apertar(
    envio: &mut quinn::SendStream,
    recebe: &mut quinn::RecvStream,
    semente: u8,
    impressao: [u8; 32],
) -> Result<(Ssrc, VoiceRoomId)> {
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(envio, &hello(semente, &chave)).await?;
    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(recebe).await? else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    let nonce: [u8; 32] = nonce.as_slice().try_into()?;
    let prova = seele_proto::assinatura::prova_de_cliente(&impressao, &nonce);
    frame::write(
        envio,
        &ClientMessage::Response {
            proof: chave.sign(&prova).to_bytes().to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session {
        ssrc, voice_rooms, ..
    } = frame::read::<ServerMessage>(recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };
    let sala = voice_rooms.first().map(|info| info.id).expect("uma sala");
    Ok((ssrc, sala))
}

/// O navegador: o cliente do `wtransport`, com os hashes das janelas.
async fn navegador(
    endereco: SocketAddr,
    hashes: Vec<[u8; 32]>,
) -> Result<(
    wtransport::Endpoint<wtransport::endpoint::endpoint_side::Client>,
    wtransport::Connection,
)> {
    let config = wtransport::ClientConfig::builder()
        .with_bind_default()
        .with_server_certificate_hashes(hashes.into_iter().map(wtransport::tls::Sha256Digest::new))
        .build();
    let ponta = wtransport::Endpoint::client(config)?;
    let conexao = tokio::time::timeout(
        Duration::from_secs(5),
        ponta.connect(format!("https://127.0.0.1:{}/", endereco.port())),
    )
    .await??;
    Ok((ponta, conexao))
}

/// O nativo: o QUIC de `seele/1`, que confere o longo (aqui, aceita qualquer um).
async fn nativo(endereco: SocketAddr) -> Result<(quinn::Endpoint, quinn::Connection)> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    #[derive(Debug)]
    struct Aceita(Arc<rustls::crypto::CryptoProvider>);
    impl rustls::client::danger::ServerCertVerifier for Aceita {
        fn verify_server_cert(
            &self,
            _: &rustls::pki_types::CertificateDer<'_>,
            _: &[rustls::pki_types::CertificateDer<'_>],
            _: &rustls::pki_types::ServerName<'_>,
            _: &[u8],
            _: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            m: &[u8],
            c: &rustls::pki_types::CertificateDer<'_>,
            d: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls12_signature(
                m,
                c,
                d,
                &self.0.signature_verification_algorithms,
            )
        }
        fn verify_tls13_signature(
            &self,
            m: &[u8],
            c: &rustls::pki_types::CertificateDer<'_>,
            d: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(
                m,
                c,
                d,
                &self.0.signature_verification_algorithms,
            )
        }
        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Aceita(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls)?,
    )));
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    Ok((ponta, conexao))
}

fn hashes(daemon: &Daemon) -> Vec<[u8; 32]> {
    daemon
        .janelas_do_navegador()
        .iter()
        .take(seele_proto::uri::LIMITE_DE_HASHES_DO_NAVEGADOR)
        .map(|janela| janela.hash)
        .collect()
}

/// Um quadro de voz com o `ssrc` de quem fala.
fn quadro(ssrc: Ssrc, seq: u16) -> Vec<u8> {
    let cabecalho = MediaHeader {
        ssrc: ssrc.get(),
        seq,
        timestamp: u32::from(seq) * 960,
    };
    let mut fora = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = cabecalho
        .encode_datagram(&[0xF8; 40], &mut fora)
        .expect("cabe");
    fora.truncate(tamanho);
    fora
}

#[tokio::test(flavor = "multi_thread")]
async fn o_navegador_entra_pela_webtransport_e_a_sessao_responde() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let (_ponta, conexao) = navegador(endereco, hashes(&daemon)).await?;
    let (mut envio, mut recebe) = conexao.open_bi().await?.await?;
    let (envio, recebe) = (envio.quic_stream_mut(), recebe.quic_stream_mut());
    apertar(envio, recebe, 9, impressao_do_longo(&daemon)).await?;

    frame::write(envio, &ClientMessage::Ping { timestamp: 4242 }).await?;
    let voltou = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::Pong { timestamp: 4242 } =
                frame::read::<ServerMessage>(recebe).await?
            {
                return Ok::<_, anyhow::Error>(true);
            }
        }
    })
    .await??;
    assert!(voltou, "a sessão pelo navegador não respondeu ao Ping");
    assert_eq!(daemon.recusas_do_navegador(), 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_voz_atravessa_do_navegador_ao_nativo_e_de_volta() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let impressao = impressao_do_longo(&daemon);

    let (_pn, nativo) = nativo(endereco).await?;
    let (mut n_envio, mut n_recebe) = nativo.open_bi().await?;
    let (ssrc_nativo, sala) = apertar(&mut n_envio, &mut n_recebe, 1, impressao).await?;

    let (_pw, web) = navegador(endereco, hashes(&daemon)).await?;
    let (mut w_envio, mut w_recebe) = web.open_bi().await?.await?;
    let (w_envio, w_recebe) = (w_envio.quic_stream_mut(), w_recebe.quic_stream_mut());
    let (ssrc_web, sala_web) = apertar(w_envio, w_recebe, 2, impressao).await?;
    assert_eq!(sala, sala_web);

    for envio in [&mut n_envio, &mut *w_envio] {
        frame::write(
            envio,
            &ClientMessage::EnterVoiceRoom {
                voice_room: sala,
                password: None,
            },
        )
        .await?;
    }
    // O controle dos dois é drenado: a sessão não pode travar no fluxo cheio.
    let mut n_recebe = n_recebe;
    tokio::spawn(async move { while frame::read::<ServerMessage>(&mut n_recebe).await.is_ok() {} });
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Do navegador ao nativo.
    let ouvinte = nativo.clone();
    let ouviu_do_navegador = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let bytes = ouvinte.read_datagram().await?;
                if let Ok((cabecalho, _)) = MediaHeader::decode(&bytes) {
                    if cabecalho.ssrc == ssrc_web.get() {
                        return Ok::<_, anyhow::Error>(true);
                    }
                }
            }
        })
        .await
    });
    for seq in 0..25_u16 {
        let _ = web.send_datagram(quadro(ssrc_web, seq));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        matches!(ouviu_do_navegador.await?, Ok(Ok(true))),
        "a voz do navegador não chegou ao nativo"
    );

    // Do nativo ao navegador.
    let ouvinte = web.clone();
    let ouviu_do_nativo = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let datagrama = ouvinte.receive_datagram().await?;
                if let Ok((cabecalho, _)) = MediaHeader::decode(datagrama.payload().as_ref()) {
                    if cabecalho.ssrc == ssrc_nativo.get() {
                        return Ok::<_, anyhow::Error>(true);
                    }
                }
            }
        })
        .await
    });
    for seq in 0..25_u16 {
        let _ = nativo.send_datagram(quadro(ssrc_nativo, seq).into());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        matches!(ouviu_do_nativo.await?, Ok(Ok(true))),
        "a voz do nativo não chegou ao navegador"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn um_hash_que_nao_e_do_servidor_e_recusado_e_contado() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let entrou = navegador(endereco, vec![[0xAB; 32]]).await;
    assert!(
        entrou.is_err(),
        "o navegador entrou com um hash que não é do servidor"
    );
    let comeco = tokio::time::Instant::now();
    while daemon.recusas_do_navegador() == 0 && comeco.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        daemon.recusas_do_navegador(),
        1,
        "a recusa do navegador não foi contada"
    );
    Ok(())
}
