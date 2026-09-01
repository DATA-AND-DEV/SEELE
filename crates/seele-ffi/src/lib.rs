//! **SEELE**, as one object a graphical shell can hold.
//!
//! `specs/06-clientes-gui.md` specifies this surface and states the rule it
//! exists to enforce: "Nenhuma funcionalidade nasce aqui: se algo é útil, é
//! implementado em `seele-core` e aparece nas duas interfaces." Nothing in this
//! crate decides anything. It owns a thread, a connection and a
//! [`seele_core::Room`], and it turns method calls into commands and server
//! events into notifications.
//!
//! # The shape, and why it is this shape
//!
//! ADR 0018: this is what `uniffi` will annotate in M6 without a rewrite.
//! One opaque object behind an `Arc`, no generics, no borrowed references
//! crossing over, errors as a closed enum, events through a `Send + Sync`
//! trait, and value types built from `u64`/`String`/`bool`/`Vec` and enums —
//! never a `seele-proto` newtype.
//!
//! # Threading
//!
//! [`Connection::connect`] **blocks**. It opens a QUIC connection and completes a
//! handshake, and a shell must call it off whatever thread draws. Everything
//! afterwards returns immediately: commands are queued to the driver thread,
//! and [`Connection::snapshot`] reads a copy.
//!
//! Events arrive on the driver thread, not the shell's. A listener that touches
//! a UI must marshal — `specs/06-clientes-gui.md`: "a casca marshala para sua
//! thread de UI."

// A test that has to handle the impossible case stops reading as a statement
// about the code. The same allowance the other crates take.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

pub mod types;

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seele_core::enlace::Enlace;
use seele_core::{
    identity, ChannelId, ClientMessageId, FilePinStore, MediaChannel, MessageId, PersonId, Room,
    Signal, SignalBand, Ssrc, SyncInputs, Voice, VoiceRoomId,
};

pub use types::{
    Attachment, AttachmentRefusal, CaptureDevice, Channel, ChannelWeight, ConnectionError,
    EndReason, Event, FonteDeTela, LimitesDeTela, LinkState, LinkTrust, Message, Notice,
    NoticeReason, PermissaoDeMicrofone, PermissaoDeTela, Person, PlaybackDevice, Preview,
    PreviewRefusal, PreviewRules, Severity, SignalBand as Band, Snapshot, TelaEmCurso, Telemetry,
    Transfer, Trust, VoiceMode, VoiceRoom, VoiceRoomSync,
};

/// O que a casca gráfica precisa do core além de um [`Connection`] vivo.
///
/// ADR 0002 deixa `seele-app` ver `seele-ffi` e mais nada, e as telas do app
/// precisam dos mesmos módulos que o `connection` usa direto: a lista de servidores
/// visitados, a busca no histórico, a leitura de um `seele://` e os ajustes
/// que ficam nesta máquina. Nenhum deles é lógica de casca — se fossem escritos
/// aqui, seriam escritos de novo no terminal e mais uma vez no cliente móvel.
/// `preferences` é o caso mais claro: um microfone escolhido no app e ignorado
/// pelo terminal é um microfone que parece ter sido esquecido.
///
/// Re-exportados um a um, e não com um `pub use seele_core::*`, pela mesma
/// doutrina que `seele-core` já aplica sobre `seele-proto`: acrescentar a esta
/// lista é a hora de perguntar se a casca precisa do valor ou da decisão por
/// trás dele.
pub use seele_core::{conhecidos, preferences, search, uri};

/// Every microphone this machine is offering, right now.
///
/// A free function and not a method on [`Connection`]: picking a microphone is a thing
/// a person does *before* connecting at least as often as during, and hanging
/// the list off a live session would put the control behind the door it exists
/// to open. It is also what makes the list answerable from the entry screen.
///
/// An empty list means the machine would not enumerate — **not** that there is
/// no microphone. The default device still opens when enumeration fails, which
/// is why [`ConnectConfig::capture_device`] of `None` never consults this. A
/// shell that draws "no audio" off an empty list is drawing the wrong sentence;
/// [`Snapshot::audio_available`] is the one that means it.
#[must_use]
pub fn capture_devices() -> Vec<CaptureDevice> {
    seele_core::capture_devices()
        .into_iter()
        .map(|found| CaptureDevice {
            id: found.id,
            name: found.name,
            default: found.default,
        })
        .collect()
}

/// Every place this machine will play sound, right now.
///
/// The twin of [`capture_devices`], a free function for the same reason, and
/// with the same warning about an empty list: the machine would not enumerate,
/// **not** that there is nowhere to play. [`Snapshot::audio_available`] is still
/// the one that means "no audio".
#[must_use]
pub fn playback_devices() -> Vec<PlaybackDevice> {
    seele_core::playback_devices()
        .into_iter()
        .map(|found| PlaybackDevice {
            id: found.id,
            name: found.name,
            default: found.default,
        })
        .collect()
}

/// O que o sistema deixa este processo fazer com o microfone.
///
/// **Livre e não método**, ao contrário da permissão de tela: não precisa de
/// sessão nenhuma. Quem descobre que está mudo quer a resposta antes de
/// entrar, e a tela de entrada não tem `Connection` para perguntar.
///
/// Só olha, e não pede nada — no Windows não há o que pedir para um app de
/// área de trabalho. Ver `seele_audio::device::consentimento_do_microfone`.
#[must_use]
pub fn permissao_de_microfone() -> PermissaoDeMicrofone {
    seele_core::consentimento_do_microfone().into()
}

/// A impressão digital da identidade desta máquina.
///
/// A mesma chave com que este computador entra em qualquer servidor, e a mesma
/// impressão que o outro lado vê — `identity.key` sob o `home`, criada na
/// primeira vez se ainda não existir.
///
/// Existe porque quem hospeda precisa se reconhecer no próprio servidor **antes**
/// de bater na própria porta, e o app não pode calcular isso sozinho: a regra de
/// dependência do ADR 0002 o deixa ver `seele-ffi` e `seele-server`, e nunca
/// `seele-core` nem `seele-proto`. A ponte é aqui.
///
/// # Errors
///
/// Falha se a identidade não puder ser lida nem criada.
pub fn impressao_desta_maquina(home: &str) -> Result<String, ConnectionError> {
    let chave =
        identity::load_or_create(&PathBuf::from(home).join("identity.key")).map_err(|error| {
            tracing::warn!(%error, "identity unavailable");
            ConnectionError::IdentityUnavailable
        })?;
    Ok(seele_core::key_fingerprint(
        chave.verifying_key().as_bytes(),
    ))
}

/// How often measurements are refreshed.
const TICK: Duration = Duration::from_millis(250);

/// How many messages of history to ask for when o canal opens.
const HISTORY_PAGE: u16 = 50;

/// What a shell needs to connect.
#[derive(Debug, Clone)]
pub struct ConnectConfig {
    /// `host` or `host:port`.
    pub server: String,
    /// The invite's other addresses for the same server, in try order.
    ///
    /// Empty for an address typed by hand, and for every invite written before
    /// ADR 0006 grew the field. One address per entry, same syntax as
    /// [`ConnectConfig::server`]; one that does not resolve is dropped rather
    /// than refused, because losing a path is not a reason to lose the link.
    pub alternate_servers: Vec<String>,
    /// How to appear in the roster.
    pub nickname: String,
    /// Where the identity and the pins live. ADR 0017.
    ///
    /// A path rather than a key, because the shell knows where its platform
    /// keeps configuration and the core knows how to persist an identity.
    pub home: String,
    /// Convite de uso único ou senha, quando o servidor exige um.
    ///
    /// `None` num servidor aberto, que é o padrão.
    pub join_secret: Option<String>,
    /// A impressão digital que o convite prometeu, quando veio de um link.
    pub expected_fingerprint: Option<String>,
    /// O bilhete de encontro do link, quando ele trouxe um.
    ///
    /// Degrau 4 do ADR 0022: com ele, quem entra bate no ponto de encontro antes
    /// de tentar os endereços, e o anfitrião fura o NAT para cá. `None` é o
    /// caminho de sempre — e é o que um link sem `enc` produz.
    pub bilhete: Option<seele_core::uri::Bilhete>,
    /// Whether to open the microphone and speakers at all.
    ///
    /// False on a machine with no sound card, which is most servers and every
    /// CI box. The text half of the product needs none.
    pub audio: bool,
    /// Which microphone to open, as a [`CaptureDevice::id`].
    ///
    /// `None` is the machine's default, and is what every session took before
    /// there was a screen to choose on. A device that is gone by the time the
    /// session opens does **not** refuse the connection — see `drive`, which
    /// falls back to the default rather than turning a stale preference into a
    /// server nobody can enter.
    pub capture_device: Option<String>,
    /// Where the sound comes out, as a [`PlaybackDevice::id`].
    ///
    /// `None` is the machine's default. Falls back the same way its twin does,
    /// and independently of it: a headset left in another room must not cost
    /// somebody the microphone they chose.
    pub playback_device: Option<String>,
}

/// Uma etapa de uma chegada, do jeito que uma casca a recebe.
///
/// Espelho de `seele_core::chegada::Etapa`, com o **mesmo nome** em cada
/// variante e em cada campo. O que esta cópia acrescenta é `Serialize` e o
/// [`ConnectionError`] no lugar do erro do núcleo; nada mais. A regra que ela segue é
/// a das outras travessias deste arquivo: o nome do tipo é do crate e nunca
/// atravessa, e o nome que atravessa — variante e campo — é o que o núcleo
/// escolheu, porque é ele que a casca usa como chave de frase. Traduzir aqui
/// daria dois vocabulários para a mesma coisa, e a casca teria de aprender os
/// dois.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ConnectStage {
    /// Parada, com o convite lido e nada tentado.
    Parada {
        /// Quantos endereços o convite trouxe.
        candidatos: u8,
        /// O link trouxe `enc=` **e** o primeiro candidato tem impressão
        /// digital.
        ///
        /// Não é «o degrau 4 do ADR 0022 vai ser tentado»: ver a variante do
        /// núcleo, que escreve o que a bandeira sabe e o que ela não sabe.
        com_bilhete_e_impressao: bool,
    },
    /// Avisando o ponto de encontro de que estamos chegando.
    Avisando {
        /// O ponto de encontro, como o convite o escreveu.
        ponto: String,
    },
    /// Um aperto de mão correndo contra um endereço do convite.
    Tentando {
        /// Qual da lista, contando do zero.
        candidato: u8,
        /// De quantos.
        de: u8,
        /// O endereço, como texto: um `SocketAddr` não atravessa (ADR 0018).
        onde: String,
        /// Um `LEVE` saiu pelo ponto de encontro por causa deste candidato.
        ///
        /// Ver a variante do núcleo. É a metade da informação de que
        /// [`Snapshot::caminho`] é feito, e a única que não se lê do endereço.
        avisou: bool,
    },
    /// Um furo com a marca certa chegou, e o caminho até aqui abriu.
    ///
    /// **Marca não é autenticação.** Ver a variante do núcleo: esta etapa não
    /// decide para onde conectar nem dispensa conferência nenhuma — ela
    /// antecipa o instante da tentativa, e mais nada.
    CaminhoAberto {
        /// De onde o furo veio.
        onde: String,
    },
    /// Dentro: o aperto de mão terminou e há sessão.
    Dentro,
    /// Nenhum candidato entrou, e este é o motivo.
    Desistiu {
        /// O mesmo erro que a casca já sabe escrever.
        motivo: ConnectionError,
    },
}

/// Uma etapa e quando ela aconteceu.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConnectStep {
    /// Onde a chegada estava.
    pub etapa: ConnectStage,
    /// Quantos milissegundos depois do começo da chegada.
    ///
    /// Milissegundos e não uma `Duration`, pela mesma regra do resto desta
    /// fronteira: um número que toda linguagem tem.
    pub em_ms: u64,
}

/// Uma conexão que não aconteceu, e por onde ela passou tentando.
///
/// «Tentei quatro candidatos, o primeiro deu prazo esgotado em 4 s, o quarto
/// recusou» é o dado que faltou quando o teste de campo das duas casas falhou e
/// ninguém soube dizer por quê. Custa zero em privacidade: todo endereço da
/// trilha já estava no convite de quem a lê.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConnectFailure {
    /// Por que não deu, do jeito de sempre.
    pub error: ConnectionError,
    /// Por onde a chegada passou, em ordem. Vazia quando nada chegou a ser
    /// tentado — uma identidade que não abre, uma thread que não sobe.
    pub trail: Vec<ConnectStep>,
}

impl From<ConnectionError> for ConnectFailure {
    /// Uma falha de antes de haver chegada: o erro sozinho, sem trilha.
    fn from(error: ConnectionError) -> Self {
        Self {
            error,
            trail: Vec::new(),
        }
    }
}

impl std::fmt::Display for ConnectFailure {
    /// Para log e para `Error`, nunca para uma pessoa.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:?} depois de {} passos",
            self.error,
            self.trail.len()
        )
    }
}

impl std::error::Error for ConnectFailure {}

impl ConnectStage {
    /// O nome estável que a casca usa como chave de frase.
    ///
    /// O mesmo de `seele_core::chegada::Etapa::nome`, e é obrigação desta
    /// travessia que continue sendo: a casca escreve uma frase por nome, e um
    /// nome que se perde entre os dois lados é uma tela muda.
    #[must_use]
    pub fn nome(&self) -> &'static str {
        match self {
            Self::Parada { .. } => "Parada",
            Self::Avisando { .. } => "Avisando",
            Self::Tentando { .. } => "Tentando",
            Self::CaminhoAberto { .. } => "CaminhoAberto",
            Self::Dentro => "Dentro",
            Self::Desistiu { .. } => "Desistiu",
        }
    }

    /// Um exemplar de cada etapa que pode atravessar esta fronteira.
    ///
    /// **Derivada, e não escrita à mão.** Ela é `seele_core::chegada::Etapa::TODAS`
    /// passada por [`From`], que é a única porta por onde uma etapa atravessa —
    /// então uma variante nova do núcleo aparece aqui sozinha, e a casca, que
    /// pelo ADR 0002 não pode ver o núcleo, ganha a lista sem repeti-la.
    ///
    /// É para isso que ela existe: o guarda de cobertura de `frases.js` mora em
    /// `apps/seele-app/tests/frontend.rs` e lia uma terceira cópia da lista,
    /// escrita à mão. Uma etapa nova atravessava e caía no «falha que esta tela
    /// não sabe nomear» no meio de uma conexão que ia bem, sem que teste nenhum
    /// acendesse.
    ///
    /// Os valores são de exemplo e não significam nada — o que se lê deles é a
    /// variante.
    #[must_use]
    pub fn todas() -> Vec<Self> {
        seele_core::chegada::Etapa::TODAS
            .iter()
            .map(Self::from)
            .collect()
    }
}

impl From<&seele_core::chegada::Etapa> for ConnectStage {
    /// Uma etapa do núcleo, do jeito que a casca a recebe.
    fn from(etapa: &seele_core::chegada::Etapa) -> Self {
        use seele_core::chegada::Etapa;
        match etapa {
            Etapa::Parada {
                candidatos,
                com_bilhete_e_impressao,
            } => Self::Parada {
                candidatos: *candidatos,
                com_bilhete_e_impressao: *com_bilhete_e_impressao,
            },
            Etapa::Avisando { ponto } => Self::Avisando {
                ponto: ponto.clone(),
            },
            Etapa::Tentando {
                candidato,
                de,
                onde,
                avisou,
            } => Self::Tentando {
                candidato: *candidato,
                de: *de,
                onde: onde.to_string(),
                avisou: *avisou,
            },
            Etapa::CaminhoAberto { onde } => Self::CaminhoAberto {
                onde: onde.to_string(),
            },
            Etapa::Dentro => Self::Dentro,
            Etapa::Desistiu(erro) => Self::Desistiu {
                motivo: classify_connect_failure(erro),
            },
        }
    }
}

/// Os três campos que uma etapa pode carregar sobre **onde** ela aconteceu.
///
/// `Option` e não um valor de enfeite: `Dentro` não tem candidato, e escrever
/// `candidato = 0` para ela seria inventar o zero. Para `Avisando` o «onde» é o
/// ponto de encontro, que é para onde aquele passo mandou alguma coisa; para
/// `Parada` o `de` é quantos endereços o convite trouxe, que é a mesma
/// grandeza que o `de` de uma tentativa — «de quantos».
fn campos_do_passo(etapa: &seele_core::chegada::Etapa) -> (Option<String>, Option<u8>, Option<u8>) {
    use seele_core::chegada::Etapa;
    match etapa {
        Etapa::Avisando { ponto } => (Some(ponto.clone()), None, None),
        Etapa::Tentando {
            candidato,
            de,
            onde,
            ..
        } => (Some(onde.to_string()), Some(*candidato), Some(*de)),
        Etapa::CaminhoAberto { onde } => (Some(onde.to_string()), None, None),
        Etapa::Parada { candidatos, .. } => (None, None, Some(*candidatos)),
        Etapa::Dentro | Etapa::Desistiu(_) => (None, None, None),
    }
}

/// A trilha de uma chegada que falhou, no log do processo.
///
/// **É a superfície inteira que responde a pergunta desta tarefa.** Enquanto
/// `apps/seele-app` entrar por [`Connection::connect`], que joga a trilha fora, este
/// log é o único lugar em que ela aparece — e um log que escrevesse só a etapa
/// e o relógio responderia «Parada, Tentando, Desistiu, aos 8003 ms», que não é
/// a pergunta. A pergunta é **qual dos quatro deu o quê**, e ela precisa do
/// endereço e do índice.
fn registrar_trilha(trilha: &[seele_core::chegada::Passo]) {
    for passo in trilha {
        let (onde, candidato, de) = campos_do_passo(&passo.etapa);
        tracing::info!(
            etapa = passo.etapa.nome(),
            candidato = ?candidato,
            de = ?de,
            onde = ?onde,
            avisou = ?avisou_do_passo(&passo.etapa),
            em_ms = %passo.em.as_millis(),
            "trilha da chegada"
        );
    }
}

/// Se um `LEVE` saiu por este passo, nos passos em que a pergunta faz sentido.
///
/// Vai para o log junto do endereço porque é a outra metade da resposta: uma
/// tentativa que falhou **tendo avisado** e uma que falhou sem aviso nenhum são
/// duas investigações diferentes, e sem esta linha as duas ficam iguais no
/// registro.
fn avisou_do_passo(etapa: &seele_core::chegada::Etapa) -> Option<bool> {
    use seele_core::chegada::Etapa;
    match etapa {
        Etapa::Tentando { avisou, .. } => Some(*avisou),
        Etapa::Parada { .. }
        | Etapa::Avisando { .. }
        | Etapa::CaminhoAberto { .. }
        | Etapa::Dentro
        | Etapa::Desistiu(_) => None,
    }
}

/// Os nomes de caminho que podem chegar a uma casca.
///
/// **Derivada, e não escrita à mão**, pela mesma regra de
/// [`ConnectStage::todas`]: é `seele_core::chegada::Caminho::TODOS` passada pelo
/// `nome()` do núcleo. O guarda de cobertura de `frases.js` lê esta lista, e o
/// ADR 0002 não deixa `apps/seele-app` ver o núcleo — sem esta função a casca
/// teria de repetir os quatro nomes, que é exatamente o defeito que este ciclo
/// já pagou uma vez.
#[must_use]
pub fn caminhos() -> Vec<&'static str> {
    seele_core::chegada::Caminho::TODOS
        .iter()
        .map(seele_core::chegada::Caminho::nome)
        .collect()
}

/// Um passo da trilha, do jeito que a casca o recebe.
fn step_of(passo: &seele_core::chegada::Passo) -> ConnectStep {
    ConnectStep {
        etapa: ConnectStage::from(&passo.etapa),
        // Saturado, e não truncado: uma chegada que levasse mais de 584 milhões
        // de anos merece um número errado no fim da escala, e não um pequeno.
        em_ms: u64::try_from(passo.em.as_millis()).unwrap_or(u64::MAX),
    }
}

/// What a shell implements to be told things changed.
///
/// Called on the driver thread. `specs/06-clientes-gui.md` puts the marshalling
/// on the shell, because only the shell knows what its UI thread is.
pub trait EventListener: Send + Sync {
    /// Something happened.
    fn on_event(&self, event: Event);
}

/// A command on its way to the driver thread.
enum Command {
    EnterVoiceRoom(VoiceRoomId),
    LeaveVoiceRoom,
    OpenChannel(ChannelId),
    Send {
        channel: ChannelId,
        body: String,
    },
    SetMuted(bool),
    SetTotalIsolation(bool),
    CreateVoiceRoom {
        name: String,
        limit: u16,
        channel: Option<ChannelId>,
    },
    CreateChannel {
        name: String,
    },
    RenameVoiceRoom {
        voice_room: VoiceRoomId,
        name: String,
    },
    RenameChannel {
        channel: ChannelId,
        name: String,
    },
    RenameServer {
        name: String,
    },
    /// Põe ou tira a imagem de perfil de quem está usando este cliente.
    SetPersonIcon {
        /// A figura, ou `None` para não ter.
        icon: Option<Vec<u8>>,
    },
    /// Troca o apelido de quem está usando este cliente.
    SetNickname {
        /// O nome novo.
        name: String,
    },
    SetServerIcon {
        icon: Option<Vec<u8>>,
    },
    KickPerson {
        person: PersonId,
    },
    BanPerson {
        person: PersonId,
        reason: Option<String>,
        expires_at: Option<i64>,
    },
    RemoveMessage {
        message: MessageId,
    },
    MovePerson {
        person: PersonId,
        voice_room: VoiceRoomId,
    },
    DeleteVoiceRoom {
        voice_room: VoiceRoomId,
    },
    DeleteChannel {
        channel: ChannelId,
    },
    /// The one command that carries somewhere to answer.
    ///
    /// Everything else on this queue is a thing to do, confirmed — when it is
    /// confirmed at all — by the server announcing it to everybody. This is a
    /// question, and its answer is only useful to the caller who asked, while
    /// the box it fills is still open.
    WeighChannel {
        channel: ChannelId,
        answer: tokio::sync::oneshot::Sender<ChannelWeight>,
    },
    /// A file, on a stream of its own. ADR 0027.
    ///
    /// Boxed because it is much larger than every other variant here, and an
    /// enum is as big as its largest arm — a path, three strings and two ids on
    /// every queued keystroke would be paid by all of them.
    Attach(Box<seele_core::enlace::Anexo>),
    SaveAttachment {
        attachment: seele_core::AttachmentId,
        destination: std::path::PathBuf,
    },
    /// A picture to look at, and where to put the verdict. ADR 0027.
    ///
    /// The second command here that carries somewhere to answer, and the same
    /// argument as [`Command::WeighChannel`]: the answer is only useful to the
    /// caller who asked, while the box it fills is still open.
    PreviewAttachment {
        attachment: seele_core::AttachmentId,
        answer: tokio::sync::oneshot::Sender<Preview>,
    },
    /// A tela escolhida, com o módulo do Cisco já carregado. §3.6.
    ///
    /// Boxeado pela mesma razão de [`Command::Attach`]: um enum é do tamanho do
    /// maior braço dele, e sem a caixa toda tecla digitada pagaria pelo peso
    /// desta.
    ShareScreen {
        pedido: Box<seele_core::PedidoDeTela>,
        limites: seele_core::LimitesDeTela,
    },
    /// Trocar os tetos da transmissão que já está de pé.
    ///
    /// Sem caixa: não carrega módulo nem captura, só três números.
    AdjustScreenLimits {
        limites: seele_core::LimitesDeTela,
    },
    StopScreenShare,
    /// Pede um quadro-chave a quem está compartilhando.
    ///
    /// **Quem chega no meio de uma transmissão precisa dele para ver alguma
    /// coisa.** Um fluxo H.264 é um quadro-chave e uma corrente de diferenças
    /// que só fazem sentido a partir dele; o codificador manda um no começo e
    /// depois **só quando alguém pede**. Entrar numa sala em que já se
    /// compartilha era receber deltas sobre um passado que nunca se viu, e ficar
    /// olhando para o nada — «quando alguém entra numa call que alguém tá
    /// compartilhando tela, a pessoa não consegue ver a transmissão».
    ///
    /// O pedido existia dos dois lados desde sempre: `Client::request_key_frame`
    /// aqui e `ClientMessage::RequestKeyFrame` no servidor, que já o traduz num
    /// aviso a quem compartilha. **Ninguém chamava nenhum dos dois.**
    RequestKeyFrame {
        tela: seele_core::ScreenId,
    },
    Shutdown,
}

