//! The voice path, as one thing a shell can hold.
//!
//! `specs/01-arquitetura.md` puts all session, protocol and audio logic in this
//! crate, and ADR 0002 keeps `seele-tui` from depending on `seele-audio` at all.
//! Those two together mean the capture → encode → send → receive → jitter →
//! decode → mix → play loop cannot live in the interface, however convenient
//! that would be. The M2 spike put it in the client binary because a spike is
//! allowed to; this is the version that survives.
//!
//! What a shell gets is a handle with verbs from the product's own vocabulary —
//! A.T. Field, Isolamento total, push-to-talk — and a telemetry snapshot. What
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
use std::sync::{Arc, Mutex};
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
use seele_audio::telemetry::{AudioTelemetry, FalhaLocal, LocalTelemetry, SourceTelemetry};
use seele_audio::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use seele_proto::ids::Ssrc;
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
}

/// What the person has set, read by the audio thread every frame.
///
/// Atomics rather than a lock: this is read from the thread that must not
/// block, and every field is one machine word.
#[derive(Debug)]
struct Controls {
    at_field: AtomicBool,
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
/// `seele-ffi` and `seele-tui` from naming `seele-audio` at all, and a
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
    fn wanted(&self) -> device::Wanted<'_> {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceRates {
    /// Native capture rate. Not necessarily 48 kHz — gap G7.
    pub capture_hz: u32,
    /// Native playback rate.
    pub playback_hz: u32,
}

/// Opens `chosen`, giving up the preference each failure blames, one at a time.
///
/// The rule this encodes, and the reason it is one function rather than a
/// pattern each shell repeats: **a preference written down yesterday never
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
#[derive(Debug)]
pub struct Voice {
    controls: Arc<Controls>,
    telemetry: Arc<Mutex<AudioTelemetry>>,
    rates: DeviceRates,
    /// The microphone this path actually opened.
    ///
    /// Read off the device rather than remembered from the request, so that a
    /// path started with no preference can still say which microphone it got.
    capture: Option<CaptureDevice>,
    /// Where this path is actually playing.
    ///
    /// Same rule as [`Voice::capture`], and it carries more weight here: a
    /// fallback to the machine's default output makes no sound of its own, so
    /// reading this channel is the only way to find out it happened.
    playback: Option<PlaybackDevice>,
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
        let rates = DeviceRates {
            capture_hz: io.capture_rate_hz,
            playback_hz: io.playback_rate_hz,
        };
        let capture = io.capture.clone().map(into_core);
        let playback = io.playback.clone().map(playback_into_core);

        let controls = Arc::new(Controls {
            at_field: AtomicBool::new(false),
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
        });
        let telemetry = Arc::new(Mutex::new(AudioTelemetry::default()));

        let thread_controls = Arc::clone(&controls);
        let thread_telemetry = Arc::clone(&telemetry);
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
                runtime.block_on(pipeline(io, media, ssrc, thread_controls, thread_telemetry));
            })?;

        Ok(Self {
            controls,
            telemetry,
            rates,
            capture,
            playback,
            chosen,
            falha_local: Mutex::new(FalhaLocal::new()),
        })
    }

    /// What the devices actually run at.
    #[must_use]
    pub fn rates(&self) -> DeviceRates {
        self.rates
    }

    /// The microphone this path is actually capturing from.
    ///
    /// `None` when the backend would not describe the device it opened. Audio is
    /// still running in that case — an interface must draw an unnamed device,
    /// not a missing one.
    #[must_use]
    pub fn capture(&self) -> Option<&CaptureDevice> {
        self.capture.as_ref()
    }

    /// Where this path is actually playing.
    ///
    /// `None` under the same two conditions as [`Voice::capture`], and read for
    /// the same reason: what opened, never what was asked for. It is the only
    /// evidence a person gets that a chosen output was not there — nothing about
    /// falling back to the machine's speakers announces itself.
    #[must_use]
    pub fn playback(&self) -> Option<&PlaybackDevice> {
        self.playback.as_ref()
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
    /// A.T. Field, Isolamento total, the mode, the held key and every per-talker
    /// volume. Left to a shell, this list would be written once per shell and
    /// each copy would forget a different item — and the item that hurts is
    /// A.T. Field: a switch that quietly unmutes a microphone is a switch that
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
        fresh.set_at_field(self.at_field());
        fresh.set_total_isolation(self.total_isolation());
        fresh.set_mode(self.mode());
        fresh.set_key_held(self.controls.key_held.load(Ordering::Relaxed));
        // O relógio de mídia junto, e é o item cuja falta calava a pessoa. Ver
        // `Controls::relogio_seq` para o porquê e `salto_do_relogio` para o
        // tamanho do pulo.
        let (seq, carimbo) = Self::salto_do_relogio(
            self.controls.relogio_seq.load(Ordering::Relaxed) as u16,
            self.controls.relogio_carimbo.load(Ordering::Relaxed),
        );
        fresh
            .controls
            .relogio_seq
            .store(u32::from(seq), Ordering::Relaxed);
        fresh
            .controls
            .relogio_carimbo
            .store(carimbo, Ordering::Relaxed);
        if let Ok(gains) = self.controls.gains.lock() {
            for (talker, gain) in gains.iter() {
                fresh.set_gain(*talker, *gain);
            }
        }
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

    /// Mutes the microphone — A.T. Field.
    pub fn set_at_field(&self, on: bool) {
        self.controls.at_field.store(on, Ordering::Relaxed);
    }

    /// Whether the microphone is muted.
    #[must_use]
    pub fn at_field(&self) -> bool {
        self.controls.at_field.load(Ordering::Relaxed)
    }

    /// Mutes the speakers — Isolamento total.
    pub fn set_total_isolation(&self, on: bool) {
        self.controls.total_isolation.store(on, Ordering::Relaxed);
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
    let (Ok(mut to_pipeline), Ok(mut to_device)) = (
        RateConverter::new(io.capture_rate_hz, SAMPLE_RATE_HZ),
        RateConverter::new_adjustable(SAMPLE_RATE_HZ, io.playback_rate_hz),
    ) else {
        return;
    };
    let anel_de_saida = io.to_device.buffer().capacity();
    let mut ritmo = RingPacer::new(io.playback_rate_hz, anel_de_saida);
    let mut ritmo_avisado = false;

    let mut gate = VoiceGate::new(GateConfig::default(), GateMode::PushToTalk);
    let mut mixer = Mixer::new();
    let mut sources: Vec<Source> = Vec::new();

    let (mut captured, mut at_48k, mut pending) = (Vec::new(), Vec::new(), Vec::<f32>::new());
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

        let at_field = controls.at_field.load(Ordering::Relaxed);
        while pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();
            // The gate still runs while muted, so the level meter keeps moving
            // and somebody talking into a muted microphone can see that they
            // are. Not showing that is how people give whole speeches to nobody.
            let open = gate.update(&frame);
            let speaking = open && !at_field;
            controls.speaking.store(speaking, Ordering::Relaxed);

            // The timestamp counts elapsed samples whether or not anything goes
            // out; the sequence counts only what does. That difference is what
            // lets the receiver tell DTX silence from real loss — M1.9.
            timestamp = timestamp.wrapping_add(u32::try_from(FRAME_SAMPLES).unwrap_or(FRAME_MS));
            controls
                .relogio_carimbo
                .store(timestamp, Ordering::Relaxed);
            if !speaking {
                continue;
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
                timestamp,
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
