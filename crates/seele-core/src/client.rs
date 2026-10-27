//! The headless client: connect, handshake, hold a session.
//!
//! `specs/01-arquitetura.md` makes this crate the one place session, protocol,
//! audio and state logic live: "one core, three shells". Nothing here formats
//! text, chooses a colour or knows what a terminal is.
//!
//! # PADRÃO: LARANJA and CONEXÃO SEGURA
//!
//! `specs/02-protocolo.md`:
//!
//! > Before `Sessao`, the client is in **PADRÃO: LARANJA** — connected, not
//! > verified. The interface must reflect that state, not hide it.
//!
//! [`Trust`] is that state, as plain data. A shell decides what orange looks
//! like; this decides when it is true.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, ConsentimentoDePar, Lido, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId, MessageId, PersonId, ScreenId, VoiceRoomId};
use seele_proto::transport::HANDSHAKE_TIMEOUT;

use crate::frame;
use crate::tofu::{PinDecision, PinStore, TofuVerifier};

/// How far a connection has got.
///
/// `specs/07-estetica.md` names these, and `specs/06-clientes-gui.md`
/// requires both to be visible states rather than a spinner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkTrust {
    /// Not connected.
    Offline,
    /// Connected, not verified. A conferência de identidade ainda não passou.
    Unverified,
    /// Verified. A identidade do servidor confere.
    Verified,
}

/// Quanto se espera para ter certeza de que a recusa dos MODs chegou.
///
/// Curto porque é tempo gasto por quem já decidiu não entrar, e existe porque
/// uma recusa não confirmada é indistinguível de uma conexão que caiu — ver
/// onde ela é usada.
const ESPERA_DA_RECUSA: std::time::Duration = std::time::Duration::from_secs(1);

/// Why a connection did not happen.
///
/// Enumerated rather than an opaque error, because `specs/06-clientes-gui.md`
/// requires errors crossing to a shell to be enums a shell can localise. The
/// alternative — matching on the text of somebody else's error message — breaks
/// silently the day `quinn` rewords a sentence, and breaks in the direction of
/// reporting the wrong cause rather than none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// The local WebRTC endpoint (the UDP socket under it) could not be opened.
    ///
    /// A blocked UDP socket or an exhausted port range. Nothing to do with the
    /// server.
    LocalEndpoint,
    /// Nothing answered, or the network refused to carry it.
    Unreachable,
    /// **O ponto de encontro levou o chamado, quem hospeda atendeu, e o
    /// caminho não abriu** (degrau 4, D7 da 0.17).
    ///
    /// Não é [`Self::SemResposta`]: ali ninguém respondeu nada. Aqui o ponto
    /// disse que a pessoa ou o servidor está lá e respondeu ao `CHAMA`, e as
    /// checagens ICE para o endereço que ele devolveu não voltaram — o caso de
    /// dois NATs simétricos, ou de um que não deixa entrar o que não saiu. Sem
    /// um relay TURN configurado por quem usa, não há o que insistir.
    NatNaoAbriu,
    /// **O link é de um Servidor 1** (a linha da 0.16 e anteriores): `v=` do
    /// link, ou a magia do ENC/2 no ponto. A 0.17 fala WebRTC e não conversa
    /// com ele; quem abre essa linha é o launcher.
    LinhaLegada,
    /// DTLS refused the connection for a reason other than the pin.
    ///
    /// Nem a impressão esperada, nem a chave que o servidor não provou: essas
    /// têm nome próprio ([`Self::InviteMismatch`] e [`Self::ChaveNaoProvada`]).
    /// O que sobra aqui é o TLS que caiu por outro alerta, ou sem decisão do
    /// verificador.
    ///
    /// Desde a 0.17 é o DTLS (ou o SCTP por cima dele) que caiu depois de o
    /// ICE achar caminho, sem ser pela impressão: o enlace devolve
    /// `FalhaAoDiscar::Protocolo`, e o `seele.log` leva o texto dele.
    TlsRefused,
    /// Quem atendeu mostrou um certificado que o verificador aceitou, e não
    /// provou ter a chave dele.
    ///
    /// **Não sai mais do núcleo desde a 0.17**, e isto está escrito para não
    /// parecer guarda. No QUIC, o `rustls` recusava a assinatura do
    /// `CertificateVerify` com o alerta 51, e dava para separá-la de outra
    /// recusa. No DTLS do enlace a mesma recusa derruba o aperto de mão por
    /// dentro, sem motivo próprio, e chega como [`Self::TlsRefused`]. O
    /// `Hello` continua não saindo: quem não prova a chave não fecha o DTLS.
    /// A variante fica porque a FFI e a tela ainda a nomeiam.
    ChaveNaoProvada,
    /// The pinned key is not the key offered. ADR 0003.
    PinChanged {
        /// What was pinned before.
        pinned: String,
        /// What was offered now.
        offered: String,
    },
    /// The handshake did not finish inside the budget in `specs/02-protocolo.md`.
    ///
    /// **Isto é o servidor demorando a responder, e não a rede sumindo.** O
    /// segundo caso tem nome próprio desde 05/09/2026 — ver [`Self::SemResposta`]
    /// —, porque os dois vestiam esta variante e mandavam quem lia procurar em
    /// lugares opostos.
    HandshakeTimeout,
    /// A conexão morreu antes de qualquer resposta chegar.
    ///
    /// # Por que ela não é [`Self::HandshakeTimeout`]
    ///
    /// Os dois eram a mesma variante, e a frase dela é «tempo esgotado na
    /// sincronização inicial». Ela descreve bem um servidor que recebeu o
    /// `Hello` e demorou a responder — Argon2 numa máquina carregada, um banco
    /// lento —, e descreve **mal** o caso mais comum: os pacotes saíram e nada
    /// voltou.
    ///
    /// Desde a 0.17 é o ICE sem resposta: nenhum candidato devolveu a checagem
    /// STUN (`FalhaAoDiscar::SemResposta`), e chega antes de existir aperto de
    /// mão nenhum. No QUIC era o `ConnectionError::TimedOut`. Traduzi-lo como
    /// «sincronização» manda quem lê investigar protocolo, versão e servidor —
    /// quando o que aconteceu foi um datagrama que não chegou ao outro lado.
    ///
    /// Foi assim que apareceu: «o Mac só dá tempo esgotado na sincronização.
    /// Como isso, se os PCs estão na mesma LAN e na mesma versão?». A pergunta é
    /// a prova de que a frase apontou para o lugar errado — as duas coisas que
    /// ela mandou conferir estavam certas.
    SemResposta,
    /// The server ended the session during the handshake, and said why.
    Refused {
        /// The enumerated reason.
        reason: seele_proto::control::DisconnectReason,
    },
    /// The server said something that is not a handshake.
    ProtocolViolation,
    /// **Quem atendeu fala um número de protocolo que esta build não fala**, e
    /// disse qual no byte 0 da primeira resposta.
    ///
    /// O byte 0 de todo quadro é o número do protocolo, e é por ele que se lê a
    /// linha de quem respondeu (ADR 0058): um servidor 0.15 responde carimbado
    /// 8, uma 1.x mais nova que não fala o 9 responde no número dela. Até a
    /// 0.15, isto caía em [`Self::Unreachable`] — «não foi possível alcançar o
    /// servidor» —, e a pessoa ia procurar defeito na rede de um servidor que
    /// tinha respondido. Quem resolve é abrir o app da versão do servidor, ou
    /// atualizar este; insistir dá o mesmo número.
    OutraLinha {
        /// O número do byte 0 da resposta.
        numero: u8,
    },
    /// A impressão esperada (a do link, ou a da lista) prometia outra
    /// identidade, e nenhum pino desempata.
    ///
    /// De propósito distinta de [`ConnectError::PinChanged`]: lá a chave de um
    /// servidor conhecido mudou, que é o alarme do ADR 0003. Aqui nenhum pino
    /// prova o servidor — nada estava fixado, ou é um candidato que ninguém
    /// escolheu ou um alvo de escopo local (um endereço de LAN, o mesmo de uma
    /// casa para outra), onde um pino que confere pode ser de outro servidor
    /// ([`crate::tofu::TofuVerifier::decide`]) —, e quem discorda é quem
    /// prometeu a impressão, o link ou a lista.
    ///
    /// **Recusada dentro do TLS, antes do `Hello`**: o verificador a decide no
    /// aperto de mão (`crate::tofu::PinDecision::InviteRefused`), e o `Hello`
    /// nem chega a sair. O convite ou a senha, o apelido e a chave de
    /// identidade não chegam a quem atendeu com a chave errada; para ele sai só
    /// o `ClientHello` do próprio TLS. O escopo é esse candidato: numa corrida,
    /// outro candidato pode ter entregado o `Hello` ao servidor verdadeiro. A
    /// conferência depois do aperto de mão, em `crate::enlace`, continua como
    /// segunda linha.
    InviteMismatch {
        /// O que a impressão esperada prometia: a do link, ou a da lista.
        expected: String,
        /// What the server offered.
        offered: String,
    },

    /// O servidor exige MODs que esta máquina ainda não aceitou. ADR 0045.
    ///
    /// # Por que ela carrega a lista, e por que isso não é um erro falante
    ///
    /// As outras variantes descrevem o que deu errado. Esta descreve **uma
    /// pergunta que ninguém respondeu ainda**, e a resposta depende de dados
    /// que só chegaram agora: quem são os MODs, de que repositório vieram, o
    /// que declaram alcançar, e quais deles rodam na máquina de quem hospeda.
    ///
    /// Sem eles aqui, a casca teria de reconectar só para poder perguntar — ou
    /// pior, perguntaria sem ter o que mostrar. Nada aqui é texto de erro: são
    /// campos que a casca desenha e traduz (ADR 0012), como todo o resto.
    ///
    /// O caminho que ela abre é um só: a tela mostra, a pessoa aceita, a casca
    /// guarda a identidade em `crate::aceites` e conecta de novo.
    ModsNaoAceitos {
        /// O que o servidor exige, como ele anunciou.
        mods: Vec<seele_proto::mods::ModAnunciado>,
        /// A identidade do conjunto, que é o que se guarda ao aceitar.
        conjunto: String,
    },

    /// Quem atendeu não é a sala do código para o qual se ligou (ADR 0060).
    ///
    /// **Recusada dentro do TLS, antes do `Hello`**, pelo
    /// [`crate::confere_codigo::ConfereCodigo`]: nada do que quem liga diria
    /// chega a quem atendeu. O motivo diz qual dos três casos foi: não é uma
    /// sala, a chave é de outro código, ou a chave não é a que ficou pinada na
    /// primeira ligação.
    CodigoNaoConfere(crate::confere_codigo::FalhaDoCodigo),

    /// Quem liga pelo código não achou onde a sala mora (tarefa 6c do Plano 5).
    ///
    /// Sai **antes da corrida**, de [`crate::chegada::Chegada`]: o quarto é
    /// quem dá os candidatos de uma ligação, e sem ele não há endereço a tentar.
    /// O motivo diz qual das frases da spec de 23/09 cabe: o ponto calado, a
    /// pessoa que não está disponível, o ponto que recusou.
    NaoLocalizou(crate::encontro::FalhaAoLocalizar),

    /// **Quem liga desistiu antes de entrar** (tarefa 16 da F7, o
    /// `cancelar_ligacao`). Não é falha de ninguém, e não há o que explicar: foi
    /// a pessoa quem cancelou.
    ///
    /// Sai de [`crate::chegada::Chegada::chegar_ate`], que larga a chegada em
    /// voo inteira: o QUIC de cada candidato fecha, e a sala que tocava vê
    /// `ParouDeTocar(Cancelada)`. A trilha acaba em
    /// [`crate::chegada::Etapa::Cancelada`].
    Cancelada,
}

impl std::fmt::Display for ConnectError {
    /// For logs, never for a user. A shell writes its own sentence.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ConnectError {}

/// Everything the server told us on the way in: o `SessionInfo` mora no
/// `seele-sessao`, com o `Room` que o dobra, para o navegador montar o mesmo
/// retrato (Tarefa 2.2 da F4).
pub use seele_sessao::SessionInfo;

/// The media half of a connection, usable independently of the control stream.
///
/// `specs/02-protocolo.md` puts voice on datagrams and control on a stream
/// precisely so neither waits for the other. Handing them out as separate
/// objects makes that structural: a caller can await both at once without one
/// borrow blocking the other, which is exactly the shape a shell needs.
#[derive(Clone)]
pub struct MediaChannel {
    connection: crate::transporte::Conexao,
}

impl MediaChannel {
    /// Sends one media datagram, on the `voz` channel of the link (unordered,
    /// no retransmission: D4 da 0.17).
    ///
    /// The `ssrc` in the header must be the one in [`SessionInfo::ssrc`]; the
    /// server refuses anything else (gap G2).
    ///
    /// # Errors
    ///
    /// Fails if the datagram is larger than `seele_enlace::MAX_VOZ` or the
    /// connection is gone.
    pub fn send(&self, datagram: Vec<u8>) -> Result<()> {
        self.connection
            .send_datagram(datagram)
            .map_err(|recusa| anyhow::anyhow!("a voz não saiu: {recusa:?}"))
    }

    /// Waits for the next media datagram.
    ///
    /// # Errors
    ///
    /// Fails when the connection closes.
    pub async fn next(&self) -> Result<Vec<u8>> {
        self.connection
            .read_datagram()
            .await
            .map_err(|motivo| anyhow::anyhow!("a conexão acabou: {motivo:?}"))
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
///
/// **Desde a 0.17, sempre zero, e não por nada ter sido segurado.** O SCTP dos
/// DataChannels não tem janela por fluxo nem conta bloqueio; a contrapressão
/// do enlace é o crédito de saída por canal (`seele_enlace::CREDITO_DE_SAIDA`),
/// que faz `write_all` esperar sem deixar rastro aqui. Ler um zero desta
/// estrutura numa sessão 0.17 é «não medido», e não «nada segurou».
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
    /// Every byte went out and the server took the stream to its end.
    ///
    /// What follows is either the message appearing on the Channel — which is how
    /// the sender learns it worked, matched by `client_message_id` — or an
    /// `AttachmentRefused` if what arrived did not hold together.
    Delivered {
        /// How many bytes went.
        bytes: u64,
    },
    /// The server stopped the stream: it refused **before** taking the bytes.
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
    /// Which Channel the message goes to.
    pub channel: ChannelId,
    /// The idempotency key of the message. Gap G9, and here it is also what
    /// makes a retry after a fall safe.
    pub client_message_id: ClientMessageId,
    /// What the sender wrote beside the file. May be empty.
    pub body: &'a str,
    /// What it replies to.
    pub replies_to: Option<MessageId>,
    /// Where the file is on this machine.
    ///
    /// Never sent. The server stores the blob under the hash of its content, so
    /// nothing about this path crosses the wire.
    pub path: &'a std::path::Path,
    /// The name to give it on the other side.
    pub file_name: &'a str,
    /// The type to claim it is. A claim, and treated as one.
    pub declared_type: &'a str,
}

/// Tells a server that refused apart from a link that fell.
///
/// `Cortado` is the link's `corte` (envelope 0x02, D4 da 0.17), what QUIC's
/// `STOP_SENDING` was: somebody at the other end decided, on purpose, not to
/// take these bytes. Anything else is the connection going away, which nobody
/// decided.
fn interpret(error: &crate::transporte::FalhaDoCanal, sent: u64) -> Sent {
    if matches!(error, crate::transporte::FalhaDoCanal::Cortado(_)) {
        Sent::Stopped { bytes: sent }
    } else {
        Sent::Interrupted { bytes: sent }
    }
}

/// The same reading, for a header that could not be written.
fn interpret_any(error: &anyhow::Error, sent: u64) -> Sent {
    error
        .downcast_ref::<crate::transporte::FalhaDoCanal>()
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
    connection: crate::transporte::Conexao,
    send: crate::transporte::Envio,
    /// O número que o servidor carimbou na primeira resposta, e com que toda
    /// escrita do fluxo de controle sai (adendo de 08/10/2026, §4). Na 1.0 é
    /// sempre o 9; numa 1.x mais nova diante de um servidor mais velho, é o
    /// dele, e é o que deixa os dois conversarem sem quebrar o fio.
    carimbo: u8,
    /// Mensagens já decodificadas, vindas da tarefa que é dona do fluxo.
    ///
    /// O fluxo de leitura não fica aqui de propósito — ver [`Client::next_event`].
    inbox: tokio::sync::mpsc::UnboundedReceiver<ServerMessage>,
    session: SessionInfo,
    pin: PinDecision,
    link_state: LinkTrust,
    /// A ping that has been sent and not yet answered.
    pending_ping: Option<(u64, std::time::Instant)>,
    /// The most recent round trip.
    last_rtt: Option<std::time::Duration>,
    /// O `Ping` da sonda que ainda não voltou: o carimbo e quando saiu.
    ping_da_sonda: Option<(u64, std::time::Instant)>,
    /// Quantos `Ping` da sonda já saíram, para o carimbo de cada um.
    pings_da_sonda: u64,
    /// Quando saiu o último `Ping` da sonda, para o intervalo entre eles.
    ultimo_ping_da_sonda: Option<std::time::Instant>,
    /// A última ida e volta da sonda, e quantas já voltaram.
    ida_e_volta_da_sonda: Option<(std::time::Duration, u64)>,
    /// Fluxos de tela, já aceitos e classificados pelo roteador.
    ///
    /// Atrás de um `Mutex` embora só um lugar o leia: quem o lê é um braço de
    /// um `select!` que, no outro braço, já pegou o `Client` emprestado mutável
    /// para ler eventos. Dois empréstimos mutáveis no mesmo `select!` não
    /// compilam, e a fila é a metade que não precisa deles.
    telas: FilaDeTelas,
    /// Fluxos de anexo, idem.
    ///
    /// Atrás de um `Mutex` e não do `&mut self` como as telas: quem recebe um
    /// anexo é uma tarefa solta, criada por comando, e ela não tem o `Client`.
    anexos: FilaDeAnexos,
    /// A ponta WebRTC que esta conexão usa para falar com o servidor.
    ///
    /// **Guardada para o caminho entre pares a reaproveitar** — `§3.1` da
    /// spec de 05/09 é explícito: "Não é escuta nova, socket novo nem porta
    /// nova [...] aquela porta já tem mapeamento de NAT vivo, mantido pelo
    /// `keep_alive_interval` da conexão com o servidor". Uma ponta nova teria
    /// endereço público desconhecido do servidor — ele só vê a origem desta
    /// conexão de controle — e furar NAT numa porta que ele nunca apontou não
    /// tem como funcionar. Ver [`Self::ponta`].
    ///
    /// **Uma por sessão** (E3 da 0.17): duas sessões não dividem ponta, então
    /// fechar uma não mexe no socket da outra.
    ponta: crate::transporte::PontaDaSessao,
}

/// A fila de onde sai a próxima transmissão de tela alheia.
pub type FilaDeTelas = std::sync::Arc<
    tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<crate::transporte::Recebe>>,
>;

/// A fila de onde as tarefas de anexo tiram o fluxo que lhes cabe.
type FilaDeAnexos = std::sync::Arc<
    tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<crate::transporte::Recebe>>,
>;

/// O código com que quem liga fecha a conexão que recusou antes do `Hello`.
///
/// Um só para toda recusa, e o motivo é uma frase neutra ([`RECUSA_NEUTRA`]):
/// quem atendeu não aprende se esta máquina tinha um link, um pino ou um
/// código, que é o que um impostor quer saber para forjar (o mesmo cuidado que
/// o `crate::tofu` tinha com a frase do TLS).
const CODIGO_DA_RECUSA: u32 = 0x5345_0010;

/// A frase da recusa antes do `Hello`.
const RECUSA_NEUTRA: &[u8] = b"certificado nao aceito";

impl Client {
    /// Connects, runs the handshake, and returns once PATTERN: BLUE is reached.
    ///
    /// `signing_key` is this client's identity (ADR 0004). ADR 0017 keeps it on
    /// disk, because PERSISTENCE binds a nickname to the identity that claimed it.
    ///
    /// `join_secret` is the invite token or the server password, when the server
    /// asks for one. `None` for an open server, which is the default.
    ///
    /// `server_name` is what TLS is told; `pin_key` is what the pin is filed
    /// under. They are separate on purpose — the TOFU verifier compares
    /// fingerprints and never looks at the certificate's names, so the TLS name
    /// is a label and the pin key is the policy. Passing the same value for
    /// both is fine for a hostname and **wrong for an IP address**, where the
    /// TLS name has to be something the certificate carries and the pin key has
    /// to be the address the person typed.
    ///
    /// # Errors
    ///
    /// A [`ConnectError`] variant, never a message. Every caller has to be able
    /// to say which of these happened in its own words.
    #[expect(
        clippy::too_many_arguments,
        reason = "o oitavo é o aceite de MODs do ADR 0045, e ele é do mesmo par \
                  que `pins`: o que esta máquina já decidiu sobre este servidor. \
                  Agrupar os oito num tipo é a saída certa e não é desta entrega \
                  — ela tocaria todo chamador do produto por uma razão que não é \
                  a desta tarefa, e o `connect_por` logo abaixo já carrega a \
                  mesma dispensa desde antes"
    )]
    pub async fn connect(
        server: SocketAddr,
        server_name: &str,
        pin_key: &str,
        nickname: &str,
        signing_key: &SigningKey,
        pins: Arc<dyn PinStore>,
        join_secret: Option<&str>,
        aceito: Option<&str>,
    ) -> Result<Self, ConnectError> {
        // Sem SNI desde a 0.17: o DTLS não leva nome de servidor, e o nome que
        // o TLS recebia era só um rótulo (o pin é por `pin_key`).
        let _ = server_name;
        let ponta = crate::transporte::abrir_ponta(None)?;
        let conexao = discar(ponta.ponta(), vec![server], None, HANDSHAKE_TIMEOUT).await?;
        Self::connect_sobre(
            ponta,
            conexao,
            pin_key,
            // Este caminho não confere impressão nenhuma: quem tem uma entra
            // por `crate::enlace::Enlace`, que a leva até antes do `Hello`.
            None,
            // Sem esperada, se o pino prova o servidor não pesa: o pin decide
            // como sempre decidiu.
            true,
            nickname,
            signing_key,
            pins,
            join_secret,
            aceito,
            // Quem entra por aqui não corre candidatos: ninguém espera o toque.
            None,
        )
        .await
    }

