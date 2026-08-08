//! Enforces the dependency rule from `specs/01-arquitetura.md`.
//!
//! > `proto` depends on nobody. `audio` depends only on `proto`. `core` depends
//! > on `proto` and `audio`. Everything else depends on `core`. Never the
//! > inverse.
//!
//! `specs/01-arquitetura.md` also calls the core/shell boundary "the most
//! important contract in the project — if it leaks, the project becomes three
//! applications". A contract checked only by review is a contract that erodes,
//! so this turns it into a red build.
//!
//! # Why this check is not redundant with Cargo
//!
//! Cargo already rejects an inverted *normal* dependency, because among the
//! existing crates every inversion forms a cycle. This check earns its keep on
//! the two classes Cargo does not catch:
//!
//! - **Dev-dependencies.** Cargo tolerates cycles through them, so
//!   `proto` dev-depending on `audio` compiles fine and silently inverts the
//!   graph.
//! - **Sideways edges.** `magi-tui` depending directly on `magi-proto` is not a
//!   cycle, so Cargo is happy — but it is exactly how protocol knowledge leaks
//!   into a shell.
//!
//! # Deliberate divergence from the literal text of specs/01-arquitetura.md
//!
//! The spec says "everything else depends on `core`". Taken literally that is
//! wrong in two places, and this table encodes the stricter reading instead.
//! See `docs/adr/0002-regra-de-dependencia.md`.
//!
//! - `magi-server` must **not** depend on `magi-core`, which is the headless
//!   *client*, nor on `magi-audio`. The server is an SFU: `specs/04` states it
//!   "never decodes Opus". Linking the audio crate would drag `cpal` and
//!   `libopus` into a daemon that is supposed to fit in 1 vCPU / 512 MB.
//! - Shells depend on `magi-core` **only**, not additionally on `proto` and
//!   `audio`. A shell that can name an `ssrc` already has logic in it.

use std::process::ExitCode;

use cargo_metadata::{DependencyKind, MetadataCommand};

/// The allow-list. See the module docs for where it tightens the spec.
///
/// A workspace crate absent from this table is a hard error rather than an
/// implicit pass: adding a crate must be a deliberate act that states where the
/// new crate sits in the graph.
const RULES: &[(&str, &[&str])] = &[
    ("magi-proto", &[]),
    ("magi-audio", &["magi-proto"]),
    ("magi-core", &["magi-proto", "magi-audio"]),
    // The daemon speaks the wire format and nothing else.
    ("magi-server", &["magi-proto"]),
    // Shells translate events into pixels and input into commands. Nothing more.
    ("magi-tui", &["magi-core"]),
    ("magi-ffi", &["magi-core"]),
    // The one deliberate exception, and it has a name so it cannot spread.
    // An end-to-end protocol test needs a server and a client at once, and the
    // rule above forbids either depending on the other. This crate ships
    // nothing: every dependency is a dev-dependency and its library is empty.
    (
        "magi-conformance",
        // The crate that proves the others meet the acceptance criteria is the
        // one place allowed to see all of them, the interface included.
        &[
            "magi-proto",
            "magi-audio",
            "magi-core",
            "magi-server",
            "magi-tui",
        ],
    ),
    // Tooling. Must not depend on the product, or `cargo xtask` would need the
    // product to compile before it could check the product.
    ("xtask", &[]),
];

/// One offending edge in the workspace graph.
#[derive(Debug, PartialEq, Eq)]
struct Violation {
    message: String,
}

/// Is this a crate the architecture rule governs?
fn is_workspace_crate(name: &str) -> bool {
    RULES.iter().any(|(crate_name, _)| *crate_name == name)
}

