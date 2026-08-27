#!/usr/bin/env python3
"""A budget without a command that exits non-zero is a wish.

`docs/implementation/01-conventions.md` §7 carries five absolute performance
limits and §7.1 carries the methodology. Neither was enforced by anything:
`cargo bench` prints a number and exits 0 whatever that number is, so a till
that got 40% slower shipped green. This is the command that exits non-zero.

What it decides, and what it deliberately refuses to decide
-----------------------------------------------------------
It compares a fresh measurement against a committed baseline and against the
absolute limit in §7. It does not measure anything: microsteps 1.2.7 (search),
1.4.9 (cart total), 1.6.2 (PIN verify) and 1.11.13 (scan to line) add the
benchmarks and the first baselines, and 1.12.3 adds the live reference-register
CI job. Until a budget has a committed baseline it is not implemented at this
gate, and naming it is refused rather than reported as a pass.

The two records, and why a blank one is refused
-----------------------------------------------
§7.1 defines the reference register as the LOWEST register-hardware row of the
supported-device matrix in `ref/hardware-and-receipts.md` §6a, mirrored in
`benchmarks/reference-register.toml`, and says "no baseline is accepted while
either record is blank or they disagree". Both records are blank today because
no register has been bought: §6a ships the matrix empty on purpose, because
"filling it with model numbers nobody has held would be the same defect as a
compliance claim nobody earned". So `--check-profile` exits non-zero, and that
is this gate working, not this gate broken. Nothing here invents a machine to
make itself pass.

Integers, not floats
--------------------
Every duration in every record is an integer nanosecond count, and every
comparison this file makes is integer arithmetic. Invariant 1 bans float for
money only, but a gate whose verdict at the 20% boundary depends on binary
rounding is a gate that argues with itself. `median_ns * 100 > baseline * 120`
either holds or it does not.

The regression rule is a conjunction
------------------------------------
§7.1: exit non-zero "when an absolute limit is exceeded, or when the median is
more than 20% slower **and** more than three baseline median absolute
deviations slower". The `and` is load-bearing in both directions. A 20%-only
trigger on a quiet baseline makes the gate fire on ordinary noise and it gets
switched off within a month; a MAD-only trigger on a very quiet baseline fires
on a change too small to care about. Both conditions, every time.

Which statistic each limit applies to
-------------------------------------
The absolute limit applies to p99, except cold start, whose §7 entry is
explicitly a median, and PIN verification, which carries both a two-sided
median band (200-350 ms) and a p99 ceiling (500 ms). Getting this backwards
makes a budget unfailable, so it is data on `Budget`, not a branch.

Usage:  scripts/bench-gate.py                       every implemented budget
        scripts/bench-gate.py --budget=search       one budget
        scripts/bench-gate.py --check-profile       the two hardware records only
        scripts/bench-gate.py --fixture-root=DIR    fixed pass/fail fixtures
        scripts/bench-gate.py --publish-baseline=SLUG --reason='why'
Exit:   0 every implemented budget passed
        1 FAILED  — an absolute limit was exceeded, or a regression is both
                    more than 20% and more than three baseline MADs slower
        2 ERROR   — could not run at all: usage, or an unreadable record
        3 REFUSED — parsed, and this run may not judge a budget: a blank or
                    mismatched reference register, a hosted runner, or nothing
                    implemented to measure
"""

from __future__ import annotations

import json
import os
import sys
import tomllib
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

LIVE_MATRIX = ROOT / "docs/implementation/ref/hardware-and-receipts.md"
LIVE_PROFILE = ROOT / "benchmarks/reference-register.toml"
LIVE_BASELINES = ROOT / "benchmarks/baselines"
LIVE_MEASUREMENTS = ROOT / "benchmarks/measurements"

# The matrix is a document first. These markers make the one machine-read table
# in it unambiguous, so the parser never has to guess which of §6a's tables is
# the register-hardware matrix, and a reader can grep for them.
MATRIX_BEGIN = "<!-- bench-gate:register-hardware-matrix -->"
MATRIX_END = "<!-- /bench-gate:register-hardware-matrix -->"

# The identity of the machine, in the order both records must declare it. The
# set comes from §7.1 ("CPU, RAM, storage, OS version, power mode, device-matrix
# identity and release-build profile") plus §6a's qualification triple, because
# §6a says a profile with no qualifying commit is a claim.
IDENTITY_FIELDS = (
    "profile_id",
    "maker",
    "model",
    "cpu",
    "ram",
    "storage",
    "os_version",
    "power_mode",
    "release_profile",
    "qualified_at",
    "qualified_by",
    "qualifying_commit",
)

# Fixtures have to be obviously synthetic, and they have to stay out of the
# committed record. Any identity value carrying this marker is refused on the
# live path, so a fixture can never be promoted into `reference-register.toml`
# by a copy-paste.
SYNTHETIC_MARKER = "synthetic"