    /// O aperto de mão do protocolo sobre uma conexão que o enlace já abriu.
    ///
    /// Quem disca é [`discar`] (ou a corrida de [`crate::enlace`], que disca
    /// todos os candidatos de um anfitrião num agente ICE só); este passo é o
    /// que vem depois do DTLS e **antes do `Hello`**: conferir quem atendeu.
    ///
    /// `esperada` é conferida aqui, contra o certificado DTLS que o enlace viu
    /// (`Conexao::impressao_dtls_do_par`), antes de o `ctl` levar qualquer
    /// coisa:
    ///
    /// - um certificado ([`crate::enlace::Esperada::Certificado`]) ou nada: o
    ///   TOFU de [`crate::tofu::TofuVerifier`], com as mesmas regras de pin e
    ///   de impressão esperada de antes. Um primeiro contato que não confere
    ///   volta como [`ConnectError::InviteMismatch`]; uma chave fixada que
    ///   mudou, como [`ConnectError::PinChanged`];
    /// - um código ([`crate::enlace::Esperada::Codigo`]): a sala apresentou a
    ///   chave Ed25519 dela e uma assinatura sobre a impressão DTLS desta
    ///   conexão (a mensagem 0x03 do enlace, D3 da 0.17), conferidas pelo
    ///   [`crate::confere_codigo::ConfereCodigo`]. A recusa volta como
    ///   [`ConnectError::CodigoNaoConfere`].
    ///
    /// Recusada, a conexão fecha com [`CODIGO_DA_RECUSA`] e uma frase neutra, e
    /// nada do que quem liga diria — convite, senha, apelido, chave — chega a
    /// quem atendeu.
    ///
    /// `o_pino_prova_o_servidor` diz se o pin de `pin_key` prova o servidor
    /// (`crate::enlace::Destino::o_pino_prova_o_servidor`), e vai ao
    /// verificador junto com a esperada: onde não prova, um pin que confere
    /// não passa por cima de uma esperada que não confere
    /// ([`crate::tofu::TofuVerifier::new`]).
    ///
    /// `tocou` recebe o `prazo_s` quando a sala manda `Tocando` (tarefa 16 da
    /// F7): é por ele que a corrida de [`crate::enlace`] sabe que este lote
    /// venceu, muito antes de o aperto de mão terminar.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Client::connect`].
    #[expect(
        clippy::too_many_arguments,
        reason = "os oito do `connect` público mais a ponta, a conexão, a impressão \
                  esperada, se o pino prova o servidor e o aviso do toque; agrupá-los \
                  tocaria todo chamador do produto"
    )]
    pub(crate) async fn connect_sobre(
        ponta: crate::transporte::PontaDaSessao,
        connection: crate::transporte::Conexao,
        pin_key: &str,
        esperada: Option<&crate::enlace::Esperada>,
        o_pino_prova_o_servidor: bool,
        nickname: &str,
        signing_key: &SigningKey,
        pins: Arc<dyn PinStore>,
        join_secret: Option<&str>,
        aceito: Option<&str>,
        tocou: Option<tokio::sync::mpsc::UnboundedSender<u16>>,
    ) -> Result<Self, ConnectError> {
        // O que o DTLS desta conexão viu: a impressão do certificado de quem
        // atendeu, lida da própria conexão — nunca de algo que o servidor diga
        // pelo protocolo, nem do link.
        let impressao_dtls = connection.impressao_dtls_do_par();
        let (pin, a_pinar, impressao) = match conferir_quem_atendeu(
            connection.apresentacao(),
            &impressao_dtls,
            pin_key,
            esperada,
            o_pino_prova_o_servidor,
            &pins,
        ) {
            Ok(conferido) => conferido,
            Err(erro) => {
                // **Antes do `Hello`.** Nada saiu pelo `ctl`.
                connection.close(CODIGO_DA_RECUSA, RECUSA_NEUTRA);
                return Err(erro);
            }
        };

        // PATTERN: ORANGE — connected, not verified.
        let (mut send, mut recv) = connection.controle().ok_or_else(|| {
            tracing::warn!("o enlace não entregou o canal de controle");
            ConnectError::ProtocolViolation
        })?;
        // Control above every transfer, on this end too. ADR 0027; the ordering
        // only holds inside this one connection, and that limit is written down
        // in `seele_server::transfer` rather than implied here.
        let _ = send.set_priority(CONTROL_PRIORITY);
        // **O aperto de mão tem dois prazos** (tarefa 16 da F7). Até a primeira
        // resposta depois da prova, e dela até a sessão, os 10 s de sempre,
        // contados do começo. Numa sala pessoal essa resposta pode ser
        // `Tocando`: quem recebe ainda vai decidir, e dali em diante o prazo é o
        // do toque ([`prazo_do_toque`]), contado de quando ele chegou.
        let fim_do_aperto = tokio::time::Instant::now() + HANDSHAKE_TIMEOUT;
        let (resposta, carimbo) = tokio::time::timeout_at(
            fim_do_aperto,
            handshake(
                &mut send,
                &mut recv,
                nickname,
                signing_key,
                &impressao,
                &connection.impressao_dtls_local(),
                join_secret,
            ),
        )
        .await
        .map_err(|_| ConnectError::HandshakeTimeout)??;
        let (session, carimbo) = if let ServerMessage::Tocando { prazo_s } = resposta {
            tracing::info!(prazo_s, "tocando: quem recebe vai decidir");
            // Um `send` recusado é quem ouvia ter desistido, e o aperto segue.
            if let Some(tocou) = &tocou {
                let _ = tocou.send(prazo_s);
            }
            tokio::time::timeout(
                prazo_do_toque(prazo_s),
                depois_do_toque(&mut send, &mut recv, carimbo, aceito),
            )
            .await
            .map_err(|_| ConnectError::HandshakeTimeout)??
        } else {
            tokio::time::timeout_at(
                fim_do_aperto,
                concluir(&mut send, &mut recv, carimbo, aceito, resposta),
            )
            .await
            .map_err(|_| ConnectError::HandshakeTimeout)??
        };

        // **A chave inteira fica pinada depois da primeira ligação completa**
        // (ADR 0060, «Força»): daqui em diante, a conferência daquele código usa
        // os 256 bits, e não só os 75. Depois do `Session`, e não dentro do
        // DTLS: uma ligação recusada não deixa pino.
        if let Some((codigo, chave)) = a_pinar {
            pins.pin(
                &crate::confere_codigo::chave_do_pino(&codigo),
                seele_proto::transport::key_fingerprint(&chave),
            );
        }

        // Uma tarefa é dona do fluxo de leitura e entrega quadros inteiros por
        // um canal. Nada mais lê deste fluxo, então não há corrida por quadro;
        // e como ninguém cancela um `read_exact` no meio, o fluxo não
        // dessincroniza.
        let (para_dentro, inbox) = tokio::sync::mpsc::unbounded_channel();
        let do_leitor = connection.clone();
        tokio::spawn(async move {
            let mut recv = recv;
            // As mensagens de um vocabulário mais novo que o servidor mandou. A
            // primeira vai ao registro; as outras só à contagem, para um
            // servidor defeituoso não encher o `seele.log`.
            let mut desconhecidas = seele_proto::control::Desconhecidas::default();
            // Se uma `Disconnecting` já passou: o código de fechamento que vier
            // depois dela não vira uma segunda despedida.
            let mut despediu = false;
            loop {
                match frame::ler_da_lista::<ServerMessage>(&mut recv).await {
                    Ok(Lido::Mensagem(mensagem)) => {
                        despediu |= matches!(mensagem, ServerMessage::Disconnecting { .. });
                        if para_dentro.send(mensagem).is_err() {
                            // Ninguém mais escuta: o `Client` foi descartado.
                            return;
                        }
                    }
                    // **Pulada, contada, e a sessão segue** (adendo de
                    // 08/10/2026, §6), como no servidor. Até a 0.15 o `decode`
                    // recusava, esta tarefa terminava, e a sessão parava de
                    // receber tudo sem dizer por quê. O quadro tem comprimento
                    // próprio, e pular não desalinha o próximo. Rede de
                    // segurança, e não licença: o servidor continua não
                    // mandando o que este app não entende (ADR 0058).
                    Ok(Lido::Desconhecida { ordinal }) => {
                        if desconhecidas.contar() {
                            tracing::warn!(
                                ordinal,
                                "mensagem de controle desconhecida, pulada: o servidor fala um \
                                 vocabulário mais novo que este app"
                            );
                        }
                    }
                    Err(erro) => {
                        tracing::debug!(%erro, "o fluxo de controle terminou");
                        // **A despedida que o fio não levou.** Sem crédito no
                        // `ctl`, o `Disconnecting { FellBehind }` do servidor
                        // pode não sair, e ele fecha com
                        // [`crate::transporte::CODIGO_FICOU_PARA_TRAS`]. O corte
                        // não depende de crédito. Lido aqui, ele vira a mesma
                        // mensagem que o quadro daria, e o motor, o `Room` e a
                        // casca seguem o caminho de sempre: o aviso, a queda e a
                        // bateria, sem fim.
                        if !despediu {
                            if let Some(reason) =
                                despedida_pelo_fechamento(&do_leitor, ESPERA_PELO_CORTE).await
                            {
                                tracing::info!(
                                    ?reason,
                                    "o servidor fechou com o código de despedida, sem o quadro"
                                );
                                let _ = para_dentro.send(ServerMessage::Disconnecting { reason });
                            }
                        }
                        return;
                    }
                }
            }
        });

        // E outra é dona do `accept_uni`, pela mesma razão e contra o mesmo
        // defeito.
        //
        // Uma conexão tem um `accept_uni` só, e dois usos para fluxo de
        // entrada: anexo e tela. Enquanto cada um chamava `accept_uni` por
        // conta própria, quem chegasse primeiro pegava o fluxo do outro — um
        // anexo baixando durante uma transmissão de tela levava o quadro-chave
        // embora, e a tela ficava esperando um fluxo que já tinha sido lido e
        // descartado por engano. É a dívida que o §5.2 da spec de 22/08
        // registrou quando o byte de tipo entrou: ele passou a dizer o que cada
        // fluxo é, e faltava alguém para ler isso e distribuir.
        //
        // O byte é lido **aqui**, e por isso quem recebe do outro lado não o lê
        // de novo: `Recepcao::do_fluxo_ja_tipado` e `receive_attachment`
        // começam depois dele.
        let (tela_para_dentro, telas) = tokio::sync::mpsc::unbounded_channel();
        let (anexo_para_dentro, anexos) = tokio::sync::mpsc::unbounded_channel();
        let conexao_do_roteador = connection.clone();
        tokio::spawn(async move {
            loop {
                let Ok(mut fluxo) = conexao_do_roteador.accept_uni().await else {
                    // A conexão acabou. As pontas dos canais caem junto, e quem
                    // esperava um fluxo recebe o fim em vez de esperar para
                    // sempre.
                    return;
                };
                let mut tipo = [0_u8; seele_proto::stream::STREAM_TYPE_LEN];
                if let Err(erro) = fluxo.read_exact(&mut tipo).await {
                    tracing::debug!(%erro, "um fluxo de entrada morreu antes de dizer o que era");
                    continue;
                }
                match seele_proto::stream::StreamType::decode(tipo[0]) {
                    Ok(seele_proto::stream::StreamType::Screen) => {
                        if tela_para_dentro.send(fluxo).is_err() {
                            return;
                        }
                    }
                    Ok(seele_proto::stream::StreamType::Attachment) => {
                        if anexo_para_dentro.send(fluxo).is_err() {
                            return;
                        }
                    }
                    // **Um fluxo de volume não entra por aqui.** ADR 0048: os
                    // bytes vão da janela para o servidor, e o que volta no
                    // outro sentido é o `volume.servir`, que ainda não existe.
                    // Descartado e nomeado, em vez de calado: se ele começar a
                    // chegar, quem lê o log fica sabendo antes de alguém
                    // descobrir pela imagem que não aparece.
                    Ok(seele_proto::stream::StreamType::ModVolume) => {
                        tracing::debug!("um fluxo de volume de MOD chegou, e nada o consome ainda");
                    }
                    Err(erro) => {
                        // Um tipo que este build não conhece é um fluxo de uma
                        // versão mais nova, e não um defeito: descartar um é
                        // muito melhor que fechar a conexão por causa dele.
                        tracing::debug!(%erro, byte = tipo[0], "fluxo de entrada de tipo estranho");
                    }
                }
            }
        });

        Ok(Self {
            connection,
            send,
            carimbo,
            inbox,
            session,
            pin,
            link_state: LinkTrust::Verified,
            pending_ping: None,
            last_rtt: None,
            ping_da_sonda: None,
            pings_da_sonda: 0,
            ultimo_ping_da_sonda: None,
            ida_e_volta_da_sonda: None,
            telas: std::sync::Arc::new(tokio::sync::Mutex::new(telas)),
            anexos: std::sync::Arc::new(tokio::sync::Mutex::new(anexos)),
            ponta,
        })
    }

    /// A ponta WebRTC que esta conexão usa para falar com o servidor.
    ///
    /// **É o que o caminho entre pares reaproveita**, e não uma ponta nova —
    /// ver o doc do campo. Devolvida como punho: a `Ponta` é barata de clonar
    /// (um `Arc` por dentro), e `enlace::Motor` guarda a própria cópia para
    /// discar e atender nela.
    #[must_use]
    pub fn ponta(&self) -> crate::transporte::PontaDaSessao {
        self.ponta.clone()
    }

    /// A fila por onde chegam os fluxos de tela, sem o byte de tipo.
    ///
    /// Devolvida como punho e não lida aqui: quem a lê é um braço de um
    /// `select!` que, no outro braço, já tem o `Client` emprestado mutável para
    /// ler eventos, e dois empréstimos no mesmo `select!` não compilam. Um
    /// punho tirado antes do laço não empresta nada.
    #[must_use]
    pub fn fila_de_telas(&self) -> FilaDeTelas {
        std::sync::Arc::clone(&self.telas)
    }

    /// What the server told us.
    #[must_use]
    pub fn session(&self) -> &SessionInfo {
        &self.session
    }

    /// How far the connection has got.
    #[must_use]
    pub fn link_state(&self) -> LinkTrust {
        self.link_state
    }

    /// What the TOFU check decided. ADR 0003.
    #[must_use]
    pub fn pin_decision(&self) -> &PinDecision {
        &self.pin
    }

    /// Enters a voice room. `InserirPlug` in `specs/02-protocolo.md`.
    ///
    /// `password` is only looked at by the server when the room actually
    /// needs one (`admissao::voice_room_liberado`); a password for an open
    /// room is simply ignored on that side.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn enter_voice_room(
        &mut self,
        voice_room: VoiceRoomId,
        password: Option<String>,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::EnterVoiceRoom {
                voice_room,
                password,
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
    /// anything else (gap G2), which is what stops one person being credited with
    /// another's audio.
    ///
    /// # Errors
    ///
    /// Fails if the datagram is too large for the path.
    pub fn send_media(&self, datagram: Vec<u8>) -> Result<()> {
        self.media().send(datagram)
    }

    /// Waits for the next media datagram.
    ///
    /// # Errors
    ///
    /// Fails when the connection closes.
    pub async fn next_media(&self) -> Result<Vec<u8>> {
        self.media().next().await
    }

    /// Subscribes to a text channel.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn join_channel(&mut self, channel: ChannelId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::JoinChannel { channel },
        )
        .await
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
        channel: ChannelId,
        body: &str,
        client_message_id: ClientMessageId,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::SendMessage {
                channel,
                body: body.to_owned(),
                replies_to: None,
                client_message_id,
            },
        )
        .await
    }

    // ---- attachments, ADR 0027 ----
    /// Sends an authenticated MOD request. Fails when the control stream closes.
    pub async fn mod_request(
        &mut self,
        request: u32,
        id: String,
        channel: ChannelId,
        payload: String,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::ModRequest {
                request,
                id,
                channel,
                payload,
            },
        )
        .await
    }

    /// Pede uma **ação declarada** de MOD (`ClientMessage::AcaoDeMod`, API 7).
    ///
    /// O mesmo pedido de [`Self::mod_request`], nomeando a ação que o
    /// `mod.json` declarou. Quem autoriza é o servidor, antes de chamar a
    /// metade de servidor; a resposta — ou a recusa `acao-nao-declarada` ou
    /// `sem-permissao` — volta no `ModReply` deste `request`.
    ///
    /// # Errors
    ///
    /// Falha quando o fluxo de controle fecha.
    pub async fn acao_de_mod(
        &mut self,
        request: u32,
        id: String,
        channel: ChannelId,
        acao: String,
        payload: String,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::AcaoDeMod {
                request,
                id,
                channel,
                acao,
                payload,
            },
        )
        .await
    }

    /// The file-moving half of this connection.
    ///
    /// Handed out separately for the reason [`Self::media`] is: a transfer that
    /// takes half a minute must not be a transfer during which nothing else can
    /// be said. A caller holding one of these can move a file while the control
    /// stream goes on carrying presence and text — which is the whole point of
    /// the transfer having a stream of its own, and would be given straight back
    /// by an API that made the caller borrow the whole client for the duration.
    #[must_use]
    pub fn transfers(&self) -> Transfers {
        Transfers {
            connection: self.connection.clone(),
            anexos: std::sync::Arc::clone(&self.anexos),
        }
    }

    /// Asks the server for an attachment's bytes.
    ///
    /// The bytes arrive on a stream of their own; [`Transfers::receive_attachment`]
    /// is what reads them. A refusal arrives here instead, as
    /// [`ServerMessage::AttachmentUnavailable`], with the enumerated reason —
    /// and the expected one is `Expired`.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn fetch_attachment(
        &mut self,
        attachment: seele_proto::ids::AttachmentId,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::FetchAttachment { attachment },
        )
        .await
    }

    /// Asks for an attachment and waits for it, in one call.
    ///
    /// The convenience shape, for a caller with one download in the air. A
    /// caller that may have two uses [`Self::fetch_attachment`] and
    /// [`Transfers::receive_attachment`] and matches them itself.
    ///
    /// `destination` é um caminho inteiro, e quem o escolhe é quem chama — a
    /// conformidade, que grava numa pasta temporária dela. O app não passa por
    /// aqui: ele grava ao lado, pela regra de [`crate::anexo_no_disco`]. Mesmo
    /// assim a regra de não substituir vale aqui também, e pelo mesmo caminho:
    /// os bytes chegam num parcial ao lado de `destination`, criado **antes** de
    /// o pedido sair por [`crate::anexo_no_disco::abrir_parcial`], e só
    /// ganham o nome `destination` depois de o hash conferir, por um link que
    /// falha se ele já existir — ou, num volume sem link físico, por uma
    /// reserva do nome que falha do mesmo jeito. O nome de `destination` passa
    /// por [`crate::anexo_no_disco::nome_seguro`] como qualquer outro.
    ///
    /// Um `destination` que já existe é recusado também antes do pedido, para
    /// não baixar à toa. Essa conferência é só um atalho, vencível por uma
    /// corrida: quem garante que nada é substituído é o link, ou, num volume
    /// sem link físico, a reserva do recuo de
    /// [`crate::anexo_no_disco::Parcial::nomear`].
    ///
    /// # Errors
    ///
    /// Fails if `destination` already exists, if its name is not just a file
    /// name, if the file cannot be written, if the bytes do not hash to what the
    /// server said, or if nothing arrives inside `wait`.
    pub async fn download_attachment(
        &mut self,
        attachment: seele_proto::ids::AttachmentId,
        destination: &std::path::Path,
        wait: std::time::Duration,
        progress: impl FnMut(u64, u64),
    ) -> Result<u64> {
        if tokio::fs::symlink_metadata(destination).await.is_ok() {
            anyhow::bail!(
                "{} já existe, e baixar não substitui nada",
                destination.display()
            );
        }
        let (Some(pasta), Some(nome)) = (
            destination.parent(),
            destination.file_name().and_then(|nome| nome.to_str()),
        ) else {
            anyhow::bail!("{} não termina num nome de arquivo", destination.display());
        };
        let (pasta, nome) = (pasta.to_owned(), nome.to_owned());
        let (arquivo, parcial) = tokio::task::spawn_blocking(move || {
            crate::anexo_no_disco::abrir_parcial(&pasta, &nome, 1)
        })
        .await??;
        let transfers = self.transfers();
        // Um pedido que não sai leva o parcial embora ao soltá-lo, na volta.
        self.fetch_attachment(attachment).await?;
        let (bytes, _) = transfers
            .receive_attachment(attachment, arquivo, parcial, wait, progress)
            .await?;
        Ok(bytes)
    }

    /// Takes the connection out of whatever voice room it is in.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn leave_voice_room(&mut self) -> Result<()> {
        frame::write_em(&mut self.send, self.carimbo, &ClientMessage::LeaveVoiceRoom).await
    }

    /// Announces the mudo — the microphone being muted.
    ///
    /// Announced rather than kept local because the roster shows it: talking to
    /// somebody whose microphone is off is worth knowing, and the mute is only
    /// half a feature if nobody else can see it. `specs/07-estetica.md`
    /// gives it a marker in the roster for exactly this reason.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn set_muted(&mut self, on: bool) -> Result<()> {
        frame::write_em(&mut self.send, self.carimbo, &ClientMessage::SetMuted(on)).await
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
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::SetTotalIsolation(on),
        )
        .await
    }

    /// Announces a presence hint.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn set_presence(&mut self, presence: seele_proto::control::Presence) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::SetPresence(presence),
        )
        .await
    }

    /// Asks the server to make a voice room.
    ///
    /// Asks, and nothing more. Nothing here checks whether this person may:
    /// `specs/08-seguranca.md` puts the decision on the server, and a core that
    /// refused on its own would be a second authority to keep in step with the
    /// first. What comes back is a `VoiceRoomCreated` on the event stream if it
    /// happened, or an `Alert` carrying `PermissionDenied` if it did not.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn create_voice_room(
        &mut self,
        name: &str,
        limit: u16,
        channel: Option<ChannelId>,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::CreateVoiceRoom {
                name: name.to_owned(),
                limit,
                channel,
            },
        )
        .await
    }

    /// Asks the server to make o canal.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn create_channel(&mut self, name: &str) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::CreateChannel {
                name: name.to_owned(),
            },
        )
        .await
    }

    /// Asks the server to rename a voice room.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn rename_voice_room(&mut self, voice_room: VoiceRoomId, name: &str) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::RenameVoiceRoom {
                voice_room,
                name: name.to_owned(),
            },
        )
        .await
    }

    /// Asks the server to rename o canal.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn rename_channel(&mut self, channel: ChannelId, name: &str) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::RenameChannel {
                channel,
                name: name.to_owned(),
            },
        )
        .await
    }

    /// Asks the server to rename itself.
    ///
    /// Asks, like the four above, and for the same reason: what may be done is
    /// the server's to decide, and it wants `Permission::AdministerServer` for
    /// this. A refusal comes back as an `Alert` carrying `PermissionDenied`;
    /// success comes back as `ServerMessage::ServerRenamed`, to everybody.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed, or if the name is blank or longer
    /// than `MAX_CLIENT_NAME_LEN` — `encode` refuses it before it leaves.
    pub async fn rename_server(&mut self, name: &str) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::RenameServer {
                name: name.to_owned(),
            },
        )
        .await
    }

    /// Asks the server to change its picture, or to have none.
    ///
    /// The bytes are refused before they leave if they are not a PNG within the
    /// bounds `seele_proto::control` sets, which is what keeps a shell from
    /// having to know the rules: it hands over whatever the person picked and
    /// reads the error.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed, or if the picture is not an
    /// acceptable icon.
    pub async fn set_server_icon(&mut self, icon: Option<Vec<u8>>) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::SetServerIcon { icon },
        )
        .await
    }

    /// Põe ou tira a imagem de perfil de quem está usando este cliente.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn set_person_icon(&mut self, icon: Option<Vec<u8>>) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::SetPersonIcon { icon },
        )
        .await
    }

    /// Troca o apelido de quem está usando este cliente.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn set_nickname(&mut self, name: String) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::SetNickname { name },
        )
        .await
    }

    /// Asks the server to end a person's session.
    ///
    /// Asks, like the four above, and for the same reason: nothing here checks
    /// whether this person may. `specs/08-seguranca.md` puts that on the server,
    /// and a core that refused on its own would be a second authority to keep
    /// in step with the first — and the one that would drift, because it is the
    /// one nobody re-reads.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn kick_person(&mut self, person: PersonId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::KickPerson { person },
        )
        .await
    }

    /// Pede um verbo de cargo (ADR 0064), com a revisão que a tela viu. Quem
    /// decide é o servidor; a resposta é o `QuadroDeCargos` novo ou um
    /// `CargoRecusado`. `None` no pedido é o `PedirCargos`.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn cargo(
        &mut self,
        pedido: Option<(u64, seele_proto::cargos::PedidoDeCargo)>,
    ) -> Result<()> {
        let mensagem = match pedido {
            Some((revisao, pedido)) => seele_proto::cargos::mensagem_do_pedido(revisao, pedido),
            None => ClientMessage::PedirCargos,
        };
        frame::write_em(&mut self.send, self.carimbo, &mensagem).await
    }

    /// Asks the server to bar a person from returning.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn ban_person(
        &mut self,
        person: PersonId,
        reason: Option<&str>,
        expires_at: Option<i64>,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::BanPerson {
                person,
                reason: reason.map(str::to_owned),
                expires_at,
            },
        )
        .await
    }

    /// Asks the server to take a message off its Channel.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn remove_message(&mut self, message: MessageId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::RemoveMessage { message },
        )
        .await
    }

    /// Asks the server to move a person into a voice room.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn move_person(&mut self, person: PersonId, voice_room: VoiceRoomId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::MovePerson { person, voice_room },
        )
        .await
    }

    /// Asks for a page of history, oldest of the page first on the wire.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn fetch_history(
        &mut self,
        channel: ChannelId,
        cursor: Option<MessageId>,
        limit: u16,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::FetchHistory {
                channel,
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
            if let Some((sent, at)) = self.ping_da_sonda {
                if sent == timestamp {
                    let contadas = self.ida_e_volta_da_sonda.map_or(0, |(_, n)| n);
                    self.ida_e_volta_da_sonda = Some((at.elapsed(), contadas.saturating_add(1)));
                    self.ping_da_sonda = None;
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
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::Ping { timestamp: stamp },
        )
        .await
    }

    /// O `Ping` que segue o pedido de entrada de número `numero` desta conexão.
    ///
    /// O carimbo é [`crate::assento::carimbo_da_entrada`], e o `Pong` dele diz
    /// que o pedido foi decidido — ele e os mais velhos: ver o doc de
    /// [`crate::assento`]. Não mexe no `Ping` de rotina que mede o ida-e-volta,
    /// e o `Pong` dele não é medido.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn send_ping_de_entrada(&mut self, numero: u64) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::Ping {
                timestamp: crate::assento::carimbo_da_entrada(numero),
            },
        )
        .await
    }

    /// Um `Ping` com o carimbo que quem chama escolheu: uma **cerca**.
    ///
    /// O servidor lê o fluxo de controle em ordem e responde o `Ping` na mesma
    /// volta em que o lê, devolvendo o carimbo inteiro sem lê-lo. O `Pong`
    /// deste carimbo chega, então, depois de tudo o que o servidor escreveu em
    /// resposta aos quadros mandados antes dele — uma página inteira de
    /// histórico, por exemplo (lote P61, a busca do buraco na volta).
    ///
    /// Não mexe no `Ping` de rotina que mede o ida-e-volta, e o `Pong` dele não
    /// é medido. Quem escolhe o carimbo escolhe também uma marca que não se
    /// confunda com a do `Ping` de entrada (o bit 63) nem com o contador de
    /// rotina.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn send_ping_marcado(&mut self, carimbo: u64) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::Ping { timestamp: carimbo },
        )
        .await
    }

    /// What the transport says about flow control on this connection.
    ///
    /// **Zero desde a 0.17**: ver [`FlowControl`]. O SCTP do enlace não conta
    /// bloqueio, e este número não é inventado aqui em cima.
    ///
    /// This exists because of `docs/pendencias.md` #1, whose first suspicion was
    /// the flow-control window at the start of a connection. A suspicion like
    /// that is either killed with a number or carried for ever — and the number
    /// has to be readable from the machine where the symptom reproduces, which
    /// is not necessarily the machine anybody is developing on.
    #[must_use]
    pub fn flow_control(&self) -> FlowControl {
        FlowControl::default()
    }

    /// O que o transporte contou sobre o caminho, para a [`crate::caminho::Sonda`].
    ///
    /// Os bytes vêm da `Leitura` do enlace, e por um motivo que vale escrever:
    /// **é a única fonte que sabe quantos bytes de fato saíram pelo soquete.**
    /// O que a bomba conta são os bytes que ela escreveu no canal, e um byte
    /// escrito num canal não é um byte na rede — pode estar na fila do SCTP
    /// esperando a vez. É justamente essa diferença que a sonda precisa medir.
    ///
    /// Devolve o acumulado desde o começo desta conexão; a subtração é da sonda,
    /// que é quem sabe onde a janela dela começou.
    #[must_use]
    pub fn amostra_do_transporte(&self) -> crate::caminho::Transporte {
        transporte_do_enlace(
            self.connection.leitura().bytes_enviados,
            self.ida_e_volta_da_sonda,
            self.ping_da_sonda.map(|(_, saiu)| saiu.elapsed()),
        )
    }

    /// Manda o `Ping` da sonda de subida, se não há um no ar.
    ///
    /// É a medida de dor da sonda desde a 0.17 (ver
    /// [`crate::caminho::IdaEVolta`]): `crate::enlace` o chama a cada
    /// [`INTERVALO_DO_PING_DA_SONDA`] enquanto a tela transmite. Um no ar de
    /// cada vez, para a fila não ser medida pelo próprio `Ping`; o que ainda
    /// não voltou é lido como piso da ida e volta (`IdaEVolta::em_voo`).
    ///
    /// O carimbo leva [`MARCA_DO_PING_DA_SONDA`], que não se confunde com o
    /// `Ping` de rotina, o da entrada (bit 63) nem o da cerca do buraco (bit 62).
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn pingar_para_a_sonda(&mut self) -> Result<()> {
        if self.ping_da_sonda.is_some()
            || self
                .ultimo_ping_da_sonda
                .is_some_and(|saiu| saiu.elapsed() < INTERVALO_DO_PING_DA_SONDA)
        {
            return Ok(());
        }
        self.ultimo_ping_da_sonda = Some(std::time::Instant::now());
        self.pings_da_sonda = self.pings_da_sonda.wrapping_add(1);
        let carimbo = MARCA_DO_PING_DA_SONDA | (self.pings_da_sonda & (MARCA_DO_PING_DA_SONDA - 1));
        self.ping_da_sonda = Some((carimbo, std::time::Instant::now()));
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::Ping { timestamp: carimbo },
        )
        .await
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

    /// Closes the connection: the person ejected.
    pub fn disconnect(&mut self) {
        self.close(EJECTED);
    }

    /// Closes the connection, naming what closed it.
    ///
    /// The reason travels in the link's closing `corte` on `ctl` and is what the
    /// server's log records, so it is the only trace the other side keeps of why
    /// a session that had already completed the handshake went away. Every
    /// close that is not a person leaving has to say so here: an invite that did
    /// not match used to be recorded as an ejection, which is the one reading
    /// that hides a refusal.
    pub fn close(&mut self, reason: &[u8]) {
        self.link_state = LinkTrust::Offline;
        self.connection.close(0, reason);
    }

    // ---- unmaking a room ----
    //
    // Appended at the end of the impl on purpose, and not beside `rename_voice_room`
    // where they belong by subject: `crates/seele-core/src/client.rs` is being
    // worked on elsewhere at the same time, around the message and event paths
    // in the middle of this file. Three additions with nothing above or below
    // them is the smallest thing that can be added here.
    //
    // Asks, and nothing more, like every other verb in here. Nothing checks
    // whether this person may — `specs/08-seguranca.md` puts that on the server,
    // and a core that refused on its own would be a second authority to keep in
    // step with the first.

    /// Asks the server to destroy a voice room.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn delete_voice_room(&mut self, voice_room: VoiceRoomId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::DeleteVoiceRoom { voice_room },
        )
        .await
    }

    /// Asks the server to destroy the channel, and everything written in it.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn delete_channel(&mut self, channel: ChannelId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::DeleteChannel { channel },
        )
        .await
    }

    /// Asks what destroying o canal would cost. Destroys nothing.
    ///
    /// The answer arrives as a `ChannelWeighed` on the event stream, like every
    /// other answer this client gets.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn weigh_channel(&mut self, channel: ChannelId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::WeighChannel { channel },
        )
        .await
    }

    // ---- compartilhamento de tela ----
    //
    // Os três verbos de controle, e mais nada: os quadros não passam nem perto
    // do fluxo de controle. Eles vão num fluxo unidirecional próprio, aberto
    // por [`Self::abrir_tela`], e o §3.1 explica por que não num datagrama —
    // `spikes/tela-no-transporte` mediu 16,1% da voz perdida quando os dois
    // dividem a fila de datagramas do `quinn`.

    /// Pede ao servidor para começar a compartilhar tela na sala de voz onde este connection
    /// está.
    ///
    /// Não carrega sala de voz, resolução nem codec. A sala de voz é o que o servidor já sabe —
    /// uma sala de voz vindo de quem pergunta é uma sala de voz que quem pergunta pode apontar
    /// para outro lugar —, e os outros dois descrevem o que sai do encoder, que
    /// não está decidido quando o botão é apertado e **muda depois** (§5: a tela
    /// não promete a escolha). Eles são declarados onde são verdade: na cabeça
    /// do fluxo.
    ///
    /// A resposta é um `ScreenShareStarted` no fluxo de eventos, e o `ScreenId`
    /// dele é o que [`Self::abrir_tela`] precisa. Não dá para abrir o fluxo
    /// antes: até o servidor responder, a transmissão não tem nome.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn start_screen_share(&mut self) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::StartScreenShare,
        )
        .await
    }

    /// Diz ao servidor que esta transmissão acabou.
    ///
    /// Fechar o fluxo diz a mesma coisa e é o que acontece quando a máquina
    /// some. Os dois existem porque a sala precisa distinguir «ela parou de
    /// compartilhar» de «o enlace dela caiu», e um verbo que só existisse como
    /// ausência não daria essa distinção a ninguém.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn stop_screen_share(&mut self) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::StopScreenShare,
        )
        .await
    }

    /// Pede um quadro-chave a quem está compartilhando.
    ///
    /// **Sob demanda, nunca periódico** (§3.3), e é medida e não preferência:
    /// um quadro-chave de 1080p custa 65 KiB, quatro vezes um quadro comum, o
    /// que são 446 ms do orçamento inteiro. Numa conversa entre pares não há
    /// quem entre no meio da transmissão, então quem sabe que um quadro-chave é
    /// preciso é justamente quem está recebendo e não tem de que predizer.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn request_key_frame(&mut self, screen: ScreenId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::RequestKeyFrame { screen },
        )
        .await
    }

    /// Pede para receber a imagem de uma transmissão.
    ///
    /// **Assistir é um pedido, e não o que acontece sozinho.** Enquanto havia uma
    /// transmissão por sala, o servidor a empurrava para todo mundo — o certo,
    /// porque ninguém quer clicar para ver a única coisa que está acontecendo.
    /// Com várias, empurrar todas para todos cobra o que ninguém pediu: cada
    /// máquina baixa uma cópia de cada e abre um decodificador para cada, e nem a
    /// descida nem a CPU de quem assiste entram na conta do teto, que só mede a
    /// subida de quem hospeda.
    ///
    /// O servidor recusa em silêncio uma tela que não seja da sala em que esta
    /// conexão está — é a mesma conferência do quadro-chave, e a razão dela é
    /// impedir que alguém assine a transmissão de outra sala.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn watch_screen(&mut self, screen: ScreenId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::WatchScreen { screen },
        )
        .await
    }

    /// Para de receber a imagem de uma transmissão.
    ///
    /// Fechar o fluxo diria quase o mesmo, e não serve pela razão que
    /// [`Self::stop_screen_share`] já dá: um verbo que só existe como ausência
    /// não distingue «fechei a janela» de «minha conexão caiu», e o servidor
    /// precisa da diferença para devolver o teto a quem ficou.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn unwatch_screen(&mut self, screen: ScreenId) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::UnwatchScreen { screen },
        )
        .await
    }

    /// Avisa o servidor que o par que devia servir esta transmissão não veio.
    ///
    /// **Mandada só por quem recebe, nunca por quem empresta** —
    /// [`seele_proto::control::ClientMessage::ParFalhou`] documenta por quê:
    /// quem sabe que a imagem parou é quem estava esperando por ela; quem
    /// empresta pode ter caído sem chegar a saber de nada.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn par_falhou(
        &mut self,
        screen: ScreenId,
        motivo: seele_proto::control::MotivoDeFalhaDePar,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::ParFalhou { screen, motivo },
        )
        .await
    }

    /// Declara ao servidor a identidade deste par — e se ela empresta a
    /// subida agora, ou só existe.
    ///
    /// **Não consentir em nada não é "esqueça-me".** Desde o fix round 1 da
    /// Task 8, `impressao` e `locais` continuam significando "eu sou esta
    /// pessoa, alcançável aqui" mesmo com o consentimento vazio — só a
    /// saída da sessão (do lado do servidor) apaga a declaração. Quem só
    /// conecta ainda precisa mandar isto **uma vez**, com
    /// `ConsentimentoDePar::de_ninguem()`: sem identidade declarada, a
    /// discagem dele para um par não tem certificado para apresentar quando
    /// `client_auth_mandatory` exigir um.
    ///
    /// `locais` tem de vir vazio quando o consentimento não alcança ninguém —
    /// é regra de fio, e `ClientMessage::validate` a cobra do outro lado.
    ///
    /// # Errors
    ///
    /// Fails if the control stream is closed.
    pub async fn emprestar_subida(
        &mut self,
        consentimento: ConsentimentoDePar,
        impressao: String,
        locais: Vec<SocketAddr>,
    ) -> Result<()> {
        frame::write_em(
            &mut self.send,
            self.carimbo,
            &ClientMessage::EmprestarSubida {
                consentimento,
                impressao,
                locais,
            },
        )
        .await
    }

    /// Abre o fluxo de vídeo desta transmissão.
    ///
    /// Na conexão que já existe, e num fluxo unidirecional — as duas metades do
    /// §3.1, as duas medidas. `teto_bps` é o que [`crate::tela::TetoDeVideo`]
    /// devolveu para a faixa em que o sinal da voz está agora.
    ///
    /// # Errors
    ///
    /// [`crate::tela::ErroDeTela`] se o cabeçalho não passa ou se a conexão não
    /// abre o fluxo.
    pub async fn abrir_tela(
        &self,
        cabecalho: seele_proto::screen::ScreenHeader,
        teto_bps: u32,
        agora: std::time::Instant,
    ) -> Result<crate::tela::Transmissao, crate::tela::ErroDeTela> {
        crate::tela::Transmissao::abrir(&self.connection, cabecalho, teto_bps, agora).await
    }

    /// Um punho desta conexão que só sabe escoar uma tela.
    ///
    /// Embrulha em vez de devolver a [`quinn::Connection`] pelo mesmo motivo
    /// que [`Self::abrir_tela`] embrulha: quem chama não precisa da conexão,
    /// precisa de uma coisa que ela sabe fazer, e uma conexão devolvida é uma
    /// conexão que qualquer um pode abrir fluxo em cima sem passar por aqui.
    ///
    /// **Não é um empréstimo**, e essa é a diferença que faz este método
    /// existir: a transmissão é escoada por uma tarefa que vive tanto quanto
    /// ela, enquanto o motor do enlace continua lendo o fluxo de controle. Um
    /// `&self` não atravessaria o `tokio::spawn`.
    #[must_use]
    pub fn escoadouro_de_tela(&self) -> crate::bomba::Escoadouro {
        crate::bomba::Escoadouro::nova(self.connection.clone())
    }
}

