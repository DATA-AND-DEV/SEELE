//! A limitação de taxa, ligada nas duas pontas.
//!
//! `crates/seele-server/src/taxa.rs` prova o mecanismo — o balde, a portaria, o
//! vigia — com o tempo entrado por parâmetro e sem dormir uma vez. O que ele
//! não pode provar é que o mecanismo está **ligado**: um balde perfeito que
//! ninguém consulta é exatamente o estado em que este repositório estava, com
//! `DisconnectReason::RateLimited` no protocolo e em lugar nenhum do servidor.
//!
//! Então isto passa pelo fio de verdade, com o cliente de verdade, e cobra as
//! duas coisas que uma pessoa percebe: **o servidor avisa antes de derrubar**, e
//! **quem não para, sai**.
//!
//! Os dois testes rodam em `current_thread` de propósito. Um `select!` com
//! várias tarefas num executor de várias linhas esconde defeito: já aconteceu
//! aqui de um teste passar dez vezes em dez com a função sob teste trocada por
//! um `drop`.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::{Client, ConnectError, MemoryPinStore};
use seele_proto::control::{AlertReason, DisconnectReason, ServerMessage};
use seele_server::persistence::Location;
use seele_server::taxa::{APERTOS_DE_RAJADA, PACIENCIA, QUADROS_DE_RAJADA};
use seele_server::{Daemon, ServerConfig};

async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
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

async fn entrar(endereco: SocketAddr) -> Result<Client, ConnectError> {
    Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "marcela",
        &ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await
}

/// Bater à porta em laço acaba em recusa, com o motivo certo.
///
/// É o balde que importa para expor um servidor à internet: o aperto de mão
/// custa Argon2id quando o servidor tem senha, e sem isto cada pacote de quem
/// varre a rede compra milissegundos de CPU do anfitrião.
#[tokio::test(flavor = "current_thread")]
async fn quem_bate_a_porta_em_laco_e_recusado_por_taxa() -> Result<()> {
    let (endereco, servidor) = server().await?;

    // A rajada inteira passa. Uma casa atrás de um NAT, com três clientes
    // gastando a bateria interna depois de o roteador reiniciar, chega perto
    // disto — e não pode ser tratada como ataque.
    for numero in 0..APERTOS_DE_RAJADA {
        let cliente = entrar(endereco).await;
        assert!(
            cliente.is_ok(),
            "a tentativa honesta {numero} foi tratada como ataque: {:?}",
            cliente.err()
        );
    }

    // Passada a rajada, a recusa aparece — e diz o que é. Um erro de rede aqui
    // mandaria a pessoa procurar defeito que não existe.
    let mut recusado = None;
    for _ in 0..APERTOS_DE_RAJADA {
        if let Err(erro) = entrar(endereco).await {
            recusado = Some(erro);
            break;
        }
    }

    match recusado {
        Some(ConnectError::Refused {
            reason: DisconnectReason::RateLimited,
        }) => {}
        outro => panic!("o laço de apertos de mão não foi limitado: {outro:?}"),
    }

    servidor.shutdown();
    Ok(())
}

/// Inundar de quadros de controle rende um aviso, e depois a porta.
///
/// O caso que o ADR 0021 nomeia na lista do que ele não resolvia: "um convidado
/// legítimo pode inundar de mensagens". Ele já entrou, já é quem diz ser, e
/// mesmo assim não pode gastar o servidor inteiro.
#[tokio::test(flavor = "current_thread")]
async fn quem_inunda_depois_de_entrar_e_avisado_e_depois_derrubado() -> Result<()> {
    let (endereco, servidor) = server().await?;
    let mut cliente = entrar(endereco).await.expect("o convidado entra");

    // Muito acima da rajada mais a paciência, com folga para as fichas que o
    // balde repõe enquanto isto roda. `Ping` é o quadro mais barato que existe:
    // o que se mede é o limite, não o custo do que foi pedido.
    let quantos = (QUADROS_DE_RAJADA + PACIENCIA) * 4;
    for _ in 0..quantos {
        if cliente.send_ping().await.is_err() {
            // O servidor já fechou o fio. É um desfecho legítimo desta rajada.
            break;
        }
    }

    let mut avisou = false;
    let mut derrubou = false;
    let prazo = tokio::time::Instant::now() + Duration::from_secs(10);
    while !derrubou && tokio::time::Instant::now() < prazo {
        match tokio::time::timeout(Duration::from_millis(500), cliente.next_event()).await {
            Ok(Ok(ServerMessage::Alert {
                reason: AlertReason::RateLimited,
                ..
            })) => avisou = true,
            Ok(Ok(ServerMessage::Disconnecting {
                reason: DisconnectReason::RateLimited,
            })) => derrubou = true,
            Ok(Ok(_)) => {}
            // O fio morreu. Sem o `Disconnecting` isto seria indistinguível de
            // rede ruim, que é o desfecho que este teste recusa.
            Ok(Err(erro)) => panic!("o enlace caiu sem motivo: {erro}"),
            Err(_) => break,
        }
    }

    assert!(
        avisou,
        "o servidor derrubou sem avisar antes; derrubar calado é o que faz alguém \
         achar que o produto está quebrado"
    );
    assert!(
        derrubou,
        "a inundação de quadros de controle nunca foi cortada"
    );

    servidor.shutdown();
    Ok(())
}
