//! The Sync Ratio — Taxa de Sincronização.
//!
//! `specs/07-tema-evangelion.md` calls this the product's signature element:
//!
//! > **A Taxa de Sincronização por pessoa.** Cada piloto no roster tem um
//! > percentual vivo derivado do RTT, jitter e perda daquela conexão. Nenhum
//! > concorrente mostra isso; aqui é a coisa mais visível da tela. É a métrica
//! > que dá caráter ao produto e, não por acaso, é genuinamente útil — quando
//! > alguém fica difícil de entender, todo mundo já sabe por quê.
//!
//! `specs/02-protocolo.md` insists it is "derived, not invented", and gives the
//! shape:
//!
//! ```text
//! sync = 100
//!      − penalidade_rtt(rtt_ms)          # até 40 pontos
//!      − penalidade_jitter(jitter_ms)    # até 30 pontos
//!      − penalidade_perda(perda_pct)     # até 30 pontos, mais agressiva
//! ```
//!
//! # A sign error in the spec
//!
//! `specs/02-protocolo.md` annotates the RTT term as *"0 acima de 40 ms, cresce
//! até 40 pontos"* — zero **above** 40 ms. Read literally that means a bad
//! connection is the one that goes unpenalised, which inverts the whole metric.
//! It is contradiction C6 in `docs/plano-m0-m1.md`, and this implements the
//! evident intent: **no penalty below 40 ms, growing above it**.
//!
//! # Why loss is punished hardest
//!
//! The spec says "mais agressiva" without saying how much. Loss is the only one
//! of the three that destroys information rather than delaying it: 200 ms of
//! extra latency is annoying, 5 % loss is words missing from sentences. The
//! curve here is therefore steep near zero — the first percent costs far more
//! than the tenth.

use serde::{Deserialize, Serialize};

/// RTT below which there is no penalty, in milliseconds.
pub const RTT_FREE_MS: f32 = 40.0;
/// RTT at which the penalty saturates.
pub const RTT_FULL_MS: f32 = 400.0;
/// Points the RTT term can take. `specs/02-protocolo.md`.
pub const RTT_MAX_PENALTY: f32 = 40.0;

/// Jitter below which there is no penalty, in milliseconds.
///
/// A few milliseconds of jitter is absorbed by the buffer at its 20 ms floor
/// without costing anybody anything, so it should not cost the score either.
pub const JITTER_FREE_MS: f32 = 5.0;
/// Jitter at which the penalty saturates.
pub const JITTER_FULL_MS: f32 = 100.0;
/// Points the jitter term can take.
pub const JITTER_MAX_PENALTY: f32 = 30.0;

/// Loss at which the penalty saturates, as a fraction.
pub const LOSS_FULL: f32 = 0.10;
/// Points the loss term can take.
pub const LOSS_MAX_PENALTY: f32 = 30.0;

/// Smoothing factor for the moving average. `specs/02-protocolo.md`: α ≈ 0,2.
///
/// The spec's reason is presentational — "para não piscar" — and it is a real
/// requirement: this number is on screen next to every pilot, and one that
/// jumps around is noise a user learns to ignore.
pub const SMOOTHING: f32 = 0.2;

/// What band a Sync Ratio falls into.
///
/// `specs/07-tema-evangelion.md` gives each a colour. The colour is **not** here:
/// that is the shell's decision, and `specs/05-cliente-tui.md` requires a
/// no-colour mode where the band still has to be conveyable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SyncBand {
    /// Below 40. Something is wrong.
    Critical,
    /// 40 to 69.
    Degraded,
    /// 70 to 89.
    Acceptable,
    /// 90 and above.
    Nominal,
}

impl SyncBand {
    /// Which band a ratio falls into. `specs/07-tema-evangelion.md`.
    #[must_use]
    pub fn of(ratio: u8) -> Self {
        match ratio {
            90..=u8::MAX => Self::Nominal,
            70..=89 => Self::Acceptable,
            40..=69 => Self::Degraded,
            _ => Self::Critical,
        }
    }
}

/// The three measurements the ratio is derived from.
///
/// `specs/03-audio.md` produces jitter and loss; RTT comes from `Ping`/`Pong` on
/// the control stream, which is why `magi-audio` cannot compute this alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncInputs {
    /// Round trip, milliseconds.
    pub rtt_ms: f32,
    /// Arrival jitter, milliseconds.
    pub jitter_ms: f32,
    /// Packet loss, 0.0 to 1.0.
    pub loss_fraction: f32,
}

/// Penalty for round-trip time, 0 to [`RTT_MAX_PENALTY`].
///
/// Zero **below** [`RTT_FREE_MS`], not above — see the module docs on C6.
#[must_use]
pub fn rtt_penalty(rtt_ms: f32) -> f32 {
    if !rtt_ms.is_finite() || rtt_ms <= RTT_FREE_MS {
        return 0.0;
    }
    let over = (rtt_ms - RTT_FREE_MS) / (RTT_FULL_MS - RTT_FREE_MS);
    (over * RTT_MAX_PENALTY).min(RTT_MAX_PENALTY)
}

