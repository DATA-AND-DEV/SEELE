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
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{CageId, ClientMessageId, LineId, MessageId, PilotId, SessionId, Ssrc};
use seele_proto::transport::{HANDSHAKE_TIMEOUT, IDLE_TIMEOUT, KEEPALIVE};

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
        reason: seele_proto::control::DisconnectReason,
    },
    /// The server said something that is not a handshake.
    ProtocolViolation,
    /// The invite promised another identity, and no pin could break the tie.
    ///
    /// Deliberately distinct from [`ConnectError::PinChanged`]: there a known
    /// server's key changed, which is the ADR 0003 alarm. Here nothing was ever
    /// known, and the party that disagrees is the link.
    InviteMismatch {
        /// What the link promised.
        expected: String,
        /// What the server offered.
        offered: String,
    },
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
    pub cages: Vec<seele_proto::control::CageInfo>,
    /// Text channels visible to us.
    ///
    /// Carried through rather than dropped: an interface that knows the Cages
    /// but not the Lines can only ever open whichever Line it was started with.
    pub lines: Vec<seele_proto::control::LineInfo>,
    /// What this pilot may do, as MELCHIOR resolved it.
    ///
    /// So a shell can decide whether to offer a control at all. **Convenience,
    /// never enforcement** — `specs/08-seguranca.md` puts the security in the
    /// server refusing, and it refuses again whatever this list says.
    pub permissions: Vec<seele_proto::control::Permission>,
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

/// How often flow control stopped either end of a connection.
///
/// Four counters and not `quinn::ConnectionStats` itself, deliberately. The
/// whole of `ConnectionStats` is quinn's type, and returning it would put the
/// version of a transport crate into this crate's public API — a shell that
/// reads one number would then have to be recompiled because a field it never
/// touched moved. These four are the ones that answer a question somebody
/// actually asks: **was the burst held back by the window, and in which
/// direction?**
///
/// `blocked` frames are QUIC's way of saying "I have data and no permission to
/// send it". Received means the peer said it about us — our side was not
/// draining fast enough. Sent means we said it about them.
///
/// Zero on both sides is the ordinary reading: nothing was ever held back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlowControl {
    /// `STREAM_DATA_BLOCKED` this end sent: one stream of ours ran out of room.
    pub stream_blocked_sent: u64,
    /// `STREAM_DATA_BLOCKED` this end received: the peer ran out of room on one
    /// stream, because this end was not reading it.
    pub stream_blocked_received: u64,
    /// `DATA_BLOCKED` this end sent: the whole connection ran out of room.
    pub connection_blocked_sent: u64,
    /// `DATA_BLOCKED` this end received.
    pub connection_blocked_received: u64,
}

impl From<&quinn::ConnectionStats> for FlowControl {
    fn from(stats: &quinn::ConnectionStats) -> Self {
        Self {
            stream_blocked_sent: stats.frame_tx.stream_data_blocked,
            stream_blocked_received: stats.frame_rx.stream_data_blocked,
            connection_blocked_sent: stats.frame_tx.data_blocked,
            connection_blocked_received: stats.frame_rx.data_blocked,
        }
    }
}

/// Stream priority for a transfer, below the control stream.
///
/// The other half of the pair lives in `seele_server::transfer`; the two are
/// separate crates by ADR 0002 and the numbers have to agree by hand. What
/// matters is the ordering, and it is written on both ends rather than left at
/// the default.
const TRANSFER_PRIORITY: i32 = -1;

/// Stream priority for the control stream, above every transfer.
const CONTROL_PRIORITY: i32 = 1;

