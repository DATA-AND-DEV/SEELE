//! The `cpal` seam: turns real devices into the ring buffers in [`crate::rt`].
//!
//! **This module is deliberately thin.** CI has no sound card, so almost
//! nothing here can be covered by a test. Nearly every channel in this file is a
//! channel no test protects, so the logic lives in [`crate::rt`] — which is fully
//! tested — and this file does nothing but wire it up.
//!
//! When reviewing a change here, the question is not "is this correct?" but
//! "could this have gone in `rt` instead?".
//!
//! The exception is device *identity*, added with the capture picker: an id that
//! does not name a device has to come back as [`DeviceError::CaptureDeviceGone`]
//! on a machine with no sound card as surely as on one with six, so that much is
//! testable everywhere. Everything below it that needs a microphone says so and
//! skips. [`DeviceError::side`] is the second such piece: which half of the pair
//! a failure blames is a total function over the enum, and a caller deciding
//! which preference to give up depends on it being right.

use std::num::NonZeroU16;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer};
use thiserror::Error;

use crate::rt::{
    capacity_for_ms, capture_path, playback_path, CaptureSink, PlaybackSource, RawSample,
    StreamCounters,
};
use crate::SAMPLE_RATE_HZ;

/// Builds an input stream for one concrete device sample format.
///
/// Generic so that every format shares the single tested implementation in
/// [`crate::rt`]; the only thing that varies is which `T` `cpal` hands over.
fn input_stream<T: RawSample + cpal::SizedSample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut sink: CaptureSink,
    errors: Arc<StreamCounters>,
) -> Result<cpal::Stream, cpal::Error> {
    device.build_input_stream(
        config,
        move |data: &[T], _info: &cpal::InputCallbackInfo| sink.on_capture(data),
        // Counting is all this does today. Task M1.14 grows device hot-swap out
        // of this seam; what matters now is that an unplugged headset is never
        // silently swallowed.
        move |_error: cpal::Error| errors.record_stream_error(),
        None,
    )
}

/// Builds an output stream for one concrete device sample format.
fn output_stream<T: RawSample + cpal::SizedSample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut source: PlaybackSource,
    errors: Arc<StreamCounters>,
) -> Result<cpal::Stream, cpal::Error> {
    device.build_output_stream(
        config,
        move |data: &mut [T], _info: &cpal::OutputCallbackInfo| source.on_playback(data),
        move |_error: cpal::Error| errors.record_stream_error(),
        None,
    )
}

/// Which half of the device pair a failure came from.
/// Acha um dispositivo pelo id, ou o padrão do sistema.
///
/// Pública dentro do crate para `laco.rs`, que precisa achar uma **saída** para
/// abri-la como entrada — é assim que o loopback do WASAPI se liga.
pub(crate) fn resolver(
    host: &cpal::Host,
    wanted: Option<&str>,
    side: Side,
) -> Result<cpal::Device, DeviceError> {
    resolve(host, wanted, side)
}

/// Abre um fluxo de entrada sobre um dispositivo e uma configuração já
/// negociados.
///
/// O mesmo caminho que a voz usa, num ponto em que `laco.rs` pode entrar: ele
/// tem uma configuração vinda de `default_input_config` numa saída, e não passa
/// por `negociar`.
pub(crate) fn abrir_entrada(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    sink: CaptureSink,
    counters: Arc<StreamCounters>,
) -> Result<cpal::Stream, DeviceError> {
    let formato = config.sample_format();
    let stream_config = config.config();
    let feito = match formato {
        cpal::SampleFormat::F32 => input_stream::<f32>(device, stream_config, sink, counters),
        cpal::SampleFormat::I16 => input_stream::<i16>(device, stream_config, sink, counters),
        cpal::SampleFormat::U16 => input_stream::<u16>(device, stream_config, sink, counters),
        outro => {
            return Err(DeviceError::UnsupportedSampleFormat {
                side: Side::Input,
                format: outro,
            })
        }
    };
    feito.map_err(|source| DeviceError::Device {
        side: Side::Input,
        stage: Stage::Build,
        source,
    })
}

/// Which half of the device pair a failure came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Capture.
    Input,
    /// Playback.
    Output,
}

impl std::fmt::Display for Side {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Input => "input",
            Self::Output => "output",
        })
    }
}

/// Which operation on the device failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Reading the device's own default configuration.
    Config,
    /// Creating the stream.
    Build,
    /// Starting a stream that was created successfully.
    Start,
}

impl std::fmt::Display for Stage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Config => "read the configuration of",
            Self::Build => "build",
            Self::Start => "start",
        })
    }
}

/// Why audio could not be opened.
///
/// `specs/02-protocolo.md` requires enumerated reasons, never free-form strings
/// reaching an interface. The `Display` text here is for `tracing`; a shell must
/// match on the variant — see ADR 0012.
#[derive(Debug, Error)]
pub enum DeviceError {
    /// The host reports no default input. On macOS this is usually the
    /// microphone permission rather than a missing device.
    #[error("no default input device")]
    NoInputDevice,

    /// A capture device was asked for by id and the host does not offer it.
    ///
    /// Its own variant rather than [`DeviceError::NoInputDevice`] because the
    /// two are different sentences for a person: "there is no microphone" and
    /// "the microphone you picked was unplugged" ask for different next steps.
    /// `specs/02-protocolo.md` wants enumerated reasons for exactly this.
    #[error("no capture device with id {id}")]
    CaptureDeviceGone {
        /// What was asked for, as [`CaptureDevice::id`] spells it.
        id: String,
    },

    /// The host reports no default output.
    #[error("no default output device")]
    NoOutputDevice,

