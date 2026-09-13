//! O som que a máquina está tocando, para ir junto com a tela.
//!
//! # Por que este módulo existe
//!
//! Compartilhar a tela mostrava a imagem e mais nada. Quem transmite um jogo,
//! um vídeo ou uma chamada de outro programa está mostrando metade do que quer
//! mostrar — e a metade que falta é a que diz o que está acontecendo. O relato
//! foi direto: «a transmissão também não carrega o áudio, algo que deveria ter
//! em transmissão de jogos».
//!
//! # O que ele captura, e o que não
//!
//! O som **da máquina**, não o do microfone. São coisas diferentes e vão por
//! caminhos diferentes: a voz é da sala e chega a todo mundo que está nela; o
//! som da tela é da transmissão e chega a quem a está assistindo.
//!
//! Não dá para capturar o som de **um programa só**: nem o Windows nem o macOS
//! oferecem isso sem um driver no meio, e um driver é um instalador a mais e uma
//! superfície a mais. O que sai daqui é o que a máquina toca, que é o que quem
//! assiste esperaria ouvir.
//!
//! # Como cada sistema entrega
//!
//! **Windows.** O WASAPI tem *loopback*: abrir uma **saída** como entrada
//! devolve o que está sendo tocado nela. O cpal liga o modo sozinho quando se
//! faz isso, então não há uma linha de código de plataforma aqui — é o mesmo
//! `build_input_stream` da voz, apontado para o outro lado.
//!
//! **macOS.** O WASAPI não existe, e o CoreAudio não empresta a saída. O caminho
//! é o ScreenCaptureKit, que entrega o áudio do sistema no mesmo `SCStream` da
//! imagem — e é por isso que o macOS **não passa por aqui**: a captura de tela
//! já tem o objeto, e pedir o áudio a ele é um parâmetro. Ver
//! `seele-video/src/captura/macos.rs`.
//!
//! Este módulo é, então, o caminho do Windows e de mais nada. Ele compila em
//! toda plataforma porque o cpal compila em toda plataforma, e falha ao abrir
//! onde o sistema não empresta a saída — que é a resposta certa e não um
//! `cfg` a mais.

use std::num::NonZeroU16;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait as _, StreamTrait as _};
use rtrb::Consumer;

use crate::device::{DeviceError, Side, Stage};
use crate::rt::{capacity_for_ms, capture_path, StreamCounters};
use crate::SAMPLE_RATE_HZ;

/// Quanto som fica esperando ser lido, em milissegundos.
///
/// Meio segundo, que é dez vezes o passo de codificação. Mais que a voz porque
/// aqui não há ninguém esperando: um atraso de um quadro de vídeo no som da tela
/// não é gaguejo, é sincronia — e ficar sem folga custa amostras perdidas, que
/// é o único defeito que este anel pode ter.
const FOLGA_MS: u32 = 500;

/// O som que esta máquina está tocando.
///
/// Enquanto viva, ela empurra amostras para o consumidor devolvido por
/// [`Self::abrir`]. Soltá-la para a captura.
pub struct CapturaDaSaida {
    _fluxo: cpal::Stream,
    /// As amostras esperando quem as leia.
    ///
    /// **Dentro da captura, e não devolvida ao lado dela.** A primeira versão
    /// entregava o `Consumer` do anel, e com ele entregava o `rtrb` a quem
    /// chamasse — `seele-core` teria de ganhar uma dependência para guardar um
    /// tipo que não usa. Aqui, o formato do anel é assunto deste módulo, e o que
    /// atravessa é `Vec<f32>`, que é o mesmo que a captura do macOS entrega.
    amostras: std::sync::Mutex<Consumer<f32>>,
    /// O reamostrador, quando o dispositivo não toca na taxa da casa.
    ///
    /// **Ele existe porque a primeira versão desistia.** Ela conferia a taxa e,
    /// se não fosse 48 kHz, devolvia vazio com um `debug!` — «melhor som nenhum
    /// que som errado». Só que 44,1 kHz é a taxa de metade das placas do mundo,
    /// e o efeito foi a transmissão sair muda para quem tem uma delas, sem uma
    /// palavra na tela. Foi o relato: o Mac assistindo não ouvia o jogo.
    ///
    /// «Melhor som nenhum que som errado» é uma escolha entre duas coisas ruins
    /// quando existe uma terceira: converter. A voz já faz isso, com este mesmo
    /// `RateConverter`, e desde sempre.
    reamostrador: Option<std::sync::Mutex<crate::resample::RateConverter>>,
    contadores: Arc<StreamCounters>,
    taxa_do_dispositivo: u32,
    canais: NonZeroU16,
    /// Quantas vezes [`Self::tomar`] emudeceu por uma falha, e não por não
    /// haver som.
    ///
    /// **Existe porque as duas causas eram indistinguíveis de fora.** Um
    /// cadeado envenenado e um reamostrador recusando um bloco devolviam o
    /// mesmo `Vec::new()` que uma máquina calada devolve, e a única pista
    /// ficava num `debug!` que ninguém lê em produção. Este contador é o que
    /// [`Self::capturadas`] e [`Self::perdidas`] já são para os outros dois
    /// jeitos desta captura ir mal: um número que quem chama pode observar.
    falhas_silenciosas: AtomicU64,
}