MILLISECOND_NS = 1_000_000
SECOND_NS = 1_000_000_000

REGRESSION_PERCENT = 20
REGRESSION_MADS = 3

P99 = "p99"
MEDIAN = "median"


@dataclass(frozen=True)
class Budget:
    """One row of conventions §7, as data rather than as a branch."""

    slug: str
    title: str
    # Which statistic the absolute limit applies to. p99 for every Phase-1
    # budget; median for cold start, whose §7 entry is explicitly a median.
    limit_statistic: str
    limit_ns: int
    # PIN verification only. Two-sided on purpose: a median under the band
    # means the Argon2 parameters are too weak, which is a security failure
    # rather than a fast till. 1.6.2 owns those parameters.
    median_band_ns: tuple[int, int] | None
    minimum_samples: int
    # The microstep that adds the benchmark and the first baseline. Named in
    # the refusal, so "not implemented" points somewhere.
    owner: str
    # Phase 1 budgets are implementable at this gate. Cold start is designed
    # for here and implemented in Phase 2; §7.1 says so and 2.9.3/2.9.5 own it.
    phase: int


BUDGETS: tuple[Budget, ...] = (
    Budget(
        slug="search",
        title="Search-as-you-type, 50k SKUs",
        limit_statistic=P99,
        limit_ns=50 * MILLISECOND_NS,
        median_band_ns=None,
        minimum_samples=50,
        owner="1.2.7",
        phase=1,
    ),
    Budget(
        slug="price-cart",
        title="Cart total recompute, 200 lines",
        limit_statistic=P99,
        limit_ns=16 * MILLISECOND_NS,
        median_band_ns=None,
        minimum_samples=50,
        owner="1.4.9",
        phase=1,
    ),
    Budget(
        slug="pin-verify",
        title="PIN verification",
        limit_statistic=P99,
        limit_ns=500 * MILLISECOND_NS,
        median_band_ns=(200 * MILLISECOND_NS, 350 * MILLISECOND_NS),
        minimum_samples=50,
        owner="1.6.2",
        phase=1,
    ),
    Budget(
        slug="scan-to-line",
        title="Scan to line visible",
        limit_statistic=P99,
        limit_ns=100 * MILLISECOND_NS,
        median_band_ns=None,
        minimum_samples=50,
        owner="1.11.13",
        phase=1,
    ),
    Budget(
        slug="cold-start",
        title="Cold start to sellable",
        limit_statistic=MEDIAN,
        limit_ns=3 * SECOND_NS,
        median_band_ns=None,
        minimum_samples=10,
        owner="2.9.3 / 2.9.5",
        phase=2,
    ),
)

BY_SLUG = {budget.slug: budget for budget in BUDGETS}

# A record is a fixed set of keys. An unknown key is refused rather than
# ignored, because a typo in `median_ns` that is silently dropped is a baseline
# with no median at all.
COMMON_KEYS = frozenset(
    {
        "budget",
        "samples",
        "median_ns",
        "p99_ns",
        "mad_ns",
        "taken_at",
        "taken_by",
        "commit",
        "profile_identity",
    }
)
BASELINE_KEYS = COMMON_KEYS | {"reason"}
MEASUREMENT_KEYS = COMMON_KEYS

# Environment variables that mean "this is a build machine". `RUNNER_ENVIRONMENT`
# is checked first because GitHub sets it to `self-hosted` on the runner §7.1
# reserves for the live job (`runs-on: [self-hosted, reference-register]`) and
# to `github-hosted` otherwise.
CI_MARKERS = (
    "APPVEYOR",
    "BITBUCKET_BUILD_NUMBER",
    "BUILDKITE",
    "CI",
    "CIRCLECI",
    "CODEBUILD_BUILD_ID",
    "CONTINUOUS_INTEGRATION",
    "DRONE",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "HEROKU_TEST_RUN_ID",
    "JENKINS_URL",
    "SEMAPHORE",
    "TEAMCITY_VERSION",
    "TF_BUILD",
    "TRAVIS",
    "WOODPECKER_CI",
)
FALSEY = frozenset({"", "0", "false", "no", "off"})

WORKSTATION = "workstation"
HOSTED = "hosted"
SELF_HOSTED = "self-hosted"


class GateError(Exception):
    """A message this gate can print, carrying the exit code it implies."""

    code = 2


class Unreadable(GateError):
    """A record could not be parsed at all. Nothing was judged."""

    code = 2


class Refused(GateError):
    """The records parsed and this run may not judge a budget."""

    code = 3


# ── the machine this is running on ────────────────────────────────────────


