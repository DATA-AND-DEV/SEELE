//! **Recusar os MODs não gasta o convite.**
//!
//! Hipótese da F8 (a tela do celular), medida em 09/10/2026: o servidor gastava
//! o convite de uso único na portaria, **antes** de perguntar dos MODs. Quem
//! recusava os MODs — o caso de todo app que ainda não os tinha aceitado: ele
//! recusa, devolve a lista à tela, e entra de novo depois do sim — ficava com o
//! convite gasto, e a volta, **mesmo com o mesmo token**, recebia
//! `CredentialRejected` com a portaria desligada. O caso irmão da P4, que
//! resolveu o mesmo para o apelido tomado.
//!
//! Desde o conserto, o convite é gasto só depois de os MODs serem aceitos: no
//! instante em que a pessoa de fato entra.
//!
//! # O que este arquivo não cobre
//!
//! A volta **sem** o token continua recusada com a portaria desligada: a
//! política de admissão não tem memória, e sem portaria nada registra quem já
//! passou pelo segredo. Fazer o app reenviar o token, ou o servidor lembrar de
//! quem só faltava aceitar os MODs, é outra decisão.
//!
//! # Por que um par cru
//!
//! O que está sob teste é o que o servidor faz com o convite; o par cru recusa
//! e aceita os MODs à mão, como o cliente de verdade faz.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_server::persistence::mods::{enable, EnabledMod};
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

/// Como uma batida terminou.
#[derive(Debug, PartialEq, Eq)]
enum Fim {
    /// Entrou: chegou o `Session`.
    Entrou,
    /// O servidor recusou, antes ou depois dos MODs.
    Recusada(DisconnectReason),
    /// Chegou outro quadro no lugar.
    Outro(String),
}

/// Uma batida inteira: o `Hello` com `convite`, a prova, e então aceita os MODs
/// (com o conjunto anunciado) ou os recusa.
async fn bater(endereco: SocketAddr, semente: u8, convite: &str, aceita: bool) -> Result<Fim> {
    let comum::Ligado {
        ponta: _ponta,
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
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: Some(convite.to_owned()),
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let fim = |quadro: ServerMessage| match quadro {
        ServerMessage::Session { .. } => Fim::Entrou,
        ServerMessage::Disconnecting { reason } => Fim::Recusada(reason),
        outro => Fim::Outro(format!("{outro:?}")),
    };
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
    let depois = frame::read::<ServerMessage>(&mut recebe).await?;
    let ServerMessage::ModsExigidos { conjunto, .. } = depois else {
        return Ok(fim(depois));
    };
    let resposta = if aceita {
        ClientMessage::AceitarMods { conjunto }
    } else {
        ClientMessage::RecusarMods
    };
    frame::write(&mut envio, &resposta).await?;
    Ok(fim(frame::read::<ServerMessage>(&mut recebe).await?))
}

/// Um servidor fechado por convite, com a portaria desligada e um MOD exigido.
/// Devolve o convite.
async fn servidor_fechado() -> Result<(SocketAddr, Arc<Daemon>, String)> {
    let (endereco, daemon) = servidor().await?;
    let convite = {
        let mut banco = daemon.server().persistence.lock().await;
        enable(
            &banco,
            &EnabledMod {
                id: "fulano/exemplo".into(),
                version: "1.0.0".into(),
                hash: "ab".repeat(32),
                repo: "https://github.com/seele/exemplo".into(),
                reach: vec!["ler".into()],
                server_half: false,
                api: 0,
                tema: false,
            },
        )?;
        seele_server::portaria::ligar(&mut banco, false)?;
        seele_server::admissao::criar_convite(&mut banco, "para quem ainda vai aceitar")?
    };
    Ok((endereco, daemon, convite))
}

/// **O caso da F8:** a primeira batida recusa os MODs, e a segunda, com o mesmo
/// convite, os aceita e entra.
#[tokio::test(flavor = "multi_thread")]
async fn recusar_os_mods_nao_gasta_o_convite() -> Result<()> {
    let (endereco, daemon, convite) = servidor_fechado().await?;

    let primeira = bater(endereco, 141, &convite, false).await?;
    assert_eq!(primeira, Fim::Recusada(DisconnectReason::ModsRecusados));

    let segunda = bater(endereco, 141, &convite, true).await?;
    assert_eq!(
        segunda,
        Fim::Entrou,
        "o convite foi gasto por quem só recusou os MODs: a volta com o mesmo \
         token não entra"
    );

    daemon.shutdown();
    Ok(())
}

/// O controle: entrar de fato gasta o convite, e outra chave com o mesmo token
/// não entra mais.
#[tokio::test(flavor = "multi_thread")]
async fn entrar_de_fato_gasta_o_convite() -> Result<()> {
    let (endereco, daemon, convite) = servidor_fechado().await?;

    assert_eq!(bater(endereco, 142, &convite, true).await?, Fim::Entrou);
    assert_eq!(
        bater(endereco, 143, &convite, true).await?,
        Fim::Recusada(DisconnectReason::CredentialRejected),
        "o convite de uso único entrou duas vezes"
    );

    daemon.shutdown();
    Ok(())
}
