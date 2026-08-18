//! Attachments: the bytes beside the database, under a fixed ceiling.
//!
//! ADR 0027. The whole of this module exists to keep one promise: **a Dogma
//! never holds more attachment bytes than the number whoever hosts it chose.**
//! Everything else here — deduplication, expiry, the sweep at boot — is
//! bookkeeping in service of that.
//!
//! # The check happens before the first byte, never after
//!
//! The tempting shape is to receive the file and then tidy up. It loses the
//! property outright: for however long the transfer took, the disk was over the
//! ceiling, and a ceiling that is exceeded for thirty seconds is not a ceiling.
//! So [`Store::reserve`] is called with the **declared** size, evicts whatever
//! it must to make that size fit, and only then is the sender allowed to write.
//!
//! A declared size that lies dies at the end rather than the beginning: the
//! stream is cut at the declared length and the hash does not close.
//!
//! # Why the reservation is counted like stored bytes
//!
//! Two transfers arriving at once would each look at the same free space and
//! each believe it fits. [`Ledger`] therefore holds `stored + reserved`, and
//! that sum — not `stored` — is what is compared against the quota. The
//! invariant is written as a test, driven by a schedule of interleaved
//! transfers, because the instant it fails is the instant between accepting and
//! evicting and no single-file test visits it.
//!
//! # Rows outlive bytes
//!
//! Expiring an attachment deletes the blob and **keeps the row**, stamping
//! `expired_at`. The row is what lets the message say «this file expired» with
//! the name and the size it had; a row deleted instead would render as a
//! message with nothing in it, and nobody would learn there had been a file.
//! The cost is written in the ADR and accepted: this table never loses rows.
//!
//! # Lock order
//!
//! [`Ledger`] first, `Casper` second, always. Eviction needs both — it picks
//! the oldest row from the database and deletes a file — and a path that took
//! them the other way round would deadlock against a transfer arriving at the
//! same moment. Both are passed in by the caller rather than held here, which
//! is what makes the order visible at every call site instead of buried.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};
use seele_proto::control::{AttachmentInfo, AttachmentState, MAX_FILE_NAME_LEN};
use seele_proto::ids::{AttachmentId, MessageId};

use super::{now_seconds, Casper};

/// How many bytes of attachments a Dogma holds by default.
///
/// ADR 0027 picks a gibibyte, and picks a **number** rather than a policy: the
/// point of the whole decision is that the worst case on disk is knowable on
/// day one. A Dogma that has never been configured is a Dogma at this number.
pub const DEFAULT_QUOTA_BYTES: u64 = 1024 * 1024 * 1024;

/// Where the chosen ceiling lives in the `configuracao` table.
///
/// Not a TOML file: `specs/04-servidor-seele.md` describes one that does not
/// exist, and migration 2 wrote the criterion for this table when it created it
/// — "configuração do Dogma que não cabe num arquivo, porque muda em tempo de
/// execução e precisa sobreviver a reinício". Changing the ceiling with the
/// Dogma up is the normal case.
pub const QUOTA_KEY: &str = "anexos_teto";

/// The per-file limit is the total divided by this.
///
/// Derived and never configured, so the two numbers cannot be set to an absurd
/// pair. Sixteen is the smallest divisor that keeps the property worth stating:
/// **one upload can never cost more than a sixteenth of the history.** A single
/// 900 MiB file into a 1 GiB Dogma would otherwise empty everybody's history in
/// one act, which is the failure this exists to prevent.
pub const PER_FILE_DIVISOR: u64 = 16;

/// The largest single file a Dogma with this quota accepts.
#[must_use]
pub fn per_file_limit(quota: u64) -> u64 {
    (quota / PER_FILE_DIVISOR).max(1)
}

/// Suffix of a transfer still in flight.
///
/// A blob is named after the SHA-256 of its content, which is not known until
/// the last byte. So a transfer writes to a scratch name and is renamed into
/// place at the end — and a scratch file found at boot is the remains of a
/// transfer that fell, which is swept.
const SCRATCH_SUFFIX: &str = ".parcial";

/// Why the Dogma would not take a file.
///
/// Enumerated, and turned into `AttachmentRefusal` on the wire by the caller.
/// `specs/02-protocolo.md`: no free-form string reaches an interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// Larger than [`per_file_limit`] for this Dogma's quota.
    #[error("file is {declared} bytes, over the {limit}-byte per-file limit")]
    TooLarge {
        /// What the sender declared.
        declared: u64,
        /// What this Dogma allows.
        limit: u64,
    },
    /// Every byte of the quota is already spoken for by transfers in flight.
    ///
    /// Distinct from evicting: eviction frees **stored** bytes, and there is
    /// nothing it can do about bytes another transfer has already reserved.
    /// Refusing is the correct failure — the sender may try again in a moment —
    /// and accepting would be the one thing this module may not do.
    #[error("no room: {reserved} bytes of the {quota}-byte ceiling are in flight")]
    NoRoom {
        /// Bytes reserved by transfers already under way.
        reserved: u64,
        /// The ceiling.
        quota: u64,
    },
}

/// The running account of attachment bytes.
///
/// Held in memory and rebuilt at boot from the database and the directory. It
/// is deliberately not a query: the question "may these bytes land" is asked
/// once per transfer and answered against the sum of what is stored **and**
/// what is in flight, and the second half exists nowhere on disk.
#[derive(Debug, Clone, Copy)]
pub struct Ledger {
    quota: u64,
    stored: u64,
    reserved: u64,
}

impl Ledger {
    /// A ledger for a Dogma holding `stored` bytes under a ceiling of `quota`.
    #[must_use]
    pub fn new(quota: u64, stored: u64) -> Self {
        Self {
            quota,
            stored,
            reserved: 0,
        }
    }

