//! O retrato da sala: a parte do `Snapshot` que sai do `Room` (Tarefa 2.2 da
//! F4, fase 2).
//!
//! Os tipos de vista e a projeção moravam no `seele-ffi`, ao lado do
//! `Snapshot`. O navegador monta o mesmo retrato a partir das mesmas mensagens
//! (ADR 0057), e uma segunda projeção escrita lá seria a segunda implementação
//! que o ADR proíbe. Por isso os dois produtores, o `seele-ffi` e o
//! `seele-web`, chamam [`foto_da_sala`], e os tipos saem daqui. O `seele-ffi`
//! os reexporta pelo `seele-core`, e o caminho deles na casca não mudou.
//!
//! **Só o que sai do `Room`.** O que cada ponta mede (o enlace, o caminho, o
//! áudio, o sinal) fica com quem mede, e o `Snapshot` junta as duas partes.
//!
//! Os tipos, as conversões e as seis funções da projeção foram movidos byte a
//! byte, trocando só os caminhos (`seele_core::X` pelo `seele_proto` ou pelo
//! `crate` de onde o `seele-core` os reexporta). A [`FotoDaSala`] é o `Foto`
//! que o `Connection::snapshot` montava dentro dele.

use seele_proto::control::{Permission, TemaDoServidor};
use seele_proto::ids::{PersonId, ScreenId};

use crate::state::Room;

/// Which band a Sync Ratio falls into.
///
/// Carried as an enum beside the number so a shell never has to know the
/// thresholds. Two shells with two copies of "85 is nominal" is two shells that
/// disagree the day one of them is updated.
///
/// Three bands, from `comp v2`. The fourth — `Acceptable`
/// — is gone, and it is gone from the JSON the webview reads too: a shell that
/// still branches on `"Acceptable"` now falls through to whatever it does with
/// an unknown band, and never to a colour it drew last week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum SignalBand {
    /// 85 and above.
    Nominal,
    /// 60 to 84.
    Degraded,
    /// Below 60.
    ///
    /// The default, because an unmeasured ratio is zero and zero is critical by
    /// the thresholds the comp sets. Defaulting to nominal would be a
    /// reassurance nobody measured — worse than an alarm, because an alarm gets
    /// checked.
    #[default]
    Critical,
}

impl From<seele_proto::signal::SignalBand> for SignalBand {
    fn from(band: seele_proto::signal::SignalBand) -> Self {
        match band {
            seele_proto::signal::SignalBand::Nominal => Self::Nominal,
            seele_proto::signal::SignalBand::Degraded => Self::Degraded,
            seele_proto::signal::SignalBand::Critical => Self::Critical,
        }
    }
}

/// How loud a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    /// Worth knowing.
    Info,
    /// Something is degrading.
    Warning,
    /// Something is wrong.
    Critical,
}

impl From<seele_proto::control::AlertSeverity> for Severity {
    fn from(severity: seele_proto::control::AlertSeverity) -> Self {
        match severity {
            seele_proto::control::AlertSeverity::Info => Self::Info,
            seele_proto::control::AlertSeverity::Warning => Self::Warning,
            seele_proto::control::AlertSeverity::Critical => Self::Critical,
        }
    }
}

