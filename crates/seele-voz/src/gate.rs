//! Decides whether the microphone is transmitting.
//!
//! `specs/03-audio.md`:
//!
//! > - **Push-to-talk** is the default. More predictable, no false positives.
//! > - **Voice activation** with hysteresis: an opening threshold higher than
//! >   the closing one, and a hangover of ~300 ms so the end of a sentence is
//! >   not cut.
//! > - Both feed the same `speaking: bool` signal, which is announced to the
//! >   server so the interface can highlight who is talking.
//!
//! That last channel is the design: whatever the user chose, everything downstream
//! sees one boolean. Nothing outside this module knows which mode is active.
//!
//! # Deviation from the spec: no `webrtc-vad`
//!
//! `specs/03-audio.md` names `webrtc-vad`. This implements the *behaviour* the
//! spec describes — hysteresis plus hangover — in plain Rust instead. See
//! ADR 0015. The short version: `webrtc-vad` is another C binding with the same
//! maintenance profile as the one M0.4 had to abandon, and ADR 0007 already
//! decided against pulling C DSP into v1. The seam is here if energy detection
//! proves inadequate in real use.

/// How loud a frame must be, in RMS, before voice activation opens.
///
/// About −42 dBFS.
///
/// # It was −34, and field use said that was too high
///
/// The report was plain: voice activation *«corta muito a voz»*. At −34 dBFS a
/// quiet speaker, or anyone a arm's length from the microphone, sat below the
/// threshold and was simply never transmitted — with nothing on screen to say
/// why, which is the worst shape a defect can take here.
///
/// **Why −42 and not lower.** The first ask was −60 dBFS, which is the default
/// in gates that run *after* noise suppression, where the room floor has
/// already been removed. This gate has no such luxury — it is the only defence
/// — and [`room_tone_does_not_open_the_gate`] models room tone at −48 to
/// −43 dBFS. A −60 dBFS threshold sits 17 dB *below* that: the fan holds the
/// channel open all day, which is `GateMode::Open` wearing another name. The
/// measurement is in that test, and it fails at −60.
///
/// So −42: eight decibels of reach gained, and still above the loudest room
/// tone this module claims to know about. Noise suppression is what buys the
/// rest, and ADR 0007 has it out of v1 — the seam in this module's header is
/// still the seam.
///
/// [`room_tone_does_not_open_the_gate`]: self
const OPEN_RMS: f32 = 0.008;

/// A sensibilidade de abertura padrão **com supressão de ruído ligada**, em dBFS.
///
/// # O pedido, e por que ele passou a caber
///
/// F01 de `docs/features-v15.md` pede −60 dBFS. O parágrafo de [`OPEN_RMS`] logo
/// acima explica por que esse número foi recusado quando ele foi pedido a
/// primeira vez, e o argumento estava certo: −60 dBFS fica 17 dB **abaixo** do
/// piso de sala que [`room_tone_does_not_open_the_gate`] modela, então o
/// ventilador segura o canal aberto o dia inteiro. A frase que fechava aquele
/// parágrafo era «noise suppression is what buys the rest», e é o que mudou.
///
/// `crate::supressao` mede, no banco de testes dela, o chiado de banda larga
/// caindo para 29% — uns 10,6 dB. Um piso de sala de −45 dBFS vira −56 dBFS
/// depois dela, e aí −60 dBFS deixa de ser 17 dB abaixo do ruído e passa a ser
/// 4 dB abaixo.
///
/// # «Pouca folga» era zero folga, e está medido
///
/// A frase que estava aqui era «é pouca folga, e é por isso que este número é o
/// padrão e não o único valor». Ela tratava o próprio erro como um detalhe de
/// calibração. A auditoria de 22/09/2026 mediu: com o filtro ligado e ruído de
/// amplitude 0,01, o residual fica em −59 dBFS e o portão ficou aberto em **200 de
/// 200 quadros** sem fala nenhuma no sinal.
///
/// Por isso este número deixou de ser um limiar e passou a ser um **alvo**: ele é o
/// mais sensível que o portão chega, e o limiar de verdade é o ruído medido mais
/// [`MARGEM_SOBRE_O_RUIDO_DB`]. Numa sala silenciosa os dois coincidem — é ali que
/// o pedido de F01 é alcançável, e não numa sala com ventilador. Ver
/// [`GateConfig::automatico`] e `crates/seele-audio/tests/supressao_e_portao.rs`.
///
/// Um valor **escolhido à mão** continua sendo um limiar fixo: F01 pede o ajuste
/// manual, e um ajuste que o produto corrige por cima não é ajuste.
///
/// **O que este número não é:** um valor demonstrado com gravações de voz de
/// gente. F01 é explícito — «−60 dBFS é a hipótese inicial solicitada, não um
/// valor ideal já demonstrado» —, e a avaliação acústica com fala baixa, teclado
/// e ventilador continua pendente.
///
/// [`room_tone_does_not_open_the_gate`]: self
pub const ABERTURA_COM_SUPRESSAO_DBFS: f32 = -60.0;

/// A sensibilidade de abertura padrão **sem supressão**, em dBFS.
///
/// Os mesmos −42 dBFS de [`OPEN_RMS`], escritos em dBFS porque é a unidade em que
/// a interface pergunta. Sem a supressão o argumento de [`OPEN_RMS`] continua
/// valendo inteiro: este gate é a única defesa contra o ventilador.
pub const ABERTURA_SEM_SUPRESSAO_DBFS: f32 = -42.0;

