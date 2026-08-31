//! The audio metrics `specs/03-audio.md` promises the rest of the system.
//!
//! > `nivel_entrada`, `nivel_saida`, `falando`, `profundidade_jitter_ms`,
//! > `perda_pct`, `quadros_ocultados`, `bitrate_atual`, `rtt_ms`. All of this
//! > appears in the interface's telemetry bar and in the sync calculation.
//!
//! This module is the one place that names them, so nobody downstream has to
//! know that the level comes from [`crate::gate`], the depth from
//! [`crate::jitter`] and the peak from [`crate::mixer`].
//!
//! # Plain data, and nothing else
//!
//! No formatting, no colour, no bands. `specs/07-estetica.md` defines
//! four Sync Ratio bands with four colours, and `specs/06-clientes-gui.md`
//! requires a no-colour mode — both are shell concerns. A `to_string()` here
//! would be the architecture error `specs/01-arquitetura.md` warns about, and
//! it would have to be written twice more for the desktop and mobile shells.
//!
//! # One item on that list is not an audio metric
//!
//! `rtt_ms` cannot be produced here. It comes from `Ping`/`Pong` on the control
//! stream (`specs/02-protocolo.md`), which this crate never sees — and must not,
//! since `seele-audio` depends only on `seele-proto`. It is named in
//! [`SourceTelemetry`] as an input the assembling layer fills in, so the shape
//! of the struct matches what `specs/03-audio.md` describes even though the
//! value arrives from elsewhere.
//!
//! # Why the split into local and per-source
//!
//! `specs/07-estetica.md` makes the per-person Sync Ratio the signature
//! element of the product: "each person in the roster has a live percentage
//! derived from the RTT, jitter and loss **of that connection**". With an SFU
//! (`specs/01-arquitetura.md`) every talker arrives as a separate stream with
//! its own jitter buffer, so those numbers are genuinely per-source. Capture
//! level and the speaking flag, by contrast, are about this machine only.

use crate::gate::GateMetrics;
use crate::jitter::JitterMetrics;
use crate::mixer::MixMetrics;
use crate::rt::StreamMetrics;

/// Metrics about this machine's own audio path.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LocalTelemetry {
    /// `nivel_entrada` — RMS of the last captured frame, 0.0 to 1.0.
    pub input_level: f32,
    /// `nivel_saida` — peak of the last mixed frame, after clipping.
    pub output_level: f32,
    /// `falando` — whether the microphone is currently transmitting.
    ///
    /// One boolean regardless of push-to-talk or voice activation
    /// (`specs/03-audio.md`), announced to the server so the interface can
    /// highlight who is talking.
    pub speaking: bool,
    /// `bitrate_atual` — encoder bitrate in bits per second.
    pub bitrate_bps: u32,
    /// Frames the capture ring dropped because the processing thread stalled.
    ///
    /// Not in the `specs/03-audio.md` list, but a non-zero value here means this
    /// machine is the problem rather than the network — which is exactly the
    /// thing a user staring at a bad Sync Ratio needs to be able to tell.
    pub capture_overruns: u64,
    /// Frames the playback callback had to invent. Same reasoning.
    pub playback_underruns: u64,
    /// Device-level errors, such as a disconnection.
    pub device_errors: u64,

    /// A volta mais longa que o laço de voz deu entre dois quadros, em ms.
    ///
    /// Não é uma medida sobre o áudio: é quanto tempo a máquina levou para
    /// voltar a conferir o prazo de reprodução. Abaixo de um quadro (20 ms) o
    /// laço acompanha o relógio sozinho. Acima, ele só acompanha porque o
    /// [`crate::playout::PlayoutClock`] repõe — e antes de haver reposição, ele
    /// não acompanhava: entregava 20 ms de áudio por volta enquanto a volta
    /// custava mais que 20 ms, e o anel de reprodução esvaziava na diferença.
    ///
    /// **É o número a pedir de quem relata áudio picotado.** Ele separa "a rede
    /// está perdendo pacote" de "esta máquina não consegue empurrar cinquenta
    /// quadros por segundo", que soam idênticos e têm consertos opostos.
    pub playout_worst_lateness_ms: f64,

    /// Quadros de reprodução que foram reposição de atraso, não o quadro da vez.
    ///
    /// Zero é o normal. Crescendo, significa que a volta do laço está passando
    /// de 20 ms com frequência.
    pub playout_catchup_frames: u64,

    /// Quantas vezes o atraso passou do que dá para repor e o relógio reacertou.
    pub playout_resyncs: u64,

    /// Correção de compasso em vigor, em partes por milhão do nominal.
    ///
    /// Em regime isto **é** a diferença entre o relógio desta máquina e o
    /// cristal do dispositivo de saída, medida — positivo é dispositivo rápido.
    /// Umas dezenas de ppm é o normal de dois cristais quaisquer. Zero durante
    /// o aquecimento, e zero num caminho de áudio sem malha própria.
    pub pacing_ppm: f64,

    /// Profundidade do anel de reprodução, alisada, em milissegundos.
    pub pacing_depth_ms: f64,

    /// Profundidade que a malha está segurando, em milissegundos.
    ///
    /// A distância entre esta e [`Self::pacing_depth_ms`] é o deslocamento que
    /// o controle proporcional deixa de pé, e ele vale `deriva × τ`: alguns
    /// milissegundos. Uma distância de dezenas significa que a malha não está
    /// dando conta, e aí o número a olhar é o de grampos.
    pub pacing_target_ms: f64,

    /// Quantas vezes a razão pedida saiu da faixa e foi grampeada.
    ///
    /// Zero é o normal. Crescendo, a causa não é deriva de cristal.
    pub pacing_clamps: u64,

    /// Quantas vezes o anel foi achado vazio e recebeu silêncio até o alvo.
    ///
    /// Uma no arranque é o normal — o anel começa vazio.
    pub pacing_primes: u64,
}