/// What a notice is about. Enumerated so each shell writes its own sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum NoticeReason {
    /// The person was named in a message.
    Mentioned,
    /// A subsystem changed health.
    SubsystemChanged,
    /// The connection is struggling.
    SyncDegraded,
    /// Entry to a voice room was refused.
    VoiceRoomEntryRefused,
    /// The action needed a permission the person lacks.
    PermissionDenied,
    /// The voice room is at its limit.
    VoiceRoomFull,
    /// The operator is saying something.
    OperatorNotice,
    /// The client is sending control frames faster than its budget.
    RateLimited,
    /// An operator moved this person's connection into another voice room.
    MovedByOperator,
    /// The voice room this person's connection was in no longer exists.
    VoiceRoomDeleted,
    /// A Channel this person had open no longer exists.
    ChannelDeleted,
    /// The voice room asked about is the only one the server has, so it stays.
    LastVoiceRoom,
    /// Somebody else is already sharing their screen in this room.
    ///
    /// One share per voice room (§6.3 da spec do compartilhamento de tela), and
    /// this is what whoever lost the race is told. Not a permission problem —
    /// the person may share the moment the other one stops — and that is why it
    /// is its own reason instead of `PermissionDenied`, which would say the
    /// wrong thing and send somebody looking for a role they already have.
    ScreenShareTaken,
    /// O servidor parou a transmissão desta pessoa: a sala cresceu além da subida
    /// de quem hospeda.
    ///
    /// §5.1 pôs o caminho de quem hospeda dentro do teto — `caminho × 60% ÷ N` —
    /// porque é o servidor que sobe as N cópias. Passado certo N nem o piso do §2
    /// cabe, e o §3.2 diz o que acontece então: *quem para é o vídeo*.
    ///
    /// Razão própria, e não [`Self::SyncDegraded`], que toda casca escreve como
    /// «sinal em queda» — uma frase sobre a conexão de quem lê, na frente de
    /// alguém cuja conexão está boa e cuja plateia cresceu. E não um
    /// `ScreenShareStopped` mudo: quem apertou parar sabe que apertou, e quem
    /// foi parado pelo servidor não ficaria sabendo de nada.
    ScreenShareOverHostUplink,

    /// A sala cresceu além do que a subida medida desta máquina comporta.
    ///
    /// Os dois números viajam porque «a sala está grande» não é acionável e
    /// «precisa de 6,5 Mbps, e a medida é 4 Mbps» é. A casca escreve a frase;
    /// aqui só chegam os valores. Ver o ADR 0038.
    VoiceRoomOverHostUplink {
        /// Quanto a sala pede no pior caso, todos falando ao mesmo tempo.
        precisa_bps: u64,
        /// A subida que esta máquina mediu para si. Nunca zero.
        medido_bps: u32,
    },

    /// A retenção deste servidor apagou histórico. R15.
    ///
    /// A casca escreve a frase e, se tiver a última página na tela, volta a
    /// buscá-la: as mensagens que ela está mostrando podem já não existir.
    RetencaoApagouHistorico {
        /// Quantas linhas saíram.
        apagadas: u64,
    },

    /// O servidor não conseguiu fazer um pedido que a pessoa podia fazer.
    ///
    /// No fio, o pedido vem pelo ordinal em `ClientMessage`; aqui ele chega
    /// pelo **nome** da variante (`"SetNickname"`, `"RenameVoiceRoom"`), porque
    /// é por nome que a casca escreve a frase e acha o pedido a desfazer. Com
    /// dois do mesmo tipo em voo, a recusa é do mais antigo (ver
    /// `AlertReason::PedidoRecusado`).
    PedidoRecusado {
        /// O nome do pedido em `ClientMessage`, ou `"?"` para um ordinal que
        /// esta build não conhece.
        pedido: &'static str,
        /// Por que não deu.
        causa: CausaDaRecusa,
    },
}

/// Por que o servidor não conseguiu fazer um pedido. O espelho de
/// `seele_proto::control::CausaDaRecusa`, para a casca.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum CausaDaRecusa {
    /// O alvo do pedido não existe mais.
    NaoExiste,
    /// O nome pedido é de outro.
    NomeTomado,
    /// O servidor falhou ao fazer o que podia ser feito.
    FalhaDoServidor,
}

impl From<seele_proto::control::CausaDaRecusa> for CausaDaRecusa {
    fn from(causa: seele_proto::control::CausaDaRecusa) -> Self {
        match causa {
            seele_proto::control::CausaDaRecusa::NaoExiste => Self::NaoExiste,
            seele_proto::control::CausaDaRecusa::NomeTomado => Self::NomeTomado,
            seele_proto::control::CausaDaRecusa::FalhaDoServidor => Self::FalhaDoServidor,
        }
    }
}

impl From<seele_proto::control::AlertReason> for NoticeReason {
    fn from(reason: seele_proto::control::AlertReason) -> Self {
        match reason {
            seele_proto::control::AlertReason::Mentioned => Self::Mentioned,
            seele_proto::control::AlertReason::SubsystemChanged => Self::SubsystemChanged,
            seele_proto::control::AlertReason::SyncDegraded => Self::SyncDegraded,
            seele_proto::control::AlertReason::VoiceRoomEntryRefused => Self::VoiceRoomEntryRefused,
            seele_proto::control::AlertReason::PermissionDenied => Self::PermissionDenied,
            seele_proto::control::AlertReason::VoiceRoomFull => Self::VoiceRoomFull,
            seele_proto::control::AlertReason::OperatorNotice => Self::OperatorNotice,
            seele_proto::control::AlertReason::VoiceRoomOverHostUplink {
                precisa_bps,
                medido_bps,
            } => Self::VoiceRoomOverHostUplink {
                precisa_bps,
                medido_bps,
            },
            seele_proto::control::AlertReason::RetencaoApagouHistorico { apagadas } => {
                Self::RetencaoApagouHistorico { apagadas }
            }
            seele_proto::control::AlertReason::PedidoRecusado { pedido, causa } => {
                Self::PedidoRecusado {
                    pedido: seele_proto::control::nome_do_pedido(pedido).unwrap_or("?"),
                    causa: causa.into(),
                }
            }
            seele_proto::control::AlertReason::RateLimited => Self::RateLimited,
            seele_proto::control::AlertReason::MovedByOperator => Self::MovedByOperator,
            seele_proto::control::AlertReason::VoiceRoomDeleted => Self::VoiceRoomDeleted,
            seele_proto::control::AlertReason::ChannelDeleted => Self::ChannelDeleted,
            seele_proto::control::AlertReason::LastVoiceRoom => Self::LastVoiceRoom,
            seele_proto::control::AlertReason::ScreenShareTaken => Self::ScreenShareTaken,
            seele_proto::control::AlertReason::ScreenShareOverHostUplink => {
                Self::ScreenShareOverHostUplink
            }
        }
    }
}

