//! The voice path, as one thing a shell can hold.
//!
//! `specs/01-arquitetura.md` puts all session, protocol and audio logic in this
//! crate, and ADR 0002 keeps the shells from depending on `seele-audio` at all.
//! Those two together mean the capture → encode → send → receive → jitter →
//! decode → mix → play loop cannot live in the interface, however convenient
//! that would be. The M2 spike put it in the client binary because a spike is
//! allowed to; this is the version that survives.
//!
//! What a shell gets is a handle with verbs from the product's own vocabulary —
//! mudo, Isolamento total, push-to-talk — and a telemetry snapshot. What
//! it does not get is a sample, a sequence number, or an opinion about Opus.
//!
//! # Why its own thread
//!
//! `cpal`'s streams are not `Send` on every backend, so the pipeline cannot be
//! a `tokio::spawn`ed task on the interface's runtime. It gets a dedicated
//! thread with a current-thread runtime instead. That is not a workaround: the
//! real-time path should not be sharing a scheduler with a terminal redraw.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_audio::codec::{VoiceDecoder, VoiceEncoder, DEFAULT_BITRATE_BPS};
use seele_audio::device::{self, AudioIo};
use seele_audio::drift::DriftTracker;
use seele_audio::gate::{GateConfig, GateMode, VoiceGate};
use seele_audio::jitter::{Decision, JitterBuffer, JitterConfig};
use seele_audio::mixer::Mixer;
use seele_audio::pacing::RingPacer;
use seele_audio::playout::PlayoutClock;
use seele_audio::resample::RateConverter;
use seele_audio::supervisor::{AvisoDeAparelho, CicloDoAparelho, DeviceState, Reabertura};
use seele_audio::telemetry::{AudioTelemetry, FalhaLocal, LocalTelemetry, SourceTelemetry};
use seele_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use seele_proto::ids::{ScreenId, Ssrc};
use seele_proto::MediaHeader;

use crate::client::MediaChannel;

/// Ring size between the device callbacks and the pipeline.
///
/// Absorbs scheduling jitter only. Not the jitter buffer.
const RING_MS: u32 = 100;

/// How often the telemetry snapshot is refreshed.
///
/// The interface redraws far more often than this, and reassembling metrics on
/// every frame would burn the real-time thread on something nobody can read at
/// that rate.
const TELEMETRY_EVERY: Duration = Duration::from_millis(250);

/// How the microphone decides to open. `specs/03-audio.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceMode {
    /// A key is held. Never false-triggers, so it is the default.
    PushToTalk,
    /// The level decides, with hysteresis and hangover.
    VoiceActivated,
    /// Always open. A recording setup, never a default.
    Open,
}

impl VoiceMode {
    fn to_gate(self) -> GateMode {
        match self {
            Self::PushToTalk => GateMode::PushToTalk,
            Self::VoiceActivated => GateMode::VoiceActivated,
            Self::Open => GateMode::Open,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            1 => Self::VoiceActivated,
            2 => Self::Open,
            _ => Self::PushToTalk,
        }
    }

    fn as_byte(self) -> u8 {
        match self {
            Self::PushToTalk => 0,
            Self::VoiceActivated => 1,
            Self::Open => 2,
        }
    }

    /// How this mode is written down in `preferences`.
    ///
    /// Words rather than the byte [`VoiceMode::as_byte`] uses, because that
    /// file is meant to be read and edited by hand — the module says so — and
    /// `voice_mode <TAB> 1` tells nobody anything. The two spellings coexist
    /// for two different readers: a machine word for the audio thread, a word
    /// for the person.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PushToTalk => "push_to_talk",
            Self::VoiceActivated => "voice_activated",
            Self::Open => "open",
        }
    }

    /// The mode a written-down name refers to, or `None` if it names none.
    ///
    /// `from_name` e não `from_str`: o segundo é o nome do método do traço
    /// `FromStr` da biblioteca padrão, e um método inerente com o mesmo nome é
    /// resolvido por regra de escopo — quem importar o traço passa a chamar
    /// outra coisa sem mudar uma linha. O `clippy` recusa, e está certo.
    ///
    /// `None` and not a default: a value this build does not know is a value a
    /// newer build wrote, and quietly turning it into push-to-talk would be
    /// this build overwriting a choice it merely failed to understand.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "push_to_talk" => Some(Self::PushToTalk),
            "voice_activated" => Some(Self::VoiceActivated),
            "open" => Some(Self::Open),
            _ => None,
        }
    }
}

/// What the person has set, read by the audio thread every frame.
///
/// Atomics rather than a lock: this is read from the thread that must not
/// block, and every field is one machine word.
#[derive(Debug)]
struct Controls {
    muted: AtomicBool,
    total_isolation: AtomicBool,
    key_held: AtomicBool,
    mode: AtomicU8,
    bitrate: AtomicU32,
    speaking: AtomicBool,
    stop: AtomicBool,
    /// Onde o relógio de mídia desta pessoa está.
    ///
    /// # Por que ele mora fora do laço
    ///
    /// Porque trocar de microfone abre um caminho de voz **novo** sobre o
    /// **mesmo** `ssrc` — o servidor recusa qualquer outro (G2) —, e um laço
    /// novo começava a contar do zero. Do outro lado, o buffer de jitter daquele
    /// `ssrc` já tocou até um carimbo alto e descarta como atrasado tudo o que
    /// venha antes dele. O efeito é exato e é cruel: a pessoa troca de microfone
    /// e a sala inteira para de ouvi-la, sem erro nenhum em lugar nenhum —
    /// ela está falando, o medidor dela mexe, os quadros saem, e são jogados
    /// fora na chegada.
    ///
    /// Guardado aqui, `Voice::carry_over` o leva para o caminho novo junto com
    /// o mudo e os ganhos, e o relógio segue de onde parou.
    ///
    /// `AtomicU32` para os dois porque não há `AtomicU16` garantido em todo
    /// alvo; o de sequência é lido de volta como `u16`.
    relogio_seq: AtomicU32,
    /// O carimbo de tempo, em amostras. Ver [`Self::relogio_seq`].
    relogio_carimbo: AtomicU32,
    /// Os pacotes de som da tela que se está assistindo, esperando a mistura.
    ///
    /// **Uma fila e não um canal**, porque quem escreve é uma tarefa `async` e
    /// quem lê é a thread de áudio: os dois já dividem este `Arc`, e um canal
    /// seria uma segunda ponte para o mesmo par.
    ///
    /// Com teto e jogando fora o **mais velho**, pela razão do módulo `laco`:
    /// quem está atrasado meio segundo já perdeu a sincronia com a imagem, e
    /// insistir nele afasta o som cada vez mais.
    som_da_tela: Mutex<std::collections::VecDeque<Vec<u8>>>,
    /// De qual transmissão são os pacotes que estão na fila acima.
    ///
    /// # Por que a fila precisa saber de quem ela é
    ///
    /// Porque ela não sabia, e qualquer `TelaFechou` a limpava — inclusive o de
    /// uma transmissão que esta pessoa **não** estava ouvindo. Com duas pessoas
    /// compartilhando, alguém parando de transmitir calava o som da outra. R17 da
    /// revisão da v15.
    ///
    /// E porque a troca de tela precisa ser limpa: os pacotes da anterior que
    /// ainda estão na fila são som de uma imagem que já saiu da frente, e tocá-los
    /// faria a troca soar como sobreposição.
    ///
    /// Zero quer dizer «nenhuma»: `ScreenId` é atribuído pelo servidor e começa
    /// em 1 — ver `voice_room::proxima_tela`.
    tela_do_som: AtomicU32,
    /// O volume do som da transmissão, separado do das pessoas.
    ///
    /// # Por que ele existe
    ///
    /// Porque não havia como silenciar o conteúdo compartilhado preservando a
    /// conversa: o isolamento total cala a mistura inteira, e ganho por
    /// interlocutor não alcança uma tela — uma tela não é um falante. Quem
    /// assistia a um vídeo e queria ouvir as pessoas não tinha o que fazer. R17.
    ///
    /// Guardado em milésimos num átomo, e não em `f32`, pela razão de
    /// [`Self::relogio_seq`]: não há átomo de `f32`, e um cadeado por volta do laço
    /// de áudio seria contenção por nada. `1000` é o volume de origem.
    volume_da_tela: AtomicU32,
    /// Se o som da transmissão está calado, sem mexer no volume dele.
    ///
    /// Separado do volume de propósito, como o mudo do microfone é separado do
    /// ganho: calar e voltar tem de devolver o volume que a pessoa escolheu, e não
    /// zero — um mudo que zerasse o volume seria um mudo que apaga a escolha.
    tela_calada: AtomicBool,
    /// Quanto da supressão de ruído do microfone é aplicado. F02.
    ///
    /// Milésimos num átomo, pela razão de [`Self::relogio_seq`]. Zero desliga, e
    /// desligado o sinal não passa pela FFT — ver `seele_audio::supressao`.
    ///
    /// O padrão é [`SUPRESSAO_PADRAO`]: ligada. É o que faz a sensibilidade de F01
    /// caber, e é o pedido de F02 — quem não quiser desliga, e a escolha fica
    /// gravada.
    supressao: AtomicU32,
    /// A sensibilidade de abertura da ativação por voz, em milésimos de dBFS. F01.
    ///
    /// **Negativa**, então `i32` e não `u32`: dBFS é relativa ao máximo, e tudo
    /// abaixo do máximo é negativo. Milésimos porque a interface oferece meio
    /// decibel de passo.
    ///
    /// Zero quer dizer «o padrão», que depende de a supressão estar ligada — ver
    /// [`configuracao_do_portao`]. Não é «0 dBFS», que seria um portão que nunca
    /// abre: zero é o valor de um átomo que ninguém escreveu, e tratá-lo como
    /// escolha faria toda sessão nova começar surda.
    abertura_dbfs_milesimos: std::sync::atomic::AtomicI32,
    /// Quadros que a Voice produziu e o transporte recusou.
    ///
    /// Terceira categoria, e ela faltava. `Telemetry` já distingue perda de
    /// rede (`loss_fraction`, que vem do servidor) de falha da máquina
    /// (`local_fault`, captura ou reprodução engasgando) porque «as duas soam
    /// idênticas ao ouvinte e têm consertos opostos». Isto é a que restava: o
    /// quadro foi codificado, estava pronto, e **nunca saiu desta máquina**.
    ///
    /// Era descartado com `let _ =`. Soava exatamente como perda de rede e não
    /// deixava rastro nenhum — nem log, nem contador —, então a pergunta «é a
    /// rede ou é daqui?» não tinha como ser respondida por ninguém.
    ///
    /// # O que este contador **não** é
    ///
    /// Ele nasceu apostando que a causa do picote era o datagrama não caber no
    /// caminho — o texto viaja em fluxo e se adapta sozinho, a voz viaja em
    /// datagrama e um que não cabe é recusado inteiro. A aposta estava errada, e
    /// `seele_audio::codec` a mata com número em vez de argumento: um quadro no
    /// teto do bitrate são 272 bytes, e a RFC 9000 §14.1 exige que todo caminho
    /// entregue 1200 — um que não entregasse nunca teria deixado o QUIC apertar
    /// a mão, e não haveria texto atravessando para comparar.
    ///
    /// O contador fica porque a pergunta que ele responde continua valendo e não
    /// tinha resposta: *este áudio sumiu antes de sair daqui?* Só a hipótese que
    /// motivou o contador morreu, e não o contador.
    recusados: std::sync::atomic::AtomicU64,
    /// Amostras que a mistura produziu e o anel do dispositivo não aceitou.
    ///
    /// A gêmea de [`Controls::recusados`], do outro lado do laço: aquela conta
    /// o que não saiu da máquina, esta conta o que não chegou ao alto-falante.
    /// Também estava sendo descartada com `let _ =`, e também soa exatamente
    /// como perda de rede para quem está ouvindo.
    ///
    /// Conta **amostras**, não quadros: o anel é de amostras e é aí que a
    /// recusa acontece. Dividir por 48 dá milissegundos de áudio perdidos.
    anel_cheio: std::sync::atomic::AtomicU64,
    /// Per-talker volume. Read once per 20 ms frame, so a lock is affordable
    /// here in a way it would not be inside a device callback.
    gains: Mutex<HashMap<u32, f32>>,
}

/// One microphone the machine is offering.
///
/// Its own type rather than a re-export of `seele_audio::device::CaptureDevice`
/// for the same reason [`DeviceRates`] is its own type: ADR 0002 keeps
/// `seele-ffi` from naming `seele-audio` at all, and a
/// re-export would make them name it through this crate's front door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDevice {
    /// The stable handle a preference is written down as. Never shown to a
    /// person — see `seele_audio::device::CaptureDevice` for why it is not the
    /// name.
    pub id: String,
    /// What the machine calls it, for a person to read.
    pub name: String,
    /// Whether this is the one a session with no preference would take.
    pub default: bool,
}

/// Every microphone the machine will describe, right now.
///
/// Answerable with no session: picking a microphone is a thing a person does
/// before connecting as often as during, and making the list depend on a live
/// [`Voice`] would put the control behind the door it is meant to open.
///
/// An empty list means the host would not enumerate, **not** that there is no
/// microphone — the default device still opens when enumeration fails. An
/// interface must not read this as "no audio"; [`crate::Voice::start`] failing
/// is what means that.
#[must_use]
pub fn capture_devices() -> Vec<CaptureDevice> {
    device::capture_devices()
        .into_iter()
        .map(into_core)
        .collect()
}

/// One place the machine will play sound.
///
/// The twin of [`CaptureDevice`], and its own type for the same reason that one
/// is: an input id and an output id are both strings, and only the type stops
/// one being handed to the wrong half of [`Voice::start_on`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackDevice {
    /// The stable handle a preference is written down as. Never shown.
    pub id: String,
    /// What the machine calls it, for a person to read.
    pub name: String,
    /// Whether this is the one a session with no preference would take.
    pub default: bool,
}

/// Every output the machine will describe, right now.
///
/// The twin of [`capture_devices`], answerable with no session for the same
/// reason, and with the same reading of an empty list: the machine would not
/// enumerate, **not** that there is nowhere to play.
#[must_use]
pub fn playback_devices() -> Vec<PlaybackDevice> {
    device::playback_devices()
        .into_iter()
        .map(playback_into_core)
        .collect()
}

/// The audio layer's device, as this crate's own.
fn into_core(found: device::CaptureDevice) -> CaptureDevice {
    CaptureDevice {
        id: found.id,
        name: found.name,
        default: found.default,
    }
}

/// The same, for the other side of the pair.
fn playback_into_core(found: device::PlaybackDevice) -> PlaybackDevice {
    PlaybackDevice {
        id: found.id,
        name: found.name,
        default: found.default,
    }
}

/// Em que pé está o aparelho de áudio desta sessão.
///
/// Tipo próprio e não um reexport de `seele_audio::supervisor::DeviceState`
/// pela mesma razão que [`CaptureDevice`] é próprio: o ADR 0002 impede a casca
/// de nomear `seele-audio`.
///
/// # Por que a interface precisa disto
///
/// Porque até aqui ela não era avisada. Quando o aparelho sumia, o único sinal
/// era o contador de erros crescendo, que a telemetria transformava num
/// `local_fault` — um aviso que diz «a máquina, não a rede» e **apaga sozinho**
/// quando o contador para de crescer. Quem tirou o fone lia «falha local» por
/// alguns segundos e depois tinha silêncio limpo, sem nada na tela. Isto é a
/// frase que faltava: o aparelho mudou, está mudando, ou foi-se.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

impl From<DeviceState> for EstadoDoAparelho {
    fn from(estado: DeviceState) -> Self {
        match estado {
            DeviceState::Running => Self::Funcionando,
            DeviceState::Recovering { .. } => Self::Trocando,
            DeviceState::Lost => Self::Perdido,
        }
    }
}

/// Os aparelhos que esta sessão está usando **agora**.
///
/// Lido pela casca a cada quadro de telemetria. Escrito pelo laço de áudio,
/// que é o único lugar onde um aparelho pode ser aberto — ver o cabeçalho de
/// [`Voice::start_on`] sobre `cpal` e `Send`.
///
/// [`Self::reaberturas`] é o que permite à interface dizer «o aparelho mudou»
/// em vez de só desenhar um nome diferente sem explicação: ele cresce uma vez
/// por reabertura conseguida.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EstadoDoAudio {
    /// O microfone que está aberto de verdade.
    pub capture: Option<CaptureDevice>,
    /// Onde o som está saindo de verdade.
    pub playback: Option<PlaybackDevice>,
    /// Em que pé está o aparelho.
    pub estado: EstadoDoAparelho,
    /// Quantas vezes o laço reabriu num aparelho, desde que esta voz começou.
    pub reaberturas: u64,
    /// A que taxas o aparelho **de agora** roda.
    ///
    /// Aqui e não num campo parado dentro de [`Voice`] porque elas mudam junto
    /// com o aparelho: um fone de 44,1 kHz que entra no lugar de uma placa de
    /// 48 kHz muda as duas, e quem guardasse as da primeira abertura passaria a
    /// responder um número que já não é de ninguém.
    pub taxas: DeviceRates,
}

