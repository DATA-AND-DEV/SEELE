//! The M3 acceptance criteria from `specs/09-roadmap.md`, executed.
//!
//! > **Aceite:** servidor reiniciado preserva estado e histórico. Queda de rede
//! > de 60 s é recuperada de forma transparente. Matriz de permissões coberta
//! > por testes.
//!
//! The permission matrix lives in `seele-server`'s unit tests, where it can
//! exercise every one of the twelve permissions against every role without a
//! network. The other two are here, because they are about a whole system: one
//! needs a real restart on a real file, the other a real reconnection.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore};
use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};
use seele_proto::ServerMessage;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

const LINE: ChannelId = ChannelId(1);
const VOICE_ROOM: VoiceRoomId = VoiceRoomId(1);
const WAIT: Duration = Duration::from_secs(5);

/// Starts a server backed by a file, and returns where to reach it plus a handle
/// to stop it.
async fn start(database: PathBuf) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(database),
        ..ServerConfig::default()
    };
    let server = Arc::new(Daemon::bind(config).await?);
    let address = server.local_addr()?;
    // `run` borrows, so the handle stays with the test and can shut the
    // endpoint down when it is finished with it.
    let accepting = Arc::clone(&server);
    tokio::spawn(async move {
        let _ = accepting.run().await;
    });
    Ok((address, server))
}