def runner_kind(environment: Mapping[str, str]) -> str:
    """`workstation`, `self-hosted` or `hosted`.

    This is not an authentication boundary and is not presented as one: anyone
    can export `RUNNER_ENVIRONMENT=self-hosted` on a laptop. It is the control
    that stops a hosted runner from *accidentally* publishing or blessing a
    number, which is the failure §7.1 names — a hosted runner varies well
    beyond 20% on a 16 ms workload, so its numbers are noise wearing a budget's
    clothes. Publishing additionally requires a filled, agreeing reference
    profile and lands as a reviewed `perf(...)` commit.
    """
    declared = environment.get("RUNNER_ENVIRONMENT", "").strip().casefold()
    if declared == SELF_HOSTED:
        return SELF_HOSTED
    if declared == "github-hosted":
        return HOSTED
    for name in CI_MARKERS:
        if environment.get(name, "").strip().casefold() not in FALSEY:
            return HOSTED
    return WORKSTATION


# ── the two hardware records ──────────────────────────────────────────────


@dataclass(frozen=True)
class Records:
    """Where this run reads its records from."""

    matrix: Path
    profile: Path
    baselines: Path
    measurements: Path
    # False for a `--fixture-root` run. A fixture proves the thresholds; it
    # never proves this machine, may never publish a baseline, and every line
    # it prints says FIXTURE.
    live: bool


def live_records() -> Records:
    return Records(
        matrix=LIVE_MATRIX,
        profile=LIVE_PROFILE,
        baselines=LIVE_BASELINES,
        measurements=LIVE_MEASUREMENTS,
        live=True,
    )


def fixture_records(root: Path) -> Records:
    return Records(
        matrix=root / "device-matrix.md",
        profile=root / "reference-register.toml",
        baselines=root / "baselines",
        measurements=root / "measurements",
        live=False,
    )


def display(path: Path) -> str:
    """Repository-relative when it can be, so a failure line is clickable."""
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError as error:
        raise Refused(f"{display(path)} does not exist") from error
    except (OSError, UnicodeError) as error:
        raise Unreadable(f"{display(path)} could not be read: {error}") from error


def table_cells(line: str) -> list[str] | None:
    """The cells of one Markdown table row, or None if this is not a row."""
    stripped = line.strip()
    if not stripped.startswith("|") or not stripped.endswith("|"):
        return None
    return [cell.strip().strip("`").strip() for cell in stripped[1:-1].split("|")]


def is_separator(cells: Sequence[str]) -> bool:
    return bool(cells) and all(
        cell and set(cell) <= set(":-") and "-" in cell for cell in cells
    )


def read_matrix(path: Path) -> tuple[tuple[str, ...], list[dict[str, str]]]:
    """The register-hardware table between the markers: columns, then rows.

    Zero rows is a legitimate, expected state today and is reported as such by
    the caller. A malformed table is not: it means the document says something
    the gate cannot read, which is `Unreadable`, never a silent empty matrix.
    """
    text = read_text(path)
    if text.count(MATRIX_BEGIN) != 1 or text.count(MATRIX_END) != 1:
        raise Unreadable(
            f"{display(path)} must contain exactly one {MATRIX_BEGIN} and one "
            f"{MATRIX_END} marker around the register-hardware table"
        )
    body = text.split(MATRIX_BEGIN, 1)[1].split(MATRIX_END, 1)[0]
    if MATRIX_BEGIN in body:
        raise Unreadable(f"{display(path)} nests the register-hardware markers")

    header: tuple[str, ...] | None = None
    separator_seen = False
    rows: list[dict[str, str]] = []
    for number, line in enumerate(body.splitlines(), start=1):
        if not line.strip():
            continue
        cells = table_cells(line)
        if cells is None:
            raise Unreadable(
                f"{display(path)}: line {number} between the register-hardware "
                "markers is neither blank nor a Markdown table row"
            )
        if header is None:
            header = tuple(cells)
            continue
        if not separator_seen:
            if not is_separator(cells):
                raise Unreadable(
                    f"{display(path)}: the register-hardware table has no "
                    "header separator row"
                )
            separator_seen = True
            continue
        if len(cells) != len(header):
            raise Unreadable(
                f"{display(path)}: register-hardware row {len(rows) + 1} has "
                f"{len(cells)} cells against {len(header)} columns"
            )
        rows.append(dict(zip(header, cells, strict=True)))

    if header is None or not separator_seen:
        raise Unreadable(
            f"{display(path)} has no register-hardware table between its markers"
        )
    return header, rows


def read_profile(path: Path) -> dict[str, str]:
    """`[identity]` from the reference-register profile, as text."""
    if not path.exists():
        raise Refused(
            f"the reference-register profile {display(path)} does not exist; "
            "conventions §7.1 accepts no baseline without it"
        )
    try:
        with path.open("rb") as handle:
            document = tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise Unreadable(f"{display(path)} is not readable TOML: {error}") from error

    identity = document.get("identity")
    if not isinstance(identity, dict):
        raise Unreadable(f"{display(path)} has no [identity] table")
    for field, value in identity.items():
        if not isinstance(value, str):
            raise Unreadable(
                f"{display(path)} [identity] {field} must be text; units belong "
                "inside the value"
            )
    return dict(identity)


