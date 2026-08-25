//! What travels on an attachment's own stream.
//!
//! ADR 0027: **one unidirectional QUIC stream per transfer, and never the
//! control stream.** The control stream is ordered, and an ordered stream blocks
//! itself: twenty megabytes written into it stop every presence event, every
//! channel of text and every `Pong` from everybody behind them until the last byte
//! goes through. It is the one stream that may not queue.
//!
//! # There is no chunk framing here, on purpose
//!
//! A QUIC stream is already the boundary. Inventing a second framing on top of
//! a transport that already delivers in order is code with no buyer, so a
//! transfer is a header and then bytes to the end of the stream. The chunking
//! that does exist is on **disk**: whoever receives writes in fixed blocks and
//! never holds the whole file in memory — `specs/04-servidor-seele.md` sizes a
//! server at 1 vCPU and 512 MB, and a `Vec` of 20 MB per simultaneous transfer
//! ends that.
//!
//! # The answer comes back on the control stream
//!
//! Not on this one. `specs/02-protocolo.md` requires every reason to be an
//! enum, and the enums live on control — see `AttachmentRefusal`.
//!
//! # There is no resumption
//!
//! A transfer that falls starts again from zero. QUIC offers no continuation of
//! a stream across connections, and offset-and-resume is a second design
//! entire. At the sizes the per-file limit allows it is an irritation rather
//! than a tragedy — but it is a real irritation on a bad link, so it is written
//! here rather than discovered. The retry is safe and free: the transfer is
//! keyed by the same `client_message_id` that already makes a send idempotent.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::control::{
    check_bounds, ControlError, Validate, MAX_BODY_LEN, MAX_DECLARED_TYPE_LEN, MAX_FILE_NAME_LEN,
};
use crate::ids::{AttachmentId, ChannelId, ClientMessageId, MessageId};

/// Length of a SHA-256 digest, in bytes.
pub const CONTENT_HASH_LEN: usize = 32;

/// How many bytes are moved between the network and the disk at a time.
///
/// The number exists so that no transfer ever holds its file in memory. Sixty
/// four kibibytes is large enough that the syscall count is irrelevant beside
/// the network, and small enough that fifty simultaneous transfers cost three
/// megabytes of buffer in total.
pub const BLOCK_LEN: usize = 64 * 1024;

/// What a sender writes before the bytes.
///
/// Everything needed to publish the message is here, and the message is
/// **only** published once the bytes have arrived whole. That is why the body
/// travels with the file rather than on a separate `SendMessage`: two channels
/// carrying halves of one message would have an order to get wrong, and the
/// cost of getting it wrong is a message on the Channel pointing at a file that
/// never landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentHeader {
    /// Which Channel the message belongs to.
    pub channel: ChannelId,
    /// The idempotency key of the message this file belongs to.
    ///
    /// The same key that already makes a send idempotent (gap G9). A retry
    /// after a fall that had in fact succeeded on the server is recognised
    /// before twenty megabytes go up again.
    pub client_message_id: ClientMessageId,
    /// What the sender wrote beside the file. May be empty.
    pub body: String,
    /// What it replies to.
    pub replies_to: Option<MessageId>,
    /// The name the sender gave the file.
    ///
    /// A **gift, not an address**. It is stored in a column and never reaches
    /// the filesystem: the blob is named after the SHA-256 of its own content,
    /// so no `../`, no `CON`, no null byte and no two capitals colliding on
    /// macOS ever touch a path.
    pub file_name: String,
    /// The type the sender claims the bytes are.
    ///
    /// A **claim**. Registered and treated as one: only a short list of image
    /// types is ever drawn inline, and only when the bytes agree with the
    /// claim. This is not about trusting the file; it is about which decoder
    /// the bytes go to.
    pub declared_type: String,
    /// How many bytes follow.
    ///
    /// Checked against the ceiling **before** the first byte is read, which is
    /// the whole of ADR 0027. A declared size that lies dies at the end
    /// instead: the stream is cut at this length and the hash does not close.
    pub declared_len: u64,
    /// SHA-256 the sender says the bytes hash to.
    ///
    /// The one question a server can answer about a file — did it arrive whole.
    /// It says nothing about the file being good, and no sentence of this
    /// product will pretend it does.
    pub content_hash: [u8; CONTENT_HASH_LEN],
}

