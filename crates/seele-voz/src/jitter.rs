//! Adaptive jitter buffer, one per source.
//!
//! `specs/03-audio.md` calls this "the component that separates *works* from
//! *works well*", and specifies it as
//!
//! > a pure, deterministic module, testable without real audio: a sequence of
//! > frames with timestamps goes in, a sequence of decisions comes out.
//!
//! That shape is load-bearing, not stylistic. **Nothing here decodes Opus.** The
//! buffer emits [`Decision`]s and the caller runs the codec, which is why the
//! whole thing can be property-tested against synthetic networks
//! ([`crate::netsim`]) with no sound card and no libopus in the loop.
//!
//! # What it does
//!
//! - Reorders frames back into sequence, within the delay it is holding.
//! - Estimates arrival jitter and adapts how much delay to hold, between the
//!   20 ms and 200 ms bounds in `specs/03-audio.md`.
//! - Grows the target fast and shrinks it slowly — "intentional asymmetry,
//!   because cutting audio is worse than delaying it".
//! - Asks for one frame of Opus PLC on a gap, then silence, per `specs/03`.
//! - Discards frames that arrive after their moment has passed, and counts them
//!   as loss rather than pretending they were useful.
//!
//! # Telling silence apart from loss
//!
//! With DTX and voice activation on (`specs/03-audio.md`), a sender puts nothing
//! on the wire while nobody is speaking. A buffer that reads every gap as loss
//! reports heavy loss whenever the room is quiet — and `specs/07-estetica.md`
//! puts loss on screen as the product's signature metric, so that is not a
//! cosmetic bug. This was gap G5 in `docs/plano-m0-m1.md`.
//!
//! `specs/02-protocolo.md` already contains the answer, in the channel saying the
//! timestamp "detects silence gaps". The two counters move differently:
//!
//! - `seq` increments once per **transmitted** packet.
//! - `timestamp` advances by the samples that **elapsed**.
//!
//! So a gap whose `seq` is contiguous is a talker who stopped, and a gap that
//! skips sequence numbers is the network. Both can happen at once, and the
//! arithmetic separates them exactly: the timestamp gap gives the total slots to
//! fill, the sequence gap gives how many of those were lost, and the remainder
//! is silence.
//!
//! This is also why **playout is indexed by timestamp, not by sequence**.
//! Playing consecutive sequence numbers back to back would compress a two-second
//! silence into 20 ms.
//!
//! # Sequence wrap
//!
//! `specs/02-protocolo.md` gives the media header a 16-bit sequence number,
//! which wraps every 21 minutes at 50 frames per second. Every comparison here
//! goes through [`seq_delta`], never through `<`. A call longer than 21 minutes
//! is entirely ordinary and a naive comparison would break exactly once per
//! wrap, in a way that looks like a random glitch.

use std::collections::VecDeque;

/// Frames the buffer can hold at once.
///
/// 256 frames is 5.12 s at 20 ms — far beyond the 200 ms ceiling in
/// `specs/03-audio.md`, so the capacity never binds in normal operation. It
/// exists to bound memory against a sender that floods, which
/// `specs/04-servidor-seele.md` also guards against on its side.
const CAPACITY: usize = 256;

/// Smoothing factor for the arrival-jitter estimate.
///
/// RFC 3550's interarrival jitter, which uses 1/16. Keeping the standard value
/// means the number is comparable with every other VoIP tool an operator might
/// hold it next to.
const JITTER_SMOOTHING: f64 = 1.0 / 16.0;

/// Acima disto, uma diferença de trânsito entre dois quadros seguidos é salto
/// do relógio de quem manda, e não oscilação da rede. Ver `update_jitter`.
const DESCONTINUIDADE_DE_RELOGIO_MS: f64 = 500.0;

/// How many jitter estimates of headroom the target aims to hold.
///
/// Three covers the great majority of a roughly normal arrival distribution.
/// Higher is safer and slower; `specs/00-visao-geral.md` is spending this
/// directly out of the latency budget.
const JITTER_HEADROOM: f64 = 3.0;

/// Fraction of the gap the target closes per frame while shrinking.
///
/// `specs/03-audio.md` asks for fast growth and slow shrink. At 50 frames per
/// second this closes ~10% of the gap per second, so a buffer that ballooned to
/// 200 ms takes roughly 20 s to come back to 20 ms once the network settles.
/// Slow enough that a second disturbance finds the buffer still deep.
const SHRINK_PER_FRAME: f64 = 0.002;

/// Gap size beyond which the buffer jumps rather than working through it.
///
/// Silence is walked slot by slot, one per playout tick, which is correct and
/// costs nothing: a ten-second silence is ten real seconds of ticks. This bound
/// exists for the other case — a receiver that fell far behind, or a sender that
/// jumped its clock. 3000 frames is a minute.
const MAX_GAP_FRAMES: u32 = 3_000;

/// Tiques de uma janela de medida do excesso, no regime de deriva: 1 s.
///
/// **A fala contínua não tem pausa para drenar.** Com DTX, o excesso da fila
/// sai sozinho no resto de uma pausa de conforto (ver `drain_gap`). Sem pausa —
/// alguém lendo, narrando, tocando —, o relógio de quem fala adiantado uns
/// ppm empurra a fila para cima a cada quadro e nada a traz de volta: medido
/// com este buffer e o `PlayoutClock` reais, +300 ppm eram 560 ms de atraso no
/// trigésimo minuto (`seele-audio/tests/atraso_na_fala_continua.rs`).
///
/// O que se mede é o **piso** da fila ao longo da janela, e não a média: o
/// jitter só faz a fila oscilar para cima e para baixo de um piso, e um piso
/// que passou um quadro inteiro do alvo durante um segundo inteiro é um quadro
/// que nenhuma chegada precisou. Tirá-lo não aproxima a fila da fome mais do
/// que o alvo já permite.
///
/// Uma emenda por segundo, no máximo, é 2% do tempo — quarenta vezes o que a
/// deriva de 500 ppm (o teto de `drift.rs`) exige.
const JANELA_DE_DERIVA_TICKS: u32 = 50;

/// Tiques de uma janela quando o excesso é grande: 200 ms.
///
/// Depois de uma pausa da máquina — meio segundo sem ler nem tocar —, a fila
/// volta com o meio segundo inteiro, e a uma emenda por segundo isso levaria
/// meio minuto para sair. Aqui o buffer tira um quadro a cada dez, nunca dois
/// mais perto que nove quadros intactos: 10% de compressão por alguns
/// segundos, em vez de 460 ms de atraso para sempre (o que a prova mediu) ou de
/// um salto que apagasse meio segundo de palavras de uma vez.
const JANELA_DE_RECUPERACAO_TICKS: u32 = 10;

/// Excesso acima do qual o buffer passa para a janela de recuperação: 3 quadros.
///
/// A deriva sozinha nunca chega aqui: ela soma um quadro em 40 s ou mais (500
/// ppm), e a janela de deriva o tira assim que ele aparece, então o excesso dela
/// fica entre um e dois quadros. Três ou mais é um degrau — uma pausa da
/// máquina, uma rajada — ou o alvo descendo depois de uma rede ruim, e nos dois
/// casos esperar um segundo por quadro é atraso que se ouve.
const EXCESSO_DE_RECUPERACAO_QUADROS: f64 = 3.0;

/// Folga na comparação do excesso com um quadro, em ms.
///
/// O alvo desce geometricamente e, em ponto flutuante, nunca chega exatamente
/// ao piso: fica em 20,000…01 ms para sempre. Sem folga, uma fila de exatamente
/// alvo + um quadro nunca contava como excesso, e o regime assentava um quadro
/// inteiro acima do que precisava (medido: 40–60 ms em vez de 20–40). Um
/// milissegundo abaixo do alvo, depois da emenda, está dentro da folga de três
/// jitters que o alvo já carrega.
const FOLGA_DO_EXCESSO_MS: f64 = 1.0;

/// Excesso sobre o alvo acima do qual a fila é uma rajada, e sai de uma vez: 500 ms.
///
/// A deriva de quem fala soma um quadro em 40 s ou mais e fica entre um e dois
/// de excesso; a compressão de recuperação age a partir de três; o alvo vai até
/// 200 ms. Meio segundo de excesso não é nenhum desses: é a fila de uma pausa
/// desta máquina lida de uma vez. Ver `cortar_a_rajada`.
const EXCESSO_DE_RAJADA_MS: f64 = 500.0;

/// Quanto uma emenda de deriva espera por uma pausa que a pague: 6 s.
///
/// Uma emenda na fala é um estalo leve; a mesma sobra paga numa pausa de quem
/// fala não se ouve (ver `plan_gap`). Numa conversa, alguém respira a cada
/// poucos segundos, e esperar seis custa um quadro de latência (20 ms) por seis
/// segundos. Medido com fala de 5 s e pausa de 1 s a +300 ppm: com 3 s de
/// espera ainda saíam 3 emendas em 30 min; com 6, nenhuma. Na fala contínua a
/// pausa não vem, e a emenda sai depois disso.
///
/// Só um quadro de excesso espera, que é a assinatura da deriva: ela soma um
/// quadro em 40 s ou mais. Dois ou mais é um degrau ou um alvo descendo, e ali
/// esperar é atraso que se ouve.
const ADIAMENTO_DA_EMENDA_TICKS: u32 = 300;

/// Wrap-aware difference between two sequence numbers.
///
/// Returns how far `a` is ahead of `b`, treating the 16-bit space as circular.
/// Positive means `a` is newer.
#[must_use]
pub fn seq_delta(a: u16, b: u16) -> i32 {
    i32::from(a.wrapping_sub(b) as i16)
}

/// Wrap-aware difference between two RTP timestamps, in samples.
///
/// The timestamp is 32 bits of 48 kHz samples, so it wraps after about 24.8
/// hours. Rare, but a session that long is not forbidden and the failure would
/// be a single inexplicable glitch.
#[must_use]
pub fn ts_delta(a: u32, b: u32) -> i64 {
    i64::from(a.wrapping_sub(b) as i32)
}

/// What the buffer wants the caller to do for one 20 ms slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision<T> {
    /// Decode and play this payload.
    Play(T),
    /// The frame is missing. Ask the decoder to conceal it — one frame of Opus
    /// PLC, per `specs/03-audio.md`.
    Conceal,
    /// The gap has outlasted concealment. Emit silence, faded rather than cut.
    Silence,
    /// The talker was silent for this slot — DTX or voice activation, not loss.
    ///
    /// Sounds identical to [`Self::Silence`], but must never be counted as loss:
    /// see the module docs.
    Comfort,
    /// Not enough depth yet to start, or nothing to play. The caller should emit
    /// silence without counting it as loss.
    Starved,
}

/// Tunables, all from `specs/03-audio.md`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JitterConfig {
    /// Where the target starts. `specs/03-audio.md`: 40 ms, two frames.
    pub initial_target_ms: f64,
    /// Lower bound on the adaptive target.
    pub min_target_ms: f64,
    /// Upper bound on the adaptive target.
    pub max_target_ms: f64,
    /// Frame duration, in milliseconds.
    pub frame_ms: f64,
}

impl Default for JitterConfig {
    fn default() -> Self {
        Self {
            initial_target_ms: 40.0,
            min_target_ms: 20.0,
            max_target_ms: 200.0,
            frame_ms: f64::from(crate::FRAME_MS),
        }
    }
}

