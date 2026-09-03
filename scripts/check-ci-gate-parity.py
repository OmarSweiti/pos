#!/usr/bin/env python3
"""Every command in the local gate must also run in CI.

`just pre-push` is advertised as the complete local gate, and CI is what makes a
violation reviewable by someone other than the person who ran it. When a script
is in one and not the other, the repository gets the worst of both: contributors
believe CI covers it, and CI believes a human ran it.

That is not hypothetical here. `check-test-catalog.py` sat in `just lint` with no
`ci.yml` step for long enough that `CLAUDE.md` documented the gap as a known
weakness — and it named ONE checker when there were two, because
`scripts/tests/bench_gate_test.py` had drifted the same way unnoticed.

The structural cause is that `ci.yml` does not call `just guards`. It
hand-enumerates the steps, so a line added to a `justfile` recipe gets no CI step
unless someone remembers to add one. Rewriting CI to shell out to `just` would
trade that for a worse problem — one opaque step whose failure names no check —
so the enumeration stays and this compares the two lists instead.

What it does NOT prove: that a CI step uses the same operands after its operation
mode, reads the same files, or runs at all (a step can be `if:`-gated). It proves
that the path and its first explicit long-option mode are referenced. One narrow,
directional exception records where CI deliberately runs a stronger operation.

Usage:  ./scripts/check-ci-gate-parity.py [--self-test]
Exit:   0 clean · 1 a violation · 2 could not run at all
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JUSTFILE = ROOT / "justfile"
WORKFLOWS = ROOT / ".github" / "workflows"

# The recipes `pre-push` composes. Reading `pre-push` itself would follow only
# one level; these are its constituents, named so a reader can check the list
# against `justfile` by eye.
GATE_RECIPES = ("lint", "test", "build-web", "guards", "secrets")

# A repository script invoked from a recipe, whatever the interpreter in front of
# it. The first long option is the operation mode; later operands remain data and
# do not let a comment or argument donate coverage to another mode.
DEFAULT_MODE = "<live>"
SELF_TEST_MODE = "--self-test"
SCRIPT_CALL = re.compile(
    r"\./(?P<path>(?:scripts|\.agents|\.claude|\.codex|\.githooks)/[\w./-]+)"
    r"(?:[ \t]+(?P<mode>--[\w-]+)(?=[ \t]|$))?"
)
Invocation = tuple[str, str]

# CI's normal verifier runs the mapping audit before attempting the database
# engine pass. The reverse is not true, so this edge is deliberately directional.
CI_MODE_COVERAGE: dict[Invocation, frozenset[Invocation]] = {
    ("scripts/verify-pg-migrations.py", "--mapping-only"): frozenset(
        {("scripts/verify-pg-migrations.py", "--verbose")}
    ),
}

# Deliberate omissions. Each needs a reason, and the reason is the review: an
# entry here is a decision that CI cannot or should not run something the local
# gate does.
ALLOWED_ABSENCES: dict[Invocation, str] = {
    # `bench-gate.py`'s live path compares a measurement against a committed
    # baseline for a specific machine. A hosted runner is refused by the gate
    # itself (conventions §7.1), and no reference register exists yet. Its
    # refusal suite, `scripts/tests/bench_gate_test.py`, IS in CI.
    ("scripts/bench-gate.py", DEFAULT_MODE): (
        "the live comparison is machine-bound and refuses hosted runners; "
        "its refusal suite runs in CI instead"
    ),
}


class ParityError(ValueError):
    """The local gate and CI do not agree."""


def recipe_bodies(text: str) -> dict[str, list[str]]:
    """Split a justfile into recipe name -> its indented body lines."""
    bodies: dict[str, list[str]] = {}
    current: str | None = None
    for line in text.splitlines():
        header = re.match(r"^([a-z][\w-]*)(?:\s+\$?[\w=' ]*)?:", line)
        if header and not line.startswith((" ", "\t")):
            current = header.group(1)
            bodies.setdefault(current, [])
            continue
        if current is not None:
            if line.strip() == "":
                continue
            if line.startswith((" ", "\t")):
                bodies[current].append(line)
            else:
                current = None
    return bodies


def script_invocations(text: str) -> set[Invocation]:
    found: set[Invocation] = set()
    for line in text.splitlines():
        if line.lstrip().startswith("#"):
            continue
        for match in SCRIPT_CALL.finditer(line):
            found.add((match.group("path"), match.group("mode") or DEFAULT_MODE))
    return found


def gate_invocations(text: str) -> set[Invocation]:
    bodies = recipe_bodies(text)
    missing = [name for name in GATE_RECIPES if name not in bodies]
    if missing:
        raise ParityError(f"justfile has no recipe named: {', '.join(missing)}")
    found: set[Invocation] = set()
    for name in GATE_RECIPES:
        found.update(script_invocations("\n".join(bodies[name])))
    if not found:
        raise ParityError("no repository scripts found in the gate recipes")
    return found


def guard_invocations(text: str) -> set[Invocation]:
    bodies = recipe_bodies(text)
    if "guards" not in bodies:
        raise ParityError("justfile has no recipe named: guards")
    return script_invocations("\n".join(bodies["guards"]))


def workflow_invocations(sources: dict[str, str]) -> set[Invocation]:
    found: set[Invocation] = set()
    for text in sources.values():
        found.update(script_invocations(text))
    return found


def invocation_label(invocation: Invocation) -> str:
    path, mode = invocation
    return path if mode == DEFAULT_MODE else f"{path} {mode}"


def covered_by_workflow(local: Invocation, covered: set[Invocation]) -> bool:
    if local in covered:
        return True
    return bool(CI_MODE_COVERAGE.get(local, frozenset()) & covered)


def audit(just_text: str, workflow_sources: dict[str, str]) -> list[str]:
    local = gate_invocations(just_text)
    local_guards = guard_invocations(just_text)
    covered = workflow_invocations(workflow_sources)
    problems = []
    for invocation in sorted(local):
        if covered_by_workflow(invocation, covered):
            continue
        if invocation in ALLOWED_ABSENCES:
            continue
        problems.append(
            f"{invocation_label(invocation)} runs in the local gate and in no "
            "workflow with that mode"
        )
    for invocation in sorted(covered - local_guards):
        if invocation[1] == SELF_TEST_MODE:
            problems.append(
                f"{invocation_label(invocation)} runs in a workflow but not in "
                "`just guards`"
            )
    for invocation, reason in sorted(ALLOWED_ABSENCES.items()):
        if covered_by_workflow(invocation, covered):
            problems.append(
                f"{invocation_label(invocation)} is allowed to be absent from CI "
                f"({reason}) but a workflow now covers it; remove the allowance"
            )
    return problems


def self_test() -> int:
    just_ok = "\n".join(
        [
            "lint:",
            "    python3 ./scripts/one.py",
            "test:",
            "    cargo nextest run",
            "build-web:",
            "    pnpm -r build",
            "guards:",
            "    bash ./scripts/two.sh",
            "    ruby ./scripts/three.rb --self-test",
            "    python3 ./scripts/parity.py",
            "    python3 ./scripts/parity.py --self-test",
            "    python3 ./.agents/test-skills.py",
            "secrets:",
            "    bash ./scripts/four.sh --history",
        ]
    )
    covered = {
        "ci.yml": (
            "run: ./scripts/one.py\nrun: ./scripts/two.sh\n"
            "run: ./scripts/three.rb --self-test\n"
            "run: ./scripts/parity.py\n"
            "run: ./scripts/parity.py --self-test\n"
            "run: ./.agents/test-skills.py\n"
            "run: ./scripts/four.sh --history"
        )
    }

    cases: list[tuple[str, bool]] = []

    cases.append(("full coverage passes", audit(just_ok, covered) == []))

    partial = {"ci.yml": "run: ./scripts/one.py\nrun: ./scripts/two.sh"}
    problems = audit(just_ok, partial)
    cases.append(
        (
            "a gate script with no workflow step is refused",
            any("three.rb" in p for p in problems)
            and any("four.sh" in p for p in problems),
        )
    )

    # The drift this checker exists for: a line appended to `guards` and to no
    # workflow. It is exactly how both real omissions happened.
    drifted = just_ok.replace(
        "    ruby ./scripts/three.rb --self-test",
        "    ruby ./scripts/three.rb --self-test\n    python3 ./scripts/tests/new.py",
    )
    cases.append(
        (
            "a newly added guard line with no CI step is refused",
            any("scripts/tests/new.py" in p for p in audit(drifted, covered)),
        )
    )

    cases.append(
        (
            "interpreters and the .agents root do not hide a script",
            gate_invocations(just_ok)
            == {
                (".agents/test-skills.py", DEFAULT_MODE),
                ("scripts/four.sh", "--history"),
                ("scripts/one.py", DEFAULT_MODE),
                ("scripts/parity.py", DEFAULT_MODE),
                ("scripts/parity.py", SELF_TEST_MODE),
                ("scripts/three.rb", SELF_TEST_MODE),
                ("scripts/two.sh", DEFAULT_MODE),
            },
        )
    )

    without_ci_self_test = {
        "ci.yml": covered["ci.yml"].replace(
            "run: ./scripts/parity.py --self-test\n", ""
        )
    }
    problems = audit(just_ok, without_ci_self_test)
    cases.append(
        (
            "a live CI reference does not cover the same script's self-test",
            any("scripts/parity.py --self-test" in p for p in problems),
        )
    )

    without_local_self_test = just_ok.replace(
        "    python3 ./scripts/parity.py --self-test\n", ""
    )
    problems = audit(without_local_self_test, covered)
    cases.append(
        (
            "a local self-test removed from just guards is refused",
            any(
                "scripts/parity.py --self-test runs in a workflow but not in "
                "`just guards`" in p
                for p in problems
            ),
        )
    )

    postgres_local = just_ok.replace(
        "    python3 ./scripts/one.py",
        "    python3 ./scripts/one.py\n"
        "    python3 ./scripts/verify-pg-migrations.py --mapping-only",
    )
    postgres_ci = {
        "ci.yml": covered["ci.yml"]
        + "\nrun: ./scripts/verify-pg-migrations.py --verbose"
    }
    cases.append(
        (
            "the stronger Postgres CI mode covers mapping-only",
            audit(postgres_local, postgres_ci) == [],
        )
    )

    weaker_postgres_local = just_ok.replace(
        "    python3 ./scripts/one.py",
        "    python3 ./scripts/one.py\n"
        "    python3 ./scripts/verify-pg-migrations.py --verbose",
    )
    weaker_postgres_ci = {
        "ci.yml": covered["ci.yml"]
        + "\nrun: ./scripts/verify-pg-migrations.py --mapping-only"
    }
    cases.append(
        (
            "the weaker Postgres mode does not cover verbose",
            any(
                "verify-pg-migrations.py --verbose" in p
                for p in audit(weaker_postgres_local, weaker_postgres_ci)
            ),
        )
    )

    distinct_mode_local = just_ok.replace(
        "    python3 ./scripts/one.py",
        "    python3 ./scripts/one.py\n"
        "    bash ./scripts/gh-actions-policy.sh --check",
    )
    distinct_mode_ci = {
        "ci.yml": covered["ci.yml"]
        + "\nrun: ./scripts/gh-actions-policy.sh --dry-run"
    }
    cases.append(
        (
            "an unrelated mode does not donate coverage",
            any(
                "gh-actions-policy.sh --check" in p
                for p in audit(distinct_mode_local, distinct_mode_ci)
            ),
        )
    )

    parsed_modes = script_invocations(
        "# run: ./scripts/pure-comment.py --self-test\n"
        "run: ./scripts/commented.py # --self-test\n"
        "run: ./scripts/not-quite.py --self-testing\n"
    )
    cases.append(
        (
            "comments and similarly named flags cannot donate self-test coverage",
            parsed_modes
            == {
                ("scripts/commented.py", DEFAULT_MODE),
                ("scripts/not-quite.py", "--self-testing"),
            },
        )
    )

    missing_recipe = "lint:\n    python3 ./scripts/one.py"
    try:
        audit(missing_recipe, covered)
    except ParityError:
        cases.append(("a missing gate recipe is fatal, not silently empty", True))
    else:
        cases.append(("a missing gate recipe is fatal, not silently empty", False))

    # A stale allowance is as wrong as a missing step: it says a decision was
    # made that no longer applies.
    stale = audit(just_ok, {"ci.yml": covered["ci.yml"] + "\nrun: ./scripts/bench-gate.py"})
    cases.append(
        (
            "an allowance that CI now covers is refused as stale",
            any("remove the allowance" in p for p in stale),
        )
    )

    allowed_mode_drift = just_ok.replace(
        "    python3 ./scripts/parity.py",
        "    python3 ./scripts/bench-gate.py --self-test\n"
        "    python3 ./scripts/parity.py",
        1,
    )
    cases.append(
        (
            "a live-mode allowance does not hide an uncovered self-test",
            any(
                "scripts/bench-gate.py --self-test" in p
                for p in audit(allowed_mode_drift, covered)
            ),
        )
    )

    failures = 0
    for label, ok in cases:
        print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
        failures += not ok
    if failures:
        print(f"\ncheck-ci-gate-parity self-test: {failures}/{len(cases)} case(s) FAILED")
        return 1
    print(f"\ncheck-ci-gate-parity self-test: {len(cases)} cases passed")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if sys.argv[1:]:
        print("usage: check-ci-gate-parity.py [--self-test]", file=sys.stderr)
        return 2
    try:
        just_text = JUSTFILE.read_text(encoding="utf-8")
        sources = {
            path.name: path.read_text(encoding="utf-8")
            for path in sorted(WORKFLOWS.glob("*.yml"))
        }
        if not sources:
            raise ParityError("no workflow definitions found")
        problems = audit(just_text, sources)
    except (OSError, ParityError) as exc:
        print(f"check-ci-gate-parity: REFUSED — {exc}", file=sys.stderr)
        return 1
    if problems:
        print("check-ci-gate-parity: REFUSED", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            "\n`just pre-push` is advertised as the complete local gate. Align each script\n"
            "mode between it and CI: add a missing workflow step, restore a workflow\n"
            "self-test to `just guards`, or record a reviewed live-mode absence.",
            file=sys.stderr,
        )
        return 1
    print(
        "every local gate command mode has a CI step "
        f"({len({path for path, _mode in gate_invocations(just_text)})} script(s), "
        f"{len(gate_invocations(just_text))} mode(s), "
        f"{len(ALLOWED_ABSENCES)} reviewed absence)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
