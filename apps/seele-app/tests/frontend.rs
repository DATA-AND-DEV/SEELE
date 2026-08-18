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
///
/// Comments stripped first, for the reason `linked_assets` strips them: prose
/// about this boundary tends to quote the boundary. A doc comment explaining
/// that "every `invoke("…")` is tied to a registered command" made this helper
/// report a command literally named `…`, and the guard then accused the
/// frontend of calling something main.rs does not register — a true statement
/// about a call that does not exist.
fn invoked_commands(script: &str) -> BTreeSet<String> {
    let script = without_comments(script);
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

/// Commands whose Rust half is finished and whose screen is not drawn yet.
///
/// Empty is the resting state. An entry here is a promise that a screen is
/// coming, and it is meant to be deleted by whoever draws it — the assertions
/// below make sure it is: the moment the page calls one of these, this test
/// fails and says to take it off the list. Nothing rots quietly.
///
/// `renomear_cage` / `renomear_linha` are the other half of managing rooms.
/// Creating them is drawn — the session screen offers both forms to whoever
/// `Snapshot::may_manage_cages` says may create — and renaming is not: it was
/// not asked for, and a rename control is a different shape from a create one
/// (it belongs on the room, not under the list).
///
/// They stay here rather than being deleted, because deleting them would take
/// the verbs down with the only thing that remembers they exist.
///
/// The day the four moderation verbs were wired, these two were looked at again
/// and left. The reason is specific rather than a shrug, and it is about the
/// shape the control has to have: a rename belongs *on the room*, which means an
/// editable name in the row — and every row of `#lista-cages` and `#lista-linhas`
/// is thrown away and rebuilt by `desenharCanais` on every snapshot, twice a
/// second. A field there cannot hold a cursor for two frames, let alone a
/// selection. Making it possible means the channel column adopting the call
/// screen's repaint-don't-rebuild discipline, which changes the one list every
/// other screen reads.
///
/// The other shape — a rename dialog, like the moderation layer — does work
/// today, and was rejected on purpose: moderation needs a dialog because it
/// needs a surface wide enough to spell out a consequence before an irreversible
/// act. Renaming is reversible and trivial, so the dialog would be pure distance
/// between the person and the room they are renaming. Doing it badly now would
/// cost more than the wait.
///
const AGUARDANDO_TELA: &[&str] = &["renomear_cage", "renomear_linha"];

#[test]
fn no_command_is_registered_and_never_called() {
    // The other direction. A command nobody calls is either dead weight or a
    // feature that was wired on one side only — and the second is the one worth
    // catching.
    let called = invoked_commands(&scripts());
    let registered = registered_commands(&read("src/main.rs"));

    let unused: Vec<&String> = registered
        .difference(&called)
        .filter(|name| !AGUARDANDO_TELA.contains(&name.as_str()))
        .collect();
    assert!(
        unused.is_empty(),
        "main.rs registers commands the frontend never calls: {unused:?}\n\
         If one of these is deliberately waiting for a screen, say so in \
         AGUARDANDO_TELA rather than deleting this check."
    );
}

#[test]
fn nothing_waits_for_a_screen_that_already_draws_it() {
    // What keeps the exception above from becoming a hole. The list has to shrink
    // on its own, and the person who makes it shrink is the one who wires the
    // screen — they will be told here, by name, on the run where it starts
    // working.
    let called = invoked_commands(&scripts());
    let registered = registered_commands(&read("src/main.rs"));

    for esperando in AGUARDANDO_TELA {
        assert!(
            !called.contains(*esperando),
            "the page calls `{esperando}` now, so it is not waiting for a screen any more: \
             take it out of AGUARDANDO_TELA and let the check above cover it again"
        );
        assert!(
            registered.contains(*esperando),
            "AGUARDANDO_TELA names `{esperando}`, which main.rs does not register. \
             A waiting list that names nothing is a waiting list nobody is reading"
        );
    }
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
    let body = body_of(&scripts(), "function linhaDeDispositivo");

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

    // As duas palavras desceram para a função que marca **uma** lista, quando a
    // saída de som passou a ser escolhível e as duas listas passaram a ser
    // desenhadas pelo mesmo código. O que o teste exige não mudou.
    let uma = body_of(&scripts(), "function marcarUmaLista");
    assert!(
        uma.contains("EM USO") && uma.contains("ESCOLHIDO"),
        "the picker has one word for both states, so it cannot show them apart:\n{uma}"
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
fn the_button_with_no_command_behind_it_cannot_be_pressed_and_the_one_that_grew_one_asks_first() {
    // This guard used to cover two buttons and now covers one and a half, and
    // the rewrite *is* the record of what changed.
    //
    // `FORÇAR REINSERÇÃO DE PLUG` has not moved: there is still no "try now" in
    // this product. The core is already retrying — that is where the attempt
    // count above the button comes from — so a pressable button there would
    // promise to speed up something that is already running. The comp wires it
    // to a handler that closes the box, which is the worst of the available
    // readings: a button that looks like it acts, and does nothing.
    //
    // The other one moved. `EJETAR PLUG DO OPERADOR` was disabled because
    // `EndReason::Kicked` existed only for the person receiving one and nothing
    // could emit it; `expulsar_piloto`, `banir_piloto`, `remover_mensagem` and
    // `mover_piloto` are what changed that. So the question about it is no
    // longer "is it disabled" — it is "does it stay honest now that it does
    // something", and that has three halves:
    //
    // - it still *starts* disabled in the markup. Before the first snapshot this
    //   window does not know which permissions it has, and a button born
    //   pressable promises what it may not be able to carry out;
    // - something turns it on from the snapshot, and from the moderation
    //   booleans rather than from `may_manage_cages` or from nothing at all;
    // - and it opens the choosing, rather than acting. A `Notice` carries a
    //   severity and a reason and never *whose* it is — the same gap that leaves
    //   the three cells of that box empty — so a button that ejected from there
    //   would have to guess who, and guessing who leaves a session is the last
    //   thing a product should do.
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let forcar = tag_with_id(&page, "bateria-forcar");
    assert!(
        forcar.contains("disabled"),
        "`bateria-forcar` is pressable, and there is no command behind it — so it \
         is a button that looks like it acts and does nothing: <{forcar}>"
    );
    assert!(
        forcar.contains("title=\""),
        "`bateria-forcar` is disabled and says nothing about why, which reads as a \
         bug rather than as a gap: <{forcar}>"
    );
    assert!(
        !script.contains("$(\"bateria-forcar\")"),
        "a script reaches for `bateria-forcar`, so the disabled button grew a \
         listener — which is the comp's mistake, one layer down"
    );

    let ejetar = tag_with_id(&page, "alerta-ejetar");
    assert!(
        ejetar.contains("disabled"),
        "`alerta-ejetar` is born pressable. It is now a real control, but which \
         permissions this session has is not known until the first snapshot — so \
         between the window opening and that frame it would be promising \
         moderation nobody has: <{ejetar}>"
    );
    assert!(
        ejetar.contains("title=\""),
        "`alerta-ejetar` can still end up disabled — a Dogma that gave this pilot \
         no moderation at all — and a disabled control that says nothing about \
         why reads as a bug: <{ejetar}>"
    );

    // Scoped to the one function that owns the button's state, because the whole
    // script says all of these words either way — the paragraph explaining why
    // the alert box has no subject would satisfy an unscoped search for it.
    let porta = body_of(&scripts(), "function atualizarPortaDoAlerta");
    assert!(
        porta.contains("$(\"alerta-ejetar\")"),
        "nothing reaches for `alerta-ejetar`, so the button that finally has a \
         command behind it is never turned on:\n{porta}"
    );
    assert!(
        porta.contains("podeModerarPilotos") || porta.contains("may_kick"),
        "`alerta-ejetar` is enabled without asking whether this session may \
         moderate anybody, so it offers what the Dogma will refuse:\n{porta}"
    );
    assert!(
        porta.contains("title"),
        "`alerta-ejetar` can be left disabled without the reason being rewritten, \
         so the `title` in the markup outlives the state it explains:\n{porta}"
    );

    // And what the press does: open the choosing, never act.
    let Some(aperto) = script
        .split("$(\"alerta-ejetar\").addEventListener")
        .nth(1)
        .and_then(|rest| rest.split("\n});").next())
    else {
        panic!("nothing is listening on `alerta-ejetar` at all");
    };
    assert!(
        aperto.contains("abrirModeracao("),
        "`alerta-ejetar` does something other than open the moderation:{aperto}"
    );
    for verbo in [
        "invoke(\"expulsar_piloto\"",
        "invoke(\"banir_piloto\"",
        "invoke(\"mover_piloto\"",
    ] {
        assert!(
            !aperto.contains(verbo),
            "`alerta-ejetar` calls `{verbo}…` straight from the alert box, which \
             has no subject — so it is acting on a pilot it guessed:{aperto}"
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
    // Scoped to the two functions that make one card — the skeleton and the
    // values written over it — because the file as a whole says all of these
    // words either way: the paragraph explaining why the waveform is gone would
    // satisfy an unscoped search for it.
    let script = scripts();
    let body = format!(
        "{}\n{}",
        body_of(&script, "function cartaoDoPiloto"),
        body_of(&script, "function pintarCartao")
    );

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

    // The v3 answer to the second failure is stronger than the v2 one, and this
    // is where the two differ. v2 drew the waveform and the per-pilot delay as
    // empty frames with a dash and a `title` saying what was missing; v3 drops
    // them, because on a screen whose whole point is being easy to read an
    // explained dash is noise — somebody entering a Cage wants to know who is
    // talking, not which fields this protocol does not carry yet. The record of
    // the gap lives in the inventory (§1.3, §7), which is where anybody looks
    // before trying to draw them again.
    //
    // So: neither the value nor the frame. Drawing them from what *does* cross
    // would be worse than either — `input_level` is ours and scalar, `rtt_ms` is
    // ours and one number, and both would animate convincingly under somebody
    // else's name.
    for ours in ["input_level", "rtt_ms"] {
        assert!(
            !body.contains(ours),
            "the card draws a per-pilot value out of `{ours}`, which is this \
             machine's own measurement and one number"
        );
    }
    for empty in ["naoMedido", "SEM_DADO", "SEM_MEDIDA"] {
        assert!(
            !body.contains(empty),
            "the card draws an empty field marked `{empty}`. On this screen the \
             decision is to omit what has no data rather than frame it: a dash \
             with an explanation is one more thing to read on the screen that \
             exists for not having to read much"
        );
    }
}

#[test]
fn the_state_of_a_pilot_is_a_word_and_never_only_a_colour() {
    // `specs/05-cliente-tui.md` forbids information carried by colour alone, and
    // who is transmitting is information: the comp marks it with an orange halo
    // around the card and nothing else. A halo is invisible to anybody who does
    // not see the hue, and this card carries three facts that way — the
    // microphone, the voice, and the ears.
    //
    // The v3 answer is two registers of the same fact, and this guard is that
    // both exist: the chip, in one word, and the plain sentence beside the name,
    // which is what `LEGENDAS SIMPLES` is for.
    let script = scripts();
    let paint = body_of(&script, "function pintarCartao");
    let sentence = body_of(&script, "function fraseDoEstado");

    // The chip, and what it says. Reaching for the element is not enough — an
    // empty chip is a card marked by paint alone, and it looks fine in a
    // screenshot taken by somebody who sees the orange. So the statement that
    // fills it has to branch on the microphone and on the voice, and every
    // branch has to be a word.
    let chips: Vec<&str> = paint
        .split(';')
        .filter(|statement| statement.contains("pastilha") && statement.contains("textContent"))
        .collect();
    assert!(
        !chips.is_empty(),
        "the card writes no state chip, so the halo is the only thing marking who \
         is speaking"
    );
    // A statement that tells the three states apart in words: it has to look at
    // the microphone and at the voice, and have a word for each way out.
    let tells_them_apart = |statement: &str| {
        statement.contains("at_field")
            && statement.contains("speaking")
            && statement
                .split('"')
                .skip(1)
                .step_by(2)
                .filter(|word| !word.trim().is_empty())
                .count()
                >= 3
    };

    // Either the chip is written with the words in place, or it is written from
    // a local that carries them. Following the local matters: assigning `""` to
    // the chip while a perfectly good ternary sits unused above it is the exact
    // shape of "marked by paint alone", and it would read as fine in a diff.
    let straight = chips.iter().copied().any(tells_them_apart);
    let through_a_local = paint.split(';').any(|declaration| {
        tells_them_apart(declaration)
            && declaration
                .split('=')
                .next()
                .and_then(|left| left.split_whitespace().last())
                .is_some_and(|name| {
                    chips.iter().any(|chip| {
                        chip.split('=')
                            .nth(1)
                            .is_some_and(|value| value.contains(name))
                    })
                })
    });
    assert!(
        straight || through_a_local,
        "no state chip on this card is written from a value that branches on both \
         the microphone and the voice with a word for each. Either a state is \
         being told apart by paint alone, or two of them come out reading the \
         same:\n{}",
        chips.join("\n")
    );

    assert!(
        paint.contains("fraseDoEstado"),
        "the card no longer writes the state as a sentence beside the name, which \
         is the half a newcomer reads"
    );

    // All three facts, in the sentence. The colour version of any one of them
    // would be a fact only some readers get.
    for fact in ["at_field", "speaking", "total_isolation"] {
        assert!(
            sentence.contains(fact),
            "the sentence beside the name never mentions `{fact}`, so that state \
             reaches the screen as paint and nothing else"
        );
    }
}

#[test]
fn the_volume_control_does_not_hide_behind_the_pointer() {
    // The defect this screen exists to fix, in one rule. `tela-sessao.css` gives
    // the per-pilot slider `opacity: 0` and reveals it on `:hover`, which is the
    // definition of a hidden control: it is not on the path of anybody using a
    // keyboard, it never appears under touch, and whoever did not sweep the
    // pointer across that row by accident never learned it was there.
    //
    // Two halves, because either alone passes with the defect present. The
    // control has to *exist* — minus, plus, and cells that are real buttons —
    // and no rule of this sheet may use the pointer to bring anything into
    // being. Highlighting on hover stays legal; that is what `background` and
    // `border-color` in those rules are.
    let script = scripts();
    // Comments stripped, and that is load-bearing rather than tidy: the sheet
    // has to be able to write down *why* nothing here hangs off `:hover`, and
    // the paragraph saying so names the word. A guard a comment can trip is as
    // broken as a guard a comment can satisfy.
    let sheet = without_comments(&read("ui/tela-chamada.css"));

    let build = body_of(&script, "function controleDeVolume");
    // U+2212, which the embedded face does have — the inventory measured it
    // (§5). The ASCII hyphen is a different character and a different width in a
    // monospaced face, beside a `+` that is ASCII.
    assert!(
        build.contains('\u{2212}') && build.contains('+'),
        "the volume control no longer offers a minus and a plus"
    );
    assert!(
        build.contains("CELULAS_DE_VOLUME"),
        "the volume control no longer draws a row of cells to click"
    );
    assert!(
        build
            .split(';')
            .any(|statement| statement.contains("elemento(\"button\"")
                && statement.contains("vol-cela")),
        "the volume cells are not buttons, so they are neither clickable nor \
         reachable by keyboard — which is the hidden control again, wearing the \
         shape of the fix"
    );
    assert!(
        scripts().contains("set_volume"),
        "nothing sends the chosen volume anywhere"
    );

    for rule in sheet.split('}') {
        let Some((selector, declarations)) = rule.split_once('{') else {
            continue;
        };
        if !selector.contains(":hover") {
            continue;
        }
        for reveal in ["opacity", "visibility", "display"] {
            assert!(
                !declarations.contains(reveal),
                "`{}` uses the pointer to decide whether something exists \
                 (`{reveal}`), which is the hidden control this screen was \
                 redrawn to fix — hover may highlight, never reveal",
                selector.trim()
            );
        }
    }
}

#[test]
fn the_two_ways_out_of_the_call_say_which_one_leaves_the_cage() {
    // The v3 comp's finding, and the inventory settles it in §7.1: changing
    // screen is not leaving the Cage. The prototype collapses the two — both
    // buttons call `ir('principal')` — and what separates them there is only
    // what they promise. Here they have to differ for real, and say so.
    //
    // The failure without this guard is the one the LAN test found: somebody
    // presses the button that goes back to the Lines and cannot tell whether
    // they are still being heard.
    let page = without_comments(&read("ui/index.html"));
    let script = scripts();

    // `VER LINHAS` is navigation. Whatever it runs must not pull the plug.
    let leaving = body_of(&script, "function fecharChamada");
    assert!(
        !leaving.contains("eject_plug"),
        "`VER LINHAS` ejects the plug, so the two buttons do the same thing again \
         and the screen's own words are wrong"
    );

    // `SAIR DA JAULA` is the eject, and nothing else on this screen is.
    let Some(exit) = script
        .split("$(\"chamada-ejetar\").addEventListener")
        .nth(1)
        .and_then(|rest| rest.split("\n});").next())
    else {
        panic!("nothing is listening on `chamada-ejetar` at all");
    };
    // The call, and not the word. The handler names `eject_plug` in the string
    // it logs a failure with, so a `contains` on the bare name stays green with
    // the command itself deleted — which is exactly the state where the red
    // button looks like it leaves and does not. Found by breaking it on purpose
    // and watching this pass.
    assert!(
        exit.contains("invoke(\"eject_plug\")"),
        "`SAIR DA JAULA` does not eject the plug, so leaving the Cage has no \
         button anywhere on this screen:{exit}"
    );

    // And both have to say, in the markup and beside themselves, what they do.
    let hint_of = |id: &str| {
        let Some(button) = page
            .split(&format!("id=\"{id}\""))
            .nth(1)
            .and_then(|rest| rest.split("</button>").next())
        else {
            panic!("index.html has no `{id}` button");
        };
        let Some(hint) = button
            .split("class=\"dica\">")
            .nth(1)
            .and_then(|rest| rest.split('<').next())
        else {
            panic!(
                "`{id}` carries no hint beside it, so the distinction between \
                 changing screen and leaving the Cage is nowhere on the screen"
            );
        };
        hint.split_whitespace().collect::<Vec<_>>().join(" ")
    };

    let back = hint_of("chamada-voltar");
    let out = hint_of("chamada-ejetar");
    assert!(
        !back.is_empty() && !out.is_empty(),
        "one of the two exits explains itself with an empty line"
    );
    assert_ne!(
        back, out,
        "both exits are explained with the same sentence, which is the prototype's \
         collapse written out in words"
    );
}

#[test]
fn no_event_in_the_call_monitor_is_older_than_the_window() {
    // The comp fills `EVENTOS` with five lines of history — `IKARI.S entrou`,
    // `HORAKI.H saiu` — and the inventory left open how much of that the Dogma
    // keeps. It keeps none: `Event::RosterChanged` says the roster changed and
    // never what changed in it, and there is no record of arrivals, departures
    // or A.T. Field anywhere in the core.
    //
    // So the list may only carry what this window watched go by, and the
    // tempting way to make it look full is a seeded line in the markup that
    // nobody ever measured. An empty list under a heading that explains why is
    // the honest version, and this is what keeps it that way.
    let page = without_comments(&read("ui/index.html"));

    let Some(list) = page
        .split("id=\"chamada-eventos\"")
        .nth(1)
        .and_then(|rest| rest.split("</ol>").next())
    else {
        panic!("index.html no longer has the events list");
    };
    assert!(
        !list.contains("<li"),
        "the events list ships with lines already in it, and this product has no \
         history to have taken them from:{list}"
    );
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
fn the_hint_layer_is_not_painted_in_the_colour_reserved_for_large_text() {
    // The hint layer explains the app to somebody who does not know it, and it
    // is on by default. Painting it in `osso-apagado` — which `tokens.css`
    // annotates in its own line as "4,11:1 só texto grande" — writes the
    // explanation in type the explained person cannot read.
    //
    // `docs/tokens-achados.md` settled this before the layer existed: the
    // pending choice on that colour is to raise it *or* to make sure nothing
    // necessary depends on it alone. A hint is necessary by definition.
    //
    // The check is on the token name and not on a computed ratio, deliberately.
    // A ratio computed here would have to be recomputed the day the palette
    // moves, and would then measure a number nobody had decided; the name is
    // what the decision was actually about.
    let sheet = read("ui/base.css");
    let Some(after) = sheet.split("\n.dica {").nth(1) else {
        panic!("base.css no longer declares `.dica`, so the hint layer has no owner");
    };
    let Some(rule) = after.split('}').next() else {
        panic!("the `.dica` rule is never closed");
    };

    for dim in ["osso-apagado", "rotulo-painel"] {
        assert!(
            !rule.contains(dim),
            "`.dica` is painted with `{dim}`, which tokens.css marks as large-text \
             only at 4,11:1. This layer is small, prose, and on by default — it is \
             the one thing on screen that has to be legible to somebody who does \
             not already know the app.\n{rule}"
        );
    }
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

// ---------------------------------------------------------------------------
// The Terminal Dogma — the settings screen, rebuilt against the v3 comp.
// ---------------------------------------------------------------------------

/// The markup of one screen, cut out of the page by the id of the next one.
///
/// Cut by the *following* screen and not by `</section>`, because the settings
/// screen nests four `<section>`s of its own — one panel per section button —
/// and the first closing tag would end the slice a third of the way in.
fn screen_markup(page: &str, id: &str, next_id: &str) -> String {
    let page = without_comments(page);
    let Some(after) = page.split(&format!("id=\"{id}\"")).nth(1) else {
        panic!("index.html has no screen with id `{id}`");
    };
    let Some(body) = after.split(&format!("id=\"{next_id}\"")).next() else {
        panic!("`{next_id}` no longer follows `{id}`, so this slice has no end");
    };
    body.to_owned()
}

/// Every `data-glifo="…"` in the page, in document order.
fn drawn_glyphs(page: &str) -> Vec<String> {
    without_comments(page)
        .split("data-glifo=\"")
        .skip(1)
        .filter_map(|piece| piece.split('"').next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_glyph_the_page_asks_for_is_one_glifos_js_can_draw() {
    // `glifo()` throws on an unknown name, and the loop at the bottom of
    // `glifos.js` runs over every `[data-glifo]` in the page — so one typo
    // anywhere takes the *whole* script file down with it, and every listener
    // registered after it is never attached. The window then renders, looks
    // right, and does nothing when clicked.
    //
    // This is the guard the v3 comp made worth having: the settings screen alone
    // asks for four of the eight new drawings by name, in markup, with nothing
    // between the attribute and the runtime.
    let page = read("ui/index.html");
    let script = read("ui/glifos.js");

    let Some(table) = script.split("const GLIFOS = {").nth(1) else {
        panic!("glifos.js no longer declares `GLIFOS`, and nothing can be drawn");
    };
    let Some(table) = table.split("\n};").next() else {
        panic!("`GLIFOS` is never closed");
    };
    let table = without_comments(table);

    let asked = drawn_glyphs(&page);
    assert!(!asked.is_empty(), "the page draws no glyph at all");

    for name in asked {
        assert!(
            table.contains(&format!("{name}: [")),
            "index.html asks for the glyph `{name}`, which `glifos.js` cannot \
             draw. `glifo()` throws, and it throws inside the loop at the bottom \
             of that file — so every listener registered after it is never \
             attached, and the window comes out looking right and doing nothing."
        );
    }
}

#[test]
fn every_section_of_the_settings_screen_carries_the_panel_and_the_heading_it_opens() {
    // The four sections are buttons in markup and the heading strings travel
    // *with* them, on `data-titulo` and `data-sub`, rather than in a table in
    // JavaScript. That is the whole reason this can be checked at all — and the
    // failure it catches is the one a fifth section would arrive with: a button
    // whose `data-painel` names a panel that is not in the page, which reads as
    // a section that opens nothing and blanks the heading on the way.
    let page = read("ui/index.html");
    let dogma = screen_markup(&page, "tela-dogma", "tela-fim");

    let mut sections = Vec::new();
    for rest in dogma.split("<button ").skip(1) {
        let Some(end) = rest.find('>') else { continue };
        let tag = &rest[..end];
        if attribute(tag, "class").as_deref() != Some("dogma-secao") {
            continue;
        }
        let Some(id) = attribute(tag, "id") else {
            panic!("a section button has no id, so no script can mark it: <{tag}>");
        };
        for name in ["data-painel", "data-titulo", "data-sub"] {
            assert!(
                attribute(tag, name).is_some(),
                "the section `{id}` carries no `{name}`, so opening it leaves the \
                 heading of the panel empty: <{tag}>"
            );
        }
        let panel = attribute(tag, "data-painel").unwrap_or_default();
        assert!(
            page.contains(&format!("id=\"{panel}\"")),
            "the section `{id}` opens `{panel}`, which is not in the page"
        );
        sections.push(id);
    }

    assert_eq!(
        sections,
        [
            "secao-audio",
            "secao-atalhos",
            "secao-aparencia",
            "secao-identidade",
            // The fifth is not the comp's — it predates the update button
            // existing at all (ADR 0026). It lands here because what this screen
            // adjusts is *this machine*, and which SEELE is installed on it is
            // the most machine-local fact there is; and last because it is the
            // one section nobody opens on a normal day.
            "secao-atualizacao",
        ],
        "the settings screen is the four sections of the v3 comp plus the update \
         one, in this order"
    );
}

#[test]
fn every_key_the_shortcut_table_names_is_one_a_script_listens_for() {
    // The shortcuts section is a *list of what the keys are*, because they are
    // fixed: there is no editable table and nowhere to save a rebinding. A list
    // like that has exactly one way to fail, and it fails silently — the key it
    // names stops being the key that acts, and the screen goes on documenting a
    // program that no longer exists. Nothing about that is visible from the
    // page, from the script, or from a running window.
    //
    // `data-tecla` carries the name the *browser* gives the key, not the word a
    // person reads, so this compares the row against the listener rather than
    // against another label.
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let mut keys = Vec::new();
    for piece in without_comments(&page).split("data-tecla=\"").skip(1) {
        let Some(key) = piece.split('"').next() else {
            continue;
        };
        keys.push(key.to_owned());
    }

    assert!(
        keys.len() >= 4,
        "the shortcuts section lists {} keys; it is documentation, and an empty \
         list of shortcuts is the same as not having the section",
        keys.len()
    );

    for key in keys {
        assert!(
            script.contains(&format!("\"{key}\"")),
            "the shortcuts section says `{key}` does something, and no script \
             listens for it — so the screen documents a program this is not"
        );
    }
}

#[test]
fn the_settings_screen_omits_what_the_product_lacks_instead_of_drawing_it_dead() {
    // This screen deliberately inverts the convention the auth screen follows.
    // There, the frame stays and the value is missing, because the gap is the
    // protocol's and worth showing. Here it does not: on a screen whose entire
    // purpose is being simple, half a dozen greyed-out controls with an
    // explanation beside each is noise, and every one of them is a promise.
    //
    // Each word below is a control the v3 comp draws and this product cannot
    // carry out. Two of them would take a written decision back:
    //
    // - `RUÍDO` — ADR 0007 kept C/C++ DSP out of v1 and made headphones a
    //   documented requirement. The control would exist to do nothing.
    // - `TEMA` — ADR 0014 freezes the palette, and a second theme is a second
    //   canonical palette.
    //
    // And `SALVAR`/`DESCARTAR` are the third: the choice applies now, so there
    // is nothing pending for a button to confirm (comp inventory §8.1). A
    // `SALVAR` on an audio panel promises that nothing changes until it is
    // pressed, which is false for sound — you have to hear the effect to know
    // you chose right.
    //
    // Comments are stripped, and that is the point: the paragraphs above the
    // markup have to be able to say *why* each of these is absent.
    let page = read("ui/index.html");
    let dogma = screen_markup(&page, "tela-dogma", "tela-fim");

    for absent in [
        "SALVAR",
        "DESCARTAR",
        "RUÍDO",
        "TEMA",
        "GANHO",
        "VOLUME",
        "GERAR",
        "COPIAR",
    ] {
        assert!(
            !names(&dogma, absent),
            "the settings screen draws `{absent}`, which nothing in this product \
             can carry out. The rule here is to omit, not to draw it dead — and \
             two of these would take an ADR back."
        );
    }
}

#[test]
fn both_sides_of_the_audio_picker_are_drawn_by_the_same_code() {
    // The output row used to be one dead control with a `title` explaining that
    // the machine could not enumerate speakers. It can now, so that guard was
    // retired — a disabled row asserting a limitation that no longer exists is
    // a test that keeps a bug alive.
    //
    // What replaces it is the risk the wiring actually introduced. Input and
    // output are picked in exactly the same dance — list, read the choice,
    // choose, and show what *opened* rather than what was asked for — and the
    // cheapest way to add the second one is to copy the first. Two copies drift
    // on the first fix somebody makes to one side only, and the drift is
    // invisible: both lists keep drawing, one of them just stops telling the
    // truth about which device is open.
    let script = without_comments(&scripts());

    assert!(
        !script.contains("desenharMicrofones") && !script.contains("linhaDeMicrofone"),
        "a capture-only drawing function is back, which is how the two sides start \
         to differ"
    );

    let tabela = body_of(&scripts(), "const LADOS");
    for comando in [
        "microfones",
        "saidas",
        "microfone_escolhido",
        "saida_escolhida",
        "escolher_microfone",
        "escolher_saida",
    ] {
        assert!(
            tabela.contains(comando),
            "`{comando}` is not in the LADOS table, so one side of the picker is \
             wired somewhere else and can be changed without the other:\n{tabela}"
        );
    }

    // The whole point of the screen: what was chosen and what opened are two
    // different questions, and both sides have to answer the second one.
    let marcar = body_of(&scripts(), "function marcarLinhas");
    for campo in ["capture", "playback"] {
        assert!(
            marcar.contains(campo),
            "`marcarLinhas` never reads `snapshot.{campo}`, so that side draws the \
             preference and calls it reality:\n{marcar}"
        );
    }

    // And nothing may seed rows in the markup: a hard-coded device is one this
    // machine may not have.
    let page = without_comments(&read("ui/index.html"));
    let Some(after) = page.split("id=\"lista-saidas\"").nth(1) else {
        panic!("the output list is gone, and with it the only place the enumeration can be drawn");
    };
    let Some(lista) = after.split("</ul>").next() else {
        panic!("the output list is never closed");
    };
    assert!(
        !lista.contains("<li"),
        "the output list ships a row in the markup, which names a device before \
         anybody asked this machine what it has:{lista}"
    );
}

#[test]
fn the_switch_that_hides_the_captions_never_hides_its_own_caption() {
    // One line in the whole screen writes its description as `.dogma-chave-desc`
    // instead of `.dica`, and it is the one that governs `.dica` itself.
    //
    // Written as a hint, the sentence explaining what simple captions are would
    // disappear at the exact moment somebody turned them off — leaving an
    // unlabelled SIM/NÃO pair as the only way back, on the screen a person just
    // arrived at because they could not work the interface out. Nothing fails:
    // the page loads, the switch works, and the way back is invisible.
    let page = without_comments(&read("ui/index.html"));

    let Some(after) = page.split("class=\"dogma-chave\"").nth(1) else {
        panic!("the behaviour switch is gone from the settings screen");
    };
    let Some(row) = after.split("</li>").next() else {
        panic!("the behaviour switch row is never closed");
    };

    assert!(
        row.contains("dogma-chave-desc"),
        "the captions switch has no always-visible description:\n{row}"
    );
    assert!(
        !row.contains("class=\"dica\""),
        "the captions switch explains itself with a `.dica`, which is the very \
         thing it turns off — so the way back disappears with it:\n{row}"
    );
    assert!(
        row.contains("role=\"switch\"") && row.contains("aria-checked"),
        "the switch carries no state anybody who cannot see the fill can read:\n{row}"
    );
}

/// The names a script declares at its top level.
///
/// Column zero and nothing else: anything indented is inside a function and
/// belongs to that function.
fn globals_declared_in(script: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for line in without_comments(script).lines() {
        let Some(rest) = ["const ", "let ", "var ", "function ", "class "]
            .iter()
            .find_map(|keyword| line.strip_prefix(keyword))
        else {
            continue;
        };
        // `const { invoke } = window.__TAURI__.core` binds through a pattern
        // rather than a name, and this does not try to read patterns.
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if !name.is_empty() {
            found.insert(name);
        }
    }
    found
}

#[test]
fn no_two_scripts_declare_the_same_top_level_name() {
    // The same hazard as the stylesheet check above, on the other half of the
    // split, and the failures are worse. ADR 0019 chose no modules, so the nine
    // scripts share one scope — and what changes when a file is split is not
    // visibility, it is what happens when two of them pick one name:
    //
    // - two top-level `const`s or `let`s with one name is a `SyntaxError`, and
    //   it kills the *whole* second script. Every listener it was going to
    //   register is never registered, so a screen's buttons simply do nothing.
    //   Nothing else in this suite would notice: the page loads, the markup is
    //   there, and only the console says why.
    // - two `function`s with one name is silent. The one that loads later wins
    //   every call, including the calls made from the file that declared the
    //   other one — which is `.magi` in `no_two_screens_claim_the_same_class_name`,
    //   one layer down and harder to see.
    //
    // Sharing on purpose stays legal and is how this frontend works: a screen
    // *reads* `medido`, `blocos`, `volumes` and `comecoDaSessao` from the file
    // that declares them. What it may not do is declare them again.
    let mut owner: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut clashes: Vec<String> = Vec::new();

    for name in ui_files(".js") {
        for global in globals_declared_in(&read(&format!("ui/{name}"))) {
            match owner.get(&global) {
                Some(first) => clashes.push(format!("`{global}` — {first} and {name}")),
                None => {
                    owner.insert(global, name.clone());
                }
            }
        }
    }

    assert!(!owner.is_empty(), "no script declares anything at all");
    assert!(
        clashes.is_empty(),
        "two scripts declare the same top-level name into the one scope they \
         share. For `const` and `let` that is a SyntaxError that takes the whole \
         second file down, listeners included; for `function` it is silent, and \
         the later file wins every call:\n{}",
        clashes.join("\n")
    );
}

#[test]
fn the_nickname_field_remembers_what_it_was_called_last_time() {
    // The name was being saved the whole time — `Conhecido` carries `apelido`
    // and `Conhecidos::listar` sorts newest first — and the entry screen simply
    // never read it, so every launch went back to the literal `piloto` written
    // in the markup. Reported from a real session, and it is the kind of thing
    // no static check would have found: the field had a value, it was just the
    // wrong one.
    let body = body_of(&scripts(), "async function desenharVisitados");

    assert!(
        body.contains("campo-apelido"),
        "the entry screen draws the visited list without ever reading the name it \
         records, so the field goes back to its markup default every launch"
    );
    assert!(
        body.contains("defaultValue"),
        "the field is filled unconditionally, which overwrites whatever the person \
         had already typed. `defaultValue` is the DOM answering «is this still \
         what the markup said?»:\n{body}"
    );
}

#[test]
fn a_failure_this_screen_cannot_name_still_says_what_it_was() {
    // `FRASES` is a list of hand-written sentences, and the Rust side keeps
    // growing error variants — three landed today alone. Every variant that
    // arrives without a sentence used to reach a dead end that read «FALHA
    // DESCONHECIDA», which tells the person nothing and tells whoever has to fix
    // it less: somebody reporting «I cannot reconnect» had nothing else to pass
    // on.
    let script = without_comments(&scripts());

    assert!(
        !script.contains("\"FALHA DESCONHECIDA\""),
        "an unnamed failure is still drawn as a dead end, so the one screen that \
         sees the error keeps it to itself"
    );

    let body = body_of(&scripts(), "function desconhecida");
    assert!(
        body.contains("JSON.stringify") && body.contains("erro"),
        "the fallback names no detail, so it is the dead end under another \
         wording:\n{body}"
    );
}

#[test]
fn the_three_subsystems_look_different_while_they_are_loading() {
    // `subsistemas("carga", "…")` writes `data-estado="carga"` on all three
    // MAGI nodes while the connection is happening, and for a long time no rule
    // matched it: the attribute changed, the screen did not. Loading looked
    // exactly like idle, which is the one thing a loading state must not do.
    //
    // The check is on the rule existing and on the state surviving without
    // motion — not on the animation itself. Somebody who turns motion off is
    // still owed the difference between "waiting" and "not started".
    let sheet = read("ui/tela-boot.css");
    let Some(after) = sheet.split("[data-estado=\"carga\"]").nth(1) else {
        panic!(
            "no rule paints the loading state, so the three subsystems look the same \
             whether or not anything is happening"
        );
    };
    let Some(rule) = after.split('}').next() else {
        panic!("the loading rule is never closed");
    };

    assert!(
        rule.contains("background") || rule.contains("color"),
        "the loading rule changes no paint, so it is a selector that does \
         nothing:\n{rule}"
    );

    // The state has to be readable with the animation gone. `acessibilidade.css`
    // kills every animation under `prefers-reduced-motion`, so anything carried
    // *only* by the keyframes disappears for those readers.
    assert!(
        !rule.trim_start().starts_with("animation"),
        "the loading state is carried by the animation before it is carried by \
         the paint, so it vanishes entirely under prefers-reduced-motion:\n{rule}"
    );

    // And the word still has to say it, for anybody reading with no colour at
    // all — `specs/05-cliente-tui.md`.
    let script = without_comments(&scripts());
    assert!(
        script.contains("subsistemas(\"carga\""),
        "nothing puts the three subsystems into the loading state any more"
    );
}

#[test]
fn creating_a_room_is_offered_by_permission_and_sized_by_the_dogma() {
    let body = body_of(&scripts(), "function desenharCanais");

    // Offered, not enforced. The server refuses `CreateCage` from anybody
    // without `ManageCages`, and `seele-conformance` proves the refusal comes
    // from there — this is the shell not putting up a control that would fail.
    // The distinction matters because the opposite reading (hide it and call it
    // secured) is the one the `plug` walks straight through.
    assert!(
        body.contains("may_manage_cages"),
        "the screen offers the create forms without asking whether this pilot may \
         create, so it either hides them from the host or shows them to everybody"
    );

    // The size of a new room is the Dogma's answer, not a number typed in here.
    // Whoever hosts already chose one when they set the Dogma up, and repeating
    // their choice beats inventing a default in JavaScript.
    assert!(
        body.contains("cages[0].limit") || body.contains("limit"),
        "the default seat count no longer comes from a room that already exists, \
         so the shell is deciding how big a room should be:\n{body}"
    );

    // And the two commands have to be reached by their written names, or the
    // guard that ties calls to registered commands goes blind — which it did,
    // twice in one day, in this very file and in the settings screen.
    let script = without_comments(&scripts());
    for comando in ["invoke(\"criar_cage\"", "invoke(\"criar_linha\""] {
        assert!(
            script.contains(comando),
            "`{comando}…` is not written out anywhere, so the command name reaches \
             `invoke` through a variable and no static check can follow it"
        );
    }
}

// ---------------------------------------------------------------------------
// Reaching the bottom of a list.
// ---------------------------------------------------------------------------

/// Elements that carry no closing tag, so they never open a level.
const VOID_TAGS: &[&str] = &["meta", "link", "img", "input", "br", "hr", "source"];

/// Every `<ul>`/`<ol>` the page leaves **empty**, with the classes of every
/// element enclosing it — outermost first, the list's own classes last.
///
/// Emptiness is the whole test, and it is the narrow thing that makes this
/// guard true rather than merely broad. A list written out in `index.html` has
/// as many rows as the page has: `.dogma-atalhos` is four shortcuts, `.luzes` is
/// three subsystems, and no Dogma can make either longer. A list the page leaves
/// empty is one a script fills from a `Snapshot`, and nothing in the protocol
/// caps how many Cages, Linhas, pilots, messages, devices or visited Dogmas come
/// back. Those are the ones that can outgrow the window.
///
/// So the distinction is not "long" against "short" — nobody can measure that
/// from the source — it is *who decides the length*. The page, or the Dogma.
///
/// It also cannot be quietly silenced. Making this guard shut up means typing
/// rows into a list a script is about to replace wholesale, which is a change
/// the next reader of the diff would ask about.
fn empty_lists_with_their_ancestry(page: &str) -> Vec<(String, Vec<String>)> {
    let page = without_comments(page);
    let mut open: Vec<String> = Vec::new();
    let mut found: Vec<(String, Vec<String>)> = Vec::new();
    let mut rest = page.as_str();

    while let Some(lt) = rest.find('<') {
        let after = &rest[lt + 1..];
        let Some(gt) = after.find('>') else { break };
        let tag = &after[..gt];
        let body = &after[gt + 1..];
        rest = body;

        // `<!doctype …>` and its kind open nothing.
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        if tag.starts_with('/') {
            open.pop();
            continue;
        }

        let name: String = tag
            .split(|c: char| c.is_whitespace())
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let classes = attribute(tag, "class").unwrap_or_default();

        if name == "ul" || name == "ol" {
            let text = body.split('<').next().unwrap_or_default();
            let closes = body[text.len()..].starts_with(&format!("</{name}"));
            if text.trim().is_empty() && closes {
                let Some(id) = attribute(tag, "id") else {
                    panic!("an empty <{name}> carries no id, so nothing can fill it: <{tag}>");
                };
                let mut chain = open.clone();
                chain.push(classes.clone());
                found.push((id, chain));
            }
        }

        if VOID_TAGS.contains(&name.as_str()) || tag.ends_with('/') {
            continue;
        }
        open.push(classes);
    }

    // The walk above is only worth trusting if it comes out level. An unbalanced
    // page — or a `>` inside an attribute value — would leave a residue here,
    // and every ancestry this function reported would be wrong by that much.
    assert!(
        open.is_empty(),
        "the page does not close everything it opens, so this walk cannot say \
         what encloses what; {} level(s) left open",
        open.len()
    );
    found
}

/// Class names whose rule makes the element a scroll container.
fn classes_that_scroll(css: &str) -> BTreeSet<String> {
    let css = without_comments(css);
    let mut found = BTreeSet::new();
    for block in css.split('}') {
        let Some((selector, declarations)) = block.split_once('{') else {
            continue;
        };
        let scrolls = declarations.split(';').any(|declaration| {
            let Some((property, value)) = declaration.split_once(':') else {
                return false;
            };
            matches!(property.trim(), "overflow" | "overflow-y")
                && (value.contains("auto") || value.contains("scroll"))
        });
        if !scrolls {
            continue;
        }
        for piece in selector.split('.').skip(1) {
            let name: String = piece
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !name.is_empty() {
                found.insert(name);
            }
        }
    }
    found
}

#[test]
fn every_list_the_dogma_fills_lives_inside_something_that_scrolls() {
    // How this was found: somebody with more than a screenful of Cages asked how
    // to see the rest. There was no way. `.canais` was `flex: 0 0 auto`, no panel
    // in the channel column declared `overflow-y`, and `base.css` puts
    // `overflow: hidden` on `body` — so the column grew past the window and the
    // window would not scroll behind it either. The rooms at the bottom did not
    // exist.
    //
    // The failure is silent twice over: nothing errors, and with the four rooms
    // a test Dogma has, nothing looks wrong.
    let page = read("ui/index.html");
    let scrolls = classes_that_scroll(&styles());
    assert!(
        scrolls.len() >= 3,
        "no stylesheet declares a scroll container any more, so this guard is \
         asserting against an empty set: {scrolls:?}"
    );

    let lists = empty_lists_with_their_ancestry(&page);
    assert!(
        lists.len() >= 8,
        "found {} lists the Dogma fills; the session alone has Cages, Linhas, \
         the messages and the roster",
        lists.len()
    );

    let mut trapped: Vec<String> = Vec::new();
    for (id, ancestry) in lists {
        let inside_a_scroller = ancestry
            .iter()
            .flat_map(|classes| classes.split_whitespace())
            .any(|class| scrolls.contains(class));
        if !inside_a_scroller {
            trapped.push(format!("#{id} — inside: {}", ancestry.join(" / ")));
        }
    }

    assert!(
        trapped.is_empty(),
        "these lists are filled from the Dogma, so nothing caps how long they \
         get, and neither they nor anything enclosing them scrolls — past the \
         bottom of the window their rows stop existing:\n{}",
        trapped.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Carrying the keyboard across a change of screen.
// ---------------------------------------------------------------------------

/// The screens, as (id, opening tag, markup up to the next screen).
///
/// Cut on `<section id="tela-` rather than on `</section>`, because the settings
/// screen nests four `<section>`s of its own and the first closing tag would end
/// that slice a third of the way in. `split` hands back exactly the text between
/// two screens, which is what "inside this screen" has to mean here.
fn screens_of(page: &str) -> Vec<(String, String, String)> {
    let page = without_comments(page);
    let mut found = Vec::new();
    for piece in page.split("<section id=\"tela-").skip(1) {
        let Some(name) = piece.split('"').next() else {
            continue;
        };
        let Some(tag) = piece.split('>').next() else {
            continue;
        };
        // The delimiter is put back so the tag reads as it does in the page —
        // an assertion that quotes half an opening tag sends the next reader
        // looking for a string that is not there.
        found.push((
            format!("tela-{name}"),
            format!("section id=\"tela-{tag}"),
            piece.to_owned(),
        ));
    }
    assert!(
        found.len() >= 4,
        "found {} screens; boot, sessao, auth and fim are all supposed to be in \
         the page — or they stopped being written `<section id=\"tela-…`",
        found.len()
    );
    found
}

/// A script cut into its top-level statements, by brace depth.
///
/// Coarser than "one function each", and that is deliberate: two of the nine
/// transitions in this frontend do not live in a named function at all — the
/// EJETAR of the end screen and the Escape handlers are arrow functions passed
/// straight to `addEventListener`. A cut that only knew about `function` would
/// have been blind to exactly the transitions nobody wrote a name for.
fn top_level_chunks(script: &str) -> Vec<String> {
    let script = without_comments(script);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut depth: i32 = 0;
    for line in script.lines() {
        current.push_str(line);
        current.push('\n');
        depth += i32::try_from(line.matches('{').count()).unwrap_or(0);
        depth -= i32::try_from(line.matches('}').count()).unwrap_or(0);
        if depth > 0 {
            continue;
        }
        depth = 0;
        if !current.trim().is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        current.clear();
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Whether this piece of script pulls a whole screen back into view.
///
/// `$("…")` with a screen id, or `$(…)` with a variable — `fecharDogma` reveals
/// `$(volta)` and `abrirDogma` hides `$(origem)`, and a check that only read
/// literals would miss the one transition that has two possible destinations.
/// A variable inside `$()` is only ever a screen here; everything else that
/// toggles `hidden` — the banner, the battery, the invite, an error line —
/// holds its element in a variable and writes `erro.hidden`, never `$(erro)`.
fn reveals_a_screen(chunk: &str, screens: &BTreeSet<String>) -> bool {
    for piece in chunk.split("$(").skip(1) {
        let Some((argument, rest)) = piece.split_once(')') else {
            continue;
        };
        if !rest.trim_start().starts_with(".hidden = false") {
            continue;
        }
        let argument = argument.trim();
        match argument
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            Some(id) => {
                if screens.contains(id) {
                    return true;
                }
            }
            None => return true,
        }
    }
    false
}

#[test]
fn every_transition_that_puts_a_screen_on_hands_it_the_keyboard() {
    // Hiding the ancestor of the focused element drops focus on `<body>`. If
    // nothing then focuses anything on the screen that arrives, three things
    // break at once and none of them errors: somebody who opened the call from
    // the keyboard lands at the top of the document and has to tab back down to
    // the button they pressed; closing does not give that button back; and a
    // screen reader announces nothing at all, because from where it stands
    // nothing happened. WCAG 2.4.3.
    //
    // Before this, `.focus()` appeared exactly once in the whole frontend — the
    // `/` shortcut of the search — and every screen change was a bare `hidden`.
    let page = read("ui/index.html");
    let screens: BTreeSet<String> = screens_of(&page).into_iter().map(|(id, _, _)| id).collect();

    let mut deaf: Vec<String> = Vec::new();
    let mut transitions = 0;
    for name in ui_files(".js") {
        for chunk in top_level_chunks(&read(&format!("ui/{name}"))) {
            if !reveals_a_screen(&chunk, &screens) {
                continue;
            }
            transitions += 1;
            if chunk.contains("abrirTela(") || chunk.contains("voltarParaTela(") {
                continue;
            }
            let head = chunk.lines().next().unwrap_or_default().trim().to_owned();
            deaf.push(format!("ui/{name}: {head}"));
        }
    }

    // A count, because the way this guard goes quiet is not somebody deleting a
    // `abrirTela` — it is somebody writing a transition the cut above cannot
    // see, and then nothing is reported because nothing was read.
    assert!(
        transitions >= 8,
        "only {transitions} places in the frontend reveal a screen; there are \
         nine — into auth, into the session, into and out of the call, into and \
         out of the settings, into the end screen, and the two ways back to the \
         entrance"
    );

    assert!(
        deaf.is_empty(),
        "these transitions reveal a screen and leave the focus on <body>, so the \
         keyboard lands at the top of the document and a screen reader is told \
         nothing:\n{}",
        deaf.join("\n")
    );
}

#[test]
fn every_screen_says_where_the_focus_lands_and_what_to_announce() {
    let page = read("ui/index.html");

    // The live region first: it is the half of this that is not about the
    // keyboard, and it has to be *outside* every screen. Inside one, it would be
    // hidden away at the exact moment it had something to say — a `hidden`
    // ancestor takes the whole subtree out of the accessibility tree.
    let anuncio = tag_with_id(&page, "anuncio");
    assert!(
        anuncio.contains("role=\"status\""),
        "the announcement region is not a live region any more, so a change of \
         screen is announced to nobody: <{anuncio}>"
    );
    for (id, _, markup) in screens_of(&page) {
        assert!(
            !markup.contains("id=\"anuncio\""),
            "the announcement region moved inside #{id}, where a `hidden` on the \
             screen takes it out of the accessibility tree along with everything \
             else — and a screen being hidden is exactly when it has to speak"
        );
    }

    // And it has to stay readable while being invisible. `display: none` and
    // `visibility: hidden` remove an element from the accessibility tree, which
    // is the one thing this element cannot afford.
    let base = without_comments(&read("ui/base.css"));
    let Some((_, rule)) = base.split_once(".anuncio {") else {
        panic!("base.css no longer styles `.anuncio`, so it is a visible paragraph")
    };
    let rule = rule.split('}').next().unwrap_or_default();
    for banned in ["display: none", "visibility: hidden"] {
        assert!(
            !rule.contains(banned),
            "`.anuncio` hides itself with `{banned}`, which takes it out of the \
             accessibility tree — it would be invisible *and* silent:\n{rule}"
        );
    }

    for (id, tag, markup) in screens_of(&page) {
        // Focusable at all. Without this the fallback in `abrirTela` — focus the
        // screen itself when it names no better control — silently does nothing,
        // and the focus stays on `<body>` exactly as before.
        assert!(
            tag.contains("tabindex=\"-1\""),
            "#{id} cannot receive focus, so a transition into it has nowhere to \
             put the keyboard: <{tag}>"
        );

        assert!(
            attribute(&tag, "data-anuncio").is_some_and(|frase| !frase.trim().is_empty()),
            "#{id} does not say what to announce when it opens, so somebody who \
             cannot see it is told nothing: <{tag}>"
        );

        // `data-foco` is optional — the session leaves it out on purpose, and the
        // markup says why. What it may not be is a name of nothing, or a name of
        // something on another screen: `focus()` on either does nothing and says
        // nothing, which is the failure this whole pair of guards is about.
        let Some(alvo) = attribute(&tag, "data-foco") else {
            continue;
        };
        assert!(
            markup.contains(&format!("id=\"{alvo}\"")),
            "#{id} sends the focus to `#{alvo}`, which is not inside it — \
             `focus()` on something hidden or absent fails silently, and the \
             keyboard would stay on <body>"
        );
    }
}

// ---------------------------------------------------------------------------
// The update button — ADR 0026.
// ---------------------------------------------------------------------------

/// The sentence `ui/frases.js` writes for one enum variant.
///
/// Comments stripped first, for the reason the rest of this file strips them:
/// the block above these entries explains *why* there are six sentences and not
/// one, and a check satisfied by that explanation is a check that cannot fail.
///
/// An entry ends at `",` followed by a line break, which is what closes the last
/// piece of a sentence whether it is written on one line or concatenated over
/// four — the `+` ending every intermediate piece is what keeps this from
/// stopping early.
fn sentence_for(variant: &str) -> String {
    let file = without_comments(&read("ui/frases.js"));
    let Some(after) = file.split(&format!("\n    {variant}:")).nth(1) else {
        panic!(
            "frases.js writes no sentence for `{variant}`, so that failure reaches \
             a screen that either says nothing or says `{variant}`"
        );
    };
    let Some(sentence) = after.split("\",\n").next() else {
        panic!("the sentence for `{variant}` is never closed");
    };
    sentence.trim().to_owned()
}

#[test]
fn every_way_the_update_can_fail_asks_something_different_of_the_reader() {
    // Six variants, and ADR 0026 is explicit that they are six because they ask
    // six different things of whoever is in front of the screen: `NaoConfigurado`
    // means *this executable will never update* and there is nothing to retry,
    // `AssinaturaRecusada` means a package arrived signed by somebody else and
    // retrying is precisely the wrong move, and `NaoAlcancei` means try again in
    // a minute. One sentence for all six would send everybody back to the same
    // button, including the two cases where pressing it again is wrong.
    //
    // So this asserts distinctness and not merely presence. A copy-pasted
    // sentence is the failure mode: it looks written, it reads fine, and it
    // quietly collapses two different situations into one instruction.
    let source = read("src/main.rs");
    let script = without_comments(&scripts());

    let variants = variants_of(&source, "FalhaAoAtualizar");
    assert_eq!(
        variants.len(),
        6,
        "`FalhaAoAtualizar` has {} variants; ADR 0026 wrote six on purpose: {variants:?}",
        variants.len()
    );

    let mut written: BTreeSet<String> = BTreeSet::new();
    for variant in &variants {
        assert!(
            names(&script, variant),
            "`FalhaAoAtualizar::{variant}` reaches the page with no sentence \
             written for it, so the screen either says nothing or says `{variant}`"
        );
        let sentence = sentence_for(variant);
        assert!(
            !sentence.is_empty(),
            "the sentence for `{variant}` is empty"
        );
        assert!(
            written.insert(sentence.clone()),
            "two ways the update can fail are written with the same sentence, so \
             the screen cannot tell them apart — and two of these six must not \
             send the reader back to the same button:\n{sentence}"
        );
    }
}

/// The markup of one panel of the settings screen, cut out by its id.
///
/// The panels nest no `<section>` of their own, so the first closing tag is
/// theirs. Comments stripped: the paragraph above this panel explains, at
/// length, exactly what the panel has to say — and a guard an explanation can
/// satisfy is a guard that cannot fail.
fn panel_markup(page: &str, id: &str) -> String {
    let page = without_comments(page);
    let Some(after) = page.split(&format!("id=\"{id}\"")).nth(1) else {
        panic!("index.html has no panel with id `{id}`");
    };
    let Some(body) = after.split("</section>").next() else {
        panic!("the panel `{id}` is never closed");
    };
    body.to_owned()
}

#[test]
fn the_update_screen_says_the_window_closes_and_that_a_hosted_dogma_falls_with_it() {
    // `instalar_atualizacao` closes and reopens SEELE on all three systems — on
    // Windows there is no choice, because the NSIS installer will not run with
    // the program open. An action that closes somebody's window has to say so
    // *before* it is pressed.
    //
    // The second half is the one that is easy to leave out, and it is the one
    // that costs other people: this app can host a Dogma inside the very window
    // that is about to be replaced (`hospedar`), and everybody inside it drops
    // when it goes. Whoever presses the button knows they are closing their own
    // window; nothing but this sentence tells them whose else.
    let page = read("ui/index.html");
    let painel = panel_markup(&page, "painel-atualizacao");

    for (what, needle) in [
        (
            "that the window closes and comes back",
            "fecha e abre de novo",
        ),
        ("that a Dogma hosted here falls too", "hospedando um Dogma"),
        (
            "that a failure leaves no half installation",
            "meia instalação",
        ),
    ] {
        assert!(
            painel.contains(needle),
            "the update panel never says {what}. Installing is the one action in \
             this app that takes the window down, and on the day it takes a room \
             full of people with it, this paragraph is the only warning there was."
        );
    }

    // And it has to be above the button, not beside the failure afterwards.
    let Some(warning) = painel.find("ANTES DE INSTALAR") else {
        panic!("the update panel no longer heads its warning");
    };
    let Some(button) = painel.find("id=\"atualizacao-instalar\"") else {
        panic!("the update panel has no install button");
    };
    assert!(
        warning < button,
        "the warning about closing the window is drawn after the button that \
         closes it, so it is read by whoever already pressed"
    );
}

#[test]
fn the_download_bar_is_left_unmeasured_when_the_package_carries_no_total() {
    // `Andamento::total` is an `Option` because the server may send no
    // `Content-Length`. A bar drawn against a denominator nobody sent is a bar
    // that stalls somewhere and lies about how much is left — the same defect
    // the battery bar exists to avoid, and it gets the same answer here: the
    // frame stays, the value is marked absent, and the count beside it carries
    // the truth.
    //
    // Scoped to the one function that draws it, because the file says all of
    // these words either way — the paragraph explaining why there is no bar
    // would satisfy an unscoped search for it.
    let body = body_of(&scripts(), "function desenharAndamentoDoDownload");

    let Some(guard) = body.find("!andamento.total") else {
        panic!(
            "nothing branches on whether the package announced a total, so the \
             bar is drawn against a denominator that may not exist:\n{body}"
        );
    };
    let Some(unmeasured) = body.find("naoMedido(") else {
        panic!("the missing total is not marked absent anywhere:\n{body}");
    };
    let Some(bar) = body.find("blocos(") else {
        panic!("nothing draws the bar at all:\n{body}");
    };

    assert!(
        guard < unmeasured && unmeasured < bar,
        "the block bar is drawn before the missing total is ruled out, so a \
         package with no announced size still gets a percentage:\n{body}"
    );
    assert!(
        body[unmeasured..bar].contains("return"),
        "the unmeasured branch falls through into the bar, so one frame says \
         both «not measured» and «47%»:\n{body}"
    );
    assert!(
        body.contains("${parte}%"),
        "the bar has no number beside it, and `specs/05-cliente-tui.md` forbids \
         information carried by shape alone — a wall of blocks is shape:\n{body}"
    );
}

#[test]
fn the_update_is_only_ever_asked_for_by_a_press() {
    // ADR 0026: "nenhuma consulta automática ao abrir". In a product whose whole
    // argument is that the server is yours, an app that talks to github.com on
    // every launch contradicts the argument — and what was asked for was a
    // *button*.
    //
    // The tempting change is small and invisible: one call from the half-second
    // tick this screen already runs, or one from `abrirDogma`, and the app
    // quietly phones home whenever anybody opens the settings. So the check is
    // that the search is reachable from exactly two places — where it is
    // declared, and where a click is bound to it — and from nowhere else.
    let file = read("ui/tela-dogma.js");

    let mut places = Vec::new();
    for chunk in top_level_chunks(&file) {
        if !chunk.contains("procurarAtualizacao") && !chunk.contains("procurar_atualizacao") {
            continue;
        }
        let head = chunk.lines().next().unwrap_or_default().trim().to_owned();
        places.push(head.clone());
        assert!(
            head.starts_with("async function procurarAtualizacao")
                || chunk.contains("addEventListener(\"click\""),
            "something other than a press reaches the update check: {head}"
        );
    }
    assert_eq!(
        places.len(),
        2,
        "the update check is reachable from {} places; there are two — the \
         declaration and the click: {places:?}",
        places.len()
    );

    // And the tick that keeps the input meter alive must not have grown a second
    // job. It runs twice a second for as long as this screen is open.
    let tick = body_of(&scripts(), "async function atualizarDogma");
    assert!(
        !tick.contains("procurar"),
        "the half-second loop of the settings screen asks whether there is a new \
         version, which is an automatic check wearing a meter's clothes:\n{tick}"
    );
}

// ---------------------------------------------------------------------------
// Moderation — the four verbs of `specs/04-servidor-seele.md`.
// ---------------------------------------------------------------------------

/// The four commands that act on a person or on what they said.
const VERBOS_DE_MODERACAO: &[&str] = &[
    "expulsar_piloto",
    "banir_piloto",
    "remover_mensagem",
    "mover_piloto",
    // Destroying a room goes through the same machine, and belongs on the same
    // list. It is the most consequential of the six — a kick lasts a session, a
    // ban is undone by whoever holds the Dogma's file, and this ends what other
    // people wrote with nothing anywhere that brings it back.
    "apagar_cage",
    "apagar_linha",
];

#[test]
fn no_moderation_act_reaches_the_dogma_without_a_sentence_that_says_what_it_costs() {
    // The rule this whole layer exists for. Kicking and banning are
    // irreversible **for the person on the receiving end**, removing a message
    // takes it away from everybody, and moving somebody takes them out of where
    // they were without their asking. None of the four can be one press away.
    //
    // And a second press is not the answer either: «tem certeza?» adds no
    // information to somebody who already decided once, so it trains people to
    // press twice. What a confirmation is *for* is saying what will happen, with
    // the name of whoever it happens to inside the sentence — which is why
    // `armarAto` takes the sentence as an argument rather than a flag.
    //
    // So every one of the four has to reach the bridge from inside a chunk that
    // arms an act, and there must be no other path. Every chunk that names the
    // command is checked, not just the first — a second, unguarded call is
    // exactly the shape this would otherwise miss.
    let layer = read("ui/camada-moderar.js");
    let script = without_comments(&scripts());

    for verbo in VERBOS_DE_MODERACAO {
        let needle = format!("invoke(\"{verbo}\"");
        assert!(
            script.contains(&needle),
            "nothing calls `{verbo}`, so the verb is registered and unreachable"
        );

        let mut armed = 0;
        for chunk in top_level_chunks(&layer) {
            if !chunk.contains(&needle) {
                continue;
            }
            armed += 1;
            assert!(
                chunk.contains("armarAto(") || chunk.contains("abrirConfirmacao("),
                "`{verbo}` is sent without arming a confirmation first, so an \
                 irreversible act is one press away:\n{chunk}"
            );
        }
        assert!(
            armed > 0,
            "`{verbo}` is called from outside ui/camada-moderar.js, where the \
             confirmation lives — so whatever calls it is not going through one"
        );
    }

    // The other end of the same wire: the box's confirm button is the only thing
    // that runs an armed act, and it runs the one that was armed.
    let Some(confirmar) = script
        .split("$(\"moderar-confirmar\").addEventListener")
        .nth(1)
        .and_then(|rest| rest.split("\n});").next())
    else {
        panic!("nothing is listening on `moderar-confirmar` at all");
    };
    assert!(
        confirmar.contains("atoArmado") && confirmar.contains("executar()"),
        "the confirm button does not run the act that was armed, so the sentence \
         the reader agreed to and the command that goes out are two different \
         things:{confirmar}"
    );

    // And the message path goes through the same door.
    let mensagem = body_of(&scripts(), "function abrirConfirmacao");
    assert!(
        mensagem.contains("armarAto("),
        "`abrirConfirmacao` opens the box without arming anything, so a second \
         confirmation shape grew beside the first:\n{mensagem}"
    );

    assert!(
        !script.to_lowercase().contains("tem certeza"),
        "something in the page asks «tem certeza?», which is the confirmation \
         that adds nothing: it does not say what happens, so it teaches people \
         to press twice"
    );
}

#[test]
fn the_ban_says_that_nothing_in_this_product_undoes_it() {
    // The sharpest edge of the four, and the one a screen can hide by accident.
    // There is no `unban` verb in this protocol: a permanent ban is undone only
    // by somebody with the Dogma's own file, by hand, on the machine hosting it.
    // A confirmation that says «bar this person?» and stops there is describing a
    // reversible act, and this one is not.
    //
    // The timed ban is in the same sentence for the same reason, from the other
    // side: it *is* the one that undoes itself, and that is exactly the fact
    // that makes it worth offering. Whoever is about to press has to be told
    // which of the two is about to happen.
    let body = body_of(&scripts(), "function consequenciaDeBanir");

    assert!(
        body.contains("ate === null") || body.contains("ate == null"),
        "the ban sentence does not branch on whether there is an expiry, so a \
         ban that lifts itself and a ban that never does read the same:\n{body}"
    );
    for (what, needle) in [
        ("that it is forever", "para sempre"),
        (
            "that no screen here undoes it",
            "Nenhuma tela deste produto desfaz",
        ),
        ("who can undo it, and how", "arquivo do Dogma"),
        ("when a timed one lifts", "cai sozinha"),
    ] {
        assert!(
            body.contains(needle),
            "the ban confirmation never says {what}. Without it the sentence \
             describes a reversible act, and this is the one act in this product \
             that no screen of it can take back:\n{body}"
        );
    }

    // The name of the person, in the sentence. A consequence written about
    // nobody is a consequence about the wrong person just as easily.
    assert!(
        body.contains("quem.nome"),
        "the ban confirmation does not name who it is about:\n{body}"
    );
}

#[test]
fn moving_somebody_says_they_did_not_ask_and_that_both_rooms_watch_it_happen() {
    // The act that is easiest to underrate of the four, because nobody is
    // disconnected by it. It takes a person out of where they were without their
    // asking, they get a notice about it, and the two rooms watch them leave and
    // arrive. All three are things the person pressing does not experience, and
    // all three are the reason it deserves the same confirmation as the others.
    let body = body_of(&scripts(), "function consequenciaDeMover");

    for (what, needle) in [
        ("that the person did not ask for it", "sem ter pedido"),
        ("that they are told", "aviso"),
        ("which room they are taken from", "quem.cage"),
        ("which room they land in", "destino"),
    ] {
        assert!(
            body.contains(needle),
            "the move confirmation never says {what}:\n{body}"
        );
    }
    assert!(
        body.contains("continua na sessão"),
        "the move confirmation does not say the person stays connected, so it \
         reads like a kick with extra steps:\n{body}"
    );
}

#[test]
fn each_moderation_verb_is_offered_by_its_own_permission() {
    // `Snapshot` carries four booleans and not one, and the reason is on the
    // wire: `specs/04-servidor-seele.md` enumerates four permissions and a role
    // may hold any subset. A Dogma can hand somebody `Kick` and nothing else.
    //
    // Gating the three on one boolean is the cheap version and it fails in both
    // directions at once — it offers `banir` to somebody who may only kick, and
    // hides `mover` from somebody who may only move. Neither shows up in a
    // build, and the first is only found by a refusal in front of a person.
    //
    // Scoped to the function that draws the box, because the file says all four
    // names either way: the paragraph explaining why there are four would
    // satisfy an unscoped search for them.
    let body = body_of(&scripts(), "function desenharModeracao");

    for (bloco, permissao) in [
        ("moderar-acao-expulsar", "may_kick"),
        ("moderar-acao-banir", "may_ban"),
        ("moderar-acao-mover", "may_move_pilot"),
    ] {
        assert!(
            body.split(';')
                .any(|statement| statement.contains(bloco) && statement.contains(permissao)),
            "`{bloco}` is not decided by `{permissao}`, so a role that carries \
             some of the four moderation permissions is offered the wrong ones:\n{body}"
        );
    }
    assert!(
        !body.contains("may_manage_cages"),
        "the moderation box is offered by the room-management permission, which \
         is a different permission for a different thing:\n{body}"
    );

    // The fourth lives on the message, and it is the one with an exception: your
    // own message needs no permission at all, because the permission in
    // `specs/04` reads «de outra pessoa».
    let mensagem = body_of(&scripts(), "function botaoDeRemoverMensagem");
    assert!(
        mensagem.contains("mensagem.own"),
        "removing a message consults only the permission, so somebody with no \
         moderation cannot take back what they themselves said:\n{mensagem}"
    );
    assert!(
        scripts().contains("may_remove_message"),
        "nothing reads `may_remove_message`, so the control is either shown to \
         everybody or to nobody"
    );

    // And never on yourself: kicking yourself is DESCONECTAR, banning yourself
    // is not a thing, and moving yourself is ENTRAR NA JAULA.
    let porta = body_of(&scripts(), "function botaoDeModerar");
    assert!(
        porta.contains("piloto.is_self"),
        "the moderation door is drawn on one's own row too:\n{porta}"
    );
}

#[test]
fn the_moderation_is_a_layer_over_the_session_and_never_replaces_it() {
    // Same decision as the alert and the battery, and for the same written
    // reason: `specs/07-tema-evangelion.md` does not let this client replace the
    // conversation, and moderating is a decision taken while *looking* at the
    // room. A full screen would blank out who is talking at the exact moment
    // somebody is deciding what to do about a person.
    //
    // Structural, and not a screenshot: it has to live inside `#tela-sessao`.
    let page = read("ui/index.html");
    let Some(after) = page.split("id=\"tela-sessao\"").nth(1) else {
        panic!("index.html no longer has the session screen");
    };
    let Some(session) = after.split("<section ").next() else {
        panic!("the session screen is never closed by another section");
    };
    assert!(
        session.contains("id=\"moderar\""),
        "`moderar` is drawn outside `#tela-sessao`, so it is a screen and not a \
         layer — and a screen replaces the history that specs/07 says has to stay \
         readable"
    );

    let tag = tag_with_id(&page, "moderar");
    assert!(
        tag.contains("hidden"),
        "the moderation box is not hidden in the markup, so it is on the screen \
         from the first frame of every session: <{tag}>"
    );
    assert!(
        tag.contains("role=\"dialog\"") && tag.contains("aria-modal"),
        "the moderation box does not announce itself as a modal dialog, so a \
         screen reader reads it as more of the page it is covering: <{tag}>"
    );
}

#[test]
fn the_moderation_does_not_spend_the_red_reserved_for_alarm_and_collapse() {
    // `tokens.css:19` marks the red "EXCLUSIVO alerta e queda". Moderation is
    // the most serious thing this window offers and it is still not a dropped
    // link — and the red spent here is the red nobody reads on the day the
    // internal battery lights up. The accent is the institutional orange.
    //
    // Comments stripped, and that is load-bearing rather than tidy: the sheet
    // has to be able to write down *why* it is not red, and the paragraph saying
    // so names the token. A guard a comment can trip is as broken as a guard a
    // comment can satisfy.
    let sheet = without_comments(&read("ui/camada-moderar.css"));
    assert!(
        !sheet.contains("vermelho"),
        "the moderation layer paints with the token reserved for alarm and \
         collapse, which is the battery's colour and nothing else's"
    );

    // The one red it may show is the text of a failure, and that comes from
    // `.erro` in base.css — the same band of seriousness as a connection error,
    // and never the frame of the box.
    let page = without_comments(&read("ui/index.html"));
    assert!(
        page.contains("id=\"moderar-erro\" class=\"erro\""),
        "the moderation box writes its failures somewhere other than the shared \
         `.erro`, so it either invented a second red or says nothing when a \
         command is refused"
    );
}

#[test]
fn opening_the_moderation_carries_the_keyboard_and_closing_gives_it_back() {
    // A modal that appears without taking the focus leaves whoever navigates by
    // keyboard on the element behind it, pressing things that are no longer in
    // front — and a screen reader is told nothing at all, because from where it
    // stands nothing happened. WCAG 2.4.3, the same rule `abrirTela` exists for.
    //
    // Closing is the half that is easy to forget, and it has a trap of its own:
    // the button that opened the box is a row of `#lista-cages`, and that list is
    // thrown away and rebuilt twice a second — so by the time the box closes the
    // element may not be in the document any more. `focus()` on a node outside
    // the tree does nothing and reports nothing, which is the original defect
    // reintroduced from the inside.
    let abrir = body_of(&scripts(), "async function abrirModeracao");
    assert!(
        abrir.contains(".focus()"),
        "opening the moderation leaves the focus on whatever was behind it:\n{abrir}"
    );
    assert!(
        abrir.contains("anunciar("),
        "opening the moderation announces nothing, so somebody who cannot see it \
         is not told the box is there:\n{abrir}"
    );

    let fechar = body_of(&scripts(), "function fecharModeracao");
    assert!(
        fechar.contains("focoAntesDeModerar"),
        "closing the moderation does not give the keyboard back to whoever opened \
         it, so the focus lands on <body>:\n{fechar}"
    );
    assert!(
        fechar.contains("focavel("),
        "closing the moderation focuses the opener without checking it is still \
         in the page — and the row it lives on is rebuilt twice a second, so \
         `focus()` on it does nothing and says nothing:\n{fechar}"
    );

    // And a session that ends with the box open must not leave it armed for the
    // next one: the act inside it names somebody from a Dogma already left.
    let fim = body_of(&scripts(), "function mostrarFim");
    assert!(
        fim.contains("abandonarModeracao"),
        "the end-of-session screen leaves the moderation box open, so it comes \
         back over the *next* session armed with an act on somebody from the \
         previous Dogma:\n{fim}"
    );
}

#[test]
fn every_failure_the_rust_side_can_name_has_a_sentence_here() {
    // `nome_da_falha` in `src/main.rs` turns each `ErroDeUri` variant into a
    // stable name, and `FRASES` in `ui/frases.js` turns that name into the
    // sentence somebody reads. The two lists are joined by nothing but
    // convention, and the seam is invisible: a new variant compiles, reaches the
    // screen, misses `FRASES`, and falls through to the `desconhecida()`
    // fallback — which prints a JSON blob of an English identifier.
    //
    // Not hypothetical. `EnderecoIpv6SemColchetes` arrived with step 2 of
    // ADR 0022 and did exactly that until this test existed.
    //
    // The fallback is still the right thing to have — it beats a dead end — but
    // it is a net, not a destination, and nothing was checking how often we
    // landed in it.
    let rust = read("src/main.rs");
    let Some(corpo) = rust.split("fn nome_da_falha").nth(1) else {
        panic!(
            "`nome_da_falha` is gone from src/main.rs, so the names the screen \
             keys off are now produced somewhere this test cannot see"
        );
    };
    let Some(corpo) = corpo.split("\n}").next() else {
        panic!("`nome_da_falha` is never closed");
    };

    // The names are the string literals on the right of each match arm.
    let nomes: Vec<&str> = corpo
        .split("=> \"")
        .skip(1)
        .filter_map(|resto| resto.split('"').next())
        .collect();
    assert!(
        nomes.len() >= 6,
        "found only {} failure names, so the match arms are no longer being read \
         correctly and this test has stopped guarding anything: {nomes:?}",
        nomes.len()
    );

    let frases = read("ui/frases.js");
    let sem_frase: Vec<&&str> = nomes
        .iter()
        .filter(|nome| !frases.contains(&format!("{nome}:")))
        .collect();
    assert!(
        sem_frase.is_empty(),
        "the Rust side can name these failures and `FRASES` has no sentence for \
         them, so each one reaches the person as a JSON blob of an English \
         identifier: {sem_frase:?}"
    );
}

#[test]
fn every_rung_of_the_reachability_ladder_has_a_sentence() {
    // `Anfitriao.alcance` crosses as one of four stable names from
    // `seele_server::alcance::Degrau`, and the sentence is here. Same seam as
    // `nome_da_falha` above, and the same failure if it drifts — except worse,
    // because this one is not an error: hosting succeeded, and the missing
    // sentence would be the one that says the link only works on the LAN.
    //
    // ADR 0022 asks for exactly this to be said out loud rather than left for
    // the person to discover as "it doesn't connect".
    let frases = read("ui/frases.js");
    for degrau in [
        "PortaNoRoteador",
        "Ipv6Direto",
        "RedeLocalOuVpn",
        "SoRedeLocal",
    ] {
        assert!(
            frases.contains(&format!("{degrau}:")),
            "the ladder can stop at `{degrau}` and no sentence says what that \
             means for the link the person is about to send"
        );
    }

    // The one that matters most is the bad one: it has to warn, and it has to
    // say what to do instead. A rung that only names itself leaves the host
    // exactly as stuck as no message at all.
    let Some(depois) = frases.split("SoRedeLocal:").nth(1) else {
        panic!("no sentence for SoRedeLocal");
    };
    let frase: String = depois.chars().take(400).collect();
    assert!(
        frase.to_lowercase().contains("rede"),
        "the LAN-only sentence never says the link is limited to the local \
         network:\n{frase}"
    );
    assert!(
        frase.to_lowercase().contains("roteador") || frase.to_lowercase().contains("vpn"),
        "the LAN-only sentence names the problem and no way out — ADR 0022 asks \
         for the way out to be written down:\n{frase}"
    );
}

#[test]
fn the_vpn_rung_names_the_vpn_and_says_what_to_do_about_it() {
    // The rung that exists because of a field failure: a Windows host with
    // Cloudflare WARP had a global IPv6 — the tunnel's — and the ladder read it
    // as "reachable from anywhere", printed under a link that accepts no
    // inbound connection at all. The rung is only worth its own name if the
    // sentence says the thing the other three cannot: that a VPN is why, and
    // that turning it off is the way out.
    let frases = read("ui/frases.js");
    let Some(depois) = frases.split("RedeLocalOuVpn:").nth(1) else {
        panic!("no sentence for RedeLocalOuVpn");
    };
    // Bounded to this entry's own text, and comment-free: the block above it in
    // `frases.js` explains the field failure using every one of these words, so
    // an unscoped search would be satisfied by the justification instead of the
    // sentence.
    let frase: String = depois
        .split("SoRedeLocal:")
        .next()
        .unwrap_or_default()
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        frase.to_lowercase().contains("vpn"),
        "the sentence never says a VPN is why the link reaches no further, which          is the only thing this rung knows that `SoRedeLocal` does not:\n{frase}"
    );
    assert!(
        frase.to_lowercase().contains("desligue") || frase.to_lowercase().contains("desligar"),
        "the sentence names the cause and withholds the fix — ADR 0022 asks for          the way out to be written down:\n{frase}"
    );
    assert!(
        !frase.to_lowercase().contains("alcança de qualquer lugar"),
        "the VPN rung repeats the promise that was the bug:\n{frase}"
    );
}

#[test]
fn the_ipv6_sentence_teaches_the_fix_with_an_address_that_would_work() {
    // This failure has a sentence of its own, rather than falling into
    // `EnderecoInvalido`, for one reason: it is fixable by the person reading
    // it. The address is fine; the punctuation is missing. So the sentence has
    // to actually show the bracketed form — naming a problem whose fix you
    // withhold is worse than the generic message it was split away from.
    //
    // And the example has to be an address that would work once bracketed. A
    // link-local (`fe80::`) example teaches the brackets and hands back an
    // address that still reaches nothing without a zone index, trading one dead
    // end for another.
    let frases = read("ui/frases.js");
    let Some(depois) = frases.split("EnderecoIpv6SemColchetes:").nth(1) else {
        panic!("no sentence for EnderecoIpv6SemColchetes");
    };
    let frase: String = depois.chars().take(400).collect();

    assert!(
        frase.contains('[') && frase.contains(']'),
        "the sentence never shows the bracketed form, so it names the problem and \
         withholds the fix:\n{frase}"
    );
    assert!(
        !frase.to_lowercase().contains("fe80"),
        "the example is a link-local address, which reaches nothing without a \
         zone index even once it is bracketed:\n{frase}"
    );
    assert!(
        !frase.contains('\u{2026}'),
        "a draft ellipsis leaked into a sentence somebody reads:\n{frase}"
    );
}

#[test]
fn the_host_is_told_how_far_the_link_they_are_about_to_send_reaches() {
    // The last unwired half of ADR 0022, and the half the whole ladder is for.
    // The Rust side climbs the rungs, the sentences exist in `FRASES`, and the
    // `alcance` field crosses the boundary — and none of that reaches anybody if
    // the page never draws it. A link that only works on the home network and a
    // link that works over the internet are the same text, so a host with no
    // sentence sends the first believing they sent the second, and the person
    // who finds out is their friend, on the other side, as "it won't connect".
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let alcance = tag_with_id(&page, "convite-alcance");
    assert!(
        alcance.contains("hidden"),
        "`convite-alcance` is born visible, so it holds a reserved empty space \
         before `hospedar` has said which rung it stopped on: <{alcance}>"
    );

    // Scoped to the two functions that own this, because the page says all of
    // these words either way — the comment above the markup explaining why the
    // reach sits next to the link would satisfy an unscoped search for it.
    // On the comment-stripped source, like every other body read in this file:
    // `body_of` over the raw script would let a comment inside `hospedar`
    // mentioning `mostrarAlcance(` stand in for the call itself.
    let hospedar = body_of(&script, "async function hospedar");
    assert!(
        hospedar.contains("mostrarAlcance("),
        "`hospedar` shows the invite without ever saying how far it goes:\n{hospedar}"
    );
    assert!(
        hospedar.contains("alcance"),
        "`hospedar` never reads `alcance` off what the Rust side answered, so the \
         rung it climbed is thrown away at the boundary:\n{hospedar}"
    );

    let mostrar = body_of(&script, "function mostrarAlcance");
    assert!(
        mostrar.contains("fraseDeErro(") || mostrar.contains("FRASES"),
        "`mostrarAlcance` writes its own wording instead of reading the one in \
         `frases.js`, which is the file every other sentence lives in:\n{mostrar}"
    );
    assert!(
        mostrar.contains("SoRedeLocal"),
        "`mostrarAlcance` treats every rung alike, so the one that means `your \
         friends cannot reach this` looks exactly like the two that mean they \
         can:\n{mostrar}"
    );
    // And the rung that looks like good news and is not: a host on a browsing
    // VPN has an address that reads as global and accepts nobody. Drawing it
    // like the two rungs that do reach outside is the same lie the ladder used
    // to tell, moved one layer up.
    assert!(
        mostrar.contains("RedeLocalOuVpn"),
        "`mostrarAlcance` draws the VPN rung as if it reached the world, which \
         is exactly the promise this rung exists to stop making:\n{mostrar}"
    );

    // And the colour it may not spend. `tokens.css` reserves red for alarm and
    // collapse; a link that reaches only the home network is neither — it works,
    // and works less far than the host probably wanted.
    //
    // Two things this assertion got wrong on the way in, both worth keeping
    // written down because both produced a green test over a broken rule:
    //
    // - it read the sheet *with* comments, so the paragraph above the rule
    //   explaining why there is no red here was itself the match. The guard
    //   failed on its own justification. Same defect class `without_comments`
    //   exists for, one file over;
    // - it then read `split(…).nth(1)`, which is the text between the *first*
    //   and *second* occurrence of the selector — that is the base rule alone,
    //   and it stops exactly before `.convite-alcance-curto`, which is the one
    //   rule that actually carries a colour decision. Painting the short rung
    //   red passed. Every rule whose selector starts with the prefix has to be
    //   read, not the first one.
    let folha = without_comments(&styles());
    let mut regras = 0;
    for trecho in folha.split(".convite-alcance").skip(1) {
        let corpo = trecho.split('}').next().unwrap_or_default();
        regras += 1;
        assert!(
            !corpo.contains("vermelho"),
            "a `.convite-alcance…` rule spends the red that tokens.css reserves \
             for alarm and collapse:\n.convite-alcance{corpo}}}"
        );
    }
    assert!(
        regras >= 3,
        "found {regras} `.convite-alcance…` rules, and there are at least three: \
         the sentence itself and the two that tell the rungs apart. A count this \
         low means the selector was renamed and this whole check went quiet"
    );

    // Both directions of the same thing: the script must not invent a class the
    // sheet never styles, which renders as unstyled text and says nothing.
    let folha_bruta = styles();
    for classe in ["convite-alcance-curto", "convite-alcance-longe"] {
        assert!(
            !mostrar.contains(classe) || folha_bruta.contains(classe),
            "`mostrarAlcance` sets `{classe}`, which no stylesheet defines — so \
             the distinction it draws is invisible"
        );
    }
}

// ---------------------------------------------------------------------------
// Destroying a room — the confirmation that says the size of the damage.
// ---------------------------------------------------------------------------

#[test]
fn the_line_confirmation_counts_what_it_is_about_to_destroy() {
    // The requirement this whole path exists to satisfy, and the one a screen
    // can quietly fail: the box promises to destroy a specific number of
    // messages, written by a specific number of people, since a specific day.
    // All three have to be **counted**, in the Dogma's database, at the moment
    // of asking.
    //
    // The tempting wrong version is right there and free: this window already
    // holds a page of history, so `mensagens.length` would compile, render, and
    // read as a real number. It would be low by whatever the Line's whole past
    // is — and a number that is nearly right in a box promising to destroy 1.847
    // messages is worse than no number at all.
    //
    // So the sentence must be built out of the answer to `peso_da_linha` and
    // nothing else.
    let frase = body_of(&scripts(), "function consequenciaDeApagarLinha");

    for (what, needle) in [
        ("how many messages", "peso.messages"),
        ("how many people wrote them", "peso.authors"),
        ("since when", "peso.oldest_at_seconds"),
    ] {
        assert!(
            frase.contains(needle),
            "the Line confirmation never says {what}, so it promises destruction \
             without saying how much:\n{frase}"
        );
    }
    assert!(
        !frase.contains("mensagens.length") && !frase.contains("messages.length"),
        "the Line confirmation counts the page this window happens to be \
         holding, which is low by the whole of the Line's past:\n{frase}"
    );

    // And the count reaches it from the Dogma, through the one command that
    // waits for an answer. Scoped to the door that opens the box, because the
    // file explains the rule in prose as well and an unscoped search would be
    // satisfied by the paragraph.
    let layer = without_comments(&read("ui/camada-moderar.js"));
    let Some(porta) = layer
        .split("$(\"lista-linhas\").addEventListener")
        .nth(1)
        .and_then(|resto| resto.split("\n});").next())
    else {
        panic!("nothing listens for a press on the Line list in the moderation layer");
    };
    assert!(
        porta.contains("invoke(\"peso_da_linha\""),
        "the box about destroying a Line opens without asking the Dogma what is \
         in it, so its numbers came from somewhere this window guessed:\n{porta}"
    );

    // The order is the assertion: asked first, box second. A box opened before
    // the answer arrives is a box with a blank where the number goes.
    let pesa = porta
        .find("invoke(\"peso_da_linha\"")
        .expect("the weigh call was just asserted");
    let abre = porta
        .find("abrirConfirmacao(")
        .expect("the door opens no confirmation at all");
    assert!(
        pesa < abre,
        "the confirmation is opened before the count is asked for:\n{porta}"
    );
}

#[test]
fn a_count_that_did_not_arrive_stops_the_question_instead_of_rounding_it() {
    // The other half of "counted, never estimated", and the half a screen fails
    // by being helpful. When the Dogma does not answer, there is no honest
    // version of this box: what is left is «apagar a Linha?», which is the
    // confirmation that adds nothing and teaches people to press twice.
    //
    // So the failure path must not reach `abrirConfirmacao`, and must not
    // invent a zero either — «isto destrói 0 mensagens» about a Line full of
    // them is the worst sentence this screen could produce.
    let layer = without_comments(&read("ui/camada-moderar.js"));
    let Some(porta) = layer
        .split("$(\"lista-linhas\").addEventListener")
        .nth(1)
        .and_then(|resto| resto.split("\n});").next())
    else {
        panic!("nothing listens for a press on the Line list in the moderation layer");
    };

    let Some(falhou) = porta
        .split("} catch (falha) {")
        .nth(1)
        .and_then(|resto| resto.split("\n  }").next())
    else {
        panic!(
            "the weigh call is not wrapped in a `catch`, so a Dogma that does not \
             answer leaves the press doing nothing at all:\n{porta}"
        );
    };
    assert!(
        !falhou.contains("abrirConfirmacao("),
        "a Line whose count never arrived is still offered for destruction, with \
         whatever number the sentence fell back to:\n{falhou}"
    );
    assert!(
        falhou.contains("abrirRecusa("),
        "the count failing says nothing to whoever pressed, so a press that was \
         refused looks exactly like one that was ignored:\n{falhou}"
    );

    // And the refusal arms nothing: no act, and no button that could run one.
    let recusa = body_of(&scripts(), "function abrirRecusa");
    assert!(
        recusa.contains("atoArmado = null"),
        "the box that refuses to ask leaves an act armed behind it:\n{recusa}"
    );
    assert!(
        recusa.contains("$(\"moderar-confirmar\").hidden = true"),
        "the box that refuses to ask still shows the button that confirms, which \
         is a button with nothing behind it in front of somebody who came here to \
         destroy something:\n{recusa}"
    );
    // Which means the confirm button has to come back for the boxes that do have
    // an act. `armarAto` is where every one of them passes.
    let armar = body_of(&scripts(), "function armarAto");
    assert!(
        armar.contains("$(\"moderar-confirmar\").hidden = false"),
        "nothing brings the confirm button back after a refusal hid it, so the \
         next act in this session is a sentence nobody can agree to:\n{armar}"
    );
}

#[test]
fn destroying_a_room_says_what_it_does_to_the_people_and_to_the_other_room() {
    // The two things the person pressing does not experience, and therefore the
    // two a confirmation has to say out loud.
    //
    // For a Cage: people are turned out of it in the middle of speaking, and
    // they are told. And the half that is easiest to get wrong from the other
    // direction — the Line bound to it is **not** destroyed with it. Without
    // that line, somebody who wanted a conversation gone destroys the Cage, sees
    // the Line still there, and concludes the product did not do what it said.
    let cage = body_of(&scripts(), "function consequenciaDeApagarCage");
    for (what, needle) in [
        ("how many people are inside", "cage.pilots.length"),
        (
            "that it happens mid-sentence",
            "no meio do que estiverem falando",
        ),
        ("that they are told", "aviso"),
        ("that the bound Line survives", "não é apagada junto"),
        (
            "that nothing here brings it back",
            "Nenhuma tela deste produto",
        ),
    ] {
        assert!(
            cage.contains(needle),
            "the Cage confirmation never says {what}:\n{cage}"
        );
    }

    // For a Line: whoever is reading it loses it from the screen at that
    // instant, and any Cage bound to it comes out with no Line — a change
    // nobody asked for, which is exactly the kind this product names.
    let linha = body_of(&scripts(), "function consequenciaDeApagarLinha");
    for (what, needle) in [
        (
            "that no screen brings the writing back",
            "Nenhuma tela deste produto",
        ),
        ("what happens to whoever is reading it", "perde da tela"),
        ("which rooms come out without a Line", "sem Linha"),
    ] {
        assert!(
            linha.contains(needle),
            "the Line confirmation never says {what}:\n{linha}"
        );
    }

    // The empty Line has a branch of its own: there is no "written since" when
    // nobody wrote, and a sentence that says "destroys 0 messages since
    // Invalid Date" is a sentence that was never read by its author.
    assert!(
        linha.contains("peso.messages === 0"),
        "the Line confirmation has one sentence for a Line with a past and a \
         Line with none, so the empty one reads as a date that does not \
         exist:\n{linha}"
    );
}

#[test]
fn the_last_cage_is_offered_disabled_with_the_reason_written_on_it() {
    // A Dogma with no Cage has nowhere to speak. The Dogma refuses the press,
    // and this window has to say why *before* it — a control that vanishes on
    // the last room teaches nothing, because an absence is not something anybody
    // reads. `moderar-acao-mover` hides instead, and the difference is real: it
    // hides when its **object** does not exist, and this room does.
    let porta = body_of(&scripts(), "function botaoDeApagarCage");
    assert!(
        porta.contains("botao.disabled = ultimo"),
        "the last Cage is offered for destruction like any other, so the only \
         thing between a Dogma and having nowhere to speak is a refusal that \
         arrives after the press:\n{porta}"
    );
    assert!(
        porta.contains("botao.title = ultimo"),
        "the disabled control says nothing about why it is disabled, which is a \
         dead button and a shrug:\n{porta}"
    );
    assert!(
        porta.contains("único Cage"),
        "the reason the last Cage stays is not written anywhere a person \
         reads:\n{porta}"
    );

    // And the count comes from the Dogma's list, not from anything this file
    // decides: one Cage left is one Cage in `snapshot.cages`.
    let desenho = body_of(&scripts(), "function desenharCanais");
    assert!(
        desenho.contains("snapshot.cages.length === 1"),
        "nothing tells the delete control which Cage is the last one:\n{desenho}"
    );
}

#[test]
fn destroying_a_room_is_offered_by_the_permission_that_destroys_it() {
    // The decision, asserted where a person meets it. Making a room and
    // renaming one are mistakes a Dogma survives; destroying one ends what other
    // people wrote. `specs/04-servidor-seele.md` enumerates `gerenciar_cages`
    // and `administrar_dogma` separately, so a role that builds rooms without
    // being able to unmake them is a role somebody can actually write — and
    // gating both on one boolean makes it impossible to offer correctly.
    //
    // Scoped to the two functions that draw the controls, because the file
    // explains the distinction in prose too and an unscoped search for either
    // name would be satisfied by the paragraph that says why they differ.
    for porta in ["function botaoDeApagarCage", "function botaoDeApagarLinha"] {
        let corpo = body_of(&scripts(), porta);
        assert!(
            corpo.contains("may_delete_rooms"),
            "`{porta}` does not consult the permission that destroys rooms:\n{corpo}"
        );
        assert!(
            !corpo.contains("may_manage_cages"),
            "`{porta}` is offered by the permission to *make* rooms, which is a \
             different permission for a different thing:\n{corpo}"
        );
    }

    // And it is a permission of its own on the bridge, rather than a second
    // reading of one that was already there.
    let types = read("../../crates/seele-ffi/src/types.rs");
    assert!(
        types.contains("may_delete_rooms"),
        "the snapshot has no field for the permission that destroys rooms, so \
         whatever the screen is reading is somebody else's answer"
    );
}

#[test]
fn a_room_that_stopped_existing_has_a_sentence_and_not_a_shrug() {
    // Somebody is standing in the Cage, or reading the Line, when it stops
    // existing. The plug is already out and the conversation is already off the
    // screen by the time this arrives — so without the sentence what is left is
    // a room that vanished on its own, which from where the reader sits is
    // indistinguishable from a window that lost track of where it was.
    //
    // The third is not about a room that went: it is the refusal, and the only
    // one of the three that has to teach what to do next.
    let frases = read("ui/frases.js");
    let Some(avisos) = frases
        .split("const AVISOS = {")
        .nth(1)
        .and_then(|resto| resto.split("\n};").next())
    else {
        panic!("`AVISOS` is gone from ui/frases.js");
    };
    let avisos = without_comments(avisos);

    for reason in ["CageDeleted", "LineDeleted", "LastCage"] {
        assert!(
            avisos.contains(&format!("{reason}:")),
            "the Dogma can raise `{reason}` and `AVISOS` has no sentence for it, \
             so it reaches the person as the word AVISO and nothing else"
        );
    }
    assert!(
        avisos.contains("Faça outra sala antes"),
        "the refusal of the last Cage does not say what to do about it, which \
         makes it a wall rather than an answer"
    );

    // Every one of the three has to be a reason the bridge can actually produce.
    let types = read("../../crates/seele-ffi/src/types.rs");
    for reason in ["CageDeleted", "LineDeleted", "LastCage"] {
        assert!(
            types.contains(reason),
            "`AVISOS` writes a sentence for `{reason}`, which `NoticeReason` \
             cannot be — a sentence nobody will ever read"
        );
    }
}

#[test]
fn the_nat_punching_rung_promises_nothing_it_cannot_keep_and_names_its_cost() {
    // Degrau 4 do ADR 0022, added the day the rung was built. Two things are
    // being asserted, and both are product decisions rather than wording.
    //
    // First: it must not promise. Symmetric NAT on both ends does not punch,
    // and the ADR keeps relaying out of scope by decision — so the sentence has
    // to say what to do when it fails, exactly like the LAN-only one does.
    //
    // Second, and the one that would be quietest if it drifted: this rung is
    // the only one in the ladder with a third party in it. A product that sells
    // itself as «sem serviço no meio» just gained a service in the middle, and
    // the difference between saying so on the screen where the link appears and
    // letting somebody find out later is the difference between honesty and
    // advertising.
    let frases = read("ui/frases.js");
    let Some(depois) = frases.split("FuroDeNat:").nth(1) else {
        panic!("the ladder can stop at `FuroDeNat` and no sentence says what that means");
    };
    let frase: String = depois.chars().take(800).collect();
    let baixa = frase.to_lowercase();

    assert!(
        baixa.contains("deve funcionar"),
        "the NAT-punching sentence promises the link works, and nothing here can \
         promise that — two symmetric NATs do not punch:\n{frase}"
    );
    assert!(
        baixa.contains("roteador"),
        "the sentence names no way out for the case where the punch fails, which \
         leaves the host as stuck as no message at all:\n{frase}"
    );
    assert!(
        baixa.contains("ponto de encontro") && baixa.contains("nunca o que foi dito"),
        "the sentence does not say what the meeting point learns. ADR 0022 accepts \
         this rung only if the metadata is said out loud rather than discovered \
         later:\n{frase}"
    );

    // And the person has to be told they can point somewhere else, or «opcional
    // e trocável» is a sentence in a document nobody can act on.
    assert!(
        frase.contains("docs/ponto-de-encontro.md"),
        "the sentence never says the meeting point can be changed or switched \
         off:\n{frase}"
    );
}

// ---------------------------------------------------------------- anexos
//
// ADR 0027. Every check below reads the source **with the comments stripped**
// and scoped to one function's body, and that is not fussiness: this file has
// been fooled seven times in one day by an assertion satisfied by prose — a
// sentence in a doc comment, or a string in a `console.warn` sitting beside the
// `invoke` that had been deleted. A guard that a comment can satisfy guards a
// comment.

/// One JavaScript function's body, comments removed.
///
/// Braces are counted rather than split on, because every one of these bodies
/// contains nested blocks and object literals. Reading to the first `\n}` would
/// stop at the first `if`.
fn js_function(source: &str, signature: &str) -> String {
    let source = without_comments(source);
    let Some(at) = source.find(signature) else {
        panic!("`{signature}` is gone from the screen that had it");
    };
    let after = &source[at + signature.len()..];
    let Some(open) = after.find('{') else {
        panic!("`{signature}` has no body");
    };
    let mut depth = 0_i32;
    for (index, character) in after[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return after[open..=open + index].to_owned();
                }
            }
            _ => {}
        }
    }
    panic!("unterminated `{signature}`");
}

#[test]
fn a_file_goes_on_its_own_path_and_never_through_send_message() {
    // ADR 0027: the body travels **with** the file, on the transfer's own
    // stream, and the message is published only once the bytes have arrived
    // whole. A separate `send_message` would put the text on the Line first and
    // let the picture turn up minutes later with nothing saying the two were
    // one thing.
    let corpo = js_function(&read("ui/tela-sessao.js"), "async function enviar(");
    assert!(
        corpo.contains("subirAnexo("),
        "sending no longer takes the attachment path at all: {corpo}"
    );
    let subir = js_function(&read("ui/tela-sessao.js"), "async function subirAnexo(");
    assert!(
        subir.contains("invoke(\"enviar_anexo\""),
        "`subirAnexo` no longer calls `enviar_anexo`, so nothing sends a file"
    );
    assert!(
        !subir.contains("send_message"),
        "the file path also posts the text separately, which publishes the \
         message before the file exists"
    );
}

#[test]
fn the_bar_measures_bytes_and_never_pretends() {
    // ADR 0026 fixed the shape and ADR 0027 inherits the easy half of it: here
    // the total is **always** known, because whoever chose the file knows how
    // big it is. So this path is always a bar with a percentage, and there is no
    // branch in it that falls back to a dash.
    let andou = js_function(&read("ui/tela-sessao.js"), "function transferenciaAndou(");
    assert!(
        andou.contains("anexo-barra") && andou.contains("transfer.done"),
        "the progress bar is no longer driven by bytes: {andou}"
    );
    assert!(
        andou.contains("100) / transfer.total") || andou.contains("* 100"),
        "the bar no longer computes a percentage from the real total"
    );

    // And the element really is a `<progress>` with a value, not a decorative
    // div somebody widens.
    let pagina = without_comments(&read("ui/index.html"));
    assert!(
        pagina.contains("<progress id=\"anexo-barra\""),
        "the upload bar stopped being a `<progress>`"
    );
}

#[test]
fn a_transfer_that_falls_says_that_trying_again_starts_from_zero() {
    // The sentence ADR 0027 requires and that nothing else in this product has
    // ever had to say: **there is no resumption.** A bar that simply returned to
    // the start would leave that for the person to work out, which is the
    // difference between a product that warns and one that surprises.
    let frases = without_comments(&read("ui/frases.js"));
    let Some(bloco) = frases
        .split("const TRANSFERENCIAS = {")
        .nth(1)
        .and_then(|resto| resto.split("\n};").next())
    else {
        panic!("`TRANSFERENCIAS` is gone from ui/frases.js");
    };
    assert!(
        bloco.contains("Fell:"),
        "a fallen transfer has no sentence, so it is a bar that stopped"
    );
    // Everything from `Fell:` to the next key. Not the first line after it: the
    // sentence is several lines of concatenated string, and reading one line
    // would let the half that matters live outside what is asserted about.
    let depois = bloco.split("Fell:").nth(1).unwrap_or_default();
    let caiu: String = depois
        .lines()
        .take_while(|line| {
            let trimmed = line.trim_start();
            !(line.starts_with("  ")
                && trimmed.split(':').next().is_some_and(|word| {
                    !word.is_empty() && word.chars().all(char::is_alphanumeric)
                })
                && trimmed.contains(':'))
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        caiu.contains("do começo") || caiu.contains("inteiro outra vez"),
        "the sentence for a fallen transfer does not say that trying again \
         starts from zero: {caiu}"
    );

    // And the screen reaches for it on `Fell` and not on `Refused`: a refusal
    // has an explanation already travelling on the control stream, and a fall
    // has nothing coming ever.
    let andou = js_function(&read("ui/tela-sessao.js"), "function transferenciaAndou(");
    assert!(
        andou.contains("TRANSFERENCIAS.Fell"),
        "nothing on the screen writes the sentence for a fallen transfer"
    );
}

#[test]
fn every_refusal_the_dogma_can_send_has_a_sentence() {
    // A refusal reaching somebody as the word FALHA is a refusal that teaches
    // nothing. Read against the enum itself, so a variant added on the wire
    // without a sentence fails here rather than in front of a person.
    let control = read("../../crates/seele-proto/src/control.rs");
    let Some(enumeracao) = control
        .split("pub enum AttachmentRefusal {")
        .nth(1)
        .and_then(|resto| resto.split("\n}").next())
    else {
        panic!("`AttachmentRefusal` is gone from the protocol");
    };
    let variantes: Vec<String> = without_comments(enumeracao)
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(',') || line.ends_with('{'))
        .filter_map(|line| {
            let name = line.trim_end_matches([',', ' ', '{']);
            name.chars()
                .next()
                .filter(char::is_ascii_uppercase)
                .map(|_| name.to_owned())
        })
        .collect();
    assert!(
        variantes.len() >= 8,
        "the variant list came back too short to be the enum: {variantes:?}"
    );

    let frases = without_comments(&read("ui/frases.js"));
    let Some(bloco) = frases
        .split("const ANEXOS = {")
        .nth(1)
        .and_then(|resto| resto.split("\n};").next())
    else {
        panic!("`ANEXOS` is gone from ui/frases.js");
    };
    for variante in &variantes {
        assert!(
            bloco.contains(&format!("{variante}:")),
            "the Dogma can refuse with `{variante}` and `ANEXOS` has no \
             sentence for it"
        );
    }

    // The one that carries a number carries it into the sentence: "too big"
    // with no number sends somebody to try again with a file that is also too
    // big.
    let frase = js_function(&read("ui/frases.js"), "function fraseDeAnexo(");
    assert!(
        frase.contains("limit"),
        "the sentence for a file that is too large never mentions the limit"
    );
}

#[test]
fn no_screen_of_this_product_opens_a_file() {
    // The one point of ADR 0027 where it is possible to be strict, and it is
    // strict. Guarding for the absence of a thing, so it is read against the
    // whole frontend rather than one function: an "open" button added anywhere
    // is the failure.
    let mut fonte = String::new();
    for name in ui_files(".js") {
        fonte.push_str(&without_comments(&read(&format!("ui/{name}"))));
    }
    fonte.push_str(&without_comments(&read("ui/index.html")));

    for proibido in ["abrir_anexo", "openPath", "shell.open", "revealItemInDir"] {
        assert!(
            !fonte.contains(proibido),
            "the frontend reaches for `{proibido}`: no client of the SEELE opens \
             a file, and this is the only place in the whole design where being \
             strict is possible"
        );
    }

    // And the block a message draws offers exactly one verb.
    let bloco = js_function(&read("ui/tela-sessao.js"), "function blocoDeAnexo(");
    assert!(
        bloco.contains("anexo-salvar"),
        "an attachment offers no way to save it: {bloco}"
    );
    assert!(
        !bloco.to_lowercase().contains("abrir"),
        "the attachment block offers something other than saving: {bloco}"
    );
}

#[test]
fn saving_says_out_loud_what_this_product_does_not_promise() {
    // ADR 0027 has two things nobody may discover afterwards: **whoever hosts
    // the Dogma could read this file**, and **the SEELE does not scan for
    // viruses**. Both are true, and both belong in front of the person before
    // they press, not on a help page.
    let salvar = js_function(&read("ui/tela-sessao.js"), "function salvarAnexo(");
    assert!(
        salvar.contains("armarAto("),
        "saving skips the confirmation every consequential act goes through"
    );
    assert!(
        salvar.contains("não varre vírus"),
        "the confirmation does not say that this product does not scan for \
         viruses, which is the thing it must not let anybody assume: {salvar}"
    );
    assert!(
        salvar.contains("quarentena"),
        "the confirmation does not mention the quarantine mark, which is the \
         one concrete guard this product can point at"
    );
    assert!(
        salvar.contains("chegou inteiro"),
        "the confirmation does not separate the question that has an answer \
         from the one that does not"
    );
    // And it says where the file lands. Choosing a file to send now opens a
    // system dialog; saving one that arrived still does not, so this side has
    // no dialog to read the destination off and has to write it out.
    assert!(
        salvar.contains("destino"),
        "the confirmation does not say where the file will be written"
    );
}

#[test]
fn there_is_no_blocklist_of_extensions_anywhere_in_the_frontend() {
    // ADR 0027 refuses one on purpose, and refusing it is a decision that has to
    // survive somebody adding one "just to be safe": a list is worked around
    // with a `rename`, it breaks sending a friend a build of this very project,
    // and — worse than both — it makes whatever got through look checked.
    let mut fonte = String::new();
    for name in ui_files(".js") {
        fonte.push_str(&without_comments(&read(&format!("ui/{name}"))));
    }
    fonte.push_str(&without_comments(&read("src/main.rs")));

    for suspeito in [".exe\"", ".bat\"", ".scr\"", ".cmd\"", ".ps1\""] {
        assert!(
            !fonte.contains(suspeito),
            "something in the shell names `{suspeito}`, which is how a blocklist \
             of extensions starts — and ADR 0027 explains why one is worse than \
             none"
        );
    }
}

#[test]
fn a_message_whose_file_expired_still_says_what_the_file_was() {
    // The whole reason the Dogma keeps the attachment row after deleting the
    // bytes. Without this the message renders as a message with nothing in it,
    // and nobody learns there was ever a file.
    let bloco = js_function(&read("ui/tela-sessao.js"), "function blocoDeAnexo(");
    assert!(
        bloco.contains("EXPIROU"),
        "an expired attachment says nothing, so it is indistinguishable from a \
         message that never had one: {bloco}"
    );
    assert!(
        bloco.contains("anexo.file_name") && bloco.contains("anexo.byte_size"),
        "the name and the size are not drawn, so the sentence has nothing to be \
         about"
    );
    // The name and the size are drawn **before** the branch, so they survive it.
    let antes = bloco.split("anexo.expired").next().unwrap_or_default();
    assert!(
        antes.contains("anexo.file_name"),
        "the name is only drawn on the branch where the bytes are still there"
    );
}

#[test]
fn the_enumerated_reason_reaches_the_screen_and_is_not_only_carried() {
    // `ANEXOS` having a sentence for every variant is half of it. The half that
    // has been forgotten in this repository before is the other one: a
    // dictionary nobody looks up. `AttachmentRefusal` crossed the wire, reached
    // `Room`, reached the bridge — and if no screen called `fraseDeAnexo`, every
    // one of those sentences would be dead text.
    let sessao = read("ui/tela-sessao.js");
    let andou = js_function(&sessao, "function transferenciaAndou(");
    assert!(
        andou.contains("fraseDeAnexo("),
        "no screen turns the refusal into a sentence, so `ANEXOS` is a \
         dictionary nobody looks up: {andou}"
    );
    assert!(
        andou.contains("RefusedBecause") && andou.contains("Unavailable"),
        "the screen handles neither of the two shapes the reason arrives in"
    );

    // And the bridge really emits them, rather than collecting them in a queue
    // that nothing drains.
    let ponte = without_comments(&read("../../crates/seele-ffi/src/lib.rs"));
    assert!(
        ponte.contains("drain_transfers()"),
        "the bridge collects the reasons and never hands them on"
    );
    assert!(
        ponte.contains("Transfer::RefusedBecause") && ponte.contains("Transfer::Unavailable"),
        "the bridge has no way to carry an enumerated refusal to a shell"
    );
}

#[test]
fn the_bridge_maps_every_refusal_the_wire_can_carry() {
    // Mirrored rather than re-exported, so this is where the two lists can
    // drift. A variant added on the wire and forgotten here would compile —
    // `refusal_of` matches exhaustively, so it would not — but a variant added
    // to the bridge and never produced would be a sentence nobody can reach.
    let control = read("../../crates/seele-proto/src/control.rs");
    let Some(enumeracao) = control
        .split("pub enum AttachmentRefusal {")
        .nth(1)
        .and_then(|resto| resto.split("\n}").next())
    else {
        panic!("`AttachmentRefusal` is gone from the protocol");
    };
    let ponte = without_comments(&read("../../crates/seele-ffi/src/types.rs"));
    let Some(espelho) = ponte
        .split("pub enum AttachmentRefusal {")
        .nth(1)
        .and_then(|resto| resto.split("\n}").next())
    else {
        panic!("the bridge no longer mirrors `AttachmentRefusal`");
    };

    for line in without_comments(enumeracao).lines() {
        let line = line.trim();
        let name = line.trim_end_matches([',', ' ', '{']);
        if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            continue;
        }
        assert!(
            espelho.contains(name),
            "the wire can refuse with `{name}` and the bridge has no variant \
             for it, so it would reach a shell as nothing"
        );
    }
}

#[test]
fn the_name_travels_as_it_was_and_the_type_is_carried_as_a_claim() {
    // ADR 0027: **não renomeia, não corta extensão.** Renaming an `.exe` to
    // look harmless makes the file lie, and lying is the last thing that helps
    // here. The declared type is registered as a claim and nothing downstream
    // decides what to decode from it alone.
    let main = without_comments(&read("src/main.rs"));
    let descrever = body_of(&main, "fn descrever_arquivo(caminho: String)");
    for suspeito in ["trim_end_matches", "replace(", "sanitiz", "strip_suffix"] {
        assert!(
            !descrever.contains(suspeito),
            "`descrever_arquivo` reaches for `{suspeito}`: the name a person \
             gave a file has to travel as it is"
        );
    }
    assert!(
        descrever.contains("file_name()"),
        "the name is no longer read off the path as it stands: {descrever}"
    );

    // And the type really is derived from the extension and used for nothing
    // but the record — the whole reason it is called a claim.
    let tipo = body_of(&main, "fn tipo_alegado(nome: &str) -> String");
    assert!(
        tipo.contains("application/octet-stream"),
        "an unknown extension no longer falls back to «bytes», which is the \
         only honest answer when nothing is known: {tipo}"
    );
}

/// Every element id a stretch of script writes to, in order.
fn ids_reached_for(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("$(\"") {
        rest = &rest[at + 3..];
        let Some(end) = rest.find('"') else { break };
        out.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    out
}

/// Every element id that a stylesheet clips out of sight.
///
/// There is one such region and it is `#anuncio`: a one-pixel box with
/// `clip-path: inset(50%)`, which exists so a screen reader hears what changed.
/// Nothing written only there is visible to anybody looking at the screen —
/// which is the whole of the bug this pair of guards was written for.
///
/// Found by reading the stylesheets rather than by naming the id, because the
/// point is the *property*: a second hidden region added tomorrow has to be
/// caught by the same rule.
fn ids_clipped_off_screen() -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    for name in ui_files(".css") {
        let folha = without_comments(&read(&format!("ui/{name}")));
        for bloco in folha.split('}') {
            let Some((seletor, regras)) = bloco.split_once('{') else {
                continue;
            };
            if !regras.contains("clip-path: inset(50%)") {
                continue;
            }
            for parte in seletor.split([',', ' ', '>']) {
                if let Some(classe) = parte.trim().strip_prefix('.') {
                    classes.insert(classe.to_owned());
                }
            }
        }
    }
    assert!(
        !classes.is_empty(),
        "no stylesheet clips anything out of sight any more, so this guard is \
         measuring nothing — if the screen-reader-only region moved, follow it"
    );

    let pagina = without_comments(&read("ui/index.html"));
    let mut ids = BTreeSet::new();
    for tag in pagina.split('<') {
        let Some(tag) = tag.split('>').next() else {
            continue;
        };
        let Some(id) = atributo(tag, "id") else {
            continue;
        };
        let Some(class) = atributo(tag, "class") else {
            continue;
        };
        if class.split_whitespace().any(|c| classes.contains(c)) {
            ids.insert(id);
        }
    }
    ids
}

/// One attribute of one opening tag, if it has it.
fn atributo(tag: &str, nome: &str) -> Option<String> {
    let agulha = format!("{nome}=\"");
    let at = tag.find(&agulha)?;
    let rest = &tag[at + agulha.len()..];
    Some(rest[..rest.find('"')?].to_owned())
}

#[test]
fn the_attachment_button_opens_a_chooser() {
    // The decision this reverses is written down in ADR 0027 and was reverted by
    // the first person to use it: the button announced «arraste um arquivo» and
    // opened nothing, because dragging was the only way to choose a file. He
    // clicked it expecting a chooser, and dragging is not something anybody
    // discovers on their own.
    let sessao = read("ui/tela-sessao.js");
    let fonte = without_comments(&sessao);

    // What the click is wired to, read off the registration itself rather than
    // assumed.
    let Some(at) = fonte.find("$(\"botao-anexar\").addEventListener(\"click\"") else {
        panic!("nothing is listening for a press on the attachment button at all");
    };
    let registro = &fonte[at..at + fonte[at..].find(");").unwrap_or(0) + 1];

    assert!(
        registro.contains("abrirSeletorDeArquivo"),
        "the attachment button no longer reaches the chooser: {registro}"
    );

    // And the chooser is a real one: a command that opens the system dialog,
    // not a sentence describing how to drag.
    let abrir = js_function(&sessao, "async function abrirSeletorDeArquivo(");
    assert!(
        abrir.contains("invoke(\"escolher_arquivo\""),
        "the button opens nothing: whatever it calls does not ask the shell for \
         a file chooser: {abrir}"
    );
    assert!(
        abrir.contains("guardarAnexo("),
        "a file is chosen and never lands on the screen: {abrir}"
    );
}

#[test]
fn a_file_that_cannot_be_read_says_so_where_it_can_be_seen() {
    // The second half of the same report, and the subtler one. Both attachment
    // failures used to be reported through `anunciar(…)` alone — and `.anuncio`
    // is a one-pixel box clipped off the screen for screen readers. To the
    // person who was looking at the window, a refused file and nothing at all
    // are the same event.
    let sessao = read("ui/tela-sessao.js");
    let recusar = js_function(&sessao, "function recusarAnexo(");

    let escondidos = ids_clipped_off_screen();
    let escritos = ids_reached_for(&recusar);
    assert!(
        !escritos.is_empty(),
        "the refusal writes to no element at all, so it is invisible again: \
         {recusar}"
    );
    assert!(
        escritos.iter().any(|id| !escondidos.contains(id)),
        "every element the refusal writes to is clipped off the screen \
         ({escondidos:?}), so nothing changes for somebody looking at it"
    );
    // And the box it writes into is opened by the same function: a phrase
    // written into a `hidden` container is as invisible as the clipped one.
    assert!(
        recusar.contains(".hidden = false"),
        "the refusal writes into a box it never unhides: {recusar}"
    );

    // Both ways of choosing a file route their failure through it. The drag was
    // the one that had the bug; the chooser is new and would have grown the same
    // one by copying its neighbour.
    for entrada in [
        "async function escolherAnexo(",
        "async function abrirSeletorDeArquivo(",
    ] {
        let corpo = js_function(&sessao, entrada);
        assert!(
            corpo.contains("recusarAnexo("),
            "`{entrada}` reports its failure some other way, and the only other \
             way this file has is the clipped region: {corpo}"
        );
    }
}

#[test]
fn a_file_offered_with_no_line_open_is_refused_out_loud() {
    // Both doors give up when there is no Line to send to, and both used to give
    // up with a bare `return`. From the outside that is the same event as the
    // bug this whole file was written for: the person acts, and the window does
    // not move.
    let sessao = read("ui/tela-sessao.js");
    for entrada in [
        "listen(\"tauri://drag-drop\"",
        "async function abrirSeletorDeArquivo(",
    ] {
        let corpo = js_function(&sessao, entrada);
        let Some(depois) = corpo.split("linhaAberta === null").nth(1) else {
            panic!("`{entrada}` no longer checks whether a Line is open at all");
        };
        // Only as far as the end of that branch: a `recusarAnexo` further down,
        // on some other path, would say nothing about this one.
        let ramo = depois.split('}').next().unwrap_or_default();
        assert!(
            ramo.contains("recusarAnexo("),
            "`{entrada}` gives up in silence when no Line is open, which reads \
             exactly like a broken button: {ramo}"
        );
    }
}

#[test]
fn the_reason_a_rung_failed_is_not_prefixed_by_a_label_it_already_carries() {
    // From a real screen, on a real network: the detail line under the invite
    // read «o roteador respondeu: o roteador respondeu, e o endereço dele
    // (100.65.128.5) não sai para a internet».
    //
    // The label was added here without reading the sentences it would sit in
    // front of. All four `FalhaAoAbrir` variants name the router, and every
    // `FalhaNoEncontro` names the rendezvous — they are written as whole
    // explanations, because that is what `specs` asks of a failure.
    //
    // So the rule is the absence: this function must not build the detail text
    // by concatenating anything in front of the reason it was handed.
    let mostrar = body_of(&without_comments(&scripts()), "function mostrarAlcance");

    for prefixo in ["o roteador respondeu:", "o ponto de encontro:"] {
        assert!(
            !mostrar.contains(prefixo),
            "`mostrarAlcance` glues `{prefixo}` in front of a sentence that already \
             says it, and the screen stutters:\n{mostrar}"
        );
    }

    // And the positive half, so the fix cannot be «delete the line». The reason
    // still has to reach the page.
    assert!(
        mostrar.contains("portaRecusada") && mostrar.contains("encontroRecusado"),
        "one of the two reasons stopped being drawn at all:\n{mostrar}"
    );
    assert!(
        mostrar.contains("convite-alcance-detalhe"),
        "the reason is drawn without the class that makes it secondary:\n{mostrar}"
    );
}

#[test]
fn arriving_at_a_dogma_opens_a_line_and_does_not_put_anybody_in_a_cage() {
    // Both used to happen together on entry, with one good reason between them:
    // arriving at an empty screen is arriving without knowing what to do. The
    // reason still holds for one of the two and never held for the other.
    //
    // Reading text is passive — nobody hears you for having read something — so
    // opening the first Line answers the empty screen and commits the person to
    // nothing.
    //
    // Entering a Cage is not passive. It takes one of fifteen seats, shows the
    // person as present, and puts a microphone at the disposal of a conversation
    // they did not pick. From the person who actually used it: «não dá para você
    // ficar fora de uma sala». They had never pressed anything.
    let entrar = body_of(&without_comments(&scripts()), "async function inserirPlug");

    assert!(
        entrar.contains("open_line"),
        "arriving no longer opens a Line, so the screen is empty again and the \
         reason the automatic step existed is lost:\n{entrar}"
    );
    assert!(
        !entrar.contains("insert_plug"),
        "arriving puts the person inside a Cage without them pressing anything — \
         a seat taken and a microphone offered to a conversation nobody \
         chose:\n{entrar}"
    );
}

#[test]
fn the_button_that_stopped_inserting_the_plug_stopped_saying_it_does() {
    // The other half, and the one that rots quietly: behaviour moves, the label
    // stays. A button reading «INSERIR PLUG» that no longer inserts is worse
    // than an unnamed one, because this particular promise is about a microphone.
    let script = without_comments(&scripts());
    let segundo = body_of(&script, "async function verificarIdentidade");

    assert!(
        !segundo.contains("INSERIR PLUG"),
        "the second step still calls itself «INSERIR PLUG», and it no longer \
         inserts anything:\n{segundo}"
    );
    assert!(
        segundo.contains("botao.textContent"),
        "the second step stopped naming itself at all, so the button keeps \
         whatever the first step wrote:\n{segundo}"
    );
}
