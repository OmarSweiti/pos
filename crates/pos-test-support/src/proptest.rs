//! The one place a property test's case count, seed and failure persistence are
//! decided — conventions §5.1, microstep 1.1.0.
//!
//! A property test that chooses its own case count is a property test that can
//! be made cheap by the person least interested in what it finds. So the counts
//! live here, the properties name only which of the two they want, and the
//! scheduled high-count lane is a raising override that a local assignment
//! cannot shadow.
//!
//! ## Why the ordering matters more than the numbers
//!
//! The obvious spelling of a shared default is also the bug:
//!
//! ```ignore
//! ProptestConfig { cases: 4_096, ..ProptestConfig::default() }   // WRONG
//! ```
//!
//! `ProptestConfig::default()` has *already* applied `PROPTEST_CASES`, and the
//! `cases: 4_096` in front of it then overwrites the override with the crate
//! default. A `PROPTEST_CASES=100000` lane would run 4,096 cases and its log
//! would say 100,000. So the environment is read **after** the crate default is
//! selected, by [`resolve_cases`], which is a pure function of the two inputs
//! and therefore provable without touching a process-global variable.
//!
//! ## Why re-reading the environment is safe rather than duplicated work
//!
//! `proptest!` calls `proptest::test_runner::contextualize_config` on whatever
//! configuration it is handed, so `PROPTEST_CASES` is applied a second time
//! after this module returns. That is not a hole, because [`resolve_cases`]
//! agrees with proptest's own parser by construction: it uses the same
//! `u32::FromStr`, without trimming, so for every value proptest accepts the two
//! compute the same number and the macro's second pass is a no-op. For every
//! value proptest would *warn about and ignore* — and for every value below the
//! crate default — this module panics first, from inside the
//! `#![proptest_config(...)]` expression, before the macro ever runs. Refusing
//! is the only way "an invalid or lower override is refused" can be true when
//! the harness does not own the last write.
//!
//! ## Environment reads live in this crate on purpose
//!
//! `pos-domain` is pure (I-8) and may not read a variable, a clock or a random
//! number. This crate can, because it is consumed only from a
//! `[dev-dependencies]` table and never reaches a register — see the crate-level
//! documentation for that boundary.

use std::env;

use ::proptest::test_runner::{Config as ProptestConfig, FileFailurePersistence, RngSeed};

/// The shared case count for a pure-domain property.
///
/// 4,096 because `pos-domain` properties are arithmetic on integers: the whole
/// suite at this count still fits inside the three-minute `just test` budget in
/// conventions §5, and four thousand attempts at a largest-remainder split find
/// an off-by-one that a few hundred do not.
pub const DOMAIN_CASES: u32 = 4_096;

/// The shared case count for a property that touches a database, a file or a
/// device simulator.
///
/// 256, deliberately lower. An I/O-bound property at 4,096 cases turns a
/// three-minute gate into one nobody runs, and a slow database property that
/// nobody runs protects nothing.
pub const IO_CASES: u32 = 256;

/// The repository's recorded property-test seed.
///
/// Conventions §5.1: "Default and pull-request runs use a repository-recorded
/// deterministic seed." This constant *is* that record — the value is here, in
/// version control, rather than in a CI variable or a developer's shell, so a
/// failure on a pull request and a failure on the machine reproducing it draw
/// the same inputs in the same order.
///
/// It is the ASCII bytes `POS-SEED` read big-endian, so the number in a log line
/// can be recognised rather than merely copied.
///
/// A scheduled lane may still choose another seed by exporting
/// `PROPTEST_RNG_SEED`; [`domain_proptest_config`] keeps an explicitly requested
/// seed rather than overwriting it, because §5.1 permits a different seed
/// exactly when the log records it well enough to replay.
pub const RECORDED_SEED: u64 = u64::from_be_bytes(*b"POS-SEED");

