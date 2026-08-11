//! Guards for the untyped half of the desktop client.
//!
//! ADR 0019 chose no framework and no type checking on the frontend, and named
//! the mitigation: the boundary that matters is generated from Rust types, so a
//! `Snapshot` that changes shape stops compiling. What that argument does *not*
//! cover is the two things a plain-JS frontend gets wrong most often, both of
//! which fail silently at runtime:
//!
//! - calling a command name that no longer exists, and
//! - reading an element id that is not in the page.
//!
//! Neither shows up in a build. Both show up here.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::path::PathBuf;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(app_dir().join(relative))
        .unwrap_or_else(|error| panic!("{relative}: {error}"))
}

/// Every `invoke("name")` in the script.
fn invoked_commands(script: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for piece in script.split("invoke(\"").skip(1) {
        if let Some(name) = piece.split('"').next() {
            found.insert(name.to_owned());
        }
    }
    found
}

/// Every command registered in `generate_handler!`.
fn registered_commands(source: &str) -> BTreeSet<String> {
    let Some(block) = source.split("tauri::generate_handler![").nth(1) else {
        panic!("no generate_handler! block in main.rs");
    };
    let Some(block) = block.split(']').next() else {
        panic!("unterminated generate_handler! block");
    };
    block
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_command_the_frontend_calls_is_registered() {
    // A typo here is a promise rejected at runtime with a message nobody reads,
    // in a build that succeeded.
    let script = read("ui/seele.js");
    let source = read("src/main.rs");

    let called = invoked_commands(&script);
    let registered = registered_commands(&source);

    assert!(!called.is_empty(), "the frontend calls nothing at all");

    let missing: Vec<&String> = called.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "the frontend calls commands that main.rs does not register: {missing:?}\n\
         registered: {registered:?}"
    );
}

#[test]
fn no_command_is_registered_and_never_called() {
    // The other direction. A command nobody calls is either dead weight or a
    // feature that was wired on one side only — and the second is the one worth
    // catching.
    let called = invoked_commands(&read("ui/seele.js"));
    let registered = registered_commands(&read("src/main.rs"));

    let unused: Vec<&String> = registered.difference(&called).collect();
    assert!(
        unused.is_empty(),
        "main.rs registers commands the frontend never calls: {unused:?}"
    );
}

#[test]
fn every_element_the_script_reaches_for_exists_in_the_page() {
    // `$("nao-existe")` returns null, and the next line throws. In a page with
    // no build step, nothing else would have said so.
    let script = read("ui/seele.js");
    let page = read("ui/index.html");

    let mut wanted = BTreeSet::new();
    for piece in script.split("$(\"").skip(1) {
        if let Some(id) = piece.split('"').next() {
            wanted.insert(id.to_owned());
        }
    }
    // Ids built at runtime from a list literal, which the split above misses.
    for piece in script.split("$(id)").skip(1) {
        let _ = piece;
    }
    for piece in script.split('"').filter(|piece| piece.starts_with("sub-")) {
        wanted.insert(piece.to_owned());
    }

    assert!(!wanted.is_empty(), "the script reaches for nothing at all");

    let missing: Vec<&String> = wanted
        .iter()
        .filter(|id| !page.contains(&format!("id=\"{id}\"")))
        .collect();
    assert!(
        missing.is_empty(),
        "the script reads element ids that index.html does not define: {missing:?}"
    );
}

#[test]
fn the_page_loads_only_files_that_are_shipped() {
    // The CSP is `default-src 'self'`, so anything external is blocked at
    // runtime rather than at build time — a broken stylesheet reference would
    // simply render an unstyled window.
    let page = read("ui/index.html");

    for asset in ["tokens.css", "seele.css", "seele.js"] {
        assert!(page.contains(asset), "index.html never loads {asset}");
        assert!(
            app_dir().join("ui").join(asset).exists(),
            "index.html loads {asset}, which is not in ui/"
        );
    }

    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "the page references something off the machine, which the CSP blocks"
    );
}