    /// A playback device was asked for by id and the host does not offer it.
    ///
    /// The twin of [`DeviceError::CaptureDeviceGone`], and separate from
    /// [`DeviceError::NoOutputDevice`] for the same reason: "this machine has no
    /// speakers" and "the headset you picked was unplugged" are two different
    /// sentences with two different next steps.
    #[error("no playback device with id {id}")]
    PlaybackDeviceGone {
        /// What was asked for, as [`PlaybackDevice::id`] spells it.
        id: String,
    },

    /// The device speaks a sample format this build does not convert yet.
    ///
    /// Only `f32` is handled today. Integer formats arrive with task M1.4, which
    /// is where sample-format and sample-rate conversion belongs — see gap G7 in
    /// `docs/plano-m0-m1.md`. Failing loudly beats silently playing noise.
    #[error("{side} device uses sample format {format:?}, only f32 is supported so far")]
    UnsupportedSampleFormat {
        /// Which side failed.
        side: Side,
        /// The format the device asked for.
        format: cpal::SampleFormat,
    },

    /// The backend refused an operation on one of the streams.
    #[error("could not {stage} the {side} stream: {source}")]
    Device {
        /// Which side failed.
        side: Side,
        /// What was being attempted.
        stage: Stage,
        /// Underlying `cpal` error.
        #[source]
        source: cpal::Error,
    },
}

impl DeviceError {
    /// Which half of the pair this failure is about.
    ///
    /// Total over the enum on purpose. A caller holding one preference per side
    /// has to know which one a failure blames before it can give that one up:
    /// dropping the other would keep the device that would not open and throw
    /// away a choice that was fine. There is nothing here a shell should print —
    /// this is for deciding what to try next.
    #[must_use]
    pub fn side(&self) -> Side {
        match self {
            Self::NoInputDevice | Self::CaptureDeviceGone { .. } => Side::Input,
            Self::NoOutputDevice | Self::PlaybackDeviceGone { .. } => Side::Output,
            Self::UnsupportedSampleFormat { side, .. } | Self::Device { side, .. } => *side,
        }
    }
}

/// Everything the processing thread needs to talk to the devices.
///
/// Dropping this stops both streams. `cpal::Stream` is not `Send` on every
/// backend, so this must be built and dropped on the same thread.
pub struct AudioIo {
    /// Held to keep the streams alive. Dropping them stops audio.
    _input: cpal::Stream,
    _output: cpal::Stream,

    /// Captured mono samples, at [`AudioIo::capture_rate_hz`].
    pub captured: Consumer<f32>,
    /// Mono samples to play, at [`AudioIo::playback_rate_hz`].
    pub to_device: Producer<f32>,
    /// Counters shared by both callbacks.
    pub counters: Arc<StreamCounters>,

    /// Native capture rate. **Not necessarily 48 kHz** — resampling is task M1.4
    /// and gap G7. Reported rather than assumed, precisely so the gap cannot be
    /// papered over.
    pub capture_rate_hz: u32,
    /// Native playback rate. Same caveat as [`AudioIo::capture_rate_hz`].
    pub playback_rate_hz: u32,

    /// The capture device that actually opened.
    ///
    /// The device that opened, not the one that was asked for — those differ
    /// whenever `None` was asked for, which is most of the time. An interface
    /// that draws the request instead of this is an interface that tells a
    /// person their microphone is "default".
    ///
    /// `None` when the backend opened a device and then would not describe it.
    /// Not a failure and not a placeholder: it is a device with nothing to show,
    /// and an interface must draw that as unmeasured rather than invent a name.
    pub capture: Option<CaptureDevice>,

    /// The playback device that actually opened.
    ///
    /// Same rule as [`AudioIo::capture`], and it matters more here: the fallback
    /// to the default output is the one a person cannot hear happening. What
    /// they can do is read this channel and see the name is not the one they
    /// picked.
    pub playback: Option<PlaybackDevice>,
}

/// One capture device the host is offering right now.
///
/// Two strings and not one, because they answer two different questions.
/// [`CaptureDevice::id`] is what a preference is written down as: `cpal` 0.18
/// documents [`cpal::DeviceId`] as stable "across program runs, device
/// disconnections, and system reboots where possible", and it round-trips
/// through `Display`/`FromStr`, which is exactly what surviving in a settings
/// file requires. [`CaptureDevice::name`] is what a person reads, and two
/// microphones of the same model report the same one.
///
/// [`CaptureDevice::default`] is carried rather than derived — "the default" is
/// a moving target, and a shell that wants to say *which* row is the current one
/// cannot recompute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDevice {
    /// The stable handle, as [`cpal::DeviceId`] writes itself. Never shown.
    pub id: String,
    /// What the host calls it, for a person to read.
    pub name: String,
    /// Whether this is the one [`open_default`] would take.
    pub default: bool,
}

/// One playback device the host is offering right now.
///
/// The twin of [`CaptureDevice`], and its own type rather than a shared one with
/// a direction field: an input id and an output id are both strings that parse
/// as a [`cpal::DeviceId`], so nothing but the type stops one being passed where
/// the other belongs. `cpal` would then hand back a device that cannot open the
/// stream being asked of it, and the failure would arrive as a build error about
/// a configuration rather than as "that is a microphone".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackDevice {
    /// The stable handle, as [`cpal::DeviceId`] writes itself. Never shown.
    pub id: String,
    /// What the host calls it, for a person to read.
    pub name: String,
    /// Whether this is the one [`open_default`] would take.
    pub default: bool,
}

