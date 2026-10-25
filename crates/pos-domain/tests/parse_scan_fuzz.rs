//! The `parse_scan` fuzz target's committed corpus, replayed on every pull
//! request (microstep 1.2.8).
//!
//! The fuzzer runs weekly and never per pull request: a time-boxed search is
//! not a deterministic merge check. Its corpus is deterministic, so it runs
//! here, through the target's own `check`, which this test compiles from the
//! target's file rather than copies. A crash input committed to the corpus is a
//! regression test from that commit on.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../fuzz/src/parse_scan.rs"]
mod parse_scan;

use std::mem::discriminant;
use std::panic::catch_unwind;
use std::path::Path;

use parse_scan::{Outcome, bounded_fixture_rules, check};
use pos_domain::{BarcodeKind, EmbeddedValue, ScanError};

/// A name libFuzzer gave an input: a bare SHA-1 for a corpus entry, or one
/// after the kind of finding for an artifact moved into the corpus.
fn named_by_libfuzzer(name: &str) -> bool {
    let hash = ["crash-", "leak-", "timeout-", "oom-", "slow-unit-"]
        .iter()
        .find_map(|kind| name.strip_prefix(kind))
        .unwrap_or(name);
    hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[test]
fn parse_scan_fuzz_seed_corpus_never_panics() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/corpus/parse_scan");
    let rules = bounded_fixture_rules();
    let mut panicked = Vec::new();
    let mut seeds = Vec::new();
    for entry in std::fs::read_dir(&corpus).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        let input = std::fs::read(&path).unwrap();
        let Ok(outcome) = catch_unwind(|| check(&input, &rules)) else {
            panicked.push(name);
            continue;
        };

        // A seed's name says what the parser must make of it: `corrupt` is
        // anything it must refuse, a damaged label or a sound one read by a
        // layout no register should have.
        let agrees = match name.split_once('-').map(|(kind, _)| kind) {
            Some("plain") => outcome == Outcome::Plain,
            Some("weight") => outcome == Outcome::Label(EmbeddedValue::WeightMilli),
            Some("price") => outcome == Outcome::Label(EmbeddedValue::PriceMinor),
            Some("corrupt") => matches!(outcome, Outcome::Refused(_)),
            _ => {
                assert!(
                    named_by_libfuzzer(&name),
                    "{name}: not a seed, not a finding"
                );
                continue;
            }
        };
        assert!(agrees, "{name} came to {outcome:?}");
        seeds.push(outcome);
    }
    assert!(panicked.is_empty(), "panicked on {panicked:?}");

    // The seeds reach every outcome: both kinds of label, an ordinary barcode,
    // and each refusal `parse_scan` can make. (The scan's other two errors are
    // `resolve_scan`'s.)
    for wanted in [
        Outcome::Plain,
        Outcome::Label(EmbeddedValue::WeightMilli),
        Outcome::Label(EmbeddedValue::PriceMinor),
    ] {
        assert!(seeds.contains(&wanted), "no seed comes to {wanted:?}");
    }
    for refusal in [
        ScanError::Empty,
        ScanError::NotUtf8,
        ScanError::BadLength(0, BarcodeKind::PriceEmbedded),
        ScanError::NonNumeric,
        ScanError::ChecksumFailed,
        ScanError::ValueOutOfRange,
        ScanError::RuleMalformed(String::new()),
    ] {
        assert!(
            seeds.iter().any(|seed| matches!(
                seed,
                Outcome::Refused(error) if discriminant(error) == discriminant(&refusal)
            )),
            "no seed is refused with {refusal:?}"
        );
    }
}
