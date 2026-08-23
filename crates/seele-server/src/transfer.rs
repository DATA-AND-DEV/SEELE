//! A stream of its own, per transfer.
//!
//! ADR 0027. A file never crosses the control stream in either direction. The
//! control stream is ordered, and an ordered stream blocks itself: twenty
//! megabytes written into it stop every presence event, every line of text and
//! every `Pong` from everybody behind them until the last byte goes through.
//!
//! So a sender opens a unidirectional stream, writes a header and then the
//! bytes; a Dogma handing a file back opens one of its own. **The answer always
//! comes back on control**, as an enumerated reason, because that is where
//! `specs/02-protocolo.md` keeps every reason.
//!
//! # What priority does, and what it does not
//!
//! `quinn::SendStream::set_priority` orders *our* streams inside **one**
//! connection, and control is put above every transfer. Two people uploading
//! from two different connections do not order against each other, and there is
//! no good answer to that on a home link — ADR 0027 lists it under what has no
//! way out. Voice is worse off still: it is a datagram, not a stream, so it
//! does not participate in this ordering at all and competes for the same
//! upstream bottleneck. The ADR says so, and this comment is not going to
//! pretend otherwise.
//!
//! # Nothing here ever holds a file
//!
//! Blocks of [`BLOCK_LEN`] between the network and the disk, both ways. A
//! Dogma is sized at 1 vCPU and 512 MB (`specs/04-servidor-seele.md`), and a
//! `Vec` of twenty megabytes per simultaneous transfer ends that.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use seele_proto::attachment::{
    AttachmentDelivery, AttachmentHeader, ContentDigest, BLOCK_LEN, CONTENT_HASH_LEN,
};
use seele_proto::control::{AttachmentRefusal, Permission};
use seele_proto::ids::{AttachmentId, ClientMessageId, PilotId};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::casper::attachments::{self, Landing, Ledger, Refusal, Store};
use crate::casper::messages::{Messages, PendingMessage, StoredMessage};
use crate::casper::Casper;
use crate::dogma::{Dogma, Event};
use crate::melchior::Melchior;
use crate::taxa::Vazao;

/// Stream priority for the control stream.
///
/// Above every transfer, and written on both ends rather than left at the
/// default, so that reading either side says what the ordering is instead of
/// implying it.
pub const CONTROL_PRIORITY: i32 = 1;

/// Stream priority for a transfer.
pub const TRANSFER_PRIORITY: i32 = -1;

/// Everything a Dogma needs to take a file or hand one back.
///
/// The store, the ledger and the byte budget travel together because no caller
/// ever wants one without the others, and because the lock order between the
/// first two is a rule rather than a suggestion — see
/// [`crate::casper::attachments`].
pub struct Vault {
    store: Store,
    ledger: Mutex<Ledger>,
    throughput: Mutex<Vazao>,
    /// Distinguishes two transfers that would otherwise write to the same
    /// scratch file. Two streams carrying the same idempotency key at the same
    /// instant is a client retrying before its first attempt died.
    scratches: AtomicU64,
}

impl Vault {
    /// Opens the directory, sweeps it against the database and counts.
    ///
    /// # Errors
    ///
    /// Fails if the directory cannot be created or read.
    pub fn open(root: PathBuf, casper: &Casper) -> Result<Self> {
        let (store, ledger) = Store::open(root, casper)?;
        tracing::info!(
            teto = ledger.quota(),
            guardado = ledger.stored(),
            por_arquivo = attachments::per_file_limit(ledger.quota()),
            "anexos: o teto está de pé"
        );
        Ok(Self {
            store,
            ledger: Mutex::new(ledger),
            throughput: Mutex::new(Vazao::nova()),
            scratches: AtomicU64::new(0),
        })
    }

    /// The ceiling, and how much of it is spent.
    pub async fn ledger(&self) -> Ledger {
        *self.ledger.lock().await
    }
}