/// Why a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum EndReason {
    /// Protocol version outside the compatibility window.
    Incompatible,
    /// Authentication failed. Says nothing about whether the account exists —
    /// `specs/08-seguranca.md` requires login failures to be uniform.
    CredentialRejected,
    /// The handshake did not finish in time.
    HandshakeTimeout,
    /// An operator disconnected this person.
    Kicked,
    /// An operator barred this person.
    Banned,
    /// The server is full.
    ServerFull,
    /// Planned downtime.
    ScheduledMaintenance,
    /// The server is stopping.
    ServerShuttingDown,
    /// Keepalive lapsed.
    Timeout,
    /// The client sent something it should not have.
    ProtocolViolation,
    /// The client exceeded its frame budget.
    RateLimited,
    /// This connection fell behind the server's events and lost some.
    ///
    /// Reconnecting is the repair, not a refusal: the session's view of the
    /// conversation has a hole in it that only a full resync fills.
    FellBehind,
    /// The host has not decided about this key yet. ADR 0030.
    ///
    /// Not a refusal: the request is written down on the hosting machine and
    /// stays there. Trying again after the host has looked is what gets in, and
    /// nothing was lost by the attempt.
    AdmissionPending,
    /// The host decided this key does not come in. ADR 0030.
    ///
    /// Milder than [`Self::Banned`]: the host can approve the same key later,
    /// and this never ended a session that was already running.
    AdmissionDenied,
    /// O apelido pedido é de outra chave. ADR 0017.
    ///
    /// Separada de [`Self::CredentialRejected`] porque é a única deste conjunto
    /// que a pessoa conserta sozinha, e porque não passa com o tempo: tentar de
    /// novo, ser aprovado de novo e reinstalar o app dão todos o mesmo
    /// resultado. Enquanto ela vestia a frase da credencial, o conselho que a
    /// tela dava — «confira o convite» — mandava a pessoa mexer na única coisa
    /// que não era o problema.
    NicknameTaken,
    /// Esta máquina não aceitou os MODs que o servidor exige. ADR 0045.
    ///
    /// Chega como fim de sessão só quando o servidor recusa a entrada; o caso
    /// normal é a tela de aceite, que vem por
    /// `seele_ffi::ConnectionError::ModsNaoAceitos` e não por aqui.
    ModsRecusados,
    /// Os MODs do servidor mudaram enquanto a sessão corria. ADR 0045.
    ///
    /// A sessão acaba e a pessoa lê o conjunto novo ao entrar de novo — é a
    /// forma de «um servidor que troca de MOD pergunta de novo».
    ModsMudaram,
    /// O servidor exige MODs que ele mesmo não consegue descrever. ADR 0045.
    ///
    /// Não é sobre a credencial de quem tentou entrar: é configuração do
    /// servidor, e quem hospeda tem o motivo no log dele.
    ModsIndisponiveis,
    /// Quem atendeu na volta da bateria não é o servidor desta sessão.
    ///
    /// A volta foi recusada dentro do TLS, antes do `Hello`, por um de dois
    /// motivos, e os dois caem aqui: a impressão esperada nomeava outra chave
    /// (`InviteMismatch`), ou quem atendeu mostrou o certificado do servidor sem
    /// provar ter a chave dele (`ChaveNaoProvada`). Era
    /// [`Self::CredentialRejected`], e a frase mandava conferir convite e senha.
    /// Eles não chegaram a quem atendeu: do aperto de mão saiu só o
    /// `ClientHello` do TLS, que vai antes de haver o que conferir.
    ///
    /// Sem as impressões de propósito: este enum é `Copy` e chega ao retrato
    /// (`ended`). Elas ficam no `seele.log`, na linha da volta recusada. Sem a
    /// origem da impressão também (o link colado ou a lista de servidores), e
    /// por isso a frase da casca só diz o que vale para as duas origens e os
    /// dois motivos.
    ServidorTrocouNaVolta,
    /// A chave fixada do servidor mudou entre a queda e a volta. ADR 0003.
    ///
    /// O alarme de chave trocada, no meio de uma sessão: a volta da bateria foi
    /// recusada dentro do TLS (`PinChanged`). Era
    /// [`Self::CredentialRejected`]. As duas impressões ficam no `seele.log`,
    /// pelo mesmo motivo de [`Self::ServidorTrocouNaVolta`].
    ChaveMudouNaVolta,
    /// Quem recebia encerrou a ligação da sala pessoal. Spec de 23/09/2026.
    ChamadaEncerrada,
    /// O toque da sala pessoal venceu sem ninguém atender.
    NinguemAtendeu,
    /// Quem recebia recusou a ligação.
    ChamadaRecusada,
    /// O servidor exige MODs de uma API que este app não roda (D-fio-7).
    ///
    /// As APIs exigidas chegam no fio como máscara, e não atravessam aqui: este
    /// enum é unitário porque a casca o lê como texto. A frase diz «atualize o
    /// app», que é o que a pessoa faz com qualquer API que falte; quem precisa
    /// dos números é o launcher, na sessão do Plano 3.
    ModsDeOutraApi,
    /// The link died without the server saying why.
    LinkLost,
}

