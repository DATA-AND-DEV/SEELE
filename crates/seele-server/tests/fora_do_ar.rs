//! **O FORA DO AR: quem entra recebe a lista de membros do servidor, e quem é
//! banido sai dela para todo mundo.**
//!
//! Decisão D-fio-4 de 08/10/2026: a lista de pessoas ganha o grupo FORA DO AR,
//! com quem tem conta aqui e não está na sala. O servidor manda a lista em
//! partes (`ServerMessage::Membros`, com `fim` só na última) no começo da
//! sessão, e avisa quando alguém deixa de ser membro (`MembroBanido`). A
//! tarefa P5 do plano F1b. O `MembroNovo` (quem entra pela primeira vez) entrou
//! depois da 4d da F7, porque mexe no aperto de mão.
//!
//! # Por que um par cru
//!
//! A casca ainda ignora estas mensagens (P6), e o que está sob teste é o que
//! o servidor põe no fio.

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
use seele_proto::control::{ClientMessage, Membro, ServerMessage, MAX_MEMBROS_POR_PARTE};
use seele_proto::ids::PersonId;
use seele_server::permissions::Permissions;
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

/// Um par cru que já entrou: a ponta, o fluxo de controle e a conta.
struct Par {
    _ponta: (seele_enlace::Ponta, seele_enlace::Conexao),
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    person: PersonId,
}

async fn entrar(endereco: SocketAddr, semente: u8) -> Result<Par> {
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
    let ServerMessage::Session { person, .. } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };
    Ok(Par {
        _ponta: (ponta, conexao.clone()),
        envio,
        recebe,
        person,
    })
}

/// Lê quadros até `achou` dizer que um serve, pulando o resto. `None` se nada
/// serviu em cinco segundos.
async fn esperar<T>(par: &mut Par, mut achou: impl FnMut(ServerMessage) -> Option<T>) -> Option<T> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match frame::read::<ServerMessage>(&mut par.recebe).await {
                Ok(mensagem) => {
                    if let Some(valor) = achou(mensagem) {
                        return Some(valor);
                    }
                }
                Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten()
}

/// Todas as partes da lista de membros, até a que diz `fim`.
async fn partes_dos_membros(par: &mut Par) -> Vec<(Vec<Membro>, bool)> {
    let mut partes = Vec::new();
    while let Some(parte) = esperar(par, |mensagem| match mensagem {
        ServerMessage::Membros { membros, fim } => Some((membros, fim)),
        _ => None,
    })
    .await
    {
        let fim = parte.1;
        partes.push(parte);
        if fim {
            break;
        }
    }
    partes
}

fn apelidos(membros: &[Membro]) -> Vec<&str> {
    membros
        .iter()
        .map(|membro| membro.nickname.as_str())
        .collect()
}

/// **O caso da P5:** A e B entram, e B sai. C entra e recebe a lista com os
/// três, numa parte só e com `fim`. A bane B, e C fica sabendo.
#[tokio::test(flavor = "multi_thread")]
async fn quem_entra_recebe_os_membros_e_fica_sabendo_do_banimento() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    // A é a proprietária declarada (ADR 0064): ninguém administra por chegar
    // primeiro.
    {
        let banco = daemon.server().persistence.lock().await;
        seele_server::cargos::declarar_proprietario(
            &banco,
            &seele_proto::transport::key_fingerprint(
                &SigningKey::from_bytes(&[91; 32]).verifying_key().to_bytes(),
            ),
        )?;
    }
    let mut a = entrar(endereco, 91).await?;
    let b = entrar(endereco, 92).await?;
    let pessoa_b = b.person;
    drop(b);

    let mut c = entrar(endereco, 93).await?;
    let partes = partes_dos_membros(&mut c).await;
    assert_eq!(
        partes.len(),
        1,
        "a lista não veio numa parte só: {partes:?}"
    );
    let (membros, fim) = &partes[0];
    assert!(*fim, "a única parte não diz que é a última");
    assert_eq!(
        apelidos(membros),
        ["pessoa091", "pessoa092", "pessoa093"],
        "quem saiu continua membro, e quem entra se vê na lista"
    );

    // A é a proprietária, e pode banir.
    frame::write(
        &mut a.envio,
        &ClientMessage::BanPerson {
            person: pessoa_b,
            reason: None,
            expires_at: None,
        },
    )
    .await?;
    let banido = esperar(&mut c, |mensagem| match mensagem {
        ServerMessage::MembroBanido { person } => Some(person),
        _ => None,
    })
    .await;
    assert_eq!(
        banido,
        Some(pessoa_b),
        "quem está na sala não soube do banimento"
    );

    // O renomear não tem mensagem própria no FORA DO AR: é o `PersonRenamed`,
    // que chega a todo mundo, inclusive a quem não está na mesma sala de voz
    // (aqui, ninguém está em sala nenhuma).
    frame::write(
        &mut a.envio,
        &ClientMessage::SetNickname {
            name: "anfitria".into(),
        },
    )
    .await?;
    let renomeado = esperar(&mut c, |mensagem| match mensagem {
        ServerMessage::PersonRenamed { person, nickname } => Some((person, nickname)),
        _ => None,
    })
    .await;
    assert_eq!(renomeado, Some((a.person, "anfitria".to_owned())));

    // E quem entra depois já não vê o banido, e vê o nome novo.
    let mut d = entrar(endereco, 94).await?;
    let partes = partes_dos_membros(&mut d).await;
    let todos: Vec<&str> = partes
        .iter()
        .flat_map(|(membros, _)| apelidos(membros))
        .collect();
    assert_eq!(todos, ["anfitria", "pessoa093", "pessoa094"]);

    daemon.shutdown();
    Ok(())
}

