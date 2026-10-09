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

pub mod mods;
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
    AberturaDoAudio, Attachment, AttachmentRefusal, CaptureDevice, Channel, ChannelWeight,
    ConnectionError, ControlesDaVoz, EndReason, EstadoDoAparelho, EstadoDoEnvio,
    EstadoDoTesteDeMicrofone, Event, ExclusaoDoSom, FalhaAoLocalizar, FonteDeTela, LimitesDeTela,
    LinkState, LinkTrust, Message, MotivoDaRecusa, NotSavedReason, Notice, NoticeReason,
    PermissaoDeMicrofone, PermissaoDeTela, Person, PlaybackDevice, Preview, PreviewRefusal,
    PreviewRules, SaveDestination, SaveRefused, Severity, SignalBand as Band, Snapshot, SomDaTela,
    TelaEmCurso, Telemetry, Transfer, TransmissaoNaSala, Trust, VoiceMode, VoiceRoom,
    VoiceRoomSync, VolumeRefusal,
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
///
/// `preview` entrou por essa pergunta, e a resposta é **a decisão**: a casca
/// desenha imagem vinda de link de conversa, e a única forma de fazê-lo sem
/// abrir a CSP para `https:` é buscar pelo Rust e conferir os bytes. A regra
/// de «o tipo alegado tem de bater com o que os bytes dizem» é a mesma do
/// anexo, e uma segunda cópia dela seria duas regras para discordarem no dia
/// em que uma mudasse.
pub use seele_core::{conhecidos, preferences, preview, search, uri};

/// O número do fio, a versão de servidor e a linha de cada número, para quem
/// confere o manifesto do launcher contra esta build. Pelo `seele-core`, e não
/// direto do `seele-proto`: o ADR 0002 deixa este crate ver só o núcleo.
pub use seele_core::{linha_do_protocolo, PROTOCOL_VERSION, VERSAO_DO_SERVIDOR};

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

/// A sensibilidade padrão da ativação por voz **com** redução de ruído, em dBFS.
///
/// F01, e o número vem do núcleo: uma cópia escrita aqui seria uma segunda
/// decisão sobre a mesma régua, esperando para discordar.
pub const ABERTURA_COM_SUPRESSAO_DBFS: f32 = seele_core::ABERTURA_COM_SUPRESSAO_DBFS;

/// A sensibilidade padrão da ativação por voz **sem** redução de ruído, em dBFS.
///
/// Ver [`ABERTURA_COM_SUPRESSAO_DBFS`]: os dois padrões existem porque o alvo de
/// −60 dBFS de F01 é do caminho com filtro.
pub const ABERTURA_SEM_SUPRESSAO_DBFS: f32 = seele_core::ABERTURA_SEM_SUPRESSAO_DBFS;

/// O extremo sensível da faixa de sensibilidade, em dBFS.
pub const ABERTURA_MINIMA_DBFS: f32 = *seele_core::FAIXA_DE_ABERTURA_DBFS.start();

/// O extremo surdo da faixa de sensibilidade, em dBFS.
pub const ABERTURA_MAXIMA_DBFS: f32 = *seele_core::FAIXA_DE_ABERTURA_DBFS.end();

/// Um teste de microfone em curso, sem sessão nenhuma. F01.
///
/// # Por que ele é livre, como a lista de aparelhos
///
/// Pela mesma razão que [`capture_devices`]: testar o microfone é algo que se faz
/// **antes** de conectar pelo menos tanto quanto durante. Pendurá-lo numa sessão
/// viva poria o controle atrás da porta que ele existe para abrir — a pessoa teria
/// de entrar em algum lugar para descobrir que o microfone estava mudo.
///
/// Largar esta alça encerra o teste.
#[derive(Debug)]
pub struct TesteDeMicrofone {
    dentro: seele_core::teste_de_microfone::TesteDeMicrofone,
}

impl TesteDeMicrofone {
    /// Abre o microfone e começa a medir.
    ///
    /// `captura` e `saida` são ids de [`capture_devices`] e [`playback_devices`];
    /// `None` é o aparelho padrão da máquina. `supressao` e `abertura_dbfs` são os
    /// mesmos controles da conversa — o teste tem de medir a régua que vale lá.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NoAudioDevice`] quando não há microfone ou o aparelho
    /// escolhido sumiu.
    pub fn abrir(
        captura: Option<String>,
        saida: Option<String>,
        supressao: f32,
        abertura_dbfs: Option<f32>,
    ) -> Result<Self, ConnectionError> {
        let escolha = seele_core::DeviceChoice {
            capture: captura,
            playback: saida,
        };
        seele_core::teste_de_microfone::TesteDeMicrofone::abrir(&escolha, supressao, abertura_dbfs)
            .map(|dentro| Self { dentro })
            .map_err(|erro| {
                tracing::warn!(%erro, "o teste de microfone não abriu");
                ConnectionError::NoAudioDevice
            })
    }

    /// O que o teste tem a contar agora.
    #[must_use]
    pub fn estado(&self) -> EstadoDoTesteDeMicrofone {
        let dentro = self.dentro.estado();
        EstadoDoTesteDeMicrofone {
            nivel_dbfs: dentro.nivel_dbfs,
            aberto: dentro.aberto,
            aberturas: dentro.aberturas,
            quadros_retidos: dentro.quadros_retidos,
            corte_dbfs: dentro.corte_dbfs,
            monitorando: self.dentro.monitorando(),
            falha: self.dentro.falha(),
        }
    }

    /// Liga ou desliga ouvir o próprio microfone. Começa desligado.
    pub fn set_monitorar(&self, ligado: bool) {
        self.dentro.set_monitorar(ligado);
    }

    /// Troca a força da supressão sem reabrir o aparelho. F02.
    pub fn set_supressao(&self, forca: f32) {
        self.dentro.set_supressao(forca);
    }

    /// Troca a sensibilidade sem reabrir o aparelho. F01.
    pub fn set_abertura_dbfs(&self, dbfs: Option<f32>) {
        self.dentro.set_abertura_dbfs(dbfs);
    }
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

/// How many messages of history to ask for when a channel opens.
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
    /// A impressão digital que esta conexão confere, dentro do TLS e antes do
    /// `Hello`.
    ///
    /// É a do link, quando a pessoa colou um nesta sessão. Na volta pela lista
    /// de servidores, sem link, é a que a lista guardou: a impressão que uma
    /// conexão anterior **aceitou** ([`impressao_a_guardar`]), que quase
    /// sempre é a do link daquela vez, mas não é a do link quando ele
    /// discordava do pin. Quem decide qual das duas vale é
    /// [`impressao_a_conferir`], e o `connect` do app a usa.
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
    ///
    /// Verdadeiro não abre nada ao conectar: pela pendência 52 o áudio abre
    /// quando a pessoa entra numa sala de voz, e fecha quando ela sai.
    pub audio: bool,
    /// Which microphone to open, as a [`CaptureDevice::id`].
    ///
    /// `None` is the machine's default, and is what every session took before
    /// there was a screen to choose on. A device that is gone by the time the
    /// voice opens does **not** refuse the room — see `abrir_a_voz`, which
    /// falls back to the default rather than turning a stale preference into a
    /// room nobody can speak in.
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
    /// Perguntando ao ponto de encontro onde a sala de um código mora.
    ///
    /// Só numa ligação. Ver a variante do núcleo: ao contrário do aviso, esta
    /// etapa pode reprovar a chegada, com [`ConnectionError::NaoLocalizou`].
    Localizando {
        /// O ponto de encontro a quem se pergunta.
        ponto: String,
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
/// trilha já estava com quem a lê, no que digitou, no convite, na lista de
/// conhecidos ou na resposta do quarto sobre o mesmo servidor.
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
            Self::Localizando { .. } => "Localizando",
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
            Etapa::Localizando { ponto } => Self::Localizando {
                ponto: ponto.clone(),
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
/// `candidato = 0` para ela seria inventar o zero. Para `Avisando` e
/// `Localizando` o «onde» é o ponto de encontro, que é para onde aquele passo
/// mandou alguma coisa; para
/// `Parada` o `de` é quantos endereços o convite trouxe, que é a mesma
/// grandeza que o `de` de uma tentativa — «de quantos».
fn campos_do_passo(etapa: &seele_core::chegada::Etapa) -> (Option<String>, Option<u8>, Option<u8>) {
    use seele_core::chegada::Etapa;
    match etapa {
        Etapa::Localizando { ponto } | Etapa::Avisando { ponto } => {
            (Some(ponto.clone()), None, None)
        }
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
        | Etapa::Localizando { .. }
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
    ImagemDeMod {
        pedido: seele_core::PedidoDeImagem,
        bytes: Vec<u8>,
        resposta: tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>,
    },

    ModRequest {
        request: u32,
        id: String,
        channel: ChannelId,
        payload: String,
    },
    /// Devolve à sonda o caminho medido da última visita a este servidor.
    LembrarCaminho(u32),
    EnterVoiceRoom(VoiceRoomId, Option<String>),
    LeaveVoiceRoom,
    OpenChannel(ChannelId),
    Send {
        channel: ChannelId,
        body: String,
        /// A chave escolhida por quem pediu o envio.
        ///
        /// Escolhida **antes** do comando e carregada nele, e não sorteada no
        /// laço: é ela que liga a pendente desenhada na tela à confirmação que
        /// volta, e um número sorteado aqui dentro seria um número que a casca
        /// não tem como esperar. Ver [`Connection::send_message`].
        id: ClientMessageId,
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
    /// Salvar um anexo nesta pasta. O nome não viaja aqui: é lido do histórico
    /// por quem executa, como o tipo alegado da prévia.
    SaveAttachment {
        attachment: seele_core::AttachmentId,
        folder: String,
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
    /// Passar a receber a imagem de uma transmissão, ou parar.
    Assistir {
        tela: seele_core::ScreenId,
        quero: bool,
    },
    /// Dizer ao servidor o que esta máquina aceita no caminho entre pares.
    ConsentirNoCaminhoEntrePares {
        pares_que_atende: u8,
        assiste_por_par: bool,
    },
    Shutdown,
}

/// Qual transmissão esta sessão escolheu assistir. ADR 0054.
///
/// # Três estados, e são três de propósito
///
/// A ausência de escolha e a escolha de não ver **não são a mesma coisa**, e
/// tratá-las como uma custou um defeito de campo: «cliquei pra não ver, ele fecha
/// e depois abre novamente sozinho».
///
/// - [`Self::NinguemEscolheu`] — o servidor decide, e ele liga todo mundo na
///   primeira transmissão da sala. É o certo para quem não disse nada: ninguém
///   quer clicar para ver a única coisa que está acontecendo.
/// - [`Self::Nenhuma`] — escolheu **não** ver. Vale contra o religar do servidor.
/// - [`Self::Esta`] — escolheu esta, e a imagem pode ainda não ter chegado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscolhaDeTela {
    /// Ninguém escolheu. O religar automático do servidor vale.
    NinguemEscolheu,
    /// Escolheu não assistir nenhuma.
    Nenhuma,
    /// Escolheu esta.
    Esta(seele_core::ScreenId),
}

impl EscolhaDeTela {
    /// O `ScreenId` escolhido, se houver um.
    #[must_use]
    pub const fn tela(self) -> Option<seele_core::ScreenId> {
        match self {
            Self::Esta(tela) => Some(tela),
            Self::NinguemEscolheu | Self::Nenhuma => None,
        }
    }
}

/// State the driver thread writes and the shell reads.
struct Shared {
    /// O caminho de subida que a sonda mediu, em bits por segundo.
    ///
    /// Zero é «ninguém mediu ainda»: a sonda mede **enquanto a tela transmite**,
    /// porque é a tela que enche o cano. Quem o lê é a casca, para guardá-lo na
    /// lista de conhecidos e devolvê-lo à sonda na próxima visita a este mesmo
    /// servidor — é o que evita os doze segundos de imagem ruim que a escada
    /// gasta reaprendendo um cano que já mediu.
    ///
    /// Átomo e não `Event`, pela razão que o `rtt` já dá: é valor de agora, e
    /// não coisa que alguém precise ver acontecer.
    caminho_medido: std::sync::atomic::AtomicU32,
    room: Mutex<Room>,
    listeners: Mutex<Vec<Arc<dyn EventListener>>>,
    /// The voice path, and what the person chose for it.
    ///
    /// **Pendência 52.** A voz só existe dentro de uma sala de voz, então fora
    /// dela não há `Voice` nenhuma para lembrar o microfone escolhido, o mudo ou
    /// o modo — e o que a pessoa escolhe fora de sala tem de valer quando a voz
    /// abrir. Por isso a escolha mora aqui, ao lado da voz, e não dentro dela.
    ///
    /// Continua havendo **uma** resposta para «qual aparelho»: com a voz viva,
    /// a troca passa por ela e a escolha só é gravada se a troca deu certo; sem
    /// voz, a escolha é só gravada. Ver [`VozDaSala`].
    voice: VozDaSala<Voice>,
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
    /// Qual transmissão esta sessão escolheu assistir, e em que estado a escolha
    /// está. ADR 0054.
    ///
    /// # Por que ela mora aqui, e não na janela
    ///
    /// Porque a janela é descartável e a sessão não. A escolha vivia numa
    /// variável de JavaScript (`telaQuerida`), então uma recarga no meio de uma
    /// sessão a perdia — e com ela a marca do botão, o controle de som daquela
    /// tela e a resposta a «qual delas é esta». Deste lado ela sobrevive à
    /// janela, e é deste lado que o servidor pergunta.
    ///
    /// Os três estados são os mesmos que a janela já distinguia, e continuam
    /// sendo três de propósito — ver [`EscolhaDeTela`].
    tela_escolhida: Mutex<EscolhaDeTela>,
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

    /// Qual transmissão esta sessão escolheu assistir. ADR 0054.
    ///
    /// Um cadeado envenenado responde [`EscolhaDeTela::NinguemEscolheu`], que é o
    /// comportamento de antes de haver escolha: o servidor decide.
    fn tela_escolhida(&self) -> EscolhaDeTela {
        self.tela_escolhida
            .lock()
            .map_or(EscolhaDeTela::NinguemEscolheu, |escolha| *escolha)
    }

    /// Guarda a escolha, e devolve se ela mudou.
    fn gravar_tela_escolhida(&self, escolha: EscolhaDeTela) -> bool {
        let Ok(mut guardada) = self.tela_escolhida.lock() else {
            return false;
        };
        if *guardada == escolha {
            return false;
        }
        *guardada = escolha;
        true
    }

    /// Esquece a escolha quando a transmissão escolhida saiu do ar. ADR 0054.
    ///
    /// **Volta a `NinguemEscolheu`, e não a `Nenhuma`.** A diferença é o defeito
    /// que a distinção entre os dois estados existe para evitar: `Nenhuma` é uma
    /// recusa que vale contra o religar do servidor, e herdá-la de uma
    /// transmissão que acabou faria a próxima a começar ser recusada por uma
    /// decisão que ninguém tomou sobre ela.
    fn esquecer_tela_que_saiu(&self) {
        let Some(escolhida) = self.tela_escolhida().tela() else {
            return;
        };
        let ainda_existe = self
            .room
            .lock()
            .is_ok_and(|room| room.telas.contains_key(&escolhida));
        if !ainda_existe {
            let _ = self.gravar_tela_escolhida(EscolhaDeTela::NinguemEscolheu);
        }
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
                seele_core::TransferNotice::VolumeRecusado { token, motivo } => {
                    Transfer::VolumeRecusado {
                        token,
                        motivo: recusa_de_volume(motivo),
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
    /// Por onde a thread da conexão diz que terminou, para
    /// [`Connection::encerrar_e_esperar`].
    ///
    /// O sinal sai quando `drive` volta — com a despedida do enlace já feita
    /// — e **antes** de o runtime cair. Num `Mutex` porque o `Receiver` não é
    /// `Sync` e a casca segura a `Connection` atrás de um `Arc`; `None` depois
    /// de alguém já ter visto o fim, e na `Connection` montada à mão dos testes.
    encerrada: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
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
            caminho_medido: std::sync::atomic::AtomicU32::new(0),
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            icon_revision: std::sync::atomic::AtomicU64::new(0),
            person_icons_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: VozDaSala::nova(config.audio, chosen_devices(&config)),
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
            tela_escolhida: Mutex::new(EscolhaDeTela::NinguemEscolheu),
            fontes_de_tela: Mutex::new(Vec::new()),
        });

        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (encerrada_tx, encerrada_rx) = std::sync::mpsc::channel();

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
                // **Antes de o runtime cair, e não depois.** `drive` voltou com
                // a despedida do enlace feita (`Enlace::sair`), e é isso que
                // `encerrar_e_esperar` espera. Soltar o runtime espera a piscina
                // de bloqueio sem prazo — a bomba da tela é parada ali —, e um
                // sinal depois dele seguraria a saída do app por isso.
                let _ = encerrada_tx.send(());
                drop(runtime);
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
            encerrada: Mutex::new(Some(encerrada_rx)),
        });
        Ok((connection, trust))
    }

    /// Puts the connection into a voice room.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn enter_voice_room(
        &self,
        voice_room: u32,
        password: Option<String>,
    ) -> Result<(), ConnectionError> {
        self.command(Command::EnterVoiceRoom(VoiceRoomId(voice_room), password))
    }

    /// Takes the connection out.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn leave_voice_room(&self) -> Result<(), ConnectionError> {
        self.command(Command::LeaveVoiceRoom)
    }

    /// Opens a channel and asks for the page of history behind it.
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

    /// Salva um anexo em `folder`, com o nome que veio com ele — ou ao lado.
    ///
    /// **O nome não é argumento.** Ele é lido do histórico local desta sessão,
    /// como o tipo alegado da prévia, e nunca vem da janela: uma página que o
    /// devolvesse poderia devolver outro. Conferido em
    /// `seele_core::anexo_no_disco::nome_seguro`, um nome que não é só um nome
    /// é recusado; e a gravação nunca substitui o que já está na pasta — se o
    /// nome estiver tomado, o arquivo vai para «foto (2).png». A regra inteira
    /// mora em `seele_core::anexo_no_disco`.
    ///
    /// Pede, e não espera: o resultado volta como [`Event::TransferChanged`],
    /// com [`Transfer::Saved`] e o caminho real, ou [`Transfer::NotSaved`] e o
    /// motivo. O arquivo é marcado com a quarentena do próprio sistema ao ser
    /// gravado — o que não é antivírus, e este produto não tem um.
    ///
    /// `folder` é escolhida por quem chama, e tem de ser um caminho absoluto: a
    /// casca do desktop passa a pasta de downloads, que ela mesma calcula.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn save_attachment(&self, attachment: u64, folder: String) -> Result<(), ConnectionError> {
        self.command(Command::SaveAttachment {
            attachment: seele_core::AttachmentId(attachment),
            folder,
        })
    }

    /// Onde [`Self::save_attachment`] gravaria este anexo, ou por que não
    /// gravaria.
    ///
    /// Só para a frase de confirmação: a mesma regra, lida agora, para que a
    /// pessoa veja o nome e a pasta antes de apertar. Na hora de gravar, tudo é
    /// derivado de novo.
    ///
    /// # Errors
    ///
    /// [`SaveRefused`], com o motivo, o nome que veio e a pasta.
    pub fn save_destination(
        &self,
        attachment: u64,
        folder: String,
    ) -> Result<SaveDestination, SaveRefused> {
        let (_, nome) = destino_de(&self.shared, seele_core::AttachmentId(attachment), &folder)?;
        Ok(SaveDestination {
            beside: seele_core::anexo_no_disco::nome_ao_lado(&nome, 2),
            file_name: nome,
            folder,
        })
    }

    /// Fetches a small attachment and says whether a window may draw it.
    ///
    /// **On a press, never on a scroll**, and that is a decision this call
    /// leaves no room to get wrong: it downloads. The file lives on the server,
    /// so looking at it costs the host's uplink, and a channel that previewed
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

    /// O maior corpo que o servidor aceita, **em bytes**.
    ///
    /// # Por que a casca precisa deste número
    ///
    /// Porque ela tinha o próprio, e o dela contava outra coisa. O campo da
    /// janela aceitava 4.000 **unidades de texto** e o contrato limita 4.096
    /// **bytes**: 3.000 letras `á` caem dentro do primeiro e ocupam 6.000 do
    /// segundo. E a recusa não era uma recusa — o quadro falhava ao ser
    /// codificado, `dizer` devolvia erro, e o laço da sessão tratava isso como
    /// «o enlace morreu» e **encerrava a sessão**. Um acento a mais derrubava a
    /// conexão sem uma palavra na tela.
    #[must_use]
    pub const fn limite_da_mensagem() -> usize {
        seele_core::MAX_BODY_LEN
    }

    /// Says something in o canal.
    ///
    /// # O que mudou, e é a metade que faltava
    ///
    /// A chave da mensagem é escolhida **aqui**, antes de qualquer coisa sair, e
    /// a mensagem é anotada como pendente antes de ser entregue ao enlace. Com
    /// isso: o texto existe em algum lugar que não é o campo da janela, a casca
    /// pode desenhá-lo como «enviando», a confirmação o encontra pela chave, e
    /// tentar de novo usa a **mesma** chave — que é o que faz o servidor
    /// reconhecer a repetição em vez de publicar duas vezes.
    ///
    /// Devolve a chave, para quem quiser acompanhar aquela mensagem.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn send_message(&self, channel: u32, body: String) -> Result<String, ConnectionError> {
        let corpo = body.trim().to_owned();
        if corpo.is_empty() {
            return Ok(String::new());
        }
        let id = ClientMessageId(next_client_message_id());
        let channel = ChannelId(channel);
        self.anotar_pendente(channel, id, corpo.clone());

