//! MELCHIOR — identity, authentication and one connection's session.
//!
//! # The handshake
//!
//! `specs/02-protocolo.md` draws it and this implements it exactly:
//!
//! ```text
//! Cliente                                Servidor
//!    │── QUIC ClientHello ──────────────────▶│   (TLS 1.3)
//!    │◀───────────────── ServerHello ────────│
//!    │── Ola { versao, cliente, apelido } ──▶│
//!    │◀── Desafio { nonce } ─────────────────│
//!    │── Resposta { prova } ────────────────▶│
//!    │◀── Sessao { id, dogma, cages, papeis }│   → PADRÃO: AZUL
//! ```
//!
//! Before `Sessao` the client is in **PADRÃO: LARANJA** — connected, not
//! verified. The whole budget is 10 s, and failure produces a **specific**
//! reason: `specs/02-protocolo.md` says "never generic".
//!
//! # What M2 can and cannot prove
//!
//! ADR 0004 makes identity an Ed25519 key. Verifying a signature over the
//! server's nonce proves the peer holds the private key — but with no
//! persistence in M2 (CASPER arrives in M3) there is no table of known accounts
//! to check that key against. So M2 authenticates a *key*, not a *person*. That
//! is the honest limit of a server with no database, and it is stated here
//! rather than left for somebody to assume otherwise.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use magi_proto::control::{
    CageInfo, ClientMessage, DisconnectReason, LineInfo, Permission, Presence, Role, ServerMessage,
};
use magi_proto::ids::{LineId, PilotId, RoleId, SessionId, Ssrc};
use magi_proto::transport::HANDSHAKE_TIMEOUT;
use tokio::sync::mpsc;

use crate::cage::CageCommand;
use crate::frame;
use crate::{DogmaConfig, PUBLIC_KEY_LEN};

/// Bytes of nonce the client signs.
const NONCE_LEN: usize = 32;

/// How many datagrams queue for one listener before the Cage starts shedding.
const OUTBOUND_DEPTH: usize = 256;

/// Hands out identifiers for the lifetime of a process.
///
/// M2 has no persistence, so these are per-run rather than stable. M3's CASPER
/// replaces the pilot side with real accounts.
pub struct Registry {
    next_pilot: AtomicU64,
    next_ssrc: AtomicU32,
    next_session: AtomicU64,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    /// A registry that has issued nothing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_pilot: AtomicU64::new(1),
            next_ssrc: AtomicU32::new(1),
            next_session: AtomicU64::new(1),
        }
    }

    fn issue(&self) -> (PilotId, Ssrc, SessionId) {
        (
            PilotId(self.next_pilot.fetch_add(1, Ordering::Relaxed)),
            Ssrc(self.next_ssrc.fetch_add(1, Ordering::Relaxed)),
            SessionId(self.next_session.fetch_add(1, Ordering::Relaxed)),
        )
    }
}

/// A connection that has reached PATTERN: BLUE.
pub struct Session {
    /// Who the server decided this connection is.
    pub pilot: PilotId,
    /// The media source bound to this connection.
    ///
    /// `specs/08-seguranca.md`: assigned here, never accepted from the client.
    /// Every datagram's header is checked against it — see [`crate::cage`].
    pub ssrc: Ssrc,
    /// Requested nickname, as accepted.
    pub nickname: String,
    /// Whether this pilot may transmit.
    pub may_speak: bool,
}