/// State the driver thread writes and the shell reads.
struct Shared {
    room: Mutex<Room>,
    listeners: Mutex<Vec<Arc<dyn EventListener>>>,
    /// The voice path, which is also where the chosen devices are remembered.
    ///
    /// Remembered there and not here, on purpose. This crate used to keep its
    /// own copy of the chosen microphone, and a second copy of a fact is a
    /// second copy that can disagree — a switch that took but was not recorded
    /// here would be undone by the next reconnection, silently. `Voice` keeps
    /// what it asked for and hands it back through `reopen`, so there is one
    /// answer and the layer that acts on it is the layer that holds it.
    voice: Mutex<Option<Voice>>,
    /// What it takes to open a voice path on the connection that is up now.
    ///
    /// Here, and not only on the driver thread, so that switching microphone can
    /// answer on the caller's thread with a real error. Sent through the command
    /// queue instead, a device that is gone would fail somewhere nobody is
    /// listening, and the screen would show a pick that never took.
    ///
    /// Rewritten on every reconnection: the ssrc and the channel are both new.
    media: Mutex<Option<(MediaChannel, Ssrc)>>,
    nickname: Mutex<String>,
    link_state: AtomicU8,
    /// Round trip in microseconds. Integer because atomics have no `f32`, and
    /// microseconds because milliseconds would round a fast local link to zero.
    rtt_micros: std::sync::atomic::AtomicU64,
    /// O jitter de chegada deste receptor, em microssegundos (RFC 3550).
    ///
    /// Guardado aqui porque quem o calcula é [`measure`], no laço de voz, e quem
    /// o mostra é o [`Connection::snapshot`], na casca — e antes disto ele era
    /// calculado, usado no Sync Ratio e jogado fora, enquanto a tela lia o zero
    /// que o servidor manda de propósito (`session.rs` diz em comentário que o
    /// servidor não tem como medir jitter, porque jitter se mede no receptor).
    ///
    /// Microssegundos inteiros, e não os milissegundos em `f32` que a tela
    /// mostra, pelo mesmo motivo de [`Self::rtt_micros`] logo acima: não há
    /// átomo de `f32`, e um cadeado por quadro de interface seria contenção por
    /// nada. Um jitter de rede honesto vive na casa das unidades de
    /// milissegundo, então um milissegundo inteiro arredondaria a diferença
    /// entre um enlace bom e um ótimo para o mesmo número.
    jitter_de_chegada_micros: std::sync::atomic::AtomicU64,
    signal: AtomicU8,
    running: AtomicBool,
    /// Onde o enlace está, para o `Snapshot` contar à casca.
    ///
    /// Átomos e não um `Mutex`: isto é lido a cada quadro de interface e
    /// escrito raramente, e um cadeado por leitura seria contenção por nada.
    /// Zero segundos restantes significa "no ar".
    link_battery: AtomicBool,
    link_seconds: std::sync::atomic::AtomicU64,
    link_attempts: std::sync::atomic::AtomicU32,
    /// Por qual caminho esta conversa saiu, quando a chegada soube dizer.
    ///
    /// Escrito uma vez, quando a sessão sobe, e lido a cada quadro de interface.
    /// Um `Mutex` e não um átomo porque é `&'static str` e porque a escrita
    /// acontece exatamente uma vez — a contenção que os átomos de telemetria
    /// evitam não existe aqui.
    ///
    /// `None` é uma resposta, e é a que a casca recebe quando a trilha não sabe
    /// dizer: sem informação a tela **não escreve nada**. Ver
    /// `seele_core::chegada::caminho`.
    ///
    /// Não é reescrito quando o enlace reconecta. A reconexão de
    /// `seele_core::enlace` volta ao mesmo endereço do mesmo destino, então a
    /// forma não muda; o que poderia mudar é o `avisou`, e trocar o nome na tela
    /// por causa de um `send_to` que falhou num soluço de rede seria movimento
    /// sem informação.
    caminho: Mutex<Option<&'static str>>,
    /// Quantas vezes o histórico mudou desde que esta sessão começou.
    ///
    /// Existe para o [`Snapshot`] poder dizer "mudou" sem carregar a conversa
    /// inteira. Antes ele carregava: cada quadro de interface clonava todo
    /// apelido e todo corpo já dito, serializava em JSON e atravessava a ponte
    /// — duas vezes por segundo, custando proporcionalmente ao tamanho da
    /// conversa. Numa sessão longa isso é atraso que só cresce, e foi assim que
    /// apareceu num teste entre duas máquinas.
    ///
    /// O número em si não significa nada; só a diferença significa. A casca
    /// guarda o último que desenhou e busca o histórico quando ele muda.
    messages_revision: std::sync::atomic::AtomicU64,
    /// Quantas vezes a imagem do servidor mudou desde que esta sessão começou.
    ///
    /// Existe pelo mesmo motivo do `messages_revision`, num tamanho menor: o
    /// [`Snapshot`] é lido a cada quadro de interface, e carregar os bytes nele
    /// seria cloná-los, serializá-los em JSON e atravessar a ponte com eles
    /// duas vezes por segundo — por um valor que muda quando alguém aperta um
    /// botão e nunca mais.
    ///
    /// O número em si não significa nada; só a diferença significa. A casca
    /// guarda o último que desenhou e busca a imagem quando ele muda.
    icon_revision: std::sync::atomic::AtomicU64,
    /// O mesmo número, para as imagens **das pessoas**.
    ///
    /// Um só para todas, e não um por pessoa: quem desenha os retratos desenha
    /// todos no mesmo quadro, então uma casca que descobre que algo mudou
    /// rebusca o que está na tela e pronto. Um contador por pessoa custaria um
    /// campo em cada `Person` do `Snapshot`, duas vezes por segundo, para
    /// distinguir um caso que ninguém trata separadamente.
    person_icons_revision: std::sync::atomic::AtomicU64,
    /// Quem está esperando o peso de uma Linha, e por qual Linha.
    ///
    /// A única pergunta com resposta deste crate. Todo o resto que a casca pede
    /// é ordem — o servidor confirma anunciando a mudança a todo mundo —, e este é
    /// o número de uma frase que só serve a quem perguntou, enquanto a caixa
    /// que ela enche estiver aberta.
    ///
    /// Uma lista e não um mapa por Linha: duas caixas sobre a mesma Linha ao
    /// mesmo tempo não acontecem numa janela só, e se acontecessem um mapa
    /// atenderia uma e deixaria a outra esperando para sempre. Aqui as duas
    /// recebem a mesma resposta.
    pending_weights: Mutex<Vec<(ChannelId, tokio::sync::oneshot::Sender<ChannelWeight>)>>,
    /// Os limites que esta sessão pediu para a transmissão de tela dela.
    ///
    /// **Mora aqui porque a janela não sobrevive à transmissão.** O §5 manda
    /// mostrar o que está saindo ao lado do que foi pedido, e o que foi pedido
    /// não atravessa o fio em lugar nenhum — o `ScreenHeader` carrega resolução
    /// e codec, nunca a escolha. A casca gráfica cobria o buraco guardando a
    /// última escolha numa variável de JavaScript, e uma janela recarregada
    /// perdia metade da comparação enquanto a tela continuava saindo.
    ///
    /// Um `Mutex` e não um átomo: são três números e um `Option`, e a escrita
    /// acontece quando alguém aperta um botão — a contenção que os átomos de
    /// telemetria evitam não existe aqui.
    ///
    /// Limpo quando a transmissão desta pessoa acaba, e é a mesma regra que a
    /// casca já escrevia do lado dela: guardá-lo faria o painel da próxima
    /// comparar o que está saindo agora com um teto de outra vez.
    limites_da_tela: Mutex<Option<LimitesDeTela>>,
    /// A última lista de fontes que [`Connection::fontes_de_tela`] devolveu.
    ///
    /// **Existe porque o número que a casca devolve é o índice desta lista**, e
    /// não um identificador do sistema: um `Alvo` da WGC não publica `HWND`
    /// nenhum, e um número estável entre duas listagens não existe nos dois
    /// sistemas. Ver `seele_core::FonteDeTela::id`.
    ///
    /// E porque o alvo nativo não atravessa a travessia: a casca recebe nome e
    /// tamanho, e o `SCWindow` — que é o que a captura precisa — fica deste
    /// lado, esperando a escolha.
    fontes_de_tela: Mutex<Vec<seele_core::FonteDeTela>>,
}

impl Shared {
    /// Says the conversation moved: bumps the revision, then notifies.
    ///
    /// One call and not two, because the two must never happen apart. They did:
    /// `Command::OpenChannel` clears the room's messages and emitted the event
    /// **without** bumping the revision, so a shell that refetches only when the
    /// number moves — which is the whole point of the number — swallowed it.
    /// Switching Channel kept the previous Channel's conversation on screen until
    /// somebody said something.
    ///
    /// That was a regression of the fix that stopped the snapshot carrying the
    /// whole history: before it, the shell re-read every message twice a second
    /// and a cleared list showed up on the next tick regardless. Making the
    /// shell smarter made this notification load-bearing, and it was not
    /// carrying its half.
    ///
    /// The bump comes first. A shell reacting to the event would otherwise read
    /// the old number and conclude there is nothing to fetch.
    fn messages_changed(&self) {
        self.messages_revision.fetch_add(1, Ordering::Relaxed);
        self.notify(&Event::MessagesChanged);
    }

    /// Guarda onde o enlace está, para o `Snapshot` contar à casca.
    fn gravar_enlace(&self, estado: seele_core::Link, restante: Option<std::time::Duration>) {
        let na_bateria = matches!(estado, seele_core::Link::InternalBattery { .. });
        self.link_battery.store(na_bateria, Ordering::Relaxed);
        self.link_seconds
            .store(restante.map_or(0, |q| q.as_secs()), Ordering::Relaxed);
        if let seele_core::Link::InternalBattery { attempts } = estado {
            self.link_attempts.store(attempts, Ordering::Relaxed);
        }
    }

    /// Guarda o que esta pessoa pediu para a tela dela, ou esquece.
    ///
    /// `None` esquece, e é o que acontece quando a transmissão acaba: um teto
    /// guardado além dela faria o painel da próxima comparar o que está saindo
    /// agora com uma escolha de outra vez.
    fn gravar_pedido_da_tela(&self, limites: Option<LimitesDeTela>) {
        if let Ok(mut guardado) = self.limites_da_tela.lock() {
            *guardado = limites;
        }
    }

    /// O que esta pessoa pediu para a tela dela, se ainda vale.
    ///
    /// Um cadeado envenenado responde `None`, que é a mesma resposta de «esta
    /// sessão não pediu nada»: a casca escreve travessão e não uma escolha que
    /// ninguém consegue mais afirmar.
    fn pedido_da_tela(&self) -> Option<LimitesDeTela> {
        self.limites_da_tela.lock().ok().and_then(|g| *g)
    }

    /// O estado do enlace como a casca o vê.
    fn enlace(&self) -> LinkState {
        if self.link_battery.load(Ordering::Relaxed) {
            LinkState::InternalBattery {
                remaining_seconds: self.link_seconds.load(Ordering::Relaxed),
                attempts: self.link_attempts.load(Ordering::Relaxed),
            }
        } else {
            LinkState::Online
        }
    }

    /// Guarda o jitter de chegada que o laço de voz acabou de medir.
    ///
    /// Só quem chama [`jitter_para_a_tela`] tem o que gravar aqui, e é de
    /// propósito: o número certo e o errado são ambos milissegundos em `f32`, e
    /// nada além daquela função separa um do outro.
    ///
    /// Devolve se o número que a casca lê mudou de verdade — o suficiente para
    /// valer um redesenho. A comparação é em microssegundos inteiros, que é como
    /// ele é guardado: comparar os `f32` de origem acenderia o evento a cada
    /// volta de telemetria, quatro vezes por segundo, porque um jitter alisado
    /// muda no último bit sempre.
    fn gravar_jitter_de_chegada(&self, ms: f32) -> bool {
        // Sem limitar a zero à mão: em Rust o `as` de ponto flutuante para
        // inteiro **satura**, e leva tanto o negativo quanto o `NaN` a zero
        // sozinho. Um `max` aqui seria uma guarda que nunca dispara, e uma
        // guarda que nunca dispara faz o teste que a exercita passar por si
        // mesmo em vez de guardar a propriedade.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "o `as` satura: negativo e NaN viram zero, e o teto de u64 está muito acima de qualquer jitter"
        )]
        let micros = (ms * 1000.0) as u64;
        let antes = self
            .jitter_de_chegada_micros
            .swap(micros, Ordering::Relaxed);
        // Meio milissegundo, que é a menor diferença que o rodapé arredonda para
        // outro número: `Math.round` na casca, `{:.0}` na TUI. Abaixo disso o
        // evento acordaria a tela para ela redesenhar o mesmo texto.
        antes.abs_diff(micros) >= 500
    }

    /// Guarda por qual caminho a chegada saiu, se ela soube dizer.
    fn gravar_caminho(&self, caminho: Option<seele_core::chegada::Caminho>) {
        if let Ok(mut slot) = self.caminho.lock() {
            *slot = caminho.map(|caminho| caminho.nome());
        }
    }

    /// O caminho como a casca o vê: um nome estável, ou nada.
    fn caminho(&self) -> Option<&'static str> {
        self.caminho.lock().ok().and_then(|slot| *slot)
    }

    /// O jitter de chegada como a casca o vê, em milissegundos.
    fn jitter_de_chegada_ms(&self) -> f32 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "um jitter em microssegundos está muito abaixo da faixa exata do f32"
        )]
        let ms = self.jitter_de_chegada_micros.load(Ordering::Relaxed) as f32 / 1000.0;
        ms
    }

    /// Entrega o peso a quem perguntou por aquela Linha.
    ///
    /// Quem sobrar na lista é de outra Linha e continua esperando. Ninguém fica
    /// esperando para sempre: o remetente é largado quando a tarefa do enlace
    /// termina, e um `oneshot` largado acorda quem espera com erro — que é como
    /// esta ponte já diz "não há sessão".
    /// Takes the transfer reasons the room collected, and empties the queue.
    ///
    /// Drained rather than read: two transfers can be in the air at once, and
    /// the second one failing must not erase the reason the first one did. The
    /// `Room` keeps them in order and this hands them on in order.
    fn drain_transfers(&self) -> Vec<Transfer> {
        let Ok(mut room) = self.room.lock() else {
            return Vec::new();
        };
        room.transfers
            .drain(..)
            .map(|aviso| match aviso {
                seele_core::TransferNotice::Refused {
                    client_message_id,
                    reason,
                } => Transfer::RefusedBecause {
                    client_message_id: client_message_id.get(),
                    reason: refusal_of(reason),
                },
                seele_core::TransferNotice::Unavailable { attachment, reason } => {
                    Transfer::Unavailable {
                        attachment: attachment.get(),
                        reason: refusal_of(reason),
                    }
                }
            })
            .collect()
    }

    fn answer_weight(&self, weight: ChannelWeight) {
        let Ok(mut pending) = self.pending_weights.lock() else {
            return;
        };
        let mut esperando = Vec::new();
        for (channel, answer) in pending.drain(..) {
            if channel.get() == weight.channel {
                let _ = answer.send(weight);
            } else {
                esperando.push((channel, answer));
            }
        }
        *pending = esperando;
    }

    fn notify(&self, event: &Event) {
        let listeners = match self.listeners.lock() {
            Ok(listeners) => listeners.clone(),
            Err(_) => return,
        };
        for listener in listeners {
            listener.on_event(event.clone());
        }
    }
}

/// A live session.
///
/// Dropping it disconnects.
pub struct Connection {
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Connection")
            .field("running", &self.shared.running.load(Ordering::Relaxed))
            .finish()
    }
}

/// Qual metade do áudio uma troca mexeu.
///
/// Só para [`Connection::conferir_troca`]: a conferência é a mesma dos dois
/// lados, e duas cópias dela divergiriam no dia em que uma ganhasse um caso.
#[derive(Debug, Clone, Copy)]
enum Lado {
    Entrada,
    Saida,
}

impl Connection {
    /// Connects, authenticates, and starts the driver thread.
    ///
    /// **Blocks** until the session reaches CONEXÃO SEGURA or fails.
    ///
    /// A porta antiga, e o que ela perde: o erro sem a trilha. Ela fica porque
    /// oito arquivos de teste entram por aqui, e trocá-los todos no commit que
    /// abre a porta nova transformaria um passo barato num caro. Quem desenha
    /// uma tela de falha quer [`Connection::connect_with_trail`].
    ///
    /// # Errors
    ///
    /// Every failure is a [`ConnectionError`] variant, never a string: a shell has to
    /// be able to write its own sentence for each one.
    pub fn connect(config: ConnectConfig) -> Result<(Arc<Self>, Trust), ConnectionError> {
        Self::connect_with_trail(config).map_err(|falha| falha.error)
    }

    /// O mesmo, respondendo também por onde a chegada passou.
    ///
    /// A trilha é o que faltava quando o teste de campo das duas casas falhou:
    /// a tela dizia «não consegui conectar» sobre quatro tentativas das quais
    /// nenhuma tinha nome. Ela sobrevive à falha e atravessa inteira — ver
    /// [`ConnectFailure`].
    ///
    /// # Errors
    ///
    /// [`ConnectFailure`], que é o [`ConnectionError`] de sempre mais a trilha. Ela
    /// vem vazia quando a falha é de antes de haver chegada: um endereço que
    /// não resolve, uma identidade que não abre, uma thread que não sobe.
    pub fn connect_with_trail(config: ConnectConfig) -> Result<(Arc<Self>, Trust), ConnectFailure> {
        Self::abrir(config, None)
    }

    /// O mesmo, com alguém ouvindo a chegada **enquanto** ela acontece.
    ///
    /// A porta que faltava, e a falta era estrutural. `seele_core` publica uma
    /// etapa por instante da travessia desde a tarefa 8, e nada em produção as
    /// lia: [`Connection::connect`] bloqueia, quem se inscreve por [`Connection::subscribe`]
    /// só tem o `Arc<Connection>` depois que ela volta, e quando ela volta a travessia
    /// inteira já terminou. O ouvinte tinha de entrar **antes** do bloqueio, e é
    /// só isso que esta função faz de diferente.
    ///
    /// Chega como [`Event::ConnectStageChanged`], na thread do motor, como todo
    /// evento deste crate — quem marshala é a casca (`specs/06-clientes-gui.md`).
    /// A última etapa (`Dentro` ou `Desistiu`) sai antes desta função devolver:
    /// o motor espera o reencaminhamento terminar, e ele termina sozinho quando
    /// a `Chegada` morre.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Connection::connect_with_trail`].
    pub fn connect_watching(
        config: ConnectConfig,
        olhos: Arc<dyn EventListener>,
    ) -> Result<(Arc<Self>, Trust), ConnectFailure> {
        Self::abrir(config, Some(olhos))
    }

