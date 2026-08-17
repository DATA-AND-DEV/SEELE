//! Segura o anel de reprodução na profundidade escolhida, reamostrando.
//!
//! O laço de voz produz 48 000 amostras por segundo **de `Instant`**. O
//! dispositivo consome no ritmo do cristal dele. Os dois nunca são o mesmo
//! relógio, e a diferença não some: ela se acumula no anel entre os dois, que é
//! o único lugar onde ela cabe.
//!
//! O anel tem duas paredes e as duas doem:
//!
//! - **O fundo.** O anel esvazia, o retorno de chamada não acha amostra e
//!   inventa silêncio — `playback_underruns` em [`crate::rt::StreamMetrics`].
//! - **O topo.** O anel enche, o laço não consegue empurrar e a amostra é
//!   descartada na entrada — o `anel` do `:sync`.
//!
//! Sem ninguém segurando, o anel **encosta** numa das duas paredes e fica lá:
//! qual delas depende só do sinal do erro dos cristais. Encostado no fundo, a
//! reprodução perde algumas amostras por segundo para sempre; encostado no topo,
//! a latência de reprodução é o anel inteiro, para sempre. É a pendência 2.
//!
//! # A correção é reamostrar, não descartar
//!
//! [`crate::drift`] já escreveu o porquê, para a outra deriva — a do falante do
//! outro lado da rede: descartar ou inserir um quadro custa um estalo de 20 ms
//! de cada vez, e reamostrar por 100 ppm é uma mudança de afinação de 0,002 de
//! um semitom, que nada ouve. [`crate::resample::RateConverter::adjust_ratio`]
//! existe para isto, e
//! [`crate::resample::RateConverter::new_adjustable`] mantém um filtro para
//! guiar mesmo quando os dois lados dizem 48 kHz — que é o caso comum, e é
//! justamente onde a deriva se esconde.
//!
//! # A malha, e por que ela é lenta
//!
//! A lei de controle cabe numa frase: **o erro de profundidade é devolvido ao
//! longo de [`CONTROL_TAU_MS`]**. Se o anel está 10 ms mais fundo do que
//! deveria, a razão pede 10 ms a menos ao longo de trinta segundos — 333 ppm.
//!
//! Ela é proporcional e só proporcional, e isso é uma escolha, não uma
//! simplificação:
//!
//! - **A planta já é um integrador.** A profundidade é a integral da diferença
//!   de taxas. Proporcional sobre integrador dá primeira ordem: aproximação
//!   monótona, sem sobressinal e sem oscilação. Um termo integral acrescentaria
//!   um segundo polo, e dois polos comparáveis é ringing — taxa de amostragem
//!   oscilando é *wow*, audível como variação de tom. **O defeito de hoje é
//!   inaudível; um conserto que introduz wow seria pior que ele.**
//! - **Não há o que enrolar.** A razão é função da profundidade suavizada e de
//!   mais nada, então uma leitura absurda não fica guardada: ela sai sozinha em
//!   [`MEASUREMENT_TAU_MS`]. Um integrador teria que ser desenrolado à mão.
//! - **O erro em regime é inofensivo.** Proporcional puro deixa a profundidade
//!   deslocada em `deriva × τ`: 300 ppm com τ de 30 s são 9 ms, menos que um
//!   quadro. O que ele **não** deixa é erro de *taxa* — em regime a razão vale
//!   exatamente a deriva, que é a única coisa que precisava estar certa.
//!
//! O que a lentidão compra: a perturbação que não pode ser perseguida é a
//! serrilha do próprio laço — 20 ms de quadro empurrados de uma vez, 50 vezes
//! por segundo. Trinta segundos são mil e quinhentas serrilhas. A malha não
//! consegue ver nenhuma delas, e é isso que se quer: deriva de cristal acumula
//! ao longo de minutos, então a correção pode e deve ser lenta.
//!
//! # E tem limite
//!
//! [`MAX_CORRECTION_PPM`] é dez vezes o que um cristal de consumo erra. Uma
//! razão pedida além disso não é deriva — é dispositivo trocado, taxa diferente
//! da anunciada, laço parado — e obedecer a um número absurdo estraga o áudio de
//! vez. Grampeia-se, e conta-se em [`PacingMetrics::clamps`]: o número que
//! cresce é o que diz que a causa não é a que este módulo trata.
//!
//! # O silêncio
//!
//! Duas perguntas diferentes, e as duas têm resposta aqui.
//!
//! Ninguém falando **não** cega esta malha: o laço mistura e empurra um quadro a
//! cada 20 ms mesmo sem falante nenhum, então a profundidade continua sendo
//! medida. É a diferença entre esta deriva e a de [`crate::drift`], que só mede
//! quando chega quadro pela rede e por isso precisa exigir janela cheia.
//!
//! O laço **parado** é outra coisa, e essa cega. Entre duas olhadas separadas
//! por mais de [`STALE_MS`] o anel esvaziou por falta de quem o encha, não por
//! causa de cristal: a leitura é resemeada e a malha recomeça o aquecimento em
//! vez de ler deriva enorme de uma parada. Ver [`PacingMetrics::stalls`].