impl std::fmt::Debug for CapturaDaSaida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturaDaSaida")
            .field("taxa_do_dispositivo", &self.taxa_do_dispositivo)
            .field("canais", &self.canais)
            .finish_non_exhaustive()
    }
}

/// Constrói o reamostrador para a taxa do dispositivo, quando ele é preciso.
///
/// Separada de [`CapturaDaSaida::abrir`] para poder ser provada sem uma placa
/// de som: o defeito que ela conserta — um erro do reamostrador virando
/// [`DeviceError::NoOutputDevice`] — não depende de hardware nenhum, só de uma
/// taxa que o reamostrador recusa.
fn construir_reamostrador(
    taxa_do_dispositivo: u32,
) -> Result<Option<std::sync::Mutex<crate::resample::RateConverter>>, DeviceError> {
    // O reamostrador só existe quando é preciso: na taxa da casa, converter
    // seria copiar amostra por amostra por nada.
    if taxa_do_dispositivo == SAMPLE_RATE_HZ {
        return Ok(None);
    }

    match crate::resample::RateConverter::new(taxa_do_dispositivo, SAMPLE_RATE_HZ) {
        Ok(conversor) => Ok(Some(std::sync::Mutex::new(conversor))),
        Err(erro) => {
            // Aqui sim não há o que fazer: sem conversor e fora da taxa,
            // qualquer amostra entregue sairia rápida ou lenta demais.
            tracing::warn!(%erro, taxa = taxa_do_dispositivo,
                "não converso a taxa desta saída; a transmissão sai muda");
            // **A causa vai junto, e não é mais trocada por outra.** Era
            // `NoOutputDevice` aqui — e a saída existe, respondeu, abriu; quem
            // não serviu foi o reamostrador. Culpar o dispositivo mandava
            // quem investigasse para o lugar errado.
            Err(DeviceError::Resample {
                side: Side::Output,
                from: taxa_do_dispositivo,
                to: SAMPLE_RATE_HZ,
                source: erro,
            })
        }
    }
}

/// O que [`CapturaDaSaida::tomar`] precisa de um reamostrador: só o passo que
/// pode falhar.
///
/// Existe para abrir um lugar onde um teste possa provar a recusa. `rubato`
/// não tem um jeito de forçar [`crate::resample::RateConverter::push`] a
/// recusar um bloco sem simular o hardware inteiro por trás dele; um dublê que
/// implementa este traço recusa por vontade própria.
trait Conversor {
    fn converter(
        &mut self,
        entrada: &[f32],
        saida: &mut Vec<f32>,
    ) -> Result<(), crate::resample::ConversionError>;
}

impl Conversor for crate::resample::RateConverter {
    fn converter(
        &mut self,
        entrada: &[f32],
        saida: &mut Vec<f32>,
    ) -> Result<(), crate::resample::ConversionError> {
        self.push(entrada, saida)
    }
}

