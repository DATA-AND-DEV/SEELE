//! **Uma mensagem apagada pela moderação não volta por quem a repete.**
//!
//! O cliente de verdade repete, com a mesma chave, o que mandou e não viu
//! confirmado — é o que a chave existe para tornar seguro. Se entre o envio e a
//! repetição alguém apagou a mensagem, o servidor reconhecia a chave, devolvia a
//! linha apagada como se fosse a gravação, e o escritor a anunciava de novo a
//! todo o canal: um `MessageReceived` com o id apagado e o corpo vazio, que cada
//! janela, por não ter mais esse id na lista, punha de volta. A moderação era
//! desfeita na tela de todo mundo, e quem repetiu via a mensagem confirmada.
//! Achado na 0.16; esta linha tinha o mesmo caminho, e este teste o mediu aqui.
//!
//! **Nesta linha a mensagem só deixa de ser anunciada**: quem repetiu fica com a
//! pendente sem resposta, que é o defeito menor. O fio desta linha não tem como
//! dizer «apagada», e a 0.16 diz.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_core::{Client, MemoryPinStore};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{ChannelId, ClientMessageId, MessageId};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const LINHA: ChannelId = ChannelId(1);

/// Só para o teste não travar: cada espera acaba antes num sinal.
const PRAZO: Duration = Duration::from_secs(20);

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

