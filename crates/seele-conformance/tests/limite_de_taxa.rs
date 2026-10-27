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

mod vaga;

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
    let _vaga = vaga::minha();
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
    //
    // **Juntas, e não uma depois da outra** (desde a 0.17). No QUIC cada
    // aperto levava milissegundos e o laço em série esvaziava o balde muito
    // antes de ele se repor. Pelo enlace, um aperto inteiro contra o loopback
    // leva perto de um segundo (medido nesta bateria: 60 em série, 70 s), e o
    // balde repõe meia ficha por segundo: em série, a reposição acompanhava e
    // nada era recusado, sem que o limite estivesse desligado. Juntas, as
    // tentativas chegam mais rápido que a reposição, que é o laço que o balde
    // existe para parar.
    //
    // E o motivo: as que passam pelo portão do enlace (`Portaria::admite`, que
    // olha sem gastar) e chegam à sessão sem ficha leem `RateLimited`; as que
    // o portão já barra não ganham nem DTLS, e para elas o silêncio é o certo
    // (um STUN se forja). Pelo menos uma tem de ter lido o motivo.
    //
    // **Quatro de cada vez**, e não trinta: o enlace tem teto de apertos
    // pendentes por IP (`MAX_PENDENTES_POR_IP`, 8), e trinta juntas medidas
    // nesta bateria deram 28 silêncios (`SemResposta`) e nenhum motivo — o
    // teto do enlace barrando antes do balde. Em lotes abaixo do teto, o que
    // barra é o balde.
    let mut recusas = Vec::new();
    for _ in 0..15 {
        let mut juntas = tokio::task::JoinSet::new();
        for _ in 0..4 {
            juntas.spawn(async move { entrar(endereco).await.err() });
        }
        while let Some(resultado) = juntas.join_next().await {
            if let Some(erro) = resultado.expect("a tentativa não entrou em pânico") {
                recusas.push(erro);
            }
        }
        if recusas.iter().any(|erro| {
            matches!(
                erro,
                ConnectError::Refused {
                    reason: DisconnectReason::RateLimited,
                }
            )
        }) {
            break;
        }
    }
    assert!(
        recusas.iter().any(|erro| matches!(
            erro,
            ConnectError::Refused {
                reason: DisconnectReason::RateLimited,
            }
        )),
        "o laço de apertos de mão não foi limitado com motivo: {recusas:?}"
    );

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
    let _vaga = vaga::minha();
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
