//! `parse_scan` against arbitrary bytes, as a scanner emits them: truncated,
//! overlong, non-numeric, not text at all (microstep 1.2.8). A panic here is a
//! lost sale at the till, and `check` also fails a clean parse of a damaged
//! label.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pos_domain_fuzz::parse_scan::{bounded_fixture_rules, check};

fuzz_target!(|input: &[u8]| {
    let rules = bounded_fixture_rules();
    check(input, &rules);
});