/// Which devices [`open`] should ask the host for.
///
/// A struct with two named fields rather than two `Option<&str>` arguments, and
/// that is the whole reason it exists: the two are the same type, so a call site
/// that swapped them would compile, run, and fail by asking the speakers to
/// record. `Default` is both sides on the machine's own choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Wanted<'a> {
    /// Which microphone, as a [`CaptureDevice::id`]. `None` is the default.
    pub capture: Option<&'a str>,
    /// Where the sound comes out, as a [`PlaybackDevice::id`]. `None` is the
    /// default.
    pub playback: Option<&'a str>,
}

/// Describes one device, or gives up on it.
///
/// A device without both halves is dropped by every caller here, and that is
/// deliberate: a row with no id cannot be re-opened on the next run, and a row
/// with no name cannot be labelled — either way it is a control that does
/// nothing.
fn describe(device: &cpal::Device) -> Option<(String, String)> {
    let id = device.id().ok()?.to_string();
    let name = device.description().ok()?.name().to_owned();
    Some((id, name))
}

/// Every capture device the host will describe.
///
/// Returns an empty list rather than an error when the host will not enumerate.
/// A shell has one honest thing to draw either way — no devices — and turning
/// "the backend is unhappy" into a second empty state buys nothing. What a
/// caller must not do is read an empty list as "there is no microphone": the
/// default device can still open when enumeration fails, which is why
/// [`open_default`] does not consult this function.
#[must_use]
pub fn capture_devices() -> Vec<CaptureDevice> {
    let host = cpal::default_host();
    let default = host
        .default_input_device()
        .and_then(|device| device.id().ok());

    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    described(devices, default.as_ref())
        .into_iter()
        .map(|(id, name, default)| CaptureDevice { id, name, default })
        .collect()
}

/// Every playback device the host will describe.
///
/// The twin of [`capture_devices`], with the same reading of an empty list: the
/// host would not enumerate, **not** that there is nowhere to play. The default
/// output still opens when enumeration fails, which is why [`open`] with no
/// `playback` asked for never consults this.
#[must_use]
pub fn playback_devices() -> Vec<PlaybackDevice> {
    let host = cpal::default_host();
    let default = host
        .default_output_device()
        .and_then(|device| device.id().ok());

    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    described(devices, default.as_ref())
        .into_iter()
        .map(|(id, name, default)| PlaybackDevice { id, name, default })
        .collect()
}

/// Every device in `devices` this host will describe, and which one is default.
///
/// Shared by both sides rather than written twice: the two lists answer the same
/// three questions, and the one thing that must not drift between them is which
/// rows get dropped. A side that kept the rows without an id would be a side
/// whose picks do not survive a restart.
fn described(
    devices: impl Iterator<Item = cpal::Device>,
    default: Option<&cpal::DeviceId>,
) -> Vec<(String, String, bool)> {
    devices
        .filter_map(|device| {
            let is_default = device.id().ok().as_ref() == default;
            let (id, name) = describe(&device)?;
            Some((id, name, is_default))
        })
        .collect()
}

/// Finds the device an id names, or takes the host's default for that side.
///
/// The id is refused rather than quietly ignored. Falling back here would put
/// the decision in the wrong place: this function cannot tell a preference
/// written down last week from a row a person clicked ten seconds ago, and only
/// one of those may be silently replaced.
fn resolve(
    host: &cpal::Host,
    wanted: Option<&str>,
    side: Side,
) -> Result<cpal::Device, DeviceError> {
    let Some(wanted) = wanted else {
        return match side {
            Side::Input => host
                .default_input_device()
                .ok_or(DeviceError::NoInputDevice),
            Side::Output => host
                .default_output_device()
                .ok_or(DeviceError::NoOutputDevice),
        };
    };
    wanted
        .parse::<cpal::DeviceId>()
        .ok()
        .and_then(|id| host.device_by_id(&id))
        .ok_or_else(|| match side {
            Side::Input => DeviceError::CaptureDeviceGone {
                id: wanted.to_owned(),
            },
            Side::Output => DeviceError::PlaybackDeviceGone {
                id: wanted.to_owned(),
            },
        })
}

/// Opens the default input and output devices and starts both streams.
///
/// `ring_ms` sizes each ring. It only has to absorb scheduling jitter between
/// the device callback and the processing thread, so a few device buffers is
/// plenty — M1.1 measured this machine's native buffer at 512 frames, about
/// 10.7 ms. This is **not** the jitter buffer, which is adaptive, per source,
/// and a different component entirely.
///
/// # Errors
///
/// Returns [`DeviceError`] if a device is missing, will not describe itself,
/// speaks a format this build does not convert, or refuses to open or start.
pub fn open_default(ring_ms: u32) -> Result<AudioIo, DeviceError> {
    open(Wanted::default(), ring_ms)
}