/// What a server writes before handing a file back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentDelivery {
    /// Which attachment this is.
    pub attachment: AttachmentId,
    /// How many bytes follow.
    pub byte_size: u64,
    /// What they hash to, so the receiver can ask the same question the server
    /// asked on the way in.
    pub content_hash: [u8; CONTENT_HASH_LEN],
}

impl Validate for AttachmentHeader {
    fn validate(&self) -> Result<(), ControlError> {
        check_bounds("body", self.body.len(), MAX_BODY_LEN)?;
        check_bounds("file_name", self.file_name.len(), MAX_FILE_NAME_LEN)?;
        check_bounds(
            "declared_type",
            self.declared_type.len(),
            MAX_DECLARED_TYPE_LEN,
        )?;
        // A name that is empty, or only spaces, draws a row with nothing in it:
        // the same failure `check_name` refuses for rooms, and here the reader
        // would be looking at a file they cannot refer to out loud.
        if self.file_name.trim().is_empty() || self.file_name.contains('\0') {
            return Err(ControlError::FieldOutOfRange { field: "file_name" });
        }
        // Zero bytes is not a file. Refused here rather than stored, because
        // every screen downstream would have to decide what an empty attachment
        // looks like, and none of them has a good answer.
        if self.declared_len == 0 {
            return Err(ControlError::FieldOutOfRange {
                field: "declared_len",
            });
        }
        Ok(())
    }
}

impl Validate for AttachmentDelivery {
    fn validate(&self) -> Result<(), ControlError> {
        Ok(())
    }
}

/// A SHA-256 taken block by block.
///
/// Here rather than in the crates that use it so that neither the daemon nor
/// the client gains a dependency for it: `sha2` is already in this crate's tree
/// — it computes the certificate fingerprint — and ADR 0027 adds nothing new.
/// `blake3` would be faster and does not enter: the gain does not pay for a
/// crate in a tree that ADR 0026 has just finished counting one by one.
#[derive(Debug, Default)]
pub struct ContentDigest {
    hasher: Sha256,
    fed: u64,
}

impl ContentDigest {
    /// A digest that has seen nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one block.
    pub fn feed(&mut self, block: &[u8]) {
        self.hasher.update(block);
        self.fed = self.fed.saturating_add(block.len() as u64);
    }

    /// How many bytes have gone in.
    #[must_use]
    pub fn fed(&self) -> u64 {
        self.fed
    }

    /// Closes the digest.
    #[must_use]
    pub fn finish(self) -> [u8; CONTENT_HASH_LEN] {
        self.hasher.finalize().into()
    }
}