impl LocalTelemetry {
    /// Assembles from the modules that actually measure these things.
    #[must_use]
    pub fn assemble(
        stream: StreamMetrics,
        gate: GateMetrics,
        mix: MixMetrics,
        bitrate_bps: u32,
        speaking: bool,
    ) -> Self {
        Self {
            input_level: gate.input_rms,
            output_level: mix.peak_output,
            speaking,
            bitrate_bps,
            capture_overruns: stream.capture_overruns,
            playback_underruns: stream.playback_underruns,
            device_errors: stream.stream_errors,
            playout_worst_lateness_ms: 0.0,
            playout_catchup_frames: 0,
            playout_resyncs: 0,
            pacing_ppm: 0.0,
            pacing_depth_ms: 0.0,
            pacing_target_ms: 0.0,
            pacing_clamps: 0,
            pacing_primes: 0,
        }
    }

    /// Anexa o que o relógio de reprodução mediu.
    ///
    /// Separado de [`Self::assemble`] porque o relógio não vive neste crate: ele
    /// é conferido pelo laço de voz, que é quem sabe quanto tempo a volta
    /// dele levou. Sem esta chamada os três campos ficam zerados, que é o que
    /// um caminho de áudio sem laço próprio deve mesmo relatar.
    #[must_use]
    pub fn with_playout(mut self, playout: crate::playout::PlayoutMetrics) -> Self {
        self.playout_worst_lateness_ms = playout.worst_lateness_ms;
        self.playout_catchup_frames = playout.catchup_frames;
        self.playout_resyncs = playout.resyncs;
        self
    }

    /// Anexa o que a malha de compasso mediu.
    ///
    /// Separado de [`Self::assemble`] pela mesma razão que
    /// [`Self::with_playout`]: a malha é conduzida pelo laço de voz, que é quem
    /// olha o anel antes de empurrar. Sem esta chamada os campos ficam zerados,
    /// que é o que um caminho de áudio sem malha própria deve mesmo relatar.
    #[must_use]
    pub fn with_pacing(mut self, pacing: crate::pacing::PacingMetrics) -> Self {
        self.pacing_ppm = pacing.ppm;
        self.pacing_depth_ms = pacing.depth_ms;
        self.pacing_target_ms = pacing.target_ms;
        self.pacing_clamps = pacing.clamps;
        self.pacing_primes = pacing.primes;
        self
    }

    /// Quantas coisas deram errado nesta máquina, desde que o áudio começou.
    ///
    /// Cumulativo. Quem quer saber se está dando errado **agora** pergunta ao
    /// [`FalhaLocal`], não a este número.
    #[must_use]
    pub fn tropecos(&self) -> u64 {
        self.capture_overruns
            .saturating_add(self.playback_underruns)
            .saturating_add(self.device_errors)
    }
}