/// A configuração de um lado, pedindo [`SAMPLE_RATE_HZ`] quando o aparelho a
/// oferece.
///
/// # Por que perguntar, em vez de aceitar o padrão
///
/// Porque o padrão do sistema não é escolhido pensando em voz. Um microfone que
/// **suporta** 48 kHz frequentemente tem 44,1 kHz como padrão — é a taxa do CD,
/// e é o que o sistema oferece a quem toca música. Aceitá-la punha um
/// reamostrador no caminho de captura e outro no de reprodução, para uma
/// conversão que o aparelho faria de graça se alguém pedisse.
///
/// O custo não é só de CPU. `specs/03-audio.md` fixa 48 kHz como a taxa do
/// projeto inteiro, e cada conversão a mais é filtro a mais entre a voz de quem
/// fala e o ouvido de quem escuta.
///
/// # O que isto **não** conserta, e é honesto dizer
///
/// Um fone Bluetooth em HFP — o modo que o sistema liga quando o microfone dele
/// é usado — não *oferece* 48 kHz: ele oferece 8 ou 16 kHz e mais nada. Aqui
/// não há o que pedir, e o resultado continua sendo o som abafado que esse modo
/// produz. O que esta função faz nesse caso é o que dá para fazer: devolve a
/// configuração de verdade, e a taxa aparece em [`AudioIo::capture_rate_hz`]
/// para quem quiser dizer à pessoa por que o microfone dela soa assim.
/// Qual das faixas oferecidas serve, se alguma serve.
///
/// Separada de [`negociar`] porque é a única parte dela que **não** precisa de
/// uma placa de som: é uma escolha sobre uma lista, e o cabeçalho deste módulo
/// pede exatamente isto — a lógica sai daqui para onde um teste alcança, e o
/// que fica é fiação.
///
/// Mesmo formato e mesma contagem de canais que o padrão, de propósito: o que
/// se está trocando é a **taxa**, e só ela. Trocar formato ou canal junto seria
/// escolher por quem não pediu, e a mistura de dois canais para um já acontece
/// em [`crate::rt`] de um jeito que esta função não deve adivinhar.
fn escolher_taxa(
    padrao: &cpal::SupportedStreamConfig,
    faixas: Vec<cpal::SupportedStreamConfigRange>,
) -> Option<cpal::SupportedStreamConfig> {
    faixas
        .into_iter()
        .filter(|faixa| {
            faixa.sample_format() == padrao.sample_format() && faixa.channels() == padrao.channels()
        })
        .find_map(|faixa| faixa.try_with_sample_rate(SAMPLE_RATE_HZ))
}

fn negociar(device: &cpal::Device, side: Side) -> Result<cpal::SupportedStreamConfig, DeviceError> {
    let padrao = match side {
        Side::Input => device.default_input_config(),
        Side::Output => device.default_output_config(),
    }
    .map_err(|source| DeviceError::Device {
        side,
        stage: Stage::Config,
        source,
    })?;

    if padrao.sample_rate() == SAMPLE_RATE_HZ {
        return Ok(padrao);
    }

    // Uma falha ao listar não é uma falha ao abrir: o padrão já está na mão e
    // funciona. Este caminho existe para melhorar o que há, nunca para impedir
    // alguém de entrar.
    let faixas = match side {
        Side::Input => device.supported_input_configs().map(Iterator::collect),
        Side::Output => device.supported_output_configs().map(Iterator::collect),
    };
    let Ok(faixas): Result<Vec<_>, _> = faixas else {
        return Ok(padrao);
    };

    match escolher_taxa(&padrao, faixas) {
        Some(config) => {
            tracing::info!(
                ?side,
                padrao_hz = padrao.sample_rate(),
                pedido_hz = SAMPLE_RATE_HZ,
                "o aparelho oferece a taxa do projeto; pedida em vez do padrão do sistema"
            );
            Ok(config)
        }
        None => {
            tracing::info!(
                ?side,
                taxa_hz = padrao.sample_rate(),
                projeto_hz = SAMPLE_RATE_HZ,
                "o aparelho não oferece a taxa do projeto; reamostragem no caminho"
            );
            Ok(padrao)
        }
    }
}

