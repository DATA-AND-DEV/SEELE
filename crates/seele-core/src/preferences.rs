//! Settings that stay on this machine.
//!
//! The comp calls the screen these belong to "Terminal server · configuração
//! local", and the word that matters is *local*: none of this is sent anywhere,
//! none of it follows the person to another computer, and every one of them is
//! about the hardware in front of the person rather than about the server.
//!
//! Today there are two — which microphone to open, and where the sound comes
//! out. They are here rather than in the desktop shell because the terminal
//! client has the same questions to answer, and a preference written down by one
//! client and ignored by the other is a preference that appears to have been
//! forgotten.
//!
//! # Why not in `conhecidos`
//!
//! That file is one line per server, and this is not per server. A microphone
//! chosen while visiting one server is the same microphone at the next one, and
//! folding it in there would either repeat it on every line or invent a line for
//! a server nobody visited.
//!
//! # Format
//!
//! One setting per line, name and value separated by a tab:
//!
//! ```text
//! capture <TAB> coreaudio:AppleUSBAudioEngine:Focusrite:Scarlett Solo:1
//! playback <TAB> coreaudio:AppleHDAEngineOutput:1:0:1:0
//! ```
//!
//! Text, and a name per line rather than a fixed column order, for the reason
//! `conhecidos` gives and one more: a version of this file written by an older
//! build is missing lines rather than misaligned, so an unknown name is skipped
//! and a missing one is simply unset. Neither can turn into the wrong value.
//!
//! A file that cannot be read is an empty one. Refusing to start a client
//! because a settings file got truncated would be the wrong trade — the whole
//! product still works with every default.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The name of the microphone setting on disk.
///
/// Written once here rather than at both ends: a reader and a writer that
/// disagree about the spelling is a setting that saves and never loads, and
/// nothing about that failure is visible.
const CAPTURE: &str = "capture";

/// The name of the sound-output setting on disk.
///
/// Spelled once, for the reason [`CAPTURE`] gives.
const PLAYBACK: &str = "playback";

/// The local settings, on disk.
#[derive(Debug, Clone, Default)]
pub struct Preferences {
    path: PathBuf,
    capture: Option<String>,
    playback: Option<String>,
}

impl Preferences {
    /// Reads the settings, or starts with the defaults.
    ///
    /// # Errors
    ///
    /// Only if the directory cannot be created. An unreadable or malformed file
    /// is treated as an unwritten one — see the module note.
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }

        let mut settings = Self {
            path,
            capture: None,
            playback: None,
        };
        if let Ok(text) = std::fs::read_to_string(&settings.path) {
            for line in text.lines() {
                let Some((name, value)) = line.split_once('\t') else {
                    continue;
                };
                let value = value.trim();
                let value = (!value.is_empty()).then(|| value.to_owned());
                // An unknown name is a setting from a newer build, and skipping
                // it is the only honest thing this one can do with it.
                match name.trim() {
                    CAPTURE => settings.capture = value,
                    PLAYBACK => settings.playback = value,
                    _ => {}
                }
            }
        }
        Ok(settings)
    }

    /// Which microphone to open, as a `CaptureDevice` id.
    ///
    /// `None` is the machine's default, and is what every client did before
    /// there was a screen to choose on.
    ///
    /// A device that is no longer plugged in still reads back from here. That is
    /// deliberate: the caller falls back to the default for *this* session and
    /// leaves the preference alone, so plugging the interface back in restores
    /// the choice instead of requiring it to be made again.
    #[must_use]
    pub fn capture(&self) -> Option<&str> {
        self.capture.as_deref()
    }

    /// Writes down which microphone to open. `None` goes back to the default.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn set_capture(&mut self, device: Option<&str>) -> Result<()> {
        // A tab or a newline inside an id would make the next read see one
        // setting as two, or as none.
        self.capture = device
            .map(sanitise)
            .filter(|device| !device.trim().is_empty());
        self.write()
    }

    /// Where the sound comes out, as a `PlaybackDevice` id.
    ///
    /// `None` is the machine's default. Everything [`Preferences::capture`] says
    /// about a device that is no longer plugged in holds here too, and matters
    /// more: falling back to the machine's speakers for one session makes no
    /// sound of its own, so a preference erased on the way would be a choice
    /// that vanished without anything to notice.
    #[must_use]
    pub fn playback(&self) -> Option<&str> {
        self.playback.as_deref()
    }

    /// Writes down where the sound comes out. `None` goes back to the default.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn set_playback(&mut self, device: Option<&str>) -> Result<()> {
        self.playback = device
            .map(sanitise)
            .filter(|device| !device.trim().is_empty());
        self.write()
    }

    fn write(&self) -> Result<()> {
        // Every setting, not only the one that just changed: this rewrites the
        // whole file, so a line left out here is a line deleted from disk. That
        // is how a second setting turns into a bug in the first one.
        let mut text = String::new();
        for (name, value) in [(CAPTURE, &self.capture), (PLAYBACK, &self.playback)] {
            let Some(value) = value else {
                continue;
            };
            text.push_str(name);
            text.push('\t');
            text.push_str(value);
            text.push('\n');
        }
        write_private(&self.path, text.as_bytes())
            .with_context(|| format!("could not write {}", self.path.display()))
    }
}

