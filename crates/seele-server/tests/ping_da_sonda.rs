//! **O `Ping` da sonda é respondido na hora, inteiro, mesmo com volume na
//! mesma conexão** (pedido do B, 0.17).
//!
//! Na 0.17 o enlace não mede perda nem RTT do SCTP, e a sonda de subida do
//! cliente (`seele-core::caminho`) passou a medir a dor pela ida e volta de um
//! `Ping` no `ctl`, a cada 500 ms enquanto a tela transmite, com o carimbo
//! `1 << 61`. Ela só funciona se o servidor:
//!
//! - responder **todo** `Ping` com o mesmo `timestamp` (nenhum descartado);
//! - não o limitar abaixo do ritmo da sonda (o `taxa::Vigia` aceita 20 quadros
//!   por segundo, e aqui vão quatro);
//! - responder enquanto um anexo grande sobe por um canal "u" da mesma
//!   conexão: o `ctl` sai na frente do volume no escalonador do enlace.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::attachment::AttachmentHeader;
use seele_proto::control::{AlertReason, ClientMessage, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId};
use seele_proto::stream::StreamType;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

/// O carimbo da sonda do cliente (`seele_core::client::MARCA_DO_PING_DA_SONDA`).
const MARCA_DO_PING_DA_SONDA: u64 = 1 << 61;
/// Quantos `Ping` vão durante o anexo.
const PINGS: u64 = 20;
/// O ritmo: o dobro do da sonda.
const ENTRE_PINGS: Duration = Duration::from_millis(250);
/// O tamanho do anexo que sobe ao mesmo tempo.
const ANEXO: usize = 8 * 1024 * 1024;

async fn servidor(pasta: &std::path::Path) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(pasta.join("seele.db")),
        anexos: Some(pasta.join("anexos")),
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

async fn entrar(endereco: SocketAddr) -> Result<comum::Ligado> {
    let mut ligado = comum::ligar(endereco).await?;
    let chave = SigningKey::from_bytes(&[61; 32]);
    frame::write(
        &mut ligado.envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: "sonda".into(),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let ServerMessage::Challenge { nonce } =
        frame::read::<ServerMessage>(&mut ligado.recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut ligado.envio,
        &ClientMessage::Response {
            proof: chave
                .sign(&comum::prova_amarrada(&ligado.conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    loop {
        if let ServerMessage::Session { .. } =
            frame::read::<ServerMessage>(&mut ligado.recebe).await?
        {
            return Ok(ligado);
        }
    }
}

/// O cabeçalho de um anexo de [`ANEXO`] bytes, com o hash errado de propósito:
/// o servidor lê tudo e recusa no fim, e o que interessa aqui é o volume.
fn cabecalho() -> Vec<u8> {
    let header = AttachmentHeader {
        channel: ChannelId(1),
        client_message_id: ClientMessageId(7),
        body: String::new(),
        replies_to: None,
        file_name: "grande.bin".into(),
        declared_type: "application/octet-stream".into(),
        declared_len: ANEXO as u64,
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

#[tokio::test(flavor = "multi_thread")]
async fn todo_ping_da_sonda_volta_sem_limite_mesmo_com_um_anexo_subindo() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let (endereco, daemon) = servidor(pasta.path()).await?;
    let comum::Ligado {
        ponta: _ponta,
        conexao,
        mut envio,
        mut recebe,
    } = entrar(endereco).await?;

    // O anexo sobe ao mesmo tempo, num "u" da mesma conexão.
    let subindo = {
        let conexao = conexao.clone();
        tokio::spawn(async move {
            let mut fluxo = conexao.open_uni().await?;
            fluxo.write_all(&[StreamType::Attachment.byte()]).await?;
            fluxo.write_all(&cabecalho()).await?;
            let bloco = vec![0x5A_u8; 64 * 1024];
            for _ in 0..ANEXO / bloco.len() {
                fluxo.write_all(&bloco).await?;
            }
            fluxo.finish()?;
            anyhow::Ok(())
        })
    };

    // A leitura do ctl numa tarefa: o Pong e qualquer alerta de taxa.
    let (pongs_tx, mut pongs_rx) = tokio::sync::mpsc::unbounded_channel();
    let lendo = tokio::spawn(async move {
        let mut limitado = false;
        while let Ok(mensagem) = frame::read::<ServerMessage>(&mut recebe).await {
            match mensagem {
                ServerMessage::Pong { timestamp } => {
                    let _ = pongs_tx.send((timestamp, Instant::now()));
                }
                ServerMessage::Alert {
                    reason: AlertReason::RateLimited,
                    ..
                } => limitado = true,
                _ => {}
            }
        }
        limitado
    });

    let mut enviados = Vec::new();
    for n in 0..PINGS {
        let timestamp = MARCA_DO_PING_DA_SONDA | n;
        frame::write(&mut envio, &ClientMessage::Ping { timestamp }).await?;
        enviados.push((timestamp, Instant::now()));
        tokio::time::sleep(ENTRE_PINGS).await;
    }

    let mut voltaram = std::collections::HashMap::new();
    let ate = tokio::time::Instant::now() + Duration::from_secs(10);
    while voltaram.len() < enviados.len() {
        match tokio::time::timeout_at(ate, pongs_rx.recv()).await {
            Ok(Some((timestamp, quando))) => {
                voltaram.insert(timestamp, quando);
            }
            _ => break,
        }
    }
    let mut pior = Duration::ZERO;
    for (timestamp, saiu) in &enviados {
        let voltou = voltaram
            .get(timestamp)
            .unwrap_or_else(|| panic!("o Ping {timestamp:#x} da sonda não voltou"));
        pior = pior.max(voltou.saturating_duration_since(*saiu));
    }
    println!("{} pings, o pior ida e volta foi {pior:?}", enviados.len());
    assert!(
        pior < Duration::from_secs(2),
        "o Pong da sonda atrasou {pior:?} com o anexo subindo: o ctl esperou atrás do volume"
    );

    let _ = subindo.await;
    conexao.close(0, b"fim");
    let limitado = lendo.await?;
    assert!(
        !limitado,
        "o servidor limitou os Pings da sonda (RateLimited)"
    );
    daemon.shutdown();
    Ok(())
}