    /// The ceiling.
    #[must_use]
    pub fn quota(&self) -> u64 {
        self.quota
    }

    /// Bytes on disk right now.
    #[must_use]
    pub fn stored(&self) -> u64 {
        self.stored
    }

    /// Bytes promised to transfers under way.
    #[must_use]
    pub fn reserved(&self) -> u64 {
        self.reserved
    }

    /// Everything the ceiling is spent on: on disk plus in flight.
    ///
    /// **This is the number that must never exceed [`Self::quota`]**, at any
    /// instant, including the instant in the middle of a transfer.
    #[must_use]
    pub fn spoken_for(&self) -> u64 {
        self.stored.saturating_add(self.reserved)
    }

    /// How much of the ceiling is unspoken for.
    #[must_use]
    pub fn free(&self) -> u64 {
        self.quota.saturating_sub(self.spoken_for())
    }
}

/// A granted right to write a known number of bytes.
///
/// Not `Copy` and not silently droppable in spirit: every path that takes one
/// must hand it back through [`Store::keep`] or [`Store::abandon`], or the
/// ceiling stays spent on a transfer that ended. The session has exactly one
/// place where a transfer finishes, for that reason.
#[derive(Debug)]
#[must_use = "a reservation that is neither kept nor abandoned spends the ceiling for ever"]
pub struct Reservation {
    bytes: u64,
    scratch: PathBuf,
}

impl Reservation {
    /// How many bytes this reservation is worth.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Where the arriving bytes are written until the hash is known.
    #[must_use]
    pub fn scratch(&self) -> &Path {
        &self.scratch
    }
}

/// Everything about a file that has just finished arriving.
///
/// One argument instead of four, because the four are always the same four and
/// three of them are strings: a call site that passed the file name where the
/// declared type belongs would compile.
#[derive(Debug, Clone, Copy)]
pub struct Landing<'a> {
    /// SHA-256 of the bytes that arrived, lowercase hex. Computed here, never
    /// taken from the sender's word for it.
    pub content_hash: &'a str,
    /// Which message it hangs off.
    pub message: MessageId,
    /// The name the sender gave.
    pub file_name: &'a str,
    /// The type the sender claimed.
    pub declared_type: &'a str,
}

/// An attachment as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAttachment {
    /// Row identifier.
    pub id: AttachmentId,
    /// Which message carries it.
    pub message: MessageId,
    /// SHA-256 of the content, lowercase hex. Also the file name on disk.
    pub content_hash: String,
    /// The name the sender gave. A column, never a path.
    pub file_name: String,
    /// The type the sender claimed. A claim, not a fact.
    pub declared_type: String,
    /// How many bytes.
    pub byte_size: u64,
    /// When it arrived, seconds since the epoch.
    pub created_at: i64,
    /// When the bytes were deleted, if they were.
    pub expired_at: Option<i64>,
}

impl StoredAttachment {
    /// What a client is told about it.
    ///
    /// The state is a variant and never a sentence: the shell decides how to
    /// present «expired», the same way it decides every other enumerated reason
    /// (`specs/02-protocolo.md`).
    #[must_use]
    pub fn info(&self) -> AttachmentInfo {
        AttachmentInfo {
            id: self.id,
            file_name: self.file_name.clone(),
            declared_type: self.declared_type.clone(),
            byte_size: self.byte_size,
            state: if self.expired_at.is_some() {
                AttachmentState::Expired
            } else {
                AttachmentState::Available
            },
        }
    }
}

/// The attachment table, over CASPER.
pub struct Attachments<'a> {
    casper: &'a Casper,
}

impl<'a> Attachments<'a> {
    /// Borrows a store.
    #[must_use]
    pub fn new(casper: &'a Casper) -> Self {
        Self { casper }
    }