/// Se a máquina está derrubando áudio **agora**.
///
/// Isto já foi uma comparação com zero sobre contadores cumulativos, e o efeito
/// era este: um estouro qualquer, uma vez, acendia "ÁUDIO LOCAL FALHANDO" e o
/// aviso não apagava mais. Abrir um dispositivo de captura produz estouros —
/// o fluxo começa a encher o anel antes de alguém drenar —, então na prática o
/// alarme acendia no arranque de toda sessão e ficava aceso com o áudio
/// perfeito. Um alerta permanente é um alerta que ninguém lê, e `specs/05`
/// separa justamente falha local de falha de rede para que cada uma signifique
/// alguma coisa.
///
/// Agora é derivada: falha é o contador **crescer** entre duas olhadas. Acende
/// enquanto o problema acontece e apaga sozinho quando para.
#[derive(Debug, Clone, Copy, Default)]
pub struct FalhaLocal {
    visto: u64,
    /// Quantas amostras seguidas sem crescimento.
    ///
    /// Uma folga curta antes de apagar: entre duas leituras o contador pode
    /// ficar parado por um instante e voltar a subir, e um aviso piscando é
    /// pior que um aviso errado.
    quietas: u8,
    acesa: bool,
}

/// Quantas amostras quietas apagam o aviso.
const QUIETAS_PARA_APAGAR: u8 = 8;

impl FalhaLocal {
    /// Um detector que ainda não viu nada.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Olha os contadores e diz se a máquina está falhando agora.
    pub fn observar(&mut self, agora: &LocalTelemetry) -> bool {
        let total = agora.tropecos();
        if total > self.visto {
            self.visto = total;
            self.quietas = 0;
            self.acesa = true;
        } else if self.acesa {
            self.quietas = self.quietas.saturating_add(1);
            if self.quietas >= QUIETAS_PARA_APAGAR {
                self.acesa = false;
            }
        }
        self.acesa
    }

    /// O que a última olhada concluiu.
    #[must_use]
    pub fn acesa(&self) -> bool {
        self.acesa
    }
}

/// Metrics about one incoming stream — one talker, one `ssrc`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SourceTelemetry {
    /// Which stream this describes. Assigned by the server on voice room entry
    /// (`specs/02-protocolo.md`); the client maps it to a person from the control
    /// channel.
    pub ssrc: u32,
    /// `profundidade_jitter_ms` — how much delay the buffer is holding.
    pub jitter_depth_ms: f64,
    /// Smoothed arrival jitter, RFC 3550.
    pub jitter_ms: f64,
    /// `perda_pct`, as a fraction in 0.0 to 1.0.
    ///
    /// The shell multiplies by 100 and decides how to render it. Silence from a
    /// talker who stopped is **not** counted — see [`crate::jitter`], task M1.9.
    pub loss_fraction: f64,
    /// `quadros_ocultados` — slots filled by Opus PLC.
    pub concealed_frames: u64,
    /// Slots the talker was simply silent for. Not loss.
    pub comfort_frames: u64,
    /// Frames that arrived too late to use.
    pub late_frames: u64,
    /// `rtt_ms` — round trip to the server.
    ///
    /// **Not measured by this crate.** Filled in by the layer that owns the
    /// control stream; see the module docs.
    pub rtt_ms: Option<f64>,
}

impl SourceTelemetry {
    /// Assembles from one jitter buffer's metrics.
    ///
    /// `rtt_ms` is left empty; the protocol layer fills it in.
    #[must_use]
    pub fn assemble(ssrc: u32, jitter: JitterMetrics) -> Self {
        Self {
            ssrc,
            jitter_depth_ms: jitter.depth_ms,
            jitter_ms: jitter.jitter_ms,
            loss_fraction: jitter.loss_fraction(),
            concealed_frames: jitter.frames_concealed,
            comfort_frames: jitter.frames_comfort,
            late_frames: jitter.late_discards,
            rtt_ms: None,
        }
    }

    /// Attaches a round-trip measurement from the control layer.
    #[must_use]
    pub fn with_rtt(mut self, rtt_ms: f64) -> Self {
        self.rtt_ms = Some(rtt_ms);
        self
    }

    /// True once there is enough information to compute a Sync Ratio.
    ///
    /// `specs/02-protocolo.md` derives it from RTT, jitter and loss. Without an
    /// RTT the shell should show the metric as unavailable rather than compute
    /// it from two of the three inputs and present a confident wrong number.
    #[must_use]
    pub fn can_compute_sync(&self) -> bool {
        self.rtt_ms.is_some()
    }
}

/// Everything the audio layer knows, in one place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioTelemetry {
    /// This machine's own path.
    pub local: LocalTelemetry,
    /// One entry per talker currently being received.
    pub sources: Vec<SourceTelemetry>,
}

