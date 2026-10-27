//! **Um pedido que o servidor não conseguiu fazer volta como `PedidoRecusado`,
//! com o pedido e a causa**, e não como «entrada na sala recusada».
//!
//! Achado da revisão do lote I-a da F5 (08/10/2026): o `nao_deu` do servidor
//! respondia `AlertReason::VoiceRoomEntryRefused` a toda falha que não era de
//! permissão — trocar para um apelido de outra chave, apagar uma mensagem que
//! já sumiu, renomear, criar, banir —, e o cliente lia «você não entrou na
//! sala». Desde a 1.0 (decisão da orquestradora em 09/10), a recusa diz o
//! ordinal do pedido em `ClientMessage` e uma de três causas.
//!
//! # Por que um par cru
//!
//! O que está sob teste é o motivo que o servidor põe no fio.

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
use seele_proto::control::{AlertReason, CausaDaRecusa, ClientMessage, ServerMessage};
use seele_proto::ids::MessageId;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

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

/// Um par cru que já entrou: a ponta e o fluxo de controle.
struct Par {
    _ligado_a: (seele_enlace::Ponta, seele_enlace::Conexao),
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
}

async fn entrar(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Par> {
    let comum::Ligado {
        ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;

    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: apelido.into(),
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
            proof: chave
                .sign(&comum::prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session { .. } = frame::read::<ServerMessage>(&mut recebe).await? else {
        anyhow::bail!("o servidor não mandou Session");
    };
    Ok(Par {
        _ligado_a: (ponta, conexao),
        envio,
        recebe,
    })
}

/// Manda `pedido` e devolve o motivo do primeiro alerta que voltar.
async fn alerta_depois_de(par: &mut Par, pedido: ClientMessage) -> Option<AlertReason> {
    frame::write(&mut par.envio, &pedido).await.ok()?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match frame::read::<ServerMessage>(&mut par.recebe).await {
                Ok(ServerMessage::Alert { reason, .. }) => return Some(reason),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten()
}

/// O ordinal de um pedido em `ClientMessage`, pela lista do `serde`, e não pela
/// conta que o servidor faz.
fn ordinal_de(nome: &str) -> u32 {
    let nomes = seele_proto::vocabulario::variantes::<ClientMessage>().unwrap();
    u32::try_from(nomes.iter().position(|n| *n == nome).unwrap()).unwrap()
}

/// **O caso da revisão:** trocar para o apelido de outra chave volta como
/// `PedidoRecusado { SetNickname, NomeTomado }`.
#[tokio::test(flavor = "multi_thread")]
async fn o_apelido_de_outra_chave_volta_como_nome_tomado() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let _maya = entrar(endereco, 121, "maya").await?;
    let mut shinji = entrar(endereco, 122, "shinji").await?;

    let motivo = alerta_depois_de(
        &mut shinji,
        ClientMessage::SetNickname {
            name: "maya".into(),
        },
    )
    .await;
    assert_eq!(
        motivo,
        Some(AlertReason::PedidoRecusado {
            pedido: ordinal_de("SetNickname"),
            causa: CausaDaRecusa::NomeTomado,
        })
    );

    daemon.shutdown();
    Ok(())
}

/// E o que já sumiu volta como `NaoExiste`, com o pedido que o pediu.
#[tokio::test(flavor = "multi_thread")]
async fn apagar_o_que_ja_sumiu_volta_como_nao_existe() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    // A primeira conta é Comandante: a recusa não é de permissão.
    let mut anfitria = entrar(endereco, 123, "anfitria").await?;

    let motivo = alerta_depois_de(
        &mut anfitria,
        ClientMessage::RemoveMessage {
            message: MessageId(9_999),
        },
    )
    .await;
    assert_eq!(
        motivo,
        Some(AlertReason::PedidoRecusado {
            pedido: ordinal_de("RemoveMessage"),
            causa: CausaDaRecusa::NaoExiste,
        })
    );

    daemon.shutdown();
    Ok(())
}