    /// O corpo que as três portas compartilham.
    fn abrir(
        config: ConnectConfig,
        olhos: Option<Arc<dyn EventListener>>,
    ) -> Result<(Arc<Self>, Trust), ConnectFailure> {
        let (address, server_name, pin_key) = resolve(&config.server)?;
        // The invite's other addresses, resolved here so the driver thread gets
        // values. An alternative that does not resolve is dropped: the first
        // address is the one the person is most likely on the same network as,
        // and refusing the whole link over a spare would trade one path fewer
        // for no path at all.
        let alternates: Vec<(SocketAddr, String, String)> = config
            .alternate_servers
            .iter()
            .filter_map(|alternate| resolve(alternate).ok())
            .collect();

        let home = PathBuf::from(&config.home);
        let key = identity::load_or_create(&home.join("identity.key")).map_err(|error| {
            tracing::warn!(%error, "identity unavailable");
            ConnectionError::IdentityUnavailable
        })?;
        let pins = Arc::new(FilePinStore::open(home.join("pins")).map_err(|error| {
            tracing::warn!(%error, "pin store unavailable");
            ConnectionError::IdentityUnavailable
        })?);

        let shared = Arc::new(Shared {
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            icon_revision: std::sync::atomic::AtomicU64::new(0),
            person_icons_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            media: Mutex::new(None),
            nickname: Mutex::new(config.nickname.clone()),
            link_state: AtomicU8::new(link_state_byte(LinkTrust::Offline)),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
            caminho: Mutex::new(None),
            signal: AtomicU8::new(0),
            running: AtomicBool::new(false),
            pending_weights: Mutex::new(Vec::new()),
            limites_da_tela: Mutex::new(None),
            fontes_de_tela: Mutex::new(Vec::new()),
        });

        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let thread_shared = Arc::clone(&shared);
        let thread_config = config.clone();
        std::thread::Builder::new()
            .name("seele-connection".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        tracing::error!(%error, "could not start the connection runtime");
                        let _ = ready_tx.send(Err(ConnectionError::Unreachable.into()));
                        return;
                    }
                };
                runtime.block_on(drive(
                    thread_config,
                    address,
                    server_name,
                    pin_key,
                    alternates,
                    key,
                    pins,
                    thread_shared,
                    command_rx,
                    &ready_tx,
                    olhos,
                ));
            })
            .map_err(|error| {
                tracing::error!(%error, "could not start the connection thread");
                ConnectionError::Unreachable
            })?;

        // The thread reports the outcome of the handshake, then keeps running.
        let trust = ready_rx
            .recv()
            .map_err(|_| ConnectionError::Unreachable)??;

        let connection = Arc::new(Self {
            commands: command_tx,
            shared,
        });
        Ok((connection, trust))
    }

    /// Puts the connection into a voice room.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn insert_plug(&self, voice_room: u32) -> Result<(), ConnectionError> {
        self.command(Command::EnterVoiceRoom(VoiceRoomId(voice_room)))
    }

    /// Takes the connection out.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn eject_plug(&self) -> Result<(), ConnectionError> {
        self.command(Command::LeaveVoiceRoom)
    }

    /// Opens o canal and asks for the page of history behind it.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn open_channel(&self, channel: u32) -> Result<(), ConnectionError> {
        self.command(Command::OpenChannel(ChannelId(channel)))
    }

    /// Sends a file, on a stream of its own. ADR 0027.
    ///
    /// Asks, and does not wait: the bar comes back as
    /// [`Event::TransferChanged`], and the message appears on the Channel only
    /// once the bytes have arrived whole. While it is going up, the only person
    /// who can see it is the sender — the cost ADR 0027 takes on purpose, so
    /// that "not arrived yet" and "expired" are never two similar absences on
    /// the same screen.
    ///
    /// `declared_type` is a **claim**, passed through as one.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn send_attachment(
        &self,
        channel: u32,
        body: String,
        path: String,
        file_name: String,
        declared_type: String,
    ) -> Result<u64, ConnectionError> {
        // The key is taken here rather than on the queue, unlike `Send`: the
        // shell needs it **now**, to hang a bar on. Nothing else about a
        // message has ever had to be known before it was sent.
        let id = next_client_message_id();
        self.command(Command::Attach(Box::new(seele_core::enlace::Anexo {
            linha: ChannelId(channel),
            id: ClientMessageId(id),
            corpo: body,
            caminho: std::path::PathBuf::from(path),
            nome: file_name,
            tipo: declared_type,
        })))?;
        Ok(id)
    }

    /// Saves an attachment where the person receiving it chose.
    ///
    /// **Where they chose, and nowhere else.** ADR 0027 gives no client of the
    /// SEELE a button that opens a file: saving is an act of the person who
    /// received it, in a place they picked. The file is marked with the
    /// operating system's own quarantine on the way down — which is not
    /// antivirus, and this product does not have one.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn save_attachment(
        &self,
        attachment: u64,
        destination: String,
    ) -> Result<(), ConnectionError> {
        self.command(Command::SaveAttachment {
            attachment: seele_core::AttachmentId(attachment),
            destination: std::path::PathBuf::from(destination),
        })
    }

    /// Fetches a small attachment and says whether a window may draw it.
    ///
    /// **On a press, never on a scroll**, and that is a decision this call
    /// leaves no room to get wrong: it downloads. The file lives on the server,
    /// so looking at it costs the host's uplink, and o canal that previewed
    /// everything as it scrolled would turn a 1 GiB disk ceiling into a 1 GiB
    /// transfer every time somebody opened it.
    ///
    /// Nothing is written to disk on the way. Saving is a separate act with a
    /// separate confirmation, and this is not it.
    ///
    /// The claim the sender made is read from this client's own history rather
    /// than taken as an argument. The fewer hands a string somebody else chose
    /// passes through before it is compared to the bytes, the better.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over, and equally if it
    /// ends while the fetch is in flight.
    pub async fn preview_attachment(&self, attachment: u64) -> Result<Preview, ConnectionError> {
        let (answer, caixa) = tokio::sync::oneshot::channel();
        self.command(Command::PreviewAttachment {
            attachment: seele_core::AttachmentId(attachment),
            answer,
        })?;
        caixa.await.map_err(|_| ConnectionError::NotConnected)
    }

    /// What a screen needs to decide whether to offer a preview at all.
    ///
    /// Convenience, exactly like [`SessionInfo::permissions`] is: a window that
    /// asks anyway gets [`PreviewRefusal::TooBig`] or
    /// [`PreviewRefusal::NotAPicture`], so this saves a round trip and enforces
    /// nothing.
    ///
    /// The four types come from here rather than being written out again in a
    /// screen, because a second copy of the list is a second copy that can
    /// disagree — and it would disagree by offering to draw something the fetch
    /// then refuses.
    ///
    /// [`SessionInfo::permissions`]: seele_core::SessionInfo::permissions
    #[must_use]
    pub fn preview_rules() -> PreviewRules {
        PreviewRules {
            limit: seele_core::PREVIEW_LIMIT,
            types: seele_core::ImageFormat::ALL
                .iter()
                .map(|format| format.media_type().to_owned())
                .collect(),
        }
    }

    /// Says something in o canal.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn send_message(&self, channel: u32, body: String) -> Result<(), ConnectionError> {
        if body.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::Send {
            channel: ChannelId(channel),
            body,
        })
    }

    /// Asks the server to make a voice room.
    ///
    /// Asks. It does not decide, and it does not report back whether it worked
    /// — because it cannot: the answer comes from the server, arrives on the
    /// event stream, and reaches the shell as [`Event::ChannelsChanged`] when
    /// the room exists or [`Event::NoticeRaised`] carrying
    /// [`NoticeReason::PermissionDenied`] when it does not. Returning a result
    /// here would mean this method waiting on a round trip, which is the one
    /// thing every other command on this object promises not to do.
    ///
    /// `channel` binds a text channel to the room; `None` leaves it a voice room
    /// only, which `specs/04-servidor-seele.md` allows.
    ///
    /// The empty-name case returns `Ok` without sending anything, the same way
    /// [`Connection::send_message`] swallows an empty body: a person who pressed the
    /// button with nothing typed has not asked for anything, and answering that
    /// with an error would put a red message on a screen where nothing went
    /// wrong.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn create_voice_room(
        &self,
        name: String,
        limit: u16,
        channel: Option<u32>,
    ) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::CreateVoiceRoom {
            name,
            limit,
            channel: channel.map(ChannelId),
        })
    }

    /// Asks the server to make o canal.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn create_channel(&self, name: String) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::CreateChannel { name })
    }

    /// Asks the server to rename a voice room.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn rename_voice_room(&self, voice_room: u32, name: String) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::RenameVoiceRoom {
            voice_room: VoiceRoomId(voice_room),
            name,
        })
    }

    /// Asks the server to rename o canal.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn rename_channel(&self, channel: u32, name: String) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::RenameChannel {
            channel: ChannelId(channel),
            name,
        })
    }

    /// Asks the server to rename itself.
    ///
    /// Asks, and reports nothing back, like [`Connection::rename_voice_room`]: the answer
    /// comes from the server, as [`Event::ServerChanged`] with the new name on
    /// the next [`Snapshot`], or as [`Event::NoticeRaised`] carrying
    /// [`NoticeReason::PermissionDenied`].
    ///
    /// A blank name is swallowed here rather than sent, exactly as
    /// [`Connection::rename_voice_room`] swallows one: a shell with an empty box and a
    /// button is not a shell reporting an error, and the server would refuse it
    /// anyway.
    ///
    /// A shell may read [`Snapshot::may_customise_server`] to decide whether to
    /// draw the control. **Convenience, never enforcement** — pressing it
    /// without the permission gets a refusal and nothing changes.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn rename_server(&self, name: String) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::RenameServer { name })
    }

    /// Asks the server to change its picture, or `None` to have none.
    ///
    /// PNG bytes, and the server refuses anything else — a real PNG, at most
    /// `seele_proto::control::MAX_SERVER_ICON_SIDE` a side, at most
    /// `seele_proto::control::MAX_SERVER_ICON_LEN` bytes. A shell hands over
    /// what the person picked; the picture that comes back to everybody is the
    /// proof it was taken.
    ///
    /// Checked here **before** the command is queued, with the protocol's own
    /// function rather than a copy of its rules. Not for politeness: a picture
    /// the protocol refuses cannot be turned into a frame, a frame that cannot
    /// be built is a send that fails, and a send that fails is how a dropped
    /// connection looks from inside `seele_core::enlace`. Somebody choosing a
    /// PDF would watch the app start its five-minute internal battery.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::IconNotAPicture`] or [`ConnectionError::IconTooBig`] when the
    /// bytes will not do; [`ConnectionError::NotConnected`] once the session is over.
    pub fn set_server_icon(&self, icon: Option<Vec<u8>>) -> Result<(), ConnectionError> {
        seele_core::check_server_icon(icon.as_deref()).map_err(|recusa| match recusa {
            seele_core::IconRefusal::NotAnIcon => ConnectionError::IconNotAPicture,
            seele_core::IconRefusal::TooBig { limit_bytes } => {
                ConnectionError::IconTooBig { limit_bytes }
            }
        })?;
        self.command(Command::SetServerIcon { icon })
    }

    /// Põe ou tira **a sua** imagem de perfil.
    ///
    /// Os mesmos limites do ícone do servidor, e conferidos aqui antes de o
    /// comando sair: só PNG, 8 KiB, 256 px de lado. Conferir deste lado é o que
    /// permite a casca dizer o motivo — a recusa do servidor derruba a conexão
    /// por violação de protocolo, e uma figura grande demais não é um ataque,
    /// é um arquivo que alguém escolheu.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::IconNotAPicture`] se não for um PNG,
    /// [`ConnectionError::IconTooBig`] se passar do teto, e
    /// [`ConnectionError::NotConnected`] quando não há sessão.
    pub fn set_person_icon(&self, icon: Option<Vec<u8>>) -> Result<(), ConnectionError> {
        seele_core::check_server_icon(icon.as_deref()).map_err(|recusa| match recusa {
            seele_core::IconRefusal::NotAnIcon => ConnectionError::IconNotAPicture,
            seele_core::IconRefusal::TooBig { limit_bytes } => {
                ConnectionError::IconTooBig { limit_bytes }
            }
        })?;
        self.command(Command::SetPersonIcon { icon })
    }

    /// Troca **o seu** apelido.
    ///
    /// O histórico não muda: cada mensagem guarda o apelido de quem a escreveu
    /// no instante em que foi escrita, e é decisão de produto que continue
    /// mostrando aquele. Ver `ClientMessage::SetNickname`.
    ///
    /// Um nome em branco é desistir da edição, e não um pedido: devolve `Ok`
    /// sem mandar nada, como `set_person_icon` faz com o seletor fechado.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] quando não há sessão. Um nome já
    /// tomado por outra conta volta como alerta do servidor, e não daqui: quem
    /// sabe quais nomes existem é ele.
    pub fn set_nickname(&self, name: String) -> Result<(), ConnectionError> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Ok(());
        }
        self.command(Command::SetNickname { name })
    }

    /// A imagem de perfil de alguém, ou `None` quando não tem.
    ///
    /// Fora do [`Snapshot`] de propósito, e pela razão que
    /// [`Connection::server_icon`] escreve: o snapshot atravessa a ponte duas
    /// vezes por segundo, e uma figura de 8 KiB **por pessoa** ali dentro seria
    /// megabytes por minuto de uma coisa que muda quase nunca. A casca pede a
    /// imagem quando a lista muda, e não a cada tique.
    #[must_use]
    pub fn person_icon(&self, person: u64) -> Option<Vec<u8>> {
        self.shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.person_icons.get(&PersonId(person)).cloned())
    }

    /// The server's picture, if it has one.
    ///
    /// Separate from [`Connection::snapshot`] for the reason [`Connection::messages`] is
    /// separate from it: the two change at completely different rates, and
    /// carrying the bytes on every frame of a redraw would mean cloning them
    /// and serialising them twice a second for a value that moves when
    /// somebody presses a button. Ask for this when
    /// [`Snapshot::icon_revision`] changes, and not otherwise.
    #[must_use]
    pub fn server_icon(&self) -> Option<Vec<u8>> {
        self.shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.icon.clone())
    }

    /// Esquece o aviso que está na tela. Ver `seele_core::Room::dispensar_aviso`.
    ///
    /// Síncrono e sem passar pelo servidor: o aviso é desta ponta, e nada do outro
    /// lado precisa saber que alguém o leu.
    pub fn dispensar_aviso(&self) {
        let dispensou = self
            .shared
            .room
            .lock()
            .is_ok_and(|mut room| room.dispensar_aviso());
        // Só quando havia: um evento por clique num botão que não fez nada é a
        // casca redesenhando por educação.
        if dispensou {
            self.shared.notify(&Event::TelemetryChanged);
        }
    }

    /// Asks the server to end a person's session — `expulsar`.
    ///
    /// Asks, and reports nothing back, for the same reason [`Connection::create_voice_room`]
    /// gives: the answer comes from the server. The roster losing them arrives as
    /// [`Event::RosterChanged`]; a refusal arrives as [`Event::NoticeRaised`]
    /// carrying [`NoticeReason::PermissionDenied`].
    ///
    /// A shell may read [`Snapshot::may_kick`] to decide whether to draw the
    /// control. That is **convenience, never enforcement** — pressing it
    /// without the permission removes nobody.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn kick_person(&self, person: u64) -> Result<(), ConnectionError> {
        self.command(Command::KickPerson {
            person: PersonId(person),
        })
    }

    /// Asks the server to bar a person from returning — `banir`.
    ///
    /// `expires_at` is seconds since the Unix epoch; `None` is permanent. The
    /// `reason` is for whoever hosts, in their own records, and never reaches
    /// the person barred.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn ban_person(
        &self,
        person: u64,
        reason: Option<String>,
        expires_at: Option<i64>,
    ) -> Result<(), ConnectionError> {
        self.command(Command::BanPerson {
            person: PersonId(person),
            reason,
            expires_at,
        })
    }

    /// Asks the server to take a message off its Channel — `remover_mensagem`.
    ///
    /// It goes away for everybody, this client included, when the server says so.
    /// An author removing their own needs no permission, which is why a shell
    /// drawing this control on one's own message may draw it for anybody.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn remove_message(&self, message: u64) -> Result<(), ConnectionError> {
        self.command(Command::RemoveMessage {
            message: MessageId(message),
        })
    }

    /// Asks the server to move a person into a voice room — `mover_pessoa`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn move_person(&self, person: u64, voice_room: u32) -> Result<(), ConnectionError> {
        self.command(Command::MovePerson {
            person: PersonId(person),
            voice_room: VoiceRoomId(voice_room),
        })
    }

    /// Asks the server to destroy a voice room — `apagar_voice_room`.
    ///
    /// Everybody inside is turned out of it and told; the Channel bound to it, if
    /// there is one, is left alone. The server refuses the last voice room, and says so
    /// with [`NoticeReason::LastVoiceRoom`] rather than with the sentence it uses for
    /// a refused entry.
    ///
    /// Asks, and nothing more. Nothing is removed from this client's own idea of
    /// the server until the server says the room is gone — a room removed
    /// optimistically would vanish off the screen of the person who asked
    /// whether or not it worked, and the refusal is the case they most need to
    /// see did not happen.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn delete_voice_room(&self, voice_room: u32) -> Result<(), ConnectionError> {
        self.command(Command::DeleteVoiceRoom {
            voice_room: VoiceRoomId(voice_room),
        })
    }

    /// Asks the server to destroy o canal, and everything written in it —
    /// `apagar_linha`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn delete_channel(&self, channel: u32) -> Result<(), ConnectionError> {
        self.command(Command::DeleteChannel {
            channel: ChannelId(channel),
        })
    }

    /// Asks what destroying o canal would cost, and waits for the answer.
    ///
    /// The one call on this handle that waits, and the reason is the sentence it
    /// feeds: a confirmation promising to destroy 1.847 messages by 6 people
    /// written since a certain day has to have counted them, in the server's own
    /// database, at the moment of asking. This client holds one page of history
    /// and would guess low by whatever the Channel's whole past is — and a number
    /// that is nearly right in that box is worse than no number at all.
    ///
    /// So the caller waits, and a shell that cannot get an answer must not open
    /// the box: there is no honest version of it without these three numbers.
    ///
    /// Destroys nothing, and needs no permission — the server answers about a
    /// Channel the asker may already read.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over, and equally if it
    /// ends while the question is in flight: the driver drops what it was going
    /// to answer with, and this returns rather than waiting for a server that is
    /// no longer there.
    pub async fn weigh_channel(&self, channel: u32) -> Result<ChannelWeight, ConnectionError> {
        let (answer, resposta) = tokio::sync::oneshot::channel();
        self.command(Command::WeighChannel {
            channel: ChannelId(channel),
            answer,
        })?;
        resposta.await.map_err(|_| ConnectionError::NotConnected)
    }

    /// Mutes or unmutes the microphone — mudo.
    ///
    /// Announced to the server as well as applied locally: the roster shows it,
    /// and a mute nobody else can see is half a feature.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn set_muted(&self, on: bool) -> Result<(), ConnectionError> {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                voice.set_muted(on);
            }
        }
        self.command(Command::SetMuted(on))
    }

    /// Mutes or unmutes the speakers — Isolamento total.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn set_total_isolation(&self, on: bool) -> Result<(), ConnectionError> {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                voice.set_total_isolation(on);
            }
        }
        self.command(Command::SetTotalIsolation(on))
    }

    /// Reports the push-to-talk key going down or coming up.
    ///
    /// Not a command: it has to take effect on the next 20 ms frame, and a
    /// round trip through the driver thread would put a queue between a key and
    /// a microphone.
    pub fn set_talking(&self, talking: bool) {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                voice.set_key_held(talking);
            }
        }
    }

    /// Switches this session to another microphone.
    ///
    /// `device` is a [`CaptureDevice::id`] from [`capture_devices`]; `None` goes
    /// back to the machine's default. The choice is remembered, so a
    /// reconnection reopens the same microphone rather than quietly falling back
    /// to the default one. The chosen sound output is left where it is.
    ///
    /// Takes effect **now**, not on the next voice room. That is worth the extra
    /// mechanism: somebody opens this screen because the microphone they are
    /// speaking into is the wrong one, and telling them to leave the server and
    /// come back is telling them to solve it themselves.
    ///
    /// Synchronous rather than queued, because the answer is the point: a device
    /// that has been unplugged since the list was drawn has to come back as an
    /// error the screen can put next to the row that was clicked.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::CaptureDeviceGone`] when the machine is not offering that
    /// device any more, in which case **nothing changed** and the previous
    /// microphone is still live. [`ConnectionError::NoAudioDevice`] when this session
    /// has no audio at all — a session joined with the audio box unticked has no
    /// voice path to move.
    pub fn set_capture_device(&self, device: Option<String>) -> Result<(), ConnectionError> {
        // The new path opens before the old one is dropped, so a microphone that
        // turns out to be gone leaves the session speaking instead of silent.
        // `switch_capture` is what carries mudo and the rest across — in
        // the core, so the terminal client gets the same list of what survives,
        // and the chosen output with it.
        let pedido = device.clone();
        self.switch_device(|running, media, ssrc| {
            running
                .switch_capture(device.as_deref(), media, ssrc)
                .map_err(|error| {
                    tracing::warn!(%error, "could not open the chosen microphone");
                    ConnectionError::CaptureDeviceGone
                })
        })?;
        self.conferir_troca(pedido.as_deref(), Lado::Entrada)
    }

    /// Pede um quadro-chave a quem está compartilhando a tela.
    ///
    /// Chamado por quem **recebe**, quando descobre que há uma transmissão em
    /// curso: sem isto, quem entra numa sala que já compartilha recebe só
    /// diferenças de um quadro que nunca viu. Ver [`Command::RequestKeyFrame`].
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] sem sessão.
    pub fn pedir_quadro_chave(&self, tela: u32) -> Result<(), ConnectionError> {
        self.command(Command::RequestKeyFrame {
            tela: seele_core::ScreenId(tela),
        })
    }

    /// Switches this session to another sound output.
    ///
    /// `device` is a [`PlaybackDevice::id`] from [`playback_devices`]; `None`
    /// goes back to the machine's default. The microphone stays where it is.
    ///
    /// Takes effect **now**, for the reason its twin does and one more: somebody
    /// changes output in the middle of a conversation they cannot hear, and
    /// "leave the server and come back" is not an instruction you can give
    /// somebody who is already unable to follow what is being said.
    ///
    /// Isolamento total survives the switch. That is decided in
    /// `Voice::switch_playback`, not here, so the terminal cannot decide it
    /// differently — and it matters because changing output is exactly what
    /// somebody does when they cannot hear anything, muted speakers included.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::PlaybackDeviceGone`] when the machine is not offering that
    /// device any more, in which case **nothing changed** and the sound is still
    /// coming out of the old one. [`ConnectionError::NoAudioDevice`] when this session
    /// has no audio at all.
    pub fn set_playback_device(&self, device: Option<String>) -> Result<(), ConnectionError> {
        let pedido = device.clone();
        self.switch_device(|running, media, ssrc| {
            running
                .switch_playback(device.as_deref(), media, ssrc)
                .map_err(|error| {
                    tracing::warn!(%error, "could not open the chosen sound output");
                    ConnectionError::PlaybackDeviceGone
                })
        })?;
        self.conferir_troca(pedido.as_deref(), Lado::Saida)
    }

    /// Confere que a troca **aconteceu**, e não só que ninguém devolveu erro.
    ///
    /// # Por que isto existe
    ///
    /// Trocar a saída de som no Windows era relatado assim: «ele mostra EM USO
    /// num e ESCOLHIDO no que eu escolhi, mas não muda». `EM USO` sai do
    /// dispositivo que o `Voice` **abriu**, e `ESCOLHIDO` da preferência —
    /// então os dois discordando é o produto dizendo, sem saber, que a ordem
    /// foi gravada e não cumprida.
    ///
    /// Nada nesse caminho devolvia erro: `resolve` acha o dispositivo, `open`
    /// abre, a troca volta `Ok`. Um sucesso que não é sucesso é a forma exata
    /// dos três defeitos que custaram versões inteiras nesta casca — o `{}` do
    /// compartilhamento, o quadro-chave descartado, o assento devolvido sem
    /// anúncio. Todos silenciosos, todos achados só quando alguém foi conferir
    /// se o que se pediu foi feito.
    ///
    /// Conferir é barato: uma leitura de campo depois de uma troca que só
    /// acontece quando alguém aperta um botão. E ela é o que transforma «não
    /// muda e não diz nada» em uma frase que aponta para o lugar certo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::PlaybackDeviceGone`] ou
    /// [`ConnectionError::CaptureDeviceGone`] quando o dispositivo aberto não é
    /// o que foi pedido. Sem pedido — o padrão do sistema — não há o que
    /// conferir: qualquer dispositivo que abra é o certo.
    fn conferir_troca(&self, pedido: Option<&str>, lado: Lado) -> Result<(), ConnectionError> {
        let Some(pedido) = pedido else {
            return Ok(());
        };
        let Ok(voice) = self.shared.voice.lock() else {
            return Ok(());
        };
        let Some(running) = voice.as_ref() else {
            return Ok(());
        };
        let aberto = match lado {
            Lado::Entrada => running.capture().map(|d| d.id.clone()),
            Lado::Saida => running.playback().map(|d| d.id.clone()),
        };
        if aberto.as_deref() == Some(pedido) {
            return Ok(());
        }
        tracing::warn!(
            ?pedido,
            ?aberto,
            ?lado,
            "a troca de dispositivo voltou sem erro e o aberto não é o pedido"
        );
        Err(match lado {
            Lado::Entrada => ConnectionError::CaptureDeviceGone,
            Lado::Saida => ConnectionError::PlaybackDeviceGone,
        })
    }

    /// Replaces the live voice path with one the caller reopens.
    ///
    /// Shared by both switches so that the order stays one decision: open the
    /// new path, and only then let go of the old. Written twice, one copy would
    /// eventually drop first and leave somebody with no audio at all on the one
    /// path where the device turned out to be missing.
    fn switch_device(
        &self,
        reopen: impl FnOnce(&Voice, MediaChannel, Ssrc) -> Result<Voice, ConnectionError>,
    ) -> Result<(), ConnectionError> {
        let Ok(mut voice) = self.shared.voice.lock() else {
            return Err(ConnectionError::NoAudioDevice);
        };
        let Some(running) = voice.as_ref() else {
            return Err(ConnectionError::NoAudioDevice);
        };
        let Some((media, ssrc)) = self.shared.media.lock().ok().and_then(|slot| slot.clone())
        else {
            return Err(ConnectionError::NotConnected);
        };

        let fresh = reopen(running, media, ssrc)?;
        *voice = Some(fresh);
        Ok(())
    }

    /// Chooses how the microphone opens.
    pub fn set_voice_mode(&self, mode: VoiceMode) {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                voice.set_mode(mode.into());
            }
        }
    }

    /// Sets one talker's volume, as a percentage. 100 is unchanged.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::UnknownPerson`] if nobody here is called that,
    /// [`ConnectionError::NoAudioDevice`] if this session has no audio.
    pub fn set_volume(&self, nickname: String, percent: u16) -> Result<(), ConnectionError> {
        let ssrc = self
            .shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.ssrc_of(&nickname))
            .ok_or(ConnectionError::UnknownPerson)?;

        let voice = self
            .shared
            .voice
            .lock()
            .map_err(|_| ConnectionError::NoAudioDevice)?;
        let voice = voice.as_ref().ok_or(ConnectionError::NoAudioDevice)?;
        voice.set_gain(ssrc.get(), f32::from(percent.min(400)) / 100.0);
        Ok(())
    }

    // ---- compartilhamento de tela ----
    //
    // Seis métodos, e a forma deles é o contrato de 22/08, fixado antes desta
    // ponte e desta casca para que as duas pudessem ser escritas ao mesmo
    // tempo. O que está escrito abaixo de cada um é o que **desta** metade
    // existe hoje; o relatório da onda 2 diz o resto com todas as letras.

    /// As telas e janelas que esta máquina pode transmitir.
    ///
    /// Uma lista vazia significaria «o sistema recusou a permissão», e aí
    /// [`Connection::permissao_de_tela`] diria qual foi a recusa — uma lista vazia sem
    /// motivo é um beco. Por isso a falta de captura sai como erro e não como
    /// lista curta.
    ///
    /// O número em [`FonteDeTela::id`] é o índice **desta** listagem, e a casca
    /// tem de devolver um dos que acabou de receber: listar de novo renumera.
    /// Ver `seele_core::FonteDeTela::id`, que diz por que um identificador
    /// estável não existe nos dois sistemas.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ScreenShareUnavailable`] quando esta máquina não tem
    /// captura de tela, ou quando o sistema recusou listar. Uma sessão sem
    /// monitor e sem janela **não** é erro: a lista vem vazia, porque não ter o
    /// que compartilhar é estado.
    pub fn fontes_de_tela(&self) -> Result<Vec<FonteDeTela>, ConnectionError> {
        let fontes = match seele_core::fontes_de_tela() {
            Ok(fontes) => fontes,
            // **Vazio, e não erro**, e é o contrato escrito acima: uma recusa de
            // permissão é a lista vazia que [`Connection::permissao_de_tela`] explica.
            // Devolvê-la como erro faria a casca escrever «esta máquina não tem
            // como compartilhar» ao lado do bloco que oferece pedir a permissão
            // — duas frases sobre o mesmo estado, e a primeira mentindo.
            Err(seele_core::ErroDeFontes::SemPermissao) => Vec::new(),
            Err(erro) => {
                tracing::debug!(%erro, "não deu para listar o que esta máquina compartilha");
                return Err(ConnectionError::ScreenShareUnavailable);
            }
        };

        let lista = fontes
            .iter()
            .map(|fonte| FonteDeTela {
                id: fonte.id(),
                nome: fonte.rotulo().to_owned(),
                monitor: fonte.monitor(),
                largura: fonte.largura(),
                altura: fonte.altura(),
            })
            .collect();

        // A lista fica deste lado inteira: o alvo nativo é o que a captura
        // precisa, e ele não cabe numa travessia de JSON.
        if let Ok(mut guardadas) = self.shared.fontes_de_tela.lock() {
            *guardadas = fontes;
        }
        Ok(lista)
    }

    /// O que o sistema operacional respondeu sobre gravar a tela.
    ///
    /// Não pergunta nada: só olha. Quem pergunta é
    /// [`Connection::pedir_permissao_de_tela`], e a separação é do §4 — no macOS o
    /// alerta do TCC aparece **uma vez só** por instalação, então uma consulta
    /// que perguntasse gastaria a única chance que a pessoa tem.
    #[must_use]
    pub fn permissao_de_tela(&self) -> PermissaoDeTela {
        permissao_daqui(seele_core::permissao_de_tela())
    }

    /// Pede a permissão ao sistema.
    ///
    /// No Windows devolve `Concedida` sem perguntar nada, porque lá não há
    /// permissão de sistema — o único consentimento é o da nossa interface. No
    /// macOS abre o alerta do TCC, **uma vez**: da segunda em diante o sistema
    /// responde o mesmo que a consulta sem mostrar nada, e o caminho de volta
    /// são os Ajustes.
    #[must_use]
    pub fn pedir_permissao_de_tela(&self) -> PermissaoDeTela {
        permissao_daqui(seele_core::pedir_permissao_de_tela())
    }

    /// Começa a transmitir a fonte escolhida, com os limites escolhidos.
    ///
    /// Uma por sala de voz: se outra pessoa já estiver compartilhando, quem
    /// perde a corrida é avisado pelo servidor — [`NoticeReason::ScreenShareTaken`]
    /// num [`Event::NoticeRaised`]. Esta ponte **não** confere a corrida por
    /// conta própria: `specs/08-seguranca.md` põe a decisão no servidor, e uma
    /// casca que recusasse aqui seria uma segunda autoridade discordando da
    /// primeira no primeiro atraso de rede.
    ///
    /// **`Ok` quer dizer «foi pedido», e não «está saindo».** O `StartScreenShare`
    /// sai daqui; o nome da transmissão volta depois, num `ScreenShareStarted`,
    /// e é só nesse instante que o codificador nasce. A casca vê o resultado
    /// pelo [`Snapshot`], como vê todo o resto.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ScreenShareUnavailable`] quando esta máquina não tem como
    /// começar: a fonte escolhida não está na última lista, o módulo do Cisco
    /// não está em disco, ou este build não tem captura.
    /// [`ConnectionError::NotConnected`] quando a sessão já acabou.
    pub fn compartilhar_tela(
        &self,
        fonte: u64,
        limites: LimitesDeTela,
    ) -> Result<(), ConnectionError> {
        let comecou = self.comecar_a_transmitir(fonte, limites);
        // O pedido é guardado **quando há transmissão a que ele pertença**, e
        // apagado quando não há. Guardá-lo depois de uma recusa poria, na coluna
        // do que foi pedido, o teto de um botão que devolveu erro — e o §5 quer
        // ali a metade que explica a outra, não um número solto.
        self.shared
            .gravar_pedido_da_tela(comecou.is_ok().then_some(limites));
        comecou
    }

    /// A metade que abre a transmissão de verdade, separada para que
    /// [`Connection::compartilhar_tela`] fique sendo só a memória do que foi pedido.
    ///
    /// As três recusas que ela sabe dar acontecem **antes** de qualquer coisa
    /// sair pelo fio, e é para isso que ela é síncrona: uma fonte que sumiu ou
    /// um codec que não está em disco viram uma resposta na mão de quem apertou,
    /// e não um silêncio no laço de comandos.
    fn comecar_a_transmitir(
        &self,
        fonte: u64,
        limites: LimitesDeTela,
    ) -> Result<(), ConnectionError> {
        let escolhida = self.fonte_escolhida(fonte)?;
        let biblioteca = modulo_de_video()?;
        let pedido = seele_core::PedidoDeTela {
            biblioteca,
            origem: escolhida.origem(),
            captura: seele_core::CapturaEmCaixa::nova(escolhida.captura()),
        };
        self.command(Command::ShareScreen {
            pedido: Box::new(pedido),
            limites: limites_do_nucleo(limites),
        })
    }

    /// Tira da última listagem a fonte que a casca escolheu.
    ///
    /// Lista de novo quando não há listagem guardada, e **só** nesse caso: uma
    /// casca que desenhou o menu tem a lista, e relistar por baixo dela
    /// renumeraria os índices no meio da escolha.
    fn fonte_escolhida(&self, fonte: u64) -> Result<seele_core::FonteDeTela, ConnectionError> {
        let mut guardadas = self
            .shared
            .fontes_de_tela
            .lock()
            .map_err(|_| ConnectionError::ScreenShareUnavailable)?;
        if guardadas.is_empty() {
            *guardadas = seele_core::fontes_de_tela().map_err(|erro| {
                tracing::debug!(%erro, "não deu para listar o que esta máquina compartilha");
                ConnectionError::ScreenShareUnavailable
            })?;
            // Aqui a recusa de permissão **é** erro, ao contrário de
            // [`Connection::fontes_de_tela`]: ali a lista vazia é uma resposta que a
            // tela sabe desenhar, e aqui alguém já escolheu uma fonte que não
            // existe mais.
        }
        let onde = guardadas
            .iter()
            .position(|candidata| candidata.id() == fonte)
            .ok_or(ConnectionError::ScreenShareUnavailable)?;
        Ok(guardadas.swap_remove(onde))
    }

    /// Para de transmitir.
    ///
    /// Idempotente: parar sem estar compartilhando não é erro, e é a resposta
    /// que esta máquina dá hoje, porque ela nunca chega a compartilhar.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] quando a sessão já acabou.
    pub fn parar_de_compartilhar(&self) -> Result<(), ConnectionError> {
        // O que foi pedido morre com a transmissão, e parar é uma das duas
        // maneiras de ela acabar — a outra é o `ScreenShareStopped` que `fold`
        // dobra. As duas limpam, porque só uma delas acontece de cada vez:
        // quando esta pessoa aperta PARAR, o quadro do servidor pode nunca chegar.
        self.shared.gravar_pedido_da_tela(None);
        self.command(Command::StopScreenShare)
    }

    /// Muda os tetos no meio da transmissão.
    ///
    /// **Recomeça o fluxo quando a resolução ou a cadência mudam**, e não há
    /// como não recomeçar: o tamanho da imagem vai no cabeçalho de abertura, e
    /// um fluxo que abriu dizendo 1080p não tem onde dizer «daqui para frente é
    /// 720p». Quem assiste vê a imagem piscar uma vez. Mexer só na banda não
    /// pisca nada — ela é uma perna do teto, e o teto se ajusta dentro do fluxo.
    ///
    /// Uma escolha idêntica à que já vale não faz nada, e é por isso que apertar
    /// APLICAR duas vezes seguidas não pisca a tela de ninguém.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] quando a sessão já acabou.
    pub fn ajustar_limites_da_tela(&self, limites: LimitesDeTela) -> Result<(), ConnectionError> {
        // Guardado aqui, e não quando a bomba responder: a coluna «pedido» do
        // painel é a escolha da pessoa, e ela vale desde o aperto. A coluna de
        // ao lado — o que está saindo — é que espera o degrau acompanhar, e as
        // duas juntas são o §5 inteiro: escolher 1080p e receber 720p não é
        // defeito, esconder que aconteceu é.
        self.shared.gravar_pedido_da_tela(Some(limites));
        self.command(Command::AdjustScreenLimits {
            limites: limites_do_nucleo(limites),
        })
    }

    /// Subscribes to changes.
    ///
    /// Called back on the driver thread. The shell marshals.
    pub fn subscribe(&self, listener: Arc<dyn EventListener>) {
        if let Ok(mut listeners) = self.shared.listeners.lock() {
            listeners.push(listener);
        }
    }

    /// The conversation in the open Channel, oldest first.
    ///
    /// Separate from [`Connection::snapshot`] because the two change at completely
    /// different rates. Telemetry moves on its own and wants reading twice a
    /// second; the history only moves when somebody says something. Carrying
    /// both in one value meant paying for the conversation on every frame —
    /// cloning each nickname and body, serialising the lot — so a session got
    /// slower the longer it went on. Ask for this when
    /// [`Snapshot::messages_revision`] changes, and not otherwise.
    #[must_use]
    pub fn messages(&self) -> Vec<Message> {
        self.shared
            .room
            .lock()
            .map(|room| messages_of(&room))
            .unwrap_or_default()
    }

    /// Everything the interface needs, in one value.
    ///
    /// Cheap enough to call on every frame of a redraw, and deliberately a copy:
    /// a shell holding a borrow into live state is a shell that can see the
    /// roster change halfway through drawing it.
    ///
    /// "Cheap" is now true. It used to carry the whole conversation, which made
    /// the cost grow with the session — see [`Snapshot::messages_revision`].
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let room = match self.shared.room.lock() {
            Ok(room) => room.clone(),
            Err(_) => Room::new(),
        };
        // **O nome vem do roster quando o roster nos conhece.**
        //
        // O `Mutex` local guarda o que se digitou ao conectar, e só isso: ele é
        // escrito uma vez, na construção. Trocar de apelido manda um comando, o
        // servidor confirma com `PersonRenamed`, o roster muda — e este número
        // continuava lendo o nome antigo, para sempre. Quem trocasse via o
        // próprio nome mudar na lista de gente e **não** no bloco do operador,
        // que é onde ele está escrito ao lado do próprio retrato. Foi relatado
        // assim: «nome do usuário não altera em tempo real».
        //
        // O `Mutex` continua sendo a resposta antes de o servidor apresentar
        // esta conexão a si mesma — entre o `connect` e o `Welcome` não há
        // roster, e um nome vazio ali seria a janela dizendo que ninguém está
        // usando ela.
        let nickname = room
            .me
            .and_then(|me| room.people.get(&me))
            .map(|person| person.nickname.clone())
            .unwrap_or_else(|| {
                self.shared
                    .nickname
                    .lock()
                    .map(|name| name.clone())
                    .unwrap_or_default()
            });

        let audio = self.audio_state();

        let signal = self.shared.signal.load(Ordering::Relaxed);
        #[allow(
            clippy::cast_precision_loss,
            reason = "a round trip in microseconds is far below f32's exact range"
        )]
        let rtt_ms = self.shared.rtt_micros.load(Ordering::Relaxed) as f32 / 1000.0;

        Snapshot {
            caminho: self.shared.caminho(),
            link: self.shared.enlace(),
            link_state: link_state_from_byte(self.shared.link_state.load(Ordering::Relaxed)),
            server: room.server.clone(),
            icon_revision: self.shared.icon_revision.load(Ordering::Relaxed),
            person_icons_revision: self.shared.person_icons_revision.load(Ordering::Relaxed),
            me: room.me.map(|person| person.0),
            nickname,
            voice_rooms: voice_rooms_of(&room),
            presentes: presentes_de(&room),
            channels: lines_of(&room),
            messages_revision: self.shared.messages_revision.load(Ordering::Relaxed),
            telemetry: Telemetry {
                rtt_ms,
                // O jitter que a pessoa quer saber é o de chegada, medido
                // aqui — e não o do relatório do servidor, que é sempre `0.0`
                // porque o servidor não tem como medir jitter. Ver
                // [`Shared::jitter_de_chegada_micros`].
                jitter_ms: self.shared.jitter_de_chegada_ms(),
                loss_fraction: room.telemetry.as_ref().map_or(0.0, |t| t.loss_fraction),
                bitrate_bps: audio.bitrate_bps,
                signal,
                sync_band: SignalBand::of(signal).into(),
                input_level: audio.input_level,
                local_fault: audio.local_fault,
                frames_refused: audio.frames_refused,
            },
            notice: room.notice.as_ref().map(|notice| Notice {
                severity: notice.severity.into(),
                reason: notice.reason.into(),
                operator_text: notice.operator_text.clone(),
            }),
            muted: audio.muted,
            total_isolation: audio.total_isolation,
            speaking: audio.speaking,
            voice_mode: audio.mode,
            audio_available: audio.available,
            capture: audio.capture,
            playback: audio.playback,
            may_manage_voice_rooms: room
                .permissions
                .contains(&seele_core::Permission::ManageVoiceRooms),
            may_kick: room.permissions.contains(&seele_core::Permission::Kick),
            may_ban: room.permissions.contains(&seele_core::Permission::Ban),
            may_remove_message: room
                .permissions
                .contains(&seele_core::Permission::RemoveMessage),
            may_move_person: room
                .permissions
                .contains(&seele_core::Permission::MovePerson),
            may_customise_server: room
                .permissions
                .contains(&seele_core::Permission::AdministerServer),
            may_delete_rooms: room
                .permissions
                .contains(&seele_core::Permission::AdministerServer),
            tela: tela_de(&room, self.shared.pedido_da_tela()),
            ended: room.ended.map(|end| end.reason.into()),
        }
    }

    /// Ends the session.
    pub fn disconnect(&self) {
        self.shared.running.store(false, Ordering::Relaxed);
        let _ = self.commands.send(Command::Shutdown);
    }

    fn command(&self, command: Command) -> Result<(), ConnectionError> {
        self.commands
            .send(command)
            .map_err(|_| ConnectionError::NotConnected)
    }

    fn audio_state(&self) -> AudioState {
        let Ok(voice) = self.shared.voice.lock() else {
            return AudioState::silent();
        };
        let Some(voice) = voice.as_ref() else {
            return AudioState::silent();
        };
        let telemetry = voice.telemetry();
        AudioState {
            available: true,
            mode: voice.mode().into(),
            speaking: telemetry.local.speaking,
            muted: voice.muted(),
            total_isolation: voice.total_isolation(),
            input_level: telemetry.local.input_level,
            local_fault: voice.falha_local(),
            frames_refused: voice.quadros_recusados(),
            bitrate_bps: telemetry.local.bitrate_bps,
            capture: voice.capture().map(|device| CaptureDevice {
                id: device.id.clone(),
                name: device.name.clone(),
                default: device.default,
            }),
            playback: voice.playback().map(|device| PlaybackDevice {
                id: device.id.clone(),
                name: device.name.clone(),
                default: device.default,
            }),
        }
    }
}

