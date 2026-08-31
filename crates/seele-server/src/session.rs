//! PERMISSIONS's front door — one connection's handshake and session.
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
//!    │◀── Sessao { id, server, voice_rooms, papeis }│   → CONEXÃO SEGURA
//! ```
//!
//! Before `Sessao` the client is in **PADRÃO: LARANJA** — connected, not
//! verified. The whole budget is 10 s, and failure produces a **specific**
//! reason: `specs/02-protocolo.md` says "never generic".
//!
//! # What the key proves, and what PERMISSIONS decides
//!
//! Verifying the signature over the nonce proves the peer holds the private key.
//! Turning that into an identity is [`crate::permissions`]'s job: it looks the key
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
    AlertReason, AlertSeverity, AttachmentRefusal, ChannelInfo, ClientMessage, DisconnectReason,
    Permission, PersonProfile, PersonState, Presence, Role, ServerMessage, Subsystem,
    SubsystemHealth, Telemetry, Validate, VoiceRoomInfo,
};
use seele_proto::ids::{ChannelId, PersonId, RoleId, ScreenId, SessionId, Ssrc, VoiceRoomId};
use seele_proto::screen::SCREEN_HEADER_LEN;
use seele_proto::signal::{Signal, SyncInputs};
use seele_proto::transport::HANDSHAKE_TIMEOUT;
use tokio::sync::mpsc;

use crate::permissions::{self, Permissions};
use crate::persistence::channels::{Channels, LastVoiceRoom};
use crate::persistence::messages::{Messages, PendingMessage, DEFAULT_PAGE};
use crate::persistence::Persistence;
use crate::server::{Event, Server};
use crate::taxa::{Veredito, Vigia};
use crate::voice_room::VoiceRoomCommand;
use crate::{frame, ServerConfig, PUBLIC_KEY_LEN};

/// Bytes of nonce the client signs.
const NONCE_LEN: usize = 32;

/// How many datagrams queue for one listener before the voice room sheds.
const OUTBOUND_DEPTH: usize = 256;

/// Quantos quadros de controle esperam a sessão antes de a leitura parar.
///
/// Limitado de propósito. Controle é raro — entrar num sala de voz, abrir uma Linha,
/// dizer uma frase — então um cliente honesto nunca chega perto disto; e um
/// desonesto encontra contrapressão em vez de memória do servidor para gastar.
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

/// O que o plano de mídia e o de controle precisam dizer um ao outro.
///
/// # Por que atômicos, e não um mutex
///
/// Porque um mutex aqui recriaria o defeito que a separação existe para
/// remover. O plano de mídia toca isto cinquenta vezes por segundo, e o de
/// controle o toca no meio de tratadores que já esperam o mutex do SQLite: pôr
/// um segundo cadeado entre os dois faria a voz voltar a esperar o disco, por
/// um caminho mais curto e mais difícil de ver.
///
/// Nenhum destes três valores precisa de leitura consistente com outro. Cada um
/// é uma palavra de máquina, lida e escrita sozinha, e `Relaxed` basta porque
/// não há nada cuja visibilidade dependa da ordem entre eles.
#[derive(Debug, Default)]
struct Midia {
    /// A sala de voz em que esta conexão está, **mais um**. Zero é «nenhuma».
    ///
    /// O deslocamento de um, e não `0` como sentinela: o `id` vem de uma
    /// `INTEGER PRIMARY KEY` do SQLite e hoje nunca vale zero, mas escrever a
    /// sentinela em cima dessa suposição amarra este arquivo a uma escolha do
    /// esquema que ninguém prometeu manter.
    sala: AtomicU32,
    /// Quando o último datagrama chegou, em milissegundos desde o início da
    /// sessão. [`u64::MAX`] enquanto nenhum chegou.
    ///
    /// Milissegundos desde uma origem, e não um `Instant`: `Instant` não cabe
    /// num atômico, e o que se pergunta a este valor — «faz menos de 250 ms?» —
    /// se responde igual com a diferença de dois números.
    ultimo_datagrama_ms: AtomicU64,
    /// Quadros de voz que o transporte recusou nesta sessão.
    recusados: AtomicU64,
}

impl Midia {
    fn nova() -> Self {
        Self {
            sala: AtomicU32::new(0),
            ultimo_datagrama_ms: AtomicU64::new(u64::MAX),
            recusados: AtomicU64::new(0),
        }
    }

    /// Onde esta conexão está, do ponto de vista de quem encaminha voz.
    fn sala(&self) -> Option<VoiceRoomId> {
        match self.sala.load(Ordering::Relaxed) {
            0 => None,
            mais_um => Some(VoiceRoomId(mais_um - 1)),
        }
    }

    /// Anuncia ao plano de mídia para onde a voz vai agora.
    ///
    /// Chamado de todo lugar que move `current_voice_room`, e é por isso que
    /// aquele valor continua existindo: o plano de controle decide com ele, em
    /// código síncrono e legível, e esta linha é a única ponte.
    fn entrou(&self, sala: Option<VoiceRoomId>) {
        let escrito = sala.map_or(0, |id| id.get().saturating_add(1));
        self.sala.store(escrito, Ordering::Relaxed);
    }

    /// Marca que voz acabou de chegar por esta conexão.
    fn chegou(&self, desde: Instant) {
        let ms = u64::try_from(desde.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.ultimo_datagrama_ms.store(ms, Ordering::Relaxed);
    }

    /// Se voz atravessou esta conexão nos últimos [`SPEAKING_TAIL`].
    ///
    /// A fonte honesta de «está falando» é o áudio chegando, e não o cliente
    /// dizendo que sim — um cliente que anuncia fala sem mandar nada acenderia
    /// o nome dele na lista de todo mundo por silêncio.
    fn falando(&self, desde: Instant) -> bool {
        let ms = self.ultimo_datagrama_ms.load(Ordering::Relaxed);
        if ms == u64::MAX {
            return false;
        }
        let agora = u64::try_from(desde.elapsed().as_millis()).unwrap_or(u64::MAX);
        Duration::from_millis(agora.saturating_sub(ms)) < SPEAKING_TAIL
    }
}

/// How often the server pushes telemetry.
///
/// `specs/07-estetica.md` wants the Sync Ratio alive on screen; once a
/// second looks live and costs nothing.
const TELEMETRY_INTERVAL: Duration = Duration::from_secs(1);

/// Hands out per-connection identifiers.
///
/// Person identifiers come from PERSISTENCE and survive restarts; these do not need
/// to. An `ssrc` is meaningful only for the life of a connection.
pub struct Registry {
    next_ssrc: AtomicU32,
    next_session: AtomicU64,
    /// O contador das transmissões de tela.
    ///
    /// **Separado do `ssrc` de propósito**, e o §3.6 da spec de
    /// compartilhamento de tela põe isso em negrito: o `ssrc` é o identificador
    /// de fonte de **áudio**, atribuído na entrada da sala de voz, e todo cliente
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
    pub person: PersonId,
    /// The media source bound to this connection.
    pub ssrc: Ssrc,
    /// Display name.
    pub nickname: String,
    /// May transmit voice.
    pub may_speak: bool,
    /// May post text.
    pub may_write: bool,
    /// A seat reclaimed from an earlier connection, if any.
    pub reclaimed_voice_room: Option<VoiceRoomId>,
    /// A versão de protocolo que este par declarou no `Hello`.
    ///
    /// Guardada porque quadro acrescentado depois da v1 só pode ser escrito para
    /// quem sabe decodificá-lo: o postcard não é autodescritivo, e uma variante
    /// desconhecida não é ignorada — ela desloca a leitura do fluxo daquele par
    /// para sempre. Ver o ADR 0036 e `seele_proto::version`.
    pub protocol_version: u8,
}

/// Runs the handshake, then the session, then cleans up.
///
/// # Errors
///
/// Returns the reason the connection ended.
pub async fn serve(
    connection: quinn::Connection,
    config: Arc<ServerConfig>,
    registry: Arc<Registry>,
    server: Arc<Server>,
    voice_rooms: Arc<crate::voice_room::VoiceRooms>,
) -> Result<()> {
    // O balde de antes de autenticar, consultado antes de qualquer trabalho.
    //
    // Aqui, e não depois do `Hello`, porque o que se protege é justamente o
    // trabalho que vem depois: ler e decodificar o quadro, e sobretudo o
    // Argon2id da admissão, que o ADR 0021 escolheu caro de propósito. Um
    // pacote que compra dezenas de milissegundos de CPU alheia é amplificação
    // boa demais para deixar de graça num servidor exposto.
    let admitido = {
        let ip = connection.remote_address().ip();
        server.portaria.lock().await.permitir(ip, Instant::now())
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
        // `worth_retrying` na casca já trata `RateLimited` como coisa que
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
        handshake(&mut send, &mut recv, &config, &registry, &server),
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
        person = %session.person,
        ssrc = %session.ssrc,
        nickname = %session.nickname,
        may_speak = session.may_speak,
        reclaimed = ?session.reclaimed_voice_room,
        "link verified"
    );

    // Guardado antes de a conexão ser movida: a limpeza lá embaixo precisa dele
    // para tirar esta conexão da soma da subida, e ali ela não existe mais.
    let id_da_conexao = connection.stable_id() as u64;

    let result = run_session(
        connection,
        send,
        recv,
        &session,
        &server,
        &voice_rooms,
        &registry,
    )
    .await;

    // Out of every room, not out of the one this connection remembers. The loop
    // above can end at any `?`, and a path that returns early does not know
    // where the person was sitting.
    voice_rooms.leave_everywhere(session.person).await;
    encerrar_telas_de(&server, session.person).await;

    // And **announced**, which it was not. `Event::PersonLeft` was sent only
    // from the `LeaveVoiceRoom` branch, so a person who closed their client, lost
    // their network or hit any `?` in the loop stayed in everybody else's
    // roster until they reconnected. Nobody saw it while a client only drew the
    // voice room it was sitting in and only learned of that voice room's arrivals; now that
    // every voice room is drawn with the people in it, a ghost is a ghost on screen.
    //
    // Here rather than at the end of `run_session` for the same reason as the
    // channel above: this is the one place every exit path passes through.
    for voice_room in server
        .occupancy
        .lock()
        .await
        .vacate_everywhere(session.person)
    {
        let _ = server.events.send(Event::PersonLeft {
            voice_room,
            person: session.person,
        });
    }

    // E some da lista de presentes, pelo mesmo caminho e pelo mesmo motivo do
    // laço acima: sem isto, todo cliente acumula o nome de quem já conectou
    // alguma vez e desenha todos como presentes. `PersonLeft` não cobre — ele
    // diz que uma sala esvaziou, e quem nunca se sentou não produz nenhum.
    if server.presentes.lock().await.saiu(session.person) {
        let _ = server.events.send(Event::PersonGone {
            person: session.person,
        });
    }

    // E os contadores desta conexão saem da soma da subida.
    //
    // Sem isto eles ficariam para sempre: a janela seguinte veria os bytes de
    // quem saiu sumirem de uma vez, o `saturating_sub` transformaria o delta
    // negativo em zero, e uma janela que carregou o cano inteiro apareceria como
    // «não entregou nada» — a sonda recuaria por causa de alguém desligando.
    server.subida.lock().await.esquecer(id_da_conexao);

    tracing::info!(person = %session.person, "session ended");
    result
}

/// Espera o motivo da recusa sair do fio antes de a conexão morrer.
///
/// `specs/02-protocolo.md` exige que a razão de uma recusa seja específica,
/// "nunca genérica". Escrever o `Disconnecting` não bastava: `bail!` devolve, a
/// `Connection` é recolhida, e o QUIC derruba tudo — inclusive o quadro que
/// ainda não tinha saído. **O cliente lia erro de conexão e mostrava "não foi
/// possível alcançar o servidor"**, mandando a pessoa procurar problema de rede
/// enquanto a resposta era "esse apelido é de outro pessoa".
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
    config: &ServerConfig,
    registry: &Registry,
    server: &Server,
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
    // servidor exposto na internet é varrido o dia inteiro.
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
        let guard = server.persistence.lock().await;
        let politica = crate::admissao::Politica::carregar(&guard).map_err(|error| Refusal {
            reason: DisconnectReason::CredentialRejected,
            detail: format!("could not read the admission policy: {error}"),
        })?;
        // A recusa aqui é **adiada**, e não devolvida na hora.
        //
        // O motivo é um defeito relatado em campo: «aprovei a entrada de
        // alguém e deu como credencial recusada». A política não tem memória —
        // com o servidor fechado ela exige segredo de todo mundo, sempre — e o
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
        let mut guard = server.persistence.lock().await;
        let impressao = seele_proto::transport::key_fingerprint(&public_key);
        let (segredo, observacao) = chegada;

