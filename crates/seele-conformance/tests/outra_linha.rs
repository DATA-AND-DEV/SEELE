//! **Um servidor de outro número de protocolo é dito como tal, e não como rede
//! fora do ar.**
//!
//! O byte 0 de todo quadro é o número do protocolo (ADR 0058). Até a 0.15, o
//! cliente decodificava a primeira resposta inteira, o `decode` recusava o
//! carimbo de fora, e o erro virava `ConnectError::Unreachable`: a tela dizia
//! «não foi possível alcançar o servidor» de um servidor que tinha respondido.
//! Desde a P8 do plano F1b, o cliente lê a primeira resposta bruta, confere o
//! byte 0 antes de qualquer outra coisa, e um número que esta build não fala
//! vira `ConnectError::OutraLinha { numero }`.
//!
//! # Por que um servidor cru
//!
//! Nenhum servidor desta build responde com outro carimbo. Os bytes vêm à mão:
//! `[8, 11, 9]` é o que a v0.15.1 publicada responde a um `Hello` carimbado 9
//! (o `decode` dela recusa o carimbo e ela despede com `ProtocolViolation` no
//! número dela, pelo código de `54f542f`); `[30, 11, 0]` é uma build de um
//! número muito à frente recusando por versão.

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
use ed25519_dalek::SigningKey;
use seele_core::{frame, Client, ConnectError, MemoryPinStore};
use seele_proto::control::ServerMessage;
use seele_proto::version::{PROTOCOL_VERSION, VERSAO_MINIMA};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// Um servidor que lê o primeiro quadro do cliente e responde `resposta`,
/// enquadrada, sem passar por `encode` nenhum. A tarefa devolve o byte 0 do
/// quadro que o cliente mandou.
async fn servidor_que_responde(
    resposta: Vec<u8>,
) -> Result<(SocketAddr, tokio::task::JoinHandle<Result<u8>>)> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let identidade = seele_server::tls::Identity::self_signed(vec!["localhost".to_owned()])?;
    let ponta = quinn::Endpoint::server(
        seele_server::tls::server_config(identidade)?,
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )?;
    let endereco = ponta.local_addr()?;
    let tarefa = tokio::spawn(async move {
        let conexao = ponta
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("ninguém bateu"))?
            .await?;
        let (mut envia, mut recebe) = conexao.accept_bi().await?;
        let mut prefixo = [0_u8; seele_proto::quadro::PREFIXO_LEN];
        recebe.read_exact(&mut prefixo).await?;
        let mut quadro = vec![0_u8; seele_proto::quadro::comprimento(prefixo)?];
        recebe.read_exact(&mut quadro).await?;
        envia
            .write_all(&seele_proto::quadro::enquadrar(&resposta)?)
            .await?;
        let _ = envia.finish();
        // O tempo de o cliente ler antes de a ponta cair.
        let _ = tokio::time::timeout(Duration::from_secs(2), conexao.closed()).await;
        drop(ponta);
        Ok(quadro[0])
    });
    Ok((endereco, tarefa))
}

async fn conectar(endereco: SocketAddr, semente: u8) -> Result<Client, ConnectError> {
    Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        &format!("pessoa{semente:03}"),
        &SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await
}

/// O que um cliente desta build diz de uma resposta carimbada como `resposta`,
/// e com que carimbo ele mandou o `Hello`.
async fn o_cliente_diz(resposta: Vec<u8>, semente: u8) -> Result<(ConnectError, u8)> {
    let (endereco, servidor) = servidor_que_responde(resposta).await?;
    let erro =
        match tokio::time::timeout(Duration::from_secs(10), conectar(endereco, semente)).await? {
            Ok(_) => anyhow::bail!("o cliente entrou num servidor que só recusou"),
            Err(erro) => erro,
        };
    let carimbo_do_hello = servidor.await??;
    Ok((erro, carimbo_do_hello))
}

/// **O servidor 0.15.1 publicado.** Ele responde ao `Hello` carimbado 9 com um
/// `Disconnecting { ProtocolViolation }` no número dele, o 8.
#[tokio::test(flavor = "multi_thread")]
async fn a_recusa_de_um_servidor_0_15_e_dita_como_outra_linha() -> Result<()> {
    let _vaga = vaga::minha();
    let (erro, carimbo_do_hello) = o_cliente_diz(vec![8, 11, 9], 151).await?;
    assert_eq!(erro, ConnectError::OutraLinha { numero: 8 });
    assert_eq!(
        carimbo_do_hello, VERSAO_MINIMA,
        "o Hello não saiu carimbado com a mínima desta build"
    );
    Ok(())
}

