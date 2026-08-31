//! A voz de quem fala não pode parar porque o controle dele travou.
//!
//! # O defeito
//!
//! `session::run_session` tem **um** `tokio::select!` e dentro dele convivem
//! duas coisas que não têm nada a ver uma com a outra:
//!
//! - o **plano de controle** — quadros do par, o barramento de eventos, o tique
//!   de telemetria. Os tratadores dele tomam `server.persistence.lock().await`
//!   cerca de quarenta vezes, num mutex de SQLite que é **um só para todas as
//!   sessões**, e escrevem no fluxo do par com `frame::write(...).await`, que
//!   bloqueia quando o par para de ler;
//! - o **plano de mídia** — `connection.read_datagram()` num braço e
//!   `outbound_rx.recv()` no outro.
//!
//! O `select!` roda o corpo do braço que ganhou **até o fim** antes de voltar ao
//! topo. Enquanto um tratador de controle espera o mutex do banco, ou espera uma
//! escrita presa num par que parou de ler, essa sessão **não lê e não escreve
//! voz**. O disco de quem hospeda, ou um par lento qualquer, escolhe quando a
//! conversa pica.
//!
//! É a terceira aparição desta forma no projeto: a pendência nº 1 é ela no
//! barramento de eventos, e o `MutexGuard` segurado através de um `.await` que o
//! commit `cb45bc1` consertou é ela na subida medida. As duas foram tratadas
//! como acidente. Esta suíte diz que é o desenho.
//!
//! # Por que um par cru, e não um `Client`
//!
//! Mesma razão de `par_lento.rs`: `seele-core` não pode ser dependência daqui
//! (ADR 0002), e o `Client` tem tarefa leitora dedicada — ele **não consegue**
//! encenar um par que parou de ler. O par que para de ler é este.

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
use seele_proto::ids::{ChannelId, ClientMessageId, Ssrc, VoiceRoomId};
use seele_proto::media::MediaHeader;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

/// O canal de texto semeado pela migração 1.
const CANAL: u32 = 1;

/// Corpo grande o bastante para não caber num pacote, como na pendência nº 1.
const CORPO: usize = 3_900;

/// Quantas conexões enchem o fluxo de controle do falante.
const AFOGADORES: usize = 8;

/// Quantas mensagens cada uma manda.
///
/// Oito vezes seis corpos de 3,9 KB são ~187 KB contra uma janela de 16 KB: o
/// fluxo do falante fecha bem antes do fim, que é a condição do cenário.
const CADA: usize = 6;

/// Quantos quadros de voz o falante manda enquanto o controle dele está preso.
const QUADROS: usize = 50;

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

