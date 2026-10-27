//! The values that cross the boundary.
//!
//! Everything here is plain: `u64`, `u32`, `String`, `bool`, `Vec`, and closed
//! enums. **No `seele-proto` newtype appears in this module**, and that is the
//! point rather than an oversight.
//!
//! `specs/06-clientes-gui.md` states the rule in one sentence: "Se o frontend
//! precisa saber o que é um `ssrc`, algo está errado." A `PersonId` crossing
//! into a shell is protocol knowledge in the shell, and the third shell would
//! have to learn it too.
//!
//! ADR 0018 explains why the shapes are what they are: this is what `uniffi`
//! will be able to annotate in M6 without a rewrite.
//!
//! # Why these derive `Serialize`
//!
//! The Tauri shell talks to its webview over JSON. Without `serde` here, the
//! desktop crate would have to declare a mirror of every type in this file plus
//! the conversion between them — the same duplication `seele_core::state` was
//! moved to avoid, one layer up. The derive is invisible to `uniffi`, which
//! generates its own marshalling and does not care.
//!
//! Field names stay `snake_case` on the wire. A rename layer buys nothing and
//! costs one more place for a field to be spelled two ways.

/// How the microphone opens. `specs/03-audio.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VoiceMode {
    /// A key is held. Never false-triggers, so it is the default.
    PushToTalk,
    /// The level decides, with hysteresis and hangover.
    VoiceActivated,
    /// Always open.
    Open,
}

impl From<VoiceMode> for seele_core::VoiceMode {
    fn from(mode: VoiceMode) -> Self {
        match mode {
            VoiceMode::PushToTalk => Self::PushToTalk,
            VoiceMode::VoiceActivated => Self::VoiceActivated,
            VoiceMode::Open => Self::Open,
        }
    }
}

/// A volta, e ela **precisa morar junto da ida**.
///
/// Esta conversão existia como um `match` solto dentro de `snapshot`, e a de
/// cima aqui: o mesmo mapeamento escrito em dois lugares, que é como um ganha
/// um quarto modo e o outro não. É exatamente a forma do defeito que custou uma
/// versão inteira em campo — o `palco-imagem.js` declarava o perfil do vídeo
/// que o `codec.rs` decidia, e os dois deixaram de concordar sem que nada
/// avisasse.
impl From<seele_core::VoiceMode> for VoiceMode {
    fn from(mode: seele_core::VoiceMode) -> Self {
        match mode {
            seele_core::VoiceMode::PushToTalk => Self::PushToTalk,
            seele_core::VoiceMode::VoiceActivated => Self::VoiceActivated,
            seele_core::VoiceMode::Open => Self::Open,
        }
    }
}

/// How far the connection has got. `specs/07-estetica.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LinkTrust {
    /// Not connected.
    Offline,
    /// Connected, not verified.
    Unverified,
    /// Verified. Normal operation.
    Verified,
}

// Os tipos de vista que saem do `Room` moram no `seele-sessao` desde a fase 2
// da Tarefa 2.2 da F4: o navegador monta o mesmo retrato com as mesmas
// funções. Reexportados aqui, o caminho deles na casca não mudou.
pub use seele_core::retrato::{
    CausaDaRecusa, Channel, EndReason, LimitesDeTela, Notice, NoticeReason, Person, Prioridade,
    Severity, SignalBand, TelaEmCurso, TemaDaSessao, TransmissaoNaSala, VoiceRoom, VoiceRoomSync,
};

/// What the channel holds, as the confirmation in front of destroying it needs it.
///
/// Counted in the server's database at the instant of asking, and carried across
/// the bridge unrounded. A shell cannot work these out for itself: it holds one
/// page of history and would guess low by whatever the Channel's whole past is,
/// and a number that is nearly right in a box promising to destroy 1.847
/// messages is worse than no number at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ChannelWeight {
    /// Which Channel was weighed.
    pub channel: u32,
    /// How many messages are in it that anybody can read.
    pub messages: u32,
    /// How many distinct people wrote them.
    pub authors: u32,
    /// When the oldest was written, in seconds since the Unix epoch.
    ///
    /// `None` when the Channel is empty — the one case where the sentence has no
    /// date to give and has to say something else instead. Turning it into a
    /// date somebody reads is a shell's job, like every other timestamp that
    /// crosses here.
    pub oldest_at_seconds: Option<i64>,
}

/// Por que a sala de um código não foi achada.
///
/// Espelho de `seele_core::encontro::FalhaAoLocalizar`, com os mesmos nomes: a
/// casca escreve uma frase por variante, e cada uma é uma frase diferente da
/// spec — o ponto calado não é a pessoa indisponível.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum FalhaAoLocalizar {
    /// `$SEELE_ENCONTRO` desligou o ponto: sem ele, código não funciona, e
    /// link com endereço continua funcionando.
    SemPonto,
    /// O ponto fala o ENC/2 e a sala não está registrada: o SEELE do outro
    /// lado está fechado, ou não está recebendo ligações.
    NinguemMora,
    /// O ponto respondeu à sonda e calou a pergunta pela sala: é anterior ao
    /// ENC/2, ou recusou por excesso e a recusa foi para outro pedido.
    Enc2Calado,
    /// O ponto recusou por excesso de pedidos desta rede.
    PontoRecusou,
    /// O ponto não respondeu a nada no prazo.
    PontoMudo,
    /// O nome do ponto não resolveu no prazo, ou não é um endereço.
    PontoNaoResolve,
    /// O ponto deu só a escuta da sala, e nenhum endereço a que conectar.
    SoAEscuta,
    /// O ponto achou o anfitrião e repassou a chamada (ENC/3), e ele não
    /// respondeu a tempo.
    AnfitriaoCalado,
}

impl From<seele_core::encontro::FalhaAoLocalizar> for FalhaAoLocalizar {
    fn from(falha: seele_core::encontro::FalhaAoLocalizar) -> Self {
        use seele_core::encontro::FalhaAoLocalizar as Nucleo;
        match falha {
            Nucleo::SemPonto => Self::SemPonto,
            Nucleo::NinguemMora => Self::NinguemMora,
            Nucleo::Enc2Calado => Self::Enc2Calado,
            Nucleo::PontoRecusou => Self::PontoRecusou,
            Nucleo::PontoMudo => Self::PontoMudo,
            Nucleo::PontoNaoResolve => Self::PontoNaoResolve,
            Nucleo::SoAEscuta => Self::SoAEscuta,
            Nucleo::AnfitriaoCalado => Self::AnfitriaoCalado,
        }
    }
}

/// Um MOD que um servidor exige, como a tela de aceite o lê. ADR 0045.
///
/// # O que cada campo responde, e por que `no_servidor` não pode faltar
///
/// Os três primeiros são identidade — quem é, qual versão, quais bytes. O
/// `hash` é o que uma pessoa compara a olho com o que o indexador publica, e é
/// o que o cliente confere antes de servir um arquivo sob `mod://`.
///
/// `repo` e `reach` são o que o ADR 0045 manda mostrar **antes** de qualquer
/// byte ser baixado: de onde o código veio, e o que ele declara alcançar.
///
/// `no_servidor` é a linha que a tela não pode omitir. Um MOD com metade de
/// servidor roda na máquina de **quem hospeda**, e alcança o bloco `world` do
/// `api/v1.json`: rede de saída, relógio e log. Em português de tela, é a
/// diferença entre um MOD que repinta uma janela e um que abre conexões a
/// partir da casa de outra pessoa — e o plano do runtime nomeia isso como «a
/// parte que a tela de aceite tem de dizer em voz alta».
///
/// Quem escreve a frase é o `frases.js`, como sempre: aqui só atravessam
/// campos (ADR 0012).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModExigido {
    /// `autor/nome`.
    pub id: String,
    /// A versão que o autor declara.
    pub version: String,
    /// O hash do conteúdo, em hexadecimal minúsculo.
    pub hash: String,
    /// O repositório público.
    pub repo: String,
    /// O que este MOD declara alcançar.
    pub reach: Vec<String>,
    /// Se ele roda na máquina de quem hospeda, com o alcance do bloco `world`.
    pub no_servidor: bool,
    /// Se ele pode pintar a aparência do servidor inteiro (D-api-8): a linha
    /// que a tela de aceite diz, porque quem aceita tem de saber.
    pub tema: bool,
}

impl From<seele_core::mods::ModAnunciado> for ModExigido {
    fn from(anunciado: seele_core::mods::ModAnunciado) -> Self {
        Self {
            id: anunciado.id,
            version: anunciado.version,
            hash: anunciado.hash,
            repo: anunciado.repo,
            reach: anunciado.reach,
            no_servidor: anunciado.no_servidor,
            tema: anunciado.tema,
        }
    }
}

/// What the server's certificate turned out to be. ADR 0003, ADR 0006.
///
/// Mirrors `seele_core::tofu::Verdict` one variant at a time — see the `From`
/// impl below — so the shell sees every distinction the core makes and none
/// it doesn't.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Trust {
    /// Nothing was pinned and no invite vouched for anything. Pinned blind.
    ///
    /// `specs/08-seguranca.md` wants this stated rather than accepted in
    /// silence, which is why it is a value the shell must handle and not a log
    /// channel it may ignore.
    FirstContact {
        /// What was pinned. Show it — somebody may want to check it elsewhere.
        fingerprint: String,
    },
    /// Nothing was pinned, and the invite confirmed what the server offered.
    ///
    /// This is what ADR 0006 invented the link to produce.
    FirstContactVerified {
        /// What was pinned, now vouched for.
        fingerprint: String,
    },
    /// The pin matches and nothing contradicts it. Nothing to say.
    Known,
    /// A impressão esperada (a do link, ou a da lista) nomeava outra chave, e
    /// nenhum pino prova o servidor. Recusado.
    ///
    /// Nasce no primeiro contato, e também sobre um pino que confere onde ele
    /// não prova o servidor: um candidato que a pessoa não escolheu, ou um alvo
    /// de escopo local (`seele_core::tofu::TofuVerifier::decide`).
    ///
    /// O verificador TLS do núcleo recusa este caso dentro do aperto de mão,
    /// antes do `Hello`, então ele nunca atravessa como `Trust`: pode chegar
    /// como [`ConnectionError::InviteMismatch`] (ver [`Self::InviteDisagrees`]
    /// para quando chega). O braço existe para o `match` continuar exaustivo.
    InviteRefused {
        /// O que a impressão esperada prometia: a do link, ou a da lista.
        expected: String,
        /// What the server offered.
        offered: String,
    },
    /// O pino confere, e a impressão esperada (a do link colado nesta sessão,
    /// ou a que a lista de servidores guardou) nomeia outra chave.
    ///
    /// A conexão segue e avisa. Só nasce onde o pino prova o servidor: o alvo
    /// (o endereço que a pessoa escolheu) quando ele é de escopo público
    /// (`seele_core::tofu::TofuVerifier::decide`). Ali o pino é a prova de
    /// continuidade do ADR 0003: é o mesmo servidor de antes, e quem está
    /// errado é quem prometeu a outra chave. Num candidato que ninguém
    /// escolheu, ou num alvo de escopo local, o mesmo caso é recusado dentro do
    /// TLS e pode chegar como [`ConnectionError::InviteMismatch`]: chega quando
    /// nenhum candidato entra e ela é a falha que a corrida mostra; nos outros
    /// casos, a recusa fica no `seele.log`.
    ///
    /// A casca diz quem prometeu: o `connect` do app sabe se a impressão veio
    /// do link ou da lista, e a frase de um não serve para o outro.
    InviteDisagrees {
        /// O que a impressão esperada prometia: a do link, ou a da lista.
        expected: String,
        /// What the server offered, and what stays pinned.
        offered: String,
    },
}

impl From<seele_core::tofu::Verdict> for Trust {
    fn from(verdict: seele_core::tofu::Verdict) -> Self {
        use seele_core::tofu::Verdict;

        // An exhaustive match rather than a blanket conversion: when the core
        // grows a sixth verdict this stops compiling, instead of silently
        // mapping it onto one that already exists.
        match verdict {
            Verdict::FirstContact { fingerprint } => Self::FirstContact { fingerprint },
            Verdict::FirstContactVerified { fingerprint } => {
                Self::FirstContactVerified { fingerprint }
            }
            Verdict::Known => Self::Known,
            Verdict::InviteRefused { expected, offered } => {
                Self::InviteRefused { expected, offered }
            }
            Verdict::InviteDisagrees { expected, offered } => {
                Self::InviteDisagrees { expected, offered }
            }
        }
    }
}

/// Um membro deste servidor que não está conectado agora: o FORA DO AR (P6).
///
/// **Só id e apelido**, sem papel e sem horário. E **fora do [`Snapshot`]**, de
/// propósito: o `Snapshot` inteiro vai a todo MOD da API 5 para baixo, e a lista
/// de membros do servidor não é dele. A janela a lê por
/// [`crate::Connection::membros_fora_do_ar`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MembroForaDoAr {
    /// A conta.
    pub id: u64,
    /// O apelido de agora.
    pub nickname: String,
}

