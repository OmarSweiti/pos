#!/usr/bin/env python3
"""Reconcile the published microstep counts against the phase files that own them.

Four surfaces state how much of the plan is built, and every one of them is typed
by hand:

  * `docs/implementation/README.md`      the frontier paragraph — N of M, and the list of N
  * `docs/implementation/00-master-plan.md`  the phase table — M per phase
  * `docs/implementation/status-page.html`   the phase cards — M per phase
  * the phase files themselves            the headings that define M, and the
                                          `**Full-step status:**` markers that define
                                          which delivered steps are only partly delivered

A hand-typed count is not evidence. The dangerous failure is not a wrong number
somebody notices — it is a right-looking number nobody checks, exactly as
`check-test-catalog.py` exists for. Two independent reviewers of this repository,
working from stale checkouts, each proposed publishing "17 of 112" into the
frontier on a day when the answer was 16; git would have merged either silently,
because both sides of the conflict had written the same wrong digits.

So: the denominator is DERIVED from the phase files and never typed. The numerator
is DECLARED in the frontier region, because "this step is done" is a judgement a
parser cannot make — a commit tagged `[1.6.1]` may be documentation only, and three
merged pull requests carry Phase-1 step tags for migrations that do not exist. What
the checker enforces is that the declaration is internally consistent, that every
declared id is a real executable microstep, that a partly-delivered step is never
declared complete, and that the three display surfaces agree with the derivation.

Exit codes: 0 reconciled, 1 drift, 2 the inputs could not be parsed.
"""

from __future__ import annotations

import argparse
import html
import re
import sys
import unittest
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DOCS = REPO / "docs" / "implementation"

README = DOCS / "README.md"
MASTER_PLAN = DOCS / "00-master-plan.md"
STATUS_PAGE = DOCS / "status-page.html"

# Phase files, in phase order. Phase 0 is deliberately absent: it is closed by
# transfer and `phase-0-closeout.md` is a dated historical record, not a plan whose
# steps are counted in the published totals.
PHASE_FILES = {
    1: DOCS / "phase-1-sellable-mvp.md",
    2: DOCS / "phase-2-money-grade.md",
    3: DOCS / "phase-3-connected.md",
    4: DOCS / "phase-4-depth.md",
    5: DOCS / "phase-5-harden-and-launch.md",
}

# `### 1.2.3 — Title` or `### 1.1.2a — Title`. The suffix letter is how a split
# microstep keeps a stable commit reference (1.1.2a / 1.1.2b).
HEADING = re.compile(r"^### (?P<id>(?P<phase>[0-5])\.\d+\.\d+[a-z]?) ", re.MULTILINE)

# A retained anchor that is not itself buildable. `1.1.2` is the only one today:
# its work lives in 1.1.2a and 1.1.2b, which carry the files, tests and command.
CONCORDANCE = "**Concordance only:**"

# Phase 5 states nine of its microsteps as table rows rather than headings. The
# anchor is exact on purpose: `phase-5-harden-and-launch.md` also has a
# `**prerequisite**` row that a looser pattern captures, which would yield ten and
# put the derived total one above the published 36.
PHASE5_TABLE_ROW = re.compile(r"^\| (?P<id>5\.2\.\d+) \|", re.MULTILINE)

# A step that shipped one half and deferred the other declares it at its own heading.
FULL_STEP_STATUS = re.compile(
    r"^\*\*Full-step status:\*\* (?P<id>\d+\.\d+\.\d+[a-z]?) ", re.MULTILINE
)

FRONTIER_BEGIN = re.compile(r"^<!-- frontier:begin phase=(?P<phase>[1-5]) -->$", re.MULTILINE)
FRONTIER_END = "<!-- frontier:end -->"

# "**17 of 112 executable microsteps fully complete (~15%)**"
FRONTIER_COUNT = re.compile(
    r"\*\*(?P<done>\d+) of (?P<total>\d+) executable microsteps fully complete "
    r"\(~(?P<pct>\d+)%\)\*\*"
)

