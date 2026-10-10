//! **Muitas pessoas escrevendo em rajada ao mesmo tempo não
//! derrubam ninguém, e tudo é gravado.**
//!
//! O escritor do servidor gravava tudo o que chegasse em 200 ms num lote sem
//! teto e o anunciava num laço sem `await`. Um lote maior que o anel do
//! barramento (1 024) passava à frente de **toda** sessão viva de uma vez — de
//! quem lia depressa e de quem nem estava no canal —, e cada uma se despedia com
//! `FellBehind`. A cauda do que os falantes ainda mandavam não era gravada.
//! Achado na 0.16 (a pendência 61 de lá) e medido nesta linha com o cliente de
//! verdade: 18 × 58 (1 044) passou 19 de 19 sessões para trás, e 24 × 58 com
//! corpos de 4 KiB, 25 de 25. Num servidor aberto, 18 conexões de um script
//! derrubavam todo mundo.
//!
//! O conserto tem duas metades, e este teste olha as duas pela porta de fora:
//! o lote do escritor com teto (`server::TETO_DO_LOTE`), e a contrapressão por
//! conexão (`session::TETO_NO_AR`): com mensagens demais no ar, a sessão para de
//! ler a entrada daquela conexão e o excesso espera no QUIC, do lado de quem
//! mandou.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_core::{Client, MemoryPinStore};
use seele_proto::control::{DisconnectReason, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const LINHA: ChannelId = ChannelId(1);

/// A rajada inteira que o balde de quadros deixa a uma conexão (60, menos o
/// que a entrada gastou).
const POR_FALANTE: usize = 58;

/// Só para o teste não travar: cada espera acaba antes num sinal.
const PRAZO: Duration = Duration::from_secs(60);

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
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

async fn entrar(endereco: SocketAddr, numero: usize) -> Result<Client> {
    let semente = u8::try_from(numero + 1).expect("cabe num byte");
    let mut cliente = Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        &format!("pessoa{numero:02}"),
        &ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await?;
    cliente.join_channel(LINHA).await?;
    Ok(cliente)
}

/// O que uma ponta viu: quantas mensagens leu, e se o servidor se despediu
/// dela com `FellBehind`.
async fn ler(cliente: &mut Client, esperadas: usize) -> (usize, bool) {
    let prazo = Instant::now() + PRAZO;
    let mut lidas = 0;
    while lidas < esperadas {
        let resta = prazo.saturating_duration_since(Instant::now());
        match tokio::time::timeout(resta, cliente.next_event()).await {
            Ok(Ok(ServerMessage::MessageReceived { .. })) => lidas += 1,
            Ok(Ok(ServerMessage::Disconnecting { reason })) => {
                return (lidas, reason == DisconnectReason::FellBehind);
            }
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => break,
        }
    }
    (lidas, false)
}

/// `falantes` pessoas mandam a rajada inteira ao mesmo tempo, com um ouvinte no
/// canal. Ninguém fica para trás, e tudo é gravado e lido.
async fn rodada(falantes: usize, tamanho: usize) -> Result<()> {
    let (endereco, servidor) = servidor().await?;
    let mut ouvinte = entrar(endereco, 0).await?;
    let mut clientes = Vec::new();
    for numero in 1..=falantes {
        clientes.push(entrar(endereco, numero).await?);
    }
    let total = falantes * POR_FALANTE;
    let largada = Arc::new(tokio::sync::Barrier::new(falantes + 1));

    let mut tarefas = Vec::new();
    for (indice, mut cliente) in clientes.into_iter().enumerate() {
        let largada = Arc::clone(&largada);
        tarefas.push(tokio::spawn(async move {
            largada.wait().await;
            for chave in 1..=POR_FALANTE {
                let corpo = format!("{indice:02}-{chave:04}-")
                    .chars()
                    .cycle()
                    .take(tamanho)
                    .collect::<String>();
                let chave = ClientMessageId(u64::try_from(chave).expect("cabe"));
                cliente
                    .send_message(LINHA, &corpo, chave)
                    .await
                    .expect("o enlace caiu no envio");
            }
            ler(&mut cliente, total).await
        }));
    }
    largada.wait().await;
    let (lidas_pelo_ouvinte, ouvinte_atras) = ler(&mut ouvinte, total).await;
    let mut falantes_atras = 0;
    for tarefa in tarefas {
        let (_, atras) = tarefa.await?;
        falantes_atras += usize::from(atras);
    }

    let caso = format!("{falantes} pessoas × {POR_FALANTE} mensagens de {tamanho} bytes");
    assert_eq!(
        servidor.server().atrasos.sessoes(),
        0,
        "{caso}: o barramento passou à frente de sessões que liam ({falantes_atras} falantes e \
         o ouvinte={ouvinte_atras} se despediram com FellBehind)"
    );
    assert!(
        !ouvinte_atras && falantes_atras == 0,
        "{caso}: alguém foi desligado com FellBehind"
    );
    assert_eq!(
        servidor.quantas_mensagens(LINHA).await?,
        total as u64,
        "{caso}: o servidor não gravou tudo o que leu da rajada"
    );
    assert_eq!(
        lidas_pelo_ouvinte, total,
        "{caso}: o ouvinte não leu tudo o que foi gravado"
    );
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn dezoito_pessoas_em_rajada_nao_derrubam_ninguem_e_tudo_e_gravado() -> Result<()> {
    let _vaga = vaga::minha();
    // A primeira conta que passava do anel: 18 × 58 = 1 044.
    rodada(18, 20).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vinte_e_quatro_pessoas_com_corpo_no_teto_nao_derrubam_ninguem() -> Result<()> {
    let _vaga = vaga::minha();
    // A que deixava a cauda sem gravar: 24 × 58 = 1 392, com corpos de 4 KiB.
    rodada(24, 4000).await
}