/// Penalty for arrival jitter, 0 to [`JITTER_MAX_PENALTY`].
#[must_use]
pub fn jitter_penalty(jitter_ms: f32) -> f32 {
    if !jitter_ms.is_finite() || jitter_ms <= JITTER_FREE_MS {
        return 0.0;
    }
    let over = (jitter_ms - JITTER_FREE_MS) / (JITTER_FULL_MS - JITTER_FREE_MS);
    (over * JITTER_MAX_PENALTY).min(JITTER_MAX_PENALTY)
}

/// Penalty for packet loss, 0 to [`LOSS_MAX_PENALTY`].
///
/// Steeper than linear near zero, because loss removes information rather than
/// delaying it. The first half percent already costs about a fifth of the term.
#[must_use]
pub fn loss_penalty(loss_fraction: f32) -> f32 {
    if !loss_fraction.is_finite() || loss_fraction <= 0.0 {
        return 0.0;
    }
    let scaled = (loss_fraction / LOSS_FULL).min(1.0);
    scaled.sqrt() * LOSS_MAX_PENALTY
}

/// The instantaneous ratio, before smoothing.
#[must_use]
pub fn raw(inputs: SyncInputs) -> f32 {
    let penalties = rtt_penalty(inputs.rtt_ms)
        + jitter_penalty(inputs.jitter_ms)
        + loss_penalty(inputs.loss_fraction);
    (100.0 - penalties).clamp(0.0, 100.0)
}

/// A Sync Ratio that smooths over time.
///
/// One per pilot in the roster, since `specs/07-tema-evangelion.md` makes the
/// number per-person.
#[derive(Debug, Clone, Copy)]
pub struct SyncRatio {
    smoothed: Option<f32>,
}

impl Default for SyncRatio {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncRatio {
    /// A ratio with no measurements yet.
    #[must_use]
    pub fn new() -> Self {
        Self { smoothed: None }
    }

    /// Folds in one measurement and returns the smoothed value.
    ///
    /// The first measurement is taken whole: starting from zero and easing up
    /// would show every pilot as critical for the first second of every call.
    pub fn update(&mut self, inputs: SyncInputs) -> u8 {
        let instant = raw(inputs);
        let smoothed = match self.smoothed {
            None => instant,
            Some(previous) => previous + (instant - previous) * SMOOTHING,
        };
        self.smoothed = Some(smoothed);
        self.value()
    }

    /// The current value, 0 to 100.
    ///
    /// A connection with no measurements yet reads as 100 rather than 0: the
    /// pilot has just arrived and nothing is known to be wrong with them.
    #[must_use]
    pub fn value(&self) -> u8 {
        self.smoothed
            .map_or(100, |value| value.round().clamp(0.0, 100.0) as u8)
    }

    /// The band the current value falls into.
    #[must_use]
    pub fn band(&self) -> SyncBand {
        SyncBand::of(self.value())
    }

