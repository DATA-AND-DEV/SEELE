//! A própria Taxa de Sincronização, vista do roster.
//!
//! O número que `specs/07-tema-evangelion.md` chama de «a coisa mais visível da
//! tela» aparece em dois lugares ao mesmo tempo: na barra de telemetria, medida
//! pela casca contra a própria conexão, e no roster, uma linha por piloto, vinda
//! do `PilotState` que o Dogma difunde a cada segundo.
//!
//! Para todo mundo **menos você** os dois batem. Para você, a linha do roster
//! nunca recebia difusão nenhuma: `translate` descartava o `Event::PilotState`
//! do próprio piloto, então a entrada de `me` ficava no padrão de `Pilot::new` —
//! zero — para sempre. A tela mostrava `SYNC 100%` na barra e `você 0%` no
//! roster, no mesmo instante e sobre a mesma métrica.
//!
//! # Por que zero e por que ele engana
//!
//! Zero não é «não medido» aos olhos de quem lê: pelas três faixas do comp
//! (`design/Entry Plug v2.dc.html`) zero é **crítico**, vermelho, a cor de «vá
//! olhar». Numa sessão local, com RTT, jitter e perda em zero, o Dogma calcula
//! cem — e o roster acusava o piloto de estar em colapso.
//!
//! E não para na linha: `Room::cage_sync` tira a média das cadeiras, então numa
//! sessão de um piloto só a MÉDIA DO CAGE também era zero, e num Cage de cinco
//! cada pessoa via a média puxada para baixo pela própria linha.
//!
//! # O que este arquivo afirma, e o que não
//!
//! Afirma que a difusão **chega** ao piloto que ela descreve e que o roster dele
//! sai do zero por medição, não por otimismo. Não afirma valor exato: a taxa é
//! medida contra a pilha QUIC real, e cravar «cem» seria cravar o desempenho da
//! máquina de integração contínua. O que dá para afirmar é a faixa — em
//! `127.0.0.1` uma conexão não é crítica —, e é isso que está aqui.
//!
//! O zero-até-medir continua de pé, e de propósito:
//! `seele-core/src/state.rs` guarda
//! `an_unmeasured_sync_ratio_reads_as_zero_and_not_as_perfect`. O defeito nunca
//! foi o valor inicial; era ele nunca ser substituído.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{MemoryPinStore, Room, SyncBand};
use seele_proto::ids::CageId;
use seele_server::casper::Location;
use seele_server::{DogmaConfig, Server};

const CAGE: u32 = 1;

/// Quanto se espera a difusão periódica.
///
/// O `TELEMETRY_INTERVAL` do Dogma é de um segundo. Quinze dá folga de sobra
/// para uma máquina de integração contínua carregada sem transformar uma
/// regressão numa espera longa: o laço abaixo devolve assim que o número chega.
const PRAZO: Duration = Duration::from_secs(15);

/// Sobe um Dogma numa porta que o sistema escolhe.
///
/// Mesma forma do `ejetar.rs` e do `convite.rs`, e pelo mesmo motivo: porta
/// zero, nunca um número escrito à mão, que colidiria com o Dogma que a pessoa
/// deixou rodando na própria máquina.
async fn dogma() -> Result<(SocketAddr, Arc<Server>)> {
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..DogmaConfig::default()
    };
    let servidor = Arc::new(Server::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
}

/// O apelido entra por fora porque o ADR 0017 o prende à identidade.
fn destino(endereco: SocketAddr, apelido: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        impressao_esperada: None,
    }
}

