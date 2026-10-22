//! The `fixtures` feature is a developer's tool, never product.
//!
//! It writes placeholder PIN hashes and invented chain hashes, and it panics on
//! any refusal. The terminal's tests enable it through a dev-dependency, which
//! never reaches a build that ships (resolver 3) — and this test is what holds
//! the terminal's *shipped* graph to that, the way
//! `the_register_bundle_never_enables_the_emulator` holds it to `pos-hardware`'s
//! `emulator`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

/// `cargo tree` of the terminal, with the features each dependency enables,
/// over the given edge kinds.
fn terminal_tree(edges: &str) -> String {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace");
    let output = Command::new(env!("CARGO"))
        .current_dir(workspace)
        .args([
            "tree",
            "--locked",
            "--offline",
            "-p",
            "terminal",
            "-e",
            edges,
            "--prefix",
            "none",
        ])
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const FIXTURES: &str = "pos-db feature \"fixtures\"";

#[test]
fn the_register_never_ships_the_sale_fixture() {
    // What ships: normal and build edges only, with their features.
    let shipped = terminal_tree("normal,build,features");
    assert!(
        shipped.lines().any(|line| line.starts_with("pos-db v")),
        "the terminal ships pos-db, so this check is about something real:\n{shipped}"
    );
    assert!(
        !shipped.contains(FIXTURES),
        "the register's shipped graph enables the fixtures:\n{shipped}"
    );

    // The same matcher does see the feature where it is meant to be: the
    // terminal's tests. A guard that could not see it would pass forever.
    let with_tests = terminal_tree("normal,build,dev,features");
    assert!(
        with_tests.contains(FIXTURES),
        "the terminal's tests no longer enable the fixtures, so the check above \
         can no longer be told apart from a matcher that never matches:\n{with_tests}"
    );
}