def blanks(identity: Mapping[str, str]) -> list[str]:
    return [field for field in IDENTITY_FIELDS if not identity.get(field, "").strip()]


def synthetic_fields(identity: Mapping[str, str]) -> list[str]:
    return [
        field
        for field in IDENTITY_FIELDS
        if SYNTHETIC_MARKER in identity.get(field, "").casefold()
    ]


@dataclass(frozen=True)
class Reference:
    """The verified reference register: one identity, agreed by both records."""

    identity: dict[str, str]


def check_reference(records: Records, report: list[str]) -> Reference:
    """The premise of every verdict: one machine, named the same way twice.

    Raises `Refused` while either record is blank or they disagree, which is
    conventions §7.1 word for word, and is the state of this repository today.
    """
    columns, rows = read_matrix(records.matrix)
    identity = read_profile(records.profile)

    # Same fields, same order, in both records. Ordering is part of the
    # contract because there is exactly one right order for a hand-maintained
    # pair, and a reordered column is how a value lands in the wrong field.
    if columns != IDENTITY_FIELDS:
        raise Refused(
            f"{display(records.matrix)}'s register-hardware columns are "
            f"{list(columns)}; conventions §7.1 requires exactly "
            f"{list(IDENTITY_FIELDS)}, in that order"
        )
    if tuple(identity) != IDENTITY_FIELDS:
        raise Refused(
            f"{display(records.profile)} [identity] declares "
            f"{list(identity)}; it must declare exactly the matrix columns "
            f"{list(IDENTITY_FIELDS)}, in that order"
        )

    profile_blanks = blanks(identity)
    if not rows:
        detail = (
            f"all {len(profile_blanks)} identity fields are blank as well"
            if len(profile_blanks) == len(IDENTITY_FIELDS)
            else f"blank profile fields: {', '.join(profile_blanks) or 'none'}"
        )
        raise Refused(
            f"{display(records.matrix)} §6a.1 has no register-hardware row, so "
            f"no reference register exists and {detail}. §6a: order the "
            "hardware before group 1.7 starts. Conventions §7.1: no baseline "
            "is accepted while either record is blank or they disagree"
        )

    lowest = rows[0]
    matrix_blanks = [
        field for field in IDENTITY_FIELDS if not lowest.get(field, "").strip()
    ]
    if matrix_blanks or profile_blanks:
        raise Refused(
            "the reference register is blank: "
            f"{display(records.matrix)} row 1 is missing "
            f"[{', '.join(matrix_blanks) or 'nothing'}] and "
            f"{display(records.profile)} is missing "
            f"[{', '.join(profile_blanks) or 'nothing'}]"
        )

    if records.live:
        planted = synthetic_fields(identity) or synthetic_fields(lowest)
        if planted:
            raise Refused(
                "a fixture identity may never be the committed reference "
                f"register; {', '.join(planted)} still carries the synthetic "
                "marker"
            )

    disagree = [
        f"{field}: matrix {lowest[field]!r} vs profile {identity[field]!r}"
        for field in IDENTITY_FIELDS
        if lowest[field].strip() != identity[field].strip()
    ]
    if disagree:
        raise Refused(
            "the two reference-register records disagree — "
            + "; ".join(disagree)
            + f". Fix {display(records.profile)} or the §6a.1 row; a baseline "
            "cannot name two machines"
        )

    report.append(
        f"reference register: {identity['maker']} {identity['model']} "
        f"({identity['profile_id']}), qualified {identity['qualified_at']} "
        f"by {identity['qualified_by']} in {identity['qualifying_commit']}"
    )
    report.append(
        f"  {len(rows)} register-hardware row(s) in "
        f"{display(records.matrix)} §6a.1; row 1 is the lowest and is the one "
        "every §7 limit is measured on"
    )
    return Reference(identity=dict(identity))


def profile_report(records: Records) -> list[str]:
    """A field-by-field view of both records, for `--check-profile`.

    Printed before the verdict so a blank pair is visible rather than merely
    asserted, and so the person filling it in can see which side is missing.
    """
    lines = [
        f"matrix:  {display(records.matrix)} §6a.1",
        f"profile: {display(records.profile)}",
        "",
        f"  {'field':<20}{'matrix row 1':<28}profile",
    ]
    try:
        columns, rows = read_matrix(records.matrix)
    except GateError:
        columns, rows = (), []
    try:
        identity = read_profile(records.profile)
    except GateError:
        identity = {}
    lowest = rows[0] if rows else {}
    for field in IDENTITY_FIELDS:
        if field not in columns:
            matrix_cell = "(no such column)"
        elif not rows:
            matrix_cell = "(no row)"
        else:
            matrix_cell = lowest.get(field, "").strip() or "(blank)"
        profile_cell = identity.get(field, "").strip() or "(blank)"
        if field not in identity:
            profile_cell = "(no such key)"
        lines.append(f"  {field:<20}{matrix_cell:<28}{profile_cell}")
    lines.append("")
    return lines