impl From<seele_proto::control::DisconnectReason> for EndReason {
    fn from(reason: seele_proto::control::DisconnectReason) -> Self {
        match reason {
            seele_proto::control::DisconnectReason::Incompatible => Self::Incompatible,
            seele_proto::control::DisconnectReason::CredentialRejected => Self::CredentialRejected,
            seele_proto::control::DisconnectReason::HandshakeTimeout => Self::HandshakeTimeout,
            seele_proto::control::DisconnectReason::Kicked => Self::Kicked,
            seele_proto::control::DisconnectReason::Banned => Self::Banned,
            seele_proto::control::DisconnectReason::ServerFull => Self::ServerFull,
            seele_proto::control::DisconnectReason::ScheduledMaintenance => {
                Self::ScheduledMaintenance
            }
            seele_proto::control::DisconnectReason::ServerShuttingDown => Self::ServerShuttingDown,
            seele_proto::control::DisconnectReason::Timeout => Self::Timeout,
            seele_proto::control::DisconnectReason::ProtocolViolation => Self::ProtocolViolation,
            seele_proto::control::DisconnectReason::RateLimited => Self::RateLimited,
            seele_proto::control::DisconnectReason::FellBehind => Self::FellBehind,
            seele_proto::control::DisconnectReason::AdmissionPending => Self::AdmissionPending,
            seele_proto::control::DisconnectReason::AdmissionDenied => Self::AdmissionDenied,
            seele_proto::control::DisconnectReason::NicknameTaken => Self::NicknameTaken,
            seele_proto::control::DisconnectReason::ModsRecusados => Self::ModsRecusados,
            seele_proto::control::DisconnectReason::ModsMudaram => Self::ModsMudaram,
            seele_proto::control::DisconnectReason::ModsIndisponiveis => Self::ModsIndisponiveis,
            seele_proto::control::DisconnectReason::ChamadaEncerrada => Self::ChamadaEncerrada,
            seele_proto::control::DisconnectReason::NinguemAtendeu => Self::NinguemAtendeu,
            seele_proto::control::DisconnectReason::ChamadaRecusada => Self::ChamadaRecusada,
            seele_proto::control::DisconnectReason::ModsDeOutraApi { .. } => Self::ModsDeOutraApi,
        }
    }
}

/// One person in a voice room.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Person {
    /// Stable identifier for this person on this server.
    pub id: u64,
    /// Display name.
    pub nickname: String,
    /// Transmitting right now.
    pub speaking: bool,
    /// Microphone muted — mudo.
    pub muted: bool,
    /// Speakers muted — Isolamento total.
    pub total_isolation: bool,
    /// Sync Ratio, 0 to 100.
    pub signal: u8,
    /// Which band that falls into, decided once, in the core.
    pub sync_band: SignalBand,
    /// Whether this is the person holding the handle.
    pub is_self: bool,
}

/// The average Sync Ratio of a voice room, already banded — **MÉDIA DO VOICE_ROOM**.
///
/// The comp computes this in the shell and colours it there. It is computed in
/// the core instead, for the reason at the top of this module: a shell that
/// knows the thresholds is a shell that will disagree with the other one. What
/// arrives here is the number, the band and the size of the sample, and drawing
/// is all that is left to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct VoiceRoomSync {
    /// The average, 0 to 100, rounded to a whole point.
    ///
    /// The comp prints `82.4`; the datum is a `u8` at every point it exists, so
    /// this is `82`. A decimal here would be precision invented at the last
    /// step, not carried from the measurement.
    pub ratio: u8,
    /// Which band the average falls into.
    pub band: SignalBand,
    /// How many people it averages — the comp's `5 PLUGS`.
    pub people: u32,
}

impl From<crate::state::VoiceRoomSync> for VoiceRoomSync {
    fn from(sync: crate::state::VoiceRoomSync) -> Self {
        Self {
            ratio: sync.ratio,
            band: sync.band.into(),
            // A voice room holds at most `limit` people and `limit` is a `u16`. The
            // saturation is unreachable and is here so the conversion cannot
            // panic on a server that says otherwise.
            people: u32::try_from(sync.people).unwrap_or(u32::MAX),
        }
    }
}

/// A voice channel.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VoiceRoom {
    /// Identifier.
    pub id: u32,
    /// Display name.
    pub name: String,
    /// How many people fit.
    pub limit: u16,
    /// Whether entry needs a password.
    pub password_required: bool,
    /// Whether this person's connection is in it.
    pub occupied_by_us: bool,
    /// The Channel bound to it, if any. `specs/04-servidor-seele.md` makes the
    /// association optional.
    ///
    /// Carried so a shell can say what destroying that Channel would do to this
    /// room. The voice room survives it and comes out with no canal attached, which is
    /// a change nobody asked for — and a product whose confirmations name their
    /// consequences has to be able to name that one.
    pub channel: Option<u32>,
    /// Who is inside, in arrival order.
    pub people: Vec<Person>,
    /// The average Sync Ratio of everybody inside, or `None` if nobody is.
    ///
    /// `None` rather than zero: an empty voice room has nothing to average, and zero
    /// would draw every idle room in the critical colour.
    pub sync: Option<VoiceRoomSync>,
}