        // Aqui a impressão digital **está provada**: a assinatura sobre o nonce
        // foi conferida acima. É o primeiro ponto do aperto de mão onde dá para
        // perguntar «esta pessoa já foi admitida?» sem que a resposta valha
        // para quem só afirmou ser ela.
        //
        // `ja_admitido` e não `bater`: aquela responde `Entra` a todo mundo com
        // a portaria desligada, e perdoar um segredo errado por causa disso
        // abriria a porta de todo servidor que não usa portaria.
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
            // e a falha aqui cai para o lado fechado como em `voice_room_liberado`.
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

    // PERMISSIONS turns the proven key into an account.
    let account = {
        let guard = server.persistence.lock().await;
        let permissions = Permissions::new(&guard);

        let person = permissions
            .register_or_find(&public_key, &nickname)
            .map_err(|error| {
                // O apelido tomado ganha razão própria, e é o conserto de um
                // defeito de campo: «aprovei a entrada e continua dando
                // credencial recusada, mesmo fechando o app». Quatro falhas
                // diferentes vestiam a mesma frase, e esta é a única que não
                // passa com o tempo — não adianta tentar de novo, ser aprovado
                // de novo, nem reinstalar. O nome é de outra chave, e só trocar
                // de nome resolve.
                //
                // Distinguir aqui não fere a uniformidade que a
                // `specs/08-seguranca.md` pede: esta linha é depois da
                // assinatura e depois da portaria, então nada foi adivinhado —
                // e num servidor sem portaria os apelidos que se poderiam
                // enumerar aqui são os que o roster entrega a quem entra.
                if matches!(
                    error.downcast_ref::<crate::permissions::Refusal>(),
                    Some(crate::permissions::Refusal::NicknameTaken)
                ) {
                    return Refusal {
                        reason: DisconnectReason::NicknameTaken,
                        detail: format!("o apelido {nickname} é de outra chave"),
                    };
                }
                Refusal {
                    reason: DisconnectReason::CredentialRejected,
                    detail: format!("could not establish an account: {error}"),
                }
            })?;

        if permissions.is_banned(person.id).unwrap_or(false) {
            return Err(Refusal {
                reason: DisconnectReason::Banned,
                detail: format!("person {} is banned", person.id),
            });
        }

        // Bootstrap: somebody has to be able to set the first roles before there
        // is an operator to do it. Applied through PERMISSIONS rather than around
        // it, so authorisation still has exactly one source of truth
        // (`specs/08-seguranca.md`).
        if config.observers.iter().any(|name| name == &nickname) {
            let _ = permissions.revoke_role(person.id, permissions::PERSON_ROLE);
            let _ = permissions.grant_role(person.id, permissions::OBSERVER_ROLE);
        }

        let may = |permission| permissions.may(person.id, permission).unwrap_or(false);
        let (voice_rooms, channels, roles) = read_server(&guard).map_err(|error| Refusal {
            reason: DisconnectReason::ServerShuttingDown,
            detail: format!("could not read the server: {error}"),
        })?;
        // Resolved here rather than left for the shell to work out from `roles`:
        // "negadas vencem concedidas" is one rule and belongs in one place.
        let permissions = permissions.permissions(person.id).unwrap_or_default();

        Account {
            id: person.id,
            nickname: person.nickname,
            may_speak: may(Permission::Speak),
            may_write: may(Permission::WriteChannel),
            voice_rooms,
            channels,
            roles,
            permissions,
        }
    };

    // Lido do PERSISTENCE, e não da [`ServerConfig`] que subiu o processo: renomear
    // com o servidor no ar é o caso normal — ADR 0032 —, e um nome que voltasse ao
    // do arranque no próximo reinício não seria um nome, seria uma sessão.
    // Ausência continua querendo dizer o padrão da configuração; ver
    // `persistence::aparencia`.
    //
    // Uma segunda tomada do mutex, e não um campo a mais na `Account`: são duas
    // perguntas sobre coisas diferentes — o que este pessoa é, e o que este
    // servidor é — e o aperto de mão já toma este mutex mais de uma vez.
    let (nome_do_server, icone_do_server) = {
        let guard = server.persistence.lock().await;
        let nome =
            crate::persistence::aparencia::nome(&guard, &config.name).unwrap_or_else(|erro| {
                // Um banco que não responde não pode deixar o servidor sem nome na
                // tela de quem entra. O padrão da configuração é a resposta honesta.
                tracing::warn!(%erro, "não deu para ler o nome do servidor");
                config.name.clone()
            });
        let icone = crate::persistence::aparencia::icone(&guard).unwrap_or_default();
        (nome, icone)
    };

    let (fresh_ssrc, session_id) = registry.issue();

    // specs/02-protocolo.md: the server holds the slot for the same five minutes
    // as the client's internal battery. A person returning inside that window
    // gets their own seat and their own `ssrc` back, so to everybody else the
    // outage looks like an outage rather than a departure and an arrival.
    let reclaimed = {
        let mut slots = server.slots.lock().await;
        slots.reclaim(account.id, Instant::now())
    };
    let (ssrc, reclaimed_voice_room) = match reclaimed {
        Some((voice_room, ssrc)) => (ssrc, Some(voice_room)),
        None => (fresh_ssrc, None),
    };