# Any other place that states the same shape of claim. Outside the checked region
# such a sentence is a second copy of the number, and a second copy is what drifts.
LOOSE_COUNT = re.compile(r"\d+ of \d+ executable microsteps")

STEP_ID = re.compile(r"`(?P<id>\d+\.\d+\.\d+[a-z]?)`")

# `| 1 | 112 | **14–20**, … |` — the phase table's count cell.
MASTER_ROW = re.compile(r"^\| (?P<phase>[1-5]) \| (?P<total>\d+) \|", re.MULTILINE)

# `<span class="effort">112 steps · 14–20 weeks …</span>`
STATUS_CARD = re.compile(r'<span class="effort">(?P<total>\d+) steps')

DONE_WHEN = "**Done when:**"


class Unparsable(Exception):
    """An input could not be read at all — exit 2, not 1. Drift is a finding; an
    unreadable input is a broken checker, and the two must not look alike."""


@dataclass
class Phase:
    number: int
    executable: dict[str, int] = field(default_factory=dict)  # id -> line number
    non_executable: set[str] = field(default_factory=set)
    partial: dict[str, int] = field(default_factory=dict)  # id -> line number

    @property
    def total(self) -> int:
        return len(self.executable)


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:  # pragma: no cover - exercised by the missing-file test
        raise Unparsable(f"cannot read {path.relative_to(REPO)}: {exc}") from exc


def _line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def derive_phase(number: int, path: Path) -> Phase:
    """Count what the phase file actually contains.

    A heading is non-executable when its section opens with the concordance
    marker. `section` runs to the next `### ` heading so the marker cannot be
    picked up from a neighbour.
    """
    text = _read(path)
    phase = Phase(number=number)

    matches = list(HEADING.finditer(text))
    if not matches:
        raise Unparsable(f"{path.relative_to(REPO)}: no `### N.N.N ` microstep headings found")

    for index, match in enumerate(matches):
        step_id = match.group("id")
        if int(match.group("phase")) != number:
            raise Unparsable(
                f"{path.relative_to(REPO)}:{_line_of(text, match.start())}: "
                f"heading `{step_id}` does not belong to phase {number}"
            )
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        section = text[match.start() : end]

        if CONCORDANCE in section:
            phase.non_executable.add(step_id)
            continue

        if step_id in phase.executable:
            raise Unparsable(
                f"{path.relative_to(REPO)}:{_line_of(text, match.start())}: "
                f"duplicate microstep heading `{step_id}`"
            )
        phase.executable[step_id] = _line_of(text, match.start())

    if number == 5:
        for row in PHASE5_TABLE_ROW.finditer(text):
            step_id = row.group("id")
            if step_id in phase.executable:
                raise Unparsable(
                    f"{path.relative_to(REPO)}:{_line_of(text, row.start())}: "
                    f"`{step_id}` is both a heading and a table row"
                )
            phase.executable[step_id] = _line_of(text, row.start())

    for marker in FULL_STEP_STATUS.finditer(text):
        phase.partial[marker.group("id")] = _line_of(text, marker.start())

    return phase


def derive_all() -> dict[int, Phase]:
    return {number: derive_phase(number, path) for number, path in PHASE_FILES.items()}


@dataclass
class Region:
    phase: int
    declared: list[str]
    done: int
    total: int
    pct: int
    line: int


def parse_regions(text: str) -> list[Region]:
    regions: list[Region] = []
    for begin in FRONTIER_BEGIN.finditer(text):
        phase = int(begin.group("phase"))
        end = text.find(FRONTIER_END, begin.end())
        if end == -1:
            raise Unparsable(
                f"README.md:{_line_of(text, begin.start())}: "
                f"`frontier:begin phase={phase}` has no matching `{FRONTIER_END}`"
            )
        body = text[begin.end() : end]
        if FRONTIER_BEGIN.search(body):
            raise Unparsable(
                f"README.md:{_line_of(text, begin.start())}: frontier regions cannot nest"
            )

        count = FRONTIER_COUNT.search(body)
        if count is None:
            raise Unparsable(
                f"README.md:{_line_of(text, begin.start())}: the frontier region states no "
                f'"**N of M executable microsteps fully complete (~P%)**"'
            )

        regions.append(
            Region(
                phase=phase,
                declared=[m.group("id") for m in STEP_ID.finditer(body)],
                done=int(count.group("done")),
                total=int(count.group("total")),
                pct=int(count.group("pct")),
                line=_line_of(text, begin.start()),
            )
        )

    stray = text.count(FRONTIER_END)
    if stray != len(regions):
        raise Unparsable(
            f"README.md: {stray} `{FRONTIER_END}` marker(s) for {len(regions)} region(s)"
        )
    if not regions:
        raise Unparsable("README.md: no `<!-- frontier:begin phase=N -->` region found")
    return regions


