//! The compile-fail lane for the cart (microstep 1.4.3). `trybuild` builds the
//! fixture as its own crate and diffs rustc's stderr against the golden beside
//! it; a fixture that compiles is a failure. Regenerate the golden deliberately,
//! and read the diff:
//!     TRYBUILD=overwrite cargo test -p pos-domain --test cart_ui

/// A line description cannot be deserialized, so no request from the screen
/// carries a price into a line: the price comes from a `Product` or a resolved
/// scan, inside the register.
#[test]
fn an_add_line_never_arrives_from_outside_with_its_price() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/add_line_cannot_be_deserialized.rs");
}

/// A parked cart is its own state (microstep 1.4.4): a cart being built cannot
/// be resumed, and a parked cart cannot be changed until it is.
#[test]
fn a_cart_being_built_cannot_be_resumed() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/parked_cart_is_its_own_state.rs");
}