impl EstadoDoAudio {
    /// Anota o que uma reabertura entregou.
    ///
    /// Uma função e não três atribuições espalhadas porque é ela que o teste de
    /// conformidade da troca exercita: o que o laço escreve no painel e o que o
    /// teste confere têm de ser o mesmo código, ou o teste prova o teste.
    pub fn reaberto(&mut self, aberto: &impl AparelhosAbertos, ciclo: &CicloDoAparelho) {
        self.capture = aberto.microfone();
        self.playback = aberto.saida();
        self.taxas = aberto.taxas();
        self.estado = ciclo.estado().into();
        self.reaberturas = ciclo.reaberturas();
    }

    /// Anota o andamento quando ainda não há aparelho novo.
    ///
    /// Separada de [`Self::reaberto`] porque aqui os nomes **não** mudam: o que
    /// muda é o estado. Escrever o aparelho velho como se fosse novo seria a
    /// interface afirmando que há som saindo dele.
    pub fn andamento(&mut self, ciclo: &CicloDoAparelho) {
        self.estado = ciclo.estado().into();
    }

    /// O aparelho abriu e mesmo assim não há laço possível com ele.
    ///
    /// Acontece quando o reamostrador recusa as taxas do que abriu. É raro, e
    /// era o único caminho em que o laço encerrava **depois** de o painel já
    /// ter sido escrito como «funcionando» com o nome do aparelho novo: a
    /// pessoa ficava sem som nenhum lendo normalidade na tela. Aqui o nome fica
    /// (foi ele mesmo que abriu), e o estado diz a verdade.
    pub fn sem_laco_possivel(&mut self) {
        self.estado = EstadoDoAparelho::Perdido;
    }
}

/// O que um aparelho recém-aberto sabe dizer sobre si.
///
/// Traço e não campo porque é o que permite ao teste de conformidade abrir um
/// aparelho de mentira e passar pela **mesma** orquestração que roda em
/// produção. `cpal` não abre nada numa máquina sem placa de som, e essa é a
/// única parte que um teste não alcança; tudo o que decide *quando* reabrir,
/// *o que* anotar e *o que a interface vê* fica deste lado do traço.
pub trait AparelhosAbertos {
    /// O microfone que abriu, quando o sistema o descreve.
    fn microfone(&self) -> Option<CaptureDevice>;
    /// A saída que abriu, quando o sistema a descreve.
    fn saida(&self) -> Option<PlaybackDevice>;
    /// A que taxas ele abriu, que é o que dimensiona o laço inteiro.
    fn taxas(&self) -> DeviceRates;

    /// Quantos quadros cabem no anel de saída que ele abriu.
    ///
    /// Aqui e não como número solto porque é dele que sai a reserva que o
    /// compasso segura: um aparelho com anel menor pede outra reserva, e
    /// reaproveitar a do anterior é o anel encostando no fundo para sempre.
    fn capacidade_de_saida(&self) -> usize;
}

impl AparelhosAbertos for AudioIo {
    fn microfone(&self) -> Option<CaptureDevice> {
        self.capture.clone().map(into_core)
    }

    fn saida(&self) -> Option<PlaybackDevice> {
        self.playback.clone().map(playback_into_core)
    }

    fn taxas(&self) -> DeviceRates {
        DeviceRates {
            capture_hz: self.capture_rate_hz,
            playback_hz: self.playback_rate_hz,
        }
    }

    fn capacidade_de_saida(&self) -> usize {
        self.to_device.buffer().capacity()
    }
}

/// O ciclo do aparelho mais o painel que a interface lê.
///
/// # Por que isto não mora dentro do laço
///
/// Porque o laço não é testável: ele tem `cpal` de um lado e um socket QUIC do
/// outro. Esta peça é a decisão inteira — ler o aviso, mandar reabrir, anotar o
/// que abriu, contar a troca para a interface — e ela roda igual com um
/// aparelho de verdade e com um de mentira. É o que faz a troca de aparelho ter
/// finalmente um teste de **comportamento**, e não um teste que lê o próprio
/// código-fonte à procura de uma palavra.
#[derive(Debug)]
pub struct Acompanhamento {
    ciclo: CicloDoAparelho,
    /// O último estado que o painel já viu, para não travar o cadeado à toa.
    anotado: DeviceState,
}

impl Default for Acompanhamento {
    fn default() -> Self {
        Self::novo()
    }
}

/// O painel, mesmo que o cadeado esteja envenenado.
///
/// Um pânico noutra thread envenena o cadeado para sempre. Com `if let Ok`, o
/// preço disso seria a interface congelar no último estado que ela viu — sem
/// dizer nada — enquanto o aparelho troca ou some. É exatamente «o produto sabe
/// e não conta», e aqui não há dado a proteger: o painel é um punhado de campos
/// que cada escrita substitui inteiros, não uma estrutura pela metade.
fn painel_mesmo_envenenado(painel: &Mutex<EstadoDoAudio>) -> MutexGuard<'_, EstadoDoAudio> {
    painel.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Acompanhamento {
    /// Um acompanhamento para uma sessão cujo aparelho está funcionando.
    #[must_use]
    pub fn novo() -> Self {
        Self {
            ciclo: CicloDoAparelho::novo(),
            anotado: DeviceState::Running,
        }
    }

    /// Em que pé está o aparelho.
    #[must_use]
    pub fn estado(&self) -> EstadoDoAparelho {
        self.ciclo.estado().into()
    }

    /// Uma volta do laço: decide, reabre quando for a hora, e conta à interface.
    ///
    /// Devolve o aparelho novo **só** quando um abriu; o chamador troca o que
    /// tem pelo que voltar aqui. `None` é o caso comum e não é falha.
    pub fn passo<R>(
        &mut self,
        aviso: AvisoDeAparelho,
        agora_ms: f64,
        abridor: &mut R,
        painel: &Mutex<EstadoDoAudio>,
    ) -> Option<R::Aberto>
    where
        R: Reabertura,
        R::Aberto: AparelhosAbertos,
    {
        if let Some(aberto) = self.ciclo.passo(aviso, agora_ms, abridor) {
            painel_mesmo_envenenado(painel).reaberto(&aberto, &self.ciclo);
            self.anotado = self.ciclo.estado();
            return Some(aberto);
        }

        if self.ciclo.estado() != self.anotado {
            // A interface é avisada **enquanto** a troca acontece, e não só
            // quando ela termina: `specs/03-audio.md` pede «tell the
            // interface», e o que ela via até aqui era um aviso de falha local
            // que apagava sozinho quando o contador parava de crescer.
            self.anotado = self.ciclo.estado();
            painel_mesmo_envenenado(painel).andamento(&self.ciclo);
        }
        None
    }
}

/// Which devices to open, as ids.
///
/// One value rather than two arguments, because the two are the same type: a
/// call that swapped them would compile and then ask the speakers to record.
/// `Default` is the machine's own choice on both sides, which is what every
/// session took before there was a screen to choose on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceChoice {
    /// Which microphone, as a [`CaptureDevice::id`].
    pub capture: Option<String>,
    /// Where the sound comes out, as a [`PlaybackDevice::id`].
    pub playback: Option<String>,
}

impl DeviceChoice {
    /// The same choice, as the audio layer asks for it.
    pub(crate) fn wanted(&self) -> device::Wanted<'_> {
        device::Wanted {
            capture: self.capture.as_deref(),
            playback: self.playback.as_deref(),
        }
    }

    /// The same choice with one side given up, or `None` when there is nothing
    /// left to give up on that side.
    ///
    /// `None` is what ends the ladder in [`open_preferring`]: a side already on
    /// the machine's default that still will not open is not a stale
    /// preference, it is no audio, and retrying the same thing forever is the
    /// one behaviour worse than saying so.
    fn without(&self, side: device::Side) -> Option<Self> {
        let mut fallen_back = self.clone();
        let given_up = match side {
            device::Side::Input => fallen_back.capture.take(),
            device::Side::Output => fallen_back.playback.take(),
        };
        given_up.map(|_| fallen_back)
    }
}

/// What the devices turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeviceRates {
    /// Native capture rate. Not necessarily 48 kHz — gap G7.
    pub capture_hz: u32,
    /// Native playback rate.
    pub playback_hz: u32,
}

/// Opens `chosen`, giving up the preference each failure blames, one at a time.
///
/// The rule this encodes, and the reason it is one function rather than a
/// link_state each shell repeats: **a preference written down yesterday never
/// stops somebody entering today**. A device moves, gets unplugged, or belongs
/// to a machine the settings file was copied from, and none of those may cost a
/// person the session.
///
/// It gives up the blamed side only. Giving up both would mean a headset left in
/// another room silently discarding a microphone choice that was working, and
/// the person would have two things to fix instead of none.
///
/// Terminates because each round either returns or empties one of the two
/// preferences, and a side with nothing left to give up returns the failure.
fn open_preferring(chosen: &DeviceChoice) -> Result<AudioIo, device::DeviceError> {
    let mut asking = chosen.clone();
    loop {
        let failure = match device::open(asking.wanted(), RING_MS) {
            Ok(io) => return Ok(io),
            Err(failure) => failure,
        };
        let Some(less) = asking.without(failure.side()) else {
            // Already on the machine's own device for the side that failed.
            // Nothing here is stale, and there is nothing left to try.
            return Err(failure);
        };
        tracing::warn!(
            %failure,
            "the chosen device is not available; falling back to the machine's own"
        );
        asking = less;
    }
}

/// A running voice path.
///
/// Dropping it stops the audio.
/// O `ssrc` que o som da tela usa na mistura.
///
/// Reservado, e alto de propósito: o `Ssrc` de uma pessoa é dado pelo servidor,
/// que nunca entrega este. Um número fixo é o que permite dar ao som da tela um
/// ganho próprio no dia em que alguém quiser um, sem inventar um segundo
/// caminho de mistura.
pub const SSRC_DA_TELA: u32 = u32::MAX;

/// O volume de origem do som da transmissão, em milésimos.
///
/// Mil e não zero: quem começa a assistir ouve o que está sendo compartilhado. Um
/// padrão calado faria o produto entregar vídeo mudo e esperar que a pessoa
/// descobrisse sozinha onde o som estava.
const VOLUME_DE_ORIGEM: u32 = 1_000;

/// Quanto da supressão de ruído é aplicado por padrão, em milésimos.
///
/// Mil: ligada, inteira. F02 pede o controle com opção de desligar, e não pede o
/// padrão desligado — um filtro que só funciona depois de alguém achar o
/// interruptor é um filtro que quase ninguém tem.
///
/// E ligada é o que faz a sensibilidade de F01 caber: o padrão de −60 dBFS só é
/// defensável com o piso da sala removido. Ver
/// `seele_audio::gate::ABERTURA_COM_SUPRESSAO_DBFS`.
const SUPRESSAO_PADRAO: u32 = 1_000;

/// A configuração do portão que os controles de agora descrevem. F01.
///
/// Zero em `abertura_dbfs_milesimos` quer dizer «o padrão», e o padrão depende de
/// a supressão estar ligada. É a decisão central de F01: −60 dBFS só é defensável
/// com o piso da sala removido, e sem supressão o portão volta a ser a única
/// defesa contra o ventilador — o argumento que `gate::OPEN_RMS` registra.
///
/// # O padrão é um alvo, e a escolha é um número
///
/// O padrão vai por [`GateConfig::automatico`]: −60 dBFS passa a ser o **mais
/// sensível que o portão chega**, e o limiar de verdade é o ruído medido mais
/// `gate::MARGEM_SOBRE_O_RUIDO_DB`. A auditoria de 22/09/2026 (A03) mediu por que:
/// como número fixo, −60 dBFS abriu em 200 de 200 quadros de ruído sem fala
/// nenhuma, porque o residual **depois** da supressão fica em torno de −59 dBFS —
/// perto de zero decibel de folga.
///
/// Um valor escolhido à mão vai por [`GateConfig::de_dbfs`], que é fixo: F01 pede
/// o ajuste manual, e um ajuste que o produto corrige por cima não é ajuste.
fn configuracao_do_portao(controls: &Controls) -> GateConfig {
    let escolhido = controls.abertura_dbfs_milesimos.load(Ordering::Relaxed);
    let dbfs = if escolhido == 0 {
        if controls.supressao.load(Ordering::Relaxed) > 0 {
            seele_audio::gate::ABERTURA_COM_SUPRESSAO_DBFS
        } else {
            seele_audio::gate::ABERTURA_SEM_SUPRESSAO_DBFS
        }
    } else {
        #[allow(
            clippy::cast_precision_loss,
            reason = "milésimos de decibel cabem exatos num f32 nesta faixa"
        )]
        let convertido = escolhido as f32 / 1000.0;
        // **Fixo**, porque foi escolhido. Ver o doc.
        return GateConfig::de_dbfs(convertido);
    };
    GateConfig::automatico(dbfs)
}

/// O carimbo de tempo de um quadro numa entrega de vários. F01.
///
/// `carimbo` é o do quadro de **agora**, `quantos` é o tamanho da entrega e
/// `indice` a posição dentro dela. O último quadro fica com `carimbo`; cada um
/// antes dele recua um quadro de amostras.
///
/// # Por que não bastava carimbar todos igual
///
/// Porque um carimbo é *quando aquele áudio aconteceu*, e os quadros retidos da
/// primeira sílaba aconteceram **antes**. Carimbados todos com o mesmo instante,
/// quem recebe vê três quadros no mesmo tempo: o `playout` trata carimbo repetido
/// como um salto do relógio e ressincroniza, e uma ressincronização no primeiro
/// quadro de cada fala é exatamente onde ela mais se ouve — é o começo da palavra
/// que F01 foi recuperar.
///
/// Auditoria de 22/09/2026, A05. Recuar é a conta certa e não uma aproximação: o
/// quadro retido *é* de 20 ms antes.
///
/// `wrapping_sub` porque o carimbo é um contador de 32 bits que dá a volta, e uma
/// volta no meio de uma abertura não é um erro — é a mesma aritmética que o resto
/// deste laço usa para andar para a frente.
fn carimbo_do_quadro(carimbo: u32, quantos: usize, indice: usize) -> u32 {
    let atras = quantos.saturating_sub(indice + 1);
    let amostras = u32::try_from(atras.saturating_mul(FRAME_SAMPLES)).unwrap_or(0);
    carimbo.wrapping_sub(amostras)
}

/// Põe um pacote de som de tela na fila, trocando de tela se for outra. R17.
///
/// **Função livre sobre os controles**, e não um corpo de método, para ser
/// exercitável sem placa de som: abrir uma `Voice` pede um dispositivo de áudio, e
/// o que este código decide — de quem é a fila — não tem nada a ver com hardware.
/// Ver `o_som_de_uma_tela_nao_e_limpo_por_outra`.
fn enfileirar_som_da_tela(controls: &Controls, tela: ScreenId, pacote: Vec<u8>) {
    /// Meio segundo a 20 ms por pacote.
    const TETO: usize = 25;

    let mut fila = controls
        .som_da_tela
        .lock()
        .unwrap_or_else(|envenenado| envenenado.into_inner());
    // **Trocou de transmissão: a fila da anterior vai fora.** Os pacotes que
    // sobraram são som de uma imagem que já não está na frente de ninguém, e
    // tocá-los faria a troca soar como sobreposição.
    let anterior = controls.tela_do_som.swap(tela.0, Ordering::Relaxed);
    if anterior != tela.0 {
        fila.clear();
    }
    while fila.len() >= TETO {
        fila.pop_front();
    }
    fila.push_back(pacote);
}

/// Esquece o som **daquela** transmissão, se é dela que a fila é. R17.
///
/// Devolve se havia o que esquecer. Função livre pela razão de
/// [`enfileirar_som_da_tela`].
fn esquecer_o_som_de(controls: &Controls, tela: ScreenId) -> bool {
    if controls.tela_do_som.load(Ordering::Relaxed) != tela.0 {
        return false;
    }
    controls.tela_do_som.store(0, Ordering::Relaxed);
    controls
        .som_da_tela
        .lock()
        .unwrap_or_else(|envenenado| envenenado.into_inner())
        .clear();
    true
}

