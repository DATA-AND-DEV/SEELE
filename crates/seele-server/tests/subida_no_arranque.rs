//! A medida da subida atravessa do `quinn` até o portão e até o disco.
//!
//! # A lacuna que este teste fecha
//!
//! Três ondas puseram a subida medida para trabalhar, e cada peça tem prova
//! isolada: a `SondaDaSubida` aprende com a janela que o cano encheu, a
//! `VoiceRoom` reage ao número partilhado, e `persistence::subida` guarda e
//! devolve. **Nenhuma delas prova a costura** — que num servidor de verdade,
//! com conexões de verdade, os contadores do `quinn` viram uma medida, e que
//! essa medida sai do tique de telemetria e chega aos dois destinos.
//!
//! É a diferença que o CLAUDE.md deste repositório chama de «existir não é
//! funcionar»: cada unidade passava, e a junção não tinha ninguém olhando.
//!
//! # Por que a conversa, e não uma transmissão de tela
//!
//! Porque é o caso que a onda existe para servir. Antes dela, a sonda só
//! aprendia enquanto a tela transmitia: `permitido_bps` é zero sem
//! transmissão, `cheia` é falso, e toda janela morria. A primeira tela de uma
//! call abria na hipótese de 2 Mbps — e o portão de admissão, que divide esse
//! número por N, recusava a partir do sétimo espectador enquanto isso.
//!
//! Aqui **ninguém transmite tela nenhuma**. O que enche o cano é a sala
//! conversando, e o braço do piso demonstrado é o único caminho pelo qual a
//! estimativa pode sair da hipótese. Se ele não estiver ligado de ponta a
//! ponta, este teste reprova.

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

/// Um quadro do maior tamanho que o protocolo aceita.
///
/// **Grande de propósito, e não realista de propósito.** O que este teste
/// precisa é que a subida do servidor passe de `CAMINHO_DO_SERVER_BPS` dentro
/// de uma janela de um segundo, e o teto de `MAX_FRAMES_PER_SECOND` limita
/// quantos quadros cabem ali. Com o payload no máximo, quatro pares bastam;
/// com o payload de um Opus a 48 kbps seriam dezenas de conexões para provar a
/// mesma costura. O servidor não decodifica nada — `specs/08-seguranca.md`
/// proíbe tocar no payload —, então o conteúdo não importa e só o tamanho faz
/// diferença aqui.
const PAYLOAD: [u8; seele_proto::media::MAX_PAYLOAD_LEN] = [7; seele_proto::media::MAX_PAYLOAD_LEN];

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
    servidor_com(Location::Memory).await
}

/// O mesmo servidor, sobre o banco que se pedir.
///
/// Um arquivo, e não memória, é o que permite o segundo arranque ler o que o
/// primeiro escreveu — que é a afirmação inteira de `partindo_de`.
async fn servidor_com(database: Location) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database,
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
///
/// `recebe` não é lido aqui — este teste não espera resposta nenhuma do
/// servidor, ele olha o estado dele por dentro. A ponta fica no `struct` assim
/// mesmo porque largá-la fecharia o fluxo de controle, e um par sem controle é
/// um par que o servidor derruba no meio da conversa.
#[allow(dead_code, reason = "a ponta é segurada, não lida — ver o doc acima")]
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