/// What became of an arriving transfer.
#[derive(Debug)]
pub enum Outcome {
    /// The bytes landed whole and the message is on the Line.
    Published(Box<StoredMessage>),
    /// Nothing was published, and here is why.
    ///
    /// Nothing means nothing: no half message, and no message pointing at a
    /// file that is not there.
    Refused {
        /// Which of the sender's messages this was.
        client_message_id: ClientMessageId,
        /// The enumerated reason, to be written on the control stream.
        reason: AttachmentRefusal,
    },
}

/// Reads only enough of a transfer to know whom to answer.
///
/// For the Dogma that has nowhere to keep files. The idempotency key is the
/// only name the two ends share before the Dogma has assigned anything, so a
/// refusal that could not name it would be a refusal addressed to nobody — and
/// simply not accepting the stream would leave the sender's bar at zero until
/// the connection went idle, which is the way of failing this project refuses
/// at every other door.
///
/// The bytes are never read.
///
/// # Errors
///
/// Fails if the header is not a header.
pub async fn quem_perguntou(stream: &mut quinn::RecvStream) -> Result<ClientMessageId> {
    let header: AttachmentHeader = crate::frame::read(stream)
        .await
        .context("could not read the transfer header")?;
    Ok(header.client_message_id)
}

/// Takes one arriving transfer, from its first byte to the row it becomes.
///
/// # Errors
///
/// Fails only when the stream or the database does; a refusal is an [`Outcome`]
/// rather than an error, because a refusal is something the sender is waiting
/// to be told.
pub async fn receive(
    vault: &Vault,
    dogma: &Dogma,
    pilot: PilotId,
    nickname: &str,
    stream: &mut quinn::RecvStream,
) -> Result<Outcome> {
    let header: AttachmentHeader = crate::frame::read(stream)
        .await
        .context("could not read the transfer header")?;
    let key = header.client_message_id;

    let refuse = |reason| {
        Ok(Outcome::Refused {
            client_message_id: key,
            reason,
        })
    };

    // Asked of MELCHIOR at the instant the verb is used, and not read from
    // anything the handshake cached. `specs/08-seguranca.md`: "Toda ação é
    // verificada no servidor, sempre, mesmo que o cliente já esconda o botão."
    // A transfer is rare and expensive, so there is nothing to save by caching
    // it, and a role revoked an hour into a session must take effect now.
    let allowed = {
        let guard = dogma.casper.lock().await;
        Melchior::new(&guard)
            .may(pilot, Permission::AttachFile)
            .unwrap_or(false)
    };
    if !allowed {
        tracing::info!(%pilot, "transfer refused: no AttachFile");
        return refuse(AttachmentRefusal::NotAllowed);
    }

    // The byte budget, consulted with the **declared** size and before a byte
    // is read, for the same reason the ceiling is: charging afterwards is
    // charging for something that already happened. ADR 0027 is clear that this
    // buys time and is not a limit; the limit is the ceiling below.
    let now = Instant::now();
    if !vault
        .throughput
        .lock()
        .await
        .permitir(pilot, header.declared_len, now)
    {
        tracing::info!(%pilot, bytes = header.declared_len, "transfer refused: rate limited");
        return refuse(AttachmentRefusal::RateLimited);
    }

    let scratch_name = format!(
        "{}-{}-{}",
        pilot.get(),
        key.get(),
        vault.scratches.fetch_add(1, Ordering::Relaxed)
    );

    // The ceiling. Everything above this line is cheap; everything below it
    // touches the disk.
    let reservation = {
        // Ledger first, CASPER second. Always.
        let mut ledger = vault.ledger.lock().await;
        let guard = dogma.casper.lock().await;
        match vault
            .store
            .reserve(&mut ledger, &guard, header.declared_len, &scratch_name)?
        {
            Ok(reservation) => reservation,
            Err(refusal) => {
                drop(guard);
                let reason = match refusal {
                    Refusal::TooLarge { limit, .. } => AttachmentRefusal::TooLarge { limit },
                    Refusal::NoRoom { .. } => AttachmentRefusal::NoRoom,
                };
                tracing::info!(%pilot, ?refusal, "transfer refused by the ceiling");
                drop(ledger);
                vault
                    .throughput
                    .lock()
                    .await
                    .devolver(pilot, header.declared_len, now);
                return refuse(reason);
            }
        }
    };

    // From here on the reservation is held, and every path out has to give it
    // back. There is one, and it is the `match` at the bottom.
    let landed = drain(stream, reservation.scratch(), header.declared_len).await;

    let digest = match landed {
        Ok(digest) => digest,
        Err(error) => {
            tracing::info!(%pilot, %error, "transfer fell before the last byte");
            let mut ledger = vault.ledger.lock().await;
            vault.store.abandon(&mut ledger, reservation);
            return refuse(AttachmentRefusal::SizeMismatch);
        }
    };

    // A declared size that lied dies here rather than at the door, which is the
    // trade ADR 0027 takes: the ceiling is enforced against the declaration, and
    // the declaration is enforced against the bytes.
    if digest != header.content_hash {
        tracing::info!(%pilot, "transfer refused: the hash did not close");
        let mut ledger = vault.ledger.lock().await;
        vault.store.abandon(&mut ledger, reservation);
        return refuse(AttachmentRefusal::HashDidNotMatch);
    }

    let content_hash = seele_proto::attachment::hex(&digest);
    let pending = PendingMessage {
        line: header.line,
        author: pilot,
        author_nickname: nickname.to_owned(),
        body: header.body.clone(),
        replies_to: header.replies_to,
        client_message_id: Some(key),
    };

    let published = {
        let mut ledger = vault.ledger.lock().await;
        let mut guard = dogma.casper.lock().await;

        // Written straight through rather than queued on `Dogma::post`, and
        // that is not an inconsistency: the batcher exists so that fifty typed
        // lines cost one `fsync`, and a transfer that has just spent seconds on
        // the wire is not that. What it needs and the queue cannot give is the
        // row identifier, now, to hang the attachment off.
        let mut stored = Messages::new(&mut guard)
            .append_batch(std::slice::from_ref(&pending))?
            .into_iter()
            .next()
            .context("the write batch answered with nothing")?;

        // A retry that had in fact succeeded. `append_batch` deduplicates by
        // `(author, client_message_id)`, so what came back is the original row
        // — and if it already carries a file, writing a second one would give
        // one message two attachments. This is the "recognised before twenty
        // megabytes go up again" of ADR 0027, arriving one transfer late
        // because there is no way to recognise it earlier without answering
        // "I already have that", which is the oracle the design refuses.
        let existing = attachments::Attachments::new(&guard).of_message(stored.id)?;
        if let Some(existing) = existing {
            tracing::info!(%pilot, message = %stored.id, "transfer was a retry of one already stored");
            stored.attachment = Some(existing.info());
            vault.store.abandon(&mut ledger, reservation);
            stored
        } else {
            let attachment = vault.store.keep(
                &mut ledger,
                &guard,
                reservation,
                &Landing {
                    content_hash: &content_hash,
                    message: stored.id,
                    file_name: &header.file_name,
                    declared_type: &header.declared_type,
                },
            )?;
            tracing::info!(
                %pilot,
                attachment = %attachment.id,
                bytes = attachment.byte_size,
                guardado = ledger.stored(),
                teto = ledger.quota(),
                "attachment stored"
            );
            stored.attachment = Some(attachment.info());
            stored
        }
    };

    // Committed, therefore durable, therefore safe to announce — the order
    // `crate::dogma` fixes for every message, and a transfer is no exception.
    let _ = dogma.events.send(Event::MessagePosted(published.clone()));
    Ok(Outcome::Published(Box::new(published)))
}

