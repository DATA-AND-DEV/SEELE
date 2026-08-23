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
    AlertReason, AlertSeverity, AttachmentRefusal, CageInfo, ClientMessage, DisconnectReason,
    LineInfo, Permission, PilotProfile, PilotState, Presence, Role, ServerMessage, Subsystem,
    SubsystemHealth, Telemetry, Validate,
};
use seele_proto::ids::{CageId, LineId, PilotId, RoleId, ScreenId, SessionId, Ssrc};
use seele_proto::screen::SCREEN_HEADER_LEN;
use seele_proto::sync_ratio::{SyncInputs, SyncRatio};
use seele_proto::transport::HANDSHAKE_TIMEOUT;
use tokio::sync::mpsc;

use crate::cage::CageCommand;
use crate::casper::channels::{Channels, LastCage};
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

/// Quantas razões de transferência esperam para ir ao fluxo de controle.
///
/// Uma por transferência viva, e transferência viva é coisa que se conta nos
/// dedos por conexão: o balde de bytes do ADR 0027 não deixa uma pessoa ter
/// dezesseis subidas grandes acontecendo. Dezesseis é folga, não medida.
const AVISOS_DEPTH: usize = 16;

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
    /// O contador das transmissões de tela.
    ///
    /// **Separado do `ssrc` de propósito**, e o §3.6 da spec de
    /// compartilhamento de tela põe isso em negrito: o `ssrc` é o identificador
    /// de fonte de **áudio**, atribuído na entrada do Cage, e todo cliente
    /// mantém uma tabela de `ssrc` → pessoa construída a partir dele. Uma tela
    /// não é um falante; dar-lhe um `ssrc` obrigaria essa tabela a ganhar uma
    /// segunda espécie de linha.
    next_screen: AtomicU32,
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
            next_screen: AtomicU32::new(1),
        }
    }

    /// Batiza a próxima transmissão de tela.
    ///
    /// Atribuído aqui e **nunca tomado de quem manda**, que é a regra que
    /// `specs/08-seguranca.md` já aplica ao `ssrc`: um identificador que o
    /// cliente escolhe é um identificador com que o cliente escolhe o de outra
    /// pessoa.
    fn issue_screen(&self) -> ScreenId {
        ScreenId(self.next_screen.fetch_add(1, Ordering::Relaxed))
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

    let result = run_session(connection, send, recv, &session, &dogma, &cages, &registry).await;

    // Out of every room, not out of the one this connection remembers. The loop
    // above can end at any `?`, and a path that returns early does not know
    // where the pilot was sitting.
    cages.leave_everywhere(session.pilot).await;
    encerrar_telas_de(&dogma, session.pilot).await;

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
    // Conferido aqui e **gasto depois**, quando a portaria também tiver dito
    // que sim. Ver `admissao::Passe`: quem bate numa porta com portaria é
    // mandado tentar de novo, e queimar o convite dele nesta linha o deixaria
    // de fora para sempre — inclusive depois de aprovado.
    let (passe, recusa_adiada, chegada) = {
        let guard = dogma.casper.lock().await;
        let politica = crate::admissao::Politica::carregar(&guard).map_err(|error| Refusal {
            reason: DisconnectReason::CredentialRejected,
            detail: format!("could not read the admission policy: {error}"),
        })?;
        // A recusa aqui é **adiada**, e não devolvida na hora.
        //
        // O motivo é um defeito relatado em campo: «aprovei a entrada de
        // alguém e deu como credencial recusada». A política não tem memória —
        // com o Dogma fechado ela exige segredo de todo mundo, sempre — e o
        // convite que trouxe a pessoa é de uso único, gasto quando ela entrou.
        // Na volta, ela é barrada por `ConviteGasto` **nesta linha**, antes de
        // a portaria poder dizer que já a admitiu.
        //
        // Quem tem decisão de `admitido` gravada não precisa de segredo nenhum:
        // a portaria **é** a credencial durável de uma pessoa, e o segredo é a
        // porta de quem ainda é estranho.
        //
        // Mas a impressão digital ainda não está provada aqui — a assinatura só
        // é conferida adiante —, e é por isso que isto vira uma recusa guardada
        // em vez de um perdão. Ela é descartada mais abaixo, e só depois da
        // assinatura, se `portaria::ja_admitido` disser que sim.
        //
        // O custo do adiamento é uma verificação de Ed25519 para quem chega com
        // segredo errado, e o balde por endereço do ADR 0025 já limita quantas
        // vezes por minuto. É mais barato que o Argon2 que o caminho da senha
        // pagaria, e o que se compra com ele é quem foi aprovado poder voltar.
        let (passe, recusa_adiada) = match politica.avaliar(&guard, join_secret.as_deref()) {
            Ok(Ok(passe)) => (Some(passe), None),
            Ok(Err(recusa)) => (None, Some(format!("admissão recusada: {recusa:?}"))),
            Err(error) => {
                return Err(Refusal {
                    reason: DisconnectReason::CredentialRejected,
                    detail: format!("could not evaluate admission: {error}"),
                });
            }
        };
        // Só o nome do que acabou de acontecer, para o cartão que a portaria
        // mostra lá embaixo. Nada aqui decide: a decisão foi a linha acima.
        let chegada =
            crate::portaria::como_chegou(&guard, politica.aberto(), join_secret.as_deref());
        (passe, recusa_adiada, chegada)
    };

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

    // A portaria — ADR 0030. A terceira camada, e a única que decide sobre
    // gente em vez de sobre um segredo.
    //
    // **Aqui**, e não junto com `admissao` lá em cima, porque só agora a chave
    // foi provada. Fixar uma impressão digital que ninguém demonstrou não é
    // TOFU, é fixar um palpite: qualquer um encheria a fila com chaves alheias,
    // e quem hospeda aprovaria uma pessoa e admitiria outra. O que impede a fila
    // de encher com chaves próprias, que são de graça, é o balde por endereço do
    // ADR 0025, que responde antes de o `Hello` ser lido.
    //
    // **Antes de `register_or_find`**, e isto vale um parágrafo: quem não passa
    // por aqui não vira conta. Se fosse depois, alguém que nunca foi aprovado
    // ocuparia um apelido para sempre — o ADR 0017 prende o nome à chave, e uma
    // batida recusada teria reservado o nome de alguém que jamais entrou.
    //
    // Nada espera. Um pedido pendente derruba a conexão neste instante e fica
    // gravado; ver o cabeçalho de `portaria`.
    {
        let mut guard = dogma.casper.lock().await;
        let impressao = seele_proto::transport::key_fingerprint(&public_key);
        let (segredo, observacao) = chegada;

        // Aqui a impressão digital **está provada**: a assinatura sobre o nonce
        // foi conferida acima. É o primeiro ponto do aperto de mão onde dá para
        // perguntar «esta pessoa já foi admitida?» sem que a resposta valha
        // para quem só afirmou ser ela.
        //
        // `ja_admitido` e não `bater`: aquela responde `Entra` a todo mundo com
        // a portaria desligada, e perdoar um segredo errado por causa disso
        // abriria a porta de todo Dogma que não usa portaria.
        if let Some(recusa) = recusa_adiada {
            let conhecida = crate::portaria::ja_admitido(&guard, &impressao).unwrap_or(false);
            if !conhecida {
                return Err(Refusal {
                    reason: DisconnectReason::CredentialRejected,
                    detail: recusa,
                });
            }
            tracing::info!(
                %impressao,
                "segredo recusado, mas a portaria já admitiu esta chave; entra"
            );
        }

        match crate::portaria::bater(&mut guard, &impressao, &nickname, segredo, &observacao) {
            Ok(crate::portaria::Resposta::Entra) => {
                // Entrou de verdade: agora o convite é gasto — quando houve um.
                // Sem passe não há o que gastar: é quem entrou pela porta que a
                // portaria já tinha aberto, e ela não consome nada.
                //
                // Perder a corrida aqui é o mesmo caso de sempre — dois
                // clientes com o mesmo convite no mesmo instante — e a recusa
                // dele continua uniforme.
                match passe.as_ref().map_or(Ok(Ok(())), |passe| {
                    crate::admissao::gastar(&mut guard, passe)
                }) {
                    Ok(Ok(())) => {}
                    Ok(Err(recusa)) => {
                        return Err(Refusal {
                            reason: DisconnectReason::CredentialRejected,
                            detail: format!("admissão recusada ao gastar: {recusa:?}"),
                        });
                    }
                    Err(error) => {
                        return Err(Refusal {
                            reason: DisconnectReason::CredentialRejected,
                            detail: format!("could not spend the invite: {error}"),
                        });
                    }
                }
            }
            Ok(crate::portaria::Resposta::Pendente) => {
                tracing::info!(%impressao, %nickname, "knock waiting for the host");
                return Err(Refusal {
                    reason: DisconnectReason::AdmissionPending,
                    detail: format!("portaria: {impressao} aguarda decisão"),
                });
            }
            Ok(crate::portaria::Resposta::Recusado) => {
                return Err(Refusal {
                    reason: DisconnectReason::AdmissionDenied,
                    detail: format!("portaria: {impressao} foi recusada"),
                });
            }
            // Um banco que não responde não é prova de que a porta pode abrir,
            // e a falha aqui cai para o lado fechado como em `cage_liberado`.
            // Pendente e não recusado: a máquina falhou, não a pessoa.
            Err(error) => {
                tracing::error!(%error, "could not consult the doorkeeper");
                return Err(Refusal {
                    reason: DisconnectReason::AdmissionPending,
                    detail: format!("could not consult the doorkeeper: {error}"),
                });
            }
        }
    }

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

    // Lido do CASPER, e não da [`DogmaConfig`] que subiu o processo: renomear
    // com o Dogma no ar é o caso normal — ADR 0032 —, e um nome que voltasse ao
    // do arranque no próximo reinício não seria um nome, seria uma sessão.
    // Ausência continua querendo dizer o padrão da configuração; ver
    // `casper::aparencia`.
    //
    // Uma segunda tomada do mutex, e não um campo a mais na `Account`: são duas
    // perguntas sobre coisas diferentes — o que este piloto é, e o que este
    // Dogma é — e o aperto de mão já toma este mutex mais de uma vez.
    let (nome_do_dogma, icone_do_dogma) = {
        let guard = dogma.casper.lock().await;
        let nome = crate::casper::aparencia::nome(&guard, &config.name).unwrap_or_else(|erro| {
            // Um banco que não responde não pode deixar o Dogma sem nome na
            // tela de quem entra. O padrão da configuração é a resposta honesta.
            tracing::warn!(%erro, "não deu para ler o nome do Dogma");
            config.name.clone()
        });
        let icone = crate::casper::aparencia::icone(&guard).unwrap_or_default();
        (nome, icone)
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
            dogma: nome_do_dogma,
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

    // Logo depois do `Session`, e num quadro próprio: o `Session` já carrega os
    // Cages, as Linhas, os papéis e as permissões dentro dos 16 KiB do
    // `MAX_FRAME_LEN`, e uma imagem disputando esse orçamento faria um Dogma
    // grande deixar de admitir alguém por causa de uma decoração — a terceira
    // razão do ADR 0032, aqui respeitada em vez de contornada.
    //
    // **Só quando há ícone**, e o silêncio quer dizer «não há». O `Session`
    // descreve o Dogma do zero — é dele que sai o nome, a lista de Cages e a de
    // Linhas —, então quem reconecta a um Dogma cuja imagem foi tirada enquanto
    // ele estava fora para de desenhar a antiga por ter sido reapresentado ao
    // Dogma, e não por receber um `None`. O que isso compra é que um Dogma sem
    // ícone, que é todo Dogma que existe hoje, troca exatamente os quadros que
    // trocava antes desta mudança.
    //
    // Um ícone que não passa pela conferência do protocolo é **descartado** em
    // vez de derrubar o aperto de mão. Os bytes vêm do banco, e quem tem o
    // arquivo tem um `sqlite3`: uma linha escrita à mão não pode ser o motivo
    // de ninguém mais conseguir entrar. Uma decoração nunca custa a conexão.
    if icone_do_dogma.is_some() {
        let anuncio = ServerMessage::DogmaIconChanged {
            icon: icone_do_dogma,
        };
        match anuncio.validate() {
            Ok(()) => frame::write(send, &anuncio)
                .await
                .map_err(|error| Refusal {
                    reason: DisconnectReason::ProtocolViolation,
                    detail: format!("could not send the Dogma icon: {error}"),
                })?,
            Err(erro) => {
                tracing::warn!(%erro, "o ícone guardado não é um ícone; seguindo sem ele");
            }
        }
    }

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
    dogma: &Arc<Dogma>,
    cages: &Arc<crate::cage::Cages>,
    registry: &Registry,
) -> Result<()> {
    // Uma tarefa é dona do fluxo de leitura e entrega quadros inteiros por um
    // canal.
    //
    // Ler direto dentro do `select!` abaixo era um defeito, e um caro. `read`
    // faz três `read_exact` — o primeiro byte, o resto do tamanho e o corpo — e
    // o `select!` cancela o que perde a corrida. Cancelado entre eles, o que já
    // foi consumido some, e o `read` seguinte lê os bytes do **corpo** como
    // tamanho: o fluxo fica deslocado para sempre. Depois disso o cliente segue conectado,
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

    // O controle fica acima de toda transferência, escrito e não implícito.
    // Ordena só os nossos fluxos dentro **desta** conexão: duas pessoas subindo
    // de conexões diferentes não se ordenam entre si, e o ADR 0027 põe isso na
    // lista do que fica sem saída.
    let _ = send.set_priority(crate::transfer::CONTROL_PRIORITY);

    // Por onde uma tarefa de transferência devolve a razão de uma recusa ao
    // fluxo de controle. `specs/02-protocolo.md` quer toda razão enumerada, e é
    // no controle que as razões moram; uma transferência não pode escrever
    // neste fluxo por conta própria porque ele é de quem está no `select!`.
    //
    // `recv` num canal é cancel-safe, que é a propriedade que todo ramo deste
    // `select!` precisa ter — ver o comentário da tarefa leitora acima.
    let (avisos_tx, mut avisos_rx) = mpsc::channel::<ServerMessage>(AVISOS_DEPTH);

    // Por onde esta conexão é convidada a assistir uma transmissão de tela, e a
    // tarefa que escreve essas transmissões no fluxo dela. Fora do `select!`
    // pelo mesmo motivo da tarefa acima e por um a mais: o `write_all` de um
    // espectador lento é onde a contrapressão dele tem de aparecer, e ela não
    // pode aparecer no laço que lê o controle de todo mundo.
    let (tela_tx, tela_rx) =
        mpsc::channel::<crate::tela::AberturaDeTela>(crate::tela::ABERTURAS_DEPTH);
    let _telas = AbortaAoSair(tokio::spawn(crate::tela::bombear(
        connection.clone(),
        tela_rx,
    )));

    // Uma tarefa que aceita os fluxos unidirecionais que chegam. São dois
    // tipos: alguém mandando um arquivo e alguém compartilhando a tela. Ela
    // vive fora do `select!` de propósito — receber vinte megabytes ali dentro
    // seria exatamente o bloqueio de cabeça de fila que o fluxo próprio existe
    // para evitar.
    //
    // Aceita **sempre**, inclusive num Dogma que não guarda arquivo nenhum.
    // Não aceitar deixaria o fluxo pendurado até o tempo ocioso do QUIC
    // recolher a conexão, com a barra do outro lado parada em zero e nada
    // sendo dito: exatamente a forma de falhar que este projeto recusa em toda
    // outra porta. Sem diretório, a resposta é `Unavailable` — uma frase.
    //
    // Guardada fora do `let` seguinte pelo motivo de sempre: o `AbortaAoSair`
    // cairia no fim de um bloco e mataria a tarefa no instante seguinte ao de
    // a criar.
    let recebedora = {
        let anexos = dogma.anexos.clone();
        let entrada = Arc::clone(dogma);
        let avisos = avisos_tx.clone();
        let conexao = connection.clone();
        let piloto = session.pilot;
        let apelido = session.nickname.clone();
        let salas = Arc::clone(cages);
        tokio::spawn(async move {
            while let Ok(mut fluxo) = conexao.accept_uni().await {
                let anexos = anexos.clone();
                let contexto = Arc::clone(&entrada);
                let avisos = avisos.clone();
                let apelido = apelido.clone();
                let salas = Arc::clone(&salas);
                // Qual dos dois usos de fluxo unidirecional é este. O byte que
                // decide é lido aqui e repassado adiante: `crate::frame::read_apos`
                // explica por que ele decide, e por que ser aritmética em vez de
                // marca é dívida.
                let mut primeiro = [0_u8; 1];
                if fluxo.read_exact(&mut primeiro).await.is_err() {
                    continue;
                }
                let primeiro = primeiro.first().copied().unwrap_or_default();
                if primeiro != 0 {
                    // Uma tarefa por transmissão, como abaixo: quem
                    // compartilha não pode fazer fila com quem manda arquivo.
                    tokio::spawn(async move {
                        if let Err(erro) =
                            receber_tela(&contexto, &salas, piloto, primeiro, &mut fluxo).await
                        {
                            tracing::debug!(%piloto, %erro, "o fluxo de tela terminou");
                        }
                    });
                    continue;
                }
                // Uma tarefa por transferência: duas pessoas mandando ao mesmo
                // tempo não se enfileiram uma atrás da outra.
                tokio::spawn(async move {
                    let Some(anexos) = anexos else {
                        // O cabeçalho é lido só para saber a quem responder: a
                        // chave de idempotência é o único nome que as duas
                        // pontas compartilham antes de o Dogma ter atribuído
                        // coisa alguma. Os bytes não são lidos.
                        match crate::transfer::quem_perguntou(&mut fluxo, primeiro).await {
                            Ok(client_message_id) => {
                                let _ = avisos
                                    .send(ServerMessage::AttachmentRefused {
                                        client_message_id,
                                        reason: AttachmentRefusal::Unavailable,
                                    })
                                    .await;
                            }
                            Err(erro) => {
                                tracing::debug!(%erro, "uma transferência chegou sem cabeçalho");
                            }
                        }
                        return;
                    };
                    match crate::transfer::receive(
                        &anexos, &contexto, piloto, &apelido, &mut fluxo, primeiro,
                    )
                    .await
                    {
                        Ok(crate::transfer::Outcome::Published(_)) => {}
                        Ok(crate::transfer::Outcome::Refused {
                            client_message_id,
                            reason,
                        }) => {
                            let _ = avisos
                                .send(ServerMessage::AttachmentRefused {
                                    client_message_id,
                                    reason,
                                })
                                .await;
                        }
                        Err(erro) => {
                            // Sem `client_message_id` não há a quem responder:
                            // o cabeçalho é justamente o que não foi lido. Uma
                            // transferência que cai antes do cabeçalho é uma
                            // transferência que o cliente sabe que caiu.
                            tracing::debug!(%erro, "uma transferência terminou sem cabeçalho");
                        }
                    }
                });
            }
        })
    };
    let _recebedora = AbortaAoSair(recebedora);

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
                tela: tela_tx.clone(),
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

    // E quais Cages já estão transmitindo tela, pelo mesmo motivo e no mesmo
    // lugar.
    //
    // §3.6 escreve esta regra como «reenviado a quem entra num Cage que já
    // está transmitindo», e ela é atendida aqui em vez de na entrada do Cage
    // porque `ScreenShareStarted` sai do barramento **sem filtro de sala** —
    // do jeito que `PilotJoined` passou a sair quando o cliente começou a
    // desenhar todos os Cages. Com a difusão cobrindo tudo o que acontece a
    // partir de agora, o que falta é exatamente o que já estava acontecendo
    // antes de esta conexão existir, e isso é uma varredura, uma vez.
    //
    // Depois do `subscribe()` acima pelo mesmo motivo que a ocupação: a ordem
    // inversa pode perder uma transmissão que começou entre as duas linhas, e
    // uma transmissão perdida é uma tela que ninguém sabe que existe.
    for (cage, pilot, screen) in dogma.telas.lock().await.todas() {
        frame::write(
            &mut send,
            &ServerMessage::ScreenShareStarted {
                cage,
                pilot,
                screen,
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
                        assentar(dogma, cages, session, &outbound_tx, &tela_tx, id).await?;
                        current_cage = Some(id);
                    }
                    ClientMessage::EjectPlug => {
                        cages.leave_everywhere(session.pilot).await;
                        encerrar_telas_de(dogma, session.pilot).await;
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
                                attachment: stored.attachment,
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

                    // ---- what the Dogma calls itself ----
                    //
                    // `AdministerDogma`, and not the `ManageCages` of the four
                    // room verbs above. `specs/04-servidor-seele.md` calls
                    // `gerenciar_cages` "criar e configurar Cages" and
                    // `administrar_dogma` "todo o resto sobre o Dogma": the
                    // name and the picture of the Dogma itself are not a room,
                    // and somebody trusted to build rooms is not thereby the
                    // person whose Dogma it is.
                    //
                    // Read from MELCHIOR **now**, like every verb here and for
                    // the same reason. Denial answers, because a refusal nobody
                    // is told about is indistinguishable from a Dogma that is
                    // broken.
                    //
                    // Committed **before** it is announced, like a message and
                    // unlike nothing else here: the bus carries what the
                    // database already holds, so a Dogma that is renamed and
                    // then loses power comes back with the name everybody was
                    // told about.
                    ClientMessage::RenameDogma { name } => {
                        if !pode(dogma, session.pilot, Permission::AdministerDogma).await {
                            recusar(&mut send, session.pilot, "RenameDogma").await?;
                            continue;
                        }
                        let feito = {
                            let guard = dogma.casper.lock().await;
                            crate::casper::aparencia::definir_nome(&guard, &name)
                        };
                        match feito {
                            Ok(name) => {
                                tracing::info!(by = %session.pilot, %name, "the Dogma was renamed");
                                let _ = dogma.events.send(Event::DogmaRenamed { name });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::SetDogmaIcon { icon } => {
                        if !pode(dogma, session.pilot, Permission::AdministerDogma).await {
                            recusar(&mut send, session.pilot, "SetDogmaIcon").await?;
                            continue;
                        }
                        // Nada confere o formato aqui. `frame::read` já recusou
                        // o quadro se estes bytes não fossem um PNG dentro do
                        // teto — `seele_proto::control::check_icon` —, e uma
                        // segunda regra neste arquivo seria a que ficaria para
                        // trás da primeira.
                        let feito = {
                            let guard = dogma.casper.lock().await;
                            crate::casper::aparencia::definir_icone(&guard, icon.as_deref())
                        };
                        match feito {
                            Ok(()) => {
                                tracing::info!(
                                    by = %session.pilot,
                                    bytes = icon.as_ref().map_or(0, Vec::len),
                                    "the Dogma changed its icon"
                                );
                                let _ = dogma.events.send(Event::DogmaIconChanged { icon });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }

                    // ---- moderation ----
                    //
                    // `specs/04-servidor-seele.md` names the four permissions
                    // and migration 1 seeds them on the Comandante and the
                    // Operador; until now nothing on the wire could ask for
                    // any of them, so the app's `EJETAR PLUG DO OPERADOR` sat
                    // drawn and disabled with nothing to call.
                    //
                    // Read from MELCHIOR **now**, like the four room verbs
                    // above and for the same reason: an operator whose Kick was
                    // revoked a minute ago should not keep it until the next
                    // reconnection. And denial answers, because a refusal
                    // nobody is told about is indistinguishable from a Dogma
                    // that is broken.
                    ClientMessage::KickPilot { pilot: alvo } => {
                        if !moderavel(dogma, session.pilot, alvo, Permission::Kick).await {
                            recusar(&mut send, session.pilot, "KickPilot").await?;
                            continue;
                        }
                        tracing::info!(by = %session.pilot, %alvo, "kicked");
                        // The target's own session does the disconnecting. It
                        // owns its stream and its cleanup, and reaching into
                        // another connection from here would need a second way
                        // to find one — see the note on `Event::SessionEnded`.
                        let _ = dogma.events.send(Event::SessionEnded {
                            pilot: alvo,
                            reason: DisconnectReason::Kicked,
                        });
                    }
                    ClientMessage::BanPilot { pilot: alvo, reason, expires_at } => {
                        // Banning yourself locks a Dogma whose only Comandante
                        // is you, and there is no verb to undo it from outside.
                        if alvo == session.pilot
                            || !moderavel(dogma, session.pilot, alvo, Permission::Ban).await
                        {
                            recusar(&mut send, session.pilot, "BanPilot").await?;
                            continue;
                        }
                        // MELCHIOR checks the permission again inside `ban`.
                        // Not redundant on purpose: the check above is what
                        // produces the enumerated refusal a client can read,
                        // and the one in there is what no future caller can
                        // forget. specs/08-seguranca.md asks for the second.
                        let gravado = {
                            let guard = dogma.casper.lock().await;
                            Melchior::new(&guard).ban(
                                alvo,
                                session.pilot,
                                reason.as_deref(),
                                expires_at,
                            )
                        };
                        match gravado {
                            Ok(()) => {
                                tracing::info!(by = %session.pilot, %alvo, ?expires_at, "banned");
                                // A ban that let the offender stay until they
                                // chose to leave would do nothing about what
                                // prompted it. The handshake refuses them from
                                // here on; this is the session they are in now.
                                let _ = dogma.events.send(Event::SessionEnded {
                                    pilot: alvo,
                                    reason: DisconnectReason::Banned,
                                });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::RemoveMessage { message: id } => {
                        // The permission is worded "delete somebody **else's**
                        // message", so an author taking back their own does not
                        // need it — a Dogma where fixing your own typo needs an
                        // operator is a Dogma where people ask an operator
                        // about typos.
                        let alvo = {
                            let mut guard = dogma.casper.lock().await;
                            Messages::new(&mut guard).one(id).ok().flatten()
                        };
                        let Some(alvo) = alvo else {
                            // Already gone, or never there. Answered rather
                            // than ignored: a removal that silently does
                            // nothing looks exactly like one that worked.
                            nao_deu(&mut send, &anyhow::anyhow!("no such message")).await?;
                            continue;
                        };
                        let seu = alvo.author == session.pilot;
                        if !seu && !pode(dogma, session.pilot, Permission::RemoveMessage).await {
                            recusar(&mut send, session.pilot, "RemoveMessage").await?;
                            continue;
                        }
                        // Soft in CASPER, gone on screen — and the two are one
                        // decision, not two. `Messages::remove` clears the body
                        // and stamps `deleted_at`; `history` filters those out
                        // and `Room::apply` drops the line. So the message
                        // **disappears** for everybody, and what survives is a
                        // row that keeps replies pointing at it from dangling
                        // and keeps an operator able to answer "what was
                        // removed and by whom". A visible "removed by operator"
                        // stub was the alternative, and it preserves the
                        // disruption along with the fact of it.
                        let feito = {
                            let mut guard = dogma.casper.lock().await;
                            Messages::new(&mut guard).remove(id)
                        };
                        match feito {
                            Ok(()) => {
                                tracing::info!(by = %session.pilot, %id, own = seu, "message removed");
                                // The Line comes from the stored row, never
                                // from the asker: a Line the client filled in
                                // is a Line the client can fill in wrong, and
                                // it would aim somebody else's announcement.
                                let _ = dogma.events.send(Event::MessageRemoved {
                                    line: alvo.line,
                                    id,
                                });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::MovePilot { pilot: alvo, cage: destino } => {
                        if !moderavel(dogma, session.pilot, alvo, Permission::MovePilot).await {
                            recusar(&mut send, session.pilot, "MovePilot").await?;
                            continue;
                        }
                        tracing::info!(by = %session.pilot, %alvo, cage = %destino, "moved");
                        let _ = dogma.events.send(Event::PilotMoved {
                            pilot: alvo,
                            cage: destino,
                        });
                    }

                    // ---- unmaking a room ----
                    //
                    // `AdministerDogma`, and not the `ManageCages` the four
                    // room verbs above use. Creating and renaming are mistakes
                    // one survives; this is the one room verb that ends
                    // somebody else's writing, and no screen of this product
                    // brings it back. `specs/04-servidor-seele.md` calls
                    // `gerenciar_cages` "criar e configurar Cages" and
                    // `administrar_dogma` "todo o resto sobre o Dogma";
                    // destroying every message six people wrote is not
                    // configuration. Read now, from MELCHIOR, like all the
                    // others.
                    ClientMessage::DeleteCage { cage: id } => {
                        if !pode(dogma, session.pilot, Permission::AdministerDogma).await {
                            recusar(&mut send, session.pilot, "DeleteCage").await?;
                            continue;
                        }
                        let feito = {
                            let guard = dogma.casper.lock().await;
                            Channels::new(&guard).delete_cage(id)
                        };
                        match feito {
                            Ok(()) => {
                                tracing::info!(by = %session.pilot, cage = %id, "cage destroyed");
                                // A transmissão morre com a sala, e antes do
                                // aviso de que a sala morreu: quem está
                                // desenhando a tela para de desenhá-la porque
                                // ela acabou, e não porque o cômodo sumiu de
                                // baixo dela.
                                if let Some(screen) = dogma.telas.lock().await.encerrar_cage(id) {
                                    let _ = dogma.events.send(Event::ScreenShareStopped {
                                        cage: id,
                                        screen,
                                    });
                                }
                                let _ = dogma.events.send(Event::CageDeleted { cage: id });
                            }
                            // The only refusal here with a sentence of its own.
                            // Everything else a write can fail with is the
                            // database's business and goes to the operator's log.
                            Err(erro) if erro.downcast_ref::<LastCage>().is_some() => {
                                frame::write(&mut send, &ServerMessage::Alert {
                                    severity: AlertSeverity::Warning,
                                    reason: AlertReason::LastCage,
                                    operator_text: None,
                                }).await?;
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::DeleteLine { line: id } => {
                        if !pode(dogma, session.pilot, Permission::AdministerDogma).await {
                            recusar(&mut send, session.pilot, "DeleteLine").await?;
                            continue;
                        }
                        let feito = {
                            let guard = dogma.casper.lock().await;
                            Channels::new(&guard).delete_line(id)
                        };
                        match feito {
                            Ok(()) => {
                                tracing::info!(by = %session.pilot, line = %id, "line destroyed");
                                let _ = dogma.events.send(Event::LineDeleted { line: id });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    // A read, answered straight down this connection rather
                    // than over the bus: it is nobody else's business how heavy
                    // a Line looked to the person about to be asked whether
                    // they mean it.
                    //
                    // No permission. Answering tells a pilot how much is in a
                    // Line they may already read, and refusing would only mean
                    // the confirmation they are shown is the vaguer one.
                    ClientMessage::WeighLine { line: id } => {
                        let pesado = {
                            let guard = dogma.casper.lock().await;
                            Channels::new(&guard).weigh_line(id)
                        };
                        match pesado {
                            Ok(peso) => {
                                frame::write(&mut send, &ServerMessage::LineWeighed {
                                    line: id,
                                    messages: peso.messages,
                                    authors: peso.authors,
                                    oldest_at_seconds: peso.oldest_at_seconds,
                                }).await?;
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }

                    // ---- attachments ----
                    //
                    // Only the download crosses control, and only as an ask:
                    // ADR 0027 puts the bytes on a stream of their own in both
                    // directions. Sending is not here at all, because a sender
                    // opens its own stream — see the `accept_uni` task above.
                    ClientMessage::FetchAttachment { attachment } => {
                        // ReadLine and nothing more: a file hanging off a
                        // message somebody may read is part of that message,
                        // and `Permission::AttachFile` is about putting bytes
                        // on somebody's disk rather than about looking at them.
                        if !pode(dogma, session.pilot, Permission::ReadLine).await {
                            frame::write(&mut send, &ServerMessage::AttachmentUnavailable {
                                attachment,
                                reason: AttachmentRefusal::NotFound,
                            }).await?;
                            continue;
                        }
                        let Some(anexos) = dogma.anexos.clone() else {
                            frame::write(&mut send, &ServerMessage::AttachmentUnavailable {
                                attachment,
                                reason: AttachmentRefusal::Unavailable,
                            }).await?;
                            continue;
                        };
                        // In a task of its own, so that a twenty-megabyte
                        // download does not stop this loop from reading the
                        // next control frame. That is the entire point of the
                        // stream being separate, and doing the write here would
                        // hand it back.
                        let para_fora = connection.clone();
                        let contexto = Arc::clone(dogma);
                        let avisos = avisos_tx.clone();
                        tokio::spawn(async move {
                            match crate::transfer::deliver(
                                &anexos, &contexto, &para_fora, attachment,
                            ).await {
                                Ok(Ok(bytes)) => {
                                    tracing::debug!(%attachment, bytes, "attachment delivered");
                                }
                                Ok(Err(reason)) => {
                                    let _ = avisos.send(ServerMessage::AttachmentUnavailable {
                                        attachment,
                                        reason,
                                    }).await;
                                }
                                Err(erro) => {
                                    tracing::warn!(%attachment, %erro, "attachment delivery failed");
                                    let _ = avisos.send(ServerMessage::AttachmentUnavailable {
                                        attachment,
                                        reason: AttachmentRefusal::Unavailable,
                                    }).await;
                                }
                            }
                        });
                    }

                    // ---- compartilhamento de tela ----
                    //
                    // Só o controle passa aqui. Os quadros vão num fluxo
                    // unidirecional QUIC aberto por quem compartilha, e o §3.1
                    // da spec do recurso é a razão medida: `send_datagram` põe
                    // voz e vídeo na mesma fila FIFO, e
                    // `spikes/tela-no-transporte` viu 16,1% da voz perdida com
                    // o buffer padrão e 98,1% com o buffer pequeno.
                    ClientMessage::StartScreenShare => {
                        // Sentado antes de transmitir. Um Cage vindo de quem
                        // pergunta é um Cage que quem pergunta aponta para
                        // outro lugar, então a mensagem não o carrega e a
                        // resposta é a sala onde o plug está.
                        //
                        // `PermissionDenied` é a recusa mais próxima que existe
                        // enumerada, e ela não diz a verdade inteira: a pessoa
                        // **pode**, só não está em sala nenhuma. Um
                        // `NotInCage` falta em `AlertReason`, e não foi
                        // acrescentado aqui porque `seele-proto` é de outro
                        // dono nesta rodada — está no relatório. Enquanto isso,
                        // recusar com a frase errada continua sendo melhor que
                        // recusar em silêncio, que é indistinguível de um Dogma
                        // quebrado.
                        let Some(cage) = current_cage else {
                            recusar(&mut send, session.pilot, "StartScreenShare fora de Cage")
                                .await?;
                            continue;
                        };
                        // A mesma permissão da voz, e nenhuma nova: quem não
                        // pode transmitir mídia nesta sala não passa a poder
                        // transmitindo-a como imagem. `specs/08-seguranca.md`:
                        // verificado no servidor, sempre.
                        if !session.may_speak {
                            recusar(&mut send, session.pilot, "StartScreenShare").await?;
                            continue;
                        }
                        let screen = registry.issue_screen();
                        match dogma.telas.lock().await.comecar(cage, session.pilot, screen) {
                            Ok(()) => {
                                let _ = dogma.events.send(Event::ScreenShareStarted {
                                    cage,
                                    pilot: session.pilot,
                                    screen,
                                });
                            }
                            // Uma transmissão por sala (§6 item 3). Quem perdeu
                            // a corrida ouve uma frase verdadeira: a vaga está
                            // tomada, e não «você não pode».
                            Err(_dono) => {
                                frame::write(&mut send, &ServerMessage::Alert {
                                    severity: AlertSeverity::Info,
                                    reason: AlertReason::ScreenShareTaken,
                                    operator_text: None,
                                }).await?;
                            }
                        }
                    }
                    ClientMessage::StopScreenShare => {
                        let parada = match current_cage {
                            Some(cage) => dogma.telas.lock().await.parar(cage, session.pilot),
                            None => None,
                        };
                        if let (Some(cage), Some(screen)) = (current_cage, parada) {
                            let _ = dogma.events.send(Event::ScreenShareStopped { cage, screen });
                        }
                    }
                    ClientMessage::RequestKeyFrame { screen } => {
                        // Conferido contra o registro, e não repassado ao
                        // acaso: um `ScreenId` inventado seria uma maneira de
                        // pedir quadro-chave a quem transmite em outra sala, e
                        // §3.3 conta o que um quadro-chave custa — 65 KiB em
                        // 1080p, 446 ms do orçamento inteiro. Um pedido que
                        // atravessasse salas seria amplificação de graça.
                        let dono = match current_cage {
                            Some(cage) => dogma.telas.lock().await.em(cage),
                            None => None,
                        };
                        if let Some((sharer, corrente)) = dono {
                            if corrente == screen {
                                let _ = dogma.events.send(Event::KeyFrameRequested {
                                    screen,
                                    pilot: session.pilot,
                                    sharer,
                                });
                            }
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

            aviso = avisos_rx.recv() => {
                // A razão de uma transferência recusada, ou de um arquivo que
                // não vem. Escrita aqui porque este é o dono do fluxo de
                // controle, e não pela tarefa que descobriu o motivo.
                let Some(aviso) = aviso else { break };
                frame::write(&mut send, &aviso).await?;
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
                let event = match event {
                    Ok(event) => event,
                    // O barramento passou à frente desta sessão.
                    //
                    // Acontece quando as escritas para este par bloqueiam — o
                    // par parou de ler, a janela do QUIC fechou — e enquanto
                    // elas estão bloqueadas ninguém tira evento do barramento.
                    // Passado o anel, o `broadcast` descarta o mais antigo:
                    // `quantos` eventos que existiram e não existem mais **para
                    // esta conexão**, mensagens já gravadas em CASPER entre
                    // eles.
                    //
                    // Aqui havia um `let Ok(event) = event else { continue }`, e
                    // era a pendência nº 1 inteira: a sessão seguia, calada, com
                    // um buraco permanente no que aquele piloto vê. Ninguém dos
                    // dois lados ficava sabendo, e não havia número nenhum para
                    // olhar depois.
                    //
                    // Encerrar é o conserto, e não o castigo. O buraco não tem
                    // remendo no lugar: evento não tem endereço, então o Dogma
                    // não sabe dizer quais faltaram e o cliente não sabe pedir.
                    // Reconectar e buscar histórico, sim — é caminho que já
                    // existe, já testado, e é o que a bateria interna faz
                    // sozinha. O assento fica reservado pela janela de graça
                    // como em qualquer queda, então voltar não custa o lugar.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(quantos)) => {
                        dogma.atrasos.registrar(quantos);
                        tracing::warn!(
                            pilot = %session.pilot,
                            quantos,
                            "o barramento passou à frente desta sessão; encerrando para ela ressincronizar"
                        );
                        // Com prazo, ao contrário das outras despedidas: este é
                        // o caminho de um par que parou de ler, e escrever para
                        // quem não lê não termina nunca. Sem o prazo, o conserto
                        // trocaria uma sessão com buraco por uma sessão presa.
                        let _ = tokio::time::timeout(
                            Duration::from_secs(1),
                            frame::write(&mut send, &ServerMessage::Disconnecting {
                                reason: DisconnectReason::FellBehind,
                            }),
                        ).await;
                        let _ = send.finish();
                        despedir(&connection, &mut send, b"fell behind").await;
                        break;
                    }
                    // O barramento fechou: o Dogma está indo embora. Era um
                    // `continue`, que num canal fechado é um laço quente para
                    // sempre — `recv` volta na hora, com o mesmo erro.
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                // Two events are aimed at **this** connection rather than
                // forwarded by it. They are on the same bus as everything else
                // because a Dogma has no other way for one session to reach
                // another — see the note beside `Event::SessionEnded`.
                match &event {
                    Event::SessionEnded { pilot, reason } if *pilot == session.pilot => {
                        tracing::info!(pilot = %session.pilot, ?reason, "session ended by an operator");
                        // Told, and then closed. `specs/02-protocolo.md` wants
                        // a specific reason and `despedir` is what makes sure
                        // it reaches the other end before the connection dies
                        // — without it the client reads a transport error and
                        // says "não foi possível alcançar o Dogma", sending
                        // somebody to look for a network problem.
                        // And the seat is **not** held. The grace period exists
                        // for a train going into a tunnel; applied here it
                        // would put a kicked pilot straight back into the Cage
                        // they were removed from the moment they reconnected,
                        // which is the whole verb undone by a feature meant for
                        // something else.
                        current_cage = None;
                        let _ = frame::write(&mut send, &ServerMessage::Disconnecting {
                            reason: *reason,
                        }).await;
                        let _ = send.finish();
                        despedir(&connection, &mut send, b"moderated").await;
                        break;
                    }
                    Event::PilotMoved { pilot, cage: destino } if *pilot == session.pilot => {
                        assentar(dogma, cages, session, &outbound_tx, &tela_tx, *destino).await?;
                        current_cage = Some(*destino);
                        // Where the plug is now, and then that somebody put it
                        // there. Two frames because they are two different
                        // things: one is state this client has to fold in or go
                        // on speaking into the room it left, the other is a
                        // sentence only a shell knows how to write. Being moved
                        // in silence is indistinguishable from a client that
                        // lost track of where it was.
                        frame::write(&mut send, &ServerMessage::MovedToCage {
                            cage: *destino,
                        }).await?;
                        frame::write(&mut send, &ServerMessage::Alert {
                            severity: AlertSeverity::Info,
                            reason: AlertReason::MovedByOperator,
                            operator_text: None,
                        }).await?;
                        continue;
                    }
                    // A Cage does not vanish from under the feet of the people
                    // speaking in it. The plug comes out first — the same
                    // bookkeeping `EjectPlug` does, because it is the same
                    // thing happening without being asked for — and only then
                    // does this client hear that the room is gone.
                    //
                    // Order matters in the other direction too: `PilotLeft`
                    // goes out before `CageDeleted` reaches anybody, so no
                    // client is ever holding a roster for a room it has already
                    // been told to forget.
                    Event::CageDeleted { cage: id } if current_cage == Some(*id) => {
                        cages.leave_everywhere(session.pilot).await;
                        encerrar_telas_de(dogma, session.pilot).await;
                        current_cage = None;
                        dogma.occupancy.lock().await.vacate(*id, session.pilot);
                        let _ = dogma.events.send(Event::PilotLeft {
                            cage: *id,
                            pilot: session.pilot,
                        });
                        frame::write(&mut send, &ServerMessage::CageDeleted {
                            cage: *id,
                        }).await?;
                        // And then the sentence. Two frames for the reason
                        // `MovedToCage` gives: one is state this client has to
                        // fold in or go on sending voice into a room that is
                        // not there, the other is what the person should be
                        // told, and only a shell knows how to say it.
                        frame::write(&mut send, &ServerMessage::Alert {
                            severity: AlertSeverity::Warning,
                            reason: AlertReason::CageDeleted,
                            operator_text: None,
                        }).await?;
                        continue;
                    }
                    // The same for a Line this connection had open. It is
                    // dropped from `lines` here rather than left to rot: that
                    // list is what `translate` filters message traffic by, and
                    // a Line that stayed in it would make this connection the
                    // one that still asks about a room that is gone.
                    Event::LineDeleted { line: id } if lines.contains(id) => {
                        lines.retain(|aberta| aberta != id);
                        frame::write(&mut send, &ServerMessage::LineDeleted {
                            line: *id,
                        }).await?;
                        frame::write(&mut send, &ServerMessage::Alert {
                            severity: AlertSeverity::Warning,
                            reason: AlertReason::LineDeleted,
                            operator_text: None,
                        }).await?;
                        continue;
                    }
                    _ => {}
                }

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

    // E quantas vezes o controle de fluxo prendeu esta sessão.
    //
    // Lido do quinn, que já conta: um `STREAM_DATA_BLOCKED` enviado por este
    // lado quer dizer que o Dogma tinha quadro para escrever e o par não tinha
    // deixado espaço — quer dizer, **o par parou de ler**. É a primeira
    // suspeita de `docs/pendencias.md` #1 virada em número, e é o número que
    // separa "a rede está ruim" de "aquele cliente travou e o Dogma ficou
    // esperando por ele".
    let bloqueios = connection.stats().frame_tx.stream_data_blocked;
    if bloqueios > 0 {
        tracing::warn!(
            pilot = %session.pilot,
            bloqueios,
            "o fluxo de controle para este cliente encheu; ele parou de ler"
        );
    }

    Ok(())
}

/// Puts this connection's plug into a Cage, and tells the Dogma.
///
/// One function because there are two ways in — the pilot asks
/// ([`ClientMessage::InsertPlug`]) or somebody with [`Permission::MovePilot`]
/// decides — and the bookkeeping either way is identical: out of the old room
/// before into the new one, the occupancy rewritten, the departure and the
/// arrival both announced. Written twice, the copy that gets a line added is
/// never both of them, and the half that goes stale is the one that leaves
/// somebody in a room they are not in.
async fn assentar(
    dogma: &Dogma,
    cages: &crate::cage::Cages,
    session: &Session,
    outbound: &mpsc::Sender<Vec<u8>>,
    tela: &mpsc::Sender<crate::tela::AberturaDeTela>,
    destino: CageId,
) -> Result<()> {
    // Out of the old room before into the new one. Without this a pilot who
    // walks from one Cage to another is still a member of the first, and goes
    // on hearing it.
    cages.leave_everywhere(session.pilot).await;
    // E a tela vai junto. Uma transmissão não anda de sala com quem a manda:
    // quem ficou na sala anterior continuaria vendo o cabeçalho de um fluxo que
    // agora aponta para outro lugar, e o §6 item 3 só permite uma por sala —
    // levar a transmissão pela mão faria a pessoa tomar a vaga da sala nova sem
    // ter pedido.
    encerrar_telas_de(dogma, session.pilot).await;
    cages
        .of(destino)
        .await
        .send(CageCommand::Join {
            pilot: session.pilot,
            ssrc: session.ssrc,
            may_speak: session.may_speak,
            outbound: outbound.clone(),
            tela: tela.clone(),
        })
        .await?;

    // No burst of "who is already here": this connection was handed every
    // Cage's occupants when it started, and has been told about every arrival
    // and departure since, wherever it happened. Repeating the room it is
    // walking into would be telling it something it already knows.
    let saiu_de = {
        let mut occupancy = dogma.occupancy.lock().await;
        let mut saiu_de = occupancy.vacate_everywhere(session.pilot);
        saiu_de.retain(|anterior| *anterior != destino);
        occupancy.seat(
            destino,
            crate::dogma::Occupant {
                pilot: session.pilot,
                nickname: session.nickname.clone(),
                ssrc: session.ssrc,
            },
        );
        saiu_de
    };

    // Walking from one Cage to another is a departure and an arrival, and both
    // have to be said. Without the first, everybody watching the old room keeps
    // the pilot in it for ever — invisible while a client only drew its own
    // Cage, and a ghost now that it draws all of them.
    for anterior in saiu_de {
        let _ = dogma.events.send(Event::PilotLeft {
            cage: anterior,
            pilot: session.pilot,
        });
    }
    let _ = dogma.events.send(Event::PilotJoined {
        cage: destino,
        profile: PilotProfile {
            id: session.pilot,
            nickname: session.nickname.clone(),
            roles: Vec::new(),
        },
        ssrc: session.ssrc,
    });

    // **Nada de tela é reenviado aqui**, e o §3.6 pede que seja — «também
    // enviado a um piloto que entra num Cage onde já há transmissão». Ele é
    // atendido em outro lugar e melhor: `ScreenShareStarted` sai pelo
    // barramento **sem filtro**, como `PilotJoined` já sai desde que o cliente
    // passou a desenhar todos os Cages, e o que faltava — o que já estava
    // acontecendo antes de esta conexão existir — é mandado uma vez, no começo
    // da sessão, ao lado do retrato da ocupação. Reenviar aqui seria o mesmo
    // quadro duas vezes para quem já o tinha.
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

/// Whether this pilot may aim a moderation verb at that one.
///
/// Two questions, and both have to be yes.
///
/// The first is the permission, asked of MELCHIOR at the instant the verb is
/// used — `specs/08-seguranca.md`: "Toda ação é verificada no servidor,
/// sempre."
///
/// The second is not in `specs/04-servidor-seele.md`, and is here because
/// leaving it out has a name: **an Operador could ban the Comandante.** The
/// spec gives Operador "moderação", which includes `expulsar` and `banir`, and
/// gives Comandante everything — so promoting a friend to Operador for the
/// evening would hand them the ability to lock you out of the Dogma you are
/// hosting, permanently, with a verb the spec says they should have. That is
/// not moderation; it is a coup with the right permission attached.
///
/// So: somebody holding [`Permission::AdministerDogma`] can only be kicked,
/// banned or moved by somebody who holds it too. Between two Comandantes it
/// does nothing, which is right — they already trust each other with the whole
/// Dogma. It matters exactly at the line the spec draws between the two roles,
/// and it matters more once ADR 0022 puts a Dogma on the open internet, where
/// "the person I promoted" is not always somebody sitting in the same room.
async fn moderavel(dogma: &Dogma, quem: PilotId, alvo: PilotId, permission: Permission) -> bool {
    if !pode(dogma, quem, permission).await {
        return false;
    }
    if quem == alvo {
        return true;
    }
    let guard = dogma.casper.lock().await;
    let melchior = Melchior::new(&guard);
    // A database error reads as denial, like `pode`: a Dogma whose disk is
    // failing must not answer "nobody here is an administrator".
    let alvo_administra = melchior
        .may(alvo, Permission::AdministerDogma)
        .unwrap_or(true);
    let quem_administra = melchior
        .may(quem, Permission::AdministerDogma)
        .unwrap_or(false);
    !alvo_administra || quem_administra
}

/// Lê o fluxo de quem compartilha e o entrega ao Cage, que o encaminha.
///
/// §5.1, decidido em 22/08/2026: **o servidor encaminha, como já faz com a
/// voz.** Esta é a metade que lê; a que escreve é `crate::tela::bombear`, uma
/// por espectador, e o que as liga é o [`crate::cage::Cage`] — que é o único
/// lugar deste Dogma que sabe quem está na sala sem perguntar a ninguém.
///
/// # Só de quem o controle já autorizou
///
/// O fluxo não carrega identidade nenhuma, e não deve carregar: quem manda é a
/// conexão, como o `ssrc` de `Cage::forward`. Então a primeira pergunta é ao
/// registro que decidiu a corrida do §6 item 3 — este piloto está transmitindo
/// em alguma sala? —, e o `ScreenId` que o cabeçalho declara é conferido contra
/// o que **este Dogma** atribuiu. Sem essas duas linhas, abrir um fluxo seria
/// uma maneira de compartilhar tela sem pedir, e de assinar a transmissão de
/// outra pessoa.
async fn receber_tela(
    dogma: &Dogma,
    cages: &crate::cage::Cages,
    pilot: PilotId,
    primeiro: u8,
    fluxo: &mut quinn::RecvStream,
) -> Result<()> {
    let Some((cage, screen)) = dogma.telas.lock().await.de(pilot) else {
        // Parar o fluxo e não ignorá-lo: um cliente que abriu sem pedir tem de
        // descobrir agora, e não pela imagem que nunca aparece do outro lado.
        let _ = fluxo.stop(quinn::VarInt::from_u32(crate::tela::CODIGO_DE_CORTE));
        bail!("um fluxo de tela chegou de quem não está transmitindo");
    };

    let mut abertura = [0_u8; SCREEN_HEADER_LEN];
    if let Some(resto) = abertura.get_mut(1..) {
        fluxo.read_exact(resto).await?;
    }
    if let Some(primeiro_byte) = abertura.first_mut() {
        *primeiro_byte = primeiro;
    }
    let (cabecalho, _) = seele_proto::screen::ScreenHeader::decode(&abertura)?;
    if cabecalho.screen != screen {
        let _ = fluxo.stop(quinn::VarInt::from_u32(crate::tela::CODIGO_DE_CORTE));
        bail!(
            "um fluxo de tela declarou {} e não {screen}",
            cabecalho.screen
        );
    }

    let sala = cages.of(cage).await;
    // Uma vaga só: o Dogma encerra uma transmissão uma vez, e a segunda razão
    // não teria o que dizer.
    let (fim_tx, mut fim_rx) = mpsc::channel::<crate::tela::FimDaTela>(1);
    sala.send(CageCommand::TelaAbriu {
        from: pilot,
        screen,
        abertura: abertura.to_vec(),
        fim: fim_tx,
    })
    .await?;

    let mut buffer = vec![0_u8; crate::tela::LEITURA_LEN];
    loop {
        // Consultado entre duas leituras e não dentro de um `select!`: a
        // leitura do `quinn` não é cancel-safe, e um `select!` que a cancelasse
        // no meio perderia os bytes já retirados do fluxo — o mesmo defeito que
        // a tarefa leitora do controle existe para não ter.
        if let Ok(motivo) = fim_rx.try_recv() {
            tracing::info!(%pilot, %cage, %screen, ?motivo, "o Dogma encerrou uma transmissão");
            let _ = fluxo.stop(quinn::VarInt::from_u32(crate::tela::CODIGO_DE_CORTE));
            // Anunciado, porque o plano de controle é o único lugar de onde a
            // sala aprende que a tela parou. Sem isto ficaria desenhada uma
            // transmissão que já não tem quem a bombeie.
            encerrar_telas_de(dogma, pilot).await;
            return Ok(());
        }
        match fluxo.read(&mut buffer).await? {
            Some(lidos) => {
                let bytes = buffer.get(..lidos).unwrap_or_default().to_vec();
                // `send` e não `try_send`: encher a fila do Cage tem de virar
                // contrapressão no QUIC de quem compartilha, que é onde ela
                // conserta alguma coisa. Descartar aqui deslocaria o
                // enquadramento de todos os espectadores de uma vez.
                sala.send(CageCommand::TelaBytes { from: pilot, bytes })
                    .await?;
            }
            None => break,
        }
    }
    // O fim limpo. Quem parou de propósito também manda `StopScreenShare` pelo
    // controle, e é ele que anuncia; quem sumiu é recolhido pelo fim da sessão.
    // Aqui só o encaminhamento morre, que é o que o §5.1 pôs sob esta função.
    let _ = sala.send(CageCommand::TelaFechou { from: pilot }).await;
    Ok(())
}

/// Encerra e anuncia o que este piloto estivesse transmitindo, onde estivesse.
///
/// Chamado em todo lugar onde o plug sai de um Cage — sair, ser movido, ser
/// expulso, ou a conexão acabar em qualquer `?` do meio do laço. Uma
/// transmissão que sobrevivesse à saída de quem a manda ficaria desenhada para
/// sempre na sala, prometendo um fluxo que não tem mais de onde vir: é o mesmo
/// defeito do piloto fantasma que `serve` conserta logo acima, com a diferença
/// de que aqui a promessa é de imagem em movimento.
async fn encerrar_telas_de(dogma: &Dogma, pilot: PilotId) {
    for (cage, screen) in dogma.telas.lock().await.encerrar_de(pilot) {
        let _ = dogma
            .events
            .send(Event::ScreenShareStopped { cage, screen });
    }
}

/// Tells a client the server said no, and why.
async fn recusar(send: &mut quinn::SendStream, pilot: PilotId, verbo: &str) -> Result<()> {
    tracing::warn!(%pilot, verbo, "refused: the server said no");
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
                    attachment: message.attachment.clone(),
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

        // Unfiltered, and to the pilot who asked as well, like the four above.
        // A header is drawn in every open window: filtering the asker out would
        // leave the one person who renamed the Dogma reading the old name.
        Event::DogmaRenamed { name } => Some(ServerMessage::DogmaRenamed { name: name.clone() }),
        Event::DogmaIconChanged { icon } => {
            Some(ServerMessage::DogmaIconChanged { icon: icon.clone() })
        }

        // Acted on by the connection they name, in the loop, and carrying
        // nothing for anybody else. A move is visible to everybody as the
        // `PilotLeft` and `PilotJoined` that `assentar` sends, and a session
        // ending is visible as the `PilotLeft` that `serve` sends when the
        // connection is gone — so there is nothing to translate here, and
        // inventing something would be a second way to say what those already
        // say.
        Event::SessionEnded { .. } | Event::PilotMoved { .. } => None,

        // Unfiltered, like the four announcements above and for the same
        // reason: a room that goes on being drawn until the next handshake is a
        // room people keep trying to walk into. The pilot who asked included —
        // they need to stop drawing it as much as anybody.
        //
        // The connections that were *inside* the Cage, or had the Line open,
        // never get here: the loop answers them itself and `continue`s, because
        // they have a plug to pull and a sentence to be told and this function
        // knows about neither.
        Event::CageDeleted { cage } => Some(ServerMessage::CageDeleted { cage: *cage }),
        Event::LineDeleted { line } => Some(ServerMessage::LineDeleted { line: *line }),

        // ---- compartilhamento de tela ----
        //
        // Sem filtro, e a quem compartilha também. É o mesmo caso de
        // `CageCreated` e por uma razão mais forte: o `ScreenId` é do servidor
        // para atribuir, e **quem compartilha precisa dele** antes de conseguir
        // abrir um fluxo. Filtrar a si mesmo aqui deixaria quem apertou o botão
        // como a única pessoa incapaz de transmitir.
        //
        // A todo mundo e não só ao Cage: é a mesma escolha que `PilotJoined`
        // fez ao deixar de filtrar por sala, e pelo mesmo motivo — a v3 desenha
        // todos os Cages, e uma sala que não diz que está transmitindo é uma
        // sala em que ninguém sabe que há o que assistir. Não revela nada que
        // entrar na sala já não revelasse.
        Event::ScreenShareStarted {
            cage,
            pilot,
            screen,
        } => Some(ServerMessage::ScreenShareStarted {
            cage: *cage,
            pilot: *pilot,
            screen: *screen,
        }),
        Event::ScreenShareStopped { cage, screen } => Some(ServerMessage::ScreenShareStopped {
            cage: *cage,
            screen: *screen,
        }),

        // A exceção: este só vai para quem está compartilhando. Um quadro-chave
        // custa 65 KiB em 1080p — 446 ms do orçamento inteiro (§3.3) —, e
        // mandar o pedido para a sala inteira faria toda máquina que assiste
        // acordar para um pedido que não é dela.
        Event::KeyFrameRequested {
            screen,
            pilot,
            sharer,
        } => (*sharer == self_pilot).then_some(ServerMessage::KeyFrameRequested {
            screen: *screen,
            pilot: *pilot,
        }),
    }
}