/// Quanto da supressão os controles de agora pedem, de zero a um. F02.
fn forca_da_supressao(controls: &Controls) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "mil milésimos cabem exatos num f32"
    )]
    let forca = controls.supressao.load(Ordering::Relaxed) as f32 / 1000.0;
    forca
}

/// A running voice path.
///
/// Dropping it stops the audio.
#[derive(Debug)]
pub struct Voice {
    controls: Arc<Controls>,
    telemetry: Arc<Mutex<AudioTelemetry>>,
    /// Os aparelhos abertos de verdade, e em que pé eles estão.
    ///
    /// Read off the device rather than remembered from the request, so that a
    /// path started with no preference can still say which microphone it got.
    ///
    /// **Compartilhado com a thread do laço**, e era um par de campos fixos.
    /// Fixos, eles só podiam dizer a verdade até a primeira vez que o sistema
    /// trocasse o aparelho por baixo da sessão — e como o laço é o único lugar
    /// onde `cpal` pode abrir um fluxo, era ele quem sabia e não tinha onde
    /// escrever. Ver [`EstadoDoAudio`].
    aparelhos: Arc<Mutex<EstadoDoAudio>>,
    /// What was *asked* for, which is not what opened.
    ///
    /// Kept so that changing one side does not quietly reset the other:
    /// [`Voice::switch_playback`] has to reopen the microphone that is running,
    /// and the only faithful way to do that is to ask for it again exactly as
    /// this path did. The ask and not the result, because a preference that fell
    /// back to the default is still the preference — plugging the interface back
    /// in has to restore it, not require it to be made again.
    chosen: DeviceChoice,
    /// Se a máquina está derrubando áudio **agora**.
    ///
    /// Mora aqui porque é uma derivada: precisa lembrar o que viu da última
    /// vez. Ver [`FalhaLocal`] — e por que isto não é mais uma comparação com
    /// zero.
    falha_local: Mutex<FalhaLocal>,
    /// Escolhe a faixa de bitrate a partir da perda de subida que o servidor
    /// relata. Ver o ADR 0036.
    ///
    /// `Mutex` e não atômico porque o controlador tem estado — o índice da faixa
    /// e desde quando a rede está boa — e porque ele é tocado uma vez por
    /// segundo, no fio que dobra quadros de controle. O que o laço de áudio lê
    /// continua sendo `Controls::bitrate`, que é atômico e não espera por
    /// ninguém: nada deste cadeado alcança a thread que não pode bloquear.
    faixa: Mutex<seele_audio::bitrate::Controlador>,
}

impl Controls {
    /// Controles zerados, como uma voz que acabou de abrir.
    ///
    /// Uma função e não um literal no meio de `around` porque
    /// `carregar_controles` precisa de dois deles para ser exercitada sem placa
    /// de som — ver `os_controles_atravessam_a_reabertura`.
    fn novos() -> Self {
        Self {
            muted: AtomicBool::new(false),
            total_isolation: AtomicBool::new(false),
            key_held: AtomicBool::new(false),
            // specs/03-audio.md makes push-to-talk the default because it never
            // false-triggers, and a client that transmits a room by accident is
            // worse than one that misses a word.
            mode: AtomicU8::new(VoiceMode::PushToTalk.as_byte()),
            bitrate: AtomicU32::new(DEFAULT_BITRATE_BPS),
            speaking: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            relogio_seq: AtomicU32::new(0),
            relogio_carimbo: AtomicU32::new(0),
            recusados: std::sync::atomic::AtomicU64::new(0),
            anel_cheio: std::sync::atomic::AtomicU64::new(0),
            gains: Mutex::new(HashMap::new()),
            som_da_tela: Mutex::new(std::collections::VecDeque::new()),
            tela_do_som: AtomicU32::new(0),
            volume_da_tela: AtomicU32::new(VOLUME_DE_ORIGEM),
            tela_calada: AtomicBool::new(false),
            supressao: AtomicU32::new(SUPRESSAO_PADRAO),
            abertura_dbfs_milesimos: std::sync::atomic::AtomicI32::new(0),
        }
    }
}

/// Leva para `para` tudo o que uma reabertura não pode perder.
///
/// **A lista mora aqui, uma vez.** Trocar de microfone, trocar de saída e
/// reabrir numa reconexão passam todos por ela, e um item acrescentado vale
/// para os três no mesmo dia. O item que machuca é o mudo: uma reabertura que
/// desliga o mudo sozinha põe uma sala no ar.
///
/// Função sobre dois [`Controls`] e não um método de [`Voice`] para que ela
/// tenha teste: dois `Voice` exigem duas placas de som, dois `Controls` não
/// exigem nada. O guarda que existia para isto lia o texto-fonte da casca
/// atrás da palavra `reopen` — o que prova que alguém a escreveu, e nada sobre
/// o que sobrevive de verdade.
fn carregar_controles(de: &Controls, para: &Controls) {
    para.muted
        .store(de.muted.load(Ordering::Relaxed), Ordering::Relaxed);
    para.total_isolation.store(
        de.total_isolation.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    para.mode
        .store(de.mode.load(Ordering::Relaxed), Ordering::Relaxed);
    para.key_held
        .store(de.key_held.load(Ordering::Relaxed), Ordering::Relaxed);
    // O relógio de mídia junto, e é o item cuja falta calava a pessoa. Ver
    // `Controls::relogio_seq` para o porquê e `salto_do_relogio` para o
    // tamanho do pulo.
    let (seq, carimbo) = Voice::salto_do_relogio(
        de.relogio_seq.load(Ordering::Relaxed) as u16,
        de.relogio_carimbo.load(Ordering::Relaxed),
    );
    para.relogio_seq.store(u32::from(seq), Ordering::Relaxed);
    para.relogio_carimbo.store(carimbo, Ordering::Relaxed);
    if let (Ok(antigos), Ok(mut novos)) = (de.gains.lock(), para.gains.lock()) {
        for (talker, gain) in antigos.iter() {
            novos.insert(*talker, *gain);
        }
    }
}

impl Voice {
    /// Opens the devices and starts the pipeline.
    ///
    /// # Errors
    ///
    /// Fails if no input or output device can be opened, or if the encoder
    /// refuses the configuration.
    pub fn start(media: MediaChannel, ssrc: Ssrc) -> Result<Self> {
        Self::start_on(&DeviceChoice::default(), media, ssrc)
    }

    /// Opens the chosen devices and starts the pipeline.
    ///
    /// Each half of `chosen` is an id from [`capture_devices`] or
    /// [`playback_devices`], never a name. Both `None` is the machine's own
    /// choice, which is what [`Voice::start`] takes.
    ///
    /// **Strict**: a device that is not there is an error, not a fallback. That
    /// is what a person clicking a row needs — the pick either took or it did
    /// not, and a screen that reports "done" after quietly opening something
    /// else is a screen that lies about the one thing it exists to do. A
    /// preference read off disk wants the opposite, and that is
    /// [`Voice::start_preferring`].
    ///
    /// Switching device is this function plus dropping the old handle: the
    /// pipeline owns its `AudioIo` for the life of its thread, and `cpal`'s
    /// streams are not `Send` on every backend, so a running path cannot have a
    /// device swapped underneath it. Stopping and restarting is not a shortcut
    /// around that — it is the only shape the audio layer allows.
    ///
    /// # Errors
    ///
    /// Fails if either named device is gone, if no device can be opened at all,
    /// or if the encoder refuses the configuration.
    pub fn start_on(chosen: &DeviceChoice, media: MediaChannel, ssrc: Ssrc) -> Result<Self> {
        // Opened here rather than on the audio thread so that "there is no
        // microphone" is a return value the interface can show, instead of a
        // thread that quietly dies.
        let io = device::open(chosen.wanted(), RING_MS)?;
        Self::around(io, chosen.clone(), media, ssrc)
    }

    /// Opens the chosen devices, giving up one preference at a time.
    ///
    /// For a choice read off disk, where [`Voice::start_on`] is for a row
    /// somebody just clicked. A preference written down last week names a device
    /// that may be in another room by now, and turning that into a session
    /// nobody can join would make the picker the most dangerous control in the
    /// product. So each side that will not open falls back to the machine's own,
    /// and [`Voice::capture`] and [`Voice::playback`] report what actually
    /// opened — the fallback is visible, and it is visible as a name rather than
    /// as the word "default".
    ///
    /// One side at a time, and the side the failure blames: giving up both
    /// because the speakers moved would throw away a microphone choice that was
    /// fine. The ask is remembered whole either way, so the next reopening tries
    /// the real preference again.
    ///
    /// # Errors
    ///
    /// Fails when a side is already on the machine's own device and still will
    /// not open. That is not a stale preference; it is no audio, and this is
    /// where saying so belongs.
    pub fn start_preferring(
        chosen: &DeviceChoice,
        media: MediaChannel,
        ssrc: Ssrc,
    ) -> Result<Self> {
        let io = open_preferring(chosen)?;
        Self::around(io, chosen.clone(), media, ssrc)
    }

    /// Wraps devices that are already open in a running pipeline.
    fn around(io: AudioIo, chosen: DeviceChoice, media: MediaChannel, ssrc: Ssrc) -> Result<Self> {
        let aparelhos = Arc::new(Mutex::new(EstadoDoAudio {
            capture: io.microfone(),
            playback: io.saida(),
            estado: EstadoDoAparelho::Funcionando,
            reaberturas: 0,
            taxas: io.taxas(),
        }));

        let controls = Arc::new(Controls::novos());
        let telemetry = Arc::new(Mutex::new(AudioTelemetry::default()));

        let thread_controls = Arc::clone(&controls);
        let thread_telemetry = Arc::clone(&telemetry);
        let thread_aparelhos = Arc::clone(&aparelhos);
        // A escolha vai junto para a thread: reabrir num aparelho novo é
        // repetir **o pedido**, não o resultado. Quem pediu o padrão do sistema
        // quer o padrão de agora; quem pediu um aparelho por id quer aquele de
        // volta assim que ele aparecer.
        let thread_escolha = chosen.clone();
        std::thread::Builder::new()
            .name("seele-voice".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => return,
                };
                runtime.block_on(pipeline(
                    io,
                    media,
                    ssrc,
                    thread_controls,
                    thread_telemetry,
                    thread_aparelhos,
                    thread_escolha,
                ));
            })?;

        Ok(Self {
            controls,
            telemetry,
            aparelhos,
            chosen,
            falha_local: Mutex::new(FalhaLocal::new()),
            faixa: Mutex::new(seele_audio::bitrate::Controlador::novo()),
        })
    }

    /// What the devices actually run at.
    ///
    /// Lido do painel e não de um campo guardado na abertura, porque o aparelho
    /// pode ter sido trocado por baixo da sessão desde então — ver
    /// [`EstadoDoAudio::taxas`].
    #[must_use]
    pub fn rates(&self) -> DeviceRates {
        self.estado_do_audio().taxas
    }

    /// Dobra a perda de subida que o servidor relatou, e move a faixa se ela
    /// mudou.
    ///
    /// Devolve o bitrate novo quando houve troca, para quem quiser registrá-la.
    /// `None` é o caso comum e **não** é falha: a maior parte das medidas não
    /// muda nada, e é exatamente isso que mantém rara a reconstrução do encoder
    /// que uma troca custa. Ver `seele_audio::bitrate` e o ADR 0036.
    ///
    /// O relógio é parâmetro pela mesma razão que no controlador: a permanência
    /// é a metade da malha que mais erra, e ela tem de ser testável sem esperar
    /// dez segundos de verdade.
    pub fn observar_perda(&self, perda: f32, agora: Instant) -> Option<u32> {
        let Ok(mut faixa) = self.faixa.lock() else {
            return None;
        };
        let novo = faixa.observar(perda, agora)?;
        // O laço de voz lê isto a cada volta e chama `VoiceEncoder::set_bitrate`,
        // que só reconstrói quando o valor mudou de verdade.
        self.controls.bitrate.store(novo, Ordering::Relaxed);
        Some(novo)
    }

    /// A faixa de bitrate em vigor, em bits por segundo.
    ///
    /// Lida do `Controls`, e não do controlador: é o valor que o laço de áudio
    /// vai usar, que é o que a pergunta quer saber.
    #[must_use]
    pub fn bitrate_bps(&self) -> u32 {
        self.controls.bitrate.load(Ordering::Relaxed)
    }

    /// The microphone this path is actually capturing from.
    ///
    /// `None` when the backend would not describe the device it opened. Audio is
    /// still running in that case — an interface must draw an unnamed device,
    /// not a missing one.
    ///
    /// Devolve cópia e não empréstimo porque o valor **muda enquanto a sessão
    /// vive**: o laço o reescreve quando o sistema troca o aparelho. Um
    /// empréstimo para dentro do `Voice` prometeria um valor parado.
    #[must_use]
    pub fn capture(&self) -> Option<CaptureDevice> {
        self.estado_do_audio().capture
    }

    /// Os aparelhos em uso e em que pé eles estão, de uma vez.
    ///
    /// É o que a casca desenha. Uma leitura só, porque as três respostas têm de
    /// ser do mesmo instante: nome novo com estado velho é a interface dizendo
    /// que já trocou quando ainda está trocando.
    #[must_use]
    pub fn estado_do_audio(&self) -> EstadoDoAudio {
        painel_mesmo_envenenado(&self.aparelhos).clone()
    }

    /// Where this path is actually playing.
    ///
    /// `None` under the same two conditions as [`Voice::capture`], and read for
    /// the same reason: what opened, never what was asked for. It is the only
    /// evidence a person gets that a chosen output was not there — nothing about
    /// falling back to the machine's speakers announces itself.
    ///
    /// Cópia e não empréstimo, pela razão escrita em [`Voice::capture`].
    #[must_use]
    pub fn playback(&self) -> Option<PlaybackDevice> {
        self.estado_do_audio().playback
    }

    /// Which devices this path asked for, which is not what it got.
    ///
    /// The ask survives a fallback on purpose — see [`Voice::chosen`] — so this
    /// is what to hand back to [`Voice::reopen`], and never what to draw.
    #[must_use]
    pub fn chosen(&self) -> &DeviceChoice {
        &self.chosen
    }

    /// Starts a second path on another microphone, carrying this one's settings.
    ///
    /// The output stays where it is, asked for exactly as this path asked for
    /// it. Reopening on the machine's default instead would make changing
    /// microphone silently undo a choice made on the other half of the same
    /// screen.
    ///
    /// The returned [`Voice`] is the live one; dropping `self` is what stops the
    /// old device. Deliberately in that order: the new path is opened **before**
    /// the old one is given up, so a microphone that turns out to be gone leaves
    /// the session where it was instead of silent. The cost is a moment with two
    /// playback streams open, which every backend this ships on allows.
    ///
    /// # What has to survive, and why it is decided here
    ///
    /// mudo, Isolamento total, the mode, the held key and every per-talker
    /// volume. Left to a shell, this list would be written once per shell and
    /// each copy would forget a different item — and the item that hurts is
    /// mudo: a switch that quietly unmutes a microphone is a switch that
    /// puts a room on the air.
    ///
    /// # Errors
    ///
    /// The same as [`Voice::start_on`]. On failure nothing has changed and
    /// `self` is still running.
    pub fn switch_capture(
        &self,
        capture: Option<&str>,
        media: MediaChannel,
        ssrc: Ssrc,
    ) -> Result<Self> {
        self.switch_to(
            &DeviceChoice {
                capture: capture.map(str::to_owned),
                playback: self.chosen.playback.clone(),
            },
            media,
            ssrc,
        )
    }

    /// Starts a second path on another output, carrying this one's settings.
    ///
    /// The twin of [`Voice::switch_capture`], and it exists rather than being
    /// left to each shell for the reason written there: the list of what has to
    /// survive a reopening lives here, once, so that no shell forgets an item.
    /// The item that hurts on this side is Isolamento total — somebody who
    /// changed output because they could not hear anything is, often enough,
    /// somebody whose speakers are muted, and a switch that quietly unmutes them
    /// puts a server into a room that had been silent.
    ///
    /// The microphone stays where it is, asked for exactly as this path asked
    /// for it.
    ///
    /// # Errors
    ///
    /// The same as [`Voice::start_on`]. On failure nothing has changed and
    /// `self` is still running — which on this side means the sound is still
    /// coming out of the old device rather than out of nowhere.
    pub fn switch_playback(
        &self,
        playback: Option<&str>,
        media: MediaChannel,
        ssrc: Ssrc,
    ) -> Result<Self> {
        self.switch_to(
            &DeviceChoice {
                capture: self.chosen.capture.clone(),
                playback: playback.map(str::to_owned),
            },
            media,
            ssrc,
        )
    }

    /// Opens the same devices again on a new connection, carrying the settings.
    ///
    /// For a reconnection, where the ssrc and the media channel are both new and
    /// the voice path therefore has to be rebuilt. The **ask** is repeated, not
    /// the result: a preference that had fallen back gets another chance, which
    /// is what a person who plugged the interface back in expects.
    ///
    /// Falls back per side like [`Voice::start_preferring`], because a
    /// reconnection is not a moment to charge somebody the rest of their voice
    /// for a device that moved while they were off the air.
    ///
    /// # Errors
    ///
    /// The same as [`Voice::start_preferring`]. On failure nothing has changed
    /// and `self` is still running, though on a connection that is now dead.
    pub fn reopen(&self, media: MediaChannel, ssrc: Ssrc) -> Result<Self> {
        let fresh = Self::start_preferring(&self.chosen, media, ssrc)?;
        self.carry_over(&fresh);
        Ok(fresh)
    }

    /// Reopens on `chosen` and carries every control across.
    ///
    /// The one place that list is written. Both switches and the reopening go
    /// through here, so an item added to it is added for all three at once.
    fn switch_to(&self, chosen: &DeviceChoice, media: MediaChannel, ssrc: Ssrc) -> Result<Self> {
        let fresh = Self::start_on(chosen, media, ssrc)?;
        self.carry_over(&fresh);
        Ok(fresh)
    }

    /// Puts this path's controls onto a freshly opened one.
    fn carry_over(&self, fresh: &Self) {
        carregar_controles(&self.controls, &fresh.controls);
    }

    /// Onde o relógio de mídia recomeça depois de trocar de dispositivo.
    ///
    /// # Por que pular, e não só continuar
    ///
    /// Porque os dois caminhos existem ao mesmo tempo por um instante: o novo é
    /// aberto **antes** de o velho ser largado, de propósito — um microfone que
    /// sumiu deixa a pessoa falando pelo antigo em vez de muda. Enquanto os dois
    /// vivem, o velho ainda manda quadros com carimbos que crescem, e um caminho
    /// novo que continuasse do último número visto ficaria atrás deles. Quem recebe
    /// descarta o que vem atrás do que já tocou, e a pessoa some de novo — pelo
    /// mesmo defeito, uma volta depois.
    ///
    /// Um segundo de folga passa na frente de qualquer coisa que ainda esteja no ar
    /// e custa, a quem escuta, um silêncio de um segundo que **de fato aconteceu**:
    /// abrir um dispositivo de áudio leva esse tempo. O buffer de jitter lê o pulo
    /// como um vão, esconde o que dá e reacerta — que é exatamente o que ele existe
    /// para fazer.
    ///
    /// A sequência anda um, e não mil: ela conta o que sai, e quem recebe usa a
    /// diferença entre ela e o carimbo para separar silêncio de perda (M1.9).
    fn salto_do_relogio(seq: u16, carimbo: u32) -> (u16, u32) {
        (
            seq.wrapping_add(1),
            carimbo.wrapping_add(seele_audio::SAMPLE_RATE_HZ),
        )
    }

    /// Mutes the microphone — mudo.
    pub fn set_muted(&self, on: bool) {
        self.controls.muted.store(on, Ordering::Relaxed);
    }

    /// Whether the microphone is muted.
    #[must_use]
    pub fn muted(&self) -> bool {
        self.controls.muted.load(Ordering::Relaxed)
    }

    /// Mutes the speakers — Isolamento total.
    pub fn set_total_isolation(&self, on: bool) {
        self.controls.total_isolation.store(on, Ordering::Relaxed);
    }

    /// Põe na fila um pacote de som da tela que se está assistindo.
    ///
    /// Chamado por quem lê o fluxo da tela, que é uma tarefa `async` noutro
    /// lugar. A decodificação acontece na thread de áudio, junto com a da voz:
    /// é lá que o codec já mora, e é lá que o isolamento total decide se alguma
    /// coisa toca.
    pub fn som_da_tela(&self, tela: ScreenId, pacote: Vec<u8>) {
        enfileirar_som_da_tela(&self.controls, tela, pacote);
    }

    /// Esquece o som da tela. Chamado quando a transmissão acaba.
    pub fn esquecer_o_som_da_tela(&self, tela: ScreenId) -> bool {
        esquecer_o_som_de(&self.controls, tela)
    }

    /// O volume do som da transmissão. `1.0` é o de origem, `0.0` é calado.
    ///
    /// Independente do volume das pessoas e do isolamento total: silenciar o
    /// conteúdo compartilhado tem de preservar a conversa, que é o pedido do R17.
    /// O teto é o mesmo de [`Self::set_gain`] — quatro vezes —, porque é a mesma
    /// mistura.
    pub fn set_volume_da_tela(&self, volume: f32) {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "o valor é fixado entre 0 e 4 antes da conversão"
        )]
        let milesimos = (volume.clamp(0.0, 4.0) * 1000.0) as u32;
        self.controls
            .volume_da_tela
            .store(milesimos, Ordering::Relaxed);
    }

    /// O volume do som da transmissão, como a casca o mostra.
    #[must_use]
    pub fn volume_da_tela(&self) -> f32 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "quatro mil milésimos cabem exatos num f32"
        )]
        let volume = self.controls.volume_da_tela.load(Ordering::Relaxed) as f32 / 1000.0;
        volume
    }

    /// Cala ou devolve o som da transmissão, **sem mexer no volume**.
    ///
    /// Separado do volume como o mudo do microfone é separado do ganho: voltar tem
    /// de devolver o volume que a pessoa escolheu, e um mudo que zerasse o volume
    /// apagaria a escolha.
    pub fn set_tela_calada(&self, calada: bool) {
        self.controls.tela_calada.store(calada, Ordering::Relaxed);
    }

    /// Se o som da transmissão está calado.
    #[must_use]
    pub fn tela_calada(&self) -> bool {
        self.controls.tela_calada.load(Ordering::Relaxed)
    }

    /// Liga, desliga ou dosa a supressão de ruído do microfone. F02.
    ///
    /// `1.0` é ela inteira, `0.0` a desliga. Valores no meio existem porque um
    /// filtro agressivo demais é um filtro que alguém desliga: quem achar a voz
    /// «de rádio» abaixa em vez de abrir mão do resto.
    ///
    /// **Não perde o piso aprendido.** Desligar e religar no meio de uma conversa
    /// custaria 100 ms de aprendizado de novo, e a primeira frase depois de religar
    /// sairia cortada — F02, critério 4.
    pub fn set_supressao(&self, forca: f32) {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "o valor é fixado entre 0 e 1 antes da conversão"
        )]
        let milesimos = (forca.clamp(0.0, 1.0) * 1000.0) as u32;
        self.controls.supressao.store(milesimos, Ordering::Relaxed);
    }

    /// Quanto da supressão de ruído está sendo aplicado.
    #[must_use]
    pub fn supressao(&self) -> f32 {
        forca_da_supressao(&self.controls)
    }

    /// A sensibilidade de abertura da ativação por voz, em dBFS. F01.
    ///
    /// `None` volta ao padrão, que depende de a supressão estar ligada — ver
    /// [`configuracao_do_portao`]. É a decisão de F01 por inteiro: o alvo de
    /// −60 dBFS é do caminho **com** supressão, e sem ela o limiar antigo continua
    /// sendo a única defesa contra o ventilador.
    ///
    /// **Não fecha o microfone.** Arrastar o controle durante uma frase não pode
    /// interromper a frase, ou a calibração só acontece entre frases — que é
    /// quando não há o que ouvir.
    pub fn set_abertura_dbfs(&self, dbfs: Option<f32>) {
        let milesimos = match dbfs {
            None => 0,
            Some(valor) => {
                let fixado = valor.clamp(
                    *seele_audio::gate::FAIXA_DE_ABERTURA_DBFS.start(),
                    *seele_audio::gate::FAIXA_DE_ABERTURA_DBFS.end(),
                );
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "a faixa é de −72 a −24 dBFS; em milésimos cabe folgado num i32"
                )]
                let convertido = (fixado * 1000.0) as i32;
                convertido
            }
        };
        self.controls
            .abertura_dbfs_milesimos
            .store(milesimos, Ordering::Relaxed);
    }

    /// A sensibilidade que está valendo, em dBFS, e se ela foi escolhida.
    ///
    /// O segundo campo existe para a interface poder dizer «padrão» em vez de um
    /// número que a pessoa não escolheu — e para o controle poder voltar ao padrão
    /// em vez de ficar preso no valor que o padrão tinha quando ela o tocou.
    #[must_use]
    pub fn abertura_dbfs(&self) -> (f32, bool) {
        let escolhido = self
            .controls
            .abertura_dbfs_milesimos
            .load(Ordering::Relaxed);
        (
            configuracao_do_portao(&self.controls).abertura_dbfs(),
            escolhido != 0,
        )
    }

    /// Whether the speakers are muted.
    #[must_use]
    pub fn total_isolation(&self) -> bool {
        self.controls.total_isolation.load(Ordering::Relaxed)
    }

    /// Reports the push-to-talk key going down or coming up.
    pub fn set_key_held(&self, held: bool) {
        self.controls.key_held.store(held, Ordering::Relaxed);
    }

    /// Chooses how the microphone opens.
    pub fn set_mode(&self, mode: VoiceMode) {
        self.controls.mode.store(mode.as_byte(), Ordering::Relaxed);
    }

    /// How the microphone currently opens.
    #[must_use]
    pub fn mode(&self) -> VoiceMode {
        VoiceMode::from_byte(self.controls.mode.load(Ordering::Relaxed))
    }

    /// Sets one talker's volume. `1.0` is unchanged, `0.0` is muted.
    ///
    /// `specs/03-audio.md` asks for per-user volume, and this is where it lives:
    /// it is a property of the mix, not of the interface showing the mix.
    pub fn set_gain(&self, ssrc: u32, gain: f32) {
        if let Ok(mut gains) = self.controls.gains.lock() {
            gains.insert(ssrc, gain.clamp(0.0, 4.0));
        }
    }

    /// Quantos quadros de voz o transporte recusou desde que esta voz abriu.
    ///
    /// Zero é o normal. Qualquer coisa acima disso significa que o áudio está
    /// sendo perdido **antes de sair desta máquina**, e a diferença importa:
    /// perda de rede e recusa de envio soam idênticas e têm consertos opostos.
    /// A causa mais provável é o datagrama não caber no caminho — o QUIC não o
    /// fragmenta, ao contrário do fluxo por onde o texto viaja, e é por isso
    /// que um enlace pode entregar todo o texto e picotar a voz num sentido só.
    #[must_use]
    pub fn quadros_recusados(&self) -> u64 {
        self.controls.recusados.load(Ordering::Relaxed)
    }

    /// Quantas amostras a reprodução produziu e o anel do dispositivo recusou.
    ///
    /// A gêmea de [`Voice::quadros_recusados`], na outra ponta do laço. Zero é
    /// o normal. Acima disso o áudio está se perdendo **depois** de decodificado
    /// e antes do alto-falante, e a causa é sempre a mesma: a mistura entregou
    /// mais depressa do que o dispositivo consumiu. Dividir por 48 dá
    /// milissegundos.
    #[must_use]
    pub fn amostras_recusadas_pelo_anel(&self) -> u64 {
        self.controls.anel_cheio.load(Ordering::Relaxed)
    }

    /// Whether this person is transmitting right now.
    #[must_use]
    pub fn speaking(&self) -> bool {
        self.controls.speaking.load(Ordering::Relaxed)
    }

    /// The latest measurements.
    #[must_use]
    pub fn telemetry(&self) -> AudioTelemetry {
        self.telemetry
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default()
    }

    /// Se esta máquina está derrubando áudio agora.
    ///
    /// Consultar **move** o detector: cada chamada é uma olhada, e é a
    /// diferença entre duas olhadas que define a falha. As cascas perguntam
    /// isto uma vez por quadro de telemetria.
    #[must_use]
    pub fn falha_local(&self) -> bool {
        let telemetria = self.telemetry();
        self.falha_local
            .lock()
            .map(|mut detector| detector.observar(&telemetria.local))
            .unwrap_or(false)
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.controls.stop.store(true, Ordering::Relaxed);
    }
}