async fn connect(address: SocketAddr, nickname: &str, key: &SigningKey) -> Result<Client> {
    Client::connect(
        address,
        "localhost",
        &address.to_string(),
        nickname,
        key,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await
    .map_err(Into::into)
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

/// Waits for a message matching a predicate, ignoring telemetry chatter.
async fn wait_for<F>(client: &mut Client, mut matches: F) -> Result<ServerMessage>
where
    F: FnMut(&ServerMessage) -> bool,
{
    tokio::time::timeout(WAIT, async {
        loop {
            let event = client.next_event().await?;
            if matches(&event) {
                return Ok::<_, anyhow::Error>(event);
            }
        }
    })
    .await?
}

#[tokio::test]
async fn a_restarted_server_keeps_its_history() -> Result<()> {
    // The headline criterion. specs/04-servidor-seele.md is stronger still:
    // "Reinício não perde mensagem confirmada ao cliente", which is why a
    // message is broadcast only after its batch has committed.
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("seele.db");

    let posted = {
        let (address, server) = start(database.clone()).await?;
        let mut ayanami = connect(address, "ayanami", &key(1)).await?;
        ayanami.join_channel(LINE).await?;
        ayanami
            .send_message(LINE, "verificando harmônicos", ClientMessageId(1))
            .await?;

        // The broadcast is the confirmation: it happens after the commit.
        let event = wait_for(&mut ayanami, |event| {
            matches!(event, ServerMessage::MessageReceived { .. })
        })
        .await?;

        server.shutdown();
        event
    };

    let ServerMessage::MessageReceived { id, body, .. } = posted else {
        panic!("expected a confirmed message");
    };
    assert_eq!(body, "verificando harmônicos");

    // A new process, the same file.
    let (address, server) = start(database).await?;
    let mut ayanami = connect(address, "ayanami", &key(1)).await?;
    ayanami.join_channel(LINE).await?;
    ayanami.fetch_history(LINE, None, 50).await?;

    let event = wait_for(&mut ayanami, |event| {
        matches!(event, ServerMessage::MessageReceived { .. })
    })
    .await?;
    let ServerMessage::MessageReceived {
        id: after,
        body: after_body,
        ..
    } = event
    else {
        panic!("history did not come back");
    };

    assert_eq!(after, id, "the message came back with a different identity");
    assert_eq!(after_body, "verificando harmônicos");

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn a_restarted_server_keeps_its_accounts() -> Result<()> {
    // The other half of "preserva estado". ADR 0004 makes the key the identity,
    // so the same key must find the same account — otherwise every restart
    // orphans everybody's history.
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("seele.db");

    let first_person = {
        let (address, server) = start(database.clone()).await?;
        let client = connect(address, "ayanami", &key(1)).await?;
        let person = client.session().person;
        server.shutdown();
        person
    };

    let (address, server) = start(database).await?;
    let client = connect(address, "ayanami", &key(1)).await?;
    assert_eq!(
        client.session().person,
        first_person,
        "the same key got a different account after a restart"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn a_message_reaches_everybody_on_the_line() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (address, server) = start(directory.path().join("seele.db")).await?;

    let mut ayanami = connect(address, "ayanami", &key(1)).await?;
    let mut shinji = connect(address, "shinji", &key(2)).await?;
    ayanami.join_channel(LINE).await?;
    shinji.join_channel(LINE).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    ayanami
        .send_message(LINE, "sync caiu aqui", ClientMessageId(1))
        .await?;

    let event = wait_for(&mut shinji, |event| {
        matches!(event, ServerMessage::MessageReceived { .. })
    })
    .await?;
    let ServerMessage::MessageReceived { body, author, .. } = event else {
        panic!("not a message");
    };
    assert_eq!(body, "sync caiu aqui");
    assert_eq!(author, ayanami.session().person);

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn a_resent_message_is_not_posted_twice() -> Result<()> {
    // specs/02-protocolo.md: idempotent by client_msg_id. The case is a client
    // that resends because the acknowledgement was lost, not because the user
    // pressed enter twice.
    let directory = tempfile::tempdir()?;
    let (address, server) = start(directory.path().join("seele.db")).await?;

    let mut ayanami = connect(address, "ayanami", &key(1)).await?;
    ayanami.join_channel(LINE).await?;

    ayanami
        .send_message(LINE, "uma vez", ClientMessageId(7))
        .await?;
    let first = wait_for(&mut ayanami, |event| {
        matches!(event, ServerMessage::MessageReceived { .. })
    })
    .await?;

    ayanami
        .send_message(LINE, "uma vez", ClientMessageId(7))
        .await?;
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Fresh history should hold exactly one.
    let mut reader = connect(address, "shinji", &key(2)).await?;
    reader.join_channel(LINE).await?;
    reader.fetch_history(LINE, None, 50).await?;

    let mut seen = 0;
    let _ = tokio::time::timeout(Duration::from_millis(800), async {
        loop {
            if let Ok(ServerMessage::MessageReceived { .. }) = reader.next_event().await {
                seen += 1;
            }
        }
    })
    .await;

    let ServerMessage::MessageReceived { id, .. } = first else {
        panic!("not a message");
    };
    let _ = id;
    assert_eq!(seen, 1, "the resend was posted twice");

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn a_person_without_write_permission_is_refused() -> Result<()> {
    // The permission matrix proper lives in seele-server's unit tests; this
    // checks the wiring, that the refusal actually reaches the wire.
    // specs/08-seguranca.md: the server denying is the security.
    let directory = tempfile::tempdir()?;
    let config = ServerConfig {
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(directory.path().join("seele.db")),
        observers: vec!["observador".into()],
        ..ServerConfig::default()
    };
    let server = Arc::new(Daemon::bind(config).await?);
    let address = server.local_addr()?;
    let accepting = Arc::clone(&server);
    tokio::spawn(async move {
        let _ = accepting.run().await;
    });

    // The bootstrap list makes this person an Observador, who by specs/04 may
    // "só ouvir e ler".
    let mut observer = connect(address, "observador", &key(3)).await?;
    observer.join_channel(LINE).await?;
    observer
        .send_message(LINE, "não deveria passar", ClientMessageId(1))
        .await?;

    let event = wait_for(&mut observer, |event| {
        matches!(
            event,
            ServerMessage::Alert {
                reason: seele_proto::control::AlertReason::PermissionDenied,
                ..
            }
        )
    })
    .await?;
    assert!(matches!(event, ServerMessage::Alert { .. }));

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn telemetry_carries_a_sync_ratio() -> Result<()> {
    // specs/07-estetica.md makes this the most visible number on screen,
    // and specs/02-protocolo.md derives it from RTT, jitter and loss. On
    // loopback it should be nominal.
    let directory = tempfile::tempdir()?;
    let (address, server) = start(directory.path().join("seele.db")).await?;
    let mut client = connect(address, "ayanami", &key(1)).await?;

    let event = wait_for(&mut client, |event| {
        matches!(event, ServerMessage::Telemetry(_))
    })
    .await?;

    let ServerMessage::Telemetry(telemetry) = event else {
        panic!("not telemetry");
    };
    assert!(telemetry.rtt_ms.is_finite() && telemetry.rtt_ms >= 0.0);
    assert!((0.0..=1.0).contains(&telemetry.loss_fraction));
    assert_eq!(
        telemetry.subsystems.len(),
        3,
        "PERMISSIONS, MEDIA, PERSISTENCE"
    );

    let ratio = seele_proto::signal::raw(seele_proto::SyncInputs {
        rtt_ms: telemetry.rtt_ms,
        jitter_ms: telemetry.jitter_ms,
        loss_fraction: telemetry.loss_fraction,
    });
    assert!(
        ratio > 70.0,
        "loopback should not be a degraded connection, scored {ratio}"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn a_returning_person_reclaims_their_seat_and_their_ssrc() -> Result<()> {
    // specs/09-roadmap.md: "Queda de rede de 60 s é recuperada de forma
    // transparente." The observable half of transparent is that the person comes
    // back as themselves: same account, same ssrc, same voice room. Otherwise the
    // outage looks to everybody else like a departure and an arrival, and every
    // listener's jitter buffer starts over.
    let directory = tempfile::tempdir()?;
    let (address, server) = start(directory.path().join("seele.db")).await?;

    let before = {
        let mut ayanami = connect(address, "ayanami", &key(1)).await?;
        ayanami.insert_plug(VOICE_ROOM).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        let session = ayanami.session().clone();
        // The train enters the tunnel.
        ayanami.disconnect();
        session
    };

    tokio::time::sleep(Duration::from_millis(300)).await;

    let after = connect(address, "ayanami", &key(1)).await?;
    assert_eq!(
        after.session().person,
        before.person,
        "came back as somebody else"
    );
    assert_eq!(
        after.session().ssrc,
        before.ssrc,
        "the seat was not reclaimed — every listener would resynchronise"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn history_pages_backwards_without_gaps() -> Result<()> {
    // specs/02-protocolo.md: "Paginação por cursor, nunca offset."
    let directory = tempfile::tempdir()?;
    let (address, server) = start(directory.path().join("seele.db")).await?;

    let mut ayanami = connect(address, "ayanami", &key(1)).await?;
    ayanami.join_channel(LINE).await?;
    for index in 1..=6_u64 {
        ayanami
            .send_message(LINE, &format!("mensagem {index}"), ClientMessageId(index))
            .await?;
    }
    tokio::time::sleep(Duration::from_millis(600)).await;

    let mut reader = connect(address, "shinji", &key(2)).await?;
    reader.join_channel(LINE).await?;
    reader.fetch_history(LINE, None, 3).await?;

    let mut page = Vec::new();
    let _ = tokio::time::timeout(Duration::from_millis(800), async {
        loop {
            if let Ok(ServerMessage::MessageReceived { id, body, .. }) = reader.next_event().await {
                page.push((id, body));
            }
        }
    })
    .await;

    assert_eq!(page.len(), 3, "asked for three, got {}", page.len());
    // Oldest of the page first on the wire, so a client can append.
    assert_eq!(page[0].1, "mensagem 4");
    assert_eq!(page[2].1, "mensagem 6");

    server.shutdown();
    Ok(())
}
