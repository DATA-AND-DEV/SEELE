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

/// A file from `apps/seele-app/`, with its line endings normalised to `\n`.
///
/// The normalisation is not tidiness. Git on Windows checks out with CRLF by
/// default and this repository ships no `.gitattributes`, so the very same
/// commit reaches a Windows runner with every `\n` spelled `\r\n`. Several
/// guards below cut text at a line boundary — `body_of` looks for `"\n}\n"` to
/// find where a function ends — and against CRLF that needle is simply never
/// found. `split` then returns the *whole remaining file* as the first piece,
/// so a guard scoped to one function silently widens to everything after it and
/// starts reporting its neighbours.
///
/// That is exactly how this was found: `a_pilot_card_…` passed on macOS and
/// failed on Windows, accusing the call screen of drawing a per-pilot waveform
/// out of `input_level` — a line that lives in a different function further
/// down the same file.
///
/// Every guard here asks about **content**, and content does not change with
/// how a checkout spells its newlines. So the spelling is settled once, here,
/// rather than in each guard that happens to cut on a line.
fn read(relative: &str) -> String {
    std::fs::read_to_string(app_dir().join(relative))
        .unwrap_or_else(|error| panic!("{relative}: {error}"))
        .replace("\r\n", "\n")
}

/// Every file directly in `ui/` with this extension, by name.
///
/// The frontend is six scripts and six stylesheets rather than one of each, so
/// every check below that used to read `ui/seele.js` has to read all of them —
/// and it has to find them by looking, not by a list somebody has to remember to
/// extend. A seventh screen that nobody adds here would otherwise arrive with
/// every guard in this file silently blind to it.
///
/// Sorted by name so the result is stable; the *load* order is asserted
/// separately, against `index.html`, which is the only place it is real.
fn ui_files(extension: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(app_dir().join("ui")) else {
        panic!("ui/ must exist: it is the whole frontend");
    };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(extension))
        .collect();
    found.sort();
    assert!(!found.is_empty(), "ui/ ships no {extension} at all");
    found
}