/// A mesma semente é a mesma pessoa: é assim que quem caiu volta.
async fn entrar(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Client> {
    let mut cliente = Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        apelido,
        &ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await?;
    cliente.join_channel(LINHA).await?;
    Ok(cliente)
}

/// Lê até `pronto` dizer que achou, guardando tudo o que passou.
async fn ler_ate(
    cliente: &mut Client,
    mut pronto: impl FnMut(&ServerMessage) -> bool,
) -> Vec<ServerMessage> {
    let prazo = Instant::now() + PRAZO;
    let mut vistas = Vec::new();
    loop {
        let resta = prazo.saturating_duration_since(Instant::now());
        match tokio::time::timeout(resta, cliente.next_event()).await {
            Ok(Ok(mensagem)) => {
                let achou = pronto(&mensagem);
                vistas.push(mensagem);
                if achou {
                    return vistas;
                }
            }
            Ok(Err(_)) | Err(_) => return vistas,
        }
    }
}

fn eco_da_chave(mensagem: &ServerMessage, chave: ClientMessageId) -> Option<MessageId> {
    match mensagem {
        ServerMessage::MessageReceived {
            id,
            client_message_id: Some(dela),
            ..
        } if *dela == chave => Some(*id),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn repetir_uma_mensagem_apagada_nao_a_anuncia_de_novo_ao_canal() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    // Quem entra primeiro é o Comandante, e pode apagar a mensagem de outro.
    let mut anfitriao = entrar(endereco, 1, "anfitriao").await?;
    let mut testemunha = entrar(endereco, 2, "testemunha").await?;
    let mut autora = entrar(endereco, 3, "autora").await?;

    let chave = ClientMessageId(0xA9A6_0001);
    autora.send_message(LINHA, "isto vai sair", chave).await?;
    let vistas = ler_ate(&mut autora, |m| eco_da_chave(m, chave).is_some()).await;
    let apagada = vistas
        .iter()
        .find_map(|m| eco_da_chave(m, chave))
        .expect("a mensagem não chegou a ser gravada, e o teste mediu outra coisa");

    // Ela cai antes de a moderação agir: a resposta que ela teria visto não
    // chega, e é o caso em que o cliente repete.
    autora.disconnect();
    drop(autora);

    anfitriao.remove_message(apagada).await?;
    let removida = ler_ate(
        &mut testemunha,
        |m| matches!(m, ServerMessage::MessageRemoved { id, .. } if *id == apagada),
    )
    .await;
    assert!(
        removida
            .iter()
            .any(|m| matches!(m, ServerMessage::MessageRemoved { id, .. } if *id == apagada)),
        "a remoção não chegou à testemunha, e o teste mediu outra coisa"
    );

    // Ela volta, a mesma pessoa, e repete com a mesma chave. Depois, uma
    // mensagem nova: o escritor anuncia na ordem em que grava, então o que a
    // repetição anunciasse chegaria antes desta.
    let mut de_volta = entrar(endereco, 3, "autora").await?;
    de_volta.send_message(LINHA, "isto vai sair", chave).await?;
    let depois = ClientMessageId(0xA9A6_0002);
    de_volta
        .send_message(LINHA, "e esta é nova", depois)
        .await?;

    let da_testemunha = ler_ate(&mut testemunha, |m| eco_da_chave(m, depois).is_some()).await;
    let da_autora = ler_ate(&mut de_volta, |m| eco_da_chave(m, depois).is_some()).await;
    assert!(
        da_testemunha
            .iter()
            .any(|m| eco_da_chave(m, depois).is_some()),
        "a mensagem nova não chegou à testemunha, e o teste mediu outra coisa"
    );

    let de_novo_na_testemunha: Vec<&ServerMessage> = da_testemunha
        .iter()
        .filter(|m| matches!(m, ServerMessage::MessageReceived { id, .. } if *id == apagada))
        .collect();
    let para_a_autora: Vec<String> = da_autora
        .iter()
        .filter(|m| {
            eco_da_chave(m, chave).is_some()
                || matches!(m, ServerMessage::MessageReceived { id, .. } if *id == apagada)
                || matches!(m, ServerMessage::MessageRejected { client_message_id, .. } if *client_message_id == chave)
                || matches!(m, ServerMessage::MessageRemoved { id, .. } if *id == apagada)
        })
        .map(|m| format!("{m:?}"))
        .collect();
    assert!(
        de_novo_na_testemunha.is_empty(),
        "a mensagem que a moderação apagou foi anunciada de novo ao canal por quem a repetiu: \
         {de_novo_na_testemunha:?}\n(a autora recebeu: {para_a_autora:?})"
    );

    servidor.shutdown();
    Ok(())
}

/// **Repetir muitas apagadas não ensurdece a sessão de quem repete.**
///
/// A sessão conta as mensagens da conexão que estão no ar (a contrapressão,
/// `session::TETO_NO_AR`, oito) e para de ler a entrada no teto. A
/// conta sai quando o escritor responde por aquela chave. Uma repetição de
/// apagada que não respondesse nada ficaria no ar para sempre, e a nona deixaria
/// a pessoa falando sozinha, até o `Ping`, sem aviso.
#[tokio::test(flavor = "multi_thread")]
async fn repetir_muitas_apagadas_nao_para_a_leitura_de_quem_repete() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let mut anfitriao = entrar(endereco, 1, "anfitriao").await?;
    let mut testemunha = entrar(endereco, 2, "testemunha").await?;
    let mut autora = entrar(endereco, 3, "autora").await?;

    // Uma a mais que o teto de mensagens no ar.
    let chaves: Vec<ClientMessageId> = (1..=9).map(|n| ClientMessageId(0xA9A6_0100 + n)).collect();
    for chave in &chaves {
        autora.send_message(LINHA, "isto vai sair", *chave).await?;
    }
    let mut ids = Vec::new();
    let vistas = ler_ate(&mut autora, |m| {
        if chaves.iter().any(|chave| eco_da_chave(m, *chave).is_some()) {
            ids.push(());
        }
        ids.len() == chaves.len()
    })
    .await;
    let apagadas: Vec<MessageId> = vistas
        .iter()
        .filter_map(|m| chaves.iter().find_map(|chave| eco_da_chave(m, *chave)))
        .collect();
    assert_eq!(
        apagadas.len(),
        chaves.len(),
        "nem todas foram gravadas, e o teste mediu outra coisa"
    );
    autora.disconnect();
    drop(autora);

    for id in &apagadas {
        anfitriao.remove_message(*id).await?;
    }
    let mut removidas = 0;
    ler_ate(&mut testemunha, |m| {
        removidas += usize::from(matches!(m, ServerMessage::MessageRemoved { .. }));
        removidas == apagadas.len()
    })
    .await;
    assert_eq!(
        removidas,
        apagadas.len(),
        "as remoções não chegaram, e o teste mediu outra coisa"
    );

    let mut de_volta = entrar(endereco, 3, "autora").await?;
    for chave in &chaves {
        de_volta
            .send_message(LINHA, "isto vai sair", *chave)
            .await?;
    }
    let nova = ClientMessageId(0xA9A6_0200);
    de_volta.send_message(LINHA, "e esta é nova", nova).await?;

    let da_autora = ler_ate(&mut de_volta, |m| eco_da_chave(m, nova).is_some()).await;
    assert!(
        da_autora.iter().any(|m| eco_da_chave(m, nova).is_some()),
        "depois de repetir {} apagadas, a mensagem nova não voltou: a sessão parou de ler o que \
         ela manda",
        chaves.len()
    );
    let da_testemunha = ler_ate(&mut testemunha, |m| eco_da_chave(m, nova).is_some()).await;
    let de_novo: Vec<&ServerMessage> = da_testemunha
        .iter()
        .filter(|m| matches!(m, ServerMessage::MessageReceived { id, .. } if apagadas.contains(id)))
        .collect();
    assert!(
        de_novo.is_empty(),
        "apagadas anunciadas de novo ao canal: {de_novo:?}"
    );

    servidor.shutdown();
    Ok(())
}