/// What the voice path is doing, read once per [`Connection::snapshot`].
///
/// A struct and not the eight-tuple this was: the tuple had already reached the
/// point of needing `clippy::type_complexity` waved through, and its run of
/// consecutive `bool`s was three chances to swap two fields with nothing
/// anywhere to catch it.
struct AudioState {
    available: bool,
    mode: VoiceMode,
    speaking: bool,
    muted: bool,
    total_isolation: bool,
    input_level: f32,
    local_fault: bool,
    frames_refused: u64,
    bitrate_bps: u32,
    capture: Option<CaptureDevice>,
    playback: Option<PlaybackDevice>,
}

impl AudioState {
    /// A session with no voice path — before audio opens, and after it stops.
    ///
    /// Push-to-talk rather than a derived `Default`, and that is the whole
    /// reason this is written out: `specs/03-audio.md` picks the mode that never
    /// false-triggers, and a placeholder that read as `Open` would draw an open
    /// microphone on a session that has none.
    fn silent() -> Self {
        Self {
            available: false,
            mode: VoiceMode::PushToTalk,
            speaking: false,
            muted: false,
            total_isolation: false,
            input_level: 0.0,
            local_fault: false,
            frames_refused: 0,
            bitrate_bps: 0,
            capture: None,
            playback: None,
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.disconnect();
    }
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
fn presentes_de(room: &Room) -> Vec<Person> {
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
            sync_band: SignalBand::of(person.signal).into(),
            is_self: room.me == Some(person.id),
        })
        .collect()
}

fn voice_rooms_of(room: &Room) -> Vec<VoiceRoom> {
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
                    sync_band: SignalBand::of(person.signal).into(),
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

fn lines_of(room: &Room) -> Vec<Channel> {
    room.channels
        .iter()
        .map(|channel| Channel {
            id: channel.id.0,
            name: channel.name.clone(),
            open: room.current_channel == Some(channel.id),
        })
        .collect()
}

fn messages_of(room: &Room) -> Vec<Message> {
    room.messages
        .iter()
        .map(|message| Message {
            id: message.id.0,
            channel: message.channel.0,
            author: message.author.0,
            author_nickname: message.author_nickname.clone(),
            at_seconds: message.at_seconds,
            body: message.body.clone(),
            own: message.own,
            edited: message.edited,
            attachment: message.attachment.as_ref().map(|anexo| Attachment {
                id: anexo.id.get(),
                file_name: anexo.file_name.clone(),
                declared_type: anexo.declared_type.clone(),
                byte_size: anexo.byte_size,
                // Flattened to a boolean, and it is the whole state: the wire
                // enum has two variants and a third would be a state the design
                // does not have. ADR 0027 argues for exactly two — "this file
                // has not arrived yet" was deliberately never made a state, so
                // that it could not be confused with this one.
                expired: anexo.state == seele_core::AttachmentState::Expired,
            }),
        })
        .collect()
}

/// A resposta do núcleo, no vocabulário que atravessa a ponte.
///
/// Um `match` exaustivo de propósito, como [`nome_da_parada`]: uma resposta nova
/// em `seele-core` reprova aqui em vez de virar silenciosamente outra.
const fn permissao_daqui(resposta: seele_core::PermissaoDeTela) -> PermissaoDeTela {
    match resposta {
        seele_core::PermissaoDeTela::Concedida => PermissaoDeTela::Concedida,
        seele_core::PermissaoDeTela::Negada => PermissaoDeTela::Negada,
        seele_core::PermissaoDeTela::NaoPerguntada => PermissaoDeTela::NaoPerguntada,
        seele_core::PermissaoDeTela::SemCaptura => PermissaoDeTela::NaoSeSabe,
    }
}

/// O que a pessoa escolheu, traduzido para os degraus fechados do §5.
///
/// **Para baixo, sempre**: a escolha é teto, então um número entre dois degraus
/// vira o degrau de baixo, e um número abaixo do menor vira o menor — que é o
/// piso da lista e não uma quarta opção. Arredondar para cima seria a ponte
/// entregando mais do que a pessoa pediu, que é o §5 ao contrário.
fn limites_do_nucleo(limites: LimitesDeTela) -> seele_core::LimitesDeTela {
    let resolucao = seele_core::Resolucao::TODAS
        .into_iter()
        .rev()
        .find(|degrau| u32::try_from(degrau.altura()).unwrap_or(u32::MAX) <= limites.altura_maxima)
        .unwrap_or(seele_core::Resolucao::P540);
    let cadencia = seele_core::Cadencia::TODAS
        .into_iter()
        .rev()
        .find(|degrau| degrau.hz() <= limites.quadros_maximos)
        .unwrap_or(seele_core::Cadencia::Q8);
    let prioridade = match limites.prioridade {
        crate::types::Prioridade::Movimento => seele_core::tela::Prioridade::Movimento,
        crate::types::Prioridade::Nitidez => seele_core::tela::Prioridade::Nitidez,
    };
    seele_core::LimitesDeTela {
        banda_bps: limites.banda_bps,
        resolucao,
        cadencia,
        prioridade,
    }
}

/// Onde o módulo do Cisco é procurado, na ordem.
///
/// A lista vem daqui porque `seele_video::modulo::procurar_em` a pede de fora, e
/// o argumento dele é o desta casa: *onde os arquivos do produto moram é decisão
/// da casca*. Esta é a casca.
///
/// A ordem é uma decisão. Primeiro o que quem roda apontou, que é como se
/// desenvolve e como se depura; depois a pasta do executável, que é onde um
/// pacote o carrega; depois `../Frameworks`, que é onde um `.app` do macOS o
/// põe; depois a pasta de build, que é onde ele cai numa árvore de fonte.
/// Base64 padrão, com o preenchimento que o `atob` da janela espera.
///
/// Sem o crate `base64`, pela mesma razão que `seele-video` escreveu quando
/// recusou o `hex`: ele entraria na árvore só para isto, e isto são vinte
/// linhas. A tabela é a do RFC 4648, que é a que o navegador decodifica.
///
/// **Público desde a imagem de perfil.** A casca desenha figuras que vêm da
/// rede, e uma `data:` URL é a única forma de pôr bytes num `<img>` sem um
/// arquivo no disco. O `seele-app` precisava exatamente disto, e escrever um
/// segundo codificador lá seria dois lugares para o mesmo alfabeto — a forma
/// de defeito que o `de_base64` logo abaixo existe para não repetir.
pub fn base64_de(bytes: &[u8]) -> String {
    const TABELA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut texto = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for grupo in bytes.chunks(3) {
        // Três bytes viram quatro caracteres de seis bits. Os que faltam no
        // último grupo entram como zero e saem como `=`.
        let (a, b, c) = (
            u32::from(grupo.first().copied().unwrap_or(0)),
            u32::from(grupo.get(1).copied().unwrap_or(0)),
            u32::from(grupo.get(2).copied().unwrap_or(0)),
        );
        let junto = (a << 16) | (b << 8) | c;
        for casa in 0..4 {
            // Cada casa depois do que o grupo de fato trouxe vira `=`: dois
            // bytes dão três caracteres e um `=`, um byte dá dois e dois `=`.
            if casa > grupo.len() {
                texto.push('=');
                continue;
            }
            let indice = ((junto >> (18 - 6 * casa)) & 0x3F) as usize;
            // `get` e não `[]`: seis bits mascarados não podem passar de 63 e a
            // tabela tem 64, mas quem lê esta linha não tem por que refazer essa
            // conta para acreditar nela — e `indexing_slicing` é aviso aqui de
            // propósito, justamente para não haver a linha que só está certa
            // porque alguém conferiu.
            if let Some(letra) = TABELA.get(indice) {
                texto.push(char::from(*letra));
            }
        }
    }
    texto
}

/// A volta: base64 padrão para os bytes que ele descreve.
///
/// **Mora colada na ida, e é regra e não arrumação.** Um par de conversões
/// escrito em dois arquivos é um par que ganha um caso de um lado só — e este
/// projeto pagou por isso em campo no mesmo mês: o `palco-imagem.js` declarava
/// o perfil do vídeo que o `codec.rs` decidia, e os dois deixaram de concordar
/// sem que nada avisasse.
///
/// Existe porque uma imagem **colada** não tem caminho de arquivo: o que a
/// janela tem são bytes, e a ponte carrega texto. A janela codifica com o que
/// já tem — `FileReader`, nativo — e este lado desfaz.
///
/// `None` quando o texto não é base64: caractere fora da tabela, ou um resto de
/// um caractere só, que não completa byte nenhum. Devolver bytes truncados
/// daria um arquivo corrompido com cara de arquivo bom, e o defeito apareceria
/// como «esta imagem não abre» na máquina de quem recebeu.
#[must_use]
pub fn de_base64(texto: &str) -> Option<Vec<u8>> {
    /// O inverso da tabela do RFC 4648, resolvido por conta e não por tabela:
    /// são quatro faixas contíguas, e escrever 256 entradas à mão seria mais
    /// linhas e mais lugares para errar uma.
    const fn valor(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some((byte - b'A') as u32),
            b'a'..=b'z' => Some((byte - b'a') as u32 + 26),
            b'0'..=b'9' => Some((byte - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    // O preenchimento sai antes: ele marca o fim e não carrega bits. Espaço em
    // branco também — quem cola base64 de um e-mail cola as quebras de linha
    // junto, e recusar por causa delas seria recusar o caso comum.
    let uteis: Vec<u8> = texto
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();

    let mut bytes = Vec::with_capacity(uteis.len() / 4 * 3);
    for grupo in uteis.chunks(4) {
        // Um caractere sozinho são seis bits: não fecha byte nenhum, e é a
        // marca de um texto cortado no meio.
        if grupo.len() < 2 {
            return None;
        }
        let mut junto = 0u32;
        for (casa, letra) in grupo.iter().enumerate() {
            junto |= valor(*letra)? << (18 - 6 * casa);
        }
        // Dois caracteres dão um byte, três dão dois, quatro dão três.
        for casa in 0..grupo.len() - 1 {
            bytes.push(((junto >> (16 - 8 * casa)) & 0xFF) as u8);
        }
    }
    Some(bytes)
}

fn pastas_do_modulo() -> Vec<std::path::PathBuf> {
    let mut pastas = Vec::new();
    if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
        let caminho = std::path::PathBuf::from(apontado);
        pastas.push(if caminho.is_dir() {
            caminho
        } else {
            caminho
                .parent()
                .map_or_else(|| caminho.clone(), std::path::Path::to_path_buf)
        });
    }
    if let Some(perto) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    {
        pastas.push(perto.join("../Frameworks"));
        // A pasta de build de uma árvore de fonte: o executável mora em
        // `target/debug`, e o módulo em `target`.
        pastas.push(perto.join(".."));
        pastas.push(perto);
    }
    pastas
}

/// O que a casca precisa dizer a quem vai decidir se baixa o módulo.
///
/// Uma pessoa não consente com «baixar um componente»; ela consente com um
/// tamanho, uma origem e um motivo. Os três campos são isso, e nenhum deles é
/// texto de interface: a frase é da casca, e estes são os números que ela põe
/// dentro dela.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuloAOferecer {
    /// De onde vem, inteiro. É do Cisco, e quem lê tem direito de ver isso.
    pub url: String,
    /// Quantos bytes viajam.
    pub bytes: u64,
    /// Como o arquivo se chama depois de instalado.
    pub nome: String,
    /// Onde ele vai ficar. A pasta de configuração, não o pacote — é o que faz
    /// isto acontecer uma vez, e não a cada atualização.
    pub pasta: String,
}

/// O módulo que falta, se é que falta e se é que existe para este sistema.
///
/// `None` nos dois casos em que não há botão a oferecer: o módulo já está em
/// disco, ou este sistema não tem módulo publicado (Linux, e Mac Intel, por
/// enquanto). A casca não precisa distinguir os dois — nos dois ela não mostra
/// a caixa.
#[must_use]
pub fn modulo_de_video_a_baixar(pasta: &str) -> Option<ModuloAOferecer> {
    let pastas = pastas_do_modulo();
    if seele_core::procurar_modulo_de_video(&pastas).is_ok() {
        return None;
    }
    let publicado = seele_core::modulo_de_video_publicado()?;
    Some(ModuloAOferecer {
        url: publicado.url(),
        bytes: publicado.bytes_comprimido,
        nome: publicado.nome_em_disco.to_owned(),
        pasta: pasta.to_owned(),
    })
}

/// Instala o módulo a partir dos bytes comprimidos que a casca buscou.
///
/// **A rede não passa por aqui.** Quem busca é a casca gráfica, que já tem
/// cliente HTTP na árvore por causa do atualizador; somar um segundo aqui seria
/// pagar duas vezes pela mesma coisa. O que é desta camada é o que vem depois
/// dos bytes: conferir o hash fixado, expandir, gravar atômico.
///
/// # Errors
///
/// [`ConnectionError::ScreenModuleRefused`] se os bytes não são os fixados, se o bz2
/// não abre ou se a pasta não aceita o arquivo. Os três dizem a mesma coisa a
/// quem clicou — «não deu, tente de novo» — e o que separa os três está no log.
pub fn instalar_modulo_de_video(pasta: &str, comprimido: &[u8]) -> Result<String, ConnectionError> {
    seele_core::instalar_modulo_de_video(std::path::Path::new(pasta), comprimido)
        .map(|caminho| caminho.display().to_string())
        .map_err(|erro| {
            tracing::warn!(%erro, pasta, bytes = comprimido.len(), "o módulo de vídeo não instalou");
            ConnectionError::ScreenModuleRefused
        })
}

/// O módulo do Cisco, achado e carregado.
///
/// **O produto não vem com codec, e é a licença que impõe isso** — o módulo do
/// Cisco não pode ser redistribuído com este binário. Não achá-lo é o estado
/// normal de quem nunca compartilhou tela, e não um defeito.
fn modulo_de_video() -> Result<seele_core::BibliotecaDeVideo, ConnectionError> {
    seele_core::BibliotecaDeVideo::procurar_e_carregar(&pastas_do_modulo()).map_err(|erro| {
        let onde = seele_core::modulo_de_video_publicado()
            .map_or_else(|| "—".to_owned(), |modulo| modulo.url());
        tracing::warn!(%erro, %onde, "o módulo de vídeo não está nesta máquina");
        // **Este erro, e não `ScreenShareUnavailable`.** A captura está neste
        // app; o que falta é o módulo, que não vem no pacote por licença. A
        // frase daquele diz que o recurso não existe, e quem a lê para de
        // tentar — aconteceu duas vezes em campo, no macOS e no Windows.
        ConnectionError::ScreenModuleMissing
    })
}

/// O nome estável de um motivo de parada, para a casca escrever a frase.
///
/// Um `match` exaustivo de propósito: um motivo novo em `seele-core` reprova
/// aqui, em vez de virar silenciosamente o nome de outro.
fn nome_da_parada(motivo: seele_core::MotivoDeParada) -> &'static str {
    match motivo {
        seele_core::MotivoDeParada::SinalCritico => "SinalCritico",
        seele_core::MotivoDeParada::AbaixoDoPiso => "AbaixoDoPiso",
    }
}

/// Os nomes de parada que podem chegar a uma casca em [`TelaEmCurso::parada`].
///
/// Existe pelo mesmo motivo de [`caminhos`]: o guarda de cobertura de frase da
/// casca lê esta lista, e sem ela os nomes teriam de ser repetidos do outro lado
/// do ADR 0002 — que é o defeito que este ciclo já pagou uma vez.
///
/// À mão, e é uma diferença de [`caminhos`] que vale registrar:
/// `seele_core::MotivoDeParada` não publica um `TODOS`, então a lista é escrita
/// aqui e presa por um teste ao `match` exaustivo que a alimenta.
#[must_use]
pub fn motivos_de_parada_da_tela() -> Vec<&'static str> {
    [
        seele_core::MotivoDeParada::SinalCritico,
        seele_core::MotivoDeParada::AbaixoDoPiso,
    ]
    .into_iter()
    .map(nome_da_parada)
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
fn tela_de(room: &Room, pedido: Option<LimitesDeTela>) -> Option<TelaEmCurso> {
    let voice_room = room.current_voice_room?;
    let tela = room.telas.get(&voice_room)?;
    // Quem compartilha não assiste a si mesmo. É o mesmo N do §5.1, contado do
    // lado de cá — ver o doc de `TelaEmCurso::espectadores` para o que ele não é.
    let espectadores = room
        .roster(voice_room)
        .filter(|person| person.id != tela.person)
        .count();
    let e_minha = room.me == Some(tela.person);
    Some(TelaEmCurso {
        de: tela.person.0,
        e_minha,
        altura: 0,
        quadros: 0,
        kbps: 0,
        // Saturado, e não truncado: uma sala com mais de quatro bilhões de
        // pessoas merece um número errado no fim da escala, e não um pequeno.
        espectadores: u32::try_from(espectadores).unwrap_or(u32::MAX),
        parada: None,
        medida: false,
        // Só para quem compartilha, e a conferência é o campo inteiro: o teto é
        // escolha de quem transmite e não viaja no fio, então o que esta
        // máquina guardou só descreve a **própria** transmissão. Mostrá-lo ao
        // lado da tela de outra pessoa seria a coluna «pedido» exibindo o que
        // um terceiro pediu na vez passada.
        pedido: if e_minha { pedido } else { None },
    })
}

/// Se o painel da tela precisa ser redesenhado.
///
/// Duas causas e não uma, e a segunda é o §5.1: a contagem de espectadores é um
/// campo de [`TelaEmCurso`] e ela anda quando o **roster** anda, sem nenhuma
/// mensagem de tela ter chegado. Sem ela, uma quinta pessoa entrando muda o que
/// está saindo e a tela continua escrevendo o número de antes.
///
/// `ha_tela` é lido **depois** de a mensagem ser dobrada, e por isso uma
/// transmissão que acabou de acabar ainda acende o evento: `changed.telas` é a
/// primeira parcela e não depende dele.
const fn a_tela_mudou(changed: seele_core::Changed, ha_tela: bool) -> bool {
    changed.telas || (changed.roster && ha_tela)
}

/// Maps a wire refusal onto this crate's own.
fn refusal_of(reason: seele_core::AttachmentRefusal) -> AttachmentRefusal {
    match reason {
        seele_core::AttachmentRefusal::NotAllowed => AttachmentRefusal::NotAllowed,
        seele_core::AttachmentRefusal::TooLarge { limit } => AttachmentRefusal::TooLarge { limit },
        seele_core::AttachmentRefusal::NoRoom => AttachmentRefusal::NoRoom,
        seele_core::AttachmentRefusal::SizeMismatch => AttachmentRefusal::SizeMismatch,
        seele_core::AttachmentRefusal::HashDidNotMatch => AttachmentRefusal::HashDidNotMatch,
        seele_core::AttachmentRefusal::RateLimited => AttachmentRefusal::RateLimited,
        seele_core::AttachmentRefusal::Unavailable => AttachmentRefusal::Unavailable,
        seele_core::AttachmentRefusal::NotFound => AttachmentRefusal::NotFound,
        seele_core::AttachmentRefusal::Expired => AttachmentRefusal::Expired,
        seele_core::AttachmentRefusal::Malformed => AttachmentRefusal::Malformed,
    }
}

/// Turns one step of a transfer into what a shell draws.
fn transfer_of(estado: &seele_core::enlace::Transferencia) -> Transfer {
    use seele_core::enlace::Transferencia;
    match estado {
        Transferencia::Subindo { id, feito, total } => Transfer::Sending {
            client_message_id: id.get(),
            done: *feito,
            total: *total,
        },
        Transferencia::Subiu { id } => Transfer::Sent {
            client_message_id: id.get(),
        },
        Transferencia::Recusada { id } => Transfer::Refused {
            client_message_id: id.get(),
        },
        Transferencia::Caiu { id } => Transfer::Fell {
            client_message_id: id.get(),
        },
        Transferencia::Baixando {
            anexo,
            feito,
            total,
        } => Transfer::Receiving {
            attachment: anexo.get(),
            done: *feito,
            total: *total,
        },
        Transferencia::Salvo { anexo, caminho } => Transfer::Saved {
            attachment: anexo.get(),
            path: caminho.display().to_string(),
        },
        Transferencia::NaoSalvou { anexo } => Transfer::NotSaved {
            attachment: anexo.get(),
        },
    }
}

fn link_state_byte(link_state: LinkTrust) -> u8 {
    match link_state {
        LinkTrust::Offline => 0,
        LinkTrust::Unverified => 1,
        LinkTrust::Verified => 2,
    }
}

fn link_state_from_byte(byte: u8) -> LinkTrust {
    match byte {
        1 => LinkTrust::Unverified,
        2 => LinkTrust::Verified,
        _ => LinkTrust::Offline,
    }
}

/// Resolves `host` or `host:port` into an address, a TLS label, and a pin key.
///
/// Three values because they are three things — see the same function in
/// the shell, and `TofuVerifier::new` for why keying the pin by the TLS label
/// was wrong.
/// The split is `seele_core::uri::separar` and not `rsplit_once(':')`: the port
/// separator and an IPv6's own separator are the same character, and doing it by
/// hand here made `[2001:db8::1]:8383` resolve to nothing. ADR 0022, step 2.
fn resolve(target: &str) -> Result<(SocketAddr, String, String), ConnectionError> {
    let alvo = seele_core::uri::separar(target).map_err(|_| ConnectionError::UnresolvableHost)?;
    let address = (alvo.maquina, alvo.porta)
        .to_socket_addrs()
        .map_err(|_| ConnectionError::UnresolvableHost)?
        .next()
        .ok_or(ConnectionError::UnresolvableHost)?;

    // TLS gets the name the M2 certificate carries; the pin gets the address,
    // which is what actually tells one server from another.
    let server_name = if alvo.maquina.parse::<std::net::IpAddr>().is_ok() {
        "localhost".to_owned()
    } else {
        alvo.maquina.to_owned()
    };
    // The pin key is written back in the canonical form rather than as typed, so
    // `[::1]:8383` and `[::1]` file under one entry instead of two. An IPv6 goes
    // back into its brackets: without them the key is ambiguous with `host:port`.
    let pin_key = if address.is_ipv6() && alvo.maquina.contains(':') {
        format!("[{}]:{}", alvo.maquina, alvo.porta)
    } else {
        format!("{}:{}", alvo.maquina, alvo.porta)
    };
    Ok((address, server_name, pin_key))
}

/// Turns a [`ConnectConfig`] and what [`resolve`] made of it into a
/// [`seele_core::enlace::Destino`].
///
/// Pulled apart from `drive` for the same reason `seele_core::enlace` pulls
/// `conferir` apart from `Enlace::conectar`: everything here is a decision on
/// values, and a decision on values does not need a QUIC socket to test.
/// Without this split, whether `expected_fingerprint` actually reaches
/// `impressao_esperada` could only be checked by a live handshake — which is
/// exactly the gap `every_expected_fingerprint_reaches_the_destino_it_promised`
/// below closes.
fn build_destino(
    config: &ConnectConfig,
    address: SocketAddr,
    server_name: &str,
    pin_key: &str,
) -> seele_core::enlace::Destino {
    seele_core::enlace::Destino {
        servidor: address,
        nome_tls: server_name.to_owned(),
        chave_do_pin: pin_key.to_owned(),
        apelido: config.nickname.clone(),
        segredo: config.join_secret.clone(),
        impressao_esperada: config.expected_fingerprint.clone(),
    }
}

/// The driver: connects, then pumps until told to stop.
#[allow(
    clippy::too_many_arguments,
    reason = "one private function, called once, with everything the thread owns"
)]
async fn drive(
    config: ConnectConfig,
    address: SocketAddr,
    server_name: String,
    pin_key: String,
    alternates: Vec<(SocketAddr, String, String)>,
    key: seele_core::SigningKey,
    pins: Arc<FilePinStore>,
    shared: Arc<Shared>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    ready: &std::sync::mpsc::Sender<Result<Trust, ConnectFailure>>,
    olhos: Option<Arc<dyn EventListener>>,
) {
    shared
        .link_state
        .store(link_state_byte(LinkTrust::Unverified), Ordering::Relaxed);

    // `Enlace` e não `Client`: é a sessão que atravessa quedas, com a bateria
    // interna dentro. Antes disto, o app pulava de "conectado" para "encerrado"
    // no primeiro soluço de rede.
    let destinos = std::iter::once((address, server_name.clone(), pin_key.clone()))
        .chain(alternates)
        .map(|(address, server_name, pin_key)| {
            build_destino(&config, address, &server_name, &pin_key)
        })
        .collect();
    // Uma `Chegada` e não a chamada direta ao laço: ela é quem dá nome a cada
    // etapa desta travessia e guarda a trilha. O laço continua exatamente onde
    // estava — com o aviso por candidato e os prazos —, e o que muda aqui é que
    // a falha deixa de ser uma frase só. O bilhete vai junto: com ele, quem
    // entra avisa o ponto de encontro por candidato que precisa de furo, que é
    // o degrau 4 do ADR 0022. Sem bilhete os dois caminhos são o mesmo.
    let chegada = seele_core::chegada::Chegada::nova(destinos, config.bilhete.clone());

    // O ouvinte entra **antes** do bloqueio, que é a coisa inteira: o `watch` da
    // chegada existe desde a tarefa 8 e não tinha um só leitor em produção,
    // porque toda inscrição acontecia depois de a travessia ter terminado.
    //
    // Uma tarefa e não um `select!` no laço abaixo: o laço só existe depois de
    // haver sessão, e estas etapas todas acontecem antes disso.
    let vigia = olhos.map(|ouvinte| {
        let mut etapas = chegada.acompanhar();
        tokio::spawn(async move {
            while etapas.changed().await.is_ok() {
                let etapa = ConnectStage::from(&*etapas.borrow_and_update());
                ouvinte.on_event(Event::ConnectStageChanged { stage: etapa });
            }
        })
    });

    let chegou = chegada.chegar(key, pins).await;
    // Esperada, e não abortada. A `Chegada` já morreu quando esta linha roda —
    // `chegar` a consome —, então o emissor do `watch` já caiu e a tarefa acima
    // termina sozinha depois de entregar a última etapa. Abortar aqui perderia
    // exatamente essa última, que é a que diz como a travessia acabou.
    if let Some(vigia) = vigia {
        let _ = vigia.await;
    }

    let chegado = match chegou {
        Ok(chegado) => chegado,
        Err(falha) => {
            tracing::warn!(motivo = %falha, "could not reach the server");
            registrar_trilha(falha.trilha());
            let _ = ready.send(Err(ConnectFailure {
                error: classify_connect_failure(falha.motivo()),
                trail: falha.trilha().iter().map(step_of).collect(),
            }));
            return;
        }
    };

    // Por qual caminho esta conversa saiu, gravado antes de a casca poder
    // perguntar: quem chama `snapshot()` no instante seguinte ao `connect` já
    // encontra o nome, e não um `None` que vira nome no quadro seguinte.
    //
    // A trilha inteira vai para o log junto, e não só na falha: «venceu o
    // terceiro candidato, público, com aviso» é a linha que responde por que a
    // tela diz `FuroDeNat`, e sem ela o nome na tela não teria como ser
    // conferido contra nada.
    registrar_trilha(&chegado.trilha);
    shared.gravar_caminho(chegado.caminho());
    let mut client = chegado.enlace;

    // `Enlace::conectar` already returned `Err` above for anything that would
    // have made this a `PinDecision::Changed` or a refused invite — see
    // `seele_core::tofu::verdict`'s own note that `Changed` never reaches a
    // caller as a verdict. What is left here is a verdict the shell is allowed
    // to see, not one that had to be turned back into a connection error.
    let trust = Trust::from(client.veredito().clone());

    if let Ok(mut room) = shared.room.lock() {
        room.adopt(client.sessao(), &config.nickname);
    }
    shared
        .link_state
        .store(link_state_byte(LinkTrust::Verified), Ordering::Relaxed);

    remember_media(&shared, client.media(), client.sessao().ssrc);

    if config.audio {
        // `start_preferring` and not `start_on`: it falls back to the machine's
        // own device, per side, rather than refusing the session. A preference
        // written down last week names a device that may be in another room by
        // now, and turning that into a server nobody can enter would make the
        // picker the most dangerous control in the app. The screen shows what
        // actually opened, so the fallback is visible — and per side, so a
        // headset left behind does not also throw away the microphone.
        let opened = Voice::start_preferring(
            &chosen_devices(&config),
            client.media(),
            client.sessao().ssrc,
        );
        match opened {
            Ok(voice) => {
                if let Ok(mut slot) = shared.voice.lock() {
                    *slot = Some(voice);
                }
            }
            // No microphone is not a reason to have no session. The text half
            // works, and `audio_available` says which half this is.
            Err(error) => tracing::warn!(%error, "no audio device; text only"),
        }
    }

    shared.running.store(true, Ordering::Relaxed);
    let _ = ready.send(Ok(trust));

    let mut sync = Signal::new();
    let mut next_tick = Instant::now() + TICK;

    while shared.running.load(Ordering::Relaxed) {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                if !run_command(&client, &shared, command).await {
                    break;
                }
            }

            aviso = client.proximo() => {
                match aviso {
                    seele_core::enlace::Aviso::Mensagem(message) => {
                        // A resposta a uma pergunta, e não um fato sobre a
                        // sala: ela vai para quem perguntou e não entra no
                        // `Room`. Antes do `fold` porque é onde ela para —
                        // `Room::apply` não tem arm que a guarde, de propósito.
                        if let seele_core::ServerMessage::ChannelWeighed {
                            channel, messages, authors, oldest_at_seconds,
                        } = message.as_ref() {
                            shared.answer_weight(ChannelWeight {
                                channel: channel.get(),
                                messages: *messages,
                                authors: *authors,
                                oldest_at_seconds: *oldest_at_seconds,
                            });
                        }
                        fold(&shared, &message);
                        // A razão de uma recusa, que o `Room` guardou ao dobrar
                        // a mensagem. Drenada em vez de lida: duas
                        // transferências podem estar no ar, e a segunda falhar
                        // não pode apagar o motivo da primeira.
                        for aviso in shared.drain_transfers() {
                            shared.notify(&Event::TransferChanged { transfer: aviso });
                        }
                    }
                    seele_core::enlace::Aviso::Transferencia(estado) => {
                        shared.notify(&Event::TransferChanged {
                            transfer: transfer_of(&estado),
                        });
                    }
                    seele_core::enlace::Aviso::TelaAbriu {
                        tela,
                        largura,
                        altura,
                    } => {
                        shared.notify(&Event::ScreenOpened {
                            screen: tela.0,
                            width: largura,
                            height: altura,
                        });
                    }
                    seele_core::enlace::Aviso::TelaQuadro { tela, chave, bytes } => {
                        shared.notify(&Event::ScreenFrame {
                            screen: tela.0,
                            key: chave,
                            data: base64_de(&bytes),
                        });
                    }
                    seele_core::enlace::Aviso::TelaFechou { tela } => {
                        shared.notify(&Event::ScreenClosed { screen: tela.0 });
                    }
                    // Cair não encerra: começa a bateria interna, e a casca
                    // esmaece em vez de fechar.
                    seele_core::enlace::Aviso::Estado { estado, restante } => {
                        shared.gravar_enlace(estado, restante);
                        shared.notify(&Event::TelemetryChanged);
                    }
                    seele_core::enlace::Aviso::Reconectado { media, sessao } => {
                        shared.gravar_enlace(seele_core::Link::Online, None);
                        if let Ok(mut room) = shared.room.lock() {
                            room.adopt(&sessao, &config.nickname);
                        }
                        // Conexão nova, `ssrc` novo, canal de mídia novo: sem
                        // reabrir, a voz sairia por uma conexão morta.
                        remember_media(&shared, (*media).clone(), sessao.ssrc);
                        if let Ok(mut slot) = shared.voice.lock() {
                            if let Some(atual) = slot.as_ref() {
                                // `reopen` e não `start_on`, porque os controles
                                // têm que atravessar a reabertura. A lista mora
                                // em `Voice::switch_capture` — mudo,
                                // Isolamento total, o modo, a tecla segura, cada
                                // ganho por interlocutor — e está lá justamente
                                // para que nenhuma casca esqueça um item. Esta
                                // esquecia todos.
                                //
                                // O que torna isto pior que "volta desmutado" é
                                // que `Enlace::tentar` **restaura** o mudo
                                // no servidor: o roster continuava mostrando a
                                // pessoa muda enquanto o portão local voltava
                                // aberto, e o indicador que todo mundo lê
                                // passava a mentir.
                                //
                                // `reopen` também é quem sabe **o que** reabrir:
                                // a voz guarda os dois dispositivos que pediu, e
                                // volta pedindo os mesmos. Sem isso, voltar do ar
                                // trocaria o microfone e a saída de quem está no
                                // meio de uma conversa — e trocaria calado, que é
                                // a pior parte. O recuo por lado é o mesmo do
                                // caminho de conexão: uma interface que sumiu
                                // enquanto estávamos fora do ar não pode custar a
                                // voz do resto da sessão.
                                match atual.reopen((*media).clone(), sessao.ssrc) {
                                    Ok(voice) => *slot = Some(voice),
                                    // A mesma degradação do caminho de conexão:
                                    // a metade de texto continua funcionando, e
                                    // `audio_available` diz qual metade é esta.
                                    Err(error) => {
                                        tracing::warn!(%error, "no audio device after reconnecting; text only");
                                        *slot = None;
                                    }
                                }
                            }
                        }
                        shared.notify(&Event::TelemetryChanged);
                    }
                    seele_core::enlace::Aviso::Encerrado(motivo) => {
                        let reason = match motivo {
                            seele_core::enlace::Motivo::Descarregou => EndReason::LinkLost,
                            seele_core::enlace::Motivo::Recusado(_) => EndReason::CredentialRejected,
                            seele_core::enlace::Motivo::Pedido => EndReason::LinkLost,
                        };
                        shared.notify(&Event::Ended { reason });
                        break;
                    }
                }
            }

            () = tokio::time::sleep_until(next_tick.into()) => {
                next_tick = Instant::now() + TICK;
                if measure(&mut sync, &client, &shared) {
                    shared.notify(&Event::TelemetryChanged);
                }
            }
        }
    }

    shared.running.store(false, Ordering::Relaxed);
    shared
        .link_state
        .store(link_state_byte(LinkTrust::Offline), Ordering::Relaxed);
    // Dropping the voice stops the audio thread with the session.
    if let Ok(mut voice) = shared.voice.lock() {
        *voice = None;
    }
    // E o canal com ela: um `set_capture_device` que chegasse depois abriria um
    // caminho de voz sobre uma conexão morta, e ele ficaria de pé sem sessão
    // nenhuma por trás.
    if let Ok(mut media) = shared.media.lock() {
        *media = None;
    }
    // Quem estava esperando o peso de uma Linha é acordado, e não deixado
    // pendurado. Largar o remetente é o que faz `weigh_channel` devolver
    // `NotConnected` em vez de esperar uma resposta que não vem mais — e o que
    // impede uma caixa de confirmação de ficar aberta e muda depois de a sessão
    // acabar debaixo dela.
    if let Ok(mut pending) = shared.pending_weights.lock() {
        pending.clear();
    }
    client.sair().await;
}

