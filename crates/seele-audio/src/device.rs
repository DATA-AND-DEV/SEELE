//! The `cpal` seam: turns real devices into the ring buffers in [`crate::rt`].
//!
//! **This module is deliberately thin.** CI has no sound card, so nothing here
//! can be covered by a test. Every line in this file is a line no test protects,
//! so the logic lives in [`crate::rt`] — which is fully tested — and this file
//! does nothing but wire it up.
//!
//! When reviewing a change here, the question is not "is this correct?" but
//! "could this have gone in `rt` instead?".

use std::num::NonZeroU16;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer};
use thiserror::Error;

use crate::rt::{
    capacity_for_ms, capture_path, playback_path, CaptureSink, PlaybackSource, RawSample,
    StreamCounters,
};

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

    /// A capture device was asked for by name and the host does not offer it.
    ///
    /// Its own variant rather than [`DeviceError::NoInputDevice`] because the
    /// two are different sentences for a person: "there is no microphone" and
    /// "the microphone you picked was unplugged" ask for different next steps.
    /// `specs/02-protocolo.md` wants enumerated reasons for exactly this.
    #[error("no capture device named {name}")]
    CaptureDeviceGone {
        /// What was asked for.
        name: String,
    },

    /// The host reports no default output.
    #[error("no default output device")]
    NoOutputDevice,

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

    /// What the capture device that actually opened calls itself.
    ///
    /// The device that opened, not the one that was asked for — those differ
    /// whenever `None` was asked for, which is most of the time. An interface
    /// that draws the request instead of this is an interface that tells a
    /// person their microphone is "default".
    ///
    /// `None` when the backend opened a device and then would not name it. Not
    /// a failure and not a placeholder: it is a device with no name to show,
    /// and an interface must draw that as unmeasured rather than invent one.
    pub capture_name: Option<String>,
}

/// One capture device the host is offering right now.
///
/// The name is the whole identity: `cpal` has no stable device id across
/// backends, so the name is what a preference can be written down as and what a
/// later run has to match. That is also why [`CaptureDevice::default`] is
/// carried rather than derived — "the default" is a moving target, and a shell
/// that wants to say *which* one is current cannot recompute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDevice {
    /// What the host calls it.
    pub name: String,
    /// Whether this is the one [`open_default`] would take.
    pub default: bool,
}

/// Every capture device the host will name.
///
/// Returns an empty list rather than an error when the host will not enumerate.
/// A shell has one honest thing to draw either way — no devices — and turning
/// "the backend is unhappy" into a second empty state buys nothing. What a
/// caller must not do is read an empty list as "there is no microphone": the
/// default device can still open when enumeration fails, which is why
/// [`open_default`] does not consult this function.
///
/// Devices whose name the backend refuses to give are dropped. A row that
/// cannot be labelled cannot be picked, and drawing a blank one would be a
/// control that does nothing.
#[must_use]
pub fn capture_devices() -> Vec<CaptureDevice> {
    let host = cpal::default_host();
    let default = host
        .default_input_device()
        .and_then(|device| device.name().ok());

    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| device.name().ok())
        .map(|name| CaptureDevice {
            default: default.as_ref() == Some(&name),
            name,
        })
        .collect()
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
    open(None, ring_ms)
}

/// Opens a named capture device, or the default one when `capture` is `None`.
///
/// Playback stays on the default device. That is not an oversight: the comp
/// draws an output picker too, but nothing in the product reads a playback
/// preference back, and shipping half a pair of controls is worse than shipping
/// one that works. See `docs/tela-inventario-camadas.md`.
///
/// # Errors
///
/// [`DeviceError::CaptureDeviceGone`] when `capture` names a device the host is
/// not offering — an unplugged interface, or a preference written down by an
/// older run. Otherwise the same failures as [`open_default`].
pub fn open(capture: Option<&str>, ring_ms: u32) -> Result<AudioIo, DeviceError> {
    let host = cpal::default_host();

    let input_device = match capture {
        // Enumerating and matching by name rather than trusting an index: the
        // list a person picked from is minutes old by the time the pick lands,
        // and an index into a list that changed underneath points at a
        // different microphone rather than at nothing.
        Some(wanted) => host
            .input_devices()
            .ok()
            .and_then(|mut devices| {
                devices.find(|device| device.name().is_ok_and(|name| name == wanted))
            })
            .ok_or_else(|| DeviceError::CaptureDeviceGone {
                name: wanted.to_owned(),
            })?,
        None => host
            .default_input_device()
            .ok_or(DeviceError::NoInputDevice)?,
    };
    let output_device = host
        .default_output_device()
        .ok_or(DeviceError::NoOutputDevice)?;

    let in_config = input_device
        .default_input_config()
        .map_err(|source| DeviceError::Device {
            side: Side::Input,
            stage: Stage::Config,
            source,
        })?;
    let out_config =
        output_device
            .default_output_config()
            .map_err(|source| DeviceError::Device {
                side: Side::Output,
                stage: Stage::Config,
                source,
            })?;

    let capture_name = input_device.name().ok();
    let capture_rate_hz = in_config.sample_rate();
    let playback_rate_hz = out_config.sample_rate();

    // A device `cpal` agreed to describe always reports at least one channel;
    // `NonZeroU16::MIN` is a defensive floor, not an expected path.
    let in_channels = NonZeroU16::new(in_config.channels()).unwrap_or(NonZeroU16::MIN);
    let out_channels = NonZeroU16::new(out_config.channels()).unwrap_or(NonZeroU16::MIN);

    let counters = StreamCounters::shared();

    // Every allocation on the audio path happens here, at setup. Nothing below
    // this line may allocate — enforced by tests/realtime_safety.rs.
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
        capture_name,
    })
}
