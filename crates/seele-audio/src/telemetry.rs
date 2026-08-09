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
//! No formatting, no colour, no bands. `specs/07-tema-evangelion.md` defines
//! four Sync Ratio bands with four colours, and `specs/05-cliente-tui.md`
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
//! `specs/07-tema-evangelion.md` makes the per-person Sync Ratio the signature
//! element of the product: "each pilot in the roster has a live percentage
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
        }
    }

    /// True when this machine, rather than the network, is dropping audio.
    ///
    /// Worth separating because the two look identical to a listener and have
    /// opposite fixes.
    #[must_use]
    pub fn local_fault(&self) -> bool {
        self.capture_overruns > 0 || self.playback_underruns > 0 || self.device_errors > 0
    }
}

/// Metrics about one incoming stream — one talker, one `ssrc`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SourceTelemetry {
    /// Which stream this describes. Assigned by the server on Cage entry
    /// (`specs/02-protocolo.md`); the client maps it to a pilot from the control
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
    /// `specs/07-tema-evangelion.md` puts a percentage next to each pilot.
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
    fn local_telemetry_separates_this_machine_from_the_network() {
        // A listener hearing gaps cannot tell an overrun from packet loss, and
        // the two have opposite fixes.
        let healthy = LocalTelemetry::default();
        assert!(!healthy.local_fault());

        let stalling = LocalTelemetry {
            capture_overruns: 12,
            ..LocalTelemetry::default()
        };
        assert!(stalling.local_fault());

        let unplugged = LocalTelemetry {
            device_errors: 1,
            ..LocalTelemetry::default()
        };
        assert!(unplugged.local_fault());
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
    fn per_source_numbers_survive_aggregation() {
        // specs/07-tema-evangelion.md puts a live percentage next to each pilot,
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
}