/// Runs the handshake, then the session, then cleans up.
///
/// # Errors
///
/// Returns the reason the connection ended. A handshake failure is reported to
/// the client as an enumerated [`DisconnectReason`] before the error propagates.
pub async fn serve(
    connection: quinn::Connection,
    config: Arc<DogmaConfig>,
    registry: Arc<Registry>,
    cage: mpsc::Sender<CageCommand>,
) -> Result<()> {
    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .context("client never opened the control stream")?;

    // specs/02-protocolo.md: 10 s for the whole handshake.
    let outcome = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        handshake(&mut send, &mut recv, &config, &registry),
    )
    .await;

    let session = match outcome {
        Ok(Ok(session)) => session,
        Ok(Err(failure)) => {
            // specs/02-protocolo.md: a specific reason, never a generic one.
            let _ = frame::write(
                &mut send,
                &ServerMessage::Disconnecting {
                    reason: failure.reason,
                },
            )
            .await;
            let _ = send.finish();
            bail!("handshake refused: {}", failure.detail);
        }
        Err(_elapsed) => {
            let _ = frame::write(
                &mut send,
                &ServerMessage::Disconnecting {
                    reason: DisconnectReason::HandshakeTimeout,
                },
            )
            .await;
            let _ = send.finish();
            bail!("handshake exceeded {HANDSHAKE_TIMEOUT:?}");
        }
    };

    tracing::info!(
        pilot = %session.pilot,
        ssrc = %session.ssrc,
        nickname = %session.nickname,
        may_speak = session.may_speak,
        "pattern blue"
    );

    let result = run_session(connection, send, recv, &session, &cage).await;

    let _ = cage
        .send(CageCommand::Leave {
            pilot: session.pilot,
        })
        .await;
    tracing::info!(pilot = %session.pilot, "session ended");
    result
}

/// A handshake that did not succeed, with the reason to send back.
struct Refusal {
    reason: DisconnectReason,
    detail: String,
}

async fn handshake(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    config: &DogmaConfig,
    registry: &Registry,
) -> std::result::Result<Session, Refusal> {
    let hello = frame::read::<ClientMessage>(recv)
        .await
        .map_err(|error| Refusal {
            reason: DisconnectReason::ProtocolViolation,
            detail: format!("could not read Hello: {error}"),
        })?;

    let ClientMessage::Hello {
        version,
        client,
        nickname,
        public_key,
    } = hello
    else {
        return Err(Refusal {
            reason: DisconnectReason::ProtocolViolation,
            detail: "first frame was not Hello".into(),
        });
    };

    magi_proto::version::negotiate(version).map_err(|_| Refusal {
        reason: DisconnectReason::Incompatible,
        detail: format!("client speaks protocol {version}"),
    })?;

    let key: [u8; PUBLIC_KEY_LEN] = public_key.try_into().map_err(|_| Refusal {
        reason: DisconnectReason::CredentialRejected,
        detail: "public key was not 32 bytes".into(),
    })?;
    let verifying = VerifyingKey::from_bytes(&key).map_err(|_| Refusal {
        reason: DisconnectReason::CredentialRejected,
        detail: "public key is not a valid Ed25519 point".into(),
    })?;

    // A fresh nonce per handshake. Reusing one would let a recorded Response be
    // replayed by anybody who saw it.
    let nonce: [u8; NONCE_LEN] = rand::random();
    frame::write(
        send,
        &ServerMessage::Challenge {
            nonce: nonce.to_vec(),
        },
    )
    .await
    .map_err(|error| Refusal {
        reason: DisconnectReason::ProtocolViolation,
        detail: format!("could not send Challenge: {error}"),
    })?;

    let response = frame::read::<ClientMessage>(recv)
        .await
        .map_err(|error| Refusal {
            reason: DisconnectReason::ProtocolViolation,
            detail: format!("could not read Response: {error}"),
        })?;

    let ClientMessage::Response { proof } = response else {
        return Err(Refusal {
            reason: DisconnectReason::ProtocolViolation,
            detail: "second frame was not Response".into(),
        });
    };

    let signature: [u8; 64] = proof.as_slice().try_into().map_err(|_| Refusal {
        reason: DisconnectReason::CredentialRejected,
        detail: "proof was not a 64-byte signature".into(),
    })?;

    // specs/08-seguranca.md requires a uniform failure message: nothing here
    // reveals whether the key is known, only whether the signature holds.
    verifying
        .verify(&nonce, &Signature::from_bytes(&signature))
        .map_err(|_| Refusal {
            reason: DisconnectReason::CredentialRejected,
            detail: "signature did not verify".into(),
        })?;

    let (pilot, ssrc, session_id) = registry.issue();
    // M2 has no accounts, so the only authorisation available is the operator's
    // configured list. M3's MELCHIOR replaces this with real roles.
    let may_speak = !config.observers.iter().any(|name| name == &nickname);

    let session = Session {
        pilot,
        ssrc,
        nickname: nickname.clone(),
        may_speak,
    };

    frame::write(
        send,
        &ServerMessage::Session {
            id: session_id,
            pilot,
            ssrc,
            dogma: config.name.clone(),
            cages: vec![CageInfo {
                id: config.cage,
                name: config.cage_name.clone(),
                limit: config.cage_limit,
                password_required: false,
                line: Some(LineId(1)),
            }],
            lines: vec![LineInfo {
                id: LineId(1),
                name: "geral".into(),
            }],
            roles: vec![
                Role {
                    id: RoleId(1),
                    name: "Pilot".into(),
                    permissions: vec![
                        Permission::ViewCage,
                        Permission::InsertPlug,
                        Permission::Speak,
                    ],
                },
                Role {
                    id: RoleId(2),
                    name: "Observer".into(),
                    permissions: vec![Permission::ViewCage, Permission::InsertPlug],
                },
            ],
        },
    )
    .await
    .map_err(|error| Refusal {
        reason: DisconnectReason::ProtocolViolation,
        detail: format!("could not send Session: {error}"),
    })?;

    let _ = client;
    Ok(session)
}