/// **Um número muito à frente**, recusando por versão no número dele.
#[tokio::test(flavor = "multi_thread")]
async fn a_recusa_de_um_numero_a_frente_e_dita_como_outra_linha() -> Result<()> {
    let _vaga = vaga::minha();
    let (erro, _) = o_cliente_diz(vec![30, 11, 0], 152).await?;
    assert_eq!(erro, ConnectError::OutraLinha { numero: 30 });
    Ok(())
}

/// O caminho de sempre continua: contra um servidor desta build, o cliente
/// entra.
#[tokio::test(flavor = "multi_thread")]
async fn contra_um_servidor_desta_build_o_cliente_entra() -> Result<()> {
    let _vaga = vaga::minha();
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

    let cliente = conectar(endereco, 153).await;
    assert!(cliente.is_ok(), "o cliente não entrou: {:?}", cliente.err());

    daemon.shutdown();
    Ok(())
}

/// Um servidor que entra com o cliente (`Challenge`, `Session`) e então manda
/// um quadro de um vocabulário mais novo, o ordinal 100 com o carimbo certo,
/// seguido de um `Pong`. A tarefa devolve o byte 0 de cada quadro que o
/// cliente mandou: o `Hello`, a `Response` e o `Ping` da sessão.
async fn servidor_com_uma_desconhecida(
) -> Result<(SocketAddr, tokio::task::JoinHandle<Result<Vec<u8>>>)> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let identidade = seele_server::tls::Identity::self_signed(vec!["localhost".to_owned()])?;
    let ponta = quinn::Endpoint::server(
        seele_server::tls::server_config(identidade)?,
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )?;
    let endereco = ponta.local_addr()?;
    let tarefa = tokio::spawn(async move {
        let conexao = ponta
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("ninguém bateu"))?
            .await?;
        let (mut envia, mut recebe) = conexao.accept_bi().await?;
        let mut carimbos = Vec::new();
        carimbos.push(frame::ler_bruto(&mut recebe).await?[0]);
        frame::write(&mut envia, &ServerMessage::Challenge { nonce: vec![7; 32] }).await?;
        carimbos.push(frame::ler_bruto(&mut recebe).await?[0]);
        frame::write(
            &mut envia,
            &ServerMessage::Session {
                id: seele_proto::ids::SessionId(1),
                person: seele_proto::ids::PersonId(1),
                ssrc: seele_proto::ids::Ssrc(1),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: Vec::new(),
            },
        )
        .await?;
        envia
            .write_all(&seele_proto::quadro::enquadrar(&[PROTOCOL_VERSION, 100])?)
            .await?;
        frame::write(&mut envia, &ServerMessage::Pong { timestamp: 42 }).await?;
        carimbos.push(frame::ler_bruto(&mut recebe).await?[0]);
        Ok(carimbos)
    });
    Ok((endereco, tarefa))
}

/// **Uma mensagem que esta build não conhece é pulada, e a sessão segue**
/// (adendo de 08/10/2026, §6, o lado do cliente).
///
/// Até a 0.15, o `decode` recusava o ordinal desconhecido, a tarefa que lê o
/// fluxo de controle terminava, e a sessão parava de receber tudo, sem dizer
/// por quê. Agora o quadro é pulado e contado, e o `Pong` que vem atrás dele
/// chega. E toda escrita do cliente sai carimbada: o `Hello` com a mínima
/// desta build, e o resto com o número que o servidor carimbou.
#[tokio::test(flavor = "multi_thread")]
async fn uma_mensagem_que_esta_build_nao_conhece_e_pulada_e_a_sessao_segue() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor_com_uma_desconhecida().await?;
    let mut cliente = conectar(endereco, 154)
        .await
        .map_err(|erro| anyhow::anyhow!("o cliente não entrou: {erro:?}"))?;

    let depois = tokio::time::timeout(Duration::from_secs(5), cliente.next_event()).await??;
    assert_eq!(
        depois,
        ServerMessage::Pong { timestamp: 42 },
        "o que veio depois da desconhecida não chegou"
    );

    cliente.send_ping().await?;
    let carimbos = tokio::time::timeout(Duration::from_secs(5), servidor).await???;
    assert_eq!(
        carimbos,
        vec![VERSAO_MINIMA, PROTOCOL_VERSION, PROTOCOL_VERSION],
        "o Hello sai com a mínima, e o resto com o número que o servidor carimbou"
    );
    Ok(())
}