/// Renders a digest as lowercase hexadecimal.
///
/// The blob's name on disk, and the only form of it that is ever written down.
#[must_use]
pub fn hex(digest: &[u8; CONTENT_HASH_LEN]) -> String {
    use std::fmt::Write as _;
    digest.iter().fold(String::with_capacity(64), |mut out, b| {
        // The write cannot fail into a `String`; the result is discarded rather
        // than unwrapped, because this crate forbids `unwrap`.
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Hashes a whole slice at once.
///
/// For a sender that already holds the file in memory, and for tests. The
/// receiving side never uses it: it never holds the file at all.
#[must_use]
pub fn hash(bytes: &[u8]) -> [u8; CONTENT_HASH_LEN] {
    let mut digest = ContentDigest::new();
    digest.feed(bytes);
    digest.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{decode, encode};

    fn header() -> AttachmentHeader {
        AttachmentHeader {
            channel: ChannelId(1),
            client_message_id: ClientMessageId(0xFEED),
            body: "olha isto".into(),
            replies_to: None,
            file_name: "harmônicos.png".into(),
            declared_type: "image/png".into(),
            declared_len: 4096,
            content_hash: hash("padrão azul".as_bytes()),
        }
    }

    #[test]
    fn a_header_round_trips() {
        let frame = encode(&header()).unwrap();
        assert_eq!(decode::<AttachmentHeader>(&frame).unwrap(), header());
    }

    #[test]
    fn a_delivery_round_trips() {
        let delivery = AttachmentDelivery {
            attachment: AttachmentId(7),
            byte_size: 4096,
            content_hash: hash("padrão azul".as_bytes()),
        };
        let frame = encode(&delivery).unwrap();
        assert_eq!(decode::<AttachmentDelivery>(&frame).unwrap(), delivery);
    }

    #[test]
    fn a_header_carries_the_version_byte_like_every_other_frame() {
        // The header goes through the same `encode`, so a build one protocol
        // version older refuses the transfer rather than reading a header it
        // does not understand as one it does.
        let frame = encode(&header()).unwrap();
        assert_eq!(frame.first(), Some(&crate::version::PROTOCOL_VERSION));
    }

    #[test]
    fn a_file_with_no_name_is_refused_in_both_directions() {
        for blank in ["", "   ", "\t\n"] {
            let ask = AttachmentHeader {
                file_name: blank.into(),
                ..header()
            };
            assert!(
                matches!(
                    encode(&ask),
                    Err(ControlError::FieldOutOfRange { field: "file_name" })
                ),
                "accepted a file named {blank:?}"
            );
        }

        // And built by hand, the way a hostile peer would.
        let ask = AttachmentHeader {
            file_name: " ".into(),
            ..header()
        };
        let mut frame = vec![crate::version::PROTOCOL_VERSION];
        frame = postcard::to_extend(&ask, frame).unwrap();
        assert!(matches!(
            decode::<AttachmentHeader>(&frame),
            Err(ControlError::FieldOutOfRange { field: "file_name" })
        ));
    }

    #[test]
    fn a_name_with_a_null_byte_is_refused() {
        let ask = AttachmentHeader {
            file_name: "foto\0.png".into(),
            ..header()
        };
        assert!(matches!(
            encode(&ask),
            Err(ControlError::FieldOutOfRange { field: "file_name" })
        ));
    }

    #[test]
    fn a_path_in_the_name_is_carried_rather_than_refused() {
        // Deliberate, and it is the difference between this design and a
        // sanitiser. The name never reaches the filesystem — the blob is named
        // after its own hash — so `../../etc/passwd` is a strange label and
        // nothing more. Refusing it would suggest the name were an address.
        let ask = AttachmentHeader {
            file_name: "../../etc/passwd".into(),
            ..header()
        };
        let frame = encode(&ask).unwrap();
        assert_eq!(
            decode::<AttachmentHeader>(&frame).unwrap().file_name,
            "../../etc/passwd"
        );
    }

    #[test]
    fn an_empty_file_is_refused() {
        let ask = AttachmentHeader {
            declared_len: 0,
            ..header()
        };
        assert!(matches!(
            encode(&ask),
            Err(ControlError::FieldOutOfRange {
                field: "declared_len"
            })
        ));
    }

    #[test]
    fn an_oversized_body_is_refused_on_the_attachment_path_too() {
        // The 4 KiB of `MAX_BODY_LEN` is right and stays right; ADR 0027 undoes
        // "no attachments" and not the body limit. Without this check the
        // transfer stream would be a way around it.
        let ask = AttachmentHeader {
            body: "x".repeat(MAX_BODY_LEN + 1),
            ..header()
        };
        assert!(matches!(
            encode(&ask),
            Err(ControlError::FieldTooLong { field: "body", .. })
        ));
    }

    #[test]
    fn the_digest_taken_block_by_block_equals_the_one_taken_at_once() {
        // The property the receiving side depends on: it never holds the file,
        // so it can only hash it in pieces.
        let bytes: Vec<u8> = (0..10_000_u32).map(|n| (n % 251) as u8).collect();
        let mut digest = ContentDigest::new();
        for block in bytes.chunks(BLOCK_LEN / 7) {
            digest.feed(block);
        }
        assert_eq!(digest.fed(), bytes.len() as u64);
        assert_eq!(digest.finish(), hash(&bytes));
    }

    #[test]
    fn hex_is_lowercase_and_sixty_four_characters() {
        // It is a file name, so its shape is load-bearing: two spellings of the
        // same digest would be two blobs of the same bytes.
        let rendered = hex(&hash(b""));
        assert_eq!(rendered.len(), 64);
        assert_eq!(
            rendered, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "the empty SHA-256 is a known constant; this is the spelling check"
        );
    }
}
