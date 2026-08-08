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

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use magi_proto::control::{ClientMessage, ServerMessage};
use magi_proto::ids::{CageId, ClientMessageId, LineId, MessageId, PilotId, SessionId, Ssrc};
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

/// Why a connection did not happen.
///
/// Enumerated rather than an opaque error, because `specs/06-clientes-gui.md`
/// requires errors crossing to a shell to be enums a shell can localise. The
/// alternative — matching on the text of somebody else's error message — breaks
/// silently the day `quinn` rewords a sentence, and breaks in the direction of
/// reporting the wrong cause rather than none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// The QUIC endpoint could not be opened locally.
    ///
    /// A blocked UDP socket or an exhausted port range. Nothing to do with the
    /// server.
    LocalEndpoint,
    /// Nothing answered, or the network refused to carry it.
    Unreachable,
    /// TLS refused the certificate for a reason other than the pin.
    TlsRefused,
    /// The pinned key is not the key offered. ADR 0003.
    PinChanged {
        /// What was pinned before.
        pinned: String,
        /// What was offered now.
        offered: String,
    },
    /// The handshake did not finish inside the budget in `specs/02-protocolo.md`.
    HandshakeTimeout,
    /// The server ended the session during the handshake, and said why.
    Refused {
        /// The enumerated reason.
        reason: magi_proto::control::DisconnectReason,
    },
    /// The server said something that is not a handshake.
    ProtocolViolation,
}

impl std::fmt::Display for ConnectError {
    /// For logs, never for a user. A shell writes its own sentence.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ConnectError {}

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
    /// Text channels visible to us.
    ///
    /// Carried through rather than dropped: an interface that knows the Cages
    /// but not the Lines can only ever open whichever Line it was started with.
    pub lines: Vec<magi_proto::control::LineInfo>,
}

/// The media half of a connection, usable independently of the control stream.
///
/// `specs/02-protocolo.md` puts voice on datagrams and control on a stream
/// precisely so neither waits for the other. Handing them out as separate
/// objects makes that structural: a caller can await both at once without one
/// borrow blocking the other, which is exactly the shape a shell needs.
#[derive(Clone)]
pub struct MediaChannel {
    connection: quinn::Connection,
}

impl MediaChannel {
    /// Sends one media datagram.
    ///
    /// The `ssrc` in the header must be the one in [`SessionInfo::ssrc`]; the
    /// server refuses anything else (gap G2).
    ///
    /// # Errors
    ///
    /// Fails if the datagram is too large for the path.
    pub fn send(&self, datagram: Vec<u8>) -> Result<()> {
        self.connection.send_datagram(datagram.into())?;
        Ok(())
    }

    /// Waits for the next media datagram.
    ///
    /// # Errors
    ///
    /// Fails when the connection closes.
    pub async fn next(&self) -> Result<Vec<u8>> {
        Ok(self.connection.read_datagram().await?.to_vec())
    }
}

/// A connected, verified client.
pub struct Client {
    connection: quinn::Connection,
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    session: SessionInfo,
    pin: PinDecision,
    pattern: Pattern,
    /// A ping that has been sent and not yet answered.
    pending_ping: Option<(u64, std::time::Instant)>,
    /// The most recent round trip.
    last_rtt: Option<std::time::Duration>,
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
    /// A [`ConnectError`] variant, never a message. Every caller has to be able
    /// to say which of these happened in its own words.
    pub async fn connect(
        server: SocketAddr,
        server_name: &str,
        nickname: &str,
        signing_key: &SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let verifier = Arc::new(TofuVerifier::new(pins));
        let mut tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::clone(&verifier) as Arc<_>)
            .with_no_client_auth();
        tls.alpn_protocols = vec![magi_proto::transport::ALPN.to_vec()];