impl AudioTelemetry {
    /// Looks up one source.
    #[must_use]
    pub fn source(&self, ssrc: u32) -> Option<&SourceTelemetry> {
        self.sources.iter().find(|source| source.ssrc == ssrc)
    }

    /// Worst loss across every incoming stream.
    ///
    /// A single bad talker should be visible without the shell having to scan
    /// the list, but the per-source numbers stay available because
    /// `specs/07-estetica.md` puts a percentage next to each person.
    #[must_use]
    pub fn worst_loss_fraction(&self) -> f64 {
        self.sources
            .iter()
            .map(|source| source.loss_fraction)
            .fold(0.0, f64::max)
    }

    /// Deepest buffer across every incoming stream.
    #[must_use]
    pub fn worst_jitter_depth_ms(&self) -> f64 {
        self.sources
            .iter()
            .map(|source| source.jitter_depth_ms)
            .fold(0.0, f64::max)
    }

    /// O pior jitter **de chegada** entre as fontes recebidas, em milissegundos.
    ///
    /// A outra grandeza chamada jitter, e a que uma pessoa quer ver: a variação
    /// do intervalo entre pacotes da RFC 3550, medida neste receptor. A de cima
    /// é a reserva do anel de reprodução, que cresce quando as coisas vão bem —
    /// mostrá-la sob o rótulo `JITTER` faz uma conexão saudável parecer ruim, e
    /// foi assim que a tela do app e a barra da TUI erraram, cada uma por sua
    /// conta.
    ///
    /// Existe aqui, e não numa casca, exatamente por isso: as duas cascas
    /// escolhiam sozinhas entre dois `f64` na mesma unidade, e escolher errado
    /// não parece um erro em lugar nenhum. Um nome que diz o destino é o que
    /// separa os dois.
    ///
    /// # Por que `Option`, e não zero
    ///
    /// Porque o pior de uma lista vazia seria zero, e zero é justamente o valor
    /// que este conserto tirou da tela — o relatório do servidor manda `0.0` fixo
    /// porque um servidor não tem como medir jitter. Sem fonte nenhuma sendo
    /// recebida não há o que dizer, e quem chama decide o que fazer com isso.
    #[must_use]
    pub fn worst_arrival_jitter_ms(&self) -> Option<f64> {
        self.sources
            .iter()
            .map(|source| source.jitter_ms)
            .reduce(f64::max)
    }
}

#[cfg(test)]
mod falha_local {
    use super::*;

    fn com(tropecos: u64) -> LocalTelemetry {
        LocalTelemetry {
            capture_overruns: tropecos,
            ..LocalTelemetry::default()
        }
    }

    #[test]
    fn um_estouro_no_arranque_nao_acende_o_aviso_para_sempre() {
        // O defeito que isto trava. A regra era `capture_overruns > 0` sobre um
        // contador cumulativo, e abrir um dispositivo de captura sempre produz
        // alguns estouros — o fluxo enche o anel antes de alguém drenar. Na
        // prática o aviso "ÁUDIO LOCAL FALHANDO" acendia no começo de toda
        // sessão e nunca mais apagava, com o áudio funcionando perfeitamente.
        let mut detector = FalhaLocal::new();

        assert!(detector.observar(&com(3392)), "não avisou quando aconteceu");

        // O contador para de crescer: o problema passou.
        for _ in 0..QUIETAS_PARA_APAGAR {
            detector.observar(&com(3392));
        }
        assert!(
            !detector.acesa(),
            "o aviso ficou aceso com o contador parado — é o defeito de volta"
        );
    }

    #[test]
    fn o_aviso_volta_quando_o_problema_volta() {
        let mut detector = FalhaLocal::new();
        detector.observar(&com(10));
        for _ in 0..QUIETAS_PARA_APAGAR {
            detector.observar(&com(10));
        }
        assert!(!detector.acesa());

        assert!(
            detector.observar(&com(11)),
            "um estouro novo tem que acender de novo"
        );
    }

    #[test]
    fn nao_pisca_a_cada_amostra_parada() {
        // Entre duas leituras o contador pode ficar parado um instante e voltar
        // a subir. Um aviso piscando é pior que um aviso errado: vira ruído.
        let mut detector = FalhaLocal::new();
        detector.observar(&com(1));
        assert!(detector.observar(&com(1)), "apagou cedo demais");
        assert!(detector.observar(&com(2)));
    }