/// One talker being decoded.
struct Source {
    ssrc: u32,
    buffer: JitterBuffer<Vec<u8>>,
    drift: DriftTracker,
    decoder: VoiceDecoder,
    /// Quando chegou o último quadro desta pessoa, no relógio deste laço.
    ///
    /// Serve para esquecê-la depois de [`SILENCIO_ATE_ESQUECER_MS`]. Ver ali
    /// por que esquecer é o certo.
    ultimo_ms: f64,
}

/// Quanto silêncio faz uma pessoa ser esquecida por quem escuta.
///
/// # Por que esquecer alguém é bom
///
/// A tabela de quem fala era um `Vec` que só crescia: uma pessoa que entrou,
/// falou e saiu ficava lá com um decodificador e um buffer inteiros até a
/// sessão acabar. Isso era só desperdício.
///
/// O que **não** era desperdício é a segunda metade: um buffer velho guarda até
/// onde aquela pessoa já tocou, e recusa qualquer carimbo anterior a isso. Se o
/// relógio dela recomeçar por qualquer motivo — o conserto de
/// `Controls::relogio_seq` cobre o motivo conhecido, e nada garante que era o
/// único —, a pessoa some para sempre e nem sair e voltar resolve. Esquecê-la
/// depois de meio minuto calado transforma «para sempre» em «meio minuto».
///
/// Meio minuto e não cinco segundos porque o silêncio aqui é normal: o DTX não
/// manda nada enquanto ninguém fala, então uma pessoa quieta numa reunião fica
/// sem mandar um quadro por minutos a fio. O preço de esquecer é o buffer
/// encher de novo na primeira palavra — algumas dezenas de milissegundos.
const SILENCIO_ATE_ESQUECER_MS: f64 = 30_000.0;

/// Quem reabre os aparelhos por conta do laço.
///
/// Repete **o pedido** e não o resultado, e cai para o aparelho da máquina um
/// lado de cada vez — é `open_preferring`, o mesmo caminho de uma reconexão.
/// Reabrir com o id do aparelho que acabou de sumir prenderia a sessão a um
/// aparelho que não existe; reabrir sempre no padrão jogaria fora a escolha de
/// quem escolheu.
///
/// Reenumera a cada tentativa: `device::open` pergunta ao host de novo, e é aí
/// que o padrão de agora — o que a pessoa acabou de escolher na bandeja do
/// sistema — entra.
struct ReabrirAparelhos<'a> {
    escolha: &'a DeviceChoice,
}

impl Reabertura for ReabrirAparelhos<'_> {
    type Aberto = AudioIo;
    type Erro = device::DeviceError;

    fn reabrir(&mut self) -> Result<AudioIo, device::DeviceError> {
        open_preferring(self.escolha).inspect_err(|falha| {
            tracing::warn!(%falha, "a reabertura do aparelho não conseguiu abrir nada");
        })
    }

    /// Os contadores do aparelho que acabou de abrir, que nascem zerados em
    /// `device::open` — e é deles que a volta seguinte do laço lê, porque ela
    /// troca o `AudioIo` inteiro.
    fn aviso_de(&self, aberto: &AudioIo) -> AvisoDeAparelho {
        aberto.counters.aviso_de_aparelho()
    }
}

/// Tudo o que as taxas do aparelho dimensionam, numa peça só.
///
/// Numa peça só e construídas num lugar só porque as quatro têm de ser refeitas
/// **juntas** quando o aparelho muda: um reamostrador da taxa antiga com um anel
/// da taxa nova toca rápido ou devagar para sempre — o defeito que chega como «a
/// voz ficou estranha depois que troquei o fone». Separadas, a reabertura podia
/// refazer três e esquecer a quarta, e nada no código diria isso.
///
/// Fora do laço pelo mesmo motivo que [`Acompanhamento`]: o laço tem `cpal` de um
/// lado e um socket do outro, e não é testável. Isto é.
struct Dimensoes {
    /// Do aparelho de captura para os 48 kHz do encanamento.
    para_o_laco: RateConverter,
    /// Dos 48 kHz para a saída, ajustável — ver o comentário em `pipeline`.
    para_o_aparelho: RateConverter,
    /// Capacidade do anel de saída, em amostras.
    anel: usize,
    /// A malha que segura o anel longe das duas pontas.
    ritmo: RingPacer,
}