/// How a transfer ended, from the sending end.
///
/// Three outcomes and not two, because the two that are not success mean
/// different things to whoever is watching the bar: one has an explanation
/// coming, and the other has nothing coming ever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// Every byte went out and the Dogma took the stream to its end.
    ///
    /// What follows is either the message appearing on the Line — which is how
    /// the sender learns it worked, matched by `client_message_id` — or an
    /// `AttachmentRefused` if what arrived did not hold together.
    Delivered {
        /// How many bytes went.
        bytes: u64,
    },
    /// The Dogma stopped the stream: it refused **before** taking the bytes.
    ///
    /// Not a failure of this end, and not something to retry blindly: the
    /// enumerated reason is on its way on the control stream, and it may well
    /// be one that says the file will never fit.
    Stopped {
        /// How many bytes had gone before the stop.
        bytes: u64,
    },
    /// The link fell in the middle.
    ///
    /// **The retry starts from zero.** ADR 0027 has no resumption: QUIC offers
    /// no continuation of a stream across connections, and offset-and-resume is
    /// a second design entire. A shell must say this rather than let it be
    /// discovered — the retry is safe, because the same `client_message_id`
    /// makes it idempotent, and it is not cheap.
    Interrupted {
        /// How many bytes had gone before it fell.
        bytes: u64,
    },
}

/// One file, on its way up, with the message it belongs to.
///
/// One argument rather than seven, three of which are strings that would
/// compile in each other's place.
#[derive(Debug, Clone, Copy)]
pub struct AttachmentRequest<'a> {
    /// Which Line the message goes to.
    pub line: LineId,
    /// The idempotency key of the message. Gap G9, and here it is also what
    /// makes a retry after a fall safe.
    pub client_message_id: ClientMessageId,
    /// What the sender wrote beside the file. May be empty.
    pub body: &'a str,
    /// What it replies to.
    pub replies_to: Option<MessageId>,
    /// Where the file is on this machine.
    ///
    /// Never sent. The Dogma stores the blob under the hash of its content, so
    /// nothing about this path crosses the wire.
    pub path: &'a std::path::Path,
    /// The name to give it on the other side.
    pub file_name: &'a str,
    /// The type to claim it is. A claim, and treated as one.
    pub declared_type: &'a str,
}

/// Tells a Dogma that refused apart from a link that fell.
///
/// `Stopped` is QUIC's `STOP_SENDING`: somebody at the other end decided, on
/// purpose, not to take these bytes. Anything else is the connection going
/// away, which nobody decided.
fn interpret(error: &quinn::WriteError, sent: u64) -> Sent {
    if matches!(error, quinn::WriteError::Stopped(_)) {
        Sent::Stopped { bytes: sent }
    } else {
        Sent::Interrupted { bytes: sent }
    }
}

/// The same reading, for a header that could not be written.
fn interpret_any(error: &anyhow::Error, sent: u64) -> Sent {
    error
        .downcast_ref::<quinn::WriteError>()
        .map_or(Sent::Interrupted { bytes: sent }, |write| {
            interpret(write, sent)
        })
}

/// Marks a downloaded file with the operating system's own quarantine.
///
/// ADR 0027: this product does not scan for viruses and will not. What it can
/// do is set the flag Gatekeeper and SmartScreen already read — the same thing
/// a browser does when it saves a download, and the concrete answer this
/// product **can** give. Best effort: a filesystem without extended attributes
/// is a reason to carry on, not to throw the file away.
fn quarantine(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        let value = format!(
            "0083;{:x};SEELE;",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs())
        );
        let _ = std::process::Command::new("xattr")
            .args(["-w", "com.apple.quarantine", &value])
            .arg(path)
            .output();
    }
    #[cfg(target_os = "windows")]
    {
        // Zone 3 is "Internet", which is what makes SmartScreen stop the file.
        let stream = format!("{}:Zone.Identifier", path.display());
        let _ = std::fs::write(stream, "[ZoneTransfer]\r\nZoneId=3\r\n");
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Linux has no system-wide equivalent that anything reads. Saying so is
        // better than writing an attribute nobody consults and calling it a
        // guard.
        let _ = path;
    }
}