    #[test]
    fn maquina_saudavel_nunca_acende() {
        let mut detector = FalhaLocal::new();
        for _ in 0..50 {
            assert!(!detector.observar(&LocalTelemetry::default()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jitter_metrics() -> JitterMetrics {
        JitterMetrics {
            frames_played: 900,
            frames_concealed: 60,
            frames_silenced: 40,
            frames_comfort: 500,
            late_discards: 3,
            depth_ms: 45.0,
            jitter_ms: 7.5,
            ..JitterMetrics::default()
        }
    }

    #[test]
    fn source_telemetry_carries_the_names_from_the_spec() {
        let telemetry = SourceTelemetry::assemble(42, jitter_metrics());

        assert_eq!(telemetry.ssrc, 42);
        assert_eq!(telemetry.jitter_depth_ms, 45.0);
        assert_eq!(telemetry.concealed_frames, 60);
        assert_eq!(telemetry.comfort_frames, 500);
        assert_eq!(telemetry.late_frames, 3);
        // 100 bad slots out of 1000 played-or-lost. The 500 comfort slots are
        // excluded from both halves, per task M1.9.
        assert!((telemetry.loss_fraction - 0.1).abs() < 1e-9);
    }

    #[test]
    fn silence_never_reaches_the_loss_figure() {
        // The regression guard for gap G5, one layer up from the jitter buffer.
        // If this ever changes, the Sync Ratio starts falling whenever a room
        // goes quiet, and that is the most visible number in the product.
        let quiet = JitterMetrics {
            frames_played: 100,
            frames_comfort: 10_000,
            ..JitterMetrics::default()
        };
        let telemetry = SourceTelemetry::assemble(1, quiet);
        assert_eq!(telemetry.loss_fraction, 0.0);
        assert_eq!(telemetry.comfort_frames, 10_000);
    }

    #[test]
    fn rtt_is_absent_until_the_protocol_layer_supplies_it() {
        // specs/03-audio.md lists rtt_ms among the audio metrics, but it comes
        // from Ping/Pong on the control stream, which this crate cannot see.
        let telemetry = SourceTelemetry::assemble(1, jitter_metrics());
        assert_eq!(telemetry.rtt_ms, None);
        assert!(
            !telemetry.can_compute_sync(),
            "a Sync Ratio from two of three inputs is a confident wrong number"
        );

        let with_rtt = telemetry.with_rtt(38.0);
        assert_eq!(with_rtt.rtt_ms, Some(38.0));
        assert!(with_rtt.can_compute_sync());
    }

    #[test]
    fn os_tropecos_desta_maquina_somam_as_tres_origens() {
        // Quem ouve buraco no áudio não distingue estouro de perda de pacote, e
        // os dois têm conserto oposto — por isso a máquina é contada à parte.
        //
        // Este teste já afirmou que qualquer valor acima de zero **é** falha.
        // Era o defeito: contador cumulativo comparado com zero acende no
        // primeiro tropeço e não apaga nunca. O total continua sendo total;
        // quem responde "está falhando agora" é o `FalhaLocal`.
        assert_eq!(LocalTelemetry::default().tropecos(), 0);

        let tropecando = LocalTelemetry {
            capture_overruns: 12,
            playback_underruns: 3,
            device_errors: 1,
            ..LocalTelemetry::default()
        };
        assert_eq!(tropecando.tropecos(), 16);
    }

    #[test]
    fn assembly_pulls_each_field_from_the_module_that_measures_it() {
        let local = LocalTelemetry::assemble(
            StreamMetrics {
                capture_overruns: 2,
                playback_underruns: 5,
                stream_errors: 1,
                ..StreamMetrics::default()
            },
            GateMetrics {
                input_rms: 0.31,
                ..GateMetrics::default()
            },
            MixMetrics {
                peak_output: 0.87,
                ..MixMetrics::default()
            },
            32_000,
            true,
        );

        assert!((local.input_level - 0.31).abs() < 1e-6);
        assert!((local.output_level - 0.87).abs() < 1e-6);
        assert!(local.speaking);
        assert_eq!(local.bitrate_bps, 32_000);
        assert_eq!(local.capture_overruns, 2);
        assert_eq!(local.playback_underruns, 5);
        assert_eq!(local.device_errors, 1);
    }

    #[test]
    fn o_compasso_chega_inteiro_e_nao_conta_como_tropeco() {
        // Duas coisas de uma vez, e a segunda é a que importa. A malha de
        // compasso corrigindo 40 ppm é a máquina **funcionando**, não falhando:
        // se estes números entrassem em `tropecos`, "ÁUDIO LOCAL FALHANDO"
        // acenderia justamente porque a deriva está sendo corrigida, que é o
        // defeito da pendência 2 de volta com outro nome.
        let local = LocalTelemetry::default().with_pacing(crate::pacing::PacingMetrics {
            ppm: -42.0,
            depth_ms: 19.5,
            target_ms: 21.3,
            clamps: 7,
            primes: 1,
            ..crate::pacing::PacingMetrics::default()
        });

        assert!((local.pacing_ppm + 42.0).abs() < 1e-9);
        assert!((local.pacing_depth_ms - 19.5).abs() < 1e-9);
        assert!((local.pacing_target_ms - 21.3).abs() < 1e-9);
        assert_eq!(local.pacing_clamps, 7);
        assert_eq!(local.pacing_primes, 1);
        assert_eq!(local.tropecos(), 0);
    }

    #[test]
    fn per_source_numbers_survive_aggregation() {
        // specs/07-estetica.md puts a live percentage next to each person,
        // so a summary that replaced the per-source list would remove the
        // product's signature element.
        let telemetry = AudioTelemetry {
            local: LocalTelemetry::default(),
            sources: vec![
                SourceTelemetry {
                    ssrc: 1,
                    loss_fraction: 0.01,
                    jitter_depth_ms: 20.0,
                    ..SourceTelemetry::default()
                },
                SourceTelemetry {
                    ssrc: 2,
                    loss_fraction: 0.15,
                    jitter_depth_ms: 120.0,
                    ..SourceTelemetry::default()
                },
            ],
        };

        assert!((telemetry.worst_loss_fraction() - 0.15).abs() < 1e-9);
        assert!((telemetry.worst_jitter_depth_ms() - 120.0).abs() < 1e-9);
        assert!((telemetry.source(1).map(|s| s.loss_fraction)).is_some_and(|l| l < 0.02));
        assert_eq!(telemetry.source(99), None);
    }

    #[test]
    fn an_empty_roster_reports_zero_rather_than_panicking() {
        let telemetry = AudioTelemetry::default();
        assert_eq!(telemetry.worst_loss_fraction(), 0.0);
        assert_eq!(telemetry.worst_jitter_depth_ms(), 0.0);
    }

    #[test]
    fn as_duas_grandezas_chamadas_jitter_nao_devolvem_uma_a_outra() {
        // As duas são `f64` em milissegundos, e nada num `f64` diz qual dos dois
        // ele é — foi assim que a barra da TUI e o rodapé do app mostraram a
        // reserva do anel sob o rótulo `JITTER`, cada um por sua conta. Os
        // números são escolhidos para que trocar um pelo outro **não possa**
        // passar: uma reserva saudável é alta e um jitter de rede honesto é
        // baixo, que é exatamente por que a troca fazia uma conexão boa parecer
        // ruim.
        let telemetry = AudioTelemetry {
            local: LocalTelemetry::default(),
            sources: vec![
                SourceTelemetry {
                    ssrc: 1,
                    jitter_depth_ms: 42.0,
                    jitter_ms: 3.5,
                    ..SourceTelemetry::default()
                },
                SourceTelemetry {
                    ssrc: 2,
                    jitter_depth_ms: 120.0,
                    jitter_ms: 7.5,
                    ..SourceTelemetry::default()
                },
            ],
        };

        let Some(chegada) = telemetry.worst_arrival_jitter_ms() else {
            panic!("havia duas fontes sendo recebidas e nenhum jitter de chegada saiu");
        };
        assert!(
            (chegada - 7.5).abs() < 1e-9,
            "o pior jitter de chegada era 7,5 ms e saiu {chegada}"
        );
        assert!((telemetry.worst_jitter_depth_ms() - 120.0).abs() < 1e-9);
    }

    #[test]
    fn sem_fonte_nenhuma_nao_ha_jitter_de_chegada_a_dizer() {
        // `None` e não zero: o pior de uma lista vazia seria zero, e zero é o
        // número que este conserto tirou da tela — o relatório do servidor manda
        // `0.0` fixo porque um servidor não tem como medir jitter. Quem chama
        // decide o que fazer com a ausência; as duas cascas escolhem manter o
        // último valor medido.
        assert_eq!(AudioTelemetry::default().worst_arrival_jitter_ms(), None);
    }
}