def check(phases: dict[int, Phase]) -> list[str]:
    """Every rule. Returns the list of failures; empty means reconciled."""
    problems: list[str] = []
    readme = _read(README)
    regions = parse_regions(readme)

    seen_phases: set[int] = set()
    for region in regions:
        if region.phase in seen_phases:
            problems.append(f"README.md:{region.line}: phase {region.phase} declared twice")
            continue
        seen_phases.add(region.phase)

        phase = phases.get(region.phase)
        if phase is None:
            problems.append(f"README.md:{region.line}: unknown phase {region.phase}")
            continue

        phase_file = PHASE_FILES[region.phase].relative_to(REPO)

        # 1 — every declared id is exactly one executable heading in its phase file.
        for step_id in region.declared:
            if step_id in phase.non_executable:
                problems.append(
                    f"README.md:{region.line}: `{step_id}` is declared complete but "
                    f"{phase_file} marks it non-executable"
                )
            elif step_id not in phase.executable:
                problems.append(
                    f"README.md:{region.line}: `{step_id}` is declared complete but is not a "
                    f"microstep in {phase_file}"
                )

        # 2 — no id repeats. A duplicate inflates the numerator invisibly.
        for step_id in sorted({s for s in region.declared if region.declared.count(s) > 1}):
            problems.append(
                f"README.md:{region.line}: `{step_id}` is declared complete more than once"
            )

        unique = sorted(set(region.declared))

        # 3 — a partly-delivered step is never complete. This is the rule that would
        #     have caught both reviewers: 1.6.3 shipped its domain half in PR #78 and
        #     is still not complete.
        for step_id in unique:
            if step_id in phase.partial:
                problems.append(
                    f"README.md:{region.line}: `{step_id}` is declared complete but "
                    f"{phase_file}:{phase.partial[step_id]} says it is not complete until its "
                    f"deferred half lands"
                )

        # 4 — a step with no `Done when` line has nothing to have satisfied.
        for step_id in unique:
            line = phase.executable.get(step_id)
            if line is None:
                continue
            if not _has_done_when(PHASE_FILES[region.phase], step_id):
                problems.append(
                    f"README.md:{region.line}: `{step_id}` is declared complete but "
                    f"{phase_file}:{line} carries no `Done when` line to have satisfied"
                )

        # 5, 6, 7 — the arithmetic.
        if region.done != len(unique):
            problems.append(
                f"README.md:{region.line}: states {region.done} complete but lists "
                f"{len(unique)} distinct microstep id(s)"
            )
        if region.total != phase.total:
            problems.append(
                f"README.md:{region.line}: states a denominator of {region.total} but "
                f"{phase_file} contains {phase.total} executable microsteps"
            )
        if phase.total:
            expected = round(100 * len(unique) / phase.total)
            if region.pct != expected:
                problems.append(
                    f"README.md:{region.line}: states ~{region.pct}% but "
                    f"{len(unique)}/{phase.total} is ~{expected}%"
                )

        # 8 — a partial step must still be visible somewhere in the README, or a
        #     half-delivered microstep silently disappears from the account.
        for step_id, marker_line in sorted(phase.partial.items()):
            if f"`{step_id}`" not in readme:
                problems.append(
                    f"{phase_file}:{marker_line}: `{step_id}` is partly delivered but the "
                    f"README never mentions it"
                )

    # 9 — no second copy of the claim anywhere else in the tracked documentation.
    problems.extend(_check_no_loose_counts(readme, regions))

    # 10, 11 — the other two display surfaces agree with the derivation.
    problems.extend(_check_master_plan(phases))
    problems.extend(_check_status_page(phases))

    return problems