        let quic = quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(|error| {
            tracing::error!(%error, "could not build the QUIC TLS config");
            ConnectError::LocalEndpoint
        })?;
        let mut client_config = quinn::ClientConfig::new(Arc::new(quic));

        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(
            IDLE_TIMEOUT
                .try_into()
                .map_err(|_| ConnectError::LocalEndpoint)?,
        ));
        transport.keep_alive_interval(Some(KEEPALIVE));
        client_config.transport_config(Arc::new(transport));

        let mut endpoint =
            quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0))).map_err(|error| {
                tracing::error!(%error, "could not open a local QUIC endpoint");
                ConnectError::LocalEndpoint
            })?;
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(server, server_name)
            .map_err(|error| {
                tracing::warn!(%error, "could not start the QUIC connection");
                ConnectError::Unreachable
            })?
            .await
            .map_err(|error| classify_connection_error(&error, verifier.last_decision()))?;

        // PATTERN: ORANGE — connected, not verified.
        let pin = verifier.last_decision().unwrap_or(PinDecision::Matches);

        let (mut send, mut recv) = connection.open_bi().await.map_err(|error| {
            tracing::warn!(%error, "could not open the control stream");
            ConnectError::Unreachable
        })?;
        let session = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            handshake(&mut send, &mut recv, nickname, signing_key),
        )
        .await
        .map_err(|_| ConnectError::HandshakeTimeout)??;

        Ok(Self {
            connection,
            send,
            recv,
            session,
            pin,
            pattern: Pattern::Blue,
            pending_ping: None,
            last_rtt: None,
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

    /// The media half of this connection.
    ///
    /// Cheap to clone, and independent of the control stream — see
    /// [`MediaChannel`].
    #[must_use]
    pub fn media(&self) -> MediaChannel {
        MediaChannel {
            connection: self.connection.clone(),
        }
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

    /// Subscribes to a text channel.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn join_line(&mut self, line: LineId) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::JoinLine { line }).await
    }

    /// Posts a message.
    ///
    /// `client_message_id` makes the send idempotent (`specs/02-protocolo.md`,
    /// gap G9): resending after a lost acknowledgement does not post twice.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn send_message(
        &mut self,
        line: LineId,
        body: &str,
        client_message_id: ClientMessageId,
    ) -> Result<()> {
        frame::write(
            &mut self.send,
            &ClientMessage::SendMessage {
                line,
                body: body.to_owned(),
                replies_to: None,
                client_message_id,
            },
        )
        .await
    }

    /// Takes the plug out of whatever Cage it is in.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn eject_plug(&mut self) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::EjectPlug).await
    }

    /// Announces the A.T. Field — the microphone being muted.
    ///
    /// Announced rather than kept local because the roster shows it: talking to
    /// somebody whose microphone is off is worth knowing, and the mute is only
    /// half a feature if nobody else can see it. `specs/07-tema-evangelion.md`
    /// gives it a marker in the roster for exactly this reason.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn set_at_field(&mut self, on: bool) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::SetAtField(on)).await
    }

    /// Announces Isolamento total — the speakers being muted.
    ///
    /// Local in effect, announced in the protocol. `specs/02-protocolo.md`:
    /// "Talking to somebody who cannot hear you is worth knowing."
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn set_total_isolation(&mut self, on: bool) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::SetTotalIsolation(on)).await
    }

    /// Announces a presence hint.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn set_presence(&mut self, presence: magi_proto::control::Presence) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::SetPresence(presence)).await
    }

    /// Asks for a page of history, oldest of the page first on the wire.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn fetch_history(
        &mut self,
        line: LineId,
        cursor: Option<MessageId>,
        limit: u16,
    ) -> Result<()> {
        frame::write(
            &mut self.send,
            &ClientMessage::FetchHistory {
                line,
                cursor,
                limit,
            },
        )
        .await
    }

    /// Waits for the next control message from the server.
    ///
    /// The shell drives this: `specs/01-arquitetura.md` has the core emit events
    /// and the shell turn them into pixels.
    ///
    /// # Errors
    ///
    /// Fails when the control stream closes.
    pub async fn next_event(&mut self) -> Result<ServerMessage> {
        let event = frame::read::<ServerMessage>(&mut self.recv).await?;
        // The round trip is measured here rather than in a separate reader:
        // two readers on one stream would race for every frame, and one of them
        // would swallow messages the other was waiting for.
        if let ServerMessage::Pong { timestamp } = event {
            if let Some((sent, at)) = self.pending_ping {
                if sent == timestamp {
                    self.last_rtt = Some(at.elapsed());
                    self.pending_ping = None;
                }
            }
        }
        Ok(event)
    }

    /// Sends a keepalive.
    ///
    /// `specs/02-protocolo.md` sends one every 5 s and treats three unanswered
    /// as `Reconectando` — which is what [`crate::Battery`] tracks. The answer
    /// arrives through [`Self::next_event`], because the control stream has
    /// exactly one reader.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn send_ping(&mut self) -> Result<()> {
        let stamp = self.pending_ping.map_or(1, |(previous, _)| previous + 1);
        self.pending_ping = Some((stamp, std::time::Instant::now()));
        frame::write(&mut self.send, &ClientMessage::Ping { timestamp: stamp }).await
    }

    /// The most recent round trip, if a ping has been answered.
    ///
    /// `specs/02-protocolo.md` makes this the base of the Sync Ratio — and the
    /// one input `magi-audio` cannot produce on its own, since it comes from the
    /// control stream.
    #[must_use]
    pub fn rtt(&self) -> Option<std::time::Duration> {
        self.last_rtt
    }

    /// Closes the connection.
    pub fn disconnect(&mut self) {
        self.pattern = Pattern::Offline;
        self.connection.close(0_u32.into(), b"ejected");
    }
}

