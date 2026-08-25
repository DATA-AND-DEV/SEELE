//! The served copy of the design tokens must be the frozen ones.
//!
//! ADR 0019 chose no bundler, so `ui/tokens.css` is a copy of
//! `design/seele-tokens.css` rather than something a build step produces. A copy
//! with nobody watching it is a copy that drifts — and the drift would be
//! silent, because both files are valid CSS and the app would simply be a
//! slightly different colour than the terminal client.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `apps/seele-app`.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Every stylesheet the window loads except the two that declare rather than
/// paint, concatenated.
///
/// It is a directory listing and not a list of names on purpose. The stylesheet
/// is six files now, one per screen plus a shared layer, and four more screens
/// are on the way: a rule against colour literals that has to be told about each
/// new file is a rule that stops holding on the first file somebody forgets.
/// `tokens.css` is the one place a literal belongs, and `fontes.css` declares
/// faces and paints nothing.
fn stylesheets() -> String {
    let ui = repo_root().join("apps/seele-app/ui");
    let entries = std::fs::read_dir(&ui).expect("apps/seele-app/ui must exist");
    let mut sheets: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "css"))
        .filter(|path| {
            !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("tokens.css") | Some("fontes.css")
            )
        })
        .collect();
    sheets.sort();
    assert!(!sheets.is_empty(), "ui/ ships no stylesheet at all");
    sheets
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("the stylesheet must exist"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_served_tokens_are_the_frozen_tokens() {
    let root = repo_root();
    let frozen = std::fs::read_to_string(root.join("design/seele-tokens.css"))
        .expect("design/seele-tokens.css is the source of truth and must exist");
    let served = std::fs::read_to_string(root.join("apps/seele-app/ui/tokens.css"))
        .expect("apps/seele-app/ui/tokens.css is what the window loads");

    assert_eq!(
        served, frozen,
        "the desktop client is serving different design tokens than the frozen ones.\n\
         Copy design/seele-tokens.css over apps/seele-app/ui/tokens.css — and if the \
         change was deliberate, ADR 0014 is the one to update first."
    );
}

#[test]
fn the_stylesheet_uses_no_colour_the_tokens_do_not_define() {
    // specs/07-tema-evangelion.md fixes the palette, and ADR 0014 makes v2
    // canonical. A hex literal in the stylesheet is a colour that exists in one
    // of the two clients and not the other.
    let css = stylesheets();

    let literals: Vec<&str> = css
        .lines()
        .filter(|channel| channel.contains('#') && !channel.trim_start().starts_with("/*"))
        .filter(|channel| {
            channel
                .split('#')
                .skip(1)
                .any(|rest| rest.chars().take(3).all(|c| c.is_ascii_hexdigit()))
        })
        .collect();

    assert!(
        literals.is_empty(),
        "the stylesheet names colours the tokens do not define:\n{}",
        literals.join("\n")
    );
}

/// The hex of one `--seele-*` custom property, as sRGB bytes.
fn token(css: &str, name: &str) -> [f64; 3] {
    let Some(after) = css.split(&format!("--seele-{name}:")).nth(1) else {
        panic!("tokens.css no longer defines --seele-{name}");
    };
    let Some(hex) = after.split('#').nth(1) else {
        panic!("--seele-{name} is not a hex literal");
    };
    let hex: String = hex.chars().take(6).collect();
    let Ok(value) = u32::from_str_radix(&hex, 16) else {
        panic!("--seele-{name} is not six hex digits: {hex}");
    };
    [
        f64::from((value >> 16) & 0xFF),
        f64::from((value >> 8) & 0xFF),
        f64::from(value & 0xFF),
    ]
}

/// WCAG 2.1 relative luminance from sRGB bytes.
fn luminance(rgb: [f64; 3]) -> f64 {
    let channel = |byte: f64| {
        let c = byte / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `over` painted on `under` at `alpha`, composited in sRGB as the browser does.
fn composite(over: [f64; 3], under: [f64; 3], alpha: f64) -> [f64; 3] {
    let mix = |o: f64, u: f64| alpha * o + (1.0 - alpha) * u;
    [
        mix(over[0], under[0]),
        mix(over[1], under[1]),
        mix(over[2], under[2]),
    ]
}

/// The `opacity` declared inside a rule, as a fraction.
fn opacity_of(css: &str, selector: &str) -> f64 {
    let Some(rule) = css
        .split(&format!("\n{selector} {{"))
        .nth(1)
        .and_then(|rest| rest.split('}').next())
    else {
        panic!("no stylesheet in ui/ has a rule for `{selector}`");
    };
    let Some(value) = rule.split("opacity:").nth(1).and_then(|rest| {
        rest.split(';')
            .next()
            .and_then(|value| value.trim().parse::<f64>().ok())
    }) else {
        panic!("`{selector}` declares no readable opacity:\n{rule}");
    };
    value
}

#[test]
fn the_scanline_does_not_take_a_token_below_the_contrast_it_already_had() {
    // The scanline is the first thing in this interface painted *over* the
    // text: `inset: 0`, `z-index: 9`. On its dark rows the veil dims the glyph
    // and the surface together, and contrast is a ratio — dimming both does not
    // preserve it, it collapses it.
    //
    // At the comp's 34% that collapse was severe and nothing measured it:
    // `vermelho-alerta` fell from 5,16:1 to 2,70:1, which is ADR 0014's
    // strongest argument for the v2 palette being quietly undone by a texture.
    // This is that arithmetic, run against whatever opacity the sheet declares,
    // so lifting the veil again has to be a decision rather than an edit.
    //
    // Measured over `negro-painel`: the lighter of the two surfaces, and so the
    // worse case for light text.
    let root = repo_root();
    let tokens = std::fs::read_to_string(root.join("apps/seele-app/ui/tokens.css"))
        .expect("the tokens the window loads must exist");
    let css = stylesheets();

    let veil = opacity_of(&css, ".varredura");
    let black = token(&tokens, "negro-absoluto");
    let surface = composite(black, token(&tokens, "negro-painel"), veil);

    // `osso-apagado` was already below AA for small text before the scanline
    // existed — `docs/tokens-achados.md` records it as large-text-only and books
    // the fix as M4 work. What it may not do is lose *that* footing too.
    //
    // The other two passed AA unveiled and must still pass. The floors are the
    // criteria each token already met, not one invented here: a decorative
    // texture is not allowed to reclassify a colour.
    for (name, floor) in [
        ("osso-apagado", 3.0),
        ("vermelho-alerta", 4.5),
        ("laranja-nerv", 4.5),
    ] {
        let lit = contrast(token(&tokens, name), token(&tokens, "negro-painel"));
        let veiled = composite(black, token(&tokens, name), veil);
        let veiled = contrast(veiled, surface);

        assert!(
            veiled >= floor,
            "under the scanline at {:.0}% opacity, `{name}` falls to {veiled:.2}:1 \
             (it is {lit:.2}:1 on the lit rows), below the {floor}:1 it already met. \
             Either bring `.varredura`'s opacity down or change the token — but the \
             texture does not get to decide this in silence.",
            veil * 100.0
        );
    }
}
