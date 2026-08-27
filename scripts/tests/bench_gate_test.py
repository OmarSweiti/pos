#!/usr/bin/env python3
"""Prove the benchmark gate refuses what conventions §7.1 says it must.

Microstep 1.2.0's whole product is a command that exits non-zero, so this file
is mostly about non-zero exits: an absolute limit breached, a real regression,
a blank or mismatched reference register, a hosted runner reaching for a
baseline. Each case asserts the exit code *and* the sentence the gate printed,
because a gate that fails for the wrong reason is a gate nobody will trust the
second time.

Every fixture identity is obviously synthetic and says so in every field. That
is a hard rule, not tidiness: `benchmarks/reference-register.toml` and
`ref/hardware-and-receipts.md` §6a.1 are both blank because no reference
register has been bought, and a plausible-looking fixture is exactly how an
invented machine would end up in the committed record. The gate refuses a
synthetic identity on its live path, and this file proves that too.

What is NOT here, and why: `reference_profile_matches_supported_device_matrix`
in its original sense — the committed matrix row and the committed profile
carrying the same real values — cannot be written until a machine exists. It is
scheduled in the deferred half of 1.2.0 in `phase-1-sellable-mvp.md`. What
stands here instead is not a substitute that passes by comparing two blanks:
`reference_profile_fields_mirror_the_device_matrix_columns` asserts the two
records declare the same twelve fields in the same order, and
`a_blank_reference_profile_and_matrix_are_detected_and_refused` asserts the
blank pair is *refused* while a filled synthetic pair through the same code path
is accepted.

Usage:  scripts/tests/bench_gate_test.py
Exit:   0 every case passed · 1 a case failed · 2 could not run at all
"""

from __future__ import annotations

import dataclasses
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Mapping, Sequence
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / "scripts/bench-gate.py"

MS = 1_000_000


def load_gate():
    """Import `bench-gate.py`, whose hyphen makes it unimportable by name.

    The constants and the record parsers come from the gate itself so a fixture
    cannot drift away from what the gate actually reads.
    """
    specification = importlib.util.spec_from_file_location("bench_gate", GATE)
    if specification is None or specification.loader is None:
        raise SystemExit(f"bench-gate-test: could not import {GATE}")
    module = importlib.util.module_from_spec(specification)
    # `dataclasses` resolves a class's own module through `sys.modules`, so the
    # module has to be registered before its body runs.
    sys.modules[specification.name] = module
    specification.loader.exec_module(module)
    return module


BENCH_GATE = load_gate()
IDENTITY_FIELDS: tuple[str, ...] = BENCH_GATE.IDENTITY_FIELDS

# Obviously synthetic in every single field, so no cell of it could be pasted
# into the committed profile and read as a machine somebody owns. The gate's
# live path refuses any value carrying this marker, and
# `a_synthetic_fixture_identity_can_never_become_the_committed_profile` proves
# both halves of that.
SYNTHETIC_IDENTITY: dict[str, str] = {
    "profile_id": "SYNTHETIC-FIXTURE-REGISTER",
    "maker": "SYNTHETIC FIXTURE (no such vendor)",
    "model": "SYNTHETIC FIXTURE (no such device)",
    "cpu": "SYNTHETIC FIXTURE cpu",
    "ram": "SYNTHETIC FIXTURE ram",
    "storage": "SYNTHETIC FIXTURE storage",
    "os_version": "SYNTHETIC FIXTURE os",
    "power_mode": "SYNTHETIC FIXTURE power mode",
    "release_profile": "SYNTHETIC FIXTURE release profile",
    "qualified_at": "SYNTHETIC FIXTURE date",
    "qualified_by": "SYNTHETIC FIXTURE operator",
    "qualifying_commit": "SYNTHETIC-FIXTURE-NOT-A-COMMIT",
}
BLANK_IDENTITY: dict[str, str] = dict.fromkeys(IDENTITY_FIELDS, "")


# ── fixtures ──────────────────────────────────────────────────────────────


def matrix_markdown(
    rows: Sequence[Mapping[str, str]],
    columns: Sequence[str] = IDENTITY_FIELDS,
) -> str:
    """A device-matrix document with the same markers §6a.1 uses."""
    lines = [
        "# SYNTHETIC FIXTURE device matrix — not a supported-device list",
        "",
        BENCH_GATE.MATRIX_BEGIN,
        "",
        "| " + " | ".join(f"`{column}`" for column in columns) + " |",
        "|" + "---|" * len(columns),
    ]
    for row in rows:
        lines.append("| " + " | ".join(row.get(column, "") for column in columns) + " |")
    lines += ["", BENCH_GATE.MATRIX_END, ""]
    return "\n".join(lines) + "\n"


