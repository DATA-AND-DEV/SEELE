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
    let css = std::fs::read_to_string(repo_root().join("apps/seele-app/ui/seele.css"))
        .expect("the stylesheet must exist");

    let literals: Vec<&str> = css
        .lines()
        .filter(|line| line.contains('#') && !line.trim_start().starts_with("/*"))
        .filter(|line| {
            line.split('#')
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