# ── measurements and baselines ────────────────────────────────────────────


@dataclass(frozen=True)
class Sample:
    """One run's three numbers, plus the machine that produced them."""

    budget: str
    samples: int
    median_ns: int
    p99_ns: int
    mad_ns: int
    taken_at: str
    taken_by: str
    commit: str
    reason: str


def whole_number(document: Mapping[str, object], key: str, where: str) -> int:
    value = document.get(key)
    # `bool` is an `int` in Python and would silently become 0 or 1.
    if not isinstance(value, int) or isinstance(value, bool):
        raise Unreadable(
            f"{where}: {key} must be a whole number of nanoseconds, not "
            f"{value!r} — every duration in this gate is an integer"
        )
    return value


def text_field(document: Mapping[str, object], key: str, where: str) -> str:
    value = document.get(key)
    if not isinstance(value, str) or not value.strip():
        raise Unreadable(f"{where}: {key} must be a non-empty string")
    return value


def read_sample(
    path: Path,
    budget: Budget,
    reference: Reference,
    *,
    baseline: bool,
) -> Sample:
    """One JSON record, validated against §7 and against the reference machine."""
    where = display(path)
    try:
        document = json.loads(read_text(path))
    except json.JSONDecodeError as error:
        raise Unreadable(f"{where} is not readable JSON: {error}") from error
    if not isinstance(document, dict):
        raise Unreadable(f"{where} must be a JSON object")

    required = BASELINE_KEYS if baseline else MEASUREMENT_KEYS
    present = set(document)
    if present != required:
        missing = sorted(required - present)
        unknown = sorted(present - required)
        raise Unreadable(
            f"{where} must carry exactly {sorted(required)}; missing "
            f"{missing}, unknown {unknown}"
        )

    named = text_field(document, "budget", where)
    if named != budget.slug:
        raise Refused(
            f"{where} names budget {named!r} but sits where {budget.slug!r} "
            "belongs"
        )

    samples = whole_number(document, "samples", where)
    if samples < budget.minimum_samples:
        raise Refused(
            f"{where}: {samples} samples; conventions §7 requires at least "
            f"{budget.minimum_samples} measured samples after warm-up for "
            f"{budget.slug}"
        )

    median_ns = whole_number(document, "median_ns", where)
    p99_ns = whole_number(document, "p99_ns", where)
    mad_ns = whole_number(document, "mad_ns", where)
    if median_ns <= 0 or p99_ns <= 0:
        raise Refused(f"{where}: median_ns and p99_ns must both be positive")
    if mad_ns < 0:
        raise Refused(f"{where}: mad_ns cannot be negative")
    if p99_ns < median_ns:
        raise Refused(
            f"{where}: p99_ns {p99_ns} is below median_ns {median_ns}, so these "
            "are not three statistics of one sample set"
        )

    # The same twelve fields, but not in a fixed order: this record is written
    # by a program, and a JSON serializer is free to sort keys. The ordering
    # contract belongs to the two hand-maintained documents, where a reordered
    # column is a review signal.
    identity = document.get("profile_identity")
    if not isinstance(identity, dict) or set(identity) != set(IDENTITY_FIELDS):
        raise Refused(
            f"{where}: profile_identity must declare exactly "
            f"{list(IDENTITY_FIELDS)} — a number with no machine attached is "
            "not evidence"
        )
    mismatch = [
        field
        for field in IDENTITY_FIELDS
        if str(identity.get(field, "")).strip() != reference.identity[field].strip()
    ]
    if mismatch:
        raise Refused(
            f"{where} was taken on a different machine: "
            f"{', '.join(mismatch)} disagree with the reference register"
        )

    return Sample(
        budget=named,
        samples=samples,
        median_ns=median_ns,
        p99_ns=p99_ns,
        mad_ns=mad_ns,
        taken_at=text_field(document, "taken_at", where),
        taken_by=text_field(document, "taken_by", where),
        commit=text_field(document, "commit", where),
        reason=text_field(document, "reason", where) if baseline else "",
    )


# ── formatting, in exact integer arithmetic ───────────────────────────────


def milliseconds(value: int) -> str:
    """Nanoseconds as exact milliseconds. No rounding, so no argument."""
    sign = "-" if value < 0 else ""
    whole, fraction = divmod(abs(value), MILLISECOND_NS)
    return f"{sign}{whole}.{fraction:06d} ms"


def tenths(numerator: int, denominator: int) -> str:
    """`numerator / denominator` to one decimal place, truncated toward zero.

    Integer arithmetic, like every other number here: a printed ratio that
    disagrees with the verdict beside it is worse than no ratio at all.
    """
    if denominator == 0:
        return "n/a"
    magnitude = abs(numerator) * 10 // abs(denominator)
    negative = (numerator < 0) != (denominator < 0)
    whole, remainder = divmod(magnitude, 10)
    return f"{'-' if negative and magnitude else ''}{whole}.{remainder}"