/// The directory a minimized failing case is persisted into, and committed from.
///
/// `proptest` resolves it as a sibling of the `src` directory holding the source
/// file that ran the property, so a failure in `crates/pos-domain/src/money.rs`
/// lands in `crates/pos-domain/proptest-regressions/money.txt`. Nothing in
/// `.gitignore` covers that path, which is the point: the file is evidence and
/// it is committed.
pub const REGRESSIONS_DIR: &str = "proptest-regressions";

/// The environment variable that raises the case count for a scheduled lane.
pub const CASES_ENV: &str = "PROPTEST_CASES";

/// `proptest`'s own switch for turning committed regression files off.
///
/// Refused rather than honoured. See [`domain_proptest_config`].
pub const DISABLE_PERSISTENCE_ENV: &str = "PROPTEST_DISABLE_FAILURE_PERSISTENCE";

/// Why a `PROPTEST_CASES` value was refused.
///
/// Both arms are refusals rather than warnings. `proptest` warns and quietly
/// keeps its default, which is the failure mode this harness exists to remove:
/// a lane whose log says 100,000 and whose run was 4,096 has a coverage claim
/// nobody can check.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CaseOverrideError {
    /// The value is not a `u32` — empty, negative, padded, or out of range.
    #[error("{CASES_ENV}={raw:?} is not a u32 case count")]
    NotAnInteger {
        /// The value exactly as the environment carried it.
        raw: String,
    },
    /// The value would lower the shared count instead of raising it.
    #[error(
        "{CASES_ENV}={requested} is below the shared default of {crate_default}; \
         the shared count is a floor and an override may only raise it"
    )]
    BelowCrateDefault {
        /// The count the environment asked for.
        requested: u32,
        /// The count the crate's own default guarantees.
        crate_default: u32,
    },
}

/// Apply `PROPTEST_CASES` to an already-selected crate default.
///
/// Pure: the caller supplies the raw value, so the ordering rule this whole
/// module is about — override *after* default, never the other way round — is
/// provable without setting a process-global variable in a test that runs
/// concurrently with every other test in its binary.
///
/// - `None` (the variable is absent) keeps `crate_default`.
/// - A value at or above `crate_default` wins.
/// - Anything else is [`CaseOverrideError`].
///
/// Parsing is plain `u32::FromStr` with no trimming, matching `proptest`'s own
/// parser exactly, so the two can never disagree about an accepted value.
pub fn resolve_cases(crate_default: u32, raw: Option<&str>) -> Result<u32, CaseOverrideError> {
    let Some(raw) = raw else {
        return Ok(crate_default);
    };
    let requested: u32 = raw.parse().map_err(|_| CaseOverrideError::NotAnInteger {
        raw: raw.to_owned(),
    })?;
    if requested < crate_default {
        return Err(CaseOverrideError::BelowCrateDefault {
            requested,
            crate_default,
        });
    }
    Ok(requested)
}

/// The shared configuration for a property test in a pure crate.
///
/// [`DOMAIN_CASES`] cases, raised — never lowered — by `PROPTEST_CASES`; the
/// [`RECORDED_SEED`] unless `PROPTEST_RNG_SEED` names another one; and
/// committed failure persistence under [`REGRESSIONS_DIR`].
///
/// # Panics
///
/// Refuses the run, loudly, when `PROPTEST_CASES` is invalid or would lower the
/// count ([`CaseOverrideError`]), or when `PROPTEST_DISABLE_FAILURE_PERSISTENCE`
/// is set at all. The last one is a refusal because this function's contract is
/// that a failure leaves a committed, replayable regression file; a variable
/// that silently deletes that guarantee would make the contract a wish.
#[must_use]
pub fn domain_proptest_config() -> ProptestConfig {
    shared_config(DOMAIN_CASES)
}

/// The shared configuration for a property test that performs I/O.
///
/// [`IO_CASES`] cases; otherwise identical to [`domain_proptest_config`],
/// including its refusals.
///
/// # Panics
///
/// As [`domain_proptest_config`].
#[must_use]
pub fn io_proptest_config() -> ProptestConfig {
    shared_config(IO_CASES)
}