/// One thing somebody said.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Message {
    /// Server-assigned identifier. Ordered; the clock is not.
    ///
    /// **Zero numa pendente**, e zero não é identificador de nada: uma mensagem
    /// que esta máquina escreveu e o servidor ainda não confirmou não existe lá,
    /// então não há id que a enderece. Quem a endereça é
    /// [`Self::client_message_id`].
    ///
    /// Quem usa este campo para pedir algo ao servidor tem de conferir
    /// [`Self::estado`] antes — ou, melhor, deixar o guarda de
    /// `Connection::remove_message` conferir. O primeiro caller a pisar nisto foi
    /// um teste de conformidade, que pegou `messages()[0].id` numa lista cuja
    /// primeira linha era uma pendente.
    pub id: u64,
    /// Which Channel.
    pub channel: u32,
    /// Who wrote it.
    pub author: u64,
    /// Their name.
    pub author_nickname: String,
    /// When the **server** accepted it, **seconds** since the Unix epoch.
    ///
    /// The unit is in the name deliberately — see `seele-proto`, where getting
    /// it wrong drew every real message as 1970 while the tests passed.
    ///
    /// Turning it into something a person reads is the shell's job: a locale, a
    /// time zone and a format are all things this layer must not decide.
    pub at_seconds: i64,
    /// The body.
    pub body: String,
    /// Whether we wrote it.
    pub own: bool,
    /// Whether it has been edited since.
    pub edited: bool,
    /// The file hanging off it, if any. ADR 0027.
    pub attachment: Option<Attachment>,
    /// Em que pé está o **envio** desta mensagem.
    ///
    /// `Confirmada` para tudo que veio do servidor, que é o caso de toda
    /// mensagem que não foi escrita nesta máquina agora. Os outros três estados
    /// são de mensagens que esta máquina compôs e o servidor ainda não gravou.
    ///
    /// # Por que ele está aqui
    ///
    /// Porque «mandei» e «está gravada» eram indistinguíveis para quem escreve.
    /// O compositor limpava o campo antes de qualquer resposta e a FFI
    /// confirmava só o enfileiramento local; uma queda, uma recusa de permissão
    /// ou um limite de taxa faziam o texto desaparecer sem rastro. R05 da
    /// revisão da v15.
    pub estado: EstadoDoEnvio,
    /// A chave desta máquina, quando ela é o que identifica a mensagem.
    ///
    /// Presente nas que ainda não foram gravadas — é por ela que a casca pede
    /// para tentar de novo ou desistir. `None` numa mensagem confirmada: ali
    /// quem identifica é o `id` do servidor.
    ///
    /// **Texto, e não número, e a razão é aritmética.** A chave é um `u64` cuja
    /// metade alta é sorteada (ver `next_client_message_id`), então ela passa de
    /// 2^53 com folga — e 2^53 é onde o `Number` do JavaScript deixa de contar
    /// inteiros um por um. Atravessar como número entregaria à janela uma chave
    /// **arredondada**, que não casaria com nenhuma pendente: o botão de tentar
    /// de novo não acharia a mensagem, ou acharia outra.
    pub client_message_id: Option<String>,
}

/// Em que pé está o envio de uma mensagem.
///
/// Quatro variantes, e a distinção entre as três últimas é o que decide o que
/// oferecer a quem escreveu: `Recusada` não passa se repetida, `SemResposta`
/// provavelmente passa, e `Enviando` ainda não é notícia nenhuma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum EstadoDoEnvio {
    /// O servidor gravou. Todo o histórico está aqui.
    Confirmada,
    /// Entregue ao enlace, sem resposta ainda.
    Enviando,
    /// O servidor recusou, e disse por quê.
    Recusada(MotivoDaRecusa),
    /// O enlace caiu antes de qualquer resposta.
    ///
    /// Pode ter sido gravada: repetir com a mesma chave é seguro, porque o
    /// servidor é idempotente por ela.
    SemResposta,
}

/// Por que o servidor não gravou uma mensagem.
///
/// Um espelho de [`seele_core::MessageRefusal`] para a casca, pela mesma
/// razão que todos os outros espelhos deste arquivo existem: uma casca não vê
/// `seele-proto` (ADR 0002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum MotivoDaRecusa {
    /// Sem permissão de escrita, ou sem o papel mínimo do canal.
    PermissionDenied,
    /// O canal não existe mais, ou nunca existiu.
    NoSuchChannel,
    /// O corpo passa do teto do contrato, **em bytes**.
    TooLong {
        /// O maior corpo que o servidor aceita, em bytes.
        limit: u32,
    },
    /// O armazenamento do servidor não aceitou a gravação.
    StorageFailed,
    /// A moderação apagou esta mensagem enquanto quem a escreveu estava fora, e
    /// a repetição não a traz de volta. Tentar de novo nunca passa.
    Apagada,
}

impl From<seele_core::MessageRefusal> for MotivoDaRecusa {
    fn from(motivo: seele_core::MessageRefusal) -> Self {
        match motivo {
            seele_core::MessageRefusal::PermissionDenied => Self::PermissionDenied,
            seele_core::MessageRefusal::NoSuchChannel => Self::NoSuchChannel,
            seele_core::MessageRefusal::TooLong { limit } => Self::TooLong { limit },
            seele_core::MessageRefusal::StorageFailed => Self::StorageFailed,
            seele_core::MessageRefusal::Apagada => Self::Apagada,
        }
    }
}

impl From<seele_core::EstadoDoEnvio> for EstadoDoEnvio {
    fn from(estado: seele_core::EstadoDoEnvio) -> Self {
        match estado {
            seele_core::EstadoDoEnvio::Enviando => Self::Enviando,
            seele_core::EstadoDoEnvio::Falhou(motivo) => Self::Recusada(motivo.into()),
            seele_core::EstadoDoEnvio::SemResposta => Self::SemResposta,
        }
    }
}

/// O volume e o mudo do som de uma transmissão, como a casca os desenha.
///
/// ADR 0054 e R17. Dois campos e não um, porque calar e baixar são decisões
/// diferentes: voltar do mudo tem de devolver o volume que a pessoa escolheu, e
/// um mudo que zerasse o volume apagaria a escolha.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct SomDaTela {
    /// `1.0` é o volume de origem, `0.0` é silêncio. Teto de `4.0`.
    pub volume: f32,
    /// Se está calado, sem mexer no volume.
    pub calada: bool,
}

/// O que esta máquina faz com o **próprio** áudio ao capturar uma tela.
///
/// R21 e ADR 0054. Um espelho de [`seele_core::ExclusaoDoSom`] para a casca, pela
/// razão de todos os espelhos deste arquivo: uma casca não vê `seele-video`.
///
/// **Três respostas, e a terceira não é uma falha.** Uma máquina que não
/// compartilha tela não tem áudio capturado, então a pergunta não se aplica —
/// responder «excluído» ali faria a tela prometer a quem não transmite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ExclusaoDoSom {
    /// O sistema tira o áudio deste processo do que a captura leva.
    ///
    /// Quem compartilha continua ouvindo todo mundo; quem assiste não recebe a
    /// própria voz de volta.
    Excluido,
    /// A captura leva a saída inteira, o SEELE incluído.
    ///
    /// A tela diz isso e oferece as duas saídas: compartilhar uma janela, ou
    /// transmitir sem áudio.
    NaoExcluido,
    /// Esta máquina não compartilha tela.
    SemCaptura,
}

impl From<seele_core::ExclusaoDoSom> for ExclusaoDoSom {
    fn from(dito: seele_core::ExclusaoDoSom) -> Self {
        match dito {
            seele_core::ExclusaoDoSom::Excluido => Self::Excluido,
            seele_core::ExclusaoDoSom::NaoExcluido => Self::NaoExcluido,
            seele_core::ExclusaoDoSom::SemCaptura => Self::SemCaptura,
        }
    }
}

/// Os controles de qualidade de voz desta sessão. F01 e F02.
///
/// # Por que a faixa atravessa junto
///
/// Porque o controle de sensibilidade é uma régua, e uma régua sem extremos não
/// é uma régua. Os dois números vêm de `seele_audio::gate::FAIXA_DE_ABERTURA_DBFS`
/// e não são escritos na janela: dois pares de extremos são dois pares esperando
/// para discordar, e o sintoma da discordância é um controle que parece funcionar
/// e cujo fim não é o fim.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ControlesDaVoz {
    /// Quanto da supressão de ruído está sendo aplicado. `0.0` é desligada.
    pub supressao: f32,
    /// A sensibilidade de abertura que está valendo, em dBFS.
    pub abertura_dbfs: f32,
    /// Se a sensibilidade acima foi escolhida, ou é o padrão do produto.
    ///
    /// A interface diz «padrão» em vez de um número que ninguém escolheu — e o
    /// controle pode voltar ao padrão em vez de ficar preso no valor que o padrão
    /// tinha quando alguém o tocou.
    pub abertura_escolhida: bool,
    /// O extremo sensível da faixa, em dBFS.
    pub abertura_minima_dbfs: f32,
    /// O extremo surdo da faixa, em dBFS.
    pub abertura_maxima_dbfs: f32,
}

/// O que um teste de microfone tem a contar enquanto roda. F01.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EstadoDoTesteDeMicrofone {
    /// O nível do microfone **depois** da supressão, em dBFS.
    ///
    /// Depois, e não antes: é o nível que o portão vê, e é sobre ele que a
    /// sensibilidade é escolhida.
    pub nivel_dbfs: f32,
    /// Se a transmissão estaria aberta agora.
    pub aberto: bool,
    /// Quantas vezes o portão abriu desde que o teste começou.
    ///
    /// Com a sala vazia ele tem de ficar parado: é o número que responde «o
    /// ventilador está abrindo a transmissão?» sem ninguém precisar ouvir.
    pub aberturas: u64,
    /// Quantos quadros anteriores a uma abertura saíram junto com ela.
    pub quadros_retidos: u64,
    /// Onde o corte **está**, em dBFS. F01, A03.
    ///
    /// No padrão, o limiar acompanha o ruído medido e a régua mostra o alvo: são
    /// dois números, e o que decide é este. A marca no medidor é desenhada daqui, e
    /// não da régua — uma marca no alvo enquanto o corte está 10 dB acima engana
    /// justamente quem abriu o teste para descobrir por que não abre.
    pub corte_dbfs: f32,
    /// Se o monitor — ouvir o próprio microfone — está ligado.
    pub monitorando: bool,
    /// O que deu errado durante o teste, se deu.
    ///
    /// Um aparelho que sumiu no meio. Sem esta frase o sintoma seria um medidor
    /// parado, indistinguível de silêncio.
    pub falha: Option<String>,
}

/// One microphone this machine is offering.
///
/// Two strings and not one, because they answer two different questions. The id
/// is what a preference is written down as — the core documents it as stable
/// across runs, unplugs and reboots — and the name is what a person reads. Two
/// microphones of the same model report the same name, so a shell that stored
/// the name would leave the second one unpickable.
///
/// A shell shows `name` and sends back `id`. It never parses either.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CaptureDevice {
    /// The stable handle to send back. Not for a person to read.
    pub id: String,
    /// What the machine calls it.
    pub name: String,
    /// Whether this is the one a session with no preference would take.
    ///
    /// Carried rather than derived: "the default" is a moving target, and a
    /// shell cannot recompute which row is the machine's own choice.
    pub default: bool,
}

/// One place this machine will play sound.
///
/// The twin of [`CaptureDevice`], with the same two strings for the same two
/// questions, and its own type rather than a shared one carrying a direction:
/// both ids are strings, so only the type stops a shell sending a microphone
/// where an output belongs.
///
/// A shell shows `name` and sends back `id`. It never parses either.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlaybackDevice {
    /// The stable handle to send back. Not for a person to read.
    pub id: String,
    /// What the machine calls it.
    pub name: String,
    /// Whether this is the one a session with no preference would take.
    pub default: bool,
}

/// Em que pé está o aparelho de áudio.
///
/// Espelha `seele_core::EstadoDoAparelho`; tipo próprio porque o ADR 0002
/// impede esta casca de nomear `seele-audio`, e a tradução tem de parar aqui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoDoAparelho {
    /// Há som entrando e saindo pelo aparelho que a sessão diz usar.
    #[default]
    Funcionando,
    /// O aparelho caiu ou o sistema o trocou, e a reabertura está em curso.
    Trocando,
    /// Todas as tentativas falharam: não há áudio agora. A sessão continua
    /// olhando devagar, e um aparelho que reapareça volta a tocar sozinho — a
    /// pessoa também pode escolher um na tela e não esperar.
    Perdido,
}

impl From<seele_core::EstadoDoAparelho> for EstadoDoAparelho {
    fn from(estado: seele_core::EstadoDoAparelho) -> Self {
        match estado {
            seele_core::EstadoDoAparelho::Funcionando => Self::Funcionando,
            seele_core::EstadoDoAparelho::Trocando => Self::Trocando,
            seele_core::EstadoDoAparelho::Perdido => Self::Perdido,
        }
    }
}