/// A connected, verified client.
pub struct Client {
    connection: quinn::Connection,
    send: quinn::SendStream,
    /// Mensagens já decodificadas, vindas da tarefa que é dona do fluxo.
    ///
    /// O fluxo de leitura não fica aqui de propósito — ver [`Client::next_event`].
    inbox: tokio::sync::mpsc::UnboundedReceiver<ServerMessage>,
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
    /// `signing_key` is this client's identity (ADR 0004). ADR 0017 keeps it on
    /// disk, because CASPER binds a nickname to the identity that claimed it.
    ///
    /// `join_secret` is the invite token or the Dogma password, when the Dogma
    /// asks for one. `None` for an open Dogma, which is the default.
    ///
    /// `server_name` is what TLS is told; `pin_key` is what the pin is filed
    /// under. They are separate on purpose — the TOFU verifier compares
    /// fingerprints and never looks at the certificate's names, so the TLS name
    /// is a label and the pin key is the policy. Passing the same value for
    /// both is fine for a hostname and **wrong for an IP address**, where the
    /// TLS name has to be something the certificate carries and the pin key has
    /// to be the address the pilot typed.
    ///
    /// # Errors
    ///
    /// A [`ConnectError`] variant, never a message. Every caller has to be able
    /// to say which of these happened in its own words.
    pub async fn connect(
        server: SocketAddr,
        server_name: &str,
        pin_key: &str,
        nickname: &str,
        signing_key: &SigningKey,
        pins: Arc<dyn PinStore>,
        join_secret: Option<&str>,
    ) -> Result<Self, ConnectError> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let verifier = Arc::new(TofuVerifier::new(pins, pin_key.to_owned()));
        let mut tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::clone(&verifier) as Arc<_>)
            .with_no_client_auth();
        tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];

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

        let mut endpoint = local_endpoint()?;
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
        let pin = verifier.last_decision().unwrap_or(PinDecision::Matches {
            fingerprint: String::new(),
        });

        let (mut send, mut recv) = connection.open_bi().await.map_err(|error| {
            tracing::warn!(%error, "could not open the control stream");
            ConnectError::Unreachable
        })?;
        // Control above every transfer, on this end too. ADR 0027; the ordering
        // only holds inside this one connection, and that limit is written down
        // in `seele_server::transfer` rather than implied here.
        let _ = send.set_priority(CONTROL_PRIORITY);
        let session = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            handshake(&mut send, &mut recv, nickname, signing_key, join_secret),
        )
        .await
        .map_err(|_| ConnectError::HandshakeTimeout)??;

        // Uma tarefa é dona do fluxo de leitura e entrega quadros inteiros por
        // um canal. Nada mais lê deste fluxo, então não há corrida por quadro;
        // e como ninguém cancela um `read_exact` no meio, o fluxo não
        // dessincroniza.
        let (para_dentro, inbox) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut recv = recv;
            loop {
                match frame::read::<ServerMessage>(&mut recv).await {
                    Ok(mensagem) => {
                        if para_dentro.send(mensagem).is_err() {
                            // Ninguém mais escuta: o `Client` foi descartado.
                            return;
                        }
                    }
                    Err(erro) => {
                        tracing::debug!(%erro, "o fluxo de controle terminou");
                        return;
                    }
                }
            }
        });

        Ok(Self {
            connection,
            send,
            inbox,
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

    // ---- attachments, ADR 0027 ----

    /// Sends a file on a stream of its own, and reports the bytes as they go.
    ///
    /// **Never on the control stream.** Twenty megabytes written there stop
    /// every presence event and every line of text from everybody behind them
    /// until the last byte goes through; the control stream is the one that may
    /// not queue.
    ///
    /// The message is published on the Line **only once the bytes have
    /// arrived whole**, so while this is running the only person who can see it
    /// is the sender. That is the trade ADR 0027 takes on purpose: without it,
    /// "the file has not arrived yet" and "the file expired" would be two
    /// similar absences on the same screen.
    ///
    /// `progress` is called with `(sent, total)` after each block. The total is
    /// always known — whoever chose the file knows how big it is — so a shell
    /// always has a real bar and never one pretending to measure something
    /// nobody measured.
    ///
    /// **A transfer that falls starts again from zero.** There is no resumption
    /// (ADR 0027), and a shell must say so rather than let it be discovered. The
    /// retry is safe: it is keyed by the same `client_message_id` that already
    /// makes a send idempotent, so an attempt that had in fact succeeded is
    /// recognised instead of stored twice.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read or the stream is closed. A **refusal**
    /// is not a failure here: it comes back later as
    /// [`ServerMessage::AttachmentRefused`] on the control stream, because
    /// `specs/02-protocolo.md` keeps every reason enumerated and on control.
    pub async fn send_attachment(
        &mut self,
        request: &AttachmentRequest<'_>,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<Sent> {
        use seele_proto::attachment::{AttachmentHeader, ContentDigest, BLOCK_LEN};
        use tokio::io::AsyncReadExt;

        // Two passes over a local file: one to know the digest, one to send it.
        // The header has to carry the hash, and the hash is not known until the
        // last byte — and holding the file in memory to avoid the second read
        // is the thing this design refuses everywhere else.
        let mut file = tokio::fs::File::open(request.path).await?;
        let declared_len = file.metadata().await?.len();
        let mut digest = ContentDigest::new();
        let mut block = vec![0_u8; BLOCK_LEN];
        loop {
            let read = file.read(&mut block).await?;
            if read == 0 {
                break;
            }
            digest.feed(block.get(..read).unwrap_or_default());
        }
        let content_hash = digest.finish();

        let mut stream = self.connection.open_uni().await?;
        // Below the control stream, so that sending a picture cannot delay
        // somebody else's `Pong` on this connection. It says nothing about
        // another connection — see ADR 0027 on what has no way out.
        stream.set_priority(TRANSFER_PRIORITY)?;
        let header = AttachmentHeader {
            line: request.line,
            client_message_id: request.client_message_id,
            body: request.body.to_owned(),
            replies_to: request.replies_to,
            file_name: request.file_name.to_owned(),
            declared_type: request.declared_type.to_owned(),
            declared_len,
            content_hash,
        };
        // Validated here rather than on arrival alone, so a file with an
        // unusable name fails before a byte leaves this machine.
        if let Err(error) = frame::write(&mut stream, &header).await {
            return Ok(interpret_any(&error, 0));
        }

        let mut file = tokio::fs::File::open(request.path).await?;
        let mut sent = 0_u64;
        progress(0, declared_len);
        loop {
            let read = file.read(&mut block).await?;
            if read == 0 {
                break;
            }
            // A write that fails is not this end's failure. The Dogma refusing
            // stops the stream **before** taking the bytes — which is the right
            // thing for it to do, and the reason is already travelling on the
            // control stream — and a link that fell looks almost the same here.
            // Telling them apart is what lets a shell say "the file will never
            // fit" in one case and "it fell, and starting again starts from
            // zero" in the other.
            if let Err(error) = stream
                .write_all(block.get(..read).unwrap_or_default())
                .await
            {
                return Ok(interpret(&error, sent));
            }
            sent = sent.saturating_add(read as u64);
            progress(sent, declared_len);
        }
        if stream.finish().is_err() {
            return Ok(Sent::Stopped { bytes: sent });
        }
        // Waits for the peer to acknowledge the end of the stream. Without it,
        // returning would drop the send half and the last blocks could still be
        // unsent — which reads, from the other end, as a transfer that fell.
        match stream.stopped().await {
            Ok(_) => Ok(Sent::Delivered { bytes: sent }),
            Err(_) => Ok(Sent::Interrupted { bytes: sent }),
        }
    }

    /// Asks for an attachment and writes it where the receiver chose.
    ///
    /// **The destination is the caller's**, and there is no other kind: ADR 0027
    /// gives no client of the SEELE a button that opens a file. Saving is an act
    /// of the person who received it, in a place they picked.
    ///
    /// The file is marked with the operating system's own quarantine on the way
    /// down — `com.apple.quarantine`, or the `Zone.Identifier` stream — so that
    /// Gatekeeper and SmartScreen stop it in front of whoever opens it later.
    /// That is not antivirus, and this product does not have one; it is the
    /// guard the system already has, and it only works if whoever writes the
    /// file turns it on.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written, if the bytes do not hash to what the
    /// Dogma said, or if nothing arrives inside `wait`. A refusal arrives
    /// separately, as [`ServerMessage::AttachmentUnavailable`] on the control
    /// stream — that is where the enumerated reason lives, and the expected one
    /// is `Expired`.
    pub async fn download_attachment(
        &mut self,
        attachment: seele_proto::ids::AttachmentId,
        destination: &std::path::Path,
        wait: std::time::Duration,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<u64> {
        use seele_proto::attachment::{AttachmentDelivery, ContentDigest, BLOCK_LEN};
        use tokio::io::AsyncWriteExt;

        frame::write(
            &mut self.send,
            &ClientMessage::FetchAttachment { attachment },
        )
        .await?;

        // The Dogma opens the stream, so this waits for it. A refusal never
        // opens one, which is why there is a deadline: the reason is on its way
        // through `next_event` and this call must not hang for ever waiting for
        // bytes that were never coming.
        let mut stream = tokio::time::timeout(wait, self.connection.accept_uni())
            .await
            .map_err(|_| anyhow::anyhow!("o Dogma não mandou o arquivo em {wait:?}"))??;

        let delivery: AttachmentDelivery = frame::read(&mut stream).await?;
        anyhow::ensure!(
            delivery.attachment == attachment,
            "o Dogma mandou o anexo {} no lugar de {attachment}",
            delivery.attachment
        );

        let mut file = tokio::fs::File::create(destination).await?;
        let mut digest = ContentDigest::new();
        let mut block = vec![0_u8; BLOCK_LEN];
        let mut got = 0_u64;
        progress(0, delivery.byte_size);
        while let Some(read) = stream.read(&mut block).await? {
            if read == 0 {
                break;
            }
            let piece = block.get(..read).unwrap_or_default();
            file.write_all(piece).await?;
            digest.feed(piece);
            got = got.saturating_add(read as u64);
            progress(got, delivery.byte_size);
        }
        file.flush().await?;
        drop(file);

        // The same question the Dogma asked on the way in: did it arrive whole.
        // It says nothing about the file being good, and nothing here pretends
        // it does.
        if digest.finish() != delivery.content_hash || got != delivery.byte_size {
            let _ = tokio::fs::remove_file(destination).await;
            anyhow::bail!("o arquivo não chegou inteiro e foi descartado");
        }

        quarantine(destination);
        Ok(got)
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
    pub async fn set_presence(&mut self, presence: seele_proto::control::Presence) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::SetPresence(presence)).await
    }

    /// Asks the Dogma to make a Cage.
    ///
    /// Asks, and nothing more. Nothing here checks whether this pilot may:
    /// `specs/08-seguranca.md` puts the decision on the server, and a core that
    /// refused on its own would be a second authority to keep in step with the
    /// first. What comes back is a `CageCreated` on the event stream if it
    /// happened, or an `Alert` carrying `PermissionDenied` if it did not.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn create_cage(
        &mut self,
        name: &str,
        limit: u16,
        line: Option<LineId>,
    ) -> Result<()> {
        frame::write(
            &mut self.send,
            &ClientMessage::CreateCage {
                name: name.to_owned(),
                limit,
                line,
            },
        )
        .await
    }

    /// Asks the Dogma to make a Line.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn create_line(&mut self, name: &str) -> Result<()> {
        frame::write(
            &mut self.send,
            &ClientMessage::CreateLine {
                name: name.to_owned(),
            },
        )
        .await
    }

    /// Asks the Dogma to rename a Cage.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn rename_cage(&mut self, cage: CageId, name: &str) -> Result<()> {
        frame::write(
            &mut self.send,
            &ClientMessage::RenameCage {
                cage,
                name: name.to_owned(),
            },
        )
        .await
    }

    /// Asks the Dogma to rename a Line.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn rename_line(&mut self, line: LineId, name: &str) -> Result<()> {
        frame::write(
            &mut self.send,
            &ClientMessage::RenameLine {
                line,
                name: name.to_owned(),
            },
        )
        .await
    }

    /// Asks the Dogma to end a pilot's session.
    ///
    /// Asks, like the four above, and for the same reason: nothing here checks
    /// whether this pilot may. `specs/08-seguranca.md` puts that on the server,
    /// and a core that refused on its own would be a second authority to keep
    /// in step with the first — and the one that would drift, because it is the
    /// one nobody re-reads.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn kick_pilot(&mut self, pilot: PilotId) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::KickPilot { pilot }).await
    }

    /// Asks the Dogma to bar a pilot from returning.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn ban_pilot(
        &mut self,
        pilot: PilotId,
        reason: Option<&str>,
        expires_at: Option<i64>,
    ) -> Result<()> {
        frame::write(
            &mut self.send,
            &ClientMessage::BanPilot {
                pilot,
                reason: reason.map(str::to_owned),
                expires_at,
            },
        )
        .await
    }

    /// Asks the Dogma to take a message off its Line.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn remove_message(&mut self, message: MessageId) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::RemoveMessage { message }).await
    }

    /// Asks the Dogma to move a pilot into a Cage.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn move_pilot(&mut self, pilot: PilotId, cage: CageId) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::MovePilot { pilot, cage }).await
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
        // Recebe de um canal, e não do fluxo QUIC direto. **Isto é o que torna
        // a função segura de cancelar**, e a diferença não é acadêmica: as duas
        // cascas chamam `next_event` dentro de um `tokio::select!`, que descarta
        // os ramos perdedores a cada volta. Uma tecla, uma tica de telemetria
        // ou um ping cancelariam a leitura em andamento.
        //
        // `frame::read` faz dois `read_exact` — o tamanho e depois o corpo. Ser
        // cancelado entre eles consome bytes que ninguém guardou, e o fluxo
        // fica dessincronizado **para sempre**: toda leitura seguinte lê lixo.
        // Em loopback o quadro inteiro costuma chegar num pacote só e a leitura
        // termina sem ceder, e é por isso que isto nunca apareceu aqui — só num
        // runner de CI carregado.
        //
        // `mpsc::Receiver::recv` é seguro de cancelar por contrato: nada se
        // perde se ninguém estiver esperando.
        let event = self
            .inbox
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("o fluxo de controle fechou"))?;
        // O tempo de ida e volta é medido aqui, e não na tarefa que lê: o
        // `Ping` é enviado por este objeto, e só ele sabe quando.
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

    /// What the transport says about flow control on this connection.
    ///
    /// Read from `quinn`'s own frame counters rather than instrumented by hand:
    /// the numbers that decide whether a burst was held back by the QUIC window
    /// are already counted one layer down, and a second count kept up here would
    /// only ever disagree with the first.
    ///
    /// This exists because of `docs/pendencias.md` #1, whose first suspicion was
    /// the flow-control window at the start of a connection. A suspicion like
    /// that is either killed with a number or carried for ever — and the number
    /// has to be readable from the machine where the symptom reproduces, which
    /// is not necessarily the machine anybody is developing on.
    #[must_use]
    pub fn flow_control(&self) -> FlowControl {
        FlowControl::from(&self.connection.stats())
    }

    /// The most recent round trip, if a ping has been answered.
    ///
    /// `specs/02-protocolo.md` makes this the base of the Sync Ratio — and the
    /// one input `seele-audio` cannot produce on its own, since it comes from the
    /// control stream.
    #[must_use]
    pub fn rtt(&self) -> Option<std::time::Duration> {
        self.last_rtt
    }

    /// Closes the connection: the pilot ejected.
    pub fn disconnect(&mut self) {
        self.close(EJECTED);
    }

    /// Closes the connection, naming what closed it.
    ///
    /// The reason travels in the QUIC `CONNECTION_CLOSE` frame and is what the
    /// Dogma's log records, so it is the only trace the other side keeps of why
    /// a session that had already completed the handshake went away. Every
    /// close that is not a pilot leaving has to say so here: an invite that did
    /// not match used to be recorded as an ejection, which is the one reading
    /// that hides a refusal.
    pub fn close(&mut self, reason: &[u8]) {
        self.pattern = Pattern::Offline;
        self.connection.close(0_u32.into(), reason);
    }

    // ---- unmaking a room ----
    //
    // Appended at the end of the impl on purpose, and not beside `rename_cage`
    // where they belong by subject: `crates/seele-core/src/client.rs` is being
    // worked on elsewhere at the same time, around the message and event paths
    // in the middle of this file. Three additions with nothing above or below
    // them is the smallest thing that can be added here.
    //
    // Asks, and nothing more, like every other verb in here. Nothing checks
    // whether this pilot may — `specs/08-seguranca.md` puts that on the server,
    // and a core that refused on its own would be a second authority to keep in
    // step with the first.

    /// Asks the Dogma to destroy a Cage.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn delete_cage(&mut self, cage: CageId) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::DeleteCage { cage }).await
    }

    /// Asks the Dogma to destroy a Line, and everything written in it.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn delete_line(&mut self, line: LineId) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::DeleteLine { line }).await
    }

    /// Asks what destroying a Line would cost. Destroys nothing.
    ///
    /// The answer arrives as a `LineWeighed` on the event stream, like every
    /// other answer this client gets.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn weigh_line(&mut self, line: LineId) -> Result<()> {
        frame::write(&mut self.send, &ClientMessage::WeighLine { line }).await
    }
}

