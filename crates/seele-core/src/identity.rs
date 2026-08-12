//! Who this client is, across restarts.
//!
//! ADR 0004 makes identity an Ed25519 key pair. What it did not say is where
//! the key lives, and until M3 it did not matter: with no accounts, a fresh key
//! every run was simply a fresh pilot every run. CASPER changed that. A Dogma
//! now binds a nickname to the identity that first claimed it, so a client that
//! forgets its key cannot come back — the server refuses the second connection
//! with "nickname belongs to a different identity", which is exactly the
//! protection it is supposed to offer and exactly the wrong answer to give
//! somebody reopening their own client.
//!
//! So the key is written to disk, and the pins beside it.
//!
//! # What this is not
//!
//! Not a key store. One identity, one file, no passphrase, no rotation. A key
//! sitting unencrypted in the user's own home directory is worth saying out
//! loud: anybody who can read that file can be this pilot. That is the same
//! trust boundary as an SSH private key without a passphrase, and it is a
//! deliberate M4 stopping point rather than an oversight — encryption at rest
//! belongs with the account work, not with the interface.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;

use crate::tofu::PinStore;

/// Loads the identity at `path`, creating one if there is none.
///
/// # Errors
///
/// Fails if the directory cannot be created, or the file exists and cannot be
/// read or is not a 32-byte key.
pub fn load_or_create(path: &Path) -> Result<SigningKey> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    if path.exists() {
        let bytes = std::fs::read(path)
            .with_context(|| format!("could not read the identity at {}", path.display()))?;
        let bytes: [u8; 32] = bytes.as_slice().try_into().with_context(|| {
            format!(
                "{} is {} bytes; an Ed25519 identity is 32",
                path.display(),
                bytes.len()
            )
        })?;
        return Ok(SigningKey::from_bytes(&bytes));
    }

    let key = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    write_private(path, key.as_bytes())
        .with_context(|| format!("could not write the identity to {}", path.display()))?;
    Ok(key)
}

/// Writes a file only this user can read.
///
/// The mode is set before the bytes go in, not after: a private key that is
/// world-readable for the width of one syscall has been world-readable.
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
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    // Windows inherits the directory's ACL, and the directory is under the
    // user's own profile. Weaker than the Unix mode, and worth knowing.
    std::fs::write(path, bytes)
}

/// A pin store backed by a file, so ADR 0003 survives a restart.
///
/// Trust on first use is only trust if the "first" is remembered. A store that
/// empties on exit makes every connection a first contact, which means the
/// warning that matters — the key changed — can never fire.
///
/// The format is one `host fingerprint` pair per line. Chosen so that somebody
/// who has been told their server's key changed can open the file, read it, and
/// compare by eye. A binary format would have made that a support conversation.
#[derive(Debug)]
pub struct FilePinStore {
    path: PathBuf,
    pins: Mutex<Vec<(String, String)>>,
}

impl FilePinStore {
    /// Opens the store at `path`, reading whatever is already there.
    ///
    /// A file that cannot be parsed is treated as empty rather than fatal: the
    /// consequence is a re-pin on next contact, and refusing to start a chat
    /// client over a corrupt cache would be the worse trade.
    ///
    /// # Errors
    ///
    /// Fails only if the containing directory cannot be created.
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let pins = std::fs::read_to_string(&path)
            .map(|text| {
                text.lines()
                    .filter_map(|line| {
                        let (host, fingerprint) = line.split_once(char::is_whitespace)?;
                        Some((host.trim().to_owned(), fingerprint.trim().to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            path,
            pins: Mutex::new(pins),
        })
    }

    fn flush(&self, pins: &[(String, String)]) {
        let text: String = pins
            .iter()
            .map(|(host, fingerprint)| format!("{host} {fingerprint}\n"))
            .collect();
        let _ = write_private(&self.path, text.as_bytes());
    }
}

impl PinStore for FilePinStore {
    fn pinned(&self, host: &str) -> Option<String> {
        self.pins.lock().ok().and_then(|pins| {
            pins.iter()
                .find(|(known, _)| known == host)
                .map(|(_, fingerprint)| fingerprint.clone())
        })
    }

    fn pin(&self, host: &str, fingerprint: String) {
        let Ok(mut pins) = self.pins.lock() else {
            return;
        };
        if let Some(slot) = pins.iter_mut().find(|(known, _)| known == host) {
            slot.1 = fingerprint;
        } else {
            pins.push((host.to_owned(), fingerprint));
        }
        self.flush(&pins);
    }

    fn unpin(&self, host: &str) {
        let Ok(mut pins) = self.pins.lock() else {
            return;
        };
        pins.retain(|(known, _)| known != host);
        self.flush(&pins);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("seele-identity-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn an_identity_survives_the_process_that_made_it() {
        // The whole point. Without this the same nickname cannot be reclaimed
        // from the same machine, because CASPER binds it to the first key.
        let dir = scratch("survives");
        let path = dir.join("identity.key");

        let first = load_or_create(&path).expect("create");
        let second = load_or_create(&path).expect("reload");

        assert_eq!(first.to_bytes(), second.to_bytes());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_identity_file_is_not_readable_by_anybody_else() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("mode");
        let path = dir.join("identity.key");
        load_or_create(&path).expect("create");

        let mode = std::fs::metadata(&path).expect("stat").permissions().mode();
        assert_eq!(mode & 0o077, 0, "the private key is readable by others");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_a_key_is_reported_rather_than_replaced() {
        // Overwriting it would destroy the identity of somebody whose disk
        // filled up mid-write. Refusing to start is recoverable; this is not.
        let dir = scratch("garbage");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("identity.key");
        std::fs::write(&path, b"not a key").expect("write");

        assert!(load_or_create(&path).is_err());
        assert_eq!(
            std::fs::read(&path).expect("read"),
            b"not a key",
            "a corrupt identity was silently replaced"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pin_survives_the_process_that_made_it() {
        // Trust on first use needs the first use remembered, or every contact
        // is a first contact and the warning can never fire.
        let dir = scratch("pins");
        let path = dir.join("pins");

        {
            let store = FilePinStore::open(path.clone()).expect("open");
            store.pin("seele.exemplo", "abc123".into());
        }

        let reopened = FilePinStore::open(path).expect("reopen");
        assert_eq!(reopened.pinned("seele.exemplo"), Some("abc123".into()));
        assert_eq!(reopened.pinned("outro"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn repinning_a_host_replaces_rather_than_appends() {
        // A file with two entries for one host answers differently depending on
        // which one is found first, which is a coin flip on a security check.
        let dir = scratch("repin");
        let path = dir.join("pins");
        let store = FilePinStore::open(path.clone()).expect("open");

        store.pin("seele.exemplo", "antigo".into());
        store.pin("seele.exemplo", "novo".into());

        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text.lines().count(), 1, "two pins for one host:\n{text}");
        assert_eq!(store.pinned("seele.exemplo"), Some("novo".into()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_pin_file_costs_a_repin_and_not_a_failure_to_start() {
        let dir = scratch("corrupt-pins");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("pins");
        std::fs::write(&path, b"\x00\x01 sem sentido\n").expect("write");

        let store = FilePinStore::open(path).expect("a corrupt cache stopped the client");
        assert_eq!(store.pinned("seele.exemplo"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