    /// Reads one row, expired or not.
    ///
    /// Expired ones come back on purpose: a client asking for a file that is
    /// gone must be told that it expired, and a `None` here would make it
    /// indistinguishable from a file that never existed.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn one(&self, id: AttachmentId) -> Result<Option<StoredAttachment>> {
        Ok(self
            .casper
            .connection()
            .query_row(
                "SELECT id, message_id, content_hash, file_name, declared_type,
                        byte_size, created_at, expired_at
                 FROM attachments WHERE id = ?1",
                [id.get() as i64],
                row_to_attachment,
            )
            .optional()?)
    }

    /// The attachment on a message, if it has one.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn of_message(&self, message: MessageId) -> Result<Option<StoredAttachment>> {
        Ok(self
            .casper
            .connection()
            .query_row(
                "SELECT id, message_id, content_hash, file_name, declared_type,
                        byte_size, created_at, expired_at
                 FROM attachments WHERE message_id = ?1 ORDER BY id LIMIT 1",
                [message.get() as i64],
                row_to_attachment,
            )
            .optional()?)
    }

    /// The attachments on a page of messages, keyed by message.
    ///
    /// One query for the page rather than one per row, for the reason
    /// [`super::messages::Messages::history`] joins nicknames: a screenful of
    /// history is fifty round trips through SQLite otherwise.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn for_messages(
        &self,
        messages: &[MessageId],
    ) -> Result<HashMap<MessageId, StoredAttachment>> {
        let mut found = HashMap::new();
        if messages.is_empty() {
            return Ok(found);
        }
        let mut statement = self.casper.connection().prepare(
            "SELECT id, message_id, content_hash, file_name, declared_type,
                    byte_size, created_at, expired_at
             FROM attachments WHERE message_id = ?1",
        )?;
        for message in messages {
            let rows = statement.query_map([message.get() as i64], row_to_attachment)?;
            for attachment in rows.flatten() {
                found.insert(attachment.message, attachment);
            }
        }
        Ok(found)
    }

    /// How many live rows still point at these bytes.
    ///
    /// The count that decides whether the blob may go. Removing one pilot's
    /// copy must not remove another's, and that is arithmetic rather than
    /// intention.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn live_copies(&self, content_hash: &str) -> Result<u64> {
        let count: i64 = self.casper.connection().query_row(
            "SELECT COUNT(*) FROM attachments
             WHERE content_hash = ?1 AND expired_at IS NULL",
            [content_hash],
            |row| row.get(0),
        )?;
        Ok(count.unsigned_abs())
    }

    /// The oldest attachment whose bytes are still on disk.
    ///
    /// What eviction asks. Oldest by arrival, ties broken by row identifier, so
    /// two files that landed in the same second still have an order.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn oldest_live(&self) -> Result<Option<StoredAttachment>> {
        Ok(self
            .casper
            .connection()
            .query_row(
                "SELECT id, message_id, content_hash, file_name, declared_type,
                        byte_size, created_at, expired_at
                 FROM attachments WHERE expired_at IS NULL
                 ORDER BY created_at, id LIMIT 1",
                [],
                row_to_attachment,
            )
            .optional()?)
    }

    /// Every distinct blob still referenced by a live row, with its size.
    ///
    /// Distinct, because two rows sharing a hash are one file on disk. Summing
    /// `byte_size` over rows would count a deduplicated picture twice and make
    /// the ledger believe the disk is fuller than it is.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn live_blobs(&self) -> Result<HashMap<String, u64>> {
        let mut statement = self.casper.connection().prepare(
            "SELECT content_hash, MAX(byte_size) FROM attachments
             WHERE expired_at IS NULL GROUP BY content_hash",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.unsigned_abs(),
            ))
        })?;
        Ok(rows.flatten().collect())
    }

    /// Stamps a row as expired. The bytes are somebody else's problem.
    ///
    /// Returns whether the row was live before this.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn expire(&self, id: AttachmentId) -> Result<bool> {
        let affected = self.casper.connection().execute(
            "UPDATE attachments SET expired_at = ?1 WHERE id = ?2 AND expired_at IS NULL",
            params![now_seconds(), id.get() as i64],
        )?;
        Ok(affected > 0)
    }

    /// Stamps every live row pointing at these bytes.
    ///
    /// Used when the blob is found missing at boot: the row is the truth, and a
    /// row claiming a file that is not there reads exactly as expired.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn expire_blob(&self, content_hash: &str) -> Result<usize> {
        Ok(self.casper.connection().execute(
            "UPDATE attachments SET expired_at = ?1
             WHERE content_hash = ?2 AND expired_at IS NULL",
            params![now_seconds(), content_hash],
        )?)
    }

    /// Writes the row for an attachment whose bytes have landed.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn record(
        &self,
        message: MessageId,
        content_hash: &str,
        file_name: &str,
        declared_type: &str,
        byte_size: u64,
    ) -> Result<StoredAttachment> {
        let now = now_seconds();
        self.casper.connection().execute(
            "INSERT INTO attachments
               (message_id, content_hash, file_name, declared_type, byte_size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                message.get() as i64,
                content_hash,
                file_name,
                declared_type,
                byte_size as i64,
                now
            ],
        )?;
        Ok(StoredAttachment {
            id: AttachmentId(self.casper.connection().last_insert_rowid() as u64),
            message,
            content_hash: content_hash.to_owned(),
            file_name: file_name.to_owned(),
            declared_type: declared_type.to_owned(),
            byte_size,
            created_at: now,
            expired_at: None,
        })
    }
}

