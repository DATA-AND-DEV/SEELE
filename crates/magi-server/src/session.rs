//! MELCHIOR's front door — one connection's handshake and session.
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
//! # What the key proves, and what MELCHIOR decides
//!
//! Verifying the signature over the nonce proves the peer holds the private key.
//! Turning that into an identity is [`crate::melchior`]'s job: it looks the key
//! up, creates an account on first sight, and refuses a banned one. Roles and
//! permissions come from there too — `specs/08-seguranca.md` is emphatic that
//! the server denying is the security, and the interface hiding the button only
//! convenience.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use magi_proto::control::{
    AlertReason, AlertSeverity, CageInfo, ClientMessage, DisconnectReason, LineInfo, Permission,
    PilotProfile, PilotState, Presence, Role, ServerMessage, Subsystem, SubsystemHealth, Telemetry,
};
use magi_proto::ids::{CageId, LineId, PilotId, RoleId, SessionId, Ssrc};
use magi_proto::sync_ratio::{SyncInputs, SyncRatio};
use magi_proto::transport::HANDSHAKE_TIMEOUT;
use tokio::sync::mpsc;

use crate::cage::CageCommand;
use crate::casper::messages::{Messages, PendingMessage, DEFAULT_PAGE};
use crate::casper::Casper;
use crate::dogma::{Dogma, Event};
use crate::melchior::{self, Melchior};
use crate::{frame, DogmaConfig, PUBLIC_KEY_LEN};

/// Bytes of nonce the client signs.
const NONCE_LEN: usize = 32;

/// How many datagrams queue for one listener before the Cage sheds.
const OUTBOUND_DEPTH: usize = 256;

/// How often the server pushes telemetry.
///
/// `specs/07-tema-evangelion.md` wants the Sync Ratio alive on screen; once a
/// second looks live and costs nothing.
const TELEMETRY_INTERVAL: Duration = Duration::from_secs(1);

/// Hands out per-connection identifiers.
///
/// Pilot identifiers come from CASPER and survive restarts; these do not need
/// to. An `ssrc` is meaningful only for the life of a connection.
pub struct Registry {
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
            next_ssrc: AtomicU32::new(1),
            next_session: AtomicU64::new(1),
        }
    }

    fn issue(&self) -> (Ssrc, SessionId) {
        (
            Ssrc(self.next_ssrc.fetch_add(1, Ordering::Relaxed)),
            SessionId(self.next_session.fetch_add(1, Ordering::Relaxed)),
        )
    }
}

/// A connection that has reached PATTERN: BLUE.
pub struct Session {
    /// Which account this connection is.
    pub pilot: PilotId,
    /// The media source bound to this connection.
    pub ssrc: Ssrc,
    /// Display name.
    pub nickname: String,
    /// May transmit voice.
    pub may_speak: bool,
    /// May post text.
    pub may_write: bool,
    /// A seat reclaimed from an earlier connection, if any.
    pub reclaimed_cage: Option<CageId>,
}