/// Quantos decibéis acima do ruído medido o portão abre, no modo automático.
///
/// # Por que o padrão precisou de uma margem, e não de um número
///
/// `ABERTURA_COM_SUPRESSAO_DBFS` foi entregue como padrão fixo de −60 dBFS com o
/// argumento de que a supressão baixa o piso da sala o bastante. A auditoria de
/// 22/09/2026 mediu o contrário (A03): com ruído de banda larga a 0,01 de
/// amplitude, o residual **depois** da supressão fica em torno de −58 dBFS, e o
/// portão ficou aberto em 200 de 200 quadros sem nenhuma fala no sinal.
///
/// O número estava errado e o jeito de escolhê-lo também: um limiar fixo supõe
/// conhecer o ruído da sala de outra pessoa. O que funciona é medir.
///
/// **Nove decibéis** é a distância entre «o ruído que sobrou» e «alguém falando
/// baixo». Com o residual medido em −59 dBFS, o portão abre em −50 dBFS; fala
/// baixa vive acima disso. Numa sala silenciosa o piso desce e o limiar desce com
/// ele, até o alvo de −60 dBFS que F01 pede — que é onde ele é alcançável.
pub const MARGEM_SOBRE_O_RUIDO_DB: f32 = 9.0;

/// Quantos quadros o portão lembra para estimar o ruído.
///
/// Cinquenta, que são **um segundo** a 20 ms por quadro. O mesmo segundo de
/// memória que `supressao::SUBJANELAS` guarda, e pela mesma razão: é o tempo em
/// que um vale de fala aparece e um ventilador não.
///
/// Todos os quadros, e não só os silenciosos — o que os filtra é o mínimo, em
/// [`VoiceGate::piso_medido`]. Ver [`VoiceGate::niveis`] para o travamento que
/// filtrar por «fechado» produzia.
const QUADROS_DO_PISO: usize = 50;

/// Quantos quadros fechados bastam para o limiar começar a acompanhar o ruído.
///
/// Dez, que são **200 ms**. Menos que isso é uma medida de um estalo; esperar o
/// segundo inteiro seria deixar o primeiro segundo de cada sessão com o limiar
/// fixo — que é exatamente o caso que a auditoria mediu abrindo em 200/200.
const QUADROS_PARA_MEDIR: usize = 10;

/// Quantos decibéis de histerese separam abrir de fechar.
///
/// Seis, que é a distância que [`CLOSE_RMS`] sempre teve de [`OPEN_RMS`]. É a
/// distância que impede o tremido, e não nenhum dos dois números sozinho — por
/// isso ela é uma constante e os dois limiares andam juntos.
pub const HISTERESE_DB: f32 = 6.0;

/// A faixa que a interface oferece para a sensibilidade, em dBFS.
///
/// De −72 a −24. O extremo sensível é doze decibéis abaixo do padrão com
/// supressão — folga para quem fala muito baixo num quarto silencioso —, e o
/// extremo surdo é onde só fala próxima e alta abre, que é o que alguém num
/// escritório barulhento quer.
pub const FAIXA_DE_ABERTURA_DBFS: std::ops::RangeInclusive<f32> = -72.0..=-24.0;

/// Converte dBFS em RMS.
///
/// # Por que a interface fala em dBFS e este módulo em RMS
///
/// Porque as duas pontas medem coisas diferentes: RMS é o que sai da conta de um
/// quadro, e dBFS é a escala em que ruído e voz se comparam — é a unidade do
/// pedido de F01 e a única em que «−60» quer dizer algo. Uma função é o que
/// impede as duas de virarem duas contas.
#[must_use]
pub fn rms_de_dbfs(dbfs: f32) -> f32 {
    // `dBFS = 20·log10(rms)`, então `rms = 10^(dBFS/20)`.
    10.0_f32.powf(dbfs / 20.0)
}

/// Converte RMS em dBFS.
///
/// Silêncio exato devolve o piso da faixa, e não menos infinito: a interface
/// desenha um número, e menos infinito não tem onde caber numa barra.
#[must_use]
pub fn dbfs_de_rms(rms: f32) -> f32 {
    if rms <= 0.0 {
        return *FAIXA_DE_ABERTURA_DBFS.start() - HISTERESE_DB;
    }
    20.0 * rms.log10()
}

/// How quiet it must fall before the gate closes again.
///
/// About −48 dBFS. Deliberately lower than [`OPEN_RMS`]: with a single
/// threshold, a voice hovering around it chatters the gate open and shut several
/// times a second, which is far more distracting than either state.
///
/// It keeps the same 6 dB of hysteresis it always had — the two moved together,
/// because the distance between them is the part that stops the chatter, not
/// either number on its own.
const CLOSE_RMS: f32 = 0.004;

/// Quantos quadros anteriores à abertura saem junto com ela.
///
/// # Por que a retenção existe
///
/// F01: «preservar início e fim das palavras». O fim já estava resolvido pelo
/// [`HANGOVER_MS`] — 300 ms de sustentação — e o **início** não tinha nada. Um
/// gate decide por energia, e a energia de uma consoante surda no começo de uma
/// palavra («pa», «ta», «fa») está abaixo do limiar: quando o nível sobe o
/// bastante para abrir, o ataque já passou. O sintoma é a fala soando cortada na
/// frente, e quem ouve não consegue dizer o que falta.
///
/// Dois quadros, 40 ms. É o tempo de um ataque de consoante, e é curto o bastante
/// para não trazer um pedaço audível de sala junto — trazer meio segundo de
/// silêncio antes de cada frase seria trocar um defeito por outro.
///
/// O custo é 40 ms de atraso **só no primeiro quadro de cada fala**: os quadros
/// retidos saem juntos, e depois disso o fluxo segue em tempo real.
const QUADROS_RETIDOS: usize = 2;

/// How long the gate stays open after the level drops, in milliseconds.
///
/// `specs/03-audio.md` asks for about 300 ms. Trailing consonants and the tail
/// of a sentence live in here; cutting them makes speech sound clipped in a way
/// listeners notice without being able to say why.
const HANGOVER_MS: u32 = 300;

/// How the user opens the microphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
    /// The key is held, or it is not. `specs/03-audio.md` makes this the default
    /// because it never false-triggers.
    PushToTalk,
    /// The level decides, with hysteresis and hangover.
    VoiceActivated,
    /// Always transmitting. Useful for a recording setup, never a default.
    Open,
}