/// Em que pé está a abertura do áudio desta sessão. Pendência 52.
///
/// # Por que existe
///
/// Na 0.16 o áudio abre quando a pessoa entra numa sala de voz e fecha quando
/// ela sai: quem fica no servidor só pela conversa por escrito devolve o repouso
/// à máquina. Com isso, [`Snapshot::audio_available`] falso deixou de querer
/// dizer «falta áudio»: fora de sala ele é falso **sem faltar nada**, e uma
/// casca que o lesse sozinho diria «sem áudio» a quem só não entrou numa sala.
/// Este campo é o que separa os casos.
///
/// Só estado, nada desta máquina: nenhum nome de aparelho, nenhum volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AberturaDoAudio {
    /// Fora de toda sala de voz. Nenhum aparelho aberto, e é o certo.
    #[default]
    ForaDeSala,
    /// Numa sala, com o aparelho abrindo. Dura o que o driver levar, até o
    /// prazo de dez segundos.
    Abrindo,
    /// Numa sala, com a voz de pé.
    Aberto,
    /// Numa sala, sem voz: o aparelho não abriu, ou esta sessão entrou sem
    /// áudio.
    SemAparelho,
    /// Numa sala, e o aparelho não abriu dentro do prazo. A conversa por
    /// escrito segue; sair da sala e entrar de novo tenta outra vez.
    NaoAbriuNoPrazo,
    /// Numa sala, e **a voz desta máquina está noutra sessão** (E3 da 0.17,
    /// `crate::foco`). Nenhum aparelho aberto aqui: nada sai e nada toca. Trazer
    /// a voz para cá é um ato explícito da pessoa, e a outra sessão para antes
    /// de esta abrir.
    EmOutraSessao,
}

/// Connection quality.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize)]
pub struct Telemetry {
    /// Round trip, milliseconds. **`None` enquanto nada foi medido** (nenhum
    /// `Pong` voltou ainda): não medido não é 0 ms, e a tela diz «sem medida».
    pub rtt_ms: Option<f32>,
    /// Arrival jitter, milliseconds.
    pub jitter_ms: f32,
    /// Packet loss, 0.0 to 1.0. **`None` quando o servidor não mediu** (desde a
    /// 0.17, o anfitrião ICE-lite nem sempre conta a perda): não medido não é
    /// zero, e a tela diz «não medido» em vez de «0,0%».
    pub loss_fraction: Option<f32>,
    /// Encoder bitrate, bits per second. Zero when there is no audio.
    pub bitrate_bps: u32,
    /// This person's Sync Ratio, 0 to 100.
    pub signal: u8,
    /// Which band that is.
    pub sync_band: SignalBand,
    /// Microphone level, 0.0 to 1.0.
    pub input_level: f32,
    /// Whether the machine, rather than the network, is dropping audio.
    ///
    /// The two sound identical to a listener and have opposite fixes, so the
    /// core separates them and the shell can say which one it is.
    pub local_fault: bool,
    /// Voice frames this machine produced and never managed to send.
    ///
    /// The third kind of loss, and the one that had no name. `loss_fraction`
    /// is the network's, reported by the server. `local_fault` is this machine's
    /// capture or playback stumbling. This is neither: the frame was encoded,
    /// it was ready, and the transport refused it.
    ///
    /// A QUIC datagram is not fragmented. Text travels on a stream and adapts
    /// to the path by itself; voice travels in datagrams, and a datagram that
    /// does not fit the path is refused whole. That is how a link can deliver
    /// every message and still chop the audio — and in one direction only,
    /// because the path is not the same both ways.
    ///
    /// Zero is normal. Anything else means the audio is being lost **before it
    /// leaves**, which sounds identical to network loss and has the opposite
    /// fix.
    pub frames_refused: u64,
    /// Se o microfone entrega **zero exato** há três segundos, aberto e sem
    /// mudo. F1-microfone-macos.
    ///
    /// É como o macOS entrega um microfone que ele bloqueia: abre sem erro e
    /// manda zeros. A casca do macOS diz então que o sistema **pode** estar
    /// bloqueando — uma interface USB com o ganho no mínimo também entrega zero
    /// exato. Falso sem voz de pé. Ver `seele_core::LocalTelemetry`.
    ///
    /// Só um sim ou não: nenhum nome de aparelho, nada desta máquina.
    pub microfone_entrega_silencio_digital: bool,
}

/// Uma tela ou uma janela que esta máquina pode transmitir.
///
/// O gêmeo de [`CaptureDevice`] para o vídeo, com a mesma divisão de trabalho:
/// a casca mostra `nome`, devolve `id` em [`crate::Connection::compartilhar_tela`], e
/// não lê nem um nem o outro.
///
/// `largura` e `altura` são o tamanho da **fonte**, e não o da transmissão. Os
/// dois quase nunca são iguais: o que sai é o degrau que o teto compra (§5.1),
/// e quem quiser o número que está saindo lê [`TelaEmCurso::altura`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FonteDeTela {
    /// O identificador a devolver. Não é para uma pessoa ler.
    pub id: u64,
    /// Como o sistema chama esta fonte.
    pub nome: String,
    /// `true` para um monitor inteiro, `false` para uma janela.
    pub monitor: bool,
    /// Largura da fonte, em pixels.
    pub largura: u32,
    /// Altura da fonte, em pixels.
    pub altura: u32,
}

/// O que o sistema deixa este processo fazer com o microfone.
///
/// O gêmeo de [`PermissaoDeTela`], e a diferença entre os dois é o que a tela
/// pode oferecer: aquele tem um botão que **pergunta**, e este não — no Windows
/// não há a quem pedir para um app de área de trabalho. Aqui o botão só abre a
/// página certa dos Ajustes.
///
/// Ver `seele_audio::device::ConsentimentoDoMicrofone` para o porquê de cada
/// caso, e para a ordem em que o sistema os decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PermissaoDeMicrofone {
    /// O sistema deixa.
    Permitida,
    /// Desligado por política da máquina; quem conserta é quem a administra.
    NegadaNaMaquina,
    /// A pessoa desligou o microfone para todos os aplicativos.
    NegadaParaTudo,
    /// O microfone está ligado e **aplicativos de área de trabalho** não.
    NegadaParaAreaDeTrabalho,
    /// Não deu para saber. **Não é «permitida»** — ver o enum do núcleo.
    NaoSeSabe,
}

impl From<seele_core::ConsentimentoDoMicrofone> for PermissaoDeMicrofone {
    fn from(consentimento: seele_core::ConsentimentoDoMicrofone) -> Self {
        match consentimento {
            seele_core::ConsentimentoDoMicrofone::Permitido => Self::Permitida,
            seele_core::ConsentimentoDoMicrofone::NegadoNaMaquina => Self::NegadaNaMaquina,
            seele_core::ConsentimentoDoMicrofone::NegadoParaTudo => Self::NegadaParaTudo,
            seele_core::ConsentimentoDoMicrofone::NegadoParaAreaDeTrabalho => {
                Self::NegadaParaAreaDeTrabalho
            }
            seele_core::ConsentimentoDoMicrofone::NaoSeSabe => Self::NaoSeSabe,
        }
    }
}

/// O que o sistema operacional respondeu sobre gravar a tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PermissaoDeTela {
    /// Pode capturar.
    Concedida,
    /// Negada, e a pessoa precisa ir aos ajustes do sistema — no macOS o TCC
    /// não pergunta duas vezes.
    Negada,
    /// Ainda não perguntada.
    NaoPerguntada,
    /// Esta compilação não tem como perguntar ao sistema.
    ///
    /// **Não estava no contrato de 22/08, e existe porque as outras três
    /// mentiriam.** É o Linux da v1: não há captura de tela neste build — o
    /// portal XDG exige `ashpd` mais `pipewire`, e com eles o binário deixa de
    /// ser autocontido (decisão de 22/08/2026, §7 item 5) —, então não há a quem
    /// perguntar. Responder `NaoPerguntada` mandaria a pessoa apertar um botão
    /// que não pergunta nada, e responder `Negada` culparia um sistema que nunca
    /// foi consultado.
    ///
    /// Uma casca que recebe isto **não desenha o controle de compartilhar**. É a
    /// mesma resposta que [`Snapshot::caminho`] dá com `None`: sem informação, a
    /// tela não escreve nada.
    NaoSeSabe,
}

