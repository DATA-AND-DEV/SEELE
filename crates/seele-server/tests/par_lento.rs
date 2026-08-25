//! Um par que parou de ler não pode custar mensagem de quem falou.
//!
//! Pendência nº 1 de `docs/pendencias.md`: rajada de mensagens grandes perde
//! entrega. O que esta suíte reproduz é a metade do defeito que ainda existe
//! depois de as duas tarefas leitoras dedicadas terem entrado — a do cliente
//! (`seele-core/src/client.rs`) e a da sessão (`session::run_session`).
//!
//! O caminho é este. A sessão escreve para o par de dentro do mesmo `select!`
//! em que lê o barramento de eventos. Quando o par para de ler, a janela de
//! controle de fluxo do QUIC fecha, a escrita bloqueia, e enquanto ela está
//! bloqueada **ninguém tira evento do barramento**. O barramento é um
//! `broadcast` de capacidade fixa: passado o tamanho do anel, ele descarta o
//! mais antigo e devolve `Lagged` na próxima leitura. Um `let Ok(event) = event
//! else { continue }` transformava isso em nada — a sessão seguia, calada, com
//! um buraco permanente no que aquele pessoa vê.
//!
//! É por isso que a pendência é a número 1: a mensagem foi gravada em PERSISTENCE,
//! quem escreveu viu a sua aparecer, e o outro lado nunca soube que faltou.
//!
//! # Por que um par cru, e não um `Client`
//!
//! `seele-core` não pode ser dependência daqui (ADR 0002), e — mais importante
//! — o `Client` **não consegue mais** encenar o cenário: a tarefa leitora dele
//! drena o fluxo para um canal sem limite, então um cliente que não chama
//! `next_event` continua esvaziando a janela do QUIC. O par que ainda existe no
//! mundo e para de ler é outro: um cliente de terceiro, ou uma casca cuja
//! tarefa travou. Este teste é esse par.

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
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_proto::ids::{ClientMessageId, LineId};
use seele_server::persistence::Location;
use seele_server::{frame, ServerConfig, Daemon};

const LINE: u32 = 1;

/// O tamanho da pendência: grande o bastante para não caber num pacote.
const CORPO: usize = 3_900;

/// Quantas conexões falam ao mesmo tempo.
///
/// Vinte, e não uma, porque o [`seele_server::taxa::Vigia`] limita cada conexão
/// a sessenta quadros de rajada — e com razão. Vinte pessoas falando de uma vez
/// é um servidor cheio numa hora movimentada, que `specs/04-servidor-seele.md`
/// dimensiona em cinquenta pessoas.
const FALANTES: usize = 20;

/// Quantas mensagens cada um manda, dentro da rajada que o Vigia permite.
const CADA: usize = 58;

/// Um verificador que aceita qualquer certificado.
///
/// O que está sob teste é a entrega, não o TOFU — que tem os testes dele em
/// `seele-core/src/tofu.rs`. Aqui o certificado é o que o próprio servidor acabou
/// de gerar nesta função de teste.
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

/// Um par cru: conexão QUIC e o fluxo de controle, sem nenhuma tarefa leitora.
struct Par {
    /// Segura a conexão viva enquanto o teste corre.
    _conexao: quinn::Connection,
    envio: quinn::SendStream,
    recebe: quinn::RecvStream,
}

/// Abre uma conexão e faz o aperto de mão à mão.
///
/// `janela` é a janela de recepção de fluxo que este par anuncia. Passar um
/// valor pequeno não cria o defeito: só encurta o tempo até o par parar de
/// aceitar bytes, que é a condição do cenário. Qualquer par pode escolher a
/// dele, e um cliente de aparelho pequeno escolhe pequena.
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
        _conexao: conexao,
        envio,
        recebe,
    })
}

