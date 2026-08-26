//! The shared property-test configuration, proved rather than described.
//!
//! Microstep 1.1.0's four named tests live here. Two of the three things this
//! harness promises are awkward to test honestly, and the awkwardness is worth
//! stating before the code:
//!
//! **The case count depends on a process-global variable.** `PROPTEST_CASES` is
//! read from the environment, `cargo nextest` runs the tests in this file
//! concurrently, and `ProptestConfig::default()` memoises its environment read
//! in a `LazyLock` the first time anything in the process touches it. A test
//! that exported the variable for itself would therefore race its neighbours,
//! and — worse — could pass while the override was being ignored entirely. So
//! every observation of an effective case count happens in a **child process**
//! whose environment this file sets explicitly, and the observation is the
//! number of times `proptest` actually invoked the property, not a field read
//! back from a struct the test just built. `pos-test-support`'s own unit tests
//! cover the same decision as a pure function of its two inputs; these cover the
//! whole chain, `proptest!` macro included.
//!
//! **Proving persistence needs a failure.** A test that leaves a red property
//! behind is not evidence, it is a broken suite. So the one deliberately failing
//! property in this workspace is driven by an explicit `TestRunner` inside a
//! passing test, persists into a `tempfile` directory rather than the
//! repository, and is asserted on: the seed line, the minimized case, and a
//! replay that reproduces the failure with zero novel cases allowed.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests; conventions §1 exempts test code the way money.rs does"
)]

use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use pos_test_support::{
    CASES_ENV, DISABLE_PERSISTENCE_ENV, DOMAIN_CASES, IO_CASES, domain_proptest_config,
    io_proptest_config,
};
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, PersistedSeed, TestError, TestRunner};

/// The scheduled `pos-domain` lane's case count, from conventions §5.1.
const SCHEDULED_CASES: u32 = 100_000;

/// Marker on the one line a fixture prints and its parent parses.
///
/// Load-bearing: `libtest` reports a filter that matched nothing as a *success*
/// with "0 passed", so a parent that only checked the exit status would pass
/// vacuously the moment a fixture were renamed. Every parent requires this line.
///
/// Searched for with `contains` rather than `starts_with`, because `--nocapture`
/// leaves `libtest`'s own unterminated `test <name> ... ` on the front of it.
const OBSERVATION: &str = "pos-test-support-observation";

const DOMAIN_FIXTURE: &str = "domain_case_count_fixture";
const IO_FIXTURE: &str = "io_case_count_fixture";

// ── the child half ──────────────────────────────────────────────────────────
//
// Ignored, so `just test` and CI never run them without an environment a parent
// chose. They are not skipped tests with no owner: each one has exactly one
// parent below, and that parent fails if the fixture does not report.

#[test]
#[ignore = "child of the case-count observations; a parent runs it with an environment it controls"]
fn domain_case_count_fixture() {
    report(domain_proptest_config());
}

#[test]
#[ignore = "child of the case-count observations; a parent runs it with an environment it controls"]
fn io_case_count_fixture() {
    report(io_proptest_config());
}

/// Run a trivial property under `config` and print what actually happened.
///
/// `declared` is the count the shared helper resolved; `observed` is how many
/// times `proptest` called the property. Both are printed because they answer
/// different questions: the first that the harness computed the right number,
/// the second that the runner honoured it.
fn report(config: ProptestConfig) {
    let declared = config.cases;
    let observed = AtomicU32::new(0);

    let hermetic = ProptestConfig {
        // `cases` is the value under observation and failure persistence cannot
        // change it. Switching persistence off keeps this fixture from writing
        // a regression file into the repository, and keeps the count exact: a
        // persisted seed is replayed *before* the novel cases and would inflate
        // it by one for every line in the file.
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..config
    };

    proptest!(hermetic, |(_ in any::<u8>())| {
        observed.fetch_add(1, Ordering::Relaxed);
    });

    println!(
        "{OBSERVATION} declared={declared} observed={}",
        observed.load(Ordering::Relaxed)
    );
}

// ── the parent half ─────────────────────────────────────────────────────────

/// Re-run this test binary with one ignored fixture selected and `PROPTEST_CASES`
/// set to exactly what the caller asked for.
fn spawn(fixture: &str, cases: Option<&str>) -> Output {
    spawn_with(fixture, &[(CASES_ENV, cases)])
}