/// Everything the buffer knows about itself, as plain data.
///
/// `specs/03-audio.md` lists exactly these and says they feed the Sync Ratio.
/// No formatting, no colour, no bands — those are the shell's job
/// (`specs/01-arquitetura.md`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JitterMetrics {
    /// Frames handed to the buffer, including ones it later discarded.
    pub frames_received: u64,
    /// Frames played out.
    pub frames_played: u64,
    /// Slots where a frame was missing and concealment was requested.
    pub frames_concealed: u64,
    /// Slots filled with silence because the gap outlasted concealment.
    pub frames_silenced: u64,
    /// Slots the talker was simply silent for. **Not loss.**
    pub frames_comfort: u64,
    /// Frames that arrived after their playout slot had passed.
    pub late_discards: u64,
    /// Frames dropped because the buffer was already full.
    pub overflow_discards: u64,
    /// Duplicate sequence numbers seen and ignored.
    pub duplicates: u64,
    /// Times the buffer gave up on a gap and resynchronised.
    pub resyncs: u64,
    /// Quadros tirados da fala porque a fila passou do alvo e não voltava.
    ///
    /// **É a deriva de relógio de quem fala, ou uma pausa desta máquina, sendo
    /// paga.** Cada um é uma emenda de 20 ms no áudio. Zero numa conversa com
    /// pausas é o normal (as pausas pagam de graça); um a cada minuto ou dois
    /// numa fala contínua é deriva de dezenas a centenas de ppm; uma rajada
    /// seguida é a recuperação de uma pausa. Não é perda: nenhum desses
    /// quadros faltou, eles chegaram e sobravam.
    pub descartes_por_atraso: u64,
    /// Quadros de pausa de quem fala que não tocaram, porque eram atraso.
    ///
    /// O mesmo excesso de [`Self::descartes_por_atraso`], pago no lugar barato:
    /// a pausa sai 20 ms mais curta, e ninguém ouve. Numa conversa com pausas,
    /// a deriva aparece aqui e não lá.
    pub pausa_encurtada_quadros: u64,
    /// Lugares de buraco (perda ou pausa) pulados sem tocar, porque já eram
    /// tempo passado: a fila estava no teto, ou passava do alvo por mais de
    /// meio segundo (ver `drain_gap` e `cortar_a_rajada`).
    ///
    /// Zero em qualquer regime normal; diferente de zero só depois de uma pausa
    /// desta máquina, ou de uma voz que abriu com segundos de fila. Não entra
    /// em [`Self::loss_fraction`], que conta lugares tocados, e estes não
    /// tocaram como nada; nem separa perda de pausa de quem fala.
    pub buraco_pulado: u64,
    /// Quadros velhos tirados de uma vez da frente, porque a fila passou do
    /// alvo por mais de meio segundo (ver `cortar_a_rajada`).
    ///
    /// Zero em qualquer regime normal; diferente de zero depois de uma pausa
    /// desta máquina que devolveu segundos de voz de uma vez. Não é perda: os
    /// quadros chegaram, e saíram por serem velhos.
    pub descartes_por_rajada: u64,
    /// Current target depth.
    pub target_ms: f64,
    /// Current actual depth.
    pub depth_ms: f64,
    /// Smoothed arrival jitter, RFC 3550 style.
    pub jitter_ms: f64,
}

impl JitterMetrics {
    /// Fraction of expected slots that could not be played from a real frame.
    ///
    /// This is what `specs/02-protocolo.md` feeds into the loss penalty of the
    /// Sync Ratio. Concealed and silenced slots count, because from the
    /// listener's side those are indistinguishable from loss.
    ///
    /// [`JitterMetrics::frames_comfort`] deliberately does **not** count, and is
    /// excluded from the denominator too. A quiet room is not a bad connection,
    /// and reporting it as one would make the signature metric of
    /// `specs/07-estetica.md` say the opposite of the truth.
    #[must_use]
    pub fn loss_fraction(&self) -> f64 {
        let slots = self.frames_played + self.frames_concealed + self.frames_silenced;
        if slots == 0 {
            return 0.0;
        }
        (self.frames_concealed + self.frames_silenced) as f64 / slots as f64
    }
}

/// One frame waiting its turn.
#[derive(Debug, Clone)]
struct Held<T> {
    seq: u16,
    timestamp: u32,
    payload: T,
}

/// An adaptive jitter buffer for a single source.
#[derive(Debug)]
pub struct JitterBuffer<T> {
    config: JitterConfig,
    queue: VecDeque<Held<T>>,
    /// Timestamp the next [`Self::tick`] will try to play. `None` until playout
    /// starts. This, not the sequence number, is the playout timeline — see the
    /// module docs.
    next_timestamp: Option<u32>,
    /// Sequence of the last frame actually played, for loss arithmetic.
    last_played_seq: Option<u16>,
    /// Loss slots still owed for the gap being worked through.
    gap_lost_left: u64,
    /// Silence slots still owed for the same gap.
    gap_comfort_left: u64,
    /// True once enough depth accumulated to begin playing.
    playing: bool,
    target_ms: f64,
    jitter_ms: f64,
    /// Previous frame's transit time, for the RFC 3550 jitter estimate.
    last_transit_ms: Option<f64>,
    /// Consecutive missing slots, for the conceal-once-then-silence rule.
    consecutive_missing: u32,
    /// Ticks spent starving since the last frame actually played.
    ///
    /// The gap a hole leaves is only discovered when the frame after it
    /// arrives, and by then part of it may already have gone out as nothing.
    /// See `plan_gap`.
    starved_since_play: u64,
    /// Menor profundidade vista na janela de excesso corrente, em ms.
    piso_da_janela_ms: f64,
    /// Tiques contados na janela de excesso corrente.
    ticks_da_janela: u32,
    /// Duração da janela corrente, em tiques — ver `JANELA_DE_DERIVA_TICKS`.
    janela_ticks: u32,
    /// Uma janela fechou com excesso, e o próximo tique contíguo tira um quadro.
    descarte_devido: bool,
    /// Tiques desde que o descarte ficou devido, esperando uma pausa.
    espera_do_descarte: u32,
    /// O excesso é de recuperação, e o descarte não espera pausa.
    descarte_urgente: bool,
    metrics: JitterMetrics,
}

impl<T> JitterBuffer<T> {
    /// Builds a buffer with the given configuration.
    #[must_use]
    pub fn new(config: JitterConfig) -> Self {
        Self {
            config,
            queue: VecDeque::with_capacity(CAPACITY),
            next_timestamp: None,
            last_played_seq: None,
            gap_lost_left: 0,
            gap_comfort_left: 0,
            playing: false,
            target_ms: config.initial_target_ms,
            jitter_ms: 0.0,
            last_transit_ms: None,
            consecutive_missing: 0,
            starved_since_play: 0,
            piso_da_janela_ms: f64::MAX,
            ticks_da_janela: 0,
            janela_ticks: JANELA_DE_DERIVA_TICKS,
            descarte_devido: false,
            espera_do_descarte: 0,
            descarte_urgente: false,
            metrics: JitterMetrics {
                target_ms: config.initial_target_ms,
                ..JitterMetrics::default()
            },
        }
    }

    /// Current metrics.
    #[must_use]
    pub fn metrics(&self) -> JitterMetrics {
        JitterMetrics {
            target_ms: self.target_ms,
            depth_ms: self.depth_ms(),
            jitter_ms: self.jitter_ms,
            ..self.metrics
        }
    }

    /// How much audio is currently held, in milliseconds.
    #[must_use]
    pub fn depth_ms(&self) -> f64 {
        self.queue.len() as f64 * self.config.frame_ms
    }

    /// Accepts a frame that has just arrived.
    ///
    /// `timestamp` is in samples at 48 kHz, per `specs/02-protocolo.md`.
    /// `arrival_ms` is the receiver's clock.
    pub fn push(&mut self, seq: u16, timestamp: u32, arrival_ms: f64, payload: T) {
        self.metrics.frames_received += 1;
        self.update_jitter(timestamp, arrival_ms);

        // Too late: its slot has already been played or concealed.
        if let Some(next) = self.next_timestamp {
            if self.playing && ts_delta(timestamp, next) < 0 {
                self.metrics.late_discards += 1;
                return;
            }
        }

        if self.queue.iter().any(|held| held.seq == seq) {
            self.metrics.duplicates += 1;
            return;
        }

        // Uma rajada que enche a fila: o que sai é o quadro mais velho, e não o
        // que acabou de chegar — descartar o novo abriria um buraco atrás da
        // rajada, que `cortar_a_rajada` corta de qualquer jeito no próximo tique.
        if self.queue.len() >= CAPACITY
            && self.playing
            && self.depth_ms() - self.target_ms > EXCESSO_DE_RAJADA_MS
        {
            let _ = self.cortar_o_mais_velho();
        }
        if self.queue.len() >= CAPACITY {
            self.metrics.overflow_discards += 1;
            return;
        }

        // Insert in timestamp order. Late-but-usable frames land in the middle,
        // which is the entire point of holding a buffer at all.
        let position = self
            .queue
            .iter()
            .position(|held| ts_delta(held.timestamp, timestamp) > 0)
            .unwrap_or(self.queue.len());
        self.queue.insert(
            position,
            Held {
                seq,
                timestamp,
                payload,
            },
        );
    }

    /// Produces the decision for one 20 ms playout slot.
    ///
    /// Call this on the playout clock, once per frame period.
    pub fn tick(&mut self) -> Decision<T> {
        self.adapt_target();

        // Prebuffering: hold until the target depth is reached, so the first
        // frames do not immediately underrun.
        if !self.playing {
            if self.depth_ms() < self.target_ms {
                return Decision::Starved;
            }
            self.playing = true;
            // Start at the earliest frame held, not at whichever arrived first.
            // With reordering those differ, and starting at the wrong one
            // conceals frames that are sitting right there.
            self.next_timestamp = self.queue.front().map(|held| held.timestamp);
        }

        // Still paying off a gap worked out on an earlier tick.
        if let Some(decision) = self.drain_gap() {
            return decision;
        }

        // O passado sai antes de qualquer decisão sobre a frente, com a fila
        // grande ou pequena: ver `descartar_o_passado`. Depois dele, nem o corte
        // nem o `plan_gap` veem frente atrás do ponteiro.
        let _ = self.descartar_o_passado();
        // Uma rajada de segundos sai de uma vez. Ver `cortar_a_rajada`.
        self.cortar_a_rajada();
        let Some(next) = self.next_timestamp else {
            return Decision::Starved;
        };

        // A fila que sobra e não volta: deriva de quem fala, ou uma pausa desta
        // máquina. Ver `JANELA_DE_DERIVA_TICKS`.
        self.medir_excesso();
        if self.descarte_devido {
            self.espera_do_descarte = self.espera_do_descarte.saturating_add(1);
            if self.descarte_urgente || self.espera_do_descarte >= ADIAMENTO_DA_EMENDA_TICKS {
                self.descartar_um(next);
            }
        }
        let Some(next) = self.next_timestamp else {
            return Decision::Starved;
        };

        let Some(front) = self.queue.front() else {
            // Nothing waiting at all: starved rather than lost. A talker who
            // stopped is not a network problem.
            //
            // Counted, because this tick is real time going by with nothing
            // played. When the next frame finally arrives, `plan_gap` uses the
            // count to know how much of the hole has already been heard.
            self.starved_since_play = self.starved_since_play.saturating_add(1);
            return Decision::Starved;
        };

        if front.timestamp == next {
            let played = self.queue.pop_front();
            self.next_timestamp = Some(next.wrapping_add(self.frame_samples()));
            self.consecutive_missing = 0;
            self.starved_since_play = 0;
            self.metrics.frames_played += 1;
            if let Some(held) = played {
                self.last_played_seq = Some(held.seq);
                return Decision::Play(held.payload);
            }
            return Decision::Starved;
        }

        // There is a hole between here and the next frame. Work out how much of
        // it the network took and how much of it the talker simply did not fill.
        let (front_ts, front_seq) = (front.timestamp, front.seq);
        self.plan_gap(front_ts, front_seq, next);

        if let Some(decision) = self.drain_gap() {
            return decision;
        }

        // `plan_gap` resynchronised rather than scheduling slots, so the frame at
        // the front is playable right now. Without this the tick is wasted and
        // playout stalls one slot per resync.
        if self.next_timestamp == Some(front_ts) {
            if let Some(held) = self.queue.pop_front() {
                self.next_timestamp = Some(front_ts.wrapping_add(self.frame_samples()));
                self.last_played_seq = Some(held.seq);
                self.consecutive_missing = 0;
                self.starved_since_play = 0;
                self.metrics.frames_played += 1;
                return Decision::Play(held.payload);
            }
        }
        Decision::Starved
    }