/// Os dois dispositivos que a casca pediu, como o core os lê.
///
/// Uma função e não dois campos soltos no ponto de chamada: os dois são
/// `Option<String>`, e trocá-los compilaria — a pessoa acabaria com a voz
/// saindo pelo microfone.
fn chosen_devices(config: &ConnectConfig) -> seele_core::DeviceChoice {
    seele_core::DeviceChoice {
        capture: config.capture_device.clone(),
        playback: config.playback_device.clone(),
    }
}

/// Guarda por onde a voz sai, para quem for reabri-la.
fn remember_media(shared: &Arc<Shared>, media: MediaChannel, ssrc: Ssrc) {
    if let Ok(mut slot) = shared.media.lock() {
        *slot = Some((media, ssrc));
    }
}

/// Folds a server message into the room and tells the shell what moved.
fn fold(shared: &Arc<Shared>, message: &seele_core::ServerMessage) {
    let changed = match shared.room.lock() {
        Ok(mut room) => room.apply(message),
        Err(_) => return,
    };

    // A malha do ADR 0036, ligada aqui e não dentro do `Room`.
    //
    // O `Room` é o que se sabe do **servidor**, e encolher o próprio microfone é
    // decisão do lado do áudio; ele não conhece o `Voice` e não deveria. Este é
    // o único lugar que segura os dois.
    //
    // Um servidor v1 nunca manda este quadro, e então nada disto acontece: a
    // faixa fica onde nasceu, que é o teto, e o comportamento é o de antes.
    if let seele_core::ServerMessage::UplinkLoss { fraction } = message {
        if let Ok(voice) = shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                if let Some(bps) = voice.observar_perda(*fraction, std::time::Instant::now()) {
                    // Registrado só quando **muda**, que é o que a malha
                    // devolve. Uma linha por medida seriam sessenta por minuto
                    // por sessão, dizendo quase sempre a mesma coisa.
                    tracing::info!(
                        bps,
                        perda = fraction,
                        "a faixa de bitrate mudou por perda de subida"
                    );
                }
            }
        }
    }

    if changed.roster {
        shared.notify(&Event::RosterChanged);
    }
    if changed.messages {
        shared.messages_changed();
    }
    if changed.channels {
        shared.notify(&Event::ChannelsChanged);
    }
    if changed.server {
        // A revisão sobe **antes** do aviso, como em `messages_changed` e pelo
        // mesmo motivo: uma casca que reage ao evento leria o número velho e
        // concluiria que não há imagem nova para buscar.
        //
        // Só quando foi a imagem que mudou. Um nome novo já viaja inteiro no
        // `Snapshot`, e mexer no número faria toda casca rebuscar 8 KiB por
        // causa de uma string que ela acabou de receber.
        if matches!(message, seele_core::ServerMessage::ServerIconChanged { .. }) {
            shared
                .icon_revision
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        // E o das pessoas, pela mesma razão e com a mesma ordem: antes do aviso.
        if matches!(message, seele_core::ServerMessage::PersonIconChanged { .. }) {
            shared
                .person_icons_revision
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        shared.notify(&Event::ServerChanged);
    }
    if changed.telemetry {
        shared.notify(&Event::TelemetryChanged);
    }
    if changed.notice {
        let notice = shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.notice.clone())
            .map(|notice| Notice {
                severity: notice.severity.into(),
                reason: notice.reason.into(),
                operator_text: notice.operator_text,
            });
        if let Some(notice) = notice {
            shared.notify(&Event::NoticeRaised { notice });
        }
    }
    // Lido do `Room` de novo, e depois da dobra: o que decide entre «acendeu» e
    // «não acendeu» quando só o roster andou é se há transmissão **agora**.
    if changed.telas || changed.roster {
        let tela = shared
            .room
            .lock()
            .ok()
            .and_then(|room| tela_de(&room, None));
        // O que foi pedido morre com a transmissão. Sem esta linha, a próxima
        // vez que esta pessoa compartilhasse mostraria o que está saindo agora
        // ao lado de um teto que ela escolheu noutra ocasião — dois números
        // lado a lado, o da direita mentindo, e nada na tela dizendo qual.
        if !tela.as_ref().is_some_and(|tela| tela.e_minha) {
            shared.gravar_pedido_da_tela(None);
        }
        if a_tela_mudou(changed, tela.is_some()) {
            shared.notify(&Event::ScreenChanged);
        }
    }
    if changed.ended {
        let reason = shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.ended)
            .map_or(EndReason::LinkLost, |end| end.reason.into());
        shared.notify(&Event::Ended { reason });
    }
}

/// Qual das duas grandezas chamadas "jitter" vai para a tela.
///
/// São duas, com o mesmo nome, e mostrar a errada é pior que não mostrar nada:
///
/// - `chegada_ms` é o **jitter de chegada** da RFC 3550 — a variação do
///   intervalo entre pacotes, medida neste receptor por
///   `seele_audio::jitter`, que alisa por `J += (|D(i-1,i)| - J) / 16`. É ruído
///   da rede, e é o que a pessoa quer saber: quanto menor, melhor;
/// - `profundidade_do_anel_ms` é a **profundidade do anel de reprodução** — a
///   nossa própria reserva contra esse ruído, que o ADR 0028 acabou de dotar de
///   alvo. Quanto maior, mais folga o anel teve. Mostrá-la como "jitter" exibiria
///   a reserva como se fosse o problema, e uma conexão saudável apareceria na
///   tela como ruim.
///
/// A resposta é sempre a primeira, e a segunda é ignorada de propósito. Um
/// parâmetro ignorado parece bobo e não é: ele existe para o segundo número ter
/// de ser **escrito** por quem chama, e para quem trocar os dois de lugar
/// reprovar aqui, num teste desta casa, em vez de na tela de outra pessoa.
///
/// O que a tela lia antes deste conserto não era nenhum dos dois: era o jitter
/// do relatório do servidor, que é `0.0` fixo porque o servidor não tem como medir
/// uma grandeza que só existe no receptor.
fn jitter_para_a_tela(chegada_ms: f32, profundidade_do_anel_ms: f32) -> f32 {
    let _ = profundidade_do_anel_ms;
    chegada_ms
}

/// As duas grandezas chamadas "jitter" que uma volta de telemetria produz, já
/// separadas e com destino escrito no nome.
///
/// Existe como estrutura, e não como uma tupla de dois `f32`, porque uma tupla
/// de dois `f32` na mesma unidade é exatamente a forma que deixa trocar os dois
/// sem nada notar — que é o defeito 3.3 outra vez, com outra roupa.
struct JitterDaVolta {
    /// Vai para o Sync Ratio: quanta reserva o anel de reprodução teve.
    profundidade_do_anel_ms: f32,
    /// Vai para a tela: o jitter de chegada da RFC 3550, medido neste receptor.
    ///
    /// Vazio quando não há fonte nenhuma sendo recebida. `None` e não zero de
    /// propósito: o pior de uma lista vazia seria zero, e zero é o valor que
    /// este conserto existe para tirar da tela.
    chegada_ms: Option<f32>,
}

/// Separa as duas, a partir do que a camada de áudio mediu.
///
/// Apartada de [`measure`] para poder ser afirmada por um teste: `measure`
/// precisa de um [`Enlace`] vivo e de um dispositivo de áudio aberto, e sem esta
/// separação a única coisa que sobrava para testar era a função pura dos dois
/// lados da fiação — que passava verde com a fiação desfeita.
fn jitter_da_volta(telemetria: &seele_core::AudioTelemetry) -> JitterDaVolta {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "milissegundos; f32 é o que a ponte para a casca carrega"
    )]
    let volta = JitterDaVolta {
        profundidade_do_anel_ms: telemetria.worst_jitter_depth_ms() as f32,
        // O pior entre as fontes, como já vale para a perda e para a
        // profundidade: um único interlocutor ruim tem de ficar visível sem a
        // casca ter de varrer a lista.
        //
        // A escolha entre os dois `f64` mora na camada de áudio, com nome que
        // diz o destino, e não aqui: a barra da TUI fazia a mesma escolha por
        // conta própria e escolhia errado, e nada num `f64` avisa qual dos dois
        // ele é.
        chegada_ms: telemetria
            .worst_arrival_jitter_ms()
            .map(|chegada| chegada as f32),
    };
    volta
}

/// Refreshes what the shell reads without asking the server.
///
/// Returns whether anything moved enough to be worth telling a shell about.
///
/// Só a fiação: pega a volta de áudio do laço de voz e o ida-e-volta do enlace,
/// e entrega a [`medir_a_volta`], que é onde as decisões moram e onde elas são
/// afirmáveis — esta função pede um [`Enlace`] vivo e um dispositivo de áudio
/// aberto, e não há nenhum dos dois numa máquina de integração contínua.
fn measure(sync: &mut Signal, client: &Enlace, shared: &Arc<Shared>) -> bool {
    let telemetria = shared
        .voice
        .lock()
        .ok()
        .and_then(|voice| voice.as_ref().map(Voice::telemetry));
    let rtt = client.rtt().unwrap_or_default();
    let rtt_micros = u64::try_from(rtt.as_micros()).unwrap_or(u64::MAX);
    medir_a_volta(sync, shared, telemetria.as_ref(), rtt_micros)
}

/// Uma volta de medição: grava o que a casca lê, e diz se vale avisá-la.
///
/// # Por que isto é uma função, e não o corpo de [`measure`]
///
/// Porque a propriedade que este ciclo consertou — o `TelemetryChanged`
/// disparar por variação **só** de jitter — não tinha como ser afirmada dentro
/// de `measure`, e o que sobrava era um guarda que lia o **texto** dela. Esse
/// guarda passa verde com a propriedade quebrada: um braço que grava o jitter e
/// devolve `false` satisfaz tanto «a última linha nomeia `mudou_o_jitter`»
/// quanto «o corpo tem `let mudou_o_jitter = match chegada_ms`».
///
/// Nada aqui precisa de sessão: a telemetria de áudio se monta à mão, o
/// ida-e-volta é um `u64`, e o `Shared` já tinha um construtor de teste. Ler o
/// texto da produção prende a forma da linha; isto prende o que ela faz.
///
/// # As duas grandezas, e os dois destinos
///
/// A profundidade do anel vai para o Sync Ratio, que é o que ela mede — quanta
/// reserva o anel teve. O jitter de chegada vai para a tela, que é o que a
/// pessoa quer saber. Ver [`jitter_para_a_tela`].
///
/// # Sem fonte nenhuma, nada é gravado
///
/// É contrapartida deliberada de não gravar zero: zero é o número que este
/// conserto tirou dali — o relatório do servidor manda `0.0` fixo porque um
/// servidor não tem como medir jitter —, e escrevê-lo quando a voz cai faria a
/// tela afirmar «rede perfeita» sobre uma sessão sem áudio nenhum.
///
/// O preço, dito por escrito porque não estava dito em lugar nenhum: numa sessão
/// viva em que a voz cai no meio, o campo **congela** no último valor em vez de
/// esvaziar. Corrigi-lo pede um terceiro estado na travessia — «medido», «sem
/// medida ainda» e «sem medida agora» —, e nenhuma das duas cascas tem hoje como
/// desenhar a diferença.
fn medir_a_volta(
    sync: &mut Signal,
    shared: &Arc<Shared>,
    telemetria: Option<&seele_core::AudioTelemetry>,
    rtt_micros: u64,
) -> bool {
    let mut jitter_ms = 0.0;
    let mut loss = 0.0;
    let mut chegada_ms = None;
    if let Some(telemetria) = telemetria {
        // Jitter and loss are only observable at the receiver, which is why
        // the server's own numbers are not the ones used here.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a fraction; f32 is what the protocol carries"
        )]
        {
            loss = telemetria.worst_loss_fraction() as f32;
        }
        let volta = jitter_da_volta(telemetria);
        jitter_ms = volta.profundidade_do_anel_ms;
        chegada_ms = volta.chegada_ms;
    }

    let mudou_o_jitter = match chegada_ms {
        Some(chegada) => shared.gravar_jitter_de_chegada(jitter_para_a_tela(chegada, jitter_ms)),
        None => false,
    };

    #[allow(
        clippy::cast_precision_loss,
        reason = "a round trip in microseconds is far below f32's exact range"
    )]
    let ratio = sync.update(SyncInputs {
        rtt_ms: rtt_micros as f32 / 1000.0,
        jitter_ms,
        loss_fraction: loss,
    });

    let previous_ratio = shared.signal.swap(ratio, Ordering::Relaxed);
    let previous_rtt = shared.rtt_micros.swap(rtt_micros, Ordering::Relaxed);

    // A shell redrawing because the round trip moved by a microsecond is a
    // shell redrawing thirty times a second for nothing.
    //
    // O jitter entra nesta conta desde que ele passou a ser um número de
    // verdade. Antes ele era o `0.0` do relatório do servidor e nunca se mexia, e o
    // evento nunca precisou falar dele; agora ele varia sozinho, e sem esta
    // parcela quem depende do evento — em vez de puxar `snapshot` em laço, como
    // o app faz — desenha o número velho até outra coisa mudar.
    previous_ratio != ratio || previous_rtt.abs_diff(rtt_micros) > 1_000 || mudou_o_jitter
}