/// A pilot left. The ordinary end of a session.
const EJECTED: &[u8] = b"ejected";

/// The client refused the server: the invite named a different key.
///
/// `seele_core::enlace::Enlace::conectar` drops the connection when
/// `tofu::verdict` returns `InviteRefused`, and this is what the Dogma sees.
/// The string matters more than it looks: `docs/pendencias.md` #12 accepted
/// that the explicit drop has no automatic guard precisely because this reason
/// is the one remaining observable difference between dropping the connection
/// and letting it die on its own.
pub const INVITE_REFUSED: &[u8] = b"invite refused";

/// The local socket a client sends from.
///
/// # Why `[::]` and not `0.0.0.0`
///
/// An IPv4 socket cannot send to an IPv6 destination at all. A client bound to
/// `0.0.0.0` therefore could not reach a Dogma over IPv6 no matter what the
/// Dogma did — which is the other half of why step 2 of ADR 0022 had not
/// started, and the half that no amount of work on the server would have fixed.
///
/// An IPv6 socket with `IPV6_V6ONLY` cleared reaches both families, and quinn
/// does both halves of that itself, which is why this is short: `client` clears
/// the option when the address is IPv6, and `connect` rewrites an IPv4
/// destination into its IPv4-mapped form (`::ffff:a.b.c.d`) before the socket
/// sees it. Its own documentation calls `[::]:0` "a reasonable default to
/// maximize the ability to connect".
///
/// # Why the fallback is not decoration
///
/// A machine with IPv6 switched off cannot open an `AF_INET6` socket, so the
/// bind itself fails. Falling back leaves such a machine working exactly as it
/// did before this change — IPv4 only, which is all it ever had.
///
/// # What is deliberately not checked here
///
/// The server reads `IPV6_V6ONLY` back after setting it (`alcance::abrir_escuta`
/// in `seele-server`), because a Dogma that silently stops answering half the
/// internet is a bug nobody can diagnose from the outside. This side does not:
/// quinn owns the socket and does not lend it out, and on the four targets
/// `deny.toml` builds for — Linux, the two macOS, Windows — clearing the option
/// works. A platform that refuses it would leave a client that reaches IPv6 and
/// not IPv4, and that is the trade being taken knowingly rather than missed.
fn local_endpoint() -> Result<quinn::Endpoint, ConnectError> {
    match quinn::Endpoint::client(SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0))) {
        Ok(endpoint) => Ok(endpoint),
        Err(error) => {
            tracing::info!(
                %error,
                "no IPv6 on this machine: falling back to an IPv4-only local socket"
            );
            quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0))).map_err(|error| {
                tracing::error!(%error, "could not open a local QUIC endpoint");
                ConnectError::LocalEndpoint
            })
        }
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
    join_secret: Option<&str>,
) -> Result<SessionInfo, ConnectError> {
    frame::write(
        send,
        &ClientMessage::Hello {
            version: seele_proto::PROTOCOL_VERSION,
            client: concat!("plug/", env!("CARGO_PKG_VERSION")).into(),
            nickname: nickname.to_owned(),
            public_key: signing_key.verifying_key().to_bytes().to_vec(),
            join_secret: join_secret.map(str::to_owned),
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
            permissions,
            ..
        } => Ok(SessionInfo {
            id,
            pilot,
            ssrc,
            dogma,
            cages,
            lines,
            permissions,
        }),
        ServerMessage::Disconnecting { reason } => Err(ConnectError::Refused { reason }),
        other => {
            tracing::warn!(?other, "expected Session");
            Err(ConnectError::ProtocolViolation)
        }
    }
}