/// A text channel.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Channel {
    /// Identifier.
    pub id: u32,
    /// Display name.
    pub name: String,
    /// Whether this is the Channel being read.
    pub open: bool,
}

/// Something worth surfacing.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Notice {
    /// How loud.
    pub severity: Severity,
    /// What about.
    pub reason: NoticeReason,
    /// The operator's own words, when they have any.
    ///
    /// Not a hole in the enumerated-reasons rule: an operator writing about
    /// their own server is data, not an error reason.
    pub operator_text: Option<String>,
}

/// O que a pessoa escolheu, e **todos são teto** (§5 da spec de tela).
///
/// Teto e nunca piso: o sistema continua livre para ficar abaixo de cada um
/// destes números, e a §3.2 depende disso — *a voz nunca cede à tela*. Uma
/// escolha tratada como piso devolve os 225 ms de atraso na voz que o
/// `spikes/tela-no-transporte` mediu.
///
/// Nada aqui é conferido nesta travessia. Os valores da lista fechada do §5 são
/// 1080/720/540 e 30/15/8, e quem os converte em degrau de codificador é quem
/// abre o codificador — uma ponte que recusasse um número seria uma segunda
/// autoridade sobre a mesma lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LimitesDeTela {
    /// Teto de banda em bits por segundo. `None` = sem limite próprio; vale só
    /// o que o caminho permitir.
    pub banda_bps: Option<u32>,
    /// Altura máxima em pixels: 1080, 720 ou 540.
    pub altura_maxima: u32,
    /// Quadros por segundo, no máximo: 30, 15 ou 8.
    pub quadros_maximos: u32,
    /// O que cede primeiro quando o orçamento aperta.
    ///
    /// **Não é teto como os três acima**, e por isso não é um número: os outros
    /// dizem «no máximo isto», este diz o que sacrificar quando o máximo não
    /// couber.
    ///
    /// Ausente no JSON é [`Prioridade::Nitidez`], que é o padrão do §2 — uma
    /// casca antiga que não conheça este campo continua pedindo o que sempre
    /// pediu.
    #[serde(default)]
    pub prioridade: Prioridade,
    /// Se o som da fonte vai junto com a imagem.
    ///
    /// R21 e ADR 0054: é o controle **de quem envia**, e é a saída que o produto
    /// oferece a quem está num sistema onde a captura leva o áudio do SEELE junto
    /// — em vez de afirmar que o eco foi resolvido.
    ///
    /// Ausente no JSON é `true`, que é o que se espera de um vídeo compartilhado.
    /// Uma casca antiga que não conheça este campo continua pedindo o que sempre
    /// pediu.
    #[serde(default = "com_som_por_padrao")]
    pub com_som: bool,
}

/// O padrão de [`LimitesDeTela::com_som`].
///
/// Uma função porque `serde` não aceita `default = "true"` num `bool`, e o padrão
/// de `bool` é `false` — que aqui seria entregar vídeo mudo a quem não escolheu
/// nada.
const fn com_som_por_padrao() -> bool {
    true
}

/// O que cede primeiro quando o orçamento aperta.
///
/// `nitidez` é a regra do §2 — a resolução segura, o quadro cede —, e é certa
/// para texto: ele continua legível a 8 quadros e vira borrão se a resolução
/// baixar. `movimento` é o contrário, e é o que jogo pede: a 8 quadros um jogo
/// não é pior, é inutilizável.
///
/// Atravessa como texto minúsculo — `"nitidez"`, `"movimento"` — porque a casca
/// gráfica escreve este JSON à mão, e um número seria uma tabela a manter dos
/// dois lados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Prioridade {
    /// A resolução segura, o quadro cede. O padrão.
    #[default]
    Nitidez,
    /// O quadro segura, a resolução cede.
    Movimento,
}