/// Runs one command. Returns false when the driver should stop.
async fn run_command(client: &Enlace, shared: &Arc<Shared>, command: Command) -> bool {
    match command {
        Command::EnterVoiceRoom(voice_room) => {
            if client.inserir_plug(voice_room).await.is_err() {
                return false;
            }
            if let Ok(mut room) = shared.room.lock() {
                room.enter_voice_room(voice_room);
            }
            shared.notify(&Event::RosterChanged);
        }
        Command::LeaveVoiceRoom => {
            if client.ejetar_plug().await.is_err() {
                return false;
            }
            // O espelho do `EnterVoiceRoom` acima, e ele faltava. O servidor não
            // devolve o `PersonLeft` a quem o causou — «essa pessoa já sabe» —,
            // então esta metade do roster é contabilidade desta casca. Sem ela
            // o assento se esvazia no servidor e em todos os outros clientes, e
            // a única tela que continua desenhando o pessoa na jaula é a de
            // quem acabou de sair dela.
            if let Ok(mut room) = shared.room.lock() {
                room.leave_voice_room();
            }
            shared.notify(&Event::RosterChanged);
        }
        Command::OpenChannel(channel) => {
            if client.abrir_linha(channel).await.is_err() {
                return false;
            }
            if let Ok(mut room) = shared.room.lock() {
                room.open_channel(channel);
            }
            // The fetch is what makes "sem perda de histórico" true: a client
            // arriving late reads what was said instead of an empty room.
            if client.historico(channel, HISTORY_PAGE).await.is_err() {
                return false;
            }
            // A Linha trocou, então a conversa trocou — mesmo que nenhuma
            // mensagem nova tenha chegado. `Room::open_channel` limpou a lista, e
            // sem isto a tela continuaria mostrando a conversa da Linha
            // anterior sob o nome da nova.
            shared.messages_changed();
        }
        Command::Send { channel, body } => {
            // specs/02-protocolo.md: idempotent by client_msg_id, so a resend
            // after a lost acknowledgement does not post twice.
            let id = ClientMessageId(next_client_message_id());
            if client
                .dizer(channel, body.trim().to_owned(), id)
                .await
                .is_err()
            {
                return false;
            }
        }
        Command::Attach(anexo) => {
            if client.anexar(*anexo).await.is_err() {
                return false;
            }
        }
        Command::SaveAttachment {
            attachment,
            destination,
        } => {
            if client.salvar_anexo(attachment, destination).await.is_err() {
                return false;
            }
        }
        Command::SetMuted(on) => {
            if client.muted(on).await.is_err() {
                return false;
            }
        }
        Command::SetTotalIsolation(on) => {
            if client.isolamento(on).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` here, unlike entering a voice room
        // or opening o canal. Those two are facts about this client, which the
        // server confirms by silence; a room is a fact about the server, and the
        // only honest source for it is the server saying it exists. Writing it in
        // optimistically would draw a room for the person who asked even when
        // the server refused them.
        Command::CreateVoiceRoom {
            name,
            limit,
            channel,
        } => {
            if client.criar_voice_room(name, limit, channel).await.is_err() {
                return false;
            }
        }
        Command::CreateChannel { name } => {
            if client.criar_linha(name).await.is_err() {
                return false;
            }
        }
        Command::RenameVoiceRoom { voice_room, name } => {
            if client.renomear_voice_room(voice_room, name).await.is_err() {
                return false;
            }
        }
        Command::RenameChannel { channel, name } => {
            if client.renomear_linha(channel, name).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` here either, and for the
        // sharpest version of the reason above: the server's own name is what
        // every window in the session is drawing. Writing it in optimistically
        // would put the new name on the screen of the one person who is not
        // allowed to change it, and put it there identically whether the server
        // agreed or refused.
        Command::RenameServer { name } => {
            if client.renomear_server(name).await.is_err() {
                return false;
            }
        }
        Command::SetServerIcon { icon } => {
            if client.definir_icone(icon).await.is_err() {
                return false;
            }
        }
        Command::SetPersonIcon { icon } => {
            if client.definir_minha_imagem(icon).await.is_err() {
                return false;
            }
        }
        Command::SetNickname { name } => {
            if client.definir_meu_apelido(name).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` for these either, and for a
        // sharper version of the reason above: what a moderation verb changes
        // is somebody **else's** session. The only honest source for "they are
        // gone" is the `PersonLeft` the server sends when it is true. Marking it
        // here would draw a roster the person who pressed the button is alone
        // in believing — and draw it identically whether the server did it or
        // refused, which is the exact difference the button exists to expose.
        Command::KickPerson { person } => {
            if client.expulsar(person).await.is_err() {
                return false;
            }
        }
        Command::BanPerson {
            person,
            reason,
            expires_at,
        } => {
            if client.banir(person, reason, expires_at).await.is_err() {
                return false;
            }
        }
        Command::RemoveMessage { message } => {
            if client.remover_mensagem(message).await.is_err() {
                return false;
            }
        }
        Command::MovePerson { person, voice_room } => {
            if client.mover_pessoa(person, voice_room).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` for these either, and here
        // the reason is at its sharpest: a room removed optimistically would
        // vanish off the screen of the person who asked whether or not the
        // server agreed — and the one case where it refuses, the last voice room, is
        // exactly the case they most need to see did not happen.
        Command::DeleteVoiceRoom { voice_room } => {
            if client.apagar_voice_room(voice_room).await.is_err() {
                return false;
            }
        }
        Command::DeleteChannel { channel } => {
            if client.apagar_linha(channel).await.is_err() {
                return false;
            }
        }
        // The question, and where to put the answer. Registered **before** the
        // ask goes out: the server is on the other side of a socket that can be
        // faster than this thread's next channel, and a reply arriving before its
        // slot exists is a reply with nowhere to go.
        Command::WeighChannel { channel, answer } => {
            if let Ok(mut pending) = shared.pending_weights.lock() {
                pending.push((channel, answer));
            }
            if client.pesar_linha(channel).await.is_err() {
                return false;
            }
        }
        // The claim is read out of this client's own history here, before
        // anything is asked of the server, and it never travels through the
        // window: a page that handed it back could hand back a different one,
        // and the whole of ADR 0027's rule is that the claim does not get to
        // choose the decoder.
        Command::PreviewAttachment { attachment, answer } => {
            let claimed = declared_type_of(shared, attachment).unwrap_or_default();
            let Ok(caixa) = client.prever_anexo(attachment).await else {
                return false;
            };
            // In a task of its own, and not awaited here. This queue is what
            // carries every keystroke of this session, and a download parked on
            // it would stop anybody saying anything until the picture came
            // down — the head-of-channel block that ADR 0027 gave each transfer
            // its own stream to avoid.
            tokio::spawn(async move {
                let _ = answer.send(preview_of(attachment.get(), &claimed, caixa.await.ok()));
            });
        }
        Command::ShareScreen { pedido, limites } => {
            if client.compartilhar_tela(*pedido, limites).await.is_err() {
                return false;
            }
        }
        Command::AdjustScreenLimits { limites } => {
            if client.ajustar_limites_da_tela(limites).await.is_err() {
                return false;
            }
        }
        Command::RequestKeyFrame { tela } => {
            // Falhar aqui não derruba a sessão: sem o quadro-chave a tela
            // alheia demora a aparecer, e com a sessão fechada não aparece
            // nunca mais.
            if let Err(erro) = client.pedir_quadro_chave(tela).await {
                tracing::debug!(%erro, "não consegui pedir um quadro-chave");
            }
        }
        Command::StopScreenShare => {
            if client.parar_de_compartilhar().await.is_err() {
                return false;
            }
        }
        Command::Shutdown => return false,
    }
    true
}

/// The type the sender claimed for one attachment, out of the local history.
fn declared_type_of(shared: &Shared, attachment: seele_core::AttachmentId) -> Option<String> {
    let room = shared.room.lock().ok()?;
    room.messages.iter().find_map(|message| {
        message
            .attachment
            .as_ref()
            .filter(|anexo| anexo.id == attachment)
            .map(|anexo| anexo.declared_type.clone())
    })
}

/// Turns fetched bytes into the verdict a window acts on.
///
/// Every branch that produces a picture goes through
/// [`seele_core::preview::judge`], and the media type in the URI comes out of
/// what that function found in the bytes. `claimed` is only ever quoted back
/// into a sentence.
fn preview_of(
    attachment: u64,
    claimed: &str,
    fetched: Option<seele_core::enlace::Previa>,
) -> Preview {
    use seele_core::enlace::Previa;
    use seele_core::preview::{data_uri, judge, Verdict};

    let refused = |refusal: PreviewRefusal, found: Option<String>| Preview {
        attachment,
        image: None,
        claimed: claimed.to_owned(),
        found,
        refusal: Some(refusal),
    };

    match fetched {
        None | Some(Previa::NaoVeio) => refused(PreviewRefusal::DidNotArrive, None),
        Some(Previa::GrandeDemais { .. }) => refused(
            PreviewRefusal::TooBig {
                limit: seele_core::PREVIEW_LIMIT,
            },
            None,
        ),
        Some(Previa::Bytes(bytes)) => match judge(claimed, &bytes) {
            Verdict::Draw(format) => Preview {
                attachment,
                image: Some(data_uri(format, &bytes)),
                claimed: claimed.to_owned(),
                found: Some(format.media_type().to_owned()),
                refusal: None,
            },
            Verdict::Disagrees { found, .. } => refused(
                PreviewRefusal::Disagrees,
                found.map(|format| format.media_type().to_owned()),
            ),
            Verdict::NotAPicture => refused(PreviewRefusal::NotAPicture, None),
        },
    }
}

/// A monotonic identifier for outgoing messages.
///
/// Process-wide rather than per-`Connection`: two handles in one process sending the
/// same number would collide in the server's idempotency check, and the second
/// message would be silently dropped as a resend of the first.
fn next_client_message_id() -> u64 {
    // Drawn at process start, not counted from one — and the difference is a
    // defect, not tidiness.
    //
    // `Messages::append_batch` deduplicates on `(author_id, client_message_id)`.
    // `author_id` comes from the Ed25519 key on disk (ADR 0004) and never
    // changes; this counter used to restart at 1 on every launch. So the first
    // message after reopening the app was read as a retry of the first message
    // of the previous launch and **was never written**, with neither end told
    // (pendency 19). Process-wide was never the scope that mattered: the reuse
    // happened *between* processes.
    //
    // The high half is random and the low half counts, which leaves four
    // billion messages before the two halves could meet.
    static NEXT: std::sync::LazyLock<std::sync::atomic::AtomicU64> =
        std::sync::LazyLock::new(|| {
            std::sync::atomic::AtomicU64::new((u64::from(rand::random::<u32>()) << 32) | 1)
        });
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Relabels a core connection failure for the shell.
///
/// A relabel and nothing more: the core already decided *what* went wrong, and
/// it decided it from typed causes rather than from the text of somebody else's
/// error message.
fn classify_connect_failure(error: &seele_core::ConnectError) -> ConnectionError {
    match error {
        seele_core::ConnectError::LocalEndpoint | seele_core::ConnectError::Unreachable => {
            ConnectionError::Unreachable
        }
        seele_core::ConnectError::TlsRefused | seele_core::ConnectError::ProtocolViolation => {
            ConnectionError::Refused {
                reason: EndReason::Incompatible,
            }
        }
        seele_core::ConnectError::PinChanged { pinned, offered } => ConnectionError::PinChanged {
            pinned: pinned.clone(),
            offered: offered.clone(),
        },
        seele_core::ConnectError::InviteMismatch { expected, offered } => {
            ConnectionError::InviteMismatch {
                expected: expected.clone(),
                offered: offered.clone(),
            }
        }
        seele_core::ConnectError::HandshakeTimeout => ConnectionError::HandshakeTimeout,
        seele_core::ConnectError::Refused { reason } => ConnectionError::Refused {
            reason: (*reason).into(),
        },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_chave_de_mensagem_nao_comeca_num_valor_fixo() {
        // Pendência 19, a metade do app. Aqui o contador é do processo, e era
        // *entre* processos que a repetição acontecia: reabrir o SEELE fazia a
        // primeira mensagem ser lida como reenvio da primeira mensagem da
        // abertura anterior, e ela não era gravada.
        //
        // Só dá para observar o sorteio uma vez por processo, então o que se
        // afirma é a propriedade que sobrevive a isso: o valor não é pequeno, e
        // o contador anda.
        let primeira = super::next_client_message_id();
        let segunda = super::next_client_message_id();

        assert!(
            primeira > u64::from(u32::MAX),
            "a metade alta não foi sorteada: {primeira}"
        );
        assert_eq!(segunda, primeira + 1, "o contador não anda de um em um");
    }

    use super::*;

    #[test]
    fn a_bare_host_gets_the_default_port() {
        let (address, name, _) = resolve("localhost").expect("resolve");
        assert_eq!(address.port(), seele_core::DEFAULT_PORT);
        assert_eq!(name, "localhost");
    }

    #[test]
    fn an_address_is_pinned_under_a_name_a_certificate_can_carry() {
        // ADR 0003 pins per name. An IP is not a name a certificate is issued
        // for, so pinning under one would file the pin somewhere nothing ever
        // looks for it again.
        let (_, name, pin) = resolve("127.0.0.1:8383").expect("resolve");
        assert_eq!(
            name, "localhost",
            "TLS gets the name the certificate carries"
        );
        assert_eq!(pin, "127.0.0.1:8383", "the pin is filed under the address");
    }

    #[test]
    fn every_expected_fingerprint_reaches_the_destino_it_promised() {
        // `drive` builds this immediately before a real QUIC connection, so
        // this is the only way to check the wiring without a live server —
        // and the only thing standing between an invite's `fp=` and the
        // refusal ADR 0006 exists to produce.
        let (address, name, pin) = resolve("localhost:8383").expect("resolve");

        let with_fingerprint = ConnectConfig {
            server: "localhost:8383".into(),
            alternate_servers: Vec::new(),
            nickname: "rafael".into(),
            home: "/tmp/does-not-matter".into(),
            join_secret: None,
            expected_fingerprint: Some("aaaa1111".into()),
            bilhete: None,
            audio: false,
            capture_device: None,
            playback_device: None,
        };
        let destino = build_destino(&with_fingerprint, address, &name, &pin);
        assert_eq!(destino.impressao_esperada, Some("aaaa1111".into()));

        let without = ConnectConfig {
            expected_fingerprint: None,
            bilhete: None,
            ..with_fingerprint
        };
        let destino = build_destino(&without, address, &name, &pin);
        assert_eq!(destino.impressao_esperada, None);
    }

    #[test]
    fn a_host_that_does_not_exist_is_an_enum_and_not_a_panic() {
        assert_eq!(
            resolve("nao-existe.invalid:8383"),
            Err(ConnectionError::UnresolvableHost)
        );
    }

    #[test]
    fn the_pattern_survives_the_round_trip_through_an_atomic() {
        for link_state in [
            LinkTrust::Offline,
            LinkTrust::Unverified,
            LinkTrust::Verified,
        ] {
            assert_eq!(
                link_state_from_byte(link_state_byte(link_state)),
                link_state
            );
        }
    }

    #[test]
    fn an_unknown_pattern_byte_reads_as_offline() {
        // Whatever goes wrong, it must not claim a verified session.
        assert_eq!(link_state_from_byte(200), LinkTrust::Offline);
    }

    #[test]
    fn message_identifiers_never_repeat_within_a_process() {
        // Two handles in one process sending the same number would collide in
        // the server's idempotency check, and the second message would vanish
        // as a resend of the first.
        let ids: std::collections::HashSet<u64> =
            (0..1000).map(|_| next_client_message_id()).collect();
        assert_eq!(ids.len(), 1000);
    }

    #[test]
    fn a_snapshot_of_an_empty_room_is_empty_and_not_a_panic() {
        let room = Room::new();
        assert!(voice_rooms_of(&room).is_empty());
        assert!(lines_of(&room).is_empty());
        assert!(messages_of(&room).is_empty());
    }

    #[test]
    fn the_snapshot_marks_which_person_is_us() {
        // Without this the shell has to compare ids, which means the shell has
        // to know what a `PersonId` is.
        use seele_core::{PersonId, PersonProfile, ServerMessage, SessionId, Ssrc, VoiceRoomInfo};

        let mut room = Room::new();
        room.apply(&ServerMessage::Session {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: vec![VoiceRoomInfo {
                id: VoiceRoomId(1),
                name: "SALA-01".into(),
                limit: 20,
                password_required: false,
                channel: None,
            }],
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: Vec::new(),
        });
        room.enter_voice_room(VoiceRoomId(1));
        room.apply(&ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(1),
            profile: PersonProfile {
                id: PersonId(3),
                nickname: "marcela".into(),
                roles: Vec::new(),
            },
            ssrc: Ssrc(30),
        });

        let voice_rooms = voice_rooms_of(&room);
        let people = &voice_rooms[0].people;
        assert!(people.iter().any(|person| person.is_self));
        assert_eq!(
            people.iter().filter(|person| person.is_self).count(),
            1,
            "more than one person claims to be us"
        );
        assert!(voice_rooms[0].occupied_by_us);
    }

    #[test]
    fn a_sync_band_travels_beside_its_number() {
        // specs/06-clientes-gui.md forbids carrying it by colour alone, and the
        // same applies here: a shell that only got a hue could not print a
        // number, and one that only got a number would have to know the
        // thresholds.
        use seele_core::{
            PersonId, PersonProfile, PersonState, Presence, ServerMessage, Ssrc, VoiceRoomInfo,
        };

        let mut room = Room::new();
        room.voice_rooms = vec![VoiceRoomInfo {
            id: VoiceRoomId(1),
            name: "SALA-01".into(),
            limit: 20,
            password_required: false,
            channel: None,
        }];
        room.apply(&ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(1),
            profile: PersonProfile {
                id: PersonId(3),
                nickname: "marcela".into(),
                roles: Vec::new(),
            },
            ssrc: Ssrc(30),
        });
        room.apply(&ServerMessage::PersonState(PersonState {
            person: PersonId(3),
            muted: false,
            total_isolation: false,
            speaking: false,
            presence: Presence::Available,
            signal: 72,
        }));

        // 72 rather than a critical number on purpose: `SignalBand::Critical` is
        // the `Default`, so a shell that received it could not tell a banded
        // ratio from a field nobody filled in.
        let person = &voice_rooms_of(&room)[0].people[0];
        assert_eq!(person.signal, 72);
        assert_eq!(person.sync_band, types::SignalBand::Degraded);
    }

    #[test]
    fn the_voice_rooms_average_crosses_already_banded() {
        // MÉDIA DO VOICE_ROOM. The comp computes it in the shell; here the shell gets
        // the number, the band and the sample size and has nothing left to
        // decide. A voice room nobody is in carries `None`, not a critical zero.
        use seele_core::{
            PersonId, PersonProfile, PersonState, Presence, ServerMessage, Ssrc, VoiceRoomInfo,
        };

        let mut room = Room::new();
        room.voice_rooms = vec![VoiceRoomInfo {
            id: VoiceRoomId(1),
            name: "SALA-01".into(),
            limit: 20,
            password_required: false,
            channel: None,
        }];
        assert_eq!(voice_rooms_of(&room)[0].sync, None, "an empty voice room");

        for (id, sync) in [(3_u64, 84_u8), (4, 85)] {
            room.apply(&ServerMessage::PersonJoined {
                voice_room: VoiceRoomId(1),
                profile: PersonProfile {
                    id: PersonId(id),
                    nickname: format!("pessoa {id}"),
                    roles: Vec::new(),
                },
                ssrc: Ssrc(u32::try_from(id * 10).expect("ssrc")),
            });
            room.apply(&ServerMessage::PersonState(PersonState {
                person: PersonId(id),
                muted: false,
                total_isolation: false,
                speaking: false,
                presence: Presence::Available,
                signal: sync,
            }));
        }

        let average = voice_rooms_of(&room)[0]
            .sync
            .expect("two people are seated");
        assert_eq!(average.ratio, 85);
        assert_eq!(average.band, types::SignalBand::Nominal);
        assert_eq!(average.people, 2);
    }

    #[test]
    fn a_listener_hears_what_the_room_folded() {
        use seele_core::{AlertReason, AlertSeverity, ServerMessage};

        #[derive(Default)]
        struct Recorder(Mutex<Vec<Event>>);
        impl EventListener for Recorder {
            fn on_event(&self, event: Event) {
                if let Ok(mut seen) = self.0.lock() {
                    seen.push(event);
                }
            }
        }

        let shared = bare_shared();
        let recorder = Arc::new(Recorder::default());
        shared
            .listeners
            .lock()
            .unwrap()
            .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

        fold(
            &shared,
            &ServerMessage::Alert {
                severity: AlertSeverity::Critical,
                reason: AlertReason::PermissionDenied,
                operator_text: None,
            },
        );

        let seen = recorder.0.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let Event::NoticeRaised { notice } = &seen[0] else {
            panic!("expected a notice, got {:?}", seen[0]);
        };
        assert_eq!(notice.severity, Severity::Critical);
        assert_eq!(notice.reason, NoticeReason::PermissionDenied);
    }

    /// A `Shared` with nothing in it, for the folds below.
    fn bare_shared() -> Arc<Shared> {
        Arc::new(Shared {
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            icon_revision: std::sync::atomic::AtomicU64::new(0),
            person_icons_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            media: Mutex::new(None),
            nickname: Mutex::new("marcela".into()),
            link_state: AtomicU8::new(0),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
            caminho: Mutex::new(None),
            signal: AtomicU8::new(0),
            running: AtomicBool::new(true),
            pending_weights: Mutex::new(Vec::new()),
            limites_da_tela: Mutex::new(None),
            fontes_de_tela: Mutex::new(Vec::new()),
        })
    }

    #[test]
    fn o_jitter_da_tela_nao_e_a_profundidade_do_anel() {
        // Duas grandezas com o mesmo nome, e mostrar a errada é pior que não
        // mostrar nada:
        //
        // - `worst_jitter_depth_ms` é **profundidade do anel de reprodução**, e o
        //   ADR 0028 acabou de dar um alvo a ele. Mostrá-lo como "jitter" exibiria a
        //   nossa própria reserva como ruído da rede — e uma reserva saudável
        //   apareceria na tela como problema;
        // - `SourceTelemetry::jitter_ms` é o de **chegada** (RFC 3550), medido aqui,
        //   e é o que a pessoa quer saber.
        //
        // E o que a tela lia antes deste conserto era um terceiro valor: o do
        // relatório do servidor, que é sempre `0.0` porque o servidor não tem como
        // saber — `session.rs` diz isso em comentário desde sempre.
        // Uma reserva de anel saudável (42 ms) ao lado de um jitter de rede baixo
        // (7,5 ms): mostrar a primeira faria uma conexão boa parecer ruim.
        assert!(
            (jitter_para_a_tela(7.5, 42.0) - 7.5).abs() < 0.01,
            "a tela mostra o jitter de chegada, não a profundidade do anel"
        );
        // E nunca o zero que o servidor manda de propósito, que era o que a tela lia.
        assert!(jitter_para_a_tela(7.5, 42.0) > 0.0);
    }

    #[test]
    fn o_jitter_que_o_laco_de_voz_grava_e_o_que_a_casca_le() {
        // A travessia inteira do número, sem o laço de voz: se o campo do
        // `Shared` perdesse a escrita ou a leitura, a tela voltaria a mostrar um
        // zero — que é exatamente o defeito de antes, com outra causa.
        let compartilhado = bare_shared();
        assert_eq!(
            compartilhado.jitter_de_chegada_ms(),
            0.0,
            "sem medida nenhuma ainda não há jitter para mostrar"
        );

        compartilhado.gravar_jitter_de_chegada(jitter_para_a_tela(7.5, 42.0));

        assert!(
            (compartilhado.jitter_de_chegada_ms() - 7.5).abs() < 0.01,
            "a casca leu {}, e o laço de voz gravou 7,5",
            compartilhado.jitter_de_chegada_ms()
        );
    }

    #[test]
    fn a_profundidade_do_anel_vai_para_o_sync_ratio_e_a_chegada_para_a_tela() {
        // A outra metade da fiação. `measure` em si precisa de um `Enlace` vivo
        // e de um dispositivo de áudio aberto, então o que se afirma é a
        // separação que ele faz, apartada nele para isto: qual das duas
        // grandezas sai por qual porta.
        //
        // Os números são escolhidos para não poderem ser confundidos, e são os
        // mesmos do teste da função pura: 42 ms de reserva de anel — saudável,
        // que é o alvo que o ADR 0028 deu a ele — ao lado de 7,5 ms de jitter de
        // rede. Trocar os dois de lugar em `measure` mostraria 42 na tela e uma
        // conexão boa pareceria ruim.
        let telemetria = seele_core::AudioTelemetry {
            local: seele_core::LocalTelemetry::default(),
            sources: vec![seele_core::SourceTelemetry {
                ssrc: 700,
                jitter_depth_ms: 42.0,
                jitter_ms: 7.5,
                ..Default::default()
            }],
        };

        let volta = jitter_da_volta(&telemetria);

        assert!(
            (volta.profundidade_do_anel_ms - 42.0).abs() < 0.01,
            "o Sync Ratio recebeu {}, e a reserva do anel era 42 ms",
            volta.profundidade_do_anel_ms
        );
        let Some(chegada) = volta.chegada_ms else {
            panic!("havia uma fonte sendo recebida e nenhum jitter de chegada saiu");
        };
        assert!(
            (chegada - 7.5).abs() < 0.01,
            "a tela recebeu {chegada}, e o jitter de chegada era 7,5 ms"
        );
    }

    #[test]
    fn sem_fonte_nenhuma_a_tela_nao_recebe_um_zero_inventado() {
        // O pior de uma lista vazia seria zero, e zero é justamente o valor que
        // esta tarefa existe para tirar da tela. Sem ninguém falando não há
        // jitter de chegada nenhum a mostrar, e o último medido continua valendo
        // até haver outro.
        let telemetria = seele_core::AudioTelemetry {
            local: seele_core::LocalTelemetry::default(),
            sources: Vec::new(),
        };

        assert!(
            jitter_da_volta(&telemetria).chegada_ms.is_none(),
            "sem fonte nenhuma saiu um jitter de chegada, e ele só pode ser zero"
        );
    }

    #[test]
    fn gravar_o_mesmo_jitter_duas_vezes_nao_e_uma_mudanca() {
        // O evento de telemetria existe para quem **não** puxa `snapshot` em
        // laço, e ele não falava do jitter: enquanto o número era o `0.0` fixo
        // do relatório do servidor isso não custava nada, porque ele nunca se
        // mexia. Agora ele varia sozinho, e quem depende do evento ficaria com o
        // número velho até outra coisa mudar.
        //
        // O limiar é meio milissegundo porque é a menor diferença que muda o
        // texto: as duas cascas arredondam para milissegundo inteiro. Sem ele o
        // evento sairia quatro vezes por segundo para redesenhar a mesma linha,
        // porque um jitter alisado muda no último bit sempre.
        let compartilhado = bare_shared();

        assert!(
            compartilhado.gravar_jitter_de_chegada(7.5),
            "a primeira medida veio de um zero e não contou como mudança"
        );
        assert!(
            !compartilhado.gravar_jitter_de_chegada(7.5),
            "o mesmo número contou como mudança, e a tela redesenharia por nada"
        );
        assert!(
            !compartilhado.gravar_jitter_de_chegada(7.6),
            "um décimo de milissegundo acordou a tela para escrever `8ms` de novo"
        );
        assert!(
            compartilhado.gravar_jitter_de_chegada(9.0),
            "um milissegundo e meio a mais não contou como mudança, e o rodapé \
             ficaria em `8ms` com a rede piorando"
        );
    }

    /// Uma volta de telemetria com as duas grandezas plantadas para não poderem
    /// ser confundidas: 42 ms de reserva de anel contra `chegada` de jitter de
    /// rede.
    fn volta_com(chegada_ms: f64) -> seele_core::AudioTelemetry {
        seele_core::AudioTelemetry {
            local: seele_core::LocalTelemetry::default(),
            sources: vec![seele_core::SourceTelemetry {
                ssrc: 1,
                jitter_depth_ms: 42.0,
                jitter_ms: chegada_ms,
                ..seele_core::SourceTelemetry::default()
            }],
        }
    }

    #[test]
    fn uma_volta_grava_o_jitter_de_chegada_e_nao_a_reserva_do_anel() {
        // A fiação de `measure`, afirmada onde ela é afirmável. A versão
        // anterior deste teste lia o **texto** de `measure` e passava verde com
        // a propriedade quebrada: um braço que gravasse o jitter e devolvesse
        // `false` satisfazia as duas asserções de texto que ela fazia.
        let compartilhado = bare_shared();
        let mut sync = Signal::new();

        let _ = medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(7.5)), 41_000);

        assert!(
            (compartilhado.jitter_de_chegada_ms() - 7.5).abs() < 0.01,
            "a casca leria {}, e o jitter de chegada era 7,5 ms — 42,0 é a \
             reserva do anel, que é a grandeza errada",
            compartilhado.jitter_de_chegada_ms()
        );
    }

    #[test]
    fn o_evento_de_telemetria_conta_a_variacao_do_jitter() {
        // A pendência do ledger, como comportamento. O evento existe para quem
        // **não** puxa `snapshot` em laço, e ele não falava do jitter: enquanto
        // o número era o `0.0` fixo do relatório do servidor isso não custava nada,
        // porque ele nunca se mexia.
        //
        // A parcela é isolada deixando o Sync Ratio assentar primeiro: ele é
        // uma média móvel, e com entradas constantes ele para de andar. A conta
        // dele não usa o jitter de **chegada** — usa a reserva do anel, o
        // ida-e-volta e a perda —, então depois de assentado o único motivo que
        // resta para responder «mudou» é o jitter da tela.
        let compartilhado = bare_shared();
        let mut sync = Signal::new();

        let mut assentou = false;
        for _ in 0..500 {
            if !medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(7.5)), 41_000) {
                assentou = true;
                break;
            }
        }
        assert!(
            assentou,
            "o Sync Ratio não parou de andar com entradas constantes, então este \
             teste não consegue isolar a parcela do jitter"
        );

        assert!(
            medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(12.0)), 41_000),
            "o jitter de chegada subiu de 7,5 ms para 12 ms e a volta disse que \
             não havia nada a contar à casca: quem depende do evento em vez de \
             puxar `snapshot` em laço desenha o número velho"
        );
        assert!(
            !medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(12.0)), 41_000),
            "o mesmo jitter contou como mudança, e a tela redesenharia por nada"
        );
    }

    #[test]
    fn o_laco_entrega_a_volta_de_audio_que_ele_leu() {
        // Um encosto, e só isso — as propriedades estão nos três testes de
        // comportamento em volta. O que este guarda é o **hop** que sobrou:
        // `measure` lê a telemetria de voz e a passa a `medir_a_volta`, e não há
        // como afirmar essa linha de outro jeito — ela pede um `Enlace` vivo e
        // um dispositivo de áudio aberto, que é justamente o motivo de tudo o
        // mais ter saído dela.
        //
        // O que ele pega é uma mutação real: `medir_a_volta(sync, shared, None,
        // rtt_micros)` tem o tipo certo, compila, e apaga a telemetria de áudio
        // da tela inteira sem que nenhum dos outros testes veja.
        let source = include_str!("lib.rs");
        let Some(corpo) = source
            .split("fn measure(sync: &mut Signal, client: &Enlace, shared: &Arc<Shared>) -> bool {")
            .nth(1)
            .and_then(|resto| resto.split("\n}").next())
        else {
            panic!("`measure` mudou de assinatura, e este encosto tem de mudar com ela");
        };
        let corpo: String = corpo
            .lines()
            .map(|linha| match linha.split_once("//") {
                Some((antes, _)) => antes,
                None => linha,
            })
            .collect::<Vec<&str>>()
            .join("\n");

        assert!(
            corpo.contains("medir_a_volta(sync, shared, telemetria.as_ref(), rtt_micros)"),
            "`measure` deixou de entregar a `medir_a_volta` a telemetria que ele \
             acabou de ler, e o laço de voz para de chegar à tela:\n{corpo}"
        );
        assert!(
            corpo.contains("shared\n        .voice\n        .lock()"),
            "`measure` deixou de ler o laço de voz:\n{corpo}"
        );
    }

    #[test]
    fn sem_volta_de_audio_nenhuma_o_ultimo_jitter_medido_fica() {
        // A contrapartida deliberada de não gravar zero, e o preço dela: numa
        // sessão viva em que a voz cai no meio, o campo congela no último valor
        // em vez de esvaziar. Gravar zero faria a tela afirmar «rede perfeita»
        // sobre uma sessão sem áudio nenhum, que é o defeito que este ciclo
        // tirou dali.
        let compartilhado = bare_shared();
        let mut sync = Signal::new();

        let _ = medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(7.5)), 41_000);
        let _ = medir_a_volta(&mut sync, &compartilhado, None, 41_000);

        assert!(
            (compartilhado.jitter_de_chegada_ms() - 7.5).abs() < 0.01,
            "a voz caiu e a tela passou a mostrar {}, que é o zero que este \
             conserto existe para não mostrar",
            compartilhado.jitter_de_chegada_ms()
        );
    }

    #[test]
    fn o_snapshot_le_o_jitter_que_o_laco_de_voz_gravou_e_nao_o_do_server() {
        // As duas linhas que consertam o defeito são de fiação, e fiação não se
        // afirma testando a função pura dos dois lados dela: `jitter_para_a_tela`
        // podia estar perfeita enquanto o `snapshot` continuava lendo o relatório
        // do servidor, e a suíte continuaria verde. Isto aqui é o teste que fica
        // vermelho quando alguém desfaz o conserto.
        //
        // Os dois números são plantados de propósito para não poderem ser
        // confundidos: 12,75 ms é o de chegada, medido neste receptor, e é o que
        // a tela tem de mostrar; 99,0 ms é o que o relatório do servidor diz, e é o
        // caminho errado. Na vida real esse campo é `0.0` fixo — o servidor não
        // tem como medir jitter, que só existe no receptor —, mas plantar um 99
        // aqui faz o teste distinguir "leu o certo" de "leu um zero que calhou de
        // bater", que um `0.0` de verdade não distinguiria.
        let compartilhado = bare_shared();
        if let Ok(mut room) = compartilhado.room.lock() {
            room.telemetry = Some(seele_core::Telemetry {
                rtt_ms: 20.0,
                jitter_ms: 99.0,
                loss_fraction: 0.0,
                subsystems: Vec::new(),
            });
        }
        compartilhado.gravar_jitter_de_chegada(jitter_para_a_tela(12.75, 42.0));

        let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection {
            commands,
            shared: Arc::clone(&compartilhado),
        };
        let mostrado = connection.snapshot().telemetry.jitter_ms;

        assert!(
            (mostrado - 12.75).abs() < 0.01,
            "a tela mostrou {mostrado}, e o laço de voz mediu 12,75 ms de chegada"
        );
        assert!(
            (mostrado - 99.0).abs() > 0.01,
            "a tela voltou a ler o jitter do relatório do servidor, que é o defeito 3.3"
        );
    }

    #[test]
    fn um_jitter_de_fracao_de_milissegundo_nao_e_arredondado_para_zero() {
        // O campo guarda microssegundos inteiros justamente para isto: num
        // enlace local o jitter fica abaixo de um milissegundo, e guardá-lo em
        // milissegundos inteiros faria a tela dizer zero num caminho excelente —
        // indistinguível do zero que o servidor manda.
        let compartilhado = bare_shared();
        compartilhado.gravar_jitter_de_chegada(0.25);

        assert!(
            (compartilhado.jitter_de_chegada_ms() - 0.25).abs() < 0.001,
            "0,25 ms virou {}",
            compartilhado.jitter_de_chegada_ms()
        );
    }

    #[test]
    fn um_jitter_impossivel_nao_atravessa_a_ponte() {
        // `NaN` e negativo não têm como sair da RFC 3550, mas atravessariam a
        // ponte para uma casca que os desenha — e um `NaN` numa tela vira texto
        // que ninguém sabe ler. Zero é a resposta honesta para "não há medida".
        //
        // Quem garante isto é o `as u64` de `gravar_jitter_de_chegada`, que em
        // Rust satura, e não uma guarda escrita à mão — havia um `max(0.0)` ali
        // e ele foi tirado justamente porque nunca disparava. Este teste então
        // não vigia código nosso: ele **fixa a linguagem**, e reprovaria se
        // alguém trocasse aquele cast por uma conversão que embrulha ou por um
        // `unsafe` de arredondamento sem verificação.
        let compartilhado = bare_shared();

        compartilhado.gravar_jitter_de_chegada(f32::NAN);
        assert_eq!(compartilhado.jitter_de_chegada_ms(), 0.0);

        compartilhado.gravar_jitter_de_chegada(-3.0);
        assert_eq!(compartilhado.jitter_de_chegada_ms(), 0.0);
    }

    #[test]
    fn a_room_made_now_reaches_the_shell_as_a_channel_change() {
        // The bridge invents nothing: the server says a voice room exists, the room
        // folds it in, and the shell is told to redraw the list it already
        // knows how to draw. If this stopped firing, the person who made the
        // room would be looking at the old list with no way to know.
        use seele_core::{ServerMessage, VoiceRoomInfo};

        #[derive(Default)]
        struct Recorder(Mutex<Vec<Event>>);
        impl EventListener for Recorder {
            fn on_event(&self, event: Event) {
                if let Ok(mut seen) = self.0.lock() {
                    seen.push(event);
                }
            }
        }

        let shared = bare_shared();
        let recorder = Arc::new(Recorder::default());
        shared
            .listeners
            .lock()
            .unwrap()
            .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

        fold(
            &shared,
            &ServerMessage::VoiceRoomCreated {
                voice_room: VoiceRoomInfo {
                    id: VoiceRoomId(2),
                    name: "SALA-02 SALA DOS FUNDOS".into(),
                    limit: 8,
                    password_required: false,
                    channel: None,
                },
            },
        );

        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec![Event::ChannelsChanged],
            "the shell was not told the channel list moved"
        );
        let room = shared.room.lock().unwrap();
        assert_eq!(voice_rooms_of(&room).len(), 1);
        assert_eq!(voice_rooms_of(&room)[0].name, "SALA-02 SALA DOS FUNDOS");
    }

    #[test]
    fn the_snapshot_says_whether_this_person_may_make_rooms() {
        // What a screen asks before drawing the control. Convenience, never
        // enforcement — but a shell with no way to ask has only two options,
        // and both are bad: hide the feature from the host, or offer everybody
        // a button that mostly answers "no".
        use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

        let shared = bare_shared();
        fold(
            &shared,
            &ServerMessage::Session {
                id: SessionId(1),
                person: seele_core::PersonId(7),
                ssrc: Ssrc(700),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: vec![Permission::Speak],
            },
        );
        assert!(
            !shared
                .room
                .lock()
                .unwrap()
                .permissions
                .contains(&Permission::ManageVoiceRooms),
            "a person who may only speak was told they may manage voice_rooms"
        );

        fold(
            &shared,
            &ServerMessage::Session {
                id: SessionId(1),
                person: seele_core::PersonId(7),
                ssrc: Ssrc(700),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: vec![Permission::Speak, Permission::ManageVoiceRooms],
            },
        );
        // And the field the screen actually reads, not only the list behind it.
        // Asserting on `room.permissions` alone would pass with
        // `may_manage_voice_rooms` hardcoded either way — measured, and it did.
        let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection {
            commands,
            shared: Arc::clone(&shared),
        };
        assert!(connection.snapshot().may_manage_voice_rooms);

        fold(
            &shared,
            &ServerMessage::Session {
                id: SessionId(1),
                person: seele_core::PersonId(7),
                ssrc: Ssrc(700),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: vec![Permission::Speak],
            },
        );
        assert!(
            !connection.snapshot().may_manage_voice_rooms,
            "the snapshot went on offering the control after the permission went away"
        );
    }

    #[test]
    fn the_snapshot_answers_each_moderation_verb_on_its_own() {
        // Four permissions, four questions, four booleans — and not one
        // `may_moderate`. `specs/04-servidor-seele.md` enumerates them
        // separately and a role may carry any subset, so collapsing them here
        // would draw the ban control for somebody who may only kick, and put a
        // shell in the position of teaching people that half its buttons say no.
        use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

        let shared = bare_shared();
        let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection {
            commands,
            shared: Arc::clone(&shared),
        };

        let sessao = |permissions: Vec<Permission>| ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions,
        };

        fold(&shared, &sessao(vec![Permission::Speak]));
        let nada = connection.snapshot();
        assert!(!nada.may_kick);
        assert!(!nada.may_ban);
        assert!(!nada.may_remove_message);
        assert!(!nada.may_move_person);

        // An Operador holding exactly one of the four. The assertion that
        // matters is the three `false`s beside the one `true`.
        fold(&shared, &sessao(vec![Permission::Speak, Permission::Kick]));
        let so_expulsa = connection.snapshot();
        assert!(so_expulsa.may_kick);
        assert!(
            !so_expulsa.may_ban && !so_expulsa.may_remove_message && !so_expulsa.may_move_person,
            "one moderation permission lit up the other three"
        );

        fold(
            &shared,
            &sessao(vec![
                Permission::Kick,
                Permission::Ban,
                Permission::RemoveMessage,
                Permission::MovePerson,
            ]),
        );
        let tudo = connection.snapshot();
        assert!(tudo.may_kick && tudo.may_ban && tudo.may_remove_message && tudo.may_move_person);

        // And they go away again. A snapshot that latched would go on offering
        // a control after a Comandante revoked it.
        fold(&shared, &sessao(Vec::new()));
        let depois = connection.snapshot();
        assert!(
            !depois.may_kick && !depois.may_ban && !depois.may_remove_message,
            "the snapshot went on offering the controls after the permissions went away"
        );
    }

    #[test]
    fn destroying_a_room_is_a_permission_of_its_own_and_not_the_one_that_makes_them() {
        // The decision this whole path turns on, asserted where a shell reads
        // it. Making and renaming a room are mistakes a server survives;
        // destroying one ends what other people wrote. A role that may build
        // rooms without being able to unmake them is a role somebody can
        // actually write — `specs/04-servidor-seele.md` enumerates
        // `gerenciar_voice_rooms` and `administrar_server` separately — and a single
        // boolean for both would make it impossible to offer correctly.
        use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

        let shared = bare_shared();
        let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection {
            commands,
            shared: Arc::clone(&shared),
        };

        let sessao = |permissions: Vec<Permission>| ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions,
        };

        // The role that builds and does not destroy. This is the pair the
        // separation exists for, and the one a single boolean would get wrong.
        fold(&shared, &sessao(vec![Permission::ManageVoiceRooms]));
        let constroi = connection.snapshot();
        assert!(constroi.may_manage_voice_rooms);
        assert!(
            !constroi.may_delete_rooms,
            "the permission to make a room was read as the permission to destroy one"
        );

        // And the reverse, so the two are not simply the same field read twice.
        fold(&shared, &sessao(vec![Permission::AdministerServer]));
        let administra = connection.snapshot();
        assert!(administra.may_delete_rooms);
        assert!(!administra.may_manage_voice_rooms);

        // Never the moderation permissions either: somebody trusted to remove a
        // person for the evening is not thereby trusted with the server's past.
        fold(
            &shared,
            &sessao(vec![
                Permission::Kick,
                Permission::Ban,
                Permission::RemoveMessage,
                Permission::MovePerson,
            ]),
        );
        assert!(
            !connection.snapshot().may_delete_rooms,
            "a moderation permission lit up the one that destroys rooms"
        );

        // And it goes away again, like the five beside it.
        fold(&shared, &sessao(Vec::new()));
        assert!(!connection.snapshot().may_delete_rooms);
    }

    /// A shell watching a server dress itself, from the outside.
    ///
    /// The three properties a screen depends on and no type can state: the name
    /// arrives whole, the picture does not, and the two do not move each
    /// other\u{2019}s counter.
    #[test]
    fn the_server_name_rides_the_snapshot_and_the_picture_rides_a_revision() {
        use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

        let shared = bare_shared();
        let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection {
            commands,
            shared: Arc::clone(&shared),
        };

        fold(
            &shared,
            &ServerMessage::Session {
                id: SessionId(1),
                person: seele_core::PersonId(7),
                ssrc: Ssrc(700),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: vec![Permission::AdministerServer],
            },
        );
        let inicio = connection.snapshot();
        assert_eq!(inicio.server, "Casa");
        assert_eq!(inicio.icon_revision, 0);
        assert_eq!(connection.server_icon(), None);
        assert!(
            inicio.may_customise_server,
            "whoever administers the server was not offered the control"
        );

        // The name travels whole, because it is a string and it is cheap.
        fold(
            &shared,
            &ServerMessage::ServerRenamed {
                name: "Casa".into(),
            },
        );
        let renomeado = connection.snapshot();
        assert_eq!(renomeado.server, "Casa");
        assert_eq!(
            renomeado.icon_revision, 0,
            "a rename made every shell refetch a picture that did not change"
        );

        // The picture does not: the snapshot only says that it moved.
        let bytes = vec![0x89, b'P', b'N', b'G', 4, 5, 6];
        fold(
            &shared,
            &ServerMessage::ServerIconChanged {
                icon: Some(bytes.clone()),
            },
        );
        assert_eq!(connection.snapshot().icon_revision, 1);
        assert_eq!(connection.server_icon(), Some(bytes));

        // Including when it moves to nothing. A revision that only counted
        // arrivals would leave the old picture on screen after it was taken
        // down, because the shell would never be told to look again.
        fold(&shared, &ServerMessage::ServerIconChanged { icon: None });
        assert_eq!(connection.snapshot().icon_revision, 2);
        assert_eq!(connection.server_icon(), None);
    }

    #[test]
    fn a_picture_that_will_not_do_never_leaves_this_crate() {
        // The failure this prevents is not a wrong error message: it is the app
        // starting its five-minute internal battery because somebody picked a
        // PDF. A frame the protocol will not build is a send that fails, and a
        // send that fails is how a dropped link looks from `enlace`.
        let shared = bare_shared();
        let (commands, mut fila) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection { commands, shared };

        assert_eq!(
            connection.set_server_icon(Some(b"%PDF-1.7".to_vec())),
            Err(ConnectionError::IconNotAPicture)
        );
        assert!(
            fila.try_recv().is_err(),
            "a picture the server would refuse was queued to be sent anyway"
        );

        // And the heavy one answers with the ceiling, because the sentence
        // about it needs the number.
        let mut gorda = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        gorda.extend_from_slice(&13_u32.to_be_bytes());
        gorda.extend_from_slice(b"IHDR");
        gorda.extend_from_slice(&128_u32.to_be_bytes());
        gorda.extend_from_slice(&128_u32.to_be_bytes());
        gorda.resize(64 * 1024, 0);
        let Err(ConnectionError::IconTooBig { limit_bytes }) =
            connection.set_server_icon(Some(gorda))
        else {
            panic!("a picture over the ceiling was queued");
        };
        assert!(limit_bytes > 0);
        assert!(fila.try_recv().is_err());

        // Taking the picture down is never refused: it carries no bytes to
        // refuse, and whoever put one up has to be able to take it away.
        assert_eq!(connection.set_server_icon(None), Ok(()));
        assert!(
            matches!(fila.try_recv(), Ok(Command::SetServerIcon { icon: None })),
            "taking the picture down was swallowed instead of sent"
        );
    }

    #[test]
    fn dressing_the_server_is_a_permission_of_its_own_field() {
        // Same permission as `may_delete_rooms` today, separate question. A
        // shell reading the destroy flag to decide whether to draw a rename box
        // would be leaning on a coincidence between two verbs.
        use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

        let shared = bare_shared();
        let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection {
            commands,
            shared: Arc::clone(&shared),
        };

        let sessao = |permissions: Vec<Permission>| ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions,
        };

        fold(&shared, &sessao(vec![Permission::ManageVoiceRooms]));
        assert!(
            !connection.snapshot().may_customise_server,
            "the permission to make rooms was read as the permission to name the server"
        );

        fold(&shared, &sessao(vec![Permission::AdministerServer]));
        assert!(connection.snapshot().may_customise_server);

        fold(&shared, &sessao(Vec::new()));
        assert!(
            !connection.snapshot().may_customise_server,
            "the snapshot went on offering the control after the permission went away"
        );
    }

    #[test]
    fn the_weight_of_a_line_reaches_the_caller_who_asked_for_it() {
        // The number in the confirmation, and the one call on this bridge that
        // waits for an answer. A shell that cannot get these three numbers must
        // not open the box at all, so the wiring that carries them is worth
        // pinning: the question registers a slot, the server's reply fills it,
        // and the caller wakes with the counts unrounded.
        let shared = bare_shared();
        let (commands, mut queue) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection {
            commands,
            shared: Arc::clone(&shared),
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let pergunta = tokio::spawn({
                let shared = Arc::clone(&shared);
                async move {
                    // The driver's half, by hand: take the command off the
                    // queue, park the sender, then let the answer arrive.
                    let Some(Command::WeighChannel { channel, answer }) = queue.recv().await else {
                        panic!("nothing asked the server to weigh anything");
                    };
                    shared
                        .pending_weights
                        .lock()
                        .expect("pending")
                        .push((channel, answer));
                    shared.answer_weight(ChannelWeight {
                        channel: 7,
                        messages: 1_847,
                        authors: 6,
                        oldest_at_seconds: Some(1_678_600_000),
                    });
                }
            });

            let peso = connection.weigh_channel(7).await.expect("weight");
            pergunta.await.expect("driver");

            assert_eq!(peso.messages, 1_847);
            assert_eq!(peso.authors, 6);
            assert_eq!(peso.oldest_at_seconds, Some(1_678_600_000));
        });

        // And nothing was kept: the weight's whole value is being fresh, so
        // asking for it left the room exactly as it was.
        assert!(shared.room.lock().expect("room").channels.is_empty());
        assert!(shared.pending_weights.lock().expect("pending").is_empty());
    }

    #[test]
    fn a_weight_for_another_line_leaves_this_caller_waiting() {
        // Two boxes cannot be open at once in one window, but a reply landing
        // against the wrong slot would be the kind of mistake that fills a
        // confirmation with somebody else's numbers — which is the exact
        // failure the counted number exists to rule out.
        let shared = bare_shared();
        let (answer, mut resposta) = tokio::sync::oneshot::channel();
        shared
            .pending_weights
            .lock()
            .expect("pending")
            .push((seele_core::ChannelId(7), answer));

        shared.answer_weight(ChannelWeight {
            channel: 9,
            messages: 3,
            authors: 1,
            oldest_at_seconds: None,
        });

        assert!(
            resposta.try_recv().is_err(),
            "a count for another Channel was handed to the box asking about this one"
        );
        assert_eq!(
            shared.pending_weights.lock().expect("pending").len(),
            1,
            "the question was dropped without being answered"
        );
    }

    #[test]
    fn asking_for_a_room_with_no_name_asks_for_nothing() {
        // The same swallow `send_message` does for an empty body. Somebody who
        // pressed the button with nothing typed has not asked for anything, and
        // answering that with a red message puts an error on a screen where
        // nothing went wrong. The check is here as well as in `seele-proto`
        // because the proto one would arrive as a dropped connection rather
        // than as nothing happening.
        let (commands, mut queue) = tokio::sync::mpsc::unbounded_channel();
        let connection = Connection {
            commands,
            shared: bare_shared(),
        };

        for blank in ["", "   ", "\t\n"] {
            connection.create_voice_room(blank.into(), 8, None).unwrap();
            connection.create_channel(blank.into()).unwrap();
            connection.rename_voice_room(1, blank.into()).unwrap();
            connection.rename_channel(1, blank.into()).unwrap();
        }
        assert!(
            queue.try_recv().is_err(),
            "a blank name was sent to the server"
        );

        connection
            .create_voice_room("SALA-02".into(), 8, None)
            .unwrap();
        assert!(
            matches!(queue.try_recv(), Ok(Command::CreateVoiceRoom { .. })),
            "a real name was swallowed too"
        );
    }

    #[test]
    fn a_capture_device_crosses_with_both_halves_of_its_identity() {
        // A shell shows the name and sends back the id, and the two are not
        // interchangeable: two microphones of the same model report the same
        // name, so a shell that stored the name would leave the second one
        // unpickable — and one that *showed* the id would put a backend string
        // on screen where a person expects "Scarlett Solo".
        //
        // Skips out loud rather than passing on a machine with no microphone. A
        // test that succeeds whether or not the feature works is not a test, and
        // CI has no sound card.
        let found = capture_devices();
        if found.is_empty() {
            eprintln!("skipped: this machine lists no capture device");
            return;
        }

        for device in &found {
            assert!(
                !device.id.is_empty(),
                "a device with no id cannot be chosen"
            );
            assert!(
                !device.name.is_empty(),
                "a device with no name cannot be labelled"
            );
        }
        assert!(
            found.iter().filter(|device| device.default).count() <= 1,
            "two devices both claim to be the machine's default"
        );
    }

    #[test]
    fn a_playback_device_crosses_with_both_halves_of_its_identity() {
        // The twin of the check above, for the same reason and with the same
        // skip: CI has no sound card, and a test that passes on a machine with
        // no speakers passes on every machine.
        let found = playback_devices();
        if found.is_empty() {
            eprintln!("skipped: this machine lists no playback device");
            return;
        }

        for device in &found {
            assert!(
                !device.id.is_empty(),
                "a device with no id cannot be chosen"
            );
            assert!(
                !device.name.is_empty(),
                "a device with no name cannot be labelled"
            );
        }
        assert!(
            found.iter().filter(|device| device.default).count() <= 1,
            "two outputs both claim to be the machine's default"
        );
    }

    #[test]
    fn the_two_device_lists_are_not_the_same_list() {
        // Both are `Vec<_>` of three strings, and the two functions differ by one
        // word. A `playback_devices` that forwarded to the capture side would
        // compile, serialise, and draw a list of microphones under SAÍDA DE SOM
        // — and every pick from it would be refused by a machine that has no
        // such output.
        let (microphones, speakers) = (capture_devices(), playback_devices());
        if microphones.is_empty() || speakers.is_empty() {
            eprintln!(
                "skipped: this machine lists {} microphones and {} outputs, and telling the \
                 two lists apart needs one of each",
                microphones.len(),
                speakers.len()
            );
            return;
        }

        let ids: Vec<&str> = microphones
            .iter()
            .map(|device| device.id.as_str())
            .collect();
        assert!(
            speakers
                .iter()
                .any(|device| !ids.contains(&device.id.as_str())),
            "every listed output is also a listed microphone, so one list is the other: \
             {ids:?}"
        );
    }

    #[test]
    fn a_capture_device_serialises_the_field_names_a_shell_reads() {
        // The desktop shell is untyped JavaScript reading these three names off
        // the wire. A `serde(rename)` added here would leave every row drawn
        // `undefined`, with nothing failing anywhere — the same defect class
        // `apps/seele-app/tests/frontend.rs` guards for `Match`.
        let device = CaptureDevice {
            id: "coreaudio:alguma-coisa".into(),
            name: "Scarlett Solo".into(),
            default: true,
        };
        let Ok(json) = serde_json::to_string(&device) else {
            panic!("CaptureDevice does not serialise, so no shell can read it at all");
        };
        for field in ["\"id\"", "\"name\"", "\"default\""] {
            assert!(
                json.contains(field),
                "a capture device no longer carries {field}: {json}"
            );
        }
    }

    #[test]
    fn a_playback_device_serialises_the_field_names_a_shell_reads() {
        // Its own test and not an addition to the one above, because the two
        // types are separate declarations: a `serde(rename)` on this one would
        // leave every row under SAÍDA DE SOM drawn `undefined` while the
        // microphone list next to it kept working, which is the version of this
        // failure that takes longest to believe.
        let device = PlaybackDevice {
            id: "coreaudio:alguma-saida".into(),
            name: "Fones de ouvido".into(),
            default: true,
        };
        let Ok(json) = serde_json::to_string(&device) else {
            panic!("PlaybackDevice does not serialise, so no shell can read it at all");
        };
        for field in ["\"id\"", "\"name\"", "\"default\""] {
            assert!(
                json.contains(field),
                "a playback device no longer carries {field}: {json}"
            );
        }
    }

    #[test]
    fn a_snapshot_with_no_audio_names_no_microphone() {
        // The two have to agree, always: a screen that read a device off a
        // session with no voice path would draw a microphone that is not open.
        // `AudioState::silent` is the one place that pairing is decided.
        let quiet = AudioState::silent();
        assert!(!quiet.available);
        assert_eq!(quiet.capture, None);
        assert_eq!(
            quiet.playback, None,
            "a session with no audio must not name an output it is not playing through"
        );
        assert_eq!(
            quiet.mode,
            VoiceMode::PushToTalk,
            "a session with no audio must not read as an open microphone"
        );
    }

    #[test]
    fn a_pong_wakes_nobody() {
        // Every event delivered is a redraw somewhere. The round-trip
        // measurement is consumed by the core and is not news.
        use seele_core::ServerMessage;

        struct Counter(std::sync::atomic::AtomicUsize);
        impl EventListener for Counter {
            fn on_event(&self, _: Event) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let shared = Arc::new(Shared {
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            icon_revision: std::sync::atomic::AtomicU64::new(0),
            person_icons_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: Mutex::new(None),
            media: Mutex::new(None),
            nickname: Mutex::new("marcela".into()),
            link_state: AtomicU8::new(0),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
            caminho: Mutex::new(None),
            signal: AtomicU8::new(0),
            running: AtomicBool::new(true),
            pending_weights: Mutex::new(Vec::new()),
            limites_da_tela: Mutex::new(None),
            fontes_de_tela: Mutex::new(Vec::new()),
        });
        let counter = Arc::new(Counter(std::sync::atomic::AtomicUsize::new(0)));
        shared
            .listeners
            .lock()
            .unwrap()
            .push(Arc::clone(&counter) as Arc<dyn EventListener>);

        fold(&shared, &ServerMessage::Pong { timestamp: 1 });

        assert_eq!(counter.0.load(Ordering::Relaxed), 0);
    }

    /// Uma sala com uma transmissão de tela em curso, e mais ninguém dentro.
    ///
    /// Devolve o `Room` para o teste acrescentar gente: é a contagem de
    /// espectadores que os testes abaixo mexem, e ela é o N do §5.1.
    fn sala_com_tela(quem_compartilha: seele_core::PersonId) -> Room {
        use seele_core::{PersonId, ScreenId, ServerMessage, SessionId, Ssrc, VoiceRoomInfo};

        let mut room = Room::new();
        room.apply(&ServerMessage::Session {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: vec![VoiceRoomInfo {
                id: VoiceRoomId(1),
                name: "SALA-01".into(),
                limit: 20,
                password_required: false,
                channel: None,
            }],
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: Vec::new(),
        });
        room.enter_voice_room(VoiceRoomId(1));
        room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VoiceRoomId(1),
            person: quem_compartilha,
            screen: ScreenId(9),
        });
        room
    }

    /// Senta mais uma pessoa na sala.
    fn sentar(room: &mut Room, pessoa: u64) {
        use seele_core::{PersonId, PersonProfile, ServerMessage, Ssrc};

        room.apply(&ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(1),
            profile: PersonProfile {
                id: PersonId(pessoa),
                nickname: format!("pessoa-{pessoa}"),
                roles: Vec::new(),
            },
            ssrc: Ssrc(pessoa.wrapping_mul(10).try_into().unwrap_or(1)),
        });
    }

    #[test]
    fn a_tela_conta_quem_compartilha_e_quem_assiste() {
        use seele_core::PersonId;

        let mut room = sala_com_tela(PersonId(3));
        sentar(&mut room, 3);
        sentar(&mut room, 4);

        let tela = tela_de(&room, None).expect("a transmissão foi anunciada nesta sala");
        assert_eq!(tela.de, 3);
        assert!(
            !tela.e_minha,
            "quem compartilha é o pessoa 3, e nós somos o 7"
        );
        // Três na sala — nós, o 3 e o 4 —, e quem compartilha não assiste a si
        // mesmo. Quem assiste são dois, e é esse N que divide o teto (§5.1).
        assert_eq!(
            tela.espectadores, 2,
            "quem compartilha entrou na própria contagem, e o teto seria dividido por gente demais"
        );

        sentar(&mut room, 5);
        assert_eq!(
            tela_de(&room, None)
                .expect("a transmissão continua")
                .espectadores,
            3,
            "entrou mais uma pessoa e a contagem não andou"
        );
    }

    #[test]
    fn a_tela_de_quem_compartilha_se_reconhece() {
        use seele_core::PersonId;

        // O pessoa 7 é quem esta sessão é — `sala_com_tela` o diz na `Session`.
        let room = sala_com_tela(PersonId(7));
        let tela = tela_de(&room, None).expect("a transmissão foi anunciada");
        assert!(
            tela.e_minha,
            "sem isto a casca desenha o painel de quem assiste para quem compartilha"
        );
    }

    #[test]
    fn nao_ha_tela_quando_ninguem_compartilha() {
        use seele_core::{PersonId, ServerMessage, SessionId, Ssrc, VoiceRoomInfo};

        let mut room = Room::new();
        room.apply(&ServerMessage::Session {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: vec![VoiceRoomInfo {
                id: VoiceRoomId(1),
                name: "SALA-01".into(),
                limit: 20,
                password_required: false,
                channel: None,
            }],
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: Vec::new(),
        });
        room.enter_voice_room(VoiceRoomId(1));
        assert!(tela_de(&room, None).is_none());
    }

    #[test]
    fn a_tela_nao_finge_saber_o_que_esta_saindo() {
        // O §5 obriga a interface a mostrar o que está saindo **agora** ao lado
        // do que foi pedido. Nada nesta ponte mede o que está saindo: quem
        // compartilha não tem codificador daqui e quem assiste não tem recepção
        // aberta. Então os três números saem zerados e `medida` diz que são
        // zero por ignorância, e não por medida.
        //
        // Este teste existe para que preenchê-los com o que foi pedido — ou com
        // o degrau que o teto compraria — custe uma linha vermelha. Era o defeito
        // exato do jitter, que a tela lia do relatório do servidor como `0.0` porque
        // o servidor não tem como medir uma grandeza do receptor.
        use seele_core::PersonId;

        let tela = tela_de(&sala_com_tela(PersonId(3)), None).expect("a transmissão foi anunciada");
        assert!(
            !tela.medida,
            "alguém passou a medir o que sai: então preencha os três números e mude este teste"
        );
        assert_eq!((tela.altura, tela.quadros, tela.kbps), (0, 0, 0));
        assert_eq!(
            tela.parada, None,
            "parar com motivo é decisão do teto, e nenhum teto roda deste lado"
        );
    }

    /// Um pedido qualquer, com os três controles do §5 preenchidos.
    fn limites() -> LimitesDeTela {
        LimitesDeTela {
            banda_bps: Some(1_200_000),
            altura_maxima: 1080,
            quadros_maximos: 30,
            prioridade: crate::types::Prioridade::Nitidez,
        }
    }

    #[test]
    fn o_pedido_so_aparece_ao_lado_da_propria_tela() {
        use seele_core::PersonId;

        // O §5 manda pôr o que está saindo ao lado do que foi pedido. O que foi
        // pedido é escolha de quem transmite e **não viaja**: o `ScreenHeader`
        // carrega resolução e codec, nunca o teto. Então esta ponte só tem como
        // preencher a coluna da própria transmissão — e preenchê-la com a
        // escolha desta máquina ao lado da tela de outra pessoa seria mostrar o
        // teto de uma transmissão como se fosse o de outra.
        let minha = tela_de(&sala_com_tela(PersonId(7)), Some(limites()))
            .expect("a transmissão foi anunciada");
        assert!(minha.e_minha);
        assert_eq!(
            minha.pedido,
            Some(limites()),
            "quem compartilha perdeu a metade da comparação que o §5 obriga"
        );

        let alheia = tela_de(&sala_com_tela(PersonId(3)), Some(limites()))
            .expect("a transmissão foi anunciada");
        assert!(!alheia.e_minha);
        assert_eq!(
            alheia.pedido, None,
            "o teto escolhido nesta máquina foi mostrado como se fosse o de quem compartilha"
        );
    }

    #[test]
    fn o_pedido_morre_com_a_transmissao_e_nao_com_o_roster() {
        use seele_core::{PersonId, ScreenId, ServerMessage};

        // A memória do que foi pedido saiu do JavaScript da casca — onde ela
        // morria com a janela — e passou a morar aqui. Isso troca um defeito por
        // outro se ela sobreviver à transmissão: a próxima vez que esta pessoa
        // compartilhasse mostraria o que está saindo agora ao lado de um teto de
        // outra vez, e nada na tela diria qual dos dois números vale.
        let shared = bare_shared();
        if let Ok(mut room) = shared.room.lock() {
            *room = sala_com_tela(PersonId(7));
        }
        shared.gravar_pedido_da_tela(Some(limites()));

        // Alguém entra na sala: o N do §5.1 anda, a transmissão continua, e o
        // que foi pedido continua valendo.
        fold(
            &shared,
            &ServerMessage::PersonJoined {
                voice_room: VoiceRoomId(1),
                profile: seele_core::PersonProfile {
                    id: PersonId(4),
                    nickname: "pessoa-4".into(),
                    roles: Vec::new(),
                },
                ssrc: Ssrc(40),
            },
        );
        assert_eq!(
            shared.pedido_da_tela(),
            Some(limites()),
            "uma pessoa entrando na sala apagou o teto de quem estava compartilhando"
        );

        // A transmissão acaba: o pedido acaba com ela.
        fold(
            &shared,
            &ServerMessage::ScreenShareStopped {
                voice_room: VoiceRoomId(1),
                screen: ScreenId(9),
            },
        );
        assert_eq!(
            shared.pedido_da_tela(),
            None,
            "o teto sobreviveu à transmissão a que ele pertencia"
        );
    }

    #[test]
    fn um_pedido_recusado_nao_vira_memoria() {
        // `compartilhar_tela` guarda o que foi pedido **quando a transmissão
        // começa**. Enquanto ela não começa, guardar seria pôr na coluna do
        // pedido o teto de um botão que devolveu erro — e a coluna existe para
        // explicar a diferença entre o que se pediu e o que está saindo, não
        // para registrar tentativas.
        let fonte = include_str!("lib.rs");
        let Some(corpo) = fonte
            .split("pub fn compartilhar_tela(")
            .nth(1)
            .and_then(|resto| resto.split("\n    }").next())
        else {
            panic!("`compartilhar_tela` mudou de forma; este guarda tem de mudar com ele");
        };
        assert!(
            corpo.contains("comecou.is_ok().then_some(limites)"),
            "`compartilhar_tela` guarda o pedido sem conferir se a transmissão começou:\n{corpo}"
        );
    }

    #[test]
    fn o_roster_redesenha_a_tela_so_quando_ha_tela() {
        // §5.1: a contagem de espectadores é o N que divide o teto, e ela anda
        // quando alguém entra ou sai — sem nenhuma mensagem de tela chegar. Uma
        // casca que só redesenhasse em `ScreenShareStarted` escreveria
        // «4 pessoas assistindo» numa sala de seis.
        let so_roster = seele_core::Changed {
            roster: true,
            ..seele_core::Changed::default()
        };
        let so_tela = seele_core::Changed {
            telas: true,
            ..seele_core::Changed::default()
        };

        assert!(a_tela_mudou(so_roster, true));
        assert!(
            !a_tela_mudou(so_roster, false),
            "sem transmissão em curso, todo mundo que entra numa sala acenderia o painel de tela"
        );
        assert!(
            a_tela_mudou(so_tela, false),
            "uma transmissão que acabou de acabar tem de apagar o painel"
        );
        assert!(a_tela_mudou(so_tela, true));
        assert!(!a_tela_mudou(seele_core::Changed::default(), true));
    }

    #[test]
    fn um_ouvinte_sabe_que_a_tela_comecou_e_que_a_sala_cresceu() {
        use seele_core::{
            PersonId, PersonProfile, ScreenId, ServerMessage, SessionId, Ssrc, VoiceRoomInfo,
        };

        #[derive(Default)]
        struct Recorder(Mutex<Vec<Event>>);
        impl EventListener for Recorder {
            fn on_event(&self, event: Event) {
                if let Ok(mut seen) = self.0.lock() {
                    seen.push(event);
                }
            }
        }

        let shared = bare_shared();
        let recorder = Arc::new(Recorder::default());
        shared
            .listeners
            .lock()
            .unwrap()
            .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

        fold(
            &shared,
            &ServerMessage::Session {
                id: SessionId(1),
                person: PersonId(7),
                ssrc: Ssrc(700),
                server: "Casa".into(),
                voice_rooms: vec![VoiceRoomInfo {
                    id: VoiceRoomId(1),
                    name: "SALA-01".into(),
                    limit: 20,
                    password_required: false,
                    channel: None,
                }],
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: Vec::new(),
            },
        );
        shared.room.lock().unwrap().enter_voice_room(VoiceRoomId(1));

        // Alguém entra antes de haver tela: nada de `ScreenChanged`.
        fold(
            &shared,
            &ServerMessage::PersonJoined {
                voice_room: VoiceRoomId(1),
                profile: PersonProfile {
                    id: PersonId(3),
                    nickname: "marcela".into(),
                    roles: Vec::new(),
                },
                ssrc: Ssrc(30),
            },
        );
        assert!(
            !recorder
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, Event::ScreenChanged)),
            "o painel de tela acordou numa sala sem transmissão nenhuma"
        );

        fold(
            &shared,
            &ServerMessage::ScreenShareStarted {
                voice_room: VoiceRoomId(1),
                person: PersonId(3),
                screen: ScreenId(9),
            },
        );
        let comecou = recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, Event::ScreenChanged))
            .count();
        assert_eq!(
            comecou, 1,
            "a transmissão começou e a casca não foi avisada"
        );

        // E agora o roster sozinho, com a tela em curso: o N mudou.
        fold(
            &shared,
            &ServerMessage::PersonJoined {
                voice_room: VoiceRoomId(1),
                profile: PersonProfile {
                    id: PersonId(4),
                    nickname: "pires".into(),
                    roles: Vec::new(),
                },
                ssrc: Ssrc(40),
            },
        );
        let depois = recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, Event::ScreenChanged))
            .count();
        assert_eq!(
            depois, 2,
            "entrou a quinta pessoa, o teto foi dividido por mais um, e a tela não soube"
        );
    }

    #[test]
    fn todo_motivo_de_parada_tem_um_nome_na_lista() {
        // A lista é escrita à mão porque `seele_core::MotivoDeParada` não
        // publica um `TODOS` — ao contrário de `chegada::Caminho`, que
        // `caminhos()` deriva. Este teste é o que prende as duas: um motivo novo
        // reprova no `match` de `nome_da_parada`, e um motivo esquecido na lista
        // reprova aqui.
        let nomes = motivos_de_parada_da_tela();
        assert_eq!(
            nomes.len(),
            2,
            "a lista de motivos andou e `nome_da_parada` não, ou o contrário"
        );
        assert!(nomes.contains(&nome_da_parada(seele_core::MotivoDeParada::SinalCritico)));
        assert!(nomes.contains(&nome_da_parada(seele_core::MotivoDeParada::AbaixoDoPiso)));
        let distintos: std::collections::BTreeSet<&str> = nomes.iter().copied().collect();
        assert_eq!(
            distintos.len(),
            nomes.len(),
            "dois motivos com o mesmo nome: a casca escreveria a mesma frase para os dois"
        );
    }

    #[test]
    fn esta_ponte_nao_decide_quem_perdeu_a_corrida_da_tela() {
        // `specs/08-seguranca.md`: a interface esconder é conveniência; o
        // servidor negar é a segurança. Uma transmissão por sala (§6.3) é regra
        // do servidor, e quem perde a corrida é avisado por ele —
        // `AlertReason::ScreenShareTaken`, que já atravessa como
        // `NoticeReason::ScreenShareTaken`.
        //
        // O guarda lê a fonte porque a propriedade é sobre o que o código **não**
        // faz, e nenhum valor a exibe: um `compartilhar_tela` que conferisse
        // `room.telas` por conta própria compila, passa em tudo o mais, e recusa
        // localmente o que o servidor teria aceitado.
        let fonte = include_str!("lib.rs");
        let Some(corpo) = fonte
            .split("pub fn compartilhar_tela(")
            .nth(1)
            .and_then(|depois| depois.split("\n    }").next())
        else {
            panic!("`compartilhar_tela` mudou de forma; este guarda tem de mudar com ele");
        };
        assert!(
            !corpo.contains("ScreenShareTaken"),
            "`compartilhar_tela` julga a corrida por conta própria:\n{corpo}"
        );
        assert!(
            !corpo.contains("telas"),
            "`compartilhar_tela` lê o mapa de transmissões para decidir; quem decide é o servidor:\n{corpo}"
        );
    }
}

