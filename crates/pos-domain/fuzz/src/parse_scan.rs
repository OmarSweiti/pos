//! What the `parse_scan` fuzz target asserts of every input, and the store
//! layouts it reads them with (microstep 1.2.8).
//!
//! One file, compiled twice: into the fuzz target, and by `#[path]` into
//! `pos-domain`'s `tests/parse_scan_fuzz.rs`, which replays the committed
//! corpus on every pull request. The weekly search and the per-PR replay
//! therefore assert the same thing, and a crash input committed to the corpus
//! is a regression test from that commit on, in the weeks the fuzzer does not
//! run as much as in the ones it does.

use pos_domain::catalog::EAN13_LEN;
use pos_domain::{
    EmbeddedBarcodeRule, EmbeddedValue, ScanError, claiming_rule, ean13_checksum_ok,
    parse_scan_bytes,
};

/// What one input came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No layout claims it: an ordinary barcode, looked up whole.
    Plain,
    /// A scale label a layout read, and what its value counts.
    Label(EmbeddedValue),
    /// Refused, and why.
    Refused(ScanError),
}

/// The store layouts every input is read with.
///
/// A fixed set, because a scanner's bytes are the hostile input here and the
/// store's configuration is not. But `0007` stores a layout's spans and divisor
/// unchecked, so the layouts no register should have are here as well: for
/// those, the parser is all that stands between a scan and a panic.
pub fn bounded_fixture_rules() -> Vec<EmbeddedBarcodeRule> {
    use EmbeddedValue::{PriceMinor, WeightMilli};
    let layout = |prefix: &str, item_code_span, value_span, value_kind, value_scale, verify| {
        EmbeddedBarcodeRule {
            prefix: prefix.to_owned(),
            item_code_span,
            value_span,
            value_kind,
            value_scale,
            verify_checksum: verify,
        }
    };
    vec![
        // The register fixture's deli scale (`apps/terminal`'s scan tests):
        // price labels on 22 and weight labels on 23, item code then value.
        layout("22", (2, 5), (7, 5), PriceMinor, 1, true),
        layout("23", (2, 5), (7, 5), WeightMilli, 1, true),
        // `pos-domain`'s own price layout, on a one-digit prefix that every
        // two-digit prefix here outranks.
        layout("2", (1, 5), (7, 5), PriceMinor, 1, true),
        // A scale that prints tenths of a fil: the divisor must divide exactly.
        layout("25", (2, 5), (7, 5), PriceMinor, 10, true),
        // A layout that does not verify the check digit, so can only guess (E.40).
        layout("26", (2, 5), (7, 5), WeightMilli, 1, false),
        // What the schema accepts and no register should have: a divisor of
        // zero, a value past the code's end, an empty value span, a span whose
        // end overflows, and an empty prefix, which claims nothing.
        layout("24", (2, 5), (7, 5), PriceMinor, 0, true),
        layout("27", (2, 5), (9, 5), PriceMinor, 1, true),
        layout("28", (2, 5), (7, 0), WeightMilli, 1, true),
        layout("29", (usize::MAX, 2), (7, 5), PriceMinor, 1, true),
        layout("", (0, 5), (5, 5), PriceMinor, 1, true),
    ]
}

/// Read `input` with `rules`, and assert what must hold whatever the bytes:
/// bytes that are not text are refused as such, an ordinary barcode is looked
/// up whole, and a clean parse of a label is thirteen digits, with a check digit
/// that agrees when its layout verifies one and the item code its layout's span
/// names.
///
/// # Panics
///
/// When `parse_scan_bytes` panics, or any of that does not hold: either one is
/// the fuzzer's finding.
pub fn check(input: &[u8], rules: &[EmbeddedBarcodeRule]) -> Outcome {
    let result = parse_scan_bytes(input, rules);
    let Ok(code) = core::str::from_utf8(input) else {
        assert_eq!(result, Err(ScanError::NotUtf8), "bytes that are not text");
        return Outcome::Refused(ScanError::NotUtf8);
    };
    let parsed = match result {
        Ok(parsed) => parsed,
        Err(error) => {
            assert_ne!(
                error,
                ScanError::NotUtf8,
                "text refused as not text: {code:?}"
            );
            return Outcome::Refused(error);
        }
    };
    let Some(rule) = claiming_rule(code, rules).and_then(|index| rules.get(index)) else {
        assert_eq!(
            parsed.item_code(),
            code,
            "an ordinary barcode is looked up whole"
        );
        return Outcome::Plain;
    };
    assert!(
        code.len() == EAN13_LEN && code.bytes().all(|byte| byte.is_ascii_digit()),
        "a clean parse of a label that is not thirteen digits: {code:?}"
    );
    assert!(
        !rule.verify_checksum || ean13_checksum_ok(code),
        "a clean parse of a label whose check digit disagrees: {code:?}"
    );
    let (start, len) = rule.item_code_span;
    assert_eq!(
        Some(parsed.item_code()),
        start.checked_add(len).and_then(|end| code.get(start..end)),
        "an item code that is not the layout's span of {code:?}"
    );
    Outcome::Label(rule.value_kind)
}