/// A transmissão de tela desta sala de voz, quando há uma.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TelaEmCurso {
    // ---- o que saiu daqui, e por quê ----
    //
    // `altura`, `quadros`, `kbps` e `medida` eram quatro campos que **nada nesta
    // ponte media**: os três primeiros saíam zerados e o quarto saía `false`,
    // sempre. A tela reservava três caixas para eles e escrevia «ainda não há
    // medida desta transmissão» em todas as três, para sempre.
    //
    // O review da v15 (R20) mediu o custo disso na tela e a decisão é a dele:
    // retirar o espaço vazio enquanto não há medição. Quando houver medida de
    // verdade — ligada ao codificador de quem transmite, e por `ScreenId` — ela
    // volta como campos que dizem o que mediram e para qual tela. Um campo que
    // promete um número e entrega zero é pior que um campo ausente: ele faz a
    // interface reservar lugar para uma resposta que não existe.
    /// **Qual** transmissão é esta, como o servidor a batizou.
    ///
    /// # Por que ela passou a estar aqui
    ///
    /// Porque nada dizia de qual tela o painel falava. `tela_de` pegava a
    /// **primeira** transmissão encontrada no mapa da sala, sem relacioná-la com
    /// a que o espectador escolheu nem com a própria — então numa sala com duas
    /// pessoas transmitindo, autoria, controles e metadados podiam estar falando
    /// de uma tela enquanto a imagem era de outra. R18 da revisão da v15 e ADR
    /// 0054: no compartilhamento, tudo é por identidade.
    pub tela: u32,
    /// Quem está compartilhando.
    pub de: u64,
    /// Se é esta pessoa.
    pub e_minha: bool,
    /// Quantas pessoas estão **recebendo** esta transmissão.
    ///
    /// # O número passou a ser o do servidor
    ///
    /// Era contado no roster desta máquina — «quem está na sala menos quem
    /// compartilha» —, e isso incluía quem escolheu não assistir. O servidor
    /// conta assinaturas de verdade e as manda no `ScreenViewers`; é o mesmo N
    /// pelo qual ele divide o teto no §5.1, e usar outro aqui era ter duas
    /// contagens para a mesma pergunta, discordando.
    ///
    /// Zero até o primeiro `ScreenViewers` chegar. Zero é a resposta honesta: uma
    /// transmissão que começou agora não tem ninguém assistindo ainda.
    pub espectadores: u32,
    /// `Some` quando a transmissão está parada, com o nome estável do motivo.
    ///
    /// Um nome e não uma frase, como `seele_ffi::Snapshot::caminho`: a lista
    /// `seele_ffi::motivos_de_parada_da_tela` é derivada deste mesmo mapeamento,
    /// para a casca poder cobrar cobertura de frase sem repetir os nomes. Uma
    /// frase pronta em português atravessando aqui seria a única sentença que a
    /// casca não escreve — e a que o guarda de vocabulário da interface não vê.
    pub parada: Option<String>,
    /// O que foi **pedido** para esta transmissão, quando ela é desta pessoa.
    ///
    /// O outro lado do §5: *a tela não promete a escolha*, e mostra o que está
    /// saindo **ao lado** do que foi pedido. Os campos acima são a primeira
    /// metade; sem esta, a segunda não atravessa e a comparação que a regra
    /// obriga não tem com o que ser feita.
    ///
    /// **Estava faltando no contrato de 22/08, e a falta tinha dono.** A casca
    /// gráfica cobriu o buraco guardando os limites que ela mesma mandou numa
    /// variável de JavaScript — e uma janela recarregada, ou uma segunda janela
    /// sobre a mesma sessão, perdia a memória de uma transmissão que continuava
    /// saindo. Aqui o pedido mora do lado que sobrevive à janela, que é o mesmo
    /// lado que o mandou ao codificador.
    ///
    /// `None` em dois casos, e os dois são a mesma resposta honesta — «não sei»:
    ///
    /// - **quem só assiste**, sempre. O teto é escolha de quem compartilha e não
    ///   atravessa o fio em lugar nenhum (`ScreenHeader` carrega resolução e
    ///   codec, nunca o que a pessoa pediu). Inventá-lo a partir do que chega
    ///   seria devolver a medida como se fosse a escolha, que é exatamente a
    ///   promessa que o §5 proíbe;
    /// - **uma transmissão própria que este processo não começou** — a sessão
    ///   caiu e voltou, ou o servidor reanunciou uma tela de antes.
    pub pedido: Option<LimitesDeTela>,
}

/// O tema do servidor, como a casca o recebe: quem o declarou e o JSON dos
/// valores, opaco até a casca, que o confere com as mesmas tabelas do
/// `mundo.tema` (tarefa 17j) e mede os pares (17e).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TemaDaSessao {
    /// O MOD que declarou o tema.
    pub mod_id: String,
    /// O JSON do tema.
    pub valores: String,
}

/// Uma transmissão em curso, do ponto de vista de quem pode assisti-la.
///
/// O mínimo para desenhar uma linha de lista e para pedir a imagem: o nome da
/// transmissão e de quem ela é. O que ela está entregando — resolução, quadros,
/// kbps — está em [`TelaEmCurso`], e só para a que está no palco: medir as
/// outras seria medir o que ninguém está recebendo.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TransmissaoNaSala {
    /// Como o servidor batizou a transmissão. É o que se passa a `assistir`.
    pub tela: u32,
    /// Quem está mandando.
    pub de: u64,
    /// Se é esta pessoa — quem transmite não assiste a si mesmo pelo servidor.
    pub e_minha: bool,
    /// Se **esta** é a transmissão que esta sessão escolheu assistir.
    ///
    /// ADR 0054. Vem do lado que sobrevive à janela, e não de uma variável de
    /// JavaScript: uma recarga no meio de uma sessão perdia a escolha, e aí a
    /// fileira de botões não sabia mais qual estava marcado.
    pub assistida: bool,
    /// Quantas pessoas estão recebendo esta transmissão, como o servidor conta.
    ///
    /// Zero até o primeiro `ScreenViewers`. Ver [`TelaEmCurso::espectadores`].
    pub espectadores: u32,
}