    /// Mede o piso da fila nesta janela e, ao fechá-la, decide se um quadro sobra.
    ///
    /// O excesso é o que **fica** na fila depois de este tique tocar, acima do
    /// alvo: o piso menos o quadro da vez menos o alvo. Só um quadro inteiro de
    /// excesso, durante a janela inteira, autoriza uma emenda — menos que isso
    /// é o buffer caçando a própria quantização, e a fala viraria uma emenda
    /// atrás da outra sem ganhar latência nenhuma.
    ///
    /// Um tique de fome põe o piso em zero e cancela a janela: uma fila que
    /// esvaziou uma vez nesse segundo não tem quadro sobrando.
    fn medir_excesso(&mut self) {
        self.piso_da_janela_ms = self.piso_da_janela_ms.min(self.depth_ms());
        self.ticks_da_janela = self.ticks_da_janela.saturating_add(1);
        if self.ticks_da_janela < self.janela_ticks {
            return;
        }
        let quadro = self.config.frame_ms;
        let excesso = self.piso_da_janela_ms - quadro - self.target_ms + FOLGA_DO_EXCESSO_MS;
        let devido = excesso >= quadro;
        if devido && !self.descarte_devido {
            self.espera_do_descarte = 0;
        }
        self.descarte_devido = devido;
        self.descarte_urgente = excesso >= 2.0 * quadro;
        self.janela_ticks = if excesso >= EXCESSO_DE_RECUPERACAO_QUADROS * quadro {
            JANELA_DE_RECUPERACAO_TICKS
        } else {
            JANELA_DE_DERIVA_TICKS
        };
        self.piso_da_janela_ms = f64::MAX;
        self.ticks_da_janela = 0;
    }

    /// **Uma rajada de segundos sai de uma vez, e não a 10%.**
    ///
    /// Depois de uma pausa desta máquina, o laço de voz lê de uma vez tudo o que
    /// a fila do enlace guardou e só então dá os tiques: a fila do buffer volta
    /// com segundos de voz contígua. A compressão de recuperação
    /// (`JANELA_DE_RECUPERACAO_TICKS`) tira um quadro a cada dez, e com 5 s isso
    /// era ~50 s de quem ouve atrás do presente — medido na LAN de 25/10 com um
    /// SIGSTOP de 8 s no receptor: 4820 ms, caindo 500 ms a cada 5 s.
    ///
    /// Acima de [`EXCESSO_DE_RAJADA_MS`] sobre o alvo, os quadros mais velhos
    /// saem da frente até a fila voltar ao alvo, e a fala continua do mais
    /// velho que sobrou. **O corte atravessa buracos** (a LAN03 de 25/10): a
    /// fila do enlace guarda os mais velhos de uma pausa e perde os seguintes,
    /// e um buraco entre os velhos e os novos é tempo que já passou. O ponteiro
    /// vai direto ao primeiro quadro que fica, e os lugares de buraco no
    /// caminho vão para `buraco_pulado` — não para `frames_silenced` nem
    /// `frames_concealed`, que contam lugares tocados.
    ///
    /// Nunca com um buraco pendente: aí o ponteiro é de `drain_gap` (o achado
    /// do Astra, `cortar_o_mais_velho`). Abaixo do limiar nada muda — a deriva
    /// soma um ou dois quadros, e uma pausa curta continua na compressão, sem
    /// apagar palavras de uma vez.
    ///
    /// Os quadros tirados vão para `descartes_por_rajada`, e não para a perda:
    /// eles chegaram, e saíram por serem velhos.
    fn cortar_a_rajada(&mut self) {
        if self.depth_ms() - self.target_ms <= EXCESSO_DE_RAJADA_MS
            || self.gap_lost_left > 0
            || self.gap_comfort_left > 0
        {
            return;
        }
        let Some(vez) = self.next_timestamp else {
            return;
        };
        // O passado sai primeiro, e nunca vira a frente: ver
        // `descartar_o_passado`. Daqui em diante, tudo na fila é da vez ou
        // depois dela, e o ponteiro só anda para a frente.
        if self.descartar_o_passado() > 0
            && self.depth_ms() - self.target_ms <= EXCESSO_DE_RAJADA_MS
        {
            return;
        }
        let quadro = i64::from(self.frame_samples().max(1));
        let mut cortados = 0_u64;
        while self.depth_ms() > self.target_ms + self.config.frame_ms && self.queue.len() > 1 {
            let Some(held) = self.queue.pop_front() else {
                break;
            };
            self.last_played_seq = Some(held.seq);
            cortados += 1;
        }
        let Some(frente) = self.queue.front().map(|held| held.timestamp) else {
            return;
        };
        if cortados == 0 {
            return;
        }
        // Os lugares do caminho, da vez até a frente que fica: os quadros
        // tirados (todos da vez em diante) e, o que sobra, buraco. A frente
        // nunca está atrás da vez, pelo descarte do passado acima.
        let lugares = u64::try_from(ts_delta(frente, vez) / quadro).unwrap_or(0);
        self.next_timestamp = Some(frente);
        self.metrics.descartes_por_rajada += cortados;
        self.metrics.buraco_pulado += lugares.saturating_sub(cortados);
        // A janela de excesso media a fila de antes do corte: recomeça, ou ela
        // ainda pediria emendas por um excesso que já saiu.
        self.piso_da_janela_ms = f64::MAX;
        self.ticks_da_janela = 0;
        self.janela_ticks = JANELA_DE_DERIVA_TICKS;
        self.descarte_devido = false;
        self.descarte_urgente = false;
        self.espera_do_descarte = 0;
    }

    /// **O que está atrás do ponteiro é passado, e sai.** Quadros atrasados de
    /// dentro de um buraco ficam na fila enquanto o buraco é pago; quando o
    /// `drain_gap` pula o resto dele, eles ficam atrás do ponteiro. Escolher um
    /// deles como frente faria o ponteiro recuar e reabrir o buraco (a revisão
    /// do Astra, 25/10). A comparação é circular (`ts_delta`), como a da `seq`
    /// e a do carimbo. Contados em `descartes_por_rajada` (quadros tirados), e
    /// nunca em `buraco_pulado` (lugares): sem dupla contagem.
    fn descartar_o_passado(&mut self) -> u64 {
        let Some(vez) = self.next_timestamp else {
            return 0;
        };
        let mut tirados = 0_u64;
        while self
            .queue
            .front()
            .is_some_and(|held| ts_delta(held.timestamp, vez) < 0)
        {
            if let Some(held) = self.queue.pop_front() {
                self.last_played_seq = Some(held.seq);
            }
            tirados += 1;
        }
        self.metrics.descartes_por_rajada += tirados;
        tirados
    }

    /// Tira o quadro da vez da frente, se ele e o seguinte são contíguos a
    /// partir de `next_timestamp`, e conta em `descartes_por_rajada`. Devolve se
    /// tirou.
    fn cortar_o_mais_velho(&mut self) -> bool {
        // Com um buraco pendente, o ponteiro é do buraco: andar com ele aqui e
        // de novo no `drain_gap` pularia um quadro e desencontraria o buraco da
        // frente planejada. Vale para os dois caminhos, o push e o tique.
        if self.gap_lost_left > 0 || self.gap_comfort_left > 0 {
            return false;
        }
        let Some(vez) = self.next_timestamp else {
            return false;
        };
        let quadro = self.frame_samples();
        let contiguo = matches!(
            (self.queue.front(), self.queue.get(1)),
            (Some(a), Some(b)) if a.timestamp == vez && b.timestamp == vez.wrapping_add(quadro)
        );
        if !contiguo {
            return false;
        }
        if let Some(held) = self.queue.pop_front() {
            self.last_played_seq = Some(held.seq);
        }
        self.next_timestamp = Some(vez.wrapping_add(quadro));
        self.metrics.descartes_por_rajada += 1;
        true
    }

    /// Tira da fala o quadro da vez, se o seguinte é contíguo a ele.
    ///
    /// **Só entre dois quadros que existem.** Emendar através de um buraco de
    /// perda ou de uma pausa misturaria esta conta com a de `plan_gap`, e um
    /// quadro descartado ali seria lido como perdido. Se a frente não é
    /// contígua, o descarte espera o próximo tique que for.
    fn descartar_um(&mut self, next: u32) {
        let seguinte = next.wrapping_add(self.frame_samples());
        let contiguo = matches!(
            (self.queue.front(), self.queue.get(1)),
            (Some(a), Some(b)) if a.timestamp == next && b.timestamp == seguinte
        );
        if !contiguo {
            return;
        }
        let _ = self.queue.pop_front();
        self.next_timestamp = Some(seguinte);
        self.descarte_devido = false;
        self.metrics.descartes_por_atraso += 1;
    }

    /// Frame duration in samples, from the configured frame length.
    fn frame_samples(&self) -> u32 {
        ((self.config.frame_ms * f64::from(crate::SAMPLE_RATE_HZ)) / 1000.0) as u32
    }

    /// Splits a gap into lost slots and silent slots.
    ///
    /// The timestamp gap says how many slots are missing in total; the sequence
    /// gap says how many packets the network actually dropped. The difference is
    /// silence the talker never sent. See the module docs.
    fn plan_gap(&mut self, front_ts: u32, front_seq: u16, next_ts: u32) {
        let frame_samples = i64::from(self.frame_samples().max(1));
        let total = (ts_delta(front_ts, next_ts) / frame_samples).max(0) as u64;
        if total == 0 {
            // Timestamps that do not land on a frame boundary. Resynchronise
            // rather than loop; a sender doing this is out of spec anyway.
            self.next_timestamp = Some(front_ts);
            self.metrics.resyncs += 1;
            return;
        }

        // A gap far beyond anything plausible means we fell behind rather than
        // lost audio — jump instead of grinding through it slot by slot.
        if total > u64::from(MAX_GAP_FRAMES) {
            self.metrics.resyncs += 1;
            self.next_timestamp = Some(front_ts);
            self.consecutive_missing = 0;
            return;
        }

        let lost = match self.last_played_seq {
            // seq counts transmitted packets, so the hole in it is exactly what
            // the network dropped, no matter how long the silence around it was.
            Some(last) => i64::from(seq_delta(front_seq, last))
                .saturating_sub(1)
                .max(0) as u64,
            // Nothing played yet, so there is no baseline to compare against.
            // Treat it as silence rather than inventing loss.
            None => 0,
        };

        // How much of this gap has already been lived through.
        //
        // A hole is only discovered when the frame *after* it arrives. If the
        // buffer ran dry in the meantime, those ticks already went out as
        // nothing — the listener has heard that silence. Scheduling slots to
        // play it again would be double-counting, and the delay it adds is
        // permanent: every pause in a conversation would push playout further
        // behind, pause length by pause length, until the queue hit its cap.
        //
        // That is why this subtraction exists, and it is the difference between
        // a call that stays at 90 ms and one that is two seconds behind after a
        // minute of ordinary back-and-forth.
        //
        // A gap found while audio was still queued is the other case: there the
        // ticks have not been spent, the buffer has depth to pay with, and
        // concealment does its job without adding delay.
        let spent = std::mem::take(&mut self.starved_since_play);
        let skipped = spent.min(total);
        let remaining = total - skipped;

        // Move the playout pointer past what has already been heard.
        //
        // This channel is the fix, and leaving it out was the first attempt: the
        // counters were reduced but the pointer was not moved, so the next tick
        // found the same hole with the counter already spent and scheduled the
        // whole thing again. The skip has to happen in the pointer or it does
        // not happen at all.
        let advance = u32::try_from(skipped)
            .unwrap_or(0)
            .wrapping_mul(self.frame_samples());
        self.next_timestamp = Some(next_ts.wrapping_add(advance));

        // What is left is spent on the lost frames first. Concealment needs a
        // slot to happen in; silence does not, because silence is what already
        // went out while the buffer was dry.
        self.gap_lost_left = lost.min(remaining);
        self.gap_comfort_left = remaining - self.gap_lost_left;

        // **A pausa paga a deriva, de graça.** Quando a pausa foi vivida em tempo
        // real (`skipped > 0`), o conforto que sobra é exatamente o quanto quem
        // ouve está atrás de quem fala: o alvo, mais o que a deriva acumulou.
        // Tocar essa sobra inteira devolve o excesso para a fala, e lá ele só sai
        // por emenda (`descartar_um`) — medido: 26 emendas em 30 min a +300 ppm
        // com fala de 5 s e pausa de 1 s. Encurtar a pausa em 20 ms não se ouve.
        //
        // Só com a pausa vivida: um buraco achado com a fila ainda cheia
        // (`skipped == 0`) é silêncio que ainda não tocou, e é dele o lugar.
        if skipped > 0 && self.gap_lost_left == 0 && self.gap_comfort_left > 0 {
            let quadro = self.config.frame_ms;
            let atras = self.gap_comfort_left as f64 * quadro + self.depth_ms() - quadro;
            let excesso = atras - self.target_ms + FOLGA_DO_EXCESSO_MS;
            let sobra = if excesso >= quadro {
                ((excesso / quadro).floor() as u64).min(self.gap_comfort_left)
            } else {
                0
            };
            if sobra > 0 {
                let pulo = u32::try_from(sobra)
                    .unwrap_or(0)
                    .wrapping_mul(self.frame_samples());
                self.next_timestamp = self.next_timestamp.map(|ts| ts.wrapping_add(pulo));
                self.gap_comfort_left -= sobra;
                self.metrics.pausa_encurtada_quadros += sobra;
                // Pago aqui: a emenda que esperava esta pausa não é mais devida.
                self.descarte_devido = false;
            }
        }

        // The skipped slots still happened, and the metrics have to describe
        // what a listener heard. Loss that fell inside them stays counted as
        // loss — understating it would make the Sync Ratio flatter the link
        // than it deserves — and the rest is the talker's pause.
        let lost_skipped = lost.saturating_sub(self.gap_lost_left);
        self.metrics.frames_silenced += lost_skipped;
        self.metrics.frames_comfort += skipped.saturating_sub(lost_skipped);

        if remaining == 0 {
            self.consecutive_missing = 0;
        }
    }

