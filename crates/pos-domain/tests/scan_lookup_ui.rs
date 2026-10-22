//! The compile-fail lane for scans (microstep 1.2.4). `trybuild` builds the
//! fixture as its own crate and diffs rustc's stderr against the golden beside
//! it; a fixture that compiles is a failure. Regenerate the golden deliberately,
//! and read the diff:
//!     TRYBUILD=overwrite cargo test -p pos-domain --test scan_lookup_ui

/// Only `resolve_scan` makes a price-embedded lookup: outside this crate the
/// variant cannot be built, the label constructor cannot be called, a parse
/// cannot be forged, and neither a price nor a lookup deserializes.
#[test]
fn only_scan_handler_constructs_price_embedded_lookup() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/price_embedded_lookup_is_not_constructible.rs");
    cases.compile_fail("tests/ui/parsed_scan_cannot_be_forged.rs");
}