/// Strips `//` comments and HTML comments.
///
/// The check below is about what the code *does*, and a comment explaining that
/// the frontend must not know what an `ssrc` is would otherwise fail the test
/// that enforces it.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return out,
        }
    }
    out.push_str(rest);

    out.lines()
        .map(|line| match line.find("//") {
            Some(at) if !line[..at].contains('"') => &line[..at],
            _ => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_highlight_ranges_land_on_the_term_inside_the_body_the_page_draws() {
    // Both halves of one invariant, and asserting one substring on one side of
    // it is how a highlight breaks with a green suite: read the wrong field
    // names in JS and the ranges arrive `undefined`, every `<mark>` comes out
    // empty, and a textual guard never notices.
    //
    // So this runs the arithmetic instead. `.mensagens .corpo` is
    // `white-space: pre-wrap`, so the page draws the body with its newlines
    // intact and `corpoComRealce` slices exactly that string by character.
    // Reintroduce `normalize` on either side and the slice stops being the
    // term.
    //
    // The body needs a *run* of whitespace, and this is the whole subtlety:
    // `normalize` rewrites `n` whitespace characters as one, so a single `\n`
    // between words collapses to a single space and every offset stays put. A
    // body like `"linha um\nlinha dois"` therefore passes this test whether or
    // not `normalize` is in the way — it is blind to exactly the defect being
    // guarded. Two spaces and two newlines is what makes the offsets move.
    let body = "linha  um\n\nlinha dois";
    let search = seele_ffi::search::Search::new([body], "linha");

    assert_eq!(
        search.matches().len(),
        2,
        "both occurrences should be found across the line break"
    );

    for found in search.matches() {
        let sliced: String = body
            .chars()
            .skip(found.start)
            .take(found.end - found.start)
            .collect();
        assert_eq!(
            sliced, "linha",
            "the range {}..{} does not land on the term in the body the page draws; \
             one side of the bridge is collapsing whitespace and the other is not",
            found.start, found.end
        );
    }
}

#[test]
fn neither_side_of_the_bridge_collapses_whitespace() {
    // The test above proves the arithmetic on raw bodies. This is what keeps
    // both callers on raw bodies — the failure it guards is a silent one-per-run
    // offset drift, invisible until somebody pastes a message with two lines.
    let source = read("src/main.rs");
    let script = read("ui/seele.js");

    assert!(
        !source.contains("search::normalize"),
        "`buscar` normalises the bodies, but the page draws them raw"
    );
    assert!(
        !script.contains("split(/\\s+/)"),
        "the page collapses the body, but the ranges index the raw one"
    );
}

#[test]
fn the_script_reads_the_field_names_a_match_actually_serialises() {
    // `Match` carries no `serde(rename)`, so the payload says `message`,
    // `start`, `end`. Portuguese names would destructure to `undefined`, slice
    // to nothing, and paint empty marks — with no error anywhere.
    let script = read("ui/seele.js");

    for field in ["message", "start", "end"] {
        assert!(
            script.contains(field),
            "the script never names `{field}`, which is what a Match serialises to"
        );
    }
    for wrong in [".mensagem", "inicio", ".fim"] {
        assert!(
            !script.contains(wrong),
            "the script reads `{wrong}` off a Match, which serialises no such field"
        );
    }
}

#[test]
fn the_visited_list_hides_itself_when_it_is_empty() {
    // A heading over an empty list is worse than no heading: with nowhere to go
    // back to, the entry screen must be exactly what it was before the section
    // existed.
    let page = read("ui/index.html");
    let script = read("ui/seele.js");

    let section = page
        .split("id=\"visitados\"")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        section.contains("hidden"),
        "the visited section is not hidden in the markup, so it flashes before the list loads"
    );
    assert!(
        script.contains("secao.hidden = lista.length === 0"),
        "nothing hides the visited section when the list comes back empty"
    );
}

#[test]
fn the_frontend_never_names_a_protocol_concept() {
    // `specs/06-clientes-gui.md`, in one sentence: "Se o frontend precisa saber
    // o que é um `ssrc`, algo está errado." This is that sentence as a test.
    let script = without_comments(&read("ui/seele.js"));
    let page = without_comments(&read("ui/index.html"));

    for forbidden in ["ssrc", "opus_frame", "datagram", "quic", "postcard"] {
        for (name, text) in [("seele.js", &script), ("index.html", &page)] {
            assert!(
                !text.to_lowercase().contains(forbidden),
                "{name} names `{forbidden}`, which is protocol knowledge in a shell"
            );
        }
    }
}
