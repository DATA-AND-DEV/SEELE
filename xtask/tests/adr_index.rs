//! The ADR index has to list every ADR.
//!
//! This exists because it did not hold. Eleven ADRs — 0016 through 0026 — sat in
//! `docs/adr/` without ever reaching the table in `docs/adr/README.md`, and the
//! gap went unnoticed for as long as it took to write eleven of them. Among the
//! missing were `0022`, which governs how a server is reached over the internet,
//! and `0026`, which governs how this product updates itself.
//!
//! An index that silently stops being an index is worse than no index: the file
//! still looks authoritative, so somebody reading it concludes the decision was
//! never written down and makes it again — differently.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

fn adr_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ always has a parent")
        .join("docs/adr")
}

/// Every `NNNN` that has a file, template excluded.
fn numbers_on_disk() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let entries = std::fs::read_dir(adr_dir()).expect("docs/adr must be readable");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name.starts_with("0000-") || name == "README.md" {
            continue;
        }
        let number: String = name.chars().take(4).collect();
        if number.chars().all(|c| c.is_ascii_digit()) {
            found.insert(number);
        }
    }
    found
}

#[test]
fn every_adr_on_disk_is_listed_in_the_index() {
    let readme = std::fs::read_to_string(adr_dir().join("README.md")).expect("README must exist");

    // Table rows only, and this is the whole subtlety.
    //
    // The first version of this searched the *whole file* for `[NNNN](`,
    // reasoning that a link is stricter than a bare number. It is — and it still
    // failed to catch anything, because the prose below the table links the same
    // ADRs it discusses: the paragraph beginning "IPv6/NAT traversal saiu desta
    // lista" carries a real `[0022](…)`. Deleting 0022's row left the test green.
    //
    // That prose paragraph is exactly the state this test exists to fail in: it
    // was written while the row was missing. A guard the surrounding text can
    // satisfy is a guard that cannot fail.
    let rows: String = readme
        .lines()
        .filter(|line| line.starts_with("| ["))
        .collect::<Vec<_>>()
        .join("\n");

    let missing: Vec<String> = numbers_on_disk()
        .into_iter()
        .filter(|number| !rows.contains(&format!("[{number}](")))
        .collect();

    assert!(
        missing.is_empty(),
        "docs/adr/ holds ADRs the index never links: {missing:?}\n\
         An index that quietly stops indexing still reads as authoritative, so the \
         next person concludes the decision was never written and makes it again."
    );
}

#[test]
fn the_index_links_no_adr_that_does_not_exist() {
    // The other direction, and the one a rename breaks: a row pointing at a file
    // that is not there renders as a dead link in every viewer, and says nothing
    // at all in a plain-text read.
    let readme = std::fs::read_to_string(adr_dir().join("README.md")).expect("README must exist");

    let mut broken = Vec::new();
    for piece in readme.split("](").skip(1) {
        let Some(target) = piece.split(')').next() else {
            continue;
        };
        if !target.ends_with(".md") || target.contains('/') {
            continue;
        }
        if !adr_dir().join(target).exists() {
            broken.push(target.to_owned());
        }
    }

    assert!(
        broken.is_empty(),
        "the ADR index links files that are not in docs/adr/: {broken:?}"
    );
}

/// The status an ADR declares, as one word, or `None` if this is not that line.
///
/// Everything after the first word is prose about the status — «(era `proposto`
/// desde M2)», «· substitui a primeira redação deste ADR», «, construído em
/// parte» — and reading it is how this check used to answer the wrong question.
fn status_value(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("Status:")
        .or_else(|| line.strip_prefix("**Estado:**"))
        .or_else(|| line.strip_prefix("Estado:"))?;
    let word: String = rest
        .trim()
        .trim_start_matches('*')
        .chars()
        .take_while(|c| c.is_alphabetic())
        .collect();
    (!word.is_empty()).then_some(word.to_lowercase())
}

#[test]
fn the_status_is_read_as_a_word_and_never_as_a_sentence_about_it() {
    // Proof by the line that broke it. Both of these are accepted ADRs whose
    // status line goes on to mention something else; a `contains` reading calls
    // the first one proposed.
    assert_eq!(
        status_value("Status: aceito (era `proposto` desde M2)").as_deref(),
        Some("aceito")
    );
    assert_eq!(
        status_value("**Estado:** aceito, construído em parte").as_deref(),
        Some("aceito")
    );
    assert_eq!(
        status_value("**Estado:** proposto — **nada foi construído**").as_deref(),
        Some("proposto")
    );
    assert_eq!(status_value("**Data:** 2026-08-18"), None);
    assert_eq!(status_value("Contexto: o ADR 0021 pôs um porteiro"), None);
}

#[test]
fn the_index_agrees_with_each_adr_about_whether_it_is_still_only_proposed() {
    // The half that rots without anybody touching the table. An ADR moves from
    // `proposto` to `aceito` inside its own file — 0022 did exactly that today —
    // and the row keeps saying `proposto` for as long as nobody remembers it is
    // written in two places. A reader who trusts the index then treats a decision
    // in force as one still up for debate.
    let readme = std::fs::read_to_string(adr_dir().join("README.md")).expect("README must exist");

    for number in numbers_on_disk() {
        let file = std::fs::read_dir(adr_dir())
            .expect("docs/adr must be readable")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.starts_with(&number) && name.ends_with(".md"))
            .expect("the number came from this directory");

        let body = std::fs::read_to_string(adr_dir().join(&file)).expect("ADR must be readable");

        // Two spellings live in this directory: `Status: aceito` in the older
        // ones and `**Estado:** aceito` in the newer. Reading only one of them is
        // how this check would pass by never looking.
        //
        // **The word this reads is the first one, and that is the whole fix of
        // 2026-09-05.** Until then it asked whether the status *line* contained
        // `proposto` anywhere, and ADR 0006 says `Status: aceito (era
        // `proposto` desde M2)` — a line that records what the status used to
        // be. The guard read it as still proposed, the index row said
        // `**proposto**`, the two agreed, and the test was green for months
        // while the index misrepresented an accepted decision. A guard that
        // compares two readings of the same sentence agrees with itself; this
        // one has to read the status, which is one word.
        let declares_proposed = body
            .lines()
            .take(20)
            .find_map(status_value)
            .is_some_and(|value| value == "proposto");

        let Some(row) = readme
            .lines()
            .find(|line| line.starts_with("| [") && line.contains(&format!("[{number}](")))
        else {
            continue; // the first test above is what reports this
        };
        let row_says_proposed = row.contains("proposto");

        assert_eq!(
            declares_proposed, row_says_proposed,
            "ADR {number} and the index disagree about whether it is still only proposed.\n\
             {file} says proposed: {declares_proposed}\n\
             index row says proposed: {row_says_proposed}\n\
             {row}"
        );
    }
}
