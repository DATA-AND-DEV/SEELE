//! Sondas da review de 21/09/2026. Asserções descrevem defeitos presentes,
//! não o comportamento desejado. Só usam servidores locais e diretórios temporários.
//! Execução: copiar para crates/seele-conformance/tests/review_v15_probe.rs,
//! cargo test -p seele-conformance --test review_v15_probe -- --nocapture
//! e remover a cópia. Não incorporar como testes de regressão sem inverter os oráculos.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore};
use seele_proto::{
    control::Permission,
    ids::{ChannelId, ClientMessageId},
    ServerMessage,
};
use seele_server::{
    permissions::{Permissions, PERSON_ROLE},
    persistence::{
        messages::{Messages, PendingMessage},
        Location,
    },
    Daemon, ServerConfig,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};

async fn start() -> Result<Arc<Daemon>> {
    let server = Arc::new(
        Daemon::bind(ServerConfig {
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            ..ServerConfig::default()
        })
        .await?,
    );
    let running = server.clone();
    tokio::spawn(async move {
        let _ = running.run().await;
    });
    Ok(server)
}
async fn connect(server: &Daemon, name: &str, seed: u8) -> Result<Client> {
    let address = server.local_addr()?;
    Ok(Client::connect(
        address,
        "localhost",
        &address.to_string(),
        name,
        &SigningKey::from_bytes(&[seed; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await?)
}
async fn message(client: &mut Client) -> Result<ServerMessage> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = client.next_event().await?;
            if matches!(event, ServerMessage::MessageReceived { .. }) {
                return Ok(event);
            }
        }
    })
    .await?
}

#[tokio::test(flavor = "multi_thread")]
async fn history_is_delivered_without_read_permission() -> Result<()> {
    let server = start().await?;
    let mut owner = connect(&server, "owner", 1).await?;
    owner.join_channel(ChannelId(1)).await?;
    owner
        .send_message(ChannelId(1), "private-history", ClientMessageId(1))
        .await?;
    message(&mut owner).await?;
    let mut visitor = connect(&server, "visitor", 2).await?;
    let person = visitor.session().person;
    visitor.disconnect();
    {
        let db = server.server().persistence.lock().await;
        let permissions = Permissions::new(&db);
        permissions.revoke_role(person, PERSON_ROLE)?;
        assert!(!permissions.may(person, Permission::ReadChannel)?);
    }
    let mut visitor = connect(&server, "visitor", 2).await?;
    assert!(!visitor
        .session()
        .permissions
        .contains(&Permission::ReadChannel));
    visitor.fetch_history(ChannelId(1), None, 50).await?;
    let received = message(&mut visitor).await?;
    assert!(
        matches!(received, ServerMessage::MessageReceived { ref body, .. } if body == "private-history")
    );
    println!("CONFIRMED: a session without ReadChannel received history");
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn old_channel_messages_enter_current_channel_state() -> Result<()> {
    let server = start().await?;
    let mut owner = connect(&server, "owner", 3).await?;
    owner.create_channel("second").await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut visitor = connect(&server, "visitor", 4).await?;
    let mut room = seele_core::Room::default();
    visitor.join_channel(ChannelId(1)).await?;
    room.open_channel(ChannelId(1));
    visitor.join_channel(ChannelId(2)).await?;
    room.open_channel(ChannelId(2));
    tokio::time::sleep(Duration::from_millis(100)).await;
    owner
        .send_message(ChannelId(1), "belongs-to-first", ClientMessageId(2))
        .await?;
    let received = message(&mut visitor).await?;
    room.apply(&received);
    assert_eq!(room.current_channel, Some(ChannelId(2)));
    assert_eq!(room.messages.len(), 1);
    assert_eq!(room.messages[0].channel, ChannelId(1));
    println!("CONFIRMED: channel 1 message was added to state while channel 2 is open");
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_channel_rolls_back_other_peoples_messages() -> Result<()> {
    let server = start().await?;
    let owner = connect(&server, "owner", 5).await?;
    let visitor = connect(&server, "visitor", 6).await?;
    let mut db = server.server().persistence.lock().await;
    let mut messages = Messages::new(&mut db);
    let make = |author, channel, id| PendingMessage {
        channel: ChannelId(channel),
        author,
        author_nickname: "probe".into(),
        body: "message".into(),
        replies_to: None,
        client_message_id: Some(ClientMessageId(id)),
    };
    let result = messages.append_batch(&[
        make(owner.session().person, 1, 1),
        make(visitor.session().person, 9999, 2),
    ]);
    assert!(result.is_err());
    assert_eq!(messages.count(ChannelId(1))?, 0);
    println!("CONFIRMED: invalid channel causes valid peer message in same batch to roll back");
    drop(db);
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn revoked_write_permission_stays_live_until_reconnect() -> Result<()> {
    let server = start().await?;
    let _owner = connect(&server, "owner", 7).await?;
    let mut visitor = connect(&server, "visitor", 8).await?;
    visitor.join_channel(ChannelId(1)).await?;
    {
        let db = server.server().persistence.lock().await;
        let permissions = Permissions::new(&db);
        permissions.revoke_role(visitor.session().person, PERSON_ROLE)?;
        assert!(!permissions.may(visitor.session().person, Permission::WriteChannel)?);
    }
    visitor
        .send_message(ChannelId(1), "after-revocation", ClientMessageId(3))
        .await?;
    assert!(
        matches!(message(&mut visitor).await?, ServerMessage::MessageReceived { ref body, .. } if body == "after-revocation")
    );
    println!("CONFIRMED: revoked WriteChannel still permits a message in existing session");
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn idempotent_retry_uses_new_channel_in_broadcast() -> Result<()> {
    let server = start().await?;
    let mut owner = connect(&server, "owner", 9).await?;
    owner.create_channel("second").await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut db = server.server().persistence.lock().await;
    let mut messages = Messages::new(&mut db);
    let make = |channel| PendingMessage {
        channel: ChannelId(channel),
        author: owner.session().person,
        author_nickname: "owner".into(),
        body: "original-body".into(),
        replies_to: None,
        client_message_id: Some(ClientMessageId(44)),
    };
    let original = messages.append_batch(&[make(1)])?;
    let replay = messages.append_batch(&[make(2)])?;
    assert_eq!(original[0].id, replay[0].id);
    assert_eq!(replay[0].channel, ChannelId(2));
    assert_eq!(messages.count(ChannelId(2))?, 0);
    println!("CONFIRMED: duplicate id returns original body/id with new channel, absent from its history");
    drop(db);
    server.shutdown();
    Ok(())
}
