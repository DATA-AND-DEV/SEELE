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
    let mut depois = frame::read::<ServerMessage>(&mut recebe).await?;
    // Um par que fala 12 recebe as plataformas antes do anúncio; este não as lê.
    if matches!(depois, ServerMessage::PlataformasDosMods { .. }) {
        depois = frame::read::<ServerMessage>(&mut recebe).await?;
    }
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
                plataformas: None,
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

/// Os quadros que o servidor manda depois da prova até o anúncio (inclusive),
/// lidos de um par cru que declara `faixa` no `Hello`, e como a entrada acabou
/// depois do aceite.
async fn quadros_depois_da_prova(
    endereco: SocketAddr,
    semente: u8,
    faixa: seele_proto::version::Faixa,
) -> Result<(Vec<ServerMessage>, Fim)> {
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
            faixa,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
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
    let mut quadros = Vec::new();
    let conjunto = loop {
        let quadro = frame::read::<ServerMessage>(&mut recebe).await?;
        let primeiro = quadros.is_empty();
        match &quadro {
            ServerMessage::ModsExigidos { conjunto, .. } => {
                let conjunto = conjunto.clone();
                quadros.push(quadro);
                break conjunto;
            }
            ServerMessage::PlataformasDosMods { .. } if primeiro => quadros.push(quadro),
            outro => anyhow::bail!("quadro inesperado antes do anúncio: {outro:?}"),
        }
    };
    frame::write(&mut envio, &ClientMessage::AceitarMods { conjunto }).await?;
    let fim = match frame::read::<ServerMessage>(&mut recebe).await? {
        ServerMessage::Session { .. } => Fim::Entrou,
        ServerMessage::Disconnecting { reason } => Fim::Recusada(reason),
        outro => Fim::Outro(format!("{outro:?}")),
    };
    Ok((quadros, fim))
}

/// Dois MODs: um que declara só o app, e um sem a chave (a coluna nula).
async fn servidor_com_dois_mods() -> Result<(SocketAddr, Arc<Daemon>)> {
    let (endereco, daemon) = servidor().await?;
    {
        let banco = daemon.server().persistence.lock().await;
        for (id, hash, plataformas) in [
            (
                "fulano/so-app",
                "a1".repeat(32),
                Some(vec![seele_proto::mods::Plataforma::App]),
            ),
            ("fulano/sem-chave", "b2".repeat(32), None),
        ] {
            enable(
                &banco,
                &EnabledMod {
                    id: id.into(),
                    version: "1.0.0".into(),
                    hash,
                    repo: "https://github.com/seele/exemplo".into(),
                    reach: vec![],
                    server_half: false,
                    api: 8,
                    tema: false,
                    plataformas,
                },
            )?;
        }
    }
    Ok((endereco, daemon))
}

/// **As plataformas viajam logo antes do anúncio** (API 8, parte 2): a lista de
/// cada MOD, pelo hash, na mesma ordem do anúncio; a coluna nula vira as duas.
#[tokio::test(flavor = "multi_thread")]
async fn o_par_doze_recebe_as_plataformas_antes_do_anuncio() -> Result<()> {
    use seele_proto::mods::{Plataforma, PlataformasDoMod};
    let (endereco, daemon) = servidor_com_dois_mods().await?;

    let (quadros, fim) = quadros_depois_da_prova(
        endereco,
        151,
        seele_proto::version::Faixa {
            minima: 10,
            maxima: 12,
        },
    )
    .await?;
    let [ServerMessage::PlataformasDosMods { mods }, ServerMessage::ModsExigidos {
        mods: anunciados, ..
    }] = quadros.as_slice()
    else {
        panic!("depois da prova tinham de vir PlataformasDosMods e ModsExigidos: {quadros:?}");
    };
    let hashes: Vec<_> = anunciados.iter().map(|m| m.hash.clone()).collect();
    assert_eq!(
        mods.iter().map(|m| m.hash.clone()).collect::<Vec<_>>(),
        hashes,
        "as plataformas não vêm pelos mesmos hashes, na mesma ordem do anúncio"
    );
    let de = |hash: &str| -> Vec<Plataforma> {
        mods.iter()
            .find(|m: &&PlataformasDoMod| m.hash == hash)
            .map(|m| m.plataformas.clone())
            .unwrap_or_default()
    };
    assert_eq!(de(&"a1".repeat(32)), vec![Plataforma::App]);
    assert_eq!(
        de(&"b2".repeat(32)),
        vec![Plataforma::App, Plataforma::Navegador],
        "sem a chave, as duas"
    );
    assert_eq!(fim, Fim::Entrou);

    daemon.shutdown();
    Ok(())
}

/// **Um par 11 entra como sempre entrou:** o quadro depois da prova é o anúncio,
/// sem as plataformas (uma variante que ele não conhece perderia o fluxo).
#[tokio::test(flavor = "multi_thread")]
async fn o_par_onze_entra_sem_as_plataformas() -> Result<()> {
    let (endereco, daemon) = servidor_com_dois_mods().await?;

    let (quadros, fim) = quadros_depois_da_prova(
        endereco,
        152,
        seele_proto::version::Faixa {
            minima: 10,
            maxima: 11,
        },
    )
    .await?;
    assert!(
        matches!(quadros.as_slice(), [ServerMessage::ModsExigidos { .. }]),
        "o par 11 recebeu outra coisa antes do anúncio: {quadros:?}"
    );
    assert_eq!(fim, Fim::Entrou, "o aceite do par 11 segue como antes");

    daemon.shutdown();
    Ok(())
}
