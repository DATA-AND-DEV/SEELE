//! **A tela de quem não coube numa sala cheia continua na sala de onde ela
//! pediu.**
//!
//! A metade de tela do defeito que a revisão do lote I-a da F5 achou em
//! `assentar` (08/10/2026), e que já estava na v0.15.1: o servidor encerrava a
//! tela da pessoa, tirava a mídia dela da sala de onde vinha e a mandava para
//! a sala pedida **antes** de conferir o teto, e na recusa por sala cheia nada
//! se desfazia. A metade de áudio — quem não coube ouvia a sala que a recusou,
//! e ficava surda e muda na sua — está em `sala_cheia_nao_se_escuta.rs`, escrito
//! pela F5.
//!
//! O conserto confere o teto antes de tocar na mídia e na tela: na recusa,
//! nada foi tocado.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore};
use seele_proto::control::{AlertReason, ServerMessage};
use seele_proto::ids::VoiceRoomId;
use seele_server::persistence::channels::Channels;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// A sala que todo servidor já tem.
const SALA_A: VoiceRoomId = VoiceRoomId(1);

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let aceitando = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, daemon))
}

async fn cliente(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Client> {
    Ok(Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        apelido,
        &SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await?)
}

/// Uma sala de teto 1: quem entra primeiro a enche.
async fn sala_de_um(daemon: &Daemon) -> VoiceRoomId {
    let db = daemon.server().persistence.lock().await;
    Channels::new(&db)
        .create_voice_room("Estreita", 1, None)
        .expect("criar a sala estreita")
        .id
}

/// Espera uma condição, ou desiste.
async fn esperar<F, Fut>(prazo: Duration, mut condicao: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    tokio::time::timeout(prazo, async {
        loop {
            if condicao().await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .is_ok()
}

async fn sentados(daemon: &Daemon, sala: VoiceRoomId) -> usize {
    daemon.server().occupancy.lock().await.quantos(sala)
}

/// Lê o fluxo de controle de `quem` até um alerta com `motivo`.
async fn esperar_alerta(quem: &mut Client, motivo: AlertReason) -> bool {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match quem.next_event().await {
                Ok(ServerMessage::Alert { reason, .. }) if reason == motivo => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false)
}

/// P e Q em A, R em B, que fica cheia. Devolve os três depois de o servidor
/// sentá-los.
async fn montar(
    endereco: SocketAddr,
    daemon: &Daemon,
) -> Result<(Client, Client, Client, VoiceRoomId)> {
    let sala_b = sala_de_um(daemon).await;
    let mut p = cliente(endereco, 81, "p").await?;
    let mut q = cliente(endereco, 82, "q").await?;
    let mut r = cliente(endereco, 83, "r").await?;
    p.enter_voice_room(SALA_A, None).await?;
    q.enter_voice_room(SALA_A, None).await?;
    r.enter_voice_room(sala_b, None).await?;
    assert!(
        esperar(Duration::from_secs(5), || async {
            sentados(daemon, SALA_A).await == 2 && sentados(daemon, sala_b).await == 1
        })
        .await,
        "o servidor não sentou P e Q em A e R em B"
    );
    Ok((p, q, r, sala_b))
}

/// **A tela de P em A não acaba** por um pedido que não deu em nada.
#[tokio::test(flavor = "multi_thread")]
async fn a_tela_de_quem_nao_cabe_na_sala_continua_na_sua() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let (mut p, _q, _r, sala_b) = montar(endereco, &daemon).await?;

    let quem = p.session().person;
    let sessao = p.session().id;
    p.start_screen_share().await?;
    assert!(
        esperar(Duration::from_secs(10), || async {
            daemon
                .server()
                .telas
                .lock()
                .await
                .de(quem, sessao)
                .is_some()
        })
        .await,
        "P não chegou a transmitir; este teste não mediria fim de tela nenhum"
    );

    p.enter_voice_room(sala_b, None).await?;
    assert!(
        esperar_alerta(&mut p, AlertReason::VoiceRoomFull).await,
        "P não foi recusada por sala cheia"
    );
    assert_eq!(
        daemon
            .server()
            .telas
            .lock()
            .await
            .de(quem, sessao)
            .map(|(sala, _)| sala),
        Some(SALA_A),
        "a tela de P em A acabou por um pedido de entrada recusado"
    );

    daemon.shutdown();
    Ok(())
}