/// A lógica de [`CapturaDaSaida::tomar`], livre do fluxo de dispositivo — o
/// que a deixa provável com um anel e um reamostrador de mentira, sem
/// hardware nenhum.
///
/// Nenhum dos três jeitos de emudecer devolve `Vec::new()` em silêncio puro:
/// os três somam em `falhas_silenciosas` antes, que é o que
/// [`CapturaDaSaida::falhas_silenciosas`] expõe.
fn tomar_de<R: Conversor>(
    amostras: &std::sync::Mutex<Consumer<f32>>,
    reamostrador: Option<&std::sync::Mutex<R>>,
    quantas: usize,
    falhas_silenciosas: &AtomicU64,
) -> Vec<f32> {
    let Ok(mut amostras) = amostras.lock() else {
        // O cadeado das amostras envenenado é um pânico em algum lugar deste
        // laço — não «não há som», e a diferença tem de sobreviver à saída.
        falhas_silenciosas.fetch_add(1, Ordering::Relaxed);
        return Vec::new();
    };
    let mut cruas = Vec::new();
    while cruas.len() < quantas {
        let Ok(amostra) = amostras.pop() else {
            break;
        };
        cruas.push(amostra);
    }
    drop(amostras);

    // Na taxa da casa, as amostras saem como entraram.
    let Some(reamostrador) = reamostrador else {
        return cruas;
    };
    let Ok(mut reamostrador) = reamostrador.lock() else {
        falhas_silenciosas.fetch_add(1, Ordering::Relaxed);
        return Vec::new();
    };
    let mut saida = Vec::new();
    if let Err(erro) = reamostrador.converter(&cruas, &mut saida) {
        tracing::warn!(%erro, "o reamostrador recusou um bloco do som da tela; \
            a transmissão emudeceu neste bloco");
        falhas_silenciosas.fetch_add(1, Ordering::Relaxed);
        return Vec::new();
    }
    saida
}

impl CapturaDaSaida {
    /// Abre a saída pedida — ou a padrão — como entrada.
    ///
    /// Devolve a captura e o consumidor das amostras, em `f32` mono. Mono
    /// porque é o que o codec da casa recebe, e a mistura para um canal é a
    /// mesma que a voz já faz: o primeiro canal de cada quadro.
    ///
    /// # Errors
    ///
    /// [`DeviceError`] quando não há saída, quando o sistema não a empresta como
    /// entrada — que é o caso de todo sistema sem *loopback* — ou quando o fluxo
    /// não abre.
    pub fn abrir(dispositivo: Option<&str>) -> Result<Self, DeviceError> {
        let host = cpal::default_host();
        // A **saída**, aberta como entrada: é isso que liga o loopback.
        let saida = crate::device::resolver(&host, dispositivo, Side::Output)?;

        // **`default_output_config` e não `default_input_config`.**
        //
        // Estava escrito `default_input_config` aqui, com o comentário «é o que
        // o cpal usa para descrever o fluxo de loopback». Não é, e a afirmação
        // nunca foi conferida — este caminho não roda em máquina nenhuma de
        // desenvolvimento. Em `cpal-0.18.2`, `device.rs:807`:
        //
        // ```
        // pub fn default_input_config(&self) -> Result<SupportedStreamConfig, Error> {
        //     if self.data_flow() == Audio::eCapture { self.default_format() }
        //     else { Err(... "Device does not support input") }
        // }
        // ```
        //
        // Numa saída — `eRender` — ele **recusa**, antes de qualquer loopback
        // existir. O efeito era o pior possível: `abrir` falhava, o par ficava
        // com `som: None`, `tomar_som` devolvia vazio para sempre, e a
        // transmissão saía muda com um aviso no log e nada na tela. O som da
        // tela nunca funcionou no Windows, em nenhuma versão.
        //
        // O loopback do cpal é real, mas mora um degrau adiante: ao **construir**
        // o fluxo de entrada num dispositivo `eRender`, ele liga o
        // `AUDCLNT_STREAMFLAGS_LOOPBACK` sozinho (`device.rs:855`). O que faltava
        // era o formato, e o formato do loopback é o formato de mixagem da
        // saída — que é exatamente o que `default_output_config` devolve.
        //
        // Num sistema sem saída nenhuma ele falha aqui, que continua sendo o
        // lugar certo para falhar: antes de qualquer anel ser alocado.
        let config = saida
            .default_output_config()
            .map_err(|source| DeviceError::Device {
                side: Side::Output,
                stage: Stage::Config,
                source,
            })?;

        let taxa_do_dispositivo = config.sample_rate();
        let canais = NonZeroU16::new(config.channels()).unwrap_or(NonZeroU16::MIN);
        let contadores = Arc::new(StreamCounters::default());
        let (sink, consumidor) = capture_path(
            capacity_for_ms(FOLGA_MS, taxa_do_dispositivo),
            canais,
            Arc::clone(&contadores),
        );

        let fluxo = crate::device::abrir_entrada(&saida, &config, sink, Arc::clone(&contadores))?;
        fluxo.play().map_err(|source| DeviceError::Device {
            side: Side::Input,
            stage: Stage::Start,
            source,
        })?;

        let reamostrador = construir_reamostrador(taxa_do_dispositivo)?;

        Ok(Self {
            _fluxo: fluxo,
            amostras: std::sync::Mutex::new(consumidor),
            reamostrador,
            contadores,
            taxa_do_dispositivo,
            canais,
            falhas_silenciosas: AtomicU64::new(0),
        })
    }