/// Pure rule evaluation for a single crate, kept free of `cargo_metadata` so it
/// can be tested without a workspace on disk.
fn evaluate(name: &str, edges: &[(String, &'static str)]) -> Vec<Violation> {
    let Some((_, allowed)) = RULES.iter().find(|(crate_name, _)| *crate_name == name) else {
        return vec![Violation {
            message: format!(
                "crate `{name}` is a workspace member but has no entry in RULES.\n    \
                 Add it to xtask/src/check_deps.rs and state which crates it may depend on."
            ),
        }];
    };

    edges
        .iter()
        .filter(|(dependency, _)| is_workspace_crate(dependency))
        .filter(|(dependency, _)| !allowed.contains(&dependency.as_str()))
        .map(|(dependency, kind)| Violation {
            message: format!(
                "`{name}` declares a {kind} on `{dependency}`, which the dependency \
                 rule forbids.\n    \
                 Allowed for `{name}`: {}.\n    \
                 specs/01-arquitetura.md: proto → audio → core → shells, never the inverse.",
                if allowed.is_empty() {
                    "nothing in the workspace".to_owned()
                } else {
                    allowed.join(", ")
                }
            ),
        })
        .collect()
}

fn describe(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Development => "dev-dependency",
        DependencyKind::Build => "build-dependency",
        _ => "dependency",
    }
}

pub(crate) fn run() -> ExitCode {
    let metadata = match MetadataCommand::new().no_deps().exec() {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("xtask check-deps: could not read cargo metadata: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mut violations: Vec<Violation> = Vec::new();
    let mut checked = 0_usize;

    for package in metadata.workspace_packages() {
        let name = package.name.to_string();
        let edges: Vec<(String, &'static str)> = package
            .dependencies
            .iter()
            .map(|dependency| (dependency.name.to_string(), describe(dependency.kind)))
            .collect();

        violations.extend(evaluate(&name, &edges));
        checked += 1;
    }

    if violations.is_empty() {
        println!("check-deps: dependency rule holds across {checked} workspace crates.");
        return ExitCode::SUCCESS;
    }

    eprintln!("check-deps: dependency rule violated.\n");
    for violation in &violations {
        eprintln!("  - {}", violation.message);
    }
    eprintln!();
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(name: &str) -> (String, &'static str) {
        (name.to_owned(), "dependency")
    }

    fn dev_edge(name: &str) -> (String, &'static str) {
        (name.to_owned(), "dev-dependency")
    }

    #[test]
    fn allowed_edges_pass() {
        assert!(evaluate("magi-audio", &[edge("magi-proto")]).is_empty());
        assert!(evaluate("magi-core", &[edge("magi-proto"), edge("magi-audio")]).is_empty());
        assert!(evaluate("magi-tui", &[edge("magi-core")]).is_empty());
    }

    #[test]
    fn third_party_edges_are_ignored() {
        assert!(evaluate("magi-proto", &[edge("serde"), edge("postcard")]).is_empty());
    }

    #[test]
    fn inverted_edge_is_rejected() {
        assert_eq!(evaluate("magi-proto", &[edge("magi-core")]).len(), 1);
        assert_eq!(evaluate("magi-audio", &[edge("magi-core")]).len(), 1);
    }

    #[test]
    fn inverted_dev_dependency_is_rejected() {
        // Cargo tolerates cycles through dev-dependencies; this is the class of
        // violation that only this check can catch.
        assert_eq!(evaluate("magi-proto", &[dev_edge("magi-audio")]).len(), 1);
    }

    #[test]
    fn shell_reaching_past_core_is_rejected() {
        // Not a cycle, so Cargo is perfectly happy — and this is exactly how
        // protocol knowledge leaks into a shell.
        assert_eq!(evaluate("magi-tui", &[edge("magi-proto")]).len(), 1);
        assert_eq!(evaluate("magi-ffi", &[edge("magi-audio")]).len(), 1);
    }

    #[test]
    fn server_must_not_link_the_client_or_the_audio_stack() {
        assert_eq!(evaluate("magi-server", &[edge("magi-core")]).len(), 1);
        assert_eq!(evaluate("magi-server", &[edge("magi-audio")]).len(), 1);
        assert!(evaluate("magi-server", &[edge("magi-proto")]).is_empty());
    }

    #[test]
    fn the_conformance_crate_may_see_both_ends() {
        // The documented exception. It exists so an end-to-end test has a home
        // that is not a hole in the rule.
        assert!(evaluate(
            "magi-conformance",
            &[edge("magi-server"), edge("magi-core"), edge("magi-proto")]
        )
        .is_empty());
    }

    #[test]
    fn the_exception_does_not_leak_to_anybody_else() {
        // If a second crate ever needs both ends, that is a design change to
        // argue about, not a table entry to copy.
        assert_eq!(evaluate("magi-server", &[edge("magi-core")]).len(), 1);
        assert_eq!(evaluate("magi-core", &[edge("magi-server")]).len(), 1);
    }

    #[test]
    fn unknown_workspace_crate_is_rejected() {
        assert_eq!(evaluate("magi-something-new", &[]).len(), 1);
    }
}