    /// Emits one slot of a planned gap, if any remain.
    fn drain_gap(&mut self) -> Option<Decision<T>> {
        // **O guarda do teto: com a fila cheia, um buraco não gasta tique.** Um
        // tique de buraco não tira quadro da fila, e com ela no teto o quadro
        // vivo que chega nesse tique transborda (`push`). Cada tique de buraco
        // vira assim um quadro descartado, e o buraco se reabre do mesmo tamanho
        // uma volta de fila adiante. Medido no P1 do CUA de 25/10: ~50% de perda
        // por dois minutos, sem nenhum pacote perdido no fio; e na pausa de 8 s
        // do laço (`seele-conformance/tests/voz_depois_de_um_tranco_do_laco.rs`).
        //
        // **E o buraco vencido** (a LAN03 de 25/10: ~2,5 s de silêncio depois
        // da volta de uma pausa): se, enquanto o buraco é pago, a fila passa do
        // alvo por mais de [`EXCESSO_DE_RAJADA_MS`], quem fala já está meio
        // segundo à frente do buraco, e o resto dele é tempo que já passou —
        // tocá-lo como silêncio só atrasa a voz do presente. O mesmo limiar da
        // rajada (`cortar_a_rajada`), e abaixo dele nada muda: uma perda comum,
        // com a fila no alvo, continua ocultada e silenciada lugar a lugar.
        //
        // O que se pula não é tocado nem como silêncio nem como ocultação, e
        // por isso é contado à parte (`buraco_pulado`), e não em
        // `frames_silenced`. O ponteiro anda o resto do buraco uma vez só, e os
        // contadores do buraco zeram junto: o estado fica coerente.
        let vencido = self.depth_ms() - self.target_ms > EXCESSO_DE_RAJADA_MS;
        if (self.gap_lost_left > 0 || self.gap_comfort_left > 0)
            && (self.queue.len() + 1 >= CAPACITY || vencido)
        {
            let resto = self.gap_lost_left + self.gap_comfort_left;
            let pulo = u32::try_from(resto)
                .unwrap_or(0)
                .wrapping_mul(self.frame_samples());
            self.next_timestamp = self.next_timestamp.map(|ts| ts.wrapping_add(pulo));
            self.gap_lost_left = 0;
            self.gap_comfort_left = 0;
            self.consecutive_missing = 0;
            self.metrics.buraco_pulado += resto;
            self.metrics.resyncs += 1;
            // Os atrasados de dentro do buraco ficaram atrás do ponteiro.
            let _ = self.descartar_o_passado();
            return None;
        }

        if self.gap_lost_left > 0 {
            self.gap_lost_left -= 1;
            self.next_timestamp = self
                .next_timestamp
                .map(|ts| ts.wrapping_add(self.frame_samples()));
            self.consecutive_missing += 1;

            // specs/03-audio.md: PLC for one frame, silence from the second on.
            if self.consecutive_missing == 1 {
                self.metrics.frames_concealed += 1;
                return Some(Decision::Conceal);
            }
            self.metrics.frames_silenced += 1;
            return Some(Decision::Silence);
        }

        // **Silêncio com a voz já esperando na fila é atraso puro.** O silêncio
        // de conforto é a pausa de quem fala, e é tocado no lugar dela. Mas se,
        // enquanto ele toca, a fila passa do alvo, a pessoa já voltou a falar e o
        // que sobra da pausa só empurra quem ouve para trás — e esse atraso não
        // volta mais. O caso que ensinou: até a 0.15.0, trocar de microfone
        // mandava o carimbo um segundo à frente, e cada troca somava ~900 ms a
        // quem ouvia (relato de 05/10/2026). O resto da pausa é pulado de uma vez,
        // contado como ressincronia, e a frente da fila toca.
        if self.gap_comfort_left > 0 && self.depth_ms() > self.target_ms + self.config.frame_ms {
            let pulo = u32::try_from(self.gap_comfort_left)
                .unwrap_or(0)
                .wrapping_mul(self.frame_samples());
            self.next_timestamp = self.next_timestamp.map(|ts| ts.wrapping_add(pulo));
            self.gap_comfort_left = 0;
            self.consecutive_missing = 0;
            self.metrics.resyncs += 1;
            return None;
        }

        if self.gap_comfort_left > 0 {
            self.gap_comfort_left -= 1;
            self.next_timestamp = self
                .next_timestamp
                .map(|ts| ts.wrapping_add(self.frame_samples()));
            self.consecutive_missing = 0;
            self.metrics.frames_comfort += 1;
            return Some(Decision::Comfort);
        }

        None
    }

    /// RFC 3550 interarrival jitter.
    ///
    /// `D` is the difference in relative transit time between consecutive
    /// frames; the estimate follows it with a 1/16 gain. Using the RTP timestamp
    /// as the send clock means no clock synchronisation between the two ends is
    /// needed — only the difference matters.
    fn update_jitter(&mut self, timestamp: u32, arrival_ms: f64) {
        let sent_ms = f64::from(timestamp) / f64::from(crate::SAMPLE_RATE_HZ) * 1000.0;
        let transit = arrival_ms - sent_ms;

        if let Some(previous) = self.last_transit_ms {
            let difference = (transit - previous).abs();
            // **Um salto do relógio de quem manda não é jitter da rede.** Numa
            // pausa de fala o carimbo e a chegada andam juntos e a diferença de
            // trânsito fica perto de zero; quando o carimbo pula sem o tempo ter
            // passado — trocar de microfone até a 0.15.0 pulava um segundo —, a
            // diferença inteira entrava aqui como oscilação da rede, e o alvo do
            // buffer subia para o teto e levava ~20 s para descer. Nenhuma rede
            // em que se conversa oscila meio segundo de um pacote para o outro:
            // acima disso é descontinuidade, e a base do trânsito recomeça.
            if difference <= DESCONTINUIDADE_DE_RELOGIO_MS {
                self.jitter_ms += (difference - self.jitter_ms) * JITTER_SMOOTHING;
            }
        }
        self.last_transit_ms = Some(transit);
    }

