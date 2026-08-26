//! The compile-fail lane. `trybuild` builds each `tests/ui/*.rs` as its own
//! crate and diffs rustc's stderr against the golden beside it; a fixture that
//! compiles is a failure. Regenerate a golden deliberately, and read the diff:
//!     TRYBUILD=overwrite cargo test -p pos-domain --test typed_ids_ui

#[test]
fn typed_ids_do_not_interconvert() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/typed_ids_do_not_interconvert.rs");
}