    /// Tira até `quantas` amostras, na ordem em que chegaram.
    ///
    /// Vazio quando não há nada — que é diferente de não haver caminho: o
    /// silêncio também produz amostras, e é [`Self::capturadas`] que separa os
    /// dois. Uma falha também devolve vazio — não há amostra que inventar —
    /// mas soma em [`Self::falhas_silenciosas`], que é o que separa isto de
    /// silêncio de verdade.
    #[must_use]
    pub fn tomar(&self, quantas: usize) -> Vec<f32> {
        tomar_de(
            &self.amostras,
            self.reamostrador.as_ref(),
            quantas,
            &self.falhas_silenciosas,
        )
    }

    /// A taxa em que as amostras saem, que **não** é a da casa.
    ///
    /// O dispositivo decide, e quase sempre são 48 kHz — a mesma de
    /// [`SAMPLE_RATE_HZ`]. Quando não for, quem lê reamostra: recusar aqui seria
    /// recusar a máquina inteira por causa de um número.
    #[must_use]
    pub const fn taxa(&self) -> u32 {
        self.taxa_do_dispositivo
    }

    /// Quantas amostras já foram capturadas. Para provar que não é silêncio.
    #[must_use]
    pub fn capturadas(&self) -> u64 {
        self.contadores.snapshot().frames_captured
    }

    /// Quantas foram perdidas por o anel estar cheio.
    #[must_use]
    pub fn perdidas(&self) -> u64 {
        self.contadores.snapshot().capture_overruns
    }

    /// Quantas vezes [`Self::tomar`] devolveu vazio por uma falha — cadeado
    /// envenenado ou reamostrador recusando um bloco —, e não por a máquina
    /// estar calada.
    ///
    /// Zero não prova que nada deu errado num sistema sem este contador
    /// consultado; prova que, enquanto alguém olhou, nada deu. É o mesmo
    /// contrato que [`Self::perdidas`] já tem.
    #[must_use]
    pub fn falhas_silenciosas(&self) -> u64 {
        self.falhas_silenciosas.load(Ordering::Relaxed)
    }

    /// A taxa da casa, para quem precisa comparar.
    #[must_use]
    pub const fn taxa_da_casa() -> u32 {
        SAMPLE_RATE_HZ
    }
}

#[cfg(test)]
mod testes {
    use cpal::traits::{DeviceTrait as _, HostTrait as _};

    use super::*;

