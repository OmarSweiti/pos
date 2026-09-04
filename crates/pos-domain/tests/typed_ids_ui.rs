//! The compile-fail lane. `trybuild` builds each `tests/ui/*.rs` as its own
//! crate and diffs rustc's stderr against the golden beside it; a fixture that
//! compiles is a failure. Regenerate a golden deliberately, and read the diff:
//!     TRYBUILD=overwrite cargo test -p pos-domain --test typed_ids_ui

#[test]
fn typed_ids_do_not_interconvert() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/typed_ids_do_not_interconvert.rs");
}

#[test]
fn authorized_tokens_cannot_be_forged_or_substituted() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/authorized_token_cannot_be_forged.rs");
    cases.compile_fail("tests/ui/authorized_capabilities_do_not_interconvert.rs");
}

#[test]
fn approval_handles_cannot_be_deserialized() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/approval_handle_cannot_be_deserialized.rs");
}