/// Removes what would break the format.
fn sanitise(value: &str) -> String {
    value
        .chars()
        .filter(|character| *character != '\t' && *character != '\n' && *character != '\r')
        .collect()
}

/// The same restricted mode the identity and the visited list are written with.
#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("seele-preferences-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path.join("preferences")
    }

    #[test]
    fn a_chosen_microphone_survives_the_process() {
        // The whole reason this file exists. A picker whose pick is forgotten
        // when the window closes is a picker that has to be used every day.
        let path = scratch("survives");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_capture(Some("coreaudio:Scarlett Solo"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.capture(), Some("coreaudio:Scarlett Solo"));
    }

    #[test]
    fn a_chosen_output_survives_the_process() {
        let path = scratch("output-survives");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_playback(Some("coreaudio:Studio Display Speakers"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(
            settings.playback(),
            Some("coreaudio:Studio Display Speakers")
        );
    }

    #[test]
    fn writing_one_setting_does_not_erase_the_other() {
        // The defect a second setting invents. `write` rewrites the whole file,
        // so a setter that forgot to put the other line back would delete it —
        // and the way that shows up is somebody choosing an output and finding
        // their microphone quietly back on the machine's default the next time
        // they start, with nothing anywhere connecting the two.
        let path = scratch("both");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings.set_capture(Some("alsa:hw:1,0")).expect("write");
            settings.set_playback(Some("alsa:hw:2,0")).expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.capture(), Some("alsa:hw:1,0"));
        assert_eq!(settings.playback(), Some("alsa:hw:2,0"));
    }

    #[test]
    fn the_two_settings_do_not_read_as_each_other() {
        // Both are ids of the same shape, written with the same grammar, and a
        // reader that matched the wrong name would send somebody's speakers to
        // the microphone. The compiler has nothing to say about it: they are
        // both `Option<String>`.
        let path = scratch("crossed");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings.set_capture(Some("o microfone")).expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.capture(), Some("o microfone"));
        assert_eq!(
            settings.playback(),
            None,
            "the microphone was read back as the sound output"
        );
    }

    #[test]
    fn nothing_written_down_means_the_machines_default() {
        let settings = Preferences::open(scratch("unset")).expect("open");
        assert_eq!(settings.capture(), None);
        assert_eq!(settings.playback(), None);
    }

    #[test]
    fn going_back_to_the_default_erases_the_choice() {
        // Not merely "stops being applied": the line has to leave the file, or
        // the next build to read it would find the old id still sitting there.
        let path = scratch("cleared");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings.set_capture(Some("alsa:hw:1,0")).expect("write");
            settings.set_capture(None).expect("clear");
        }
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            !text.contains("alsa:hw:1,0"),
            "the cleared id is still in the file: {text:?}"
        );
        assert_eq!(Preferences::open(path).expect("reopen").capture(), None);
    }

    #[test]
    fn an_id_with_a_tab_in_it_cannot_forge_a_second_setting() {
        // `cpal` builds ids out of strings the operating system hands over, and
        // this file's whole grammar is one tab per line. Without the filter, an
        // id carrying one would be read back as a different, shorter id.
        let path = scratch("forged");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_capture(Some("coreaudio:Mic\tcapture\televado"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.capture(), Some("coreaudio:Miccaptureelevado"));
    }

    #[test]
    fn an_output_id_with_a_tab_in_it_cannot_forge_the_microphone_setting() {
        // Worse than the same hole on the capture side, and that is why it gets
        // its own test rather than being assumed from the twin: with two
        // settings in one file, an unfiltered tab in an output id writes a
        // `capture` line. Choosing where the sound comes out would change which
        // microphone opens, and nothing on either screen would connect the two.
        let path = scratch("forged-capture");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_playback(Some("coreaudio:Caixa\ncapture\tum microfone alheio"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(
            settings.capture(),
            None,
            "an output id wrote itself a microphone setting"
        );
    }

    #[test]
    fn a_file_full_of_nonsense_reads_as_the_defaults() {
        // A settings file that got truncated must not stop a client from
        // starting. Every default still works.
        let path = scratch("nonsense");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&path, "\0\u{feff}not a setting\nfuturo\tvalor\n").expect("write");

        let settings = Preferences::open(path).expect("open");
        assert_eq!(
            settings.capture(),
            None,
            "an unreadable file must not become a microphone nobody chose"
        );
        assert_eq!(
            settings.playback(),
            None,
            "an unreadable file must not become an output nobody chose"
        );
    }
}