        // **O teto é conferido aqui**, e não descoberto quando o quadro se
        // recusa a ser codificado. Ver [`Self::limite_da_mensagem`]: o caminho
        // antigo encerrava a sessão.
        if corpo.len() > seele_core::MAX_BODY_LEN {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "o teto é 4 KiB e cabe folgado num u32"
            )]
            let limite = seele_core::MAX_BODY_LEN as u32;
            self.recusar_pendente(id, seele_core::MessageRefusal::TooLong { limit: limite });
            return Ok(id.get().to_string());
        }

        self.command(Command::Send {
            channel,
            body: corpo,
            id,
        })?;
        Ok(id.get().to_string())
    }

    /// Manda de novo uma mensagem que não foi gravada, com a **mesma** chave.
    ///
    /// A mesma chave é o ponto: o servidor é idempotente por ela, então repetir
    /// uma mensagem que na verdade havia sido gravada devolve a original em vez
    /// de publicar uma segunda. É por isso que tentar de novo é seguro mesmo
    /// depois de uma queda — o estado [`EstadoDoEnvio::SemResposta`] existe para
    /// dizer exatamente esse caso.
    ///
    /// `false` quando não há pendente com essa chave: ela já foi confirmada ou
    /// descartada, e mandar de novo seria publicar de novo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn reenviar_mensagem(&self, client_message_id: &str) -> Result<bool, ConnectionError> {
        let Ok(chave) = client_message_id.parse::<u64>() else {
            // Uma chave que não é número não é de nenhuma pendente. `false` é a
            // mesma resposta de «essa já foi confirmada», e é a honesta.
            return Ok(false);
        };
        let id = ClientMessageId(chave);
        let Some((channel, corpo)) = self.shared.room.lock().ok().and_then(|room| {
            room.pendentes
                .iter()
                .find(|pendente| pendente.client_message_id == id)
                .map(|pendente| (pendente.channel, pendente.body.clone()))
        }) else {
            return Ok(false);
        };
        self.anotar_pendente(channel, id, corpo.clone());
        self.command(Command::Send {
            channel,
            body: corpo,
            id,
        })?;
        Ok(true)
    }

    /// Desiste de uma mensagem que não foi gravada, e devolve o texto dela.
    ///
    /// Devolve o corpo para que a casca possa pô-lo de volta no compositor: uma
    /// desistência que apagasse o texto seria o R05 de novo, por outra porta.
    #[must_use]
    pub fn descartar_mensagem(&self, client_message_id: &str) -> Option<String> {
        let chave = client_message_id.parse::<u64>().ok()?;
        let corpo = self
            .shared
            .room
            .lock()
            .ok()?
            .descartar_envio(ClientMessageId(chave));
        if corpo.is_some() {
            self.shared.messages_changed();
        }
        corpo
    }

    /// Anota uma pendente e acorda quem desenha.
    fn anotar_pendente(&self, channel: ChannelId, id: ClientMessageId, body: String) {
        if let Ok(mut room) = self.shared.room.lock() {
            room.anotar_envio(channel, id, body, agora_em_segundos());
        }
        self.shared.messages_changed();
    }

    /// Marca uma pendente como recusada e acorda quem desenha.
    fn recusar_pendente(&self, id: ClientMessageId, motivo: seele_core::MessageRefusal) {
        let mexeu = self
            .shared
            .room
            .lock()
            .map(|mut room| room.recusar_envio(id, motivo))
            .unwrap_or(false);
        if mexeu {
            self.shared.messages_changed();
        }
    }

    /// Requests an enabled MOD action. Responses arrive as `Event::ModReply`.
    /// Returns an error when disconnected.
    pub fn mod_request(
        &self,
        request: u32,
        id: String,
        channel: u32,
        payload: String,
    ) -> Result<(), ConnectionError> {
        self.command(Command::ModRequest {
            request,
            id,
            channel: ChannelId(channel),
            payload,
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
    /// # Uma pendente não tem o que remover
    ///
    /// Zero não é um identificador de mensagem: é o que
    /// [`Message::id`] vale numa mensagem que esta máquina escreveu e o servidor
    /// ainda não confirmou. Ela não existe lá, então não há o que tirar de canal
    /// nenhum — e mandar o pedido faria o servidor responder «não há tal
    /// mensagem», que é verdade e é a frase errada para quem clicou num texto que
    /// está na tela.
    ///
    /// Quem quer largar uma pendente usa [`Self::descartar_mensagem`], que
    /// endereça pela chave desta máquina.
    ///
    /// **Este guarda nasceu de uma reprovação.** Sem ele,
    /// `conformance::moderacao` pegava `messages()[0].id` numa lista onde a
    /// primeira linha era uma pendente, pedia a remoção do zero, e recebia «não
    /// há tal mensagem» no lugar da recusa de permissão que o teste mede — o
    /// primeiro caller a pisar nisto foi um teste, e o próximo seria uma tela.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over, or
    /// [`ConnectionError::UnknownMessage`] for a message the server never saw.
    pub fn remove_message(&self, message: u64) -> Result<(), ConnectionError> {
        if message == 0 {
            return Err(ConnectionError::UnknownMessage);
        }
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

    /// Asks the server to destroy the channel, and everything written in it —
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

    /// Asks what destroying the channel would cost, and waits for the answer.
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
    /// Fora de sala não há voz para mutar, e o mudo é guardado: a voz abre muda
    /// quando a pessoa entrar numa sala. Pendência 52.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn set_muted(&self, on: bool) -> Result<(), ConnectionError> {
        self.shared.voice.escolher(|escolhida| escolhida.mudo = on);
        self.command(Command::SetMuted(on))
    }

    /// Mutes or unmutes the speakers — Isolamento total.
    ///
    /// Guardado fora de sala, como o mudo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn set_total_isolation(&self, on: bool) -> Result<(), ConnectionError> {
        self.shared
            .voice
            .escolher(|escolhida| escolhida.isolamento = on);
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
    /// **Fora de sala, só grava.** Pendência 52: sem sala não há voz aberta para
    /// trocar, e a escolha vale quando a pessoa entrar numa. Recusar ali seria a
    /// tela dizendo que o microfone sumiu a quem só ainda não entrou.
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
        self.switch_device(Lado::Entrada, pedido.clone(), |running, media, ssrc| {
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

    /// Passa a receber a imagem de uma transmissão, ou para de receber.
    ///
    /// **Assistir é um pedido.** Enquanto havia uma transmissão por sala, ela era
    /// empurrada para todo mundo; com várias, empurrar todas para todos faria cada
    /// máquina baixar uma cópia de cada e abrir um decodificador para cada — e nem
    /// a descida nem a CPU de quem assiste entram na conta do teto.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] sem sessão.
    /// **A escolha é gravada antes de o pedido sair**, e é o que a torna a
    /// autoridade sobre o que aparece. ADR 0054.
    ///
    /// Antes ela morava numa variável da janela, e a janela não sobrevive a uma
    /// recarga. Deste lado ela sobrevive, e é deste lado que
    /// [`Snapshot::tela`] e [`TransmissaoNaSala::assistida`] a leem — então a
    /// autoria, os controles e os metadados param de poder apontar para outra
    /// tela.
    pub fn assistir(&self, tela: u32, quero: bool) -> Result<(), ConnectionError> {
        let tela = seele_core::ScreenId(tela);
        let escolha = if quero {
            EscolhaDeTela::Esta(tela)
        } else if self.shared.tela_escolhida().tela() == Some(tela)
            || self.shared.tela_escolhida() == EscolhaDeTela::NinguemEscolheu
        {
            // **Recusar a que se estava assistindo é escolher não assistir.**
            // Recusar **outra** não mexe na escolha: é o que acontece quando o
            // servidor empurra uma transmissão que esta pessoa não pediu, e
            // devolvê-la não pode desfazer a escolha dela.
            EscolhaDeTela::Nenhuma
        } else {
            self.shared.tela_escolhida()
        };
        if self.shared.gravar_tela_escolhida(escolha) {
            self.shared.notify(&Event::ScreenChanged);
        }
        self.command(Command::Assistir { tela, quero })
    }

    /// Qual transmissão esta sessão escolheu assistir, e em que estado. ADR 0054.
    ///
    /// A casca a lê para desenhar a fileira de botões sem guardar a escolha
    /// dela própria — duas cópias da mesma escolha são duas cópias que
    /// discordam depois de uma recarga.
    #[must_use]
    pub fn tela_escolhida(&self) -> Option<u32> {
        self.shared.tela_escolhida().tela().map(|tela| tela.0)
    }

    /// O volume do som da transmissão. `1.0` é o de origem, `0.0` é calado.
    ///
    /// **Independente do volume das pessoas**, que é o pedido inteiro do R17:
    /// silenciar o conteúdo compartilhado tem de preservar a conversa. O
    /// isolamento total continua calando a mistura inteira, porque é isso que
    /// ele diz que faz.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] sem voz aberta.
    pub fn ajustar_volume_da_tela(&self, volume: f32) -> Result<(), ConnectionError> {
        let voice = self
            .shared
            .voice
            .lock()
            .map_err(|_| ConnectionError::NotConnected)?;
        let voice = voice.as_ref().ok_or(ConnectionError::NotConnected)?;
        voice.set_volume_da_tela(volume);
        Ok(())
    }

    /// Cala ou devolve o som da transmissão, sem mexer no volume dela.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] sem voz aberta.
    pub fn calar_a_tela(&self, calada: bool) -> Result<(), ConnectionError> {
        let voice = self
            .shared
            .voice
            .lock()
            .map_err(|_| ConnectionError::NotConnected)?;
        let voice = voice.as_ref().ok_or(ConnectionError::NotConnected)?;
        voice.set_tela_calada(calada);
        Ok(())
    }

    /// O que esta máquina faz com o **próprio** áudio ao capturar uma tela.
    ///
    /// R21 e ADR 0054. Três respostas, e uma delas é «a saída inteira entra» —
    /// que a tela tem de dizer, junto com as duas saídas que existem:
    /// compartilhar uma janela em vez do monitor, ou transmitir sem áudio.
    ///
    /// Estática e não por sessão: é uma propriedade desta máquina e deste
    /// sistema, e vale antes de haver sessão — que é quando a pessoa está
    /// escolhendo o que compartilhar.
    #[must_use]
    pub fn exclusao_do_som_da_captura() -> ExclusaoDoSom {
        seele_core::exclusao_medida_no_sistema().into()
    }

    /// O volume e o mudo do som da transmissão, como a casca os desenha.
    ///
    /// `None` sem voz aberta, que é a mesma resposta de «não há o que ajustar».
    #[must_use]
    pub fn som_da_tela(&self) -> Option<SomDaTela> {
        let voice = self.shared.voice.lock().ok()?;
        let voice = voice.as_ref()?;
        Some(SomDaTela {
            volume: voice.volume_da_tela(),
            calada: voice.tela_calada(),
        })
    }

    /// Diz o que esta máquina aceita no caminho entre pares — ADR 0045 do §5.
    ///
    /// **As duas perguntas são separadas de propósito**, e o `control.rs` é
    /// explícito em não juntá-las num interruptor só:
    ///
    /// - `pares_que_atende` é *quanto da minha internet eu aceito gastar
    ///   servindo os outros*. Número, e não sim/não: um interruptor responderia
    ///   pela pessoa, fixando o teto em um. `0` é «não empresto».
    /// - `assiste_por_par` é *aceito que o meu endereço seja entregue a quem
    ///   for me servir*. É a metade de privacidade, e ela é de quem recebe.
    ///
    /// Esta versão do cliente atende no máximo
    /// [`pares_que_esta_versao_atende`], e o núcleo **baixa** um pedido maior
    /// em vez de recusá-lo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] sem sessão.
    pub fn consentir_no_caminho_entre_pares(
        &self,
        pares_que_atende: u8,
        assiste_por_par: bool,
    ) -> Result<(), ConnectionError> {
        self.command(Command::ConsentirNoCaminhoEntrePares {
            pares_que_atende,
            assiste_por_par,
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
    /// Fora de sala, só grava — ver [`Connection::set_capture_device`].
    ///
    /// # Errors
    ///
    /// [`ConnectionError::PlaybackDeviceGone`] when the machine is not offering that
    /// device any more, in which case **nothing changed** and the sound is still
    /// coming out of the old one. [`ConnectionError::NoAudioDevice`] when this session
    /// has no audio at all.
    pub fn set_playback_device(&self, device: Option<String>) -> Result<(), ConnectionError> {
        let pedido = device.clone();
        self.switch_device(Lado::Saida, pedido.clone(), |running, media, ssrc| {
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
            Lado::Entrada => running.capture().map(|aparelho| aparelho.id),
            Lado::Saida => running.playback().map(|aparelho| aparelho.id),
        };
        conferencia(Some(pedido), aberto.as_deref(), lado)
    }

    /// Replaces the live voice path with one the caller reopens.
    ///
    /// Shared by both switches so that the order stays one decision: open the
    /// new path, and only then let go of the old. Written twice, one copy would
    /// eventually drop first and leave somebody with no audio at all on the one
    /// path where the device turned out to be missing.
    ///
    /// **Sem voz viva, só grava** (pendência 52): fora de sala, ou com a voz
    /// ainda abrindo. A abertura em curso confere a escolha quando chega, e
    /// recomeça se ela mudou no meio — ver [`PortaDaVoz::chegou`].
    fn switch_device(
        &self,
        lado: Lado,
        pedido: Option<String>,
        reopen: impl FnOnce(&Voice, MediaChannel, Ssrc) -> Result<Voice, ConnectionError>,
    ) -> Result<(), ConnectionError> {
        if !self.shared.voice.quer_audio {
            return Err(ConnectionError::NoAudioDevice);
        }
        let Ok(mut voice) = self.shared.voice.lock() else {
            return Err(ConnectionError::NoAudioDevice);
        };
        if let Some(running) = voice.as_ref() {
            let Some((media, ssrc)) = self.shared.media.lock().ok().and_then(|slot| slot.clone())
            else {
                return Err(ConnectionError::NotConnected);
            };
            let fresh = reopen(running, media, ssrc)?;
            *voice = Some(fresh);
        }
        // Gravada só depois da troca dar certo: uma que falhou deixou o aparelho
        // antigo de pé, e é ele que continua sendo a escolha.
        self.shared.voice.guardar_aparelho(lado, pedido);
        Ok(())
    }

    /// Chooses how the microphone opens.
    ///
    /// Guardado fora de sala, e aplicado quando a voz abrir. A casca chama isto
    /// logo depois do `connect`, com o modo gravado em disco, e é por isso que
    /// ele tem de valer sem voz nenhuma de pé.
    pub fn set_voice_mode(&self, mode: VoiceMode) {
        self.shared
            .voice
            .escolher(|escolhida| escolhida.modo = mode.into());
    }

    /// Liga, desliga ou dosa a supressão de ruído do microfone. F02.
    ///
    /// `1.0` é ela inteira, `0.0` a desliga. A escolha vale para esta sessão e
    /// **não** é gravada aqui: gravar é da casca, que é quem tem onde. Ver
    /// `seele_core::preferences::Preferences::set_supressao_de_ruido`.
    ///
    /// Guardada fora de sala, e aplicada quando a voz abrir.
    pub fn set_supressao_de_ruido(&self, forca: f32) {
        self.shared
            .voice
            .escolher(|escolhida| escolhida.supressao = forca.clamp(0.0, 1.0));
    }

    /// A sensibilidade da ativação por voz, em dBFS. `None` volta ao padrão. F01.
    ///
    /// O padrão depende de a supressão estar ligada — o alvo de −60 dBFS é do
    /// caminho **com** supressão. Ver `seele_core::voice`.
    ///
    /// Guardada fora de sala, e aplicada quando a voz abrir.
    pub fn set_abertura_da_voz_dbfs(&self, dbfs: Option<f32>) {
        let dbfs = dbfs.map(|valor| {
            valor.clamp(
                *seele_core::FAIXA_DE_ABERTURA_DBFS.start(),
                *seele_core::FAIXA_DE_ABERTURA_DBFS.end(),
            )
        });
        self.shared
            .voice
            .escolher(|escolhida| escolhida.abertura_dbfs = dbfs);
    }

    /// Os controles de voz desta sessão, como a casca os desenha. F01 e F02.
    ///
    /// Com a voz viva, os dela. **Sem voz — fora de sala, ou com ela abrindo —,
    /// os que a pessoa escolheu**, que são os que a voz vai vestir ao abrir
    /// (pendência 52). Era `None` aí, e a tela de configuração caía nos
    /// valores do disco, que podem não ser os desta sessão.
    ///
    /// **`None` numa sessão que entrou sem áudio**, e a casca lê o disco: ali
    /// não há voz que vá vestir controle nenhum, e os guardados seriam só os
    /// padrões, por cima da escolha gravada (M-e da rodada 1 do lote I-a).
    #[must_use]
    pub fn controles_da_voz(&self) -> Option<ControlesDaVoz> {
        if !self.shared.voice.quer_audio {
            return None;
        }
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                let (abertura_dbfs, escolhida) = voice.abertura_dbfs();
                return Some(ControlesDaVoz {
                    supressao: voice.supressao(),
                    abertura_dbfs,
                    abertura_escolhida: escolhida,
                    abertura_minima_dbfs: *seele_core::FAIXA_DE_ABERTURA_DBFS.start(),
                    abertura_maxima_dbfs: *seele_core::FAIXA_DE_ABERTURA_DBFS.end(),
                });
            }
        }
        let escolhida = self.shared.voice.escolhida();
        Some(ControlesDaVoz {
            supressao: escolhida.supressao,
            // O mesmo padrão da voz, e pela mesma regra: −60 dBFS é do caminho
            // com filtro.
            abertura_dbfs: escolhida
                .abertura_dbfs
                .unwrap_or(if escolhida.supressao > 0.0 {
                    ABERTURA_COM_SUPRESSAO_DBFS
                } else {
                    ABERTURA_SEM_SUPRESSAO_DBFS
                }),
            abertura_escolhida: escolhida.abertura_dbfs.is_some(),
            abertura_minima_dbfs: ABERTURA_MINIMA_DBFS,
            abertura_maxima_dbfs: ABERTURA_MAXIMA_DBFS,
        })
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
    ///
    /// # E «barato» passou a ser verdade de verdade
    ///
    /// Tirar as mensagens do JSON não tirou a cópia: esta função fazia
    /// `room.clone()`, `Room` carrega a conversa e deriva `Clone`, então o custo
    /// continuava crescendo com o histórico a cada redesenho — quatro vezes por
    /// segundo. Era o R12 da revisão da v15.
    ///
    /// Agora os campos são lidos **sob o cadeado**, e o cadeado é soltado antes
    /// de qualquer outra coisa. O que a documentação acima promete continua de
    /// pé: o que sai daqui é uma cópia dos campos que a casca desenha, e nenhuma
    /// casca fica com um empréstimo do estado vivo.
    ///
    /// A ordem também é a que evita um travamento: nada que tome outro cadeado
    /// — `audio_state` toma o da voz — é chamado enquanto este está na mão.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        struct Foto {
            instancia_me: Option<seele_core::PersonId>,
            nickname: Option<String>,
            server: String,
            voice_rooms: Vec<VoiceRoom>,
            presentes: Vec<Person>,
            channels: Vec<Channel>,
            loss_fraction: f32,
            notice: Option<Notice>,
            permissions: Vec<seele_core::Permission>,
            tela: Option<TelaEmCurso>,
            minha_transmissao: Option<TelaEmCurso>,
            transmissoes: Vec<TransmissaoNaSala>,
            ended: Option<EndReason>,
        }

        let foto = match self.shared.room.lock() {
            Ok(room) => Foto {
                instancia_me: room.me,
                nickname: room
                    .me
                    .and_then(|me| room.people.get(&me))
                    .map(|person| person.nickname.clone()),
                server: room.server.clone(),
                voice_rooms: voice_rooms_of(&room),
                presentes: presentes_de(&room),
                channels: lines_of(&room),
                loss_fraction: room.telemetry.as_ref().map_or(0.0, |t| t.loss_fraction),
                notice: room.notice.as_ref().map(|notice| Notice {
                    severity: notice.severity.into(),
                    reason: notice.reason.into(),
                    operator_text: notice.operator_text.clone(),
                }),
                permissions: room.permissions.clone(),
                tela: self
                    .shared
                    .tela_escolhida()
                    .tela()
                    .and_then(|tela| tela_por_id(&room, tela, self.shared.pedido_da_tela())),
                minha_transmissao: minha_tela_de(&room, self.shared.pedido_da_tela()),
                transmissoes: transmissoes_de(&room, self.shared.tela_escolhida().tela()),
                ended: room.ended.map(|end| end.reason.into()),
            },
            // Um cadeado envenenado é uma sessão que já não tem estado. A foto
            // vazia é a resposta honesta, e é a mesma que o `Room::new()` de
            // antes dava.
            Err(_) => {
                let room = Room::new();
                Foto {
                    instancia_me: None,
                    nickname: None,
                    server: String::new(),
                    voice_rooms: voice_rooms_of(&room),
                    presentes: presentes_de(&room),
                    channels: lines_of(&room),
                    loss_fraction: 0.0,
                    notice: None,
                    permissions: Vec::new(),
                    tela: None,
                    minha_transmissao: None,
                    transmissoes: Vec::new(),
                    ended: None,
                }
            }
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
        let nickname = foto.nickname.clone().unwrap_or_else(|| {
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
            server: foto.server.clone(),
            icon_revision: self.shared.icon_revision.load(Ordering::Relaxed),
            person_icons_revision: self.shared.person_icons_revision.load(Ordering::Relaxed),
            me: foto.instancia_me.map(|person| person.0),
            nickname,
            voice_rooms: foto.voice_rooms,
            presentes: foto.presentes,
            channels: foto.channels,
            messages_revision: self.shared.messages_revision.load(Ordering::Relaxed),
            telemetry: Telemetry {
                rtt_ms,
                // O jitter que a pessoa quer saber é o de chegada, medido
                // aqui — e não o do relatório do servidor, que é sempre `0.0`
                // porque o servidor não tem como medir jitter. Ver
                // [`Shared::jitter_de_chegada_micros`].
                jitter_ms: self.shared.jitter_de_chegada_ms(),
                loss_fraction: foto.loss_fraction,
                bitrate_bps: audio.bitrate_bps,
                signal,
                sync_band: SignalBand::of(signal).into(),
                input_level: audio.input_level,
                local_fault: audio.local_fault,
                frames_refused: audio.frames_refused,
                microfone_entrega_silencio_digital: audio.silencio_digital,
            },
            notice: foto.notice,
            muted: audio.muted,
            total_isolation: audio.total_isolation,
            speaking: audio.speaking,
            voice_mode: audio.mode,
            audio_available: audio.available,
            abertura_do_audio: audio.abertura,
            capture: audio.capture,
            playback: audio.playback,
            aparelho: audio.aparelho,
            trocas_de_aparelho: audio.trocas_de_aparelho,
            may_manage_voice_rooms: foto
                .permissions
                .contains(&seele_core::Permission::ManageVoiceRooms),
            may_kick: foto.permissions.contains(&seele_core::Permission::Kick),
            may_ban: foto.permissions.contains(&seele_core::Permission::Ban),
            may_remove_message: foto
                .permissions
                .contains(&seele_core::Permission::RemoveMessage),
            may_move_person: foto
                .permissions
                .contains(&seele_core::Permission::MovePerson),
            may_customise_server: foto
                .permissions
                .contains(&seele_core::Permission::AdministerServer),
            may_delete_rooms: foto
                .permissions
                .contains(&seele_core::Permission::AdministerServer),
            tela: foto.tela,
            minha_transmissao: foto.minha_transmissao,
            transmissoes: foto.transmissoes,
            ended: foto.ended,
        }
    }

    /// Ends the session.
    /// Devolve à sonda o caminho medido da última visita a este servidor.
    ///
    /// Zero não faz nada: é o que a lista de conhecidos guarda para um servidor
    /// onde ninguém compartilhou tela ainda.
    pub fn lembrar_o_caminho(&self, bps: u32) {
        if bps != 0 {
            // Perder isto não é perder nada: uma sessão que já caiu não tem
            // sonda para semear, e a próxima vai ler o mesmo número do disco.
            let _ = self.command(Command::LembrarCaminho(bps));
        }
    }

    /// O caminho de subida que a sonda mediu nesta sessão, em bits por segundo.
    ///
    /// Zero enquanto ninguém compartilhou tela. Quem guarda isto deve tratar
    /// zero como «não mediu» e **deixar o valor antigo** — sobrescrever uma
    /// medida boa com um zero é jogar fora exatamente o que ela serve para
    /// evitar.
    #[must_use]
    pub fn caminho_medido(&self) -> u32 {
        self.shared
            .caminho_medido
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Ends the session.
    pub fn disconnect(&self) {
        self.shared.running.store(false, Ordering::Relaxed);
        let _ = self.commands.send(Command::Shutdown);
    }

    /// Encerra a sessão e espera a thread da conexão terminar, até `teto`.
    ///
    /// Para quem vai embora em seguida: a casca no `ExitRequested`, com o
    /// processo morrendo depois. [`Self::disconnect`] só pede o fim, e o
    /// `CONNECTION_CLOSE` sai pela despedida do enlace, na thread da conexão;
    /// um processo que morre antes dela leva o aviso junto, e quem ficou só vê
    /// a saída pelo prazo de ociosidade do servidor, 20 s. Era a espera cega de
    /// 150 ms da casca, que não sabia se a despedida tinha acontecido.
    ///
    /// Devolve `true` quando a thread terminou: deu o sinal, ou acabou sem
    /// chegar a ele — um runtime que não subiu, um pânico — e não há mais o que
    /// esperar. Também quando outra chamada já viu o fim. `false` só quando o
    /// teto venceu com a thread ainda correndo, e aí quem chama sabe que a
    /// despedida pode não ter saído.
    ///
    /// **Bloqueia** quem chama. Sozinha, até `teto`. Duas chamadas ao mesmo tempo
    /// não dividem a espera: a segunda espera a primeira soltar o cadeado do
    /// recebedor, e depois o próprio teto — até o dobro, quando a thread não
    /// termina.
    #[must_use]
    pub fn encerrar_e_esperar(&self, teto: Duration) -> bool {
        self.disconnect();
        let mut encerrada = self
            .encerrada
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(fim) = encerrada.as_ref() else {
            return true;
        };
        match fim.recv_timeout(teto) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                *encerrada = None;
                true
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
        }
    }

    fn command(&self, command: Command) -> Result<(), ConnectionError> {
        self.commands
            .send(command)
            .map_err(|_| ConnectionError::NotConnected)
    }

    fn audio_state(&self) -> AudioState {
        // Lidos antes do cadeado da voz, e não dentro dele: a ordem dos dois
        // cadeados é a voz primeiro, e é a que `VozDaSala` segue.
        let escolhida = self.shared.voice.escolhida();
        let abertura = self.shared.voice.abertura();
        let Ok(voice) = self.shared.voice.lock() else {
            return AudioState::sem_voz(&escolhida, abertura);
        };
        let Some(voice) = voice.as_ref() else {
            return AudioState::sem_voz(&escolhida, abertura);
        };
        let telemetry = voice.telemetry();
        // Uma leitura só do painel: nome novo com estado velho seria a
        // interface dizendo que já trocou quando ainda está trocando.
        let aparelhos = voice.estado_do_audio();
        AudioState {
            available: true,
            // Com a voz de pé, aberta por definição: o estado guardado pode
            // estar um passo atrás, entre a voz entrar no lugar e ele mudar.
            abertura: AberturaDoAudio::Aberto,
            mode: voice.mode().into(),
            speaking: telemetry.local.speaking,
            muted: voice.muted(),
            total_isolation: voice.total_isolation(),
            input_level: telemetry.local.input_level,
            local_fault: voice.falha_local(),
            frames_refused: voice.quadros_recusados(),
            silencio_digital: telemetry.local.microfone_entrega_silencio_digital,
            bitrate_bps: telemetry.local.bitrate_bps,
            capture: aparelhos.capture.map(|device| CaptureDevice {
                id: device.id,
                name: device.name,
                default: device.default,
            }),
            playback: aparelhos.playback.map(|device| PlaybackDevice {
                id: device.id,
                name: device.name,
                default: device.default,
            }),
            aparelho: aparelhos.estado.into(),
            trocas_de_aparelho: aparelhos.reaberturas,
        }
    }
}

/// O veredito da conferência da troca, sem sessão nenhuma por perto.
///
/// Uma função sobre dois `Option<&str>` e não um método porque é assim que ela
/// vira **teste de comportamento**. O guarda que existia lia o texto-fonte deste
/// arquivo com `include_str!` e conferia se certas palavras apareciam no corpo
/// do método — o que prova que alguém escreveu as palavras, e nada sobre o que
/// o produto faz. «Existir não é funcionar»: um `contains` verde sobreviveria a
/// uma comparação invertida.
///
/// # Errors
///
/// [`ConnectionError::CaptureDeviceGone`] ou
/// [`ConnectionError::PlaybackDeviceGone`] quando o aparelho aberto não é o que
/// foi pedido. Sem pedido — o padrão do sistema — qualquer um que abra é o
/// certo.
fn conferencia(
    pedido: Option<&str>,
    aberto: Option<&str>,
    lado: Lado,
) -> Result<(), ConnectionError> {
    let Some(pedido) = pedido else {
        return Ok(());
    };
    if aberto == Some(pedido) {
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

/// What the voice path is doing, read once per [`Connection::snapshot`].
///
/// A struct and not the eight-tuple this was: the tuple had already reached the
/// point of needing `clippy::type_complexity` waved through, and its run of
/// consecutive `bool`s was three chances to swap two fields with nothing
/// anywhere to catch it.
struct AudioState {
    available: bool,
    abertura: AberturaDoAudio,
    mode: VoiceMode,
    speaking: bool,
    muted: bool,
    total_isolation: bool,
    input_level: f32,
    local_fault: bool,
    frames_refused: u64,
    /// Ver [`Telemetry::microfone_entrega_silencio_digital`].
    silencio_digital: bool,
    bitrate_bps: u32,
    capture: Option<CaptureDevice>,
    playback: Option<PlaybackDevice>,
    aparelho: EstadoDoAparelho,
    trocas_de_aparelho: u64,
}

impl AudioState {
    /// A session with no voice path — before audio opens, and after it stops.
    ///
    /// **O mudo, o isolamento e o modo são os escolhidos**, e não os de uma voz
    /// recém-aberta: pela pendência 52, fora de sala não há voz, e o botão de
    /// mudo que a pessoa apertou ali tem de continuar apertado na tela. O padrão
    /// de [`ControlesGuardados`] continua sendo push-to-talk — `specs/03-audio.md`
    /// escolhe o modo que nunca dispara sozinho, e um padrão `Open` desenharia
    /// um microfone aberto numa sessão que não tem nenhum.
    fn sem_voz(escolhida: &ControlesGuardados, abertura: AberturaDoAudio) -> Self {
        Self {
            available: false,
            abertura,
            mode: escolhida.modo.into(),
            speaking: false,
            muted: escolhida.mudo,
            total_isolation: escolhida.isolamento,
            input_level: 0.0,
            local_fault: false,
            frames_refused: 0,
            // Sem voz não há microfone aberto, e nada a dizer sobre ele.
            silencio_digital: false,
            bitrate_bps: 0,
            capture: None,
            playback: None,
            // Sem voz não há aparelho a acompanhar. `Perdido` seria uma falha
            // que não houve: ninguém pediu áudio nesta sessão.
            aparelho: EstadoDoAparelho::Funcionando,
            trocas_de_aparelho: 0,
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

/// A conversa do **canal aberto**, confirmadas e pendentes, na ordem de leitura.
///
/// # A projeção explícita que o R02 pede
///
/// Antes disto a FFI devolvia a lista inteira do core, e a lista inteira do core
/// era uma só para todos os canais: quem desenhava recebia mensagens de dois
/// canais sem nada que as separasse, e as desenhava todas sob o título de um. A
/// fronteira é aqui — a casca pede a conversa que está aberta, e é isso que ela
/// recebe.
///
/// As pendentes vão **depois** das confirmadas. Elas são sempre as mais novas: o
/// servidor ordena por id, e uma mensagem sem id ainda não tem lugar nessa
/// ordem. Ver [`seele_core::Pendente`].
fn messages_of(room: &Room) -> Vec<Message> {
    let Some(aberto) = room.current_channel else {
        return Vec::new();
    };
    let mut lista: Vec<Message> = room
        .mensagens_do_canal(aberto)
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
            estado: EstadoDoEnvio::Confirmada,
            client_message_id: None,
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
        .collect();

    // As pendentes desta máquina, nas mesmas linhas da conversa. Nas mesmas
    // linhas de propósito: uma área separada para «o que ainda não foi» tiraria
    // a mensagem do lugar onde ela vai ficar, e quem escreveu perderia de vista
    // onde ela entra.
    //
    // `id` zero e autor próprio: elas não têm id de servidor porque ainda não
    // existem para ele, e quem as identifica é a `client_message_id`. A casca
    // desenha por `estado`, nunca por `id`.
    let eu = room.me.map_or(0, |pessoa| pessoa.0);
    let meu_nome = room
        .me
        .and_then(|pessoa| room.people.get(&pessoa))
        .map_or_else(String::new, |pessoa| pessoa.nickname.clone());
    lista.extend(room.pendentes_do_canal(aberto).map(|pendente| Message {
        id: 0,
        channel: pendente.channel.0,
        author: eu,
        author_nickname: meu_nome.clone(),
        at_seconds: pendente.at_seconds,
        body: pendente.body.clone(),
        own: true,
        edited: false,
        estado: pendente.estado.into(),
        client_message_id: Some(pendente.client_message_id.get().to_string()),
        attachment: None,
    }));
    lista
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
        com_som: limites.com_som,
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
/// Tudo o que está sendo transmitido na sala em que esta pessoa está.
///
/// **Só a sala dela.** O servidor só anuncia uma transmissão a quem está lá
/// dentro, e listar outra aqui seria a casca contando algo que a sessão não viu.
///
/// Ordenada pelo nome da transmissão, que é crescente: sem ordem, um mapa
/// devolveria as linhas em ordem diferente a cada desenho, e a lista piscaria
/// de lugar duas vezes por segundo.
fn transmissoes_de(room: &Room, assistida: Option<seele_core::ScreenId>) -> Vec<TransmissaoNaSala> {
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
/// que encontrasse no mapa da sala — ver o doc de [`Snapshot::tela`] para o que
/// isso custava. ADR 0054.
fn tela_por_id(
    room: &Room,
    tela: seele_core::ScreenId,
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
fn minha_tela_de(room: &Room, pedido: Option<LimitesDeTela>) -> Option<TelaEmCurso> {
    let voice_room = room.current_voice_room?;
    let eu = room.me?;
    let minha = room
        .telas
        .values()
        .find(|tela| tela.person == eu && tela.voice_room == voice_room)?;
    tela_por_id(room, minha.screen, pedido)
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

/// O mesmo espelho, para a recusa de volume de um MOD — ADR 0048.
///
/// Exaustivo de propósito, como o de cima: uma variante nova no fio quebra a
/// compilação aqui em vez de virar um `_ => Outra` que a tela desenha errado.
fn recusa_de_volume(motivo: seele_core::VolumeRefusal) -> VolumeRefusal {
    use seele_core::VolumeRefusal as Fio;
    match motivo {
        Fio::SemEspera => VolumeRefusal::SemEspera,
        Fio::TipoRecusado => VolumeRefusal::TipoRecusado,
        Fio::NaoGravei => VolumeRefusal::NaoGravei,
        Fio::Incompleto => VolumeRefusal::Incompleto,
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
        // O nome e a pasta já passaram por `destino_de` antes de o pedido sair,
        // então o que sobra para o motor recusar é a gravação.
        Transferencia::NaoSalvou { anexo } => Transfer::NotSaved {
            attachment: anexo.get(),
            reason: NotSavedReason::Falhou,
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
    // `[::1]:8383` and `[::1]` file under one entry instead of two.
    //
    // **Uma função só, e por quê.** Esta chave não arquiva mais só o pin: desde
    // o anúncio de MODs ela arquiva também o aceite, e quem grava o aceite é a
    // casca, por um verbo que não tem socket nenhum. Duas cópias da regra
    // seriam duas chaves para o mesmo servidor no dia em que uma delas mudasse
    // — e o efeito de errar a chave é mudo: o aceite fica escrito onde ninguém
    // lê, e a pessoa que já disse sim é perguntada de novo para sempre.
    let pin_key = chave_do_servidor(target).ok_or(ConnectionError::UnresolvableHost)?;
    debug_assert_eq!(
        pin_key.contains('['),
        address.is_ipv6() && alvo.maquina.contains(':'),
        "a chave canônica discordou da resolução sobre ser um IPv6 literal"
    );
    Ok((address, server_name, pin_key))
}

/// A chave sob a qual tudo que é **deste servidor** é arquivado nesta máquina.
///
/// O pin do certificado (ADR 0022) e o aceite dos MODs (ADR 0045) moram os dois
/// sob ela, e é de propósito: são duas memórias sobre o mesmo servidor, e uma
/// pessoa que apaga um servidor da lista espera apagar as duas.
///
/// É `host:porta` na forma canônica — a porta preenchida quando quem digitou a
/// omitiu, e um IPv6 literal de volta entre colchetes, sem os quais a chave
/// seria ambígua com `host:porta`. **Não resolve nome nenhum:** é uma decisão
/// sobre o texto, e por isso vale igual dentro de um comando da janela, onde
/// não há socket.
///
/// Devolve `None` quando o alvo não é um endereço — o mesmo caso em que
/// `resolve` devolve [`ConnectionError::UnresolvableHost`].
#[must_use]
pub fn chave_do_servidor(alvo: &str) -> Option<String> {
    let alvo = seele_core::uri::separar(alvo).ok()?;
    // `maquina` só contém `:` quando veio de dentro de colchetes: `separar`
    // recusa um IPv6 escrito cru e corta a porta em qualquer outro caso. Então
    // este teste diz «é um IPv6 literal» sem precisar resolver o nome.
    if alvo.maquina.contains(':') {
        Some(format!("[{}]:{}", alvo.maquina, alvo.porta))
    } else {
        Some(format!("{}:{}", alvo.maquina, alvo.porta))
    }
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
    chave_do_aceite: &str,
) -> seele_core::enlace::Destino {
    seele_core::enlace::Destino {
        servidor: address,
        nome_tls: server_name.to_owned(),
        chave_do_pin: pin_key.to_owned(),
        apelido: config.nickname.clone(),
        segredo: config.join_secret.clone(),
        esperada: config
            .expected_fingerprint
            .clone()
            .map(seele_core::enlace::Esperada::Certificado),
        // **O pino deste destino prova o servidor só no alvo, e só num alvo de
        // escopo público.** A regra mora aqui, e em nenhum outro lugar: ver
        // `seele_core::enlace::Destino::o_pino_prova_o_servidor`.
        //
        // O alvo é o candidato cuja chave de pino é a do destino. A chave do
        // aceite é a do endereço que a pessoa escolheu (o do link, o digitado,
        // o da entrada da lista), e `drive` a passa igual a todos os
        // candidatos; a chave de pino é a de cada um. Só o primeiro destino de
        // `drive` tem as duas iguais, e um alternativo que as tenha é o mesmo
        // `host:porta` escrito de novo.
        //
        // O escopo é o do endereço **resolvido**, e não o do texto: um nome como
        // `casa.local` decide pelo endereço a que chegou. Um endereço de escopo
        // local (privado, link-local, CGNAT, ULA) é o mesmo de uma casa para
        // outra, e o pin dele prova só quem atende ali na rede em que a pessoa
        // está. O loopback conta como público: é esta máquina em qualquer rede.
        o_pino_prova_o_servidor: pin_key == chave_do_aceite
            && !seele_core::enlace::e_privado(address.ip()),
        // Lido de disco aqui, e não pedido à casca: o `home` é o mesmo caminho
        // de onde saem a identidade e os pins (ADR 0017), e quem sabe persistir
        // é o núcleo. Uma casca que tivesse de carregar o aceite junto seria uma
        // casca que pode esquecer de carregá-lo — e esquecer, aqui, é perguntar
        // de novo a quem já respondeu.
        // **A chave do aceite é a do destino, e não a do candidato.**
        //
        // O pin é por endereço porque é isso que ele prova: *este* endereço
        // apresentou *esta* chave. O aceite responde outra pergunta — «eu
        // autorizei rodar estes MODs neste servidor» —, e o servidor é um só,
        // por quantos caminhos se chegue a ele.
        //
        // Aqui as duas usavam a mesma chave, e o resultado era um laço que
        // ninguém conseguia romper: a pessoa aceitava, o sim era gravado sob o
        // endereço que ela digitou, a conexão entrava pelo candidato de
        // retransmissão — cuja porta o NAT troca a cada sessão —, e o aceite
        // procurado sob *essa* porta nunca existia. Aceitar de novo gravava sob
        // o mesmo endereço digitado e falhava de novo, para sempre.
        //
        // Visto em produção, com os dados na mão: `aceites` tinha
        // `192.168.50.30:8383 → acff8e16…`, exatamente o conjunto exigido, e a
        // conexão que respondia era `187.255.97.152:9368`. O arquivo de pins
        // tinha nove portas diferentes do mesmo endereço público, uma por
        // sessão — o rastro do mesmo problema.
        //
        // Todos os candidatos de uma chegada são o mesmo servidor: eles vêm do
        // mesmo convite e são conferidos contra a mesma impressão digital. O
        // consentimento é daquele servidor, e acompanha a chegada inteira.
        aceito: seele_core::aceites::Aceites::em(std::path::Path::new(&config.home))
            .aceito_de(chave_do_aceite),
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
    // O alvo como a pessoa o digitou. É sob ele que a casca grava o sim, e é
    // ele que vale para todos os candidatos desta chegada.
    let chave_do_aceite = pin_key.clone();
    let destinos = std::iter::once((address, server_name.clone(), pin_key.clone()))
        .chain(alternates)
        .map(|(address, server_name, pin_key)| {
            build_destino(&config, address, &server_name, &pin_key, &chave_do_aceite)
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

    // **O áudio não abre aqui.** Pendência 52: ele abre quando a pessoa passa a
    // estar numa sala de voz, e fecha quando ela sai — quem fica no servidor só
    // pela conversa por escrito devolve o repouso à máquina. Este primeiro olhar
    // é o do aperto de mão: o servidor adotado, ninguém sentado, nada a abrir.
    let mut porta = PortaDaVoz::nova();
    porta.olhar(&shared.voice, sala_de(&shared), |pedidos| {
        abrir_a_voz(&shared, pedidos)
    });

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
                    seele_core::enlace::Aviso::TelaIlegivel { motivo } => {
                        shared.notify(&Event::ScreenUnreadable { reason: motivo });
                    }
                    seele_core::enlace::Aviso::TelaQuadro { tela, chave, bytes } => {
                        shared.notify(&Event::ScreenFrame {
                            screen: tela.0,
                            key: chave,
                            data: base64_de(&bytes),
                        });
                    }
                    // **O som não vira evento.** Ele vai direto para a mistura,
                    // que é onde o codec já mora e onde o isolamento total
                    // decide se alguma coisa toca. Mandá-lo à casca seria pedir
                    // a ela que decodificasse Opus para depois devolvê-lo.
                    seele_core::enlace::Aviso::TelaSom { tela, bytes } => {
                        // **O identificador segue junto até a mistura.** Ele era
                        // descartado aqui — `{ bytes, .. }` —, e por isso a fila
                        // não sabia de quem ela era: qualquer `TelaFechou`
                        // limpava a fila, então alguém parando de transmitir
                        // calava o som de outra transmissão. R17 e ADR 0054.
                        if let Ok(voice) = shared.voice.lock() {
                            if let Some(voice) = voice.as_ref() {
                                voice.som_da_tela(tela, bytes);
                            }
                        }
                    }
                    seele_core::enlace::Aviso::TelaFechou { tela } => {
                        // A transmissão acabou: o que sobrou na fila é som de
                        // uma tela que já não está na frente de ninguém — **se a
                        // fila for daquela tela.** Ver
                        // `Voice::esquecer_o_som_da_tela`.
                        if let Ok(voice) = shared.voice.lock() {
                            if let Some(voice) = voice.as_ref() {
                                voice.esquecer_o_som_da_tela(tela);
                            }
                        }
                        shared.notify(&Event::ScreenClosed { screen: tela.0 });
                    }
                    // Cair não encerra: começa a bateria interna, e a casca
                    // esmaece em vez de fechar.
                    seele_core::enlace::Aviso::Estado { estado, restante } => {
                        shared.gravar_enlace(estado, restante);
                        shared.notify(&Event::TelemetryChanged);
                        // **O que esta máquina escreveu e ainda esperava passa a
                        // «sem resposta».** O enlace caiu no meio, então o
                        // servidor pode ter gravado e a confirmação ter se
                        // perdido — é por isso que o estado não é «falhou».
                        //
                        // Sem esta linha, uma mensagem escrita no instante da
                        // queda ficaria «enviando» para sempre, o que é o mesmo
                        // defeito do R05 com outra cara: a interface parada num
                        // estado que nunca sai dele.
                        if matches!(estado, seele_core::Link::InternalBattery { .. }) {
                            let mexeu = shared
                                .room
                                .lock()
                                .map(|mut room| room.envios_sem_resposta())
                                .unwrap_or(false);
                            if mexeu {
                                shared.messages_changed();
                            }
                        }
                    }
                    seele_core::enlace::Aviso::Reconectado { media, sessao, voice_room } => {
                        shared.gravar_enlace(seele_core::Link::Online, None);
                        // A pessoa volta sentada onde o motor a sentou de novo:
                        // ver `readotar_na_volta`. É esse assento que a porta da
                        // voz olha no fim desta volta do laço.
                        if let Ok(mut room) = shared.room.lock() {
                            readotar_na_volta(&mut room, &sessao, &config.nickname, voice_room);
                        }
                        // Conexão nova, `ssrc` novo, canal de mídia novo: sem
                        // reabrir, a voz sairia por uma conexão morta.
                        remember_media(&shared, (*media).clone(), sessao.ssrc);
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
                        //
                        // **Na thread de `esperar_o_audio`, com o prazo, e sem
                        // o cadeado da voz preso** (pendência 52): a voz velha
                        // vai inteira para a reabertura, e a nova chega pelo
                        // braço da porta abaixo. Quem estava fora de sala não
                        // tem voz, e continua sem aparelho.
                        let (canal, ssrc) = ((*media).clone(), sessao.ssrc);
                        let vez = Arc::clone(&shared.voice.vez_do_driver);
                        porta.na_volta(
                            &shared.voice,
                            move |atual: Voice| -> Futuro<Voice> {
                                Box::pin(async move {
                                    esperar_o_audio(
                                        &vez,
                                        move || atual.reopen(canal, ssrc).map_err(|erro| format!("{erro:#}")),
                                        PRAZO_DO_AUDIO,
                                    )
                                    .await
                                })
                            },
                            |pedidos| abrir_a_voz(&shared, pedidos),
                        );
                        shared.notify(&Event::TelemetryChanged);
                    }
                    seele_core::enlace::Aviso::Encerrado(motivo) => {
                        shared.notify(&Event::Ended {
                            reason: fim_da_sessao(&motivo),
                        });
                        break;
                    }
                }
            }

            // A voz da sala chegou — ou o prazo venceu, ou o aparelho recusou.
            aberta = porta.chegada() => {
                porta.chegou(&shared.voice, aberta, |pedidos| abrir_a_voz(&shared, pedidos));
                shared.notify(&Event::TelemetryChanged);
            }

            () = tokio::time::sleep_until(next_tick.into()) => {
                next_tick = Instant::now() + TICK;
                if measure(&mut sync, &client, &shared) {
                    shared.notify(&Event::TelemetryChanged);
                }
            }
        }

        // **O gatilho da pendência 52**, depois de cada volta e não dentro de
        // cada braço: o assento muda por cinco caminhos — a entrada pedida, o
        // operador que move, a saída pedida, a recusa por `Alert` e a sala
        // apagada —, e todos terminam no assento do `Room`, pela porta única de
        // `seele_core::assento`. Olhar o antes e o depois aqui cobre os cinco, e
        // o sexto que vier. O assento olhado é o confirmado: ver `sala_de`.
        let antes = shared.voice.abertura();
        porta.olhar(&shared.voice, sala_de(&shared), |pedidos| {
            abrir_a_voz(&shared, pedidos)
        });
        if shared.voice.abertura() != antes {
            shared.notify(&Event::TelemetryChanged);
        }
    }

    shared.running.store(false, Ordering::Relaxed);
    shared
        .link_state
        .store(link_state_byte(LinkTrust::Offline), Ordering::Relaxed);
    // Dropping the voice stops the audio thread with the session. A abertura
    // em curso vai junto, e a voz que ela trouxer cai na thread que a abria:
    // nada dela segura a despedida abaixo nem o `drop` do runtime.
    drop(porta);
    shared.voice.marcar(AberturaDoAudio::ForaDeSala);
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

/// Quanto a abertura do áudio pode levar antes de a sala seguir sem voz.
///
/// Dez segundos cobrem o driver são: abrir o CoreAudio ou o WASAPI leva de
/// dezenas a poucas centenas de milissegundos. **Não** foram medidas aqui, e são
/// medidas do dono: a primeira abertura no macOS, quando o sistema pergunta à
/// pessoa se o SEELE pode usar o microfone e a abertura espera a resposta, e a
/// do WASAPI numa máquina Windows lenta.
const PRAZO_DO_AUDIO: Duration = Duration::from_secs(10);

/// A abertura do áudio que não entregou nada dentro do prazo.
///
/// Inclui a thread que acabou sem entregar — não subiu, ou entrou em pânico, e
/// aí o gancho de pânico do app já deixou a linha dele no log. Para quem está
/// na sala é o mesmo fato: a voz não veio, e a conversa por escrito segue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Atraso;

/// Abre o áudio numa thread própria e espera a voz, até `prazo`. Pendência 52.
///
/// # Por que uma thread própria
///
/// A entrada numa sala acontece dentro do laço de `drive`, que roda num runtime
/// de uma thread só. Abrir ali, de forma síncrona, seguraria a conexão inteira
/// pelo tempo que o driver quisesse — e o driver é o que menos se controla aqui.
///
/// **Thread com nome, e não `spawn_blocking`.** Uma tarefa bloqueante presa no
/// driver seguraria o `drop` do runtime, e com ele a saída do app: soltar o
/// runtime espera a piscina de bloqueio sem prazo. Uma thread solta não segura
/// nada. Ela continua viva até o driver a soltar, e é o único custo.
///
/// # A voz que chega tarde
///
/// Volta no `Err` do envio e cai **na própria thread**, e o `drop` fecha o
/// aparelho. Quem esperava já desistiu: o prazo venceu, ou a pessoa saiu da sala
/// antes de a voz chegar.
async fn esperar_o_audio<V: Send + 'static>(
    vez: &Arc<Mutex<()>>,
    abrir: impl FnOnce() -> V + Send + 'static,
    prazo: Duration,
) -> Result<V, Atraso> {
    let (entrega, chegada) = tokio::sync::oneshot::channel();
    let vez = Arc::clone(vez);
    let subiu = std::thread::Builder::new()
        .name("seele-abre-audio".into())
        .spawn(move || {
            // **Uma abertura por vez no driver**, e a espera pela vez conta no
            // prazo de quem pediu. Ver `VozDaSala::vez_do_driver`.
            let _vez = vez
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if entrega.is_closed() {
                // Quem pediu desistiu enquanto a outra abertura corria: o
                // prazo venceu, ou a pessoa saiu da sala. Não há para quem abrir.
                return;
            }
            let aberta = abrir();
            // `Err(aberta)` quando ninguém espera mais, e ela é largada aqui.
            drop(entrega.send(aberta));
        });
    if let Err(erro) = subiu {
        tracing::error!(%erro, "a thread que abre o áudio não subiu; a sala segue sem voz");
        return Err(Atraso);
    }
    match tokio::time::timeout(prazo, chegada).await {
        Ok(Ok(aberta)) => Ok(aberta),
        Ok(Err(_)) => {
            tracing::warn!(
                "a thread que abria o áudio acabou sem entregar nada; a sala segue sem voz"
            );
            Err(Atraso)
        }
        Err(_) => {
            tracing::warn!("o áudio não abriu em {prazo:?}; a sala segue sem voz");
            Err(Atraso)
        }
    }
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

/// O que a reconexão faz com a voz que estava viva.
///
/// Mora aqui fora, e não dentro do braço de `Aviso::Reconectado`, para que a
/// decisão tenha guarda de comportamento: ninguém constrói uma
/// [`seele_core::Voice`] sem placa de som, mas nada do que se decide aqui
/// depende de placa nenhuma. São três decisões, e as três já foram o defeito:
/// reabrir **a partir** da voz viva (é o que carrega os controles), não abrir
/// voz para quem estava em texto puro, e não guardar a voz que não reabriu.
///
/// **A voz sai do lugar e vai inteira para quem reabre**, e não emprestada:
/// pela pendência 52 a reabertura corre na thread de [`esperar_o_audio`], com
/// prazo, e o cadeado da voz não pode ficar preso enquanto o aparelho abre. A
/// voz velha é largada lá, depois de a nova abrir — que é a ordem que
/// `Voice::reopen` já seguia. A terceira decisão mora em
/// [`PortaDaVoz::chegou`]: uma reabertura que falha não devolve nada ao lugar.
///
/// `None` é «não havia voz»: texto puro, ou uma sala em que o aparelho já não
/// tinha aberto. Nada é reaberto.
fn reabrir_voz_na_reconexao<V, F>(slot: &mut Option<V>, reabrir: impl FnOnce(V) -> F) -> Option<F> {
    slot.take().map(reabrir)
}

/// O que a pessoa escolheu para a voz, guardado fora dela. Pendência 52.
///
/// Fora de sala não há [`Voice`] para segurar estes valores, e eles têm de
/// valer quando ela abrir: a voz nasce com os padrões e **veste** isto ao entrar
/// no lugar ([`VozDaSala::instalar`]). Com a voz de pé, cada escolha é gravada
/// aqui e aplicada nela, e as duas cópias andam juntas.
///
/// O volume de cada pessoa não está aqui: ele é por apelido, e é do lote I-b.
#[derive(Debug, Clone, PartialEq)]
struct ControlesGuardados {
    mudo: bool,
    isolamento: bool,
    modo: seele_core::VoiceMode,
    /// De zero a um, como `Voice::set_supressao` a recebe.
    supressao: f32,
    /// Já dentro da faixa. `None` é o padrão, que depende da supressão.
    abertura_dbfs: Option<f32>,
    /// Os aparelhos pedidos — o pedido, e não o que abriu.
    aparelhos: seele_core::DeviceChoice,
}

impl ControlesGuardados {
    /// Os de uma voz que acabou de abrir, com os aparelhos que a casca pediu.
    ///
    /// Push-to-talk, supressão inteira e a sensibilidade no padrão: os mesmos de
    /// `Controls::novos`, no núcleo. Uma voz que vestisse outra coisa por padrão
    /// abriria diferente da que se abria antes da pendência 52.
    fn de_partida(aparelhos: seele_core::DeviceChoice) -> Self {
        Self {
            mudo: false,
            isolamento: false,
            modo: seele_core::VoiceMode::PushToTalk,
            supressao: 1.0,
            abertura_dbfs: None,
            aparelhos,
        }
    }
}

/// O que a pendência 52 pede de uma voz viva: vestir o que a pessoa escolheu.
///
/// Um traço com uma implementação de verdade e uma de teste. Abrir uma
/// [`Voice`] pede placa de som, e o que se decide sobre ela — quando abrir,
/// quando fechar, o que ela veste — não depende de placa nenhuma.
trait VozViva {
    /// Aplica tudo o que a pessoa escolheu.
    fn vestir(&self, escolhida: &ControlesGuardados);
}

impl VozViva for Voice {
    fn vestir(&self, escolhida: &ControlesGuardados) {
        self.set_muted(escolhida.mudo);
        self.set_total_isolation(escolhida.isolamento);
        self.set_mode(escolhida.modo);
        self.set_supressao(escolhida.supressao);
        self.set_abertura_dbfs(escolhida.abertura_dbfs);
    }
}

/// A voz desta sessão, o que a pessoa escolheu para ela, e em que pé a abertura
/// está. Pendência 52.
///
/// # A ordem dos cadeados
///
/// A voz primeiro, a escolha depois, sempre. [`Self::escolher`] grava a escolha
/// **antes** de olhar a voz, e [`Self::instalar`] lê a escolha **com** a voz
/// presa: uma escolha feita enquanto a voz abre é vista por um dos dois, em
/// qualquer ordem que as threads andem.
struct VozDaSala<V> {
    viva: Mutex<Option<V>>,
    escolhida: Mutex<ControlesGuardados>,
    abertura: Mutex<AberturaDoAudio>,
    /// Se esta sessão pediu áudio ([`ConnectConfig::audio`]). Sem ele, nenhuma
    /// sala abre aparelho.
    quer_audio: bool,
    /// A vez do driver: no máximo uma abertura de aparelho em voo por sessão.
    ///
    /// M-c da rodada 1 do lote I-a. Entrar e sair de uma sala depressa larga a
    /// abertura em curso, mas a thread dela segue no driver até ele soltar; a
    /// entrada seguinte poria uma segunda abertura no CoreAudio ao mesmo tempo,
    /// e duas simultâneas nunca foram medidas. A thread de
    /// [`esperar_o_audio`] toma esta vez antes de abrir, e a de uma abertura
    /// de que ninguém espera mais desiste sem abrir nada quando a vez chega.
    vez_do_driver: Arc<Mutex<()>>,
}

impl<V> VozDaSala<V> {
    fn nova(quer_audio: bool, aparelhos: seele_core::DeviceChoice) -> Self {
        Self {
            viva: Mutex::new(None),
            escolhida: Mutex::new(ControlesGuardados::de_partida(aparelhos)),
            abertura: Mutex::new(AberturaDoAudio::ForaDeSala),
            quer_audio,
            vez_do_driver: Arc::new(Mutex::new(())),
        }
    }

    /// O cadeado da voz viva.
    ///
    /// O mesmo nome do `Mutex` que este campo era antes da pendência 52, para
    /// que quem só lê a voz — a telemetria, o som da tela, a malha de bitrate —
    /// continue lendo do mesmo jeito.
    fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, Option<V>>> {
        self.viva.lock()
    }

    /// O que a pessoa escolheu, agora.
    fn escolhida(&self) -> ControlesGuardados {
        self.escolhida
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Em que pé a abertura está, guardado.
    fn abertura(&self) -> AberturaDoAudio {
        *self
            .abertura
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn marcar(&self, estado: AberturaDoAudio) {
        *self
            .abertura
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = estado;
    }

    /// Grava o aparelho escolhido de um lado, sem mexer no outro.
    fn guardar_aparelho(&self, lado: Lado, pedido: Option<String>) {
        let mut escolhida = self
            .escolhida
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match lado {
            Lado::Entrada => escolhida.aparelhos.capture = pedido,
            Lado::Saida => escolhida.aparelhos.playback = pedido,
        }
    }
}

impl<V: VozViva> VozDaSala<V> {
    /// Grava uma escolha e a aplica na voz viva, se há uma.
    fn escolher(&self, mudar: impl FnOnce(&mut ControlesGuardados)) {
        mudar(
            &mut self
                .escolhida
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        if let Ok(viva) = self.viva.lock() {
            if let Some(viva) = viva.as_ref() {
                viva.vestir(&self.escolhida());
            }
        }
    }

    /// Põe no lugar uma voz recém-aberta, vestida com o que a pessoa escolheu
    /// enquanto ela abria.
    ///
    /// **É aqui que o mudo pedido fora de sala vale.** Sem o `vestir`, a voz
    /// abriria com os padrões e o microfone de quem se mutou antes de entrar
    /// estaria aberto — com o roster dizendo «mudo», porque o servidor recebeu o
    /// pedido.
    ///
    /// **E é aqui que se confere o aparelho, sob o cadeado da voz** (M-b da
    /// rodada 1 do lote I-a). `pedidos` são os aparelhos com que a voz abriu; se
    /// a pessoa escolheu outro enquanto ela abria, a voz volta no `Err` e quem
    /// chamou recomeça. Conferido fora do cadeado, como era em
    /// [`PortaDaVoz::chegou`], uma troca entre a conferência e este cadeado
    /// instalava uma voz aberta no aparelho de antes: `switch_device` grava a
    /// escolha segurando este mesmo cadeado, e só sob ele as duas coisas não se
    /// cruzam.
    fn instalar(&self, nova: V, pedidos: &seele_core::DeviceChoice) -> Result<(), V> {
        match self.viva.lock() {
            Ok(mut viva) => {
                let escolhida = self.escolhida();
                if escolhida.aparelhos != *pedidos {
                    return Err(nova);
                }
                nova.vestir(&escolhida);
                *viva = Some(nova);
                self.marcar(AberturaDoAudio::Aberto);
            }
            Err(_) => self.marcar(AberturaDoAudio::SemAparelho),
        }
        Ok(())
    }

    /// Larga a voz viva, se há uma. O `drop` fecha o aparelho.
    fn largar(&self) {
        if let Ok(mut viva) = self.viva.lock() {
            *viva = None;
        }
    }

    fn tem_voz(&self) -> bool {
        self.viva.lock().is_ok_and(|viva| viva.is_some())
    }
}

/// O que uma abertura entrega: a voz, ou por que ela não veio.
type Aberta<V> = Result<Result<V, String>, Atraso>;

/// Uma abertura que ainda vai entregar.
type Futuro<V> = std::pin::Pin<Box<dyn std::future::Future<Output = Aberta<V>>>>;

/// Uma abertura em curso, com os aparelhos que ela pediu.
///
/// Largá-la desiste dela: a voz que ela trouxer cai na thread que a abriu, e o
/// `drop` fecha o aparelho. Ver [`esperar_o_audio`].
struct EmCurso<V> {
    futuro: Futuro<V>,
    pedidos: seele_core::DeviceChoice,
}

/// O que o assento pede da voz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PassoDaVoz {
    Abrir,
    Fechar,
    /// Numa sala, numa sessão que entrou sem áudio.
    SemAudio,
    Nada,
}

/// A decisão da pendência 52, sozinha: o áudio abre quando a pessoa **passa a
/// estar** numa sala, e fecha quando deixa de estar.
///
/// Lida de antes e depois do assento, e não de qual mensagem chegou, porque os
/// caminhos para dentro e para fora de uma sala são cinco — a entrada pedida, o
/// `MovedToVoiceRoom` de um operador, a saída pedida, a recusa por `Alert` e a
/// sala apagada — e todos já terminam no mesmo `Room::current_voice_room`. Um
/// gatilho que casasse as mensagens seria um sexto lugar para esquecer uma.
///
/// Mudar de uma sala para outra com a voz de pé não reabre nada: a voz não é
/// de uma sala, e o servidor é quem decide para onde ela vai. Sem voz — o
/// aparelho não abriu na sala de antes —, a sala nova tenta de novo.
fn passo_da_voz(
    antes: Option<VoiceRoomId>,
    agora: Option<VoiceRoomId>,
    quer_audio: bool,
    ha_voz: bool,
) -> PassoDaVoz {
    match (antes, agora) {
        (Some(_), None) => PassoDaVoz::Fechar,
        (_, Some(sala)) if antes != Some(sala) && !ha_voz => {
            if quer_audio {
                PassoDaVoz::Abrir
            } else {
                PassoDaVoz::SemAudio
            }
        }
        _ => PassoDaVoz::Nada,
    }
}

/// Quando o áudio abre e fecha, do lado de `drive`. Pendência 52.
///
/// Mora no laço, e não em [`Shared`], porque a abertura em curso é um futuro que
/// só o laço espera. O que a casca lê — a voz, a escolha, o estado — mora em
/// [`VozDaSala`].
struct PortaDaVoz<V> {
    /// O assento da última vez que se olhou.
    sala_vista: Option<VoiceRoomId>,
    em_curso: Option<EmCurso<V>>,
}

impl<V: VozViva> PortaDaVoz<V> {
    const fn nova() -> Self {
        Self {
            sala_vista: None,
            em_curso: None,
        }
    }

    /// Olha o assento de agora e abre, fecha, ou não faz nada.
    ///
    /// `drive` chama isto logo depois do aperto de mão — com o servidor adotado
    /// e ninguém sentado, e é por isso que nada abre ao conectar — e depois de
    /// cada volta do laço.
    fn olhar(
        &mut self,
        voz: &VozDaSala<V>,
        agora: Option<VoiceRoomId>,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        let antes = std::mem::replace(&mut self.sala_vista, agora);
        let ha_voz = self.em_curso.is_some() || voz.tem_voz();
        match passo_da_voz(antes, agora, voz.quer_audio, ha_voz) {
            PassoDaVoz::Abrir => self.comecar(voz, abrir),
            PassoDaVoz::Fechar => {
                // A abertura em curso é largada, e a voz que ela trouxer cai na
                // thread que a abriu. O estado muda antes da voz sair do lugar:
                // quem ler entre os dois vê a voz ainda de pé, e não um «fora
                // de sala» com aparelho aberto.
                self.em_curso = None;
                voz.marcar(AberturaDoAudio::ForaDeSala);
                voz.largar();
            }
            PassoDaVoz::SemAudio => voz.marcar(AberturaDoAudio::SemAparelho),
            PassoDaVoz::Nada => {}
        }
    }

    fn comecar(
        &mut self,
        voz: &VozDaSala<V>,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        let pedidos = voz.escolhida().aparelhos;
        voz.marcar(AberturaDoAudio::Abrindo);
        self.em_curso = Some(EmCurso {
            futuro: abrir(&pedidos),
            pedidos,
        });
    }

    /// A volta do enlace: a voz viva reabre na conexão nova.
    ///
    /// Quem estava fora de sala não tem voz e continua sem aparelho. Uma
    /// abertura que estava em curso abria sobre a conexão que caiu, e recomeça
    /// na nova.
    fn na_volta(
        &mut self,
        voz: &VozDaSala<V>,
        reabrir: impl FnOnce(V) -> Futuro<V>,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        if self.em_curso.take().is_some() {
            self.comecar(voz, abrir);
            return;
        }
        if !voz.tem_voz() {
            return;
        }
        voz.marcar(AberturaDoAudio::Abrindo);
        let pedidos = voz.escolhida().aparelhos;
        let reaberta = voz
            .lock()
            .ok()
            .and_then(|mut viva| reabrir_voz_na_reconexao(&mut viva, reabrir));
        match reaberta {
            Some(futuro) => self.em_curso = Some(EmCurso { futuro, pedidos }),
            None => voz.marcar(AberturaDoAudio::SemAparelho),
        }
    }

    /// Espera a abertura em curso. Sem abertura, nunca volta.
    ///
    /// É o braço do `select!` de `drive`: um futuro que nunca volta é um braço
    /// que nunca é escolhido, e o laço não precisa de um `if` para ele.
    async fn chegada(&mut self) -> Aberta<V> {
        match self.em_curso.as_mut() {
            Some(em_curso) => em_curso.futuro.as_mut().await,
            None => std::future::pending().await,
        }
    }

    /// O que fazer com o que a abertura entregou.
    fn chegou(
        &mut self,
        voz: &VozDaSala<V>,
        aberta: Aberta<V>,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        let Some(em_curso) = self.em_curso.take() else {
            return;
        };
        match aberta {
            Ok(Ok(nova)) => {
                if let Err(nova) = voz.instalar(nova, &em_curso.pedidos) {
                    // A pessoa escolheu outro aparelho enquanto este abria. O
                    // que abriu não é mais o pedido, e guardá-lo seria a tela
                    // dizendo ESCOLHIDO num e EM USO noutro.
                    drop(nova);
                    self.comecar(voz, abrir);
                }
            }
            Ok(Err(erro)) => {
                // A metade de texto funciona, e `abertura_do_audio` diz qual
                // metade é esta.
                tracing::warn!(%erro, "o áudio não abriu nesta sala; só texto");
                voz.marcar(AberturaDoAudio::SemAparelho);
            }
            Err(Atraso) => voz.marcar(AberturaDoAudio::NaoAbriuNoPrazo),
        }
    }
}

/// Abre a voz da sala nos aparelhos pedidos, na thread de [`esperar_o_audio`].
///
/// `start_preferring` and not `start_on`: it falls back to the machine's own
/// device, per side, rather than refusing the room. A preference written down
/// last week names a device that may be in another room by now. The screen
/// shows what actually opened, so the fallback is visible — and per side, so a
/// headset left behind does not also throw away the microphone.
fn abrir_a_voz(shared: &Shared, pedidos: &seele_core::DeviceChoice) -> Futuro<Voice> {
    let canal = shared.media.lock().ok().and_then(|slot| slot.clone());
    let pedidos = pedidos.clone();
    let vez = Arc::clone(&shared.voice.vez_do_driver);
    Box::pin(async move {
        let Some((media, ssrc)) = canal else {
            return Ok(Err("sem conexão por onde a voz saia".to_owned()));
        };
        esperar_o_audio(
            &vez,
            move || {
                Voice::start_preferring(&pedidos, media, ssrc).map_err(|erro| format!("{erro:#}"))
            },
            PRAZO_DO_AUDIO,
        )
        .await
    })
}

/// O assento pelo qual o áudio abre e fecha.
///
/// **O confirmado, e não o especulativo** (M-d da rodada 1 do lote I-a): o
/// assento muda no instante em que a entrada é pedida, e o servidor ainda pode
/// recusá-la. Abrir o áudio por ele abriria o microfone numa sala que vai dizer
/// não. O confirmado chega uma volta de rede depois: o `Pong` do `Ping` marcado
/// que o motor manda atrás do pedido. Ver `seele_core::assento`.
fn sala_do_audio(room: &Room) -> Option<VoiceRoomId> {
    room.sala_confirmada()
}

/// [`sala_do_audio`], lida do `Room` desta sessão.
fn sala_de(shared: &Shared) -> Option<VoiceRoomId> {
    shared
        .room
        .lock()
        .ok()
        .and_then(|room| sala_do_audio(&room))
}

/// O que a volta do enlace faz com o `Room`: adota a sessão nova e refaz o que
/// o motor refez nela.
///
/// `assento` é o de `Aviso::Reconectado`: a sala em que o motor pediu a
/// entrada de novo, ou nenhuma. A sessão nova chega sem assento, o servidor
/// confirma a entrada por silêncio, e a fotografia de ocupação pula a própria
/// pessoa. Medido em `a_volta_mostra_a_pessoa_na_sala_em_que_o_motor_a_sentou_de_novo`:
/// sem refazer, quem caía e voltava era desenhado FORA DE SALA com a voz dentro
/// da sala, e o áudio da pendência 52, que segue o assento, fecharia.
///
/// **A mesma conta do motor, pelo mesmo `seele_core::assento`.** `Room::adopt`
/// guarda o assento que o servidor resgatou (o mesmo `ssrc`), e por cima dele
/// vem o pedido que `restabelecer` mandou: a entrada, no ar até o servidor
/// decidir — e a recusa dela, numa sala com senha, devolve o resgatado —, ou a
/// saída de quem deixou a sala durante a queda.
fn readotar_na_volta(
    room: &mut Room,
    sessao: &seele_core::SessionInfo,
    apelido: &str,
    assento: Option<VoiceRoomId>,
) {
    room.adopt(sessao, apelido);
    match assento {
        Some(sala) => room.enter_voice_room(sala),
        None => room.leave_voice_room(),
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
    if let seele_core::ServerMessage::ModReply {
        request,
        part,
        total,
        payload,
    } = message
    {
        shared.notify(&Event::ModReply {
            request: *request,
            part: *part,
            total: *total,
            payload: payload.clone(),
        });
        return;
    }
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
        // **A revisão dos retratos sobe aqui, e não no ramo do servidor.**
        //
        // Ela nasceu ao lado da revisão do retrato **do servidor**, por
        // semelhança — as duas fazem a mesma coisa pelo mesmo motivo. Mas o
        // `ServerIconChanged` marca `changed.server` e o `PersonIconChanged`
        // marca `changed.roster`: são partes diferentes do estado, e o ramo em
        // que a subida morava nunca rodava para ela.
        //
        // O efeito era completo e silencioso. A revisão ficava parada, o cache
        // de retratos da casca nunca invalidava, e **nenhuma** superfície
        // mudava: nem o bloco do operador, nem a grade da chamada, nem a
        // mensagem, e nem para as outras pessoas — o cliente delas tinha o
        // mesmo ramo morto. Só a prévia do próprio diálogo acompanhava, porque
        // ela lê o arquivo desta máquina e não passa por aqui.
        //
        // Antes do aviso, como a do servidor e pelo mesmo motivo: uma casca que
        // reage ao evento leria o número velho e concluiria que não há imagem
        // nova para buscar.
        if matches!(message, seele_core::ServerMessage::PersonIconChanged { .. }) {
            shared
                .person_icons_revision
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
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
        // **A própria**, por identidade, e não «a da sala». Ver o ADR 0054: a
        // pergunta desta linha é sobre a transmissão desta pessoa, e pegar a
        // primeira da sala respondia com a de outra.
        let minha = shared
            .room
            .lock()
            .ok()
            .and_then(|room| minha_tela_de(&room, None));
        let ha_transmissao = shared
            .room
            .lock()
            .ok()
            .is_some_and(|room| !transmissoes_de(&room, None).is_empty());
        // O que foi pedido morre com a transmissão. Sem esta linha, a próxima
        // vez que esta pessoa compartilhasse mostraria o que está saindo agora
        // ao lado de um teto que ela escolheu noutra ocasião — dois números
        // lado a lado, o da direita mentindo, e nada na tela dizendo qual.
        if minha.is_none() {
            shared.gravar_pedido_da_tela(None);
        }
        // **E a escolha de assistir é esquecida quando a tela escolhida sai do
        // ar.** ADR 0054: sem isto, a escolha ficaria apontando para uma
        // transmissão que já não existe, e a próxima a começar com o mesmo
        // `ScreenId` seria recusada por uma decisão sobre outra tela.
        shared.esquecer_tela_que_saiu();
        if a_tela_mudou(changed, ha_transmissao) {
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
    // Na mesma volta que o resto da telemetria, e pelo mesmo motivo: quem lê
    // isto lê quando quiser, e uma medição não precisa virar evento.
    shared.caminho_medido.store(
        client.caminho_medido(),
        std::sync::atomic::Ordering::Relaxed,
    );
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
        Command::LembrarCaminho(bps) => {
            let _ = client.lembrar_o_caminho(bps).await;
        }
        Command::EnterVoiceRoom(voice_room, password) => {
            if client
                .entrar_na_voice_room(voice_room, password)
                .await
                .is_err()
            {
                return false;
            }
            if let Ok(mut room) = shared.room.lock() {
                room.enter_voice_room(voice_room);
            }
            shared.notify(&Event::RosterChanged);
        }
        Command::LeaveVoiceRoom => {
            if client.sair_da_voice_room().await.is_err() {
                return false;
            }
            // O espelho do `EnterVoiceRoom` acima, e ele faltava. O servidor não
            // devolve o `PersonLeft` a quem o causou — «essa pessoa já sabe» —,
            // então esta metade do roster é contabilidade desta casca. Sem ela
            // o assento se esvazia no servidor e em todos os outros clientes, e
            // a única tela que continua desenhando a pessoa na sala é a de
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
        Command::Send { channel, body, id } => {
            // specs/02-protocolo.md: idempotent by client_msg_id, so a resend
            // after a lost acknowledgement does not post twice.
            //
            // A chave vem no comando — ver [`Command::Send`]. E o corpo já veio
            // conferido contra o teto de bytes, que é o que impede este `dizer`
            // de falhar na codificação e derrubar a sessão inteira por causa de
            // um acento.
            if client
                .dizer(channel, body.trim().to_owned(), id)
                .await
                .is_err()
            {
                return false;
            }
        }
        Command::ImagemDeMod {
            pedido,
            bytes,
            resposta,
        } => {
            if client.imagem_de_mod(pedido, bytes, resposta).await.is_err() {
                return false;
            }
        }
        Command::ModRequest {
            request,
            id,
            channel,
            payload,
        } => {
            if client
                .mod_request(request, id, channel, payload)
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
        // O nome é lido do histórico aqui, e não da janela, pela mesma razão do
        // tipo alegado da prévia logo abaixo. Uma recusa não derruba nada: ela
        // vira o motivo do `NotSaved`, e nada é pedido ao servidor.
        Command::SaveAttachment { attachment, folder } => {
            match destino_de(shared, attachment, &folder) {
                Ok((pasta, nome)) => {
                    if client.salvar_anexo(attachment, pasta, nome).await.is_err() {
                        return false;
                    }
                }
                Err(recusa) => shared.notify(&Event::TransferChanged {
                    transfer: Transfer::NotSaved {
                        attachment: attachment.get(),
                        reason: recusa.reason,
                    },
                }),
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
        // or opening a channel. Those two are facts about this client, which the
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
        Command::Assistir { tela, quero } => {
            // Falhar aqui não derruba a sessão, pela mesma razão do
            // quadro-chave: sem o pedido a imagem não aparece, e com a sessão
            // fechada não aparece nunca mais.
            if let Err(erro) = client.assistir(tela, quero).await {
                tracing::debug!(%erro, quero, "não consegui mudar o que assisto");
            }
        }
        Command::ConsentirNoCaminhoEntrePares {
            pares_que_atende,
            assiste_por_par,
        } => {
            // Falhar aqui não derruba a sessão: sem o consentimento a tela vem
            // do servidor, que é o caminho de sempre — «a malha é alívio, nunca
            // dependência».
            if let Err(erro) = client
                .consentir_no_caminho_entre_pares(seele_core::ConsentimentoDePar {
                    pares_que_atende,
                    assiste_por_par,
                })
                .await
            {
                tracing::debug!(%erro, "não consegui declarar o consentimento de par");
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

/// O nome que o remetente deu a um anexo, tirado do histórico local.
///
/// Em todos os canais, pela razão de [`declared_type_of`].
fn file_name_of(shared: &Shared, attachment: seele_core::AttachmentId) -> Option<String> {
    let room = shared.room.lock().ok()?;
    room.mensagens.values().flatten().find_map(|message| {
        message
            .attachment
            .as_ref()
            .filter(|anexo| anexo.id == attachment)
            .map(|anexo| anexo.file_name.clone())
    })
}

/// A pasta e o nome com que um anexo seria gravado, ou por que não seria.
///
/// Um lugar só para as duas perguntas — a da frase de confirmação e a da
/// gravação —, para que elas não possam discordar. O nome sai do histórico e
/// passa por [`seele_core::anexo_no_disco::nome_seguro`]; a regra que o pegou,
/// quando pega, vai para o log, e a tela recebe só o motivo.
fn destino_de(
    shared: &Shared,
    attachment: seele_core::AttachmentId,
    folder: &str,
) -> Result<(PathBuf, String), SaveRefused> {
    let Some(alegado) = file_name_of(shared, attachment) else {
        return Err(SaveRefused {
            reason: NotSavedReason::AnexoDesconhecido,
            claimed: String::new(),
            folder: folder.to_owned(),
        });
    };
    let claimed = seele_core::anexo_no_disco::para_mostrar(&alegado);
    // Absoluta, e não só presente: uma pasta relativa grava onde quer que o
    // processo tenha sido iniciado, num lugar que a frase não consegue nomear.
    let pasta = PathBuf::from(folder);
    if folder.is_empty() || !pasta.is_absolute() {
        return Err(SaveRefused {
            reason: NotSavedReason::SemPasta,
            claimed,
            folder: folder.to_owned(),
        });
    }
    if let Err(regra) = seele_core::anexo_no_disco::nome_seguro(&alegado) {
        tracing::warn!(
            %attachment,
            nome = %claimed,
            %regra,
            "o nome que veio com o anexo não é só um nome; nada foi gravado"
        );
        return Err(SaveRefused {
            reason: NotSavedReason::NomeRecusado,
            claimed,
            folder: folder.to_owned(),
        });
    }
    Ok((pasta, alegado))
}

/// The type the sender claimed for one attachment, out of the local history.
fn declared_type_of(shared: &Shared, attachment: seele_core::AttachmentId) -> Option<String> {
    let room = shared.room.lock().ok()?;
    // Em todos os canais, e não só no aberto: um arquivo pode ter sido pedido
    // num canal e o download terminar depois de a pessoa trocar de canal.
    room.mensagens.values().flatten().find_map(|message| {
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
/// O relógio local, em segundos desde a época.
///
/// Só para uma pendente: ela é a única coisa nesta ponte cujo instante não vem
/// do servidor, porque o servidor ainda não sabe que ela existe. Uma mensagem
/// confirmada carrega o relógio de quem a aceitou, e é o certo — dois clientes
/// com relógios diferentes concordariam sobre a ordem e discordariam sobre a
/// hora.
fn agora_em_segundos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |desde| {
            i64::try_from(desde.as_secs()).unwrap_or(i64::MAX)
        })
}

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

/// Por que a sessão acabou, como a casca o lê, a partir do que o enlace disse.
///
/// Fora de `drive` para ter teste: é a única tradução entre o fim que o núcleo
/// enumera e o `EndReason` que chega à tela e ao retrato.
fn fim_da_sessao(motivo: &seele_core::enlace::Motivo) -> EndReason {
    match motivo {
        seele_core::enlace::Motivo::Descarregou | seele_core::enlace::Motivo::Pedido => {
            EndReason::LinkLost
        }
        seele_core::enlace::Motivo::Recusado(erro) => fim_da_recusa(erro),
        // O motivo enumerado que o servidor mandou, e não uma tradução dele:
        // quem foi expulso tem de ler «expulso». Cair aqui em
        // `CredentialRejected` mandaria a pessoa conferir uma senha que está
        // certa — a mesma armadilha que o doc de
        // `DisconnectReason::NicknameTaken` registra.
        seele_core::enlace::Motivo::Moderado(porque) => (*porque).into(),
    }
}

/// O fim de uma sessão cuja volta pela bateria foi recusada.
///
/// Era `CredentialRejected` para toda recusa, e a tela mandava conferir convite
/// e senha a quem voltou para um servidor que não é o do convite, cuja chave
/// mudou, que lotou ou cujos MODs mudaram. A recusa já vinha tipada do núcleo
/// ([`seele_core::enlace::Motivo::Recusado`]); só esta tradução a jogava fora.
///
/// # `match` exaustivo, e sem braço `_`
///
/// Pela regra que `enlace::alguem_respondeu` escreve: uma variante nova de
/// `ConnectError` não compila até alguém dizer o que ela vira na tela.
///
/// As de rede não chegam aqui hoje — `enlace::vale_insistir` as devolve à
/// bateria —, e cada uma está nomeada para que uma mudança lá não as faça
/// chegar como credencial.
fn fim_da_recusa(erro: &seele_core::ConnectError) -> EndReason {
    match erro {
        seele_core::ConnectError::Refused { reason } => (*reason).into(),
        // Sem as impressões: `EndReason` é `Copy` e chega ao retrato. Elas
        // ficam no `seele.log`, na linha da volta recusada.
        seele_core::ConnectError::InviteMismatch { .. }
        | seele_core::ConnectError::ChaveNaoProvada => EndReason::ServidorTrocouNaVolta,
        seele_core::ConnectError::PinChanged { .. } => EndReason::ChaveMudouNaVolta,
        // Na volta de uma ligação, quem atendeu não é mais a sala do código: o
        // mesmo que o servidor que trocou, ou, com o pino, a chave que mudou.
        seele_core::ConnectError::CodigoNaoConfere(falha) => match falha {
            seele_core::confere_codigo::FalhaDoCodigo::SePassouPor { .. } => {
                EndReason::ChaveMudouNaVolta
            }
            seele_core::confere_codigo::FalhaDoCodigo::NaoEhChaveDePessoa { .. }
            | seele_core::confere_codigo::FalhaDoCodigo::CodigoNaoBate { .. } => {
                EndReason::ServidorTrocouNaVolta
            }
        },
        // Na volta, o anúncio que esta máquina não aceitou é o servidor que
        // passou a exigir outro conjunto no meio da sessão.
        seele_core::ConnectError::ModsNaoAceitos { .. } => EndReason::ModsMudaram,
        // Não chega à volta hoje: ela é ao `IP:porta` que atendeu, e não ao
        // código. Se chegar, é a conversa que caiu, como as de rede.
        seele_core::ConnectError::NaoLocalizou(_)
        | seele_core::ConnectError::LocalEndpoint
        | seele_core::ConnectError::Unreachable
        | seele_core::ConnectError::TlsRefused
        | seele_core::ConnectError::HandshakeTimeout
        | seele_core::ConnectError::SemResposta
        | seele_core::ConnectError::ProtocolViolation => EndReason::LinkLost,
    }
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
        seele_core::ConnectError::CodigoNaoConfere(falha) => match falha {
            seele_core::confere_codigo::FalhaDoCodigo::NaoEhChaveDePessoa { esperado } => {
                ConnectionError::CodigoNaoBate {
                    esperado: esperado.to_string(),
                    veio: None,
                }
            }
            seele_core::confere_codigo::FalhaDoCodigo::CodigoNaoBate { esperado, veio } => {
                ConnectionError::CodigoNaoBate {
                    esperado: esperado.to_string(),
                    veio: Some(veio.to_string()),
                }
            }
            seele_core::confere_codigo::FalhaDoCodigo::SePassouPor { codigo } => {
                ConnectionError::SePassouPorCodigo {
                    codigo: codigo.to_string(),
                }
            }
        },
        seele_core::ConnectError::NaoLocalizou(falha) => ConnectionError::NaoLocalizou {
            motivo: (*falha).into(),
        },
        // Era `TlsRefused`, e caía no braço de cima: o impostor que repete o
        // certificado sem ter a chave aparecia como versão incompatível, e a
        // tela mandava atualizar o SEELE nas duas máquinas.
        seele_core::ConnectError::ChaveNaoProvada => ConnectionError::ChaveNaoProvada,
        // Não é uma falha traduzida: é a pergunta do ADR 0045 atravessando
        // inteira, com o que a tela precisa para fazê-la.
        seele_core::ConnectError::ModsNaoAceitos { mods, conjunto } => {
            ConnectionError::ModsNaoAceitos {
                mods: mods
                    .iter()
                    .cloned()
                    .map(crate::types::ModExigido::from)
                    .collect(),
                conjunto: conjunto.clone(),
            }
        }
        seele_core::ConnectError::HandshakeTimeout => ConnectionError::HandshakeTimeout,
        seele_core::ConnectError::SemResposta => ConnectionError::SemResposta,
        seele_core::ConnectError::Refused { reason } => ConnectionError::Refused {
            reason: (*reason).into(),
        },
    }
}

#[cfg(test)]
mod a_volta_recusada_diz_o_motivo {
    use super::{classify_connect_failure, fim_da_sessao, ConnectionError, EndReason};
    use seele_core::enlace::Motivo;
    use seele_core::{ConnectError, DisconnectReason};

    #[test]
    fn quem_nao_prova_a_chave_nao_chega_como_versao_incompativel() {
        // Era `TlsRefused`, que esta tradução manda para
        // `Refused { Incompatible }`: o impostor que repete o certificado do
        // servidor aparecia como versão incompatível, e a tela mandava
        // atualizar o SEELE nas duas máquinas.
        assert_eq!(
            classify_connect_failure(&ConnectError::ChaveNaoProvada),
            ConnectionError::ChaveNaoProvada,
            "quem não provou a chave do servidor chega à tela com outro nome, e a frase \
             manda atualizar o SEELE por causa de um impostor"
        );
    }

    #[test]
    fn fim_da_recusa_diz_o_motivo_de_verdade_e_nao_credencial_recusada() {
        // A volta da bateria recusada acabava em `CredentialRejected` qualquer
        // que fosse o motivo, e a tela mandava conferir convite e senha: sobre
        // um servidor que não é o do convite, sobre a chave que mudou, sobre o
        // servidor lotado e sobre os MODs que mudaram. Passa pelo mesmo
        // `fim_da_sessao` que `drive` chama, para que a fiação conte junto.
        let casos = [
            (
                "InviteMismatch",
                ConnectError::InviteMismatch {
                    expected: "bbbb2222".into(),
                    offered: "aaaa1111".into(),
                },
                EndReason::ServidorTrocouNaVolta,
            ),
            (
                "Refused { ServerFull }",
                ConnectError::Refused {
                    reason: DisconnectReason::ServerFull,
                },
                EndReason::ServerFull,
            ),
            (
                "PinChanged",
                ConnectError::PinChanged {
                    pinned: "aaaa1111".into(),
                    offered: "cccc3333".into(),
                },
                EndReason::ChaveMudouNaVolta,
            ),
            (
                "ModsNaoAceitos",
                ConnectError::ModsNaoAceitos {
                    mods: Vec::new(),
                    conjunto: "conjunto".into(),
                },
                EndReason::ModsMudaram,
            ),
            (
                "ChaveNaoProvada",
                ConnectError::ChaveNaoProvada,
                EndReason::ServidorTrocouNaVolta,
            ),
        ];
        let erradas: Vec<String> = casos
            .into_iter()
            .filter_map(|(nome, erro, esperado)| {
                let obtido = fim_da_sessao(&Motivo::Recusado(erro));
                (obtido != esperado).then(|| format!("{nome} deu {obtido:?}, e não {esperado:?}"))
            })
            .collect();
        assert!(
            erradas.is_empty(),
            "a volta recusada chega à tela com um motivo que não é o dela, e a frase manda \
             a pessoa conferir o que não está errado: {erradas:#?}"
        );
    }

    #[test]
    fn fim_da_recusa_do_que_nao_e_recusa_e_enlace_perdido() {
        // Estas não chegam a `Recusado` hoje (`vale_insistir` as manda de volta
        // à bateria). Se uma chegar, ela é rede, e não credencial.
        for erro in [
            ConnectError::LocalEndpoint,
            ConnectError::Unreachable,
            ConnectError::TlsRefused,
            ConnectError::HandshakeTimeout,
            ConnectError::SemResposta,
            ConnectError::ProtocolViolation,
        ] {
            let obtido = fim_da_sessao(&Motivo::Recusado(erro.clone()));
            assert_eq!(
                obtido,
                EndReason::LinkLost,
                "a volta que acabou em {erro:?} chega à tela como {obtido:?}, e não como \
                 enlace perdido: a frase manda procurar outra causa para uma falha de rede"
            );
        }
    }

    /// A ligação de `drive` a [`fim_da_sessao`], que nenhum teste de
    /// comportamento alcança.
    ///
    /// Os testes de cima provam `fim_da_sessao`. `drive` só a usa com uma
    /// sessão viva cuja volta pela bateria o servidor recuse, e nenhum teste
    /// deste crate abre uma. Sem este guarda, um `match` reescrito à mão dentro
    /// de `drive` (o `Recusado(_) => CredentialRejected` de antes do lote H)
    /// deixaria todos verdes, e a tela voltaria a dizer «credencial recusada»
    /// para toda volta recusada.
    ///
    /// Comentários fora, para que um parágrafo que cite a chamada não conte
    /// como a chamada. As agulhas são montadas, e não escritas: este é o
    /// arquivo que o teste lê.
    #[test]
    fn drive_traduz_o_fim_que_o_enlace_avisa_por_fim_da_sessao() {
        let fonte = include_str!("lib.rs");
        let sem_comentario: String = fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let inicio = sem_comentario
            .find(&format!("{} fn drive(", "async"))
            .expect("`drive` sumiu de `lib.rs`");
        let depois = sem_comentario.get(inicio..).expect("o começo de `drive`");
        let fim = depois.find("\n}\n").expect("`drive` nunca fecha");
        let corpo = depois.get(..fim).expect("o corpo de `drive`");
        let Some(braco) = corpo
            .split(&format!("enlace::Aviso::{}(motivo) =>", "Encerrado"))
            .nth(1)
        else {
            panic!(
                "`drive` não trata mais o fim que o enlace avisa pelo nome de sempre, e este \
                 guarda deixou de achar o braço que ele prende"
            );
        };
        let braco = braco
            .split(&format!("{};", "break"))
            .next()
            .unwrap_or(braco);
        assert!(
            braco.contains(&format!("{}(&motivo)", "fim_da_sessao")),
            "`drive` traduz o fim da sessão sem passar por `fim_da_sessao`: os testes de \
             `fim_da_recusa` ficam verdes e a tela pode voltar a dizer «credencial recusada» \
             para toda volta recusada:\n{braco}"
        );
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
        let destino = build_destino(&with_fingerprint, address, &name, &pin, &pin);
        assert_eq!(destino.impressao_esperada(), Some("aaaa1111"));

        let without = ConnectConfig {
            expected_fingerprint: None,
            bilhete: None,
            ..with_fingerprint
        };
        let destino = build_destino(&without, address, &name, &pin, &pin);
        assert_eq!(destino.impressao_esperada(), None);
    }

    /// A configuração de uma conexão por link, com a impressão que ele promete.
    fn config_com_esperada(alvo: &str) -> ConnectConfig {
        ConnectConfig {
            server: alvo.into(),
            alternate_servers: Vec::new(),
            nickname: "rafael".into(),
            home: "/tmp/does-not-matter".into(),
            join_secret: None,
            expected_fingerprint: Some(ESPERADA_DO_LINK.into()),
            bilhete: None,
            audio: false,
            capture_device: None,
            playback_device: None,
        }
    }

    /// O que o link promete nos testes do pino que prova o servidor.
    const ESPERADA_DO_LINK: &str = "aaaa1111";

    /// O destino que `drive` monta para o alvo: o primeiro, com a chave do
    /// aceite igual à própria chave de pino.
    fn destino_do_alvo(alvo: &str) -> seele_core::enlace::Destino {
        let (address, name, pin) = resolve(alvo).expect("um endereço literal resolve sem rede");
        build_destino(&config_com_esperada(alvo), address, &name, &pin, &pin)
    }

    #[test]
    fn um_alternativo_nao_tem_o_pino_que_prova_o_servidor() {
        // `drive` dá a todos os candidatos a chave do destino como chave do
        // aceite, e a cada um a própria chave de pino: só o endereço que a
        // pessoa escolheu tem as duas iguais. Num alternativo, o pin pode ser de
        // outro servidor que esta máquina já fixou ali
        // (`seele_core::tofu::TofuVerifier::decide`). Os dois endereços daqui
        // são de escopo público, para a diferença ser só a de ser o alvo.
        let config = config_com_esperada("203.0.113.7:8383");
        let (address, name, pin) = resolve("203.0.113.7:8383").expect("resolve");
        assert!(
            build_destino(&config, address, &name, &pin, &pin).o_pino_prova_o_servidor,
            "o alvo público deixou de ter o pino que prova o servidor: um link velho contra \
             um servidor já fixado passaria a ser recusado, contra o ADR 0003"
        );

        let (outro, nome_do_outro, pin_do_outro) = resolve("203.0.113.9:9368").expect("resolve");
        assert!(
            !build_destino(&config, outro, &nome_do_outro, &pin_do_outro, &pin)
                .o_pino_prova_o_servidor,
            "um alternativo ficou com o pino que prova o servidor: o pin de outro servidor \
             naquele endereço passaria por cima da impressão esperada, e o `Hello` sairia \
             para ele"
        );
    }

    #[test]
    fn um_alvo_de_escopo_local_nao_tem_o_pino_que_prova_o_servidor() {
        // O alvo de um link de quem hospeda em casa é o endereço da rede local
        // do anfitrião (`seele_proto::uri`), e é também a chave da entrada que o
        // link deixa na lista. Para quem visita pela internet, esse endereço se
        // repete de uma casa para outra: na rede em que a pessoa está, outro
        // servidor pode atender nele, já fixado nesta máquina. Ali, um pin que
        // confere com a esperada discordando é, no caso comum, colisão, porque
        // um servidor que trocou de chave dá `Changed`.
        for alvo in [
            "192.168.0.20:8383",
            "10.0.0.5",
            "172.16.3.4:9000",
            "169.254.1.1:8383",
            "100.64.1.2:8383",
            "[fd00::5]:8383",
            "[fe80::1]:8383",
            "[::ffff:192.168.0.20]:8383",
        ] {
            assert!(
                !destino_do_alvo(alvo).o_pino_prova_o_servidor,
                "o alvo de escopo local {alvo} ficou com o pino que prova o servidor: o \
                 servidor que atende nesse endereço na rede desta pessoa passaria pelo pino \
                 dele, e o `Hello` sairia para ele com o convite, o apelido e a assinatura"
            );
        }
    }

    #[test]
    fn um_alvo_publico_ou_de_loopback_tem_o_pino_que_prova_o_servidor() {
        // O guarda de não ir longe demais. Um endereço público é o mesmo em
        // qualquer rede, e o loopback é esta máquina em qualquer rede: nos dois,
        // o pin do alvo prova a continuidade do servidor, e vale o ADR 0003.
        for alvo in [
            "203.0.113.7:8383",
            "[2001:db8::7]:8383",
            "127.0.0.1:8383",
            "[::1]:8383",
        ] {
            assert!(
                destino_do_alvo(alvo).o_pino_prova_o_servidor,
                "o alvo {alvo} perdeu o pino que prova o servidor: um link velho contra um \
                 servidor já fixado ali passaria a ser recusado, contra o ADR 0003"
            );
        }
    }

    #[test]
    fn um_nome_decide_pelo_endereco_resolvido() {
        // A decisão é sobre o endereço resolvido, o `SocketAddr` do destino, e
        // não sobre o texto: `casa.local` que resolve para a rede de casa é um
        // alvo de escopo local como `192.168.0.20`.
        let config = config_com_esperada("casa.local:8383");
        let na_rede_de_casa: SocketAddr = "192.168.0.20:8383".parse().expect("endereço");
        assert!(
            !build_destino(
                &config,
                na_rede_de_casa,
                "casa.local",
                "casa.local:8383",
                "casa.local:8383"
            )
            .o_pino_prova_o_servidor,
            "um nome que resolve para a rede de casa ficou com o pino que prova o servidor: \
             o escopo não saiu do endereço resolvido, e o nome passou por público"
        );
        let publico: SocketAddr = "203.0.113.7:8383".parse().expect("endereço");
        assert!(
            build_destino(
                &config,
                publico,
                "seele.exemplo.org",
                "seele.exemplo.org:8383",
                "seele.exemplo.org:8383"
            )
            .o_pino_prova_o_servidor,
            "um nome que resolve para um endereço público perdeu o pino que prova o servidor"
        );
    }

    /// O que o verificador decide com o destino que `drive` monta para `alvo`,
    /// quando esta máquina já fixou naquele endereço a chave de quem atende e o
    /// link promete outra. Devolve a decisão, a loja e a chave fixada.
    fn a_volta_ao_alvo_ja_fixado(
        alvo: &str,
    ) -> (
        seele_core::PinDecision,
        Arc<seele_core::MemoryPinStore>,
        String,
    ) {
        use seele_core::tofu::TofuVerifier;
        use seele_core::{MemoryPinStore, PinDecision, PinStore};

        const DE_QUEM_ATENDE: &[u8] = b"o certificado de quem atende no alvo";
        let destino = destino_do_alvo(alvo);
        let loja = Arc::new(MemoryPinStore::new());

        // A visita de antes, sem link: a chave de quem atende ali fica fixada.
        let visita = TofuVerifier::new(
            Arc::clone(&loja) as Arc<dyn PinStore>,
            destino.chave_do_pin.clone(),
            None,
            true,
        );
        let PinDecision::FirstContact {
            fingerprint: fixada,
        } = visita.decide(&destino.chave_do_pin, DE_QUEM_ATENDE)
        else {
            panic!("a visita de antes não fixou a chave de quem atende em {alvo}");
        };

        let verificador = TofuVerifier::new(
            Arc::clone(&loja) as Arc<dyn PinStore>,
            destino.chave_do_pin.clone(),
            destino.impressao_esperada().map(str::to_owned),
            destino.o_pino_prova_o_servidor,
        );
        let decisao = verificador.decide(&destino.chave_do_pin, DE_QUEM_ATENDE);
        (decisao, loja, fixada)
    }

    #[test]
    fn num_alvo_de_lan_o_pino_que_confere_nao_passa_por_cima_da_esperada() {
        // O que a regra de cima compra, visto no verificador. Na rede desta
        // pessoa, o servidor da casa dela atende no `192.168.0.20:8383` que o
        // link de um amigo tem como alvo, e esta máquina já o fixou ali. O pin
        // confere, e a esperada do link é a do amigo: é colisão, e o `Hello` não
        // pode sair para quem atendeu.
        use seele_core::{PinDecision, PinStore};

        for alvo in ["192.168.0.20:8383", "[fd00::5]:8383"] {
            let (decisao, loja, fixada) = a_volta_ao_alvo_ja_fixado(alvo);
            assert_eq!(
                decisao,
                PinDecision::InviteRefused {
                    expected: ESPERADA_DO_LINK.into(),
                    offered: fixada.clone(),
                },
                "no alvo de LAN {alvo}, o pino de quem atende passou por cima da impressão \
                 que o link promete: o `Hello` sairia para o servidor da rede desta pessoa \
                 com o convite, o apelido e a assinatura, e o do amigo ficaria inalcançável \
                 daqui"
            );
            let chave = chave_do_servidor(alvo).expect("chave");
            assert_eq!(
                loja.pinned(&chave),
                Some(fixada),
                "a recusa no alvo de LAN {alvo} mexeu no pino dali: ele continua sendo a \
                 prova de quem atende nesse endereço, e a recusa não fixa nem desfaz nada"
            );
        }
    }

    #[test]
    fn num_alvo_publico_ou_de_loopback_o_pino_que_confere_passa_e_so_avisa() {
        // A metade que a recusa de cima não pode levar junto (ADR 0003): onde o
        // pino prova o servidor, quem discorda dele é o link.
        use seele_core::PinDecision;

        for alvo in ["203.0.113.7:8383", "127.0.0.1:8383"] {
            let (decisao, _, fixada) = a_volta_ao_alvo_ja_fixado(alvo);
            assert_eq!(
                decisao,
                PinDecision::Matches {
                    fingerprint: fixada
                },
                "o alvo {alvo}, já fixado, passou a ser recusado pela impressão do link: um \
                 link velho trancaria a pessoa para fora do servidor que ela usa, contra o \
                 ADR 0003"
            );
        }
    }

    /// **O aceite acompanha o servidor, e não o caminho até ele.**
    ///
    /// Encontrado em produção, e o laço era completo: a pessoa clicava em
    /// ACEITAR, o sim era gravado sob o endereço que ela digitou, a conexão
    /// entrava pelo candidato de retransmissão — cuja porta o NAT troca a cada
    /// sessão — e o aceite, procurado sob *essa* porta, nunca existia. A tela
    /// de aceite voltava. Aceitar de novo gravava no mesmo lugar e falhava do
    /// mesmo jeito, para sempre.
    ///
    /// Os dados que fecharam o diagnóstico: `aceites` tinha
    /// `192.168.50.30:8383 → acff8e16…`, exatamente o conjunto exigido, e quem
    /// respondia era `187.255.97.152:9368`. O arquivo de pins guardava nove
    /// portas diferentes do mesmo endereço público — uma por sessão.
    ///
    /// O pin continua por endereço, porque é isso que ele prova: *este*
    /// endereço apresentou *esta* chave. O aceite é do **servidor**, e todos os
    /// candidatos de uma chegada são o mesmo servidor — mesmo convite, mesma
    /// impressão digital conferida.
    #[test]
    fn um_candidato_alternativo_encontra_o_aceite_do_destino() {
        let home =
            std::env::temp_dir().join(format!("seele-ffi-aceite-candidato-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("temporário");
        let casa = home.to_string_lossy().into_owned();
        let digitado = "192.168.50.30:8383";
        let conjunto = "acff8e1650272dcca3835f2a8a6dc0af6ce12fd081e9806ee1ced396f3e4339d";
        crate::mods::aceitar(&casa, digitado, conjunto).expect("gravar o sim");

        let config = ConnectConfig {
            home: casa.clone(),
            server: digitado.into(),
            alternate_servers: Vec::new(),
            nickname: "alguem".into(),
            join_secret: None,
            expected_fingerprint: None,
            bilhete: None,
            audio: false,
            capture_device: None,
            playback_device: None,
        };

        // O candidato que de fato responde: outro endereço, outra porta — a
        // porta que o NAT escolheu nesta sessão e vai trocar na próxima.
        let alternativo: SocketAddr = "187.255.97.152:9368".parse().expect("endereço");
        let destino = build_destino(
            &config,
            alternativo,
            "servidor",
            "187.255.97.152:9368",
            digitado,
        );
        assert_eq!(
            destino.aceito.as_deref(),
            Some(conjunto),
            "o candidato alternativo não encontrou o aceite do destino: a tela \
             de aceite volta, e aceitar de novo grava no mesmo lugar que já não \
             é consultado"
        );

        // E a outra metade: o aceite continua sendo **daquele** servidor. Um
        // destino que a pessoa nunca autorizou não herda o sim de ninguém.
        let outro = build_destino(
            &config,
            alternativo,
            "servidor",
            "187.255.97.152:9368",
            "outro.exemplo:8383",
        );
        assert_eq!(
            outro.aceito, None,
            "um servidor que nunca foi aceito herdou o consentimento de outro"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// E a chamada de verdade passa a chave do destino a **todos** os
    /// candidatos, e não a de cada um.
    ///
    /// Pelo mesmo motivo que `every_expected_fingerprint_reaches_the_destino_it_promised`
    /// existe: `build_destino` pode estar certo e `drive` continuar chamando-o
    /// com o argumento errado, e essa diferença só um aperto de mão de verdade
    /// mostraria.
    #[test]
    fn a_chave_do_aceite_que_chega_aos_candidatos_e_a_do_destino() {
        let fonte = include_str!("lib.rs");
        let sem_comentario: String = fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let chamada = sem_comentario
            .split_once("std::iter::once((address, server_name.clone(), pin_key.clone()))")
            .expect("a montagem dos destinos")
            .1;
        let ate_o_collect = chamada
            .split_once(".collect()")
            .map_or(chamada, |(antes, _)| antes);
        assert!(
            ate_o_collect.contains("&chave_do_aceite"),
            "os candidatos voltaram a receber a própria chave como chave do \
             aceite: quem entra pelo candidato de retransmissão cai no laço da \
             tela de aceite: {ate_o_collect}"
        );
    }

    /// O sim que a casca guarda é lido pela conexão seguinte.
    ///
    /// **A trava que este teste põe.** São dois caminhos que nunca se
    /// encontram: `mods::aceitar` grava sem socket nenhum, de dentro de um
    /// comando da janela, e `build_destino` lê já com o endereço resolvido. Se
    /// os dois derivarem a chave por conta própria, o dia em que uma das regras
    /// mudar é um dia em que o aceite é escrito num lugar e procurado noutro —
    /// e o sintoma é mudo: ninguém vê um erro, a pessoa só é perguntada de novo
    /// em toda entrada, para sempre.
    ///
    /// As três formas abaixo são as que já custaram caro em outro lugar: a
    /// porta omitida (que a lista de visitados duplicava), o IPv6 literal (ADR
    /// 0022, passo 2) e o endereço comum.
    #[test]
    fn o_aceite_e_guardado_sob_a_chave_que_a_conexao_vai_procurar() {
        let home =
            std::env::temp_dir().join(format!("seele-ffi-chave-do-aceite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("temporário");
        let casa = home.to_string_lossy().into_owned();
        let conjunto = "b2".repeat(32);

        for digitado in ["localhost:8383", "localhost", "[::1]:8383"] {
            let (address, name, pin) = resolve(digitado).expect("resolve");

            crate::mods::aceitar(&casa, digitado, &conjunto).expect("guardar o sim");

            let config = ConnectConfig {
                server: digitado.into(),
                alternate_servers: Vec::new(),
                nickname: "rafael".into(),
                home: casa.clone(),
                join_secret: None,
                expected_fingerprint: None,
                bilhete: None,
                audio: false,
                capture_device: None,
                playback_device: None,
            };
            let destino = build_destino(&config, address, &name, &pin, &pin);
            assert_eq!(
                destino.aceito,
                Some(conjunto.clone()),
                "o aceite guardado para «{digitado}» não foi encontrado pela conexão"
            );

            crate::mods::esquecer_aceite(&casa, digitado).expect("esquecer");
            let destino = build_destino(&config, address, &name, &pin, &pin);
            assert_eq!(
                destino.aceito, None,
                "retirar o consentimento de «{digitado}» não chegou à conexão"
            );
        }

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Um alvo que não é endereço é recusado por nome, e não gravado como veio.
    ///
    /// Gravá-lo criaria a entrada que nada lê — ver o teste acima.
    #[test]
    fn um_alvo_que_nao_e_endereco_nao_vira_um_aceite_escrito_no_vazio() {
        let casa = std::env::temp_dir().to_string_lossy().into_owned();
        assert_eq!(
            crate::mods::aceitar(&casa, "::1", &"c3".repeat(32)),
            Err("EnderecoInvalido".to_owned()),
            "um IPv6 sem colchetes foi aceito como chave"
        );
        assert_eq!(crate::mods::aceite_de(&casa, "::1"), None);
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
                material_e2ee: None,
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
                material_e2ee: None,
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
                    material_e2ee: None,
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

    /// Uma `Connection` montada à mão, com o fim da thread da conexão em
    /// `encerrada`.
    fn conexao_com_o_fim(encerrada: std::sync::mpsc::Receiver<()>) -> Connection {
        let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
        Connection {
            commands,
            shared: bare_shared(),
            encerrada: Mutex::new(Some(encerrada)),
        }
    }

    /// **Sem o sinal da thread da conexão, `encerrar_e_esperar` espera o teto e
    /// diz que não terminou.**
    ///
    /// O `false` é o que a casca escreve no `seele.log`: quem ficou vai notar a
    /// saída pelo prazo de ociosidade. Uma `encerrar_e_esperar` que voltasse
    /// `true` sem esperar o sinal deixaria o processo sair com a despedida no
    /// meio, e a linha que diz isso nunca sairia.
    #[test]
    fn encerrar_e_esperar_sem_o_sinal_espera_o_teto_e_diz_que_nao_terminou() {
        let (ainda_correndo, encerrada) = std::sync::mpsc::channel();
        let connection = conexao_com_o_fim(encerrada);
        let teto = Duration::from_millis(50);

        let comeco = Instant::now();
        let terminou = connection.encerrar_e_esperar(teto);
        let levou = comeco.elapsed();

        assert!(
            !terminou,
            "a thread da conexão não deu sinal nenhum, e `encerrar_e_esperar` disse que ela \
             terminou: a casca deixa o processo sair com a despedida no meio, calada"
        );
        assert!(
            levou >= teto,
            "`encerrar_e_esperar` voltou em {levou:?}, antes do teto de {teto:?}: não esperou o \
             sinal da thread da conexão"
        );
        drop(ainda_correndo);
    }

    /// **O sinal dado, ou a thread que terminou sem dá-lo, dizem que terminou.**
    ///
    /// O segundo é a thread que caiu antes de chegar ao sinal — o runtime que não
    /// subiu, um pânico —: o remetente largado diz o mesmo fim, e esperar o teto
    /// por ela seria segurar a saída por uma thread que não existe mais.
    #[test]
    fn encerrar_e_esperar_volta_na_hora_com_o_sinal_dado_ou_a_thread_acabada() {
        let (sinal, encerrada) = std::sync::mpsc::channel();
        sinal.send(()).expect("o sinal");
        let deu_o_sinal = conexao_com_o_fim(encerrada);
        let (sem_sinal, acabou) = std::sync::mpsc::channel::<()>();
        drop(sem_sinal);
        let acabou_sem_sinal = conexao_com_o_fim(acabou);
        let teto = Duration::from_secs(5);

        let comeco = Instant::now();
        assert!(
            deu_o_sinal.encerrar_e_esperar(teto),
            "a thread da conexão deu o sinal, e `encerrar_e_esperar` disse que não terminou"
        );
        assert!(
            acabou_sem_sinal.encerrar_e_esperar(teto),
            "a thread da conexão acabou sem sinal, e `encerrar_e_esperar` disse que não terminou"
        );
        assert!(
            comeco.elapsed() < teto,
            "`encerrar_e_esperar` esperou o teto por uma thread que já tinha terminado"
        );
    }

    /// A `Shared` with nothing in it, for the folds below.
    fn bare_shared() -> Arc<Shared> {
        Arc::new(Shared {
            caminho_medido: std::sync::atomic::AtomicU32::new(0),
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            icon_revision: std::sync::atomic::AtomicU64::new(0),
            person_icons_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: VozDaSala::nova(false, seele_core::DeviceChoice::default()),
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
            tela_escolhida: Mutex::new(EscolhaDeTela::NinguemEscolheu),
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
            encerrada: Mutex::new(None),
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
            encerrada: Mutex::new(None),
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
            encerrada: Mutex::new(None),
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
            encerrada: Mutex::new(None),
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
            encerrada: Mutex::new(None),
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
        let connection = Connection {
            commands,
            shared,
            encerrada: Mutex::new(None),
        };

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
            encerrada: Mutex::new(None),
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
            encerrada: Mutex::new(None),
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
            encerrada: Mutex::new(None),
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
        // `AudioState::sem_voz` is the one place that pairing is decided, and
        // it starts from what a person who chose nothing has chosen.
        let quiet = AudioState::sem_voz(
            &ControlesGuardados::de_partida(seele_core::DeviceChoice::default()),
            AberturaDoAudio::ForaDeSala,
        );
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
            caminho_medido: std::sync::atomic::AtomicU32::new(0),
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            icon_revision: std::sync::atomic::AtomicU64::new(0),
            person_icons_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: VozDaSala::nova(false, seele_core::DeviceChoice::default()),
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
            tela_escolhida: Mutex::new(EscolhaDeTela::NinguemEscolheu),
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
    /// Um `Shared` vazio, para exercer o `fold` sem uma conexão.
    pub(super) fn compartilhado_de_teste() -> Arc<Shared> {
        Arc::new(Shared {
            caminho_medido: std::sync::atomic::AtomicU32::new(0),
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            icon_revision: std::sync::atomic::AtomicU64::new(0),
            person_icons_revision: std::sync::atomic::AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: VozDaSala::nova(false, seele_core::DeviceChoice::default()),
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
            tela_escolhida: Mutex::new(EscolhaDeTela::NinguemEscolheu),
            fontes_de_tela: Mutex::new(Vec::new()),
        })
    }

    /// Um retrato novo faz a revisão dos retratos andar.
    ///
    /// **É a diferença deste número que manda a casca rebuscar as imagens.**
    /// Parada, o cache dela nunca invalida — e o efeito era completo e
    /// silencioso: quem trocava a própria imagem via a prévia do diálogo mudar,
    /// porque ela lê o arquivo desta máquina, e mais nada mudava em lugar
    /// nenhum. Nem o bloco do operador, nem a grade da chamada, nem para as
    /// outras pessoas, cujo cliente tinha o mesmo defeito.
    ///
    /// A causa foi a subida ter nascido no ramo errado: ao lado da revisão do
    /// retrato **do servidor**, por semelhança, dentro de um `if changed.server`
    /// — e `PersonIconChanged` marca `changed.roster`. Duas partes diferentes do
    /// estado, e o ramo nunca rodava para ela.
    #[test]
    fn um_retrato_novo_faz_a_revisao_dos_retratos_andar() {
        use seele_core::{PersonId, ServerMessage};

        let shared = compartilhado_de_teste();
        let antes = shared.person_icons_revision.load(Ordering::Relaxed);

        fold(
            &shared,
            &ServerMessage::PersonIconChanged {
                person: PersonId(7),
                icon: Some(vec![1, 2, 3]),
            },
        );

        assert!(
            shared.person_icons_revision.load(Ordering::Relaxed) > antes,
            "a revisão dos retratos não andou com um `PersonIconChanged`, e o \
             cache da casca nunca vai rebuscar imagem nenhuma"
        );
    }

    /// E uma mensagem que não é de retrato **não** a faz andar.
    ///
    /// Sem isto, a asserção acima passaria com a subida solta fora de qualquer
    /// condição — e cada quadro do roster mandaria a casca rebuscar 8 KiB por
    /// pessoa, que é exatamente o custo que a revisão existe para evitar.
    #[test]
    fn um_nome_novo_nao_faz_a_revisao_dos_retratos_andar() {
        use seele_core::{PersonId, ServerMessage};

        let shared = compartilhado_de_teste();
        let antes = shared.person_icons_revision.load(Ordering::Relaxed);

        fold(
            &shared,
            &ServerMessage::PersonRenamed {
                person: PersonId(7),
                nickname: "aleta".into(),
            },
        );

        assert_eq!(
            shared.person_icons_revision.load(Ordering::Relaxed),
            antes,
            "trocar de nome mandou a casca rebuscar todos os retratos"
        );
    }

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
                material_e2ee: None,
            },
            ssrc: Ssrc(pessoa.wrapping_mul(10).try_into().unwrap_or(1)),
        });
    }

    /// **A contagem de quem assiste é a do servidor, e não a do roster daqui.**
    ///
    /// # O que mudou no R20
    ///
    /// Era «quem está na sala menos quem compartilha», contado neste lado. Isso
    /// **inclui quem escolheu não assistir** — e desde que a escolha existe, isso
    /// é gente de verdade. Duas contagens para a mesma pergunta, discordando, e o
    /// servidor já mandava a certa: `ScreenViewers` conta assinaturas, e é o mesmo
    /// N pelo qual ele divide o teto no §5.1.
    #[test]
    fn a_tela_conta_quem_o_servidor_disse_que_assiste() {
        use seele_core::{PersonId, ScreenId, ServerMessage};

        let mut room = sala_com_tela(PersonId(3));
        sentar(&mut room, 3);
        sentar(&mut room, 4);

        let tela = tela_por_id(&room, ScreenId(9), None).expect("a transmissão foi anunciada");
        assert_eq!(tela.tela, 9, "a transmissão não diz qual ela é");
        assert_eq!(tela.de, 3);
        assert!(
            !tela.e_minha,
            "quem compartilha é a pessoa 3, e nós somos a 7"
        );
        // **Zero até o servidor contar.** Zero é a resposta honesta: uma
        // transmissão que começou agora não tem ninguém assistindo ainda, e
        // deduzir do roster era contar quem recusou.
        assert_eq!(
            tela.espectadores, 0,
            "a contagem voltou a ser deduzida do roster desta máquina, e ela \
             inclui quem escolheu não assistir"
        );

        room.apply(&ServerMessage::ScreenViewers {
            tela: ScreenId(9),
            quantos: 2,
        });
        assert_eq!(
            tela_por_id(&room, ScreenId(9), None)
                .expect("a transmissão continua")
                .espectadores,
            2,
            "o `ScreenViewers` do servidor não chegou à ponte"
        );
    }

    /// E uma transmissão que não é a pedida não é devolvida no lugar dela.
    ///
    /// R18: `tela_de` pegava a **primeira** do mapa da sala, então numa sala com
    /// dois transmissores o painel podia falar de uma e a imagem ser de outra.
    #[test]
    fn a_tela_e_procurada_por_identidade() {
        use seele_core::{PersonId, ScreenId, ServerMessage};

        let mut room = sala_com_tela(PersonId(3));
        room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VoiceRoomId(1),
            person: PersonId(4),
            screen: ScreenId(11),
        });

        assert_eq!(
            tela_por_id(&room, ScreenId(11), None)
                .expect("a segunda transmissão existe")
                .de,
            4,
            "a busca por identidade devolveu outra transmissão"
        );
        assert_eq!(
            tela_por_id(&room, ScreenId(9), None)
                .expect("a primeira continua")
                .de,
            3
        );
        assert!(
            tela_por_id(&room, ScreenId(99), None).is_none(),
            "uma transmissão que não existe foi respondida com outra"
        );
    }

    #[test]
    fn a_tela_de_quem_compartilha_se_reconhece() {
        use seele_core::{PersonId, ScreenId};

        // O pessoa 7 é quem esta sessão é — `sala_com_tela` o diz na `Session`.
        let room = sala_com_tela(PersonId(7));
        let tela = tela_por_id(&room, ScreenId(9), None).expect("a transmissão foi anunciada");
        assert!(
            tela.e_minha,
            "sem isto a casca desenha o painel de quem assiste para quem compartilha"
        );
        // E `minha_tela_de` a encontra sem que ninguém diga o `ScreenId`: é a
        // pergunta «qual é a minha», e ela é por identidade também — pela da
        // pessoa. ADR 0054.
        assert_eq!(
            minha_tela_de(&room, None).map(|minha| minha.tela),
            Some(9),
            "a própria transmissão não é encontrada por identidade"
        );
    }

    #[test]
    fn nao_ha_tela_quando_ninguem_compartilha() {
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
        assert!(tela_por_id(&room, ScreenId(9), None).is_none());
        assert!(minha_tela_de(&room, None).is_none());
    }

    /// **A ponte não finge saber o que está saindo, e já não reserva lugar.**
    ///
    /// # O que este teste era, e por que ele mudou
    ///
    /// Ele prendia `medida == false` e os três zeros — «alguém passou a medir o
    /// que sai: então preencha os três números e mude este teste». A regra estava
    /// certa e os quatro campos nunca tiveram valor: a tela reservava três caixas
    /// e escrevia «ainda não há medida» nas três, para sempre.
    ///
    /// No R20 os quatro saíram da ponte, e a regra continua: nenhum campo de
    /// medida existe enquanto não houver medida. O que este teste prende agora é
    /// a **ausência** deles, e a mensagem diz o que fazer no dia em que houver.
    #[test]
    fn a_tela_nao_finge_saber_o_que_esta_saindo() {
        use seele_core::{PersonId, ScreenId};

        let tela = tela_por_id(&sala_com_tela(PersonId(3)), ScreenId(9), None)
            .expect("a transmissão foi anunciada");
        assert_eq!(
            tela.parada, None,
            "parar com motivo é decisão do teto, e nenhum teto roda deste lado"
        );
        // O guarda contra o campo que volta sem medida atrás dele mora em
        // `types::a_tela_em_curso_atravessa_pelo_nome_e_diz_de_qual_tela_fala`,
        // que é onde a forma do JSON é conferida. Aqui fica o que esta função
        // devolve, e ela devolve só o que se sabe.
        assert_eq!(tela.espectadores, 0, "a contagem foi inventada deste lado");
    }

    /// Um pedido qualquer, com os três controles do §5 preenchidos.
    fn limites() -> LimitesDeTela {
        LimitesDeTela {
            banda_bps: Some(1_200_000),
            altura_maxima: 1080,
            quadros_maximos: 30,
            prioridade: crate::types::Prioridade::Nitidez,
            com_som: true,
        }
    }

    #[test]
    fn o_pedido_so_aparece_ao_lado_da_propria_tela() {
        use seele_core::{PersonId, ScreenId};

        // O §5 manda pôr o que está saindo ao lado do que foi pedido. O que foi
        // pedido é escolha de quem transmite e **não viaja**: o `ScreenHeader`
        // carrega resolução e codec, nunca o teto. Então esta ponte só tem como
        // preencher a coluna da própria transmissão — e preenchê-la com a
        // escolha desta máquina ao lado da tela de outra pessoa seria mostrar o
        // teto de uma transmissão como se fosse o de outra.
        let minha = tela_por_id(&sala_com_tela(PersonId(7)), ScreenId(9), Some(limites()))
            .expect("a transmissão foi anunciada");
        assert!(minha.e_minha);
        assert_eq!(
            minha.pedido,
            Some(limites()),
            "quem compartilha perdeu a metade da comparação que o §5 obriga"
        );

        let alheia = tela_por_id(&sala_com_tela(PersonId(3)), ScreenId(9), Some(limites()))
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
                    material_e2ee: None,
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
                    material_e2ee: None,
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
                    material_e2ee: None,
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
mod uma_pendente_nao_e_uma_mensagem_do_servidor {
    use super::*;

    /// **Uma pendente não pode ser endereçada como mensagem do servidor.**
    ///
    /// R05 abriu a porta e este guarda a fecha: `Message::id` vale zero numa
    /// mensagem que o servidor ainda não confirmou, e zero não endereça nada.
    ///
    /// O primeiro caller a pisar nisto foi `conformance::moderacao`, que pegava
    /// `messages()[0].id` e recebia «não há tal mensagem» no lugar da recusa de
    /// permissão que ele mede. O próximo seria uma tela, e ali o sintoma seria
    /// clicar em «remover» numa linha visível e nada acontecer.
    #[test]
    fn remover_uma_pendente_e_recusado_antes_de_sair() {
        // Um `Connection` com o canal de comandos vivo: o teste não roda laço
        // nenhum, e o que ele mede é a recusa acontecer **antes** de o comando
        // sair. O receptor fica na mão para o canal não fechar.
        let (manda, _recebe) = tokio::sync::mpsc::unbounded_channel();
        let conexao = Connection {
            commands: manda,
            shared: super::tests::compartilhado_de_teste(),
            encerrada: Mutex::new(None),
        };
        assert_eq!(
            conexao.remove_message(0),
            Err(ConnectionError::UnknownMessage),
            "o pedido de remover o zero saiu para o servidor: ele responde «não \
             há tal mensagem», que é verdade e é a frase errada para quem clicou \
             num texto que está na tela"
        );
        // E um id de verdade continua saindo: um guarda que barrasse tudo passaria
        // no teste acima e desligaria a remoção inteira.
        assert!(conexao.remove_message(7).is_ok());
    }
}

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
                com_som: true,
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
    fn uma_ligacao_que_nao_achou_a_sala_atravessa_com_o_motivo() {
        // A tarefa 6c: cada motivo é uma frase diferente na tela (spec de
        // 23/09), e a casca a escolhe pelo nome que atravessa. Achatar o motivo
        // aqui, num `Unreachable`, mandaria quem liga procurar defeito de rede
        // quando a pessoa só está com o SEELE fechado.
        use seele_core::chegada::Etapa;
        use seele_core::encontro::FalhaAoLocalizar as Nucleo;

        for falha in [
            Nucleo::SemPonto,
            Nucleo::NinguemMora,
            Nucleo::Enc2Calado,
            Nucleo::PontoRecusou,
            Nucleo::PontoMudo,
            Nucleo::PontoNaoResolve,
            Nucleo::SoAEscuta,
        ] {
            let etapa = Etapa::Desistiu(seele_core::ConnectError::NaoLocalizou(falha));
            let Ok(json) = serde_json::to_string(&ConnectStage::from(&etapa)) else {
                panic!("a desistência de uma ligação não serializa");
            };
            assert_eq!(
                json,
                format!(
                    r#"{{"Desistiu":{{"motivo":{{"NaoLocalizou":{{"motivo":"{falha:?}"}}}}}}}}"#
                ),
                "o motivo de não achar a sala atravessou com outro nome"
            );
        }

        // E a etapa que pergunta ao quarto atravessa com o ponto, que é o
        // «onde» do passo no log.
        let localizando = Etapa::Localizando {
            ponto: "encontro.exemplo:8384".into(),
        };
        assert_eq!(
            ConnectStage::from(&localizando),
            ConnectStage::Localizando {
                ponto: "encontro.exemplo:8384".into()
            }
        );
        assert_eq!(
            campos_do_passo(&localizando),
            (Some("encontro.exemplo:8384".into()), None, None)
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

    /// Este continua lendo texto-fonte, e de propósito.
    ///
    /// O que ele guarda não é o que a conferência decide — isso agora é
    /// comportamento, no `mod comportamento` logo abaixo. O que ele guarda é a
    /// casca **chamar** a conferência: `set_capture_device` e a irmã dela
    /// abrem aparelho de verdade, e conformidade roda sem placa de som, então
    /// não há como exercitá-las aqui. Quem apaga a chamada volta a devolver
    /// `Ok` para uma troca que não aconteceu, e nenhum tipo acusa isso.
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

    /// A conferência, exercitada pelo que ela **faz**.
    ///
    /// Substitui o guarda que lia o corpo de `conferir_troca` com
    /// `include_str!` e conferia se certas palavras estavam lá. Aquele guarda
    /// passaria com a comparação invertida, com o lado trocado, e com um `Ok`
    /// devolvido sempre — nenhuma dessas três coisas é texto que falte, e todas
    /// as três são o defeito que ele existe para acusar.
    mod comportamento {
        use super::super::{conferencia, ConnectionError, Lado};

        #[test]
        fn abrir_o_que_se_pediu_passa() {
            assert!(conferencia(Some("fone-usb"), Some("fone-usb"), Lado::Saida).is_ok());
        }

        #[test]
        fn abrir_outro_aparelho_e_recusado_pelo_lado_certo() {
            // O defeito relatado: «mostra EM USO num e ESCOLHIDO no que eu
            // escolhi, mas não muda». Voltar `Ok` aqui é o produto afirmando
            // que fez o que não fez.
            assert_eq!(
                conferencia(Some("fone-usb"), Some("caixas-da-mesa"), Lado::Saida),
                Err(ConnectionError::PlaybackDeviceGone)
            );
            assert_eq!(
                conferencia(
                    Some("microfone-bom"),
                    Some("microfone-do-laptop"),
                    Lado::Entrada
                ),
                Err(ConnectionError::CaptureDeviceGone)
            );
        }

        #[test]
        fn nao_abrir_nada_tambem_e_recusado() {
            // Um aparelho que o sistema não descreve não é um aparelho que
            // atende ao pedido: `None` não é «qualquer um serve».
            assert_eq!(
                conferencia(Some("fone-usb"), None, Lado::Saida),
                Err(ConnectionError::PlaybackDeviceGone)
            );
        }

        #[test]
        fn sem_pedido_qualquer_aparelho_que_abra_esta_certo() {
            // O caso mais comum de todos: o padrão da máquina. Exigir igualdade
            // aqui recusaria toda sessão que nunca escolheu nada.
            assert!(conferencia(None, Some("qualquer-um"), Lado::Entrada).is_ok());
            assert!(conferencia(None, None, Lado::Saida).is_ok());
        }
    }
}

// O que o quarto disse sobre onde um servidor mora hoje, reexportado para a
// casca ler a resposta sem nomear o `seele-core`, que o ADR 0002 não a deixa
// alcançar. E a regra que monta com ela o bilhete do `LEVE` desta volta: a
// escuta do quarto só troca o aviso quando mora no IP do servidor que a mesma
// resposta deu (o I1 da revisão final do Plano 1). A casca usa a regra, e não
// `OndeMora::escuta`, que é a resposta crua.
pub use seele_core::encontro::{bilhete_desta_volta, OndeMora};

/// Onde este servidor mora hoje: o socket dele, e a escuta de avisos dele.
///
/// # Por que a casca chama isto e não monta as marcas
///
/// A marca é vocabulário de fio (ADR 0002). A casca tem uma impressão digital
/// e um ponto de encontro, e o que ela quer são dois endereços. As marcas saem
/// de [`seele_core::encontro::Marcas::do_servidor`], a **mesma** função com que
/// o anfitrião se registra (`Convocacao::para_servidor`, no `seele-server`).
/// Quando cada lado derivava as suas, os dois divergiram, e a escuta nunca foi
/// achada.
///
/// # O que volta
///
/// Um [`OndeMora`], e não um par de `Option`: «o ponto não respondeu» e «o
/// ponto respondeu e ninguém mora lá» apontam para lugares diferentes, e a
/// consulta registra no log qual foi. A casca lê [`OndeMora::servidor`], que é
/// `None` quando o quarto não o deu, e monta o bilhete desta volta com
/// [`bilhete_desta_volta`], que só usa a escuta que o servidor da mesma
/// resposta confirma. [`OndeMora::escuta`] é a resposta crua, e quem ocupou a
/// marca da escuta também aparece nela.
pub async fn onde_mora_hoje(ponto: &str, impressao: &str) -> OndeMora {
    let Some(marcas) = seele_core::encontro::Marcas::do_servidor(impressao) else {
        tracing::info!(
            "quarto: a impressão digital guardada não forma marca; nenhuma pergunta saiu"
        );
        return OndeMora::SemMarca;
    };
    seele_core::encontro::onde_mora_hoje(ponto, &marcas, seele_core::encontro::PRAZO_DO_QUARTO)
        .await
}

#[cfg(test)]
mod a_consulta_ao_quarto {
    //! A porta da casca para o quarto: `onde_mora_hoje` com uma impressão
    //! digital, e não com marcas.

    use super::{onde_mora_hoje, OndeMora};

    #[tokio::test]
    async fn uma_impressao_que_nao_forma_marca_nao_manda_pergunta_nenhuma() {
        // O ponto é um socket local **mudo**, e é ele quem prova que nada saiu:
        // se alguma pergunta partisse, ela pararia no `recv_from` dele. Só a
        // variante e a duração não bastam: uma consulta que perguntasse e
        // desistisse depressa passaria nas duas.
        let mudo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ponto = mudo.local_addr().unwrap().to_string();

        let comecou = std::time::Instant::now();
        let achado = onde_mora_hoje(&ponto, "curta").await;
        let levou = comecou.elapsed();
        assert_eq!(
            achado,
            OndeMora::SemMarca,
            "uma impressão digital que não forma marca virou outra resposta: quem ler o log vai \
             procurar defeito de rede onde não há"
        );
        assert!(
            levou < std::time::Duration::from_millis(200),
            "sem marca não há pergunta, e mesmo assim a consulta esperou {levou:?}"
        );

        let mut balde = [0_u8; 64];
        let chegou = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            mudo.recv_from(&mut balde),
        )
        .await;
        assert!(
            chegou.is_err(),
            "a consulta devolveu `SemMarca` mas mandou um datagrama ao ponto assim mesmo: sem \
             marca não há o que perguntar, e o que sai é tráfego sem resposta possível"
        );
    }
}

/// A impressão contra a qual uma conexão confere o certificado do servidor.
///
/// # Por que não é só a do link
///
/// Era só a do link, e isso eram dois defeitos com uma causa (§2.1 e §3.1 da
/// análise de 22/09). Quem volta a um servidor pela lista de conhecidos não tem
/// link nesta sessão: ele foi lido uma vez, num processo que já fechou. A
/// pessoa entrava então sem impressão esperada nenhuma:
///
/// - o endereço novo que o quarto devolvia era fixado **às cegas**, com a faixa
///   PRIMEIRO CONTATO. A marca do quarto está em todo link, e no quarto fica
///   quem escreveu primeiro. Bastava o anfitrião ficar fora mais de 60 s para
///   um impostor ser fixado e gravado na lista (o S3);
/// - `Batida::preparar` tira a marca do `LEVE` da impressão e, sem ela,
///   devolvia `None`: quem estava atrás de NAT não era alcançado na volta, nem
///   na reconexão (o terceiro defeito do link).
///
/// A impressão guardada existia, e só servia para formar a marca da pergunta.
///
/// # A ordem
///
/// O link desta sessão vence a guardada. Ele é o que a pessoa acabou de colar
/// para este endereço, e a guardada é, quase sempre, a impressão que uma conexão
/// anterior **aceitou** ([`impressao_a_guardar`]). Não é assim numa lista gravada
/// pela 0.15.0: ela guardava a impressão do link daquela vez qualquer que
/// tivesse sido o veredito, inclusive quando o link discordava do pino e a
/// conexão ficou com a chave fixada. Um link colado de novo corrige essa entrada,
/// e a primeira volta pelo alvo já fixado também, quando ele é de escopo público
/// ([`impressao_a_guardar`]). Num alvo de LAN, essa volta é recusada dentro do
/// TLS, e o remédio é colar o link de novo.
/// Se o link e a guardada discordam, conferir pela velha recusaria o servidor
/// que a pessoa acabou de pedir. A mesma impressão forma a marca da pergunta ao
/// quarto (o `connect` do app a usa nas duas coisas), pela mesma razão:
/// perguntar onde mora uma chave e conferir outra faria o quarto apontar para o
/// servidor que a conferência recusaria logo em seguida.
#[must_use]
pub fn impressao_a_conferir(do_link: Option<&str>, guardada: Option<&str>) -> Option<String> {
    do_link.or(guardada).map(str::to_owned)
}

#[cfg(test)]
mod a_volta_pela_trilha_confere {
    use super::impressao_a_conferir;

    #[test]
    fn sem_link_nesta_sessao_confere_pela_guardada() {
        assert_eq!(
            impressao_a_conferir(None, Some("aaaa1111")),
            Some("aaaa1111".to_owned()),
            "quem volta pela lista entra num endereço novo sem conferir nada — o \
             PRIMEIRO CONTATO cego do S3 — e sem `LEVE`"
        );
    }

    #[test]
    fn o_link_desta_sessao_vence_a_guardada() {
        assert_eq!(
            impressao_a_conferir(Some("bbbb2222"), Some("aaaa1111")),
            Some("bbbb2222".to_owned()),
            "um link colado agora é a intenção mais nova de quem conecta; conferir \
             pela guardada recusaria o servidor que a pessoa acabou de pedir"
        );
    }

    #[test]
    fn so_o_link_e_o_caminho_de_sempre() {
        assert_eq!(
            impressao_a_conferir(Some("bbbb2222"), None),
            Some("bbbb2222".to_owned()),
            "sem nada guardado, o link desta sessão deixou de ser conferido: quem cola um \
             link num endereço novo entra às cegas"
        );
    }

    #[test]
    fn sem_nenhuma_das_duas_nao_ha_o_que_conferir() {
        assert_eq!(
            impressao_a_conferir(None, None),
            None,
            "sem link e sem guardada apareceu uma impressão do nada: a conexão \
             recusaria o servidor por uma promessa que ninguém fez"
        );
    }
}

/// A impressão que a lista de servidores guarda depois de uma conexão.
///
/// Recebe as mesmas duas entradas de [`impressao_a_conferir`], o link desta
/// sessão e a guardada: num [`Trust::Known`], o que vai para a lista é a
/// impressão que se conferiu, e ela sai de [`impressao_a_conferir`] com as
/// mesmas duas, na mesma ordem.
///
/// # Por que não é a do link
///
/// Era a do link desta sessão, qualquer que fosse o veredito. Com um link que
/// discorda do pin ([`Trust::InviteDisagrees`]), a conexão fica de pé com a
/// chave fixada, que é a decisão do ADR 0003: o pin prova a continuidade, e
/// quem discorda dele é o link. A lista, porém, ficava com a chave do link,
/// que é a de **outro** servidor. A guardada é a impressão que a volta pela
/// lista confere ([`impressao_a_conferir`]), e com ela o servidor verdadeiro
/// passava a ser recusado em todo endereço sem pin: a porta nova do NAT, o
/// endereço que o quarto devolve.
///
/// # O que volta
///
/// A chave que a conexão aceitou, quando alguém a conferiu:
///
/// - [`Trust::FirstContactVerified`]: a conferida no aperto de mão, pelo link
///   desta sessão ou pela guardada;
/// - [`Trust::InviteDisagrees`]: a ofertada, que é a fixada num alvo de escopo
///   público, com ou sem link nesta sessão. Sem link, quem discorda do pin é a
///   guardada, e é assim que uma lista gravada pela 0.15.0 (com a impressão de
///   um link que discordava do pin) se cura na primeira volta por um alvo
///   público;
/// - [`Trust::Known`]: a impressão que se conferiu, venha do link desta sessão
///   ou da lista ([`impressao_a_conferir`]), quando há uma, porque é ela
///   concordar com o pin que faz o veredito ser `Known`.
///
/// `None` quando não há nada conferido a guardar, e `None` deixa a lista como
/// estava (`Conhecidos::anotar_caminhos` não apaga a impressão). É o caso de
/// [`Trust::Known`] sem impressão conferida; de [`Trust::FirstContact`], em que
/// a chave foi fixada às cegas e ninguém a prometeu; e de
/// [`Trust::InviteRefused`], que não atravessa.
///
/// # A invariante de que isso depende
///
/// **`InviteDisagrees` só vem do alvo, e só num alvo de escopo público — ver
/// `TofuVerifier`** (`seele_core::tofu::TofuVerifier::decide`). O pin é por
/// endereço de candidato e prova só aquele endereço. Num alvo de escopo
/// público, o endereço que a pessoa escolheu e que é o mesmo em qualquer rede,
/// ele prova a continuidade do servidor da entrada, e a ofertada é a chave
/// dele. Num candidato que ninguém escolheu (a resposta do quarto, um caminho
/// da lista) e num alvo de escopo local (um endereço de LAN, que é o mesmo de
/// uma casa para outra), o pin pode ser de outro servidor, e ali o verificador
/// recusa dentro do TLS a esperada que não confere: o caso chega como
/// [`ConnectionError::InviteMismatch`], e nunca aqui. Quem decide onde o pin
/// prova o servidor é `build_destino`, e só ele
/// (`seele_core::enlace::Destino::o_pino_prova_o_servidor`). Se
/// `InviteDisagrees` voltasse a nascer onde o pin não prova o servidor, gravar
/// a ofertada poria na entrada a chave de outro servidor, e a volta seguinte
/// conferiria por ela, calada.
#[must_use]
pub fn impressao_a_guardar(
    veredito: &Trust,
    do_link: Option<&str>,
    guardada: Option<&str>,
) -> Option<String> {
    match veredito {
        Trust::FirstContactVerified { fingerprint } => Some(fingerprint.clone()),
        // **A ofertada, e não a esperada, com ou sem link.** A conexão ficou de
        // pé com ela, e ela é a fixada num alvo de escopo público: ver «A
        // invariante de que isso depende».
        Trust::InviteDisagrees { offered, .. } => Some(offered.clone()),
        Trust::Known => impressao_a_conferir(do_link, guardada),
        // Fixada às cegas: guardá-la faria a volta por outro endereço conferir
        // contra uma chave que ninguém prometeu. Fica como na 0.15.0, em que
        // este caso nunca tinha link.
        Trust::FirstContact { .. } => None,
        // Não atravessa: o núcleo recusa este caso no TLS, e ele chega como
        // `ConnectionError::InviteMismatch`. O braço mantém o `match` exaustivo.
        Trust::InviteRefused { .. } => None,
    }
}

#[cfg(test)]
mod a_lista_guarda_a_impressao_aceita {
    use super::{impressao_a_guardar, Trust};

    const DO_SERVIDOR: &str = "aaaa1111";
    const DE_OUTRO: &str = "bbbb2222";

    #[test]
    fn um_link_que_discorda_do_pin_nao_vai_para_a_lista() {
        // A conexão ficou de pé com a chave fixada (ADR 0003), e é ela que vale
        // guardar. A do link é de outro servidor.
        let veredito = Trust::InviteDisagrees {
            expected: DE_OUTRO.into(),
            offered: DO_SERVIDOR.into(),
        };
        assert_eq!(
            impressao_a_guardar(&veredito, Some(DE_OUTRO), None),
            Some(DO_SERVIDOR.to_owned()),
            "a lista guardou a impressão do link que discordava do pin: a volta pela \
             lista passa a esperar a chave de outro servidor e recusa o verdadeiro em \
             todo endereço sem pin"
        );
    }

    #[test]
    fn a_volta_pelo_alvo_que_discorda_do_pin_grava_a_ofertada() {
        // Sem link, quem discorda do pin é a guardada. `InviteDisagrees` só nasce
        // num alvo de escopo público (`seele_core::tofu::TofuVerifier::decide`,
        // com o pino que `build_destino` diz provar o servidor): ali o pin prova
        // a continuidade do servidor da entrada (ADR 0003), e a ofertada é a
        // chave dele. É assim que uma lista gravada pela 0.15.0, com a impressão
        // de um link que discordava do pin, se cura na primeira volta pelo alvo.
        // Num alvo de LAN, a mesma volta é recusada dentro do TLS
        // (`num_alvo_de_lan_o_pino_que_confere_nao_passa_por_cima_da_esperada`).
        let veredito = Trust::InviteDisagrees {
            expected: DE_OUTRO.into(),
            offered: DO_SERVIDOR.into(),
        };
        assert_eq!(
            impressao_a_guardar(&veredito, None, Some(DE_OUTRO)),
            Some(DO_SERVIDOR.to_owned()),
            "a volta pelo alvo, onde o pin prova o servidor, deixou na lista a impressão que \
             o pin desmente: uma lista envenenada pela 0.15.0 continua recusando o servidor \
             verdadeiro em todo endereço sem pin"
        );
    }

    #[test]
    fn o_que_foi_conferido_vai_para_a_lista() {
        let conferido = Trust::FirstContactVerified {
            fingerprint: DO_SERVIDOR.into(),
        };
        assert_eq!(
            impressao_a_guardar(&conferido, Some(DO_SERVIDOR), None),
            Some(DO_SERVIDOR.to_owned()),
            "o primeiro contato conferido pelo link não foi para a lista"
        );
        // O que o app passa numa volta pela lista: sem link, com a guardada.
        assert_eq!(
            impressao_a_guardar(&conferido, None, Some(DO_SERVIDOR)),
            Some(DO_SERVIDOR.to_owned()),
            "o primeiro contato conferido pela guardada, num endereço novo, não foi \
             para a lista"
        );
        assert_eq!(
            impressao_a_guardar(&Trust::Known, Some(DO_SERVIDOR), None),
            Some(DO_SERVIDOR.to_owned()),
            "a impressão do link que concorda com o pin não foi para a lista"
        );
        assert_eq!(
            impressao_a_guardar(&Trust::Known, None, Some(DO_SERVIDOR)),
            Some(DO_SERVIDOR.to_owned()),
            "a guardada que concorda com o pin, numa volta pela lista, deixou de ser \
             regravada, e a lista fica com uma impressão que ninguém confirmou hoje"
        );
    }

    #[test]
    fn o_que_ninguem_conferiu_deixa_a_lista_como_estava() {
        assert_eq!(
            impressao_a_guardar(&Trust::Known, None, None),
            None,
            "a volta sem impressão conferida inventou uma impressão para a lista"
        );
        assert_eq!(
            impressao_a_guardar(
                &Trust::FirstContact {
                    fingerprint: DO_SERVIDOR.into()
                },
                None,
                None
            ),
            None,
            "um primeiro contato cego foi para a lista como se tivesse sido \
             conferido: a volta por outro endereço passaria a conferir contra uma \
             chave que ninguém prometeu"
        );
        assert_eq!(
            impressao_a_guardar(
                &Trust::InviteRefused {
                    expected: DE_OUTRO.into(),
                    offered: DO_SERVIDOR.into()
                },
                Some(DE_OUTRO),
                None
            ),
            None,
            "uma recusa deixou impressão na lista"
        );
    }
}

/// Se `endereco` já é um candidato desta conexão: o alvo ou um dos
/// alternativos.
///
/// É a pergunta que o `connect` do app faz antes de pôr na frente da corrida o
/// endereço que o quarto devolveu. Ela só olhava os alternativos, e não o
/// alvo: quando o quarto devolvia o próprio endereço da entrada, a corrida
/// tinha dois candidatos com a mesma chave de pino, e o mesmo servidor
/// recebia dois apertos de mão e dois `Hello`. É o caso em que um candidato
/// perdedor pode desfazer o pin que o vencedor acabou de fixar
/// (`seele_core::enlace`, `desfazer_o_pin_deste_aperto`), e ele deixava de
/// precisar de dois endereços no convite para acontecer.
///
/// A comparação é pela chave de pino ([`chave_do_servidor`]) dos dois lados,
/// e não pelo texto: é ela que diz se dois candidatos são o mesmo endereço, e
/// `192.168.50.30` e `192.168.50.30:8383` são. Um `endereco` sem chave nunca
/// seria candidato (a conexão descarta o alternativo que não resolve), e
/// devolve `false`.
#[must_use]
pub fn ja_esta_na_corrida(endereco: &str, alvo: &str, alternativos: &[String]) -> bool {
    let Some(chave) = chave_do_servidor(endereco) else {
        return false;
    };
    std::iter::once(alvo)
        .chain(alternativos.iter().map(String::as_str))
        .any(|candidato| chave_do_servidor(candidato).as_deref() == Some(chave.as_str()))
}

#[cfg(test)]
mod a_resposta_do_quarto_nao_repete_um_candidato {
    use super::ja_esta_na_corrida;

    #[test]
    fn o_proprio_alvo_ja_esta_na_corrida() {
        assert!(
            ja_esta_na_corrida("187.255.97.152:9368", "187.255.97.152:9368", &[]),
            "o endereço que o quarto devolveu é o próprio alvo e entrou de novo na corrida: \
             dois candidatos com a mesma chave de pino, e dois `Hello` para o mesmo servidor"
        );
    }

    #[test]
    fn a_comparacao_e_pela_chave_de_pino_e_nao_pelo_texto() {
        assert!(
            ja_esta_na_corrida("192.168.50.30:8383", "192.168.50.30", &[]),
            "o alvo escrito sem a porta padrão não foi reconhecido no endereço que o quarto \
             devolveu: a chave de pino dos dois é a mesma"
        );
        assert!(
            ja_esta_na_corrida(
                "[2001:db8::1]:8383",
                "casa.exemplo:8383",
                &["[2001:db8::1]".to_owned()]
            ),
            "um alternativo escrito sem a porta padrão não foi reconhecido no endereço que o \
             quarto devolveu: a chave de pino dos dois é a mesma"
        );
    }

    #[test]
    fn um_endereco_novo_entra_na_corrida() {
        assert!(
            !ja_esta_na_corrida(
                "187.255.97.152:9400",
                "187.255.97.152:9368",
                &["192.168.50.30:8383".to_owned()]
            ),
            "a porta nova que o NAT deu ao servidor foi tratada como candidato repetido, e a \
             resposta do quarto não entra na corrida"
        );
    }
}

#[cfg(test)]
mod a_abertura_do_audio {
    //! A espera de [`esperar_o_audio`], pelo que ela **faz** com o relógio.
    //!
    //! A abertura acontece dentro do laço de `drive`, que roda num runtime de
    //! uma thread só. Uma abertura síncrona presa no driver seguraria a conexão
    //! inteira — comandos, mensagens, a telemetria — pelo tempo que o driver
    //! quisesse. Estes testes não abrem aparelho nenhum: o abridor é uma função
    //! que dorme, e o que se afirma é quanto a espera leva e o que ela devolve.

    use super::{esperar_o_audio, Atraso};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Uma vez do driver nova, de uma sessão só deste teste.
    fn vez() -> Arc<Mutex<()>> {
        Arc::new(Mutex::new(()))
    }

    /// Um abridor que conta quantas aberturas estão no driver ao mesmo tempo, e
    /// quantas aconteceram.
    #[derive(Default)]
    struct Driver {
        em_voo: AtomicUsize,
        no_maximo: AtomicUsize,
        aberturas: AtomicUsize,
    }

    impl Driver {
        fn abrir(&self, demora: Duration) -> &'static str {
            self.aberturas.fetch_add(1, Ordering::SeqCst);
            let agora = self.em_voo.fetch_add(1, Ordering::SeqCst) + 1;
            self.no_maximo.fetch_max(agora, Ordering::SeqCst);
            std::thread::sleep(demora);
            self.em_voo.fetch_sub(1, Ordering::SeqCst);
            "a voz"
        }
    }

    /// M-c: duas aberturas da mesma sessão nunca ficam no driver ao mesmo
    /// tempo. Entrar, sair e entrar de novo depressa larga a primeira, e a
    /// thread dela segue no driver; a segunda espera a vez.
    #[tokio::test(flavor = "multi_thread")]
    async fn duas_aberturas_da_mesma_sessao_nunca_ficam_no_driver_ao_mesmo_tempo() {
        let vez = vez();
        let driver = Arc::new(Driver::default());
        let (primeiro, segundo) = (Arc::clone(&driver), Arc::clone(&driver));

        let (a, b) = tokio::join!(
            esperar_o_audio(
                &vez,
                move || primeiro.abrir(Duration::from_millis(150)),
                Duration::from_secs(5)
            ),
            esperar_o_audio(
                &vez,
                move || segundo.abrir(Duration::from_millis(150)),
                Duration::from_secs(5)
            ),
        );

        assert!(
            a.is_ok() && b.is_ok(),
            "as duas aberturas tinham tempo de sobra"
        );
        assert_eq!(
            driver.no_maximo.load(Ordering::SeqCst),
            1,
            "duas aberturas da mesma sessão ficaram no driver ao mesmo tempo"
        );
    }

    /// E a abertura de que ninguém espera mais desiste quando a vez chega, sem
    /// abrir aparelho nenhum.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_abertura_que_ja_desistiu_nao_abre_quando_chega_a_vez() {
        let vez = vez();
        let driver = Arc::new(Driver::default());
        let (primeiro, segundo) = (Arc::clone(&driver), Arc::clone(&driver));

        let (a, b) = tokio::join!(
            esperar_o_audio(
                &vez,
                move || primeiro.abrir(Duration::from_millis(300)),
                Duration::from_secs(5)
            ),
            async {
                // A segunda pede depois de a primeira tomar a vez, e desiste
                // antes de a primeira terminar.
                tokio::time::sleep(Duration::from_millis(50)).await;
                esperar_o_audio(
                    &vez,
                    move || segundo.abrir(Duration::from_millis(10)),
                    Duration::from_millis(50),
                )
                .await
            },
        );
        assert!(a.is_ok());
        assert_eq!(b, Err(Atraso));

        // Dá tempo de a vez chegar à segunda thread.
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            driver.aberturas.load(Ordering::SeqCst),
            1,
            "a abertura de que ninguém esperava mais abriu o aparelho quando chegou a vez"
        );
    }

    /// (d) da pendência 52: um driver preso não prende quem espera.
    ///
    /// A asserção de tempo vem antes da do valor, e é de propósito: a versão
    /// síncrona — abrir na própria thread de quem espera — leva os dois
    /// segundos inteiros e devolve a voz, e é o relógio que a reprova, sem
    /// pendurar a bateria.
    #[tokio::test]
    async fn um_audio_que_nao_abre_no_prazo_volta_atrasado_sem_esperar_o_driver() {
        let comeco = Instant::now();
        let veio = esperar_o_audio(
            &vez(),
            || {
                std::thread::sleep(Duration::from_secs(2));
                "a voz"
            },
            Duration::from_millis(200),
        )
        .await;
        let levou = comeco.elapsed();

        assert!(
            levou < Duration::from_secs(1),
            "a espera pelo áudio levou {levou:?} com um prazo de 200 ms: ela esperou o \
             driver, e um driver preso prende a conexão inteira junto"
        );
        assert_eq!(
            veio,
            Err(Atraso),
            "o prazo venceu e a espera não disse que o áudio não veio"
        );
    }

    #[tokio::test]
    async fn a_voz_que_abre_no_prazo_chega_inteira_e_pela_thread_com_nome() {
        let veio = esperar_o_audio(
            &vez(),
            || std::thread::current().name().map(str::to_owned),
            Duration::from_secs(5),
        )
        .await;

        assert_eq!(
            veio,
            Ok(Some("seele-abre-audio".to_owned())),
            "a abertura não correu na thread própria com nome: um pânico ali sai \
             no log sem dizer de onde"
        );
    }

    /// Uma voz que se larga, avisando que foi largada.
    struct Aparelho(Arc<AtomicBool>);

    impl Drop for Aparelho {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// A voz que chega depois do prazo é largada na thread que a abriu.
    ///
    /// É o `drop` que fecha o aparelho. Uma voz tardia guardada em algum lugar
    /// seria um microfone aberto para uma sala que já desistiu dele.
    #[tokio::test]
    async fn a_voz_que_chega_tarde_e_largada_na_thread_que_a_abriu() {
        let largada = Arc::new(AtomicBool::new(false));
        let marca = Arc::clone(&largada);
        let veio = esperar_o_audio(
            &vez(),
            move || {
                std::thread::sleep(Duration::from_millis(300));
                Aparelho(marca)
            },
            Duration::from_millis(50),
        )
        .await;
        assert!(
            veio.is_err(),
            "o prazo de 50 ms venceu e a voz veio assim mesmo"
        );

        let ate = Instant::now() + Duration::from_secs(5);
        while !largada.load(Ordering::SeqCst) && Instant::now() < ate {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            largada.load(Ordering::SeqCst),
            "a voz que chegou depois do prazo não foi largada: o aparelho fica aberto \
             para uma sala que já seguiu sem ele"
        );
    }
}

#[cfg(test)]
mod o_audio_segue_a_sala {
    //! A pendência 52: o áudio abre quando a pessoa passa a estar numa sala de
    //! voz, e fecha quando ela deixa de estar.
    //!
    //! **A prova é da decisão, e nenhum aparelho abre aqui.** A voz é de
    //! mentira e o abridor é injetado, no molde dos testes de
    //! `reabrir_voz_na_reconexao`: `PortaDaVoz` e `VozDaSala` são genéricas na
    //! voz justamente para isso, e `drive` usa as mesmas com a [`Voice`] de
    //! verdade.
    //!
    //! [`Voice`]: seele_core::Voice

    use super::{
        readotar_na_volta, sala_do_audio, voice_rooms_of, Aberta, AberturaDoAudio,
        ControlesGuardados, Futuro, PortaDaVoz, VozDaSala, VozViva,
    };
    use seele_core::{
        AlertReason, AlertSeverity, DeviceChoice, PersonId, PersonProfile, Room, ServerMessage,
        SessionId, SessionInfo, Ssrc, VoiceMode, VoiceRoomId, VoiceRoomInfo,
    };
    use std::sync::Mutex;
    use std::time::Duration;

    const SALA: VoiceRoomId = VoiceRoomId(1);

    /// Uma voz de mentira, que lembra o que vestiu ao abrir.
    #[derive(Debug, Default)]
    struct Voz {
        ssrc: u32,
        vestida: Mutex<Option<ControlesGuardados>>,
    }

    impl VozViva for Voz {
        fn vestir(&self, escolhida: &ControlesGuardados) {
            *self.vestida.lock().expect("o que a voz vestiu") = Some(escolhida.clone());
        }
    }

    fn voz_de(ssrc: u32) -> Voz {
        Voz {
            ssrc,
            vestida: Mutex::new(None),
        }
    }

    fn entregue(aberta: Aberta<Voz>) -> Futuro<Voz> {
        Box::pin(async move { aberta })
    }

    fn nada_abre(_: &DeviceChoice) -> Futuro<Voz> {
        panic!("o áudio abriu num ponto em que nada devia abrir")
    }

    /// O que `drive` faz no fim de cada volta do laço: olha o assento, e, se
    /// uma abertura começou, espera a chegada dela como o braço do `select!`.
    ///
    /// Devolve quantas vezes o abridor foi chamado.
    async fn olhar_e_esperar(
        porta: &mut PortaDaVoz<Voz>,
        voz: &VozDaSala<Voz>,
        room: &Room,
        ssrc: u32,
    ) -> u32 {
        let mut aberturas = 0;
        // O assento pelo qual o áudio abre, como `sala_de` o lê em `drive`.
        porta.olhar(voz, sala_do_audio(room), |_| {
            aberturas += 1;
            entregue(Ok(Ok(voz_de(ssrc))))
        });
        if voz.abertura() == AberturaDoAudio::Abrindo {
            let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
                .await
                .expect("a abertura começou e não chegou");
            porta.chegou(voz, aberta, nada_abre);
        }
        aberturas
    }

    fn ha_voz(voz: &VozDaSala<Voz>) -> bool {
        voz.lock().expect("a voz").is_some()
    }

    /// O servidor decidiu o pedido de entrada que estava no ar: o `Pong` do
    /// `Ping` marcado que o motor manda atrás de cada pedido.
    fn decidida(room: &mut Room) {
        room.apply(&ServerMessage::Pong {
            // O `Pong` de um pedido mais novo que todos os destes testes: fecha
            // o que está no ar, como o `Pong` do pedido que acabou de sair.
            timestamp: seele_core::assento::carimbo_da_entrada(u64::from(u32::MAX)),
        });
    }

    fn recusa(reason: AlertReason) -> ServerMessage {
        ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason,
            operator_text: None,
        }
    }

    /// (a) Fora de sala, nenhum aparelho aberto; entrar numa sala abre, sair
    /// fecha.
    ///
    /// O primeiro olhar é o de `drive` logo depois do aperto de mão: o servidor
    /// adotado e ninguém sentado. Antes da pendência 52, era ali que a voz abria,
    /// para a sessão inteira.
    #[tokio::test]
    async fn fora_de_sala_nenhum_aparelho_abre_e_a_sala_abre_e_fecha() {
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        let mut room = Room::new();
        room.adopt(&sessao(700), "marcela");
        room.apply(&lia_na_sala());

        let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 700).await;
        assert_eq!(
            aberturas, 0,
            "entrar no servidor abriu o áudio: quem fica só pela conversa por \
             escrito segura o microfone e o repouso da máquina a sessão inteira"
        );
        assert!(!ha_voz(&voz), "há voz de pé fora de sala");
        assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);

        room.enter_voice_room(SALA);
        let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 700).await;
        assert_eq!(
            aberturas, 0,
            "o áudio abriu com a entrada ainda no ar: o microfone abre numa sala que \
             ainda pode recusar (M-d)"
        );
        assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);

        decidida(&mut room);
        let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 700).await;
        assert_eq!(aberturas, 1, "entrar numa sala não abriu o áudio");
        assert!(ha_voz(&voz), "a voz que abriu não ficou no lugar");
        assert_eq!(voz.abertura(), AberturaDoAudio::Aberto);

        room.leave_voice_room();
        let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 700).await;
        assert_eq!(aberturas, 0);
        assert!(
            !ha_voz(&voz),
            "sair da sala não fechou o áudio: o microfone continua aberto para \
             sala nenhuma"
        );
        assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);
    }

    /// Os cinco caminhos para dentro e para fora de uma sala, um depois do outro.
    ///
    /// Todos são aplicados no `seele-core` — `Room::apply` e os dois métodos de
    /// assento, pela porta única de `seele_core::assento` —, e a porta da voz só
    /// vê o efeito no assento confirmado. É o que este teste prende: um caminho
    /// novo que esquecesse o assento seria pego aqui, e não na tela de alguém com
    /// o microfone aberto fora de sala.
    ///
    /// Cada pedido de entrada vem com a resposta que o servidor dá a ele, na
    /// ordem em que ela chega: a recusa, quando há, e o `Pong` do `Ping`
    /// marcado. E entram as três recusas que **não** tiram ninguém da sala
    /// (C1 da rodada 1): a troca recusada, a troca para uma sala lotada e a
    /// recusa sem pedido no ar, que é o apelido tomado.
    #[tokio::test]
    async fn o_audio_abre_e_fecha_pelos_cinco_caminhos_da_sala() {
        const OUTRA: VoiceRoomId = VoiceRoomId(2);
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        let mut room = Room::new();
        room.adopt(&sessao(700), "marcela");

        /// O nome do caminho, o que ele faz com o assento, e se a voz fica
        /// aberta depois dele.
        type Caminho = (&'static str, Box<dyn Fn(&mut Room)>, bool);
        let caminhos: Vec<Caminho> = vec![
            (
                "a entrada pedida e aceita",
                Box::new(|room| {
                    room.enter_voice_room(SALA);
                    decidida(room);
                }),
                true,
            ),
            ("a saída pedida", Box::new(Room::leave_voice_room), false),
            (
                "o operador que move para a sala",
                Box::new(|room| {
                    room.apply(&ServerMessage::MovedToVoiceRoom { voice_room: SALA });
                }),
                true,
            ),
            (
                "a troca para outra sala, recusada",
                Box::new(|room| {
                    room.enter_voice_room(OUTRA);
                    room.apply(&recusa(AlertReason::VoiceRoomEntryRefused));
                    decidida(room);
                }),
                true,
            ),
            (
                "a troca para outra sala, lotada",
                Box::new(|room| {
                    room.enter_voice_room(OUTRA);
                    room.apply(&recusa(AlertReason::VoiceRoomFull));
                    decidida(room);
                }),
                true,
            ),
            (
                "uma recusa sem pedido no ar (o apelido tomado)",
                Box::new(|room| {
                    room.apply(&recusa(AlertReason::VoiceRoomEntryRefused));
                }),
                true,
            ),
            (
                "a saída pedida de novo",
                Box::new(Room::leave_voice_room),
                false,
            ),
            (
                "a entrada recusada de quem estava fora",
                Box::new(|room| {
                    room.enter_voice_room(SALA);
                    room.apply(&recusa(AlertReason::VoiceRoomEntryRefused));
                    decidida(room);
                }),
                false,
            ),
            (
                "a entrada lotada de quem estava fora",
                Box::new(|room| {
                    room.enter_voice_room(SALA);
                    room.apply(&recusa(AlertReason::VoiceRoomFull));
                    decidida(room);
                }),
                false,
            ),
            (
                "a entrada pedida e aceita mais uma vez",
                Box::new(|room| {
                    room.enter_voice_room(SALA);
                    decidida(room);
                }),
                true,
            ),
            (
                "a sala apagada",
                Box::new(|room| {
                    room.apply(&ServerMessage::VoiceRoomDeleted { voice_room: SALA });
                }),
                false,
            ),
        ];

        for (caminho, andar, dentro) in caminhos {
            andar(&mut room);
            olhar_e_esperar(&mut porta, &voz, &room, 700).await;
            assert_eq!(
                ha_voz(&voz),
                dentro,
                "depois de «{caminho}» a voz está {}: o áudio não segue a sala por \
                 este caminho",
                if ha_voz(&voz) { "aberta" } else { "fechada" }
            );
        }
    }

    /// (b) O que se escolhe fora de sala vale ao entrar: a voz abre muda.
    ///
    /// `escolher` é o que `Connection::set_muted` e os irmãos chamam — ver
    /// `fora_de_sala_a_conexao_guarda_os_controles_e_o_retrato_os_mostra`.
    #[tokio::test]
    async fn os_controles_escolhidos_fora_de_sala_valem_ao_entrar() {
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        let mut room = Room::new();
        room.adopt(&sessao(700), "marcela");
        olhar_e_esperar(&mut porta, &voz, &room, 700).await;

        voz.escolher(|escolhida| escolhida.mudo = true);
        voz.escolher(|escolhida| escolhida.isolamento = true);
        voz.escolher(|escolhida| escolhida.modo = VoiceMode::VoiceActivated);
        voz.escolher(|escolhida| escolhida.supressao = 0.25);
        voz.escolher(|escolhida| escolhida.abertura_dbfs = Some(-48.0));

        room.enter_voice_room(SALA);
        decidida(&mut room);
        olhar_e_esperar(&mut porta, &voz, &room, 700).await;

        let viva = voz.lock().expect("a voz");
        let vestida = viva
            .as_ref()
            .and_then(|viva| viva.vestida.lock().expect("o que a voz vestiu").clone());
        let Some(vestida) = vestida else {
            panic!(
                "a voz abriu sem vestir o que a pessoa escolheu fora de sala: ela se \
                 mutou antes de entrar, o roster a mostra muda, e o microfone está aberto"
            );
        };
        assert!(
            vestida.mudo,
            "a voz abriu com o microfone aberto para quem se mutou fora de sala, \
             enquanto o roster a mostra muda"
        );
        assert!(
            vestida.isolamento,
            "o Isolamento total pedido fora de sala se perdeu"
        );
        assert_eq!(vestida.modo, VoiceMode::VoiceActivated);
        assert!((vestida.supressao - 0.25).abs() < f32::EPSILON);
        assert_eq!(vestida.abertura_dbfs, Some(-48.0));
    }

    /// (c) A volta de quem está numa sala reabre a voz, e a de quem está fora
    /// não abre nada.
    ///
    /// Na ordem do braço `Aviso::Reconectado` de `drive`: o `Room` volta com o
    /// assento que o motor restabeleceu, a porta reabre a voz viva, e o olhar do
    /// fim da volta não pode desfazer nada disso.
    #[tokio::test]
    async fn a_volta_de_quem_esta_na_sala_reabre_a_voz_e_a_de_quem_esta_fora_nao_abre() {
        // Na sala, com a voz de pé.
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        let mut room = Room::new();
        room.adopt(&sessao(700), "marcela");
        room.enter_voice_room(SALA);
        decidida(&mut room);
        olhar_e_esperar(&mut porta, &voz, &room, 700).await;
        assert!(ha_voz(&voz));

        // A volta dentro da carência: o servidor devolve o assento e o `ssrc`
        // (o mesmo 700, numa sessão nova), e o motor refaz a entrada. O canal de mídia é novo, e
        // a voz reabre nele.
        readotar_na_volta(&mut room, &sessao_numero(700, 701), "marcela", Some(SALA));
        porta.na_volta(&voz, |_velha| entregue(Ok(Ok(voz_de(701)))), nada_abre);
        olhar_e_esperar(&mut porta, &voz, &room, 701).await;

        assert_eq!(
            voz.lock().expect("a voz").as_ref().map(|viva| viva.ssrc),
            Some(701),
            "quem caiu e voltou numa sala ficou sem a voz da conexão nova: o \
             motor a sentou de novo, e o áudio fechou como se ela tivesse saído"
        );
        assert_eq!(voz.abertura(), AberturaDoAudio::Aberto);

        // M3: a sala tem senha, e a entrada refeita sem ela é recusada. O
        // servidor manteve a pessoa sentada, e a voz continua.
        room.apply(&recusa(AlertReason::VoiceRoomEntryRefused));
        decidida(&mut room);
        olhar_e_esperar(&mut porta, &voz, &room, 701).await;
        assert!(
            ha_voz(&voz),
            "M3: a entrada refeita numa sala com senha foi recusada, o servidor manteve a \
             pessoa sentada, e o áudio fechou: surda e muda numa sala em que todos a veem"
        );
        assert_eq!(room.current_voice_room, Some(SALA));

        // Fora de sala, sem voz: a volta não abre aparelho nenhum.
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        let mut room = Room::new();
        room.adopt(&sessao(700), "marcela");
        olhar_e_esperar(&mut porta, &voz, &room, 700).await;

        readotar_na_volta(&mut room, &sessao(701), "marcela", None);
        porta.na_volta(&voz, |_| panic!("não havia voz para reabrir"), nada_abre);
        let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 701).await;

        assert_eq!(
            aberturas, 0,
            "a volta de quem estava fora de sala abriu o áudio"
        );
        assert!(!ha_voz(&voz));
        assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);
    }

    /// Uma abertura em curso na hora da volta abria sobre a conexão que caiu, e
    /// recomeça na nova.
    #[tokio::test]
    async fn a_abertura_em_curso_na_volta_recomeca_na_conexao_nova() {
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        porta.olhar(&voz, Some(SALA), |_| {
            Box::pin(std::future::pending::<Aberta<Voz>>())
        });
        assert_eq!(voz.abertura(), AberturaDoAudio::Abrindo);

        let mut recomecou = false;
        porta.na_volta(
            &voz,
            |_| panic!("não havia voz viva para reabrir"),
            |_| {
                recomecou = true;
                entregue(Ok(Ok(voz_de(701))))
            },
        );
        assert!(
            recomecou,
            "a abertura que corria sobre a conexão que caiu não recomeçou na nova"
        );
        let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
            .await
            .expect("a abertura recomeçada não chegou");
        porta.chegou(&voz, aberta, nada_abre);
        assert_eq!(
            voz.lock().expect("a voz").as_ref().map(|viva| viva.ssrc),
            Some(701)
        );
    }

    /// Um aparelho escolhido enquanto a voz abria vale: a abertura que chega com
    /// o pedido velho recomeça com o novo.
    #[tokio::test]
    async fn o_aparelho_escolhido_enquanto_a_voz_abre_nao_se_perde() {
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        porta.olhar(&voz, Some(SALA), |_| entregue(Ok(Ok(voz_de(1)))));

        voz.guardar_aparelho(super::Lado::Entrada, Some("fone".into()));

        let aberta = porta.chegada().await;
        let mut pedido = None;
        porta.chegou(&voz, aberta, |pedidos| {
            pedido = Some(pedidos.clone());
            entregue(Ok(Ok(voz_de(2))))
        });
        assert_eq!(
            pedido.and_then(|pedidos| pedidos.capture),
            Some("fone".to_owned()),
            "a voz que abriu no microfone de antes ficou no lugar: a tela diz \
             ESCOLHIDO num e EM USO noutro"
        );
        assert!(!ha_voz(&voz), "a voz do pedido velho ficou no lugar");
        let aberta = porta.chegada().await;
        porta.chegou(&voz, aberta, nada_abre);
        assert_eq!(
            voz.lock().expect("a voz").as_ref().map(|viva| viva.ssrc),
            Some(2)
        );
    }

    /// M-b: quem confere o aparelho é `instalar`, com o cadeado da voz preso.
    ///
    /// A corrida em si — uma troca entre a conferência e o cadeado — não tem
    /// como ser forçada num teste sem uma costura no meio. O que se prende aqui
    /// é que a conferência mora dentro de `instalar`: uma voz aberta com os
    /// aparelhos de antes volta no `Err`, e o lugar fica vazio.
    #[test]
    fn instalar_recusa_a_voz_aberta_com_os_aparelhos_de_antes() {
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let pedidos = voz.escolhida().aparelhos;
        voz.guardar_aparelho(super::Lado::Entrada, Some("fone".into()));

        let devolvida = voz.instalar(voz_de(1), &pedidos);

        assert!(
            devolvida.is_err(),
            "`instalar` pôs no lugar uma voz aberta no microfone de antes da escolha"
        );
        assert!(!ha_voz(&voz));
        assert_eq!(
            voz.instalar(voz_de(2), &voz.escolhida().aparelhos)
                .map_err(|_| ()),
            Ok(()),
            "a voz aberta com os aparelhos de agora não entrou no lugar"
        );
        assert!(ha_voz(&voz));
    }

    /// O prazo vencido e o aparelho recusado têm estados próprios, e a sala
    /// segue.
    #[tokio::test]
    async fn o_audio_que_nao_abre_deixa_o_estado_dito() {
        for (aberta, estado) in [
            (Err(super::Atraso), AberturaDoAudio::NaoAbriuNoPrazo),
            (
                Ok(Err("sem microfone".to_owned())),
                AberturaDoAudio::SemAparelho,
            ),
        ] {
            let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
            let mut porta = PortaDaVoz::nova();
            porta.olhar(&voz, Some(SALA), move |_| entregue(aberta));
            let chegou = porta.chegada().await;
            porta.chegou(&voz, chegou, nada_abre);

            assert!(!ha_voz(&voz));
            assert_eq!(
                voz.abertura(),
                estado,
                "o áudio não abriu e o estado não diz por quê: a tela fica sem a frase"
            );

            // E sair da sala volta a «fora de sala», sem frase nenhuma.
            porta.olhar(&voz, None, nada_abre);
            assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);
        }
    }

    /// Uma sessão que entrou sem áudio não abre aparelho em sala nenhuma.
    #[test]
    fn a_sessao_sem_audio_nao_abre_aparelho_na_sala() {
        let voz: VozDaSala<Voz> = VozDaSala::nova(false, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        porta.olhar(&voz, Some(SALA), nada_abre);
        assert_eq!(voz.abertura(), AberturaDoAudio::SemAparelho);
    }

    /// Um `Shared` sem voz de pé, de uma sessão que pediu áudio ou não.
    fn compartilhado(quer_audio: bool) -> std::sync::Arc<super::Shared> {
        use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8};
        std::sync::Arc::new(super::Shared {
            caminho_medido: AtomicU32::new(0),
            link_battery: AtomicBool::new(false),
            link_seconds: AtomicU64::new(0),
            link_attempts: AtomicU32::new(0),
            messages_revision: AtomicU64::new(0),
            icon_revision: AtomicU64::new(0),
            person_icons_revision: AtomicU64::new(0),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            voice: VozDaSala::nova(quer_audio, DeviceChoice::default()),
            media: Mutex::new(None),
            nickname: Mutex::new("marcela".into()),
            link_state: AtomicU8::new(0),
            rtt_micros: AtomicU64::new(0),
            jitter_de_chegada_micros: AtomicU64::new(0),
            caminho: Mutex::new(None),
            signal: AtomicU8::new(0),
            running: AtomicBool::new(true),
            pending_weights: Mutex::new(Vec::new()),
            limites_da_tela: Mutex::new(None),
            tela_escolhida: Mutex::new(super::EscolhaDeTela::NinguemEscolheu),
            fontes_de_tela: Mutex::new(Vec::new()),
        })
    }

    /// M-e: numa sessão que entrou sem áudio, `controles_da_voz` não responde,
    /// e a casca lê os controles do disco, como sempre leu.
    ///
    /// Os guardados da sessão são os padrões — nada os aplica a voz nenhuma, e
    /// nenhuma tela os muda —, e devolvê-los escondia da configuração a escolha
    /// gravada (`controles_da_voz`, em `main.rs`, cai no disco só com `None`).
    #[test]
    fn a_sessao_sem_audio_deixa_os_controles_da_voz_para_o_disco() {
        let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
        let connection = super::Connection {
            commands,
            shared: compartilhado(false),
            encerrada: Mutex::new(None),
        };
        assert_eq!(
            connection.controles_da_voz(),
            None,
            "a sessão sem áudio respondeu os controles com os padrões dela, e a tela de \
             configuração deixou de mostrar o que está gravado no disco"
        );
    }

    /// Fora de sala, cada controle da conexão é guardado, nenhum é recusado, e
    /// o retrato mostra o escolhido.
    ///
    /// Antes da pendência 52 a voz estava sempre de pé, e estes comandos falavam
    /// com ela. Sem ela: o mudo, o isolamento, o modo, a supressão e a
    /// sensibilidade pulavam calados; `controles_da_voz` devolvia `None`; e
    /// trocar o microfone devolvia `NoAudioDevice` — que a casca escreve como
    /// «o microfone sumiu» a quem só ainda não entrou numa sala.
    #[test]
    fn fora_de_sala_a_conexao_guarda_os_controles_e_o_retrato_os_mostra() {
        let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
        let connection = super::Connection {
            commands,
            shared: compartilhado(true),
            encerrada: Mutex::new(None),
        };

        connection.set_muted(true).expect("o mudo fora de sala");
        connection
            .set_total_isolation(true)
            .expect("o isolamento fora de sala");
        connection.set_voice_mode(crate::VoiceMode::VoiceActivated);
        connection.set_supressao_de_ruido(0.25);
        connection.set_abertura_da_voz_dbfs(Some(-48.0));
        assert_eq!(
            connection.set_capture_device(Some("fone".into())),
            Ok(()),
            "escolher o microfone fora de sala foi recusado: a casca diz que o \
             aparelho sumiu a quem só ainda não entrou numa sala"
        );
        assert_eq!(connection.set_playback_device(Some("caixa".into())), Ok(()));

        let retrato = connection.snapshot();
        assert!(
            retrato.muted && retrato.total_isolation,
            "o retrato esqueceu o mudo e o isolamento escolhidos fora de sala: o \
             botão volta solto na tela, e a voz abre sem eles"
        );
        assert_eq!(retrato.voice_mode, crate::VoiceMode::VoiceActivated);
        assert!(!retrato.audio_available);
        assert_eq!(retrato.abertura_do_audio, AberturaDoAudio::ForaDeSala);

        let Some(controles) = connection.controles_da_voz() else {
            panic!("fora de sala não há controles de voz para a tela desenhar");
        };
        assert!((controles.supressao - 0.25).abs() < f32::EPSILON);
        assert!((controles.abertura_dbfs + 48.0).abs() < f32::EPSILON);
        assert!(controles.abertura_escolhida);

        let escolhida = connection.shared.voice.escolhida();
        assert_eq!(escolhida.aparelhos.capture.as_deref(), Some("fone"));
        assert_eq!(escolhida.aparelhos.playback.as_deref(), Some("caixa"));
    }

    /// F1-microfone-macos: o silêncio digital atravessa a ponte na telemetria,
    /// pelo nome que a tela lê, e falso sem voz.
    ///
    /// A tela o lê como `snapshot.telemetry.microfone_entrega_silencio_digital`
    /// (`desenharAberturaDoAudio`). Um nome que mudasse deste lado calaria a
    /// frase do macOS sem erro em lugar nenhum: o campo chegaria `undefined`.
    #[test]
    fn o_silencio_digital_chega_na_telemetria_pelo_nome_que_a_tela_le() {
        let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
        let connection = super::Connection {
            commands,
            shared: compartilhado(true),
            encerrada: Mutex::new(None),
        };
        let retrato = serde_json::to_value(connection.snapshot()).expect("o retrato vira JSON");
        assert_eq!(
            retrato
                .get("telemetry")
                .and_then(|telemetria| telemetria.get("microfone_entrega_silencio_digital")),
            Some(&serde_json::Value::Bool(false)),
            "o retrato não leva `telemetry.microfone_entrega_silencio_digital`, ou o leva \
             verdadeiro sem voz nenhuma: a frase do macOS nunca acende, ou acende fora de \
             sala"
        );
    }

    /// E com a voz de pé, o retrato lê o silêncio digital da telemetria dela.
    ///
    /// Lido do fonte porque o caminho com voz pede uma [`Voice`] de verdade, e
    /// ela pede placa de som. Sem esta linha, um `false` escrito no lugar da
    /// leitura deixava o teste de cima verde e a frase do macOS apagada para
    /// sempre.
    ///
    /// [`Voice`]: seele_core::Voice
    #[test]
    fn o_retrato_com_voz_le_o_silencio_digital_da_telemetria_dela() {
        let fonte = include_str!("lib.rs");
        let programa = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
        let Some(corpo) = programa
            .split("fn audio_state(&self) -> AudioState {")
            .nth(1)
            .and_then(|resto| resto.split("\n    }\n").next())
        else {
            panic!("`Connection::audio_state` sumiu, e com ela o retrato da voz");
        };
        assert!(
            corpo.contains("silencio_digital: telemetry.local.microfone_entrega_silencio_digital"),
            "com a voz de pé, o retrato não lê o silêncio digital que o laço de voz \
             mediu: o produto sabe que o microfone entrega zero e não conta"
        );
    }

    /// O corpo de `drive`, sem as linhas de comentário, e os três pedaços que a
    /// ligação da porta da voz atravessa: antes do laço, o `select!`, e o resto
    /// da volta do laço depois dele.
    fn pedacos_de_drive() -> (String, String, String, String) {
        let fonte = include_str!("lib.rs");
        let sem_comentario: String = fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let inicio = sem_comentario
            .find(&format!("{} fn drive(", "async"))
            .expect("`drive` sumiu de `lib.rs`");
        let depois = &sem_comentario[inicio..];
        let corpo = &depois[..depois.find("\n}\n").expect("`drive` nunca fecha")];
        let (antes_do_laco, laco) = corpo
            .split_once("    while shared.running")
            .expect("o laço de `drive` mudou de forma, e este guarda tem de mudar com ele");
        let (_, select) = laco
            .split_once("tokio::select! {")
            .expect("o `select!` de `drive` sumiu");
        // O `select!` fecha na primeira linha com a indentação dele.
        let (select, resto_da_volta) = select
            .split_once("\n        }\n")
            .expect("o `select!` de `drive` não fecha na indentação de sempre");
        (
            corpo.to_owned(),
            antes_do_laco.to_owned(),
            select.to_owned(),
            resto_da_volta.to_owned(),
        )
    }

    /// I2 da rodada 1 do lote I-a: a porta da voz está ligada ao laço de
    /// `drive`, nos três lugares, e só nos três.
    ///
    /// Os testes acima provam a porta, e nenhum deles vê `drive`. A revisão
    /// desfez duas ligações e ficou tudo verde: sem o olhar do fim da volta do
    /// laço — o único ponto que abre o áudio ao entrar numa sala — e com um
    /// `porta.comecar` logo depois do olhar do aperto de mão, que abre o áudio
    /// ao conectar. Este guarda lê o texto de `drive` porque é ali que a ligação
    /// mora, e nenhum valor a exibe sem um enlace de verdade.
    #[test]
    fn a_porta_da_voz_esta_ligada_ao_laco_de_drive() {
        let (corpo, antes_do_laco, select, resto_da_volta) = pedacos_de_drive();
        let olhar = "porta.olhar(&shared.voice, sala_de(&shared),";

        assert_eq!(
            antes_do_laco.matches("porta.").count(),
            1,
            "antes do laço, `drive` mexe na porta da voz mais de uma vez — ou nenhuma: o \
             aperto de mão é um olhar só, com o servidor adotado e ninguém sentado, e \
             qualquer outra coisa ali abre o áudio ao conectar"
        );
        assert!(
            antes_do_laco.contains(olhar),
            "o olhar do aperto de mão sumiu, ou deixou de ler o assento confirmado"
        );
        assert!(
            select.contains("aberta = porta.chegada() =>"),
            "o `select!` de `drive` não espera mais a abertura em curso: a voz abre e nunca \
             chega ao lugar"
        );
        assert!(
            resto_da_volta.contains(olhar),
            "o olhar do fim da volta do laço sumiu: é o único ponto que abre o áudio ao \
             entrar numa sala e o fecha ao sair"
        );
        for chamada in corpo
            .match_indices("porta.")
            .map(|(onde, _)| &corpo[onde..])
        {
            assert!(
                [
                    "porta.olhar(",
                    "porta.na_volta(",
                    "porta.chegada()",
                    "porta.chegou("
                ]
                .iter()
                .any(|permitida| chamada.starts_with(permitida)),
                "`drive` chama a porta da voz por fora das quatro portas dela: {}",
                chamada.lines().next().unwrap_or_default()
            );
        }
        for (onde, _) in corpo.match_indices("abrir_a_voz(") {
            let antes = corpo[..onde].trim_end().trim_end_matches('{').trim_end();
            assert!(
                antes.ends_with("|pedidos|"),
                "`drive` chama `abrir_a_voz` fora de uma closure dada à porta: a voz abre \
                 sem a porta decidir"
            );
        }
    }

    /// Nenhum caminho abre a voz por fora da porta da sala.
    ///
    /// Os testes acima provam a decisão da porta, e nenhum deles vê `drive`:
    /// uma abertura escrita de volta logo depois do aperto de mão — onde ela
    /// morou até a 0.15 — passaria por todos. Este guarda conta, no programa e
    /// sem comentários, quem chama o núcleo para abrir uma voz: uma vez só, e
    /// dentro de `abrir_a_voz`, que só a porta chama. A reabertura da volta é
    /// `reopen`, que parte da voz que já estava aberta.
    #[test]
    fn a_voz_so_abre_pela_porta_da_sala() {
        let fonte = include_str!("lib.rs");
        let programa = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
        let sem_comentarios: String = programa
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let abertura = format!("Voice::{}(", "start_preferring");
        assert_eq!(
            sem_comentarios.matches(&abertura).count(),
            1,
            "a voz abre por fora de `abrir_a_voz`: uma abertura no aperto de mão \
             segura o microfone a sessão inteira, dentro de sala ou não"
        );
        for outra in [
            format!("Voice::{}(", "start"),
            format!("Voice::{}(", "start_on"),
        ] {
            assert!(
                !sem_comentarios.contains(&outra),
                "a FFI abre uma voz com `{outra}` por fora da porta da sala"
            );
        }
        let Some(abridor) = sem_comentarios
            .split(&format!("fn {}(", "abrir_a_voz"))
            .nth(1)
            .and_then(|resto| resto.split("\n}\n").next())
        else {
            panic!("`abrir_a_voz` sumiu, e com ela a única porta da voz");
        };
        assert!(
            abridor.contains(&abertura),
            "a única abertura da voz não está em `abrir_a_voz`"
        );
    }

    /// A sessão que o aperto de mão entrega. O `ssrc` muda a cada conexão.
    fn sessao(ssrc: u32) -> SessionInfo {
        sessao_numero(ssrc, u64::from(ssrc))
    }

    /// A sessão com o número que o teste escolhe. O resgate de assento é o
    /// mesmo `ssrc` numa sessão de número maior.
    fn sessao_numero(ssrc: u32, numero: u64) -> SessionInfo {
        SessionInfo {
            id: SessionId(numero),
            person: PersonId(7),
            ssrc: Ssrc(ssrc),
            server: "Casa".into(),
            voice_rooms: vec![VoiceRoomInfo {
                id: SALA,
                name: "Sala".into(),
                limit: 0,
                password_required: false,
                channel: None,
            }],
            channels: Vec::new(),
            permissions: Vec::new(),
        }
    }

    /// A Lia sentada na sala, do jeito que o servidor a apresenta.
    fn lia_na_sala() -> ServerMessage {
        ServerMessage::PersonJoined {
            voice_room: SALA,
            profile: PersonProfile {
                id: PersonId(9),
                nickname: "lia".into(),
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(900),
        }
    }

    /// (c), a medida: o que a FFI vê depois de uma volta de quem estava numa sala.
    ///
    /// A volta é a do braço `Aviso::Reconectado` de `drive`: a sessão nova é
    /// adotada, e o servidor reapresenta quem está sentado — e nunca a própria
    /// pessoa (`seele-server`, `session.rs`, pula `session.person` na
    /// fotografia de ocupação). Enquanto isso o motor do enlace **já** mandou
    /// `EnterVoiceRoom` pela conexão nova (`restabelecer`, em `enlace.rs`), e o
    /// servidor a sentou: a voz dela está na sala.
    ///
    /// `Some(SALA)` é o que `Aviso::Reconectado` traz: o assento que o motor
    /// restabeleceu.
    #[test]
    fn a_volta_mostra_a_pessoa_na_sala_em_que_o_motor_a_sentou_de_novo() {
        let mut room = Room::new();
        room.adopt(&sessao(700), "marcela");
        room.apply(&lia_na_sala());
        room.enter_voice_room(SALA);

        readotar_na_volta(&mut room, &sessao(701), "marcela", Some(SALA));
        room.apply(&lia_na_sala());

        let salas = voice_rooms_of(&room);
        assert_eq!(
            room.current_voice_room,
            Some(SALA),
            "depois da volta a FFI tira a pessoa da sala em que o motor a sentou de \
             novo: a tela a desenha FORA DE SALA enquanto a voz dela está lá, e um \
             gatilho do áudio lido deste espelho fecharia a voz de quem só caiu e \
             voltou. O que a tela recebe: {salas:?}"
        );
        assert!(
            salas
                .iter()
                .any(|sala| sala.occupied_by_us && sala.people.iter().any(|p| p.is_self)),
            "a pessoa voltou à sala e não aparece sentada nela na própria tela: {salas:?}"
        );
    }

    /// Quem estava fora de sala volta fora de sala.
    #[test]
    fn a_volta_de_quem_estava_fora_de_sala_continua_fora() {
        let mut room = Room::new();
        room.adopt(&sessao(700), "marcela");
        room.apply(&lia_na_sala());

        readotar_na_volta(&mut room, &sessao(701), "marcela", None);

        assert_eq!(
            room.current_voice_room, None,
            "a volta sentou numa sala quem estava fora de todas"
        );
    }
}

#[cfg(test)]
mod a_voz_quando_o_enlace_volta {
    //! O braço da reconexão, exercitado pelo que ele **faz**.
    //!
    //! Complementa o guarda de texto-fonte de
    //! `crates/seele-conformance/tests/voz_na_reconexao.rs`, que só conseguia
    //! afirmar que a palavra `reopen` aparecia no braço. Três coisas que aquele
    //! guarda deixaria passar são defeitos de verdade, e são estas:
    //!
    //! - reabrir **a partir** da voz viva, e não construir uma do zero — é o
    //!   que carrega mudo, Isolamento total, modo e ganhos por cima da
    //!   reabertura;
    //! - abrir voz para quem estava em texto puro, que devolveria microfone a
    //!   quem não tem nenhum;
    //! - guardar uma voz que não reabriu, que deixaria a casca falando por uma
    //!   conexão morta em vez de esmaecer para texto.
    //!
    //! Desde a pendência 52 a reabertura corre na thread de `esperar_o_audio`,
    //! e a voz nova chega pelo braço da porta: os três testes passam por
    //! `PortaDaVoz::na_volta` e `PortaDaVoz::chegou`, que é o caminho de `drive`.

    use super::{
        Aberta, AberturaDoAudio, ControlesGuardados, Futuro, PortaDaVoz, VozDaSala, VozViva,
    };
    use seele_core::{DeviceChoice, VoiceRoomId};

    /// Uma voz de mentira: o que interessa é o que atravessa a reabertura.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Voz {
        controles: Vec<&'static str>,
        ssrc: u32,
    }

    impl VozViva for Voz {
        fn vestir(&self, _: &ControlesGuardados) {}
    }

    fn entregue(aberta: Aberta<Voz>) -> Futuro<Voz> {
        Box::pin(async move { aberta })
    }

    fn nada_abre_do_zero(_: &DeviceChoice) -> Futuro<Voz> {
        panic!("a volta abriu uma voz do zero, sem partir da que estava viva")
    }

    /// Uma sala com esta voz de pé, como a volta a encontra.
    fn na_sala_com(viva: Voz) -> (VozDaSala<Voz>, PortaDaVoz<Voz>) {
        let voz = VozDaSala::nova(true, DeviceChoice::default());
        *voz.lock().expect("a voz") = Some(viva);
        let mut porta = PortaDaVoz::nova();
        porta.olhar(&voz, Some(VoiceRoomId(1)), nada_abre_do_zero);
        (voz, porta)
    }

    async fn esperar(porta: &mut PortaDaVoz<Voz>, voz: &VozDaSala<Voz>) {
        let aberta = tokio::time::timeout(std::time::Duration::from_secs(5), porta.chegada())
            .await
            .expect("a reabertura não chegou");
        porta.chegou(voz, aberta, nada_abre_do_zero);
    }

    #[tokio::test]
    async fn a_voz_viva_reabre_carregando_os_controles() {
        let (voz, mut porta) = na_sala_com(Voz {
            controles: vec!["mudo", "isolamento", "ganho de quem fala alto"],
            ssrc: 1,
        });

        porta.na_volta(
            &voz,
            |atual| {
                // A reabertura parte da voz que estava viva: é daí que os
                // controles vêm. Quem chama `start_on` não precisa deste `atual`.
                entregue(Ok(Ok(Voz {
                    controles: atual.controles,
                    ssrc: 2,
                })))
            },
            nada_abre_do_zero,
        );
        esperar(&mut porta, &voz).await;

        assert_eq!(
            voz.lock().expect("a voz").clone(),
            Some(Voz {
                controles: vec!["mudo", "isolamento", "ganho de quem fala alto"],
                ssrc: 2,
            }),
            "a reconexão devolveu uma voz nova sem os controles de agora: o \
             roster continua mostrando quem estava mudo como mudo, e o \
             microfone volta aberto"
        );
        assert_eq!(voz.abertura(), AberturaDoAudio::Aberto);
    }

    #[test]
    fn quem_estava_em_texto_puro_continua_em_texto_puro() {
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        porta.olhar(&voz, None, nada_abre_do_zero);
        let mut tentou = false;

        porta.na_volta(
            &voz,
            |_| {
                tentou = true;
                entregue(Ok(Ok(Voz {
                    controles: vec![],
                    ssrc: 2,
                })))
            },
            nada_abre_do_zero,
        );

        assert!(
            !tentou && voz.lock().expect("a voz").is_none(),
            "a reconexão abriu voz para quem não tinha nenhuma; sem placa de \
             som isso é abrir um microfone que não existe"
        );
        assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);
    }

    #[tokio::test]
    async fn a_voz_que_nao_reabre_da_lugar_ao_texto() {
        let (voz, mut porta) = na_sala_com(Voz {
            controles: vec!["mudo"],
            ssrc: 1,
        });

        porta.na_volta(
            &voz,
            |_| entregue(Ok(Err("sem aparelho".to_owned()))),
            nada_abre_do_zero,
        );
        esperar(&mut porta, &voz).await;

        assert!(
            voz.lock().expect("a voz").is_none(),
            "a reconexão guardou a voz que não reabriu: ela sai por uma \
             conexão morta, com `ssrc` velho, e `audio_available` mente"
        );
        assert_eq!(voz.abertura(), AberturaDoAudio::SemAparelho);
    }
}

/// Quantos pares esta versão do cliente consegue atender ao mesmo tempo.
///
/// Reexportado do núcleo, e não copiado: a tela precisa do número para não
/// oferecer o que o cliente não honra, e duas cópias do mesmo teto divergem no
/// dia em que ele subir.
#[must_use]
pub const fn pares_que_esta_versao_atende() -> u8 {
    seele_core::enlace::PARES_QUE_ESTA_VERSAO_ATENDE
}

// Ponte nativa de imagens; bytes nunca passam pelo executor JavaScript do MOD.
impl Connection {
    async fn imagem_de_mod(
        &self,
        pedido: seele_core::PedidoDeImagem,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let (resposta, recebe) = tokio::sync::oneshot::channel();
        self.command(Command::ImagemDeMod {
            pedido,
            bytes,
            resposta,
        })
        .map_err(|_| "sessao-encerrada".to_owned())?;
        recebe.await.map_err(|_| "sessao-encerrada".to_owned())?
    }
}

impl Connection {
    /// Envia a imagem escolhida pela UI e aguarda a gravação no servidor.
    pub async fn enviar_imagem_mod(
        &self,
        id: String,
        token: String,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        self.imagem_de_mod(
            seele_core::PedidoDeImagem::Enviar {
                cabecalho: seele_core::VolumeHeader { mod_id: id, token },
                bytes: bytes.len() as u64,
            },
            bytes,
        )
        .await
        .map(|_| ())
    }
    /// Lê a imagem somente após o MOD autorizar o pedido nesta sessão.
    pub async fn ler_imagem_mod(
        &self,
        id: String,
        channel: u32,
        payload: String,
    ) -> Result<Vec<u8>, String> {
        self.imagem_de_mod(
            seele_core::PedidoDeImagem::Ler {
                mod_id: id,
                channel: ChannelId(channel),
                payload,
            },
            Vec::new(),
        )
        .await
    }
}

#[cfg(test)]
mod onde_um_anexo_grava {
    //! A pergunta que a casca faz antes de salvar, e que a gravação refaz.
    //!
    //! `destino_de` é o lugar só onde o nome sai do histórico e passa pela regra
    //! de `seele_core::anexo_no_disco`, e onde a pasta é conferida. A frase de
    //! confirmação e a gravação perguntam aqui, e por isso não discordam.

    use super::*;

    const LINHA_ABERTA: ChannelId = ChannelId(1);
    const OUTRA_LINHA: ChannelId = ChannelId(2);

    /// Um histórico com um anexo deste nome, numa linha que não é a aberta.
    fn com_anexo(nome: &str) -> Arc<Shared> {
        let shared = super::tests::compartilhado_de_teste();
        let mensagem = seele_core::Message {
            id: MessageId(1),
            channel: OUTRA_LINHA,
            author: PersonId(2),
            author_nickname: "rafael".into(),
            at_seconds: 0,
            body: String::new(),
            replies_to: None,
            own: false,
            edited: false,
            attachment: Some(seele_core::AttachmentInfo {
                id: seele_core::AttachmentId(7),
                file_name: nome.to_owned(),
                declared_type: "image/png".into(),
                byte_size: 10,
                state: seele_core::AttachmentState::Available,
            }),
        };
        shared
            .room
            .lock()
            .unwrap()
            .mensagens
            .entry(OUTRA_LINHA)
            .or_default()
            .push(mensagem);
        shared
    }

    fn pasta() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    #[test]
    fn um_nome_que_e_so_um_nome_grava_na_pasta_dada() {
        let shared = com_anexo("foto.png");
        let (onde, nome) = destino_de(&shared, seele_core::AttachmentId(7), &pasta())
            .expect("um nome comum numa pasta absoluta é recusado");
        assert_eq!(
            onde,
            std::env::temp_dir(),
            "a pasta mudou entre a casca e a gravação"
        );
        assert_eq!(
            nome, "foto.png",
            "o nome que sai do histórico não é o que o remetente mandou"
        );
        // E em qualquer linha, não só na aberta: o download pode terminar depois
        // de a pessoa trocar de linha.
        assert!(
            !shared
                .room
                .lock()
                .unwrap()
                .mensagens
                .contains_key(&LINHA_ABERTA),
            "o anexo deste teste devia estar fora da linha aberta"
        );
    }

    #[test]
    fn um_nome_que_nao_e_so_um_nome_e_recusado_e_citado_por_extenso() {
        for (alegado, citado) in [
            ("../.zshrc", "../.zshrc"),
            ("foto\u{202E}gnp.exe", "foto\\u{202E}gnp.exe"),
        ] {
            let shared = com_anexo(alegado);
            let recusa = destino_de(&shared, seele_core::AttachmentId(7), &pasta())
                .expect_err("um nome com caminho ou disfarce passou para a gravação");
            assert_eq!(
                recusa,
                SaveRefused {
                    reason: NotSavedReason::NomeRecusado,
                    claimed: citado.to_owned(),
                    folder: pasta(),
                },
                "a recusa de {alegado:?} não diz o motivo, ou cita o nome de um \
                 jeito que a própria frase não consegue mostrar"
            );
        }
    }

    #[test]
    fn sem_pasta_absoluta_nada_grava() {
        // A janela fazia esta conferência com `pastaDeDestino === ""`. Agora a
        // casca calcula a pasta, e uma vazia ou relativa gravaria onde quer que
        // o processo tenha sido iniciado — um lugar que a frase não nomeia.
        let shared = com_anexo("foto.png");
        for folder in ["", "Downloads", "./pasta"] {
            let recusa = destino_de(&shared, seele_core::AttachmentId(7), folder)
                .expect_err("uma pasta vazia ou relativa foi aceita para gravar");
            assert_eq!(
                recusa.reason,
                NotSavedReason::SemPasta,
                "a pasta {folder:?} não foi recusada como falta de pasta"
            );
        }
    }

    #[test]
    fn um_anexo_fora_do_historico_nao_ganha_nome_de_fora() {
        let shared = com_anexo("foto.png");
        let recusa = destino_de(&shared, seele_core::AttachmentId(8), &pasta())
            .expect_err("um anexo que o histórico não tem ganhou um nome");
        assert_eq!(
            recusa,
            SaveRefused {
                reason: NotSavedReason::AnexoDesconhecido,
                claimed: String::new(),
                folder: pasta(),
            },
            "um anexo fora do histórico não foi recusado com motivo próprio"
        );
    }
}