/// A voz atravessa enquanto o plano de controle do falante está preso.
///
/// O cenário é o de todo dia, e não um caso de laboratório: alguém numa sala de
/// voz cuja casca parou de drenar o fluxo de controle — uma janela recarregando,
/// uma tarefa travada, um cliente de terceiro — enquanto continua falando. O
/// microfone dele não sabe de nada disso e segue mandando cinquenta quadros por
/// segundo.
///
/// O que se cobra aqui é **só** que a voz atravesse. Não se cobra que o controle
/// dele continue: um par que parou de ler tem o tratamento dele na pendência nº
/// 1, e é outro assunto. A afirmação é que os dois planos são independentes, e
/// que o de mídia não paga o preço do outro.
#[tokio::test(flavor = "multi_thread")]
async fn a_voz_atravessa_enquanto_o_controle_do_falante_esta_preso() -> Result<()> {
    let (endereco, servidor) = server().await?;

    // Quem escuta: janela larga, e uma tarefa que drena o controle dele para que
    // a sessão dele nunca seja a que trava. O defeito sob teste é o do falante.
    let ouvinte = abrir(endereco, 1, 1024 * 1024).await?;
    let sala = ouvinte.sala;
    let mut ouvinte_envio = ouvinte.envio;
    let mut ouvinte_recebe = ouvinte.recebe;
    let ouvinte_conexao = ouvinte.conexao;
    frame::write(
        &mut ouvinte_envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;
    tokio::spawn(async move {
        while frame::read::<ServerMessage>(&mut ouvinte_recebe)
            .await
            .is_ok()
        {}
    });

    // Quem fala: janela de 16 KB, e para de ler o controle logo depois de entrar.
    let mut falante = abrir(endereco, 2, 16 * 1024).await?;
    frame::write(
        &mut falante.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;
    // Inscrito no canal de texto: é por aqui que o afogamento chega até ele.
    frame::write(
        &mut falante.envio,
        &ClientMessage::JoinChannel {
            channel: ChannelId(CANAL),
        },
    )
    .await?;
    // As duas entradas precisam ter sido processadas antes de o fluxo fechar.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Agora a sala de texto enche, e o fluxo de controle do falante fecha: ele
    // não lê, a janela de 16 KB se esgota, e a escrita do servidor para ele
    // bloqueia dentro do `select!` da sessão dele.
    let mut afogadores = Vec::new();
    for numero in 0..AFOGADORES {
        let semente = u8::try_from(numero).expect("cabe") + 10;
        let mut par = abrir(endereco, semente, 1024 * 1024).await?;
        frame::write(
            &mut par.envio,
            &ClientMessage::JoinChannel {
                channel: ChannelId(CANAL),
            },
        )
        .await?;
        afogadores.push(par);
    }
    for (indice, par) in afogadores.iter_mut().enumerate() {
        for numero in 0..CADA {
            let corpo = format!("{indice:02}{numero:02}").repeat(CORPO / 4);
            frame::write(
                &mut par.envio,
                &ClientMessage::SendMessage {
                    channel: ChannelId(CANAL),
                    body: corpo,
                    replies_to: None,
                    client_message_id: ClientMessageId(u64::try_from(numero).expect("cabe") + 1),
                },
            )
            .await?;
        }
    }

    // Tempo para o barramento rodar e a escrita para o falante encostar na
    // janela fechada. Daqui em diante a sessão dele está presa no braço de
    // controle — e é exatamente agora que ele fala.
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Quem escuta conta o que chega, em paralelo com quem fala.
    let contagem = tokio::spawn(async move {
        let mut chegaram = 0_usize;
        let prazo = tokio::time::Instant::now() + Duration::from_secs(4);
        while tokio::time::Instant::now() < prazo {
            match tokio::time::timeout(Duration::from_millis(500), ouvinte_conexao.read_datagram())
                .await
            {
                Ok(Ok(bytes)) => {
                    if MediaHeader::decode(&bytes).is_ok() {
                        chegaram += 1;
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => {}
            }
        }
        chegaram
    });

    // Cinquenta quadros no ritmo de um microfone de verdade: 20 ms entre um e o
    // seguinte, que é um segundo de fala e fica sob o teto de sessenta por
    // segundo que o `taxa::Vigia` aplica.
    for seq in 0..QUADROS {
        let _ = falante
            .conexao
            .send_datagram(quadro(falante.ssrc, u16::try_from(seq).expect("cabe")).into());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let chegaram = contagem.await?;
    println!("mandados {QUADROS}, chegaram {chegaram}");

    // O piso é generoso de propósito. Não se cobra entrega de datagrama — o QUIC
    // não a promete, e o `send_datagram` recusa o que não cabe no caminho. O que
    // se cobra é que a voz **não pare**: com o defeito no lugar o número é zero,
    // ou perto disso, porque a sessão do falante nunca chega a olhar o braço que
    // lê datagrama enquanto está presa escrevendo no braço que não pode escrever.
    assert!(
        chegaram >= QUADROS / 2,
        "só {chegaram} de {QUADROS} quadros de voz atravessaram enquanto o plano de \
         controle do falante estava preso. Os dois planos dividem um `select!` em \
         `session::run_session`, então uma escrita de controle bloqueada — ou o mutex \
         do SQLite — para a voz de quem fala."
    );

    servidor.shutdown();
    Ok(())
}
