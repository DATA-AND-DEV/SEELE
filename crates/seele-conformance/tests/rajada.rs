//! A pendência nº 1: rajada de mensagens grandes perdia entrega.
//!
//! O sintoma medido e anotado em `docs/pendencias.md` era: dez mensagens de
//! ~3,9 KB enviadas em rajada, sem o receptor ler no meio, e só duas chegavam.
//! As mesmas dez, com drenagem entre lotes, chegavam todas.
//!
//! O texto da pendência descartava o cancelamento como causa — "o comportamento
//! é idêntico antes e depois" — com base numa meia correção: naquela hora só o
//! lado do **cliente** tinha sido consertado, e a sessão do servidor continuou
//! lendo quadro dentro de um `select!` que corria contra um `interval` de um
//! segundo. Uma mensagem de 3,9 KB não cabe num pacote; numa rajada a sessão
//! fica com quadro pela metade quase o tempo todo, o relógio bate no meio, e
//! daí em diante o servidor lê corpo como se fosse tamanho.
//!
//! **Isto é a hipótese, e este teste não a confirma.** Ele passa nesta máquina
//! com a correção *e sem ela*: a sessão sabotada de volta para a leitura direta
//! dentro do `select!` e as dez mensagens continuaram chegando. Ou seja, não
//! reproduz aqui o sintoma que a pendência registra — o que também quer dizer
//! que o cancelamento continua sem ser descartado nem confirmado como a causa
//! dela.
//!
//! Fica pelo que é: o comportamento que o usuário sente — colar um texto longo
//! e ele chegar inteiro — travado num teste que roda nos três sistemas. Se a
//! perda voltar, cai aqui, e no Linux, que é onde o defeito irmão aparecia.
//!
//! O mecanismo do cancelamento, esse sim, está provado em
//! `crates/seele-core/src/frame.rs`, e a regra que impede a volta está em
//! `tests/cancelamento.rs`. A **outra** metade da pendência — a que reproduz,
//! reprova sem conserto e destrói entrega de verdade — está em
//! `crates/seele-server/tests/par_lento.rs`: um par que para de ler.
//!
//! # O que este teste passou a medir, além de contar
//!
//! Verde aqui nunca provou nada, porque já era verde antes. Então ele deixou de
//! olhar só uma perna do caminho:
//!
//! **As duas pernas, separadas.** Mensagem que nunca entrou e mensagem que
//! entrou e não saiu não têm nada em comum além do sintoma, e contar só o que
//! chegou não distingue as duas. Agora se pergunta ao servidor quantas ele gravou.
//!
//! **A primeira suspeita da pendência, em número.** Ela era "a janela de
//! controle de fluxo do QUIC no começo da conexão". A janela de fluxo por
//! stream do quinn abre em 1,25 MB; dez corpos de 3,9 KB são 39 KB, uma ordem e
//! meia de grandeza abaixo. A asserção de que nenhum dos dois lados nunca disse
//! `STREAM_DATA_BLOCKED` é essa suspeita morrendo com uma medida em vez de com
//! um argumento — e é o alarme para quem um dia encolher a janela.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::{Client, MemoryPinStore};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

const VOICE_ROOM: u32 = 1;
const LINE: u32 = 1;

/// O tamanho da pendência. Grande o bastante para não caber num pacote, que é
/// a condição de o quadro chegar partido.
const CORPO: usize = 3_900;
const QUANTAS: usize = 10;

async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let server = Arc::new(Daemon::bind(config).await?);
    let address = server.local_addr()?;
    let aceitando = Arc::clone(&server);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((address, server))
}

async fn entrar(address: SocketAddr, apelido: &str, semente: u8) -> Result<Client> {
    let mut cliente = Client::connect(
        address,
        "localhost",
        &address.to_string(),
        apelido,
        &ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await?;
    cliente.insert_plug(VoiceRoomId(VOICE_ROOM)).await?;
    cliente.join_channel(ChannelId(LINE)).await?;
    Ok(cliente)
}

#[tokio::test(flavor = "multi_thread")]
async fn uma_rajada_de_mensagens_grandes_chega_inteira() -> Result<()> {
    let (address, server) = server().await?;

    let mut autor = entrar(address, "marcela", 11).await?;
    let mut ouvinte = entrar(address, "rafael", 22).await?;

    // Sem ler nada no meio: é essa a condição da pendência. Drenar entre lotes
    // escondia o defeito.
    for numero in 0..QUANTAS {
        let corpo = format!("{numero:04}").repeat(CORPO / 4);
        autor
            .send_message(
                ChannelId(LINE),
                &corpo,
                ClientMessageId(u64::try_from(numero).expect("cabe") + 1),
            )
            .await?;
    }

    // Só agora se lê. O lote grava a cada 200 ms; dez mensagens cabem em poucos
    // lotes, e cinco segundos é folga de uma ordem de grandeza.
    let mut chegaram = 0_usize;
    let prazo = tokio::time::Instant::now() + Duration::from_secs(5);
    while chegaram < QUANTAS && tokio::time::Instant::now() < prazo {
        match tokio::time::timeout(Duration::from_millis(500), ouvinte.next_event()).await {
            Ok(Ok(ServerMessage::MessageReceived { body, .. })) if body.len() >= CORPO - 4 => {
                chegaram += 1;
            }
            Ok(Ok(_)) => {}
            // O fluxo caiu: informação, não motivo para insistir.
            Ok(Err(erro)) => panic!("o enlace caiu com {chegaram}/{QUANTAS}: {erro}"),
            Err(_) => {}
        }
    }

    assert_eq!(
        chegaram, QUANTAS,
        "pendência nº 1: a rajada perdeu entrega de novo"
    );

    // A outra perna. Se um dia isto e a linha acima discordarem, a discordância
    // é o diagnóstico: menos aqui é mensagem que o servidor nunca aceitou, menos
    // ali é mensagem que ele aceitou e não entregou.
    assert_eq!(
        server.quantas_mensagens(ChannelId(LINE)).await?,
        QUANTAS as u64,
        "pendência nº 1: a rajada não chegou nem a ser gravada"
    );

    // E o barramento não passou à frente de ninguém: nenhuma sessão ficou com
    // um buraco calado, que é o defeito de `seele-server/tests/par_lento.rs`.
    assert_eq!(server.server().atrasos.eventos(), 0);

    // A primeira suspeita da pendência, medida. Trinta e nove kilobytes contra
    // uma janela de 1,25 MB: nunca houve bloqueio, em nenhum dos dois sentidos.
    let fluxo = ouvinte.flow_control();
    assert_eq!(
        fluxo,
        seele_core::FlowControl::default(),
        "o controle de fluxo entrou na história desta rajada: {fluxo:?}"
    );

    server.shutdown();
    Ok(())
}