    /// Moves the target toward what the measured jitter demands.
    ///
    /// Growth is immediate, shrinking is gradual — `specs/03-audio.md` is
    /// explicit that this asymmetry is intentional, "because cutting audio is
    /// worse than delaying it".
    fn adapt_target(&mut self) {
        let needed = (self.jitter_ms * JITTER_HEADROOM)
            .max(self.config.frame_ms)
            .clamp(self.config.min_target_ms, self.config.max_target_ms);

        if needed > self.target_ms {
            self.target_ms = needed;
        } else {
            self.target_ms += (needed - self.target_ms) * SHRINK_PER_FRAME;
        }
        self.target_ms = self
            .target_ms
            .clamp(self.config.min_target_ms, self.config.max_target_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netsim::{run, NetworkProfile};

    fn buffer() -> JitterBuffer<u16> {
        JitterBuffer::new(JitterConfig::default())
    }

    /// Feeds a whole simulated run through a buffer, ticking on the playout
    /// clock between arrivals, and returns every decision in order.
    fn drive(
        profile: NetworkProfile,
        seed: u64,
        frames: usize,
    ) -> (Vec<Decision<u16>>, JitterMetrics) {
        let (arrivals, _) = run(profile, seed, frames);
        let mut buffer = buffer();
        let mut decisions = Vec::new();
        let mut clock_ms = 0.0_f64;
        let mut arrival_index = 0;

        // Run the playout clock past the end so the buffer drains.
        let total_ticks = frames + 50;
        for _ in 0..total_ticks {
            while arrivals
                .get(arrival_index)
                .is_some_and(|frame| frame.arrived_at_ms <= clock_ms)
            {
                if let Some(frame) = arrivals.get(arrival_index) {
                    buffer.push(frame.seq, frame.timestamp, frame.arrived_at_ms, frame.seq);
                }
                arrival_index += 1;
            }
            decisions.push(buffer.tick());
            clock_ms += f64::from(crate::FRAME_MS);
        }
        let metrics = buffer.metrics();
        (decisions, metrics)
    }

    fn count_played(decisions: &[Decision<u16>]) -> usize {
        decisions
            .iter()
            .filter(|d| matches!(d, Decision::Play(_)))
            .count()
    }

    #[test]
    fn sequence_delta_handles_wrap() {
        // 21 minutes into a call the sequence wraps. A naive `<` breaks here
        // exactly once, and it looks like a random glitch.
        assert_eq!(seq_delta(5, 3), 2);
        assert_eq!(seq_delta(3, 5), -2);
        assert_eq!(seq_delta(0, u16::MAX), 1, "0 comes after 65535");
        assert_eq!(seq_delta(u16::MAX, 0), -1);
        assert_eq!(seq_delta(10, u16::MAX - 10), 21);
        assert_eq!(seq_delta(7, 7), 0);
    }

    #[test]
    fn a_perfect_network_plays_every_frame_in_order() {
        let (decisions, metrics) = drive(NetworkProfile::perfect(), 1, 500);

        assert_eq!(metrics.frames_concealed, 0);
        assert_eq!(metrics.frames_silenced, 0);
        assert_eq!(metrics.late_discards, 0);
        assert_eq!(count_played(&decisions), 500);

        let played: Vec<u16> = decisions
            .iter()
            .filter_map(|d| match d {
                Decision::Play(seq) => Some(*seq),
                _ => None,
            })
            .collect();
        let mut sorted = played.clone();
        sorted.sort_unstable();
        assert_eq!(played, sorted, "playout must be in sequence order");
    }

    #[test]
    fn reordered_frames_are_put_back_in_order() {
        // The buffer's whole reason to exist. Push out of order, expect in order.
        let mut buffer = buffer();
        buffer.push(3, 2_880, 60.0, 3);
        buffer.push(1, 960, 61.0, 1);
        buffer.push(2, 1_920, 62.0, 2);
        buffer.push(4, 3_840, 63.0, 4);

        let mut played = Vec::new();
        for _ in 0..4 {
            if let Decision::Play(seq) = buffer.tick() {
                played.push(seq);
            }
        }
        assert_eq!(played, vec![1, 2, 3, 4]);
        assert_eq!(buffer.metrics().frames_concealed, 0);
    }

    #[test]
    fn a_gap_conceals_once_then_falls_back_to_silence() {
        // specs/03-audio.md: "missing frame -> Opus PLC for one frame; from the
        // second on, silence with fade".
        let mut buffer = buffer();
        buffer.push(1, 960, 0.0, 1);
        buffer.push(2, 1_920, 20.0, 2);
        // 3, 4 and 5 never arrive.
        buffer.push(6, 5_760, 100.0, 6);

        let decisions: Vec<Decision<u16>> = (0..6).map(|_| buffer.tick()).collect();

        assert_eq!(decisions.first(), Some(&Decision::Play(1)));
        assert_eq!(decisions.get(1), Some(&Decision::Play(2)));
        assert_eq!(
            decisions.get(2),
            Some(&Decision::Conceal),
            "first gap frame"
        );
        assert_eq!(decisions.get(3), Some(&Decision::Silence), "second onwards");
        assert_eq!(decisions.get(4), Some(&Decision::Silence));
        assert_eq!(decisions.get(5), Some(&Decision::Play(6)));

        let metrics = buffer.metrics();
        assert_eq!(metrics.frames_concealed, 1);
        assert_eq!(metrics.frames_silenced, 2);
    }

    #[test]
    fn a_frame_that_arrives_after_its_slot_is_discarded_and_counted() {
        let mut buffer = buffer();
        buffer.push(1, 960, 0.0, 1);
        buffer.push(2, 1_920, 20.0, 2);
        buffer.push(4, 3_840, 60.0, 4);

        // Play through slot 3, concealing it.
        let _ = buffer.tick();
        let _ = buffer.tick();
        let _ = buffer.tick();

        // Frame 3 finally shows up. Too late to be useful.
        buffer.push(3, 2_880, 200.0, 3);

        assert_eq!(buffer.metrics().late_discards, 1);
        assert_eq!(buffer.metrics().frames_concealed, 1);
    }

    #[test]
    fn duplicates_are_ignored() {
        let mut buffer = buffer();
        // Two distinct frames, so the 40 ms prebuffer target is met and playout
        // can begin; the repeats of frame 1 are the thing under test.
        buffer.push(1, 960, 0.0, 1);
        buffer.push(1, 960, 1.0, 1);
        buffer.push(1, 960, 2.0, 1);
        buffer.push(2, 1_920, 20.0, 2);

        assert_eq!(buffer.metrics().duplicates, 2);
        assert_eq!(buffer.tick(), Decision::Play(1));
        assert_eq!(buffer.tick(), Decision::Play(2));
        assert!(matches!(buffer.tick(), Decision::Starved));
    }

    #[test]
    fn target_grows_fast_and_shrinks_slowly() {
        // specs/03-audio.md calls this asymmetry intentional. Prove both halves,
        // because getting only the growth right still sounds fine in testing and
        // wastes latency forever afterwards.
        let mut buffer = buffer();
        let start = buffer.metrics().target_ms;

        // A burst of wildly varying transit times.
        for index in 0..40_u32 {
            let wobble = if index % 2 == 0 { 0.0 } else { 90.0 };
            buffer.push(
                index as u16 + 1,
                index * 960,
                f64::from(index) * 20.0 + wobble,
                index as u16,
            );
            let _ = buffer.tick();
        }
        let grown = buffer.metrics().target_ms;
        assert!(
            grown > start * 2.0,
            "target should have grown quickly: {start} -> {grown}"
        );

        // Now perfectly regular arrivals for the same number of frames.
        for index in 100..140_u32 {
            buffer.push(
                index as u16,
                index * 960,
                f64::from(index) * 20.0,
                index as u16,
            );
            let _ = buffer.tick();
        }
        let after = buffer.metrics().target_ms;
        assert!(
            after > grown * 0.9,
            "shrinking must be slow: {grown} -> {after} in 40 frames"
        );
    }

    #[test]
    fn target_stays_inside_the_spec_bounds() {
        // specs/03-audio.md: adjusts the target between 20 ms and 200 ms.
        let config = JitterConfig::default();
        for profile in [
            NetworkProfile::perfect(),
            NetworkProfile::lan(),
            NetworkProfile::wifi(),
            NetworkProfile::regional(),
            NetworkProfile::mobile_poor(),
        ] {
            let (_, metrics) = drive(profile, 5, 3_000);
            assert!(
                metrics.target_ms >= config.min_target_ms - f64::EPSILON
                    && metrics.target_ms <= config.max_target_ms + f64::EPSILON,
                "target {} outside [{}, {}]",
                metrics.target_ms,
                config.min_target_ms,
                config.max_target_ms
            );
        }
    }

    #[test]
    fn a_wide_jitter_path_holds_more_delay_than_a_quiet_one() {
        let (_, lan) = drive(NetworkProfile::lan(), 9, 3_000);
        let (_, mobile) = drive(NetworkProfile::mobile_poor(), 9, 3_000);
        assert!(
            mobile.target_ms > lan.target_ms,
            "a jittery path must buy more headroom: mobile {} vs lan {}",
            mobile.target_ms,
            lan.target_ms
        );
    }

    #[test]
    fn the_acceptance_profile_stays_mostly_intelligible() {
        // specs/09-roadmap.md accepts M1 on "induced loss of 5% remains
        // intelligible". This cannot judge intelligibility, but it can hold the
        // channel that the buffer is not making it worse: what the network dropped
        // should be roughly what the listener loses, no more.
        let (decisions, metrics) =
            drive(NetworkProfile::acceptance_five_percent_loss(), 77, 10_000);

        let played = count_played(&decisions) as f64;
        let lost = (metrics.frames_concealed + metrics.frames_silenced) as f64;
        let observed = lost / (played + lost);

        assert!(
            observed < 0.08,
            "buffer lost {observed:.3} of slots on a 5% path — it is adding loss"
        );
        assert_eq!(
            metrics.overflow_discards, 0,
            "a 5% path should never overflow a 5 s buffer"
        );
    }

    #[test]
    fn no_profile_makes_the_buffer_panic_or_lose_track() {
        // The closest thing to a property test over the whole space: every
        // profile, several seeds, checking the invariants that must hold no
        // matter what arrives.
        for profile in [
            NetworkProfile::perfect(),
            NetworkProfile::lan(),
            NetworkProfile::wifi(),
            NetworkProfile::regional(),
            NetworkProfile::mobile_poor(),
            NetworkProfile::acceptance_five_percent_loss(),
        ] {
            for seed in [1_u64, 7, 42, 1_000] {
                let (decisions, metrics) = drive(profile, seed, 2_000);

                assert!(!decisions.is_empty());
                assert!(
                    metrics.frames_played <= metrics.frames_received,
                    "played more frames than arrived"
                );
                assert!(metrics.loss_fraction() >= 0.0 && metrics.loss_fraction() <= 1.0);
                assert!(metrics.target_ms.is_finite());
                assert!(metrics.jitter_ms.is_finite() && metrics.jitter_ms >= 0.0);
                assert!(metrics.depth_ms.is_finite() && metrics.depth_ms >= 0.0);
            }
        }
    }

    #[test]
    fn the_buffer_is_deterministic() {
        let first = drive(NetworkProfile::mobile_poor(), 314, 2_000);
        let second = drive(NetworkProfile::mobile_poor(), 314, 2_000);
        assert_eq!(first.0, second.0);
        assert_eq!(first.1, second.1);
    }

    /// Pushes a stream of frames onto a buffer with no network impairment, one
    /// per playout tick, and collects the decisions.
    fn play_stream(
        frames: &[crate::netsim::SentFrame],
        ticks: usize,
    ) -> (Vec<Decision<u16>>, JitterMetrics) {
        let mut buffer = buffer();
        let mut decisions = Vec::new();
        let mut index = 0;
        let mut clock_ms = 0.0_f64;

        for _ in 0..ticks {
            while frames
                .get(index)
                .is_some_and(|frame| frame.sent_at_ms <= clock_ms)
            {
                if let Some(frame) = frames.get(index) {
                    buffer.push(frame.seq, frame.timestamp, frame.sent_at_ms, frame.seq);
                }
                index += 1;
            }
            decisions.push(buffer.tick());
            clock_ms += f64::from(crate::FRAME_MS);
        }
        let metrics = buffer.metrics();
        (decisions, metrics)
    }

    #[test]
    fn dtx_silence_is_not_counted_as_loss() {
        // Gap G5. A talker who stops sends nothing, so the sequence stays
        // contiguous while the timestamp jumps. Reading that as loss would sink
        // the Sync Ratio every time the room goes quiet.
        let mut buffer = buffer();
        let frame_samples = crate::FRAME_SAMPLES as u32;

        buffer.push(1, frame_samples, 0.0, 1);
        buffer.push(2, frame_samples * 2, 20.0, 2);
        // Frame 2 sits at slot 2 and frame 3 at slot 12, so nine slots are
        // silent. The sequence stays contiguous across all of them.
        buffer.push(3, frame_samples * 12, 220.0, 3);

        let decisions: Vec<Decision<u16>> = (0..12).map(|_| buffer.tick()).collect();

        assert_eq!(decisions.first(), Some(&Decision::Play(1)));
        assert_eq!(decisions.get(1), Some(&Decision::Play(2)));
        assert!(
            decisions
                .get(2..11)
                .is_some_and(|slots| slots.iter().all(|decision| *decision == Decision::Comfort)),
            "the silence should be comfort, not concealment: {decisions:?}"
        );
        assert_eq!(decisions.get(11), Some(&Decision::Play(3)));

        let metrics = buffer.metrics();
        assert_eq!(metrics.frames_comfort, 9);
        assert_eq!(metrics.frames_concealed, 0, "silence must not conceal");
        assert_eq!(metrics.frames_silenced, 0);
        assert_eq!(
            metrics.loss_fraction(),
            0.0,
            "a quiet room reported as packet loss"
        );
    }

    #[test]
    fn real_loss_is_still_counted_as_loss() {
        // The control for the test above. Here the sequence skips too, which is
        // the network rather than the talker.
        let mut buffer = buffer();
        let frame_samples = crate::FRAME_SAMPLES as u32;

        buffer.push(1, frame_samples, 0.0, 1);
        buffer.push(2, frame_samples * 2, 20.0, 2);
        // Frames 3, 4, 5 were dropped: sequence jumps to 6 and the timestamp
        // moves by the same three slots.
        buffer.push(6, frame_samples * 6, 100.0, 6);

        let decisions: Vec<Decision<u16>> = (0..6).map(|_| buffer.tick()).collect();

        assert_eq!(decisions.get(2), Some(&Decision::Conceal));
        assert_eq!(decisions.get(3), Some(&Decision::Silence));
        assert_eq!(decisions.get(4), Some(&Decision::Silence));
        assert_eq!(decisions.get(5), Some(&Decision::Play(6)));

        let metrics = buffer.metrics();
        assert_eq!(metrics.frames_comfort, 0);
        assert_eq!(metrics.frames_concealed, 1);
        assert_eq!(metrics.frames_silenced, 2);
        assert!(metrics.loss_fraction() > 0.0);
    }

    #[test]
    fn loss_inside_a_silence_is_separated_exactly() {
        // Both at once, which is the case a simpler rule gets wrong. The
        // timestamp gap gives the total slots; the sequence gap gives how many
        // were lost; the remainder is silence.
        let mut buffer = buffer();
        let frame_samples = crate::FRAME_SAMPLES as u32;

        buffer.push(1, frame_samples, 0.0, 1);
        buffer.push(2, frame_samples * 2, 20.0, 2);
        // Nineteen slots between frame 2 and frame 5, but the sequence only
        // skipped two packets: the talker was quiet for the other seventeen.
        buffer.push(5, frame_samples * 22, 440.0, 5);

        let _decisions: Vec<Decision<u16>> = (0..22).map(|_| buffer.tick()).collect();

        let metrics = buffer.metrics();
        assert_eq!(
            metrics.frames_concealed + metrics.frames_silenced,
            2,
            "lost"
        );
        assert_eq!(metrics.frames_comfort, 17, "silent");
    }

    #[test]
    fn a_pause_does_not_push_playout_permanently_behind() {
        // The failure this is here to stop, found by the M1.16 soak: a hole is
        // only discovered when the frame after it arrives, and by then the
        // buffer has already spent that time starving. Scheduling slots to play
        // the silence *again* costs the length of the pause, every pause, and
        // never gives it back — a conversation with a breath every few seconds
        // ends up seconds behind.
        let mut buffer = buffer();
        let frame = crate::FRAME_SAMPLES as u32;

        // Ten frames of speech, played out.
        for index in 0..10_u16 {
            buffer.push(
                index + 1,
                u32::from(index) * frame,
                f64::from(index) * 20.0,
                index,
            );
        }
        for _ in 0..10 {
            buffer.tick();
        }

        // Fifty ticks of nothing: the talker paused. The listener hears silence
        // as it happens, because there is nothing to play.
        let during: Vec<Decision<u16>> = (0..50).map(|_| buffer.tick()).collect();
        assert!(
            during.iter().all(|d| matches!(d, Decision::Starved)),
            "the buffer had audio it was not playing during the pause"
        );

        // The talker resumes. Sequence is contiguous — DTX sent nothing — and
        // the timestamp has jumped by the whole pause.
        buffer.push(11, 60 * frame, 1_200.0, 99);

        // This tick must play. Anything else is the pause being replayed.
        match buffer.tick() {
            Decision::Play(payload) => assert_eq!(payload, 99),
            other => panic!("the pause was replayed instead of resuming: {other:?}"),
        }
    }

    #[test]
    fn a_quiet_conversation_reports_no_loss_at_all() {
        // The headline case. A talker who speaks in bursts, on a perfect link,
        // must report exactly zero loss — otherwise specs/07-estetica.md's
        // signature metric says the opposite of the truth every time somebody
        // pauses for breath.
        let frames = crate::netsim::talker_stream(25, 50, 20);
        let (_, metrics) = play_stream(&frames, 1_600);

        assert!(metrics.frames_played > 400, "the talker never got heard");
        assert!(
            metrics.frames_comfort > 400,
            "the silence was not recognised"
        );
        assert_eq!(
            metrics.frames_concealed, 0,
            "concealed {} frames on a perfect link",
            metrics.frames_concealed
        );
        assert_eq!(metrics.loss_fraction(), 0.0);
    }

    #[test]
    fn silence_is_not_compressed_into_a_moment() {
        // Playing consecutive sequence numbers back to back would turn a second
        // of silence into 20 ms, which is why playout is indexed by timestamp.
        let frames = crate::netsim::talker_stream(10, 40, 3);
        let (decisions, _) = play_stream(&frames, 200);

        // Counted rather than sliced, because prebuffering shifts playout by a
        // tick and a hard-coded window would test the offset, not the silence.
        let plays_in_first_50 = decisions
            .get(..50)
            .unwrap_or_default()
            .iter()
            .filter(|decision| matches!(decision, Decision::Play(_)))
            .count();
        assert_eq!(
            plays_in_first_50, 10,
            "the first burst is ten frames; more means the silence was compressed"
        );
    }

    /// **Trocar de microfone não pode empurrar quem ouve para trás.**
    ///
    /// Até a 0.15.0, quem trocava de microfone mandava o caminho novo um segundo
    /// à frente no carimbo e um só número à frente na sequência. Aqui isso lia
    /// como um segundo de pausa de quem fala, e o buffer agendava ~900 ms de
    /// silêncio de conforto enquanto a voz nova já chegava e se empilhava na
    /// fila. Esse atraso não voltava nunca, e cada troca somava outro: relato de
    /// 05/10/2026, medido 50 → 950 → 1850 → 2750 ms.
    ///
    /// Simulado em tempo real, um tique a cada 20 ms: o remetente da 0.15.0
    /// troca de aparelho quatro vezes, cada troca com 100 ms reais sem quadro.
    /// O atraso de cada trecho entre trocas não pode passar do primeiro mais
    /// 100 ms. As máquinas da 0.15.0 continuam mandando o salto, e por isso a
    /// defesa mora aqui, e não só no remetente.
    #[test]
    fn trocar_de_microfone_do_outro_lado_nao_acumula_atraso() {
        let mut buffer = buffer();
        let quadro = crate::FRAME_SAMPLES as u32;
        let (mut seq, mut carimbo) = (0_u16, 0_u32);
        // Quando cada quadro saiu, em tiques: o atraso é o tique de agora menos esse.
        let mut atrasos_por_trecho: Vec<Vec<u64>> = vec![Vec::new()];
        let mut proximo_envio = 0_u64;
        let mut trocas = 0;
        for tique in 0_u64..1_400 {
            // A cada 250 tiques (5 s), uma troca: 5 tiques (100 ms) sem quadro, e o
            // caminho novo com o salto da 0.15.0.
            if tique > 0 && tique % 250 == 0 && trocas < 4 {
                trocas += 1;
                proximo_envio = tique + 5;
                seq = seq.wrapping_add(1);
                carimbo = carimbo.wrapping_add(48_000);
                atrasos_por_trecho.push(Vec::new());
            }
            if tique >= proximo_envio {
                // Rede de 30 ms, constante: chega no mesmo tique, com o relógio certo.
                buffer.push(seq, carimbo, tique as f64 * 20.0 + 30.0, tique as u16);
                seq = seq.wrapping_add(1);
                carimbo = carimbo.wrapping_add(quadro);
            }
            if let Decision::Play(enviado) = buffer.tick() {
                let enviado = (tique & !0xffff) | u64::from(enviado);
                if let Some(trecho) = atrasos_por_trecho.last_mut() {
                    trecho.push(tique.saturating_sub(enviado) * 20);
                }
            }
        }
        let medianas: Vec<u64> = atrasos_por_trecho
            .iter()
            .map(|trecho| {
                let mut t = trecho.clone();
                t.sort_unstable();
                t.get(t.len() / 2).copied().unwrap_or(0)
            })
            .collect();
        let primeiro = *medianas.first().expect("há sempre o primeiro trecho");
        for (i, mediana) in medianas.iter().enumerate() {
            assert!(
                *mediana <= primeiro + 100,
                "o atraso cresceu com as trocas de microfone do outro lado: {medianas:?} ms \
                 (trecho {i})"
            );
        }
    }

    /// Um falante com relógio adiantado `ppm`, por um perfil de rede, tocado no
    /// relógio de quem ouve. Devolve as métricas, as profundidades no 1º e no
    /// último minuto, e quantos quadros recebidos nunca tocaram.
    fn fala_continua_com_deriva(
        profile: NetworkProfile,
        ppm: f64,
        seed: u64,
        minutos: usize,
    ) -> (JitterMetrics, f64, f64, u64) {
        let quadros = minutos * 3_000;
        let mut rede = crate::netsim::Network::new(profile, seed);
        let mut chegadas = Vec::new();
        for quadro in crate::netsim::drifting_stream(quadros, ppm) {
            rede.transmit(quadro, &mut chegadas);
        }
        rede.sort_arrivals(&mut chegadas);

        let mut buffer = buffer();
        let (mut cedo, mut tarde) = (0.0, 0.0);
        let mut indice = 0;
        let mut tocados = std::collections::BTreeSet::new();
        let ticks = quadros - 3_000;
        for tique in 0..ticks {
            let agora = tique as f64 * f64::from(crate::FRAME_MS);
            while let Some(chegou) = chegadas.get(indice).filter(|c| c.arrived_at_ms <= agora) {
                buffer.push(
                    chegou.seq,
                    chegou.timestamp,
                    chegou.arrived_at_ms,
                    chegou.seq,
                );
                indice += 1;
            }
            if let Decision::Play(seq) = buffer.tick() {
                tocados.insert(seq);
            }
            if tique == 3_000 {
                cedo = buffer.depth_ms();
            }
            if tique == ticks - 1 {
                tarde = buffer.depth_ms();
            }
        }
        let metricas = buffer.metrics();
        // Recebidos e únicos, antes do último quadro tocado, que não tocaram:
        // perdidos por atraso, ou emendados.
        let ultimo = tocados.iter().copied().max().unwrap_or(0);
        let recebidos: std::collections::BTreeSet<u16> = chegadas
            .iter()
            .take(indice)
            .map(|c| c.seq)
            .filter(|seq| *seq <= ultimo)
            .collect();
        let nao_tocados = recebidos.difference(&tocados).count() as u64;
        (metricas, cedo, tarde, nao_tocados)
    }

    #[test]
    fn cada_emenda_e_contada_e_so_as_emendas_somem() {
        // «O produto sabe e não conta.» Numa rede perfeita, todo quadro chega; o
        // que não tocou só pode ter sido emendado, e o contador tem que bater com
        // o que se vê de fora — nem a mais, nem a menos.
        let (m, cedo, tarde, nao_tocados) =
            fala_continua_com_deriva(NetworkProfile::perfect(), 300.0, 1, 10);
        assert!(
            m.descartes_por_atraso > 0,
            "300 ppm em 9 min sem emenda nenhuma"
        );
        assert_eq!(m.late_discards, 0);
        assert_eq!(
            nao_tocados, m.descartes_por_atraso,
            "o contador de emendas não bate com os quadros que não tocaram"
        );
        // 9 min a 300 ppm são 162 ms: 9 quadros, no máximo um a mais.
        assert!(m.descartes_por_atraso <= 10, "emendou demais: {m:?}");
        assert!((tarde - cedo).abs() <= 20.0, "fila andou {cedo} -> {tarde}");
        assert_eq!(m.frames_concealed + m.frames_silenced, 0, "{m:?}");
    }

    #[test]
    fn com_jitter_a_emenda_nao_tira_quadro_que_ia_faltar() {
        // O medo de qualquer descarte: tirar da fila um quadro que o jitter da
        // rede ia precisar, e trocar atraso por ocultação. O piso da janela é o
        // que impede: só sai o que sobrou durante um segundo inteiro. A prova é
        // comparar a perda com e sem deriva, na mesma rede e semente.
        for profile in [
            NetworkProfile::lan(),
            NetworkProfile::wifi(),
            NetworkProfile::regional(),
            NetworkProfile::mobile_poor(),
        ] {
            let (parado, ..) = fala_continua_com_deriva(profile, 0.0, 11, 10);
            let (derivando, cedo, tarde, _) = fala_continua_com_deriva(profile, 300.0, 11, 10);
            eprintln!(
                "{profile:?}\n  0 ppm: perda {:.4} emendas {} alvo {:.1}\n  +300: perda {:.4} \
                 emendas {} alvo {:.1} fila {cedo}->{tarde}",
                parado.loss_fraction(),
                parado.descartes_por_atraso,
                parado.target_ms,
                derivando.loss_fraction(),
                derivando.descartes_por_atraso,
                derivando.target_ms,
            );
            assert!(
                derivando.loss_fraction() <= parado.loss_fraction() + 0.005,
                "as emendas viraram perda: {:.4} contra {:.4} sem deriva",
                derivando.loss_fraction(),
                parado.loss_fraction()
            );
            assert!(
                tarde - cedo <= 2.0 * f64::from(crate::FRAME_MS) + derivando.target_ms,
                "a fila andou {cedo} -> {tarde} ms com o alvo em {}",
                derivando.target_ms
            );
        }
    }

    #[test]
    fn an_implausible_gap_resyncs_instead_of_grinding() {
        // Not silence and not loss: a receiver that fell badly behind, or a
        // sender whose clock jumped. Walking a million slots would hang playout.
        let mut buffer = buffer();
        let frame_samples = crate::FRAME_SAMPLES as u32;

        buffer.push(1, frame_samples, 0.0, 1);
        buffer.push(2, frame_samples * 2, 20.0, 2);
        let _ = buffer.tick();
        let _ = buffer.tick();

        // An hour into the future.
        buffer.push(3, frame_samples * 180_000, 3_600_000.0, 3);

        let decision = buffer.tick();
        assert_eq!(decision, Decision::Play(3), "should have jumped the gap");
        assert_eq!(buffer.metrics().resyncs, 1);
        assert_eq!(buffer.metrics().frames_comfort, 0);
        assert_eq!(buffer.metrics().frames_concealed, 0);
    }

    #[test]
    fn an_empty_buffer_reports_starved_not_loss() {
        // Starvation before anyone has spoken is not a network fault, and must
        // not show up in the metric that specs/07 puts on screen.
        let mut buffer = buffer();
        for _ in 0..10 {
            assert!(matches!(buffer.tick(), Decision::Starved));
        }
        assert_eq!(buffer.metrics().loss_fraction(), 0.0);
        assert_eq!(buffer.metrics().frames_concealed, 0);
    }

    #[test]
    fn overflow_is_bounded_and_counted() {
        // specs/04-servidor-seele.md guards the server against a flooding sender;
        // the client must not fall over either.
        let mut buffer = buffer();
        for index in 0..(CAPACITY as u32 * 2) {
            buffer.push(
                index as u16 + 1,
                index * 960,
                f64::from(index),
                index as u16,
            );
        }
        assert!(buffer.metrics().overflow_discards > 0);
        assert!(buffer.depth_ms() <= CAPACITY as f64 * f64::from(crate::FRAME_MS));
    }

    /// **Com a fila no teto, um buraco não gasta tique**: ele é pulado de uma
    /// vez — sem gastar um tique por lugar, que era o que fazia o quadro vivo
    /// transbordar e reabrir o buraco adiante (o P1 de 25/10). Desde a LAN03 o
    /// corte da rajada atravessa o buraco no mesmo tique, e quem toca é o mais
    /// novo da fila, e não o primeiro depois do buraco.
    #[test]
    fn com_a_fila_no_teto_o_buraco_e_pulado_e_a_frente_toca() {
        let mut buffer = buffer();
        let quadro = crate::FRAME_SAMPLES as u32;
        for n in 1..=10_u32 {
            buffer.push(n as u16, n * quadro, f64::from(n) * 20.0, n as u16);
            let _ = buffer.tick();
        }
        // O pré-buffer atrasa um tique: o 10 toca agora, e a fila esvazia.
        assert_eq!(buffer.tick(), Decision::Play(10));
        // Cem quadros sumiram (11..=110), e os 256 seguintes chegam de uma vez.
        for n in 111..111 + CAPACITY as u32 {
            buffer.push(n as u16, n * quadro, 3_000.0, n as u16);
        }
        assert_eq!(buffer.queue.len(), CAPACITY);
        let decisao = buffer.tick();
        let m = buffer.metrics();
        assert!(
            matches!(decisao, Decision::Play(n) if n >= 360),
            "{decisao:?} {m:?}"
        );
        assert_eq!(m.buraco_pulado, 100);
        assert_eq!(m.frames_silenced + m.frames_concealed, 0, "{m:?}");
        assert_eq!(m.overflow_discards, 0);
    }

    /// **O cenário ORIGINAL da revisão do Astra** (25/10): um buraco
    /// 11..=110, os 254 quadros seguintes de uma vez e três atrasados de
    /// dentro do buraco chegando com a fila cheia.
    ///
    /// O que era correto mudou com a regra do buraco vencido: o primeiro tique
    /// não paga mais o buraco (antes, uma ocultação, e o buraco ficava
    /// pendente); o corte da rajada o atravessa e toca o presente. A asserção
    /// nova, explícita: o ponteiro fica coerente — nenhum buraco pendente, o
    /// que tocou está depois do buraco, os atrasados são recusados como
    /// atrasados (estão atrás do ponteiro) sem mexer nele, e os tiques
    /// seguintes tocam em ordem, um quadro por tique, sem andar duas vezes.
    #[test]
    fn o_cenario_original_do_buraco_pendente_com_a_fila_cheia_fica_coerente() {
        let mut buffer = buffer();
        let quadro = crate::FRAME_SAMPLES as u32;
        for n in 1..=10_u32 {
            buffer.push(n as u16, n * quadro, f64::from(n) * 20.0, n as u16);
            let _ = buffer.tick();
        }
        assert_eq!(buffer.tick(), Decision::Play(10));
        for n in 111..111 + 254_u32 {
            buffer.push(n as u16, n * quadro, 3_000.0, n as u16);
        }
        let Decision::Play(primeiro) = buffer.tick() else {
            panic!(
                "o primeiro tique depois do buraco não tocou: {:?}",
                buffer.metrics()
            );
        };
        assert!(primeiro >= 111, "{primeiro}");
        assert_eq!(buffer.gap_lost_left + buffer.gap_comfort_left, 0);
        let ponteiro = buffer.next_timestamp;
        assert_eq!(ponteiro, Some((u32::from(primeiro) + 1) * quadro));
        let atrasados_antes = buffer.metrics().late_discards;
        for n in 12..15_u32 {
            buffer.push(n as u16, n * quadro, 3_020.0, n as u16);
        }
        assert_eq!(
            buffer.next_timestamp, ponteiro,
            "um atrasado mexeu no ponteiro"
        );
        assert_eq!(buffer.metrics().late_discards, atrasados_antes + 3);
        let mut anterior = primeiro;
        while let Decision::Play(n) = buffer.tick() {
            assert_eq!(n, anterior + 1, "andou duas vezes, ou para trás");
            anterior = n;
        }
        assert_eq!(anterior, 111 + 253, "a fila não tocou até o fim");
    }

    /// **Com um buraco pendente, nem o push nem o tique cortam o mais velho**
    /// (a revisão do Astra, 25/10), no caminho do `push`: o buraco fica
    /// pendente com a fila no alvo e enche antes do tique seguinte. Quadros
    /// atrasados que caem dentro do buraco
    /// ficam na frente, contíguos a partir da vez; cortá-los avançaria o
    /// ponteiro da reprodução enquanto o buraco ainda conta os lugares dele, e
    /// o `drain_gap` avançaria de novo. O estado do buraco tem de continuar
    /// coerente: o ponteiro mais o que falta do buraco é a frente planejada.
    #[test]
    fn com_um_buraco_pendente_a_fila_cheia_nao_corta_o_mais_velho() {
        let mut buffer = buffer();
        let quadro = crate::FRAME_SAMPLES as u32;
        for n in 1..=10_u32 {
            buffer.push(n as u16, n * quadro, f64::from(n) * 20.0, n as u16);
            let _ = buffer.tick();
        }
        assert_eq!(buffer.tick(), Decision::Play(10));
        // Cem quadros sumiram (11..=110); dez chegam depois do buraco.
        for n in 111..121_u32 {
            buffer.push(n as u16, n * quadro, 3_000.0, n as u16);
        }
        // O tique acha o buraco e começa a pagá-lo: com a fila no alvo (bem
        // abaixo do excesso de rajada), ocultação.
        assert_eq!(buffer.tick(), Decision::Conceal);
        assert!(buffer.gap_lost_left > 0);
        // Antes do próximo tique, dois atrasados de dentro do buraco e o resto
        // da rajada, até a fila encher; o último atrasado chega com ela cheia,
        // acima do excesso de rajada: é o caminho do `push`.
        for n in 12..14_u32 {
            buffer.push(n as u16, n * quadro, 3_020.0, n as u16);
        }
        for n in 121..121 + 244_u32 {
            buffer.push(n as u16, n * quadro, 3_020.0, n as u16);
        }
        assert_eq!(buffer.queue.len(), CAPACITY);
        buffer.push(14, 14 * quadro, 3_020.0, 14);
        let pendente = buffer.gap_lost_left + buffer.gap_comfort_left;
        let planejada = buffer
            .next_timestamp
            .map(|ts| ts.wrapping_add(u32::try_from(pendente).unwrap() * quadro));
        assert_eq!(
            planejada,
            Some(111 * quadro),
            "o ponteiro andou com o buraco ainda pendente: {:?}",
            buffer.metrics()
        );
        assert_eq!(buffer.metrics().descartes_por_rajada, 0);
    }

    /// **O ponteiro nunca recua** (a revisão do Astra da regra do buraco
    /// vencido, 25/10): atrasados de dentro de um buraco que o `drain_gap`
    /// acabou de pular são passado, e o corte da rajada não pode escolher um
    /// deles como frente — o ponteiro voltaria de 111 para 47, tocaria o 47 e
    /// reabriria o buraco 48..110 com a fila já abaixo do limiar. Pela decisão
    /// de cada tique: o que toca nunca anda para trás, nada de 48..110 toca, e
    /// a voz não fica sem `Play` por mais de um tique.
    #[test]
    fn o_corte_depois_do_buraco_vencido_nunca_faz_o_ponteiro_recuar() {
        // O cenário exato da revisão (10 quadros depois do buraco) e um com
        // menos quadros depois dele (5), em que a frente retida cai com
        // folga entre os atrasados, sem depender do arredondamento do alvo.
        for depois_do_buraco in [10_u32, 5] {
            ponteiro_depois_do_buraco_vencido(depois_do_buraco, 47, 200.0);
        }
        // E a FILA PEQUENA depois do pulo: poucos atrasados (12..=20) e vinte
        // quadros depois do buraco, com o alvo de sempre. O pulo deixa a fila
        // abaixo do limiar da rajada, e não há corte: o passado tem de sair do
        // mesmo jeito, ou a frente fica atrás do ponteiro e o `plan_gap`
        // ressincroniza para trás.
        ponteiro_depois_do_buraco_vencido(20, 20, 40.0);
    }

    /// Um buraco 11..=110 com o ponteiro no 12, os atrasados 12..=47 e
    /// `depois` quadros a partir do 111 na fila, o alvo em 200 ms; e então
    /// cem tiques com um quadro vivo cada.
    fn ponteiro_depois_do_buraco_vencido(depois: u32, ultimo_atrasado: u32, alvo_ms: f64) {
        let mut buffer = buffer();
        let quadro = crate::FRAME_SAMPLES as u32;
        for n in 1..=10_u32 {
            buffer.push(n as u16, n * quadro, f64::from(n) * 20.0, n as u16);
            let _ = buffer.tick();
        }
        assert_eq!(buffer.tick(), Decision::Play(10));
        buffer.target_ms = alvo_ms;
        for n in 111..111 + depois {
            buffer.push(n as u16, n * quadro, 2_400.0, n as u16);
        }
        assert_eq!(buffer.tick(), Decision::Conceal);
        assert_eq!(buffer.next_timestamp, Some(12 * quadro));
        for n in 12..=ultimo_atrasado {
            buffer.push(n as u16, n * quadro, 2_420.0, n as u16);
        }
        assert_eq!(buffer.queue.len(), (ultimo_atrasado - 11 + depois) as usize);

        let mut tocados: Vec<u16> = Vec::new();
        let (mut sem_play, mut maior_sem_play) = (0_u32, 0_u32);
        for (tique, n) in (111 + depois..211 + depois).enumerate() {
            if tique > 0 {
                buffer.push(
                    n as u16,
                    n * quadro,
                    2_420.0 + 20.0 * tique as f64,
                    n as u16,
                );
            }
            match buffer.tick() {
                Decision::Play(n) => {
                    tocados.push(n);
                    sem_play = 0;
                }
                _ => {
                    sem_play += 1;
                    maior_sem_play = maior_sem_play.max(sem_play);
                }
            }
        }
        let m = buffer.metrics();
        println!("depois={depois}: tocados {tocados:?} maior_sem_play {maior_sem_play} {m:?}");
        assert!(
            tocados.windows(2).all(|par| matches!(par, [a, b] if a < b)),
            "depois={depois}: o que tocou andou para trás: {tocados:?} {m:?}"
        );
        assert!(
            tocados.iter().all(|n| *n >= 111),
            "depois={depois}: tocou passado de dentro do buraco pulado: {tocados:?} {m:?}"
        );
        assert!(
            maior_sem_play <= 1,
            "depois={depois}: {maior_sem_play} tiques sem Play (o buraco 48..110 reaberto?): {m:?}"
        );
        // O passado não conta como fila: ele não pode inflar a profundidade e
        // fazer o corte levar o presente junto. O primeiro a tocar é o 111.
        assert_eq!(
            tocados.first(),
            Some(&111),
            "depois={depois}: o presente não tocou primeiro: {tocados:?} {m:?}"
        );
    }

    /// **O corte da rajada atravessa o buraco** (a LAN03): os velhos de uma
    /// pausa, um buraco que o enlace deixou e os novos. Fica o mais novo, o
    /// ponteiro vai direto a ele, e o buraco no caminho é pulado e contado à
    /// parte — nada de silêncio nem ocultação por tempo que já passou.
    #[test]
    fn o_corte_da_rajada_atravessa_o_buraco_e_conta_os_dois_a_parte() {
        let mut buffer = buffer();
        let quadro = crate::FRAME_SAMPLES as u32;
        for n in 1..=10_u32 {
            buffer.push(n as u16, n * quadro, f64::from(n) * 20.0, n as u16);
            let _ = buffer.tick();
        }
        assert_eq!(buffer.tick(), Decision::Play(10));
        for n in (11..=60_u32).chain(91..=95) {
            buffer.push(n as u16, n * quadro, 2_000.0, n as u16);
        }
        let decisao = buffer.tick();
        let m = buffer.metrics();
        assert_eq!(decisao, Decision::Play(93), "{m:?}");
        assert_eq!(m.descartes_por_rajada, 52, "{m:?}");
        assert_eq!(m.buraco_pulado, 30, "{m:?}");
        assert_eq!(m.frames_silenced + m.frames_concealed, 0, "{m:?}");
        assert_eq!(buffer.tick(), Decision::Play(94));
        assert_eq!(buffer.tick(), Decision::Play(95));
    }

    /// **A rajada contígua de segundos sai de uma vez**, da frente, até o alvo;
    /// a de 400 ms, abaixo do limiar, fica para a compressão.
    #[test]
    fn a_rajada_acima_do_limiar_sai_de_uma_vez_e_a_curta_nao() {
        let quadro = crate::FRAME_SAMPLES as u32;
        for (quadros, cortado) in [(50_u32, true), (20, false)] {
            let mut buffer = buffer();
            for n in 1..=10_u32 {
                buffer.push(n as u16, n * quadro, f64::from(n) * 20.0, n as u16);
                let _ = buffer.tick();
            }
            for n in 11..11 + quadros {
                buffer.push(n as u16, n * quadro, 500.0, n as u16);
            }
            let decisao = buffer.tick();
            let m = buffer.metrics();
            assert!(matches!(decisao, Decision::Play(_)), "{decisao:?}");
            assert_eq!(m.descartes_por_rajada > 0, cortado, "{quadros}: {m:?}");
            assert_eq!(m.frames_concealed + m.frames_silenced, 0, "{m:?}");
            if cortado {
                assert!(m.depth_ms <= m.target_ms + 20.0, "{m:?}");
            }
        }
    }

    /// O controle: o mesmo buraco com a fila longe do teto continua sendo
    /// ocultado e silenciado lugar a lugar, como sempre.
    #[test]
    fn longe_do_teto_o_buraco_continua_ocultado_e_silenciado() {
        let mut buffer = buffer();
        let quadro = crate::FRAME_SAMPLES as u32;
        for n in 1..=10_u32 {
            buffer.push(n as u16, n * quadro, f64::from(n) * 20.0, n as u16);
            let _ = buffer.tick();
        }
        // O pré-buffer atrasa um tique: o 10 toca agora, e a fila esvazia.
        assert_eq!(buffer.tick(), Decision::Play(10));
        for n in 111..121_u32 {
            buffer.push(n as u16, n * quadro, 3_000.0, n as u16);
        }
        assert_eq!(buffer.tick(), Decision::Conceal);
        assert_eq!(buffer.tick(), Decision::Silence);
        assert_eq!(buffer.metrics().buraco_pulado, 0);
    }

    /// Fala contínua de `quadros` quadros a partir de `seq0`/`ts0`, chegando em
    /// tempo real desde `t0_ms`, com um tique por quadro. Devolve quantos
    /// tocaram.
    fn falar(buffer: &mut JitterBuffer<u16>, seq0: u16, ts0: u32, t0_ms: f64, quadros: u32) -> u32 {
        let mut tocados = 0;
        for indice in 0..quadros {
            buffer.push(
                seq0.wrapping_add(indice as u16),
                ts0.wrapping_add(indice * 960),
                t0_ms + f64::from(indice) * 20.0,
                indice as u16,
            );
            if matches!(buffer.tick(), Decision::Play(_)) {
                tocados += 1;
            }
        }
        tocados
    }

    /// O tempo que passa do lado de quem ouve sem nada chegar: um tique a
    /// cada 20 ms, como o laço de voz faz.
    fn calar(buffer: &mut JitterBuffer<u16>, quadros: u32) {
        for _ in 0..quadros {
            let _ = buffer.tick();
        }
    }

    /// 76 s de voz velha, e 22 s fora (o CUA de 25/10, 03:38–03:39), com quem
    /// ouve tocando o tempo todo. Devolve o buffer e o carimbo do próximo
    /// quadro da voz velha.
    fn a_voz_velha_e_a_saida(ts0: u32) -> (JitterBuffer<u16>, u32) {
        let mut buffer = buffer();
        let velha = 76 * 50;
        assert!(falar(&mut buffer, 0, ts0, 0.0, velha) > velha - 10);
        calar(&mut buffer, 22 * 50);
        (buffer, ts0.wrapping_add(velha * 960))
    }

    /// **O limite deste buffer, que é o motivo de quem manda não recomeçar o
    /// carimbo do zero** (`partida` em `crate::relogio`): uma fonte cujo
    /// carimbo volta para trás é descartada como atrasada até alcançar o que já
    /// tocou. Uma caracterização, e não um defeito a consertar aqui: o buffer
    /// não tem como separar «a fonte recomeçou» de «chegou a cauda velha
    /// atrasada» (revisão do Astra, 25/10), e decidir errado derruba quem está
    /// falando.
    #[test]
    fn um_carimbo_que_volta_para_tras_e_descartado_como_atrasado() {
        let (mut buffer, _) = a_voz_velha_e_a_saida(0);
        let tocados = falar(&mut buffer, 0, 0, 98_000.0, 100);
        assert_eq!(tocados, 0);
        assert!(buffer.metrics().late_discards >= 100);
    }

    /// **A conexão nova que parte do relógio da máquina toca logo**: o carimbo
    /// dela está à frente do da voz velha pelo tempo que a pessoa ficou fora, e
    /// o buffer lê o vão como a pausa que ele já tocou (os tiques da saída).
    #[test]
    fn um_carimbo_que_recomeca_a_frente_pelo_tempo_fora_toca_logo() {
        let (mut buffer, fim_da_velha) = a_voz_velha_e_a_saida(0);
        let nova = fim_da_velha.wrapping_add(22 * 48_000);
        let tocados = falar(&mut buffer, 0, nova, 98_000.0, 100);
        assert!(tocados >= 95, "a voz nova tocou só {tocados} de 100");
        assert_eq!(buffer.metrics().late_discards, 0);
    }

    /// **E através da volta do carimbo de 32 bits** (a cada 24,8 h): a conta
    /// circular de `ts_delta` lê a partida nova como à frente.
    #[test]
    fn um_carimbo_que_recomeca_a_frente_atraves_da_volta_dos_32_bits_toca_logo() {
        let (mut buffer, fim_da_velha) = a_voz_velha_e_a_saida(u32::MAX - 30 * 48_000);
        assert!(fim_da_velha < 76 * 48_000, "a voz velha não deu a volta");
        let nova = fim_da_velha.wrapping_add(22 * 48_000);
        let tocados = falar(&mut buffer, 0, nova, 98_000.0, 100);
        assert!(tocados >= 95, "a voz nova tocou só {tocados} de 100");
    }

    /// **Fora mais de um minuto**, o vão passa do `MAX_GAP_FRAMES`, e o buffer
    /// ressincroniza em vez de andar quadro a quadro: toca logo do mesmo jeito.
    #[test]
    fn um_carimbo_que_recomeca_muito_a_frente_ressincroniza_e_toca_logo() {
        let (mut buffer, fim_da_velha) = a_voz_velha_e_a_saida(0);
        calar(&mut buffer, 100 * 50);
        let nova = fim_da_velha.wrapping_add(122 * 48_000);
        let tocados = falar(&mut buffer, 0, nova, 198_000.0, 100);
        assert!(tocados >= 95, "a voz nova tocou só {tocados} de 100");
    }

    /// Quantos quadros a voz nova manda até o primeiro tocar, depois de uma
    /// voz velha de `minutos` e 22 s fora, com as duas vozes partindo de
    /// `partida` (o relógio da máquina, ou o zero de antes). Tempo simulado:
    /// uma hora de fala são 180 mil quadros, e não uma hora de teste.
    fn quadros_ate_tocar(minutos: u32, partida: impl Fn(u64) -> u32) -> Option<u32> {
        let mut buffer = buffer();
        let velha = minutos * 60 * 50;
        let t0_ms = 1_792_900_000_000_u64;
        let comeco = partida(t0_ms);
        for indice in 0..velha {
            buffer.push(
                indice as u16,
                comeco.wrapping_add(indice.wrapping_mul(960)),
                f64::from(indice) * 20.0,
                0,
            );
            let _ = buffer.tick();
        }
        calar(&mut buffer, 22 * 50);
        let nova = partida(t0_ms + u64::from(velha) * 20 + 22_000);
        for indice in 0..(velha + 10 * 50) {
            buffer.push(
                indice as u16,
                nova.wrapping_add(indice.wrapping_mul(960)),
                f64::from(velha + 22 * 50 + indice) * 20.0,
                0,
            );
            if matches!(buffer.tick(), Decision::Play(_)) {
                return Some(indice);
            }
        }
        None
    }

    /// **Com a partida no relógio da máquina, o mudo depois da volta não
    /// depende de quanto a voz velha falou** (o relato: 15 min a 1 h de
    /// sessão). E com a partida no zero, a de antes, ele é a vida da voz velha
    /// inteira: com uma hora, uma hora de «sem áudio».
    #[test]
    fn o_mudo_da_volta_nao_cresce_com_a_duracao_da_fala() {
        let da_maquina = |ms: u64| crate::relogio::partida(std::time::Duration::from_millis(ms)).1;
        for minutos in [1, 15, 60] {
            let quadros = quadros_ate_tocar(minutos, da_maquina);
            assert!(
                quadros.is_some_and(|quadros| quadros <= 3),
                "depois de {minutos} min de fala, a voz nova esperou {quadros:?} quadros"
            );
        }
        // A de antes, para a conta ficar à vista: 15 min de fala, 15 min mudo.
        let do_zero = quadros_ate_tocar(15, |_| 0);
        assert!(
            do_zero.is_some_and(|quadros| quadros >= 15 * 60 * 50 - 50),
            "do zero, o mudo foi de {do_zero:?} quadros"
        );
    }
}