def percent(numerator: int, denominator: int) -> str:
    return tenths(numerator * 100, denominator)


# ── the verdict ───────────────────────────────────────────────────────────


@dataclass(frozen=True)
class Check:
    name: str
    passed: bool
    detail: str


def absolute_checks(budget: Budget, run: Sample) -> list[Check]:
    """§7's absolute limit, against the statistic §7 wrote it against."""
    checks: list[Check] = []
    measured = run.p99_ns if budget.limit_statistic == P99 else run.median_ns
    checks.append(
        Check(
            name=f"{budget.limit_statistic} under {milliseconds(budget.limit_ns)}",
            passed=measured < budget.limit_ns,
            detail=f"{budget.limit_statistic} {milliseconds(measured)}",
        )
    )
    if budget.median_band_ns is not None:
        low, high = budget.median_band_ns
        checks.append(
            Check(
                name=(
                    f"median inside {milliseconds(low)} - {milliseconds(high)} "
                    "(a median below the band is too weak, not fast)"
                ),
                passed=low <= run.median_ns <= high,
                detail=f"median {milliseconds(run.median_ns)}",
            )
        )
    return checks


def regression_check(budget: Budget, run: Sample, baseline: Sample) -> Check:
    """§7.1's conjunction: more than 20% slower AND more than three MADs."""
    slower_ns = run.median_ns - baseline.median_ns
    over_percent = run.median_ns * 100 > baseline.median_ns * (100 + REGRESSION_PERCENT)
    over_mads = slower_ns > REGRESSION_MADS * baseline.mad_ns
    detail = (
        f"median {milliseconds(run.median_ns)} against baseline "
        f"{milliseconds(baseline.median_ns)}: "
        f"{percent(slower_ns, baseline.median_ns)}% slower "
        f"({'over' if over_percent else 'within'} {REGRESSION_PERCENT}%), "
        f"{tenths(slower_ns, baseline.mad_ns)} baseline MADs "
        f"({'over' if over_mads else 'within'} {REGRESSION_MADS})"
    )
    return Check(
        name=(
            f"not more than {REGRESSION_PERCENT}% AND more than "
            f"{REGRESSION_MADS} baseline MADs slower"
        ),
        passed=not (over_percent and over_mads),
        detail=detail,
    )


def judge(
    budget: Budget, records: Records, reference: Reference, report: list[str]
) -> bool:
    """One budget. True when every check passed."""
    baseline_path = records.baselines / f"{budget.slug}.json"
    measurement_path = records.measurements / f"{budget.slug}.json"
    baseline = read_sample(baseline_path, budget, reference, baseline=True)
    breached = [check for check in absolute_checks(budget, baseline) if not check.passed]
    if breached:
        # Otherwise the way to fix a red gate is to publish the slow number as
        # the new baseline, which is §7.1's "moving the baseline deletes the
        # budget" with extra steps. An absolute limit is absolute.
        raise Refused(
            f"{display(baseline_path)} is itself outside conventions §7 — "
            + "; ".join(f"{check.name}: {check.detail}" for check in breached)
            + "; a baseline may not sit outside its own absolute limit"
        )
    if not measurement_path.exists():
        raise Refused(
            f"no measurement for {budget.slug} at {display(measurement_path)}; "
            f"run its benchmark first (microstep {budget.owner})"
        )
    run = read_sample(measurement_path, budget, reference, baseline=False)

    checks = absolute_checks(budget, run)
    checks.append(regression_check(budget, run, baseline))

    report.append(f"{budget.slug} — {budget.title}, {run.samples} samples")
    report.append(
        f"  median {milliseconds(run.median_ns)}  p99 {milliseconds(run.p99_ns)}  "
        f"mad {milliseconds(run.mad_ns)}"
    )
    for check in checks:
        report.append(f"  {'PASS' if check.passed else 'FAIL'}  {check.name}")
        report.append(f"        {check.detail}")
    return all(check.passed for check in checks)


def implemented(records: Records) -> list[Budget]:
    """A budget is implemented at this gate exactly when it has a baseline.

    Nothing else can be the test. A benchmark that runs and reports a number
    without a committed baseline enforces nothing, which is the state §7.1
    calls a wish.
    """
    if not records.baselines.is_dir():
        return []
    return [
        budget
        for budget in BUDGETS
        if (records.baselines / f"{budget.slug}.json").is_file()
    ]


def resolve(slug: str | None) -> Budget | None:
    """Validate the caller's slug against §7.1's list, and nothing else.

    Deliberately independent of every file: a mistyped slug earns the message
    about the slug, not the message about the reference register, and the name
    the caller passed is echoed back verbatim so `just bench-gate` can prove
    its argument arrived as inert data.
    """
    if slug is None:
        return None
    budget = BY_SLUG.get(slug)
    if budget is None:
        raise Unreadable(
            f"unknown budget {slug!r}; conventions §7.1 names exactly "
            f"{', '.join(sorted(BY_SLUG))}"
        )
    return budget