use std::time::Instant;

use crate::FRAME_MS;

/// Constante de tempo da malha, em milissegundos.
///
/// O erro de profundidade é devolvido ao longo dela. Trinta segundos são três
/// ordens de grandeza acima da serrilha de 20 ms que o laço produz, então a
/// malha não consegue perseguir a serrilha nem sob provocação — e são duas
/// ordens abaixo dos minutos em que a deriva de cristal se acumula, então ela
/// chega lá com folga.
const CONTROL_TAU_MS: f64 = 30_000.0;

/// Constante de tempo do alisamento da medida, em milissegundos.
///
/// A profundidade lida numa volta é um ponto de uma serrilha, não uma média.
/// Alisar é o que transforma uma na outra. Cinco segundos por dois motivos: é
/// duzentas serrilhas, e é um sexto de [`CONTROL_TAU_MS`] — dois polos de
/// tempos parecidos oscilam, e este tem que ficar bem longe do outro.
const MEASUREMENT_TAU_MS: f64 = 5_000.0;

/// Maior correção que será aplicada, em partes por milhão.
///
/// O mesmo teto de [`crate::drift`], pela mesma razão: um cristal de consumo é
/// especificado a algo como ±50 ppm, e dois lados 100 ppm afastados estão os
/// dois dentro da especificação. Dez vezes isso já não é cristal.
pub const MAX_CORRECTION_PPM: f64 = 500.0;

/// Quanto tempo a medida precisa amadurecer antes da primeira correção.
///
/// O anel começa vazio e enche até o alvo; nada nesse trecho é deriva. Cinco
/// segundos são um [`MEASUREMENT_TAU_MS`] inteiro, que é quando a média alisada
/// deixa de ser o valor com que foi semeada.
const WARMUP_MS: f64 = 5_000.0;

/// Intervalo acima do qual duas olhadas não são a mesma medida.
///
/// Meio segundo é vinte e cinco voltas do laço. Se passou mais que isso, o laço
/// não estava empurrando, e o que a profundidade mostra é a parada — não os
/// cristais.
const STALE_MS: f64 = 500.0;

/// Diferença mínima entre duas razões para valer uma aplicação, em ppm.
///
/// Abaixo disto a razão nova é a razão velha com ruído em cima, e mexer no
/// filtro por causa de ruído é a definição de perseguir o próprio erro de
/// medida.
const APPLY_EPSILON_PPM: f64 = 1.0;

/// Quantos retornos de chamada do dispositivo o anel guarda de reserva.
///
/// O retorno de chamada leva o bloco dele **inteiro** de uma vez: com menos que
/// um bloco no anel ele já inventa silêncio, por mais cheio que o anel esteja
/// em média. Um bloco é o mínimo aritmético; o segundo é a folga para a volta
/// do laço atrasar — e a volta do laço atrasa, é a medida da pendência 15.
const TARGET_BURSTS: usize = 2;

/// O que a malha viu e o que ela está fazendo.
///
/// Dados puros, como todo este crate. Quem desenha decide o que "−120 ppm"
/// parece na tela.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PacingMetrics {
    /// Correção em vigor, em partes por milhão do nominal.
    ///
    /// Em regime, isto **é** a deriva medida entre o relógio desta máquina e o
    /// cristal do dispositivo, com o sinal invertido: é quanto se está pedindo
    /// a mais ou a menos para cancelá-la.
    pub ppm: f64,

    /// Profundidade alisada do anel, em milissegundos.
    pub depth_ms: f64,

    /// Profundidade que a malha está segurando, em milissegundos.
    pub target_ms: f64,

    /// Quantas vezes a razão pedida saiu da faixa e foi grampeada.
    ///
    /// **Zero é o normal.** Crescendo, a causa não é deriva de cristal: é taxa
    /// diferente da anunciada, dispositivo trocado, ou um laço que parou de
    /// empurrar. Obedecer àquele número teria estragado o áudio de vez.
    pub clamps: u64,

    /// Quantas vezes o anel foi achado vazio e recebeu silêncio até o alvo.
    ///
    /// Uma no arranque é o normal — o anel começa vazio. Mais que isso é o laço
    /// não acompanhando o relógio, e quem responde a isso é
    /// [`crate::playout::PlayoutMetrics`], não esta malha.
    pub primes: u64,

    /// Quantas vezes o laço passou de [`STALE_MS`] sem empurrar nada.
    pub stalls: u64,

    /// Olhadas dadas, desde o começo.
    pub observations: u64,

    /// Se a malha já está corrigindo, ou ainda aquecendo.
    pub correcting: bool,
}