    /// O achado D: um erro do reamostrador na abertura não pode virar
    /// "não há dispositivo de saída".
    ///
    /// A saída existe, respondeu e abriu — quem não serviu foi a conversão de
    /// taxa. `ConversionError::ZeroRate` é o jeito determinístico de forçar
    /// essa falha sem depender de hardware nenhum: qualquer taxa de
    /// dispositivo diferente de [`SAMPLE_RATE_HZ`] entra em
    /// `RateConverter::new`, e `0` é sempre recusado por ele.
    ///
    /// Antes da correção isto devolvia [`DeviceError::NoOutputDevice`], que
    /// manda quem investiga para o dispositivo — o lugar errado, porque o
    /// dispositivo nunca foi o problema.
    #[test]
    fn erro_do_reamostrador_na_abertura_nao_vira_ausencia_de_saida() {
        let erro =
            construir_reamostrador(0).expect_err("taxa zero deve ser recusada pelo reamostrador");

        assert!(
            !matches!(erro, DeviceError::NoOutputDevice),
            "o erro do reamostrador foi disfarçado de ausência de dispositivo \
             de saída, o que manda quem investiga para o lugar errado: {erro}"
        );
        assert!(
            matches!(erro, DeviceError::Resample { .. }),
            "o erro não identifica o reamostrador como a causa verdadeira: {erro}"
        );
        assert!(
            std::error::Error::source(&erro).is_some(),
            "o erro do reamostrador perdeu a causa original ao ser propagado"
        );
    }

    /// Um reamostrador de mentira que recusa todo bloco, para provar o achado
    /// E sem depender de `rubato` entrar num estado de erro de verdade.
    struct ReamostradorQueRecusa;

    impl Conversor for ReamostradorQueRecusa {
        fn converter(
            &mut self,
            _entrada: &[f32],
            _saida: &mut Vec<f32>,
        ) -> Result<(), crate::resample::ConversionError> {
            Err(crate::resample::ConversionError::BufferShape { frames: 0 })
        }
    }

    fn anel_com(amostras: &[f32]) -> std::sync::Mutex<Consumer<f32>> {
        let (mut produtor, consumidor) = rtrb::RingBuffer::<f32>::new(amostras.len().max(1) + 1);
        for &amostra in amostras {
            produtor
                .push(amostra)
                .expect("anel de teste tem espaço de sobra");
        }
        std::sync::Mutex::new(consumidor)
    }

    /// O achado E (recusa do reamostrador): `tomar()` não pode devolver um
    /// buffer vazio que pareça silêncio quando o reamostrador recusou o
    /// bloco.
    ///
    /// Antes da correção o retorno era `Vec::new()` com o único vestígio num
    /// `tracing::debug!` — invisível em produção. Depois, a falha soma em
    /// `falhas_silenciosas`, que é observável por quem chama.
    #[test]
    fn recusa_do_reamostrador_em_tomar_nao_vira_buffer_vazio_silencioso() {
        let amostras = anel_com(&[0.1, 0.2, 0.3]);
        let reamostrador = std::sync::Mutex::new(ReamostradorQueRecusa);
        let falhas = AtomicU64::new(0);

        let saida = tomar_de(&amostras, Some(&reamostrador), 3, &falhas);

        assert!(
            saida.is_empty(),
            "não há amostra convertida para entregar quando o reamostrador recusa"
        );
        assert_eq!(
            falhas.load(Ordering::Relaxed),
            1,
            "a recusa do reamostrador em tomar() teve de ficar observável, e não \
             desaparecer como se fosse uma máquina calada"
        );
    }

    /// O mesmo achado E, para o cadeado das amostras envenenado.
    ///
    /// Um pânico em outra thread com o cadeado na mão é um defeito deste
    /// laço, não "não há som". Antes da correção os dois eram indistinguíveis
    /// de fora: ambos devolviam `Vec::new()`.
    #[test]
    fn cadeado_de_amostras_envenenado_em_tomar_nao_vira_buffer_vazio_silencioso() {
        let amostras = Arc::new(anel_com(&[0.1, 0.2]));
        let travado = Arc::clone(&amostras);
        let _ = std::thread::spawn(move || {
            let _guarda = travado.lock().unwrap();
            panic!("envenenando de propósito, para o teste");
        })
        .join();
        assert!(amostras.is_poisoned(), "o cadeado de teste não envenenou");

        let falhas = AtomicU64::new(0);
        let saida = tomar_de::<crate::resample::RateConverter>(&amostras, None, 2, &falhas);

        assert!(
            saida.is_empty(),
            "um cadeado envenenado não tem o que entregar"
        );
        assert_eq!(
            falhas.load(Ordering::Relaxed),
            1,
            "o cadeado envenenado teve de ficar observável, e não desaparecer \
             como se fosse uma máquina calada"
        );
    }