/// A conversa ensina o cano, e o número chega ao portão e ao disco.
///
/// # O que cada asserção prende
///
/// **A primeira** é a costura de leitura: os contadores por conexão do `quinn`,
/// somados na `Subida`, atravessaram uma janela cheia e moveram a estimativa
/// acima da hipótese. Sem o braço do piso demonstrado ela não sai de
/// `CAMINHO_DO_SERVER_BPS`, porque `permitido_bps` é zero sem tela.
///
/// **A segunda** é a costura de escrita: o mesmo número está no banco, pronto
/// para o arranque seguinte não tatear de novo.
///
/// **E o portão?** `session.rs` avisa as salas e grava no disco **no mesmo
/// bloco**, sob o mesmo `if let Some(bps)`. Provar que o disco recebeu prova
/// que o bloco correu com um número de verdade; que a sala reage a ele é o que
/// `a_sala_que_nasce_depois_da_medida_nasce_com_ela` afirma, em unidade. As
/// duas juntas fecham a corrente, e esta linha existe para dizer qual metade é
/// de quem — em vez de este teste fingir provar as duas.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversa_ensina_o_cano_e_a_medida_chega_ao_portao_e_ao_disco() -> Result<()> {
    let (endereco, servidor) = server().await?;

    // Quatro pares na mesma sala. O servidor copia o quadro de cada um para os
    // outros três, então o que sai desta máquina é doze vezes o que entra — é
    // essa multiplicação que enche o cano, e é a mesma do §5.1.
    let mut pares = Vec::new();
    for semente in 1..=4_u8 {
        let mut par = abrir(endereco, semente, 1024 * 1024).await?;
        let sala = par.sala;
        frame::write(
            &mut par.envio,
            &ClientMessage::EnterVoiceRoom {
                voice_room: sala,
                password: None,
            },
        )
        .await?;
        pares.push(par);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Vinte milissegundos entre um quadro e o seguinte: o ritmo de um microfone
    // de verdade, sob o teto que o `taxa::Vigia` aplica. Três segundos são
    // janela mais que suficiente — a da `Subida` dura um.
    let mut falando = Vec::new();
    for par in pares {
        falando.push(tokio::spawn(async move {
            for seq in 0..150_u16 {
                let _ = par.conexao.send_datagram(quadro(par.ssrc, seq).into());
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            par
        }));
    }
    let mut pares = Vec::new();
    for quem in falando {
        pares.push(quem.await?);
    }

    let medida = servidor.server().subida.lock().await.medida();
    println!("subida medida: {medida:?}");
    let medida = medida.expect(
        "a sala inteira conversou por três segundos e o servidor continua sem medida nenhuma: \
         a janela cheia por voz não chegou à sonda",
    );
    assert!(
        medida > seele_server::tela::CAMINHO_DO_SERVER_BPS,
        "a conversa encheu o cano e a estimativa ficou na hipótese de {} bps (mediu {medida}): \
         a primeira tela desta call ainda vai abrir tateando",
        seele_server::tela::CAMINHO_DO_SERVER_BPS
    );

    let guardada = {
        let banco = servidor.server().persistence.lock().await;
        seele_server::persistence::subida::lembrada(&banco)?
    };
    assert_eq!(
        guardada,
        Some(medida),
        "a subida foi medida e não foi guardada: o próximo arranque recomeça na hipótese"
    );

    // Sem isto os pares caem antes do servidor e o desligamento vira corrida.
    drop(pares);
    servidor.shutdown();
    Ok(())
}

/// O segundo arranque começa onde o primeiro parou.
///
/// # A afirmação que faltava
///
/// `persistence::subida` prova a ida e a volta num banco **em memória**, e o
/// teste acima prova que a medida chega ao disco — dentro de um arranque só.
/// Nenhum dos dois prova a frase que a onda vende: *«a medida sobrevive ao
/// reinício»*. Entre eles cabia um servidor que grava direito, fecha, sobe de novo
/// e ignora o que gravou — e o sintoma seria o tateio de catorze segundos
/// voltando em silêncio, na primeira tela de todo arranque, que é exatamente o
/// defeito que ninguém relata porque parece normal.
///
/// Aqui o banco é um arquivo, o primeiro servidor morre, e o segundo nasce sobre o
/// mesmo arquivo.
#[tokio::test(flavor = "multi_thread")]
async fn o_segundo_arranque_comeca_onde_o_primeiro_parou() -> Result<()> {
    let diretorio = tempfile::tempdir()?;
    let banco = diretorio.path().join("seele.db");

    // Primeiro arranque: a conversa ensina o cano.
    let medida = {
        let (endereco, servidor) = servidor_com(Location::File(banco.clone())).await?;
        let mut pares = Vec::new();
        for semente in 1..=4_u8 {
            let mut par = abrir(endereco, semente, 1024 * 1024).await?;
            let sala = par.sala;
            frame::write(
                &mut par.envio,
                &ClientMessage::EnterVoiceRoom {
                    voice_room: sala,
                    password: None,
                },
            )
            .await?;
            pares.push(par);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;

        let mut falando = Vec::new();
        for par in pares {
            falando.push(tokio::spawn(async move {
                for seq in 0..150_u16 {
                    let _ = par.conexao.send_datagram(quadro(par.ssrc, seq).into());
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                par
            }));
        }
        let mut pares = Vec::new();
        for quem in falando {
            pares.push(quem.await?);
        }

        let medida = servidor
            .server()
            .subida
            .lock()
            .await
            .medida()
            .expect("o primeiro arranque não mediu nada");
        drop(pares);
        servidor.shutdown();
        servidor.wait_idle().await;
        medida
    };
    println!("o primeiro arranque mediu {medida} bps");

    // Segundo arranque, sobre o mesmo arquivo, sem ninguém conversando.
    let (_, servidor) = servidor_com(Location::File(banco)).await?;
    let ao_nascer = servidor.server().subida.lock().await.medida();
    assert_eq!(
        ao_nascer,
        Some(medida),
        "o servidor reabriu o mesmo banco e nasceu sem medida nenhuma: a primeira \
         tela deste arranque vai tatear de novo o que ontem já se sabia"
    );
    servidor.shutdown();
    Ok(())
}