/// O que fazer com o anel nesta volta.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pacing {
    /// Amostras de silêncio a empurrar **antes** do quadro, para repor a reserva.
    ///
    /// Só é diferente de zero com o anel vazio, e aí o dispositivo já está
    /// inventando silêncio sozinho: o que se insere aqui não custa nada que não
    /// estivesse sendo pago, e o que se evita é subir de zero até o alvo no
    /// passo lento da malha, que leva um minuto de perda contínua.
    pub prime_samples: usize,

    /// Razão nova para
    /// [`crate::resample::RateConverter::adjust_ratio`], quando ela mudou o
    /// bastante para valer a chamada.
    pub ratio: Option<f64>,
}

/// Segura a profundidade do anel de reprodução.
///
/// Um por dispositivo de saída. Não tem relógio próprio, de propósito: quem
/// chama passa o `Instant`, o que torna cada comportamento daqui verificável em
/// tempo simulado, sem dormir e sem placa de som — a mesma escolha de
/// [`crate::playout::PlayoutClock`].
#[derive(Debug)]
pub struct RingPacer {
    rate_hz: u32,
    capacity: usize,
    target: usize,
    smoothed: f64,
    last_seen: Option<Instant>,
    /// Quando a medida atual começou a valer. Reiniciado por parada e reposição.
    settled_at: Option<Instant>,
    applied_ppm: f64,
    metrics: PacingMetrics,
}

impl RingPacer {
    /// Uma malha para um anel de `capacity` amostras a `rate_hz`.
    ///
    /// `rate_hz` é a taxa do **dispositivo**, não a do interior do encanamento:
    /// o anel guarda o que já foi convertido.
    #[must_use]
    pub fn new(rate_hz: u32, capacity: usize) -> Self {
        let mut pacer = Self {
            rate_hz: rate_hz.max(1),
            capacity,
            target: 0,
            smoothed: 0.0,
            last_seen: None,
            settled_at: None,
            applied_ppm: 0.0,
            metrics: PacingMetrics::default(),
        };
        pacer.retarget(0);
        pacer
    }

    /// A reserva que a malha está segurando, em amostras.
    #[must_use]
    pub fn target_samples(&self) -> usize {
        self.target
    }

    /// O que a malha viu.
    #[must_use]
    pub fn metrics(&self) -> PacingMetrics {
        self.metrics
    }

    /// Esquece a medida. Para um dispositivo que foi trocado embaixo do laço.
    pub fn reset(&mut self) {
        let (rate_hz, capacity) = (self.rate_hz, self.capacity);
        *self = Self::new(rate_hz, capacity);
    }