/// Decides what a failed QUIC connection actually was.
///
/// The pin decision is consulted first: a certificate that changed produces a
/// TLS rejection like any other, and the difference between "this server is not
/// who it was" and "this server's certificate is unacceptable" is the whole of
/// ADR 0003.
fn classify_connection_error(
    error: &quinn::ConnectionError,
    pin: Option<PinDecision>,
) -> ConnectError {
    if let Some(PinDecision::Changed { pinned, offered }) = pin {
        return ConnectError::PinChanged { pinned, offered };
    }
    match error {
        quinn::ConnectionError::TimedOut => ConnectError::HandshakeTimeout,
        quinn::ConnectionError::TransportError(_) => {
            tracing::warn!(%error, "TLS refused the connection");
            ConnectError::TlsRefused
        }
        other => {
            tracing::warn!(error = %other, "could not establish the QUIC connection");
            ConnectError::Unreachable
        }
    }
}

async fn handshake(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    nickname: &str,
    signing_key: &SigningKey,
) -> Result<SessionInfo, ConnectError> {
    frame::write(
        send,
        &ClientMessage::Hello {
            version: magi_proto::PROTOCOL_VERSION,
            client: concat!("plug/", env!("CARGO_PKG_VERSION")).into(),
            nickname: nickname.to_owned(),
            public_key: signing_key.verifying_key().to_bytes().to_vec(),
        },
    )
    .await
    .map_err(|error| {
        tracing::warn!(%error, "could not send Hello");
        ConnectError::Unreachable
    })?;

    let challenge = frame::read::<ServerMessage>(recv).await.map_err(|error| {
        tracing::warn!(%error, "no Challenge came back");
        ConnectError::Unreachable
    })?;
    let nonce = match challenge {
        ServerMessage::Challenge { nonce } => nonce,
        // specs/02-protocolo.md: the reason is enumerated and specific, and it
        // survives all the way to the shell that has to explain it.
        ServerMessage::Disconnecting { reason } => {
            return Err(ConnectError::Refused { reason });
        }
        other => {
            tracing::warn!(?other, "expected Challenge");
            return Err(ConnectError::ProtocolViolation);
        }
    };

    frame::write(
        send,
        &ClientMessage::Response {
            proof: signing_key.sign(&nonce).to_bytes().to_vec(),
        },
    )
    .await
    .map_err(|error| {
        tracing::warn!(%error, "could not send the challenge response");
        ConnectError::Unreachable
    })?;

    let answer = frame::read::<ServerMessage>(recv).await.map_err(|error| {
        tracing::warn!(%error, "no Session came back");
        ConnectError::Unreachable
    })?;
    match answer {
        ServerMessage::Session {
            id,
            pilot,
            ssrc,
            dogma,
            cages,
            lines,
            ..
        } => Ok(SessionInfo {
            id,
            pilot,
            ssrc,
            dogma,
            cages,
            lines,
        }),
        ServerMessage::Disconnecting { reason } => Err(ConnectError::Refused { reason }),
        other => {
            tracing::warn!(?other, "expected Session");
            Err(ConnectError::ProtocolViolation)
        }
    }
}