/// A person left. The ordinary end of a session.
const EJECTED: &[u8] = b"ejected";

/// A marca do carimbo do `Ping` da sonda de subida: o bit 61. O 63 é o da
/// entrada (`assento`) e o 62 o da cerca do buraco (`volta`).
pub const MARCA_DO_PING_DA_SONDA: u64 = 1 << 61;

/// De quanto em quanto a sonda pede uma ida e volta enquanto a tela transmite.
///
/// Meio segundo: duas medidas por janela de um segundo da sonda, e duas
/// mensagens por segundo no `ctl`, muito abaixo do balde de quadros do servidor
/// (20 por segundo).
pub const INTERVALO_DO_PING_DA_SONDA: std::time::Duration = std::time::Duration::from_millis(500);

/// Quanto a tarefa leitora espera pelo motivo da conexão depois de o `ctl`
/// terminar.
///
/// **Limitada, e não zero nem infinita** (a revisão do cliente lento). O fim do
/// `ctl` pode chegar antes do corte que diz o motivo, então ler o motivo na hora
/// às vezes não acha nada. E esperar sem teto seguraria a queda de toda conexão
/// cujo `ctl` acabou e o resto não. Na queda comum o motivo já está lá, e a
/// espera não custa nada; ela só é paga no caso raro do `ctl` terminado com a
/// conexão de pé.
const ESPERA_PELO_CORTE: std::time::Duration = std::time::Duration::from_secs(1);

