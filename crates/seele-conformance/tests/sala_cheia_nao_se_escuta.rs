//! Quem pede uma sala cheia continua onde estava — no assento **e** no áudio — e
//! não passa a ouvir a sala que a recusou.
//!
//! # O defeito
//!
//! P está sentada na sala A, conversando com Q. Ela pede a sala B, que tem teto
//! 1 e já tem R dentro. O servidor recusa com `Alert { VoiceRoomFull }`, e o
//! comentário do braço `EnterVoiceRoom` (`seele-server/src/session.rs`, «Recusado
//! pelo teto. `current_voice_room` **não** se mexe») promete que a conexão
//! «continua onde estava». A ocupação cumpre a promessa; a mídia não:
//!
//! - P continua sentada em A para todo mundo, Q a vê lá, e
//! - a voz de P não chega mais a Q, nem a de Q a P: os datagramas de P vão para
//!   a tarefa de A, que já não a tem como membro, e morrem como `not_a_member`;
//! - **e P passa a ouvir B**, a sala que acabou de recusá-la. Um cliente
//!   honesto fecha o próprio áudio ao receber a recusa e não percebe; um cliente
//!   hostil escuta uma sala em que não coube.
//!
//! Medido em 09/10/2026 sobre `02f1ecad` (`0.16/lotes`): P→Q `false`, Q→P
//! `false`, R→P `true`, e o `not_a_member` de A indo de 0 a 2.
//!
//! **Na 0.15.2** (`linha/0.15`), o mesmo arquivo, com duas trocas de nome que a
//! 0.15 tem: `Destino::impressao_esperada` no lugar de `esperada`, e o
//! `PROTOCOL_VERSION` no cabeçalho da mídia, que lá não tem versão própria.
//! Medido sobre `fe1cd866`, antes do conserto: o assento verde e os dois de
//! áudio vermelhos, com P passando a ouvir R.
//!
//! # A causa, lida
//!
//! `assentar` (`seele-server/src/session.rs`) faz, nesta ordem:
//!
//! 1. `voice_rooms.leave_everywhere(session.person, None)` — tira P da tarefa de
//!    mídia de **toda** sala, A incluída;
//! 2. `encerrar_telas_da_pessoa(…)`;
//! 3. `voice_rooms.of(destino).send(VoiceRoomCommand::Join { … })` — põe P como
//!    membro da tarefa de mídia de **B**, com o `outbound` dela: daqui em diante
//!    B encaminha a P a voz de quem fala lá;
//! 4. só então `occupancy.sentar_se_couber(destino, quem, teto)`, que devolve
//!    `Lotada` e faz `assentar` voltar `Ok(false)`.
//!
//! No `Lotada` nada dos passos 1 a 3 é desfeito: P não volta à tarefa de A, não
//! sai da tarefa de B, e `midia.entrou(current_voice_room)` continua apontando a
//! subida dela para A — onde ela já não é membro. O comentário do braço `Lotada`
//! («A sala não foi tocada. Nada de anunciar saída, nada de mexer na mídia») é
//! verdade só para a ocupação.
//!
//! # O que este arquivo exige
//!
//! O comportamento **certo**. Hoje ele reprova; com o conserto do `assentar`
//! (conferir o teto antes de mexer na mídia, ou desfazer tudo no `Lotada`), passa.
//! Três testes, um por sentido, para que o vermelho diga qual dos três quebrou.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore, Room};
use seele_proto::control::{AlertReason, ServerMessage};
use seele_proto::ids::{PersonId, Ssrc, VoiceRoomId};
use seele_proto::MediaHeader;
use seele_server::persistence::{channels::Channels, Location};
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// A sala padrão do servidor, onde P e Q conversam.
const A: VoiceRoomId = VoiceRoomId(1);

/// Quanto se espera por um datagrama antes de dizer que ele não veio.
const ESPERA_DA_VOZ: Duration = Duration::from_millis(1500);

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            ..ServerConfig::default()
        })
        .await?,
    );
    let endereco = daemon.local_addr()?;
    let servindo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    Ok((endereco, daemon))
}

fn destino(endereco: SocketAddr, apelido: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        impressao_esperada: None,
        o_pino_prova_o_servidor: true,
        aceito: None,
    }
}