impl Dimensoes {
    /// As dimensões de um aparelho que acabou de abrir.
    ///
    /// `None` quando o reamostrador recusa as taxas dele, que é o único caso em
    /// que não há laço possível com este aparelho.
    fn do_aparelho(captura_hz: u32, saida_hz: u32, anel: usize) -> Option<Self> {
        let (Ok(para_o_laco), Ok(para_o_aparelho)) = (
            RateConverter::new(captura_hz, SAMPLE_RATE_HZ),
            RateConverter::new_adjustable(SAMPLE_RATE_HZ, saida_hz),
        ) else {
            return None;
        };
        Some(Self {
            para_o_laco,
            para_o_aparelho,
            anel,
            ritmo: RingPacer::new(saida_hz, anel),
        })
    }
}

/// As dimensões do aparelho, **e** a tela dizendo a verdade quando não há.
///
/// Uma função só porque as duas coisas não podem se separar: recusar as taxas
/// encerra o laço, e um laço que encerra deixando o painel em «funcionando» é a
/// pessoa sem som nenhum lendo normalidade na tela. Junto aqui, o teste
/// consegue provar a ligação entre as duas — separadas, ela só existia na
/// ordem em que o laço, que nenhum teste alcança, chamava uma e depois a outra.
fn dimensoes_ou_dizer_que_nao_ha(
    painel: &Mutex<EstadoDoAudio>,
    captura_hz: u32,
    saida_hz: u32,
    anel: usize,
) -> Option<Dimensoes> {
    let dimensoes = Dimensoes::do_aparelho(captura_hz, saida_hz, anel);
    if dimensoes.is_none() {
        tracing::error!(
            captura_hz,
            saida_hz,
            "o aparelho abriu e o reamostrador recusou as taxas dele; não há laço possível"
        );
        painel_mesmo_envenenado(painel).sem_laco_possivel();
    }
    dimensoes
}

/// Esvazia o que ficou a caminho, porque era do aparelho anterior.
///
/// Amostras na taxa de antes, tocadas no aparelho de agora, saem como um estalo
/// ou um trecho acelerado no instante da troca — o primeiro som que a pessoa
/// ouve do aparelho novo, e o pior possível.
fn esvaziar_o_que_era_do_antigo(restos: [&mut Vec<f32>; 4]) {
    for resto in restos {
        resto.clear();
    }
}

/// Tudo o que precisa acontecer na volta em que o aparelho mudou.
///
/// Uma função só, e pelo mesmo motivo de [`dimensoes_ou_dizer_que_nao_ha`]: os
/// três passos não se separam, e nenhum teste alcança o laço onde eles moravam.
/// Separados, cada um tinha o seu teste de unidade e a **ordem** entre eles —
/// que é o que a pessoa ouve — não tinha nenhum.
///
/// O que cada passo impede, se faltar:
///
/// - **Redimensionar.** O aparelho novo pode ter outra taxa e outro anel, e
///   reaproveitar os de antes é tocar mais rápido ou mais devagar para sempre —
///   o defeito que soa como «a voz ficou estranha depois que troquei o fone».
/// - **Esvaziar.** Amostras na taxa de antes, tocadas no aparelho de agora,
///   saem como um estalo no primeiro instante do aparelho novo.
/// - **Reacertar o relógio.** Abrir um endpoint custa centenas de
///   milissegundos, e essa pausa é desta volta do laço. Sem reacertar, ela
///   entraria no relógio de reprodução como reacerto e como atraso máximo — e o
///   instrumento que responde *é a rede ou é esta máquina?* passaria a acusar a
///   máquina de quem apenas trocou de fone. Ver [`PlayoutClock::reacertar`].
///
/// `None` quando o reamostrador recusa as taxas do aparelho novo: aí não há
/// laço possível, o painel já foi avisado e não há o que esvaziar nem
/// reacertar, porque o laço acaba nesta volta.
fn recomecar_no_aparelho_novo(
    painel: &Mutex<EstadoDoAudio>,
    captura_hz: u32,
    saida_hz: u32,
    anel: usize,
    restos: [&mut Vec<f32>; 4],
    playout: &mut PlayoutClock,
    agora: Instant,
) -> Option<Dimensoes> {
    let dimensoes = dimensoes_ou_dizer_que_nao_ha(painel, captura_hz, saida_hz, anel)?;
    esvaziar_o_que_era_do_antigo(restos);
    playout.reacertar(agora);
    Some(dimensoes)
}

/// O que uma troca conseguida entrega ao laço.
///
/// O aparelho aberto e as dimensões dele **juntos**, porque quem recebe um sem
/// o outro é quem toca no aparelho novo com o reamostrador do velho.
pub struct TrocaFeita<A> {
    /// O aparelho que abriu. O laço larga o que tinha e passa a ler deste.
    pub aberto: A,
    dimensoes: Dimensoes,
}

/// O que a volta do laço descobriu sobre o aparelho.
pub enum PassoDoAparelho<A> {
    /// Nada aconteceu com o aparelho. O caso comum, e não é falha.
    Segue,
    /// Um aparelho novo abriu e já está dimensionado.
    ///
    /// Numa caixa porque o caso comum é [`Self::Segue`], devolvido a cada volta
    /// do laço: sem a caixa, toda volta carregaria de volta o tamanho de dois
    /// reamostradores para dizer «nada aconteceu». A alocação cai na volta em
    /// que o aparelho mudou, que é a volta em que um endpoint acabou de ser
    /// aberto — ela não se compara ao que já custou ali.
    Reaberto(Box<TrocaFeita<A>>),
    /// O aparelho abriu e o reamostrador recusou as taxas dele.
    ///
    /// Não há laço possível: o painel já foi avisado e quem chama encerra.
    SemLacoPossivel,
}

/// A volta do laço no que diz respeito ao aparelho, inteira.
///
/// # Por que ela existe em vez de morar dentro de `pipeline`
///
/// Porque morando lá dentro ela não tinha teste de comportamento nenhum. O
/// ciclo tinha o seu, o redimensionamento tinha o seu, e a **composição** dos
/// dois — ler o aviso, reabrir, redimensionar junto, largar o que era do
/// aparelho antigo e reacertar o relógio, nessa ordem — só existia na ordem em
/// que o laço as chamava, e nenhum teste alcança o laço: ele tem `cpal` de um
/// lado e um socket QUIC do outro. O teste de conformidade da troca refazia
/// essa composição à mão, e um teste que refaz o que ele deveria provar prova o
/// teste. Agora é esta função que ele conduz, e ela é a mesma que roda em
/// produção; o que sobra do outro lado do traço é só a atribuição das peças
/// devolvidas — e um `cpal` que nenhuma máquina de CI tem como exercer.
#[allow(
    clippy::too_many_arguments,
    reason = "são os locais do próprio laço; uma estrutura para agrupá-los \
              existiria só para o lint e teria de ser desmontada dos dois lados"
)]
pub fn seguir_o_aparelho<R>(
    acompanhamento: &mut Acompanhamento,
    aviso: AvisoDeAparelho,
    relogio_ms: f64,
    abridor: &mut R,
    painel: &Mutex<EstadoDoAudio>,
    restos: [&mut Vec<f32>; 4],
    playout: &mut PlayoutClock,
    agora: Instant,
) -> PassoDoAparelho<R::Aberto>
where
    R: Reabertura,
    R::Aberto: AparelhosAbertos,
{
    let Some(aberto) = acompanhamento.passo(aviso, relogio_ms, abridor, painel) else {
        return PassoDoAparelho::Segue;
    };
    let taxas = aberto.taxas();
    let Some(dimensoes) = recomecar_no_aparelho_novo(
        painel,
        taxas.capture_hz,
        taxas.playback_hz,
        aberto.capacidade_de_saida(),
        restos,
        playout,
        agora,
    ) else {
        return PassoDoAparelho::SemLacoPossivel;
    };
    PassoDoAparelho::Reaberto(Box::new(TrocaFeita { aberto, dimensoes }))
}