/// A despedida que um código de fechamento diz, quando diz alguma.
///
/// **Só o código do «ficou para trás» vira despedida.** O 0 é o fechamento
/// comum; o 1, o 2 e o 3 são cortes do enlace; os do core (`0x5345_0010`, a
/// recusa antes do `Hello`, e `0x5345_0020`, a recusa de par) são deste lado; e
/// um código desconhecido continua sendo uma queda sem nome. Inferir
/// `FellBehind` de qualquer corte seria dar nome ao que não se sabe.
fn despedida_do_codigo(codigo: u32) -> Option<seele_proto::control::DisconnectReason> {
    (codigo == crate::transporte::CODIGO_FICOU_PARA_TRAS)
        .then_some(seele_proto::control::DisconnectReason::FellBehind)
}

/// O motivo tipado da conexão, lido depois de o `ctl` terminar, como
/// despedida. Espera no máximo `espera` pelo motivo.
async fn despedida_pelo_fechamento(
    conexao: &crate::transporte::Conexao,
    espera: std::time::Duration,
) -> Option<seele_proto::control::DisconnectReason> {
    match tokio::time::timeout(espera, conexao.closed()).await {
        Ok(crate::transporte::Motivo::Fechada { codigo }) => despedida_do_codigo(codigo),
        Ok(_) | Err(_) => None,
    }
}

/// A telemetria que o enlace da 0.17 dá à sonda: **só o que existe**.
///
/// O SCTP do enlace não conta perda nem reação de congestionamento, e esses
/// ficam `None` — e não zero, que a sonda leria como «nada doeu». A ida e volta
/// é a do `Ping` da sonda (`(última, quantas)`), e `em_voo` é há quanto tempo
/// saiu o que ainda não voltou.
fn transporte_do_enlace(
    bytes_enviados: u64,
    ida_e_volta_da_sonda: Option<(std::time::Duration, u64)>,
    em_voo: Option<std::time::Duration>,
) -> crate::caminho::Transporte {
    crate::caminho::Transporte {
        bytes_enviados,
        eventos_de_congestionamento: None,
        pacotes_perdidos: None,
        ida_e_volta: ida_e_volta_da_sonda.map(|(valor, amostras)| crate::caminho::IdaEVolta {
            valor,
            amostras,
            em_voo,
        }),
    }
}

#[cfg(test)]
mod a_telemetria_do_enlace {
    use super::transporte_do_enlace;
    use crate::caminho::{Amostra, IdaEVolta, Sonda, TETO_SEM_MEDIDA_BPS};
    use crate::tela::{TetoDeVideo, CAMINHO_DA_PROVA_BPS};
    use seele_proto::signal::SignalBand;
    use std::time::{Duration, Instant};

    /// O que o enlace não mede chega à sonda como ausência, nunca como zero.
    #[test]
    fn perda_e_congestionamento_sao_ausentes_e_nao_zero() {
        let t = transporte_do_enlace(1_000, None, None);
        assert_eq!(t.bytes_enviados, 1_000);
        assert_eq!(t.pacotes_perdidos, None);
        assert_eq!(t.eventos_de_congestionamento, None);
        assert_eq!(t.ida_e_volta, None, "sem Pong não há ida e volta");

        let t = transporte_do_enlace(
            1_000,
            Some((Duration::from_millis(30), 4)),
            Some(Duration::from_millis(5)),
        );
        assert_eq!(t.pacotes_perdidos, None);
        assert_eq!(t.eventos_de_congestionamento, None);
        assert_eq!(
            t.ida_e_volta,
            Some(IdaEVolta {
                valor: Duration::from_millis(30),
                amostras: 4,
                em_voo: Some(Duration::from_millis(5)),
            })
        );
    }

    /// De ponta a ponta no cliente: janelas cheias com a telemetria que o
    /// `Client` monta, sem `Pong`, não levam a estimativa acima do teto sem
    /// medida.
    #[test]
    fn sem_pong_a_amostra_do_cliente_nao_faz_a_sonda_subir() {
        let mut sonda = Sonda::nova();
        let inicio = Instant::now();
        // A perna de quem hospeda larga, fora do caminho: só a nossa conta.
        let teto = TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS)
            .com_caminho_de_quem_hospeda(200_000_000)
            .teto(SignalBand::Nominal);
        let mut bytes = 0;
        for s in 0..30_u64 {
            bytes += 400_000;
            sonda.observar(
                inicio + Duration::from_secs(s),
                &Amostra {
                    transporte: transporte_do_enlace(bytes, None, None),
                    teto,
                    faixa: SignalBand::Nominal,
                },
            );
        }
        assert!(sonda.estimativa() <= TETO_SEM_MEDIDA_BPS);
        assert!(sonda.janelas_sem_medida() > 0);
    }
}

/// The client refused the server: the invite named a different key.
///
/// **Segunda linha, e não o caminho de sempre.** Desde que a impressão esperada
/// é conferida dentro do TLS, a recusa acontece no aperto de mão
/// (`crate::tofu::PinDecision::InviteRefused`): quem atendeu vê um TLS que
/// falhou, com a frase neutra de `crate::tofu`, e este motivo **não chega a
/// ele**. Só chegaria pela conferência de depois do aperto de mão: o
/// `seele_core::enlace::Enlace::conectar` derruba a conexão com ele quando
/// `tofu::verdict` devolve `InviteRefused`, e isso exige um verificador que
/// tenha deixado passar um primeiro contato que a impressão desmente.
///
/// A string continua importando, para essa segunda linha: `docs/pendencias.md`
/// #12 aceitou que a queda explícita não tem guarda automático justamente
/// porque este motivo é a única diferença observável que sobra entre derrubar a
/// conexão e deixá-la morrer sozinha.
pub const INVITE_REFUSED: &[u8] = b"invite refused";

/// The local socket a client sends from.
///
/// # Why `[::]` and not `0.0.0.0`
///
/// An IPv4 socket cannot send to an IPv6 destination at all. A client bound to
/// `0.0.0.0` therefore could not reach a server over IPv6 no matter what the
/// server did — which is the other half of why step 2 of ADR 0022 had not
/// started, and the half that no amount of work on the server would have fixed.
///
/// An IPv6 socket with `IPV6_V6ONLY` cleared reaches both families. quinn's
/// `connect` rewrites an IPv4 destination into its IPv4-mapped form
/// (`::ffff:a.b.c.d`) before the socket sees it, and its own documentation
/// calls `[::]:0` "a reasonable default to maximize the ability to connect".
///
/// # Why the socket is no longer `Endpoint::client`'s
///
/// Because of the port. `Endpoint::client([::]:0)` takes the one the system
/// picks, and on macOS that pick may be a port an IPv4 socket bound to a
/// specific address already holds — and then the server's handshake reply goes
/// to that socket instead. [`socket_de_quem_conecta`] opens the same dual-stack
/// socket on a port it has checked; see [`socket_conferido`].
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
/// in `seele-server`), because a server that silently stops answering half the
/// internet is a bug nobody can diagnose from the outside. This side does not,
/// as `Endpoint::client` did not: on the four targets `deny.toml` builds for —
/// Linux, the two macOS, Windows — clearing the option works, and
/// [`pilha_dupla`] only logs a refusal. A platform that refuses it would leave a
/// client that reaches IPv6 and not IPv4, and that is the trade being taken
/// knowingly rather than missed.
///
/// # Desde a 0.17
///
/// Quem abre a ponta WebRTC em cima deste socket é
/// `crate::transporte::abrir_ponta`, uma por sessão (E3): o `CHAMA` do degrau 4
/// sai da porta dela, e a reconexão de uma sessão não fecha a de outra. O
/// recuo para um socket só IPv4 continua lá.
///
/// # Errors
///
/// Os do sistema ao abrir ou ligar o socket IPv6 — numa máquina sem IPv6, por
/// exemplo, e aí quem chama cai para um socket só IPv4.
pub(crate) fn socket_de_quem_conecta() -> std::io::Result<std::net::UdpSocket> {
    socket_conferido(proposta_do_sistema)
}

/// O socket que o sistema propõe: `[::]:0`, de pilha dupla, o mesmo que
/// `quinn::Endpoint::client` abria.
///
/// Nos testes, a proposta guardada em [`PROPOSTA_DE_TESTE`] por esta thread
/// passa na frente, uma vez — um socket, ou um erro.
fn proposta_do_sistema() -> std::io::Result<socket2::Socket> {
    #[cfg(test)]
    if let Some(proposta) = PROPOSTA_DE_TESTE.with(|vaga| vaga.borrow_mut().take()) {
        return proposta;
    }
    pilha_dupla(0)
}

/// Um socket IPv6 com o `IPV6_V6ONLY` desligado, ligado a `[::]:porta`.
///
/// O que `quinn::Endpoint::client` faz com um endereço IPv6, inclusive seguir
/// só IPv6 quando o sistema recusa a pilha dupla.
fn pilha_dupla(porta: u16) -> std::io::Result<socket2::Socket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    if let Err(erro) = socket.set_only_v6(false) {
        tracing::debug!(%erro, "o socket local não ficou de pilha dupla");
    }
    socket.bind(&SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, porta)).into())?;
    Ok(socket)
}

/// Quantas portas o sistema pode propor antes de [`socket_conferido`] desistir.
///
/// Uma proposta só é recusada quando um socket IPv4 desta máquina segura a
/// porta dela, e eles ocupam uma fração mínima das 16 384 portas efêmeras:
/// dezesseis recusas seguidas querem dizer uma máquina sem porta livre.
const PROPOSTAS_DE_PORTA: usize = 16;