async fn conectar(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Enlace> {
    Ok(Enlace::conectar(
        destino(endereco, apelido),
        SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?)
}

fn sala_local(enlace: &Enlace, apelido: &str) -> Room {
    let mut room = Room::new();
    room.adopt(enlace.sessao(), apelido);
    room
}

/// Um datagrama de voz bem-formado, com o `ssrc` de quem o manda.
fn datagrama(ssrc: Ssrc, seq: u16) -> Vec<u8> {
    let header = MediaHeader {
        version: seele_proto::PROTOCOL_VERSION,
        ssrc: ssrc.get(),
        seq,
        timestamp: u32::from(seq) * 960,
    };
    let mut bytes = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = header
        .encode_datagram(b"sala cheia nao se escuta", &mut bytes)
        .expect("um datagrama bem-formado");
    bytes.truncate(tamanho);
    bytes
}

/// Absorve avisos por um tempo, aplicando ao `Room`, e devolve os alertas vistos.
async fn absorver(enlace: &mut Enlace, room: &mut Room, por: Duration) -> Vec<AlertReason> {
    let mut alertas = Vec::new();
    let fim = tokio::time::Instant::now() + por;
    while tokio::time::Instant::now() < fim {
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(50), enlace.proximo()).await
        {
            if let ServerMessage::Alert { reason, .. } = &*mensagem {
                alertas.push(*reason);
            }
            room.apply(&mensagem);
        }
    }
    alertas
}

/// A voz de `de` chega a `para`?
///
/// Três datagramas, e basta um chegar: o QUIC não retransmite datagrama, e uma
/// perda isolada em `127.0.0.1` não pode virar «não ouve». O inverso também
/// vale: para dizer «não ouve», nenhum dos três pode chegar em
/// [`ESPERA_DA_VOZ`].
async fn ouve(de: &Enlace, para: &Enlace, primeiro_seq: u16) -> bool {
    let canal = para.media();
    let vozes: Vec<Vec<u8>> = (0..3)
        .map(|i| datagrama(de.sessao().ssrc, primeiro_seq + i))
        .collect();
    for voz in &vozes {
        de.media().send(voz.clone()).expect("mandar a voz");
    }
    let fim = tokio::time::Instant::now() + ESPERA_DA_VOZ;
    loop {
        let falta = fim.saturating_duration_since(tokio::time::Instant::now());
        if falta.is_zero() {
            return false;
        }
        match tokio::time::timeout(falta, canal.next()).await {
            Ok(Ok(bytes)) if vozes.contains(&bytes) => return true,
            Ok(Ok(_)) => {}
            _ => return false,
        }
    }
}

async fn esperar_sentados(daemon: &Daemon, sala: VoiceRoomId, quantos: usize) {
    let fim = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < fim {
        if daemon.server().occupancy.lock().await.quantos(sala) == quantos {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "a sala {sala:?} não chegou a {quantos} sentados em 5 s: o cenário não se \
         montou, e o resto do teste mediria outra coisa"
    );
}

/// O cenário, montado e já recusado: P e Q em A, R sozinha em B (teto 1), e P
/// acabou de pedir B e ouvir `VoiceRoomFull`.
struct Recusada {
    daemon: Arc<Daemon>,
    b: VoiceRoomId,
    p: Enlace,
    q: Enlace,
    r: Enlace,
    quem_p: PersonId,
    sala_de_q: Room,
}

async fn p_pede_a_sala_cheia(sementes: [u8; 3]) -> Result<Recusada> {
    let (endereco, daemon) = servidor().await?;
    let b = {
        let db = daemon.server().persistence.lock().await;
        Channels::new(&db)
            .create_voice_room("Estreita", 1, None)?
            .id
    };

    let mut p = conectar(endereco, sementes[0], "p").await?;
    let mut q = conectar(endereco, sementes[1], "q").await?;
    let mut r = conectar(endereco, sementes[2], "r").await?;
    let mut sala_de_p = sala_local(&p, "p");
    let mut sala_de_q = sala_local(&q, "q");
    let mut sala_de_r = sala_local(&r, "r");
    let quem_p = p.sessao().person;

    r.entrar_na_voice_room(b, None).await?;
    sala_de_r.enter_voice_room(b);
    esperar_sentados(&daemon, b, 1).await;
    p.entrar_na_voice_room(A, None).await?;
    sala_de_p.enter_voice_room(A);
    q.entrar_na_voice_room(A, None).await?;
    sala_de_q.enter_voice_room(A);
    esperar_sentados(&daemon, A, 2).await;
    absorver(&mut p, &mut sala_de_p, Duration::from_millis(300)).await;
    absorver(&mut q, &mut sala_de_q, Duration::from_millis(300)).await;
    absorver(&mut r, &mut sala_de_r, Duration::from_millis(300)).await;

    // O antes, que é o que separa «a recusa quebrou» de «nunca funcionou».
    assert!(
        ouve(&p, &q, 100).await && ouve(&q, &p, 200).await,
        "antes de qualquer recusa, P e Q, sentadas em A, já não se ouviam: o \
         teste não chegou a medir a recusa"
    );
    assert!(
        !ouve(&r, &p, 300).await,
        "antes de qualquer recusa, P, sentada em A, já ouvia R, sentada em B: o \
         defeito medido aqui seria outro"
    );

    // P pede a sala cheia.
    p.entrar_na_voice_room(b, None).await?;
    let alertas = absorver(&mut p, &mut sala_de_p, Duration::from_millis(1500)).await;
    absorver(&mut q, &mut sala_de_q, Duration::from_millis(500)).await;
    absorver(&mut r, &mut sala_de_r, Duration::from_millis(500)).await;
    assert!(
        alertas.contains(&AlertReason::VoiceRoomFull),
        "P pediu uma sala de teto 1 já ocupada e não ouviu `VoiceRoomFull` \
         (alertas: {alertas:?}): o teste não chegou ao caso da sala cheia"
    );

    Ok(Recusada {
        daemon,
        b,
        p,
        q,
        r,
        quem_p,
        sala_de_q,
    })
}

/// Um: o assento. P continua sentada em A, B continua só com R, e Q vê P em A.
///
/// Hoje este já passa — a ocupação é a metade que o `Lotada` não toca. Fica
/// aqui porque é ele que diz o que o resto do arquivo cobra: se P está sentada
/// em A para o servidor e para Q, ela tem de estar em A também no áudio.
#[tokio::test(flavor = "multi_thread")]
async fn quem_nao_coube_na_sala_cheia_continua_sentada_onde_estava() -> Result<()> {
    let _vaga = vaga::minha();
    let caso = p_pede_a_sala_cheia([110, 111, 112]).await?;

    let onde = caso
        .daemon
        .server()
        .occupancy
        .lock()
        .await
        .onde_esta(caso.quem_p);
    assert_eq!(
        onde,
        Some(A),
        "o servidor tirou P da sala A por ter pedido uma sala cheia: a recusa \
         custou a ela o lugar onde estava"
    );
    let em_b = caso.daemon.server().occupancy.lock().await.quantos(caso.b);
    assert_eq!(
        em_b, 1,
        "o servidor sentou P na sala cheia: o teto declarado não barrou ninguém"
    );
    assert!(
        caso.sala_de_q
            .seats
            .get(&A)
            .is_some_and(|sentados| sentados.contains(&caso.quem_p)),
        "Q deixou de ver P sentada em A depois de uma recusa que não a tirou de \
         lá: o roster de quem ficou mente"
    );

    caso.daemon.shutdown();
    Ok(())
}

/// Dois: o áudio entre P e Q, nos dois sentidos. Quem continua sentada em A
/// continua ouvindo A e sendo ouvida em A.
///
/// Hoje reprova: `leave_everywhere` tirou P da tarefa de mídia de A, e o
/// `Lotada` não a devolve. P fica em A para todo mundo, surda e muda.
#[tokio::test(flavor = "multi_thread")]
async fn quem_nao_coube_na_sala_cheia_continua_ouvindo_e_sendo_ouvida_onde_estava() -> Result<()> {
    let _vaga = vaga::minha();
    let caso = p_pede_a_sala_cheia([113, 114, 115]).await?;
    let nao_membro_antes = caso
        .daemon
        .voice_rooms()
        .contadores(A)
        .await
        .map_or(0, |contadores| contadores.not_a_member);

    assert!(
        ouve(&caso.p, &caso.q, 400).await,
        "depois de ser recusada numa sala cheia, a voz de P não chega mais a Q, \
         sentada com ela em A: P continua no roster de A e ninguém de A a ouve"
    );
    assert!(
        ouve(&caso.q, &caso.p, 500).await,
        "depois de ser recusada numa sala cheia, P não ouve mais Q, sentada com \
         ela em A: a recusa a deixou surda na sala de onde ela nunca saiu"
    );
    let nao_membro_depois = caso
        .daemon
        .voice_rooms()
        .contadores(A)
        .await
        .map_or(0, |contadores| contadores.not_a_member);
    assert_eq!(
        nao_membro_depois, nao_membro_antes,
        "a sala A descartou a voz de P como de quem não é membro ({nao_membro_antes} \
         → {nao_membro_depois}): a recusa tirou P da tarefa de mídia de A e não a \
         devolveu"
    );

    caso.daemon.shutdown();
    Ok(())
}

/// Três: quem não coube não ouve a sala que a recusou.
///
/// Hoje reprova: o `Join` de B foi mandado antes da conta do teto, e o `Lotada`
/// não o desfaz. P passa a receber a voz de quem está em B.
#[tokio::test(flavor = "multi_thread")]
async fn quem_nao_coube_na_sala_cheia_nao_ouve_a_sala_que_a_recusou() -> Result<()> {
    let _vaga = vaga::minha();
    let caso = p_pede_a_sala_cheia([116, 117, 118]).await?;

    assert!(
        !ouve(&caso.r, &caso.p, 600).await,
        "P foi recusada por a sala B estar cheia e passou a ouvir R, que está em \
         B: a recusa entregou a ela o áudio da sala em que ela não coube"
    );
    assert!(
        !ouve(&caso.p, &caso.r, 700).await,
        "P foi recusada por a sala B estar cheia e R, em B, passou a ouvi-la"
    );

    caso.daemon.shutdown();
    Ok(())
}