fn shared_config(crate_default: u32) -> ProptestConfig {
    if env::var_os(DISABLE_PERSISTENCE_ENV).is_some() {
        refuse(&format!(
            "{DISABLE_PERSISTENCE_ENV} is set. A property test in this workspace \
             must persist its minimized failing case under {REGRESSIONS_DIR}/ so \
             the case can be committed and replayed (conventions §5.1). Unset the \
             variable."
        ));
    }

    let cases = match cases_from_environment(crate_default) {
        Ok(cases) => cases,
        Err(error) => refuse(&error.to_string()),
    };

    pinned(cases, ProptestConfig::default())
}

/// Read `PROPTEST_CASES` and hand it to [`resolve_cases`].
///
/// The thin env-reading wrapper around the pure function. `var_os` rather than
/// `var` so a value that is present but not UTF-8 is a refusal instead of an
/// indistinguishable "absent".
fn cases_from_environment(crate_default: u32) -> Result<u32, CaseOverrideError> {
    match env::var_os(CASES_ENV) {
        None => Ok(crate_default),
        Some(raw) => match raw.to_str() {
            Some(text) => resolve_cases(crate_default, Some(text)),
            None => Err(CaseOverrideError::NotAnInteger {
                raw: raw.to_string_lossy().into_owned(),
            }),
        },
    }
}

/// Stamp the repository's seed and persistence policy onto `base`.
///
/// Split out from [`shared_config`] so the policy can be asserted against a
/// `base` the test constructed, rather than against whatever the ambient
/// environment happened to put in `ProptestConfig::default()`.
fn pinned(cases: u32, base: ProptestConfig) -> ProptestConfig {
    let mut config = base;
    config.cases = cases;

    // `RngSeed::Random` is what `ProptestConfig::default()` holds when
    // PROPTEST_RNG_SEED is absent, so this pins the recorded seed for the
    // default and pull-request runs while leaving an explicitly requested
    // scheduled seed alone.
    if config.rng_seed == RngSeed::Random {
        config.rng_seed = RngSeed::Fixed(RECORDED_SEED);
    }

    // Stated rather than inherited. This is `proptest`'s own default today, and
    // a default that nothing asserts is a default that a dependency bump can
    // remove without a single test going red.
    config.failure_persistence = Some(Box::new(FileFailurePersistence::SourceParallel(
        REGRESSIONS_DIR,
    )));

    config
}