#[cfg(test)]
mod aviso_de_mensagens {
    /// `Event::MessagesChanged` may only be raised by `Shared::messages_changed`.
    ///
    /// The two halves — bump the revision, then tell the shell — have to move
    /// together, and they did not: `Command::OpenChannel` clears the room's
    /// messages and raised the event **without** the bump. A shell that
    /// refetches only when the number moves, which is the entire point of the
    /// number, swallowed it, and switching Channel left the previous Channel's
    /// conversation on screen under the new heading.
    ///
    /// Nothing about that failed. The event fired, the listener ran, the guard
    /// compared two equal numbers and correctly concluded there was nothing to
    /// do. Every part behaved; the pair was wrong.
    ///
    /// Reading the source is the only way to state "one door and no other" —
    /// a behavioural test would have to know every future caller in advance.
    #[test]
    fn nobody_raises_the_event_without_moving_the_revision() {
        let fonte = include_str!("lib.rs");
        let Some(depois) = fonte.split("fn messages_changed(&self)").nth(1) else {
            panic!(
                "`messages_changed` is gone, and with it the only place the two halves are tied"
            );
        };
        let Some(corpo) = depois.split("\n    }").next() else {
            panic!("`messages_changed` is never closed");
        };
        assert!(
            corpo.contains("fetch_add") && corpo.contains("Event::MessagesChanged"),
            "the one function that ties the revision to the notice no longer does \
             both:\n{corpo}"
        );

        // Everything else may only reach it through that function.
        //
        // Scanned up to the first `#[cfg(test)]` and no further, and comments
        // dropped. Both exclusions are load-bearing rather than tidy: this very
        // test names the event three times in its own assertions, and the
        // paragraphs above name it because that is what they are about. A guard
        // its own text can trip is as broken as one its own text can satisfy.
        let producao = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
        let sem_comentarios: String = producao
            .lines()
            .filter(|channel| !channel.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let mencoes = sem_comentarios.matches("Event::MessagesChanged").count();
        assert_eq!(
            mencoes, 1,
            "`Event::MessagesChanged` is raised somewhere other than \
             `messages_changed`, which is how the revision and the notice came \
             apart the first time"
        );
    }
}

#[cfg(test)]
mod previa {
    //! What a window is handed for one attachment. ADR 0027.
    //!
    //! This is the last place the bytes and the claim are both in scope, so it
    //! is the last place the wrong one could be picked. The window past this
    //! point gets a string and no choice.

    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];