/// O socket de pilha dupla numa porta que `propor` escolheu e que **nenhum
/// socket IPv4 desta máquina segura**.
///
/// # Por que conferir a porta
///
/// Porque no macOS a porta que o sistema escolhe para `[::]:0` de pilha dupla
/// pode ser a de um socket IPv4 ligado a um endereço — `127.0.0.1:X`, ou o da
/// rede de casa. Medido no lote Y da F5: com 4 000 sockets em `127.0.0.1:0` de
/// pé, 992 de 4 000 `[::]:0` caíram numa porta deles, e na ordem inversa
/// nenhum. Os dois convivem, e o datagrama endereçado a `127.0.0.1:X` vai ao
/// mais específico, o IPv4: a resposta do servidor ao aperto de mão chega ao
/// socket alheio, o QUIC deste lado não ouve nada e desiste pela ociosidade,
/// 20 s depois (`IDLE_TIMEOUT`), com `TimedOut`. Na bateria de testes, era a
/// volta que não voltava; no produto, é a entrada ou a volta que de vez em
/// quando espera 20 s por nada, quando outro programa segura um socket UDP
/// ligado ao endereço por onde a resposta volta (medido com `127.0.0.1` e com
/// o endereço da rede de casa, e também com um socket IPv4 no curinga
/// `0.0.0.0:X`).
///
/// # Por que a sonda IPv4, e não só ligar de novo
///
/// A escolha de porta do sistema confere só a família do socket. Então a
/// escolha vira proposta, e quem confere é uma **sonda IPv4 no curinga**,
/// ligada explicitamente a `0.0.0.0:X`: o macOS a recusa com `AddrInUse`
/// quando qualquer socket IPv4 segura `X` — num endereço da máquina ou no
/// próprio curinga — e quando um de pilha dupla a segura. A ligação explícita
/// a `[::]:X` sozinha não basta: ela recusa um IPv4 ligado a um endereço, mas
/// **aceita** um IPv4 no curinga, e a resposta a `127.0.0.1:X` vai a ele
/// (medido: 20 de 20).
///
/// A sonda é largada **antes** de ligar `[::]:X`: segurá-la junto funciona no
/// macOS, mas um sistema que trate a pilha dupla e o curinga IPv4 como o mesmo
/// lugar recusaria o `[::]:X` sempre, e todo cliente cairia para só IPv4.
/// Ligado, o socket é dono da porta nas duas famílias: um IPv4 que viesse
/// depois, no endereço ou no curinga, é recusado, e a escolha de porta IPv4
/// não cai nela (medido no macOS: 0 em 3 000).
///
/// **A janela entre largar a sonda e ligar não está coberta.** Um IPv4 que
/// se ligue a `X` *num endereço* ali dentro faz o `[::]:X` voltar `AddrInUse`,
/// e vem outra proposta. Um que se ligue ao *curinga* `0.0.0.0:X` ali dentro é
/// aceito junto (revisão do lote Y), e a resposta daquela conexão vai a ele,
/// como antes do conserto. Para cair nela, outro programa tem de ligar um
/// IPv4 no curinga exatamente em `X` dentro da janela. Ela foi medida duas
/// vezes, 5 000 vezes cada, e a cauda cresce com a carga:
/// - em Python, com carga ~10 (o implementador do lote Y): 4,1 µs na mediana,
///   9,0 µs no p99;
/// - em Rust, por este mesmo caminho, com carga 35 (a revisão do lote Y):
///   7,7–8,0 µs na mediana, 46,6–325,8 µs no p99, e máximo de até 348 ms.
///
/// Com `r` ligações dessas por segundo na máquina, a chance por conexão é
/// ~`r` × janela / 16 384: com `r` = 100, uma em ~18 milhões pelo p99 da
/// primeira medida, e uma em ~500 mil pelo p99 de 326 µs da segunda.
/// Desprezível nas duas. `r` não foi medido.
///
/// # Errors
///
/// Os de `propor`, os da ligação que não sejam `AddrInUse`, e `AddrInUse` se
/// nenhuma das [`PROPOSTAS_DE_PORTA`] estiver livre.
fn socket_conferido(
    mut propor: impl FnMut() -> std::io::Result<socket2::Socket>,
) -> std::io::Result<std::net::UdpSocket> {
    let mut recusa = None;
    for _ in 0..PROPOSTAS_DE_PORTA {
        let proposta = propor()?;
        let porta = proposta
            .local_addr()?
            .as_socket()
            .map(|onde| onde.port())
            .ok_or_else(|| std::io::Error::other("o socket proposto não tem porta IP"))?;
        drop(proposta);
        // Um erro que não seja `AddrInUse` — uma máquina sem IPv4, por
        // exemplo — não diz nada sobre a porta: a ligação abaixo decide.
        match std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, porta)) {
            Ok(sonda) => drop(sonda),
            Err(erro) if erro.kind() == std::io::ErrorKind::AddrInUse => {
                tracing::debug!(
                    porta,
                    "a porta que o sistema propôs é de um socket IPv4 desta máquina, \
                     que ficaria com a resposta do servidor; outra"
                );
                recusa = Some(erro);
                continue;
            }
            Err(_) => {}
        }
        match pilha_dupla(porta) {
            Ok(socket) => {
                socket.set_nonblocking(true)?;
                return Ok(socket.into());
            }
            // Alguém tomou a porta entre a sonda e a ligação, num endereço, ou
            // em IPv6. O IPv4 que a tomar no curinga não chega aqui: a ligação
            // o aceita junto, e é a janela que a doc acima diz não cobrir.
            Err(erro) if erro.kind() == std::io::ErrorKind::AddrInUse => {
                tracing::debug!(porta, "a porta proposta foi tomada antes da ligação; outra");
                recusa = Some(erro);
            }
            Err(erro) => return Err(erro),
        }
    }
    Err(recusa.unwrap_or_else(|| std::io::ErrorKind::AddrInUse.into()))
}

#[cfg(test)]
thread_local! {
    /// A próxima proposta de socket desta thread, no lugar da do sistema.
    ///
    /// Existe para um teste encenar o que o sistema faz por conta própria de
    /// vez em quando — ver [`testes_do_socket::encenar_a_colisao`] — no
    /// socket que `Motor::tentar` abre, que nenhum parâmetro alcança. Um erro
    /// guardado encena o socket de pilha dupla que não abre.
    pub(crate) static PROPOSTA_DE_TESTE: std::cell::RefCell<
        Option<std::io::Result<socket2::Socket>>,
    > = const { std::cell::RefCell::new(None) };
}

/// Disca um anfitrião pela ponta desta sessão: todos os `alvos` num agente ICE
/// só (D2 da 0.17).
///
/// **Sem pino no DTLS, de propósito** (`Pino::Qualquer`): quem confere a
/// impressão é [`conferir_quem_atendeu`], antes do `Hello`, com as regras de
/// pin e de impressão esperada do ADR 0003 inteiras — inclusive a que deixa
/// passar, com aviso, um servidor fixado cujo link está velho
/// (`Verdict::InviteDisagrees`), que um pino no DTLS recusaria. O que sai para
/// quem atendeu antes disso é o DTLS e nada do protocolo.
///
/// `credencial` é o U do `CHAMA` do degrau 4, quando houve; `None` sorteia.
///
/// # Errors
///
/// [`ConnectError::SemResposta`] quando nenhum candidato respondeu ao ICE, e os
/// outros de [`classificar_a_discagem`].
pub(crate) async fn discar(
    ponta: &crate::transporte::Ponta,
    alvos: Vec<SocketAddr>,
    credencial: Option<String>,
    prazo: std::time::Duration,
) -> Result<crate::transporte::Conexao, ConnectError> {
    let quantos = alvos.len();
    let mut discagem = crate::transporte::Discagem::nova(alvos, crate::transporte::Pino::Qualquer);
    discagem.credencial = credencial;
    discagem.prazo = prazo;
    ponta.discar(discagem).await.map_err(|falha| {
        let erro = classificar_a_discagem(&falha);
        tracing::warn!(?falha, ?erro, candidatos = quantos, "a discagem não abriu");
        erro
    })
}

/// O que uma discagem que falhou foi, com nome.
///
/// Sem braço `_`, pela regra de `enlace::alguem_respondeu`: uma falha nova do
/// enlace não compila até alguém dizer o que ela vira na tela.
pub(crate) fn classificar_a_discagem(falha: &crate::transporte::FalhaAoDiscar) -> ConnectError {
    use crate::transporte::FalhaAoDiscar;
    match falha {
        // **Nenhum candidato devolveu a checagem ICE.** É o «os pacotes saíram e
        // nada voltou» de sempre, e não a sincronização.
        FalhaAoDiscar::SemResposta => ConnectError::SemResposta,
        // Só com um pino no DTLS, que o nativo não usa (ver [`discar`]). Se um
        // dia usar, é a impressão que não confere, e sem a vista não há o que
        // mostrar além da recusa.
        FalhaAoDiscar::ImpressaoDiferente { vista } => ConnectError::InviteMismatch {
            expected: String::new(),
            offered: vista.map(|vista| hex(&vista)).unwrap_or_default(),
        },
        // O ICE achou caminho e o DTLS ou o SCTP caíram por outro motivo.
        FalhaAoDiscar::Protocolo(_) => ConnectError::TlsRefused,
        // Alguém respondeu ao ICE e o resto não fechou no prazo: é o servidor
        // demorando, e não a rede sumindo.
        FalhaAoDiscar::Prazo => ConnectError::HandshakeTimeout,
        FalhaAoDiscar::SemAlvos | FalhaAoDiscar::Io(_) => ConnectError::Unreachable,
        FalhaAoDiscar::PontaFechada => ConnectError::LocalEndpoint,
    }
}

/// A impressão em hex minúsculo, como o `fp=` do link e o arquivo de pins a
/// escrevem (`seele_proto::transport::certificate_fingerprint`).
pub(crate) fn hex(digest: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    digest
        .iter()
        .fold(String::with_capacity(64), |mut texto, byte| {
            let _ = write!(texto, "{byte:02x}");
            texto
        })
}

/// O que [`conferir_quem_atendeu`] decidiu: a decisão de pin, a chave da sala a
/// pinar depois da sessão (só no primeiro contato por código), e a impressão
/// que a prova de cliente amarra.
type Conferido = (
    PinDecision,
    Option<(seele_proto::codigo::Codigo, [u8; 32])>,
    [u8; 32],
);

/// Confere quem atendeu **antes do `Hello`**: o TOFU por endereço, ou a sala
/// do código.
///
/// - Por endereço: a impressão do certificado DTLS que o enlace viu passa pelo
///   [`TofuVerifier`], com a esperada e a regra de onde o pino prova o
///   servidor. A prova amarra essa impressão. **Se quem atendeu é uma sala**
///   (apresentação 0x03 presente), a impressão conferida e amarrada é a da
///   chave dela, depois de conferida a assinatura sobre este DTLS.
/// - Por código: `apresentacao` é a mensagem 0x03 da sala (chave Ed25519 e a
///   assinatura dela sobre esta impressão DTLS), conferida pelo
///   [`crate::confere_codigo::ConfereCodigo`]. A prova amarra a impressão da
///   sala, o SHA-256 da chave (D5).
///
/// # Errors
///
/// [`ConnectError::PinChanged`], [`ConnectError::InviteMismatch`] ou
/// [`ConnectError::CodigoNaoConfere`].
pub(crate) fn conferir_quem_atendeu(
    apresentacao: &[u8],
    impressao_dtls: &[u8; 32],
    pin_key: &str,
    esperada: Option<&crate::enlace::Esperada>,
    o_pino_prova_o_servidor: bool,
    pins: &Arc<dyn PinStore>,
) -> Result<Conferido, ConnectError> {
    if let Some(crate::enlace::Esperada::Codigo(codigo)) = esperada {
        // **Uma pessoa, e não um endereço** (ADR 0060): o pino é o da chave
        // sob `@CÓDIGO`, e não o do endereço que respondeu.
        let pino = pins.pinned(&crate::confere_codigo::chave_do_pino(codigo));
        let chave = crate::confere_codigo::ConfereCodigo::novo(*codigo, pino.clone())
            .confere_apresentacao(apresentacao, impressao_dtls)
            .map_err(ConnectError::CodigoNaoConfere)?;
        let impressao_da_sala = seele_proto::transport::certificate_digest(&chave);
        let fingerprint = hex(&impressao_da_sala);
        let decisao = if pino.is_some() {
            PinDecision::Matches { fingerprint }
        } else {
            PinDecision::FirstContact { fingerprint }
        };
        let a_pinar = pino.is_none().then_some((*codigo, chave));
        return Ok((decisao, a_pinar, impressao_da_sala));
    }
    // **A sala atendeu um endereço, e entra pelo TOFU da chave dela** (como na
    // 0.16: a decisão de 16/10 e o ADR 0060, «o dono entra na própria sala por
    // `127.0.0.1:8385`, e o pino do TOFU é por endereço»).
    //
    // Um servidor manda a apresentação vazia; a sala manda a chave e a
    // assinatura sobre este DTLS. O DTLS da sala é efêmero desde a 0.17, então
    // pinar a impressão dele daria «a chave mudou» na entrada seguinte, e a
    // prova amarrada a ele seria recusada pela sala, que confere pela da chave.
    // Conferida a assinatura (o mesmo `ConfereCodigo`, com o código que a
    // própria chave dá), a impressão que o TOFU confere e a prova amarra é a da
    // chave, `certificate_digest(chave)`: a mesma que o `fp=` de um link da
    // sala carrega.
    let chave_da_sala = apresentacao
        .get(..32)
        .and_then(|chave| <[u8; 32]>::try_from(chave).ok())
        .filter(|_| apresentacao.len() == crate::confere_codigo::APRESENTACAO_DA_SALA_LEN);
    let impressao_conferida = match chave_da_sala {
        Some(chave) => {
            let codigo = seele_proto::codigo::Codigo::da_chave(&chave);
            crate::confere_codigo::ConfereCodigo::novo(codigo, None)
                .confere_apresentacao(apresentacao, impressao_dtls)
                .map_err(ConnectError::CodigoNaoConfere)?;
            seele_proto::transport::certificate_digest(&chave)
        }
        None => *impressao_dtls,
    };
    let verificador = TofuVerifier::new(
        Arc::clone(pins),
        pin_key.to_owned(),
        esperada.and_then(|esperada| match esperada {
            crate::enlace::Esperada::Certificado(impressao) => Some(impressao.clone()),
            crate::enlace::Esperada::Codigo(_) => None,
        }),
        o_pino_prova_o_servidor,
    );
    match verificador.conferir_impressao(hex(&impressao_conferida)) {
        decisao @ (PinDecision::FirstContact { .. } | PinDecision::Matches { .. }) => {
            Ok((decisao, None, impressao_conferida))
        }
        PinDecision::Changed { pinned, offered } => {
            Err(ConnectError::PinChanged { pinned, offered })
        }
        PinDecision::InviteRefused { expected, offered } => {
            Err(ConnectError::InviteMismatch { expected, offered })
        }
    }
}

#[cfg(test)]
mod quem_atendeu_e_conferido_antes_do_hello {
    use super::{conferir_quem_atendeu, discar, hex, ConnectError};
    use crate::tofu::{MemoryPinStore, PinDecision, PinStore};
    use std::sync::Arc;

    fn loja() -> Arc<dyn PinStore> {
        Arc::new(MemoryPinStore::new())
    }

    #[test]
    fn o_hex_e_o_mesmo_do_fp_do_link() {
        let der = b"um certificado qualquer";
        assert_eq!(
            hex(&seele_proto::transport::certificate_digest(der)),
            seele_proto::transport::certificate_fingerprint(der),
            "a impressão do DTLS saiu num formato diferente do `fp=` do link: todo pino \
             existente dispararia «a chave mudou»"
        );
    }

    #[test]
    fn primeiro_contato_fixa_e_o_segundo_confere() {
        let pins = loja();
        let fp = [7_u8; 32];
        let (decisao, _, amarrada) =
            conferir_quem_atendeu(&[], &fp, "casa:8383", None, true, &pins).expect("primeiro");
        assert_eq!(
            decisao,
            PinDecision::FirstContact {
                fingerprint: hex(&fp)
            }
        );
        assert_eq!(
            amarrada, fp,
            "a prova tem de amarrar a impressão DTLS vista"
        );
        let (decisao, _, _) =
            conferir_quem_atendeu(&[], &fp, "casa:8383", None, true, &pins).expect("segundo");
        assert_eq!(
            decisao,
            PinDecision::Matches {
                fingerprint: hex(&fp)
            }
        );
    }

    #[test]
    fn a_chave_que_mudou_e_recusada_antes_do_hello() {
        let pins = loja();
        pins.pin("casa:8383", hex(&[1; 32]));
        assert_eq!(
            conferir_quem_atendeu(&[], &[2; 32], "casa:8383", None, true, &pins).map(|_| ()),
            Err(ConnectError::PinChanged {
                pinned: hex(&[1; 32]),
                offered: hex(&[2; 32]),
            })
        );
        assert_eq!(
            pins.pinned("casa:8383"),
            Some(hex(&[1; 32])),
            "o pino mudou"
        );
    }

    #[test]
    fn a_impressao_do_link_que_nao_confere_e_recusada_e_nada_e_fixado() {
        let pins = loja();
        let esperada = crate::enlace::Esperada::Certificado(hex(&[3; 32]));
        assert_eq!(
            conferir_quem_atendeu(&[], &[4; 32], "casa:8383", Some(&esperada), true, &pins)
                .map(|_| ()),
            Err(ConnectError::InviteMismatch {
                expected: hex(&[3; 32]),
                offered: hex(&[4; 32]),
            })
        );
        assert_eq!(
            pins.pinned("casa:8383"),
            None,
            "a recusa fixou a chave errada"
        );
    }