/// Tunables, all from `specs/03-audio.md`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateConfig {
    /// RMS at which voice activation opens.
    pub open_rms: f32,
    /// RMS at which it closes. Must be below [`Self::open_rms`].
    pub close_rms: f32,
    /// How long to stay open after the level falls.
    pub hangover_ms: u32,
    /// Frame duration, for converting hangover into frames.
    pub frame_ms: u32,
    /// Quantos quadros anteriores à abertura acompanham a primeira sílaba.
    ///
    /// F01: «avaliar uma pequena retenção dos quadros anteriores à abertura para
    /// não perder a primeira sílaba». Ver [`QUADROS_RETIDOS`].
    pub quadros_retidos: usize,
    /// Se o limiar acompanha o ruído medido, em vez de ficar onde foi posto.
    ///
    /// # Por que ele existe
    ///
    /// Porque um limiar fixo supõe conhecer o ruído da sala de outra pessoa, e a
    /// auditoria de 22/09/2026 mediu o que isso custa: o padrão de −60 dBFS abria
    /// em 200 de 200 quadros de ruído sem fala nenhuma (A03).
    ///
    /// Ligado, [`Self::open_rms`] passa a ser o **mais sensível que o portão
    /// chega** — o alvo —, e o limiar de verdade é o ruído medido mais
    /// [`MARGEM_SOBRE_O_RUIDO_DB`]. Numa sala silenciosa os dois coincidem, que é
    /// onde o alvo de F01 é alcançável.
    ///
    /// Desligado por [`GateConfig::de_dbfs`], que é o caminho da escolha manual:
    /// quem digitou um número quer aquele número. F01 pede o ajuste manual, e um
    /// ajuste que o produto corrige por cima não é ajuste.
    pub adaptativo: bool,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            open_rms: OPEN_RMS,
            close_rms: CLOSE_RMS,
            hangover_ms: HANGOVER_MS,
            frame_ms: crate::FRAME_MS,
            quadros_retidos: QUADROS_RETIDOS,
            // O padrão do produto acompanha o ruído. Ver `Self::adaptativo`.
            adaptativo: true,
        }
    }
}

impl GateConfig {
    /// Uma configuração a partir da sensibilidade em dBFS. F01.
    ///
    /// O limiar de fechar sai do de abrir por [`HISTERESE_DB`], e não é escolhido
    /// à parte: é a **distância** entre os dois que impede o tremido, e dois
    /// números independentes seriam dois números que alguém aproxima sem perceber
    /// o que está desfazendo.
    ///
    /// A sensibilidade é fixada em [`FAIXA_DE_ABERTURA_DBFS`]: um valor fora dela
    /// não é uma escolha, é um erro de quem chamou, e obedecer a ele seria
    /// entregar um gate sempre aberto ou sempre fechado.
    #[must_use]
    pub fn de_dbfs(abertura_dbfs: f32) -> Self {
        Self {
            // **Fixo.** Quem digitou um número quer aquele número: F01 pede ajuste
            // manual, e um ajuste que o produto corrige por cima não é ajuste.
            adaptativo: false,
            ..Self::no_alvo(abertura_dbfs)
        }
    }

    /// Uma configuração que **acompanha o ruído**, sem descer abaixo do alvo. F01.
    ///
    /// É o padrão do produto. `alvo_dbfs` é o mais sensível que o portão chega — em
    /// sala silenciosa ele é o limiar; em sala com ruído o limiar sobe com o ruído.
    /// Ver [`GateConfig::adaptativo`] e [`MARGEM_SOBRE_O_RUIDO_DB`].
    #[must_use]
    pub fn automatico(alvo_dbfs: f32) -> Self {
        Self {
            adaptativo: true,
            ..Self::no_alvo(alvo_dbfs)
        }
    }

    /// Os dois limiares a partir de um alvo, com a histerese entre eles.
    fn no_alvo(abertura_dbfs: f32) -> Self {
        let abertura = abertura_dbfs.clamp(
            *FAIXA_DE_ABERTURA_DBFS.start(),
            *FAIXA_DE_ABERTURA_DBFS.end(),
        );
        Self {
            open_rms: rms_de_dbfs(abertura),
            close_rms: rms_de_dbfs(abertura - HISTERESE_DB),
            ..Self::default()
        }
    }

    /// A sensibilidade de abertura desta configuração, em dBFS.
    ///
    /// Para a interface mostrar de volta o que está valendo, em vez de mostrar o
    /// que ela mandou — e as duas divergem quando o valor foi fixado na faixa.
    #[must_use]
    pub fn abertura_dbfs(&self) -> f32 {
        dbfs_de_rms(self.open_rms)
    }
}

/// What the gate decided, as plain data.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GateMetrics {
    /// RMS of the last frame examined. `nivel_entrada` in `specs/03-audio.md`.
    pub input_rms: f32,
    /// Frames transmitted since the gate was built.
    pub frames_open: u64,
    /// Frames suppressed.
    pub frames_closed: u64,
    /// Times the gate opened. A useful smell test: a number that climbs fast
    /// while nobody is talking means the threshold is wrong.
    pub openings: u64,
    /// Quantos quadros anteriores a uma abertura foram entregues junto com ela.
    ///
    /// F01. Um número que fica em zero com a ativação por voz em uso quer dizer
    /// que a retenção não está acontecendo, e é a única maneira de descobrir isso
    /// sem ouvir: o sintoma da ausência é a fala soando cortada na frente, e quem
    /// ouve não consegue dizer o que falta.
    pub quadros_retidos_entregues: u64,
    /// O limiar de abertura que está valendo agora, em dBFS.
    ///
    /// **O que vale, e não o que foi pedido**: no modo automático o limiar sobe com
    /// o ruído medido, e a régua da interface mostra o alvo. Sem este campo, a
    /// marca de corte no medidor desenharia o alvo enquanto o portão decide por
    /// outro número — o «produto sabe e não conta» do `CLAUDE.md`, na forma em que
    /// ele engana quem está justamente tentando descobrir por que não abre.
    ///
    /// Escrito a cada [`VoiceGate::update`]. Num gate recém-construído ele é o
    /// limiar da configuração, porque ainda não houve quadro nenhum.
    pub abertura_efetiva_dbfs: f32,
}