/// Every script the window loads, concatenated.
fn scripts() -> String {
    ui_files(".js")
        .iter()
        .map(|name| read(&format!("ui/{name}")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every stylesheet the window loads, concatenated in load order.
///
/// Load order, and not alphabetical: two of the checks below read a rule out of
/// this text, and one of them (`.veredito`) would find the wrong block if the
/// sheets were stitched in an order the browser never uses. `tokens.css` and
/// `fontes.css` are left out — they are declarations, not rules, and
/// `tests/tokens.rs` and `tests/fontes.rs` own them.
fn styles() -> String {
    let page = read("ui/index.html");
    linked_assets(&page, "href")
        .into_iter()
        .filter(|name| name.ends_with(".css") && name != "tokens.css" && name != "fontes.css")
        .map(|name| read(&format!("ui/{name}")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The files an attribute loads, in the order the page names them.
///
/// Comments are stripped first, and that is the whole point rather than tidiness:
/// the block above the `<link>` list names half of these files in prose, so a
/// check that read the raw page could be satisfied by the paragraph explaining
/// the tag it was meant to guard.
fn linked_assets(page: &str, attribute: &str) -> Vec<String> {
    let page = without_comments(page);
    let needle = format!("{attribute}=\"");
    page.split(&needle)
        .skip(1)
        .filter_map(|piece| piece.split('"').next())
        .map(str::to_owned)
        .collect()
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
    let script = scripts();
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
    let called = invoked_commands(&scripts());
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
    let script = scripts();
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

    // Both directions, and both matter now that the frontend is eleven files
    // instead of four.
    //
    // Outwards: everything the page names has to be on disk. `fontes.css` is
    // the one whose absence would be hardest to notice — the page would still
    // render, in Arial Narrow. `tests/fontes.rs` guards what is inside it.
    //
    // Inwards: every sheet and every script in `ui/` has to be *loaded*. This
    // is the half the split made necessary. A `tela-chamada.css` that nobody
    // links is a screen with no styling and no error anywhere, and the old
    // hard-coded list of four would have said nothing about it.
    //
    // The attribute, not the bare name, and on the comment-stripped page. A
    // bare `contains` was an implicit tag check only while each name appeared
    // exactly once in the file; the comment above the `<link>` block names
    // several of these files in prose, and with that the assertion became
    // satisfiable by the explanation of the tag it was meant to guard. Deleting
    // the `fontes.css` link once left all nineteen tests green. Same defect
    // class as the one `without_comments` exists for, one file over: a guard a
    // comment can satisfy is a guard that cannot fail.
    let stylesheets = linked_assets(&page, "href");
    let sources = linked_assets(&page, "src");

    for asset in stylesheets.iter().chain(sources.iter()) {
        assert!(
            app_dir().join("ui").join(asset).exists(),
            "index.html loads {asset}, which is not in ui/"
        );
    }

    for (extension, loaded) in [(".css", &stylesheets), (".js", &sources)] {
        for shipped in ui_files(extension) {
            assert!(
                loaded.contains(&shipped),
                "ui/ ships {shipped}, and index.html never loads it — so it is a \
                 file nobody sees and every guard in this suite reads"
            );
        }
    }

    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "the page references something off the machine, which the CSP blocks"
    );
}

#[test]
fn the_shared_layer_loads_before_the_screens_and_accessibility_loads_last() {
    // Splitting one stylesheet into six reorders every rule in it, and order is
    // what decides a specificity tie. Two ties in this page are decided by
    // nothing else:
    //
    // - `acessibilidade.css` sets `.rotulo`, `.painel-titulo` and
    //   `.lista .piloto` under `prefers-contrast: more`, against the very rules
    //   it is correcting, at the same specificity. Move that file up and the
    //   high-contrast mode stops working — silently, and only for the people who
    //   asked for it.
    // - every screen sheet refines a primitive from `base.css` at equal or
    //   higher specificity (`.compor input`, `.busca .botao-fantasma`,
    //   `.visitados-titulo` over `.rotulo`). Move `base.css` down and those
    //   refinements lose.
    //
    // The scripts have the same shape of hazard for a different reason: they
    // share one global scope, so what changes when they are split is not
    // visibility but *when* each name comes to exist. `base.js` is executed by
    // every other file's top level; a screen that loads before it registers a
    // listener on a function that is not there yet, and the page dies on load.
    let page = read("ui/index.html");
    let sheets = linked_assets(&page, "href");
    let sources = linked_assets(&page, "src");

    let position = |list: &[String], name: &str| {
        list.iter()
            .position(|entry| entry == name)
            .unwrap_or_else(|| panic!("index.html never loads {name}"))
    };

    let base = position(&sheets, "base.css");
    let accessibility = position(&sheets, "acessibilidade.css");
    // Every sheet between the two ends, and not only the ones named `tela-`.
    // The prefix was the rule while every sheet was a screen; the alert and the
    // battery are *layers* over the session rather than screens of their own,
    // so they are named `camada-` — and under the old filter they were the one
    // kind of file this check silently skipped. A guard that has to be told
    // about each new naming convention is a guard that stops holding on the
    // first convention somebody adds.
    for (at, sheet) in sheets.iter().enumerate() {
        // `href` also carries the favicon, which is an SVG and has no place in a
        // cascade order at all. `tokens.css` and `fontes.css` declare rather
        // than paint and belong *before* `base.css` on purpose — the same two
        // `styles()` leaves out, for the same reason.
        if !sheet.ends_with(".css")
            || matches!(
                sheet.as_str(),
                "base.css" | "acessibilidade.css" | "tokens.css" | "fontes.css"
            )
        {
            continue;
        }
        assert!(
            at > base,
            "{sheet} is loaded before base.css, so every rule it refines wins over it"
        );
        assert!(
            at < accessibility,
            "{sheet} is loaded after acessibilidade.css, which silently turns off \
             the high-contrast rules that only win by being last"
        );
    }
    let last_sheet = sheets
        .iter()
        .rposition(|entry| entry.ends_with(".css"))
        .unwrap_or_default();
    assert_eq!(
        accessibility, last_sheet,
        "acessibilidade.css is not the last stylesheet the page loads"
    );

    let base = position(&sources, "base.js");
    for (at, source) in sources.iter().enumerate() {
        if !source.ends_with(".js") || source == "base.js" {
            continue;
        }
        assert!(
            at > base,
            "{source} is loaded before base.js, whose helpers it calls the moment \
             it registers a listener"
        );
    }
}

/// Strips `//`, `/* */` and HTML comments.
///
/// The checks below are about what the code *does*. A comment explaining that
/// the frontend must not know what an `ssrc` is would otherwise fail the test
/// that enforces it — and, in the other direction, a doc comment naming a
/// verdict would satisfy the test that demands the verdict be *handled*. Block
/// comments are stripped for that second reason: every explanation in
/// `seele.js` is a `/** */`, and a guard a comment can satisfy is a guard that
/// cannot fail.
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

    let mut without_blocks = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(start) = rest.find("/*") {
        without_blocks.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => return without_blocks,
        }
    }
    without_blocks.push_str(rest);
    let out = without_blocks;

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
    let script = scripts();

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
    let script = scripts();

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
fn a_message_arriving_does_not_throw_the_search_back_to_the_first_occurrence() {
    // What `buscar` does on every `MessagesChanged`, with the same types: build
    // the search again over the new history, then put the cursor back.
    //
    // Without the second half the counter snapped to `[1/12]` and the pane
    // jumped to match one every time anybody spoke, which is precisely the
    // conversation where searching is worth doing.
    let before_bodies = ["sync caiu", "o sync voltou", "o sync nem caiu"];
    let mut before = seele_ffi::search::Search::new(before_bodies, "sync");
    before.next_match();
    assert_eq!(
        before.position(),
        (2, 3),
        "the cursor should be on match two"
    );
    let Some(was_on) = before.current() else {
        panic!("a search with three matches has a current one");
    };

    // Somebody speaks. The list is rebuilt and every later index shifts.
    let after_bodies = [
        "sync caiu",
        "o sync voltou",
        "o sync nem caiu",
        "sync de novo",
    ];
    let mut after = seele_ffi::search::Search::new(after_bodies, "sync");
    assert_eq!(
        after.position(),
        (1, 4),
        "a freshly built search starts at one — this is the state being corrected"
    );

    after.resume_at(was_on);
    assert_eq!(
        after.position(),
        (2, 4),
        "the cursor did not stay on the occurrence the reader was on"
    );
    assert_eq!(
        after.current(),
        Some(was_on),
        "the cursor moved to a different occurrence than the one it was on"
    );
}

#[test]
fn the_search_command_puts_the_cursor_back_after_rebuilding() {
    // The test above proves the rule; this is what keeps `buscar` calling it.
    // The rule has to run on the Rust side — the cursor lives in
    // `Session::busca`, and `specs/06-clientes-gui.md:19` keeps decisions like
    // this one out of the frontend.
    let source = read("src/main.rs");
    let script = scripts();

    assert!(
        source.contains("resume_at"),
        "`buscar` rebuilds the search and never restores the cursor, so every \
         incoming message sends the reader back to occurrence one"
    );
    assert!(
        !script.contains("resume_at") && !script.contains("busca.cursor"),
        "the frontend is deciding where the search cursor goes, which is protocol \
         logic in JavaScript"
    );
}

#[test]
fn the_ordinal_indexes_the_same_list_the_page_groups_by_message() {
    // `desenharMensagens` groups the matches by message, in the order they
    // arrive, and lights the `ordinal`-th one of that group. So the ordinal has
    // to be an index into exactly that list — if the core counted any other way
    // the wrong word would be marked, silently and always by the same offset.
    let body = "sync caiu, o sync voltou, e o sync nem caiu";
    let mut search = seele_ffi::search::Search::new(["antes", body], "sync");

    let in_this_message: Vec<_> = search
        .matches()
        .iter()
        .filter(|found| found.message == 1)
        .copied()
        .collect();
    assert_eq!(in_this_message.len(), 3, "the body matches three times");

    for expected in 0..3 {
        assert_eq!(
            search.current().map(|found| found.message),
            Some(1),
            "the walk left the message under test"
        );
        let Some(ordinal) = search.ordinal_in_message() else {
            panic!("a search with a current match has an ordinal");
        };
        assert_eq!(
            ordinal, expected,
            "the ordinal is not counting in drawing order"
        );
        assert_eq!(
            search.current(),
            in_this_message.get(ordinal).copied(),
            "the ordinal does not index the list the page groups by message"
        );
        search.next_match();
    }
}

#[test]
fn the_current_occurrence_is_not_marked_out_by_colour_alone() {
    // `specs/05-cliente-tui.md:144`. The terminal separates the two states with
    // REVERSED against plain accent; the browser has weight and decoration, and
    // has to use one of them. A rule that only swaps hues would leave the
    // cursor invisible on a monochrome display — which is the same failure as
    // not marking it at all, for the people it happens to.
    let script = scripts();
    let css = styles();

    assert!(
        script.contains("realce-atual"),
        "nothing in the page marks the occurrence the cursor is on"
    );
    assert!(
        script.contains("estado.ordinal"),
        "the script never reads the ordinal, so it cannot know which match is the current one"
    );
    assert!(
        read("src/main.rs").contains("ordinal_in_message"),
        "the bridge never sends which occurrence inside its message the cursor is on"
    );

    let rule = css
        .split(".realce-atual")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or_default();
    assert!(
        !rule.is_empty(),
        "the stylesheet has no rule for the current occurrence"
    );
    assert!(
        rule.contains("font-weight") || rule.contains("outline") || rule.contains("border"),
        "the current occurrence differs from the others only by colour:\n{rule}"
    );
}

#[test]
fn the_visited_list_hides_itself_when_it_is_empty() {
    // A heading over an empty list is worse than no heading: with nowhere to go
    // back to, the entry screen must be exactly what it was before the section
    // existed.
    let page = read("ui/index.html");
    let script = scripts();

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
fn the_event_name_the_script_branches_on_is_the_one_the_bridge_sends() {
    // The listener rebuilds a live search only when the message list changed —
    // an edit rewrites a body in place and a removal shortens the list, and
    // either one leaves cached ranges pointing at different text.
    //
    // That branch is a string compared against whatever serde makes of a unit
    // variant. Rename the variant and the branch simply stops firing: no error,
    // no warning, and a search that quietly goes stale again.
    let sent = serde_json::to_string(&seele_ffi::Event::MessagesChanged)
        .expect("the event must serialise to cross the bridge at all");
    assert_eq!(
        sent, "\"MessagesChanged\"",
        "the bridge no longer sends the string the script branches on"
    );

    assert!(
        scripts().contains("payload === \"MessagesChanged\""),
        "nothing in the script rebuilds the search when the message list changes"
    );
}

/// The body of a top-level `fn`, comments stripped.
///
/// Two of the checks below are about one statement being inside one function,
/// and `main.rs` as a whole says the word either way — the paragraph explaining
/// why the invite dies in `disconnect` would satisfy a search for `convite`
/// long after the line itself was deleted. Scoping and stripping is what makes
/// those checks able to fail.
///
/// `\n}\n` terminates: every brace inside a body is indented.
fn body_of(source: &str, signature: &str) -> String {
    let Some(after) = source.split(signature).nth(1) else {
        panic!("main.rs no longer has `{signature}`");
    };
    let Some(body) = after.split("\n}\n").next() else {
        panic!("unterminated `{signature}`");
    };
    without_comments(body)
}

/// Does the text name `word` on its own, rather than inside a longer name?
///
/// The distinction is the whole point of the test below. `FirstContact` is a
/// prefix of `FirstContactVerified`, so a plain `contains` would call a verdict
/// handled when the only thing left in the file is the *other* one — which is
/// precisely the deletion this has to catch.
fn names(text: &str, word: &str) -> bool {
    let edge = |character: Option<char>| {
        character.is_none_or(|character| !character.is_alphanumeric() && character != '_')
    };
    text.match_indices(word).any(|(at, _)| {
        edge(text[..at].chars().next_back()) && edge(text[at + word.len()..].chars().next())
    })
}

#[test]
fn every_verdict_the_bridge_can_send_has_its_own_sentence_in_the_page() {
    // A variant with no sentence is a blank screen at the moment it most needs
    // to say something — and this shell spent its whole life pinning keys in
    // silence, so the failure is not hypothetical.
    //
    // It serialises the real `Trust` rather than listing strings, so renaming a
    // variant breaks this instead of leaving a dead branch behind. Comments are
    // stripped first, and that is load-bearing rather than tidy: the doc comment
    // on `fraseDoVeredito` says `Known` in prose, so deleting the branch that
    // handles it would leave this green on the strength of an explanation.
    //
    // `InviteRefused` is left out on purpose — the core drops the connection,
    // so it reaches the shell as a `PlugError`. The test below covers it.
    let script = without_comments(&scripts());

    for verdict in [
        seele_ffi::Trust::FirstContact {
            fingerprint: "a".into(),
        },
        seele_ffi::Trust::FirstContactVerified {
            fingerprint: "a".into(),
        },
        seele_ffi::Trust::Known,
        seele_ffi::Trust::InviteDisagrees {
            expected: "b".into(),
            offered: "a".into(),
        },
    ] {
        let Ok(json) = serde_json::to_string(&verdict) else {
            panic!("Trust does not serialise, so no shell can read it at all");
        };
        // The variant name, exactly as serde writes it on the wire.
        let Some(name) = json.trim_matches('"').split("\":").next() else {
            panic!("unexpected shape: {json}");
        };
        let name = name.trim_start_matches('{').trim_matches('"');

        assert!(
            names(&script, name),
            "the verdict {name} has no handling in the script, so it lands on a \
             screen that says nothing"
        );
    }
}

#[test]
fn the_refused_invite_reaches_the_screen_with_both_fingerprints() {
    // The fifth verdict never crosses as a `Trust`: the core drops the
    // connection, so it arrives as the error below and its sentence lives on
    // the `#boot-erro` path. Both prints have to be in it — an accusation that
    // shows one of the two is an accusation nobody can check.
    let refusal = seele_ffi::PlugError::InviteMismatch {
        expected: "bbbb".into(),
        offered: "aaaa".into(),
    };
    let Ok(json) = serde_json::to_string(&refusal) else {
        panic!("PlugError does not serialise, so no shell can read it at all");
    };
    let script = without_comments(&scripts());

    assert!(
        names(&script, "InviteMismatch"),
        "nothing in the script reads the refusal, so a link that names another \
         Dogma fails with a sentence about nothing: {json}"
    );
    for field in ["expected", "offered"] {
        assert!(
            json.contains(&format!("\"{field}\"")),
            "the refusal no longer carries `{field}`: {json}"
        );
        assert!(
            names(&script, field),
            "the script never names `{field}`, so the reader gets half a comparison"
        );
    }
}

#[test]
fn the_informative_verdicts_do_not_spend_the_alarm_reserved_for_a_key_change() {
    // `specs/08-seguranca.md` reserves the impossible-to-ignore treatment for a
    // key that changed, and `tokens.css:19` marks the red "EXCLUSIVO alerta e
    // queda". A first contact and a link that names another Dogma stop nobody
    // from entering; dressing them as an alarm is what teaches people to
    // dismiss the alarm on the day it means the other thing.
    let page = without_comments(&read("ui/index.html"));
    let css = without_comments(&styles());

    let tag = page
        .split("id=\"veredito\"")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .unwrap_or_default();
    assert!(
        tag.contains("role=\"status\""),
        "the verdict is announced as an alert, which is the treatment reserved \
         for what stops you entering: {tag}"
    );

    let rule = css
        .split(".veredito")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or_default();
    assert!(
        !rule.is_empty(),
        "the stylesheet has no rule for the verdict band"
    );
    assert!(
        !rule.contains("vermelho"),
        "the verdict band is painted in the colour the tokens reserve for alarm \
         and collapse:\n{rule}"
    );
}

#[test]
fn the_comparison_stays_in_rust_and_only_its_verdict_crosses() {
    // `specs/06-clientes-gui.md:19`. The fingerprint crosses the bridge now,
    // which is a reversal — but it crosses as the *output* of a decision, to be
    // read by a person. What must never cross is the input: the value to check
    // against comes off `Session::convite` and goes straight into the FFI.
    let script = without_comments(&scripts());
    let connect = body_of(&read("src/main.rs"), "async fn connect");

    // Scoped to the body and blind to the variable that carries the value: what
    // matters is that the field is filled from the stored invite, not what the
    // local is called on the way.
    assert!(
        connect.contains("impressao_digital") && connect.contains("session.convite"),
        "`connect` never reads the invite's fingerprint, so a pasted link is \
         parsed and thrown away exactly as before"
    );
    assert!(
        connect.contains("expected_fingerprint") && !connect.contains("expected_fingerprint: None"),
        "`connect` hands the FFI no fingerprint to check against, so every \
         connection is as blind as a typed address"
    );
    assert!(
        !script.contains("expected_fingerprint") && !script.contains("expectedFingerprint"),
        "the frontend is feeding the comparison its input, which is the \
         comparison moving to JavaScript one argument at a time"
    );
    assert!(
        !script.contains("impressao_digital"),
        "the frontend reads the parsed invite's own field, so the whole `Convite` \
         crossed the bridge instead of the verdict"
    );
    assert!(
        !script.contains("conferencia_pendente"),
        "the script still branches on a pending check that no longer exists, so \
         that branch is dead and the screen it drew is gone"
    );
}

#[test]
fn leaving_forgets_the_invite_that_let_us_in() {
    // Inert while nothing was checked, and not inert any more: the fingerprint
    // that `connect` checks against comes from this slot. Left behind, the next
    // connection to a different Dogma would be checked against the previous
    // link's promise and refused for a reason nobody could explain.
    let body = body_of(&read("src/main.rs"), "async fn disconnect");
    assert!(
        body.contains("session.convite"),
        "`disconnect` drops the plug and the hosting but keeps the invite, so a \
         fingerprint from a previous link outlives the session it belonged to"
    );
}

#[test]
fn the_page_never_draws_a_glyph_the_data_face_does_not_have() {
    // The embedded IBM Plex Mono has 1049 cmap entries and exactly one glyph in
    // U+25A0–U+25CF, so every one of these fell through to whatever monospace
    // the machine happens to have — SF Mono, Consolas, something else — putting
    // a second face in the middle of a line, in an interface whose whole claim
    // is that every line is a grid.
    //
    // `docs/marca.md` settled the same argument one layer up: the brand's three
    // katakana are outlines and not text, because as text the mark would be
    // Hiragino on macOS and Yu Gothic on Windows. `ui/glifos.js` is that answer
    // applied to the interface's own six.
    //
    // Comments are stripped first, and that is load-bearing rather than tidy:
    // the files that explain these characters have to be able to name them, and
    // a comment draws nothing. What this catches is a seventh screen typing one
    // back into a template string.
    let script = without_comments(&scripts());
    let page = without_comments(&read("ui/index.html"));

    for glyph in [
        '\u{25B8}', // ▸ small right-pointing triangle
        '\u{25C2}', // ◂ small left-pointing triangle
        '\u{25BC}', // ▼ down-pointing triangle
        '\u{25B6}', // ▶ right-pointing triangle
        '\u{25CF}', // ● black circle
        '\u{25CB}', // ○ white circle
        '\u{2318}', // ⌘ place of interest sign
        // The eight the v3 comp introduces. Every one measured against the
        // shipped face rather than assumed from its Unicode block: the `cmap`
        // of `fontes/ibm-plex-mono-400.woff2` has 499 codepoints and none of
        // these is among them. `ui/glifos.js` draws all eight.
        '\u{2315}', // ⌕ telephone recorder — drawn as the search lens
        '\u{2328}', // ⌨ keyboard
        '\u{23FB}', // ⏻ power on/off symbol
        '\u{25A4}', // ▤ square with horizontal fill
        '\u{25CD}', // ◍ circle with vertical fill
        '\u{25E7}', // ◧ square with left half black
        '\u{2699}', // ⚙ gear
        '\u{26BF}', // ⚿ squared key
    ] {
        for (name, text) in [("the scripts", &script), ("index.html", &page)] {
            assert!(
                !text.contains(glyph),
                "{name} draws U+{:04X} as a character, and the data face has no glyph \
                 for it — it falls through to the system monospace, mid-line. \
                 `glifo()` in ui/glifos.js draws it instead.",
                u32::from(glyph)
            );
        }
    }
}

#[test]
fn the_microphone_list_reads_the_field_names_a_device_actually_serialises() {
    // Same defect class as the `Match` guard above, one screen over. The picker
    // is untyped JavaScript reading three names off the wire; a `serde(rename)`
    // or a Portuguese field name would draw every row `undefined`, send back
    // `undefined` when one is clicked, and fail nowhere.
    let device = seele_ffi::CaptureDevice {
        id: "coreaudio:alguma-coisa".into(),
        name: "Scarlett Solo".into(),
        default: true,
    };
    let Ok(json) = serde_json::to_string(&device) else {
        panic!("CaptureDevice does not serialise, so no shell can read it at all");
    };
    let script = without_comments(&scripts());

    for field in ["id", "name", "default"] {
        assert!(
            json.contains(&format!("\"{field}\"")),
            "a capture device no longer carries `{field}`: {json}"
        );
        assert!(
            names(&script, field),
            "the picker never names `{field}`, which is what a capture device \
             serialises to"
        );
    }
    for wrong in ["dispositivo.nome", "dispositivo.padrao"] {
        assert!(
            !script.contains(wrong),
            "the picker reads `{wrong}` off a capture device, which serialises no \
             such field"
        );
    }
}

#[test]
fn the_picker_sends_back_the_id_and_never_the_name() {
    // The one mistake this screen can make silently. Two microphones of the same
    // model report the same name, so a picker that sent the name back would work
    // on every machine with one interface and pick the wrong device on the
    // machines the feature exists for.
    //
    // Scoped to the function that builds a row, because the file as a whole says
    // both words either way — the paragraph explaining why the id is not shown
    // would otherwise satisfy a search for it.
    let body = body_of(&scripts(), "function linhaDeMicrofone");

    assert!(
        body.contains("dataset.dispositivo = id"),
        "a row no longer carries the id it stands for, so nothing can be sent back"
    );
    assert!(
        !body.contains("escolher(nome"),
        "the picker hands back the name, and two microphones of one model share it"
    );
}

#[test]
fn the_screen_says_which_microphone_is_open_and_not_only_which_was_chosen() {
    // The two diverge exactly when this screen is worth opening: an interface
    // chosen yesterday and unplugged today reads as chosen, while something else
    // is actually capturing. A screen that drew only the preference would call
    // the choice reality and leave somebody talking into the wrong microphone
    // while looking at a row that says it is the right one.
    // Scoped to the one function that marks a row. The file as a whole says both
    // words either way — the paragraph explaining the distinction would satisfy
    // a search for it — and `EM USO` in particular appears in an unrelated
    // sentence about port 8383 two files over, which is enough to make an
    // unscoped `contains` a guard that cannot fail. Found by breaking the code
    // on purpose and watching this pass.
    let body = body_of(&scripts(), "function marcarLinhas");

    assert!(
        body.contains("snapshot?.capture?.id") || body.contains("snapshot.capture.id"),
        "nothing in the picker reads which device actually opened, so it draws the \
         preference and calls it reality"
    );
    assert!(
        body.contains("EM USO") && body.contains("ESCOLHIDO"),
        "the picker has one word for both states, so it cannot show them apart:\n{body}"
    );

    // And the bridge has to still answer the other half.
    assert!(
        read("src/main.rs").contains("microfone_escolhido"),
        "the bridge no longer answers which microphone was chosen"
    );
}

/// The variants of one `enum` declared in `main.rs`.
///
/// Serde writes a fieldless variant as its own name, so these are exactly the
/// strings the frontend receives when the command rejects.
fn variants_of(source: &str, name: &str) -> Vec<String> {
    let Some(after) = source.split(&format!("enum {name} {{")).nth(1) else {
        panic!("main.rs no longer declares `enum {name}`");
    };
    let Some(block) = after.split("\n}").next() else {
        panic!("unterminated `enum {name}`");
    };
    let found: Vec<String> = without_comments(block)
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_suffix(','))
        .filter(|line| {
            !line.is_empty()
                && line
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '_')
        })
        .map(str::to_owned)
        .collect();
    assert!(!found.is_empty(), "`enum {name}` came out with no variants");
    found
}

#[test]
fn every_refusal_the_bridge_writes_itself_has_a_sentence_in_the_page() {
    // Two of this shell's commands answer with an enum of their own rather than
    // a `PlugError` — hosting, and choosing a microphone — because their
    // failures are local ones the FFI has no business naming. That freedom costs
    // exactly this guard: a variant added here with no sentence over there lands
    // on a screen that says nothing, or worse, prints the name of a Rust variant
    // at somebody.
    //
    // The refusal most likely to be met is the one this was written for: the
    // list is drawn, an interface is unplugged, and then a row is clicked.
    //
    // Comments are stripped from both sides. The paragraph in `main.rs`
    // explaining why a variant exists must not count as declaring it, and the
    // paragraph in `frases.js` explaining a sentence must not count as writing
    // it.
    let source = read("src/main.rs");
    let script = without_comments(&scripts());

    for enumeration in ["FalhaAoHospedar", "FalhaAoEscolher"] {
        for variant in variants_of(&source, enumeration) {
            assert!(
                names(&script, &variant),
                "`{enumeration}::{variant}` reaches the page with no sentence written \
                 for it, so the screen either says nothing or says `{variant}`"
            );
        }
    }
}

#[test]
fn the_frontend_never_names_a_protocol_concept() {
    // `specs/06-clientes-gui.md`, in one sentence: "Se o frontend precisa saber
    // o que é um `ssrc`, algo está errado." This is that sentence as a test.
    let script = without_comments(&scripts());
    let page = without_comments(&read("ui/index.html"));

    for forbidden in ["ssrc", "opus_frame", "datagram", "quic", "postcard"] {
        for (name, text) in [("the scripts", &script), ("index.html", &page)] {
            assert!(
                !text.to_lowercase().contains(forbidden),
                "{name} names `{forbidden}`, which is protocol knowledge in a shell"
            );
        }
    }
}

/// Every screen carries an id, and the app opens on exactly one of them.
///
/// Screens are swapped by assigning `.hidden` on the section directly, so the
/// *initial* state is in the markup and nowhere else — no script sets it on
/// load. A screen added without `hidden` therefore does not fail, does not
/// throw, and does not warn: it renders stacked below whichever screen is
/// supposed to be showing, and the window looks like boot with a second screen
/// nailed underneath. That is the failure this catches, and it is the one a
/// fifth screen is most likely to arrive with.
#[test]
fn the_app_opens_on_exactly_one_screen() {
    let page = read("ui/index.html");

    let mut screens = Vec::new();
    for rest in page.split("<section ").skip(1) {
        let Some(end) = rest.find('>') else { continue };
        let tag = &rest[..end];

        let classes = attribute(tag, "class").unwrap_or_default();
        if !classes.split_whitespace().any(|class| class == "tela") {
            continue;
        }

        let Some(id) = attribute(tag, "id") else {
            panic!("a `<section class=\"tela\">` has no id, so no script can reach it: <{tag}>");
        };
        screens.push((id, tag.contains("hidden")));
    }

    assert!(
        screens.len() >= 4,
        "found {} screens; boot, sessao, auth and fim are all supposed to be in the page",
        screens.len()
    );

    let open: Vec<&str> = screens
        .iter()
        .filter(|(_, hidden)| !hidden)
        .map(|(id, _)| id.as_str())
        .collect();

    assert_eq!(
        open,
        ["tela-boot"],
        "the app has to open on boot and on nothing else, but these screens start visible: {open:?}"
    );
}

/// The value of `name="…"` in an already-isolated opening tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let (_, after) = tag.split_once(&format!("{name}=\""))?;
    let (value, _) = after.split_once('"')?;
    Some(value.to_owned())
}

/// The opening tag of the element carrying `id`.
///
/// Comments are stripped first for the reason the rest of this file strips
/// them: several of these ids are named in the prose above the element they
/// belong to, and a check satisfied by an explanation is a check that cannot
/// fail.
fn tag_with_id(page: &str, id: &str) -> String {
    let page = without_comments(page);
    let Some(after) = page.split(&format!("id=\"{id}\"")).nth(1) else {
        panic!("index.html has no element with id `{id}`");
    };
    let Some(rest) = after.split('>').next() else {
        panic!("unterminated tag for id `{id}`");
    };
    rest.to_owned()
}

#[test]
fn the_alert_and_the_battery_are_layers_over_the_session_and_not_screens() {
    // The comp's inventory settles this on line 281: `alerta` and `bateria` are
    // layers over `principal`, and `ehPrincipal` is true for all three. They are
    // not screens.
    //
    // Promoting either to a `<section class="tela">` is the tempting mistake —
    // both are full-window overlays now, and both would *look* right as
    // screens. What breaks is invisible in a screenshot: `specs/07` forbids this
    // client from replacing the conversation when the link drops ("a interface
    // esmaece … e o histórico continua ali para leitura"), and a screen replaces
    // it by definition. So the guard is structural: they have to live inside
    // `#tela-sessao`.
    //
    // The call screen pins the opposite decision in the same test, because the
    // two are decided together and drift apart otherwise: it *does* replace the
    // conversation, so it must not be nested in the session.
    let page = read("ui/index.html");
    let Some(after) = page.split("id=\"tela-sessao\"").nth(1) else {
        panic!("index.html no longer has the session screen");
    };
    let Some(session) = after.split("<section ").next() else {
        panic!("the session screen is never closed by another section");
    };

    for id in ["banner", "bateria"] {
        assert!(
            session.contains(&format!("id=\"{id}\"")),
            "`{id}` is drawn outside `#tela-sessao`, so it is a screen and not a \
             layer — and a screen replaces the history that specs/07 says has to \
             stay readable while the link is down"
        );
    }

    assert!(
        !session.contains("id=\"tela-chamada\""),
        "the call screen is nested inside the session, so it is a layer — but it \
         replaces the Line's history instead of sitting over it, which is the one \
         thing a layer must not do"
    );
}

#[test]
fn the_severity_of_a_notice_reads_without_colour() {
    // `specs/05-cliente-tui.md` forbids information carried by colour alone, and
    // the severity of a `Notice` is information: the core decided between three
    // values and the shell has to show which one. The alert box is one orange
    // box for all three — the comp's own legend says orange for mention and
    // identity, red only for a dropped link — so nothing about the box's paint
    // separates them. The word has to.
    //
    // Built from the real enum, so renaming a variant fails here instead of
    // leaving a `data-para` that matches nothing and a chip that never shows.
    let page = without_comments(&read("ui/index.html"));
    let css = styles();

    let mut words = BTreeSet::new();
    for severity in [
        seele_ffi::Severity::Info,
        seele_ffi::Severity::Warning,
        seele_ffi::Severity::Critical,
    ] {
        let Ok(json) = serde_json::to_string(&severity) else {
            panic!("Severity does not serialise, so no shell can read it at all");
        };
        let name = json.trim_matches('"').to_owned();

        let marker = format!("data-para=\"{name}\">");
        let Some(word) = page
            .split(&marker)
            .nth(1)
            .and_then(|rest| rest.split('<').next())
        else {
            panic!(
                "the page has no chip for severity `{name}`, so that severity \
                 arrives with nothing but the same orange box as the other two"
            );
        };
        assert!(
            !word.trim().is_empty(),
            "the chip for severity `{name}` is empty"
        );
        assert!(
            css.contains(&format!("data-severidade=\"{name}\"")),
            "nothing in the stylesheet reveals the chip for severity `{name}`, so \
             the word is in the markup and never on the screen"
        );
        words.insert(word.trim().to_owned());
    }

    assert_eq!(
        words.len(),
        3,
        "two severities are written with the same word, so the box cannot tell \
         them apart at all: {words:?}"
    );
}

#[test]
fn the_two_buttons_with_no_command_behind_them_cannot_be_pressed() {
    // Both are drawn because the comp draws them, and both are disabled because
    // nothing in this product can carry them out. `EJETAR PLUG DO OPERADOR` has
    // no moderation verb — `EndReason::Kicked` exists for the person receiving
    // one, and there is no way to emit it — and `FORÇAR REINSERÇÃO DE PLUG` has
    // no "try now": the core is already retrying, which is where the attempt
    // count above the button comes from.
    //
    // The comp wires both to a handler that closes the box, which is the worst
    // of the available readings: a button that looks like it acts, and does
    // nothing. Enabling either of these here reproduces exactly that.
    let page = read("ui/index.html");

    for id in ["alerta-ejetar", "bateria-forcar"] {
        let tag = tag_with_id(&page, id);
        assert!(
            tag.contains("disabled"),
            "`{id}` is pressable, and there is no command behind it — so it is a \
             button that looks like it acts and does nothing: <{tag}>"
        );
        assert!(
            tag.contains("title=\""),
            "`{id}` is disabled and says nothing about why, which reads as a bug \
             rather than as a gap: <{tag}>"
        );
    }

    // And nothing may quietly wire them up instead of enabling them.
    let script = without_comments(&scripts());
    for id in ["alerta-ejetar", "bateria-forcar"] {
        assert!(
            !script.contains(&format!("$(\"{id}\")")),
            "a script reaches for `{id}`, so the disabled button grew a listener \
             — which is the comp's mistake, one layer down"
        );
    }
}

#[test]
fn the_alert_box_does_not_spend_the_red_reserved_for_a_dropped_link() {
    // `tokens.css:19` marks the red "EXCLUSIVO alerta e queda", and the comp's
    // own banner legend narrows it further: "laranja para menção e identidade;
    // vermelho apenas quando a conexão cai". The battery box is the red one.
    //
    // The tempting change is to escalate `Severity::Critical` to red, and it is
    // exactly what teaches people to read the battery's red as "some notice",
    // on the day it means the session is being held in memory.
    //
    // Comments are stripped from both sides, and that is load-bearing rather
    // than tidy: the alert sheet has to be able to write down *why* it is not
    // red, and the paragraph saying so names the token. A guard a comment can
    // trip is as broken as a guard a comment can satisfy.
    let sheet = without_comments(&read("ui/camada-alerta.css"));
    assert!(
        !sheet.contains("vermelho"),
        "the alert layer paints with the token reserved for alarm and collapse, \
         which is the battery's colour and nothing else's"
    );
    assert!(
        without_comments(&read("ui/camada-bateria.css")).contains("vermelho"),
        "the battery layer no longer uses the red that is the whole point of it"
    );
}

#[test]
fn a_pilot_card_passes_the_band_through_and_never_measures_anything_itself() {
    // Two failures in one function, both silent.
    //
    // The first is the one `crates/seele-ffi/src/types.rs:58-79` argues against
    // by name: the comp calls `corSync(media)` in the shell, and a shell that
    // knows "85 is nominal" is a shell that will disagree with the terminal the
    // day one of the two is updated. The band arrives decided; this card may
    // only pass it on.
    //
    // The second is drawing what nobody measured. `Telemetry.input_level` is a
    // scalar and it is *ours* — amplitude per pilot does not cross — and
    // `set_volume` writes with nothing reading back. Twenty-six bars driven by
    // our own microphone would animate convincingly under somebody else's name.
    //
    // Scoped to the function that builds one card, because the file as a whole
    // says all of these words either way: the paragraph explaining why the
    // waveform is empty would satisfy an unscoped search for it.
    let body = body_of(&scripts(), "function cartaoDoPiloto");

    assert!(
        body.contains("piloto.sync_band"),
        "the card no longer reads the band the core decided"
    );
    for threshold in ["Nominal", "Degraded", "Critical"] {
        assert!(
            !body.contains(threshold),
            "the card names the band `{threshold}` itself, which is the shell \
             deciding a threshold the core already decided"
        );
    }

    assert!(
        !body.contains("input_level"),
        "the card draws a per-pilot waveform out of this machine's own input \
         level, which is one number, and ours"
    );
    for missing in ["SEM_DADO.onda", "SEM_DADO.volume"] {
        assert!(
            body.contains(missing),
            "the card no longer marks `{missing}` as unmeasured, so it is either \
             drawing a value nothing carries or hiding that the gap exists"
        );
    }
}

#[test]
fn the_battery_bar_stays_empty_because_nothing_carries_its_total() {
    // `remaining_seconds` crosses; the total does not. The comp divides by a
    // literal 299, which is the shell guessing the spec and being wrong the day
    // the spec changes — and the count beside the bar is already the same
    // information with no denominator at all.
    //
    // Two halves, because either one alone passes while the defect is present:
    // the element has to be marked absent, *and* no script may fill it.
    let page = without_comments(&read("ui/index.html"));
    let script = without_comments(&scripts());

    let Some(after) = page.split("class=\"bateria-barra").nth(1) else {
        panic!(
            "index.html no longer draws the battery bar at all — the frame is \
                what makes the gap visible"
        );
    };
    let Some(tag) = after.split('>').next() else {
        panic!("unterminated battery bar tag");
    };
    assert!(
        tag.contains("ausente"),
        "the battery bar is drawn as a measured value: <{tag}>"
    );
    assert!(
        tag.contains("title=\""),
        "the battery bar is empty and says nothing about why: <{tag}>"
    );
    assert!(
        !script.contains("bateria-barra"),
        "a script writes into the battery bar, so it is being filled from a \
         denominator this protocol never sent"
    );
}

#[test]
fn ending_the_session_takes_the_call_screen_down_with_it() {
    // Every `.tela` is `height: 100vh`, so two visible ones do not overlap: they
    // stack, and the second sits below the fold where nobody finds it. A session
    // can end with the call screen open — that is precisely who gets kicked, the
    // person sitting in a Cage — and `mostrarFim` picks the next screen on its
    // own.
    //
    // Scoped to `mostrarFim`, because `tela-chamada.js` names the function in
    // its own declaration and in the paragraph above it either way.
    let body = body_of(&scripts(), "function mostrarFim");
    assert!(
        body.contains("abandonarChamada"),
        "the end-of-session screen does not take the call screen down, so the two \
         stack and the reason the session ended lands below the fold"
    );
}

/// The class names a stylesheet *defines* — the first class of every selector
/// that starts a line.
///
/// The first class and not all of them, because that first one is the owner:
/// `.busca .botao-fantasma` is `tela-sessao.css` refining a primitive it did not
/// invent, and only `.busca` says whose rule it is.
fn classes_defined_in(css: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for line in css.lines() {
        let Some(rest) = line.strip_prefix('.') else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !name.is_empty() {
            found.insert(name);
        }
    }
    found
}

#[test]
fn the_hint_layer_is_defined_once_and_never_by_a_screen() {
    // `LEGENDAS SIMPLES` is the v3 comp's answer to an interface nobody could
    // work out: a short line beside each control saying what it does, shown by
    // default. It is one class toggled on `<body>` and one rule in `base.css`,
    // and it has to stay one.
    //
    // The failure this catches is a screen writing its own `.dica { display:
    // block }`. That screen's hints then ignore the toggle — they show while
    // the rest of the app has them off — and nothing fails: the page loads, the
    // hint is legible, and only somebody who turned the mode off and looked at
    // every screen would notice one kept talking.
    //
    // Refining is still allowed and is the point of the load order: a screen
    // may write `.painel .dica { margin-top: 4px }`, because the owner of that
    // rule is `.painel`. What it may not do is claim `.dica` itself. Same rule
    // the collision guard below applies, narrowed to the classes that carry a
    // behaviour rather than a look.
    let shared = classes_defined_in(&read("ui/base.css"));
    for name in ["dica", "dica-linha"] {
        assert!(
            shared.contains(name),
            "`.{name}` is not defined in base.css, so the hint layer has no shared \
             owner and each screen is free to invent one"
        );
    }

    let not_a_screen = ["base.css", "acessibilidade.css", "tokens.css", "fontes.css"];
    for name in ui_files(".css") {
        if not_a_screen.contains(&name.as_str()) {
            continue;
        }
        let owned = classes_defined_in(&read(&format!("ui/{name}")));
        for hint in ["dica", "dica-linha"] {
            assert!(
                !owned.contains(hint),
                "{name} claims `.{hint}`, which base.css owns. A screen that \
                 redefines it opts itself out of the simple-captions toggle, and \
                 nothing about that failure is visible — the hint just keeps \
                 showing after somebody turned the mode off."
            );
        }
    }
}

#[test]
fn the_cage_says_who_is_inside_and_says_their_state_in_words() {
    // The v3 comp's biggest single gain, and it costs no protocol: `cages_of`
    // already fills `Cage.pilots` from `room.roster(cage.id)` for *every* Cage,
    // not only the occupied one. The app spent that on a block bar — twelve
    // characters standing in for the four names it had in hand.
    //
    // The second half is the part that would rot silently. The comp marks who
    // is talking with a coloured dot and nothing else, and
    // `specs/05-cliente-tui.md` forbids information carried by colour alone —
    // a dot is carried by shape alone too, which is the same failure wearing
    // the other hat. So the states that change something have to be a *word*.
    // Deleting the words leaves a screen that still looks right in a
    // screenshot and says nothing to anybody reading it without colour.
    //
    // Scoped to the two functions that build the rows, because the file as a
    // whole says all of these words either way: the paragraph explaining why
    // the word is there would satisfy an unscoped search for it.
    let lista = body_of(&scripts(), "function desenharCanais");
    let dentro = body_of(&scripts(), "function linhaDeQuemEstaDentro");

    assert!(
        lista.contains("cage.pilots") && lista.contains("linhaDeQuemEstaDentro"),
        "the Cage list no longer draws who is inside, so the one thing the v3 \
         added to this column is a block bar again"
    );
    assert!(
        dentro.contains("piloto.speaking") && dentro.contains("piloto.at_field"),
        "the row inside a Cage reads neither who is talking nor who is muted, \
         which is everything it was drawn to say"
    );
    for word in ["fala", "mudo"] {
        assert!(
            dentro.contains(&format!("\"{word}\"")),
            "the state `{word}` is drawn without a word beside the glyph, so it \
             reaches a monochrome screen — and a colour-blind reader — as a dot \
             that means nothing:\n{dentro}"
        );
    }
}

#[test]
fn entering_and_leaving_a_cage_are_labelled_buttons_and_not_a_click_on_the_row() {
    // What the v2 shipped: a `<li>` with `cursor: pointer` and one listener on
    // the `<ul>`. Nothing about it said it could be pressed, no keyboard could
    // reach it, and no screen reader announced it as anything at all. The LAN
    // test found the same defect one column over, on the `+` that was the only
    // way out of a Dogma — this is that finding applied here.
    //
    // Leaving is asserted beside entering on purpose. The comp writes
    // `VOCÊ ESTÁ AQUI` on the occupied Cage and wires it to nothing, and taking
    // that literally would trade a mute button for a dead one: this screen
    // would lose its only way out of a Cage, and gain a button that looks like
    // it acts.
    let handler = body_of(&scripts(), "async function alternarCanal");
    let lista = body_of(&scripts(), "function desenharCanais");

    assert!(
        handler.contains("button[data-cage]") && handler.contains("button[data-linha]"),
        "the channel handler is looking for something other than a button, so \
         whatever it finds is not focusable and announces as nothing:\n{handler}"
    );
    assert!(
        !handler.contains("closest(\"li\")"),
        "the channel handler is back to catching the row, which is the shape \
         that has no keyboard and no accessible name"
    );
    for label in ["ENTRAR NA JAULA", "SAIR DA JAULA"] {
        assert!(
            lista.contains(label),
            "the Cage no longer offers `{label}`, so one half of the pair the v3 \
             split apart has gone missing again"
        );
    }
    assert!(
        lista.contains("eject_plug") || handler.contains("eject_plug"),
        "nothing on this screen takes the plug out, and the only other way out \
         lives on a screen reached from here"
    );
}

#[test]
fn the_session_screen_omits_what_nothing_measures_rather_than_a_dash_per_row() {
    // The v2 rule was: draw the frame, leave the value visibly unmeasured, put
    // the reason in a `title`. It is the right rule where the absence answers a
    // question the screen just asked — the average with no plug in, the battery
    // bar, the alert's three cells — and all of those stay.
    //
    // It is the wrong rule *per row*. A dash beside every Line and two more
    // inside every pilot card is half a dozen explained em-dashes on a screen
    // whose entire purpose is being simple, each one asking to be read and none
    // of them answering anything. The v3 inverts it here: what has no data
    // leaves the screen.
    //
    // Guarded as "these two builders draw no dash", plus a control that the
    // helper is still in use elsewhere — otherwise deleting `naoMedido`
    // outright would satisfy this and quietly take the honest gaps with it.
    let script = scripts();
    let canais = body_of(&script, "function desenharCanais");
    let piloto = body_of(&script, "function linhaDoRoster");
    let media = body_of(&script, "function desenharMedia");

    for (name, body) in [("desenharCanais", &canais), ("linhaDoRoster", &piloto)] {
        assert!(
            !body.contains("naoMedido"),
            "`{name}` draws an unmeasured value once per row, which is the noise \
             the v3 took off this screen:\n{body}"
        );
    }
    assert!(
        media.contains("naoMedido"),
        "the Sync average no longer marks itself unmeasured when there is no \
         plug in — that gap is an answer to a question the panel just asked, and \
         it is the one this rule does not touch"
    );
}

#[test]
fn the_bound_name_is_stated_once_and_never_worn_as_a_badge() {
    // The v3 comp draws a `verif` seal per pilot and another per message. Both
    // are gone, and the reasoning is in §1.2 of its inventory: the CASPER binds
    // a nickname to the identity that claimed it first and the MELCHIOR refuses
    // any other (ADR 0017), so the seal would be true on every line forever — and
    // a badge everybody wears is a badge nobody learns to read, on the day one
    // of them is missing.
    //
    // What replaced it is one sentence. Two failure modes, and this catches
    // both: the sentence quietly disappearing in a later edit, and the seal
    // creeping back in per message.
    let page = without_comments(&read("ui/index.html"));
    let mensagens = body_of(&scripts(), "function desenharMensagens");

    let sentence = "ninguém consegue usar o nome de outra pessoa";
    assert_eq!(
        page.matches(sentence).count(),
        1,
        "the sentence about names being bound to keys is either gone from the \
         page or said more than once — it is worth exactly one telling, which is \
         the whole reason the per-line seal was dropped for it"
    );
    assert!(
        page.contains("class=\"dica operador-frase\""),
        "the sentence is no longer part of the simple-captions layer, so it \
         stays on screen for the people who turned that layer off — who are \
         precisely the people it was not written for"
    );
    assert!(
        !mensagens.contains("selo"),
        "a per-message seal is back beside the author, and it will read `true` \
         on every message this product can ever draw:\n{mensagens}"
    );
}

#[test]
fn the_search_starts_closed_and_opens_from_something_that_says_buscar() {
    // The search bar used to live open, spending 40px of the Line column on
    // every session for something done once an hour. The v3 puts a labelled
    // `BUSCAR` in the Line's header instead.
    //
    // Which creates one way to get this exactly wrong, and it is silent:
    // `focus()` on an input inside a `hidden` element does nothing and reports
    // nothing, so pressing `/` would simply stop working — with the field still
    // in the page, the listener still bound, and no error anywhere.
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let tag = tag_with_id(&page, "form-busca");
    assert!(
        tag.contains("hidden"),
        "the search bar starts open, so the header button that opens it is \
         either a no-op or a second way to do what is already done: <{tag}>"
    );
    assert!(
        script.contains("$(\"botao-buscar\")"),
        "nothing opens the search, so the bar the markup hides can never come back"
    );

    // Focusing the field is legal in exactly one place — after the bar has been
    // revealed — so the check is "nowhere else", with that one body cut out.
    let opener = body_of(&scripts(), "async function alternarBusca");
    let Some(revealed) = opener.find("barra.hidden =") else {
        panic!("`alternarBusca` no longer reveals the bar at all:\n{opener}");
    };
    let Some(focused) = opener.find(".focus()") else {
        panic!("`alternarBusca` reveals the bar and never puts the cursor in it");
    };
    assert!(
        revealed < focused,
        "`alternarBusca` focuses the field before revealing the bar, and \
         `focus()` inside a `hidden` element does nothing and reports nothing"
    );

    let elsewhere = script.replace(&opener, "");
    assert!(
        !elsewhere.contains("$(\"campo-busca\").focus()"),
        "something outside `alternarBusca` focuses the search field directly, so \
         it lands on a field inside a `hidden` element — which does nothing and \
         says nothing, and the `/` key just stops working"
    );
}

#[test]
fn the_add_dogma_button_promises_nothing_this_product_can_do() {
    // The v3 gives the `+` one meaning — add — and hands leaving to
    // `DESCONECTAR`, which is where the LAN test's finding put it. Adding is
    // still not a thing this product does: `Session` holds one `Plug` and
    // `connect` answers `AlreadyConnected` when there is one.
    //
    // So it is drawn and disabled, for the same reason as
    // `EJETAR PLUG DO OPERADOR` and `FORÇAR REINSERÇÃO DE PLUG` — and it needs
    // the same guard, because the tempting edit is to wire it to the entry
    // screen and call that "adding a Dogma", which is leaving with a friendlier
    // label. That is the exact conflation the v3 took apart.
    let page = read("ui/index.html");
    let tag = tag_with_id(&page, "trilha-adicionar");

    assert!(
        tag.contains("disabled"),
        "the `+` is pressable, and a second Dogma is a second Plug this product \
         does not have: <{tag}>"
    );
    assert!(
        tag.contains("title=\""),
        "the `+` is disabled and says nothing about why, which reads as a bug \
         rather than as a gap: <{tag}>"
    );
    assert!(
        tag.contains("aria-label=\""),
        "the `+` is a glyph with no accessible name, so it announces as `+`: <{tag}>"
    );
    assert!(
        !without_comments(&scripts()).contains("$(\"trilha-adicionar\")"),
        "a script reaches for the `+`, so the disabled button grew a listener — \
         and the only thing it could be wired to is leaving, which is the \
         conflation the v3 exists to undo"
    );
}

#[test]
fn no_two_screens_claim_the_same_class_name() {
    // The screens are one stylesheet each, loaded one after another into one
    // flat namespace — so two screens choosing the same name is not a clash the
    // browser reports, it is the later sheet quietly winning every tie.
    //
    // This is not hypothetical. `tela-boot.css` calls the polygon diagram
    // `.magi`; the session footer called its three lights `.magi` too, and it
    // loads second. The boot diagram — `position: relative`, absolutely placed
    // children — silently also got `display: flex; align-items: stretch`, and
    // worse, the footer's `.magi li` (one class, one type) outranked the
    // diagram's own `.magi-no` (one class) on the very `<li>`s it was meant to
    // paint. Nothing failed. The boot screen just came out wrong.
    //
    // A name that a *shared* sheet owns is a different thing and stays legal: a
    // screen writing `.ausente[title] { cursor: help }` is refining a primitive
    // from `base.css` on purpose, which is what the load order exists for.
    let shared: BTreeSet<String> = ["base.css", "acessibilidade.css"]
        .iter()
        .flat_map(|name| classes_defined_in(&read(&format!("ui/{name}"))))
        .collect();

    // Everything that is *not* one of the four sheets below, rather than
    // everything named `tela-*`. The prefix spelling had this guard blind to
    // `camada-alerta.css` and `camada-bateria.css` the day they landed — 464
    // lines of new CSS that could neither report a collision nor be reported
    // for one — and it was blind silently, which is the same failure the guard
    // exists to catch, one level up. A guard whose coverage depends on the next
    // author picking a blessed prefix is a guard with a hole in it.
    let not_a_screen = ["base.css", "acessibilidade.css", "tokens.css", "fontes.css"];
    let screens: Vec<String> = ui_files(".css")
        .into_iter()
        .filter(|name| !not_a_screen.contains(&name.as_str()))
        .collect();
    assert!(
        screens.len() >= 2,
        "there is nothing to collide: ui/ ships {} screen stylesheets",
        screens.len()
    );

    let mut owner: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut clashes: Vec<String> = Vec::new();
    for sheet in &screens {
        for class in classes_defined_in(&read(&format!("ui/{sheet}"))) {
            if shared.contains(&class) {
                continue;
            }
            match owner.get(&class) {
                Some(first) => clashes.push(format!(".{class} — {first} and {sheet}")),
                None => {
                    owner.insert(class, sheet.clone());
                }
            }
        }
    }

    assert!(
        clashes.is_empty(),
        "two screen stylesheets define the same class, and the one that loads \
         later wins every tie between them:\n{}",
        clashes.join("\n")
    );
}
