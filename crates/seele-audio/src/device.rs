//! The `cpal` seam: turns real devices into the ring buffers in [`crate::rt`].
//!
//! **This module is deliberately thin.** CI has no sound card, so almost
//! nothing here can be covered by a test. Nearly every line in this file is a
//! line no test protects, so the logic lives in [`crate::rt`] — which is fully
//! tested — and this file does nothing but wire it up.
//!
//! When reviewing a change here, the question is not "is this correct?" but
//! "could this have gone in `rt` instead?".
//!
//! The exception is device *identity*, added with the capture picker: an id that
//! does not name a device has to come back as [`DeviceError::CaptureDeviceGone`]
//! on a machine with no sound card as surely as on one with six, so that much is
//! testable everywhere. Everything below it that needs a microphone says so and
//! skips.

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
    devices
        .filter_map(|device| {
            let is_default = device.id().ok().as_ref() == default.as_ref();
            let (id, name) = describe(&device)?;
            Some(CaptureDevice {
                id,
                name,
                default: is_default,
            })
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

/// Opens a chosen capture device, or the default one when `capture` is `None`.
///
/// `capture` is a [`CaptureDevice::id`], never a name. An id and not an index
/// because the list a person picked from is minutes old by the time the pick
/// lands, and an index into a list that changed underneath points at a
/// *different* microphone rather than at nothing; an id and not a name because
/// two microphones of the same model share a name, and the second one would be
/// unpickable.
///
/// Playback stays on the default device. That is not an oversight: nothing in
/// the product reads a playback preference back, and shipping half a pair of
/// controls is worse than shipping one that works.
///
/// # Errors
///
/// [`DeviceError::CaptureDeviceGone`] when `capture` names a device the host is
/// not offering — an unplugged interface, a preference written down by an older
/// run, or an id from another host. Otherwise the same failures as
/// [`open_default`].
pub fn open(capture: Option<&str>, ring_ms: u32) -> Result<AudioIo, DeviceError> {
    let host = cpal::default_host();

    let input_device = match capture {
        Some(wanted) => wanted
            .parse::<cpal::DeviceId>()
            .ok()
            .and_then(|id| host.device_by_id(&id))
            .ok_or_else(|| DeviceError::CaptureDeviceGone {
                id: wanted.to_owned(),
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
        capture,
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
        let refused = open(Some("isto nao e um dispositivo"), 100);
        assert!(
            matches!(refused, Err(DeviceError::CaptureDeviceGone { .. })),
            "an unparseable id came back as something other than a missing device"
        );
    }

    #[test]
    fn a_refusal_carries_the_id_that_was_asked_for() {
        // Without it the log line says a device is gone and never says which,
        // which is exactly the report nobody can act on.
        let Err(DeviceError::CaptureDeviceGone { id }) = open(Some("alsa:hw:99,99"), 100) else {
            // A machine that really does have `hw:99,99` would land here. It
            // does not exist, but saying so beats an `unwrap`.
            eprintln!("skipped: this machine claims to have alsa:hw:99,99");
            return;
        };
        assert_eq!(id, "alsa:hw:99,99");
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
}