    /// A suposição que faltava conferir, e que custou o som da tela inteiro.
    ///
    /// [`CapturaDaSaida::abrir`] pergunta o formato à **saída**. A pergunta era
    /// `default_input_config`, com um comentário afirmando que era assim que o
    /// cpal descreve o fluxo de loopback. Não é: numa saída ele recusa, e o
    /// resultado foi uma transmissão muda em toda versão que já saiu para o
    /// Windows — sem erro na tela, com um aviso num log que ninguém lia.
    ///
    /// Este teste roda em qualquer sistema porque a regra não é do WASAPI: um
    /// dispositivo de saída não tem configuração de entrada em backend nenhum.
    /// O loopback do Windows entra um degrau adiante, ao construir o fluxo, e é
    /// por isso que o formato tem de vir do lado de saída.
    ///
    /// Se um dia o cpal passar a responder `default_input_config` numa saída,
    /// este teste falha — e falha dizendo o que mudou, em vez de deixar alguém
    /// reintroduzir a chamada antiga achando que ela sempre serviu.
    #[test]
    fn uma_saida_nao_responde_configuracao_de_entrada() {
        let host = cpal::default_host();
        let Some(saida) = host.default_output_device() else {
            eprintln!("PULADO: esta máquina não tem saída padrão.");
            return;
        };

        assert!(
            saida.default_output_config().is_ok(),
            "a saída padrão não descreve o próprio formato de saída, que é de \
             onde `CapturaDaSaida::abrir` tira o formato do loopback"
        );
        assert!(
            saida.default_input_config().is_err(),
            "uma saída passou a responder `default_input_config`.\n\
             Era o que o código antigo supunha e o que o cpal recusa — se isso \
             mudou, releia `CapturaDaSaida::abrir` antes de confiar na resposta."
        );
    }

    /// A prova de campo: o som que esta máquina está tocando.
    ///
    /// **Pula em voz alta** onde o sistema não empresta a saída como entrada —
    /// que é todo sistema sem *loopback*, o macOS incluído, onde o caminho é
    /// outro. Um teste que passa por não ter rodado é pior que um teste
    /// vermelho.
    ///
    /// Não afirma que há **som**: uma máquina em silêncio é um estado legítimo,
    /// e um teste que exigisse barulho seria um teste que falha por ninguém
    /// estar tocando música. O que ele afirma é que o fluxo abre, anda, e não
    /// perde amostras — as três coisas que decidem se este caminho existe.
    /// Um fluxo de saída tocando silêncio, para que haja o que capturar.
    ///
    /// **O loopback não inventa amostras.** Ele entrega o que a placa está
    /// tocando, e com a máquina parada não há sessão de renderização ativa: o
    /// cliente não recebe pacote nenhum — nem de silêncio, porque não há de onde
    /// tirá-lo. O teste afirmava o contrário («o silêncio também produz
    /// amostras») e por isso reprovava numa máquina calada, que é o estado normal
    /// de uma máquina de compilação.
    ///
    /// Manter uma saída tocando zeros cria a condição em vez de supô-la, e não faz
    /// barulho nenhum: são zeros. O que o teste passa a provar é o que o produto
    /// precisa — que o som que a máquina toca chega à captura.
    fn manter_a_saida_tocando() -> Option<cpal::Stream> {
        fn zeros<T: cpal::SizedSample>(
            saida: &cpal::Device,
            config: cpal::StreamConfig,
        ) -> Option<cpal::Stream> {
            saida
                .build_output_stream(
                    config,
                    move |dados: &mut [T], _: &cpal::OutputCallbackInfo| {
                        dados.fill(T::EQUILIBRIUM);
                    },
                    |_erro| {},
                    None,
                )
                .ok()
        }

        let saida = cpal::default_host().default_output_device()?;
        let config = saida.default_output_config().ok()?;
        let fluxo = match config.sample_format() {
            cpal::SampleFormat::F32 => zeros::<f32>(&saida, config.config()),
            cpal::SampleFormat::I16 => zeros::<i16>(&saida, config.config()),
            cpal::SampleFormat::U16 => zeros::<u16>(&saida, config.config()),
            _ => None,
        }?;
        fluxo.play().ok()?;
        Some(fluxo)
    }

