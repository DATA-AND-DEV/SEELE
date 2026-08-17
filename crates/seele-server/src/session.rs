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
use seele_proto::control::{
    AlertReason, AlertSeverity, CageInfo, ClientMessage, DisconnectReason, LineInfo, Permission,
    PilotProfile, PilotState, Presence, Role, ServerMessage, Subsystem, SubsystemHealth, Telemetry,
};
use seele_proto::ids::{CageId, LineId, PilotId, RoleId, SessionId, Ssrc};
use seele_proto::sync_ratio::{SyncInputs, SyncRatio};
use seele_proto::transport::HANDSHAKE_TIMEOUT;
use tokio::sync::mpsc;

use crate::cage::CageCommand;
use crate::casper::channels::Channels;
use crate::casper::messages::{Messages, PendingMessage, DEFAULT_PAGE};
use crate::casper::Casper;
use crate::dogma::{Dogma, Event};
use crate::melchior::{self, Melchior};
use crate::taxa::{Veredito, Vigia};
use crate::{frame, DogmaConfig, PUBLIC_KEY_LEN};

/// Bytes of nonce the client signs.
const NONCE_LEN: usize = 32;

/// How many datagrams queue for one listener before the Cage sheds.
const OUTBOUND_DEPTH: usize = 256;

/// Quantos quadros de controle esperam a sessão antes de a leitura parar.
///
/// Limitado de propósito. Controle é raro — entrar num Cage, abrir uma Linha,
/// dizer uma frase — então um cliente honesto nunca chega perto disto; e um
/// desonesto encontra contrapressão em vez de memória do Dogma para gastar.
const ENTRADA_DEPTH: usize = 64;

/// Aborta uma tarefa quando sai de escopo.
///
/// A tarefa leitora é dona do fluxo de recepção. Sem isto ela sobreviveria a um
/// retorno por `?` no meio da sessão, segurando o fluxo de uma conexão que já
/// acabou.
struct AbortaAoSair(tokio::task::JoinHandle<()>);

impl Drop for AbortaAoSair {
    fn drop(&mut self) {
        self.0.abort();
    }
}

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
    cages: Arc<crate::cage::Cages>,
) -> Result<()> {
    // O balde de antes de autenticar, consultado antes de qualquer trabalho.
    //
    // Aqui, e não depois do `Hello`, porque o que se protege é justamente o
    // trabalho que vem depois: ler e decodificar o quadro, e sobretudo o
    // Argon2id da admissão, que o ADR 0021 escolheu caro de propósito. Um
    // pacote que compra dezenas de milissegundos de CPU alheia é amplificação
    // boa demais para deixar de graça num Dogma exposto.
    let admitido = {
        let ip = connection.remote_address().ip();
        dogma.portaria.lock().await.permitir(ip, Instant::now())
    };

    // Com prazo. Uma conexão que nunca abre o fluxo de controle segurava esta
    // tarefa até o tempo ocioso do QUIC recolhê-la; o orçamento do aperto de
    // mão em `specs/02-protocolo.md` é de dez segundos, e vale para a espera
    // inteira e não só para a parte depois do primeiro quadro.
    let (mut send, mut recv) = tokio::time::timeout(HANDSHAKE_TIMEOUT, connection.accept_bi())
        .await
        .with_context(|| format!("client opened no control stream in {HANDSHAKE_TIMEOUT:?}"))?
        .context("client never opened the control stream")?;

    if !admitido {
        // Recusado com motivo, e não em silêncio: quem estoura este balde por
        // engano — um cliente com laço de reconexão defeituoso, uma casa
        // inteira saindo do mesmo NAT — precisa poder ler o que houve.
        // `worth_retrying` no `seele-tui` já trata `RateLimited` como coisa que
        // não se repete na hora, então o aviso também serve para o laço parar.
        tracing::warn!(peer = %connection.remote_address(), "handshake refused: rate limited");
        let _ = frame::write(
            &mut send,
            &ServerMessage::Disconnecting {
                reason: DisconnectReason::RateLimited,
            },
        )
        .await;
        let _ = send.finish();
        despedir(&connection, &mut send, b"rate limited").await;
        bail!("handshake refused: too many attempts from this address");
    }

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
            despedir(&connection, &mut send, b"handshake refused").await;
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
            despedir(&connection, &mut send, b"handshake refused").await;
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

    let result = run_session(connection, send, recv, &session, &dogma, &cages).await;

    // Out of every room, not out of the one this connection remembers. The loop
    // above can end at any `?`, and a path that returns early does not know
    // where the pilot was sitting.
    cages.leave_everywhere(session.pilot).await;

    // And **announced**, which it was not. `Event::PilotLeft` was sent only
    // from the `EjectPlug` branch, so a pilot who closed their client, lost
    // their network or hit any `?` in the loop stayed in everybody else's
    // roster until they reconnected. Nobody saw it while a client only drew the
    // Cage it was sitting in and only learned of that Cage's arrivals; now that
    // every Cage is drawn with the people in it, a ghost is a ghost on screen.
    //
    // Here rather than at the end of `run_session` for the same reason as the
    // line above: this is the one place every exit path passes through.
    for cage in dogma
        .occupancy
        .lock()
        .await
        .vacate_everywhere(session.pilot)
    {
        let _ = dogma.events.send(Event::PilotLeft {
            cage,
            pilot: session.pilot,
        });
    }

    tracing::info!(pilot = %session.pilot, "session ended");
    result
}

