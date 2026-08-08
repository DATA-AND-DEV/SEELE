//! End-to-end protocol conformance tests.
//!
//! This crate is deliberately empty. Everything lives in `tests/`, because the
//! point is not to ship code but to hold the one place where `magi-server` and
//! `magi-core` are allowed to meet.
//!
//! ADR 0002 forbids either from depending on the other: the daemon must not link
//! the headless client, and the client must not link the daemon. An end-to-end
//! test needs both, so it gets its own crate rather than a hole in the rule.
