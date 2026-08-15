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

    // `fontes.css` is on this list for the same reason as the other three, and
    // it is the one whose absence would be hardest to notice: the page would
    // still render, in Arial Narrow. `tests/fontes.rs` guards what is inside it.
    //
    // The attribute, not the bare name. A bare `contains` was an implicit tag
    // check only while each name appeared exactly once in the file; the comment
    // above the `<link>` block now names `tokens.css` and `fontes.css` in prose,
    // and with that the assertion became satisfiable by the explanation of the
    // tag it was meant to guard. Deleting the `fontes.css` link left all
    // nineteen tests green. Same defect class as the one `without_comments`
    // exists for, one file over: a guard a comment can satisfy is a guard that
    // cannot fail.
    for (asset, attribute) in [
        ("fontes.css", "href"),
        ("tokens.css", "href"),
        ("seele.css", "href"),
        ("seele.js", "src"),
    ] {
        assert!(
            page.contains(&format!("{attribute}=\"{asset}\"")),
            "index.html never loads {asset}"
        );
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
    let script = read("ui/seele.js");

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
    let script = read("ui/seele.js");
    let css = read("ui/seele.css");

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
        read("ui/seele.js").contains("payload === \"MessagesChanged\""),
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
    let script = without_comments(&read("ui/seele.js"));

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
    let script = without_comments(&read("ui/seele.js"));

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
    let css = without_comments(&read("ui/seele.css"));

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
    let script = without_comments(&read("ui/seele.js"));
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