/// Moves the bytes off the stream and onto the disk, hashing on the way.
///
/// Cut at `declared` exactly. A sender that promises less than it sends finds
/// the stream refused at the boundary; one that promises more never closes its
/// hash. Either way nothing over the declared size is ever written, which is
/// what makes the reservation an honest reservation.
///
/// Generic over the reader so that the two guarantees above can be proved
/// without a network. They are byte arithmetic, and byte arithmetic tested
/// through a QUIC connection is byte arithmetic tested by something else.
async fn drain<R: tokio::io::AsyncRead + Unpin>(
    stream: &mut R,
    scratch: &std::path::Path,
    declared: u64,
) -> Result<[u8; CONTENT_HASH_LEN]> {
    let mut file = tokio::fs::File::create(scratch)
        .await
        .with_context(|| format!("could not open {}", scratch.display()))?;
    let mut digest = ContentDigest::new();
    let mut block = vec![0_u8; BLOCK_LEN];
    let mut written = 0_u64;

    loop {
        let room = declared.saturating_sub(written);
        if room == 0 {
            break;
        }
        let wanted = usize::try_from(room).unwrap_or(BLOCK_LEN).min(BLOCK_LEN);
        let room = block
            .get_mut(..wanted)
            .context("block is smaller than a block")?;
        let read = stream.read(room).await?;
        if read == 0 {
            break;
        }
        let piece = block.get(..read).context("short block")?;
        file.write_all(piece).await?;
        digest.feed(piece);
        written = written.saturating_add(read as u64);
    }
    file.flush().await?;
    drop(file);

    anyhow::ensure!(
        written == declared,
        "the transfer carried {written} bytes and declared {declared}"
    );
    // And nothing after the declared length. A sender with more to say is a
    // sender the reservation did not cover.
    anyhow::ensure!(
        stream
            .read(block.get_mut(..1).context("a block holds a byte")?)
            .await?
            == 0,
        "the transfer carried more than it declared"
    );
    Ok(digest.finish())
}