/// **Quem entra pela primeira vez vira membro para quem já está.** D é conta
/// nova, e A e C recebem `MembroNovo` com o id e o apelido dela. Quando D volta,
/// não sai outro: a conta já existia. A prova do «não sai» é a ordem — depois
/// da volta de D entra E, e o próximo `MembroNovo` que A vê é o de E.
#[tokio::test(flavor = "multi_thread")]
async fn a_conta_nova_vira_membro_para_quem_ja_esta_e_a_volta_nao() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut a = entrar(endereco, 101).await?;
    let mut c = entrar(endereco, 103).await?;

    // A já viu C chegar como conta nova; o que se mede aqui é D.
    let c_para_a = esperar(&mut a, |mensagem| match mensagem {
        ServerMessage::MembroNovo { membro } => Some(membro.id),
        _ => None,
    })
    .await;
    assert_eq!(c_para_a, Some(c.person));

    let d = entrar(endereco, 104).await?;
    let pessoa_d = d.person;
    for (quem, par) in [("A", &mut a), ("C", &mut c)] {
        let novo = esperar(par, |mensagem| match mensagem {
            ServerMessage::MembroNovo { membro } => Some(membro),
            _ => None,
        })
        .await;
        assert_eq!(
            novo,
            Some(Membro {
                id: pessoa_d,
                nickname: "pessoa104".into(),
            }),
            "{quem} não soube da conta nova"
        );
    }

    drop(d);
    let d_de_novo = entrar(endereco, 104).await?;
    assert_eq!(d_de_novo.person, pessoa_d, "a volta criou outra conta");
    let e = entrar(endereco, 105).await?;
    let proximo = esperar(&mut a, |mensagem| match mensagem {
        ServerMessage::MembroNovo { membro } => Some(membro),
        _ => None,
    })
    .await;
    assert_eq!(
        proximo,
        Some(Membro {
            id: e.person,
            nickname: "pessoa105".into(),
        }),
        "a volta de uma conta que já existia saiu como membro novo"
    );

    daemon.shutdown();
    Ok(())
}

/// **As partes:** com 300 membros, duas partes, de 256 e de 44, e o `fim` só
/// na segunda. Quem entra é o trecentésimo.
#[tokio::test(flavor = "multi_thread")]
async fn trezentos_membros_vao_em_duas_partes() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    {
        let guard = daemon.server().persistence.lock().await;
        let permissions = Permissions::new(&guard);
        for n in 0..299_u16 {
            let mut chave = [0xA0_u8; 32];
            chave[..2].copy_from_slice(&n.to_be_bytes());
            permissions.register_or_find(&chave, &format!("semeada{n:03}"))?;
        }
    }

    let mut par = entrar(endereco, 95).await?;
    let partes = partes_dos_membros(&mut par).await;
    let tamanhos: Vec<(usize, bool)> = partes
        .iter()
        .map(|(membros, fim)| (membros.len(), *fim))
        .collect();
    assert_eq!(tamanhos, [(MAX_MEMBROS_POR_PARTE, false), (44, true)]);

    daemon.shutdown();
    Ok(())
}