fn row_to_attachment(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredAttachment> {
    Ok(StoredAttachment {
        id: AttachmentId(row.get::<_, i64>(0)? as u64),
        message: MessageId(row.get::<_, i64>(1)? as u64),
        content_hash: row.get(2)?,
        file_name: row.get(3)?,
        declared_type: row.get(4)?,
        byte_size: row.get::<_, i64>(5)?.unsigned_abs(),
        created_at: row.get(6)?,
        expired_at: row.get(7)?,
    })
}

/// The ceiling read out of `configuracao`, or the default.
///
/// Absence means the default rather than a written row, so a Dogma that existed
/// before attachments did comes up at [`DEFAULT_QUOTA_BYTES`] without any
/// migration touching it — and a later build that changed the default would
/// change it for everybody who never chose. Choosing writes the row.
///
/// # Errors
///
/// Fails on a database error.
pub fn quota(casper: &Casper) -> Result<u64> {
    let chosen: Option<i64> = casper
        .connection()
        .query_row(
            "SELECT valor FROM configuracao WHERE chave = ?1",
            [QUOTA_KEY],
            |row| row.get(0),
        )
        .optional()?;
    Ok(chosen.map_or(DEFAULT_QUOTA_BYTES, i64::unsigned_abs))
}

/// Whether whoever hosts has chosen a ceiling, or is on the default.
///
/// # Errors
///
/// Fails on a database error.
pub fn quota_is_chosen(casper: &Casper) -> Result<bool> {
    let count: i64 = casper.connection().query_row(
        "SELECT COUNT(*) FROM configuracao WHERE chave = ?1",
        [QUOTA_KEY],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Writes the chosen ceiling.
///
/// # Errors
///
/// Fails on a database error, or if the number is zero — a Dogma that accepts
/// no attachment at all is spelled by not granting the permission, and a
/// ceiling of zero would instead be a Dogma that accepts the transfer and then
/// cannot keep it.
pub fn set_quota(casper: &Casper, bytes: u64) -> Result<()> {
    anyhow::ensure!(bytes > 0, "o teto de anexos não pode ser zero");
    casper.connection().execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
         ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
        params![QUOTA_KEY, bytes as i64],
    )?;
    Ok(())
}

/// The bytes on disk, and the arithmetic that keeps them under the ceiling.
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Opens the directory, sweeps it against the database, and counts.
    ///
    /// Two kinds of disagreement are possible between a directory and a table,
    /// and the rule is that **the row is the truth**:
    ///
    /// - a blob no live row points at is an orphan, and is deleted;
    /// - a live row whose blob is missing is expired, which is a state the
    ///   design already has and which every shell already draws.
    ///
    /// # Errors
    ///
    /// Fails if the directory cannot be created or read, or the database cannot
    /// be queried.
    pub fn open(root: PathBuf, casper: &Casper) -> Result<(Self, Ledger)> {
        std::fs::create_dir_all(&root)
            .with_context(|| format!("could not create {}", root.display()))?;

        let attachments = Attachments::new(casper);
        let live = attachments.live_blobs()?;

        // Keyed by hash, valued by what the **file** weighs rather than what a
        // row says it weighs. The ceiling is a promise about a directory, so
        // every number in the ledger is measured there; a row and a file that
        // disagreed would otherwise drift the ledger away from the disk one
        // byte at a time, and nothing would notice until it mattered.
        let mut on_disk: HashMap<String, u64> = HashMap::new();
        let entries = std::fs::read_dir(&root)
            .with_context(|| format!("could not read {}", root.display()))?;
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            // A scratch file is the remains of a transfer that fell. There is no
            // resumption (ADR 0027), so it is rubbish rather than progress.
            if name.ends_with(SCRATCH_SUFFIX) || !live.contains_key(&name) {
                if let Err(error) = std::fs::remove_file(entry.path()) {
                    tracing::warn!(%error, file = %name, "could not sweep an orphan attachment");
                } else {
                    tracing::info!(file = %name, "orphan attachment swept");
                }
                continue;
            }
            let weight = entry.metadata().map(|meta| meta.len()).unwrap_or_default();
            on_disk.insert(name, weight);
        }

        let mut stored = 0_u64;
        for hash in live.keys() {
            if let Some(weight) = on_disk.get(hash) {
                stored = stored.saturating_add(*weight);
            } else {
                let rows = attachments.expire_blob(hash)?;
                tracing::warn!(hash = %hash, rows, "attachment bytes are missing; the rows now read as expired");
            }
        }

        let quota = quota(casper)?;
        let store = Self { root };
        // A ceiling lowered below what is already there is not a mistake to
        // refuse: whoever hosts has said the disk is worth less than it was.
        // Evicting now is the only reading of that which keeps the promise.
        let mut ledger = Ledger::new(quota, stored);
        store.evict_until(&mut ledger, casper, 0)?;
        Ok((store, ledger))
    }

    /// Where the blobs live.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a blob with this hash is, or would be.
    ///
    /// The name is the hash and nothing else. **No character anybody else chose
    /// touches the filesystem** — no `../`, no `CON`, no null byte, no two
    /// capitals colliding on macOS. The name the sender gave is a column.
    #[must_use]
    pub fn blob_path(&self, content_hash: &str) -> PathBuf {
        self.root.join(content_hash)
    }

    /// Whether these bytes are already here.
    #[must_use]
    pub fn has_blob(&self, content_hash: &str) -> bool {
        self.blob_path(content_hash).is_file()
    }

    /// Makes room for a declared size and reserves it.
    ///
    /// The order is the decision: refuse what is too big, evict until the
    /// declared size fits under the ceiling, and only then hand back the right
    /// to write. Nothing here reads a byte of the file.
    ///
    /// # Errors
    ///
    /// Fails on a database or filesystem error. Returns [`Refusal`] when the
    /// file may not be taken at all.
    pub fn reserve(
        &self,
        ledger: &mut Ledger,
        casper: &Casper,
        declared: u64,
        scratch_name: &str,
    ) -> Result<std::result::Result<Reservation, Refusal>> {
        let limit = per_file_limit(ledger.quota);
        if declared > limit {
            return Ok(Err(Refusal::TooLarge { declared, limit }));
        }

        self.evict_until(ledger, casper, declared)?;

        if ledger.free() < declared {
            return Ok(Err(Refusal::NoRoom {
                reserved: ledger.reserved,
                quota: ledger.quota,
            }));
        }

        ledger.reserved = ledger.reserved.saturating_add(declared);
        Ok(Ok(Reservation {
            bytes: declared,
            scratch: self.root.join(format!("{scratch_name}{SCRATCH_SUFFIX}")),
        }))
    }

    /// Expires the oldest attachments until `wanted` bytes would fit.
    ///
    /// Expiring one row frees nothing when another live row still points at the
    /// same blob — which is exactly right, and why this loops rather than
    /// counting. It stops when there is nothing live left to expire, and the
    /// caller is the one that decides what a still-full ledger means.
    fn evict_until(&self, ledger: &mut Ledger, casper: &Casper, wanted: u64) -> Result<()> {
        let attachments = Attachments::new(casper);
        while ledger.free() < wanted || ledger.spoken_for() > ledger.quota {
            let Some(oldest) = attachments.oldest_live()? else {
                return Ok(());
            };
            if !attachments.expire(oldest.id)? {
                return Ok(());
            }
            // The blob goes only when the last live row that referenced it has.
            // `RemoveMessage` deletes somebody else's message, and deleting one
            // pilot's copy may not delete another's.
            if attachments.live_copies(&oldest.content_hash)? == 0 {
                let path = self.blob_path(&oldest.content_hash);
                // What the file weighs, not what the row claims. They are the
                // same number every time the wire produced them — the declared
                // size is enforced against the bytes before anything is kept —
                // but the ledger's job is to describe the directory, and the
                // directory is what it asks.
                let weight = std::fs::metadata(&path).map_or(oldest.byte_size, |meta| meta.len());
                if let Err(error) = std::fs::remove_file(&path) {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!(%error, path = %path.display(), "could not delete an evicted attachment");
                    }
                    ledger.stored = ledger.stored.saturating_sub(oldest.byte_size);
                } else {
                    ledger.stored = ledger.stored.saturating_sub(weight);
                }
            }
            tracing::info!(
                attachment = %oldest.id,
                name = %oldest.file_name,
                bytes = oldest.byte_size,
                "attachment expired to stay under the ceiling"
            );
        }
        Ok(())
    }

    /// Gives a reservation back without keeping anything.
    ///
    /// A fallen transfer, a size that lied, a hash that did not close. The
    /// partial file goes and nothing is published — no half message, and no
    /// message pointing at a file that does not exist.
    pub fn abandon(&self, ledger: &mut Ledger, reservation: Reservation) {
        if let Err(error) = std::fs::remove_file(&reservation.scratch) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, "could not delete a partial attachment");
            }
        }
        ledger.reserved = ledger.reserved.saturating_sub(reservation.bytes);
    }

    /// Keeps the bytes and writes the row.
    ///
    /// Deduplication happens here, **after** the bytes arrived, and that is
    /// deliberate: answering "I already have that" before receiving would tell
    /// the sender, by timing alone, that somebody has already sent that exact
    /// file — including into a Line they may not read. The disk is the resource
    /// under a hard ceiling, and the disk is still spared.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be moved into place or the row cannot be
    /// written.
    pub fn keep(
        &self,
        ledger: &mut Ledger,
        casper: &Casper,
        reservation: Reservation,
        landing: &Landing<'_>,
    ) -> Result<StoredAttachment> {
        let Landing {
            content_hash,
            message,
            file_name,
            declared_type,
        } = *landing;
        let destination = self.blob_path(content_hash);
        let already_here = destination.is_file();
        if already_here {
            let _ = std::fs::remove_file(&reservation.scratch);
        } else {
            std::fs::rename(&reservation.scratch, &destination).with_context(|| {
                format!(
                    "could not store the attachment at {}",
                    destination.display()
                )
            })?;
            quarantine(&destination);
        }

        let stored = Attachments::new(casper).record(
            message,
            content_hash,
            file_name,
            declared_type,
            reservation.bytes,
        )?;

        ledger.reserved = ledger.reserved.saturating_sub(reservation.bytes);
        if !already_here {
            ledger.stored = ledger.stored.saturating_add(reservation.bytes);
        }
        Ok(stored)
    }
}