/// Todo mundo que está conectado, em ordem de chegada.
///
/// Inclui quem está numa sala, e é de propósito: a casca precisa das duas
/// listas para desenhar «no servidor, fora das salas» sem fazer conta de menos
/// — subtrair uma da outra é trabalho de quem desenha, e ele é uma linha.
///
/// A própria pessoa entra aqui: o servidor não anuncia a chegada de volta para
/// quem chegou, então quem monta esta lista soma o `me` que a sessão já tem.
/// Sem isso, cada um seria o único ausente da própria lista de presentes.
pub fn presentes_de(room: &Room) -> Vec<Person> {
    room.presentes
        .iter()
        .chain(room.me.iter().filter(|eu| !room.presentes.contains(eu)))
        .filter_map(|id| room.people.get(id))
        .map(|person| Person {
            id: person.id.0,
            nickname: person.nickname.clone(),
            speaking: person.speaking,
            muted: person.muted,
            total_isolation: person.total_isolation,
            signal: person.signal,
            sync_band: seele_proto::signal::SignalBand::of(person.signal).into(),
            is_self: room.me == Some(person.id),
        })
        .collect()
}

/// As salas de voz, com quem está em cada uma e a sincronia de cada uma.
pub fn voice_rooms_of(room: &Room) -> Vec<VoiceRoom> {
    room.voice_rooms
        .iter()
        .map(|voice_room| VoiceRoom {
            id: voice_room.id.0,
            name: voice_room.name.clone(),
            limit: voice_room.limit,
            password_required: voice_room.password_required,
            occupied_by_us: room.current_voice_room == Some(voice_room.id),
            channel: voice_room.channel.map(|channel| channel.0),
            people: room
                .roster(voice_room.id)
                .map(|person| Person {
                    id: person.id.0,
                    nickname: person.nickname.clone(),
                    speaking: person.speaking,
                    muted: person.muted,
                    total_isolation: person.total_isolation,
                    signal: person.signal,
                    sync_band: seele_proto::signal::SignalBand::of(person.signal).into(),
                    is_self: room.me == Some(person.id),
                })
                .collect(),
            // Off the core, not folded here: the terminal draws the same number
            // from the same method, and a mean computed in two shells is a mean
            // two shells will one day round differently.
            sync: room.voice_room_sync(voice_room.id).map(Into::into),
        })
        .collect()
}

/// As linhas de texto, com a que está aberta marcada.
pub fn lines_of(room: &Room) -> Vec<Channel> {
    room.channels
        .iter()
        .map(|channel| Channel {
            id: channel.id.0,
            name: channel.name.clone(),
            open: room.current_channel == Some(channel.id),
        })
        .collect()
}

/// A transmissão de tela da sala onde esta pessoa está, como a casca a vê.
///
/// Lê o `Room` e o que esta sessão pediu, e nada mais. Os três números do que
/// está saindo — altura, quadros, kbps — ficam zerados com `medida: false`,
/// porque **nada nesta ponte os mede**: quem compartilha não tem codificador
/// daqui, e quem assiste não tem recepção aberta. Preenchê-los com o que foi
/// pedido, ou com o degrau que o teto compraria, seria a ponte prometendo a
/// escolha — o oposto exato da regra do §5.
///
/// `pedido` é a outra metade dessa mesma regra, e entra por parâmetro em vez de
/// sair do `Room`: o `Room` é o que o servidor contou, e o que esta pessoa
/// escolheu nunca passou por ele. Ele só sai daqui quando a transmissão é
/// desta pessoa — ver [`TelaEmCurso::pedido`].
/// Tudo o que está sendo transmitido na sala em que esta pessoa está.
///
/// **Só a sala dela.** O servidor só anuncia uma transmissão a quem está lá
/// dentro, e listar outra aqui seria a casca contando algo que a sessão não viu.
///
/// Ordenada pelo nome da transmissão, que é crescente: sem ordem, um mapa
/// devolveria as linhas em ordem diferente a cada desenho, e a lista piscaria
/// de lugar duas vezes por segundo.
pub fn transmissoes_de(room: &Room, assistida: Option<ScreenId>) -> Vec<TransmissaoNaSala> {
    let Some(voice_room) = room.current_voice_room else {
        return Vec::new();
    };
    let mut lista: Vec<TransmissaoNaSala> = room
        .telas
        .values()
        .filter(|tela| tela.voice_room == voice_room)
        .map(|tela| TransmissaoNaSala {
            tela: tela.screen.0,
            de: tela.person.0,
            e_minha: room.me == Some(tela.person),
            assistida: assistida == Some(tela.screen),
            espectadores: tela.espectadores,
        })
        .collect();
    lista.sort_by_key(|transmissao| transmissao.tela);
    lista
}

