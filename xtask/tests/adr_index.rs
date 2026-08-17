//! The ADR index has to list every ADR.
//!
//! This exists because it did not hold. Eleven ADRs — 0016 through 0026 — sat in
//! `docs/adr/` without ever reaching the table in `docs/adr/README.md`, and the
//! gap went unnoticed for as long as it took to write eleven of them. Among the
//! missing were `0022`, which governs how a Dogma is reached over the internet,
//! and `0026`, which governs how this product updates itself.
//!
//! An index that silently stops being an index is worse than no index: the file
//! still looks authoritative, so somebody reading it concludes the decision was
//! never written down and makes it again — differently.

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

    // The link, not the bare number. A bare `contains("0022")` would be
    // satisfied by the paragraph further down that *talks about* 0022 in prose —
    // and that paragraph existed while the table row did not, which is exactly
    // the state this test has to be able to fail in.
    let missing: Vec<String> = numbers_on_disk()
        .into_iter()
        .filter(|number| !readme.contains(&format!("[{number}](")))
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
        let declares_proposed = body.lines().take(20).any(|line| {
            line.contains("roposto") && (line.contains("tatus") || line.contains("stado"))
        });

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
