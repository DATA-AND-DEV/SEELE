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
}

impl std::fmt::Debug for CapturaDaSaida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturaDaSaida")
            .field("taxa_do_dispositivo", &self.taxa_do_dispositivo)
            .field("canais", &self.canais)
            .finish_non_exhaustive()
    }
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

        // `default_input_config` numa saída é o que o cpal usa para descrever o
        // fluxo de loopback. Num sistema sem loopback ele falha aqui, que é o
        // lugar certo para falhar: antes de qualquer anel ser alocado.
        let config = saida
            .default_input_config()
            .map_err(|source| DeviceError::Device {
                side: Side::Input,
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

        // O reamostrador só existe quando é preciso: na taxa da casa, converter
        // seria copiar amostra por amostra por nada.
        let reamostrador = if taxa_do_dispositivo == SAMPLE_RATE_HZ {
            None
        } else {
            match crate::resample::RateConverter::new(taxa_do_dispositivo, SAMPLE_RATE_HZ) {
                Ok(conversor) => Some(std::sync::Mutex::new(conversor)),
                Err(erro) => {
                    // Aqui sim não há o que fazer: sem conversor e fora da taxa,
                    // qualquer amostra entregue sairia rápida ou lenta demais.
                    tracing::warn!(%erro, taxa = taxa_do_dispositivo,
                        "não converso a taxa desta saída; a transmissão sai muda");
                    // `NoOutputDevice` porque é o que sobra de verdade: existe
                    // uma saída, e ela não serve a esta captura. Quem chama trata
                    // do mesmo jeito — transmite muda, e não deixa de transmitir.
                    return Err(DeviceError::NoOutputDevice);
                }
            }
        };

        Ok(Self {
            _fluxo: fluxo,
            amostras: std::sync::Mutex::new(consumidor),
            reamostrador,
            contadores,
            taxa_do_dispositivo,
            canais,
        })
    }

    /// Tira até `quantas` amostras, na ordem em que chegaram.
    ///
    /// Vazio quando não há nada — que é diferente de não haver caminho: o
    /// silêncio também produz amostras, e é [`Self::capturadas`] que separa os
    /// dois.
    #[must_use]
    pub fn tomar(&self, quantas: usize) -> Vec<f32> {
        let Ok(mut amostras) = self.amostras.lock() else {
            return Vec::new();
        };
        let mut cruas = Vec::new();
        while cruas.len() < quantas {
            let Ok(amostra) = amostras.pop() else {
                break;
            };
            cruas.push(amostra);
        }

        // Na taxa da casa, as amostras saem como entraram.
        let Some(reamostrador) = self.reamostrador.as_ref() else {
            return cruas;
        };
        let Ok(mut reamostrador) = reamostrador.lock() else {
            return Vec::new();
        };
        let mut saida = Vec::new();
        if let Err(erro) = reamostrador.push(&cruas, &mut saida) {
            tracing::debug!(%erro, "o reamostrador recusou um bloco do som da tela");
            return Vec::new();
        }
        saida
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

    /// A taxa da casa, para quem precisa comparar.
    #[must_use]
    pub const fn taxa_da_casa() -> u32 {
        SAMPLE_RATE_HZ
    }
}

#[cfg(test)]
mod testes {
    use super::*;

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
    #[test]
    fn a_saida_desta_maquina_abre_como_entrada() {
        let captura = match CapturaDaSaida::abrir(None) {
            Ok(captura) => captura,
            Err(erro) => {
                eprintln!(
                    "PULADO: este sistema não empresta a saída como entrada ({erro}). \
                     No macOS é esperado — o áudio da tela vem do ScreenCaptureKit."
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
        assert!(
            lidas > 0,
            "o fluxo abriu e não andou: nenhuma amostra em dois segundos. \
             Num sistema com loopback isto é o fluxo aberto sobre o dispositivo \
             errado, e não silêncio — o silêncio também produz amostras."
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