def selected(records: Records, budget: Budget | None) -> list[Budget]:
    if budget is None:
        return implemented(records)

    baseline = records.baselines / f"{budget.slug}.json"
    if baseline.is_file():
        # Implemented is decided by the baseline, never by the phase field, so
        # a Phase-2 budget starts being judged the moment its baseline lands.
        return [budget]
    if budget.phase != 1:
        raise Refused(
            f"{budget.slug} arrives in Phase 2 (microstep {budget.owner}); its "
            f"§7 limit is a {budget.limit_statistic}, not a p99, and no budget "
            "for it is implemented at this gate"
        )
    raise Refused(
        f"{budget.slug} is not implemented at this gate: no baseline at "
        f"{display(baseline)}. Microstep {budget.owner} adds the benchmark and "
        "its first baseline"
    )


# ── publishing a baseline ─────────────────────────────────────────────────


def baseline_document(
    run: Sample, reference: Reference, reason: str
) -> dict[str, object]:
    """The exact bytes a baseline carries. Separate so it can be round-tripped."""
    return {
        "budget": run.budget,
        "commit": run.commit,
        "mad_ns": run.mad_ns,
        "median_ns": run.median_ns,
        "p99_ns": run.p99_ns,
        "profile_identity": {
            field: reference.identity[field] for field in IDENTITY_FIELDS
        },
        "reason": reason,
        "samples": run.samples,
        "taken_at": run.taken_at,
        "taken_by": run.taken_by,
    }