/// Re-run this test binary with one ignored fixture selected and an environment
/// stated variable by variable.
///
/// `None` *removes* a variable rather than leaving it alone, so a developer or a
/// CI lane that already exported one cannot silently change what a
/// shared-default assertion means.
fn spawn_with(fixture: &str, environment: &[(&str, Option<&str>)]) -> Output {
    let binary = std::env::current_exe().expect("the running test binary");
    let mut command = Command::new(binary);
    command.args([
        "--exact",
        fixture,
        "--ignored",
        "--nocapture",
        "--test-threads",
        "1",
    ]);
    for (name, value) in environment {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command
        .output()
        .unwrap_or_else(|error| panic!("could not run the {fixture} child: {error}"))
}

/// What a fixture reported: the resolved count, and the count actually run.
#[derive(Debug, PartialEq, Eq)]
struct Observation {
    declared: u32,
    observed: u32,
}

fn observe(fixture: &str, cases: Option<&str>) -> Observation {
    let output = spawn(fixture, cases);
    let transcript = transcript(&output);
    assert!(
        output.status.success(),
        "the {fixture} child failed ({:?}):\n{transcript}",
        output.status
    );

    let line = transcript
        .lines()
        .find(|line| line.contains(OBSERVATION))
        .unwrap_or_else(|| {
            panic!("the {fixture} child ran no property and reported nothing:\n{transcript}")
        });

    Observation {
        declared: field(line, "declared="),
        observed: field(line, "observed="),
    }
}

fn field(line: &str, key: &str) -> u32 {
    line.split_whitespace()
        .find_map(|token| token.strip_prefix(key))
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("no {key} in {line:?}"))
}