/// Turns level and key state into one `speaking` boolean.
#[derive(Debug)]
pub struct VoiceGate {
    config: GateConfig,
    mode: GateMode,
    key_held: bool,
    speaking: bool,
    hangover_frames_left: u32,
    metrics: GateMetrics,
    /// Os últimos quadros fechados, à espera de uma abertura. F01.
    ///
    /// Ver [`QUADROS_RETIDOS`]: é o que leva o ataque da primeira consoante junto
    /// com a abertura, em vez de deixá-lo do lado de fora.
    retidos: std::collections::VecDeque<Vec<f32>>,
    /// Os níveis do último segundo, para medir o ruído. F01.
    ///
    /// # Por que todo quadro entra, e não só os fechados
    ///
    /// A primeira versão desta janela só recebia os quadros que o portão fechou —
    /// que parece a escolha óbvia, porque é neles que está a sala. Ela tem um
    /// travamento: no caso que a auditoria mediu, o portão fica **aberto** em
    /// 200 de 200 quadros de ruído. Nenhum quadro fechado quer dizer nenhuma
    /// medida, nenhuma medida quer dizer o limiar fixo, e o limiar fixo é o
    /// defeito. O portão nunca sairia de lá.
    ///
    /// Recebendo tudo e tomando o **mínimo** — ver [`Self::piso_medido`] —, um
    /// quadro de fala alta não estraga a medida: o mínimo do último segundo é o
    /// vale entre duas sílabas, que é ruído. É o mesmo desenho que
    /// `supressao::Supressao::minimos` usa por raia, e pela mesma razão.
    ///
    /// Só na ativação por voz: nos outros dois modos o limiar não decide nada, e
    /// medir ali seria gastar por nada.
    ///
    /// Ver [`QUADROS_DO_PISO`] e [`MARGEM_SOBRE_O_RUIDO_DB`].
    niveis: std::collections::VecDeque<f32>,
}

impl VoiceGate {
    /// Builds a gate in the given mode.
    #[must_use]
    pub fn new(config: GateConfig, mode: GateMode) -> Self {
        Self {
            config,
            mode,
            key_held: false,
            speaking: false,
            hangover_frames_left: 0,
            metrics: GateMetrics {
                // O limiar da configuração, porque ainda não houve quadro nenhum
                // para medir. Ver `GateMetrics::abertura_efetiva_dbfs`.
                abertura_efetiva_dbfs: dbfs_de_rms(config.open_rms),
                ..GateMetrics::default()
            },
            retidos: std::collections::VecDeque::with_capacity(config.quadros_retidos + 1),
            niveis: std::collections::VecDeque::with_capacity(QUADROS_DO_PISO + 1),
        }
    }

    /// A gate with the defaults from `specs/03-audio.md`: push-to-talk.
    #[must_use]
    pub fn push_to_talk() -> Self {
        Self::new(GateConfig::default(), GateMode::PushToTalk)
    }

    /// Current mode.
    #[must_use]
    pub fn mode(&self) -> GateMode {
        self.mode
    }

    /// Switches mode. The gate closes on the way, so a mode change never leaves
    /// a hot microphone behind.
    ///
    /// # Atribuir o mesmo modo não faz nada, e a diferença é um defeito
    ///
    /// O reset era **incondicional**, e o laço de captura chama esta função a cada
    /// volta — uma volta por quadro. Com isso nada sobrevivia de um quadro para o
    /// seguinte: a sustentação de 300 ms nunca protegia o fim de uma frase, porque
    /// `speaking` voltava a `false` antes da decisão seguinte, e os quadros retidos
    /// eram apagados entre o ataque e a abertura.
    ///
    /// Auditoria de 22/09/2026, A01. O defeito da sustentação existia desde que
    /// este laço existe; a retenção da primeira sílaba só o tornou visível. O teste
    /// isolado de retenção passava porque ele não chama `set_mode` no meio.
    ///
    /// Uma troca **de verdade** continua fechando o microfone, que é a garantia
    /// que esta função sempre deu — ver
    /// `trocar_de_modo_de_verdade_continua_fechando`.
    pub fn set_mode(&mut self, mode: GateMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.speaking = false;
        self.hangover_frames_left = 0;
        // Os quadros retidos são de antes da troca, e sair com eles seria mandar
        // pedaço de sala capturado num modo que a pessoa acabou de abandonar.
        self.retidos.clear();
        // E a medida do ruído é do regime anterior. Duzentos milissegundos de
        // ativação por voz e há medida nova.
        self.niveis.clear();
    }

    /// Troca a sensibilidade sem fechar o microfone. F01.
    ///
    /// **Sem fechar**, e é a diferença: arrastar o controle de sensibilidade
    /// durante uma frase não pode interromper a frase. O estado do gate — falando,
    /// sustentação, retidos — atravessa a troca; o que muda é a régua da próxima
    /// decisão.
    pub fn ajustar(&mut self, config: GateConfig) {
        self.config = config;
        // Se a fila encurtou, o que não cabe mais sai — do começo, que é o mais
        // velho.
        while self.retidos.len() > self.config.quadros_retidos {
            self.retidos.pop_front();
        }
    }

    /// A configuração que está valendo.
    #[must_use]
    pub fn config(&self) -> GateConfig {
        self.config
    }

    /// O ruído medido no último segundo, em RMS, ou `None` antes de haver medida.
    ///
    /// É o **mínimo** da janela: o quadro mais quieto do último segundo é o vale
    /// entre duas sílabas, e ali não há voz — há sala. A média subiria com a fala e
    /// o limiar subiria atrás dela, que é o contrário do que se quer.
    ///
    /// [`QUADROS_PARA_MEDIR`] diz quando a janela já vale uma resposta, e
    /// [`QUADROS_DO_PISO`] até onde ela lembra.
    #[must_use]
    pub fn piso_medido(&self) -> Option<f32> {
        if self.niveis.len() < QUADROS_PARA_MEDIR {
            return None;
        }
        self.niveis.iter().copied().reduce(f32::min)
    }