/// Um par que para de ler não pode custar mensagem de quem falou.
///
/// Sem asserção sobre *quantas* mensagens ele lê depois de voltar: a resposta
/// certa é que ou ele lê todas, ou ele é desligado com um motivo. O que não
/// pode acontecer é a terceira coisa — seguir conectado, calado, com um buraco
/// no meio da conversa que ninguém dos dois lados consegue nomear.
#[tokio::test(flavor = "multi_thread")]
async fn o_server_nao_perde_mensagem_calado_quando_um_par_para_de_ler() -> Result<()> {
    let (endereco, servidor) = server().await?;

    // O ouvinte anuncia uma janela pequena e para de ler no instante seguinte.
    let mut ouvinte = abrir(endereco, 1, 16 * 1024).await?;
    frame::write(
        &mut ouvinte.envio,
        &ClientMessage::JoinLine { line: LineId(LINE) },
    )
    .await?;

    // E agora a sala inteira fala de uma vez.
    let mut falantes = Vec::new();
    for numero in 0..FALANTES {
        let semente = u8::try_from(numero).expect("cabe") + 10;
        falantes.push(abrir(endereco, semente, 1024 * 1024).await?);
    }
    for (indice, falante) in falantes.iter_mut().enumerate() {
        for numero in 0..CADA {
            let corpo = format!("{indice:02}{numero:02}").repeat(CORPO / 4);
            frame::write(
                &mut falante.envio,
                &ClientMessage::SendMessage {
                    line: LineId(LINE),
                    body: corpo,
                    replies_to: None,
                    client_message_id: ClientMessageId(u64::try_from(numero).expect("cabe") + 1),
                },
            )
            .await?;
        }
    }

    let ditas = FALANTES * CADA;

    // Tempo de sobra para o lote gravar (200 ms por volta) e para o barramento
    // rodar inteiro. É aqui que a sessão do ouvinte fica presa escrevendo.
    tokio::time::sleep(Duration::from_secs(3)).await;

    // O ouvinte volta a ler. Conta o que chega até o fluxo secar.
    let mut chegaram = 0_usize;
    let mut desligado = None;
    let prazo = tokio::time::Instant::now() + Duration::from_secs(20);
    while chegaram < ditas && tokio::time::Instant::now() < prazo {
        match tokio::time::timeout(
            Duration::from_secs(2),
            frame::read::<ServerMessage>(&mut ouvinte.recebe),
        )
        .await
        {
            Ok(Ok(ServerMessage::MessageReceived { body, .. })) if body.len() >= CORPO - 4 => {
                chegaram += 1;
            }
            Ok(Ok(ServerMessage::Disconnecting { reason })) => {
                desligado = Some(reason);
                break;
            }
            Ok(Ok(_)) => {}
            Ok(Err(erro)) => {
                println!("o fluxo terminou com {chegaram}/{ditas}: {erro}");
                break;
            }
            Err(_) => break,
        }
    }

    let perdidos = servidor.server().atrasos.eventos();
    println!(
        "ditas {ditas}, chegaram {chegaram}, perdidos no barramento {perdidos}, \
         desligado {desligado:?}"
    );

    // Uma das duas, e nunca a terceira: ou chegou tudo, ou o servidor disse, com
    // esse nome, que ficou faltando. O que ninguém pode aceitar é o par voltar
    // a ler, seguir conectado, e faltar conversa no meio sem uma palavra de
    // nenhum dos dois lados.
    assert!(
        chegaram == ditas || desligado == Some(DisconnectReason::FellBehind),
        "pendência nº 1: {chegaram} de {ditas} chegaram e o servidor não disse nada — \
         {perdidos} eventos morreram no barramento"
    );

    // E o que houve ficou contado. Desligamento sem número por trás seria a
    // mesma cegueira com outra roupa: ninguém sabe se aconteceu uma vez ou mil.
    if desligado.is_some() {
        assert!(
            perdidos > 0,
            "a sessão foi encerrada por ter ficado para trás e o contador marcou zero"
        );
        assert!(servidor.server().atrasos.sessoes() > 0);
    }

    // O que se perdeu foi a **entrega**, e não a mensagem: tudo o que os
    // falantes disseram está em PERSISTENCE, que é o que faz de reconectar um
    // conserto e não um consolo.
    let gravadas = servidor.quantas_mensagens(LineId(LINE)).await?;
    assert_eq!(
        gravadas, ditas as u64,
        "o servidor perdeu mensagem antes de gravar, e aí reconectar não repõe nada"
    );

    servidor.shutdown();
    Ok(())
}