/// Espera o motivo da recusa sair do fio antes de a conexão morrer.
///
/// `specs/02-protocolo.md` exige que a razão de uma recusa seja específica,
/// "nunca genérica". Escrever o `Disconnecting` não bastava: `bail!` devolve, a
/// `Connection` é recolhida, e o QUIC derruba tudo — inclusive o quadro que
/// ainda não tinha saído. **O cliente lia erro de conexão e mostrava "não foi
/// possível alcançar o Dogma"**, mandando a pessoa procurar problema de rede
/// enquanto a resposta era "esse apelido é de outro piloto".
///
/// `stopped()` volta quando o outro lado reconheceu o fim do fluxo, que é a
/// prova de que ele leu. O prazo existe porque um cliente que sumiu no meio da
/// recusa não pode segurar a tarefa: um segundo é muito mais do que o
/// loopback precisa e pouco para quem não está mais lá.
async fn despedir(connection: &quinn::Connection, send: &mut quinn::SendStream, motivo: &[u8]) {
    let _ = tokio::time::timeout(Duration::from_secs(1), send.stopped()).await;
    // Fechar com motivo, em vez de deixar cair: dá ao outro lado um encerramento
    // limpo em vez de um tempo esgotado.
    connection.close(0_u32.into(), motivo);
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
        join_secret,
        nickname,
        public_key,
    } = hello
    else {
        return Err(Refusal {
            reason: DisconnectReason::ProtocolViolation,
            detail: "first frame was not Hello".into(),
        });
    };

    seele_proto::version::negotiate(version).map_err(|_| Refusal {
        reason: DisconnectReason::Incompatible,
        detail: format!("client speaks protocol {version}"),
    })?;

    // O porteiro, antes do desafio criptográfico. Quem não tem direito de estar
    // batendo à porta não deve custar uma verificação de assinatura, e um
    // Dogma exposto na internet é varrido o dia inteiro.
    //
    // A recusa é sempre `CredentialRejected`, seja senha errada, convite gasto
    // ou convite vencido. `specs/08-seguranca.md` exige falha uniforme: um erro
    // que distingue os casos conta a quem está adivinhando qual palpite chegou
    // mais perto. O motivo real vai para o log do operador.
    {
        let mut guard = dogma.casper.lock().await;
        let politica = crate::admissao::Politica::carregar(&guard).map_err(|error| Refusal {
            reason: DisconnectReason::CredentialRejected,
            detail: format!("could not read the admission policy: {error}"),
        })?;
        match politica.admitir(&mut guard, join_secret.as_deref()) {
            Ok(Ok(())) => {}
            Ok(Err(recusa)) => {
                return Err(Refusal {
                    reason: DisconnectReason::CredentialRejected,
                    detail: format!("admissão recusada: {recusa:?}"),
                });
            }
            Err(error) => {
                return Err(Refusal {
                    reason: DisconnectReason::CredentialRejected,
                    detail: format!("could not evaluate admission: {error}"),
                });
            }
        }
    }

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
        // Resolved here rather than left for the shell to work out from `roles`:
        // "negadas vencem concedidas" is one rule and belongs in one place.
        let permissions = melchior.permissions(pilot.id).unwrap_or_default();

        Account {
            id: pilot.id,
            nickname: pilot.nickname,
            may_speak: may(Permission::Speak),
            may_write: may(Permission::WriteLine),
            cages,
            lines,
            roles,
            permissions,
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
            permissions: account.permissions,
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
    permissions: Vec<Permission>,
}