/// Runs the handshake, then the session, then cleans up.
///
/// # Errors
///
/// Returns the reason the connection ended.
pub async fn serve(
    connection: quinn::Connection,
    config: Arc<DogmaConfig>,
    registry: Arc<Registry>,
    dogma: Arc<Dogma>,
    cage: mpsc::Sender<CageCommand>,
) -> Result<()> {
    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .context("client never opened the control stream")?;

    let outcome = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        handshake(&mut send, &mut recv, &config, &registry, &dogma),
    )
    .await;

    let session = match outcome {
        Ok(Ok(session)) => session,
        Ok(Err(failure)) => {
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
        reclaimed = ?session.reclaimed_cage,
        "pattern blue"
    );

    let result = run_session(connection, send, recv, &session, &dogma, &cage).await;

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
    dogma: &Dogma,
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

    let key: [u8; PUBLIC_KEY_LEN] = public_key.clone().try_into().map_err(|_| Refusal {
        reason: DisconnectReason::CredentialRejected,
        detail: "public key was not 32 bytes".into(),
    })?;
    let verifying = VerifyingKey::from_bytes(&key).map_err(|_| Refusal {
        reason: DisconnectReason::CredentialRejected,
        detail: "public key is not a valid Ed25519 point".into(),
    })?;

    // A fresh nonce per handshake, or a recorded Response could be replayed.
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

    let ClientMessage::Response { proof } =
        frame::read::<ClientMessage>(recv)
            .await
            .map_err(|error| Refusal {
                reason: DisconnectReason::ProtocolViolation,
                detail: format!("could not read Response: {error}"),
            })?
    else {
        return Err(Refusal {
            reason: DisconnectReason::ProtocolViolation,
            detail: "second frame was not Response".into(),
        });
    };

    let signature: [u8; 64] = proof.as_slice().try_into().map_err(|_| Refusal {
        reason: DisconnectReason::CredentialRejected,
        detail: "proof was not a 64-byte signature".into(),
    })?;

    // specs/08-seguranca.md wants a uniform failure: nothing here says whether
    // the key is known, only whether the signature holds.
    verifying
        .verify(&nonce, &Signature::from_bytes(&signature))
        .map_err(|_| Refusal {
            reason: DisconnectReason::CredentialRejected,
            detail: "signature did not verify".into(),
        })?;

    // MELCHIOR turns the proven key into an account.
    let account = {
        let guard = dogma.casper.lock().await;
        let melchior = Melchior::new(&guard);

        let pilot = melchior
            .register_or_find(&public_key, &nickname)
            .map_err(|error| Refusal {
                reason: DisconnectReason::CredentialRejected,
                detail: format!("could not establish an account: {error}"),
            })?;

        if melchior.is_banned(pilot.id).unwrap_or(false) {
            return Err(Refusal {
                reason: DisconnectReason::Banned,
                detail: format!("pilot {} is banned", pilot.id),
            });
        }

        // Bootstrap: somebody has to be able to set the first roles before there
        // is an operator to do it. Applied through MELCHIOR rather than around
        // it, so authorisation still has exactly one source of truth
        // (`specs/08-seguranca.md`).
        if config.observers.iter().any(|name| name == &nickname) {
            let _ = melchior.revoke_role(pilot.id, melchior::PILOT_ROLE);
            let _ = melchior.grant_role(pilot.id, melchior::OBSERVER_ROLE);
        }

        let may = |permission| melchior.may(pilot.id, permission).unwrap_or(false);
        let (cages, lines, roles) = read_dogma(&guard).map_err(|error| Refusal {
            reason: DisconnectReason::ServerShuttingDown,
            detail: format!("could not read the Dogma: {error}"),
        })?;

        Account {
            id: pilot.id,
            nickname: pilot.nickname,
            may_speak: may(Permission::Speak),
            may_write: may(Permission::WriteLine),
            cages,
            lines,
            roles,
        }
    };

    let (fresh_ssrc, session_id) = registry.issue();

    // specs/02-protocolo.md: the server holds the slot for the same five minutes
    // as the client's internal battery. A pilot returning inside that window
    // gets their own seat and their own `ssrc` back, so to everybody else the
    // outage looks like an outage rather than a departure and an arrival.
    let reclaimed = {
        let mut slots = dogma.slots.lock().await;
        slots.reclaim(account.id, Instant::now())
    };
    let (ssrc, reclaimed_cage) = match reclaimed {
        Some((cage, ssrc)) => (ssrc, Some(cage)),
        None => (fresh_ssrc, None),
    };

    frame::write(
        send,
        &ServerMessage::Session {
            id: session_id,
            pilot: account.id,
            ssrc,
            dogma: config.name.clone(),
            cages: account.cages,
            lines: account.lines,
            roles: account.roles,
        },
    )
    .await
    .map_err(|error| Refusal {
        reason: DisconnectReason::ProtocolViolation,
        detail: format!("could not send Session: {error}"),
    })?;

    let _ = client;
    Ok(Session {
        pilot: account.id,
        ssrc,
        nickname: account.nickname,
        may_speak: account.may_speak,
        may_write: account.may_write,
        reclaimed_cage,
    })
}