    frame::write(
        send,
        &ServerMessage::Session {
            id: session_id,
            person: account.id,
            ssrc,
            server: nome_do_server,
            voice_rooms: account.voice_rooms,
            channels: account.channels,
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
    // salas de voz, as Linhas, os papéis e as permissões dentro dos 16 KiB do
    // `MAX_FRAME_LEN`, e uma imagem disputando esse orçamento faria um servidor
    // grande deixar de admitir alguém por causa de uma decoração — a terceira
    // razão do ADR 0032, aqui respeitada em vez de contornada.
    //
    // **Só quando há ícone**, e o silêncio quer dizer «não há». O `Session`
    // descreve o servidor do zero — é dele que sai o nome, a lista de salas de voz e a de
    // Linhas —, então quem reconecta a um servidor cuja imagem foi tirada enquanto
    // ele estava fora para de desenhar a antiga por ter sido reapresentado ao
    // servidor, e não por receber um `None`. O que isso compra é que um servidor sem
    // ícone, que é todo servidor que existe hoje, troca exatamente os quadros que
    // trocava antes desta mudança.
    //
    // Um ícone que não passa pela conferência do protocolo é **descartado** em
    // vez de derrubar o aperto de mão. Os bytes vêm do banco, e quem tem o
    // arquivo tem um `sqlite3`: uma linha escrita à mão não pode ser o motivo
    // de ninguém mais conseguir entrar. Uma decoração nunca custa a conexão.
    if icone_do_server.is_some() {
        let anuncio = ServerMessage::ServerIconChanged {
            icon: icone_do_server,
        };
        match anuncio.validate() {
            Ok(()) => frame::write(send, &anuncio)
                .await
                .map_err(|error| Refusal {
                    reason: DisconnectReason::ProtocolViolation,
                    detail: format!("could not send the server icon: {error}"),
                })?,
            Err(erro) => {
                tracing::warn!(%erro, "o ícone guardado não é um ícone; seguindo sem ele");
            }
        }
    }

    let _ = client;
    Ok(Session {
        person: account.id,
        ssrc,
        nickname: account.nickname,
        may_speak: account.may_speak,
        may_write: account.may_write,
        reclaimed_voice_room,
        // O que **o par** declarou, e não `PROTOCOL_VERSION`: o que se decide
        // com isto é o que ele consegue ler.
        protocol_version: version,
    })
}

/// What the handshake learned from PERMISSIONS and PERSISTENCE.
struct Account {
    id: PersonId,
    nickname: String,
    may_speak: bool,
    may_write: bool,
    voice_rooms: Vec<VoiceRoomInfo>,
    channels: Vec<ChannelInfo>,
    roles: Vec<Role>,
    permissions: Vec<Permission>,
}

/// Reads the voice room and Channel tree, and the roles, out of PERSISTENCE.
fn read_server(
    persistence: &Persistence,
) -> Result<(Vec<VoiceRoomInfo>, Vec<ChannelInfo>, Vec<Role>)> {
    let connection = persistence.connection();

    // The same reader the creating verbs use, so the tree the handshake sends
    // and the tree a new room lands in cannot drift apart.
    let channels = Channels::new(persistence);
    let voice_rooms = channels.voice_rooms()?;
    let channels = channels.channels()?;

    let mut role_statement = connection.prepare("SELECT id, name, permissions FROM roles")?;
    let roles = role_statement
        .query_map([], |row| {
            let permissions: String = row.get(2)?;
            Ok(Role {
                id: RoleId(row.get::<_, i64>(0)? as u32),
                name: row.get(1)?,
                permissions: permissions::permissions_from_json(&permissions),
            })
        })?
        .filter_map(Result::ok)
        .collect();

    Ok((voice_rooms, channels, roles))
}

/// The session loop.
#[allow(clippy::too_many_lines, reason = "one select over every event source")]
async fn run_session(
    connection: quinn::Connection,
    mut send: quinn::SendStream,
    recv: quinn::RecvStream,
    session: &Session,
    server: &Arc<Server>,
    voice_rooms: &Arc<crate::voice_room::VoiceRooms>,
    registry: &Registry,
) -> Result<()> {
    // Uma tarefa é dona do fluxo de leitura e entrega quadros inteiros por um
    // canal.
    //
    // Ler direto dentro do `select!` abaixo era um defeito, e um caro. `read`
    // faz dois `read_exact` — o tamanho e o corpo — e o `select!` cancela o que
    // perde a corrida. Cancelado entre eles, o que já
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
    // sem teto na memória do servidor. Cheio, a tarefa leitora para de ler, e a
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
    // Aceita **sempre**, inclusive num servidor que não guarda arquivo nenhum.
    // Não aceitar deixaria o fluxo pendurado até o tempo ocioso do QUIC
    // recolher a conexão, com a barra do outro lado parada em zero e nada
    // sendo dito: exatamente a forma de falhar que este projeto recusa em toda
    // outra porta. Sem diretório, a resposta é `Unavailable` — uma frase.
    //
    // Guardada fora do `let` seguinte pelo motivo de sempre: o `AbortaAoSair`
    // cairia no fim de um bloco e mataria a tarefa no instante seguinte ao de
    // a criar.
    let recebedora = {
        let anexos = server.anexos.clone();
        let entrada = Arc::clone(server);
        let avisos = avisos_tx.clone();
        let conexao = connection.clone();
        let pessoa = session.person;
        let apelido = session.nickname.clone();
        let salas = Arc::clone(voice_rooms);
        tokio::spawn(async move {
            while let Ok(mut fluxo) = conexao.accept_uni().await {
                let anexos = anexos.clone();
                let contexto = Arc::clone(&entrada);
                let avisos = avisos.clone();
                let apelido = apelido.clone();
                let salas = Arc::clone(&salas);
                // Qual dos dois usos de fluxo unidirecional é este, dito pelo
                // fio em vez de deduzido dele. Era aritmética sobre o primeiro
                // byte do cabeçalho — zero é anexo porque um quadro de controle
                // cabe em 16 KiB, não-zero é tela porque a versão nasceu em 1 —
                // e as duas premissas eram emprestadas de outra seção (§5.2).
                // No dia em que uma delas mudasse, o sintoma seria um fluxo
                // lido como o tipo errado, que é o pior erro de protocolo que
                // existe.
                let mut marca = [0_u8; 1];
                if fluxo.read_exact(&mut marca).await.is_err() {
                    continue;
                }
                let tipo = seele_proto::stream::StreamType::decode(
                    marca.first().copied().unwrap_or_default(),
                );
                match tipo {
                    Ok(seele_proto::stream::StreamType::Screen) => {
                        // Uma tarefa por transmissão, como abaixo: quem
                        // compartilha não pode fazer fila com quem manda
                        // arquivo.
                        tokio::spawn(async move {
                            if let Err(erro) =
                                receber_tela(&contexto, &salas, pessoa, &avisos, &mut fluxo).await
                            {
                                tracing::debug!(%pessoa, %erro, "o fluxo de tela terminou");
                            }
                        });
                        continue;
                    }
                    Ok(seele_proto::stream::StreamType::Attachment) => {}
                    // Inclusive o zero, que é o valor que a leitura antiga
                    // produzia: recusá-lo faz um par velho falhar alto em vez
                    // de despejar um cabeçalho de anexo onde se espera um de
                    // tela. Parar o fluxo e não ignorá-lo, para que o outro
                    // lado descubra agora.
                    Err(_) => {
                        let _ = fluxo.stop(quinn::VarInt::from_u32(crate::tela::CODIGO_DE_CORTE));
                        tracing::debug!(%pessoa, "um fluxo unidirecional chegou sem tipo conhecido");
                        continue;
                    }
                }
                // Uma tarefa por transferência: duas pessoas mandando ao mesmo
                // tempo não se enfileiram uma atrás da outra.
                tokio::spawn(async move {
                    let Some(anexos) = anexos else {
                        // O cabeçalho é lido só para saber a quem responder: a
                        // chave de idempotência é o único nome que as duas
                        // pontas compartilham antes de o servidor ter atribuído
                        // coisa alguma. Os bytes não são lidos.
                        match crate::transfer::quem_perguntou(&mut fluxo).await {
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
                    match crate::transfer::receive(&anexos, &contexto, pessoa, &apelido, &mut fluxo)
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
    let mut events = server.events.subscribe();
    let mut channels: Vec<ChannelId> = Vec::new();
    let mut current_voice_room: Option<VoiceRoomId> = None;

    // A origem contra a qual o plano de mídia carimba o último datagrama.
    let inicio = Instant::now();
    let midia = Arc::new(Midia::nova());

    // O plano de mídia sai do `select!` do controle, e esta é a correção.
    //
    // # O defeito, escrito antes de ser removido
    //
    // Os dois braços de voz — `read_datagram` e `outbound_rx` — viviam no
    // mesmo `select!` que os quadros de controle, o barramento de eventos e o
    // tique de telemetria. O `select!` roda o corpo do braço vencedor **até o
    // fim** antes de voltar ao topo, e os tratadores de controle esperam duas
    // coisas demoradas: `server.persistence.lock().await`, num mutex de SQLite
    // que é um só para todas as sessões, e `frame::write(...).await`, que
    // bloqueia quando o par para de ler. Enquanto qualquer uma delas esperava,
    // esta conexão não lia nem escrevia voz.
    //
    // Medido em `tests/voz_sob_carga.rs`, que reprovava antes desta tarefa
    // existir: com o fluxo de controle do falante fechado, **zero** de cinquenta
    // quadros atravessavam. Não era voz picotada, era voz nenhuma.
    //
    // É a mesma forma que a tarefa leitora acima já removeu do fluxo de
    // controle, e a mesma da pendência nº 1. A regra que fica: **o plano de
    // mídia não espera nada que o plano de controle espera.** Ele não toca no
    // banco, não escreve no fluxo de controle e não lê o barramento.
    let _tarefa_de_midia = AbortaAoSair({
        let connection = connection.clone();
        let voice_rooms = Arc::clone(voice_rooms);
        let midia = Arc::clone(&midia);
        let ssrc = session.ssrc;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    datagram = connection.read_datagram() => {
                        let Ok(bytes) = datagram else { break };
                        // Para a sala em que esta conexão está agora. Mandar
                        // para uma sala fixa era correto enquanto um servidor
                        // tinha uma, e virou fio trocado quando pôde ter duas.
                        let Some(id) = midia.sala() else { continue };
                        midia.chegou(inicio);
                        let _ = voice_rooms.of(id).await.send(VoiceRoomCommand::Datagram {
                            from: ssrc,
                            bytes: bytes.to_vec(),
                        }).await;
                    }

                    outbound = outbound_rx.recv() => {
                        let Some(bytes) = outbound else { break };
                        // Contado, e não descartado. Um datagrama do QUIC não é
                        // fragmentado: o texto viaja num fluxo e se adapta ao
                        // caminho sozinho, a voz viaja aqui e um datagrama que
                        // não cabe é recusado inteiro. É assim que um enlace
                        // entrega toda a conversa escrita e ainda pica a voz — e
                        // num sentido só, porque o caminho de ida não é o de
                        // volta.
                        //
                        // Contado por sessão e dito uma vez ao fim, e não por
                        // quadro: isto passa cinquenta vezes por segundo, e um
                        // log por quadro afogaria o arquivo no instante em que
                        // alguém precisa lê-lo.
                        if connection.send_datagram(bytes.into()).is_err() {
                            midia.recusados.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        })
    });

    // What this person has announced about themselves. Held here rather than
    // rebuilt each tick, because the telemetry broadcast carries it and a tick
    // that reported a hardcoded `false` would undo every mute a second later.
    let mut muted = false;
    let mut total_isolation = false;
    let mut presence = Presence::Available;
    // The last Sync Ratio this connection measured. Carried so that announcing
    // a mute does not report a ratio of zero alongside it — every client folds
    // the whole `PersonState` in, so a field left at a default is not left
    // alone, it is overwritten.
    let mut last_ratio = 0_u8;
    let mut sync = Signal::new();
    let mut telemetry = tokio::time::interval(TELEMETRY_INTERVAL);
    telemetry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // A reclaimed seat means the person was already in a voice room when they dropped.
    if let Some(reclaimed) = session.reclaimed_voice_room {
        voice_rooms
            .of(reclaimed)
            .await
            .send(VoiceRoomCommand::Join {
                person: session.person,
                ssrc: session.ssrc,
                may_speak: session.may_speak,
                outbound: outbound_tx.clone(),
                tela: tela_tx.clone(),
            })
            .await?;
        current_voice_room = Some(reclaimed);
        midia.entrou(current_voice_room);
        server.occupancy.lock().await.seat(
            reclaimed,
            crate::server::Occupant {
                person: session.person,
                nickname: session.nickname.clone(),
                ssrc: session.ssrc,
            },
        );
        tracing::info!(person = %session.person, "seat reclaimed");
    }

    // Who is already seated, in **every** voice room — the whole picture, once.
    //
    // The wider half of gap G15. The narrow half was closed inside
    // `EnterVoiceRoom`: walk into an occupied voice room and the server listed the people
    // in *that* voice room. Every other voice room stayed empty on the client for the whole
    // session, because nothing had ever carried who was in it — and the screen
    // in `comp v3` draws occupants under all of them. That
    // is the defect reported from a real session as the voice_rooms showing empty
    // when they were not.
    //
    // Sent as `PersonJoined`, which is what the client already folds in, so this
    // needs no new message and no new arm in any of the three shells. From this
    // connection's point of view every one of these people did just arrive: it
    // is the moment it learned about them.
    //
    // After `events.subscribe()` above, deliberately. Subscribing first and
    // snapshotting second can duplicate an arrival — the client is idempotent
    // about that — while the other order would drop one, and a dropped arrival
    // is a person who is in the room and not on the screen, which is the whole
    // bug again.
    // Quem está aqui, sentado ou não — a metade que faltava da fotografia.
    //
    // A ocupação abaixo diz quem está em qual sala; esta diz quem está no
    // servidor. Sem ela, alguém que conecta e fica fora das salas não existe para
    // mais ninguém, e a lista de pessoas mostra os sentados chamando-se
    // «pessoas». Mesma ordem e mesmo motivo: depois do `subscribe`, para não
    // perder uma chegada entre as duas linhas.
    {
        let mut presentes = server.presentes.lock().await;
        for quem in presentes.todos() {
            if quem.person == session.person {
                continue;
            }
            frame::write(
                &mut send,
                &ServerMessage::PersonPresent {
                    profile: PersonProfile {
                        id: quem.person,
                        nickname: quem.nickname.clone(),
                        roles: Vec::new(),
                    },
                    ssrc: quem.ssrc,
                },
            )
            .await?;
        }
        // E esta conexão passa a existir para as próximas. Anunciada só quando
        // é chegada de verdade: uma reconexão dentro da carência é a mesma
        // pessoa, e um segundo anúncio faria a lista de todo mundo piscar.
        let chegou = presentes.chegou(crate::server::Occupant {
            person: session.person,
            nickname: session.nickname.clone(),
            ssrc: session.ssrc,
        });
        if chegou {
            let _ = server.events.send(Event::PersonPresent {
                quem: crate::server::Occupant {
                    person: session.person,
                    nickname: session.nickname.clone(),
                    ssrc: session.ssrc,
                },
            });
        }
    }

    for (voice_room, occupant) in server.occupancy.lock().await.everywhere() {
        if occupant.person == session.person {
            continue;
        }
        frame::write(
            &mut send,
            &ServerMessage::PersonJoined {
                voice_room,
                profile: PersonProfile {
                    id: occupant.person,
                    nickname: occupant.nickname.clone(),
                    roles: Vec::new(),
                },
                ssrc: occupant.ssrc,
            },
        )
        .await?;
    }

    // E quais salas de voz já estão transmitindo tela, pelo mesmo motivo e no mesmo
    // lugar.
    //
    // §3.6 escreve esta regra como «reenviado a quem entra num sala de voz que já
    // está transmitindo», e ela é atendida aqui em vez de na entrada da sala de voz
    // porque `ScreenShareStarted` sai do barramento **sem filtro de sala** —
    // do jeito que `PersonJoined` passou a sair quando o cliente começou a
    // desenhar todos as salas de voz. Com a difusão cobrindo tudo o que acontece a
    // partir de agora, o que falta é exatamente o que já estava acontecendo
    // antes de esta conexão existir, e isso é uma varredura, uma vez.
    //
    // Depois do `subscribe()` acima pelo mesmo motivo que a ocupação: a ordem
    // inversa pode perder uma transmissão que começou entre as duas linhas, e
    // uma transmissão perdida é uma tela que ninguém sabe que existe.
    for (voice_room, person, screen) in server.telas.lock().await.todas() {
        frame::write(
            &mut send,
            &ServerMessage::ScreenShareStarted {
                voice_room,
                person,
                screen,
            },
        )
        .await?;
    }

    // E quanto a subida **desta máquina** carrega, que é a perna do teto do
    // §5.1 que só o servidor sabe:
    //
    // ```text
    // teto = min(
    //     caminho de quem HOSPEDA × 60% ÷ N espectadores,   ← esta
    //     caminho de quem COMPARTILHA × 60%,
    //     o que a pessoa escolheu (§5),
    // )
    // ```
    //
    // Na entrada da sessão e não no `StartScreenShare`: quem aperta o botão
    // precisa do número **antes** de escolher resolução, e a alternativa —
    // mandar junto com a resposta — deixaria o seletor sem nada a dizer até que
    // a transmissão já tivesse começado. Uma vez, porque é uma declaração de
    // configuração e não uma medida: enquanto ninguém medir, ela não muda de
    // faixa e não há segundo quadro a mandar. Ver `crate::tela::caminho_no_fio`.
    // A trava sai **antes** da escrita, e a linha existe por causa disso.
    //
    // Escrito como argumento de `frame::write`, o `MutexGuard` é um temporário
    // que vive até o fim da instrução — e a instrução contém um `.await` que
    // escreve na rede. Um par que parou de ler bloqueia essa escrita, o mutex
    // fica preso, e todo aperto de mão novo e todo tique de telemetria param
    // atrás dele: o sintoma foi `HandshakeTimeout` no `acceptance_m3`. É a mesma
    // família da pendência 1 — segurar um recurso compartilhado enquanto se
    // escreve para quem não lê.
    let subida_medida = server.subida.lock().await.medida();
    frame::write(
        &mut send,
        &ServerMessage::HostUplink {
            bps: crate::tela::caminho_no_fio(server.caminho_bps, subida_medida),
        },
    )
    .await?;

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
                // ponto é não gastar o servidor com ele.
                match vigia.avaliar(Instant::now()) {
                    Veredito::Passa => {}
                    Veredito::Avisa => {
                        tracing::warn!(person = %session.person, "control frames over budget");
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
                            person = %session.person,
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
                    ClientMessage::EnterVoiceRoom { voice_room: id, password } => {
                        // A senha da sala de voz era declarada no protocolo, relatada
                        // ao cliente em `password_required` e **nunca
                        // conferida**. Uma fechadura que se anuncia trancada e
                        // não está é pior que porta aberta: quem confia nela
                        // toma decisão errada sobre o que dizer ali dentro.
                        if !crate::admissao::voice_room_liberado(
                            &*server.persistence.lock().await,
                            id,
                            password.as_deref(),
                        ) {
                            frame::write(&mut send, &ServerMessage::Alert {
                                severity: AlertSeverity::Warning,
                                reason: AlertReason::VoiceRoomEntryRefused,
                                operator_text: None,
                            }).await?;
                            continue;
                        }
                        assentar(server, voice_rooms, session, &outbound_tx, &tela_tx, id).await?;
                        current_voice_room = Some(id);
                        midia.entrou(current_voice_room);
                    }
                    ClientMessage::LeaveVoiceRoom => {
                        voice_rooms.leave_everywhere(session.person).await;
                        encerrar_telas_de(server, session.person).await;
                        if let Some(id) = current_voice_room.take() {
                            midia.entrou(current_voice_room);
                            server.occupancy.lock().await.vacate(id, session.person);
                            let _ = server.events.send(Event::PersonLeft {
                                voice_room: id,
                                person: session.person,
                            });
                        }
                    }
                    ClientMessage::JoinChannel { channel } => {
                        if !channels.contains(&channel) {
                            channels.push(channel);
                        }
                    }
                    ClientMessage::SendMessage { channel, body, replies_to, client_message_id } => {
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
                        server.post(PendingMessage {
                            channel,
                            author: session.person,
                            author_nickname: session.nickname.clone(),
                            body,
                            replies_to,
                            client_message_id: Some(client_message_id),
                        }).await?;
                    }
                    ClientMessage::FetchHistory { channel, cursor, limit } => {
                        let page = {
                            let mut guard = server.persistence.lock().await;
                            let messages = Messages::new(&mut guard);
                            messages.history(
                                channel,
                                cursor,
                                if limit == 0 { DEFAULT_PAGE } else { limit },
                            )?
                        };
                        // Oldest first on the wire, so a client can append.
                        for stored in page.into_iter().rev() {
                            frame::write(&mut send, &ServerMessage::MessageReceived {
                                channel: stored.channel,
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
                    // The roster shows all three (specs/07-estetica.md).
                    // Ignoring them, as this did, made every mute local-only:
                    // the marker existed and could never light up.
                    ClientMessage::SetMuted(on) => {
                        muted = on;
                        announce(server, session, &AnnouncedState {
                            muted,
                            total_isolation,
                            speaking: midia.falando(inicio),
                            presence,
                            signal: last_ratio,
                        });
                    }
                    ClientMessage::SetTotalIsolation(on) => {
                        total_isolation = on;
                        announce(server, session, &AnnouncedState {
                            muted,
                            total_isolation,
                            speaking: midia.falando(inicio),
                            presence,
                            signal: last_ratio,
                        });
                    }
                    ClientMessage::SetPresence(
                        announced @ (Presence::Available | Presence::Away
                                     | Presence::DoNotDisturb),
                    ) => {
                        presence = announced;
                        announce(server, session, &AnnouncedState {
                            muted,
                            total_isolation,
                            speaking: midia.falando(inicio),
                            presence,
                            signal: last_ratio,
                        });
                    }
                    // ---- rooms, made by whoever hosts ----
                    //
                    // The permission is read **now**, from PERMISSIONS, and not
                    // from anything the handshake cached. `may_speak` and
                    // `may_write` are cached because they are consulted per
                    // audio frame and per message; these four are consulted
                    // once in a while, and a Comandante who revoked somebody's
                    // ManageVoiceRooms a minute ago should not have to wait for that
                    // person to reconnect before it means anything.
                    //
                    // Denial answers with `PermissionDenied` rather than
                    // silence. specs/08-seguranca.md makes the server the
                    // security and the hidden button the convenience — but a
                    // refusal nobody is told about is indistinguishable from a
                    // server that is broken.
                    ClientMessage::CreateVoiceRoom { name, limit, channel } => {
                        if !pode(server, session.person, Permission::ManageVoiceRooms).await {
                            recusar(&mut send, session.person, "CreateVoiceRoom").await?;
                            continue;
                        }
                        let feito = {
                            let guard = server.persistence.lock().await;
                            Channels::new(&guard).create_voice_room(&name, limit, channel)
                        };
                        match feito {
                            Ok(voice_room) => {
                                tracing::info!(person = %session.person, voice_room = %voice_room.id, "voice room created");
                                let _ = server.events.send(Event::VoiceRoomCreated { voice_room });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::CreateChannel { name } => {
                        if !pode(server, session.person, Permission::ManageVoiceRooms).await {
                            recusar(&mut send, session.person, "CreateChannel").await?;
                            continue;
                        }
                        let feito = {
                            let guard = server.persistence.lock().await;
                            Channels::new(&guard).create_channel(&name)
                        };
                        match feito {
                            Ok(channel) => {
                                tracing::info!(person = %session.person, channel = %channel.id, "channel created");
                                let _ = server.events.send(Event::ChannelCreated { channel });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::RenameVoiceRoom { voice_room: id, name } => {
                        if !pode(server, session.person, Permission::ManageVoiceRooms).await {
                            recusar(&mut send, session.person, "RenameVoiceRoom").await?;
                            continue;
                        }
                        let feito = {
                            let guard = server.persistence.lock().await;
                            Channels::new(&guard).rename_voice_room(id, &name)
                        };
                        match feito {
                            Ok(name) => {
                                let _ = server.events.send(Event::VoiceRoomRenamed { voice_room: id, name });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::RenameChannel { channel: id, name } => {
                        if !pode(server, session.person, Permission::ManageVoiceRooms).await {
                            recusar(&mut send, session.person, "RenameChannel").await?;
                            continue;
                        }
                        let feito = {
                            let guard = server.persistence.lock().await;
                            Channels::new(&guard).rename_channel(id, &name)
                        };
                        match feito {
                            Ok(name) => {
                                let _ = server.events.send(Event::ChannelRenamed { channel: id, name });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }

                    // ---- what the server calls itself ----
                    //
                    // `AdministerServer`, and not the `ManageVoiceRooms` of the four
                    // room verbs above. `specs/04-servidor-seele.md` calls
                    // `gerenciar_voice_rooms` "criar e configurar voice_rooms" and
                    // `administrar_server` "todo o resto sobre o servidor": the
                    // name and the picture of the server itself are not a room,
                    // and somebody trusted to build rooms is not thereby the
                    // person whose server it is.
                    //
                    // Read from PERMISSIONS **now**, like every verb here and for
                    // the same reason. Denial answers, because a refusal nobody
                    // is told about is indistinguishable from a server that is
                    // broken.
                    //
                    // Committed **before** it is announced, like a message and
                    // unlike nothing else here: the bus carries what the
                    // database already holds, so a server that is renamed and
                    // then loses power comes back with the name everybody was
                    // told about.
                    ClientMessage::RenameServer { name } => {
                        if !pode(server, session.person, Permission::AdministerServer).await {
                            recusar(&mut send, session.person, "RenameServer").await?;
                            continue;
                        }
                        let feito = {
                            let guard = server.persistence.lock().await;
                            crate::persistence::aparencia::definir_nome(&guard, &name)
                        };
                        match feito {
                            Ok(name) => {
                                tracing::info!(by = %session.person, %name, "the server was renamed");
                                let _ = server.events.send(Event::ServerRenamed { name });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::SetServerIcon { icon } => {
                        if !pode(server, session.person, Permission::AdministerServer).await {
                            recusar(&mut send, session.person, "SetServerIcon").await?;
                            continue;
                        }
                        // Nada confere o formato aqui. `frame::read` já recusou
                        // o quadro se estes bytes não fossem um PNG dentro do
                        // teto — `seele_proto::control::check_icon` —, e uma
                        // segunda regra neste arquivo seria a que ficaria para
                        // trás da primeira.
                        let feito = {
                            let guard = server.persistence.lock().await;
                            crate::persistence::aparencia::definir_icone(&guard, icon.as_deref())
                        };
                        match feito {
                            Ok(()) => {
                                tracing::info!(
                                    by = %session.person,
                                    bytes = icon.as_ref().map_or(0, Vec::len),
                                    "the server changed its icon"
                                );
                                let _ = server.events.send(Event::ServerIconChanged { icon });
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
                    // Read from PERMISSIONS **now**, like the four room verbs
                    // above and for the same reason: an operator whose Kick was
                    // revoked a minute ago should not keep it until the next
                    // reconnection. And denial answers, because a refusal
                    // nobody is told about is indistinguishable from a server
                    // that is broken.
                    ClientMessage::KickPerson { person: alvo } => {
                        if !moderavel(server, session.person, alvo, Permission::Kick).await {
                            recusar(&mut send, session.person, "KickPerson").await?;
                            continue;
                        }
                        tracing::info!(by = %session.person, %alvo, "kicked");
                        // The target's own session does the disconnecting. It
                        // owns its stream and its cleanup, and reaching into
                        // another connection from here would need a second way
                        // to find one — see the note on `Event::SessionEnded`.
                        let _ = server.events.send(Event::SessionEnded {
                            person: alvo,
                            reason: DisconnectReason::Kicked,
                        });
                    }
                    ClientMessage::BanPerson { person: alvo, reason, expires_at } => {
                        // Banning yourself locks a server whose only Comandante
                        // is you, and there is no verb to undo it from outside.
                        if alvo == session.person
                            || !moderavel(server, session.person, alvo, Permission::Ban).await
                        {
                            recusar(&mut send, session.person, "BanPerson").await?;
                            continue;
                        }
                        // PERMISSIONS checks the permission again inside `ban`.
                        // Not redundant on purpose: the check above is what
                        // produces the enumerated refusal a client can read,
                        // and the one in there is what no future caller can
                        // forget. specs/08-seguranca.md asks for the second.
                        let gravado = {
                            let guard = server.persistence.lock().await;
                            Permissions::new(&guard).ban(
                                alvo,
                                session.person,
                                reason.as_deref(),
                                expires_at,
                            )
                        };
                        match gravado {
                            Ok(()) => {
                                tracing::info!(by = %session.person, %alvo, ?expires_at, "banned");
                                // A ban that let the offender stay until they
                                // chose to leave would do nothing about what
                                // prompted it. The handshake refuses them from
                                // here on; this is the session they are in now.
                                let _ = server.events.send(Event::SessionEnded {
                                    person: alvo,
                                    reason: DisconnectReason::Banned,
                                });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::RemoveMessage { message: id } => {
                        // The permission is worded "delete somebody **else's**
                        // message", so an author taking back their own does not
                        // need it — a server where fixing your own typo needs an
                        // operator is a server where people ask an operator
                        // about typos.
                        let alvo = {
                            let mut guard = server.persistence.lock().await;
                            Messages::new(&mut guard).one(id).ok().flatten()
                        };
                        let Some(alvo) = alvo else {
                            // Already gone, or never there. Answered rather
                            // than ignored: a removal that silently does
                            // nothing looks exactly like one that worked.
                            nao_deu(&mut send, &anyhow::anyhow!("no such message")).await?;
                            continue;
                        };
                        let seu = alvo.author == session.person;
                        if !seu && !pode(server, session.person, Permission::RemoveMessage).await {
                            recusar(&mut send, session.person, "RemoveMessage").await?;
                            continue;
                        }
                        // Soft in PERSISTENCE, gone on screen — and the two are one
                        // decision, not two. `Messages::remove` clears the body
                        // and stamps `deleted_at`; `history` filters those out
                        // and `Room::apply` drops the channel. So the message
                        // **disappears** for everybody, and what survives is a
                        // row that keeps replies pointing at it from dangling
                        // and keeps an operator able to answer "what was
                        // removed and by whom". A visible "removed by operator"
                        // stub was the alternative, and it preserves the
                        // disruption along with the fact of it.
                        let feito = {
                            let mut guard = server.persistence.lock().await;
                            Messages::new(&mut guard).remove(id)
                        };
                        match feito {
                            Ok(()) => {
                                tracing::info!(by = %session.person, %id, own = seu, "message removed");
                                // The Channel comes from the stored row, never
                                // from the asker: o canal the client filled in
                                // is o canal the client can fill in wrong, and
                                // it would aim somebody else's announcement.
                                let _ = server.events.send(Event::MessageRemoved {
                                    channel: alvo.channel,
                                    id,
                                });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::MovePerson { person: alvo, voice_room: destino } => {
                        if !moderavel(server, session.person, alvo, Permission::MovePerson).await {
                            recusar(&mut send, session.person, "MovePerson").await?;
                            continue;
                        }
                        tracing::info!(by = %session.person, %alvo, voice_room = %destino, "moved");
                        let _ = server.events.send(Event::PersonMoved {
                            person: alvo,
                            voice_room: destino,
                        });
                    }

                    // ---- unmaking a room ----
                    //
                    // `AdministerServer`, and not the `ManageVoiceRooms` the four
                    // room verbs above use. Creating and renaming are mistakes
                    // one survives; this is the one room verb that ends
                    // somebody else's writing, and no screen of this product
                    // brings it back. `specs/04-servidor-seele.md` calls
                    // `gerenciar_voice_rooms` "criar e configurar voice_rooms" and
                    // `administrar_server` "todo o resto sobre o servidor";
                    // destroying every message six people wrote is not
                    // configuration. Read now, from PERMISSIONS, like all the
                    // others.
                    ClientMessage::DeleteVoiceRoom { voice_room: id } => {
                        if !pode(server, session.person, Permission::AdministerServer).await {
                            recusar(&mut send, session.person, "DeleteVoiceRoom").await?;
                            continue;
                        }
                        let feito = {
                            let guard = server.persistence.lock().await;
                            Channels::new(&guard).delete_voice_room(id)
                        };
                        match feito {
                            Ok(()) => {
                                tracing::info!(by = %session.person, voice_room = %id, "voice room destroyed");
                                // A transmissão morre com a sala, e antes do
                                // aviso de que a sala morreu: quem está
                                // desenhando a tela para de desenhá-la porque
                                // ela acabou, e não porque o cômodo sumiu de
                                // baixo dela.
                                if let Some(screen) = server.telas.lock().await.encerrar_voice_room(id) {
                                    let _ = server.events.send(Event::ScreenShareStopped {
                                        voice_room: id,
                                        screen,
                                    });
                                }
                                let _ = server.events.send(Event::VoiceRoomDeleted { voice_room: id });
                            }
                            // The only refusal here with a sentence of its own.
                            // Everything else a write can fail with is the
                            // database's business and goes to the operator's log.
                            Err(erro) if erro.downcast_ref::<LastVoiceRoom>().is_some() => {
                                frame::write(&mut send, &ServerMessage::Alert {
                                    severity: AlertSeverity::Warning,
                                    reason: AlertReason::LastVoiceRoom,
                                    operator_text: None,
                                }).await?;
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    ClientMessage::DeleteChannel { channel: id } => {
                        if !pode(server, session.person, Permission::AdministerServer).await {
                            recusar(&mut send, session.person, "DeleteChannel").await?;
                            continue;
                        }
                        let feito = {
                            let guard = server.persistence.lock().await;
                            Channels::new(&guard).delete_channel(id)
                        };
                        match feito {
                            Ok(()) => {
                                tracing::info!(by = %session.person, channel = %id, "channel destroyed");
                                let _ = server.events.send(Event::ChannelDeleted { channel: id });
                            }
                            Err(erro) => nao_deu(&mut send, &erro).await?,
                        }
                    }
                    // A read, answered straight down this connection rather
                    // than over the bus: it is nobody else's business how heavy
                    // o canal looked to the person about to be asked whether
                    // they mean it.
                    //
                    // No permission. Answering tells a person how much is in a
                    // Channel they may already read, and refusing would only mean
                    // the confirmation they are shown is the vaguer one.
                    ClientMessage::WeighChannel { channel: id } => {
                        let pesado = {
                            let guard = server.persistence.lock().await;
                            Channels::new(&guard).weigh_channel(id)
                        };
                        match pesado {
                            Ok(peso) => {
                                frame::write(&mut send, &ServerMessage::ChannelWeighed {
                                    channel: id,
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
                        // ReadChannel and nothing more: a file hanging off a
                        // message somebody may read is part of that message,
                        // and `Permission::AttachFile` is about putting bytes
                        // on somebody's disk rather than about looking at them.
                        if !pode(server, session.person, Permission::ReadChannel).await {
                            frame::write(&mut send, &ServerMessage::AttachmentUnavailable {
                                attachment,
                                reason: AttachmentRefusal::NotFound,
                            }).await?;
                            continue;
                        }
                        let Some(anexos) = server.anexos.clone() else {
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
                        let contexto = Arc::clone(server);
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
                        // Sentado antes de transmitir. Uma sala de voz vindo de quem
                        // pergunta é uma sala de voz que quem pergunta aponta para
                        // outro lugar, então a mensagem não o carrega e a
                        // resposta é a sala onde o connection está.
                        //
                        // `PermissionDenied` é a recusa mais próxima que existe
                        // enumerada, e ela não diz a verdade inteira: a pessoa
                        // **pode**, só não está em sala nenhuma. Um
                        // `NotInVoiceRoom` falta em `AlertReason`, e não foi
                        // acrescentado aqui porque `seele-proto` é de outro
                        // dono nesta rodada — está no relatório. Enquanto isso,
                        // recusar com a frase errada continua sendo melhor que
                        // recusar em silêncio, que é indistinguível de um servidor
                        // quebrado.
                        let Some(voice_room) = current_voice_room else {
                            recusar(&mut send, session.person, "StartScreenShare fora de voice room")
                                .await?;
                            continue;
                        };
                        // A mesma permissão da voz, e nenhuma nova: quem não
                        // pode transmitir mídia nesta sala não passa a poder
                        // transmitindo-a como imagem. `specs/08-seguranca.md`:
                        // verificado no servidor, sempre.
                        if !session.may_speak {
                            recusar(&mut send, session.person, "StartScreenShare").await?;
                            continue;
                        }
                        let screen = registry.issue_screen();
                        match server.telas.lock().await.comecar(voice_room, session.person, screen) {
                            Ok(()) => {
                                let _ = server.events.send(Event::ScreenShareStarted {
                                    voice_room,
                                    person: session.person,
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
                        let parada = match current_voice_room {
                            Some(voice_room) => server.telas.lock().await.parar(voice_room, session.person),
                            None => None,
                        };
                        if let (Some(voice_room), Some(screen)) = (current_voice_room, parada) {
                            let _ = server.events.send(Event::ScreenShareStopped { voice_room, screen });
                        }
                    }
                    ClientMessage::RequestKeyFrame { screen } => {
                        // Conferido contra o registro, e não repassado ao
                        // acaso: um `ScreenId` inventado seria uma maneira de
                        // pedir quadro-chave a quem transmite em outra sala, e
                        // §3.3 conta o que um quadro-chave custa — 65 KiB em
                        // 1080p, 446 ms do orçamento inteiro. Um pedido que
                        // atravessasse salas seria amplificação de graça.
                        let dono = match current_voice_room {
                            Some(voice_room) => server.telas.lock().await.em(voice_room),
                            None => None,
                        };
                        if let Some((sharer, corrente)) = dono {
                            if corrente == screen {
                                let _ = server.events.send(Event::KeyFrameRequested {
                                    screen,
                                    person: session.person,
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

            // Os dois braços de voz que ficavam aqui vivem agora na tarefa de
            // mídia, criada acima. Ver a nota lá: enquanto estavam neste
            // `select!`, uma espera do plano de controle — o mutex do SQLite,
            // ou uma escrita para um par que parou de ler — parava a voz.

            aviso = avisos_rx.recv() => {
                // A razão de uma transferência recusada, ou de um arquivo que
                // não vem. Escrita aqui porque este é o dono do fluxo de
                // controle, e não pela tarefa que descobriu o motivo.
                let Some(aviso) = aviso else { break };
                frame::write(&mut send, &aviso).await?;
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
                    // esta conexão**, mensagens já gravadas em PERSISTENCE entre
                    // eles.
                    //
                    // Aqui havia um `let Ok(event) = event else { continue }`, e
                    // era a pendência nº 1 inteira: a sessão seguia, calada, com
                    // um buraco permanente no que aquele pessoa vê. Ninguém dos
                    // dois lados ficava sabendo, e não havia número nenhum para
                    // olhar depois.
                    //
                    // Encerrar é o conserto, e não o castigo. O buraco não tem
                    // remendo no lugar: evento não tem endereço, então o servidor
                    // não sabe dizer quais faltaram e o cliente não sabe pedir.
                    // Reconectar e buscar histórico, sim — é caminho que já
                    // existe, já testado, e é o que a bateria interna faz
                    // sozinha. O assento fica reservado pela janela de graça
                    // como em qualquer queda, então voltar não custa o lugar.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(quantos)) => {
                        server.atrasos.registrar(quantos);
                        tracing::warn!(
                            person = %session.person,
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
                    // O barramento fechou: o servidor está indo embora. Era um
                    // `continue`, que num canal fechado é um laço quente para
                    // sempre — `recv` volta na hora, com o mesmo erro.
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                // Two events are aimed at **this** connection rather than
                // forwarded by it. They are on the same bus as everything else
                // because a server has no other way for one session to reach
                // another — see the note beside `Event::SessionEnded`.
                // Quantos assistem decide **o que o cano tinha licença de
                // gastar**, e sem esse número a `Subida` não sabe distinguir «não
                // coube» de «não havia o que mandar». Aqui e não no conversor
                // evento→quadro porque aquele é síncrono e este mutex é `await`.
                //
                // Toda sessão faz isto com o mesmo valor, e é idempotente de
                // propósito: o barramento entrega o evento a todas, e escolher
                // uma delas para ser a dona seria inventar um líder onde não há.
                if let Event::ScreenViewers { quantos, .. } = &event {
                    server.subida.lock().await.assistindo(*quantos);
                }
                if matches!(event, Event::ScreenShareStopped { .. }) {
                    server.subida.lock().await.assistindo(0);
                }

                match &event {
                    // O teto contado do ADR 0038. Entregue só a quem pode agir:
                    // quem entrou numa sala não decide nada sobre o cano da
                    // casa, e um alerta sobre banda na tela de quem não pode
                    // fazer nada a respeito é ruído com aparência de informação.
                    Event::VoiceRoomOverHostUplink {
                        precisa_bps,
                        medido_bps,
                        ..
                    } => {
                        if pode(server, session.person, Permission::AdministerServer).await {
                            frame::write(
                                &mut send,
                                &ServerMessage::Alert {
                                    severity: AlertSeverity::Warning,
                                    reason: AlertReason::VoiceRoomOverHostUplink {
                                        precisa_bps: *precisa_bps,
                                        medido_bps: *medido_bps,
                                    },
                                    operator_text: None,
                                },
                            )
                            .await?;
                        }
                        continue;
                    }
                    Event::SessionEnded { person, reason } if *person == session.person => {
                        tracing::info!(person = %session.person, ?reason, "session ended by an operator");
                        // Told, and then closed. `specs/02-protocolo.md` wants
                        // a specific reason and `despedir` is what makes sure
                        // it reaches the other end before the connection dies
                        // — without it the client reads a transport error and
                        // says "não foi possível alcançar o servidor", sending
                        // somebody to look for a network problem.
                        // And the seat is **not** held. The grace period exists
                        // for a train going into a tunnel; applied here it
                        // would put a kicked person straight back into the voice room
                        // they were removed from the moment they reconnected,
                        // which is the whole verb undone by a feature meant for
                        // something else.
                        current_voice_room = None;
                        midia.entrou(current_voice_room);
                        let _ = frame::write(&mut send, &ServerMessage::Disconnecting {
                            reason: *reason,
                        }).await;
                        let _ = send.finish();
                        despedir(&connection, &mut send, b"moderated").await;
                        break;
                    }
                    Event::PersonMoved { person, voice_room: destino } if *person == session.person => {
                        assentar(server, voice_rooms, session, &outbound_tx, &tela_tx, *destino).await?;
                        current_voice_room = Some(*destino);
                        midia.entrou(current_voice_room);
                        // Where the connection is now, and then that somebody put it
                        // there. Two frames because they are two different
                        // things: one is state this client has to fold in or go
                        // on speaking into the room it left, the other is a
                        // sentence only a shell knows how to write. Being moved
                        // in silence is indistinguishable from a client that
                        // lost track of where it was.
                        frame::write(&mut send, &ServerMessage::MovedToVoiceRoom {
                            voice_room: *destino,
                        }).await?;
                        frame::write(&mut send, &ServerMessage::Alert {
                            severity: AlertSeverity::Info,
                            reason: AlertReason::MovedByOperator,
                            operator_text: None,
                        }).await?;
                        continue;
                    }
                    // A voice room does not vanish from under the feet of the people
                    // speaking in it. The connection comes out first — the same
                    // bookkeeping `LeaveVoiceRoom` does, because it is the same
                    // thing happening without being asked for — and only then
                    // does this client hear that the room is gone.
                    //
                    // Order matters in the other direction too: `PersonLeft`
                    // goes out before `VoiceRoomDeleted` reaches anybody, so no
                    // client is ever holding a roster for a room it has already
                    // been told to forget.
                    Event::VoiceRoomDeleted { voice_room: id } if current_voice_room == Some(*id) => {
                        voice_rooms.leave_everywhere(session.person).await;
                        encerrar_telas_de(server, session.person).await;
                        current_voice_room = None;
                        midia.entrou(current_voice_room);
                        server.occupancy.lock().await.vacate(*id, session.person);
                        let _ = server.events.send(Event::PersonLeft {
                            voice_room: *id,
                            person: session.person,
                        });
                        frame::write(&mut send, &ServerMessage::VoiceRoomDeleted {
                            voice_room: *id,
                        }).await?;
                        // And then the sentence. Two frames for the reason
                        // `MovedToVoiceRoom` gives: one is state this client has to
                        // fold in or go on sending voice into a room that is
                        // not there, the other is what the person should be
                        // told, and only a shell knows how to say it.
                        frame::write(&mut send, &ServerMessage::Alert {
                            severity: AlertSeverity::Warning,
                            reason: AlertReason::VoiceRoomDeleted,
                            operator_text: None,
                        }).await?;
                        continue;
                    }
                    // The same for o canal this connection had open. It is
                    // dropped from `channels` here rather than left to rot: that
                    // list is what `translate` filters message traffic by, and
                    // o canal that stayed in it would make this connection the
                    // one that still asks about a room that is gone.
                    Event::ChannelDeleted { channel: id } if channels.contains(id) => {
                        channels.retain(|aberta| aberta != id);
                        frame::write(&mut send, &ServerMessage::ChannelDeleted {
                            channel: *id,
                        }).await?;
                        frame::write(&mut send, &ServerMessage::Alert {
                            severity: AlertSeverity::Warning,
                            reason: AlertReason::ChannelDeleted,
                            operator_text: None,
                        }).await?;
                        continue;
                    }
                    _ => {}
                }

                if let Some(message) = translate(&event, &channels, session.person) {
                    // Um cliente v1 não conhece as variantes que a v2
                    // acrescentou, e o postcard não é autodescritivo: mandá-la
                    // não seria ignorada do outro lado — deslocaria o fluxo de
                    // controle dele para sempre, e a partir dali ele segue
                    // conectado sem entender mais nenhum quadro. A janela de
                    // compatibilidade do ADR 0036 promete que ele continua
                    // funcionando, e esta é a linha que cumpre a promessa.
                    let entende = match message {
                        ServerMessage::UplinkLoss { .. } => session.protocol_version >= 2,
                        _ => true,
                    };
                    if entende {
                        frame::write(&mut send, &message).await?;
                    }
                }
            }

            _ = telemetry.tick() => {
                // The server measures RTT and loss from QUIC itself, which is
                // the only vantage point that sees both directions. Jitter is
                // measured at the receiver, so the server reports zero rather
                // than a number it cannot know.
                let stats = connection.stats();

                // A subida deste servidor, medida enquanto ele empurra cópias.
                //
                // Aqui e não numa tarefa própria: este tique já existe, já lê
                // `connection.stats()`, e um relógio a mais para ler o mesmo
                // contador seria uma tarefa por sessão sem nada a mostrar. Quem
                // soma sobre as conexões é a `Subida`; esta linha só entrega a
                // fatia desta.
                let andou = {
                    let mut subida = server.subida.lock().await;
                    subida.observar(
                        std::time::Instant::now(),
                        connection.stable_id() as u64,
                        (&stats).into(),
                    )
                };
                if let Some(bps) = andou {
                    // Difundido, e não escrito aqui: a subida é do **cano**, e
                    // as três pernas do §5.1 são calculadas em cada cliente.
                    // Mandar só para este par deixaria os outros com o número
                    // velho e a mesma sala com dois tetos diferentes.
                    let _ = server.events.send(Event::HostUplink { bps });
                }

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
                        (Subsystem::Permissions, SubsystemHealth::Nominal),
                        (Subsystem::Media, SubsystemHealth::Nominal),
                        (Subsystem::Persistence, SubsystemHealth::Nominal),
                    ],
                })).await?;

                let _ = server.events.send(Event::PersonState(PersonState {
                    person: session.person,
                    muted,
                    total_isolation,
                    speaking: midia.falando(inicio),
                    presence,
                    signal: ratio,
                }));
            }
        }
    }

    // The connection is gone. Hold the seat for the grace window rather than
    // letting a tunnel cost somebody their place — specs/02-protocolo.md.
    if let Some(id) = current_voice_room {
        let mut slots = server.slots.lock().await;
        slots.reserve(session.person, id, session.ssrc, Instant::now());
    }
    // Coming out of the occupancy — and being announced — is `serve`'s job, and
    // it happens the moment this returns. It used to happen here, which meant
    // it did not happen at all on any path that left the loop through a `?`:
    // the seat was cleared for a clean shutdown and kept for ever for a broken
    // pipe, which is the case that actually occurs. Doing it there costs the
    // few microseconds between this channel and that one, and covers every exit.

    // Dito só quando aconteceu, e uma vez.
    //
    // Zero é o normal e não merece linha nenhuma; qualquer outro número
    // significa que a voz **saiu deste servidor pela metade** para aquele cliente,
    // e que quem ouviu culpou a rede. Sem esta linha não havia como saber: a
    // recusa era descartada, e recusa de envio soa exatamente igual a perda de
    // rede — com o conserto no lado oposto.
    let recusados = midia.recusados.load(Ordering::Relaxed);
    if recusados > 0 {
        tracing::warn!(
            person = %session.person,
            recusados,
            "o transporte recusou quadros de voz para este cliente; \
             o caminho até ele não comporta o tamanho do datagrama"
        );
    }

    // E quantas vezes o controle de fluxo prendeu esta sessão.
    //
    // Lido do quinn, que já conta: um `STREAM_DATA_BLOCKED` enviado por este
    // lado quer dizer que o servidor tinha quadro para escrever e o par não tinha
    // deixado espaço — quer dizer, **o par parou de ler**. É a primeira
    // suspeita de `docs/pendencias.md` #1 virada em número, e é o número que
    // separa "a rede está ruim" de "aquele cliente travou e o servidor ficou
    // esperando por ele".
    let bloqueios = connection.stats().frame_tx.stream_data_blocked;
    if bloqueios > 0 {
        tracing::warn!(
            person = %session.person,
            bloqueios,
            "o fluxo de controle para este cliente encheu; ele parou de ler"
        );
    }

    Ok(())
}

/// Puts this connection's connection into a voice room, and tells the server.
///
/// One function because there are two ways in — the person asks
/// ([`ClientMessage::EnterVoiceRoom`]) or somebody with [`Permission::MovePerson`]
/// decides — and the bookkeeping either way is identical: out of the old room
/// before into the new one, the occupancy rewritten, the departure and the
/// arrival both announced. Written twice, the copy that gets a channel added is
/// never both of them, and the half that goes stale is the one that leaves
/// somebody in a room they are not in.
async fn assentar(
    server: &Server,
    voice_rooms: &crate::voice_room::VoiceRooms,
    session: &Session,
    outbound: &mpsc::Sender<Vec<u8>>,
    tela: &mpsc::Sender<crate::tela::AberturaDeTela>,
    destino: VoiceRoomId,
) -> Result<()> {
    // Out of the old room before into the new one. Without this a person who
    // walks from one voice room to another is still a member of the first, and goes
    // on hearing it.
    voice_rooms.leave_everywhere(session.person).await;
    // E a tela vai junto. Uma transmissão não anda de sala com quem a manda:
    // quem ficou na sala anterior continuaria vendo o cabeçalho de um fluxo que
    // agora aponta para outro lugar, e o §6 item 3 só permite uma por sala —
    // levar a transmissão pela mão faria a pessoa tomar a vaga da sala nova sem
    // ter pedido.
    encerrar_telas_de(server, session.person).await;
    voice_rooms
        .of(destino)
        .await
        .send(VoiceRoomCommand::Join {
            person: session.person,
            ssrc: session.ssrc,
            may_speak: session.may_speak,
            outbound: outbound.clone(),
            tela: tela.clone(),
        })
        .await?;

    // No burst of "who is already here": this connection was handed every
    // voice room's occupants when it started, and has been told about every arrival
    // and departure since, wherever it happened. Repeating the room it is
    // walking into would be telling it something it already knows.
    let saiu_de = {
        let mut occupancy = server.occupancy.lock().await;
        let mut saiu_de = occupancy.vacate_everywhere(session.person);
        saiu_de.retain(|anterior| *anterior != destino);
        occupancy.seat(
            destino,
            crate::server::Occupant {
                person: session.person,
                nickname: session.nickname.clone(),
                ssrc: session.ssrc,
            },
        );
        saiu_de
    };

    // O teto contado do ADR 0038, conferido no instante em que a sala cresceu.
    //
    // Dentro de um `lock` próprio e depois do `seat` acima: o que a conta quer é
    // quantas pessoas há **com** esta, porque é essa sala que passou a existir.
    //
    // Nada aqui barra ninguém. A subida é uma estimativa, e recusar entrada por
    // estimativa tranca a pessoa fora do servidor dela por causa de um número que
    // o servidor deduziu — o `limit` que quem hospeda escreveu continua sendo o
    // único que barra.
    {
        let quantos = server.occupancy.lock().await.quantos(destino);
        let medido = server.subida.lock().await.medida().unwrap_or(0);
        // Os mesmos 60% que o §5.1 do compartilhamento de tela usa. Reusado, e
        // não inventado: uma segunda margem para a mesma máquina seria dois
        // orçamentos discordando sobre o mesmo cano.
        let orcamento = (u64::from(medido) * u64::from(crate::tela::FRACAO_DO_CAMINHO)) / 100;
        let pessoas = u32::try_from(quantos).unwrap_or(u32::MAX);
        // No teto do codec, que é o pior caso: quem está com rede ruim já manda
        // menos por causa do ADR 0036, então o pior caso real é um pouco menor.
        // Errar para cima num aviso é o lado seguro de errar.
        if seele_proto::transport::a_sala_acabou_de_estourar(
            pessoas,
            seele_proto::transport::MAX_BITRATE_BPS,
            orcamento,
        ) {
            let precisa = seele_proto::transport::subida_da_sala_bps(
                pessoas,
                seele_proto::transport::MAX_BITRATE_BPS,
            );
            tracing::info!(
                voice_room = %destino,
                pessoas,
                precisa_bps = precisa,
                medido_bps = medido,
                "esta sala passou do que a subida medida comporta"
            );
            let _ = server.events.send(Event::VoiceRoomOverHostUplink {
                voice_room: destino,
                precisa_bps: precisa,
                medido_bps: medido,
            });
        }
    }

    // Walking from one voice room to another is a departure and an arrival, and both
    // have to be said. Without the first, everybody watching the old room keeps
    // the person in it for ever — invisible while a client only drew its own
    // voice room, and a ghost now that it draws all of them.
    for anterior in saiu_de {
        let _ = server.events.send(Event::PersonLeft {
            voice_room: anterior,
            person: session.person,
        });
    }
    let _ = server.events.send(Event::PersonJoined {
        voice_room: destino,
        profile: PersonProfile {
            id: session.person,
            nickname: session.nickname.clone(),
            roles: Vec::new(),
        },
        ssrc: session.ssrc,
    });

    // **Nada de tela é reenviado aqui**, e o §3.6 pede que seja — «também
    // enviado a um pessoa que entra num sala de voz onde já há transmissão». Ele é
    // atendido em outro lugar e melhor: `ScreenShareStarted` sai pelo
    // barramento **sem filtro**, como `PersonJoined` já sai desde que o cliente
    // passou a desenhar todos as salas de voz, e o que faltava — o que já estava
    // acontecendo antes de esta conexão existir — é mandado uma vez, no começo
    // da sessão, ao lado do retrato da ocupação. Reenviar aqui seria o mesmo
    // quadro duas vezes para quem já o tinha.
    Ok(())
}

/// Asks PERMISSIONS, right now, whether this person may do something.
///
/// Every call takes the PERSISTENCE lock, which is the point: the answer is the one
/// that is true at the instant the verb is used, not the one that was true when
/// the connection opened. `specs/08-seguranca.md`: "Toda ação é verificada no
/// servidor, sempre." Control verbs are rare enough that the lock costs nothing
/// worth measuring — the frame budget in [`crate::taxa`] already caps how often
/// one connection can ask.
///
/// A database error reads as denial. The alternative is to let a server whose
/// disk is failing hand out `ManageVoiceRooms` to whoever asks while it fails.
async fn pode(server: &Server, person: PersonId, permission: Permission) -> bool {
    let guard = server.persistence.lock().await;
    Permissions::new(&guard)
        .may(person, permission)
        .unwrap_or(false)
}

/// Whether this person may aim a moderation verb at that one.
///
/// Two questions, and both have to be yes.
///
/// The first is the permission, asked of PERMISSIONS at the instant the verb is
/// used — `specs/08-seguranca.md`: "Toda ação é verificada no servidor,
/// sempre."
///
/// The second is not in `specs/04-servidor-seele.md`, and is here because
/// leaving it out has a name: **an Operador could ban the Comandante.** The
/// spec gives Operador "moderação", which includes `expulsar` and `banir`, and
/// gives Comandante everything — so promoting a friend to Operador for the
/// evening would hand them the ability to lock you out of the server you are
/// hosting, permanently, with a verb the spec says they should have. That is
/// not moderation; it is a coup with the right permission attached.
///
/// So: somebody holding [`Permission::AdministerServer`] can only be kicked,
/// banned or moved by somebody who holds it too. Between two Comandantes it
/// does nothing, which is right — they already trust each other with the whole
/// server. It matters exactly at the channel the spec draws between the two roles,
/// and it matters more once ADR 0022 puts a server on the open internet, where
/// "the person I promoted" is not always somebody sitting in the same room.
async fn moderavel(
    server: &Server,
    quem: PersonId,
    alvo: PersonId,
    permission: Permission,
) -> bool {
    if !pode(server, quem, permission).await {
        return false;
    }
    if quem == alvo {
        return true;
    }
    let guard = server.persistence.lock().await;
    let permissions = Permissions::new(&guard);
    // A database error reads as denial, like `pode`: a server whose disk is
    // failing must not answer "nobody here is an administrator".
    let alvo_administra = permissions
        .may(alvo, Permission::AdministerServer)
        .unwrap_or(true);
    let quem_administra = permissions
        .may(quem, Permission::AdministerServer)
        .unwrap_or(false);
    !alvo_administra || quem_administra
}

/// Lê o fluxo de quem compartilha e o entrega à sala de voz, que o encaminha.
///
/// §5.1, decidido em 22/08/2026: **o servidor encaminha, como já faz com a
/// voz.** Esta é a metade que lê; a que escreve é `crate::tela::bombear`, uma
/// por espectador, e o que as liga é o [`crate::voice_room::VoiceRoom`] — que é o único
/// lugar deste servidor que sabe quem está na sala sem perguntar a ninguém.
///
/// # Só de quem o controle já autorizou
///
/// O fluxo não carrega identidade nenhuma, e não deve carregar: quem manda é a
/// conexão, como o `ssrc` de `VoiceRoom::forward`. Então a primeira pergunta é ao
/// registro que decidiu a corrida do §6 item 3 — este pessoa está transmitindo
/// em alguma sala? —, e o `ScreenId` que o cabeçalho declara é conferido contra
/// o que **este servidor** atribuiu. Sem essas duas linhas, abrir um fluxo seria
/// uma maneira de compartilhar tela sem pedir, e de assinar a transmissão de
/// outra pessoa.
async fn receber_tela(
    server: &Server,
    voice_rooms: &crate::voice_room::VoiceRooms,
    person: PersonId,
    avisos: &mpsc::Sender<ServerMessage>,
    fluxo: &mut quinn::RecvStream,
) -> Result<()> {
    let Some((voice_room, screen)) = server.telas.lock().await.de(person) else {
        // Parar o fluxo e não ignorá-lo: um cliente que abriu sem pedir tem de
        // descobrir agora, e não pela imagem que nunca aparece do outro lado.
        let _ = fluxo.stop(quinn::VarInt::from_u32(crate::tela::CODIGO_DE_CORTE));
        bail!("um fluxo de tela chegou de quem não está transmitindo");
    };

    // Inteiro, e não remendado: o byte que a tarefa de aceitação consumiu é o
    // **tipo do fluxo**, e ele vem antes do cabeçalho em vez de ser o primeiro
    // byte dele. Era isto que o `primeiro` carregava de volta para cá.
    let mut abertura = [0_u8; SCREEN_HEADER_LEN];
    fluxo.read_exact(&mut abertura).await?;
    let (cabecalho, _) = seele_proto::screen::ScreenHeader::decode(&abertura)?;
    if cabecalho.screen != screen {
        let _ = fluxo.stop(quinn::VarInt::from_u32(crate::tela::CODIGO_DE_CORTE));
        bail!(
            "um fluxo de tela declarou {} e não {screen}",
            cabecalho.screen
        );
    }

    let sala = voice_rooms.of(voice_room).await;
    // Uma vaga só: o servidor encerra uma transmissão uma vez, e a segunda razão
    // não teria o que dizer.
    let (fim_tx, mut fim_rx) = mpsc::channel::<crate::tela::FimDaTela>(1);
    sala.send(VoiceRoomCommand::TelaAbriu {
        from: person,
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
            tracing::info!(%person, %voice_room, %screen, ?motivo, "o servidor encerrou uma transmissão");
            let _ = fluxo.stop(quinn::VarInt::from_u32(crate::tela::CODIGO_DE_CORTE));
            // Anunciado, porque o plano de controle é o único lugar de onde a
            // sala aprende que a tela parou. Sem isto ficaria desenhada uma
            // transmissão que já não tem quem a bombeie.
            encerrar_telas_de(server, person).await;
            // E com nome, para quem a mandava. `ScreenShareStopped` vai para a
            // sala inteira e não carrega razão de propósito — as duas maneiras
            // comuns de acabar já se distinguem sozinhas —, mas esta terceira
            // não: quem apertou parar sabe que apertou, e quem foi parado pelo
            // servidor não descobriria nada. A frase que falta é «a sala cresceu
            // além do que esta subida carrega», e ela é do §5.1.
            //
            // Pela sessão de quem compartilha, que é a única a quem ela
            // interessa, e pelo barramento de avisos: escrever no fluxo de
            // controle daqui exigiria a caneta que o laço da sessão segura.
            if motivo == crate::tela::FimDaTela::AlemDoQueOHospedeiroCarrega {
                let _ = avisos
                    .send(ServerMessage::Alert {
                        severity: AlertSeverity::Warning,
                        reason: AlertReason::ScreenShareOverHostUplink,
                        operator_text: None,
                    })
                    .await;
            }
            // `FluxoMalformado` não ganha frase, e é decisão: ele quer dizer
            // que o cliente escreveu um enquadramento que não é o do §3.6, o
            // que nenhuma frase de interface conserta e nenhum usuário
            // provocou. Fica no log, onde quem escreve cliente vai procurar.
            return Ok(());
        }
        match fluxo.read(&mut buffer).await? {
            Some(lidos) => {
                let bytes = buffer.get(..lidos).unwrap_or_default().to_vec();
                // `send` e não `try_send`: encher a fila da sala de voz tem de virar
                // contrapressão no QUIC de quem compartilha, que é onde ela
                // conserta alguma coisa. Descartar aqui deslocaria o
                // enquadramento de todos os espectadores de uma vez.
                sala.send(VoiceRoomCommand::TelaBytes {
                    from: person,
                    bytes,
                })
                .await?;
            }
            None => break,
        }
    }
    // O fim limpo. Quem parou de propósito também manda `StopScreenShare` pelo
    // controle, e é ele que anuncia; quem sumiu é recolhido pelo fim da sessão.
    // Aqui só o encaminhamento morre, que é o que o §5.1 pôs sob esta função.
    let _ = sala
        .send(VoiceRoomCommand::TelaFechou { from: person })
        .await;
    Ok(())
}

/// Encerra e anuncia o que este pessoa estivesse transmitindo, onde estivesse.
///
/// Chamado em todo lugar onde o connection sai de uma sala de voz — sair, ser movido, ser
/// expulso, ou a conexão acabar em qualquer `?` do meio do laço. Uma
/// transmissão que sobrevivesse à saída de quem a manda ficaria desenhada para
/// sempre na sala, prometendo um fluxo que não tem mais de onde vir: é o mesmo
/// defeito do pessoa fantasma que `serve` conserta logo acima, com a diferença
/// de que aqui a promessa é de imagem em movimento.
async fn encerrar_telas_de(server: &Server, person: PersonId) {
    for (voice_room, screen) in server.telas.lock().await.encerrar_de(person) {
        let _ = server
            .events
            .send(Event::ScreenShareStopped { voice_room, screen });
    }
}

/// Tells a client the server said no, and why.
async fn recusar(send: &mut quinn::SendStream, person: PersonId, verbo: &str) -> Result<()> {
    tracing::warn!(%person, verbo, "refused: the server said no");
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
/// `VoiceRoomEntryRefused` is the nearest enumerated reason for "that room is not
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
            reason: AlertReason::VoiceRoomEntryRefused,
            operator_text: None,
        },
    )
    .await
}

/// How long after the last datagram a person still counts as speaking.
///
/// One telemetry tick is too coarse and one frame is too twitchy: at 20 ms per
/// frame, a quarter of a second is about a dozen frames of grace, which rides
/// out a hiccup without leaving the mark lit through a pause.
const SPEAKING_TAIL: Duration = Duration::from_millis(250);

/// Everything a `PersonState` broadcast carries about one person.
///
/// Grouped rather than passed as six arguments because every one of them is
/// overwritten wholesale on the receiving side: a client folds the entire
/// struct in, so a field left at a default is not left alone — it replaces
/// whatever that client knew.
struct AnnouncedState {
    muted: bool,
    total_isolation: bool,
    speaking: bool,
    presence: Presence,
    signal: u8,
}

/// Tells everybody what this person just announced about themselves.
fn announce(server: &Server, session: &Session, state: &AnnouncedState) {
    let _ = server.events.send(Event::PersonState(PersonState {
        person: session.person,
        muted: state.muted,
        total_isolation: state.total_isolation,
        speaking: state.speaking,
        presence: state.presence,
        signal: state.signal,
    }));
}

/// Decides whether an event concerns this connection, and what to send.
fn translate(
    event: &Event,
    channels: &[ChannelId],
    self_person: PersonId,
) -> Option<ServerMessage> {
    match event {
        // A subida medida andou. Todo mundo recebe, porque o `min` do §5.1 é
        // calculado em cada cliente e esta é uma das três pernas dele.
        Event::HostUplink { bps } => Some(ServerMessage::HostUplink { bps: *bps }),

        // E esta, ao contrário da de cima, é de uma pessoa só.
        //
        // O filtro está aqui e não em quem emite porque a `VoiceRoom` que mede
        // não conhece sessão nenhuma — o barramento é o único caminho de uma
        // para outra. Difundir contaria a toda a sala a qualidade da rede de
        // cada um, que é o oposto da promessa do ADR 0036.
        Event::UplinkLoss { person, fraction } if *person == self_person => {
            Some(ServerMessage::UplinkLoss {
                fraction: *fraction,
            })
        }
        Event::UplinkLoss { .. } => None,

        // Nunca por aqui. A audiência deste depende de `AdministerServer`, e
        // conferir permissão é uma pergunta ao banco — `translate` é síncrona e
        // não tem como fazê-la. Quem o entrega é o braço de eventos do laço, que
        // pode esperar. Ver o ADR 0038.
        Event::VoiceRoomOverHostUplink { .. } => None,
        Event::MessagePosted(message) => {
            channels
                .contains(&message.channel)
                .then(|| ServerMessage::MessageReceived {
                    channel: message.channel,
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
        Event::MessageEdited { channel, id, body } => {
            channels
                .contains(channel)
                .then(|| ServerMessage::MessageEdited {
                    channel: *channel,
                    id: *id,
                    body: body.clone(),
                })
        }
        Event::MessageRemoved { channel, id } => {
            channels
                .contains(channel)
                .then_some(ServerMessage::MessageRemoved {
                    channel: *channel,
                    id: *id,
                })
        }
        // Every voice room, and not only the one this connection is sitting in.
        //
        // The filter that used to be here — `voice_room == Some(*joined)` — is what
        // made four rooms out of five permanently empty on screen: a client was
        // told about arrivals in its own voice room and about nothing else, so the
        // occupants the v3 layout draws under every other voice room were data it had
        // never been sent. Reported from a real session as the voice_rooms showing
        // empty when they were not.
        //
        // Weighed against a count on `VoiceRoomInfo` and against a snapshot the
        // client asks for. The count is cheaper and loses the names the screen
        // is built around; the snapshot keeps the names and goes stale the
        // instant it lands, which is the same bug moving more slowly. This is
        // the only one of the three that is still true a second later.
        //
        // What it costs is that everybody learns where everybody is. That was
        // already the case: `Event::PersonState` — speaking, Sync Ratio, both
        // mutes — has always gone to every connection unfiltered, so a client
        // was already being told about people it had no seat for, and drew them
        // as ghosts with no room. `specs/04-servidor-seele.md` sizes a server at
        // fifty people and five voice_rooms, and the voice room list itself is not filtered
        // per person, so this reveals nothing that walking into the room would
        // not. ADR 0022 opens a server to the internet; what changes there is who
        // may hold an account, which is `crate::admissao`'s question, not this
        // one.
        //
        // Not echoed to the person who caused it: they already know.
        Event::PersonJoined {
            voice_room: joined,
            profile,
            ssrc,
        } => (profile.id != self_person).then(|| ServerMessage::PersonJoined {
            voice_room: *joined,
            profile: profile.clone(),
            ssrc: *ssrc,
        }),
        Event::PersonLeft {
            voice_room: left,
            person,
        } => (*person != self_person).then_some(ServerMessage::PersonLeft {
            voice_room: *left,
            person: *person,
        }),
        // Estes dois **também** não voltam para quem os causou: quem acabou de
        // conectar sabe que conectou, e quem saiu não está mais aqui para ler.
        Event::PersonPresent { quem } => {
            (quem.person != self_person).then(|| ServerMessage::PersonPresent {
                profile: PersonProfile {
                    id: quem.person,
                    nickname: quem.nickname.clone(),
                    roles: Vec::new(),
                },
                ssrc: quem.ssrc,
            })
        }
        Event::PersonGone { person } => {
            (*person != self_person).then_some(ServerMessage::PersonGone { person: *person })
        }
        // Echoed back to the person it describes, unlike the two above.
        //
        // "They already know" is true of joining and leaving — the client asked
        // for both — and false of everything in here. The Sync Ratio is measured
        // by the *server*, from QUIC's own view of the path, and this broadcast
        // is the only thing that carries it; `speaking` is decided by whether
        // audio is actually arriving, which is likewise the server's to know.
        // Filtering self out left every client's own roster row frozen at
        // `Person::new`'s defaults for the life of the session: nought per cent,
        // which by the three bands reads as critical, beside a telemetry bar
        // measuring the same connection at a hundred.
        //
        // The two flags it also carries are the person's own, so echoing them
        // costs nothing: the server is repeating what this client just said, and
        // a client that folds them back in lands on the value it sent.
        Event::PersonState(state) => Some(ServerMessage::PersonState(*state)),

        // Unfiltered, and to the person who caused it as well.
        //
        // Unlike a voice room arrival, a new room is not something a client can infer
        // from having asked: the identifier is the server's to assign, and the
        // maker needs it as much as everybody else does. Filtering self out here
        // would leave whoever made the room as the one person who cannot see it.
        Event::VoiceRoomCreated { voice_room } => Some(ServerMessage::VoiceRoomCreated {
            voice_room: voice_room.clone(),
        }),
        Event::ChannelCreated { channel } => Some(ServerMessage::ChannelCreated {
            channel: channel.clone(),
        }),
        Event::VoiceRoomRenamed { voice_room, name } => Some(ServerMessage::VoiceRoomRenamed {
            voice_room: *voice_room,
            name: name.clone(),
        }),
        Event::ChannelRenamed { channel, name } => Some(ServerMessage::ChannelRenamed {
            channel: *channel,
            name: name.clone(),
        }),

        // Unfiltered, and to the person who asked as well, like the four above.
        // A header is drawn in every open window: filtering the asker out would
        // leave the one person who renamed the server reading the old name.
        Event::ServerRenamed { name } => Some(ServerMessage::ServerRenamed { name: name.clone() }),
        Event::ServerIconChanged { icon } => {
            Some(ServerMessage::ServerIconChanged { icon: icon.clone() })
        }

        // Acted on by the connection they name, in the loop, and carrying
        // nothing for anybody else. A move is visible to everybody as the
        // `PersonLeft` and `PersonJoined` that `assentar` sends, and a session
        // ending is visible as the `PersonLeft` that `serve` sends when the
        // connection is gone — so there is nothing to translate here, and
        // inventing something would be a second way to say what those already
        // say.
        Event::SessionEnded { .. } | Event::PersonMoved { .. } => None,

        // Unfiltered, like the four announcements above and for the same
        // reason: a room that goes on being drawn until the next handshake is a
        // room people keep trying to walk into. The person who asked included —
        // they need to stop drawing it as much as anybody.
        //
        // The connections that were *inside* the voice room, or had the Channel open,
        // never get here: the loop answers them itself and `continue`s, because
        // they have a connection to pull and a sentence to be told and this function
        // knows about neither.
        Event::VoiceRoomDeleted { voice_room } => Some(ServerMessage::VoiceRoomDeleted {
            voice_room: *voice_room,
        }),
        Event::ChannelDeleted { channel } => {
            Some(ServerMessage::ChannelDeleted { channel: *channel })
        }

        // ---- compartilhamento de tela ----
        //
        // Sem filtro, e a quem compartilha também. É o mesmo caso de
        // `VoiceRoomCreated` e por uma razão mais forte: o `ScreenId` é do servidor
        // para atribuir, e **quem compartilha precisa dele** antes de conseguir
        // abrir um fluxo. Filtrar a si mesmo aqui deixaria quem apertou o botão
        // como a única pessoa incapaz de transmitir.
        //
        // A todo mundo e não só à sala de voz: é a mesma escolha que `PersonJoined`
        // fez ao deixar de filtrar por sala, e pelo mesmo motivo — a v3 desenha
        // todos as salas de voz, e uma sala que não diz que está transmitindo é uma
        // sala em que ninguém sabe que há o que assistir. Não revela nada que
        // entrar na sala já não revelasse.
        Event::ScreenShareStarted {
            voice_room,
            person,
            screen,
        } => Some(ServerMessage::ScreenShareStarted {
            voice_room: *voice_room,
            person: *person,
            screen: *screen,
        }),
        Event::ScreenShareStopped { voice_room, screen } => {
            Some(ServerMessage::ScreenShareStopped {
                voice_room: *voice_room,
                screen: *screen,
            })
        }

        // A todo mundo, como o `ScreenShareStarted` de que ele é a continuação.
        //
        // Quem compartilha precisa de N para dividir o caminho do anfitrião
        // (§5.1) — é a razão de a mensagem existir. Quem assiste precisa dele
        // para a frase que o §5.1 desenha, «720p · 6 pessoas assistindo»: a
        // resolução muda porque N mudou, e uma tela que cai de degrau sem dizer
        // por quê é o produto sabendo algo que quem está na frente dele não
        // sabe. O `voice_room` não viaja porque `ScreenId` já é único neste servidor e
        // quem recebeu o `ScreenShareStarted` já sabe de que sala ele é.
        Event::ScreenViewers {
            voice_room: _,
            screen,
            quantos,
        } => Some(ServerMessage::ScreenViewers {
            tela: *screen,
            quantos: *quantos,
        }),

        // A exceção: este só vai para quem está compartilhando. Um quadro-chave
        // custa 65 KiB em 1080p — 446 ms do orçamento inteiro (§3.3) —, e
        // mandar o pedido para a sala inteira faria toda máquina que assiste
        // acordar para um pedido que não é dela.
        Event::KeyFrameRequested {
            screen,
            person,
            sharer,
        } => (*sharer == self_person).then_some(ServerMessage::KeyFrameRequested {
            screen: *screen,
            person: *person,
        }),
    }
}

#[cfg(test)]
mod plano_de_midia {
    /// Toda mudança de sala tem de chegar ao plano de mídia.
    ///
    /// # Por que ler o próprio código, e não afirmar sobre um valor
    ///
    /// Porque a propriedade é invisível ao compilador e a qualquer asserção
    /// sobre uma sessão só. `current_voice_room` decide o plano de controle e
    /// `Midia::sala` decide para onde a voz vai; nada no tipo dos dois liga um
    /// ao outro. Uma sétima atribuição escrita amanhã sem a ponte compila,
    /// passa em todo teste que existe, e produz um defeito de uma forma cruel:
    /// a pessoa entra na sala, a lista mostra que ela entrou, o texto funciona,
    /// e a voz dela vai para a sala anterior — ou para lugar nenhum.
    ///
    /// A regra é estreita de propósito. Não se cobra que a ponte seja a linha
    /// seguinte por estilo: cobra-se porque ela **é** a única coisa que faz a
    /// atribuição valer, e separá-la da atribuição é o começo de esquecê-la.
    #[test]
    fn toda_troca_de_sala_atravessa_para_o_plano_de_midia() {
        let fonte = include_str!("session.rs");

        let mut linhas = fonte.lines().enumerate().peekable();
        let mut atribuicoes = 0_usize;
        let mut orfas = Vec::new();

        while let Some((numero, linha)) = linhas.next() {
            let corte = linha.trim();
            // A declaração não é uma troca, e o `Midia::entrou` que a segue não
            // existiria: a tarefa de mídia nasce logo depois dela.
            let e_troca = corte.starts_with("current_voice_room = ") && corte.ends_with(';');
            let e_take = corte == "if let Some(id) = current_voice_room.take() {";
            if !e_troca && !e_take {
                continue;
            }
            atribuicoes += 1;
            let seguinte = linhas.peek().map(|(_, texto)| texto.trim()).unwrap_or("");
            if seguinte != "midia.entrou(current_voice_room);" {
                orfas.push(numero + 1);
            }
        }

        assert!(
            orfas.is_empty(),
            "estas linhas movem `current_voice_room` e não avisam o plano de mídia: {orfas:?}\n\
             A voz é encaminhada a partir de `Midia::sala`, e não deste valor. Sem a ponte, \
             quem trocar de sala continua falando para a sala de antes — e a lista de pessoas \
             mostra a mudança, então nada na tela contradiz o defeito."
        );
        // Se as trocas sumirem, este guarda passa a não guardar nada e ninguém
        // percebe. O número não precisa estar certo; precisa não ser zero.
        assert!(
            atribuicoes >= 6,
            "só {atribuicoes} trocas de sala encontradas — o guarda perdeu o alvo"
        );
    }
}