/// Stop the run and say why.
///
/// `clippy::panic` is denied workspace-wide because a panic in a register is a
/// lost sale. This crate is not a register — it is dev-only test support that
/// ships nowhere — and the alternative to panicking here is running fewer cases
/// than the repository promised while reporting success, which is the exact
/// failure conventions §5.1 is written to prevent. The opt-out is on this one
/// three-line function rather than the module so nothing else in the crate can
/// acquire it by accident.
#[allow(
    clippy::panic,
    reason = "dev-only test support: a misconfigured property lane must stop, not \
              silently shrink"
)]
fn refuse(message: &str) -> ! {
    panic!("pos-test-support: {message}");
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn an_absent_override_keeps_the_crate_default() {
        assert_eq!(resolve_cases(DOMAIN_CASES, None), Ok(DOMAIN_CASES));
        assert_eq!(resolve_cases(IO_CASES, None), Ok(IO_CASES));
    }

    #[test]
    fn the_scheduled_lane_value_raises_both_defaults() {
        // The number the scheduled domain lane exports. It must survive for the
        // low default too: an I/O crate asked for 100,000 gets 100,000, because
        // the shared count is a floor and not a ceiling.
        assert_eq!(resolve_cases(DOMAIN_CASES, Some("100000")), Ok(100_000));
        assert_eq!(resolve_cases(IO_CASES, Some("100000")), Ok(100_000));
    }

    #[test]
    fn an_override_equal_to_the_default_is_accepted() {
        // Equal is not below. A CI lane that exports the default explicitly is
        // being redundant, not wrong.
        assert_eq!(resolve_cases(DOMAIN_CASES, Some("4096")), Ok(DOMAIN_CASES));
    }

    #[test]
    fn a_lower_override_is_refused() {
        // The whole point. `PROPTEST_CASES=1` is how a red property becomes a
        // green one without editing a line of test code.
        assert_eq!(
            resolve_cases(DOMAIN_CASES, Some("1")),
            Err(CaseOverrideError::BelowCrateDefault {
                requested: 1,
                crate_default: DOMAIN_CASES,
            })
        );
        assert_eq!(
            resolve_cases(DOMAIN_CASES, Some("0")),
            Err(CaseOverrideError::BelowCrateDefault {
                requested: 0,
                crate_default: DOMAIN_CASES,
            })
        );
        // 256 is the other shared default, and it is still a lowering here.
        assert!(resolve_cases(DOMAIN_CASES, Some("256")).is_err());
    }

    #[test]
    fn an_unparseable_override_is_refused() {
        // Every one of these makes proptest print a warning and keep its own
        // default, which is the silent-coverage-loss case.
        for raw in ["", " ", "lots", "-1", "4096.0", "4_096", "1e6", " 100000 "] {
            assert_eq!(
                resolve_cases(DOMAIN_CASES, Some(raw)),
                Err(CaseOverrideError::NotAnInteger {
                    raw: raw.to_owned()
                }),
                "{raw:?} must be refused, not warned about"
            );
        }
        // Above u32::MAX is not a case count either.
        assert!(resolve_cases(DOMAIN_CASES, Some("4294967296")).is_err());
    }

    #[test]
    fn our_parser_accepts_exactly_what_proptest_accepts() {
        // `proptest!` re-applies PROPTEST_CASES after this module returns, using
        // plain `u32::FromStr`. If the two parsers ever disagreed on an accepted
        // value, the effective count would be whichever ran last.
        for raw in ["100000", "+100000", "4096", "0", "", "lots", " 4096", "1e6"] {
            assert_eq!(
                raw.parse::<u32>().is_ok(),
                !matches!(
                    resolve_cases(0, Some(raw)),
                    Err(CaseOverrideError::NotAnInteger { .. })
                ),
                "{raw:?}: this module and proptest must agree on parseability"
            );
        }
    }

    #[test]
    fn the_recorded_seed_is_pinned_when_the_environment_names_none() {
        let base = ProptestConfig {
            rng_seed: RngSeed::Random,
            ..ProptestConfig::default()
        };
        let config = pinned(DOMAIN_CASES, base);
        assert_eq!(config.rng_seed, RngSeed::Fixed(RECORDED_SEED));
        assert_eq!(config.cases, DOMAIN_CASES);
    }

    #[test]
    fn a_scheduled_seed_survives_the_recorded_one() {
        // §5.1 allows a scheduled run its own seed when the log records it. The
        // harness must not quietly replace it with the committed default.
        let base = ProptestConfig {
            rng_seed: RngSeed::Fixed(4_242),
            ..ProptestConfig::default()
        };
        assert_eq!(pinned(DOMAIN_CASES, base).rng_seed, RngSeed::Fixed(4_242));
    }

    #[test]
    fn the_recorded_seed_is_the_ascii_it_claims_to_be() {
        assert_eq!(RECORDED_SEED.to_be_bytes(), *b"POS-SEED");
    }

    #[test]
    fn failure_persistence_is_stated_not_inherited() {
        let base = ProptestConfig {
            failure_persistence: None,
            ..ProptestConfig::default()
        };
        let config = pinned(IO_CASES, base);
        let persistence = config
            .failure_persistence
            .as_ref()
            .expect("the harness must always enable failure persistence");
        assert!(
            ::proptest::test_runner::FailurePersistence::eq(
                &**persistence,
                &FileFailurePersistence::SourceParallel(REGRESSIONS_DIR),
            ),
            "expected SourceParallel({REGRESSIONS_DIR:?}), got {persistence:?}"
        );
    }
}