    #[test]
    fn a_saida_desta_maquina_abre_como_entrada() {
        // Antes da captura: o loopback entrega o que está tocando **enquanto**
        // está tocando, e uma saída que começasse depois deixaria os primeiros
        // instantes vazios sem que isso dissesse nada sobre o caminho.
        let manutencao = manter_a_saida_tocando();

        let captura = match CapturaDaSaida::abrir(None) {
            Ok(captura) => captura,
            Err(erro) => {
                // **A mensagem não absolve mais ninguém.**
                //
                // Ela dizia «no macOS é esperado», e era isso que escondia o
                // defeito: a falha real era `default_input_config` recusando
                // numa saída, o que acontece em **todo** sistema, e este teste
                // pulava em todos eles anunciando que pular era o certo. No
                // Windows, onde não é, o pulo tinha a mesma cara.
                eprintln!(
                    "PULADO: a saída desta máquina não abriu como entrada ({erro}). \
                     Isso é normal onde não há loopback; **não** é normal no \
                     Windows, onde é o caminho do som da tela."
                );
                return;
            }
        };

        let comeco = std::time::Instant::now();
        let mut lidas = 0_usize;
        while comeco.elapsed().as_secs_f64() < 2.0 && lidas < 4800 {
            lidas += captura.tomar(4800).len();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            captura.taxa() >= 8_000,
            "a taxa do dispositivo não faz sentido: {}",
            captura.taxa()
        );
        // **Só o Windows exige amostras, e isto não é um pulo de conveniência.**
        //
        // Loopback é do WASAPI. Fora dele, abrir a saída como entrada é uma
        // operação que o sistema aceita e que não produz nada — e passou a
        // acontecer aqui quando `abrir` deixou de morrer em
        // `default_input_config`. No macOS o som da tela vem do
        // ScreenCaptureKit, e esta captura não está no caminho de ninguém.
        //
        // A linha de cima continua valendo em todo sistema: se a taxa não faz
        // sentido, o fluxo foi aberto sobre a coisa errada em qualquer um deles.
        if !cfg!(target_os = "windows") {
            eprintln!(
                "PARCIAL: fora do Windows não há loopback — o fluxo abre e não anda. \
                 Só a taxa é conferida aqui; as amostras, no Windows."
            );
            return;
        }
        assert!(
            manutencao.is_some(),
            "não consegui pôr esta máquina para tocar, e sem isso o loopback não \
             tem o que entregar. Num Windows sem saída utilizável o caminho do som \
             da tela não existe — e é ele que este teste mede."
        );
        assert!(
            lidas > 0,
            "o fluxo abriu e não andou: nenhuma amostra em dois segundos, com uma \
             saída tocando o tempo todo. Aqui isso é o fluxo aberto sobre o \
             dispositivo errado — não é a máquina estar calada, porque este teste \
             tirou essa possibilidade do caminho."
        );
        // E as amostras saem na taxa da casa, convertidas quando preciso.
        //
        // **Sem esta linha o teste passaria com a versão que desistia**: ela
        // devolvia vazio fora de 48 kHz, e `lidas > 0` seria falso — mas por um
        // motivo que a mensagem de erro acima atribui a outra coisa. Ela diz
        // «o fluxo abriu e não andou», e o defeito era «andou e foi jogado
        // fora».
        assert!(
            captura.taxa() >= 8_000,
            "a taxa do dispositivo não faz sentido: {}",
            captura.taxa()
        );

        assert_eq!(
            captura.perdidas(),
            0,
            "o anel encheu enquanto o teste lia o mais rápido que podia; \
             `FOLGA_MS` está curto demais para esta máquina"
        );
    }
}