    /// Whether anything has been measured yet.
    #[must_use]
    pub fn has_measurement(&self) -> bool {
        self.smoothed.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(rtt: f32, jitter: f32, loss: f32) -> SyncInputs {
        SyncInputs {
            rtt_ms: rtt,
            jitter_ms: jitter,
            loss_fraction: loss,
        }
    }

    #[test]
    fn a_perfect_connection_scores_one_hundred() {
        assert_eq!(raw(inputs(2.0, 0.3, 0.0)).round() as u8, 100);
    }

    #[test]
    fn the_rtt_penalty_grows_above_forty_milliseconds_not_below() {
        // Contradiction C6. specs/02-protocolo.md says "0 acima de 40 ms",
        // which read literally means a bad connection is the unpenalised one.
        // If this test ever inverts, the product's signature metric is lying.
        assert_eq!(rtt_penalty(0.0), 0.0);
        assert_eq!(rtt_penalty(39.0), 0.0);
        assert_eq!(rtt_penalty(40.0), 0.0);
        assert!(rtt_penalty(80.0) > 0.0, "80 ms should cost something");
        assert!(
            rtt_penalty(300.0) > rtt_penalty(80.0),
            "a worse rtt must cost more, not less"
        );
    }

    #[test]
    fn every_penalty_respects_its_ceiling() {
        // specs/02-protocolo.md: 40, 30 and 30. They sum to exactly 100, so a
        // term that overshot could push the score negative.
        assert!(rtt_penalty(f32::MAX) <= 40.0);
        assert!(jitter_penalty(f32::MAX) <= 30.0);
        assert!(loss_penalty(1.0) <= 30.0);
        assert_eq!(raw(inputs(10_000.0, 10_000.0, 1.0)), 0.0);
    }

    #[test]
    fn loss_is_punished_harder_than_delay() {
        // "mais agressiva" in specs/02-protocolo.md. Loss destroys information;
        // latency only delays it. Half a percent of loss should already outweigh
        // a moderate rtt.
        assert!(
            loss_penalty(0.005) > rtt_penalty(60.0),
            "0.5% loss should cost more than a 60 ms round trip"
        );
    }

    #[test]
    fn the_loss_curve_is_steep_near_zero() {
        // The first percent must cost far more than the tenth, or a connection
        // dropping words reads as merely slow.
        let first = loss_penalty(0.01) - loss_penalty(0.0);
        let tenth = loss_penalty(0.10) - loss_penalty(0.09);
        assert!(
            first > tenth * 3.0,
            "the curve is not steep near zero: {first:.2} vs {tenth:.2}"
        );
    }

    #[test]
    fn the_bands_match_the_theme_document() {
        // specs/07-tema-evangelion.md: >= 90 nominal, 70-89 aceitável,
        // 40-69 degradado, < 40 crítico.
        assert_eq!(SyncBand::of(100), SyncBand::Nominal);
        assert_eq!(SyncBand::of(90), SyncBand::Nominal);
        assert_eq!(SyncBand::of(89), SyncBand::Acceptable);
        assert_eq!(SyncBand::of(70), SyncBand::Acceptable);
        assert_eq!(SyncBand::of(69), SyncBand::Degraded);
        assert_eq!(SyncBand::of(40), SyncBand::Degraded);
        assert_eq!(SyncBand::of(39), SyncBand::Critical);
        assert_eq!(SyncBand::of(0), SyncBand::Critical);
    }

    #[test]
    fn the_measured_network_profiles_land_in_sensible_bands() {
        // Calibration against the numbers M1 actually measured, from
        // docs/m1-medicoes.md. A metric that called a good Wi-Fi connection
        // critical would be worse than no metric.
        let lan = raw(inputs(2.0, 0.3, 0.0)).round() as u8;
        assert_eq!(SyncBand::of(lan), SyncBand::Nominal, "lan scored {lan}");

        let wifi = raw(inputs(28.0, 4.7, 0.0109)).round() as u8;
        assert!(
            SyncBand::of(wifi) >= SyncBand::Acceptable,
            "household wifi scored {wifi}"
        );

        let regional = raw(inputs(75.0, 4.5, 0.0061)).round() as u8;
        assert!(
            SyncBand::of(regional) >= SyncBand::Acceptable,
            "regional internet scored {regional}"
        );

        let mobile = raw(inputs(160.0, 21.9, 0.0434)).round() as u8;
        assert_eq!(
            SyncBand::of(mobile),
            SyncBand::Degraded,
            "a bad mobile link scored {mobile}"
        );
    }

    #[test]
    fn smoothing_stops_the_number_flickering() {
        // specs/02-protocolo.md asks for an exponential moving average "para não
        // piscar". A number that jumps every frame is one users learn to ignore.
        let mut ratio = SyncRatio::new();
        ratio.update(inputs(2.0, 0.3, 0.0));
        let steady = ratio.value();

        // One bad sample must not swing the display.
        let after_spike = ratio.update(inputs(300.0, 80.0, 0.08));
        assert!(
            steady - after_spike < 25,
            "one bad sample moved the score by {} points",
            steady - after_spike
        );
    }

    #[test]
    fn a_sustained_problem_does_arrive() {
        // The other half: smoothing must not hide a connection that is genuinely
        // bad, only stop it flickering.
        let mut ratio = SyncRatio::new();
        ratio.update(inputs(2.0, 0.3, 0.0));
        for _ in 0..40 {
            ratio.update(inputs(300.0, 80.0, 0.08));
        }
        assert_eq!(ratio.band(), SyncBand::Critical);
    }

    #[test]
    fn the_first_measurement_is_taken_whole() {
        // Easing up from zero would show every pilot as critical for the first
        // second of every call.
        let mut ratio = SyncRatio::new();
        assert_eq!(ratio.update(inputs(2.0, 0.3, 0.0)), 100);
    }

    #[test]
    fn a_pilot_with_no_measurements_reads_as_nominal() {
        // They just arrived. Nothing is known to be wrong with them, and showing
        // a red zero would be an accusation rather than a measurement.
        let ratio = SyncRatio::new();
        assert_eq!(ratio.value(), 100);
        assert_eq!(ratio.band(), SyncBand::Nominal);
        assert!(!ratio.has_measurement());
    }

    #[test]
    fn nonsense_input_cannot_produce_a_nonsense_score() {
        // The fuzzer found NaN reaching Telemetry in M2.1. Even with validation
        // upstream, the arithmetic here must not produce a score outside 0-100
        // that a shell would then have no band for.
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            let score = raw(inputs(bad, bad, bad));
            assert!(
                (0.0..=100.0).contains(&score),
                "input {bad} produced a score of {score}"
            );
        }
    }
}