/// Marks a freshly written file with the operating system's own quarantine.
///
/// ADR 0027: the Dogma does not scan for viruses, does not have an engine and
/// will not grow one. What it can do is set the flag Gatekeeper and SmartScreen
/// already look at — the same thing a browser does, and the only concrete
/// answer this product has. It is not antivirus; it is the guard the system
/// already has, and it only works if whoever writes the file turns it on.
///
/// Best effort by design: a filesystem that does not carry extended attributes
/// is a reason to go on, not to refuse the file.
fn quarantine(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        // `com.apple.quarantine`. The five fields are flags, timestamp, agent,
        // UUID; the agent name is what the dialog shows.
        let value = format!("0083;{:x};SEELE;", now_seconds());
        let _ = std::process::Command::new("xattr")
            .args(["-w", "com.apple.quarantine", &value])
            .arg(path)
            .output();
    }
    #[cfg(target_os = "windows")]
    {
        // The `Zone.Identifier` alternate data stream. Zone 3 is "Internet",
        // which is what makes SmartScreen stop the file in front of whoever
        // opens it.
        let stream = format!("{}:Zone.Identifier", path.display());
        let _ = std::fs::write(stream, "[ZoneTransfer]\r\nZoneId=3\r\n");
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Linux has no system-wide equivalent that anything reads. Saying so is
        // better than writing an attribute nobody consults and calling it a
        // guard.
        let _ = path;
    }
}

