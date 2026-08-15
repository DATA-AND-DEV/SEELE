//! The three faces cannot go missing without somebody being told.
//!
//! This is a guard written against a defect that was live, not an imagined one.
//! `ui/tokens.css` has named `Saira Condensed`, `IBM Plex Mono` and
//! `Noto Sans JP` since M0.12, and the sheets in `ui/` have been asking for all
//! three ever since — while the app shipped none of them. Every rule fell through
//! to the second entry of its stack: Arial Narrow where the display face belonged,
//! the system monospace where the data face belonged, Hiragino or Yu Gothic
//! where the brand's Japanese belonged.
//!
//! Nothing failed. The window opened, the layout was right, and the typography
//! was somebody else's. There is no build step (ADR 0019) and no runtime error:
//! a `@font-face` that points at a file nobody shipped is a request the CSP
//! denies in silence, and a family nobody declares is a fallback nobody sees.
//!
//! So the two halves are checked from opposite ends. Every face the tokens name
//! must be served by a rule, and every file a rule serves must be on disk.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    match std::fs::read_to_string(app_dir().join(relative)) {
        Ok(text) => text,
        Err(error) => panic!("{relative}: {error}"),
    }
}

/// The first family of a `--seele-…` font stack, unquoted.
///
/// The first is the only one that matters here. Everything after it is a
/// fallback the machine happens to have, and naming a fallback is not a promise
/// to ship anything — naming the head of the stack is.
fn first_family(tokens: &str, variable: &str) -> String {
    let Some(after) = tokens.split(&format!("--{variable}:")).nth(1) else {
        panic!("tokens.css no longer declares `--{variable}`");
    };
    let Some(declaration) = after.split(';').next() else {
        panic!("`--{variable}` is not terminated in tokens.css");
    };
    let Some(quoted) = declaration.split('"').nth(1) else {
        panic!("`--{variable}` does not start with a quoted family: {declaration}");
    };
    quoted.to_owned()
}

/// Every `font-family: "…"` declared by a `@font-face` block.
fn served_families(sheet: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for block in sheet.split("@font-face").skip(1) {
        let Some(body) = block.split('}').next() else {
            continue;
        };
        let Some(after) = body.split("font-family:").nth(1) else {
            panic!("a @font-face block declares no font-family:\n{body}");
        };
        let Some(name) = after.split('"').nth(1) else {
            panic!("a @font-face names an unquoted family:\n{body}");
        };
        found.insert(name.to_owned());
    }
    found
}

/// Every path a `src: url("…")` points at, in source order.
fn served_files(sheet: &str) -> Vec<String> {
    let mut found = Vec::new();
    for piece in sheet.split("url(\"").skip(1) {
        if let Some(path) = piece.split('"').next() {
            found.push(path.to_owned());
        }
    }
    found
}

#[test]
fn every_face_the_tokens_name_is_actually_served() {
    // The defect this file exists for, in one assertion. `tokens.css` is a
    // byte-for-byte copy of the frozen design tokens (`tests/tokens.rs`), so
    // the list of faces is not something this app gets to shorten: if the
    // design names three, the app ships three or it is lying about its
    // typography.
    let tokens = read("ui/tokens.css");
    let sheet = read("ui/fontes.css");
    let served = served_families(&sheet);

    assert!(
        !served.is_empty(),
        "ui/fontes.css declares no @font-face at all"
    );

    for variable in ["seele-display", "seele-mono", "seele-jp"] {
        let family = first_family(&tokens, variable);
        assert!(
            served.contains(&family),
            "`--{variable}` asks for \"{family}\" first and no @font-face serves it, \
             so every rule using it falls through to the next entry of the stack — \
             silently, and on every machine.\n\
             served: {served:?}"
        );
    }
}