def _has_done_when(path: Path, step_id: str) -> bool:
    text = _read(path)
    matches = list(HEADING.finditer(text))
    for index, match in enumerate(matches):
        if match.group("id") != step_id:
            continue
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        return DONE_WHEN in text[match.start() : end]
    # A Phase-5 table row has no section of its own; the table states the step and
    # the group's prose carries the gate. Nothing to check.
    return True


def _check_no_loose_counts(readme: str, regions: list[Region]) -> list[str]:
    problems: list[str] = []

    spans: list[tuple[int, int]] = []
    for begin in FRONTIER_BEGIN.finditer(readme):
        end = readme.find(FRONTIER_END, begin.end())
        if end != -1:
            spans.append((begin.start(), end + len(FRONTIER_END)))

    for match in LOOSE_COUNT.finditer(readme):
        if not any(start <= match.start() < stop for start, stop in spans):
            problems.append(
                f"README.md:{_line_of(readme, match.start())}: "
                f'"{match.group(0)}" states a completion count outside the checked region'
            )

    for path in sorted(DOCS.rglob("*.md")):
        if path == README:
            continue
        text = _read(path)
        for match in LOOSE_COUNT.finditer(text):
            problems.append(
                f"{path.relative_to(REPO)}:{_line_of(text, match.start())}: "
                f'"{match.group(0)}" states a completion count; the checked region in '
                f"README.md is the only place that may"
            )
    return problems


def _check_master_plan(phases: dict[int, Phase]) -> list[str]:
    text = _read(MASTER_PLAN)
    rows = {int(m.group("phase")): m for m in MASTER_ROW.finditer(text)}
    problems: list[str] = []
    for number, phase in sorted(phases.items()):
        row = rows.get(number)
        if row is None:
            problems.append(
                f"00-master-plan.md: the phase table has no row for phase {number}"
            )
            continue
        stated = int(row.group("total"))
        if stated != phase.total:
            problems.append(
                f"00-master-plan.md:{_line_of(text, row.start())}: phase {number} states "
                f"{stated} microsteps but "
                f"{PHASE_FILES[number].relative_to(REPO)} contains {phase.total}"
            )
    return problems


def _check_status_page(phases: dict[int, Phase]) -> list[str]:
    text = html.unescape(_read(STATUS_PAGE))
    cards = [int(m.group("total")) for m in STATUS_CARD.finditer(text)]
    expected = [phases[n].total for n in sorted(phases)]
    if len(cards) != len(expected):
        return [
            f"status-page.html: found {len(cards)} phase card(s) stating a step count, "
            f"expected {len(expected)}"
        ]
    problems: list[str] = []
    for number, (stated, derived) in enumerate(zip(cards, expected, strict=True), start=1):
        if stated != derived:
            problems.append(
                f"status-page.html: the phase {number} card states {stated} steps but "
                f"{PHASE_FILES[number].relative_to(REPO)} contains {derived}"
            )
    return problems


def run() -> int:
    try:
        phases = derive_all()
        problems = check(phases)
    except Unparsable as exc:
        print(f"frontier: CANNOT CHECK — {exc}", file=sys.stderr)
        return 2

    if problems:
        print("frontier: REFUSED — the published counts do not reconcile.", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            "\n  The phase files are the record. Correct the frontier region in "
            "docs/implementation/README.md\n  in the same commit that noticed.",
            file=sys.stderr,
        )
        return 1

    summary = ", ".join(f"phase {n}: {p.total}" for n, p in sorted(phases.items()))
    print(f"implementation frontier reconciles with the phase files ({summary})")
    return 0


# ── self-test ─────────────────────────────────────────────────────────────────
# Every rule above must be shown to fire. A guard nobody has seen fail is a guard
# nobody should trust.