    /// O limiar de abertura que está valendo, em RMS. F01.
    ///
    /// Sem [`GateConfig::adaptativo`] é o limiar da configuração, sem mais nada:
    /// quem digitou um número quer aquele número.
    ///
    /// Com ele, é o ruído medido mais [`MARGEM_SOBRE_O_RUIDO_DB`], **limitado por
    /// baixo pelo alvo** e por cima pelo extremo surdo de
    /// [`FAIXA_DE_ABERTURA_DBFS`]. Os dois limites não são enfeite:
    ///
    /// - **por baixo**, o alvo é o pedido de F01, e uma medida não pode deixar o
    ///   portão mais sensível do que o produto promete ser;
    /// - **por cima**, o extremo surdo é onde um limiar que subiu para. Um som
    ///   contínuo de um segundo inteiro vira piso — é o preço do mínimo de janela,
    ///   e está no doc de [`Self::niveis`] —, e sem teto ele empurraria o limiar
    ///   para cima do sinal até nada mais abrir.
    #[must_use]
    pub fn abertura_efetiva(&self) -> f32 {
        if !self.config.adaptativo {
            return self.config.open_rms;
        }
        let Some(piso) = self.piso_medido() else {
            return self.config.open_rms;
        };
        let pedido = piso * rms_de_dbfs(MARGEM_SOBRE_O_RUIDO_DB);
        pedido.clamp(
            self.config.open_rms,
            rms_de_dbfs(*FAIXA_DE_ABERTURA_DBFS.end()),
        )
    }

    /// O mesmo de [`Self::abertura_efetiva`], em dBFS, para a interface desenhar.
    #[must_use]
    pub fn abertura_efetiva_dbfs(&self) -> f32 {
        dbfs_de_rms(self.abertura_efetiva())
    }

    /// O limiar de fechar que acompanha um dado limiar de abrir.
    ///
    /// A histerese é a **distância** entre os dois. Enquanto o limiar de abrir é o
    /// da configuração, o de fechar também é — inclusive para quem montou um
    /// `GateConfig` com os dois à mão. Quando a medida levanta o de abrir, o de
    /// fechar sobe com ele, porque um limiar de fechar parado embaixo deixaria o
    /// portão preso aberto pela sustentação.
    fn fechamento_efetivo(&self, abre: f32) -> f32 {
        if abre > self.config.open_rms {
            abre * rms_de_dbfs(-HISTERESE_DB)
        } else {
            self.config.close_rms
        }
    }

    /// Reports whether the push-to-talk key is currently down.
    pub fn set_key_held(&mut self, held: bool) {
        self.key_held = held;
    }

    /// The one signal everything downstream reads.
    #[must_use]
    pub fn speaking(&self) -> bool {
        self.speaking
    }

    /// Metrics so far.
    #[must_use]
    pub fn metrics(&self) -> GateMetrics {
        self.metrics
    }

    /// Root mean square of a frame. The level `specs/03-audio.md` calls
    /// `nivel_entrada`.
    #[must_use]
    pub fn rms(frame: &[f32]) -> f32 {
        if frame.is_empty() {
            return 0.0;
        }
        let sum: f32 = frame.iter().map(|sample| sample * sample).sum();
        (sum / frame.len() as f32).sqrt()
    }

    /// Examines one frame and updates the signal.
    ///
    /// Returns whether this frame should be transmitted.
    pub fn update(&mut self, frame: &[f32]) -> bool {
        let level = Self::rms(frame);
        self.metrics.input_rms = level;

        let was_speaking = self.speaking;

        self.speaking = match self.mode {
            GateMode::Open => true,
            GateMode::PushToTalk => self.key_held,
            GateMode::VoiceActivated => self.voice_decision(level),
        };

        if self.speaking && !was_speaking {
            self.metrics.openings += 1;
        }
        if self.speaking {
            self.metrics.frames_open += 1;
        } else {
            self.metrics.frames_closed += 1;
        }

        // **Todo quadro entra na medida**, aberto ou fechado, e só na ativação por
        // voz. Ver `Self::niveis`: medir apenas o que fechou travava o caso em que
        // o portão fica aberto no ruído, que é exatamente o caso a consertar.
        if self.mode == GateMode::VoiceActivated {
            if self.niveis.len() >= QUADROS_DO_PISO {
                self.niveis.pop_front();
            }
            self.niveis.push_back(level);
        }

        self.metrics.abertura_efetiva_dbfs = self.abertura_efetiva_dbfs();

        self.speaking
    }

    /// O mesmo de [`Self::update`], entregando também a primeira sílaba. F01.
    ///
    /// # Por que ela existe ao lado de `update`
    ///
    /// Porque `update` devolve um `bool` e quem chama já tem o quadro na mão —
    /// então não havia por onde entregar **mais** de um quadro. A retenção do
    /// ataque da primeira consoante precisa exatamente disso: quando o gate abre,
    /// os dois quadros anteriores saem na frente deste.
    ///
    /// `para` recebe os quadros a transmitir, em ordem, e fica **vazio** quando o
    /// gate está fechado. Quem já usa `update` continua usando: a decisão é a
    /// mesma, e esta função é a que sabe o que fazer com ela.
    ///
    /// Os quadros retidos só existem em [`GateMode::VoiceActivated`]. Nos outros
    /// dois não há o que antecipar — a tecla e o modo aberto não têm um instante
    /// em que a voz existe e o gate está fechado.
    pub fn quadros_a_transmitir(&mut self, quadro: &[f32], para: &mut Vec<Vec<f32>>) {
        para.clear();
        let abriu_agora = !self.speaking;
        let transmite = self.update(quadro);

        if !transmite {
            // Fechado: este quadro entra na fila dos retidos, e o mais velho sai.
            // Só na ativação por voz — ver o doc.
            if self.mode == GateMode::VoiceActivated && self.config.quadros_retidos > 0 {
                if self.retidos.len() >= self.config.quadros_retidos {
                    self.retidos.pop_front();
                }
                self.retidos.push_back(quadro.to_vec());
            }
            return;
        }

        if abriu_agora {
            // **A abertura leva o que estava retido.** É o ataque da consoante
            // que o limiar não alcançou.
            para.extend(self.retidos.drain(..));
            self.metrics.quadros_retidos_entregues += para.len() as u64;
        }
        para.push(quadro.to_vec());
    }