def profile_toml(
    identity: Mapping[str, str],
    order: Sequence[str] = IDENTITY_FIELDS,
) -> str:
    lines = [
        "# SYNTHETIC FIXTURE reference register — not a real machine",
        "schema_version = 1",
        "",
        "[identity]",
    ]
    for field in order:
        lines.append(f"{field} = {json.dumps(identity.get(field, ''))}")
    return "\n".join(lines) + "\n"


def sample(
    *,
    median_ns: int,
    p99_ns: int,
    mad_ns: int,
    budget: str = "search",
    samples: int = 50,
    reason: str | None = None,
    identity: Mapping[str, str] | None = None,
) -> dict[str, object]:
    """One measurement or baseline record. `reason` makes it a baseline."""
    document: dict[str, object] = {
        "budget": budget,
        "samples": samples,
        "median_ns": median_ns,
        "p99_ns": p99_ns,
        "mad_ns": mad_ns,
        "taken_at": "SYNTHETIC FIXTURE timestamp",
        "taken_by": "SYNTHETIC FIXTURE operator",
        "commit": "SYNTHETIC-FIXTURE-NOT-A-COMMIT",
        "profile_identity": dict(identity or SYNTHETIC_IDENTITY),
    }
    if reason is not None:
        document["reason"] = reason
    return document


def tree(
    directory: Path,
    *,
    identity: Mapping[str, str] | None = None,
    matrix_rows: Sequence[Mapping[str, str]] | None = None,
    matrix_columns: Sequence[str] = IDENTITY_FIELDS,
    profile_order: Sequence[str] = IDENTITY_FIELDS,
    write_profile: bool = True,
    write_matrix: bool = True,
    baselines: Mapping[str, Mapping[str, object]] | None = None,
    measurements: Mapping[str, Mapping[str, object]] | None = None,
) -> Path:
    """A complete `--fixture-root`: matrix, profile, baselines, measurements."""
    resolved = dict(identity if identity is not None else SYNTHETIC_IDENTITY)
    rows = list(matrix_rows) if matrix_rows is not None else [resolved]
    directory.mkdir(parents=True, exist_ok=True)
    if write_matrix:
        (directory / "device-matrix.md").write_text(
            matrix_markdown(rows, matrix_columns), encoding="utf-8"
        )
    if write_profile:
        (directory / "reference-register.toml").write_text(
            profile_toml(resolved, profile_order), encoding="utf-8"
        )
    for name, documents in (
        ("baselines", baselines),
        ("measurements", measurements),
    ):
        (directory / name).mkdir(exist_ok=True)
        for slug, document in (documents or {}).items():
            (directory / name / f"{slug}.json").write_text(
                json.dumps(document, indent=2) + "\n", encoding="utf-8"
            )
    return directory


def scratch(name: str) -> tempfile.TemporaryDirectory[str]:
    return tempfile.TemporaryDirectory(prefix=f"bench-gate-{name}-")


# ── running the gate ──────────────────────────────────────────────────────


class Result:
    def __init__(self, code: int, output: str) -> None:
        self.code = code
        self.output = output


def workstation_environment() -> dict[str, str]:
    """This machine, with every build-machine marker removed.

    Without this the whole suite would run as "hosted" inside CI's own guards
    job, and the one case that is *about* being hosted would prove nothing.
    """
    blocked = {*BENCH_GATE.CI_MARKERS, "RUNNER_ENVIRONMENT"}
    return {
        name: value for name, value in os.environ.items() if name not in blocked
    }


def hosted_environment() -> dict[str, str]:
    environment = workstation_environment()
    environment.update(
        {
            "CI": "true",
            "GITHUB_ACTIONS": "true",
            "RUNNER_ENVIRONMENT": "github-hosted",
        }
    )
    return environment


def run_gate(*arguments: str, environment: Mapping[str, str] | None = None) -> Result:
    completed = subprocess.run(
        [sys.executable, str(GATE), *arguments],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        env=dict(environment) if environment is not None else workstation_environment(),
    )
    return Result(completed.returncode, completed.stdout + completed.stderr)


# ── assertions ────────────────────────────────────────────────────────────


class Expect:
    """Collects failures, and counts assertions so a silent case cannot pass."""

    def __init__(self) -> None:
        self.problems: list[str] = []
        self.assertions = 0

    def that(self, passed: bool, description: str) -> None:
        self.assertions += 1
        if not passed:
            self.problems.append(description)

    def exits(self, result: Result, code: int, label: str) -> None:
        self.that(
            result.code == code,
            f"{label}: expected exit {code}, got {result.code} — "
            f"{result.output.strip()[:600]}",
        )

    def says(self, result: Result, label: str, *needles: str) -> None:
        for needle in needles:
            self.that(
                needle in result.output,
                f"{label}: output never said {needle!r} — "
                f"{result.output.strip()[:600]}",
            )

    def silent_about(self, result: Result, label: str, *needles: str) -> None:
        for needle in needles:
            self.that(
                needle not in result.output,
                f"{label}: output should not have mentioned {needle!r} — "
                f"{result.output.strip()[:600]}",
            )

    def refuses(self, result: Result, label: str, *needles: str) -> None:
        self.exits(result, 3, label)
        self.says(result, label, "REFUSED", *needles)

    def fails_a_budget(self, result: Result, label: str, *needles: str) -> None:
        self.exits(result, 1, label)
        self.says(result, label, *needles)

    def passes(self, result: Result, label: str, *needles: str) -> None:
        self.exits(result, 0, label)
        self.says(result, label, *needles)