/// Everything the interface needs, in one value.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Snapshot {
    /// Por qual caminho esta conversa saiu, como nome estável.
    ///
    /// Um de `RedeLocal`, `Ipv6Direto`, `EnderecoPublico` ou `FuroDeNat` — os
    /// quatro de `seele_core::chegada::Caminho`, que a lista
    /// [`crate::caminhos`] deriva do enum para a casca poder cobrar cobertura
    /// de frase sem repetir os nomes.
    ///
    /// **`None` quando não se sabe, e a casca então não escreve nada.** Não
    /// existe um quinto nome para «direto»: a distinção que essa palavra
    /// apagaria é justamente a que importa — em `FuroDeNat` a conversa é direta
    /// **e** alguém soube que ela existe. Inventar um nome quando não se sabe é
    /// a mentira confiante que o ADR 0022 existe para não produzir.
    ///
    /// O grau de certeza de `FuroDeNat` está escrito onde ele nasce, em
    /// `seele_core::chegada::Caminho`: um `LEVE` que saiu é evidência forte de
    /// que o furo abriu, e não é prova.
    pub caminho: Option<&'static str>,
    /// Onde o enlace está: no ar, na bateria interna, ou acabado.
    ///
    /// `specs/07-estetica.md` manda a interface não fechar quando a
    /// conexão cai — ela esmaece, conta cinco minutos e continua legível. Sem
    /// este campo a casca gráfica não tinha como saber disso e pulava direto
    /// para "encerrado", que é a simplificação que a spec proíbe.
    pub link: LinkState,
    /// How far the connection has got.
    pub link_state: LinkTrust,
    /// What the server is called.
    ///
    /// Follows a rename **inside the session** now: the server announces it to
    /// everybody connected, so a shell that redraws its header on
    /// [`Event::ServerChanged`] never shows a name the server stopped using.
    pub server: String,
    /// How many times the server's picture has changed, this session.
    ///
    /// Not the picture. The same reasoning as [`Snapshot::messages_revision`],
    /// with the numbers a size smaller: this value is read on every interface
    /// frame — twice a second — and carrying the bytes would mean cloning them,
    /// turning them into JSON, and pushing them across the bridge each time,
    /// for a value that changes when somebody presses a button and never
    /// otherwise. At the protocol ceiling that is 8 KiB of PNG becoming rather
    /// more than that as text, sixty times a minute, for ever.
    ///
    /// The number itself means nothing — only the difference does. A shell
    /// keeps the last one it drew and calls [`crate::Connection::server_icon`] when it
    /// moves. Zero, and never moving, is the ordinary server: it has no picture.
    pub icon_revision: u64,
    /// Sobe quando **qualquer** retrato de pessoa muda.
    ///
    /// A casca guarda o último que desenhou e rebusca os retratos que estão na
    /// tela quando ele muda — o mesmo acordo de [`Snapshot::icon_revision`], e
    /// pela mesma razão: os bytes não atravessam a ponte duas vezes por segundo
    /// por causa de uma imagem que muda quando alguém aperta um botão.
    pub person_icons_revision: u64,
    /// This person's identifier, once known.
    pub me: Option<u64>,
    /// This person's name.
    pub nickname: String,
    /// Voice channels, each carrying who is in it.
    pub voice_rooms: Vec<VoiceRoom>,
    /// Quem está conectado neste servidor, em sala ou fora dela.
    ///
    /// **Não é a soma de [`VoiceRoom::people`]**, e essa diferença é a razão de o
    /// campo existir: quem entra no servidor e fica fora das salas não aparece
    /// em nenhuma sala de voz, e por muito tempo não aparecia em lugar nenhum — a
    /// interface listava os sentados e chamava aquilo de «pessoas». A lista de
    /// quem está fora das salas é esta menos aquelas, e a subtração é uma linha
    /// de quem desenha.
    pub presentes: Vec<Person>,
    /// Text channels.
    pub channels: Vec<Channel>,
    /// How many times the conversation has changed, this session.
    ///
    /// Not the conversation itself. This snapshot is read on every interface
    /// frame — twice a second, plus once per event — and carrying the history
    /// meant cloning every nickname and every body already said, serialising
    /// the lot to JSON, and pushing it across the bridge each time. The cost
    /// grows with the conversation, so a long session gets steadily slower to
    /// type into. That is not a tuning problem; it is the shape of the loop,
    /// and it showed up the first time two machines talked for a while.
    ///
    /// The number itself means nothing — only the difference does. A shell
    /// keeps the last one it drew and calls [`Connection::messages`] when it moves.
    /// `seele_core::Changed` already said when that was, and this is the shell
    /// finally being able to act on it.
    pub messages_revision: u64,
    /// Quantas vezes o FORA DO AR pode ter mudado: **o número, e nunca a lista**.
    ///
    /// A lista de membros do servidor fica fora do `Snapshot`, porque todo MOD
    /// da API 5 para baixo lê o `Snapshot` inteiro e ela não é dele (P6). A
    /// casca guarda o último número que desenhou e chama
    /// [`crate::Connection::membros_fora_do_ar`] quando ele muda.
    pub fora_do_ar_revision: u64,
    /// **Os cargos desta sessão**: sobe a cada `QuadroDeCargos`,
    /// `SuasPermissoes` ou `CargoRecusado` dobrado (ADR 0064). A casca guarda o
    /// último que desenhou e relê [`crate::Connection::cargos_na_tela`] quando
    /// ele muda — o mesmo contrato do [`Self::fora_do_ar_revision`].
    pub cargos_revision: u64,
    /// Measurements.
    pub telemetry: Telemetry,
    /// The last thing worth surfacing.
    pub notice: Option<Notice>,
    /// Whether this person's microphone is muted.
    pub muted: bool,
    /// Whether this person's speakers are muted.
    pub total_isolation: bool,
    /// Whether this person is transmitting.
    pub speaking: bool,
    /// How the microphone opens.
    pub voice_mode: VoiceMode,
    /// Whether audio is running at all.
    ///
    /// **Falso fora de sala, sem faltar nada** desde a pendência 52: o áudio só
    /// abre dentro de uma sala de voz. Quem quer saber se falta áudio lê
    /// [`Snapshot::abertura_do_audio`].
    pub audio_available: bool,
    /// Em que pé está a abertura do áudio. Pendência 52.
    ///
    /// É o que deixa a casca dizer a coisa certa quando [`Self::audio_available`]
    /// é falso: fora de sala não há o que dizer; abrindo, espera-se; e numa sala
    /// em que o aparelho não abriu a tempo há uma frase própria.
    pub abertura_do_audio: AberturaDoAudio,
    /// The microphone this session is actually capturing from.
    ///
    /// The device that **opened**, not the one that was asked for — those are
    /// different whenever nothing was asked for, which is most of the time. A
    /// screen that drew the request would tell a person their microphone is
    /// called "default".
    ///
    /// `None` when there is no audio, or when the machine opened a device and
    /// then would not describe it. The second is not a failure: audio is
    /// running, and a shell must draw an unnamed device rather than a missing
    /// one. [`Snapshot::audio_available`] is what tells the two apart.
    pub capture: Option<CaptureDevice>,
    /// Where this session is actually playing.
    ///
    /// The device that **opened**, read the same way [`Snapshot::capture`] is
    /// read, and needed more than that one: falling back to the machine's own
    /// speakers makes no sound of its own. A person who picked a headset and
    /// hears nothing has this channel and nothing else to tell them the pick did
    /// not take.
    pub playback: Option<PlaybackDevice>,
    /// Em que pé está o aparelho de áudio desta sessão.
    ///
    /// # Por que uma casca precisa disto
    ///
    /// Porque trocar de microfone ou de fone **no sistema operacional** não é a
    /// mesma coisa que trocar na tela do SEELE, e até aqui a segunda era a
    /// única que a interface sabia contar. Quando o sistema trocava o aparelho
    /// padrão ou alguém tirava o fone da tomada, o produto sabia — o `cpal`
    /// avisa — e não dizia nada: o único sinal era `local_fault`, que diz «a
    /// máquina, não a rede» e **apaga sozinho** quando o contador para de
    /// crescer.
    ///
    /// [`EstadoDoAparelho::Trocando`] é o instante entre o aparelho cair e o
    /// novo abrir; [`EstadoDoAparelho::Perdido`] é o que sobra quando nenhuma
    /// tentativa deu certo, e aí a única saída é a pessoa escolher um.
    pub aparelho: EstadoDoAparelho,
    /// Quantas vezes esta sessão reabriu o áudio num aparelho.
    ///
    /// Cresce uma vez por reabertura conseguida. É o que deixa uma casca dizer
    /// «o aparelho mudou» — um nome diferente sem aviso é a mesma tela de
    /// sempre com outra palavra, e ninguém repara.
    pub trocas_de_aparelho: u64,
    /// Whether this person may make and rename voice_rooms and Channels.
    ///
    /// So a shell can decide whether the control exists at all. `ManageVoiceRooms`
    /// as PERMISSIONS resolved it, sent down in the handshake — a single boolean
    /// and not the permission list, because this is the one a screen asks
    /// about, and a list would invite each shell to start deciding things out
    /// of it.
    ///
    /// **Convenience, never enforcement.** `specs/08-seguranca.md`: "A
    /// interface esconder é conveniência; o servidor negar é a segurança." A
    /// shell that ignores this and asks anyway gets a `NoticeRaised` carrying
    /// `PermissionDenied`, and nothing is created.
    pub may_manage_voice_rooms: bool,
    /// Whether this person may end somebody else's session — `expulsar`.
    ///
    /// One boolean per moderation verb rather than the permission list, for the
    /// reason [`Snapshot::may_manage_voice_rooms`] gives: a list invites each shell to
    /// start deciding things out of it, and four separate controls ask four
    /// separate questions. They are separate on the wire too —
    /// `specs/04-servidor-seele.md` enumerates four permissions and a role may
    /// carry any subset.
    ///
    /// **Convenience, never enforcement.** The app's `EJETAR PLUG DO OPERADOR`
    /// has been drawn and disabled since v2; this is what may enable it, and
    /// the server refusing is still what makes it safe.
    pub may_kick: bool,
    /// Whether this person may bar somebody from returning — `banir`.
    pub may_ban: bool,
    /// Whether this person may take somebody else's message off o canal.
    ///
    /// Only somebody else's: removing one's own needs no permission, so a shell
    /// offering the control on a message the reader wrote does not consult this.
    pub may_remove_message: bool,
    /// Whether this person may move somebody between voice_rooms — `mover_pessoa`.
    pub may_move_person: bool,
    /// Whether this person may name the server and give it a picture.
    ///
    /// `AdministerServer`, which is the same permission behind
    /// [`Snapshot::may_delete_rooms`] today — and a separate field anyway,
    /// because they are separate questions. A shell that read the destroy flag
    /// to decide whether to offer a rename box would be leaning on a
    /// coincidence between two verbs that the roles table can pull apart the
    /// moment somebody makes a role that may dress the server without being able
    /// to destroy what people wrote in it.
    ///
    /// **Convenience, never enforcement**, like every flag beside it.
    pub may_customise_server: bool,
    /// Whether this person may destroy voice_rooms and Channels — `administrar_server`.
    ///
    /// Its own boolean, and deliberately not [`Snapshot::may_manage_voice_rooms`].
    /// Making a room and renaming one are mistakes a server survives; destroying
    /// one ends what other people wrote, and no screen of this product brings
    /// it back. `specs/04-servidor-seele.md` calls `gerenciar_voice_rooms` "criar e
    /// configurar salas de voz" and `administrar_server` "todo o resto sobre o servidor",
    /// so a role that may build rooms without being able to unmake them is a
    /// role somebody can actually write — and a single boolean for both would
    /// make that role impossible to offer correctly.
    ///
    /// **Convenience, never enforcement**, like the five above it.
    pub may_delete_rooms: bool,
    /// A transmissão que esta sessão está **assistindo**, quando há uma.
    ///
    /// `None` quando ninguém está compartilhando na sala onde esta pessoa está,
    /// ou quando ela escolheu não assistir nenhuma. Uma transmissão noutra sala
    /// não aparece aqui: o servidor só a anuncia a quem está lá dentro, e
    /// desenhá-la fora seria a casca contando algo que a sessão não viu.
    ///
    /// # Era «a da sala», e aí não era de ninguém
    ///
    /// `tela_de` pegava a primeira transmissão que encontrasse no mapa da sala,
    /// sem relacioná-la com a escolhida por quem assiste nem com a própria. Numa
    /// sala com dois transmissores, autoria, controles e metadados podiam estar
    /// falando de uma tela enquanto a imagem era de outra — e a interface não
    /// tinha como saber. ADR 0054 e R18.
    ///
    /// **Assistida e própria são coisas diferentes**, e por isso são dois
    /// campos: quem transmite e assiste a tela de outra pessoa ao mesmo tempo é
    /// o caso normal, não o raro.
    pub tela: Option<TelaEmCurso>,
    /// A transmissão que **esta** pessoa está mandando, quando há uma.
    ///
    /// É a única que carrega [`TelaEmCurso::pedido`], porque o teto é escolha de
    /// quem transmite e não viaja no fio.
    pub minha_transmissao: Option<TelaEmCurso>,
    /// Tudo o que está sendo transmitido na sala em que esta pessoa está.
    ///
    /// **A lista existe porque a sala passou a caber mais de uma.** Com uma só,
    /// [`Self::tela`] respondia tudo: quem transmite, quanto sai, se parou. Com
    /// duas, alguém precisa escolher qual assistir — e para escolher é preciso
    /// saber o que há.
    ///
    /// Vazia quando ninguém transmite, e com **um** elemento no caso comum, que
    /// é quando a casca não desenha lista nenhuma: escolher entre uma coisa não
    /// é escolher.
    pub transmissoes: Vec<TransmissaoNaSala>,
    /// Set once the session is over.
    pub ended: Option<EndReason>,
    /// **O tema do servidor**, o que a metade de servidor de um MOD declarou e
    /// o servidor mandou no `TemaMudou` (API 6 de MODs, rascunho §9; tarefa
    /// 17d da F3). A casca o confere e o aplica antes de desenhar a sessão;
    /// `None` é sem tema, e tira o que havia.
    pub tema_do_servidor: Option<TemaDaSessao>,
}

/// What the shell subscribes to.
///
/// Deliberately coarse: a shell is told *that* the roster moved, then reads
/// [`Snapshot`]. Delivering the change itself would mean every shell
/// reimplementing the fold that `seele_core::state` already does.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Event {
    /// A private part of a MOD response, forwarded without interpreting its JSON.
    ModReply {
        /// Request correlation.
        request: u32,
        /// Zero-based index.
        part: u32,
        /// Total number of parts.
        total: u32,
        /// JSON fragment.
        payload: String,
    },
    /// A metade de servidor de um MOD avisou as janelas dele (rascunho da API
    /// 6, §7.3). A casca o entrega às janelas daquele MOD; a carga é o JSON do
    /// MOD, sem interpretação aqui.
    ModAviso {
        /// O MOD que avisa.
        mod_id: String,
        /// O canal do aviso; `None` é o servidor inteiro.
        canal: Option<u32>,
        /// A carga, em JSON.
        carga: String,
    },
    /// Somebody joined, left, or changed state.
    RosterChanged,
    /// A message arrived, changed, or went away.
    MessagesChanged,
    /// voice_rooms or Channels changed.
    ChannelsChanged,
    /// The server renamed itself, or changed its picture.
    ///
    /// Separate from [`Self::ChannelsChanged`] for the reason
    /// `seele_core::Changed` gives: what moved is the header and the badge, and
    /// a shell that redrew every room because the server was renamed would be
    /// redrawing the one part of the screen that did not change.
    ///
    /// The new name is on the next [`Snapshot`]. The picture is not — read
    /// [`Snapshot::icon_revision`] and fetch it with
    /// [`crate::Connection::server_icon`] when the number moves.
    ServerChanged,
    /// Os cargos desta sessão mudaram: chegou um `QuadroDeCargos`, um
    /// `SuasPermissoes` ou um `CargoRecusado` (ADR 0064). A casca relê
    /// [`crate::Connection::cargos_na_tela`] quando a `marca` anda — sem laço de
    /// leituras e sem pedido ao servidor.
    CargosChanged {
        /// O [`Snapshot::cargos_revision`] depois desta mudança.
        marca: u64,
    },
    /// New measurements.
    TelemetryChanged,
    /// Something to surface.
    NoticeRaised {
        /// What it is.
        notice: Notice,
    },
    /// The session is over.
    Ended {
        /// Why.
        reason: EndReason,
    },
    /// **A sessão caiu com motivo dito, e vai voltar.** Não é fim: a bateria
    /// reconecta, e a volta chega com a geração nova do carimbo.
    ///
    /// É a despedida recuperável do servidor (`seele_core::despedida::a_sessao_acabou_aqui`
    /// diz que não acabou): hoje, na prática, o `FellBehind`, «este enlace ficou
    /// para trás do servidor». Ela chega pelo quadro `Disconnecting` ou, quando o
    /// quadro não pôde sair (o `ctl` sem crédito), pelo código de fechamento
    /// `seele_core::transporte::CODIGO_FICOU_PARA_TRAS`, e é **um** evento nos
    /// dois casos.
    ///
    /// Antes dele, a casca só via a faixa genérica de reconexão, e a causa ficava
    /// no `seele.log`. O carimbo é o da geração que caiu.
    CaiuComMotivo {
        /// Por quê, no vocabulário de [`Self::Ended`].
        reason: EndReason,
    },
    /// Uma chegada mudou de etapa, enquanto ela acontece.
    ///
    /// O único evento desta lista que chega **antes** de haver sessão, e é para
    /// isso que ele existe: `Connection::connect` bloqueia, e quem se inscrevesse
    /// depois dela só teria o `Arc<Connection>` com a travessia inteira já terminada.
    /// Quem quiser este evento entra por `Connection::connect_watching`, que recebe o
    /// ouvinte antes de bloquear.
    ///
    /// Grosso como os outros? Não, e é a exceção: um `Snapshot` não carrega
    /// etapa nenhuma — não há sessão para carregá-lo — então a etapa viaja
    /// dentro do evento ou não viaja.
    ConnectStageChanged {
        /// Onde a chegada está agora.
        stage: crate::ConnectStage,
    },
    /// A file moved, finished moving, or stopped moving. ADR 0027.
    ///
    /// Its own event and not folded into [`Self::MessagesChanged`]: while a
    /// file is going up there is no message yet — the server publishes it only
    /// once the bytes have arrived whole — so there is nothing for a message
    /// event to be about. What the screen has is a bar.
    TransferChanged {
        /// Where it is.
        transfer: Transfer,
    },
    /// A transmissão de tela desta sala começou, acabou, ou mudou de número.
    ///
    /// Grosso como os outros: o que mudou está no próximo [`Snapshot::tela`].
    ///
    /// Acende também quando **só o roster** anda e há uma transmissão em curso,
    /// e isso não é generosidade — é o §5.1. A contagem de espectadores é um
    /// campo de [`TelaEmCurso`], ela é o N que divide o teto, e uma quinta
    /// pessoa entrando muda o que está saindo sem que nenhuma mensagem de tela
    /// tenha chegado. Uma casca que só redesenhasse a tela em
    /// `ScreenShareStarted` mostraria `720p · 4 pessoas assistindo` para uma
    /// sala de seis.
    ScreenChanged,
    /// Chegou uma transmissão de tela e esta versão não sabe lê-la.
    ///
    /// **É o evento que apagava a tela preta.** Quando o cabeçalho de um fluxo
    /// de tela não decodifica, este lado não sabe nem o número da transmissão —
    /// então não há `ScreenClosed` a mandar, e não havia nada. A casca não ficava
    /// sabendo que existiu alguma coisa: nenhum desenho, nenhuma frase, e a
    /// pessoa olhando um retângulo escuro.
    ///
    /// Relatado assim: «quem assiste com uma versão mais velha vê tela preta,
    /// sem mensagem nenhuma».
    ///
    /// O motivo vai como texto porque é para uma pessoa ler, e porque o que o
    /// causou é um formato que este build não conhece — enumerá-lo exigiria
    /// conhecer de antemão o que ainda não foi inventado.
    ScreenUnreadable {
        /// O que o decodificador do cabeçalho respondeu.
        reason: String,
    },
    /// Uma transmissão alheia começou a chegar, e este é o seu tamanho.
    ///
    /// Separado de [`Self::ScreenChanged`], que é grosso e manda reler o
    /// `Snapshot`: este carrega o que **não** está no snapshot, porque é do
    /// fluxo e não da sala — a largura e a altura com que o cabeçalho abriu.
    /// Sem elas a casca não tem como armar o decodificador, e o primeiro quadro
    /// chegaria a uma tela que ainda não sabe de que tamanho é a imagem.
    ScreenOpened {
        /// Qual transmissão.
        screen: u32,
        /// Largura em pixels.
        width: u16,
        /// Altura em pixels.
        height: u16,
    },
    /// Um quadro comprimido de uma transmissão alheia.
    ///
    /// **A exceção à regra de que os eventos são grossos**, e ela se paga: um
    /// quadro não é uma mudança que se lê do `Snapshot` depois, é o conteúdo. A
    /// alternativa seria a casca vir buscar quadro a quadro por comando, o que
    /// custa uma travessia de ida e volta por quadro em vez de meia.
    ///
    /// Os bytes vão em base64 e não como lista de números: o JSON de um `Vec<u8>`
    /// gasta umas quatro vezes mais para dizer a mesma coisa, e um quadro-chave
    /// de 1080p tem 65 KiB.
    ///
    /// Quem decodifica é a janela, com o decodificador do sistema. É o que faz
    /// **assistir não exigir o módulo do Cisco** — só transmitir exige.
    ScreenFrame {
        /// Qual transmissão.
        screen: u32,
        /// Se dá para começar a decodificar por este.
        key: bool,
        /// O quadro em Annex-B, em base64.
        data: String,
    },
    /// A transmissão que estava chegando acabou.
    ScreenClosed {
        /// Qual transmissão.
        screen: u32,
    },
}