    /// Olha o anel e diz o que fazer.
    ///
    /// Chamada **uma vez por volta em que o laço vai empurrar quadro**, com a
    /// profundidade lida imediatamente antes de empurrar. Antes e não depois
    /// porque o que interessa é o fundo do vale: é lá que o retorno de chamada
    /// fica sem amostra, e uma média que inclua o pico esconde exatamente isso.
    ///
    /// `burst` é o maior bloco que o dispositivo já pediu de uma vez
    /// ([`crate::rt::StreamMetrics::playback_burst_frames`]); zero enquanto
    /// nenhum retorno de chamada tiver rodado ainda.
    pub fn observe(&mut self, depth: usize, burst: usize, now: Instant) -> Pacing {
        self.retarget(burst);
        self.metrics.observations = self.metrics.observations.saturating_add(1);

        let gap_ms = self
            .last_seen
            .map(|last| now.saturating_duration_since(last).as_secs_f64() * 1000.0);
        self.last_seen = Some(now);

        // Primeira olhada, ou olhada depois de o laço ter parado de empurrar. A
        // profundidade conta o que a parada fez, e não o que os cristais fazem.
        let stale = gap_ms.is_none_or(|gap| gap > STALE_MS);
        if stale {
            if gap_ms.is_some() {
                self.metrics.stalls = self.metrics.stalls.saturating_add(1);
            }
            self.reseed(depth_as_f64(depth), now);
        }

        // Anel vazio não é medida, é dispositivo faminto: o retorno de chamada
        // já está inventando silêncio, então o silêncio que entra aqui não custa
        // nada que não estivesse sendo pago.
        if depth == 0 {
            self.metrics.primes = self.metrics.primes.saturating_add(1);
            self.reseed(depth_as_f64(self.target), now);
            self.metrics.depth_ms = self.as_ms(self.smoothed);
            return Pacing {
                prime_samples: self.target,
                ratio: None,
            };
        }

        if let Some(gap) = gap_ms.filter(|_| !stale) {
            let alpha = (gap / MEASUREMENT_TAU_MS).clamp(0.0, 1.0);
            self.smoothed += (depth_as_f64(depth) - self.smoothed) * alpha;
        }
        self.metrics.depth_ms = self.as_ms(self.smoothed);

        let warm = self.settled_at.is_some_and(|at| {
            now.saturating_duration_since(at).as_secs_f64() * 1000.0 >= WARMUP_MS
        });
        self.metrics.correcting = warm;
        if !warm {
            return Pacing::default();
        }

        // A lei inteira: devolver o erro ao longo de uma constante de tempo.
        let error_seconds = (self.smoothed - depth_as_f64(self.target)) / f64::from(self.rate_hz);
        let wanted_ppm = -error_seconds / (CONTROL_TAU_MS / 1000.0) * 1e6;
        let ppm = wanted_ppm.clamp(-MAX_CORRECTION_PPM, MAX_CORRECTION_PPM);
        if (ppm - wanted_ppm).abs() > f64::EPSILON {
            self.metrics.clamps = self.metrics.clamps.saturating_add(1);
        }
        self.metrics.ppm = ppm;

        if (ppm - self.applied_ppm).abs() < APPLY_EPSILON_PPM {
            return Pacing::default();
        }
        self.applied_ppm = ppm;
        Pacing {
            prime_samples: 0,
            ratio: Some(1.0 + ppm / 1e6),
        }
    }

    /// Recomeça a medida a partir de um valor conhecido.
    fn reseed(&mut self, depth: f64, now: Instant) {
        self.smoothed = depth;
        self.settled_at = Some(now);
        self.metrics.correcting = false;
    }

    /// A reserva, que é uma propriedade do dispositivo e não um palpite.
    ///
    /// Dois blocos do dispositivo, com dois limites: nunca menos que um quadro,
    /// que é o passo com que o laço enche o anel, e nunca mais que metade do
    /// anel, porque o que não cabe no anel não é reserva.
    fn retarget(&mut self, burst: usize) {
        let frame = (self.rate_hz as usize).saturating_mul(FRAME_MS as usize) / 1000;
        let ceiling = (self.capacity / 2).max(1);
        self.target = burst
            .saturating_mul(TARGET_BURSTS)
            .max(frame)
            .min(ceiling)
            .max(1);
        self.metrics.target_ms = self.as_ms(depth_as_f64(self.target));
    }

    fn as_ms(&self, samples: f64) -> f64 {
        samples / f64::from(self.rate_hz) * 1000.0
    }
}

