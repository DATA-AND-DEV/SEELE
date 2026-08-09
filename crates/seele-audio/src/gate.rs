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
//! That last line is the design: whatever the user chose, everything downstream
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
/// About −34 dBFS. Above room tone and laptop fan, below a quiet speaking voice.
const OPEN_RMS: f32 = 0.02;

/// How quiet it must fall before the gate closes again.
///
/// About −40 dBFS. Deliberately lower than [`OPEN_RMS`]: with a single
/// threshold, a voice hovering around it chatters the gate open and shut several
/// times a second, which is far more distracting than either state.
const CLOSE_RMS: f32 = 0.01;

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
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            open_rms: OPEN_RMS,
            close_rms: CLOSE_RMS,
            hangover_ms: HANGOVER_MS,
            frame_ms: crate::FRAME_MS,
        }
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
            metrics: GateMetrics::default(),
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
    pub fn set_mode(&mut self, mode: GateMode) {
        self.mode = mode;
        self.speaking = false;
        self.hangover_frames_left = 0;
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

        self.speaking
    }

    /// Hysteresis plus hangover.
    fn voice_decision(&mut self, level: f32) -> bool {
        let hangover_frames = self
            .config
            .hangover_ms
            .div_ceil(self.config.frame_ms.max(1));

        if self.speaking {
            if level >= self.config.close_rms {
                // Still above the closing threshold: refresh the hangover.
                self.hangover_frames_left = hangover_frames;
                return true;
            }
            self.hangover_frames_left = self.hangover_frames_left.saturating_sub(1);
            return self.hangover_frames_left > 0;
        }

        if level >= self.config.open_rms {
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

    #[test]
    fn the_closing_threshold_is_below_the_opening_one() {
        // If these are ever equal the hysteresis silently disappears and the
        // gate starts chattering, which is hard to spot from behaviour alone.
        let config = GateConfig::default();
        assert!(config.close_rms < config.open_rms);
    }
}