def passing_pair() -> dict[str, dict[str, Mapping[str, object]]]:
    """A baseline and a measurement that agree and are well inside §7."""
    return {
        "baselines": {
            "search": sample(
                median_ns=31 * MS,
                p99_ns=44 * MS,
                mad_ns=1 * MS,
                reason="SYNTHETIC FIXTURE baseline",
            )
        },
        "measurements": {
            "search": sample(median_ns=31 * MS, p99_ns=44 * MS, mad_ns=1 * MS)
        },
    }


# ── the cases ─────────────────────────────────────────────────────────────


def bench_gate_fails_an_absolute_budget(expect: Expect) -> None:
    """A p99 over §7's limit fails, with the median exactly on its baseline.

    The median is unchanged on purpose, so the regression rule passes and the
    only thing that can have failed the run is the absolute limit.
    """
    with scratch("absolute") as temporary:
        root = tree(
            Path(temporary),
            baselines={
                "search": sample(
                    median_ns=31 * MS,
                    p99_ns=44 * MS,
                    mad_ns=1 * MS,
                    reason="SYNTHETIC FIXTURE baseline",
                )
            },
            measurements={
                "search": sample(median_ns=31 * MS, p99_ns=51 * MS, mad_ns=1 * MS)
            },
        )
        result = run_gate(f"--fixture-root={root}", "--budget=search")
        expect.fails_a_budget(
            result,
            "p99 over the 50 ms search limit",
            "FAIL  p99 under 50.000000 ms",
            "p99 51.000000 ms",
            "FAILED: search",
        )
        expect.says(
            result,
            "the regression rule must not be what failed",
            "PASS  not more than 20% AND more than 3 baseline MADs slower",
        )

        # One microsecond under the limit passes, so the limit is a limit and
        # not a permanent red.
        (root / "measurements" / "search.json").write_text(
            json.dumps(
                sample(median_ns=31 * MS, p99_ns=50 * MS - 1, mad_ns=1 * MS), indent=2
            ),
            encoding="utf-8",
        )
        expect.passes(
            run_gate(f"--fixture-root={root}", "--budget=search"),
            "49.999999 ms is inside a 50 ms p99 limit",
            "1 budget(s) within limits and baselines",
        )