/// The session loop: control frames one way, datagrams both.
async fn run_session(
    connection: quinn::Connection,
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    session: &Session,
    cage: &mpsc::Sender<CageCommand>,
) -> Result<()> {
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Vec<u8>>(OUTBOUND_DEPTH);
    let mut in_cage = false;

    loop {
        tokio::select! {
            // Control frames from this client.
            frame = frame::read::<ClientMessage>(&mut recv) => {
                let Ok(message) = frame else { return Ok(()) };
                match message {
                    ClientMessage::InsertPlug { .. } => {
                        cage.send(CageCommand::Join {
                            pilot: session.pilot,
                            ssrc: session.ssrc,
                            may_speak: session.may_speak,
                            outbound: outbound_tx.clone(),
                        }).await?;
                        in_cage = true;
                        tracing::info!(pilot = %session.pilot, "plug inserted");
                    }
                    ClientMessage::EjectPlug => {
                        cage.send(CageCommand::Leave { pilot: session.pilot }).await?;
                        in_cage = false;
                    }
                    ClientMessage::Ping { timestamp } => {
                        frame::write(&mut send, &ServerMessage::Pong { timestamp }).await?;
                    }
                    ClientMessage::SetAtField(_)
                    | ClientMessage::SetTotalIsolation(_)
                    | ClientMessage::SetPresence(Presence::Available | Presence::Away | Presence::DoNotDisturb) => {
                        // Accepted and acknowledged by silence. Broadcasting
                        // state to the rest of the Cage needs the roster that
                        // arrives with M3.
                    }
                    other => {
                        tracing::debug!(?other, "message not implemented in M2");
                    }
                }
            }

            // Voice from this client.
            datagram = connection.read_datagram() => {
                let Ok(bytes) = datagram else { return Ok(()) };
                if !in_cage {
                    continue;
                }
                // The `ssrc` comes from the connection, never from the datagram.
                // The Cage compares the two and refuses a mismatch — gap G2.
                let _ = cage.send(CageCommand::Datagram {
                    from: session.ssrc,
                    bytes: bytes.to_vec(),
                }).await;
            }

            // Voice for this client.
            outbound = outbound_rx.recv() => {
                let Some(bytes) = outbound else { return Ok(()) };
                // A datagram that will not fit is dropped rather than queued:
                // late audio helps nobody.
                let _ = connection.send_datagram(bytes.into());
            }
        }
    }
}