/// Where one file is on its way.
///
/// Enumerated, and the shell writes the sentence. The one that has to be
/// written and cannot be inferred is [`Transfer::Fell`]: ADR 0027 has no
/// resumption, so trying again starts from zero, and whoever is waiting has to
/// be told rather than left to discover it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
pub enum Transfer {
    /// Going up.
    Sending {
        /// The message's idempotency key, which is how a screen finds its own.
        client_message_id: u64,
        /// Bytes gone.
        done: u64,
        /// Bytes in total. **Always known** — whoever chose the file knows how
        /// big it is — so this is always a bar and never a bar pretending to
        /// measure something nobody measured.
        total: u64,
    },
    /// Every byte went out. The message appears on the Channel next.
    Sent {
        /// Which message.
        client_message_id: u64,
    },
    /// The server cut the stream: it refused. The reason is still travelling.
    ///
    /// Two variants for one refusal, and it is not a duplication: this one is
    /// what the *sending* end observes — the stream stopped — and it arrives
    /// first, because it is a fact about a socket. [`Self::RefusedBecause`] is
    /// the reason, which comes back on the control stream as an enumerated
    /// value. A screen that waited for the second would show nothing while the
    /// first was already true.
    Refused {
        /// Which message.
        client_message_id: u64,
    },
    /// The server said **why** it refused.
    RefusedBecause {
        /// Which message.
        client_message_id: u64,
        /// The enumerated reason.
        reason: AttachmentRefusal,
    },
    /// A file that was asked for is not coming, and why.
    ///
    /// The expected reason is [`AttachmentRefusal::Expired`]: the bytes were
    /// evicted to keep the server under its ceiling, the row survived, and this
    /// is what turns that row into a sentence on somebody's screen.
    Unavailable {
        /// Which attachment.
        attachment: u64,
        /// The enumerated reason.
        reason: AttachmentRefusal,
    },
    /// Bytes going into a MOD's volume were refused, and why — ADR 0048.
    ///
    /// Named by **token** and not by message: a volume transfer belongs to an
    /// authorisation a MOD issued, not to a line on a Channel, and a window may
    /// have more than one in the air. A refusal without the token is a refusal
    /// no screen can place against a progress bar.
    VolumeRecusado {
        /// The authorisation the refused stream tried to use.
        token: String,
        /// The enumerated reason.
        motivo: VolumeRefusal,
    },
    /// The link fell in the middle. **Trying again starts from zero.**
    Fell {
        /// Which message.
        client_message_id: u64,
    },
    /// Coming down.
    Receiving {
        /// Which attachment.
        attachment: u64,
        /// Bytes arrived.
        done: u64,
        /// Bytes in total.
        total: u64,
    },
    /// On the receiver's disk.
    Saved {
        /// Which attachment.
        attachment: u64,
        /// Onde ficou de verdade: «foto (2).png», se o nome estava tomado e foi
        /// ao lado que ele ficou. É este, e não o nome que veio, que a tela
        /// mostra depois de salvar.
        path: String,
    },
    /// It did not save, and why.
    NotSaved {
        /// Which attachment.
        attachment: u64,
        /// Por quê. Cada motivo manda a pessoa fazer uma coisa diferente.
        reason: NotSavedReason,
    },
}

/// Por que um anexo não foi, ou não seria, gravado.
///
/// Enumerado pelo motivo de sempre nesta fronteira: quem escreve a frase é a
/// casca. E são quatro porque mandam coisas diferentes — um nome recusado não
/// adianta tentar de novo, e uma gravação que falhou no meio adianta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum NotSavedReason {
    /// O nome que veio com o arquivo não serve de nome de arquivo no Windows, no
    /// Mac e no Linux ao mesmo tempo: tem um caminho, um caractere ou um nome
    /// que o Windows não aceita, um caractere invisível que disfarça a extensão,
    /// ou é comprido demais. Nomes comuns do Mac e do Linux caem aqui também —
    /// «Notas 04:10.txt», «Por quê?.pdf» —, porque quem mandou não sabe onde o
    /// arquivo vai ser salvo. A regra mora em `seele_core::anexo_no_disco`, e
    /// qual dela pegou vai para o `seele.log`.
    NomeRecusado,
    /// O anexo não está no histórico que esta sessão carregou. O nome sai dele,
    /// e um anexo fora dele é recusado em vez de ganhar um nome vindo de fora.
    AnexoDesconhecido,
    /// Não há pasta onde gravar: a casca não achou a de downloads nem a pessoal,
    /// ou passou uma que não é um caminho absoluto.
    SemPasta,
    /// Não deu para gravar: a pasta não deixou criar o parcial (sem permissão,
    /// pasta que não existe), o disco recusou no meio, os bytes não fecharam com
    /// o hash, nada chegou a tempo, ou os noventa e nove nomes possíveis
    /// estavam tomados na hora de dar o nome final — que é depois de o hash
    /// conferir. Qual foi a causa vai para o `seele.log`.
    ///
    /// Os bytes chegavam num parcial e nunca tiveram o nome final. O parcial é
    /// apagado, e também a reserva vazia com o nome final que o recuo de um
    /// volume sem link físico cria; um apagar que falha vai para o `seele.log`
    /// com o caminho e o erro, e o que ele não apagou fica na pasta.
    ///
    /// Em várias das causas — as da pasta, ou um arquivo estragado no disco do
    /// servidor — tentar de novo dá no mesmo, e a frase da tela ainda manda
    /// tentar: separá-las pede um motivo no `NaoSalvou` do core, que hoje não
    /// tem.
    Falhou,
}

/// Onde um anexo vai ser gravado — para a frase de confirmação, e só para ela.
///
/// Na hora de gravar, [`crate::Connection::save_attachment`] deriva tudo de novo
/// do histórico e da pasta, e não lê nada disto de volta: uma janela que
/// devolvesse um destino poderia devolver outro.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SaveDestination {
    /// A pasta, por extenso.
    pub folder: String,
    /// O nome com que o arquivo é gravado, se ele estiver livre.
    pub file_name: String,
    /// O nome que ele ganha ao lado, se o primeiro estiver tomado.
    pub beside: String,
}

/// Por que um anexo não seria gravado, com o que a frase de recusa cita.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SaveRefused {
    /// O motivo.
    pub reason: NotSavedReason,
    /// O nome que veio com o arquivo, com todo caractere invisível ou de
    /// controle escrito por extenso — um U+202E citado como está inverteria a
    /// frase que o recusa. Vazio quando o anexo não está no histórico.
    pub claimed: String,
    /// A pasta que a casca passou, como veio: vazia quando ela não achou
    /// nenhuma, e relativa quando a recusa é `SemPasta` por ela não ser um
    /// caminho absoluto.
    pub folder: String,
}

/// Why a server would not take, or would not hand back, a file.
///
/// Mirrored here rather than re-exported from the wire, like every other
/// enumeration that crosses this boundary: the shape a shell matches on is this
/// crate's promise, and a variant renamed on the wire should break a build here
/// rather than silently change what a screen writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AttachmentRefusal {
    /// The person lacks the permission to attach.
    NotAllowed,
    /// Larger than this server's per-file limit.
    TooLarge {
        /// The largest file this server accepts, in bytes. Carried because "too
        /// big" with no number sends somebody to try again with a file that is
        /// also too big.
        limit: u64,
    },
    /// Every byte of the ceiling is held by transfers already under way.
    NoRoom,
    /// The stream ended before the declared number of bytes arrived.
    SizeMismatch,
    /// The bytes did not hash to what was declared.
    HashDidNotMatch,
    /// Bytes are going up faster than the budget allows.
    RateLimited,
    /// This server is not storing attachments at all.
    Unavailable,
    /// No such attachment, or it is in a channel this person may not read.
    NotFound,
    /// The bytes were evicted to keep the server under its ceiling.
    Expired,
    /// The header was not a header.
    Malformed,
}

/// Why a stream of bytes into a MOD's volume was not taken — ADR 0048.
///
/// Espelhado aqui e não reexportado do fio, como todo enumerado que atravessa
/// esta fronteira: a forma sobre a qual uma casca casa é promessa **deste**
/// crate, e uma variante renomeada no fio deve quebrar a compilação aqui em vez
/// de mudar calada o que uma tela escreve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum VolumeRefusal {
    /// Não há autorização com aquele token — para aquele MOD, para aquela
    /// pessoa, e dentro do prazo.
    ///
    /// **Uma recusa para os quatro casos**, e é de propósito: distinguir
    /// «token errado» de «prazo vencido» contaria a quem tenta qual metade do
    /// palpite acertou.
    SemEspera,
    /// Os bytes não são de um tipo que o MOD declarou aceitar.
    TipoRecusado,
    /// O disco de quem hospeda recusou a escrita.
    NaoGravei,
    /// O fluxo terminou no meio.
    Incompleto,
}

/// A file hanging off a message, as a screen sees it.
///
/// ADR 0027. Present **even when the bytes are gone**, which is the whole
/// reason the server keeps the row after deleting the blob: a message that had a
/// picture and now draws as an empty channel leaves nobody able to tell there had
/// been one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Attachment {
    /// What to ask for when saving it.
    pub id: u64,
    /// The name the sender gave it.
    pub file_name: String,
    /// The type the sender claimed. **A claim**, and the shell treats it as one:
    /// only a short list of image types is ever drawn, and everything else is a
    /// name and a size with no preview and no way to open it.
    pub declared_type: String,
    /// How many bytes it was.
    pub byte_size: u64,
    /// Whether the bytes are still on the server.
    pub expired: bool,
}

