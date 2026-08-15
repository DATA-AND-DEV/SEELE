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
    for (at, sheet) in sheets.iter().enumerate() {
        if !sheet.starts_with("tela-") {
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

    let screens: Vec<String> = ui_files(".css")
        .into_iter()
        .filter(|name| name.starts_with("tela-"))
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