/// Amostras como número com vírgula. Um anel não chega perto do limite de `f64`.
fn depth_as_f64(samples: usize) -> f64 {
    samples as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Um anel e um dispositivo, em tempo simulado.
    ///
    /// Modela as três coisas que fazem a pendência 2 existir: o laço empurra
    /// quadros no ritmo do `Instant`, o dispositivo leva blocos inteiros no
    /// ritmo do cristal **dele**, e o anel entre os dois tem fundo e tem topo.
    /// Sem o bloco inteiro não haveria falta a medir — é com meio bloco no anel
    /// que o retorno de chamada inventa silêncio.
    struct Machine {
        /// Erro do cristal do dispositivo, em ppm. Positivo é dispositivo rápido.
        drift_ppm: f64,
        nominal_hz: f64,
        burst: usize,
        capacity: usize,
        depth: usize,
        ratio: f64,
        /// Fração de amostra que o dispositivo deve levar e ainda não completou.
        device_owed: f64,
        /// Fração de amostra que o laço deve produzir e ainda não completou.
        produced_owed: f64,
        underruns: u64,
        overflows: u64,
        pushed: u64,
    }

    const RATE: u32 = 48_000;
    /// 100 ms, o `RING_MS` do `seele-core`.
    const CAPACITY: usize = 4_800;
    /// O bloco medido neste Mac em M1.1.
    const BURST: usize = 512;

    impl Machine {
        fn new(drift_ppm: f64) -> Self {
            Self {
                drift_ppm,
                nominal_hz: f64::from(RATE),
                burst: BURST,
                capacity: CAPACITY,
                depth: 0,
                ratio: 1.0,
                device_owed: 0.0,
                produced_owed: 0.0,
                underruns: 0,
                overflows: 0,
                pushed: 0,
            }
        }

        /// O dispositivo consome `dt` de tempo, em blocos inteiros.
        fn consume(&mut self, dt_ms: f64) {
            self.device_owed += self.nominal_hz * (1.0 + self.drift_ppm / 1e6) * dt_ms / 1000.0;
            while self.device_owed >= self.burst as f64 {
                self.device_owed -= self.burst as f64;
                let served = self.depth.min(self.burst);
                self.underruns += (self.burst - served) as u64;
                self.depth -= served;
            }
        }

        /// O laço empurra `frames` quadros, já convertidos pela razão em vigor.
        fn push_frames(&mut self, frames: u32) {
            let per_frame = f64::from(RATE) * f64::from(FRAME_MS) / 1000.0;
            self.produced_owed += f64::from(frames) * per_frame * self.ratio;
            let whole = self.produced_owed.floor();
            self.produced_owed -= whole;
            self.push(whole as usize);
        }

        fn push(&mut self, samples: usize) {
            let room = self.capacity - self.depth;
            let taken = samples.min(room);
            self.overflows += (samples - taken) as u64;
            self.depth += taken;
            self.pushed += taken as u64;
        }
    }

    /// Como uma corrida foi conduzida.
    struct Run {
        seconds: u64,
        /// Duração da volta do laço, em milissegundos.
        turn_ms: f64,
        corrected: bool,
    }

    impl Run {
        fn new(seconds: u64) -> Self {
            Self {
                seconds,
                turn_ms: 5.0,
                corrected: true,
            }
        }

        fn uncorrected(mut self) -> Self {
            self.corrected = false;
            self
        }
    }

    /// O que uma corrida deixou para trás, amostrado a cada segundo.
    struct Trace {
        underruns: Vec<u64>,
        overflows: Vec<u64>,
        depth_ms: Vec<f64>,
        ppm: Vec<f64>,
        metrics: PacingMetrics,
        target: usize,
    }

    impl Trace {
        /// Falta acumulada entre dois instantes, em segundos.
        fn lost_between(&self, from: usize, to: usize) -> u64 {
            let (a, b) = (
                self.underruns.get(from).copied().unwrap_or(0),
                self.underruns.get(to).copied().unwrap_or(0),
            );
            b.saturating_sub(a) + {
                let (c, d) = (
                    self.overflows.get(from).copied().unwrap_or(0),
                    self.overflows.get(to).copied().unwrap_or(0),
                );
                d.saturating_sub(c)
            }
        }
    }

    /// Roda o laço de voz contra um dispositivo, com ou sem a malha.
    ///
    /// A forma é a do laço de verdade: a cada volta pergunta quantos quadros
    /// venceram pelo relógio, olha o anel **antes** de empurrar, e empurra.
    fn run(machine: &mut Machine, plan: &Run, mut perturb: impl FnMut(u64, &mut Machine)) -> Trace {
        let start = Instant::now();
        let mut pacer = RingPacer::new(RATE, machine.capacity);
        let mut trace = Trace {
            underruns: Vec::new(),
            overflows: Vec::new(),
            depth_ms: Vec::new(),
            ppm: Vec::new(),
            metrics: PacingMetrics::default(),
            target: 0,
        };

        let mut elapsed_ms = 0.0_f64;
        let mut next_frame_ms = 0.0_f64;
        let mut next_sample_ms = 0.0_f64;
        let mut second = 0_u64;

        while elapsed_ms < (plan.seconds as f64) * 1000.0 {
            machine.consume(plan.turn_ms);
            elapsed_ms += plan.turn_ms;
            let now = start + Duration::from_nanos((elapsed_ms * 1e6) as u64);

            // Quantos quadros venceram, como o `PlayoutClock` responde.
            let mut due = 0_u32;
            while next_frame_ms <= elapsed_ms {
                next_frame_ms += f64::from(FRAME_MS);
                due += 1;
            }
            if due > 0 {
                if plan.corrected {
                    let pacing = pacer.observe(machine.depth, machine.burst, now);
                    machine.push(pacing.prime_samples);
                    if let Some(ratio) = pacing.ratio {
                        machine.ratio = ratio;
                    }
                }
                machine.push_frames(due);
            }

            perturb(second, machine);

            if elapsed_ms >= next_sample_ms {
                next_sample_ms += 1000.0;
                second += 1;
                trace.underruns.push(machine.underruns);
                trace.overflows.push(machine.overflows);
                trace.depth_ms.push(pacer.metrics().depth_ms);
                trace.ppm.push(pacer.metrics().ppm);
            }
        }

        trace.metrics = pacer.metrics();
        trace.target = pacer.target_samples();
        trace
    }

    /// Uma corrida sem perturbação nenhuma.
    fn plain(drift_ppm: f64, plan: &Run) -> Trace {
        let mut machine = Machine::new(drift_ppm);
        run(&mut machine, plan, |_, _| {})
    }

    #[test]
    fn sem_a_malha_o_anel_encosta_na_parede_e_nao_sai_mais() {
        // **O defeito, em número, e a prova de que o simulador o reproduz.**
        // Um teste da correção que passasse aqui também estaria medindo nada.
        //
        // 200 ppm de cristal rápido esvaziam o anel e a falta não para mais; 200
        // ppm de cristal lento enchem o anel e o descarte não para mais. Qual das
        // duas paredes depende só do sinal, e as duas são a pendência 2.
        let rapido = plain(200.0, &Run::new(600).uncorrected());
        assert!(
            rapido.lost_between(540, 599) > 500,
            "o cristal rápido devia estar perdendo amostra no décimo minuto, perdeu {}",
            rapido.lost_between(540, 599)
        );

        let lento = plain(-200.0, &Run::new(600).uncorrected());
        assert!(
            lento.lost_between(540, 599) > 500,
            "o cristal lento devia estar descartando na entrada do anel, descartou {}",
            lento.lost_between(540, 599)
        );
    }

    #[test]
    fn a_malha_para_a_perda_e_ela_nao_volta() {
        // O critério da pendência 2: o contador **para de crescer** e nada novo
        // cresce no lugar. Um minuto inteiro no fim de dez, sem uma amostra.
        for drift_ppm in [0.0, 50.0, 200.0, -50.0, -200.0, 400.0] {
            let trace = plain(drift_ppm, &Run::new(600));
            assert_eq!(
                trace.lost_between(540, 599),
                0,
                "{drift_ppm} ppm ainda perdia no décimo minuto \
                 (anel {:.1} ms, alvo {:.1} ms, {:+.0} ppm)",
                trace.metrics.depth_ms,
                trace.metrics.target_ms,
                trace.metrics.ppm
            );
        }
    }

    #[test]
    fn a_razao_converge_para_a_deriva_do_cristal() {
        // Em regime a correção **é** a deriva, com o sinal trocado. Errar o sinal
        // dobraria a deriva em vez de cancelá-la, e o sintoma seria idêntico ao
        // de não haver correção nenhuma — que é como esta classe de defeito
        // sobrevive a uma revisão.
        for drift_ppm in [100.0, 250.0, -100.0, -250.0] {
            let trace = plain(drift_ppm, &Run::new(600));
            let ppm = trace.metrics.ppm;
            assert!(
                (ppm + drift_ppm).abs() < 30.0,
                "{drift_ppm} ppm de cristal deviam pedir {:+.0} ppm de correção, pediram {ppm:+.0}",
                -drift_ppm
            );
        }
    }

    #[test]
    fn a_correcao_e_pequena_demais_para_ser_ouvida() {
        // Reamostrar por 250 ppm é 0,004 de um semitom. O que seria audível é a
        // razão **se mexendo** depressa, e o que este teste trava é a velocidade
        // dela: nada que se mova menos que alguns ppm por segundo produz wow.
        let trace = plain(250.0, &Run::new(600));
        let maior_passo = trace
            .ppm
            .windows(2)
            .map(|par| match par {
                [antes, depois] => (depois - antes).abs(),
                _ => 0.0,
            })
            .fold(0.0, f64::max);
        assert!(
            maior_passo < 40.0,
            "a razão andou {maior_passo:.0} ppm num segundo, o que é rápido demais para uma \
             deriva que leva minutos"
        );
    }

    #[test]
    fn empurrada_para_fora_do_alvo_ela_volta_sem_oscilar() {
        // **A regressão que ninguém pediu.** Uma malha mal amortecida corrige
        // demais: a taxa sobe, o anel esvazia, a taxa desce, o anel enche — e
        // oscilação de taxa de amostragem é audível como variação de tom, num
        // defeito que era inaudível.
        //
        // O contador é o cruzamento de zero do erro: uma resposta amortecida
        // atravessa o alvo no máximo uma vez. Uma que oscila atravessa muitas.
        let mut machine = Machine::new(100.0);
        let trace = run(&mut machine, &Run::new(900), |second, machine| {
            // No terceiro minuto, meio anel de áudio de uma vez — o tamanho de
            // um susto do escalonador, e bem maior que qualquer serrilha.
            if second == 180 {
                machine.push(2_000);
            }
        });

        let depois: Vec<f64> = trace
            .depth_ms
            .iter()
            .skip(190)
            .map(|depth| depth - trace.metrics.target_ms)
            .collect();
        let cruzamentos = depois
            .windows(2)
            .filter(|par| match par {
                [antes, depois] => antes.signum() != depois.signum(),
                _ => false,
            })
            .count();
        assert!(
            cruzamentos <= 1,
            "o erro atravessou o alvo {cruzamentos} vezes depois do empurrão — isto é oscilação, \
             e oscilação de taxa é wow"
        );
        assert_eq!(
            trace.lost_between(190, 899),
            0,
            "o empurrão não podia custar amostra nenhuma"
        );
    }

    #[test]
    fn o_absurdo_e_grampeado_e_contado() {
        // Um dispositivo 5% fora não tem cristal ruim: tem taxa diferente da
        // anunciada, ou foi trocado embaixo do laço. Obedecer ao número que a
        // profundidade pede ali estragaria o áudio de vez — e o conserto certo é
        // outro, então o que este módulo deve fazer é grampear e **contar**.
        let trace = plain(50_000.0, &Run::new(300));

        assert!(
            trace.metrics.clamps > 0,
            "grampeou e não contou: o contador é o único aviso de que a causa não é deriva"
        );
        for ppm in &trace.ppm {
            assert!(
                ppm.abs() <= MAX_CORRECTION_PPM,
                "pediu {ppm:+.0} ppm, além do teto de {MAX_CORRECTION_PPM:.0}"
            );
        }
    }

    #[test]
    fn no_silencio_a_malha_continua_medindo() {
        // A pergunta que derruba uma malha ingênua: sem ninguém falando não
        // chega quadro nenhum da rede. Aqui isso não cega nada — o laço mistura
        // e empurra silêncio no mesmo compasso —, e é por isso que a medida é a
        // profundidade do anel e não a chegada de quadro.
        //
        // O simulador nunca teve falante: tudo acima já é silêncio. O que este
        // teste fixa é que a malha corrige mesmo assim.
        let trace = plain(150.0, &Run::new(600));
        assert!(
            trace.metrics.correcting,
            "a malha desistiu de corrigir com o encanamento em silêncio"
        );
        assert!(
            (trace.metrics.ppm + 150.0).abs() < 30.0,
            "no silêncio a correção virou {:+.0} ppm",
            trace.metrics.ppm
        );
    }

    #[test]
    fn um_laco_parado_nao_e_lido_como_deriva() {
        // O laço parar é a outra pergunta do silêncio, e essa cega mesmo: o anel
        // esvazia porque ninguém o encheu. Ler aquilo como deriva grampearia a
        // razão por minutos por causa de uma parada de dois segundos.
        let start = Instant::now();
        let mut pacer = RingPacer::new(RATE, CAPACITY);

        // Dez segundos de compasso normal, com o anel no alvo.
        let alvo = pacer.target_samples();
        let mut at_ms = 0.0_f64;
        while at_ms < 10_000.0 {
            pacer.observe(alvo, BURST, start + Duration::from_millis(at_ms as u64));
            at_ms += 20.0;
        }
        let antes = pacer.metrics().ppm;

        // O laço some por dois segundos e volta com o anel raspando.
        at_ms += 2_000.0;
        let volta = pacer.observe(64, BURST, start + Duration::from_millis(at_ms as u64));

        assert_eq!(pacer.metrics().stalls, 1, "a parada não foi notada");
        assert_eq!(
            volta.ratio, None,
            "a malha corrigiu em cima de uma parada, que é justamente o que ela não pode fazer"
        );
        assert_eq!(
            pacer.metrics().ppm,
            antes,
            "a razão se mexeu por causa de uma parada"
        );
        assert!(!pacer.metrics().correcting, "voltou sem reaquecer");
    }

    #[test]
    fn o_anel_vazio_recebe_silencio_em_vez_de_um_minuto_de_rampa() {
        // Subir de zero até o alvo no passo da malha custa 500 ppm durante um
        // minuto — um minuto de perda contínua, que é o defeito. Com o anel
        // vazio o dispositivo já está inventando silêncio; o que entra aqui não
        // custa nada que não estivesse sendo pago.
        let mut pacer = RingPacer::new(RATE, CAPACITY);
        let primeira = pacer.observe(0, BURST, Instant::now());

        assert_eq!(
            primeira.prime_samples,
            pacer.target_samples(),
            "o anel vazio tinha que subir até o alvo de uma vez"
        );
        assert_eq!(primeira.ratio, None, "não se corrige em cima de anel vazio");
        assert_eq!(pacer.metrics().primes, 1);
    }

    #[test]
    fn o_alvo_vem_do_bloco_do_dispositivo() {
        // O retorno de chamada leva o bloco inteiro de uma vez: com menos que um
        // bloco no anel ele inventa silêncio por mais cheio que o anel esteja em
        // média. O alvo é uma propriedade do dispositivo, e não um palpite em
        // milissegundos que estaria errado nas duas pontas — folgado num
        // dispositivo de 128 quadros, apertado num de 2048.
        let mut pacer = RingPacer::new(RATE, CAPACITY);
        pacer.observe(1, 512, Instant::now());
        assert_eq!(pacer.target_samples(), 1_024);

        // Um bloco pequeno não deixa o alvo abaixo do passo com que o laço
        // enche o anel: um quadro é o mínimo.
        let mut miudo = RingPacer::new(RATE, CAPACITY);
        miudo.observe(1, 128, Instant::now());
        assert_eq!(miudo.target_samples(), 960);

        // E um bloco enorme não pede mais reserva do que o anel tem.
        let mut enorme = RingPacer::new(RATE, CAPACITY);
        enorme.observe(1, 8_192, Instant::now());
        assert_eq!(enorme.target_samples(), CAPACITY / 2);
    }

    #[test]
    fn nada_e_corrigido_durante_o_aquecimento() {
        // O anel começa vazio e enche até o alvo. Nada nesse trecho é deriva, e
        // corrigir ali seria corrigir o arranque.
        let start = Instant::now();
        let mut pacer = RingPacer::new(RATE, CAPACITY);
        let alvo = pacer.target_samples();

        let mut at_ms = 0.0_f64;
        while at_ms < WARMUP_MS - 20.0 {
            let pacing = pacer.observe(alvo / 4, BURST, start + Duration::from_millis(at_ms as u64));
            assert_eq!(
                pacing.ratio, None,
                "corrigiu {at_ms} ms depois de abrir, ainda dentro do aquecimento"
            );
            at_ms += 20.0;
        }
        assert!(!pacer.metrics().correcting);
    }

    #[test]
    fn um_relogio_perfeito_nao_inventa_deriva() {
        // O contrário do defeito: uma malha que mexe na razão sem ter o que
        // corrigir produz wow de graça.
        let trace = plain(0.0, &Run::new(600));
        assert!(
            trace.metrics.ppm.abs() < 10.0,
            "inventou {:+.1} ppm de deriva num relógio perfeito",
            trace.metrics.ppm
        );
        assert_eq!(trace.metrics.clamps, 0, "grampeou sem ter o que corrigir");
    }

    #[test]
    fn o_deslocamento_em_regime_cabe_dentro_de_um_quadro() {
        // O preço do proporcional puro, e a conta que o torna aceitável: a
        // profundidade em regime fica `deriva × τ` acima do alvo. 400 ppm com τ
        // de 30 s são 12 ms, menos que um quadro — longe das duas paredes.
        let trace = plain(400.0, &Run::new(900));
        let deslocamento = trace.metrics.depth_ms - trace.metrics.target_ms;
        assert!(
            deslocamento.abs() < f64::from(FRAME_MS),
            "o anel ficou {deslocamento:.1} ms fora do alvo em regime"
        );
    }
}