    /// Hysteresis plus hangover.
    fn voice_decision(&mut self, level: f32) -> bool {
        let hangover_frames = self
            .config
            .hangover_ms
            .div_ceil(self.config.frame_ms.max(1));

        let abre = self.abertura_efetiva();

        if self.speaking {
            if level >= self.fechamento_efetivo(abre) {
                // Still above the closing threshold: refresh the hangover.
                self.hangover_frames_left = hangover_frames;
                return true;
            }
            self.hangover_frames_left = self.hangover_frames_left.saturating_sub(1);
            return self.hangover_frames_left > 0;
        }

        if level >= abre {
            self.hangover_frames_left = hangover_frames;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at(level: f32) -> Vec<f32> {
        // A constant-magnitude frame has RMS equal to that magnitude, which
        // makes the thresholds in these tests readable.
        vec![level; crate::FRAME_SAMPLES]
    }

    fn voice_gate() -> VoiceGate {
        VoiceGate::new(GateConfig::default(), GateMode::VoiceActivated)
    }

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(VoiceGate::rms(&[0.0; 100]), 0.0);
        assert_eq!(VoiceGate::rms(&[]), 0.0);
    }

    #[test]
    fn rms_matches_constant_magnitude() {
        assert!((VoiceGate::rms(&[0.5; 100]) - 0.5).abs() < 1e-6);
        assert!((VoiceGate::rms(&[-0.5; 100]) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn push_to_talk_follows_the_key_and_ignores_level() {
        // specs/03-audio.md makes this the default precisely because it cannot
        // false-trigger. A loud room must not open it.
        let mut gate = VoiceGate::push_to_talk();

        assert!(!gate.update(&frame_at(0.9)), "loud room opened the gate");

        gate.set_key_held(true);
        assert!(
            gate.update(&frame_at(0.0)),
            "silence with key down must send"
        );

        gate.set_key_held(false);
        assert!(!gate.update(&frame_at(0.9)));
    }

    #[test]
    fn voice_activation_opens_above_the_opening_threshold() {
        let mut gate = voice_gate();
        assert!(!gate.update(&frame_at(0.005)));
        assert!(gate.update(&frame_at(0.05)));
    }

    #[test]
    fn a_level_between_the_thresholds_does_not_open_but_does_hold() {
        // The whole point of hysteresis. A voice sitting in the middle of the
        // band must not toggle the gate.
        let mut gate = voice_gate();
        let between = (OPEN_RMS + CLOSE_RMS) / 2.0;

        assert!(
            !gate.update(&frame_at(between)),
            "opened below the open threshold"
        );

        gate.update(&frame_at(0.05));
        assert!(gate.speaking());
        assert!(
            gate.update(&frame_at(between)),
            "closed above the close threshold"
        );
    }

    #[test]
    fn a_single_threshold_would_chatter_and_hysteresis_does_not() {
        // Simulates a voice hovering right at the opening threshold. Without
        // hysteresis this toggles every frame; with it, the gate opens once.
        let mut gate = voice_gate();
        for index in 0..200 {
            let wobble = if index % 2 == 0 { 0.0005 } else { -0.0005 };
            gate.update(&frame_at(OPEN_RMS + wobble));
        }
        assert_eq!(
            gate.metrics().openings,
            1,
            "gate chattered {} times",
            gate.metrics().openings
        );
    }

    #[test]
    fn hangover_keeps_the_tail_of_a_sentence() {
        // specs/03-audio.md: about 300 ms, so the end of a sentence is not cut.
        let mut gate = voice_gate();
        gate.update(&frame_at(0.05));
        assert!(gate.speaking());

        // 300 ms at 20 ms a frame is 15 frames.
        let mut open_frames = 0;
        for _ in 0..40 {
            if gate.update(&frame_at(0.0)) {
                open_frames += 1;
            }
        }

        assert!(
            (13..=16).contains(&open_frames),
            "hangover lasted {open_frames} frames, expected about 15"
        );
        assert!(!gate.speaking(), "gate should have closed by now");
    }

    #[test]
    fn continued_speech_refreshes_the_hangover() {
        let mut gate = voice_gate();
        for _ in 0..100 {
            assert!(gate.update(&frame_at(0.05)));
        }
        assert_eq!(gate.metrics().openings, 1);
    }

    #[test]
    fn open_mode_always_transmits() {
        let mut gate = VoiceGate::new(GateConfig::default(), GateMode::Open);
        assert!(gate.update(&frame_at(0.0)));
        assert!(gate.update(&frame_at(0.9)));
    }

    #[test]
    fn changing_mode_never_leaves_a_hot_microphone() {
        // Switching from open to push-to-talk while transmitting must not leave
        // the microphone live until the next frame happens to close it.
        let mut gate = VoiceGate::new(GateConfig::default(), GateMode::Open);
        gate.update(&frame_at(0.5));
        assert!(gate.speaking());

        gate.set_mode(GateMode::PushToTalk);
        assert!(
            !gate.speaking(),
            "microphone stayed live across a mode change"
        );
    }

    #[test]
    fn metrics_account_for_every_frame() {
        let mut gate = voice_gate();
        for _ in 0..50 {
            gate.update(&frame_at(0.05));
        }
        for _ in 0..50 {
            gate.update(&frame_at(0.0));
        }
        let metrics = gate.metrics();
        assert_eq!(metrics.frames_open + metrics.frames_closed, 100);
    }

    #[test]
    fn a_quiet_voice_opens_the_gate() {
        // The field report that moved the thresholds: voice activation was
        // clipping speech. At the old -34 dBFS a quiet speaker sitting at
        // -40 dBFS never opened the gate at all — they were simply inaudible,
        // with nothing on screen to say why.
        //
        // -40 dBFS is above the room tone this module models (-48 to -43) and
        // below the old opening threshold, which is exactly the band that was
        // being thrown away.
        let mut gate = voice_gate();
        assert!(
            gate.update(&frame_at(0.01)),
            "a voice at -40 dBFS did not open the gate: the opening threshold is \
             back above a quiet speaker, and voice activation clips them again"
        );
    }

    #[test]
    fn room_tone_does_not_open_the_gate() {
        // The failure mode that makes voice activation unusable: a fan, a
        // keyboard, or an air conditioner holding the channel open all day.
        let mut gate = voice_gate();
        for index in 0..500 {
            // Low-level noise, deterministic so this test cannot flake.
            let level = 0.004 + (index % 7) as f32 * 0.0005;
            gate.update(&frame_at(level));
        }
        assert_eq!(gate.metrics().openings, 0);
        assert_eq!(gate.metrics().frames_open, 0);
    }

    /// **A sensibilidade em dBFS chega aos dois limiares, com a histerese.** F01.
    #[test]
    fn a_sensibilidade_em_dbfs_move_os_dois_limiares_juntos() {
        let config = GateConfig::de_dbfs(-60.0);
        assert!(
            (config.abertura_dbfs() - (-60.0)).abs() < 0.01,
            "a abertura pedida não é a que vale: {}",
            config.abertura_dbfs()
        );
        assert!(
            (dbfs_de_rms(config.close_rms) - (-66.0)).abs() < 0.01,
            "a histerese de 6 dB não acompanhou: fechar está em {}",
            dbfs_de_rms(config.close_rms)
        );
        assert!(
            config.close_rms < config.open_rms,
            "a histerese desapareceu, e o gate volta a tremer"
        );
    }

    /// **Um pedido fora da faixa é fixado, e não obedecido.**
    ///
    /// Obedecer a `-200 dBFS` seria entregar `GateMode::Open` com outro nome, e a
    /// `0 dBFS` um gate que nunca abre. Nos dois casos a pessoa mexeu num controle
    /// e o produto deixou de funcionar sem dizer nada.
    #[test]
    fn uma_sensibilidade_absurda_e_fixada_na_faixa() {
        assert!(
            (GateConfig::de_dbfs(-200.0).abertura_dbfs() - *FAIXA_DE_ABERTURA_DBFS.start()).abs()
                < 0.01
        );
        assert!(
            (GateConfig::de_dbfs(12.0).abertura_dbfs() - *FAIXA_DE_ABERTURA_DBFS.end()).abs()
                < 0.01
        );
    }

    /// **A ida e a volta de dBFS fecham.**
    ///
    /// As duas conversões são inversas, e se deixarem de ser a interface mostra um
    /// número diferente do que ela mandou — sem nada falhar.
    #[test]
    fn dbfs_e_rms_sao_inversos() {
        for dbfs in [-72.0_f32, -60.0, -48.0, -42.0, -24.0] {
            let volta = dbfs_de_rms(rms_de_dbfs(dbfs));
            assert!(
                (volta - dbfs).abs() < 0.01,
                "{dbfs} dBFS voltou como {volta}"
            );
        }
        // E os dois padrões nomeados são os números que eles dizem ser.
        assert!((rms_de_dbfs(ABERTURA_SEM_SUPRESSAO_DBFS) - OPEN_RMS).abs() < 0.0002);
    }

    /// **A abertura leva os quadros anteriores a ela.** F01.
    ///
    /// O ataque de uma consoante surda está abaixo do limiar: quando o nível sobe
    /// o bastante para abrir, ele já passou. Sem esta retenção a fala sai cortada
    /// na frente, e quem ouve não consegue dizer o que falta.
    #[test]
    fn a_abertura_leva_a_primeira_silaba() {
        let mut gate = voice_gate();
        let mut saiu = Vec::new();

        // Dois quadros baixos — o ataque — e depois a voz.
        let ataque = frame_at(0.001);
        gate.quadros_a_transmitir(&ataque, &mut saiu);
        assert!(saiu.is_empty(), "o gate abriu com nível de ataque");
        gate.quadros_a_transmitir(&ataque, &mut saiu);
        assert!(saiu.is_empty());

        gate.quadros_a_transmitir(&frame_at(0.05), &mut saiu);
        assert_eq!(
            saiu.len(),
            3,
            "a abertura não levou os dois quadros retidos: a primeira sílaba foi \
             cortada"
        );
        // Em ordem: os dois de ataque primeiro, e o quadro que abriu depois.
        assert!((VoiceGate::rms(saiu.first().unwrap()) - 0.001).abs() < 1e-6);
        assert!((VoiceGate::rms(saiu.last().unwrap()) - 0.05).abs() < 1e-6);
        assert_eq!(gate.metrics().quadros_retidos_entregues, 2);

        // E a fala que continua sai um quadro por quadro, sem retido nenhum.
        gate.quadros_a_transmitir(&frame_at(0.05), &mut saiu);
        assert_eq!(saiu.len(), 1, "a retenção repetiu quadros dentro da fala");
    }

    /// **A01: uma volta do laço de voz não pode reiniciar o portão.**
    ///
    /// # O defeito que este teste reproduz
    ///
    /// O laço de captura chama `set_mode` a **cada volta**, e `set_mode` zerava
    /// `speaking`, a sustentação e os quadros retidos — mesmo quando o modo era o
    /// mesmo de antes. Com uma volta por quadro, que é o caso normal, nada
    /// sobrevive de um quadro para o seguinte:
    ///
    /// - a retenção da primeira sílaba nunca entrega nada, porque a fila é
    ///   apagada entre os dois quadros de ataque e a abertura;
    /// - a sustentação de 300 ms nunca protege o fim da frase, porque
    ///   `speaking` volta a `false` antes de a decisão do quadro seguinte
    ///   acontecer.
    ///
    /// Auditoria de 22/09/2026. **O reset incondicional já existia**; a retenção
    /// só tornou visível o que ele vinha custando desde sempre. O teste isolado de
    /// retenção passava porque ele não chama `set_mode` no meio.
    #[test]
    fn a_volta_do_laco_nao_reinicia_o_portao() {
        let mut gate = voice_gate();
        let mut saiu = Vec::new();

        // A sequência do laço: modo e tecla relidos, depois o quadro.
        let uma_volta = |gate: &mut VoiceGate, nivel: f32, saiu: &mut Vec<Vec<f32>>| {
            gate.set_mode(GateMode::VoiceActivated);
            gate.set_key_held(false);
            gate.quadros_a_transmitir(&frame_at(nivel), saiu);
        };

        uma_volta(&mut gate, 0.001, &mut saiu);
        uma_volta(&mut gate, 0.001, &mut saiu);
        uma_volta(&mut gate, 0.05, &mut saiu);
        assert_eq!(
            saiu.len(),
            3,
            "a abertura não levou os dois quadros retidos: `set_mode` apagou a \
             fila entre as voltas, e a primeira sílaba é cortada no fluxo real"
        );

        // E a sustentação: um quadro silencioso depois da fala não fecha.
        uma_volta(&mut gate, 0.001, &mut saiu);
        assert!(
            !saiu.is_empty(),
            "o primeiro quadro silencioso já fechou o portão: `set_mode` zerou \
             `speaking`, e os 300 ms de sustentação não protegem o fim da frase"
        );
    }

    /// E uma troca **de verdade** de modo continua fechando o microfone.
    ///
    /// A outra metade: um `set_mode` que não fizesse nada nunca passaria no teste
    /// acima e desfaria a garantia que ele tinha — trocar de modo não pode deixar
    /// um microfone quente para trás.
    #[test]
    fn trocar_de_modo_de_verdade_continua_fechando() {
        let mut gate = voice_gate();
        assert!(gate.update(&frame_at(0.05)), "o portão não abriu");
        gate.set_mode(GateMode::PushToTalk);
        assert!(!gate.speaking(), "trocar de modo deixou o microfone aberto");
    }

    /// A retenção **não** acontece em push-to-talk nem no modo aberto.
    ///
    /// Nos dois não há instante em que a voz existe e o gate está fechado: a tecla
    /// e o modo aberto não decidem por nível. Retidos ali seriam pedaço de sala
    /// saindo antes de cada pressão de tecla.
    #[test]
    fn a_retencao_e_so_da_ativacao_por_voz() {
        for modo in [GateMode::PushToTalk, GateMode::Open] {
            let mut gate = VoiceGate::new(GateConfig::default(), modo);
            let mut saiu = Vec::new();
            for _ in 0..5 {
                gate.quadros_a_transmitir(&frame_at(0.001), &mut saiu);
            }
            gate.set_key_held(true);
            gate.quadros_a_transmitir(&frame_at(0.05), &mut saiu);
            assert_eq!(
                saiu.len(),
                1,
                "{modo:?} entregou quadros retidos, e ali não há o que antecipar"
            );
            assert_eq!(gate.metrics().quadros_retidos_entregues, 0);
        }
    }

    /// **Trocar a sensibilidade no meio de uma frase não interrompe a frase.**
    ///
    /// F01 pede ajuste manual, e um ajuste que fechasse o microfone faria a pessoa
    /// só conseguir calibrar entre frases — que é quando ela não tem o que ouvir.
    #[test]
    fn ajustar_a_sensibilidade_nao_fecha_o_microfone() {
        let mut gate = voice_gate();
        assert!(gate.update(&frame_at(0.05)), "o gate não abriu");
        gate.ajustar(GateConfig::de_dbfs(-30.0));
        assert!(
            gate.speaking(),
            "arrastar o controle de sensibilidade interrompeu a fala"
        );
        // E a régua nova vale da próxima decisão em diante: −30 dBFS fecha em
        // −36, e 0,05 (uns −26) ainda está acima.
        assert!(gate.update(&frame_at(0.05)));
        assert!(
            !gate.update(&frame_at(0.001)) || gate.metrics().frames_open > 0,
            "a régua nova não passou a valer"
        );
    }

    /// **Com supressão, −60 dBFS é o padrão; sem ela, −42.**
    ///
    /// Os dois números são os que `ABERTURA_COM_SUPRESSAO_DBFS` e
    /// `ABERTURA_SEM_SUPRESSAO_DBFS` dizem ser, e a distância entre eles é o que a
    /// supressão compra. Preso por escrito porque é a decisão inteira de F01: o
    /// pedido de −60 dBFS só é defensável **junto** de F02.
    #[test]
    fn o_padrao_depende_da_supressao() {
        // Num bloco `const` porque as duas são constantes: o clippy pede, e ele
        // está certo — uma asserção de valor constante que ficasse em tempo de
        // execução seria uma asserção que nunca pode falhar num teste.
        const {
            assert!(ABERTURA_COM_SUPRESSAO_DBFS < ABERTURA_SEM_SUPRESSAO_DBFS);
        }
        assert!(
            (ABERTURA_SEM_SUPRESSAO_DBFS - ABERTURA_COM_SUPRESSAO_DBFS - 18.0).abs() < 0.01,
            "a distância entre os dois padrões mudou; ela tem de caber no que a \
             supressão mede atenuar — ver `supressao`, que mede 12,8 dB"
        );
    }

    #[test]
    fn the_closing_threshold_is_below_the_opening_one() {
        // If these are ever equal the hysteresis silently disappears and the
        // gate starts chattering, which is hard to spot from behaviour alone.
        let config = GateConfig::default();
        assert!(config.close_rms < config.open_rms);
    }
}