/// What a window may draw for one attachment, and why. ADR 0027.
///
/// The answer to one press of one button, and never anything a screen gets by
/// scrolling: the file lives on the server, so looking at it is downloading it,
/// and a channel that fetched every picture as it scrolled past would turn the
/// host's disk ceiling into everybody's uplink.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Preview {
    /// Which attachment this is about.
    pub attachment: u64,
    /// The picture, whole, as a `data:` URI — or nothing.
    ///
    /// **Composed on this side of the boundary, media type included, and the
    /// media type came from the bytes.** A shell handed the bytes and a type to
    /// join up would be a shell that could join them up with the sender's
    /// claim, which is the one thing the whole design refuses.
    pub image: Option<String>,
    /// The type the sender claimed, so a sentence can quote it back.
    pub claimed: String,
    /// What the leading bytes turned out to be, when they are a picture this
    /// product knows. `None` means they are not one.
    pub found: Option<String>,
    /// Why there is no picture. `None` exactly when [`Self::image`] is `Some`.
    pub refusal: Option<PreviewRefusal>,
}

/// What a screen may offer a preview for, before it asks for one.
///
/// Both halves are advice and neither is the rule: the rule is applied where
/// the bytes are, on the way down. A window that offers nothing here still
/// cannot draw anything it should not, and a window that offers too much gets
/// a refusal instead of a picture.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PreviewRules {
    /// The most a preview will pull down, in bytes.
    pub limit: u64,
    /// The media types that could be drawn, if their bytes agree.
    pub types: Vec<String>,
}

/// Os tetos dos campos que a casca escreve e o fio carrega (pendência 58, item
/// 55; lote C55 da F5).
///
/// # Por que a casca precisa destes números
///
/// Pela razão de [`crate::Connection::limite_da_mensagem`], cinco vezes. Os
/// campos da janela contavam **caracteres** (`maxlength`), e o fio conta
/// **bytes**: um apelido de 20 letras `á` cabe nos 32 caracteres do campo e
/// ocupa 40 bytes. Desde a rodada 1 do lote P61, o núcleo recusa esse comando
/// sem mandar o quadro e o registra no `seele.log` (`Motor::recusar_aqui`), e o
/// enlace segue; mas a janela não fica sabendo, e o pedido some calado. Com os
/// tetos aqui, a janela confere antes de mandar e diz o número.
///
/// Os números saem do `seele-proto`, pelos reexports do `seele-core`, e não de
/// uma cópia: são os que o `encode` confere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct LimitesDosCampos {
    /// O apelido, em bytes: o do `SetNickname` e o do `Hello`, que leva o
    /// apelido gravado nesta máquina (`MAX_NICKNAME_LEN`).
    pub apelido: usize,
    /// O nome de um canal ou de uma sala de voz, em bytes, ao criar e ao
    /// renomear (`MAX_CHANNEL_NAME_LEN`).
    pub nome_de_canal: usize,
    /// Quantas pessoas uma sala nova pode ter, no máximo (`MAX_VOICE_ROOM_LIMIT`).
    /// O mínimo é um: uma sala em que ninguém entra não é sala.
    pub lugares_da_sala: u16,
    /// O nome do servidor, em bytes (`MAX_CLIENT_NAME_LEN`).
    pub nome_do_servidor: usize,
    /// O motivo de um banimento, em bytes (`MAX_ALERT_TEXT_LEN`).
    pub motivo_do_banimento: usize,
}

/// Why a file was not drawn.
///
/// Four, and they are four because they ask different things of the person
/// reading. One says the file is fine and this window will not spend the memory;
/// another says the file is not what it claimed to be, which is not a transfer
/// problem and not something to retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
pub enum PreviewRefusal {
    /// The bytes are not what the file said it was.
    ///
    /// `NOTAS-DE-RELEASE.md` separates "did it arrive whole" from "is it what
    /// it says it is". The hash answered the first, with a yes. This is the
    /// second, with a no — and not drawing is not the same as hiding: the file
    /// is still on the screen, with its name, its size and its save button.
    Disagrees,
    /// Larger than this window will decode.
    TooBig {
        /// The most a preview will pull down, in bytes. Carried for the same
        /// reason [`AttachmentRefusal::TooLarge`] carries one: "too big" with
        /// no number is a sentence nobody can act on.
        limit: u64,
    },
    /// Never a picture in the first place — a document, a build, a text file.
    ///
    /// A window should not have asked, and this is what it gets if it did.
    NotAPicture,
    /// The bytes never came: expired, gone, or they did not arrive whole.
    DidNotArrive,
}

/// Onde o enlace está.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LinkState {
    /// Conectado e respondendo.
    Online,
    /// Bateria interna: caiu, e a sessão está sendo segurada.
    InternalBattery {
        /// Segundos que faltam dos cinco minutos.
        remaining_seconds: u64,
        /// Tentativas de reconexão até agora.
        attempts: u32,
    },
}

/// **De qual sessão, e de qual volta do enlace dela** (E3 da 0.17, multi-sessão).
///
/// `sessao` é o `SessaoId`: nasce na tentativa de conexão, nunca é 0 e nunca se
/// repete no processo ([`crate::SessaoId::nova`]). `geracao` começa em 1 e sobe a
/// cada reconexão do enlace daquela sessão.
///
/// Todo evento sai com ele ([`crate::EventListener::on_event_carimbado`]), e todo
/// comando de sessão entra por [`crate::Connection::com`], que recusa um carimbo
/// que não é o de agora: um clique feito sobre a tela de outra sessão, ou de
/// antes de uma reconexão, nunca cai calado na conexão errada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Carimbo {
    /// O `SessaoId` desta conexão.
    pub sessao: u64,
    /// Quantas vezes o enlace desta sessão abriu: 1 na entrada, +1 por volta.
    pub geracao: u64,
}