    /// Um anfitrião de mentira em `127.0.0.1` que faz o aperto do protocolo e
    /// confere a prova D5 como o `seeled` confere: sobre a impressão dele, a
    /// impressão DTLS **que ele viu** de quem ligou, e o nonce. Devolve o
    /// endereço, a impressão dele e por onde sai o veredito da prova.
    fn anfitriao_que_confere_a_prova(
        apresentacao: Vec<u8>,
        certificado: crate::transporte::CertificadoDtls,
        impressao_da_prova: [u8; 32],
    ) -> (std::net::SocketAddr, tokio::sync::oneshot::Receiver<bool>) {
        use ed25519_dalek::Verifier as _;
        use seele_proto::control::{ClientMessage, ServerMessage};
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let config = crate::transporte::ConfigDoAnfitriao {
            certificado,
            apresentacao,
            admitir: Arc::new(|_| true),
        };
        let ponta = crate::transporte::Ponta::de_socket(socket, Some(config)).unwrap();
        let onde = ponta.local_addr();
        let (veredito, ouvir) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let Some(conexao) = ponta.aceitar().await else {
                return;
            };
            let Some((mut envio, mut recebe)) = conexao.controle() else {
                return;
            };
            let Ok(ClientMessage::Hello { public_key, .. }) =
                crate::frame::read::<ClientMessage>(&mut recebe).await
            else {
                return;
            };
            let nonce = [9_u8; 32];
            let _ = crate::frame::write(
                &mut envio,
                &ServerMessage::Challenge {
                    nonce: nonce.to_vec(),
                },
            )
            .await;
            let Ok(ClientMessage::Response { proof }) =
                crate::frame::read::<ClientMessage>(&mut recebe).await
            else {
                return;
            };
            let chave = <[u8; 32]>::try_from(public_key.as_slice())
                .ok()
                .and_then(|chave| ed25519_dalek::VerifyingKey::from_bytes(&chave).ok());
            let assinatura = <[u8; 64]>::try_from(proof.as_slice())
                .ok()
                .map(|bytes| ed25519_dalek::Signature::from_bytes(&bytes));
            let mensagem = seele_proto::assinatura::prova_de_cliente_2(
                &impressao_da_prova,
                &conexao.impressao_dtls_do_par(),
                &nonce,
            );
            let confere = matches!((chave, assinatura), (Some(chave), Some(assinatura))
                if chave.verify(&mensagem, &assinatura).is_ok());
            let _ = veredito.send(confere);
            std::future::pending::<()>().await;
        });
        (onde, ouvir)
    }

    /// Como o anfitrião de [`anfitriao_que_despede`] encerra a sessão.
    #[derive(Clone, Copy)]
    enum Despedida {
        /// Só o código de fechamento, sem o quadro: o `ctl` sem crédito.
        SoOCodigo,
        /// O quadro `Disconnecting { FellBehind }` e depois o código.
        QuadroEOCodigo,
        /// Um fechamento com este código, sem quadro.
        Codigo(u32),
    }

    /// Um anfitrião de mentira que completa o aperto do protocolo, entrega o
    /// `Session` e encerra como `despedida` diz.
    fn anfitriao_que_despede(despedida: Despedida) -> std::net::SocketAddr {
        use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let config = crate::transporte::ConfigDoAnfitriao {
            certificado: crate::transporte::CertificadoDtls::efemero().unwrap(),
            apresentacao: Vec::new(),
            admitir: Arc::new(|_| true),
        };
        let ponta = crate::transporte::Ponta::de_socket(socket, Some(config)).unwrap();
        let onde = ponta.local_addr();
        tokio::spawn(async move {
            let Some(conexao) = ponta.aceitar().await else {
                return;
            };
            let Some((mut envio, mut recebe)) = conexao.controle() else {
                return;
            };
            if crate::frame::read::<ClientMessage>(&mut recebe)
                .await
                .is_err()
            {
                return;
            }
            let _ =
                crate::frame::write(&mut envio, &ServerMessage::Challenge { nonce: vec![9; 32] })
                    .await;
            if crate::frame::read::<ClientMessage>(&mut recebe)
                .await
                .is_err()
            {
                return;
            }
            let _ = crate::frame::write(
                &mut envio,
                &ServerMessage::Session {
                    id: seele_proto::ids::SessionId(1),
                    person: seele_proto::ids::PersonId(1),
                    ssrc: seele_proto::ids::Ssrc(1),
                    server: "servidor de teste".into(),
                    voice_rooms: vec![],
                    channels: vec![],
                    roles: vec![],
                    permissions: vec![],
                },
            )
            .await;
            let codigo = match despedida {
                Despedida::SoOCodigo => crate::transporte::CODIGO_FICOU_PARA_TRAS,
                Despedida::QuadroEOCodigo => {
                    let _ = crate::frame::write(
                        &mut envio,
                        &ServerMessage::Disconnecting {
                            reason: DisconnectReason::FellBehind,
                        },
                    )
                    .await;
                    crate::transporte::CODIGO_FICOU_PARA_TRAS
                }
                Despedida::Codigo(codigo) => codigo,
            };
            // Um instante para o `Session` (e o quadro) saírem antes do corte,
            // que no fechamento passa à frente do que está na fila.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            conexao.close(codigo, b"teste");
            let _ = conexao.closed().await;
            std::future::pending::<()>().await;
            drop((ponta, envio, recebe));
        });
        onde
    }

    /// As despedidas que a sessão viu até o `ctl` acabar.
    async fn despedidas_vistas(
        onde: std::net::SocketAddr,
    ) -> Vec<seele_proto::control::DisconnectReason> {
        use seele_proto::control::ServerMessage;
        let mut cliente = super::Client::connect(
            onde,
            "servidor de teste",
            &onde.to_string(),
            "pessoa",
            &ed25519_dalek::SigningKey::from_bytes(&[5; 32]),
            loja(),
            None,
            None,
        )
        .await
        .expect("a sessão não abriu");
        let mut vistas = Vec::new();
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(10), cliente.next_event())
                .await
            {
                Ok(Ok(ServerMessage::Disconnecting { reason })) => vistas.push(reason),
                Ok(Ok(_)) => {}
                Ok(Err(_)) => return vistas,
                Err(_) => panic!("o `ctl` não acabou em 10 s depois do fechamento"),
            }
        }
    }

    /// **Sem o quadro, o código do «ficou para trás» é a mesma despedida.** É o
    /// caso do cliente lento: o `ctl` sem crédito não leva o `Disconnecting`, e o
    /// corte leva o código. A sessão vê um `Disconnecting { FellBehind }`, que o
    /// motor trata como queda com nome (`a_sessao_acabou_aqui`): o aviso, a
    /// bateria, e nada de fim.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_codigo_de_ficar_para_tras_sem_o_quadro_e_a_despedida_fell_behind() {
        let vistas = despedidas_vistas(anfitriao_que_despede(Despedida::SoOCodigo)).await;
        assert_eq!(
            vistas,
            vec![seele_proto::control::DisconnectReason::FellBehind],
            "o fechamento com o código do «ficou para trás» não chegou como a despedida dele"
        );
    }

    /// O quadro e o código juntos dão **uma** despedida, e não duas.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_quadro_seguido_do_codigo_da_uma_despedida_so() {
        let vistas = despedidas_vistas(anfitriao_que_despede(Despedida::QuadroEOCodigo)).await;
        assert_eq!(
            vistas,
            vec![seele_proto::control::DisconnectReason::FellBehind],
            "o quadro e o código viraram {} despedidas",
            vistas.len()
        );
    }

    /// O fechamento comum continua sem nome: o 0 não vira despedida nenhuma.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_codigo_zero_continua_sendo_um_fechamento_comum() {
        let vistas = despedidas_vistas(anfitriao_que_despede(Despedida::Codigo(0))).await;
        assert!(
            vistas.is_empty(),
            "um fechamento comum virou despedida: {vistas:?}"
        );
    }

    /// A matriz dos códigos: só o do «ficou para trás» tem nome. Os do enlace,
    /// os do core e um desconhecido continuam quedas sem nome.
    #[test]
    fn so_o_codigo_de_ficar_para_tras_vira_despedida() {
        use seele_proto::control::DisconnectReason;
        assert_eq!(
            super::despedida_do_codigo(crate::transporte::CODIGO_FICOU_PARA_TRAS),
            Some(DisconnectReason::FellBehind)
        );
        for codigo in [
            0,
            1,
            2,
            3,
            0x5345_0010,
            0x5345_0020,
            0x5345_0031,
            0xDEAD_BEEF,
        ] {
            assert_eq!(
                super::despedida_do_codigo(codigo),
                None,
                "o código {codigo:#x} virou despedida"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_prova_de_cliente_amarra_o_certificado_dtls_que_o_servidor_viu() {
        // D5: a prova assina a impressão do servidor **e** a do DTLS desta
        // ponta. Quem está no meio e repassa a prova numa conexão dele não tem
        // o certificado DTLS que ela amarra.
        let certificado = crate::transporte::CertificadoDtls::efemero().unwrap();
        let impressao = certificado.impressao();
        let (onde, veredito) = anfitriao_que_confere_a_prova(Vec::new(), certificado, impressao);
        let pins: Arc<dyn PinStore> = Arc::new(MemoryPinStore::new());
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            super::Client::connect(
                onde,
                "localhost",
                &onde.to_string(),
                "pessoa",
                &ed25519_dalek::SigningKey::from_bytes(&[5; 32]),
                Arc::clone(&pins),
                None,
                None,
            ),
        )
        .await;
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(10), veredito)
                .await
                .ok()
                .and_then(Result::ok),
            Some(true),
            "a prova do cliente não confere sobre a impressão do servidor, a do DTLS de quem \
             ligou e o nonce"
        );
        assert_eq!(
            pins.pinned(&onde.to_string()),
            Some(hex(&impressao)),
            "o primeiro contato não fixou a impressão DTLS de quem atendeu"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_sala_que_assina_este_dtls_e_aceita_e_a_de_outro_codigo_e_recusada_antes_do_hello() {
        use ed25519_dalek::Signer as _;
        let semente = ed25519_dalek::SigningKey::from_bytes(&[42; 32]);
        let codigo = seele_proto::codigo::Codigo::da_chave(&semente.verifying_key().to_bytes());
        let certificado = crate::transporte::CertificadoDtls::efemero().unwrap();
        let assinatura = semente.sign(&seele_proto::assinatura::apresentacao_da_sala(
            &certificado.impressao(),
        ));
        let mut apresentacao = semente.verifying_key().to_bytes().to_vec();
        apresentacao.extend_from_slice(&assinatura.to_bytes());
        let impressao_da_sala =
            seele_proto::transport::certificate_digest(&semente.verifying_key().to_bytes());

        // A sala certa: a prova amarra a impressão da sala (D5).
        let (onde, veredito) = anfitriao_que_confere_a_prova(
            apresentacao.clone(),
            certificado.clone(),
            impressao_da_sala,
        );
        let ponta =
            crate::transporte::abrir_ponta(std::net::UdpSocket::bind("127.0.0.1:0").ok()).unwrap();
        let conexao = discar(
            ponta.ponta(),
            vec![onde],
            None,
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("discar a sala");
        let pins: Arc<dyn PinStore> = Arc::new(MemoryPinStore::new());
        let esperada = crate::enlace::Esperada::Codigo(codigo);
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            super::Client::connect_sobre(
                ponta,
                conexao,
                "sala",
                Some(&esperada),
                false,
                "pessoa",
                &ed25519_dalek::SigningKey::from_bytes(&[5; 32]),
                Arc::clone(&pins),
                None,
                None,
                None,
            ),
        )
        .await;
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(10), veredito)
                .await
                .ok()
                .and_then(Result::ok),
            Some(true),
            "a prova numa sala não amarrou a impressão da sala"
        );

        // Outro código: recusado antes do `Hello` (o anfitrião nunca dá veredito).
        let (onde, veredito) =
            anfitriao_que_confere_a_prova(apresentacao, certificado, impressao_da_sala);
        let ponta =
            crate::transporte::abrir_ponta(std::net::UdpSocket::bind("127.0.0.1:0").ok()).unwrap();
        let conexao = discar(
            ponta.ponta(),
            vec![onde],
            None,
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("discar a sala");
        let outro =
            crate::enlace::Esperada::Codigo(seele_proto::codigo::Codigo::da_chave(&[7; 32]));
        let resultado = super::Client::connect_sobre(
            ponta,
            conexao,
            "sala",
            Some(&outro),
            false,
            "pessoa",
            &ed25519_dalek::SigningKey::from_bytes(&[5; 32]),
            Arc::new(MemoryPinStore::new()),
            None,
            None,
            None,
        )
        .await;
        assert!(
            matches!(
                resultado,
                Err(ConnectError::CodigoNaoConfere(
                    crate::confere_codigo::FalhaDoCodigo::CodigoNaoBate { .. }
                ))
            ),
            "quem atendeu tinha outro código e a ligação não foi recusada pelo código"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(500), veredito)
                .await
                .map_or(true, |lido| lido.is_err()),
            "o `Hello` saiu para uma sala de outro código"
        );
    }

    /// A apresentação de uma sala com a assinatura e a chave dela sobre `dtls`.
    fn apresentacao_de(semente: &ed25519_dalek::SigningKey, dtls: &[u8; 32]) -> Vec<u8> {
        use ed25519_dalek::Signer as _;
        let assinatura = semente.sign(&seele_proto::assinatura::apresentacao_da_sala(dtls));
        let mut apresentacao = semente.verifying_key().to_bytes().to_vec();
        apresentacao.extend_from_slice(&assinatura.to_bytes());
        apresentacao
    }

    /// **A sala pessoal pelo endereço entra, como na 0.16** (decisão de 16/10;
    /// ADR 0060: o pino do TOFU é por endereço). Sem código, a sala confere pelo
    /// TOFU da **chave** dela, e não do DTLS, que é efêmero desde a 0.17: a
    /// prova amarra a impressão da chave, que é a que a sala confere, e a
    /// segunda entrada, com outro DTLS, encontra o mesmo pino.
    ///
    /// Antes disto a prova ia amarrada ao DTLS efêmero, a sala a recusava, e a
    /// pessoa lia `CredentialRejected` (o achado da conformidade e do
    /// hosting-lifecycle).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_sala_pelo_endereco_entra_pelo_tofu_da_chave_e_a_volta_confere_com_outro_dtls() {
        let semente = ed25519_dalek::SigningKey::from_bytes(&[42; 32]);
        let impressao_da_sala =
            seele_proto::transport::certificate_digest(&semente.verifying_key().to_bytes());
        let pins: Arc<dyn PinStore> = Arc::new(MemoryPinStore::new());
        let pin_key = "sala.exemplo:8385";

        for vez in ["a primeira", "a segunda"] {
            // Um DTLS novo a cada abertura da sala, como no produto.
            let certificado = crate::transporte::CertificadoDtls::efemero().unwrap();
            let apresentacao = apresentacao_de(&semente, &certificado.impressao());
            let (onde, veredito) =
                anfitriao_que_confere_a_prova(apresentacao, certificado, impressao_da_sala);
            let ponta =
                crate::transporte::abrir_ponta(std::net::UdpSocket::bind("127.0.0.1:0").ok())
                    .unwrap();
            let conexao = discar(
                ponta.ponta(),
                vec![onde],
                None,
                std::time::Duration::from_secs(5),
            )
            .await
            .expect("discar a sala");
            let entrada = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                super::Client::connect_sobre(
                    ponta,
                    conexao,
                    pin_key,
                    None,
                    false,
                    "pessoa",
                    &ed25519_dalek::SigningKey::from_bytes(&[5; 32]),
                    Arc::clone(&pins),
                    None,
                    None,
                    None,
                ),
            )
            .await;
            if let Ok(Err(erro)) = &entrada {
                panic!("{vez} entrada pelo endereço da sala foi recusada: {erro:?}");
            }
            assert_eq!(
                tokio::time::timeout(std::time::Duration::from_secs(10), veredito)
                    .await
                    .ok()
                    .and_then(Result::ok),
                Some(true),
                "{vez} entrada pelo endereço não amarrou a prova à impressão da chave da sala"
            );
            assert_eq!(
                pins.pinned(pin_key),
                Some(hex(&impressao_da_sala)),
                "{vez} entrada fixou outra coisa que não a chave da sala"
            );
        }
    }

    /// Uma apresentação de sala que não assina este DTLS é recusada antes do
    /// `Hello`, e nada é fixado: sem isto, quem repete a apresentação de uma
    /// sala de verdade passaria pelo TOFU com a chave dela.
    #[test]
    fn a_apresentacao_que_nao_assina_este_dtls_e_recusada_e_nada_e_fixado() {
        let pins = loja();
        let semente = ed25519_dalek::SigningKey::from_bytes(&[42; 32]);
        let de_outro_dtls = apresentacao_de(&semente, &[1; 32]);
        let resultado =
            conferir_quem_atendeu(&de_outro_dtls, &[2; 32], "sala:8385", None, false, &pins);
        assert!(
            matches!(resultado, Err(ConnectError::CodigoNaoConfere(_))),
            "uma apresentação sobre outro DTLS passou: {:?}",
            resultado.map(|(decisao, _, _)| decisao)
        );
        assert_eq!(pins.pinned("sala:8385"), None, "a recusa fixou um pino");
    }

    #[test]
    fn a_frase_da_recusa_nao_conta_se_havia_link_pino_ou_codigo() {
        // A frase vai no corte que quem atendeu recebe. Uma por caso ensinaria a
        // um impostor qual defesa esta máquina tinha; o porquê fica no log
        // desta máquina e no erro com nome.
        let frase = String::from_utf8_lossy(super::RECUSA_NEUTRA).to_lowercase();
        for palavra in [
            "pin", "fix", "link", "convite", "impress", "esperad", "invite", "chang", "mudou",
            "troc", "promete", "codigo", "sala",
        ] {
            assert!(
                !frase.contains(palavra),
                "a frase da recusa diz «{palavra}» e conta a quem atendeu como esta \
                 máquina decidiu: {frase}"
            );
        }
    }

    #[test]
    fn connect_sobre_confere_antes_de_tocar_no_canal_de_controle() {
        // O `Hello` sai pelo `ctl`, e o `ctl` só é tirado da conexão depois da
        // conferência. Quem prende a ordem é o texto: as agulhas são montadas,
        // e não escritas, porque este é o arquivo que o teste lê.
        let fonte = include_str!("client.rs");
        let inicio = fonte
            .find(&format!("{} fn connect_sobre(", "pub(crate) async"))
            .expect("`connect_sobre` sumiu de `client.rs`");
        let depois = fonte.get(inicio..).expect("o começo de `connect_sobre`");
        let conferencia = depois
            .find(&format!("{}(", "conferir_quem_atendeu"))
            .expect("`connect_sobre` deixou de conferir quem atendeu");
        let controle = depois
            .find(&format!("connection.{}()", "controle"))
            .expect("`connect_sobre` deixou de tirar o canal de controle");
        assert!(
            conferencia < controle,
            "o canal de controle saiu antes da conferência: o `Hello` iria a quem \
             ninguém conferiu"
        );
    }

    #[test]
    fn o_fluxo_de_controle_escreve_no_numero_negociado() {
        // O `Hello` sai com a mínima desta build, e o resto da sessão no número
        // que o servidor carimbou. As agulhas são montadas, e não escritas:
        // este é o arquivo que o teste lê.
        let fonte: String = include_str!("client.rs").split_whitespace().collect();
        let hello = format!(
            "{}(send,seele_proto::version::{},&ClientMessage::Hello{{",
            "frame::write_em", "VERSAO_MINIMA"
        );
        assert!(
            fonte.contains(&hello),
            "o `Hello` deixou de sair carimbado com a mínima desta build"
        );
        for sem_carimbo in [
            format!("{}(send,", "frame::write"),
            format!("{}(&mutself.send,", "frame::write"),
        ] {
            assert!(
                !fonte.contains(&sem_carimbo),
                "o fluxo de controle voltou a escrever com o número desta build \
                 (`{sem_carimbo}`), e não com o que o servidor carimbou"
            );
        }
    }
}

/// Como este build se apresenta ao servidor, no `Hello`.
///
/// # Por que não `CARGO_PKG_VERSION`
///
/// Porque ele diz `0.0.0`, e dizia. A versão do workspace é `0.0.0` de
/// propósito — o número de verdade é escolhido no empacotamento —, então o que
/// atravessava o fio era `connection/0.0.0` de toda máquina do mundo, de toda
/// release. É o mesmo raciocínio que a pendência 30 já tinha escrito para o
/// `seeled --versao`, aplicado ao lugar onde ele valia igual e ninguém tinha
/// olhado.
///
/// `SEELE_VERSAO` é passada pelo empacotamento. Um build feito à mão não a tem
/// e **diz isso**, em vez de inventar um número: um cliente de
/// desenvolvimento se anunciando como `0.10.5` é pior que um que se anuncia
/// como local, porque o primeiro mente para quem está lendo o registro de um
/// servidor tentando descobrir quem está com qual build.
fn quem_sou_eu() -> &'static str {
    match option_env!("SEELE_VERSAO") {
        Some(versao) => versao,
        None => "connection/local",
    }
}