/// A transmissão de um `ScreenId`, se ela está acontecendo na sala desta pessoa.
///
/// **Por identidade, sempre.** A versão anterior pegava a primeira transmissão
/// que encontrasse no mapa da sala — ver o doc de `seele_ffi::Snapshot::tela` para o que
/// isso custava. ADR 0054.
pub fn tela_por_id(
    room: &Room,
    tela: ScreenId,
    pedido: Option<LimitesDeTela>,
) -> Option<TelaEmCurso> {
    let voice_room = room.current_voice_room?;
    let encontrada = room
        .telas
        .values()
        .find(|candidata| candidata.screen == tela && candidata.voice_room == voice_room)?;
    let e_minha = room.me == Some(encontrada.person);
    Some(TelaEmCurso {
        tela: encontrada.screen.0,
        de: encontrada.person.0,
        e_minha,
        // **O número do servidor**, que conta assinaturas de verdade. O de antes
        // era o roster desta máquina menos quem compartilha, e incluía quem
        // escolheu não assistir — duas contagens para a mesma pergunta.
        espectadores: encontrada.espectadores,
        parada: None,
        // Só para quem compartilha, e a conferência é o campo inteiro: o teto é
        // escolha de quem transmite e não viaja no fio, então o que esta
        // máquina guardou só descreve a **própria** transmissão. Mostrá-lo ao
        // lado da tela de outra pessoa seria a coluna «pedido» exibindo o que
        // um terceiro pediu na vez passada.
        pedido: if e_minha { pedido } else { None },
    })
}

/// A transmissão que **esta** pessoa está mandando, se há uma.
pub fn minha_tela_de(room: &Room, pedido: Option<LimitesDeTela>) -> Option<TelaEmCurso> {
    let voice_room = room.current_voice_room?;
    let eu = room.me?;
    let minha = room
        .telas
        .values()
        .find(|tela| tela.person == eu && tela.voice_room == voice_room)?;
    tela_por_id(room, minha.screen, pedido)
}

/// O que o `Room` diz para o retrato, lido de uma vez, sob o cadeado de quem o
/// guarda.
///
/// Era o `Foto` de dentro do `Connection::snapshot`: os campos são lidos e o
/// cadeado é soltado antes de qualquer outra coisa (o R12 da revisão da v15),
/// e é por isso que a foto é uma cópia, e não um empréstimo do `Room`.
#[derive(Debug, Clone, PartialEq)]
pub struct FotoDaSala {
    /// Quem esta sessão é, quando o servidor já a apresentou.
    pub instancia_me: Option<PersonId>,
    /// O apelido desta pessoa no roster, quando o roster a conhece.
    pub nickname: Option<String>,
    /// O nome do servidor.
    pub server: String,
    /// As salas de voz, com quem está em cada uma.
    pub voice_rooms: Vec<VoiceRoom>,
    /// Todo mundo conectado. Ver [`presentes_de`].
    pub presentes: Vec<Person>,
    /// As linhas de texto.
    pub channels: Vec<Channel>,
    /// A perda que o servidor contou, de zero a um.
    pub loss_fraction: f32,
    /// O aviso em curso.
    pub notice: Option<Notice>,
    /// O que esta pessoa pode fazer.
    pub permissions: Vec<Permission>,
    /// A transmissão escolhida, se ela está acontecendo na sala desta pessoa.
    pub tela: Option<TelaEmCurso>,
    /// A transmissão desta pessoa, se há uma.
    pub minha_transmissao: Option<TelaEmCurso>,
    /// Tudo o que se transmite na sala desta pessoa.
    pub transmissoes: Vec<TransmissaoNaSala>,
    /// Por que a sessão acabou, se acabou.
    pub ended: Option<EndReason>,
    /// O tema do servidor, se ele declarou um.
    pub tema: Option<TemaDaSessao>,
}

/// O aviso do `Room` na forma da casca.
#[must_use]
pub fn aviso_de(notice: &crate::state::Notice) -> Notice {
    Notice {
        severity: notice.severity.into(),
        reason: notice.reason.into(),
        operator_text: notice.operator_text.clone(),
    }
}

/// O tema do servidor na forma da casca.
#[must_use]
pub fn tema_de(tema: &TemaDoServidor) -> TemaDaSessao {
    TemaDaSessao {
        mod_id: tema.mod_id.clone(),
        valores: tema.valores.clone(),
    }
}

/// A foto da sala para o retrato.
///
/// `escolhida` é a transmissão que esta pessoa escolheu assistir, e `pedido` o
/// teto que ela escolheu para a própria: as duas são escolhas desta ponta, e
/// nunca passaram pelo `Room` (ver [`tela_por_id`]).
#[must_use]
pub fn foto_da_sala(
    room: &Room,
    escolhida: Option<ScreenId>,
    pedido: Option<LimitesDeTela>,
) -> FotoDaSala {
    FotoDaSala {
        instancia_me: room.me,
        nickname: room
            .me
            .and_then(|me| room.people.get(&me))
            .map(|person| person.nickname.clone()),
        server: room.server.clone(),
        voice_rooms: voice_rooms_of(room),
        presentes: presentes_de(room),
        channels: lines_of(room),
        loss_fraction: room.telemetry.as_ref().map_or(0.0, |t| t.loss_fraction),
        notice: room.notice.as_ref().map(aviso_de),
        permissions: room.permissions.clone(),
        tela: escolhida.and_then(|tela| tela_por_id(room, tela, pedido)),
        minha_transmissao: minha_tela_de(room, pedido),
        transmissoes: transmissoes_de(room, escolhida),
        ended: room.ended.map(|end| end.reason.into()),
        tema: room.tema.as_ref().map(tema_de),
    }
}