/// Everything that can go wrong, enumerated.
///
/// `specs/02-protocolo.md` refuses generic error strings on the wire, and the
/// same reasoning applies here: a variant carrying free text is a variant the
/// shell cannot localise and must print verbatim. Detail that helps a developer
/// goes to `tracing`, not across this boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ConnectionError {
    /// No connection, or it is already gone.
    NotConnected,
    /// **O comando veio com o carimbo de outra sessão, ou de uma volta do
    /// enlace que já passou** (E3 da 0.17). Ver [`Carimbo`] e
    /// `Connection::com`.
    ///
    /// Recusado antes de sair qualquer coisa: nada foi enviado. A casca decide
    /// se refaz o pedido com o carimbo novo ou se avisa; nunca há reenvio
    /// automático para outra sessão. Os dois carimbos vêm juntos porque é a
    /// diferença entre eles que diz o que aconteceu: sessão diferente é clique
    /// na tela errada, geração velha é clique de antes da reconexão.
    SessaoNaoConfere {
        /// O carimbo desta conexão agora.
        esperado: Carimbo,
        /// O carimbo que o comando trouxe.
        veio: Carimbo,
    },
    /// O servidor exige MODs que esta máquina ainda não aceitou. ADR 0045.
    ///
    /// **Não é uma falha: é uma pergunta.** Carrega o que a tela de aceite tem
    /// de mostrar antes de qualquer byte ser baixado — quem é cada MOD, de que
    /// repositório veio, o que declara alcançar, e quais rodam na máquina de
    /// quem hospeda.
    ///
    /// O caminho que ela abre: a tela mostra, a pessoa aceita, a casca chama o
    /// verbo que guarda a identidade, e conecta de novo.
    ModsNaoAceitos {
        /// Os MODs exigidos, como o servidor os anunciou.
        mods: Vec<ModExigido>,
        /// A identidade do conjunto — o que se guarda ao aceitar.
        conjunto: String,
    },
    /// A connection is already open on this handle.
    AlreadyConnected,
    /// The address could not be resolved.
    UnresolvableHost,
    /// Nothing answered.
    Unreachable,
    /// The handshake did not finish in time.
    ///
    /// **O servidor recebeu e demorou**, que é diferente de nada ter voltado —
    /// ver [`Self::SemResposta`].
    HandshakeTimeout,
    /// Os pacotes saíram e nada voltou.
    ///
    /// A conexão inteira expirou, antes de existir aperto de mão nenhum. Era
    /// [`Self::HandshakeTimeout`] até 05/09/2026, e a frase daquele — «tempo
    /// esgotado na sincronização inicial» — mandava quem lia investigar
    /// protocolo, versão e servidor, quando o que aconteceu foi um datagrama que
    /// não chegou ao outro lado.
    ///
    /// Relatado assim: «o Mac só dá tempo esgotado na sincronização. Como isso,
    /// se os PCs estão na mesma LAN e na mesma versão?». As duas coisas que a
    /// frase mandou conferir estavam certas.
    ///
    /// Desde a 0.17: nenhum candidato devolveu a checagem ICE (STUN).
    SemResposta,
    /// **O ponto de encontro levou o chamado e quem hospeda atendeu, mas o
    /// caminho direto não abriu** (o degrau 4 da 0.17).
    ///
    /// Típico de dois NATs que não deixam entrar o que não saiu (NAT simétrico).
    /// Não há o que insistir: só um relay TURN configurado por quem usa passa
    /// por aí, e a frase não promete mais que isso.
    NatNaoAbriu,
    /// **O link, ou o ponto de encontro, é de um Servidor 1** (a 0.16 e
    /// anteriores). A 0.17 fala WebRTC e não conversa com ele; a ação é abrir
    /// a linha legada pelo launcher.
    LinhaLegada,
    /// The server's key is not the one that was pinned.
    ///
    /// ADR 0003. The two fingerprints are carried because the whole point is
    /// that a human compares them; they are data, not an error message.
    PinChanged {
        /// What was pinned before.
        pinned: String,
        /// What was offered now.
        offered: String,
    },
    /// A impressão esperada (a do link colado nesta sessão, ou a que a lista de
    /// servidores guardou) nomeava uma chave, e quem atendeu ofereceu outra.
    ///
    /// ADR 0006, e de propósito não [`ConnectionError::PinChanged`]: nada estava
    /// fixado aqui, ou o que estava não prova o servidor — é de um endereço que
    /// a pessoa não escolheu (um candidato do convite, da lista ou do quarto do
    /// ponto de encontro), ou de um alvo de escopo local (um endereço de LAN, o
    /// mesmo de uma casa para outra) —, e o alarme de chave trocada da casca
    /// acusaria o culpado errado. A continuidade do servidor não está em
    /// questão: o que não confere é a promessa, do link ou da lista, com quem
    /// atendeu. Ou a promessa está errada, ou ela está certa e quem atendeu é
    /// outro servidor: num alvo de LAN, outro servidor no mesmo endereço,
    /// porque o endereço da rede de casa se repete de uma casa para outra. A
    /// casca diz de quem era a promessa: o `connect` do app sabe se a impressão
    /// veio do link ou da lista.
    ///
    /// **Recusada dentro do TLS, antes do `Hello`**: o convite, a senha e o
    /// apelido (e, com eles, a chave e a assinatura da identidade) não chegaram
    /// a quem atendeu com a chave errada. O que sai para ele é o `ClientHello`
    /// do TLS, com o nome, o ALPN e a chave efêmera.
    ///
    /// **O escopo é por candidato, e a frase da casca respeita isso.** Os
    /// candidatos de um convite correm em paralelo, cada um com o próprio
    /// `Hello` depois do próprio TLS, e quando todos falham o núcleo guarda a
    /// primeira falha de quem respondeu, na ordem de conclusão: um candidato que
    /// recusou no TLS pode terminar antes de outro que já entregou o convite, a
    /// senha e o apelido ao servidor verdadeiro. Por isso a frase diz que eles
    /// não chegaram «a ele», e não diz «nada», «nenhum» nem «desta máquina».
    InviteMismatch {
        /// O que a impressão esperada prometia: a do link, ou a da lista.
        expected: String,
        /// What the server offered.
        offered: String,
    },
    /// Quem atendeu não é a sala do código para o qual se ligou (ADR 0060).
    ///
    /// Recusada dentro do TLS, antes do `Hello`: nada do que quem liga diria
    /// chegou a quem atendeu. `veio` é o código da chave que quem atendeu
    /// apresentou, e `None` quando nem era a chave de uma pessoa.
    ///
    /// A frase («QUEM ATENDEU NÃO É O DONO DESTE CÓDIGO») é da casca, junto com
    /// o caminho que disca um código (tarefa 7f da sala). Até lá, nada no app
    /// disca um código, e esta variante não chega à tela.
    CodigoNaoBate {
        /// O código que quem liga digitou.
        esperado: String,
        /// O código da chave que quem atendeu apresentou.
        veio: Option<String>,
    },
    /// A chave de quem atendeu tem o código certo e não é a que ficou pinada na
    /// primeira ligação: alguém achou uma chave com o mesmo código (ADR 0060,
    /// «Força»). A frase («ALGUÉM TENTOU SE PASSAR POR …») é da casca, como a
    /// de [`Self::CodigoNaoBate`].
    SePassouPorCodigo {
        /// O código de quem se passou.
        codigo: String,
    },
    /// Quem liga pelo código não achou onde a sala mora (spec de 23/09, «Erros
    /// e frases»): o motivo é o que o ponto de encontro disse, ou a falta dele.
    ///
    /// Sai antes da corrida, sem endereço tentado: a trilha acaba em
    /// `Localizando`. As frases («O PONTO DE ENCONTRO NÃO RESPONDEU», «ALICE NÃO
    /// ESTÁ DISPONÍVEL») são da casca, com a tarefa 7f; até lá nada no app
    /// disca um código, e esta variante não chega à tela.
    NaoLocalizou {
        /// O que faltou para achar a sala.
        motivo: FalhaAoLocalizar,
    },
    /// Quem liga desistiu antes de entrar: o [`crate::Cancelamento`] desta
    /// ligação foi usado (tarefa 16 da F7). Não é falha, e a casca sabe que foi
    /// ela quem cancelou; a frase existe para a tela nunca escrever o nome cru.
    Cancelada,
    /// Quem atendeu mostrou o certificado do servidor sem provar ter a chave
    /// que o assina.
    ///
    /// O verificador aceitou o certificado (o pin conferia, ou era o primeiro
    /// contato), e o TLS caiu logo depois, na assinatura de quem atendeu: o
    /// certificado é público e se repete inteiro, e a chave privada não.
    /// Recusada dentro do TLS, antes do `Hello`, como [`Self::InviteMismatch`]:
    /// o convite, a senha e o apelido não chegaram a ele.
    ///
    /// Era `Refused { Incompatible }`, e a tela mandava atualizar o SEELE nas
    /// duas máquinas. Unitária: não há impressão a comparar, porque a que quem
    /// atendeu mostrou é a do servidor verdadeiro.
    ChaveNaoProvada,
    /// O apelido desta entrada passa do teto do fio, em **bytes**, e o `Hello`
    /// não o levaria (pendência 58, item 55; lote C55 da F5).
    ///
    /// **Recusado antes de abrir a conexão**, em `Connection::connect`: o
    /// apelido está na mão antes de qualquer endereço ser discado, e nada sai
    /// desta máquina. Era `Unreachable`: o QUIC e o TLS abriam, o `encode` do
    /// `Hello` recusava o apelido, e a escrita do `Hello` traduzia isso como
    /// «nada respondeu», com o servidor de pé. A janela conferia o apelido em
    /// caracteres, então um gravado de antes pode passar do teto. O teto vem
    /// junto, para a frase dizer o número; o desfecho é encurtar o apelido.
    ApelidoGrandeDemais {
        /// O teto do apelido, em bytes (`MAX_NICKNAME_LEN`).
        limite_bytes: u64,
    },
    /// The server refused the session, and said why.
    Refused {
        /// The enumerated reason.
        reason: EndReason,
    },
    /// **O servidor fala um número de protocolo que este app não fala**, e
    /// disse qual no byte 0 da resposta (ADR 0058).
    ///
    /// Até a 0.15, isto chegava como `Unreachable`, e a tela dizia «não foi
    /// possível alcançar» de um servidor que tinha respondido. A casca escolhe
    /// a frase pelos dois campos de baixo, e o launcher (a tarefa 13 da F6) só
    /// age com o `servidor` presente.
    OutraLinha {
        /// O número do byte 0 da resposta.
        numero: u8,
        /// A versão do servidor **só quando é uma que o launcher abre**, como
        /// ele a chama: `"0.15"` para o 8. `None` para um número de antes da
        /// 0.15, que nenhum app oferecido fala, e para um acima desta build.
        ///
        /// A tabela congelada (`linha_do_protocolo`) chama de «1» todo número
        /// do 9 em diante, mas quem recusa o número desta build não é uma 1.x:
        /// toda 1.x fala o 9 (ADR 0058). Dizer «1» ali seria a FFI dizer uma
        /// versão que o servidor não é (decisão da orquestradora, 15/10).
        servidor: Option<String>,
        /// Se o número está acima do desta build (`Linha::Posterior`): o
        /// servidor é mais novo, e o remédio é atualizar este app.
        mais_nova: bool,
    },
    /// **O servidor recusou a pergunta**, com a causa do fio
    /// (`AlertReason::PedidoRecusado`). Hoje, só a pesagem de um canal antes de
    /// apagá-lo (`weigh_channel`), o único pedido que a FFI espera responder e
    /// que o servidor recusa pelo `nao_deu`.
    PedidoRecusado {
        /// Por que não deu.
        causa: CausaDaRecusa,
    },
    /// The identity on disk could not be read or written.
    IdentityUnavailable,
    /// No usable microphone or speaker.
    NoAudioDevice,
    /// The chosen microphone is not one this machine is offering any more.
    ///
    /// Its own variant rather than [`ConnectionError::NoAudioDevice`] because the two
    /// ask different things of the person reading them: "this machine has no
    /// microphone" and "the one you picked was unplugged" have different next
    /// steps, and the second one has a list to pick from again.
    CaptureDeviceGone,
    /// The chosen output is not one this machine is offering any more.
    ///
    /// Its own variant rather than [`ConnectionError::CaptureDeviceGone`] because a
    /// shell writes one sentence per variant, and "the microphone you picked was
    /// unplugged" is the wrong sentence to read after choosing a headset. The
    /// two point at opposite halves of the screen.
    PlaybackDeviceGone,
    /// The named person is not in this voice room.
    UnknownPerson,
    /// Esta mensagem não existe no servidor.
    ///
    /// Hoje isto quer dizer uma coisa só: alguém endereçou uma **pendente** —
    /// uma mensagem que esta máquina escreveu e o servidor ainda não confirmou.
    /// `Message::id` vale zero nessas, e zero não é identificador de nada.
    ///
    /// Variante própria e não [`Self::NotConnected`]: a sessão está boa, e a
    /// frase «a conexão caiu» mandaria a pessoa olhar a rede por causa de um
    /// clique numa linha que ainda está subindo. Quem quer largar uma pendente
    /// usa `Connection::descartar_mensagem`.
    UnknownMessage,
    /// No voice room or Channel by that name or number.
    UnknownChannel,
    /// The control stream broke.
    LinkLost,
    /// The file offered as the server's icon is not a picture that fits there.
    ///
    /// Answered **before** anything is sent, by
    /// `seele_core::check_server_icon`, and that is the whole reason this
    /// variant exists: the rule lives on the wire, and a picture that breaks it
    /// makes the frame unbuildable — which everything above the link reads as a
    /// dropped connection. Without this, choosing a PDF would look exactly like
    /// the network falling over, five-minute internal battery included.
    IconNotAPicture,
    /// The picture offered as the server's icon is heavier than a server takes.
    ///
    /// Separate from [`ConnectionError::IconNotAPicture`] because the next step
    /// differs: a photograph can be shrunk, and a PDF cannot be made into an
    /// icon. The ceiling travels with it so the sentence can carry the number
    /// — a shell has no other way to name a limit that lives in the protocol.
    IconTooBig {
        /// The most bytes a server accepts.
        limit_bytes: u64,
    },
    /// Alguém já está compartilhando a tela nesta sala de voz.
    ///
    /// Uma transmissão por sala (§6.3 da spec de tela). **Não é permissão**: a
    /// pessoa pode compartilhar assim que a outra parar, e dizer
    /// [`ConnectionError::Refused`] ou o `PermissionDenied` de um aviso a mandaria
    /// procurar um papel que ela já tem.
    ///
    /// Quem decide é o servidor, e por isso este veredito chega **também** — hoje,
    /// só — pelo caminho assíncrono, como [`NoticeReason::ScreenShareTaken`]
    /// num [`Event::NoticeRaised`]. A variante existe aqui para o dia em que o
    /// pedido tiver resposta síncrona; esta ponte nunca a devolve por conta
    /// própria, porque decidir localmente quem perdeu a corrida seria a casca
    /// julgando no lugar do servidor.
    ScreenShareTaken,
    /// Esta máquina não tem como começar a compartilhar.
    ///
    /// **Não é uma recusa do servidor e não é rede.** É uma das quatro coisas que
    /// faltam **aqui**, e três delas a pessoa consegue mudar:
    ///
    /// 1. o módulo do Cisco não está em disco. O produto não vem com codec, e é
    ///    a licença que impõe isso — há um endereço de download, e o log da
    ///    ponte o escreve;
    /// 2. o sistema não concedeu gravação de tela. Aí quem responde é
    ///    [`PermissaoDeTela`], e o botão a oferecer é o de pedir;
    /// 3. a fonte escolhida não está na última lista — a janela fechou entre o
    ///    menu e o clique. Listar de novo resolve;
    /// 4. este build não tem captura de tela (o Linux da v1). Só este último não
    ///    tem conserto do lado de quem usa, e é o que
    ///    [`PermissaoDeTela::NaoSeSabe`] diz.
    ///
    /// Também é o que `Connection::ajustar_limites_da_tela` devolve, e ali o motivo
    /// é outro: a bomba não tem ordem que troque a escolha de resolução ou de
    /// cadência depois de armada. Parar e recomeçar aplica as três.
    ScreenShareUnavailable,
    /// O módulo de vídeo não está nesta máquina.
    ///
    /// **Não é o mesmo que [`Self::ScreenShareUnavailable`]**, e confundir os
    /// dois já custou dois testes de campo: aquele diz «esta compilação não
    /// captura tela», e este diz «captura, e falta um arquivo».
    ///
    /// O módulo do OpenH264 não vem no pacote por decisão de licença — a
    /// cobertura de patente do Cisco acompanha o binário que o Cisco entrega,
    /// então embrulhá-lo junto seria trocar uma patente respondida por uma
    /// contornada. O que resta é buscá-lo, uma vez, com consentimento e com o
    /// hash conferido.
    ///
    /// Quem lê a frase errada conclui que o recurso não existe e para de
    /// tentar. Quem lê esta sabe que falta um megabyte e o que fazer com isso.
    ScreenModuleMissing,
    /// O download veio, e o módulo não ficou em disco.
    ///
    /// Hash que não bate, bz2 que não abre, pasta que não aceita gravação. São
    /// três causas e uma frase só, porque a ação de quem clicou é a mesma nas
    /// três — tentar de novo — e o que as separa está no log de quem depura.
    ///
    /// Um hash que não bate quase nunca é corrupção: é uma página de erro de
    /// proxy chegando no lugar do arquivo.
    ScreenModuleRefused,
}

impl std::fmt::Display for ConnectionError {
    /// For logs and `Error`, never for a user.
    ///
    /// A shell that prints this is a shell that skipped writing its own
    /// sentence, which is what the enum exists to make it do.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ConnectionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_disconnect_reason_the_protocol_has_maps_to_one_here() {
        // If the protocol grows a reason, this fails to compile rather than
        // quietly folding the new one into something else.
        let all = [
            seele_core::DisconnectReason::Incompatible,
            seele_core::DisconnectReason::CredentialRejected,
            seele_core::DisconnectReason::HandshakeTimeout,
            seele_core::DisconnectReason::Kicked,
            seele_core::DisconnectReason::Banned,
            seele_core::DisconnectReason::ServerFull,
            seele_core::DisconnectReason::ScheduledMaintenance,
            seele_core::DisconnectReason::ServerShuttingDown,
            seele_core::DisconnectReason::Timeout,
            seele_core::DisconnectReason::ProtocolViolation,
            seele_core::DisconnectReason::RateLimited,
            seele_core::DisconnectReason::FellBehind,
            seele_core::DisconnectReason::AdmissionPending,
            seele_core::DisconnectReason::AdmissionDenied,
        ];
        let mapped: std::collections::HashSet<EndReason> =
            all.into_iter().map(EndReason::from).collect();