/// What the handshake learned from MELCHIOR and CASPER.
struct Account {
    id: PilotId,
    nickname: String,
    may_speak: bool,
    may_write: bool,
    cages: Vec<CageInfo>,
    lines: Vec<LineInfo>,
    roles: Vec<Role>,
}

/// Reads the Cage and Line tree, and the roles, out of CASPER.
fn read_dogma(casper: &Casper) -> Result<(Vec<CageInfo>, Vec<LineInfo>, Vec<Role>)> {
    let connection = casper.connection();

    let mut cage_statement = connection.prepare(
        "SELECT id, name, member_limit, password_hash IS NOT NULL, line_id
         FROM cages ORDER BY position, id",
    )?;
    let cages = cage_statement
        .query_map([], |row| {
            Ok(CageInfo {
                id: CageId(row.get::<_, i64>(0)? as u32),
                name: row.get(1)?,
                limit: row.get::<_, i64>(2)? as u16,
                password_required: row.get(3)?,
                line: row.get::<_, Option<i64>>(4)?.map(|id| LineId(id as u32)),
            })
        })?
        .filter_map(Result::ok)
        .collect();

    let mut line_statement =
        connection.prepare("SELECT id, name FROM lines ORDER BY position, id")?;
    let lines = line_statement
        .query_map([], |row| {
            Ok(LineInfo {
                id: LineId(row.get::<_, i64>(0)? as u32),
                name: row.get(1)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    let mut role_statement = connection.prepare("SELECT id, name, permissions FROM roles")?;
    let roles = role_statement
        .query_map([], |row| {
            let permissions: String = row.get(2)?;
            Ok(Role {
                id: RoleId(row.get::<_, i64>(0)? as u32),
                name: row.get(1)?,
                permissions: melchior::permissions_from_json(&permissions),
            })
        })?
        .filter_map(Result::ok)
        .collect();

    Ok((cages, lines, roles))
}

/// The session loop.
#[allow(clippy::too_many_lines, reason = "one select over every event source")]
async fn run_session(
    connection: quinn::Connection,
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    session: &Session,
    dogma: &Dogma,
    cage: &mpsc::Sender<CageCommand>,
) -> Result<()> {
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Vec<u8>>(OUTBOUND_DEPTH);
    let mut events = dogma.events.subscribe();
    let mut lines: Vec<LineId> = Vec::new();
    let mut current_cage: Option<CageId> = None;
    let mut sync = SyncRatio::new();
    let mut telemetry = tokio::time::interval(TELEMETRY_INTERVAL);
    telemetry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // A reclaimed seat means the pilot was already in a Cage when they dropped.
    if let Some(reclaimed) = session.reclaimed_cage {
        cage.send(CageCommand::Join {
            pilot: session.pilot,
            ssrc: session.ssrc,
            may_speak: session.may_speak,
            outbound: outbound_tx.clone(),
        })
        .await?;
        current_cage = Some(reclaimed);
        dogma.occupancy.lock().await.seat(
            reclaimed,
            crate::dogma::Occupant {
                pilot: session.pilot,
                nickname: session.nickname.clone(),
                ssrc: session.ssrc,
            },
        );
        tracing::info!(pilot = %session.pilot, "seat reclaimed");
    }

    loop {
        tokio::select! {
            incoming = frame::read::<ClientMessage>(&mut recv) => {
                let Ok(message) = incoming else { break };
                match message {
                    ClientMessage::InsertPlug { cage: id, .. } => {
                        cage.send(CageCommand::Join {
                            pilot: session.pilot,
                            ssrc: session.ssrc,
                            may_speak: session.may_speak,
                            outbound: outbound_tx.clone(),
                        }).await?;
                        current_cage = Some(id);

                        // Who was already here — gap G15. The protocol
                        // announces arrivals going forward and nothing else, so
                        // without this a pilot walking into an occupied Cage
                        // sees an empty room until somebody else moves.
                        //
                        // Sent as `PilotJoined` on purpose: from this client's
                        // point of view that is exactly what happened, and it
                        // needs no new message the other two shells would also
                        // have to learn.
                        {
                            let mut occupancy = dogma.occupancy.lock().await;
                            for occupant in occupancy.in_cage(id) {
                                if occupant.pilot == session.pilot {
                                    continue;
                                }
                                frame::write(&mut send, &ServerMessage::PilotJoined {
                                    cage: id,
                                    profile: PilotProfile {
                                        id: occupant.pilot,
                                        nickname: occupant.nickname.clone(),
                                        roles: Vec::new(),
                                    },
                                    ssrc: occupant.ssrc,
                                }).await?;
                            }
                            occupancy.seat(
                                id,
                                crate::dogma::Occupant {
                                    pilot: session.pilot,
                                    nickname: session.nickname.clone(),
                                    ssrc: session.ssrc,
                                },
                            );
                        }

                        let _ = dogma.events.send(Event::PilotJoined {
                            cage: id,
                            profile: PilotProfile {
                                id: session.pilot,
                                nickname: session.nickname.clone(),
                                roles: Vec::new(),
                            },
                            ssrc: session.ssrc,
                        });
                    }
                    ClientMessage::EjectPlug => {
                        cage.send(CageCommand::Leave { pilot: session.pilot }).await?;
                        if let Some(id) = current_cage.take() {
                            dogma.occupancy.lock().await.vacate(id, session.pilot);
                            let _ = dogma.events.send(Event::PilotLeft {
                                cage: id,
                                pilot: session.pilot,
                            });
                        }
                    }
                    ClientMessage::JoinLine { line } => {
                        if !lines.contains(&line) {
                            lines.push(line);
                        }
                    }
                    ClientMessage::SendMessage { line, body, replies_to, client_message_id } => {
                        // specs/08-seguranca.md: verified here, always.
                        if !session.may_write {
                            frame::write(&mut send, &ServerMessage::Alert {
                                severity: AlertSeverity::Warning,
                                reason: AlertReason::PermissionDenied,
                                operator_text: None,
                            }).await?;
                            continue;
                        }
                        // Queued, not confirmed. The broadcast after the commit
                        // is what tells anybody it happened.
                        dogma.post(PendingMessage {
                            line,
                            author: session.pilot,
                            body,
                            replies_to,
                            client_message_id: Some(client_message_id),
                        }).await?;
                    }
                    ClientMessage::FetchHistory { line, cursor, limit } => {
                        let page = {
                            let mut guard = dogma.casper.lock().await;
                            let messages = Messages::new(&mut guard);
                            messages.history(
                                line,
                                cursor,
                                if limit == 0 { DEFAULT_PAGE } else { limit },
                            )?
                        };
                        // Oldest first on the wire, so a client can append.
                        for stored in page.into_iter().rev() {
                            frame::write(&mut send, &ServerMessage::MessageReceived {
                                line: stored.line,
                                id: stored.id,
                                author: stored.author,
                                at: stored.created_at,
                                body: stored.body,
                                replies_to: stored.replies_to,
                                client_message_id: stored.client_message_id,
                            }).await?;
                        }
                    }
                    ClientMessage::Ping { timestamp } => {
                        frame::write(&mut send, &ServerMessage::Pong { timestamp }).await?;
                    }
                    ClientMessage::SetAtField(_)
                    | ClientMessage::SetTotalIsolation(_)
                    | ClientMessage::SetPresence(
                        Presence::Available | Presence::Away | Presence::DoNotDisturb,
                    ) => {}
                    // The handshake is over. Repeating it is a protocol
                    // violation, not a re-authentication.
                    ClientMessage::Response { .. } | ClientMessage::Hello { .. } => break,
                }
            }

            datagram = connection.read_datagram() => {
                let Ok(bytes) = datagram else { break };
                if current_cage.is_none() {
                    continue;
                }
                let _ = cage.send(CageCommand::Datagram {
                    from: session.ssrc,
                    bytes: bytes.to_vec(),
                }).await;
            }

            outbound = outbound_rx.recv() => {
                let Some(bytes) = outbound else { break };
                let _ = connection.send_datagram(bytes.into());
            }

            event = events.recv() => {
                let Ok(event) = event else { continue };
                if let Some(message) = translate(&event, &lines, current_cage, session.pilot) {
                    frame::write(&mut send, &message).await?;
                }
            }

            _ = telemetry.tick() => {
                // The server measures RTT and loss from QUIC itself, which is
                // the only vantage point that sees both directions. Jitter is
                // measured at the receiver, so the server reports zero rather
                // than a number it cannot know.
                let stats = connection.stats();
                let rtt_ms = connection.rtt().as_secs_f32() * 1000.0;
                let sent = stats.path.sent_packets.max(1) as f32;
                let lost = stats.path.lost_packets as f32;
                let inputs = SyncInputs {
                    rtt_ms,
                    jitter_ms: 0.0,
                    loss_fraction: (lost / sent).clamp(0.0, 1.0),
                };
                let ratio = sync.update(inputs);

                frame::write(&mut send, &ServerMessage::Telemetry(Telemetry {
                    rtt_ms,
                    jitter_ms: inputs.jitter_ms,
                    loss_fraction: inputs.loss_fraction,
                    subsystems: vec![
                        (Subsystem::Melchior, SubsystemHealth::Nominal),
                        (Subsystem::Balthasar, SubsystemHealth::Nominal),
                        (Subsystem::Casper, SubsystemHealth::Nominal),
                    ],
                })).await?;

                let _ = dogma.events.send(Event::PilotState(PilotState {
                    pilot: session.pilot,
                    at_field: false,
                    total_isolation: false,
                    speaking: false,
                    presence: Presence::Available,
                    sync_ratio: ratio,
                }));
            }
        }
    }

    // The connection is gone. Hold the seat for the grace window rather than
    // letting a tunnel cost somebody their place — specs/02-protocolo.md.
    if let Some(id) = current_cage {
        let mut slots = dogma.slots.lock().await;
        slots.reserve(session.pilot, id, session.ssrc, Instant::now());
        // The seat is held, but the pilot is not in the room. Leaving them in
        // the occupancy would show everybody a roster with somebody who left,
        // for five minutes.
        dogma.occupancy.lock().await.vacate(id, session.pilot);
    }

    Ok(())
}

/// Decides whether an event concerns this connection, and what to send.
fn translate(
    event: &Event,
    lines: &[LineId],
    cage: Option<CageId>,
    self_pilot: PilotId,
) -> Option<ServerMessage> {
    match event {
        Event::MessagePosted(message) => {
            lines
                .contains(&message.line)
                .then(|| ServerMessage::MessageReceived {
                    line: message.line,
                    id: message.id,
                    author: message.author,
                    at: message.created_at,
                    body: message.body.clone(),
                    replies_to: message.replies_to,
                    client_message_id: message.client_message_id,
                })
        }
        Event::MessageEdited { line, id, body } => {
            lines.contains(line).then(|| ServerMessage::MessageEdited {
                line: *line,
                id: *id,
                body: body.clone(),
            })
        }
        Event::MessageRemoved { line, id } => {
            lines
                .contains(line)
                .then_some(ServerMessage::MessageRemoved {
                    line: *line,
                    id: *id,
                })
        }
        // Not echoed to the pilot who caused it: they already know.
        Event::PilotJoined {
            cage: joined,
            profile,
            ssrc,
        } => (cage == Some(*joined) && profile.id != self_pilot).then(|| {
            ServerMessage::PilotJoined {
                cage: *joined,
                profile: profile.clone(),
                ssrc: *ssrc,
            }
        }),
        Event::PilotLeft { cage: left, pilot } => (cage == Some(*left) && *pilot != self_pilot)
            .then_some(ServerMessage::PilotLeft {
                cage: *left,
                pilot: *pilot,
            }),
        Event::PilotState(state) => {
            (state.pilot != self_pilot).then_some(ServerMessage::PilotState(*state))
        }
    }
}