/// Bounds a sender-chosen file name.
///
/// The name never touches the filesystem — the blob is named after its hash —
/// so this is about what a shell has to draw, not about path safety. A name
/// that is empty or only spaces draws a row with nothing in it, which is the
/// same failure `check_name` refuses for rooms.
///
/// # Errors
///
/// Returns `false` when the name may not be carried.
#[must_use]
pub fn file_name_is_usable(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= MAX_FILE_NAME_LEN && !name.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casper::messages::{Messages, PendingMessage};
    use crate::casper::Location;

    /// A Dogma with one Line, one pilot, and a directory to put bytes in.
    fn dogma() -> (Casper, tempfile::TempDir) {
        let casper = Casper::open(&Location::Memory).unwrap();
        casper
            .connection()
            .execute_batch(
                "INSERT INTO lines (id, name) VALUES (1, 'geral');
                 INSERT INTO pilots (id, nickname, public_key, created_at)
                   VALUES (1, 'ayanami', X'01', 0);",
            )
            .unwrap();
        (casper, tempfile::tempdir().unwrap())
    }

    /// Writes a message so an attachment has something to hang off.
    fn message(casper: &mut Casper, body: &str) -> MessageId {
        let stored = Messages::new(casper)
            .append_batch(&[PendingMessage {
                line: seele_proto::ids::LineId(1),
                author: seele_proto::ids::PilotId(1),
                author_nickname: "ayanami".into(),
                body: body.into(),
                replies_to: None,
                client_message_id: None,
            }])
            .unwrap();
        stored[0].id
    }

    /// Puts a file of `size` bytes through the whole path: reserve, write, keep.
    ///
    /// The hash is made up rather than computed, because these tests are about
    /// the ceiling and not about hashing — a distinct `seed` is a distinct blob.
    fn land(
        store: &Store,
        ledger: &mut Ledger,
        casper: &mut Casper,
        size: u64,
        seed: u8,
    ) -> Result<StoredAttachment, Refusal> {
        let hash = format!("{seed:064x}");
        let reservation = store
            .reserve(ledger, casper, size, &format!("scratch-{seed}"))
            .unwrap()?;
        std::fs::write(reservation.scratch(), vec![seed; size as usize]).unwrap();
        let id = message(casper, "com anexo");
        Ok(store
            .keep(
                ledger,
                casper,
                reservation,
                &Landing {
                    content_hash: &hash,
                    message: id,
                    file_name: &format!("foto-{seed}.png"),
                    declared_type: "image/png",
                },
            )
            .unwrap())
    }

    fn open(root: &Path, casper: &Casper) -> (Store, Ledger) {
        Store::open(root.to_path_buf(), casper).unwrap()
    }

    #[test]
    fn a_dogma_that_was_never_configured_is_at_the_default() {
        // The first boot of a Dogma that already existed. No migration writes
        // this row, so absence has to mean the default rather than zero — and a
        // quota of zero would be a Dogma that accepts a transfer and then
        // cannot keep it.
        let (casper, _) = dogma();
        assert_eq!(quota(&casper).unwrap(), DEFAULT_QUOTA_BYTES);
        assert!(!quota_is_chosen(&casper).unwrap());
    }

    #[test]
    fn choosing_a_ceiling_survives_a_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let location = Location::File(directory.path().join("dogma.db"));
        {
            let casper = Casper::open(&location).unwrap();
            set_quota(&casper, 4 * 1024 * 1024).unwrap();
        }
        let casper = Casper::open(&location).unwrap();
        assert_eq!(quota(&casper).unwrap(), 4 * 1024 * 1024);
        assert!(quota_is_chosen(&casper).unwrap());
    }

    #[test]
    fn a_ceiling_of_zero_is_refused() {
        let (casper, _) = dogma();
        assert!(set_quota(&casper, 0).is_err());
    }

    #[test]
    fn the_per_file_limit_is_derived_and_leaves_room_for_a_history() {
        // The property, stated as arithmetic: one upload can never cost more
        // than a sixteenth of what is stored. A 900 MiB file into a 1 GiB Dogma
        // would empty everybody's history in one act.
        assert_eq!(per_file_limit(DEFAULT_QUOTA_BYTES), 64 * 1024 * 1024);
        assert!(per_file_limit(1) >= 1, "a tiny Dogma still takes something");
        for quota in [1_024, 1_000_000, DEFAULT_QUOTA_BYTES, u64::MAX / 2] {
            assert!(
                per_file_limit(quota) * PER_FILE_DIVISOR <= quota + PER_FILE_DIVISOR,
                "the per-file limit is not a fraction of {quota}"
            );
        }
    }

    #[test]
    fn a_file_over_the_per_file_limit_is_refused_and_nothing_is_evicted() {
        // Refused, not accepted to be thrown away afterwards — and refusing it
        // must not have cost anybody their history on the way.
        let (mut casper, directory) = dogma();
        set_quota(&casper, 1_600).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);

        land(&store, &mut ledger, &mut casper, 100, 1).unwrap();
        let refusal = land(&store, &mut ledger, &mut casper, 1_000, 2).unwrap_err();
        assert!(matches!(refusal, Refusal::TooLarge { limit: 100, .. }));
        assert_eq!(ledger.stored(), 100, "a refusal evicted somebody's file");
    }

    #[test]
    fn the_ceiling_holds_when_the_disk_fills() {
        // The plain case. Sixteen files of the per-file limit exactly fill the
        // quota; the seventeenth pushes the first out and the total does not
        // move.
        let (mut casper, directory) = dogma();
        set_quota(&casper, 1_600).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);

        for seed in 0..16 {
            land(&store, &mut ledger, &mut casper, 100, seed).unwrap();
            assert!(
                ledger.spoken_for() <= ledger.quota(),
                "the ceiling was passed at file {seed}"
            );
        }
        assert_eq!(ledger.stored(), 1_600);

        land(&store, &mut ledger, &mut casper, 100, 16).unwrap();
        assert_eq!(ledger.stored(), 1_600, "the seventeenth file grew the disk");

        // And what was evicted is the oldest, which is the whole policy.
        let survivors: Vec<String> = Attachments::new(&casper)
            .live_blobs()
            .unwrap()
            .into_keys()
            .collect();
        assert!(
            !survivors.contains(&format!("{:064x}", 0)),
            "the oldest attachment survived: {survivors:?}"
        );
        assert_eq!(survivors.len(), 16);
    }

    #[test]
    fn eviction_happens_before_the_first_byte_and_not_after() {
        // The property the ADR says is the whole decision. Measured where it
        // can actually fail: at the instant the reservation is granted, with
        // nothing written yet. If the check ran after the transfer, the ledger
        // here would be over the quota by exactly one file.
        let (mut casper, directory) = dogma();
        set_quota(&casper, 320).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);

        for seed in 0..16 {
            land(&store, &mut ledger, &mut casper, 20, seed).unwrap();
        }
        assert_eq!(ledger.stored(), 320);

        let reservation = store
            .reserve(&mut ledger, &casper, 20, "medindo")
            .unwrap()
            .unwrap();
        assert!(
            ledger.spoken_for() <= ledger.quota(),
            "accepting spent more than the ceiling before a byte was written: \
             {} of {}",
            ledger.spoken_for(),
            ledger.quota()
        );
        assert_eq!(
            ledger.stored(),
            300,
            "the room was not made before the transfer was allowed to start"
        );
        store.abandon(&mut ledger, reservation);
    }

    #[test]
    fn transfers_in_flight_cannot_each_believe_they_fit() {
        // Two arriving at once is where a ceiling built on `stored` alone
        // fails: each looks at the same free space, each is told yes, and
        // together they pass the ceiling with nothing to evict.
        let (casper, directory) = dogma();
        set_quota(&casper, 160).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);

        let first = store
            .reserve(&mut ledger, &casper, 10, "a")
            .unwrap()
            .unwrap();
        let mut held = vec![first];
        for numero in 1..16 {
            held.push(
                store
                    .reserve(&mut ledger, &casper, 10, &format!("f{numero}"))
                    .unwrap()
                    .unwrap(),
            );
            assert!(ledger.spoken_for() <= ledger.quota());
        }

        let refusal = store
            .reserve(&mut ledger, &casper, 10, "excedente")
            .unwrap()
            .unwrap_err();
        assert!(
            matches!(refusal, Refusal::NoRoom { .. }),
            "a seventeenth transfer was let in on top of sixteen in flight"
        );

        for reservation in held {
            store.abandon(&mut ledger, reservation);
        }
        assert_eq!(
            ledger.reserved(),
            0,
            "abandoning did not give the room back"
        );
    }

    #[test]
    fn the_ceiling_holds_at_every_instant_under_interleaved_transfers() {
        // The test that matters. A schedule of overlapping transfers — some
        // kept, some abandoned mid-flight, some deduplicating onto bytes that
        // are already here — with the invariant checked after **every** step
        // rather than at the end. The instant between accepting and evicting is
        // where the ADR says the defect lives, and a per-file test never visits
        // it.
        //
        // The numbers are chosen so that the transfers in flight can, on their
        // own, ask for more than the whole ceiling: up to twenty at once, of up
        // to 112 bytes, against a 2 048-byte quota. A schedule whose in-flight
        // total never approaches the quota proves nothing about reservations —
        // it was written that way first, and a mutation that made the ceiling
        // count only what is on disk survived it.
        let (mut casper, directory) = dogma();
        let quota = 2_048_u64;
        set_quota(&casper, quota).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);

        let mut flying: Vec<(Reservation, u8)> = Vec::new();
        let mut refused = 0_u32;
        let mut kept = 0_u32;
        let check = |ledger: &Ledger, step: u32| {
            assert!(
                ledger.spoken_for() <= ledger.quota(),
                "step {step}: {} bytes spoken for under a {}-byte ceiling",
                ledger.spoken_for(),
                ledger.quota()
            );
        };

        for step in 0..400_u32 {
            // A spread of sizes, all inside the per-file limit, plus a handful
            // that repeat so deduplication is exercised on the way.
            //
            // The size is derived from the seed and not from the step, because
            // the seed is what picks the hash and equal hashes mean equal bytes.
            // Written the other way round first, and the test caught the ledger
            // adding one size and subtracting another for the same blob — which
            // is why the ledger now weighs the directory instead of trusting a
            // row.
            let seed = u8::try_from(step % 23).unwrap();
            let size = 16 + u64::from(seed % 7) * 16;
            let scratch = format!("s{step}");

            match store.reserve(&mut ledger, &casper, size, &scratch).unwrap() {
                Ok(reservation) => {
                    check(&ledger, step);
                    std::fs::write(reservation.scratch(), vec![seed; size as usize]).unwrap();
                    flying.push((reservation, seed));
                }
                Err(Refusal::NoRoom { .. }) => refused += 1,
                Err(other) => panic!("step {step}: unexpected refusal {other:?}"),
            }
            check(&ledger, step);

            // Land or drop what is in flight, oldest first, so transfers really
            // do overlap instead of running one at a time. The depth breathes
            // between two and twenty, so the ledger meets both "almost nothing
            // in flight" and "more asked for than the ceiling holds".
            let depth = 2 + usize::try_from((step / 7) % 19).unwrap();
            while flying.len() > depth {
                let (reservation, seed) = flying.remove(0);
                if step % 5 == 0 {
                    store.abandon(&mut ledger, reservation);
                } else {
                    let id = message(&mut casper, "com anexo");
                    // The size that came back is the size that was reserved,
                    // which is what the stream enforces on the wire.
                    let hash = format!("{:064x}", u32::from(seed) * 7 + 1);
                    store
                        .keep(
                            &mut ledger,
                            &casper,
                            reservation,
                            &Landing {
                                content_hash: &hash,
                                message: id,
                                file_name: "foto.png",
                                declared_type: "image/png",
                            },
                        )
                        .unwrap();
                    kept += 1;
                }
                check(&ledger, step);
            }
        }

        for (reservation, _) in flying {
            store.abandon(&mut ledger, reservation);
            check(&ledger, 999);
        }

        assert!(kept > 50, "the schedule barely stored anything: {kept}");
        assert_eq!(ledger.reserved(), 0);
        assert!(ledger.stored() <= quota);
        // And the disk agrees with the ledger, which is the half a counter
        // alone cannot promise.
        let on_disk: u64 = std::fs::read_dir(store.root())
            .unwrap()
            .flatten()
            .filter_map(|entry| entry.metadata().ok())
            .map(|meta| meta.len())
            .sum();
        assert_eq!(
            on_disk,
            ledger.stored(),
            "the ledger and the directory disagree; {refused} transfers were refused"
        );
        assert!(on_disk <= quota, "the directory passed the ceiling");
    }

    #[test]
    fn the_same_file_from_two_people_is_one_blob_and_two_rows() {
        let (mut casper, directory) = dogma();
        set_quota(&casper, 1_600).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);

        let hash = format!("{:064x}", 9);
        for numero in 0..2 {
            let reservation = store
                .reserve(&mut ledger, &casper, 100, &format!("c{numero}"))
                .unwrap()
                .unwrap();
            std::fs::write(reservation.scratch(), vec![9; 100]).unwrap();
            let id = message(&mut casper, "mesma foto");
            store
                .keep(
                    &mut ledger,
                    &casper,
                    reservation,
                    &Landing {
                        content_hash: &hash,
                        message: id,
                        file_name: "foto.png",
                        declared_type: "image/png",
                    },
                )
                .unwrap();
        }

        assert_eq!(ledger.stored(), 100, "the second copy took disk");
        assert_eq!(Attachments::new(&casper).live_copies(&hash).unwrap(), 2);
        assert_eq!(std::fs::read_dir(store.root()).unwrap().count(), 1);
    }

    #[test]
    fn expiring_one_copy_leaves_the_other_readable() {
        // `RemoveMessage` deletes somebody else's message. Deleting one pilot's
        // copy may not delete another's, and that is a count rather than an
        // intention.
        let (mut casper, directory) = dogma();
        set_quota(&casper, 1_600).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);

        let hash = format!("{:064x}", 5);
        let mut rows = Vec::new();
        for numero in 0..2 {
            let reservation = store
                .reserve(&mut ledger, &casper, 100, &format!("c{numero}"))
                .unwrap()
                .unwrap();
            std::fs::write(reservation.scratch(), vec![5; 100]).unwrap();
            let id = message(&mut casper, "mesma foto");
            rows.push(
                store
                    .keep(
                        &mut ledger,
                        &casper,
                        reservation,
                        &Landing {
                            content_hash: &hash,
                            message: id,
                            file_name: "foto.png",
                            declared_type: "image/png",
                        },
                    )
                    .unwrap(),
            );
        }

        let attachments = Attachments::new(&casper);
        attachments.expire(rows[0].id).unwrap();
        assert_eq!(attachments.live_copies(&hash).unwrap(), 1);
        assert!(
            store.has_blob(&hash),
            "one person deleting their copy destroyed somebody else's"
        );
    }

    #[test]
    fn expiring_keeps_the_row_so_the_message_can_still_say_what_was_here() {
        let (mut casper, directory) = dogma();
        set_quota(&casper, 320).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);

        let first = land(&store, &mut ledger, &mut casper, 20, 1).unwrap();
        for seed in 2..18 {
            land(&store, &mut ledger, &mut casper, 20, seed).unwrap();
        }

        let attachments = Attachments::new(&casper);
        let row = attachments
            .one(first.id)
            .unwrap()
            .expect("the row survives");
        assert!(row.expired_at.is_some());
        assert_eq!(row.file_name, "foto-1.png", "the name went with the bytes");
        assert_eq!(row.byte_size, 20);
        assert_eq!(row.info().state, AttachmentState::Expired);
        // And the message it hangs off is untouched — body, and everything else.
        let stored = Messages::new(&mut casper).one(row.message).unwrap();
        assert_eq!(stored.map(|m| m.body), Some("com anexo".to_owned()));
    }

    #[test]
    fn a_blob_that_vanished_from_the_disk_reads_as_expired() {
        // Two things to back up instead of one, and they can diverge. The rule
        // is that the row is the truth: a missing file is not a crash and not a
        // blank, it is the state the design already has.
        let (mut casper, directory) = dogma();
        set_quota(&casper, 1_600).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);
        let landed = land(&store, &mut ledger, &mut casper, 100, 3).unwrap();
        std::fs::remove_file(store.blob_path(&landed.content_hash)).unwrap();
        drop(store);

        let (_store, ledger) = open(directory.path(), &casper);
        assert_eq!(ledger.stored(), 0, "the ledger counted bytes that are gone");
        let row = Attachments::new(&casper).one(landed.id).unwrap().unwrap();
        assert!(row.expired_at.is_some());
        assert_eq!(row.info().state, AttachmentState::Expired);
    }

    #[test]
    fn an_orphan_blob_is_swept_at_boot() {
        let (casper, directory) = dogma();
        std::fs::write(directory.path().join("deadbeef"), b"nobody points at me").unwrap();
        std::fs::write(directory.path().join("meio.parcial"), b"a fallen transfer").unwrap();

        let (_store, ledger) = open(directory.path(), &casper);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        assert_eq!(ledger.stored(), 0);
    }

    #[test]
    fn lowering_the_ceiling_below_what_is_stored_evicts_at_the_next_boot() {
        // Whoever hosts has said the disk is worth less than it was. Keeping
        // the old bytes would be the product deciding it knows better.
        let (mut casper, directory) = dogma();
        set_quota(&casper, 1_600).unwrap();
        let (store, mut ledger) = open(directory.path(), &casper);
        for seed in 0..16 {
            land(&store, &mut ledger, &mut casper, 100, seed).unwrap();
        }
        assert_eq!(ledger.stored(), 1_600);
        drop(store);

        set_quota(&casper, 500).unwrap();
        let (_store, ledger) = open(directory.path(), &casper);
        assert!(
            ledger.stored() <= 500,
            "a lowered ceiling was ignored: {} bytes",
            ledger.stored()
        );
    }

    #[test]
    fn a_file_name_that_draws_nothing_is_refused() {
        assert!(file_name_is_usable("foto.png"));
        assert!(file_name_is_usable("../../etc/passwd"), "names are columns");
        assert!(!file_name_is_usable(""));
        assert!(!file_name_is_usable("   "));
        assert!(!file_name_is_usable("nome\0nulo"));
        assert!(!file_name_is_usable(&"n".repeat(MAX_FILE_NAME_LEN + 1)));
    }
}