        assert_eq!(
            mapped.len(),
            all.len(),
            "two protocol reasons collapsed into one, and a shell can no longer tell them apart"
        );
        assert!(
            !mapped.contains(&EndReason::LinkLost),
            "LinkLost is for a link that died without the server saying why"
        );
    }

    #[test]
    fn every_alert_reason_maps_to_one_of_its_own() {
        let all = [
            seele_core::AlertReason::Mentioned,
            seele_core::AlertReason::SubsystemChanged,
            seele_core::AlertReason::SyncDegraded,
            seele_core::AlertReason::VoiceRoomEntryRefused,
            seele_core::AlertReason::PermissionDenied,
            seele_core::AlertReason::VoiceRoomFull,
            seele_core::AlertReason::OperatorNotice,
            seele_core::AlertReason::RateLimited,
            seele_core::AlertReason::MovedByOperator,
            seele_core::AlertReason::VoiceRoomDeleted,
            seele_core::AlertReason::ChannelDeleted,
            seele_core::AlertReason::LastVoiceRoom,
            seele_core::AlertReason::PedidoRecusado {
                pedido: 29,
                causa: seele_core::CausaDaRecusa::NomeTomado,
            },
        ];
        let mapped: std::collections::HashSet<NoticeReason> =
            all.into_iter().map(NoticeReason::from).collect();
        assert_eq!(mapped.len(), all.len());
    }

    /// O pedido atravessa pelo nome que a casca lê, e um ordinal que esta build
    /// não conhece não vira o nome de outro pedido.
    #[test]
    fn o_pedido_recusado_chega_pelo_nome() {
        assert_eq!(
            NoticeReason::from(seele_core::AlertReason::PedidoRecusado {
                pedido: 29,
                causa: seele_core::CausaDaRecusa::NomeTomado,
            }),
            NoticeReason::PedidoRecusado {
                pedido: "SetNickname",
                causa: CausaDaRecusa::NomeTomado,
            }
        );
        assert_eq!(
            NoticeReason::from(seele_core::AlertReason::PedidoRecusado {
                pedido: 9_999,
                causa: seele_core::CausaDaRecusa::FalhaDoServidor,
            }),
            NoticeReason::PedidoRecusado {
                pedido: "?",
                causa: CausaDaRecusa::FalhaDoServidor,
            }
        );
    }

    #[test]
    fn the_bands_agree_with_the_core() {
        // Two shells with two copies of "85 is nominal" is two shells that
        // disagree the day one of them is updated. The core decides; this only
        // relabels.
        for ratio in 0..=100u8 {
            let band: SignalBand = seele_core::SignalBand::of(ratio).into();
            let expected = match ratio {
                85..=255 => SignalBand::Nominal,
                60..=84 => SignalBand::Degraded,
                _ => SignalBand::Critical,
            };
            assert_eq!(band, expected, "ratio {ratio}");
        }
    }

    #[test]
    fn the_band_a_shell_reads_is_spelled_the_way_the_comp_bands_it() {
        // The desktop shell branches on this string. The edges are what a
        // shell would get wrong if it kept its own thresholds, so the edges
        // are what is pinned — 84 and 85, 59 and 60.
        let json = |ratio: u8| {
            let band: SignalBand = seele_core::SignalBand::of(ratio).into();
            serde_json::to_string(&band).expect("a bare enum always serialises")
        };

        assert_eq!(json(85), "\"Nominal\"");
        assert_eq!(json(84), "\"Degraded\"");
        assert_eq!(json(60), "\"Degraded\"");
        assert_eq!(json(59), "\"Critical\"");
        assert_eq!(json(0), "\"Critical\"");
        assert_eq!(json(100), "\"Nominal\"");

        // And nothing anywhere in the range still says the fourth band's name.
        for ratio in 0..=100u8 {
            assert_ne!(json(ratio), "\"Acceptable\"", "ratio {ratio}");
        }
    }

    #[test]
    fn every_verdict_the_core_produces_has_a_shell_facing_twin() {
        // Trust used to have two variants where the core now has five, and
        // folding five into two would throw away exactly the information this
        // work exists to create.
        use seele_core::tofu::Verdict;

        let cases = [
            Verdict::FirstContact {
                fingerprint: "a".into(),
            },
            Verdict::FirstContactVerified {
                fingerprint: "a".into(),
            },
            Verdict::Known,
            Verdict::InviteRefused {
                expected: "b".into(),
                offered: "a".into(),
            },
            Verdict::InviteDisagrees {
                expected: "b".into(),
                offered: "a".into(),
            },
        ];

        let seen: std::collections::BTreeSet<String> = cases
            .iter()
            .map(|verdict| format!("{:?}", Trust::from(verdict.clone())))
            .collect();

        assert_eq!(
            seen.len(),
            cases.len(),
            "two verdicts collapsed into one Trust"
        );
    }

    #[test]
    fn no_error_variant_carries_free_text() {
        // The two that carry strings carry *fingerprints*, which are data a
        // human compares — not a sentence a shell would have to print.
        let pin = ConnectionError::PinChanged {
            pinned: "aaa".into(),
            offered: "bbb".into(),
        };
        let ConnectionError::PinChanged { pinned, offered } = &pin else {
            panic!("shape changed");
        };
        assert_ne!(pinned, offered);

        // Everything else is a bare variant, which is what makes it localisable.
        assert_eq!(format!("{}", ConnectionError::Unreachable), "Unreachable");
    }

    /// The snapshot must stay cheap, and "cheap" means it carries no list that
    /// grows with the session.
    ///
    /// This reads the source because the property is invisible to the compiler
    /// and to any assertion about one value: a `Snapshot` carrying the whole
    /// conversation type-checks, serialises, and passes every other test here.
    /// What it does *not* do is stay the same size as a session goes on — and
    /// that only shows up as a person noticing the app got slower to type into,
    /// after a long conversation, on two machines. It took a real test between
    /// two machines to find it the first time, and the cost of finding it that
    /// way again is another release.
    ///
    /// The narrow rule: no `Vec<…>` field here may hold [`Message`]. Anything
    /// else that grows without bound belongs to the same family, but a rule
    /// written wider than the thing it guards is a rule people argue with.
    #[test]
    fn the_snapshot_does_not_carry_the_conversation() {
        let source = include_str!("types.rs");
        let Some(after) = source.split("pub struct Snapshot {").nth(1) else {
            panic!("`Snapshot` is no longer declared here; this guard has to move with it");
        };
        let Some(body) = after.split("\n}").next() else {
            panic!("`Snapshot` is never closed");
        };

        let campos: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|channel| !channel.starts_with("//") && channel.contains(known_field_marker()))
            .collect();

        for campo in &campos {
            assert!(
                !campo.contains("Vec<Message>"),
                "`Snapshot` carries the conversation again: `{campo}`\n\
                 Reading this costs one clone of every nickname and body already \
                 said, on every interface frame — twice a second — so a long \
                 session gets steadily slower. Use `messages_revision` and \
                 `Connection::messages`, which exist for exactly this."
            );
            assert!(
                !campo.contains("Vec<u8>"),
                "`Snapshot` carries raw bytes: `{campo}`\n\
                 The one thing that would be, today, is the Server\u{2019}s icon, and it \
                 is bounded rather than unbounded — but it is still kilobytes \
                 cloned and turned into JSON twice a second for a value that \
                 changes when somebody presses a button. Use `icon_revision` and \
                 `Connection::server_icon`, which are the same answer one size smaller."
            );
        }

        assert!(
            campos.iter().any(|campo| campo.contains("icon_revision")),
            "`icon_revision` is gone, and with it the only way a shell can tell \
             that the Server\u{2019}s picture moved without being handed it on every frame"
        );

        assert!(
            campos
                .iter()
                .any(|campo| campo.contains("messages_revision")),
            "`messages_revision` is gone, and with it the only way a shell can \
             tell that the history moved without being handed all of it"
        );
    }

    #[test]
    fn os_limites_da_tela_atravessam_pelo_nome() {
        // Este é o único valor deste módulo que viaja **para dentro**: a casca
        // gráfica o escreve em JSON e o Tauri o desserializa aqui. Um campo
        // renomeado de um lado só compila nos dois lados e falha em silêncio —
        // e o que a pessoa vê é um teto de banda que não pega.
        let escrito = r#"{"banda_bps":1200000,"altura_maxima":720,"quadros_maximos":15}"#;
        let limites: LimitesDeTela =
            serde_json::from_str(escrito).expect("a casca escreve exatamente estes três nomes");
        assert_eq!(limites.banda_bps, Some(1_200_000));
        assert_eq!(limites.altura_maxima, 720);
        assert_eq!(limites.quadros_maximos, 15);

        // «Sem limite próprio» é `null`, e não zero: zero seria um teto de zero
        // bits por segundo, que é a mesma conta que parar.
        let sem_teto: LimitesDeTela =
            serde_json::from_str(r#"{"banda_bps":null,"altura_maxima":1080,"quadros_maximos":30}"#)
                .expect("`null` é como a casca diz «sem limite próprio»");
        assert_eq!(sem_teto.banda_bps, None);
    }

    /// **A transmissão atravessa pelo nome, e diz de qual tela ela fala.**
    ///
    /// # O que saiu deste teste no R20
    ///
    /// Ele exigia `altura`, `quadros`, `kbps` e `medida`, e a asserção que mais
    /// valia era `"medida":false` — «a ponte diz ter medido o que não mediu».
    /// Estava certa, e era uma regra sobre quatro campos que **nunca** tiveram
    /// valor: os três números saíam zerados e o quarto saía `false`, sempre.
    ///
    /// Os quatro saíram da ponte, e a regra que eles serviam continua valendo
    /// noutro lugar: um campo de medida só volta quando houver medida, ligado a
    /// um `ScreenId`. Ver a nota no corpo de [`TelaEmCurso`].
    ///
    /// O que entrou no lugar é `tela`, e é o que o R18 pedia: sem ela, nada no
    /// painel dizia de qual transmissão ele falava.
    #[test]
    fn a_tela_em_curso_atravessa_pelo_nome_e_diz_de_qual_tela_fala() {
        let tela = TelaEmCurso {
            tela: 9,
            de: 3,
            e_minha: false,
            espectadores: 6,
            parada: None,
            pedido: Some(LimitesDeTela {
                banda_bps: Some(1_200_000),
                altura_maxima: 1080,
                quadros_maximos: 30,
                prioridade: Prioridade::Nitidez,
                com_som: true,
            }),
        };
        let json = serde_json::to_string(&tela).expect("uma estrutura simples sempre serializa");

        for nome in [
            "\"tela\"",
            "\"de\"",
            "\"e_minha\"",
            "\"espectadores\"",
            "\"parada\"",
            "\"pedido\"",
        ] {
            assert!(json.contains(nome), "{nome} não atravessa: {json}");
        }

        // **A identidade da transmissão, e é a metade que faltava.** `tela_de`
        // pegava a primeira do mapa da sala, então autoria, controles e
        // metadados podiam falar de uma tela enquanto a imagem era de outra.
        assert!(
            json.contains("\"tela\":9"),
            "a transmissão atravessa sem dizer qual ela é: {json}"
        );
        // E nenhum campo de medida voltou sem medida por trás dele.
        for inventado in ["\"altura\"", "\"quadros\"", "\"kbps\"", "\"medida\""] {
            assert!(
                !json.contains(inventado),
                "{inventado} voltou à ponte: se há medida agora, ela tem de dizer \
                 o que mediu e de qual tela — e este teste tem de passar a \
                 exigir isso em vez de proibir o campo: {json}"
            );
        }

        // E o pedido atravessa com os **mesmos três nomes** com que a casca o
        // escreveu ao mandá-lo (`LimitesDeTela`, logo acima). Renomear um deles
        // aqui deixaria a coluna do que foi pedido vazia sem nada falhar: os
        // dois lados continuariam compilando e a comparação do §5 sumiria da
        // tela.
        for nome in ["\"banda_bps\"", "\"altura_maxima\"", "\"quadros_maximos\""] {
            assert!(
                json.contains(nome),
                "{nome} não atravessa dentro do pedido: {json}"
            );
        }
    }

    #[test]
    fn quem_so_assiste_nao_recebe_um_pedido_que_nao_e_dele() {
        // O teto é escolha de quem compartilha, e ela não viaja: o
        // `ScreenHeader` carrega resolução e codec, nunca o que a pessoa pediu.
        // Preencher esta coluna para quem assiste só seria possível copiando a
        // medida para o lugar da escolha — e aí as duas metades que o §5 manda
        // pôr lado a lado passariam a ser o mesmo número, sempre iguais, sempre
        // dizendo que o teto nunca apertou.
        let alheia = TelaEmCurso {
            tela: 9,
            de: 3,
            e_minha: false,
            espectadores: 6,
            parada: None,
            pedido: None,
        };
        let json = serde_json::to_string(&alheia).expect("uma estrutura simples sempre serializa");
        assert!(
            json.contains("\"pedido\":null"),
            "o pedido de outra pessoa foi inventado deste lado: {json}"
        );
    }

    #[test]
    fn a_permissao_de_tela_atravessa_com_os_nomes_que_a_casca_ramifica() {
        let nome = |permissao: PermissaoDeTela| {
            serde_json::to_string(&permissao).expect("um enum simples sempre serializa")
        };
        assert_eq!(nome(PermissaoDeTela::Concedida), "\"Concedida\"");
        assert_eq!(nome(PermissaoDeTela::Negada), "\"Negada\"");
        assert_eq!(nome(PermissaoDeTela::NaoPerguntada), "\"NaoPerguntada\"");
        // A quarta, que não estava no contrato de 22/08. Uma casca que a
        // tratasse como `NaoPerguntada` desenharia um botão de pedir permissão
        // que não tem a quem pedir.
        assert_eq!(nome(PermissaoDeTela::NaoSeSabe), "\"NaoSeSabe\"");
    }

    #[test]
    fn a_tela_tem_evento_proprio_no_barramento() {
        // Separado de `RosterChanged` pelo motivo que `seele_core::Changed` já
        // dá: o que se mexe é um painel, e uma casca que redesenhasse a lista de
        // pessoas inteira por causa de um número de kbps estaria redesenhando a
        // parte da tela que não mudou.
        let json = serde_json::to_string(&Event::ScreenChanged)
            .expect("uma variante sem campo sempre serializa");
        assert_eq!(json, "\"ScreenChanged\"");
    }

    #[test]
    fn a_tela_tomada_nao_e_uma_recusa_de_permissao() {
        // Duas variantes, e a distinção é o produto inteiro: quem perde a
        // corrida pode compartilhar assim que o outro parar, e `PermissionDenied`
        // a mandaria procurar um papel que ela já tem.
        assert_ne!(
            serde_json::to_string(&ConnectionError::ScreenShareTaken).ok(),
            serde_json::to_string(&ConnectionError::ScreenShareUnavailable).ok(),
            "as duas respostas de tela viraram a mesma, e as saídas delas são opostas"
        );
        assert_ne!(
            NoticeReason::ScreenShareTaken,
            NoticeReason::PermissionDenied
        );
    }

    /// What a field declaration looks like, so a doc channel is not mistaken for one.
    fn known_field_marker() -> &'static str {
        "pub "
    }
}