#[test]
fn every_file_a_font_face_points_at_is_in_the_tree() {
    // The other direction, and the one a rename breaks. The CSP is
    // `default-src 'self'`, so a `src:` that misses its file produces no error
    // anybody sees: the browser simply moves on to the fallback.
    let sheet = read("ui/fontes.css");
    let files = served_files(&sheet);

    assert!(
        !files.is_empty(),
        "no @font-face in ui/fontes.css loads a file"
    );

    let missing: Vec<&String> = files
        .iter()
        .filter(|path| !app_dir().join("ui").join(path).is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "ui/fontes.css serves files that are not in ui/: {missing:?}"
    );

    // And nothing off the machine, for the same reason `tests/frontend.rs`
    // checks the page: a Google Fonts URL is what the comp does, and it is
    // exactly what the CSP would refuse.
    assert!(
        !sheet.contains("http://") && !sheet.contains("https://"),
        "ui/fontes.css loads a face from off the machine, which the CSP blocks"
    );
}

#[test]
fn no_shipped_face_file_is_left_unserved() {
    // Dead weight in the bundle, and the way a rename half-lands: the new file
    // is added, the rule still points at the old one, and both sit in the tree.
    let served: BTreeSet<String> = served_files(&read("ui/fontes.css"))
        .into_iter()
        .filter_map(|path| path.strip_prefix("fontes/").map(str::to_owned))
        .collect();

    let directory = app_dir().join("ui").join("fontes");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        panic!("ui/fontes/ must exist: it is where the faces live");
    };

    let mut orphans = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".woff2") && !served.contains(&name) {
            orphans.push(name);
        }
    }
    orphans.sort();
    assert!(
        orphans.is_empty(),
        "ui/fontes/ ships faces no @font-face serves: {orphans:?}"
    );
}

#[test]
fn every_face_carries_the_licence_it_was_given_under() {
    // All three are SIL OFL 1.1, and the OFL is a licence with a redistribution
    // condition: the notice travels with the software. A font is not an asset
    // that arrives without paperwork just because it is binary — ADR 0019
    // refused npm over precisely that reflex.
    let directory = app_dir().join("ui").join("fontes");

    for face in ["saira-condensed", "ibm-plex-mono", "noto-sans-jp"] {
        let licence = directory.join(format!("LICENCA-{face}.txt"));
        let Ok(text) = std::fs::read_to_string(&licence) else {
            panic!("{face} ships with no licence beside it: {licence:?}");
        };
        assert!(
            text.contains("SIL OPEN FONT LICENSE Version 1.1"),
            "the licence shipped for {face} is not the OFL 1.1 it was read as; \
             re-read it and update fontes/PROCEDENCIA.md before shipping"
        );
    }

    let provenance = std::fs::read_to_string(directory.join("PROCEDENCIA.md"))
        .unwrap_or_else(|error| panic!("fontes/PROCEDENCIA.md: {error}"));
    for face in ["Saira Condensed", "IBM Plex Mono", "Noto Sans JP"] {
        assert!(
            provenance.contains(face),
            "fontes/PROCEDENCIA.md does not record where {face} came from"
        );
    }
}

#[test]
fn the_faces_are_held_rather_than_swapped_in() {
    // `font-display: swap` paints the fallback and replaces it a beat later.
    // Everywhere else that is the kind choice; here the fallback is Arial
    // Narrow inside a NERV console, and a flash of it is worse than a beat of
    // nothing. The files are local, so the beat is a disk read.
    //
    // Scoped to the blocks, not to the file: the prose at the top of the sheet
    // explains this choice and names the property, and a guard a comment can
    // satisfy is a guard that cannot fail.
    let sheet = read("ui/fontes.css");
    let mut checked = 0;
    for block in sheet.split("@font-face").skip(1) {
        let Some(body) = block.split('}').next() else {
            continue;
        };
        assert!(
            body.contains("font-display: block"),
            "a @font-face does not hold the text while the face loads, so it \
             flashes a fallback first:\n{body}"
        );
        checked += 1;
    }
    assert!(checked > 0, "ui/fontes.css declares no @font-face at all");
}
