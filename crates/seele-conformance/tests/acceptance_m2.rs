//! The M2 acceptance criteria from `specs/09-roadmap.md`, executed.
//!
//! > **Aceite:** três clientes entram no mesmo Cage e conversam por voz através
//! > do servidor. Cliente sem permissão é rejeitado. Fuzzing do parser sem
//! > crash.
//!
//! The fuzzing lives in `fuzz/`; the other two are here. Everything runs
//! in-process on an ephemeral port, so this passes in CI on a machine with no
//! sound card and no second host — which is why `seele-server` is a library as
//! well as a binary (`specs/10-convencoes.md`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore, Pattern, PinDecision};
use seele_proto::ids::{CageId, Ssrc};
use seele_proto::MediaHeader;
use seele_server::{DogmaConfig, Server};

/// How long to wait for a datagram before calling it lost.
///
/// Generous: this runs on loopback, so anything approaching this is a hang
/// rather than slowness.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(3);

/// Starts a Dogma on an ephemeral port and returns where to reach it.
async fn start(observers: Vec<String>) -> Result<SocketAddr> {
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        observers,
        ..DogmaConfig::default()
    };
    let server = Server::bind(config).await?;
    let address = server.local_addr()?;
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    Ok(address)
}

/// Connects one client with a fresh identity.
async fn connect(address: SocketAddr, nickname: &str) -> Result<Client> {
    // ADR 0004: identity is an Ed25519 key pair generated on first use.
    // ed25519-dalek 3.0 builds a key from 32 bytes of entropy directly,
    // which avoids coupling this to whichever rand version is in the tree.
    let key = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    Client::connect(
        address,
        "localhost",
        &address.to_string(),
        nickname,
        &key,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await
    .map_err(Into::into)
}

/// Builds one media datagram for a source.
fn media(ssrc: Ssrc, seq: u16, payload: &[u8]) -> Vec<u8> {
    let header = MediaHeader {
        version: seele_proto::PROTOCOL_VERSION,
        ssrc: ssrc.get(),
        seq,
        timestamp: u32::from(seq) * 960,
    };
    let mut out = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let len = header
        .encode_datagram(payload, &mut out)
        .expect("a well-formed datagram");
    out.truncate(len);
    out
}

#[tokio::test]
async fn three_clients_in_one_cage_hear_each_other() -> Result<()> {
    // The headline criterion of specs/09-roadmap.md.
    let address = start(Vec::new()).await?;

    let mut ayanami = connect(address, "ayanami").await?;
    let mut shinji = connect(address, "shinji").await?;
    let mut asuka = connect(address, "asuka").await?;

    // The handshake reached PADRÃO: AZUL for all three.
    assert_eq!(ayanami.pattern(), Pattern::Blue);
    assert_eq!(shinji.pattern(), Pattern::Blue);
    assert_eq!(asuka.pattern(), Pattern::Blue);

    // Gap G1: each client learned its own ssrc, and they are distinct. Without
    // this nobody could attribute audio to anybody.
    let sources = [
        ayanami.session().ssrc,
        shinji.session().ssrc,
        asuka.session().ssrc,
    ];
    assert_eq!(
        sources
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3,
        "the server handed out a duplicate ssrc"
    );

    for client in [&mut ayanami, &mut shinji, &mut asuka] {
        client.insert_plug(CageId(1)).await?;
    }
    // The Cage task processes joins asynchronously; give it a moment before
    // anybody speaks, or the first datagram races the membership.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let spoken = media(ayanami.session().ssrc, 1, b"harmonics");
    ayanami.send_media(spoken.clone())?;

    // specs/04-servidor-seele.md: forwarded to every *other* subscriber, payload
    // untouched.
    let heard_by_shinji = tokio::time::timeout(DELIVERY_TIMEOUT, shinji.next_media()).await??;
    let heard_by_asuka = tokio::time::timeout(DELIVERY_TIMEOUT, asuka.next_media()).await??;

    assert_eq!(
        heard_by_shinji, spoken,
        "the payload was modified in transit"
    );
    assert_eq!(heard_by_asuka, spoken);

    // And not back to the speaker.
    let echo = tokio::time::timeout(Duration::from_millis(400), ayanami.next_media()).await;
    assert!(echo.is_err(), "the speaker heard their own voice");

    Ok(())
}

#[tokio::test]
async fn a_client_without_permission_is_refused() -> Result<()> {
    // The second criterion. specs/04-servidor-seele.md: "always validate — do not
    // trust the client"; specs/07 calls the role that may listen but not speak
    // an Observador.
    let address = start(vec!["observador".into()]).await?;

    let mut observer = connect(address, "observador").await?;
    let mut pilot = connect(address, "ayanami").await?;

    observer.insert_plug(CageId(1)).await?;
    pilot.insert_plug(CageId(1)).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The observer is in the Cage and may listen.
    observer.send_media(media(observer.session().ssrc, 1, b"should not carry"))?;
    let leaked = tokio::time::timeout(Duration::from_millis(600), pilot.next_media()).await;
    assert!(leaked.is_err(), "an observer was forwarded to the Cage");

    // The other direction still works, so the refusal is about permission rather
    // than a broken Cage.
    let spoken = media(pilot.session().ssrc, 1, b"carries");
    pilot.send_media(spoken.clone())?;
    let heard = tokio::time::timeout(DELIVERY_TIMEOUT, observer.next_media()).await??;
    assert_eq!(heard, spoken, "the observer could not listen either");

    Ok(())
}

#[tokio::test]
async fn a_forged_ssrc_is_refused() -> Result<()> {
    // Gap G2. specs/08-seguranca.md promises that "a client forging another's
    // identity" is handled because the ssrc is server-assigned, but nothing said
    // the header had to be checked against the connection. Without that check a
    // pilot puts somebody else's ssrc in their datagrams and every listener
    // credits them with the wrong voice.
    let address = start(Vec::new()).await?;

    let mut ayanami = connect(address, "ayanami").await?;
    let mut shinji = connect(address, "shinji").await?;
    let mut asuka = connect(address, "asuka").await?;

    for client in [&mut ayanami, &mut shinji, &mut asuka] {
        client.insert_plug(CageId(1)).await?;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Shinji claims to be Ayanami.
    let forged = media(ayanami.session().ssrc, 1, b"impersonation");
    shinji.send_media(forged)?;

    let delivered = tokio::time::timeout(Duration::from_millis(600), asuka.next_media()).await;
    assert!(delivered.is_err(), "a forged datagram reached the Cage");

    // An honest datagram from the same connection still goes through, so the
    // refusal is about the forgery and not about Shinji.
    let honest = media(shinji.session().ssrc, 2, b"honest");
    shinji.send_media(honest.clone())?;
    let heard = tokio::time::timeout(DELIVERY_TIMEOUT, asuka.next_media()).await??;
    assert_eq!(heard, honest);

    Ok(())
}

#[tokio::test]
async fn the_first_connection_pins_the_certificate() -> Result<()> {
    // ADR 0003. The pin is what makes the change warning of specs/08 possible at
    // all, so the first contact has to record something.
    let address = start(Vec::new()).await?;
    let client = connect(address, "ayanami").await?;

    let PinDecision::FirstContact { fingerprint } = client.pin_decision() else {
        panic!("the first connection should have pinned the certificate");
    };
    assert_eq!(fingerprint.len(), 64, "a SHA-256 fingerprint in hex");

    Ok(())
}

#[tokio::test]
async fn two_dogmas_on_one_machine_do_not_share_a_pin() -> Result<()> {
    // The bug this exists to stop: both shells hand TLS the name `localhost`
    // for any IP address, because that is the name the M2 certificate carries.
    // While the pin was filed under that same label, two Dogmas shared one
    // entry — and the second one contacted looked like the first one's key had
    // changed. That is the most alarming false positive this system can
    // produce, and the first LAN test between two machines would have hit it.
    let first_address = start(Vec::new()).await?;
    let second_address = start(Vec::new()).await?;
    assert_ne!(first_address, second_address);

    // One store, the way one pilot's `~/.config/seele/pins` is one store.
    let pins = Arc::new(MemoryPinStore::new());
    let key = SigningKey::from_bytes(&[70; 32]);

    let first = Client::connect(
        first_address,
        "localhost",
        &first_address.to_string(),
        "ayanami",
        &key,
        Arc::clone(&pins) as Arc<_>,
        None,
    )
    .await?;
    assert!(matches!(
        first.pin_decision(),
        PinDecision::FirstContact { .. }
    ));

    // A different Dogma with a different self-signed certificate. This must be
    // a first contact too, not a key change.
    let second = Client::connect(
        second_address,
        "localhost",
        &second_address.to_string(),
        "ayanami",
        &key,
        Arc::clone(&pins) as Arc<_>,
        None,
    )
    .await
    .map_err(|error| {
        anyhow::anyhow!(
            "a second Dogma was refused as if the first one's key had changed: {error:?}"
        )
    })?;
    assert!(matches!(
        second.pin_decision(),
        PinDecision::FirstContact { .. }
    ));

    // And going back to the first one still matches what was pinned for it.
    let again = Client::connect(
        first_address,
        "localhost",
        &first_address.to_string(),
        "ayanami",
        &key,
        Arc::clone(&pins) as Arc<_>,
        None,
    )
    .await?;
    assert!(matches!(again.pin_decision(), PinDecision::Matches { .. }));

    Ok(())
}

#[tokio::test]
async fn a_second_connection_reuses_the_pin() -> Result<()> {
    // The store is shared between the two connections, as it would be on disk.
    let address = start(Vec::new()).await?;
    let pins = Arc::new(MemoryPinStore::new());
    // ed25519-dalek 3.0 builds a key from 32 bytes of entropy directly,
    // which avoids coupling this to whichever rand version is in the tree.
    let key = SigningKey::from_bytes(&rand::random::<[u8; 32]>());

    let first = Client::connect(
        address,
        "localhost",
        &address.to_string(),
        "ayanami",
        &key,
        Arc::clone(&pins) as Arc<_>,
        None,
    )
    .await?;
    assert!(matches!(
        first.pin_decision(),
        PinDecision::FirstContact { .. }
    ));

    let second = Client::connect(
        address,
        "localhost",
        &address.to_string(),
        "ayanami",
        &key,
        Arc::clone(&pins) as Arc<_>,
        None,
    )
    .await?;
    assert!(matches!(second.pin_decision(), PinDecision::Matches { .. }));

    Ok(())
}

#[tokio::test]
async fn a_ping_comes_back_as_a_pong() -> Result<()> {
    // specs/02-protocolo.md makes this the base of the Sync Ratio, and the one
    // input seele-audio cannot produce on its own.
    //
    // The answer arrives through the event stream rather than a blocking read:
    // the control stream carries telemetry and roster changes too, and a second
    // reader would swallow whatever the first was waiting for.
    let address = start(Vec::new()).await?;
    let mut client = connect(address, "ayanami").await?;

    client.send_ping().await?;
    tokio::time::timeout(DELIVERY_TIMEOUT, async {
        loop {
            if matches!(
                client.next_event().await?,
                seele_proto::ServerMessage::Pong { .. }
            ) {
                return Ok::<_, anyhow::Error>(());
            }
        }
    })
    .await??;

    let rtt = client.rtt().expect("a measured round trip");
    assert!(rtt < Duration::from_secs(1), "loopback rtt was {rtt:?}");

    Ok(())
}

#[tokio::test]
async fn the_session_names_the_dogma_and_its_cage() -> Result<()> {
    // specs/02-protocolo.md: the Session carries the Dogma description and the
    // tree of Cages and Lines, which is what a shell draws its first screen from.
    let address = start(Vec::new()).await?;
    let client = connect(address, "ayanami").await?;

    let session = client.session();
    assert_eq!(session.dogma, "Terceira Tóquio");
    assert_eq!(session.cages.len(), 1);
    assert_eq!(session.cages.first().map(|cage| cage.id), Some(CageId(1)));

    Ok(())
}

#[tokio::test]
async fn media_before_entering_a_cage_goes_nowhere() -> Result<()> {
    // A connection that authenticated but never inserted its plug has no
    // business reaching a Cage. specs/04: validate that the sender is in it.
    let address = start(Vec::new()).await?;

    let listener = connect(address, "ayanami").await?;
    let mut inside = connect(address, "shinji").await?;
    inside.insert_plug(CageId(1)).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // `listener` never inserted its plug.
    listener.send_media(media(listener.session().ssrc, 1, b"nope"))?;

    let leaked = tokio::time::timeout(Duration::from_millis(600), inside.next_media()).await;
    assert!(leaked.is_err(), "media from outside the Cage was forwarded");

    Ok(())
}