/// Hands a file back on a stream of its own.
///
/// # Errors
///
/// Returns the enumerated reason when the file is not coming; fails only when
/// the connection or the disk does.
pub async fn deliver(
    vault: &Vault,
    dogma: &Dogma,
    connection: &quinn::Connection,
    id: AttachmentId,
) -> Result<std::result::Result<u64, AttachmentRefusal>> {
    let row = {
        let guard = dogma.casper.lock().await;
        attachments::Attachments::new(&guard).one(id)?
    };
    let Some(row) = row else {
        return Ok(Err(AttachmentRefusal::NotFound));
    };
    if row.expired_at.is_some() {
        return Ok(Err(AttachmentRefusal::Expired));
    }

    let path = vault.store.blob_path(&row.content_hash);
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(_) => {
            // The row is the truth, and the row now says the bytes are gone.
            // Stamping it here rather than leaving the disagreement means the
            // next reader is told the same thing this one was.
            let guard = dogma.casper.lock().await;
            let _ = attachments::Attachments::new(&guard).expire_blob(&row.content_hash);
            tracing::warn!(attachment = %id, "the bytes are missing; the row now reads as expired");
            return Ok(Err(AttachmentRefusal::Expired));
        }
    };

    let mut stream = connection.open_uni().await?;
    // Below control, so that fetching a picture cannot delay a `Pong`.
    stream.set_priority(TRANSFER_PRIORITY)?;
    // Que tipo de fluxo é este, antes do cabeçalho. A regra vale nos dois
    // sentidos: quem recebe tem um `accept_uni` só e mais de um uso para ele, e
    // adivinhar pelo conteúdo é o que o §5.2 chama de dívida.
    stream
        .write_all(&[seele_proto::stream::StreamType::Attachment.byte()])
        .await?;
    crate::frame::write(
        &mut stream,
        &AttachmentDelivery {
            attachment: row.id,
            byte_size: row.byte_size,
            content_hash: from_hex(&row.content_hash),
        },
    )
    .await?;

    let mut block = vec![0_u8; BLOCK_LEN];
    let mut sent = 0_u64;
    loop {
        let read = file.read(&mut block).await?;
        if read == 0 {
            break;
        }
        let piece = block.get(..read).context("short block")?;
        stream.write_all(piece).await?;
        sent = sent.saturating_add(read as u64);
    }
    stream.finish()?;
    Ok(Ok(sent))
}