    #[test]
    fn bytes_that_agree_with_the_claim_come_back_as_a_picture() {
        // The claim is **shouted**, and the URI comes out lower case. A media
        // type is case-insensitive, so this file agrees with itself and is
        // drawn — and the spelling of the string a window receives is proof of
        // which side of the agreement wrote it. With `image/png` on both sides
        // the two are indistinguishable, and a URI spliced together from the
        // claim would pass unnoticed.
        let previa = preview_of(
            7,
            "IMAGE/PNG",
            Some(seele_core::enlace::Previa::Bytes(PNG.to_vec())),
        );
        assert_eq!(previa.attachment, 7);
        assert_eq!(previa.refusal, None);
        assert_eq!(previa.found.as_deref(), Some("image/png"));
        assert_eq!(
            previa.image.as_deref(),
            Some("data:image/png;base64,iVBORw0KGgoAAAAN"),
            "the URI a window draws from was not written from the bytes"
        );
    }

    #[test]
    fn bytes_that_disagree_with_the_claim_come_back_with_no_picture_at_all() {
        // The one that matters. A JPEG that called itself a PNG is refused, and
        // the refusal carries both halves so a sentence can name them — but
        // `image` is `None`, which is what a window acts on.
        let previa = preview_of(
            7,
            "image/png",
            Some(seele_core::enlace::Previa::Bytes(JPEG.to_vec())),
        );
        assert_eq!(previa.image, None, "a lying file was drawn anyway");
        assert_eq!(previa.refusal, Some(PreviewRefusal::Disagrees));
        assert_eq!(previa.claimed, "image/png");
        assert_eq!(previa.found.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn a_claim_never_reaches_the_string_a_window_draws_from() {
        // Whatever the sender wrote, it is quoted into `claimed` and nowhere
        // else. Here the claim is a whole `data:` URI of somebody's choosing,
        // and the only thing it produces is a refusal.
        let mentira = "data:image/png;base64,AAAA";
        let previa = preview_of(
            7,
            mentira,
            Some(seele_core::enlace::Previa::Bytes(PNG.to_vec())),
        );
        assert_eq!(previa.image, None);
        assert_eq!(previa.refusal, Some(PreviewRefusal::NotAPicture));
        assert_eq!(previa.claimed, mentira);
    }

    #[test]
    fn a_file_over_the_limit_says_so_and_says_what_the_limit_is() {
        let previa = preview_of(
            7,
            "image/png",
            Some(seele_core::enlace::Previa::GrandeDemais { tamanho: 99 }),
        );
        assert_eq!(previa.image, None);
        assert_eq!(
            previa.refusal,
            Some(PreviewRefusal::TooBig {
                limit: seele_core::PREVIEW_LIMIT
            }),
            "«too big» with no number sends somebody nowhere"
        );
    }

    #[test]
    fn a_session_that_ended_mid_fetch_is_not_a_picture_and_not_a_panic() {
        // The dropped channel. It reads as "the bytes did not come", which is
        // true, and not as any of the three that say something about the file.
        let previa = preview_of(7, "image/png", None);
        assert_eq!(previa.image, None);
        assert_eq!(previa.refusal, Some(PreviewRefusal::DidNotArrive));
    }
}

/// O log que responde «qual dos quatro deu o quê».
#[cfg(test)]
mod trilha_no_log {
    use super::*;

    /// Um assinante de `tracing` que guarda o que foi escrito.
    ///
    /// Escrito à mão, e não com `tracing-subscriber`: o que se quer aqui é o
    /// nome dos campos de um `event!`, e trazer um crate inteiro para lê-los
    /// custaria mais do que as vinte linhas abaixo.
    #[derive(Clone, Default)]
    struct Espiao(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

    impl Espiao {
        fn escrito(&self) -> Vec<String> {
            self.0
                .lock()
                .map(|linhas| linhas.clone())
                .unwrap_or_default()
        }
    }

    /// Cada campo do evento como `nome=valor`, na ordem em que foi escrito.
    struct Anotador(String);

    impl tracing::field::Visit for Anotador {
        fn record_debug(&mut self, campo: &tracing::field::Field, valor: &dyn std::fmt::Debug) {
            self.0.push_str(&format!("{}={valor:?} ", campo.name()));
        }
    }

    impl tracing::Subscriber for Espiao {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        fn event(&self, evento: &tracing::Event<'_>) {
            let mut anotador = Anotador(String::new());
            evento.record(&mut anotador);
            if let Ok(mut linhas) = self.0.lock() {
                linhas.push(anotador.0);
            }
        }

        fn enter(&self, _: &tracing::span::Id) {}

        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn a_escolha_da_pessoa_vira_degrau_arredondando_para_baixo() {
        // §5: o que se escolhe é o **máximo**. Arredondar para cima seria a
        // ponte entregando mais do que a pessoa pediu — 1080p a quem escolheu
        // 800 —, que é a regra ao contrário e o `spikes/tela-no-transporte`
        // de volta.
        let escolha = |altura, quadros| {
            limites_do_nucleo(LimitesDeTela {
                banda_bps: Some(1_200_000),
                altura_maxima: altura,
                quadros_maximos: quadros,
                prioridade: crate::types::Prioridade::Nitidez,
            })
        };

        // Os três da lista fechada caem em si mesmos.
        assert_eq!(escolha(1080, 30).resolucao, seele_core::Resolucao::P1080);
        assert_eq!(escolha(720, 15).resolucao, seele_core::Resolucao::P720);
        assert_eq!(escolha(540, 8).resolucao, seele_core::Resolucao::P540);
        assert_eq!(escolha(1080, 30).cadencia, seele_core::Cadencia::Q30);
        assert_eq!(escolha(1080, 15).cadencia, seele_core::Cadencia::Q15);
        assert_eq!(escolha(1080, 8).cadencia, seele_core::Cadencia::Q8);

        // Entre dois degraus, o de baixo.
        assert_eq!(escolha(900, 29).resolucao, seele_core::Resolucao::P720);
        assert_eq!(escolha(900, 29).cadencia, seele_core::Cadencia::Q15);

        // Abaixo do menor, o menor: 540p é o piso da lista e não há uma quarta
        // opção — abaixo dele o encoder deixa de conseguir gastar o orçamento.
        assert_eq!(escolha(240, 1).resolucao, seele_core::Resolucao::P540);
        assert_eq!(escolha(240, 1).cadencia, seele_core::Cadencia::Q8);

        // E o teto de banda atravessa como está: quem o converte em degrau é o
        // `TetoDeVideo`, e não esta travessia.
        assert_eq!(escolha(720, 30).banda_bps, Some(1_200_000));
    }

    #[test]
    fn o_modulo_de_video_e_procurado_onde_a_casca_o_poe() {
        // A lista vem daqui porque `procurar_em` a pede de fora — «onde os
        // arquivos do produto moram é decisão da casca» —, e uma lista vazia
        // faria o botão de compartilhar responder «este build não sabe» numa
        // máquina que tem o módulo ao lado do executável.
        //
        // A variável de ambiente não é mexida aqui de propósito: ela é global ao
        // processo, e uma bateria que roda em paralelo não tem como emprestá-la.
        let pastas = pastas_do_modulo();
        let perto = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
            .expect("o executável do teste");

        assert!(
            pastas.contains(&perto),
            "sem a pasta do executável, um binário empacotado não acharia o \
             módulo que veio com ele: {pastas:?}"
        );
        assert!(
            pastas.contains(&perto.join("..")),
            "sem a pasta acima, uma árvore de fonte não acharia o módulo em \
             `target/`: {pastas:?}"
        );
    }

    #[test]
    fn o_log_da_trilha_diz_qual_candidato_deu_o_que() {
        // Enquanto o app entrar por `Connection::connect`, este log é a superfície
        // inteira em que a trilha aparece. Ele escrevia só a etapa e o
        // relógio, e com isso respondia «Parada, Tentando, Desistiu, aos
        // 8003 ms» — que é a etapa e não o candidato, ou seja, não é a
        // pergunta que esta tarefa existe para responder.
        use seele_core::chegada::{Etapa, Passo};
        use std::time::Duration;

        let onde = std::net::SocketAddr::from(([203, 0, 113, 7], 8383));
        let trilha = vec![
            Passo {
                etapa: Etapa::Parada {
                    candidatos: 4,
                    com_bilhete_e_impressao: true,
                },
                em: Duration::ZERO,
            },
            Passo {
                etapa: Etapa::Avisando {
                    ponto: "encontro.exemplo:8384".into(),
                },
                em: Duration::from_millis(3),
            },
            Passo {
                etapa: Etapa::Tentando {
                    candidato: 2,
                    de: 4,
                    onde,
                    avisou: true,
                },
                em: Duration::from_millis(8003),
            },
        ];

        let espiao = Espiao::default();
        tracing::subscriber::with_default(espiao.clone(), || registrar_trilha(&trilha));

        let escrito = espiao.escrito();
        assert_eq!(
            escrito.len(),
            trilha.len(),
            "um passo por linha, e saíram {} linhas: {escrito:?}",
            escrito.len()
        );

        let Some(tentativa) = escrito.iter().find(|linha| linha.contains("Tentando")) else {
            panic!("a tentativa não foi registrada: {escrito:?}");
        };
        assert!(
            tentativa.contains("203.0.113.7:8383"),
            "o log não diz qual endereço foi tentado: {tentativa}"
        );
        assert!(
            tentativa.contains("candidato=Some(2)"),
            "o log não diz qual dos candidatos era: {tentativa}"
        );
        assert!(
            tentativa.contains("de=Some(4)"),
            "o log diz o candidato e esconde de quantos, que é metade da \
             resposta: {tentativa}"
        );

        let Some(aviso) = escrito.iter().find(|linha| linha.contains("Avisando")) else {
            panic!("o aviso não foi registrado: {escrito:?}");
        };
        assert!(
            aviso.contains("encontro.exemplo:8384"),
            "o «onde» de um aviso é o ponto de encontro, e ele não está no \
             log: {aviso}"
        );
    }
}

#[cfg(test)]
mod base64_escrito_a_mao {
    use super::base64_de;

    #[test]
    fn os_vetores_do_rfc_4648() {
        // Escrever base64 à mão é barato; escrevê-lo **errado** é caro de um
        // jeito específico: o `atob` da janela não reclama de quase nada, então
        // um preenchimento errado no último grupo vira um quadro truncado que o
        // decodificador recusa em silêncio — e o sintoma é uma tela preta, que
        // é o mesmo sintoma de outras seis coisas.
        //
        // Os vetores são os do §10 do RFC 4648, que existem exatamente para
        // isto e cobrem os três restos possíveis.
        assert_eq!(base64_de(b""), "");
        assert_eq!(base64_de(b"f"), "Zg==");
        assert_eq!(base64_de(b"fo"), "Zm8=");
        assert_eq!(base64_de(b"foo"), "Zm9v");
        assert_eq!(base64_de(b"foob"), "Zm9vYg==");
        assert_eq!(base64_de(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_de(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn os_bytes_altos_tambem() {
        // A tabela tem 64 entradas e o índice tem seis bits, então nenhum byte
        // pode sair dela — mas um `>>` com o deslocamento errado sai, e sai
        // justamente nos bytes altos, que texto ASCII nunca exercita. Um quadro
        // de vídeo é quase só byte alto.
        assert_eq!(base64_de(&[0xFF, 0xFF, 0xFF]), "////");
        assert_eq!(base64_de(&[0x00, 0x00, 0x00]), "AAAA");
        assert_eq!(base64_de(&[0xFB, 0xFF, 0xBE]), "+/++");
        // E o Annex-B começa sempre assim: `00 00 00 01`.
        assert_eq!(base64_de(&[0x00, 0x00, 0x00, 0x01]), "AAAAAQ==");
    }
}

#[cfg(test)]
mod base64_ida_e_volta {
    use super::{base64_de, de_base64};

    #[test]
    fn a_volta_desfaz_a_ida_em_todo_tamanho() {
        // Os três restos, porque é neles que base64 erra: múltiplo de três,
        // sobra de um byte (dois `=`) e sobra de dois (um `=`). Um decodificador
        // que ignorasse o resto devolveria bytes a mais no último grupo.
        for tamanho in 0..=64usize {
            let bytes: Vec<u8> = (0..tamanho).map(|i| (i * 7 % 256) as u8).collect();
            let texto = base64_de(&bytes);
            assert_eq!(
                de_base64(&texto).as_deref(),
                Some(bytes.as_slice()),
                "não fechou em {tamanho} bytes: {texto}"
            );
        }
    }

    #[test]
    fn o_que_o_navegador_escreve_e_lido() {
        // Um vetor de fora, para o par não provar só que concorda consigo
        // mesmo: `btoa("Olá, SEELE")` numa janela dá exatamente isto.
        assert_eq!(
            de_base64("T2zDoSwgU0VFTEU=").as_deref(),
            Some("Olá, SEELE".as_bytes())
        );
    }

    #[test]
    fn espaco_em_branco_nao_atrapalha() {
        // Base64 de e-mail vem quebrado em linhas, e recusar por isso seria
        // recusar o caso comum de quem cola de fora.
        assert_eq!(
            de_base64("T2zDoSwg\n  U0VFTEU =").as_deref(),
            Some("Olá, SEELE".as_bytes())
        );
    }

    #[test]
    fn texto_que_nao_e_base64_nao_vira_bytes_truncados() {
        // O caso que importa: devolver o que deu para ler produziria um arquivo
        // corrompido com cara de arquivo bom, e o defeito só apareceria na
        // máquina de quem recebeu.
        assert_eq!(de_base64("não é base64"), None);
        // Um caractere sozinho são seis bits e não fecha byte nenhum.
        assert_eq!(de_base64("QUJD RA=="), Some(b"ABCD".to_vec()));
        assert_eq!(de_base64("QUJDR"), None);
    }
}

#[cfg(test)]
mod conferir_a_troca {
    //! A troca de dispositivo que volta `Ok` tem de ter acontecido.
    //!
    //! Não há como abrir uma placa de som num teste, então o que se prende aqui
    //! é a **forma** da conferência: que ela exista no caminho dos dois lados, e
    //! que ela compare o pedido com o aberto em vez de confiar no `Ok`.
    //!
    //! O defeito que ela existe para acusar foi relatado assim: «mostra EM USO
    //! num e ESCOLHIDO no que eu escolhi, mas não muda». `EM USO` sai do
    //! dispositivo que o `Voice` abriu e `ESCOLHIDO` da preferência gravada —
    //! os dois discordando é o produto dizendo, sem saber, que a ordem foi
    //! gravada e não cumprida. Nada no caminho devolvia erro.

    const FONTE: &str = include_str!("lib.rs");

    fn corpo(assinatura: &str) -> &'static str {
        let depois = FONTE
            .split(assinatura)
            .nth(1)
            .unwrap_or_else(|| panic!("`{assinatura}` mudou de forma"));
        depois
            .split("\n    }")
            .next()
            .unwrap_or_else(|| panic!("`{assinatura}` nunca fecha"))
    }

    #[test]
    fn as_duas_trocas_conferem_o_que_pediram() {
        for assinatura in [
            "pub fn set_capture_device(&self, device: Option<String>)",
            "pub fn set_playback_device(&self, device: Option<String>)",
        ] {
            let corpo = corpo(assinatura);
            assert!(
                corpo.contains("conferir_troca"),
                "`{assinatura}` devolve `Ok` sem conferir se a troca aconteceu, \
                 e um sucesso que não é sucesso é o defeito que custou três \
                 versões nesta casca:\n{corpo}"
            );
        }
    }

    #[test]
    fn a_conferencia_compara_o_aberto_com_o_pedido() {
        let corpo = corpo("fn conferir_troca(&self, pedido: Option<&str>, lado: Lado)");
        assert!(
            corpo.contains("running.playback()") && corpo.contains("running.capture()"),
            "a conferência não lê o dispositivo que de fato abriu:\n{corpo}"
        );
        assert!(
            corpo.contains("aberto.as_deref() == Some(pedido)"),
            "a conferência não compara o aberto com o pedido:\n{corpo}"
        );
        // Sem pedido não há o que conferir: o padrão do sistema é qualquer um
        // que abra, e exigir igualdade ali recusaria o caso mais comum de todos.
        assert!(
            corpo.contains("let Some(pedido) = pedido else"),
            "a conferência exige igualdade mesmo quando ninguém pediu um \
             dispositivo em particular, o que recusaria o padrão do sistema:\n{corpo}"
        );
    }
}
