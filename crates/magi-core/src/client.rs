//! The headless client: connect, handshake, hold a session.
//!
//! `specs/01-arquitetura.md` makes this crate the one place session, protocol,
//! audio and state logic live: "one core, three shells". Nothing here formats
//! text, chooses a colour or knows what a terminal is.
//!
//! # PADRÃO: LARANJA and PADRÃO: AZUL
//!
//! `specs/02-protocolo.md`:
//!
//! > Before `Sessao`, the client is in **PADRÃO: LARANJA** — connected, not
//! > verified. The interface must reflect that state, not hide it.
//!
//! [`Pattern`] is that state, as plain data. A shell decides what orange looks
//! like; this decides when it is true.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use magi_proto::control::{ClientMessage, ServerMessage};
use magi_proto::ids::{CageId, PilotId, SessionId, Ssrc};
use magi_proto::transport::{HANDSHAKE_TIMEOUT, IDLE_TIMEOUT, KEEPALIVE};

use crate::frame;
use crate::tofu::{PinDecision, PinStore, TofuVerifier};

/// How far a connection has got.
///
/// `specs/07-tema-evangelion.md` names these, and `specs/05-cliente-tui.md`
/// requires both to be visible states rather than a spinner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    /// Not connected.
    Offline,
    /// Connected, not verified. **PADRÃO: LARANJA.**
    Orange,
    /// Verified. **PADRÃO: AZUL.**
    Blue,
}

/// Everything the server told us on the way in.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: SessionId,
    /// Which pilot this connection is.
    pub pilot: PilotId,
    /// The media source the server assigned. Gap G1.
    pub ssrc: Ssrc,
    /// What the Dogma is called.
    pub dogma: String,
    /// Voice channels visible to us.
    pub cages: Vec<magi_proto::control::CageInfo>,
}

/// A connected, verified client.
pub struct Client {
    connection: quinn::Connection,
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    session: SessionInfo,
    pin: PinDecision,
    pattern: Pattern,
}

impl Client {
    /// Connects, runs the handshake, and returns once PATTERN: BLUE is reached.
    ///
    /// `signing_key` is this client's identity (ADR 0004). Generating a fresh
    /// one produces a fresh identity, which in M2 — with no accounts — is all
    /// there is.
    ///
    /// # Errors
    ///
    /// Fails if the pinned certificate changed, the handshake is refused, or the
    /// budget in `specs/02-protocolo.md` runs out.
    pub async fn connect(
        server: SocketAddr,
        server_name: &str,
        nickname: &str,
        signing_key: &SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let verifier = Arc::new(TofuVerifier::new(pins));
        let mut tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::clone(&verifier) as Arc<_>)
            .with_no_client_auth();
        tls.alpn_protocols = vec![magi_proto::transport::ALPN.to_vec()];

        let quic = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
            .context("could not build the QUIC TLS config")?;
        let mut client_config = quinn::ClientConfig::new(Arc::new(quic));

        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(IDLE_TIMEOUT.try_into()?));
        transport.keep_alive_interval(Some(KEEPALIVE));
        client_config.transport_config(Arc::new(transport));

        let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(server, server_name)?
            .await
            .context("could not establish the QUIC connection")?;

        // PATTERN: ORANGE — connected, not verified.
        let pin = verifier.last_decision().unwrap_or(PinDecision::Matches);

        let (mut send, mut recv) = connection.open_bi().await?;
        let session = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            handshake(&mut send, &mut recv, nickname, signing_key),
        )
        .await
        .context("handshake timed out")??;

        Ok(Self {
            connection,
            send,
            recv,
            session,
            pin,
            pattern: Pattern::Blue,
        })
    }

    /// What the server told us.
    #[must_use]
    pub fn session(&self) -> &SessionInfo {
        &self.session
    }

    /// How far the connection has got.
    #[must_use]
    pub fn pattern(&self) -> Pattern {
        self.pattern
    }

    /// What the TOFU check decided. ADR 0003.
    #[must_use]
    pub fn pin_decision(&self) -> &PinDecision {
        &self.pin
    }

    /// Enters a Cage. "Inserir plug".
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn insert_plug(&mut self, cage: CageId) -> Result<()> {
        frame::write(
            &mut self.send,
            &ClientMessage::InsertPlug {
                cage,
                password: None,
            },
        )
        .await
    }

    /// Sends one media datagram.
    ///
    /// The `ssrc` must be the one in [`SessionInfo::ssrc`]; the server refuses
    /// anything else (gap G2), which is what stops one pilot being credited with
    /// another's audio.
    ///
    /// # Errors
    ///
    /// Fails if the datagram is too large for the path.
    pub fn send_media(&self, datagram: Vec<u8>) -> Result<()> {
        self.connection.send_datagram(datagram.into())?;
        Ok(())
    }

    /// Waits for the next media datagram.
    ///
    /// # Errors
    ///
    /// Fails when the connection closes.
    pub async fn next_media(&self) -> Result<Vec<u8>> {
        Ok(self.connection.read_datagram().await?.to_vec())
    }

    /// Round trip, measured with `Ping`/`Pong`.
    ///
    /// `specs/02-protocolo.md` makes this the base of the Sync Ratio — and the
    /// one input `magi-audio` cannot produce on its own.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed or the server answers something
    /// else.
    pub async fn ping(&mut self) -> Result<std::time::Duration> {
        let started = std::time::Instant::now();
        let stamp = started.elapsed().as_nanos() as u64;
        frame::write(&mut self.send, &ClientMessage::Ping { timestamp: stamp }).await?;

        match frame::read::<ServerMessage>(&mut self.recv).await? {
            ServerMessage::Pong { timestamp } if timestamp == stamp => Ok(started.elapsed()),
            other => bail!("expected Pong, got {other:?}"),
        }
    }

    /// Closes the connection.
    pub fn disconnect(&mut self) {
        self.pattern = Pattern::Offline;
        self.connection.close(0_u32.into(), b"ejected");
    }
}

async fn handshake(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    nickname: &str,
    signing_key: &SigningKey,
) -> Result<SessionInfo> {
    frame::write(
        send,
        &ClientMessage::Hello {
            version: magi_proto::PROTOCOL_VERSION,
            client: concat!("plug/", env!("CARGO_PKG_VERSION")).into(),
            nickname: nickname.to_owned(),
            public_key: signing_key.verifying_key().to_bytes().to_vec(),
        },
    )
    .await?;

    let challenge = frame::read::<ServerMessage>(recv).await?;
    let nonce = match challenge {
        ServerMessage::Challenge { nonce } => nonce,
        ServerMessage::Disconnecting { reason } => {
            // specs/02-protocolo.md: the reason is enumerated and specific.
            bail!("pattern blue not established: {reason:?}");
        }
        other => bail!("expected Challenge, got {other:?}"),
    };

    frame::write(
        send,
        &ClientMessage::Response {
            proof: signing_key.sign(&nonce).to_bytes().to_vec(),
        },
    )
    .await?;

    match frame::read::<ServerMessage>(recv).await? {
        ServerMessage::Session {
            id,
            pilot,
            ssrc,
            dogma,
            cages,
            ..
        } => Ok(SessionInfo {
            id,
            pilot,
            ssrc,
            dogma,
            cages,
        }),
        ServerMessage::Disconnecting { reason } => {
            bail!("pattern blue not established: {reason:?}")
        }
        other => bail!("expected Session, got {other:?}"),
    }
}