/// The whole loop, on its own thread.
#[allow(
    clippy::too_many_lines,
    reason = "one real-time loop; splitting it would hide the ordering that matters"
)]
async fn pipeline(
    mut io: AudioIo,
    media: MediaChannel,
    ssrc: Ssrc,
    controls: Arc<Controls>,
    telemetry: Arc<Mutex<AudioTelemetry>>,
    aparelhos: Arc<Mutex<EstadoDoAudio>>,
    escolha: DeviceChoice,
) {
    let Ok(mut encoder) = VoiceEncoder::with_defaults() else {
        return;
    };
    // A saída é **ajustável** e a entrada não, e a assimetria é o M1.8. O
    // dispositivo de saída consome no ritmo do cristal dele enquanto este laço
    // produz no ritmo do `Instant`, e a diferença só cabe no anel entre os dois:
    // ela o encosta no fundo ou no topo e o deixa lá. Corrigir isso é reamostrar
    // por algumas partes por milhão, o que exige um filtro para guiar mesmo com
    // 48 kHz dos dois lados — que é o caso comum, e é onde a deriva se esconde.
    //
    // A captura não precisa: este laço a drena inteira a cada volta, então a
    // diferença de cristal de lá não se acumula em anel nenhum. Ela sai daqui
    // como quadros ligeiramente mais rápidos ou mais lentos, e quem a corrige é
    // o `DriftTracker` de quem recebe.
    let Some(Dimensoes {
        para_o_laco: mut to_pipeline,
        para_o_aparelho: mut to_device,
        anel: mut anel_de_saida,
        mut ritmo,
    }) = dimensoes_ou_dizer_que_nao_ha(
        &aparelhos,
        io.capture_rate_hz,
        io.playback_rate_hz,
        io.to_device.buffer().capacity(),
    )
    else {
        return;
    };
    let mut ritmo_avisado = false;

    // O ciclo do aparelho. Sem ele, um aparelho trocado no sistema operacional
    // só valia reiniciando o aplicativo: o `cpal` avisava pelo retorno de erro,
    // o aviso virava «mais um» num contador, e este laço seguia falando com um
    // endpoint que já não era o de ninguém.
    let mut acompanhamento = Acompanhamento::novo();
    let mut abridor = ReabrirAparelhos { escolha: &escolha };

    let mut gate = VoiceGate::new(configuracao_do_portao(&controls), GateMode::PushToTalk);
    // A supressão de ruído do microfone. F02.
    //
    // Uma por caminho de voz, como o ganho, e pelo mesmo motivo: ela guarda o piso
    // aprendido entre quadros. Ver `seele_audio::supressao`.
    let mut supressao = seele_audio::supressao::Supressao::nova(forca_da_supressao(&controls));
    let mut limpo: Vec<f32> = Vec::new();
    let mut a_transmitir: Vec<Vec<f32>> = Vec::new();
    let mut supressao_avisada = false;
    let mut mixer = Mixer::new();
    let mut sources: Vec<Source> = Vec::new();
    // O decodificador do som da tela, à parte dos das pessoas.
    //
    // À parte porque a fonte é outra: a voz de cada um chega por datagrama, com
    // perda e reordenação, e por isso tem fila de compasso e ocultação. O som da
    // tela chega pelo fluxo QUIC da imagem, que é ordenado e confiável — não há
    // buraco a ocultar nem ordem a corrigir, e dar-lhe uma fila de compasso
    // seria acrescentar atraso para resolver um problema que ele não tem.
    let mut som_da_tela = VoiceDecoder::new().ok();
    let mut som_da_tela_avisado = false;

    let (mut captured, mut at_48k, mut pending) = (Vec::new(), Vec::new(), Vec::<f32>::new());
    // O ganho automático do microfone. Um por caminho de voz, e ele guarda
    // estado entre quadros — é isso que faz a subida ser lenta o bastante para
    // ninguém ouvir.
    let mut ganho = seele_audio::ganho::Ganho::novo();
    let (mut ganho_avisado, mut quadros_com_ganho) = (false, 0_u32);
    let mut datagram = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let mut mixed = vec![0.0_f32; FRAME_SAMPLES];
    let mut for_device = Vec::new();

    // De onde o caminho anterior parou, e não de zero. Ver `Controls::relogio_seq`.
    let (mut seq, mut timestamp) = (
        controls.relogio_seq.load(Ordering::Relaxed) as u16,
        controls.relogio_carimbo.load(Ordering::Relaxed),
    );
    let started = Instant::now();
    // Não é `Instant + 20 ms` somado à mão. Este laço faz outras cinco coisas
    // entre duas conferidas do prazo, e quanto tempo isso custa é do sistema
    // operacional — ver `seele_audio::playout`, que é onde a conta está.
    let mut playout = PlayoutClock::new(Instant::now(), FRAME_MS);
    let mut atraso_avisado = false;
    let mut next_telemetry = Instant::now() + TELEMETRY_EVERY;

    while !controls.stop.load(Ordering::Relaxed) {
        // ---- o aparelho, antes de qualquer amostra ----
        //
        // Duas leituras atômicas por volta quando não há nada acontecendo, que é
        // o caso comum. Primeiro de tudo porque capturar e tocar num aparelho
        // que já foi embora é gastar uma volta inteira para produzir silêncio.
        let relogio_ms = started.elapsed().as_secs_f64() * 1000.0;
        match seguir_o_aparelho(
            &mut acompanhamento,
            io.counters.aviso_de_aparelho(),
            relogio_ms,
            &mut abridor,
            &aparelhos,
            [&mut pending, &mut captured, &mut at_48k, &mut for_device],
            &mut playout,
            Instant::now(),
        ) {
            PassoDoAparelho::Segue => {}
            // O painel já diz que não há laço possível; ver
            // `EstadoDoAudio::sem_laco_possivel`.
            PassoDoAparelho::SemLacoPossivel => return,
            PassoDoAparelho::Reaberto(troca) => {
                io = troca.aberto;
                to_pipeline = troca.dimensoes.para_o_laco;
                to_device = troca.dimensoes.para_o_aparelho;
                anel_de_saida = troca.dimensoes.anel;
                ritmo = troca.dimensoes.ritmo;
                ritmo_avisado = false;

                tracing::info!(
                    microfone = ?io.capture.as_ref().map(|aparelho| &aparelho.name),
                    saida = ?io.playback.as_ref().map(|aparelho| &aparelho.name),
                    estado = ?acompanhamento.estado(),
                    "o aparelho de áudio mudou; a voz foi reaberta no aparelho de agora"
                );
            }
        }

        // ---- receive ----
        while let Ok(Ok(bytes)) = tokio::time::timeout(Duration::from_millis(1), media.next()).await
        {
            let arrival_ms = started.elapsed().as_secs_f64() * 1000.0;
            let Ok((header, payload)) = MediaHeader::decode(&bytes) else {
                continue;
            };
            // Our own audio coming back is not something to play.
            if header.ssrc == ssrc.get() {
                continue;
            }
            let index = match sources.iter().position(|s| s.ssrc == header.ssrc) {
                Some(index) => index,
                None => {
                    let Ok(decoder) = VoiceDecoder::new() else {
                        continue;
                    };
                    sources.push(Source {
                        ssrc: header.ssrc,
                        buffer: JitterBuffer::new(JitterConfig::default()),
                        drift: DriftTracker::new(),
                        decoder,
                        ultimo_ms: arrival_ms,
                    });
                    sources.len() - 1
                }
            };
            if let Some(source) = sources.get_mut(index) {
                source.ultimo_ms = arrival_ms;
                let sent_ms = f64::from(header.timestamp) / f64::from(SAMPLE_RATE_HZ) * 1000.0;
                source.drift.observe(arrival_ms - sent_ms, arrival_ms);
                source
                    .buffer
                    .push(header.seq, header.timestamp, arrival_ms, payload.to_vec());
            }
        }

        // Quem calou faz meio minuto é esquecido. Ver `SILENCIO_ATE_ESQUECER_MS`.
        let agora_ms = started.elapsed().as_secs_f64() * 1000.0;
        sources.retain(|fonte| agora_ms - fonte.ultimo_ms < SILENCIO_ATE_ESQUECER_MS);

        // ---- capture, encode, send ----
        gate.set_mode(VoiceMode::from_byte(controls.mode.load(Ordering::Relaxed)).to_gate());
        gate.set_key_held(controls.key_held.load(Ordering::Relaxed));
        // **A régua e a força são relidas a cada volta**, e as duas trocam sem
        // fechar o microfone: arrastar o controle de sensibilidade durante uma frase
        // não pode interromper a frase, e ligar ou desligar o filtro não pode custar
        // o piso aprendido. F01 e F02, critério 4.
        //
        // Fora do laço de quadros de propósito: a volta é de 20 ms, e as duas
        // leituras são átomos — comparar antes de escrever seria dois ramos para
        // evitar duas escritas que custam nada.
        gate.ajustar(configuracao_do_portao(&controls));
        supressao.ajustar(forca_da_supressao(&controls));

        // Rebuilds the encoder when it actually changes, and only then — see
        // `VoiceEncoder::set_bitrate` on why a no-op must stay a no-op.
        let _ = encoder.set_bitrate(controls.bitrate.load(Ordering::Relaxed));

        captured.clear();
        while let Ok(sample) = io.captured.pop() {
            captured.push(sample);
        }
        at_48k.clear();
        if to_pipeline.push(&captured, &mut at_48k).is_ok() {
            pending.extend_from_slice(&at_48k);
        }

        let muted = controls.muted.load(Ordering::Relaxed);
        while pending.len() >= FRAME_SAMPLES {
            let bruto: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();

            // **A supressão, antes de qualquer decisão sobre o quadro.** F02, e a
            // ordem inteira é `captura → supressão → portão → ganho → codec`:
            //
            // - antes do **portão**, porque é ela que torna a sensibilidade de F01
            //   defensável — com o piso da sala removido, o mesmo limiar ouve fala
            //   baixa em vez de ouvir a sala;
            // - antes do **ganho**, porque o ganho amplifica o que recebe. Depois
            //   dela ele amplifica voz; antes, amplificaria o ventilador junto.
            //
            // Desligada, `processar` devolve o que entrou, sem FFT e sem atraso.
            // Ligada, ela retém meio quadro no começo — por isso pode devolver menos
            // do que recebeu, e o laço trata a saída como fluxo.
            supressao.processar(&bruto, &mut limpo);
            if limpo.is_empty() {
                // Ainda dentro do atraso de um salto. O relógio anda mesmo assim:
                // ele conta amostras que passaram, e elas passaram.
                timestamp =
                    timestamp.wrapping_add(u32::try_from(FRAME_SAMPLES).unwrap_or(FRAME_MS));
                controls.relogio_carimbo.store(timestamp, Ordering::Relaxed);
                continue;
            }
            if !supressao_avisada && supressao.forca() > 0.0 {
                supressao_avisada = true;
                tracing::info!(
                    forca = supressao.forca(),
                    atraso_amostras = seele_audio::supressao::Supressao::atraso_em_amostras(),
                    "a supressão de ruído do microfone está tratando o sinal"
                );
            }
            let frame: Vec<f32> = std::mem::take(&mut limpo);

            // The gate still runs while muted, so the level meter keeps moving
            // and somebody talking into a muted microphone can see that they
            // are. Not showing that is how people give whole speeches to nobody.
            //
            // **E ele entrega mais de um quadro na abertura**, que é a retenção da
            // primeira sílaba de F01: o ataque de uma consoante surda está abaixo do
            // limiar, e quando o nível sobe o bastante para abrir ele já passou. Ver
            // `VoiceGate::quadros_a_transmitir`.
            gate.quadros_a_transmitir(&frame, &mut a_transmitir);
            let open = !a_transmitir.is_empty();
            let speaking = open && !muted;
            controls.speaking.store(speaking, Ordering::Relaxed);

            // The timestamp counts elapsed samples whether or not anything goes
            // out; the sequence counts only what does. That difference is what
            // lets the receiver tell DTX silence from real loss — M1.9.
            timestamp = timestamp.wrapping_add(u32::try_from(FRAME_SAMPLES).unwrap_or(FRAME_MS));
            controls.relogio_carimbo.store(timestamp, Ordering::Relaxed);
            if !speaking {
                continue;
            }

            // Os quadros retidos e o que abriu, em ordem.
            let quantos = a_transmitir.len();
            for (indice, mut frame) in std::mem::take(&mut a_transmitir).into_iter().enumerate() {
                // **Cada quadro com o instante em que ele aconteceu.** Os retidos
                // são de antes deste; ver `carimbo_do_quadro`.
                let carimbo = carimbo_do_quadro(timestamp, quantos, indice);
                // **O ganho, aqui e não antes do portão.** Multiplicar antes faria
                // ruído de sala virar fala, e cada abertura à toa do portão é banda
                // gasta e voz de alguém sendo cortada para dar lugar a um
                // ventilador. Ver `seele_audio::ganho`.
                ganho.aplicar(&mut frame);
                // **Uma linha, quando o ganho assenta**, e ela existe porque a
                // primeira resposta de campo a este recurso foi «não notei muita
                // diferença» — que pode significar «o microfone já estava bom» ou
                // «não funcionou», e as duas pedem trabalhos opostos.
                //
                // Meio segundo de fala é o bastante para a subida sair do zero, e
                // uma vez por sessão porque a condição, quando é verdade, é verdade
                // cinquenta vezes por segundo.
                quadros_com_ganho += 1;
                if !ganho_avisado && quadros_com_ganho >= 25 {
                    ganho_avisado = true;
                    tracing::info!(
                        vezes = ganho.atual(),
                        "o ganho automático do microfone assentou"
                    );
                }

                // Encoded from `f32` directly: the pipeline is `f32` end to end,
                // and the conversion to `i16` that used to be here was a rounding
                // step that existed only because the call site did not know
                // `encode_f32` was available.
                let Ok(payload) = encoder.encode(&frame) else {
                    continue;
                };
                // Empty is DTX deciding this frame is silence, not a failure. The
                // timestamp already advanced, which is what lets the receiver tell
                // silence from loss — M1.9.
                if payload.is_empty() {
                    continue;
                }
                seq = seq.wrapping_add(1);
                controls
                    .relogio_seq
                    .store(u32::from(seq), Ordering::Relaxed);
                let header = MediaHeader {
                    version: seele_proto::PROTOCOL_VERSION,
                    // The server refuses anything but the ssrc it assigned — G2.
                    ssrc: ssrc.get(),
                    seq,
                    // **O deste quadro**, e não o do laço: ver `carimbo_do_quadro`.
                    timestamp: carimbo,
                };
                if let Ok(len) = header.encode_datagram(&payload, &mut datagram) {
                    if let Some(bytes) = datagram.get(..len) {
                        if media.send(bytes.to_vec()).is_err() {
                            // Contado e não registrado em log: isto acontece por
                            // quadro, cinquenta vezes por segundo, e um log por
                            // quadro afogaria o arquivo no exato momento em que
                            // alguém precisa lê-lo.
                            controls.recusados.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }

        // ---- playout ----
        //
        // **Quantos** venceram, e não se um venceu. A diferença é o defeito que
        // `seele_audio::playout` documenta: um quadro por volta é bastante
        // enquanto a volta durar menos que um quadro, e é um vazamento
        // permanente assim que ela durar mais.
        let vencidos = playout.due(Instant::now());
        if vencidos > 0 {
            // Isolamento total silences the mix rather than stopping the
            // pipeline: the jitter buffers keep draining, so unmuting lands the
            // person in the present instead of replaying the last ten seconds.
            mixer.set_master(if controls.total_isolation.load(Ordering::Relaxed) {
                0.0
            } else {
                1.0
            });
            // Fora do laço de quadros: os ganhos não mudam dentro de 20 ms, e
            // este é o único cadeado desta volta.
            if let Ok(gains) = controls.gains.lock() {
                for (talker, gain) in gains.iter() {
                    mixer.set_gain(*talker, *gain);
                }
            }
            // **O som da transmissão tem o ganho dele**, aplicado aqui pelo mesmo
            // caminho dos falantes e não por um segundo caminho de mistura. Ver
            // `Controls::volume_da_tela`: era o que faltava para silenciar o
            // conteúdo compartilhado preservando a conversa (R17), e o comentário
            // ao lado do `SSRC_DA_TELA` já dizia que este dia ia chegar.
            //
            // O mudo vence o volume, e não o apaga: voltar do mudo devolve o volume
            // que a pessoa escolheu.
            mixer.set_gain(
                SSRC_DA_TELA,
                if controls.tela_calada.load(Ordering::Relaxed) {
                    0.0
                } else {
                    #[allow(
                        clippy::cast_precision_loss,
                        reason = "quatro mil milésimos cabem exatos num f32"
                    )]
                    let volume = controls.volume_da_tela.load(Ordering::Relaxed) as f32 / 1000.0;
                    volume
                },
            );

            // O compasso, uma vez por volta e **antes** de empurrar: o que
            // interessa é o fundo do vale, que é onde o dispositivo fica sem
            // amostra. Ver `seele_audio::pacing`, que é onde a malha está.
            let profundidade = anel_de_saida.saturating_sub(io.to_device.slots());
            let compasso = ritmo.observe(
                profundidade,
                io.counters.playback_burst_frames(),
                Instant::now(),
            );
            for _ in 0..compasso.prime_samples {
                if io.to_device.push(0.0).is_err() {
                    controls.anel_cheio.fetch_add(1, Ordering::Relaxed);
                }
            }
            if let Some(razao) = compasso.ratio {
                // Uma linha por sessão, e ela responde em vez de sugerir: se o
                // reamostrador recusa a razão, a deriva fica sem correção e o
                // anel volta a encostar numa parede — mas o áudio continua
                // saindo, então nada mais avisaria.
                if to_device.adjust_ratio(razao).is_err() && !ritmo_avisado {
                    ritmo_avisado = true;
                    tracing::warn!(
                        razao,
                        "o reamostrador recusou a razão de compasso; a deriva de relógio \
                         entre esta máquina e o dispositivo fica sem correção"
                    );
                }
            }

            for _ in 0..vencidos {
                let mut decoded: Vec<(u32, Vec<f32>)> = Vec::new();
                for source in &mut sources {
                    let samples = match source.buffer.tick() {
                        Decision::Play(payload) => source.decoder.decode(&payload).ok(),
                        Decision::Conceal => source.decoder.conceal().ok(),
                        Decision::Silence | Decision::Comfort | Decision::Starved => None,
                    };
                    if let Some(samples) = samples {
                        decoded.push((source.ssrc, samples));
                    }
                }
                // O som da tela, como mais uma fonte da mistura.
                //
                // **Uma fonte e não um caminho próprio**, e é o que faz o
                // isolamento total continuar significando o que diz: quem se
                // isola não ouve nem a voz nem a tela, porque quem decide isso é
                // o `set_master` do misturador, uma vez, para todas as fontes.
                //
                // O `SSRC_DA_TELA` é reservado e não pode colidir com o de
                // ninguém: `Ssrc` de pessoa vem do servidor, que nunca o
                // entrega. Um número fixo aqui é o que dá ao som da tela um
                // ganho próprio no dia em que alguém quiser um.
                if let Some(decodificador) = som_da_tela.as_mut() {
                    let pacote = controls
                        .som_da_tela
                        .lock()
                        .unwrap_or_else(|envenenado| envenenado.into_inner())
                        .pop_front();
                    if let Some(pacote) = pacote {
                        if let Ok(amostras) = decodificador.decode(&pacote) {
                            // **A linha espelho da do outro lado.**
                            //
                            // Quem transmite já diz «o som da tela começou a
                            // sair para o fio», e essa linha provou que o envio
                            // funciona. Quem recebe não dizia nada, e sem ela
                            // «não sai som» é indistinguível entre três coisas:
                            // não saiu, não chegou, ou chegou e não tocou.
                            //
                            // Uma vez por sessão, como as irmãs: a condição,
                            // quando é verdade, é verdade cinquenta vezes por
                            // segundo.
                            if !som_da_tela_avisado {
                                som_da_tela_avisado = true;
                                tracing::info!(
                                    amostras = amostras.len(),
                                    "o som da tela alheia começou a ser tocado aqui"
                                );
                            }
                            decoded.push((SSRC_DA_TELA, amostras));
                        }
                    }
                }

                let borrowed: Vec<(u32, &[f32])> = decoded
                    .iter()
                    .map(|(talker, samples)| (*talker, samples.as_slice()))
                    .collect();
                mixer.mix(&borrowed, &mut mixed);

                for_device.clear();
                if to_device.push(&mixed, &mut for_device).is_ok() {
                    for sample in for_device.drain(..) {
                        if io.to_device.push(sample).is_err() {
                            // Terceiro lugar onde áudio se perdia dentro desta
                            // máquina sem deixar rastro. Contado por amostra e
                            // não registrado em log, pela mesma razão que os
                            // quadros recusados: acontece aos milhares.
                            controls.anel_cheio.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }

        // Uma linha, uma vez por sessão, e ela responde a pergunta em vez de
        // sugerir: se a volta deste laço passa de um quadro, o áudio que sai
        // desta máquina pica **por causa disto**, e nada no áudio recebido
        // explicaria o buraco. Uma vez porque a condição, quando é verdade, é
        // verdade cinquenta vezes por segundo.
        if !atraso_avisado {
            let medido = playout.metrics();
            if medido.worst_lateness_ms > f64::from(FRAME_MS) {
                atraso_avisado = true;
                tracing::warn!(
                    volta_ms = medido.worst_lateness_ms,
                    quadro_ms = FRAME_MS,
                    reposicoes = medido.catchup_frames,
                    "o laço de voz demorou mais que um quadro entre duas conferidas; \
                     a reprodução está sendo mantida em dia por reposição"
                );
            }
        }

        // ---- telemetry ----
        if Instant::now() >= next_telemetry {
            next_telemetry += TELEMETRY_EVERY;
            let snapshot = AudioTelemetry {
                local: LocalTelemetry::assemble(
                    io.counters.snapshot(),
                    gate.metrics(),
                    mixer.metrics(),
                    encoder.bitrate_bps(),
                    controls.speaking.load(Ordering::Relaxed),
                )
                .with_playout(playout.metrics())
                .with_pacing(ritmo.metrics()),
                sources: sources
                    .iter()
                    .map(|source| SourceTelemetry::assemble(source.ssrc, source.buffer.metrics()))
                    .collect(),
            };
            if let Ok(mut slot) = telemetry.lock() {
                *slot = snapshot;
            }
        }

        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

#[cfg(test)]
mod relogio_de_midia {
    use super::Voice;
    use seele_audio::jitter::{JitterBuffer, JitterConfig};

    /// O defeito, escrito como teste antes do conserto.
    ///
    /// Trocar de microfone abria um caminho novo sobre o mesmo `ssrc`, e o
    /// caminho novo contava do zero. Este teste mostra o que acontecia do outro
    /// lado — e ele não falha por causa do buffer, que está certo: um carimbo
    /// anterior ao que já tocou **é** atrasado, e descartá-lo é o trabalho dele.
    /// O erro estava em produzir esse carimbo.
    #[test]
    fn um_relogio_que_recomeca_do_zero_e_descartado_inteiro() {
        let mut buffer = JitterBuffer::new(JitterConfig::default());
        // Uma conversa que já dura cinco minutos.
        let inicio = 48_000 * 300_u32;
        for quadro in 0..40_u32 {
            buffer.push(
                quadro as u16,
                inicio + quadro * 960,
                f64::from(quadro) * 20.0,
                vec![1_u8],
            );
        }
        // Toca o suficiente para o buffer sair do enchimento e ter um «próximo».
        for _ in 0..20 {
            let _ = buffer.tick();
        }

        // Agora a pessoa troca de microfone, e o caminho novo começa em zero.
        let antes = buffer.metrics().late_discards;
        for quadro in 0..40_u32 {
            buffer.push(quadro as u16, quadro * 960, 900.0, vec![2_u8]);
        }
        assert!(
            buffer.metrics().late_discards - antes >= 40,
            "o buffer aceitou carimbos anteriores ao que já tocou; se isto mudar,              o teste de baixo deixa de provar o que prova"
        );
    }

    #[test]
    fn o_salto_passa_na_frente_do_caminho_que_ainda_esta_no_ar() {
        // O caminho velho não morre no instante em que o novo abre — ele é
        // largado **depois**, de propósito, para que um microfone que sumiu
        // deixe a pessoa falando pelo antigo em vez de muda. Enquanto os dois
        // vivem, o velho continua carimbando para a frente.
        let (_, carimbo) = Voice::salto_do_relogio(700, 48_000 * 300);
        // Meio segundo de caminho velho ainda saindo: 25 quadros de 20 ms.
        let ultimo_do_velho = 48_000 * 300 + 25 * 960;
        assert!(
            seele_audio::jitter::ts_delta(carimbo, ultimo_do_velho) > 0,
            "o caminho novo abriu atrás do velho, e quem escuta descartaria              o novo exatamente como descartava antes"
        );
    }

    #[test]
    fn a_sequencia_anda_um_e_nao_mil() {
        // Ela conta o que sai, e quem recebe usa a distância entre ela e o
        // carimbo para separar silêncio de perda (M1.9). Um pulo grande aqui
        // seria lido como meio segundo de pacotes perdidos que nunca existiram.
        let (seq, _) = Voice::salto_do_relogio(700, 0);
        assert_eq!(seq, 701);
        // E dá a volta sem estourar, que é o caso de uma conversa longa.
        let (volta, _) = Voice::salto_do_relogio(u16::MAX, 0);
        assert_eq!(volta, 0);
    }
}

#[cfg(test)]
mod tests {

    /// O laço de áudio conduz o acompanhamento do aparelho a cada volta.
    ///
    /// Sem esta chamada, todo o ciclo de reabertura continua existindo, continua
    /// correto e continua provado pelos testes de conformidade — e nunca corre.
    /// O defeito de origem volta inteiro: trocar de fone ou de microfone pelo
    /// sistema operacional só vale depois de reiniciar o aplicativo. Foi medido:
    /// apagando o bloco de `pipeline` que chama `passo`, a suíte inteira
    /// continua verde.
    ///
    /// Ler o fonte porque não há tipo que expresse «este laço chama aquela
    /// função». É o mesmo recurso do guarda de ordem logo acima, pela mesma
    /// razão: o teste de conformidade conduz [`seguir_o_aparelho`], que é a
    /// decisão inteira, mas nem ele nem tipo nenhum alcança a **chamada** dela
    /// aqui dentro — e sem a chamada a decisão inteira não roda para ninguém.
    ///
    /// A âncora é o **laço**, e não o nome da função que o contém: renomear
    /// `pipeline` ou mudar a forma como ela fecha não faz este guarda entrar em
    /// pânico com uma mensagem de que ninguém entende a causa. O que ele exige é
    /// que a chamada esteja depois da volta que roda enquanto a chamada dura, e
    /// antes do primeiro `#[cfg(test)]` — quer dizer, no programa e não num
    /// teste. Se um dia o laço deixar de ser escrito assim, o guarda diz isso
    /// com todas as letras, em vez de morrer num `expect` cru.
    #[test]
    fn o_laco_de_audio_conduz_o_acompanhamento_do_aparelho() {
        let fonte = include_str!("voice.rs");
        // Só o que vira programa: daqui em diante é bateria de testes, e uma
        // chamada aqui de dentro não conduziria aparelho nenhum de ninguém.
        let programa = match fonte.find("\n#[cfg(test)]") {
            Some(corte) => &fonte[..corte],
            None => fonte,
        };
        let Some(laco) = programa.find("while !controls.stop") else {
            panic!(
                "o laço de áudio deixou de ser uma volta sobre `controls.stop`, \
                 e este guarda não sabe mais onde procurar.\n\
                 Ele não está dizendo que o programa quebrou: está dizendo que \
                 precisa ser reapontado para a forma nova do laço, senão a troca \
                 de aparelho fica sem quem a verifique."
            )
        };
        let depois_do_laco = &programa[laco..];
        assert!(
            depois_do_laco.contains("seguir_o_aparelho("),
            "o laço de áudio deixou de conduzir o passo do aparelho.\n\
             O supervisor volta a ser código morto e a troca de microfone ou de \
             fone feita no sistema operacional volta a só valer depois de \
             reiniciar o aplicativo."
        );
    }

    /// A ordem do caminho do microfone: **supressão → portão → ganho → codec**.
    ///
    /// A ordem é a coisa toda, e ela some numa refatoração sem que nada quebre.
    /// Cada fronteira tem um sintoma próprio:
    ///
    /// - **ganho antes do portão** faz ruído de sala passar do limiar e virar
    ///   fala. O §3 paga caro por um portão que abre à toa — cada abertura é banda
    ///   gasta e é a voz de alguém sendo cortada para dar lugar a um ventilador —,
    ///   e o sintoma seria «o SEELE está transmitindo o meu ar-condicionado», que
    ///   ninguém liga a um ganho posto no lugar errado;
    /// - **supressão depois do portão** desfaz F01 inteiro. A sensibilidade de
    ///   −60 dBFS só é defensável com o piso da sala já removido; com a supressão
    ///   depois, o portão continua medindo a sala e o padrão novo mantém o canal
    ///   aberto o dia inteiro — pior que antes de F01.
    ///
    /// Ler o fonte porque não há tipo que expresse «esta linha vem depois
    /// daquela». É o mesmo recurso da costura do codec, pela mesma razão.
    #[test]
    fn o_caminho_do_microfone_esta_na_ordem_de_f01_e_f02() {
        let fonte = include_str!("voice.rs");
        let supressao = fonte
            .find("supressao.processar(&bruto, &mut limpo)")
            .expect("a supressão de ruído sumiu do laço de captura");
        let portao = fonte
            .find("gate.quadros_a_transmitir(&frame, &mut a_transmitir)")
            .expect("o portão de voz sumiu do laço de captura");
        let ganho = fonte
            .find("ganho.aplicar(&mut frame)")
            .expect("o ganho do microfone sumiu do laço de captura");
        let codec = fonte
            .find("encoder.encode(&frame)")
            .expect("a codificação sumiu do laço de captura");

        assert!(
            supressao < portao,
            "a supressão passou a correr depois do portão.\n\
             Nessa ordem o portão volta a medir a sala, e a sensibilidade de \
             −60 dBFS de F01 mantém o canal aberto o dia inteiro."
        );
        assert!(
            portao < ganho,
            "o ganho passou a correr antes do portão de voz.\n\
             Nessa ordem o portão mede o sinal já amplificado, ruído de sala \
             passa a abrir a transmissão, e a banda vai embora em ventilador."
        );
        assert!(
            ganho < codec,
            "o ganho passou a correr depois da codificação, onde ele não alcança \
             mais o que sai."
        );
    }

    /// **Os quadros retidos não chegam todos no mesmo instante.** F01, A05.
    ///
    /// Um carimbo é *quando aquele áudio aconteceu*. Os quadros que a retenção da
    /// primeira sílaba entrega aconteceram antes do quadro que abriu o portão, e
    /// carimbá-los todos igual põe três quadros no mesmo instante — o `playout` de
    /// quem recebe lê isso como salto de relógio e ressincroniza, no primeiro
    /// quadro da fala, que é onde mais se ouve.
    #[test]
    fn os_quadros_retidos_recuam_um_quadro_cada_um() {
        let agora = 96_000;
        // Uma entrega de três: dois retidos e o que abriu.
        let carimbos: Vec<u32> = (0..3).map(|i| carimbo_do_quadro(agora, 3, i)).collect();
        assert_eq!(
            carimbos,
            vec![
                agora - 2 * FRAME_SAMPLES as u32,
                agora - FRAME_SAMPLES as u32,
                agora
            ],
            "os quadros retidos não estão recuando: carimbos iguais viram \
             ressincronização no começo de cada fala"
        );
        // O caso comum — um quadro só — continua carimbado com o instante de agora.
        assert_eq!(carimbo_do_quadro(agora, 1, 0), agora);
        // E a volta do contador de 32 bits não é um erro: ela acontece a cada 24
        // horas de conversa, e no meio de uma abertura como em qualquer outro lugar.
        assert_eq!(
            carimbo_do_quadro(10, 2, 0),
            10u32.wrapping_sub(FRAME_SAMPLES as u32),
            "a volta do contador virou um carimbo saturado, que é um quadro \
             no futuro"
        );
    }

    /// **E o relógio de quem recebe aceita esses carimbos sem ressincronizar.**
    ///
    /// A conta certa é uma coisa; o efeito dela no outro lado é outra, e é o efeito
    /// que a auditoria mediu — «o jitter buffer real reproduziu os pacotes, porém
    /// precisou de **duas ressincronizações**». Este teste passa os três carimbos
    /// pelo buffer de verdade e exige zero.
    ///
    /// O segundo caso é o controle negativo, e ele é o defeito: com carimbos iguais
    /// o `plan_gap` do buffer cai no ramo «total == 0» e ressincroniza. Sem ele, um
    /// dia em que `carimbo_do_quadro` voltasse a devolver o mesmo número para todos
    /// teria um teste verde por baixo.
    #[test]
    fn o_relogio_de_quem_recebe_aceita_os_quadros_retidos() {
        fn atravessa(carimbos: &[u32]) -> (usize, u64) {
            use seele_audio::jitter::{Decision, JitterBuffer, JitterConfig};
            let mut buffer: JitterBuffer<usize> = JitterBuffer::new(JitterConfig::default());
            for (indice, carimbo) in carimbos.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation, reason = "três pacotes num teste")]
                let seq = indice as u16 + 1;
                #[allow(clippy::cast_precision_loss, reason = "três pacotes num teste")]
                let chegada = indice as f64 * 20.0;
                buffer.push(seq, *carimbo, chegada, indice);
            }
            let mut tocados = 0;
            for _ in 0..(carimbos.len() * 4) {
                if matches!(buffer.tick(), Decision::Play(_)) {
                    tocados += 1;
                }
            }
            (tocados, buffer.metrics().resyncs)
        }

        let agora = 96_000;
        let quadro = FRAME_SAMPLES as u32;
        let (tocados, ressincronizacoes) = atravessa(&[
            carimbo_do_quadro(agora, 3, 0),
            carimbo_do_quadro(agora, 3, 1),
            carimbo_do_quadro(agora, 3, 2),
        ]);
        assert_eq!(
            ressincronizacoes, 0,
            "o relógio de quem recebe ressincronizou na abertura de uma fala, que é \
             onde ela mais se ouve"
        );
        assert_eq!(tocados, 3, "os três quadros da abertura não tocaram");

        // O controle negativo: é o que a auditoria mediu.
        let (_, com_carimbos_iguais) = atravessa(&[agora, agora, agora]);
        assert!(
            com_carimbos_iguais > 0,
            "carimbos iguais passaram pelo buffer sem ressincronizar: então este \
             teste não está medindo o defeito que ele diz medir"
        );
        // E os carimbos de verdade são espaçados por um quadro de amostras.
        assert_eq!(
            carimbo_do_quadro(agora, 3, 2) - carimbo_do_quadro(agora, 3, 1),
            quadro
        );
    }

    /// **E o cabeçalho usa o carimbo do quadro, e não o do laço.**
    ///
    /// A conta certa numa função que ninguém chama é o «existir não é funcionar» do
    /// `CLAUDE.md`: o teste acima passa com `carimbo_do_quadro` guardada na gaveta.
    /// Ler o fonte é o que prende as duas pontas, e é o mesmo recurso da ordem do
    /// caminho, pela mesma razão — não há tipo que expresse «este campo vem
    /// daquela função».
    #[test]
    fn o_cabecalho_carimba_cada_quadro_com_o_instante_dele() {
        let fonte = include_str!("voice.rs");
        assert!(
            fonte.contains("let carimbo = carimbo_do_quadro(timestamp, quantos, indice);"),
            "o laço de captura deixou de calcular o carimbo por quadro"
        );
        assert!(
            fonte.contains("timestamp: carimbo,"),
            "o cabeçalho voltou a carimbar todo quadro com o instante do laço: os \
             retidos da primeira sílaba passam a chegar no mesmo tempo do quadro \
             que abriu"
        );
    }

    /// **A régua do portão sai dos controles, e o padrão depende da supressão.**
    ///
    /// F01, e é a decisão inteira dela: −60 dBFS é do caminho **com** supressão.
    /// Sem ela o limiar antigo continua valendo, porque ali o portão é a única
    /// defesa contra o ventilador — o argumento que `gate::OPEN_RMS` registra.
    #[test]
    fn o_padrao_da_sensibilidade_segue_a_supressao() {
        let controls = Controls::novos();
        // Ligada é o padrão — ver `SUPRESSAO_PADRAO`.
        let com = configuracao_do_portao(&controls).abertura_dbfs();
        assert!(
            (com - seele_audio::gate::ABERTURA_COM_SUPRESSAO_DBFS).abs() < 0.01,
            "com supressão o padrão deixou de ser o alvo de F01: {com} dBFS"
        );

        controls.supressao.store(0, Ordering::Relaxed);
        let sem = configuracao_do_portao(&controls).abertura_dbfs();
        assert!(
            (sem - seele_audio::gate::ABERTURA_SEM_SUPRESSAO_DBFS).abs() < 0.01,
            "sem supressão o padrão desceu junto, e o portão volta a ser a única \
             defesa contra o ventilador com a régua de quem tem filtro: {sem} dBFS"
        );

        // E uma escolha explícita vence os dois padrões.
        controls
            .abertura_dbfs_milesimos
            .store(-50_000, Ordering::Relaxed);
        let escolhido = configuracao_do_portao(&controls).abertura_dbfs();
        assert!(
            (escolhido - (-50.0)).abs() < 0.01,
            "a escolha manual foi ignorada: {escolhido} dBFS"
        );
    }

    /// **A fila do som da tela só é limpa pela tela dela.** R17.
    ///
    /// Qualquer `TelaFechou` a limpava, então com duas pessoas compartilhando
    /// alguém parando de transmitir calava o som da outra — e o sintoma é o pior
    /// desta casa: o som simplesmente para, sem nada dizendo por quê.
    #[test]
    fn o_som_de_uma_tela_nao_e_limpo_por_outra() {
        let controls = Controls::novos();
        let quantos = || controls.som_da_tela.lock().expect("fila").len();

        enfileirar_som_da_tela(&controls, ScreenId(7), vec![1, 2, 3]);
        assert_eq!(quantos(), 1);

        // **Outra transmissão fechou.** A fila não é dela, e não é tocada.
        assert!(
            !esquecer_o_som_de(&controls, ScreenId(8)),
            "esquecer respondeu que havia o que esquecer numa tela que não é a da fila"
        );
        assert_eq!(
            quantos(),
            1,
            "a fila foi limpa por uma transmissão que não é a dela: com duas \
             pessoas compartilhando, uma parando calaria o som da outra"
        );

        // E a dela, sim.
        assert!(esquecer_o_som_de(&controls, ScreenId(7)));
        assert_eq!(quantos(), 0);
    }

    /// **Trocar de transmissão limpa a fila da anterior.** R17.
    ///
    /// Os pacotes que sobraram são som de uma imagem que já saiu da frente, e
    /// tocá-los faria a troca soar como sobreposição.
    #[test]
    fn trocar_de_tela_larga_o_som_da_anterior() {
        let controls = Controls::novos();
        for _ in 0..5 {
            enfileirar_som_da_tela(&controls, ScreenId(7), vec![1]);
        }
        assert_eq!(controls.som_da_tela.lock().expect("fila").len(), 5);

        enfileirar_som_da_tela(&controls, ScreenId(8), vec![2]);
        assert_eq!(
            controls.som_da_tela.lock().expect("fila").len(),
            1,
            "o som da transmissão anterior continuou na fila, e vai tocar por cima \
             da nova"
        );
        assert_eq!(controls.tela_do_som.load(Ordering::Relaxed), 8);
    }

    /// **Volume e mudo são separados.** R17.
    ///
    /// Voltar do mudo devolve o volume que a pessoa escolheu. Um mudo que zerasse
    /// o volume apagaria a escolha, e a pessoa teria de reajustar a cada vez.
    #[test]
    fn o_mudo_da_tela_nao_apaga_o_volume() {
        let controls = Controls::novos();
        assert_eq!(
            controls.volume_da_tela.load(Ordering::Relaxed),
            VOLUME_DE_ORIGEM,
            "quem começa a assistir tem de ouvir: um padrão calado entrega vídeo \
             mudo e espera que a pessoa descubra onde o som estava"
        );

        controls.volume_da_tela.store(300, Ordering::Relaxed);
        controls.tela_calada.store(true, Ordering::Relaxed);
        controls.tela_calada.store(false, Ordering::Relaxed);
        assert_eq!(
            controls.volume_da_tela.load(Ordering::Relaxed),
            300,
            "calar e voltar apagou o volume escolhido"
        );
    }

    use super::*;

    #[test]
    fn the_default_is_push_to_talk() {
        // specs/03-audio.md picks it because it never false-triggers, and a
        // client that broadcasts a room by accident is the worse failure.
        assert_eq!(VoiceMode::from_byte(0), VoiceMode::PushToTalk);
    }

    #[test]
    fn modes_survive_the_round_trip_through_an_atomic() {
        for mode in [
            VoiceMode::PushToTalk,
            VoiceMode::VoiceActivated,
            VoiceMode::Open,
        ] {
            assert_eq!(VoiceMode::from_byte(mode.as_byte()), mode);
        }
    }

    #[test]
    fn an_unknown_byte_falls_back_to_the_safe_mode() {
        // Whatever goes wrong, it must not end with an open microphone.
        assert_eq!(VoiceMode::from_byte(200), VoiceMode::PushToTalk);
    }

    /// A choice with a preference on each side.
    fn both_chosen() -> DeviceChoice {
        DeviceChoice {
            capture: Some("o microfone".to_owned()),
            playback: Some("a caixa".to_owned()),
        }
    }

    #[test]
    fn giving_up_one_side_leaves_the_other_alone() {
        // The rule the fallback is made of. A headset left in another room must
        // not cost somebody the microphone they picked: they would arrive at a
        // server with two things wrong and nothing saying the second was a
        // consequence of the first.
        let sem_saida = both_chosen()
            .without(device::Side::Output)
            .expect("there was an output preference to give up");
        assert_eq!(sem_saida.capture.as_deref(), Some("o microfone"));
        assert_eq!(sem_saida.playback, None);

        let sem_microfone = both_chosen()
            .without(device::Side::Input)
            .expect("there was a capture preference to give up");
        assert_eq!(sem_microfone.capture, None);
        assert_eq!(sem_microfone.playback.as_deref(), Some("a caixa"));
    }

    #[test]
    fn a_side_already_on_the_default_has_nothing_left_to_give_up() {
        // What ends the ladder in `open_preferring`. Without it, a machine with
        // no sound card at all would be asked the same question forever instead
        // of being told it has no sound card — a hang where there should be a
        // sentence, and on the path that opens every session.
        assert_eq!(DeviceChoice::default().without(device::Side::Input), None);
        assert_eq!(DeviceChoice::default().without(device::Side::Output), None);
    }

    #[test]
    fn the_ask_crosses_into_the_audio_layer_on_the_side_it_was_made() {
        // Both halves are `Option<String>`, so nothing but this catches a swap
        // — and a swap is silent until somebody wonders why picking a headset
        // muted their microphone.
        let chosen = both_chosen();
        let wanted = chosen.wanted();
        assert_eq!(wanted.capture, Some("o microfone"));
        assert_eq!(wanted.playback, Some("a caixa"));
    }
}

#[cfg(test)]
mod faixa_de_bitrate {
    use seele_audio::bitrate::{Controlador, FAIXAS_BPS};
    use seele_audio::codec::{DEFAULT_BITRATE_BPS, MAX_BITRATE_BPS, MIN_BITRATE_BPS};

    /// O padrão do codec é a faixa de cima do controlador.
    ///
    /// Os dois números moram em módulos diferentes e nada no tipo os obriga a
    /// concordar. Se divergirem, a primeira medida de rede boa «subiria» para um
    /// valor que já estava em vigor, e a primeira de rede ruim desceria a partir
    /// de outro lugar — o encoder e a malha discordando sobre onde a conversa
    /// começou, sem nada na tela que contradissesse.
    #[test]
    fn o_padrao_do_codec_e_a_faixa_de_cima() {
        assert_eq!(DEFAULT_BITRATE_BPS, FAIXAS_BPS[0]);
    }

    /// Nenhuma faixa cai fora do que o codec aceita.
    ///
    /// `VoiceEncoder::new` satura no intervalo permitido, calado. Uma faixa fora
    /// dele viraria outro bitrate na hora de codificar, e a malha passaria a
    /// mandar num número enquanto o encoder roda noutro — a pior forma de errar,
    /// porque a telemetria mostraria o valor certo.
    #[test]
    fn toda_faixa_cabe_no_que_o_codec_aceita() {
        for faixa in FAIXAS_BPS {
            assert!(
                (MIN_BITRATE_BPS..=MAX_BITRATE_BPS).contains(&faixa),
                "a faixa {faixa} está fora de {MIN_BITRATE_BPS}..={MAX_BITRATE_BPS}, \
                 e o encoder a trocaria em silêncio"
            );
        }
    }

    /// As faixas descem, e o piso é o da spec.
    #[test]
    fn as_faixas_vao_do_teto_ao_piso_da_spec() {
        assert_eq!(FAIXAS_BPS[0], MAX_BITRATE_BPS);
        assert_eq!(
            FAIXAS_BPS[FAIXAS_BPS.len() - 1],
            MIN_BITRATE_BPS,
            "a última faixa não é o piso que `specs/03-audio.md` declara"
        );
        for par in FAIXAS_BPS.windows(2) {
            assert!(
                par[0] > par[1],
                "as faixas não estão em ordem decrescente: {par:?}"
            );
        }
    }

    /// Um controlador recém-criado já concorda com o `Controls` recém-criado.
    #[test]
    fn o_controlador_nasce_no_mesmo_valor_que_o_encoder() {
        assert_eq!(Controlador::novo().bitrate_bps(), DEFAULT_BITRATE_BPS);
    }
}

#[cfg(test)]
mod controles_na_reabertura {
    //! O que **não** pode se perder quando a voz reabre num aparelho novo.
    //!
    //! Reabrir acontece de três jeitos — trocar de microfone, trocar de saída,
    //! e a reabertura automática quando o sistema troca o aparelho por baixo da
    //! sessão — e os três passam por `carregar_controles`. O guarda que existia
    //! para isto lia o texto-fonte da casca à procura da palavra `reopen`: ele
    //! prova que alguém a escreveu, e **nada** sobre o que sobrevive. Um
    //! `carregar_controles` que esquecesse o mudo passaria por ele inteiro.
    //!
    //! O item que machuca é o mudo: uma reabertura que o desliga sozinha põe no
    //! ar uma sala que estava calada, sem ninguém ter pedido.

    use super::{carregar_controles, Controls, VoiceMode};
    use std::sync::atomic::Ordering;

    #[test]
    fn os_controles_atravessam_a_reabertura() {
        let velho = Controls::novos();
        velho.muted.store(true, Ordering::Relaxed);
        velho.total_isolation.store(true, Ordering::Relaxed);
        velho.key_held.store(true, Ordering::Relaxed);
        velho
            .mode
            .store(VoiceMode::VoiceActivated.as_byte(), Ordering::Relaxed);
        velho.gains.lock().expect("ganhos").insert(7, 0.25);

        let novo = Controls::novos();
        carregar_controles(&velho, &novo);

        assert!(
            novo.muted.load(Ordering::Relaxed),
            "a reabertura desligou o mudo sozinha, que é pôr uma sala no ar"
        );
        assert!(
            novo.total_isolation.load(Ordering::Relaxed),
            "quem estava em Isolamento total voltou ouvindo todo mundo"
        );
        assert!(novo.key_held.load(Ordering::Relaxed));
        assert_eq!(
            novo.mode.load(Ordering::Relaxed),
            VoiceMode::VoiceActivated.as_byte(),
            "o modo do microfone voltou ao padrão"
        );
        assert_eq!(
            novo.gains.lock().expect("ganhos").get(&7).copied(),
            Some(0.25),
            "o volume que alguém ajustou para uma pessoa voltou ao padrão"
        );
    }

    #[test]
    fn o_relogio_de_midia_pula_para_a_frente_em_vez_de_recomecar() {
        // O defeito que calava a pessoa do outro lado: um caminho novo sobre o
        // mesmo `ssrc` contando do zero é descartado inteiro por quem recebe.
        let velho = Controls::novos();
        velho.relogio_seq.store(4_000, Ordering::Relaxed);
        velho.relogio_carimbo.store(1_000_000, Ordering::Relaxed);

        let novo = Controls::novos();
        carregar_controles(&velho, &novo);

        assert_eq!(novo.relogio_seq.load(Ordering::Relaxed), 4_001);
        assert_eq!(
            novo.relogio_carimbo.load(Ordering::Relaxed),
            1_000_000 + seele_audio::SAMPLE_RATE_HZ,
            "sem a folga de um segundo, os quadros do caminho novo chegam \
             atrás dos do velho, que ainda está no ar"
        );
    }
}

/// O dimensionamento do aparelho recém-aberto, que é a outra metade da troca.
///
/// Reabrir no aparelho certo e seguir tocando com as medidas do anterior é
/// trocar um defeito por outro: o som sai, mas rápido ou devagar para sempre.
/// Estes guardas provam que as medidas seguem o aparelho de agora, e que o que
/// estava a caminho no aparelho de antes não é despejado no novo.
#[cfg(test)]
mod dimensoes_do_aparelho {
    use super::{
        dimensoes_ou_dizer_que_nao_ha, esvaziar_o_que_era_do_antigo, recomecar_no_aparelho_novo,
        Dimensoes, EstadoDoAparelho, EstadoDoAudio, Instant, Mutex, PlayoutClock,
    };
    use seele_audio::{FRAME_MS, SAMPLE_RATE_HZ};
    use std::time::Duration;

    /// Um painel de sessão que a interface leria como normalidade.
    fn painel_de_quem_estava_ouvindo() -> Mutex<EstadoDoAudio> {
        Mutex::new(EstadoDoAudio {
            estado: EstadoDoAparelho::Funcionando,
            ..EstadoDoAudio::default()
        })
    }

    #[test]
    fn as_medidas_seguem_o_aparelho_de_agora_e_nao_o_de_antes() {
        // Um fone de 48 kHz com anel de 20 ms, e depois um de 44,1 kHz com anel
        // maior — o caso comum de trocar fone de USB por saída embutida.
        let antes = Dimensoes::do_aparelho(48_000, 48_000, 960)
            .expect("48 kHz dos dois lados é o caso mais simples que existe");
        let depois = Dimensoes::do_aparelho(44_100, 44_100, 4_410)
            .expect("44,1 kHz é taxa de aparelho de verdade, não de exceção");

        assert_eq!(depois.para_o_laco.from_hz(), 44_100);
        assert_eq!(depois.para_o_laco.to_hz(), SAMPLE_RATE_HZ);
        assert_eq!(depois.para_o_aparelho.from_hz(), SAMPLE_RATE_HZ);
        assert_eq!(
            depois.para_o_aparelho.to_hz(),
            44_100,
            "o reamostrador de saída ficou na taxa do aparelho anterior; a voz \
             sai acelerada enquanto a sessão durar"
        );
        assert_eq!(depois.anel, 4_410);
        assert_ne!(
            depois.ritmo.target_samples(),
            antes.ritmo.target_samples(),
            "a malha do anel ficou com a medida do aparelho anterior; ela vai \
             mirar uma profundidade que este anel não tem"
        );
    }

    #[test]
    fn uma_taxa_que_o_reamostrador_recusa_nao_vira_laco() {
        // Zero não é aparelho: é o que sobra quando a descrição do dispositivo
        // vem vazia. Melhor não abrir laço nenhum do que dividir por ela.
        assert!(Dimensoes::do_aparelho(0, 48_000, 960).is_none());
        assert!(Dimensoes::do_aparelho(48_000, 0, 960).is_none());
    }

    #[test]
    fn o_aparelho_cuja_taxa_e_recusada_nao_fica_na_tela_como_funcionando() {
        let painel = painel_de_quem_estava_ouvindo();

        let dimensoes = dimensoes_ou_dizer_que_nao_ha(&painel, 0, 48_000, 960);

        assert!(dimensoes.is_none(), "não há laço possível com taxa zero");
        assert_eq!(
            painel.lock().expect("painel só é travado aqui").estado,
            EstadoDoAparelho::Perdido,
            "o laço vai encerrar e a tela continua dizendo que está tudo              funcionando; a pessoa fica sem som nenhum sem nada que explique"
        );
    }

    #[test]
    fn o_aparelho_que_o_laco_aceita_nao_e_anunciado_como_perdido() {
        let painel = painel_de_quem_estava_ouvindo();

        let dimensoes = dimensoes_ou_dizer_que_nao_ha(&painel, 44_100, 48_000, 960);

        assert!(dimensoes.is_some(), "44,1 kHz de entrada é aparelho comum");
        assert_eq!(
            painel.lock().expect("painel só é travado aqui").estado,
            EstadoDoAparelho::Funcionando,
            "um aparelho que abriu e serve foi anunciado como perdido"
        );
    }

    #[test]
    fn o_que_estava_a_caminho_do_aparelho_antigo_nao_toca_no_novo() {
        let (mut pendentes, mut capturadas) = (vec![0.1_f32; 480], vec![0.2_f32; 240]);
        let (mut em_48k, mut para_o_aparelho) = (vec![0.3_f32; 960], vec![0.4_f32; 120]);

        esvaziar_o_que_era_do_antigo([
            &mut pendentes,
            &mut capturadas,
            &mut em_48k,
            &mut para_o_aparelho,
        ]);

        for resto in [&pendentes, &capturadas, &em_48k, &para_o_aparelho] {
            assert!(
                resto.is_empty(),
                "restou amostra da taxa do aparelho anterior para tocar no novo; \
                 é um estalo no primeiro instante do fone que a pessoa acabou de pôr"
            );
        }
    }

    /// A volta inteira em que o aparelho mudou, e não cada peça dela.
    ///
    /// Redimensionar, esvaziar e reacertar o relógio têm guarda cada um acima.
    /// O que este prova é a **ordem entre os três** numa só troca: é ela que a
    /// pessoa ouve, e era ela que só existia dentro do laço, onde nenhum teste
    /// chegava.
    #[test]
    fn a_troca_redimensiona_esvazia_e_reacerta_o_relogio_na_mesma_volta() {
        let painel = painel_de_quem_estava_ouvindo();
        let (mut pendentes, mut capturadas) = (vec![0.1_f32; 480], vec![0.2_f32; 240]);
        let (mut em_48k, mut para_o_aparelho) = (vec![0.3_f32; 960], vec![0.4_f32; 120]);

        // O relógio vinha em compasso e a reabertura custou meio segundo, que é
        // o preço normal de abrir um endpoint.
        let partida = Instant::now();
        let mut relogio = PlayoutClock::new(partida, FRAME_MS);
        relogio.due(partida);
        let depois_da_troca = partida + Duration::from_millis(520);

        let novas = recomecar_no_aparelho_novo(
            &painel,
            44_100,
            44_100,
            4_410,
            [
                &mut pendentes,
                &mut capturadas,
                &mut em_48k,
                &mut para_o_aparelho,
            ],
            &mut relogio,
            depois_da_troca,
        )
        .expect("44,1 kHz dos dois lados é aparelho de verdade");

        assert_eq!(
            novas.para_o_aparelho.to_hz(),
            44_100,
            "as medidas ficaram nas do aparelho anterior; a voz sai acelerada \
             enquanto a sessão durar"
        );
        assert_eq!(novas.anel, 4_410);
        for resto in [&pendentes, &capturadas, &em_48k, &para_o_aparelho] {
            assert!(
                resto.is_empty(),
                "restou amostra da taxa de antes para tocar no aparelho novo"
            );
        }
        assert_eq!(
            relogio.due(depois_da_troca),
            1,
            "o compasso recomeçou num despejo de reposição em vez de um quadro"
        );
        assert_eq!(
            relogio.metrics().resyncs,
            0,
            "a pausa de reabrir o aparelho foi contada como máquina travada"
        );
        assert_eq!(
            painel.lock().expect("painel só é travado aqui").estado,
            EstadoDoAparelho::Funcionando,
            "o aparelho novo abriu e serve, e a tela o anuncia como perdido"
        );
    }

    #[test]
    fn o_aparelho_novo_que_o_reamostrador_recusa_encerra_a_volta_sem_mexer_em_nada() {
        // Taxa zero é o que sobra quando a descrição do dispositivo vem vazia.
        // Aqui o laço acaba nesta volta: não há o que esvaziar nem relógio a
        // reacertar, e o que **não** pode faltar é a tela dizer a verdade.
        let painel = painel_de_quem_estava_ouvindo();
        let (mut pendentes, mut capturadas) = (vec![0.1_f32; 480], vec![0.2_f32; 240]);
        let (mut em_48k, mut para_o_aparelho) = (vec![0.3_f32; 960], vec![0.4_f32; 120]);

        let partida = Instant::now();
        let mut relogio = PlayoutClock::new(partida, FRAME_MS);

        let novas = recomecar_no_aparelho_novo(
            &painel,
            0,
            48_000,
            960,
            [
                &mut pendentes,
                &mut capturadas,
                &mut em_48k,
                &mut para_o_aparelho,
            ],
            &mut relogio,
            partida,
        );

        assert!(novas.is_none(), "não há laço possível com taxa zero");
        assert_eq!(
            painel.lock().expect("painel só é travado aqui").estado,
            EstadoDoAparelho::Perdido,
            "o laço vai encerrar e a tela continua dizendo que está tudo \
             funcionando; a pessoa fica sem som nenhum sem nada que explique"
        );
    }
}