#[cfg(test)]
mod como_este_build_se_apresenta {
    #[test]
    fn a_apresentacao_nunca_e_a_versao_do_cargo_toml() {
        // O guarda irmão do que `seele-instalador` já tem, e pela mesma razão:
        // `0.0.0` é a versão do workspace e nunca foi a do produto. Um `Hello`
        // que a carrega enche o registro de todo servidor com um número que não
        // distingue build nenhum de build nenhum.
        assert!(
            !super::quem_sou_eu().contains("0.0.0"),
            "o Hello voltou a se apresentar com a versão do Cargo.toml"
        );
    }

    #[test]
    fn um_build_sem_carimbo_diz_que_e_local_em_vez_de_inventar() {
        // Nesta árvore, rodando o teste sem `SEELE_VERSAO`, é o que tem de sair.
        // Com a variável posta pelo empacotamento, sai o número — e aí este
        // teste não roda, porque quem empacota não roda testes.
        if option_env!("SEELE_VERSAO").is_none() {
            assert_eq!(super::quem_sou_eu(), "connection/local");
        }
    }
}

/// **A folga do prazo do toque** (tarefa 16 da F7): o que quem liga espera além
/// do `prazo_s` que a sala mandou no `Tocando`.
///
/// A sala manda «ninguém atendeu» no instante em que o prazo vence, e a razão
/// ainda tem de atravessar a rede. Sem folga, quem liga desistiria no mesmo
/// instante em que ela está a caminho, e a tela diria «tempo esgotado» onde a
/// sala disse por quê.
const FOLGA_DO_TOQUE: std::time::Duration = std::time::Duration::from_secs(5);

/// O prazo de quem ouviu `Tocando { prazo_s }`: o do toque, mais a
/// [`FOLGA_DO_TOQUE`].
fn prazo_do_toque(prazo_s: u16) -> std::time::Duration {
    std::time::Duration::from_secs(u64::from(prazo_s)) + FOLGA_DO_TOQUE
}