def bench_gate_fails_a_significant_regression(expect: Expect) -> None:
    """30% slower and 60 baseline MADs slower, with the p99 still legal."""
    with scratch("regression") as temporary:
        root = tree(
            Path(temporary),
            baselines={
                "search": sample(
                    median_ns=20 * MS,
                    p99_ns=30 * MS,
                    mad_ns=MS // 10,
                    reason="SYNTHETIC FIXTURE baseline",
                )
            },
            measurements={
                "search": sample(median_ns=26 * MS, p99_ns=30 * MS, mad_ns=MS // 10)
            },
        )
        result = run_gate(f"--fixture-root={root}", "--budget=search")
        expect.fails_a_budget(
            result,
            "30% and 60 MADs slower",
            "FAIL  not more than 20% AND more than 3 baseline MADs slower",
            "30.0% slower (over 20%)",
            "60.0 baseline MADs (over 3)",
            "FAILED: search",
        )
        expect.says(
            result,
            "the absolute limit must not be what failed",
            "PASS  p99 under 50.000000 ms",
        )


def bench_gate_refuses_a_missing_reference_profile(expect: Expect) -> None:
    """No profile, no matrix, a blank profile, or a disagreeing one.

    Each fixture carries a baseline and a measurement that would pass, so the
    refusal can only be the reference register's doing.
    """
    pair = passing_pair()
    cases = (
        (
            "no profile file",
            {"write_profile": False},
            ("reference-register.toml does not exist", "§7.1"),
        ),
        (
            "no device matrix",
            {"write_matrix": False},
            ("device-matrix.md does not exist",),
        ),
        (
            "a blank profile against a filled matrix row",
            {"identity": BLANK_IDENTITY, "matrix_rows": [SYNTHETIC_IDENTITY]},
            ("the reference register is blank", "profile_id", "qualifying_commit"),
        ),
        (
            "a profile that disagrees with the matrix row",
            {
                "identity": {**SYNTHETIC_IDENTITY, "model": "SYNTHETIC FIXTURE other"},
                "matrix_rows": [SYNTHETIC_IDENTITY],
            },
            ("the two reference-register records disagree", "model"),
        ),
    )
    for label, arguments, needles in cases:
        with scratch("profile") as temporary:
            root = tree(Path(temporary), **arguments, **pair)
            result = run_gate(f"--fixture-root={root}", "--budget=search")
            expect.refuses(result, label, *needles)
            expect.silent_about(
                result, label, "budget(s) within limits", "PASS  p99 under"
            )


def hosted_runner_cannot_publish_a_performance_baseline(expect: Expect) -> None:
    """The refusal is enforced, targeted, and not the only refusal available."""
    hosted = hosted_environment()
    workstation = workstation_environment()
    live_baseline = ROOT / "benchmarks/baselines/search.json"

    # Detection first, marker by marker, from the gate's own list.
    expect.that(
        BENCH_GATE.runner_kind({}) == BENCH_GATE.WORKSTATION,
        "an empty environment is not a build machine",
    )
    expect.that(
        BENCH_GATE.runner_kind({"CI": "false"}) == BENCH_GATE.WORKSTATION,
        "CI=false is not a build machine",
    )
    for marker in BENCH_GATE.CI_MARKERS:
        expect.that(
            BENCH_GATE.runner_kind({marker: "true"}) == BENCH_GATE.HOSTED,
            f"{marker}=true must read as a hosted runner",
        )
    expect.that(
        BENCH_GATE.runner_kind(
            {"CI": "true", "RUNNER_ENVIRONMENT": "self-hosted"}
        )
        == BENCH_GATE.SELF_HOSTED,
        "the self-hosted reference register must not read as hosted",
    )
    expect.that(
        BENCH_GATE.runner_kind({"RUNNER_ENVIRONMENT": "github-hosted"})
        == BENCH_GATE.HOSTED,
        "GitHub's hosted runners must read as hosted",
    )

    # Publishing, on a hosted runner, against the live records.
    result = run_gate(
        "--publish-baseline=search",
        "--reason=SYNTHETIC FIXTURE reason",
        environment=hosted,
    )
    expect.refuses(
        result,
        "a hosted runner publishing a live baseline",
        "never produce or bless a performance baseline",
        "runs-on: [self-hosted, reference-register]",
    )
    expect.silent_about(
        result,
        "the hosted refusal must come before any record is read",
        "no register-hardware row",
    )
    expect.that(
        not live_baseline.exists(),
        "a hosted publish attempt wrote benchmarks/baselines/search.json",
    )

    # The same command on a workstation is refused for a DIFFERENT reason,
    # which is what proves the refusal above was the hosted rule.
    result = run_gate(
        "--publish-baseline=search",
        "--reason=SYNTHETIC FIXTURE reason",
        environment=workstation,
    )
    expect.refuses(
        result,
        "a workstation publishing against blank records",
        "no register-hardware row",
    )
    expect.silent_about(
        result, "the workstation refusal is not the hosted one", "never produce or bless"
    )
    expect.that(
        not live_baseline.exists(),
        "a workstation publish attempt wrote a baseline over blank records",
    )

    # A hosted runner may not judge the live records either: it can only run
    # fixed fixtures.
    expect.refuses(
        run_gate(environment=hosted),
        "a hosted runner judging the live records",
        "may never produce or bless a performance baseline",
        "--fixture-root",
    )

    with scratch("hosted") as temporary:
        root = tree(Path(temporary), **passing_pair())
        for label, environment in (
            ("a fixture publish on a workstation", workstation),
            ("a fixture publish on a hosted runner", hosted),
        ):
            expect.refuses(
                run_gate(
                    f"--fixture-root={root}",
                    "--publish-baseline=search",
                    "--reason=SYNTHETIC FIXTURE reason",
                    environment=environment,
                ),
                label,
                "a fixture run cannot publish a baseline",
            )
        expect.that(
            not (root / "baselines" / "search.json").exists()
            or json.loads((root / "baselines" / "search.json").read_text())["reason"]
            == "SYNTHETIC FIXTURE baseline",
            "a fixture publish attempt rewrote the fixture baseline",
        )

        # And the permitted half: fixed pass/fail fixtures run anywhere, so the
        # refusal is targeted rather than a blanket ban on build machines.
        expect.passes(
            run_gate(f"--fixture-root={root}", "--budget=search", environment=hosted),
            "a hosted runner exercising a fixed fixture",
            "[FIXTURE]",
            "1 budget(s) within limits and baselines",
            "a fixture proves these thresholds, never this machine",
        )


def reference_profile_fields_mirror_the_device_matrix_columns(expect: Expect) -> None:
    """The two committed records declare the same twelve fields, in one order.

    This is the half of the correspondence that is checkable without a machine.
    The value-level half — a filled matrix row equalling a filled profile — is
    `reference_profile_matches_supported_device_matrix`, and it is deferred to
    the second half of 1.2.0 because no register exists to describe.
    """
    columns, rows = BENCH_GATE.read_matrix(BENCH_GATE.LIVE_MATRIX)
    identity = BENCH_GATE.read_profile(BENCH_GATE.LIVE_PROFILE)
    expect.that(
        columns == IDENTITY_FIELDS,
        f"§6a.1's register-hardware columns are {list(columns)}, not "
        f"{list(IDENTITY_FIELDS)}",
    )
    expect.that(
        tuple(identity) == IDENTITY_FIELDS,
        f"reference-register.toml declares {list(identity)}, not "
        f"{list(IDENTITY_FIELDS)}",
    )
    expect.that(
        rows == [],
        "§6a.1 now has a register-hardware row; the deferred half of 1.2.0 owns "
        "reference_profile_matches_supported_device_matrix and this test's "
        "assumption of an empty matrix",
    )

    # Negatives, so the correspondence above is a check and not a coincidence.
    with scratch("fields") as temporary:
        base = Path(temporary)
        expect.refuses(
            run_gate(
                f"--fixture-root={tree(base / 'short', matrix_columns=IDENTITY_FIELDS[:-1])}",
                "--check-profile",
            ),
            "a matrix missing a column",
            "requires exactly",
            "in that order",
        )
        expect.refuses(
            run_gate(
                f"--fixture-root={tree(base / 'reordered', profile_order=tuple(reversed(IDENTITY_FIELDS)))}",
                "--check-profile",
            ),
            "a profile whose keys are reordered",
            "must declare exactly the matrix columns",
        )
        expect.passes(
            run_gate(f"--fixture-root={tree(base / 'agreeing')}", "--check-profile"),
            "matching fields and values",
            "both records are filled and agree",
        )


def a_blank_reference_profile_and_matrix_are_detected_and_refused(
    expect: Expect,
) -> None:
    """Today's committed pair is blank on both sides, and is refused.

    The third assertion is the one that matters: two blank records are refused
    *as a pair*, so this case can never pass by comparing nothing to nothing.
    """
    result = run_gate("--check-profile")
    expect.refuses(
        result,
        "the committed records",
        "has no register-hardware row",
        "all 12 identity fields are blank",
        "order the hardware before group 1.7 starts",
    )
    expect.that(
        result.output.count("(no row)") == len(IDENTITY_FIELDS),
        "the profile report must show every matrix field as absent",
    )
    expect.that(
        result.output.count("(blank)") == len(IDENTITY_FIELDS),
        "the profile report must show every profile field as blank",
    )

    with scratch("blank") as temporary:
        base = Path(temporary)
        expect.passes(
            run_gate(f"--fixture-root={tree(base / 'filled')}", "--check-profile"),
            "a filled synthetic pair through the same code path",
            "both records are filled and agree",
        )
        expect.refuses(
            run_gate(
                "--fixture-root="
                f"{tree(base / 'blank', identity=BLANK_IDENTITY, matrix_rows=[BLANK_IDENTITY])}",
                "--check-profile",
            ),
            "a blank matrix row against a blank profile",
            "the reference register is blank",
        )


def a_noisy_regression_within_three_mads_is_not_a_failure(expect: Expect) -> None:
    """25% slower on a noisy baseline passes. Conjunction, not disjunction."""
    with scratch("noisy") as temporary:
        root = tree(
            Path(temporary),
            baselines={
                "search": sample(
                    median_ns=20 * MS,
                    p99_ns=30 * MS,
                    mad_ns=5 * MS,
                    reason="SYNTHETIC FIXTURE baseline",
                )
            },
            measurements={
                "search": sample(median_ns=25 * MS, p99_ns=30 * MS, mad_ns=5 * MS)
            },
        )
        expect.passes(
            run_gate(f"--fixture-root={root}", "--budget=search"),
            "25% slower but 1 MAD",
            "PASS  not more than 20% AND more than 3 baseline MADs slower",
            "25.0% slower (over 20%)",
            "1.0 baseline MADs (within 3)",
            "1 budget(s) within limits and baselines",
        )


def a_tight_regression_beyond_three_mads_is_not_a_failure(expect: Expect) -> None:
    """10 MADs slower on a very quiet baseline passes, at 5%."""
    with scratch("tight") as temporary:
        root = tree(
            Path(temporary),
            baselines={
                "search": sample(
                    median_ns=20 * MS,
                    p99_ns=30 * MS,
                    mad_ns=MS // 10,
                    reason="SYNTHETIC FIXTURE baseline",
                )
            },
            measurements={
                "search": sample(median_ns=21 * MS, p99_ns=30 * MS, mad_ns=MS // 10)
            },
        )
        expect.passes(
            run_gate(f"--fixture-root={root}", "--budget=search"),
            "10 MADs slower but 5%",
            "PASS  not more than 20% AND more than 3 baseline MADs slower",
            "5.0% slower (within 20%)",
            "10.0 baseline MADs (over 3)",
            "1 budget(s) within limits and baselines",
        )


def a_pin_verify_median_below_its_band_is_a_failure(expect: Expect) -> None:
    """§7's PIN row is a band and a ceiling, and the band has two sides.

    A 40 ms median passes the 500 ms p99 ceiling and is not a regression. It is
    still a failure, because a PIN verify that fast means the Argon2 parameters
    are too weak — which is the security half of the budget, and the half a
    p99-only reading would silently delete.
    """
    band = "median inside 200.000000 ms - 350.000000 ms"
    with scratch("pin") as temporary:
        base = Path(temporary)
        too_fast = tree(
            base / "too-fast",
            baselines={
                "pin-verify": sample(
                    budget="pin-verify",
                    median_ns=250 * MS,
                    p99_ns=300 * MS,
                    mad_ns=5 * MS,
                    reason="SYNTHETIC FIXTURE baseline",
                )
            },
            measurements={
                "pin-verify": sample(
                    budget="pin-verify",
                    median_ns=40 * MS,
                    p99_ns=60 * MS,
                    mad_ns=5 * MS,
                )
            },
        )
        result = run_gate(f"--fixture-root={too_fast}", "--budget=pin-verify")
        expect.fails_a_budget(
            result,
            "a 40 ms PIN median",
            f"FAIL  {band}",
            "FAILED: pin-verify",
        )
        expect.says(
            result,
            "only the band may have failed a 40 ms median",
            "PASS  p99 under 500.000000 ms",
            "PASS  not more than 20% AND more than 3 baseline MADs slower",
        )

        # The upper side, chosen so the p99 ceiling and the regression rule both
        # pass: 355 ms is 4.4% over a 340 ms baseline, and 420 ms is legal p99.
        too_slow = tree(
            base / "too-slow",
            baselines={
                "pin-verify": sample(
                    budget="pin-verify",
                    median_ns=340 * MS,
                    p99_ns=400 * MS,
                    mad_ns=20 * MS,
                    reason="SYNTHETIC FIXTURE baseline",
                )
            },
            measurements={
                "pin-verify": sample(
                    budget="pin-verify",
                    median_ns=355 * MS,
                    p99_ns=420 * MS,
                    mad_ns=20 * MS,
                )
            },
        )
        result = run_gate(f"--fixture-root={too_slow}", "--budget=pin-verify")
        expect.fails_a_budget(result, "a 355 ms PIN median", f"FAIL  {band}")
        expect.says(
            result,
            "only the band may have failed a 355 ms median",
            "PASS  p99 under 500.000000 ms",
            "PASS  not more than 20% AND more than 3 baseline MADs slower",
        )

        inside = tree(
            base / "inside",
            baselines={
                "pin-verify": sample(
                    budget="pin-verify",
                    median_ns=250 * MS,
                    p99_ns=300 * MS,
                    mad_ns=5 * MS,
                    reason="SYNTHETIC FIXTURE baseline",
                )
            },
            measurements={
                "pin-verify": sample(
                    budget="pin-verify",
                    median_ns=260 * MS,
                    p99_ns=310 * MS,
                    mad_ns=5 * MS,
                )
            },
        )
        expect.passes(
            run_gate(f"--fixture-root={inside}", "--budget=pin-verify"),
            "a 260 ms PIN median",
            f"PASS  {band}",
            "1 budget(s) within limits and baselines",
        )


def a_baseline_outside_its_own_absolute_limit_is_refused(expect: Expect) -> None:
    """Otherwise a red gate is repaired by republishing the slow number."""
    with scratch("baseline-limit") as temporary:
        root = tree(
            Path(temporary),
            baselines={
                "search": sample(
                    median_ns=31 * MS,
                    p99_ns=60 * MS,
                    mad_ns=1 * MS,
                    reason="SYNTHETIC FIXTURE baseline",
                )
            },
            measurements={
                "search": sample(median_ns=31 * MS, p99_ns=44 * MS, mad_ns=1 * MS)
            },
        )
        expect.refuses(
            run_gate(f"--fixture-root={root}", "--budget=search"),
            "a baseline whose own p99 breaches §7",
            "is itself outside conventions §7",
            "may not sit outside its own absolute limit",
        )


def a_record_from_another_machine_is_refused(expect: Expect) -> None:
    """A number with the wrong machine attached is not evidence."""
    other = {**SYNTHETIC_IDENTITY, "model": "SYNTHETIC FIXTURE other device"}
    cases = (
        (
            "a baseline from another machine",
            {
                "baselines": {
                    "search": sample(
                        median_ns=31 * MS,
                        p99_ns=44 * MS,
                        mad_ns=1 * MS,
                        reason="SYNTHETIC FIXTURE baseline",
                        identity=other,
                    )
                },
                "measurements": {
                    "search": sample(median_ns=31 * MS, p99_ns=44 * MS, mad_ns=1 * MS)
                },
            },
        ),
        (
            "a measurement from another machine",
            {
                "baselines": {
                    "search": sample(
                        median_ns=31 * MS,
                        p99_ns=44 * MS,
                        mad_ns=1 * MS,
                        reason="SYNTHETIC FIXTURE baseline",
                    )
                },
                "measurements": {
                    "search": sample(
                        median_ns=31 * MS,
                        p99_ns=44 * MS,
                        mad_ns=1 * MS,
                        identity=other,
                    )
                },
            },
        ),
    )
    for label, arguments in cases:
        with scratch("machine") as temporary:
            root = tree(Path(temporary), **arguments)
            expect.refuses(
                run_gate(f"--fixture-root={root}", "--budget=search"),
                label,
                "was taken on a different machine",
                "model",
            )


def an_empty_budget_set_cannot_report_success(expect: Expect) -> None:
    """Exit zero over nothing measured would read as "budgets met"."""
    with scratch("empty") as temporary:
        root = tree(Path(temporary))
        expect.refuses(
            run_gate(f"--fixture-root={root}"),
            "no budget implemented, no argument",
            "cannot report success over an empty set",
            "1.2.7",
        )
        expect.refuses(
            run_gate(f"--fixture-root={root}", "--budget=search"),
            "a named budget with no baseline",
            "not implemented at this gate",
            "1.2.7",
        )
        expect.refuses(
            run_gate(f"--fixture-root={root}", "--budget=cold-start"),
            "the Phase-2 budget",
            "arrives in Phase 2",
            "median, not a p99",
        )
        unknown = run_gate(f"--fixture-root={root}", "--budget=cold-brew")
        expect.exits(unknown, 2, "an unknown slug")
        expect.says(unknown, "an unknown slug", "unknown budget 'cold-brew'")

    # And the committed tree, today: `just bench-gate` cannot exit zero.
    expect.refuses(
        run_gate(), "the committed tree with no argument", "no register-hardware row"
    )


def a_synthetic_fixture_identity_can_never_become_the_committed_profile(
    expect: Expect,
) -> None:
    """The live path refuses a fixture identity; the fixture path accepts it."""
    expect.that(
        all(
            "synthetic" in value.casefold()
            for value in SYNTHETIC_IDENTITY.values()
        ),
        "every fixture identity value must be obviously synthetic",
    )
    expect.that(
        sorted(SYNTHETIC_IDENTITY) == sorted(IDENTITY_FIELDS),
        "the fixture identity must carry exactly the gate's identity fields",
    )
    expect.that(
        BENCH_GATE.synthetic_fields(BENCH_GATE.read_profile(BENCH_GATE.LIVE_PROFILE))
        == [],
        "the committed reference-register profile carries a synthetic marker",
    )

    with scratch("synthetic") as temporary:
        root = tree(Path(temporary))
        fixture_records = BENCH_GATE.Records(
            matrix=root / "device-matrix.md",
            profile=root / "reference-register.toml",
            baselines=root / "baselines",
            measurements=root / "measurements",
            live=False,
        )
        live_records = dataclasses.replace(fixture_records, live=True)
        live_refusal = ""
        try:
            BENCH_GATE.check_reference(live_records, [])
        except BENCH_GATE.Refused as error:
            live_refusal = str(error)
        expect.that(
            "synthetic marker" in live_refusal,
            "the live path must refuse a synthetic fixture identity; it said "
            f"{live_refusal!r}",
        )

        fixture_refusal = ""
        try:
            BENCH_GATE.check_reference(fixture_records, [])
        except BENCH_GATE.GateError as error:
            fixture_refusal = str(error)
        expect.that(
            fixture_refusal == "",
            f"a fixture run refused its own synthetic identity: {fixture_refusal}",
        )


def a_published_baseline_is_readable_by_the_gate(expect: Expect) -> None:
    """What `--publish-baseline` writes, the next run must accept.

    Nothing exercises this end to end from the command line yet — publishing
    needs a filled, non-synthetic reference register, which does not exist — so
    the round trip is driven through the gate's own writer and then judged by
    the real command. It is worth its own case: the writer sorts keys, and the
    reader briefly required `profile_identity` in a fixed order, which meant the
    gate would have refused its own output the first time 1.2.7 published one.
    """
    with scratch("roundtrip") as temporary:
        root = tree(
            Path(temporary),
            measurements={
                "search": sample(median_ns=31 * MS, p99_ns=44 * MS, mad_ns=1 * MS)
            },
        )
        records = BENCH_GATE.Records(
            matrix=root / "device-matrix.md",
            profile=root / "reference-register.toml",
            baselines=root / "baselines",
            measurements=root / "measurements",
            live=False,
        )
        reference = BENCH_GATE.check_reference(records, [])
        run = BENCH_GATE.read_sample(
            root / "measurements" / "search.json",
            BENCH_GATE.BY_SLUG["search"],
            reference,
            baseline=False,
        )
        written = root / "baselines" / "search.json"
        BENCH_GATE.write_record(
            written,
            BENCH_GATE.baseline_document(run, reference, "SYNTHETIC FIXTURE reason"),
        )
        expect.that(
            json.loads(written.read_text(encoding="utf-8"))["reason"]
            == "SYNTHETIC FIXTURE reason",
            "the published baseline must carry the reason it was moved for",
        )
        expect.passes(
            run_gate(f"--fixture-root={root}", "--budget=search"),
            "the gate reading its own published baseline",
            "1 budget(s) within limits and baselines",
        )


def a_recipe_argument_never_becomes_shell_source(expect: Expect) -> None:
    """`just bench-gate <arg>` keeps its argument as inert argv data.

    The same property `scripts/check-justfile-policy.py` proves for `branch`,
    `pr` and `merge`, proved here for the recipe this microstep adds.
    """
    just = shutil.which("just")
    if just is None:
        expect.that(
            False,
            "just is not on PATH, so the recipe's argument-safety proof could "
            "not run; this case is not optional",
        )
        return

    def recipe(*arguments: str) -> Result:
        completed = subprocess.run(
            [just, *arguments],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            env=workstation_environment(),
        )
        return Result(completed.returncode, completed.stdout + completed.stderr)

    rendered = recipe("--dry-run", "bench-gate", "x$(printf INJECTED)")
    expect.that(
        rendered.code == 0, f"the dry run failed unexpectedly: {rendered.output}"
    )
    expect.that(
        "INJECTED" not in rendered.output,
        "the recipe rendered its argument into shell source",
    )
    expect.that(
        '[ -z "$budget" ]' in rendered.output
        and '"--budget=$budget"' in rendered.output,
        "the recipe must pass its exported parameter as quoted shell data",
    )

    with scratch("recipe") as temporary:
        sentinel = Path(temporary) / "injection-ran"
        payload = f"x$(touch {sentinel})"
        result = recipe("bench-gate", payload)
        expect.that(
            result.code != 0, "a hostile argument produced a zero exit status"
        )
        expect.that(
            not sentinel.exists(), "a recipe argument became executable shell source"
        )
        expect.says(
            result, "the argument must arrive verbatim as data", "unknown budget", payload
        )


CASES = (
    bench_gate_fails_an_absolute_budget,
    bench_gate_fails_a_significant_regression,
    bench_gate_refuses_a_missing_reference_profile,
    hosted_runner_cannot_publish_a_performance_baseline,
    reference_profile_fields_mirror_the_device_matrix_columns,
    a_blank_reference_profile_and_matrix_are_detected_and_refused,
    a_noisy_regression_within_three_mads_is_not_a_failure,
    a_tight_regression_beyond_three_mads_is_not_a_failure,
    a_pin_verify_median_below_its_band_is_a_failure,
    a_baseline_outside_its_own_absolute_limit_is_refused,
    a_record_from_another_machine_is_refused,
    an_empty_budget_set_cannot_report_success,
    a_synthetic_fixture_identity_can_never_become_the_committed_profile,
    a_published_baseline_is_readable_by_the_gate,
    a_recipe_argument_never_becomes_shell_source,
)


def run_cases() -> tuple[int, int, list[str]]:
    """Every case, with its own failures. Returns (failures, assertions, lines)."""
    failures = 0
    assertions = 0
    lines: list[str] = []
    for case in CASES:
        expect = Expect()
        try:
            case(expect)
        except Exception as error:
            expect.problems.append(f"raised {type(error).__name__}: {error}")
        if expect.assertions == 0:
            # A case that asserts nothing and reports success is worse than an
            # absent case, because a green row hides it.
            expect.problems.append("the case made no assertions at all")
        assertions += expect.assertions
        if expect.problems:
            failures += 1
            lines.append(f"  FAIL  {case.__name__}")
            lines.extend(f"          {problem}" for problem in expect.problems)
        else:
            lines.append(f"  ok    {case.__name__}  ({expect.assertions} assertions)")
    return failures, assertions, lines


def main(argv: Sequence[str]) -> int:
    if argv:
        print("usage: bench_gate_test.py", file=sys.stderr)
        return 2
    if not GATE.is_file():
        print(f"bench-gate tests: {GATE} is missing", file=sys.stderr)
        return 2

    failures, assertions, lines = run_cases()
    for line in lines:
        print(line)
    print()
    if failures:
        print(
            f"bench-gate tests: {failures} of {len(CASES)} case(s) FAILED "
            f"({assertions} assertions)"
        )
        return 1
    print(
        f"bench-gate tests: {len(CASES)} cases passed, {assertions} assertions; "
        "every refusal path exits non-zero and says why"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