/// Opens the chosen devices, taking the host's default for each side left unset.
///
/// Each half of `wanted` is an id — a [`CaptureDevice::id`] or a
/// [`PlaybackDevice::id`] — never a name. An id and not an index because the
/// list a person picked from is minutes old by the time the pick lands, and an
/// index into a list that changed underneath points at a *different* device
/// rather than at nothing; an id and not a name because two interfaces of the
/// same model share a name, and the second one would be unpickable.
///
/// # Errors
///
/// [`DeviceError::CaptureDeviceGone`] or [`DeviceError::PlaybackDeviceGone`]
/// when an id names a device the host is not offering — an unplugged interface,
/// a preference written down by an older run, or an id from another host.
/// Otherwise the same failures as [`open_default`].
pub fn open(wanted: Wanted<'_>, ring_ms: u32) -> Result<AudioIo, DeviceError> {
    let host = cpal::default_host();

    let input_device = resolve(&host, wanted.capture, Side::Input)?;
    let output_device = resolve(&host, wanted.playback, Side::Output)?;

    let in_config = negociar(&input_device, Side::Input)?;
    let out_config = negociar(&output_device, Side::Output)?;

    // Read off the device that opened, not off the request: with `None` asked
    // for — which is most of the time — the request has no name in it at all.
    let capture = describe(&input_device).map(|(id, name)| CaptureDevice {
        default: host
            .default_input_device()
            .and_then(|device| device.id().ok())
            .is_some_and(|default| default.to_string() == id),
        id,
        name,
    });
    let playback = describe(&output_device).map(|(id, name)| PlaybackDevice {
        default: host
            .default_output_device()
            .and_then(|device| device.id().ok())
            .is_some_and(|default| default.to_string() == id),
        id,
        name,
    });
    let capture_rate_hz = in_config.sample_rate();
    let playback_rate_hz = out_config.sample_rate();

    // A device `cpal` agreed to describe always reports at least one channel;
    // `NonZeroU16::MIN` is a defensive floor, not an expected path.
    let in_channels = NonZeroU16::new(in_config.channels()).unwrap_or(NonZeroU16::MIN);
    let out_channels = NonZeroU16::new(out_config.channels()).unwrap_or(NonZeroU16::MIN);

    let counters = StreamCounters::shared();

    // Every allocation on the audio path happens here, at setup. Nothing below
    // this channel may allocate — enforced by tests/realtime_safety.rs.
    let (sink, captured) = capture_path(
        capacity_for_ms(ring_ms, capture_rate_hz),
        in_channels,
        Arc::clone(&counters),
    );
    let (to_device, source) = playback_path(
        capacity_for_ms(ring_ms, playback_rate_hz),
        out_channels,
        Arc::clone(&counters),
    );

    // specs/03-audio.md wants 48 kHz mono f32 internally, but devices deliver
    // whatever they like: CoreAudio tends to f32, ALSA often i16. Format
    // conversion happens per sample inside the callback (see `rt::RawSample`);
    // rate conversion is `resample`'s job on the processing thread.
    let input = match in_config.sample_format() {
        cpal::SampleFormat::F32 => {
            input_stream::<f32>(&input_device, in_config.into(), sink, Arc::clone(&counters))
        }
        cpal::SampleFormat::I16 => {
            input_stream::<i16>(&input_device, in_config.into(), sink, Arc::clone(&counters))
        }
        cpal::SampleFormat::U16 => {
            input_stream::<u16>(&input_device, in_config.into(), sink, Arc::clone(&counters))
        }
        cpal::SampleFormat::I32 => {
            input_stream::<i32>(&input_device, in_config.into(), sink, Arc::clone(&counters))
        }
        format => {
            return Err(DeviceError::UnsupportedSampleFormat {
                side: Side::Input,
                format,
            });
        }
    }
    .map_err(|source| DeviceError::Device {
        side: Side::Input,
        stage: Stage::Build,
        source,
    })?;

    let output = match out_config.sample_format() {
        cpal::SampleFormat::F32 => output_stream::<f32>(
            &output_device,
            out_config.into(),
            source,
            Arc::clone(&counters),
        ),
        cpal::SampleFormat::I16 => output_stream::<i16>(
            &output_device,
            out_config.into(),
            source,
            Arc::clone(&counters),
        ),
        cpal::SampleFormat::U16 => output_stream::<u16>(
            &output_device,
            out_config.into(),
            source,
            Arc::clone(&counters),
        ),
        cpal::SampleFormat::I32 => output_stream::<i32>(
            &output_device,
            out_config.into(),
            source,
            Arc::clone(&counters),
        ),
        format => {
            return Err(DeviceError::UnsupportedSampleFormat {
                side: Side::Output,
                format,
            });
        }
    }
    .map_err(|source| DeviceError::Device {
        side: Side::Output,
        stage: Stage::Build,
        source,
    })?;

    input.play().map_err(|source| DeviceError::Device {
        side: Side::Input,
        stage: Stage::Start,
        source,
    })?;
    output.play().map_err(|source| DeviceError::Device {
        side: Side::Output,
        stage: Stage::Start,
        source,
    })?;

    Ok(AudioIo {
        _input: input,
        _output: output,
        captured,
        to_device,
        counters,
        capture_rate_hz,
        playback_rate_hz,
        capture,
        playback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The devices this machine is offering, or `None` when it offers none.
    ///
    /// A skip and not an empty pass: a test that silently succeeds on a machine
    /// with no microphone is a test that succeeds on every machine, which is the
    /// same as not having written it. CI has no sound card, so the tests below
    /// that need one say so out loud instead.
    fn microphones_or_skip(what: &str) -> Option<Vec<CaptureDevice>> {
        let found = capture_devices();
        if found.is_empty() {
            eprintln!("skipped {what}: this machine lists no capture device");
            return None;
        }
        Some(found)
    }

    /// The same, for the other side of the pair.
    fn speakers_or_skip(what: &str) -> Option<Vec<PlaybackDevice>> {
        let found = playback_devices();
        if found.is_empty() {
            eprintln!("skipped {what}: this machine lists no playback device");
            return None;
        }
        Some(found)
    }

    /// Asks for a capture device by id, with the machine's own speakers.
    fn open_capture(id: &str) -> Result<AudioIo, DeviceError> {
        open(
            Wanted {
                capture: Some(id),
                playback: None,
            },
            100,
        )
    }

    /// Asks for a playback device by id, with the machine's own microphone.
    fn open_playback(id: &str) -> Result<AudioIo, DeviceError> {
        open(
            Wanted {
                capture: None,
                playback: Some(id),
            },
            100,
        )
    }

    #[test]
    fn an_id_that_names_no_device_is_an_enum_and_not_a_panic() {
        // Needs no sound card: nothing that is not `host:device` can name a
        // device on any host, so this is refused before the audio subsystem is
        // asked anything at all.
        //
        // The case is not hypothetical. A preference written down by an older
        // build, a settings file copied between two machines, or an interface
        // that was unplugged — all three arrive here as a string that has to
        // come back as a variant somebody can write a sentence for, rather than
        // as a client that will not start.
        let refused = open_capture("isto nao e um dispositivo");
        assert!(
            matches!(refused, Err(DeviceError::CaptureDeviceGone { .. })),
            "an unparseable id came back as something other than a missing device: \
             {refused:?}"
        );
    }

    #[test]
    fn a_playback_id_that_names_no_device_is_an_enum_and_not_a_panic() {
        // Needs no sound card, for the same reason its twin does not. And it
        // matters more on this side: the fallback that follows a refusal is
        // silent by construction — nobody hears the speakers they did not pick
        // — so the refusal itself is the only thing there is to act on.
        let refused = open_playback("isto tambem nao e um dispositivo");
        assert!(
            matches!(refused, Err(DeviceError::PlaybackDeviceGone { .. })),
            "an unparseable playback id came back as something other than a missing \
             device: {refused:?}"
        );
    }

    #[test]
    fn a_refusal_carries_the_id_that_was_asked_for() {
        // Without it the log channel says a device is gone and never says which,
        // which is exactly the report nobody can act on.
        let Err(DeviceError::CaptureDeviceGone { id }) = open_capture("alsa:hw:99,99") else {
            // A machine that really does have `hw:99,99` would land here. It
            // does not exist, but saying so beats an `unwrap`.
            eprintln!("skipped: this machine claims to have alsa:hw:99,99");
            return;
        };
        assert_eq!(id, "alsa:hw:99,99");
    }

    #[test]
    fn a_playback_refusal_carries_the_id_that_was_asked_for() {
        let Err(DeviceError::PlaybackDeviceGone { id }) = open_playback("alsa:hw:98,98") else {
            eprintln!("skipped: this machine claims to have alsa:hw:98,98");
            return;
        };
        assert_eq!(id, "alsa:hw:98,98");
    }

    #[test]
    fn every_failure_blames_exactly_one_side() {
        // The whole point of `side`: a caller holding one preference per side
        // gives up the one that failed. Blame the wrong half and it keeps the
        // device that would not open while discarding a choice that was fine —
        // and on the audio side that is a session that comes up silent for a
        // reason nothing in it explains.
        //
        // Written out one variant at a time rather than looped, because the
        // value of the test is the pairing, and a loop would have to carry the
        // answer next to the question anyway.
        let both_sides = [
            (DeviceError::NoInputDevice, Side::Input),
            (
                DeviceError::CaptureDeviceGone { id: "x".into() },
                Side::Input,
            ),
            (DeviceError::NoOutputDevice, Side::Output),
            (
                DeviceError::PlaybackDeviceGone { id: "x".into() },
                Side::Output,
            ),
            (
                DeviceError::UnsupportedSampleFormat {
                    side: Side::Input,
                    format: cpal::SampleFormat::F64,
                },
                Side::Input,
            ),
            (
                DeviceError::UnsupportedSampleFormat {
                    side: Side::Output,
                    format: cpal::SampleFormat::F64,
                },
                Side::Output,
            ),
        ];
        for (error, side) in both_sides {
            assert_eq!(
                error.side(),
                side,
                "`{error}` blames the wrong half of the pair"
            );
        }
    }

    #[test]
    fn every_listed_device_is_labelled_and_at_most_one_is_the_default() {
        let Some(found) = microphones_or_skip("the labelling check") else {
            return;
        };

        for device in &found {
            assert!(
                !device.id.is_empty(),
                "a device with no id cannot be chosen"
            );
            assert!(
                !device.name.is_empty(),
                "a device with no name cannot be labelled, so it is a row that does nothing"
            );
        }
        if found.len() < 2 {
            // Said out loud, because the channel below cannot fail with one row
            // and a check that cannot fail reads like a check that passed.
            eprintln!(
                "the capture default check proves nothing here: this machine lists one microphone"
            );
        }
        assert!(
            found.iter().filter(|device| device.default).count() <= 1,
            "two devices both claim to be the machine's default, so a screen \
             marking the current row would mark two"
        );
    }

    #[test]
    fn every_listed_id_finds_its_device_again() {
        // The whole contract of an id: it is what gets written to disk, and the
        // next run has to be able to turn it back into the same microphone. A
        // list whose ids do not round-trip is a picker whose picks never take.
        let Some(found) = microphones_or_skip("the round-trip check") else {
            return;
        };
        let host = cpal::default_host();

        for device in &found {
            let Ok(id) = device.id.parse::<cpal::DeviceId>() else {
                panic!("the id {:?} does not parse back", device.id);
            };
            let Some(again) = host.device_by_id(&id) else {
                panic!(
                    "the id {:?} finds no device on the host that listed it",
                    device.id
                );
            };
            assert_eq!(
                describe(&again).map(|(_, name)| name),
                Some(device.name.clone()),
                "the id {:?} came back as a different device",
                device.id
            );
        }
    }

    #[test]
    fn every_listed_output_is_labelled_and_at_most_one_is_the_default() {
        let Some(found) = speakers_or_skip("the playback labelling check") else {
            return;
        };

        for device in &found {
            assert!(
                !device.id.is_empty(),
                "a device with no id cannot be chosen"
            );
            assert!(
                !device.name.is_empty(),
                "a device with no name cannot be labelled, so it is a row that does nothing"
            );
        }
        if found.len() < 2 {
            eprintln!(
                "the playback default check proves nothing here: this machine lists one output"
            );
        }
        assert!(
            found.iter().filter(|device| device.default).count() <= 1,
            "two outputs both claim to be the machine's default, so a screen \
             marking the current row would mark two"
        );
    }

    #[test]
    fn every_listed_output_id_finds_its_device_again() {
        // The same contract the capture list has to keep, and the same reason:
        // the id is what gets written to disk, and the next run has to turn it
        // back into the same speakers.
        let Some(found) = speakers_or_skip("the playback round-trip check") else {
            return;
        };
        let host = cpal::default_host();

        for device in &found {
            let Ok(id) = device.id.parse::<cpal::DeviceId>() else {
                panic!("the id {:?} does not parse back", device.id);
            };
            let Some(again) = host.device_by_id(&id) else {
                panic!(
                    "the id {:?} finds no device on the host that listed it",
                    device.id
                );
            };
            assert_eq!(
                describe(&again).map(|(_, name)| name),
                Some(device.name.clone()),
                "the id {:?} came back as a different device",
                device.id
            );
        }
    }

    #[test]
    fn a_listed_output_opens_and_reports_itself() {
        // The two halves this feature is made of, together: an id off the list
        // opens, and what comes back names the device that opened rather than
        // the request. A picker built on either half alone is a picker that
        // shows a row it did not open, or opens a device it cannot name.
        let (Some(speakers), Some(_)) = (
            speakers_or_skip("the playback open check"),
            microphones_or_skip("the playback open check"),
        ) else {
            return;
        };
        // Deliberately not the machine's default. Asking for the default proves
        // nothing: an `open` that dropped the request on the floor and took the
        // default anyway would pass, and dropping the request on the floor is
        // the entire failure this guards against.
        let Some(wanted) = speakers.iter().find(|device| !device.default) else {
            eprintln!(
                "skipped the playback open check: this machine offers only its default output, \
                 so asking for one proves nothing"
            );
            return;
        };

        let opened = match open_playback(&wanted.id) {
            Ok(opened) => opened,
            // A device that is listed and will not open is a real machine
            // state — exclusive-mode outputs and virtual devices both do it —
            // and this test has no way to tell that from a defect.
            Err(error) => {
                eprintln!(
                    "skipped: this machine would not open {:?}: {error}",
                    wanted.name
                );
                return;
            }
        };
        assert_eq!(
            opened.playback.as_ref().map(|device| device.id.as_str()),
            Some(wanted.id.as_str()),
            "the stream opened on a device other than the one asked for"
        );
    }
}

// ---------------------------------------------------------------------------
// O consentimento do microfone, no Windows
// ---------------------------------------------------------------------------

/// O que o sistema deixa este processo fazer com o microfone.
///
/// # Por que isto existe, e por que só no Windows
///
/// Porque no Windows **não há a quem pedir**. Um app empacotado (Store/MSIX)
/// tem prompt e API de consentimento; um app de área de trabalho — que é o que
/// este é, e o que o Discord também é — não tem nem um nem outro. O que existe
/// é um interruptor nos Ajustes, ligado de fábrica, e nada acontece quando ele
/// está desligado: a captura simplesmente entrega silêncio.
///
/// Foi assim que uma pessoa passou uma conversa inteira falando sozinha, e a
/// única coisa que a tela dela dizia era `SEM ÁUDIO`. O bloqueio estava em
/// `HKLM`, que é política de máquina e passa por cima de tudo — e nada em
/// lugar nenhum apontava para lá.
///
/// Ler o registro não pede permissão nenhuma e não muda nada. É só parar de
/// deixar a pessoa adivinhando.
///
/// No macOS o caminho é outro e já existe: o TCC pergunta, uma vez, e o
/// `Info.plist` diz por quê. No Linux não há a quem perguntar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentimentoDoMicrofone {
    /// O sistema deixa. Se ainda assim não há som, o problema é outro.
    Permitido,
    /// Desligado para **toda** esta máquina, por política.
    ///
    /// `HKLM` vence o ajuste de quem está usando: quem tentar ligar na própria
    /// conta vai ver o interruptor voltar. Quem conserta é quem administra a
    /// máquina.
    NegadoNaMaquina,
    /// A pessoa desligou o microfone para todos os aplicativos.
    NegadoParaTudo,
    /// O microfone está ligado, e **aplicativos de área de trabalho** não.
    ///
    /// É o interruptor separado, e é o que mais pega: ele fica no fim de uma
    /// página longa, depois da lista de aplicativos da Store, e desligá-lo não
    /// muda nada visível até alguém tentar falar.
    NegadoParaAreaDeTrabalho,
    /// Não deu para ler, ou este sistema não tem esse controle.
    ///
    /// **Não é «permitido»**, e a diferença importa: dizer que está tudo certo
    /// sem ter olhado é a mentira confiante que este enum existe para não
    /// contar.
    NaoSeSabe,
}

/// Lê o que o Windows já decidiu sobre o microfone. Não pergunta nada.
///
/// A ordem é a que o sistema usa, e ela não é óbvia: a política da máquina vem
/// primeiro, e dentro de cada colmeia o interruptor geral vem antes do de
/// aplicativos de área de trabalho. Ler fora de ordem faz um `Allow` de usuário
/// esconder um `Deny` de máquina que é quem manda.
#[must_use]
pub fn consentimento_do_microfone() -> ConsentimentoDoMicrofone {
    #[cfg(windows)]
    {
        windows_consentimento()
    }
    #[cfg(not(windows))]
    {
        // macOS tem TCC, que pergunta e tem caminho próprio; Linux não tem a
        // quem perguntar. Nos dois, esta função não sabe de nada — e diz isso.
        ConsentimentoDoMicrofone::NaoSeSabe
    }
}

#[cfg(windows)]
fn windows_consentimento() -> ConsentimentoDoMicrofone {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    const RAIZ: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    /// `Deny` de forma exata: qualquer outro valor — inclusive lixo — não é
    /// negativa, e tratá-lo como negativa poria a tela a acusar o sistema por
    /// causa de uma chave escrita à mão.
    ///
    /// `winreg::HKEY` e **não** `winreg::enums::HKEY`: o módulo `enums` carrega
    /// as constantes — `HKEY_LOCAL_MACHINE` e as irmãs, importadas acima — e o
    /// **tipo** delas mora na raiz do crate, reexportado do `windows_sys`. A
    /// confusão compila em qualquer máquina que não seja Windows, porque este
    /// bloco inteiro é `cfg(windows)` e nunca foi visto por um compilador aqui.
    fn negado(colmeia: winreg::HKEY, caminho: &str) -> Option<bool> {
        let raiz = RegKey::predef(colmeia);
        let chave = raiz.open_subkey(caminho).ok()?;
        let valor: String = chave.get_value("Value").ok()?;
        Some(valor.eq_ignore_ascii_case("Deny"))
    }

    let area_de_trabalho = format!(r"{RAIZ}\NonPackaged");

    // A máquina primeiro: `HKLM` passa por cima do que a pessoa escolher.
    if negado(HKEY_LOCAL_MACHINE, RAIZ) == Some(true)
        || negado(HKEY_LOCAL_MACHINE, &area_de_trabalho) == Some(true)
    {
        return ConsentimentoDoMicrofone::NegadoNaMaquina;
    }
    if negado(HKEY_CURRENT_USER, RAIZ) == Some(true) {
        return ConsentimentoDoMicrofone::NegadoParaTudo;
    }
    if negado(HKEY_CURRENT_USER, &area_de_trabalho) == Some(true) {
        return ConsentimentoDoMicrofone::NegadoParaAreaDeTrabalho;
    }

    // Nenhum `Deny` encontrado. Só é `Permitido` se **alguma** chave foi lida:
    // um registro que não abriu não é uma autorização.
    let leu_alguma = negado(HKEY_CURRENT_USER, RAIZ).is_some()
        || negado(HKEY_CURRENT_USER, &area_de_trabalho).is_some()
        || negado(HKEY_LOCAL_MACHINE, RAIZ).is_some();
    if leu_alguma {
        ConsentimentoDoMicrofone::Permitido
    } else {
        ConsentimentoDoMicrofone::NaoSeSabe
    }
}

#[cfg(test)]
mod taxa_do_aparelho {
    use super::{escolher_taxa, SAMPLE_RATE_HZ};
    use cpal::{
        SampleFormat, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange,
    };

    const BLOCO: SupportedBufferSize = SupportedBufferSize::Range { min: 64, max: 4096 };

    fn padrao(canais: u16, taxa: u32, formato: SampleFormat) -> SupportedStreamConfig {
        SupportedStreamConfig::new(canais, taxa, BLOCO, formato)
    }

    fn faixa(
        canais: u16,
        minima: u32,
        maxima: u32,
        formato: SampleFormat,
    ) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(canais, minima, maxima, BLOCO, formato)
    }

    /// O caso comum, e a razão de esta função existir.
    ///
    /// Um microfone cujo padrão do sistema é 44,1 kHz — a taxa do CD, escolhida
    /// pensando em música — e que aceita 48 kHz sem reclamar. Aceitar o padrão
    /// punha um reamostrador no caminho para uma conversão que o aparelho faz
    /// de graça.
    #[test]
    fn um_aparelho_que_aceita_a_taxa_do_projeto_recebe_o_pedido() {
        let escolhida = escolher_taxa(
            &padrao(1, 44_100, SampleFormat::F32),
            vec![faixa(1, 44_100, 48_000, SampleFormat::F32)],
        );
        assert_eq!(
            escolhida.map(|config| config.sample_rate()),
            Some(SAMPLE_RATE_HZ)
        );
    }

    /// O fone Bluetooth em HFP, que é o que produz «abafado».
    ///
    /// Ele não oferece 48 kHz: oferece 16 kHz e mais nada. Não há o que pedir, e
    /// inventar um pedido fora da faixa faria `with_sample_rate` entrar em
    /// pânico. A resposta certa é «nenhuma serve», e quem chama fica com o
    /// padrão de verdade — que é o que permite dizer à pessoa por quê.
    #[test]
    fn um_aparelho_que_so_oferece_taxa_baixa_nao_e_forcado() {
        let escolhida = escolher_taxa(
            &padrao(1, 16_000, SampleFormat::F32),
            vec![faixa(1, 8_000, 16_000, SampleFormat::F32)],
        );
        assert!(
            escolhida.is_none(),
            "pediu-se ao aparelho uma taxa que ele não oferece"
        );
    }

    /// A taxa é a única coisa que se troca.
    ///
    /// Uma faixa que chega a 48 kHz com outro formato, ou com outra contagem de
    /// canais, resolveria a taxa e mudaria duas coisas que ninguém pediu. O
    /// caminho de conversão de formato e a mistura de canais vivem em
    /// `crate::rt` e são escolhidos lá.
    #[test]
    fn nem_formato_nem_canal_sao_trocados_de_carona() {
        let alvo = padrao(1, 44_100, SampleFormat::F32);
        assert!(
            escolher_taxa(&alvo, vec![faixa(1, 48_000, 48_000, SampleFormat::I16)]).is_none(),
            "trocou o formato do aparelho para conseguir a taxa"
        );
        assert!(
            escolher_taxa(&alvo, vec![faixa(2, 48_000, 48_000, SampleFormat::F32)]).is_none(),
            "trocou a contagem de canais para conseguir a taxa"
        );
    }

    /// Entre várias, serve a primeira que couber — e nenhuma tem de caber.
    #[test]
    fn a_lista_e_varrida_ate_achar_uma_que_sirva() {
        let escolhida = escolher_taxa(
            &padrao(2, 44_100, SampleFormat::F32),
            vec![
                faixa(2, 8_000, 8_000, SampleFormat::F32),
                faixa(2, 44_100, 44_100, SampleFormat::F32),
                faixa(2, 32_000, 96_000, SampleFormat::F32),
            ],
        );
        assert_eq!(
            escolhida.map(|config| config.sample_rate()),
            Some(SAMPLE_RATE_HZ),
            "a faixa que contém a taxa do projeto estava na lista e não foi achada"
        );
    }
}