PHASE_FIXTURE = """# Phase 1

### 1.1.0 — First
**Done when:** it runs.

### 1.1.1 — Second
**Done when:** it runs.

### 1.1.2 — Retained anchor
**Concordance only:** this anchor is not an executable microstep.

### 1.1.3 — Third
**Full-step status:** 1.1.3 is not complete until its deferred half lands.
**Done when:** it runs.

### 1.1.4 — Fourth, with no gate
**Files:** somewhere.
"""

README_FIXTURE = """# Implementation documentation

<!-- frontier:begin phase=1 -->
Phase 1 has **2 of 4 executable microsteps fully complete (~50%)**: `1.1.0` (first) and
`1.1.1` (second).
<!-- frontier:end -->

`1.1.3` is partly delivered.
"""


class _Harness(unittest.TestCase):
    """Each test rewrites one fixture and asserts the specific failure fires."""

    def setUp(self) -> None:
        import tempfile

        self._tmp = tempfile.TemporaryDirectory()
        root = Path(self._tmp.name)
        (root / "docs" / "implementation").mkdir(parents=True)
        self.docs = root / "docs" / "implementation"

        global REPO, DOCS, README, MASTER_PLAN, STATUS_PAGE, PHASE_FILES
        self._saved = (REPO, DOCS, README, MASTER_PLAN, STATUS_PAGE, PHASE_FILES)
        REPO, DOCS = root, self.docs
        README = self.docs / "README.md"
        MASTER_PLAN = self.docs / "00-master-plan.md"
        STATUS_PAGE = self.docs / "status-page.html"
        PHASE_FILES = {1: self.docs / "phase-1.md"}

        self.write_phase(PHASE_FIXTURE)
        self.write_readme(README_FIXTURE)
        MASTER_PLAN.write_text(
            "| Phase | Microsteps |\n|---|---:|\n| 1 | 4 | x |\n", encoding="utf-8"
        )
        STATUS_PAGE.write_text('<span class="effort">4 steps</span>\n', encoding="utf-8")

    def tearDown(self) -> None:
        global REPO, DOCS, README, MASTER_PLAN, STATUS_PAGE, PHASE_FILES
        REPO, DOCS, README, MASTER_PLAN, STATUS_PAGE, PHASE_FILES = self._saved
        self._tmp.cleanup()

    def write_phase(self, text: str) -> None:
        (self.docs / "phase-1.md").write_text(text, encoding="utf-8")

    def write_readme(self, text: str) -> None:
        (self.docs / "README.md").write_text(text, encoding="utf-8")

    def failures(self) -> list[str]:
        return check(derive_all())

    # the fixture as written must pass, or every negative test below proves nothing
    def test_the_fixture_reconciles(self) -> None:
        self.assertEqual(self.failures(), [])

    def test_a_stale_numerator_is_refused(self) -> None:
        self.write_readme(README_FIXTURE.replace("**2 of 4", "**3 of 4"))
        self.assertTrue(any("states 3 complete but lists 2" in p for p in self.failures()))

    def test_a_stale_denominator_is_refused(self) -> None:
        self.write_readme(README_FIXTURE.replace("of 4 executable", "of 5 executable"))
        self.assertTrue(any("denominator of 5" in p for p in self.failures()))

    def test_a_stale_percentage_is_refused(self) -> None:
        self.write_readme(README_FIXTURE.replace("(~50%)", "(~75%)"))
        self.assertTrue(any("states ~75% but 2/4 is ~50%" in p for p in self.failures()))

    def test_declaring_a_partial_step_complete_is_refused(self) -> None:
        self.write_readme(
            README_FIXTURE.replace("`1.1.1` (second)", "`1.1.1` (second) and `1.1.3` (third)")
            .replace("**2 of 4", "**3 of 4")
            .replace("(~50%)", "(~75%)")
        )
        self.assertTrue(
            any("not complete until its deferred half" in p for p in self.failures())
        )

    def test_declaring_an_unknown_id_complete_is_refused(self) -> None:
        self.write_readme(README_FIXTURE.replace("`1.1.1` (second)", "`1.9.9` (invented)"))
        self.assertTrue(any("`1.9.9` is declared complete but" in p for p in self.failures()))

    def test_declaring_a_non_executable_anchor_complete_is_refused(self) -> None:
        self.write_readme(README_FIXTURE.replace("`1.1.1` (second)", "`1.1.2` (anchor)"))
        self.assertTrue(any("marks it non-executable" in p for p in self.failures()))

    def test_a_duplicate_id_is_refused(self) -> None:
        self.write_readme(
            README_FIXTURE.replace("`1.1.1` (second)", "`1.1.1` (second) and `1.1.0` (again)")
        )
        self.assertTrue(any("more than once" in p for p in self.failures()))

    def test_declaring_a_step_with_no_done_when_complete_is_refused(self) -> None:
        self.write_readme(
            README_FIXTURE.replace("`1.1.1` (second)", "`1.1.1` (second) and `1.1.4` (fourth)")
            .replace("**2 of 4", "**3 of 4")
            .replace("(~50%)", "(~75%)")
        )
        self.assertTrue(any("no `Done when` line" in p for p in self.failures()))

    def test_a_partial_step_absent_from_the_readme_is_refused(self) -> None:
        self.write_readme(README_FIXTURE.replace("`1.1.3` is partly delivered.", ""))
        self.assertTrue(any("the README never mentions it" in p for p in self.failures()))

    def test_a_count_outside_the_region_is_refused(self) -> None:
        self.write_readme(README_FIXTURE + "\nElsewhere: 9 of 112 executable microsteps.\n")
        self.assertTrue(any("outside the checked region" in p for p in self.failures()))

    def test_a_count_in_another_document_is_refused(self) -> None:
        (self.docs / "other.md").write_text(
            "Progress is 11 of 112 executable microsteps.\n", encoding="utf-8"
        )
        self.assertTrue(any("other.md" in p for p in self.failures()))

    def test_a_new_heading_that_the_master_table_ignores_is_refused(self) -> None:
        self.write_phase(PHASE_FIXTURE + "\n### 1.1.5 — Fifth\n**Done when:** it runs.\n")
        problems = self.failures()
        self.assertTrue(any("00-master-plan.md" in p and "states 4" in p for p in problems))
        self.assertTrue(any("status-page.html" in p and "states 4" in p for p in problems))

    def test_a_concordance_anchor_is_excluded_from_the_denominator(self) -> None:
        self.assertEqual(derive_all()[1].total, 4)
        self.assertIn("1.1.2", derive_all()[1].non_executable)

    def test_an_unterminated_region_cannot_be_checked(self) -> None:
        self.write_readme(README_FIXTURE.replace(FRONTIER_END, ""))
        with self.assertRaises(Unparsable):
            self.failures()

    def test_a_region_with_no_count_cannot_be_checked(self) -> None:
        self.write_readme(
            "<!-- frontier:begin phase=1 -->\nnothing stated\n<!-- frontier:end -->\n"
        )
        with self.assertRaises(Unparsable):
            self.failures()

    def test_a_phase_file_with_no_headings_cannot_be_checked(self) -> None:
        self.write_phase("# Phase 1\n\nno microsteps here\n")
        with self.assertRaises(Unparsable):
            self.failures()

    def test_a_duplicate_heading_cannot_be_checked(self) -> None:
        self.write_phase(PHASE_FIXTURE + "\n### 1.1.0 — First again\n**Done when:** x.\n")
        with self.assertRaises(Unparsable):
            self.failures()

    def test_a_foreign_phase_heading_cannot_be_checked(self) -> None:
        self.write_phase(PHASE_FIXTURE + "\n### 2.1.0 — Wrong phase\n**Done when:** x.\n")
        with self.assertRaises(Unparsable):
            self.failures()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove every refusal path fires, against fixtures rather than the repository",
    )
    args = parser.parse_args()

    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(_Harness)
        result = unittest.TextTestRunner(verbosity=1).run(suite)
        return 0 if result.wasSuccessful() else 1

    return run()


if __name__ == "__main__":
    sys.exit(main())