/// Conecta, entra no Cage e monta a sala como a casca monta.
///
/// `adopt` e `enter_cage` na mão são o que `plug` e o aplicativo fazem: o
/// aperto de mão consome o `Session` antes de qualquer casca ver, e o Dogma
/// confirma a entrada no Cage pelo silêncio. Sem os dois, o roster deste teste
/// começaria vazio e não haveria linha de `me` para medir.
async fn sentar(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<(Enlace, Room)> {
    let enlace = Enlace::conectar(
        destino(endereco, apelido),
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.inserir_plug(CageId(CAGE)).await?;

    let mut sala = Room::new();
    sala.adopt(enlace.sessao(), apelido);
    sala.enter_cage(CageId(CAGE));
    Ok((enlace, sala))
}

/// Dobra avisos na sala até a condição valer, ou até o prazo acabar.
///
/// Sondar com prazo, e não olhar uma vez: a difusão é periódica e a primeira
/// pode chegar a qualquer momento dentro do primeiro segundo. Devolve se a
/// condição valeu, para a asserção de quem chamou poder dizer o que viu.
async fn dobrar_ate<F>(enlace: &mut Enlace, sala: &mut Room, prazo: Duration, mut vale: F) -> bool
where
    F: FnMut(&Room) -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        if vale(sala) {
            return true;
        }
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(300), enlace.proximo()).await
        {
            sala.apply(&mensagem);
        }
    }
    vale(sala)
}

/// A taxa que a sala conhece do próprio piloto.
fn minha_taxa(sala: &Room) -> Option<u8> {
    let eu = sala.me?;
    sala.current_roster()
        .find(|piloto| piloto.id == eu)
        .map(|piloto| piloto.sync_ratio)
}

#[tokio::test(flavor = "multi_thread")]
async fn o_roster_mostra_a_taxa_do_proprio_piloto() -> Result<()> {
    let (endereco, servidor) = dogma().await?;
    let (mut enlace, mut sala) = sentar(endereco, 46, "ayanami").await?;

    assert_eq!(
        minha_taxa(&sala),
        Some(0),
        "a linha de `me` devia começar em zero: é o zero-até-medir que \
         `state.rs` guarda, e sem ele este teste não mede nada"
    );

    let mediu = dobrar_ate(&mut enlace, &mut sala, PRAZO, |sala| {
        minha_taxa(sala).is_some_and(|taxa| taxa > 0)
    })
    .await;

    let taxa = minha_taxa(&sala).expect("o próprio piloto sumiu do roster");
    assert!(
        mediu,
        "a barra de telemetria mede a Taxa de Sincronização e o roster continua \
         em {taxa}%: a difusão do `PilotState` nunca chegou ao piloto que ela \
         descreve"
    );

    // A faixa, e não o valor: cravar cem seria cravar o desempenho da máquina.
    // Em `127.0.0.1` o que se pode exigir é que ninguém precise ir olhar.
    assert_ne!(
        SyncBand::of(taxa),
        SyncBand::Critical,
        "uma conexão local marcou {taxa}%, que pelas três faixas é crítico"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_media_do_cage_conta_a_propria_linha() -> Result<()> {
    // A média sai de `Room::cage_sync`, que soma as cadeiras. Enquanto a linha
    // de `me` ficava em zero, um piloto sozinho via MÉDIA DO CAGE: 0 — e num
    // Cage cheio cada pessoa via a média puxada para baixo pela própria linha,
    // cada uma por um valor diferente. Uma média que discorda entre as telas da
    // mesma sala não é média de nada.
    let (endereco, servidor) = dogma().await?;
    let (mut enlace, mut sala) = sentar(endereco, 47, "shikinami").await?;

    let mediu = dobrar_ate(&mut enlace, &mut sala, PRAZO, |sala| {
        sala.current_cage_sync()
            .is_some_and(|media| media.ratio > 0)
    })
    .await;

    let media = sala
        .current_cage_sync()
        .expect("um Cage com alguém dentro tem média");
    assert_eq!(media.pilots, 1, "o Cage devia ter só este piloto");
    assert!(
        mediu,
        "a média do Cage de um piloto medido ficou em {}%",
        media.ratio
    );

    servidor.shutdown();
    Ok(())
}
