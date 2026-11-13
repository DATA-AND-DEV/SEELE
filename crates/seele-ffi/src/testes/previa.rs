//! What a window is handed for one attachment. ADR 0027.
//!
//! This is the last place the bytes and the claim are both in scope, so it
//! is the last place the wrong one could be picked. The window past this
//! point gets a string and no choice.

use super::*;

/// A real picture, 1 × 1, and not a signature alone: a preview's
/// dimensions are read from its header before it is drawn.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0dIDAT\x78\xda\x63\xd0\x08\xa8\xf8\x0f\x00\x03\x84\x01\xf0\xb6\x34\x04\x4b\x00\x00\x00\x00IEND\xae\x42\x60\x82";
const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];

/// A PNG of 69 bytes whose header declares 30 000 × 30 000.
const BOMBA: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x75\x30\x00\x00\x75\x30\x08\x06\x00\x00\x00\x66\x27\xf8\xba\x00\x00\x00\x0cIDAT\x78\xda\x63\x60\xa0\x0c\x00\x00\x00\x40\x00\x01\x89\xc9\xaf\x43\x00\x00\x00\x00IEND\xae\x42\x60\x82";

/// **A file within the byte limit that declares too many pixels is not
/// drawn, and says so with the numbers** (the sweep after the 4K cover,
/// 11/11/2026). It is 69 bytes, it agrees with its claim, and opened it
/// would ask the window for 3.6 GB: it used to come back as a picture.
#[test]
fn a_picture_that_declares_too_many_pixels_is_refused_with_the_numbers() {
    let previa = preview_of(
        7,
        "image/png",
        Some(seele_core::enlace::Previa::Bytes(BOMBA.to_vec())),
    );
    assert_eq!(previa.image, None, "900 megapixels went to the window");
    assert_eq!(
        previa.refusal,
        Some(PreviewRefusal::TooManyPixels {
            width: 30_000,
            height: 30_000,
            limit: seele_core::reducao::TETO_DE_PIXELS,
        }),
        "«too many pixels» with no number sends somebody nowhere"
    );
    assert_eq!(previa.found.as_deref(), Some("image/png"));
}

/// Bytes that start as a picture and carry no readable header are not
/// handed to the window to find out: with no dimensions there is no bound.
#[test]
fn a_picture_whose_header_cannot_be_read_is_not_drawn() {
    let so_a_assinatura = PNG.get(..12).unwrap_or_default().to_vec();
    let previa = preview_of(
        7,
        "image/png",
        Some(seele_core::enlace::Previa::Bytes(so_a_assinatura)),
    );
    assert_eq!(previa.image, None);
    assert_eq!(previa.refusal, Some(PreviewRefusal::Unreadable));
}

#[test]
fn bytes_that_agree_with_the_claim_come_back_as_a_picture() {
    // The claim is **shouted**, and the URI comes out lower case. A media
    // type is case-insensitive, so this file agrees with itself and is
    // drawn — and the spelling of the string a window receives is proof of
    // which side of the agreement wrote it. With `image/png` on both sides
    // the two are indistinguishable, and a URI spliced together from the
    // claim would pass unnoticed.
    let previa = preview_of(
        7,
        "IMAGE/PNG",
        Some(seele_core::enlace::Previa::Bytes(PNG.to_vec())),
    );
    assert_eq!(previa.attachment, 7);
    assert_eq!(previa.refusal, None);
    assert_eq!(previa.found.as_deref(), Some("image/png"));
    assert_eq!(
        previa.image,
        Some(seele_core::preview::data_uri(
            seele_core::ImageFormat::Png,
            PNG
        )),
        "the URI a window draws from was not written from the bytes"
    );
}

#[test]
fn bytes_that_disagree_with_the_claim_come_back_with_no_picture_at_all() {
    // The one that matters. A JPEG that called itself a PNG is refused, and
    // the refusal carries both halves so a sentence can name them — but
    // `image` is `None`, which is what a window acts on.
    let previa = preview_of(
        7,
        "image/png",
        Some(seele_core::enlace::Previa::Bytes(JPEG.to_vec())),
    );
    assert_eq!(previa.image, None, "a lying file was drawn anyway");
    assert_eq!(previa.refusal, Some(PreviewRefusal::Disagrees));
    assert_eq!(previa.claimed, "image/png");
    assert_eq!(previa.found.as_deref(), Some("image/jpeg"));
}

#[test]
fn a_claim_never_reaches_the_string_a_window_draws_from() {
    // Whatever the sender wrote, it is quoted into `claimed` and nowhere
    // else. Here the claim is a whole `data:` URI of somebody's choosing,
    // and the only thing it produces is a refusal.
    let mentira = "data:image/png;base64,AAAA";
    let previa = preview_of(
        7,
        mentira,
        Some(seele_core::enlace::Previa::Bytes(PNG.to_vec())),
    );
    assert_eq!(previa.image, None);
    assert_eq!(previa.refusal, Some(PreviewRefusal::NotAPicture));
    assert_eq!(previa.claimed, mentira);
}

#[test]
fn a_file_over_the_limit_says_so_and_says_what_the_limit_is() {
    let previa = preview_of(
        7,
        "image/png",
        Some(seele_core::enlace::Previa::GrandeDemais { tamanho: 99 }),
    );
    assert_eq!(previa.image, None);
    assert_eq!(
        previa.refusal,
        Some(PreviewRefusal::TooBig {
            limit: seele_core::PREVIEW_LIMIT
        }),
        "«too big» with no number sends somebody nowhere"
    );
}

#[test]
fn a_session_that_ended_mid_fetch_is_not_a_picture_and_not_a_panic() {
    // The dropped channel. It reads as "the bytes did not come", which is
    // true, and not as any of the three that say something about the file.
    let previa = preview_of(7, "image/png", None);
    assert_eq!(previa.image, None);
    assert_eq!(previa.refusal, Some(PreviewRefusal::DidNotArrive));
}