/// Both streams together. A `panic!` inside the child reaches stderr under
/// `--nocapture` and stdout when `libtest` formats a failure report, and a
/// refusal must be found either way.
fn transcript(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn shared_domain_config_uses_4096_cases() {
    let observation = observe(DOMAIN_FIXTURE, None);
    assert_eq!(
        observation,
        Observation {
            declared: DOMAIN_CASES,
            observed: DOMAIN_CASES,
        },
        "a pure-domain property runs {DOMAIN_CASES} cases when nothing raises it"
    );
    assert_eq!(DOMAIN_CASES, 4_096, "conventions §5.1 fixes this number");
}

#[test]
fn shared_io_config_uses_256_cases() {
    let observation = observe(IO_FIXTURE, None);
    assert_eq!(
        observation,
        Observation {
            declared: IO_CASES,
            observed: IO_CASES,
        },
        "an I/O-bound property runs {IO_CASES} cases when nothing raises it"
    );
    assert_eq!(IO_CASES, 256, "conventions §5.1 fixes this number");
    const {
        assert!(
            IO_CASES < DOMAIN_CASES,
            "a slow database property that nobody runs protects nothing"
        )
    };
}

#[test]
fn scheduled_case_override_is_not_shadowed_by_shared_default() {
    // The same fixture, twice, with one difference: the environment. That is
    // what makes this test impossible to pass vacuously. If the shared default
    // shadowed the override — the `ProptestConfig { cases: 4_096, ..default() }`
    // bug — both runs would report 4,096 and the first assertion would fail. If
    // the override were the only thing that ever applied, the second would.
    let scheduled = observe(DOMAIN_FIXTURE, Some("100000"));
    let ambient = observe(DOMAIN_FIXTURE, None);

    assert_eq!(
        scheduled,
        Observation {
            declared: SCHEDULED_CASES,
            observed: SCHEDULED_CASES,
        },
        "PROPTEST_CASES=100000 must be exactly 100,000 effective cases"
    );
    assert_eq!(
        ambient,
        Observation {
            declared: DOMAIN_CASES,
            observed: DOMAIN_CASES,
        },
        "and the same fixture without the variable must fall back to the default"
    );
    assert_ne!(
        scheduled.observed, ambient.observed,
        "the environment, and nothing else, changed between these two runs"
    );
}

#[test]
fn a_case_override_below_the_shared_default_is_refused() {
    // `PROPTEST_CASES=1` is how a red property becomes a green one without
    // touching a line of test code, so the harness refuses instead of obeying.
    let output = spawn(DOMAIN_FIXTURE, Some("100"));
    let transcript = transcript(&output);
    assert!(
        !output.status.success(),
        "a lowering override must fail the run:\n{transcript}"
    );
    assert!(
        transcript.contains("below the shared default"),
        "the refusal must say why:\n{transcript}"
    );
    assert!(
        !transcript.contains(OBSERVATION),
        "the property must not have run at all:\n{transcript}"
    );
}

#[test]
fn an_unparseable_case_override_is_refused() {
    // proptest's own behaviour here is to print a warning and keep its default,
    // which is how a lane's log says 100,000 and its run was 4,096.
    let output = spawn(DOMAIN_FIXTURE, Some("100_000"));
    let transcript = transcript(&output);
    assert!(
        !output.status.success(),
        "an unparseable override must fail the run:\n{transcript}"
    );
    assert!(
        transcript.contains("is not a u32 case count"),
        "the refusal must say why:\n{transcript}"
    );
}

#[test]
fn disabling_failure_persistence_is_refused() {
    // Committed regressions are the whole point of the persistence half of this
    // harness, and `proptest` lets any value of this variable switch them off.
    // The helper refuses instead, because a guarantee an environment variable can
    // delete is not a guarantee.
    let output = spawn_with(
        DOMAIN_FIXTURE,
        &[(CASES_ENV, None), (DISABLE_PERSISTENCE_ENV, Some("1"))],
    );
    let transcript = transcript(&output);
    assert!(
        !output.status.success(),
        "disabling failure persistence must fail the run:\n{transcript}"
    );
    assert!(
        transcript.contains(DISABLE_PERSISTENCE_ENV),
        "the refusal must name the variable:\n{transcript}"
    );
    assert!(
        !transcript.contains(OBSERVATION),
        "the property must not have run at all:\n{transcript}"
    );
}

#[test]
fn a_failed_property_persists_its_seed_and_minimized_case() {
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let path = scratch.path().join("regressions.txt");
    // `FileFailurePersistence::Direct` takes a `&'static str`, so a path only
    // known at run time has to be leaked. One short string, once, in a test
    // process that is about to exit — and the alternative is writing the proof
    // of committed persistence into the repository it is a proof about.
    let direct: &'static str = Box::leak(
        path.to_str()
            .expect("a UTF-8 temporary path")
            .to_owned()
            .into_boxed_str(),
    );

    // A deliberately false claim about a bounded range: "nothing in
    // 1..=1_000_000 reaches a thousand". 1000 is the boundary, so a correct
    // shrink reports exactly 1000 and nothing smaller.
    let strategy = 1i64..=1_000_000i64;
    let claim = |value: i64| {
        prop_assert!(value < 1_000, "{value} reached a thousand");
        Ok(())
    };

    let mut runner = TestRunner::new(ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(direct))),
        ..domain_proptest_config()
    });
    let minimized = match runner.run(&strategy, claim) {
        Err(TestError::Fail(_, value)) => value,
        other => panic!("the deliberately false claim did not fail: {other:?}"),
    };
    assert_eq!(minimized, 1_000, "shrinking must reach the boundary");

    let persisted = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{direct} was never written: {error}"));

    // proptest writes this header the first time it creates the file, and it is
    // the instruction the repository follows: the file is committed evidence.
    assert!(
        persisted.contains("check this file in to source control"),
        "the committed-file header is missing:\n{persisted}"
    );

    let mut seed_lines = persisted
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty());
    let seed_line = seed_lines
        .next()
        .unwrap_or_else(|| panic!("no seed line was persisted:\n{persisted}"));
    assert!(
        seed_lines.next().is_none(),
        "one failure must persist one seed line:\n{persisted}"
    );

    let (seed, shrunk) = seed_line
        .split_once(" # shrinks to ")
        .unwrap_or_else(|| panic!("{seed_line:?} records no minimized case"));
    assert_eq!(
        shrunk,
        minimized.to_string(),
        "the persisted case must be the minimized one, not the first failure"
    );
    assert!(
        seed.parse::<PersistedSeed>().is_ok(),
        "{seed:?} is not a seed proptest can replay"
    );

    // Now prove the seed really is replayable, and that the *file* is what
    // replays it. `cases: 0` forbids every novel case, so the persisted seed is
    // the only thing left that can produce a failure.
    let before = std::fs::read(&path).expect("the persisted file");
    let mut replay = TestRunner::new(ProptestConfig {
        cases: 0,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(direct))),
        ..domain_proptest_config()
    });
    match replay.run(&strategy, claim) {
        // A zero case budget also disables shrinking, so this is the seed's own
        // original counterexample rather than the minimum — which is exactly the
        // claim being made: the seed regenerates a failing input.
        Err(TestError::Fail(_, value)) => assert!(
            value >= 1_000,
            "the replayed input must be a counterexample, got {value}"
        ),
        other => panic!("the persisted seed did not replay the failure: {other:?}"),
    }
    assert_eq!(
        before,
        std::fs::read(&path).expect("the persisted file"),
        "a replayed failure is already recorded and must not be appended again"
    );

    // Nothing about this test may reach the repository.
    drop(scratch);
}