/// Reads the Cage and Line tree, and the roles, out of CASPER.
fn read_dogma(casper: &Casper) -> Result<(Vec<CageInfo>, Vec<LineInfo>, Vec<Role>)> {
    let connection = casper.connection();

    // The same reader the creating verbs use, so the tree the handshake sends
    // and the tree a new room lands in cannot drift apart.
    let channels = Channels::new(casper);
    let cages = channels.cages()?;
    let lines = channels.lines()?;

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
    recv: quinn::RecvStream,
    session: &Session,
    dogma: &Dogma,
    cages: &crate::cage::Cages,
) -> Result<()> {
    // Uma tarefa é dona do fluxo de leitura e entrega quadros inteiros por um
    // canal.
    //
    // Ler direto dentro do `select!` abaixo era um defeito, e um caro. `read`
    // faz dois `read_exact` — tamanho e corpo — e o `select!` cancela o que
    // perde a corrida. Cancelado entre os dois, o tamanho já consumido some, e
    // o `read` seguinte lê os primeiros bytes do **corpo** como tamanho: o
    // fluxo fica deslocado para sempre. Depois disso o cliente segue conectado,
    // manda mensagens, e o servidor não entende mais nenhuma.
    //
    // Aqui havia cinco ramos, um deles um `interval` de um segundo — uma
    // oportunidade de cancelar por segundo, em toda sessão, para sempre. Foi o
    // que derrubou `acceptance_m5` no runner do Linux, onde tamanho e corpo
    // caem em pacotes separados com mais frequência.
    // `crates/seele-core/src/frame.rs` tem o teste que prova o mecanismo.
    //
    // O canal é **limitado**: o par não é confiável, e um canal sem limite
    // deixaria um cliente que fala mais rápido do que a sessão processa crescer
    // sem teto na memória do Dogma. Cheio, a tarefa leitora para de ler, e a
    // contrapressão volta pelo QUIC — que é onde ela deve aparecer.
    let (para_dentro, mut entrada) = mpsc::channel::<ClientMessage>(ENTRADA_DEPTH);
    let leitora = tokio::spawn(async move {
        let mut recv = recv;
        loop {
            match frame::read::<ClientMessage>(&mut recv).await {
                Ok(mensagem) => {
                    if para_dentro.send(mensagem).await.is_err() {
                        return;
                    }
                }
                Err(erro) => {
                    tracing::debug!(%erro, "o fluxo de controle do cliente terminou");
                    return;
                }
            }
        }
    });
    // Abortada ao sair por qualquer caminho, inclusive por `?`.
    let _leitora = AbortaAoSair(leitora);

    // Um vigia por conexão. Estado local da sessão, sem mapa nem tranca: nada
    // fora desta conexão precisa saber quantos quadros ela gastou.
    let mut vigia = Vigia::novo(Instant::now());

    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Vec<u8>>(OUTBOUND_DEPTH);
    // Quadros de voz que o transporte recusou nesta sessão. Ver o `select!`.
    let mut recusados: u64 = 0;
    let mut events = dogma.events.subscribe();
    let mut lines: Vec<LineId> = Vec::new();
    let mut current_cage: Option<CageId> = None;

    // What this pilot has announced about themselves. Held here rather than
    // rebuilt each tick, because the telemetry broadcast carries it and a tick
    // that reported a hardcoded `false` would undo every mute a second later.
    let mut at_field = false;
    let mut total_isolation = false;
    let mut presence = Presence::Available;
    // Whether they are transmitting. Not announceable on the control channel,
    // and it should not be: the truthful source is whether audio is actually
    // arriving. A client that says it is speaking while sending nothing would
    // light up somebody else's roster for silence.
    let mut last_datagram: Option<Instant> = None;
    // The last Sync Ratio this connection measured. Carried so that announcing
    // a mute does not report a ratio of zero alongside it — every client folds
    // the whole `PilotState` in, so a field left at a default is not left
    // alone, it is overwritten.
    let mut last_ratio = 0_u8;
    let mut sync = SyncRatio::new();
    let mut telemetry = tokio::time::interval(TELEMETRY_INTERVAL);
    telemetry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // A reclaimed seat means the pilot was already in a Cage when they dropped.
    if let Some(reclaimed) = session.reclaimed_cage {
        cages
            .of(reclaimed)
            .await
            .send(CageCommand::Join {
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

    // Who is already seated, in **every** Cage — the whole picture, once.
    //
    // The wider half of gap G15. The narrow half was closed inside
    // `InsertPlug`: walk into an occupied Cage and the server listed the people
    // in *that* Cage. Every other Cage stayed empty on the client for the whole
    // session, because nothing had ever carried who was in it — and the screen
    // in `design/Entry Plug v3.dc.html` draws occupants under all of them. That
    // is the defect reported from a real session as the Cages showing empty
    // when they were not.
    //
    // Sent as `PilotJoined`, which is what the client already folds in, so this
    // needs no new message and no new arm in any of the three shells. From this
    // connection's point of view every one of these people did just arrive: it
    // is the moment it learned about them.
    //
    // After `events.subscribe()` above, deliberately. Subscribing first and
    // snapshotting second can duplicate an arrival — the client is idempotent
    // about that — while the other order would drop one, and a dropped arrival
    // is a person who is in the room and not on the screen, which is the whole
    // bug again.
    for (cage, occupant) in dogma.occupancy.lock().await.everywhere() {
        if occupant.pilot == session.pilot {
            continue;
        }
        frame::write(
            &mut send,
            &ServerMessage::PilotJoined {
                cage,
                profile: PilotProfile {
                    id: occupant.pilot,
                    nickname: occupant.nickname.clone(),
                    roles: Vec::new(),
                },
                ssrc: occupant.ssrc,
            },
        )
        .await?;
    }

    loop {
        tokio::select! {
            // `recv` num canal é cancel-safe: nada é consumido pelo ramo que
            // perde a corrida. É a propriedade que o `frame::read` não tem.
            incoming = entrada.recv() => {
                let Some(message) = incoming else { break };

                // O balde de depois de autenticar. O ADR 0021 fechou a porta e
                // deixou escrito o que não resolvia: "um convidado legítimo
                // pode inundar de mensagens". É aqui.
                //
                // Julgado antes de o quadro ser executado, e não depois: o
                // ponto é não gastar o Dogma com ele.
                match vigia.avaliar(Instant::now()) {
                    Veredito::Passa => {}
                    Veredito::Avisa => {
                        tracing::warn!(pilot = %session.pilot, "control frames over budget");
                        frame::write(&mut send, &ServerMessage::Alert {
                            severity: AlertSeverity::Warning,
                            reason: AlertReason::RateLimited,
                            operator_text: None,
                        }).await?;
                        continue;
                    }
                    Veredito::Descarta => continue,
                    Veredito::Derruba => {
                        tracing::warn!(
                            pilot = %session.pilot,
                            descartados = vigia.descartados(),
                            "disconnecting: rate limited"
                        );
                        let _ = frame::write(&mut send, &ServerMessage::Disconnecting {
                            reason: DisconnectReason::RateLimited,
                        }).await;
                        let _ = send.finish();
                        despedir(&connection, &mut send, b"rate limited").await;
                        break;
                    }
                }

                match message {
                    ClientMessage::InsertPlug { cage: id, password } => {
                        // A senha do Cage era declarada no protocolo, relatada
                        // ao cliente em `password_required` e **nunca
                        // conferida**. Uma fechadura que se anuncia trancada e
                        // não está é pior que porta aberta: quem confia nela
                        // toma decisão errada sobre o que dizer ali dentro.
                        if !crate::admissao::cage_liberado(
                            &*dogma.casper.lock().await,
                            id,
                            password.as_deref(),
                        ) {
                            frame::write(&mut send, &ServerMessage::Alert {
                                severity: AlertSeverity::Warning,
                                reason: AlertReason::CageEntryRefused,
                                operator_text: None,
                            }).await?;
                            continue;
                        }
                        // Out of the old room before into the new one. Without
                        // this a pilot who walks from one Cage to another is
                        // still a member of the first, and goes on hearing it.
                        cages.leave_everywhere(session.pilot).await;
                        cages.of(id).await.send(CageCommand::Join {
                            pilot: session.pilot,
                            ssrc: session.ssrc,
                            may_speak: session.may_speak,
                            outbound: outbound_tx.clone(),
                        }).await?;
                        current_cage = Some(id);

                        // No burst of "who is already here" any more: this
                        // connection was handed every Cage's occupants when it
                        // started, and has been told about every arrival and
                        // departure since, wherever it happened. Repeating the
                        // room it is walking into would be telling it something
                        // it already knows.
                        //
                        // The pilot's own departure from wherever they were is
                        // announced by `seat` clearing the old row and the
                        // `PilotLeft` below it.
                        let saiu_de = {
                            let mut occupancy = dogma.occupancy.lock().await;
                            let saiu_de = occupancy.vacate_everywhere(session.pilot);
                            occupancy.seat(
                                id,
                                crate::dogma::Occupant {
                                    pilot: session.pilot,
                                    nickname: session.nickname.clone(),
                                    ssrc: session.ssrc,
                                },
                            );
                            saiu_de
                        };
                        // Walking from one Cage to another is a departure and
                        // an arrival, and both have to be said. Without the
                        // first, everybody watching the old room keeps the
                        // pilot in it for ever — invisible while a client only
                        // drew its own Cage, and a ghost now that it draws all
                        // of them.
                        for anterior in saiu_de {
                            let _ = dogma.events.send(Event::PilotLeft {
                                cage: anterior,
                                pilot: session.pilot,
                            });
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
                        cages.leave_everywhere(session.pilot).await;
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
                            author_nickname: session.nickname.clone(),
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
                                at_seconds: stored.created_at,
                                author_nickname: stored.author_nickname,
                                body: stored.body,
                                replies_to: stored.replies_to,
                                client_message_id: stored.client_message_id,
                            }).await?;
                        }
                    }
                    ClientMessage::Ping { timestamp } => {
                        frame::write(&mut send, &ServerMessage::Pong { timestamp }).await?;
                    }
                    // The roster shows all three (specs/07-tema-evangelion.md).
                    // Ignoring them, as this did, made every mute local-only:
                    // the marker existed and could never light up.
                    ClientMessage::SetAtField(on) => {
                        at_field = on;
                        announce(dogma, session, &AnnouncedState {
                            at_field,
                            total_isolation,
                            speaking: speaking_now(last_datagram),
                            presence,
                            sync_ratio: last_ratio,
                        });
                    }
                    ClientMessage::SetTotalIsolation(on) => {
                        total_isolation = on;
                        announce(dogma, session, &AnnouncedState {
                            at_field,
                            total_isolation,
                            speaking: speaking_now(last_datagram),
                            presence,
                            sync_ratio: last_ratio,
                        });
                    }
                    ClientMessage::SetPresence(
                        announced @ (Presence::Available | Presence::Away
                                     | Presence::DoNotDisturb),
                    ) => {
                        presence = announced;
                        announce(dogma, session, &AnnouncedState {
                            at_field,
                            total_isolation,
                            speaking: speaking_now(last_datagram),
                            presence,
                            sync_ratio: last_ratio,
                        });
                    }
                    // ---- rooms, made by whoever hosts ----
                    //
                    // The permission is read **now**, from MELCHIOR, and not
                    // from anything the handshake cached. `may_speak` and
                    // `may_write` are cached because they are consulted per
                    // audio frame and per message; these four are consulted
                    // once in a while, and a Comandante who revoked somebody's
                    // ManageCages a minute ago should not have to wait for that
                    // person to reconnect before it means anything.
                    //
                    // Denial answers with `PermissionDenied` rather than
                    // silence. specs/08-seguranca.md makes the server the
                    // security and the hidden button the convenience — but a
                    // refusal nobody is told about is indistinguishable from a
                    // Dogma that is broken.
                    ClientMessage::CreateCage { name, limit, line } => {
                        if !pode(dogma, session.pilot, Permission::ManageCages).await {
                            recusar(&mut send, session.pilot, "CreateCage").await?;
                            continue;
                        }
                        let feito = {
                            let guard = dogma.casper.lock().await;
                            Channels::new(&guard).create_cage(&name, limit, line)
                        };
                        match feito {
                            Ok(cage) => {
                                tracing::info!(pilot = %session.pilot, cage = %cage.id, "cage created");
                                let _ = dogma.events.send(Event::CageCreated { cage });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::CreateLine { name } => {
                        if !pode(dogma, session.pilot, Permission::ManageCages).await {
                            recusar(&mut send, session.pilot, "CreateLine").await?;
                            continue;
                        }
                        let feito = {
                            let guard = dogma.casper.lock().await;
                            Channels::new(&guard).create_line(&name)
                        };
                        match feito {
                            Ok(line) => {
                                tracing::info!(pilot = %session.pilot, line = %line.id, "line created");
                                let _ = dogma.events.send(Event::LineCreated { line });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::RenameCage { cage: id, name } => {
                        if !pode(dogma, session.pilot, Permission::ManageCages).await {
                            recusar(&mut send, session.pilot, "RenameCage").await?;
                            continue;
                        }
                        let feito = {
                            let guard = dogma.casper.lock().await;
                            Channels::new(&guard).rename_cage(id, &name)
                        };
                        match feito {
                            Ok(name) => {
                                let _ = dogma.events.send(Event::CageRenamed { cage: id, name });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::RenameLine { line: id, name } => {
                        if !pode(dogma, session.pilot, Permission::ManageCages).await {
                            recusar(&mut send, session.pilot, "RenameLine").await?;
                            continue;
                        }
                        let feito = {
                            let guard = dogma.casper.lock().await;
                            Channels::new(&guard).rename_line(id, &name)
                        };
                        match feito {
                            Ok(name) => {
                                let _ = dogma.events.send(Event::LineRenamed { line: id, name });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }

                    // The handshake is over. Repeating it is a protocol
                    // violation, not a re-authentication.
                    ClientMessage::Response { .. } | ClientMessage::Hello { .. } => break,
                }
            }

            datagram = connection.read_datagram() => {
                let Ok(bytes) = datagram else { break };
                // Into the room this connection is actually in. Sending to a
                // fixed Cage was correct while a Dogma had one and became a
                // crossed wire the moment it could have two.
                let Some(id) = current_cage else { continue };
                last_datagram = Some(Instant::now());
                let _ = cages.of(id).await.send(CageCommand::Datagram {
                    from: session.ssrc,
                    bytes: bytes.to_vec(),
                }).await;
            }

            outbound = outbound_rx.recv() => {
                let Some(bytes) = outbound else { break };
                // Contado, e não descartado. Um datagrama do QUIC não é
                // fragmentado: o texto viaja num fluxo e se adapta ao caminho
                // sozinho, a voz viaja aqui e um datagrama que não cabe é
                // recusado inteiro. É assim que um enlace entrega toda a
                // conversa escrita e ainda pica a voz — e num sentido só,
                // porque o caminho de ida não é o de volta.
                //
                // Contado por sessão e dito uma vez ao fim, e não por quadro:
                // isto passa cinquenta vezes por segundo, e um log por quadro
                // afogaria o arquivo no instante em que alguém precisa lê-lo.
                if connection.send_datagram(bytes.into()).is_err() {
                    recusados = recusados.saturating_add(1);
                }
            }

            event = events.recv() => {
                let Ok(event) = event else { continue };
                if let Some(message) = translate(&event, &lines, session.pilot) {
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
                last_ratio = ratio;

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
                    at_field,
                    total_isolation,
                    speaking: speaking_now(last_datagram),
                    presence,
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
    }
    // Coming out of the occupancy — and being announced — is `serve`'s job, and
    // it happens the moment this returns. It used to happen here, which meant
    // it did not happen at all on any path that left the loop through a `?`:
    // the seat was cleared for a clean shutdown and kept for ever for a broken
    // pipe, which is the case that actually occurs. Doing it there costs the
    // few microseconds between this line and that one, and covers every exit.

    // Dito só quando aconteceu, e uma vez.
    //
    // Zero é o normal e não merece linha nenhuma; qualquer outro número
    // significa que a voz **saiu deste Dogma pela metade** para aquele cliente,
    // e que quem ouviu culpou a rede. Sem esta linha não havia como saber: a
    // recusa era descartada, e recusa de envio soa exatamente igual a perda de
    // rede — com o conserto no lado oposto.
    if recusados > 0 {
        tracing::warn!(
            pilot = %session.pilot,
            recusados,
            "o transporte recusou quadros de voz para este cliente; \
             o caminho até ele não comporta o tamanho do datagrama"
        );
    }

    Ok(())
}

/// Asks MELCHIOR, right now, whether this pilot may do something.
///
/// Every call takes the CASPER lock, which is the point: the answer is the one
/// that is true at the instant the verb is used, not the one that was true when
/// the connection opened. `specs/08-seguranca.md`: "Toda ação é verificada no
/// servidor, sempre." Control verbs are rare enough that the lock costs nothing
/// worth measuring — the frame budget in [`crate::taxa`] already caps how often
/// one connection can ask.
///
/// A database error reads as denial. The alternative is to let a Dogma whose
/// disk is failing hand out `ManageCages` to whoever asks while it fails.
async fn pode(dogma: &Dogma, pilot: PilotId, permission: Permission) -> bool {
    let guard = dogma.casper.lock().await;
    Melchior::new(&guard)
        .may(pilot, permission)
        .unwrap_or(false)
}

/// Tells a client the server said no, and why.
async fn recusar(send: &mut quinn::SendStream, pilot: PilotId, verbo: &str) -> Result<()> {
    tracing::warn!(%pilot, verbo, "refused: the pilot does not have ManageCages");
    frame::write(
        send,
        &ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::PermissionDenied,
            operator_text: None,
        },
    )
    .await
}

/// Tells a client the room could not be made, without saying what the database
/// thinks about it.
///
/// `CageEntryRefused` is the nearest enumerated reason for "that room is not
/// there" — `specs/02-protocolo.md` allows no free-form string on the wire, and
/// inventing a variant for every way a write can fail would be inventing an
/// error language. The detail goes to the operator's log, which is where a
/// developer will look for it.
async fn nao_deu(send: &mut quinn::SendStream, erro: &anyhow::Error) -> Result<()> {
    tracing::warn!(%erro, "a room could not be made");
    frame::write(
        send,
        &ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::CageEntryRefused,
            operator_text: None,
        },
    )
    .await
}

/// How long after the last datagram a pilot still counts as speaking.
///
/// One telemetry tick is too coarse and one frame is too twitchy: at 20 ms per
/// frame, a quarter of a second is about a dozen frames of grace, which rides
/// out a hiccup without leaving the mark lit through a pause.
const SPEAKING_TAIL: Duration = Duration::from_millis(250);

/// Whether audio has arrived recently enough to call this pilot speaking.
fn speaking_now(last_datagram: Option<Instant>) -> bool {
    last_datagram.is_some_and(|at| at.elapsed() < SPEAKING_TAIL)
}

/// Everything a `PilotState` broadcast carries about one pilot.
///
/// Grouped rather than passed as six arguments because every one of them is
/// overwritten wholesale on the receiving side: a client folds the entire
/// struct in, so a field left at a default is not left alone — it replaces
/// whatever that client knew.
struct AnnouncedState {
    at_field: bool,
    total_isolation: bool,
    speaking: bool,
    presence: Presence,
    sync_ratio: u8,
}

/// Tells everybody what this pilot just announced about themselves.
fn announce(dogma: &Dogma, session: &Session, state: &AnnouncedState) {
    let _ = dogma.events.send(Event::PilotState(PilotState {
        pilot: session.pilot,
        at_field: state.at_field,
        total_isolation: state.total_isolation,
        speaking: state.speaking,
        presence: state.presence,
        sync_ratio: state.sync_ratio,
    }));
}

/// Decides whether an event concerns this connection, and what to send.
fn translate(event: &Event, lines: &[LineId], self_pilot: PilotId) -> Option<ServerMessage> {
    match event {
        Event::MessagePosted(message) => {
            lines
                .contains(&message.line)
                .then(|| ServerMessage::MessageReceived {
                    line: message.line,
                    id: message.id,
                    author: message.author,
                    at_seconds: message.created_at,
                    author_nickname: message.author_nickname.clone(),
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
        // Every Cage, and not only the one this connection is sitting in.
        //
        // The filter that used to be here — `cage == Some(*joined)` — is what
        // made four rooms out of five permanently empty on screen: a client was
        // told about arrivals in its own Cage and about nothing else, so the
        // occupants the v3 layout draws under every other Cage were data it had
        // never been sent. Reported from a real session as the Cages showing
        // empty when they were not.
        //
        // Weighed against a count on `CageInfo` and against a snapshot the
        // client asks for. The count is cheaper and loses the names the screen
        // is built around; the snapshot keeps the names and goes stale the
        // instant it lands, which is the same bug moving more slowly. This is
        // the only one of the three that is still true a second later.
        //
        // What it costs is that everybody learns where everybody is. That was
        // already the case: `Event::PilotState` — speaking, Sync Ratio, both
        // mutes — has always gone to every connection unfiltered, so a client
        // was already being told about pilots it had no seat for, and drew them
        // as ghosts with no room. `specs/04-servidor-seele.md` sizes a Dogma at
        // fifty pilots and five Cages, and the Cage list itself is not filtered
        // per pilot, so this reveals nothing that walking into the room would
        // not. ADR 0022 opens a Dogma to the internet; what changes there is who
        // may hold an account, which is `crate::admissao`'s question, not this
        // one.
        //
        // Not echoed to the pilot who caused it: they already know.
        Event::PilotJoined {
            cage: joined,
            profile,
            ssrc,
        } => (profile.id != self_pilot).then(|| ServerMessage::PilotJoined {
            cage: *joined,
            profile: profile.clone(),
            ssrc: *ssrc,
        }),
        Event::PilotLeft { cage: left, pilot } => {
            (*pilot != self_pilot).then_some(ServerMessage::PilotLeft {
                cage: *left,
                pilot: *pilot,
            })
        }
        // Echoed back to the pilot it describes, unlike the two above.
        //
        // "They already know" is true of joining and leaving — the client asked
        // for both — and false of everything in here. The Sync Ratio is measured
        // by the *server*, from QUIC's own view of the path, and this broadcast
        // is the only thing that carries it; `speaking` is decided by whether
        // audio is actually arriving, which is likewise the server's to know.
        // Filtering self out left every client's own roster row frozen at
        // `Pilot::new`'s defaults for the life of the session: nought per cent,
        // which by the three bands reads as critical, beside a telemetry bar
        // measuring the same connection at a hundred.
        //
        // The two flags it also carries are the pilot's own, so echoing them
        // costs nothing: the server is repeating what this client just said, and
        // a client that folds them back in lands on the value it sent.
        Event::PilotState(state) => Some(ServerMessage::PilotState(*state)),

        // Unfiltered, and to the pilot who caused it as well.
        //
        // Unlike a Cage arrival, a new room is not something a client can infer
        // from having asked: the identifier is the server's to assign, and the
        // maker needs it as much as everybody else does. Filtering self out here
        // would leave whoever made the room as the one person who cannot see it.
        Event::CageCreated { cage } => Some(ServerMessage::CageCreated { cage: cage.clone() }),
        Event::LineCreated { line } => Some(ServerMessage::LineCreated { line: line.clone() }),
        Event::CageRenamed { cage, name } => Some(ServerMessage::CageRenamed {
            cage: *cage,
            name: name.clone(),
        }),
        Event::LineRenamed { line, name } => Some(ServerMessage::LineRenamed {
            line: *line,
            name: name.clone(),
        }),
    }
}