/// Reads a stored hex digest back into bytes.
///
/// Anything that is not 64 hex characters comes back as zeros, which cannot
/// match any real content and therefore fails closed: a receiver checking the
/// digest refuses the file rather than trusting a row nobody can parse.
fn from_hex(hex: &str) -> [u8; CONTENT_HASH_LEN] {
    let mut out = [0_u8; CONTENT_HASH_LEN];
    if hex.len() != CONTENT_HASH_LEN * 2 {
        return out;
    }
    for (slot, pair) in out.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
        let Ok(text) = std::str::from_utf8(pair) else {
            return [0_u8; CONTENT_HASH_LEN];
        };
        let Ok(byte) = u8::from_str_radix(text, 16) else {
            return [0_u8; CONTENT_HASH_LEN];
        };
        *slot = byte;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_proto::attachment::hash;

    #[test]
    fn a_digest_survives_the_round_trip_through_its_written_form() {
        // The hex spelling is the blob's file name, so it is the only place a
        // digest is stored. A delivery that could not read it back would hand
        // out files nobody could verify.
        let digest = hash(b"padrao azul confirmado");
        let written = seele_proto::attachment::hex(&digest);
        assert_eq!(from_hex(&written), digest);
    }

    #[test]
    fn an_unreadable_digest_fails_closed() {
        // Zeros cannot match any real content, so a receiver refuses the file.
        // The alternative — guessing — would hand over bytes under a digest
        // that says nothing.
        for broken in ["", "nao é hexadecimal", &"zz".repeat(32)] {
            assert_eq!(from_hex(broken), [0_u8; CONTENT_HASH_LEN]);
        }
    }

    /// Runs `drain` over bytes held in memory, the way a hostile sender would
    /// hand them over.
    async fn through(bytes: &[u8], declared: u64) -> (Result<[u8; CONTENT_HASH_LEN]>, Vec<u8>) {
        let directory = tempfile::tempdir().expect("tempdir");
        let scratch = directory.path().join("parcial");
        let mut reader = std::io::Cursor::new(bytes.to_vec());
        let outcome = drain(&mut reader, &scratch, declared).await;
        let landed = std::fs::read(&scratch).unwrap_or_default();
        (outcome, landed)
    }

    #[tokio::test]
    async fn an_honest_transfer_hashes_to_what_arrived() {
        let bytes = vec![7_u8; 5_000];
        let (digest, landed) = through(&bytes, 5_000).await;
        assert_eq!(digest.expect("drained"), hash(&bytes));
        assert_eq!(landed, bytes);
    }

    #[tokio::test]
    async fn a_sender_that_declared_more_than_it_sent_is_refused() {
        // The reservation was made against the declaration, so a short stream
        // has already cost the ceiling more than it used. Refusing is what
        // stops the difference being published as a message.
        let (digest, _) = through(&[7_u8; 100], 5_000).await;
        assert!(digest.is_err(), "a short transfer was accepted");
    }

    #[tokio::test]
    async fn a_sender_that_declared_less_than_it_sent_is_cut_and_refused() {
        // The half that matters for the ceiling: nothing over the declared
        // length is ever written, so a lying declaration cannot make the disk
        // pass the ceiling. It is refused as well, but it was already harmless.
        let (digest, landed) = through(&[7_u8; 5_000], 100).await;
        assert_eq!(
            landed.len(),
            100,
            "bytes past the declared length reached the disk: the reservation \
             did not cover them and the ceiling is no longer a ceiling"
        );
        assert!(digest.is_err(), "a transfer that overflowed was accepted");
    }

    #[tokio::test]
    async fn a_declared_size_that_lies_never_closes_its_hash() {
        // ADR 0027: "tamanho declarado que mente morre no fim: o fluxo é
        // cortado no tamanho declarado e o hash não fecha". The cut is above;
        // this is the second half, and it is why the cut is safe to make.
        let honest = vec![3_u8; 900];
        let (digest, _) = through(&honest, 900).await;
        let mut lying = honest.clone();
        lying.extend_from_slice(&[9_u8; 100]);
        let (cut, _) = through(&lying, 1_000).await;
        assert_ne!(
            digest.expect("honest"),
            cut.expect("cut"),
            "two different files hashed the same"
        );
    }

    #[test]
    fn control_outranks_every_transfer() {
        // Stated as a comparison rather than as two numbers, because the two
        // numbers are meaningless apart and this is the property. A `const`
        // assertion, so that swapping them does not compile rather than does
        // not pass.
        const _: () = assert!(CONTROL_PRIORITY > TRANSFER_PRIORITY);
    }
}