def write_record(path: Path, document: Mapping[str, object]) -> None:
    """Sorted keys and a trailing newline, so a baseline diff is about numbers."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(document, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def publish(
    records: Records, slug: str, reason: str, kind: str, report: list[str]
) -> None:
    """Write `benchmarks/baselines/<slug>.json` from the current measurement.

    Refused on a hosted runner before anything is read, because §7.1 says a
    hosted runner may "never produce or bless a performance baseline". Refused
    from a fixture run, because a fixture proves a threshold and not a machine.
    """
    if not records.live:
        raise Refused(
            "a fixture run cannot publish a baseline; it proves the thresholds, "
            "not this machine"
        )
    if kind == HOSTED:
        raise Refused(
            "a hosted runner may never produce or bless a performance baseline "
            "(conventions §7.1). Hosted jobs run this gate's fixed pass/fail "
            "fixtures with --fixture-root; the live job is pinned to "
            "runs-on: [self-hosted, reference-register] and belongs to 1.12.3"
        )
    if not reason.strip():
        raise Unreadable(
            "--publish-baseline needs --reason='why this till is slower'; "
            "conventions §7.1 requires a perf(...) change with before/after "
            "measurements and the reason, because moving a baseline without "
            "explaining it deletes the budget"
        )

    budget = resolve(slug)
    if budget is None:
        raise Unreadable(f"--publish-baseline needs a budget\n{USAGE}")
    reference = check_reference(records, report)
    measurement_path = records.measurements / f"{budget.slug}.json"
    if not measurement_path.exists():
        raise Refused(
            f"no measurement to publish at {display(measurement_path)}; run "
            f"the benchmark first (microstep {budget.owner})"
        )
    run = read_sample(measurement_path, budget, reference, baseline=False)
    breached = [check for check in absolute_checks(budget, run) if not check.passed]
    if breached:
        raise Refused(
            "this measurement is outside conventions §7 — "
            + "; ".join(f"{check.name}: {check.detail}" for check in breached)
            + "; an absolute limit is not a baseline and cannot be republished"
        )

    destination = records.baselines / f"{budget.slug}.json"
    if destination.is_file():
        previous = read_sample(destination, budget, reference, baseline=True)
        report.append(
            f"before: median {milliseconds(previous.median_ns)}  p99 "
            f"{milliseconds(previous.p99_ns)}  mad {milliseconds(previous.mad_ns)}"
        )
    report.append(
        f"after:  median {milliseconds(run.median_ns)}  p99 "
        f"{milliseconds(run.p99_ns)}  mad {milliseconds(run.mad_ns)}"
    )

    write_record(destination, baseline_document(run, reference, reason))
    report.append(f"wrote {display(destination)}")
    report.append(
        f"commit it as: perf({budget.slug.split('-')[0]}): <what got slower or "
        "faster>  [<microstep>], with the before/after above in the body"
    )


# ── command line ──────────────────────────────────────────────────────────


@dataclass(frozen=True)
class Options:
    budget: str | None
    fixture_root: Path | None
    check_profile: bool
    publish_baseline: str | None
    reason: str


USAGE = (
    "usage: bench-gate.py [--budget=SLUG] [--fixture-root=DIR] "
    "[--check-profile] [--publish-baseline=SLUG --reason=WHY]"
)


def parse_arguments(argv: Sequence[str]) -> Options:
    """Parse by hand, so a caller's argument can never become another flag.

    `just bench-gate <budget>` exports its parameter and passes it as one
    quoted argv element. Taking the value of `--budget` verbatim — even when it
    starts with a dash — means a hostile argument becomes an unknown slug and a
    refusal, never `--publish-baseline`.
    """
    values: dict[str, str] = {}
    flags: set[str] = set()
    positional: list[str] = []
    expecting: str | None = None

    for argument in argv:
        if expecting is not None:
            values[expecting] = argument
            expecting = None
            continue
        if argument in ("--help", "-h"):
            flags.add("help")
            continue
        if argument == "--":
            continue
        if argument == "--check-profile":
            flags.add("check-profile")
            continue
        name, separator, value = argument.partition("=")
        if name in ("--budget", "--fixture-root", "--publish-baseline", "--reason"):
            if separator:
                values[name] = value
            else:
                expecting = name
            continue
        if argument.startswith("-"):
            raise Unreadable(f"unknown option {argument!r}\n{USAGE}")
        positional.append(argument)

    if expecting is not None:
        raise Unreadable(f"{expecting} needs a value\n{USAGE}")
    if "help" in flags:
        raise Unreadable(USAGE)
    if len(positional) > 1:
        raise Unreadable(f"at most one budget may be named\n{USAGE}")
    if positional and "--budget" in values:
        raise Unreadable(f"name the budget once, not twice\n{USAGE}")

    budget = values.get("--budget") or (positional[0] if positional else None)
    if budget is not None and not budget.strip():
        budget = None

    fixture = values.get("--fixture-root")
    return Options(
        budget=budget,
        fixture_root=Path(fixture) if fixture else None,
        check_profile="check-profile" in flags,
        publish_baseline=values.get("--publish-baseline"),
        reason=values.get("--reason", ""),
    )


def run(
    options: Options, environment: Mapping[str, str], report: list[str]
) -> int:
    """Every exit path, appending to `report` rather than printing.

    A refusal raises, and the caller still prints everything appended so far.
    """
    records = (
        fixture_records(options.fixture_root)
        if options.fixture_root is not None
        else live_records()
    )
    kind = runner_kind(environment)

    if options.publish_baseline is not None:
        publish(records, options.publish_baseline, options.reason, kind, report)
        return 0

    # The hosted refusal comes before any record is read, so it cannot be
    # shadowed by a different refusal and cannot be argued with.
    if records.live and kind == HOSTED:
        raise Refused(
            "a hosted runner may exercise this gate's fixed pass/fail fixtures "
            "with --fixture-root, and may never produce or bless a performance "
            "baseline (conventions §7.1). The live job is pinned to "
            "runs-on: [self-hosted, reference-register] and belongs to 1.12.3"
        )

    # Before the records, because a mistyped slug is the caller's mistake and
    # deserves its own message.
    named = resolve(options.budget)

    if options.check_profile:
        report.extend(profile_report(records))
        check_reference(records, report)
        report.append("both records are filled and agree; a baseline may name them")
        return 0

    reference = check_reference(records, report)
    budgets = selected(records, named)
    if not budgets:
        raise Refused(
            "no budget is implemented at this gate, so nothing was measured "
            "and this command cannot report success over an empty set. "
            "Conventions §7 owns the five budgets; 1.2.7, 1.4.9, 1.6.2 and "
            "1.11.13 add the Phase-1 benchmarks and their first baselines"
        )

    failed = [
        budget.slug
        for budget in budgets
        if not judge(budget, records, reference, report)
    ]
    if failed:
        report.append(f"FAILED: {', '.join(failed)}")
        return 1
    report.append(f"{len(budgets)} budget(s) within limits and baselines")
    return 0


HEADLINE = {2: "ERROR", 3: "REFUSED"}


def main(argv: Sequence[str], environment: Mapping[str, str]) -> int:
    # The report is built here and printed whatever happens, so a refusal
    # arrives with the evidence that produced it rather than instead of it.
    if "--help" in argv or "-h" in argv:
        print(USAGE)
        return 0

    report: list[str] = []
    label = "bench-gate"
    fixture = False
    code = 0
    failure = ""
    try:
        options = parse_arguments(argv)
        fixture = options.fixture_root is not None
        label = "bench-gate [FIXTURE]" if fixture else "bench-gate"
        code = run(options, environment, report)
        if code:
            failure = "FAILED — a budget is outside conventions §7"
    except GateError as error:
        code = error.code
        failure = f"{HEADLINE[error.code]} — {error}"

    for line in report:
        print(f"{label}: {line}" if line else "")
    if fixture and code == 0:
        print(f"{label}: a fixture proves these thresholds, never this machine")
    if failure:
        # Flush first: a refusal that a pipe reorders above its own evidence
        # reads like a refusal with no evidence.
        sys.stdout.flush()
        print(f"{label}: {failure}", file=sys.stderr)
    return code


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:], os.environ))