/// O aperto de mão até a prova da chave, e a primeira resposta depois dela, com
/// o número da sessão.
///
/// A resposta é a sessão, o anúncio de MODs, uma recusa, ou, numa sala pessoal,
/// `Tocando`: quem chama decide o prazo do resto, e o resto é [`concluir`].
async fn handshake(
    send: &mut crate::transporte::Envio,
    recv: &mut crate::transporte::Recebe,
    nickname: &str,
    signing_key: &SigningKey,
    impressao: &[u8; 32],
    impressao_dtls_local: &[u8; 32],
    join_secret: Option<&str>,
) -> Result<(ServerMessage, u8), ConnectError> {
    // **O `Hello` sai com a mínima desta build** (adendo de 08/10/2026, §4):
    // é o número que qualquer servidor da mesma linha lê. O da sessão é o que o
    // servidor carimbar na primeira resposta, logo abaixo.
    frame::write_em(
        send,
        seele_proto::version::VERSAO_MINIMA,
        &ClientMessage::Hello {
            faixa: seele_proto::version::FAIXA,
            // As APIs de MOD que esta build roda (D-fio-7). O servidor ainda
            // não as lê; quem lê é a sessão do Plano 3.
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
            client: quem_sou_eu().into(),
            nickname: nickname.to_owned(),
            public_key: signing_key.verifying_key().to_bytes().to_vec(),
            join_secret: join_secret.map(str::to_owned),
            material_e2ee: None,
        },
    )
    .await
    .map_err(|error| {
        tracing::warn!(%error, "could not send Hello");
        ConnectError::Unreachable
    })?;

    // **A primeira resposta é lida bruta**, e o byte 0 dela é conferido antes
    // do `decode` (ADR 0058). O `decode` recusaria um número de fora sem dizer
    // qual veio, e o erro virava `Unreachable`: a tela dizia «não foi possível
    // alcançar o servidor» de um servidor que tinha respondido — e respondido
    // justamente o que a pessoa precisava saber, a versão dele.
    let primeira = frame::ler_bruto(recv).await.map_err(|error| {
        tracing::warn!(%error, "no Challenge came back");
        ConnectError::Unreachable
    })?;
    let Some(&numero) = primeira.first() else {
        // `ler_bruto` recusa o quadro vazio antes; isto só fecha o `match`.
        return Err(ConnectError::ProtocolViolation);
    };
    // Aceito, o número é o da sessão: toda escrita daqui em diante sai com ele.
    let carimbo = seele_proto::version::confere_o_carimbo(numero).map_err(|recusa| {
        tracing::warn!(
            numero,
            %recusa,
            "o servidor respondeu num número de protocolo que esta build não fala"
        );
        ConnectError::OutraLinha { numero }
    })?;
    let challenge = seele_proto::control::decode::<ServerMessage>(&primeira).map_err(|error| {
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

    // **O cliente nunca assina bytes vindos do fio** (o S2, adendo de
    // 08/10/2026, §9). O nonce tem de ter exatamente 32 bytes, e o que se
    // assina é a mensagem que o `seele-proto` monta: o rótulo, a impressão do
    // servidor que esta conexão conferiu, a impressão DTLS **desta** ponta e o
    // nonce (D5 da 0.17). A impressão local amarra a prova a esta sessão DTLS:
    // o servidor confere que ela é a do certificado que viu, e uma prova
    // repassada por quem está no meio não vale noutra conexão.
    let Ok(nonce) = <[u8; seele_proto::control::NONCE_LEN]>::try_from(nonce.as_slice()) else {
        tracing::warn!(
            tamanho = nonce.len(),
            "o desafio não tem 32 bytes; o cliente não assina"
        );
        return Err(ConnectError::ProtocolViolation);
    };
    let assinada =
        seele_proto::assinatura::prova_de_cliente_2(impressao, impressao_dtls_local, &nonce);
    frame::write_em(
        send,
        carimbo,
        &ClientMessage::Response {
            proof: signing_key.sign(&assinada).to_bytes().to_vec(),
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
    Ok((answer, carimbo))
}

/// Depois de `Tocando`: a decisão de quem recebe, e o resto do aperto.
///
/// Atendida, a sala segue para a sessão; recusada ou vencida, ela manda a
/// razão (`ChamadaRecusada`, `NinguemAtendeu`), que [`concluir`] devolve como
/// [`ConnectError::Refused`].
async fn depois_do_toque(
    send: &mut crate::transporte::Envio,
    recv: &mut crate::transporte::Recebe,
    carimbo: u8,
    aceito: Option<&str>,
) -> Result<(SessionInfo, u8), ConnectError> {
    let answer = frame::read::<ServerMessage>(recv).await.map_err(|error| {
        tracing::warn!(%error, "nada veio depois do Tocando");
        ConnectError::Unreachable
    })?;
    concluir(send, recv, carimbo, aceito, answer).await
}

/// O fim do aperto de mão: o anúncio de MODs, quando há, e a sessão.
async fn concluir(
    send: &mut crate::transporte::Envio,
    recv: &mut crate::transporte::Recebe,
    carimbo: u8,
    aceito: Option<&str>,
    mut answer: ServerMessage,
) -> Result<(SessionInfo, u8), ConnectError> {
    // O anúncio de MODs, quando o servidor exige algum. ADR 0045.
    //
    // # Por que a decisão não é tomada aqui
    //
    // Aceitar é de uma pessoa, e um aperto de mão não tem como perguntar a
    // ninguém: ele corre dentro de um prazo, sem tela e sem ninguém olhando.
    // O que este código faz é comparar com o que **já foi** aceito, e devolver
    // a lista para a casca quando não bate.
    //
    // É a mesma forma do TOFU dos pins (ADR 0003): o núcleo confere contra o
    // que está guardado, e quem decide o que guardar é quem tem uma tela.
    if let ServerMessage::ModsExigidos { mods, conjunto } = answer {
        if aceito == Some(conjunto.as_str()) {
            frame::write_em(
                send,
                carimbo,
                &ClientMessage::AceitarMods {
                    conjunto: conjunto.clone(),
                },
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, "could not send AceitarMods");
                ConnectError::Unreachable
            })?;
            answer = frame::read::<ServerMessage>(recv).await.map_err(|error| {
                tracing::warn!(%error, "no Session came back after accepting the MODs");
                ConnectError::Unreachable
            })?;
        } else {
            // **Recusa explícita, e não sumir.** Quem hospeda precisa distinguir
            // «leu e não quis» de «a conexão caiu» — ver
            // `ClientMessage::RecusarMods`. O erro devolvido não é uma queixa:
            // é a pergunta, com o que a casca precisa para fazê-la.
            let _ = frame::write_em(send, carimbo, &ClientMessage::RecusarMods).await;
            // **E uma recusa que não chega não é uma recusa.**
            //
            // Escrever não é entregar: logo abaixo esta função devolve erro, e
            // quem a chamou larga o `Endpoint` — que é dono do socket. O quadro
            // escrito e não confirmado morre com ele, e quem hospeda vê um
            // aperto de mão que sumiu no meio, que é a leitura errada.
            //
            // Encontrado por teste, e não por leitura: o servidor de mentira de
            // `aceite_dos_mods.rs` recebia a recusa numa corrida e não na
            // outra. No QUIC, `stopped` voltava quando o outro lado confirmava
            // tudo depois do `finish`. O canal do enlace não tem essa
            // confirmação, então se espera a **reação** de quem hospeda: ele
            // lê a recusa e encerra o `ctl`, e o fim (ou a queda) chega aqui.
            // O prazo existe para que um servidor que não lê nunca prenda quem
            // está saindo.
            let _ = send.finish();
            let _ = tokio::time::timeout(ESPERA_DA_RECUSA, async {
                let mut sobra = [0_u8; 64];
                while let Ok(Some(_)) = recv.read(&mut sobra).await {}
            })
            .await;
            tracing::info!(
                %conjunto,
                quantos = mods.len(),
                "o servidor exige um conjunto de MODs que esta máquina não aceitou"
            );
            return Err(ConnectError::ModsNaoAceitos { mods, conjunto });
        }
    }

    match answer {
        ServerMessage::Session {
            id,
            person,
            ssrc,
            server,
            voice_rooms,
            channels,
            permissions,
            ..
        } => Ok((
            SessionInfo {
                id,
                person,
                ssrc,
                server,
                voice_rooms,
                channels,
                permissions,
            },
            carimbo,
        )),
        ServerMessage::Disconnecting { reason } => Err(ConnectError::Refused { reason }),
        other => {
            tracing::warn!(?other, "expected Session");
            Err(ConnectError::ProtocolViolation)
        }
    }
}

/// The file-moving half of a connection, usable while the control stream is busy.
///
/// The same shape as [`MediaChannel`], and for the same reason: ADR 0027 gives
/// every transfer a stream of its own precisely so that moving twenty megabytes
/// does not stop anything else, and an API that made a caller hold the whole
/// client for the duration would hand that back at the last step.
///
/// Cloneable, so a shell can put a transfer on a task of its own.
#[derive(Clone)]
pub struct Transfers {
    connection: crate::transporte::Conexao,
    /// A fila que o roteador de `Client::connect` alimenta.
    anexos: FilaDeAnexos,
}

impl Transfers {
    /// Imagens de MOD por fluxo próprio; a resposta pertence ao mesmo fluxo.
    pub async fn imagem_de_mod(
        &self,
        pedido: seele_proto::volume::PedidoDeImagem,
        bytes: &[u8],
    ) -> Result<Vec<u8>> {
        use seele_proto::volume::{PedidoDeImagem, RespostaDeImagem};
        let inicio = std::time::Instant::now();
        let (mut envio, mut entrada) = self.connection.open_bi().await?;
        envio.set_priority(TRANSFER_PRIORITY)?;
        frame::write(&mut envio, &pedido).await?;
        if let PedidoDeImagem::Enviar { bytes: total, .. } = &pedido {
            anyhow::ensure!(*total == bytes.len() as u64, "tamanho de imagem mudou");
            for bloco in bytes.chunks(seele_proto::attachment::BLOCK_LEN) {
                envio.write_all(bloco).await?;
            }
        }
        envio.finish()?;
        let resposta: RespostaDeImagem = frame::read(&mut entrada).await?;
        anyhow::ensure!(
            resposta.erro.is_none(),
            "{}",
            resposta.erro.unwrap_or_default()
        );
        let lidos = entrada
            .read_to_end(seele_proto::midia_de_mod::TETO_DE_ARQUIVO)
            .await?;
        // O nome da lista `recusas.volume`, e não uma frase: ele chega ao MOD
        // em `e.erro` (a auditoria do `v6.json`, B2).
        anyhow::ensure!(lidos.len() as u64 == resposta.bytes, "incompleto");
        tracing::info!(
            enviados = bytes.len(),
            recebidos = lidos.len(),
            em_ms = inicio.elapsed().as_millis(),
            "imagem de MOD transferida por fluxo"
        );
        Ok(lidos)
    }

    /// O próximo fluxo de anexo da fila do roteador, dentro do prazo.
    ///
    /// **A fila é uma só e as tarefas de anexo são várias.** Duas transferências
    /// ao mesmo tempo podem trocar de fluxo aqui — e o conserto está logo
    /// abaixo, em quem chama: o `AttachmentDelivery` diz de qual anexo o fluxo
    /// é, e um que não seja o pedido é recusado por nome. Era assim antes do
    /// roteador, com o `accept_uni` no lugar desta fila, e continua sendo:
    /// endereçar fluxo por identificador é trabalho de outro dia, e não do dia
    /// em que a tela passou a ter para onde ir.
    async fn proximo_fluxo(&self, wait: std::time::Duration) -> Result<crate::transporte::Recebe> {
        let mut fila = self.anexos.lock().await;
        tokio::time::timeout(wait, fila.recv())
            .await
            .map_err(|_| anyhow::anyhow!("o servidor não mandou o arquivo em {wait:?}"))?
            .ok_or_else(|| anyhow::anyhow!("a conexão acabou antes do arquivo"))
    }

    /// Sends a file on a stream of its own, and reports the bytes as they go.
    ///
    /// **Never on the control stream.** Twenty megabytes written there stop
    /// every presence event and every channel of text from everybody behind them
    /// until the last byte goes through; the control stream is the one that may
    /// not queue.
    ///
    /// The message is published on the Channel **only once the bytes have
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
        &self,
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
        // O tipo do fluxo, antes de tudo. Um servidor recebe dois tipos de fluxo
        // unidirecional — anexo e tela — e até a 0.7.8 o que os separava era
        // aritmética sobre o primeiro byte do cabeçalho. O §5.2 da spec do
        // compartilhamento de tela conta a dívida e este byte é o pagamento
        // dela; sem escrevê-lo aqui, o servidor lê o cabeçalho um byte adiante
        // e recusa o anexo inteiro.
        stream
            .write_all(&[seele_proto::stream::StreamType::Attachment.byte()])
            .await?;
        let header = AttachmentHeader {
            channel: request.channel,
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
            // A write that fails is not this end's failure. The server refusing
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
        //
        // `Ok(Some(code))` is the peer having sent `STOP_SENDING`, which is a
        // refusal that happened to arrive after the last block went out. Reading
        // it as success was a real defect and an intermittent one: on a small
        // file the whole thing fits in one write, so whether the refusal shows
        // up as a failed `write_all` or as a stop code here is a race with the
        // network. Only the channel read to its end is delivery.
        Ok(crate::transporte::esperar_a_entrega(&mut stream, sent).await)
    }

    /// Envia bytes para o volume de um MOD, por um fluxo próprio — ADR 0048.
    ///
    /// # Por que um fluxo, e não a ponte de pedidos
    ///
    /// A ponte existe para «uma mensagem de texto e um `Ping` a cada cinco
    /// segundos». Empurrar uma imagem por ela custava 2 331 pedidos para
    /// 10 MiB, cada um atravessando o QuickJS, a vinte quadros por segundo. Por
    /// fluxo, os mesmos 10 MiB passam dentro da rajada de bytes, na velocidade
    /// do enlace, e **o QuickJS não vê byte nenhum**.
    ///
    /// # O que vai na frente, e o que não vai
    ///
    /// Dois nomes: de quem é a pasta, e qual autorização isto usa. Não vai
    /// caminho — quem o escolhe é o MOD, e ele já está guardado na espera —,
    /// não vai tamanho e não vai tipo. O porquê de cada ausência está em
    /// [`seele_proto::volume`].
    ///
    /// # Errors
    ///
    /// Falha quando o fluxo não abre ou o cabeçalho não sai. Uma **recusa** não
    /// é erro: ela volta pelo controle, como
    /// `ServerMessage::VolumeRecusado`, com o motivo enumerado — que é onde
    /// `specs/02-protocolo.md` guarda todo motivo.
    pub async fn enviar_volume(
        &self,
        mod_id: &str,
        token: &str,
        bytes: &[u8],
        mut progress: impl FnMut(u64, u64),
    ) -> Result<Sent> {
        use seele_proto::attachment::BLOCK_LEN;

        let mut stream = self.connection.open_uni().await?;
        // Abaixo do controle, como o anexo e pela mesma razão: mandar uma
        // imagem não pode atrasar o `Pong` de ninguém nesta conexão.
        stream.set_priority(TRANSFER_PRIORITY)?;
        stream
            .write_all(&[seele_proto::stream::StreamType::ModVolume.byte()])
            .await?;
        let header = seele_proto::volume::VolumeHeader {
            mod_id: mod_id.to_owned(),
            token: token.to_owned(),
        };
        if let Err(error) = frame::write(&mut stream, &header).await {
            return Ok(interpret_any(&error, 0));
        }

        let total = bytes.len() as u64;
        let mut enviados = 0_u64;
        progress(0, total);
        for bloco in bytes.chunks(BLOCK_LEN) {
            if let Err(error) = stream.write_all(bloco).await {
                return Ok(interpret(&error, enviados));
            }
            enviados = enviados.saturating_add(bloco.len() as u64);
            progress(enviados, total);
        }
        if stream.finish().is_err() {
            return Ok(Sent::Stopped { bytes: enviados });
        }
        // Espera o outro lado reconhecer o fim, pela razão escrita em
        // `send_attachment`: sem isto, um `STOP_SENDING` que chega depois do
        // último bloco seria lido como entrega.
        Ok(crate::transporte::esperar_a_entrega(&mut stream, enviados).await)
    }

    /// Recebe os bytes de um anexo no parcial que quem chama acabou de criar, e
    /// dá a ele o nome final quando o hash confere.
    ///
    /// **Esta função não escolhe caminho nenhum.** `arquivo` chega já aberto no
    /// parcial, `.foto.png.seele-parcial`, criado por
    /// [`crate::anexo_no_disco::abrir_parcial`] — que é onde mora a regra de que
    /// nome um anexo pode ganhar no disco, de que ele só o ganha depois de
    /// chegar inteiro e de que nada que já existe é substituído. O ADR 0027 não dá a cliente
    /// nenhum do SEELE um botão que abre arquivo: salvar é um ato de quem
    /// recebeu.
    ///
    /// Os bytes chegam no parcial, e só com o hash conferido o arquivo ganha o
    /// nome final, por [`crate::anexo_no_disco::Parcial::nomear`]: um processo
    /// que morre no meio deixa o parcial, e nunca um arquivo truncado com o
    /// nome de um completo. Volta o caminho real, que é o que a tela mostra:
    /// «foto (2).png», se foi lá que ele ficou.
    ///
    /// The file is marked with the operating system's own quarantine on the way
    /// down — `com.apple.quarantine`, or the `Zone.Identifier` stream — so that
    /// Gatekeeper and SmartScreen stop it in front of whoever opens it later.
    /// That is not antivirus, and this product does not have one; it is the
    /// guard the system already has, and it only works if whoever writes the
    /// file turns it on. A marca entra no parcial, **antes** de ele ganhar o nome
    /// final: o link e a troca de nome levam junto o que é do arquivo, e assim o
    /// arquivo com os bytes nunca tem o nome final sem ela — quando o sistema de
    /// arquivos guarda a marca. Um FAT ou exFAT no Windows não tem onde guardar
    /// o fluxo `Zone.Identifier`, e lá o arquivo fica sem marca nenhuma; e no
    /// recuo de um volume sem link físico a reserva tem o nome final sem a marca
    /// por um instante, vazia, até a troca pôr o parcial no lugar dela.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written, if the bytes do not hash to what the
    /// server said, if nothing arrives inside `wait`, or if every name it may
    /// take is taken — and in all of them the partial file is removed, by
    /// dropping the [`crate::anexo_no_disco::Parcial`]. A refusal arrives
    /// separately, as [`ServerMessage::AttachmentUnavailable`] on the control
    /// stream — that is where the enumerated reason lives, and the expected one
    /// is `Expired`.
    pub async fn receive_attachment(
        &self,
        attachment: seele_proto::ids::AttachmentId,
        arquivo: std::fs::File,
        parcial: crate::anexo_no_disco::Parcial,
        wait: std::time::Duration,
        progress: impl FnMut(u64, u64),
    ) -> Result<(u64, std::path::PathBuf)> {
        // Numa falha, o `?` solta o `parcial`, e é soltá-lo que apaga o arquivo.
        // `receber_no_arquivo` recebeu o arquivo por valor e já o soltou ao
        // voltar. Soltar um `tokio::fs::File` não espera uma escrita que ainda
        // esteja terminando no pool de bloqueio — numa falha de leitura no
        // meio do download pode haver uma —, e isso não impede o apagar. No Mac
        // e no Linux, apagar um arquivo aberto tira o nome dele da pasta na
        // hora. No Windows o std abre com `FILE_SHARE_DELETE`, então o apagar é
        // aceito com ele aberto, e o arquivo sai no máximo quando a escrita
        // termina e solta o último handle.
        let got = self
            .receber_no_arquivo(
                attachment,
                tokio::fs::File::from_std(arquivo),
                wait,
                progress,
            )
            .await?;
        // A marca e o nome final são chamadas ao disco, e o `xattr` é um
        // processo: fora da tarefa assíncrona, como a abertura do parcial.
        let caminho = tokio::task::spawn_blocking(move || {
            quarantine(parcial.caminho());
            parcial.nomear()
        })
        .await??;
        Ok((got, caminho))
    }

    /// O miolo de [`Self::receive_attachment`]: espera o fluxo e grava em `file`.
    ///
    /// Ele não vê o parcial, só o arquivo aberto: toda falha daqui volta pelo
    /// `?` de cima, que solta o [`crate::anexo_no_disco::Parcial`], e é soltá-lo
    /// que apaga o arquivo.
    async fn receber_no_arquivo(
        &self,
        attachment: seele_proto::ids::AttachmentId,
        mut file: tokio::fs::File,
        wait: std::time::Duration,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<u64> {
        use seele_proto::attachment::{AttachmentDelivery, ContentDigest, BLOCK_LEN};
        use tokio::io::AsyncWriteExt;

        // The server opens the stream, so this waits for it. A refusal never
        // opens one, which is why there is a deadline: the reason is on its way
        // through `next_event` and this call must not hang for ever waiting for
        // bytes that were never coming.
        //
        // Da fila do roteador e não do `accept_uni`: o byte de tipo já foi lido
        // por ele, e é ele que garante que o que sai daqui é anexo e não a tela
        // de alguém que começou a transmitir no meio do download.
        let mut stream = self.proximo_fluxo(wait).await?;

        let delivery: AttachmentDelivery = frame::read(&mut stream).await?;
        anyhow::ensure!(
            delivery.attachment == attachment,
            "o servidor mandou o anexo {} no lugar de {attachment}",
            delivery.attachment
        );

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

        // The same question the server asked on the way in: did it arrive whole.
        // It says nothing about the file being good, and nothing here pretends
        // it does. Quem apaga o parcial é `receive_attachment`, na volta.
        anyhow::ensure!(
            digest.finish() == delivery.content_hash && got == delivery.byte_size,
            "o arquivo não chegou inteiro e foi descartado"
        );
        Ok(got)
    }

    /// Fetches an attachment **into memory**, small enough to look at.
    ///
    /// The other half of ADR 0027's preview rule: before anything can decide
    /// whether the bytes agree with the claim, somebody has to have the bytes.
    ///
    /// **Nothing is written to disk, at any point.** That is the channel between a
    /// preview and a save, and it is drawn here rather than left to a caller:
    /// saving is an act of the person who received the file, confirmed with the
    /// place written out in front of them, and a thumbnail that quietly left a
    /// copy in a cache directory would have made that act happen without
    /// anybody asking for it. No
    /// quarantine mark either, for the same reason — there is no file to mark.
    ///
    /// `limit` is checked against the **declared** size in the header, before a
    /// single byte of the body is read, and the stream is stopped rather than
    /// drained. It is the same shape as the server's own ceiling check, for the
    /// same reason: reading twenty megabytes and then deciding not to look at
    /// them costs the whole download.
    ///
    /// # Errors
    ///
    /// Fails if nothing arrives inside `wait`, if the stream ends early, or if
    /// the bytes do not hash to what the server said. A refusal — the expected
    /// one being `Expired` — arrives separately on the control stream.
    pub async fn preview_attachment(
        &self,
        attachment: seele_proto::ids::AttachmentId,
        limit: u64,
        wait: std::time::Duration,
    ) -> Result<Previewed> {
        use seele_proto::attachment::{AttachmentDelivery, ContentDigest, BLOCK_LEN};

        let mut stream = self.proximo_fluxo(wait).await?;

        let delivery: AttachmentDelivery = frame::read(&mut stream).await?;
        anyhow::ensure!(
            delivery.attachment == attachment,
            "o servidor mandou o anexo {} no lugar de {attachment}",
            delivery.attachment
        );

        if delivery.byte_size > limit {
            // Stopped, not drained: telling the sender to stop is what keeps a
            // file nobody is going to look at off this link.
            let _ = stream.stop(0);
            return Ok(Previewed::TooBig {
                byte_size: delivery.byte_size,
            });
        }

        let capacity = usize::try_from(delivery.byte_size).unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        let mut digest = ContentDigest::new();
        let mut block = vec![0_u8; BLOCK_LEN];
        while let Some(read) = stream.read(&mut block).await? {
            if read == 0 {
                break;
            }
            let piece = block.get(..read).unwrap_or_default();
            bytes.extend_from_slice(piece);
            digest.feed(piece);
            // A header that declared one size and a body that keeps coming is
            // the one way the limit above could be walked past. It cannot.
            anyhow::ensure!(
                u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= limit,
                "o servidor mandou mais bytes do que declarou"
            );
        }

        // The same question as on the way down to disk, and it is asked here
        // too: half a picture decodes into something, and something is worse
        // than nothing.
        anyhow::ensure!(
            digest.finish() == delivery.content_hash
                && u64::try_from(bytes.len()).unwrap_or(u64::MAX) == delivery.byte_size,
            "o arquivo não chegou inteiro e foi descartado"
        );
        Ok(Previewed::Whole(bytes))
    }
}

/// What came back from a preview fetch.
///
/// [`Previewed::TooBig`] is not an error and is enumerated for it: nothing went
/// wrong, the file is simply larger than a window will decode, and whoever is
/// looking deserves a different sentence from the one a broken transfer gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Previewed {
    /// The bytes, whole, hashed, and in memory only.
    Whole(Vec<u8>),
    /// Over the limit. Nothing was read, and the stream was stopped.
    TooBig {
        /// What the server said it would have sent.
        byte_size: u64,
    },
}

#[cfg(test)]
pub(crate) mod testes_do_socket {
    use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
    use std::time::Duration;

    use super::{pilha_dupla, socket_conferido};

    /// Quantos sockets IPv4 o teste põe de pé em `127.0.0.1` para o sistema
    /// cair na porta de um deles.
    const LADROES: usize = 64;

    /// Quantas propostas de pilha dupla se pedem ao sistema, no máximo — o
    /// teto do laço, que acaba aqui também onde a colisão nunca acontece.
    ///
    /// Onde o sistema deixa a colisão, cada proposta cai numa porta de ladrão
    /// com chance de `LADROES` em 16 384 (o intervalo efêmero do macOS): a
    /// espera é de 16 384 / 64 = 256, e a chance de nenhuma das 8 192 cair é de
    /// e^-32. Por isso nenhuma em 8 192 é a medida de que o sistema recusa.
    const PROPOSTAS: usize = 8_192;

    /// O que o sistema fez com as propostas de pilha dupla, com `LADROES`
    /// sockets IPv4 presos em `127.0.0.1`.
    pub(crate) enum Encenacao {
        /// Uma proposta caiu na porta de um ladrão: o sistema deixa os dois
        /// conviverem, e o defeito pode acontecer aqui.
        Colisao {
            /// O socket IPv4 que fica com o que for mandado a `127.0.0.1`
            /// naquela porta.
            ladrao: UdpSocket,
            /// A proposta, aberta exatamente como
            /// [`super::proposta_do_sistema`] a abre, pelo sistema e não por
            /// escolha do teste.
            proposta: socket2::Socket,
        },
        /// Nenhuma das propostas caiu numa porta presa: a escolha de porta do
        /// sistema confere as duas famílias, e o defeito não acontece aqui.
        Recusa {
            /// Os ladrões, ainda de pé.
            ladroes: Vec<UdpSocket>,
            /// Quantas propostas o sistema deu sem cair em nenhum.
            propostas: usize,
        },
    }

    /// Tenta encenar o que o macOS faz por conta própria, de vez em quando, a
    /// um `[::]:0`: dar-lhe a porta de um socket IPv4 em `127.0.0.1`.
    ///
    /// Acaba sempre, em no máximo [`PROPOSTAS`] pedidos: com a colisão onde o
    /// sistema a deixa acontecer, e com a recusa medida onde não deixa.
    ///
    /// # Onde o sistema deixa
    ///
    /// Medido no macOS (lote Y da F5): a escolha de porta para `[::]:0` de
    /// pilha dupla confere só os sockets IPv6. Com 4 000 sockets em
    /// `127.0.0.1:0` de pé, 992 de 4 000 `[::]:0` caíram numa porta deles; na
    /// ordem inversa, nenhum. E o datagrama para `127.0.0.1:X` vai ao mais
    /// específico dos dois, o IPv4. Linux e Windows não foram medidos: é o CI
    /// deles que diz em qual braço caem.
    pub(crate) fn encenar_a_colisao() -> Encenacao {
        encenar_a_colisao_com(Ipv4Addr::LOCALHOST)
    }

    /// [`encenar_a_colisao`] com os ladrões ligados a `endereco`: um endereço
    /// da máquina, ou o curinga `0.0.0.0`.
    ///
    /// O curinga também rouba (medido no macOS: 535 colisões em 3 000, e a
    /// resposta a `127.0.0.1:X` foi ao IPv4 curinga 20 de 20), e com uma
    /// diferença que pesa no conserto: a ligação explícita a `[::]:X` **não** é
    /// recusada por ele.
    pub(crate) fn encenar_a_colisao_com(endereco: Ipv4Addr) -> Encenacao {
        let mut ladroes: Vec<UdpSocket> = (0..LADROES)
            .filter_map(|_| UdpSocket::bind((endereco, 0)).ok())
            .collect();
        let portas: Vec<u16> = ladroes
            .iter()
            .filter_map(|ladrao| ladrao.local_addr().ok().map(|onde| onde.port()))
            .collect();
        let mut propostas = 0;
        while propostas < PROPOSTAS {
            let Ok(proposta) = pilha_dupla(0) else {
                break;
            };
            propostas += 1;
            let porta = proposta
                .local_addr()
                .ok()
                .and_then(|onde| onde.as_socket())
                .map(|onde| onde.port());
            if let Some(qual) = portas.iter().position(|&dela| Some(dela) == porta) {
                return Encenacao::Colisao {
                    ladrao: ladroes.swap_remove(qual),
                    proposta,
                };
            }
        }
        Encenacao::Recusa { ladroes, propostas }
    }

    /// Se a ligação **explícita** de pilha dupla na porta de `ladrao` é
    /// recusada — a regra em que [`socket_conferido`] se apoia onde o sistema
    /// deixa a colisão.
    pub(crate) fn a_ligacao_explicita_e_recusada(ladrao: &UdpSocket) -> bool {
        let porta = ladrao.local_addr().expect("porta do ladrão").port();
        matches!(
            pilha_dupla(porta),
            Err(erro) if erro.kind() == std::io::ErrorKind::AddrInUse
        )
    }

    /// O socket de quem conecta não divide a porta com um socket IPv4: o que o
    /// servidor manda a `127.0.0.1` naquela porta chega a ele.
    ///
    /// A metade de baixo de
    /// `enlace::tests::a_volta_avisa_a_voice_room_em_que_o_motor_sentou_a_pessoa_de_novo`,
    /// sem o QUIC e sem os 20 s. Roda nos três sistemas, e mede alguma coisa
    /// em cada um:
    /// - onde o sistema deixa a colisão (o macOS), a proposta que colide é a
    ///   primeira que [`socket_conferido`] recebe, e o datagrama do
    ///   «servidor» diz para onde a resposta iria;
    /// - onde o sistema recusa, a recusa fica medida ([`PROPOSTAS`] propostas
    ///   sem cair numa porta presa), e o socket aberto pelo caminho do produto,
    ///   com os ladrões de pé, recebe a resposta.
    ///
    /// Duas vezes: com os ladrões em `127.0.0.1` e no curinga `0.0.0.0`.
    #[test]
    fn o_socket_de_quem_conecta_nao_divide_a_porta_com_um_socket_ipv4() {
        for endereco in [Ipv4Addr::LOCALHOST, Ipv4Addr::UNSPECIFIED] {
            println!("ladrões em {endereco}:");
            nao_divide_com_ladroes_em(endereco);
        }
    }

    fn nao_divide_com_ladroes_em(endereco: Ipv4Addr) {
        match encenar_a_colisao_com(endereco) {
            // Se este sistema também aceitasse a ligação explícita na porta
            // presa, `socket_conferido` devolveria a mesma porta, e é a
            // resposta que reprova abaixo.
            Encenacao::Colisao { ladrao, proposta } => {
                let mut propostas = vec![proposta];
                let socket =
                    socket_conferido(|| propostas.pop().map_or_else(|| pilha_dupla(0), Ok))
                        .expect("o socket de quem conecta");
                assert!(
                    propostas.is_empty(),
                    "a proposta que colide não foi usada, e o teste mediu outra coisa"
                );
                a_resposta_chega(&socket, &[&ladrao]);
            }
            Encenacao::Recusa { ladroes, propostas } => {
                assert!(
                    propostas > 0,
                    "o sistema não abriu nenhum socket de pilha dupla: o teste não mediu nada"
                );
                // A ligação explícita fica só dita: onde a escolha de porta já
                // recusa, o produto está a salvo com ela ou sem ela, e reprovar
                // por ela seria reprovar o sistema por estar certo.
                let explicita = ladroes.first().map(a_ligacao_explicita_e_recusada);
                println!(
                    "a escolha de porta recusa a colisão: nenhuma de {propostas} propostas de \
                     pilha dupla caiu numa das {} portas IPv4 presas (a ligação explícita numa \
                     delas é recusada: {explicita:?})",
                    ladroes.len()
                );
                let socket = super::socket_de_quem_conecta().expect("o socket de quem conecta");
                a_resposta_chega(&socket, &ladroes.iter().collect::<Vec<_>>());
            }
        }
    }

    /// O que um «servidor» em `127.0.0.1` manda à porta de `socket` chega a
    /// ele, e não a um dos `ladroes`.
    ///
    /// Reprova dizendo as portas: quando o defeito volta, a de quem conecta é
    /// a de um ladrão.
    pub(crate) fn a_resposta_chega(socket: &UdpSocket, ladroes: &[&UdpSocket]) {
        let porta = socket.local_addr().expect("porta de quem conecta").port();
        let do_ladrao = ladroes
            .iter()
            .filter_map(|ladrao| ladrao.local_addr().ok().map(|onde| onde.port()))
            .find(|&dele| dele == porta);
        let servidor = UdpSocket::bind("127.0.0.1:0").expect("servidor");
        servidor
            .send_to(b"resposta", SocketAddr::from(([127, 0, 0, 1], porta)))
            .expect("mandar a resposta");
        // O prazo só pesa quando a resposta foi para outro lugar: no loopback,
        // o datagrama que chega, chega na hora.
        socket.set_nonblocking(false).expect("bloqueante");
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("prazo");
        let mut buffer = [0_u8; 16];
        let chegou = socket.recv_from(&mut buffer).map(|(lidos, _)| lidos);
        assert_eq!(
            chegou.ok(),
            Some(b"resposta".len()),
            "a resposta mandada a 127.0.0.1:{porta} não chegou ao socket de quem conecta \
             (a porta de um socket IPv4 preso que é a mesma: {do_ladrao:?}): o QUIC deste \
             lado não ouve o aperto de mão e desiste pela ociosidade, 20 s depois"
        );
    }
}
