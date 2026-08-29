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

What it does NOT prove: that a CI step runs the script with the same arguments,
on the same files, or at all (a step can be `if:`-gated). It proves the path is
referenced. That is the drift that actually happened twice.

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
# it. Anchored on the leading `./` the justfile uses everywhere.
SCRIPT_CALL = re.compile(r"\./((?:scripts|\.claude|\.codex|\.githooks)/[\w./-]+)")

# Deliberate omissions. Each needs a reason, and the reason is the review: an
# entry here is a decision that CI cannot or should not run something the local
# gate does.
ALLOWED_ABSENCES: dict[str, str] = {
    # `bench-gate.py`'s live path compares a measurement against a committed
    # baseline for a specific machine. A hosted runner is refused by the gate
    # itself (conventions §7.1), and no reference register exists yet. Its
    # refusal suite, `scripts/tests/bench_gate_test.py`, IS in CI.
    "scripts/bench-gate.py": (
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


def gate_scripts(text: str) -> set[str]:
    bodies = recipe_bodies(text)
    missing = [name for name in GATE_RECIPES if name not in bodies]
    if missing:
        raise ParityError(f"justfile has no recipe named: {', '.join(missing)}")
    found: set[str] = set()
    for name in GATE_RECIPES:
        for line in bodies[name]:
            found.update(SCRIPT_CALL.findall(line))
    if not found:
        raise ParityError("no repository scripts found in the gate recipes")
    return found


def workflow_scripts(sources: dict[str, str]) -> set[str]:
    found: set[str] = set()
    for text in sources.values():
        found.update(SCRIPT_CALL.findall(text))
    return found


def audit(just_text: str, workflow_sources: dict[str, str]) -> list[str]:
    local = gate_scripts(just_text)
    covered = workflow_scripts(workflow_sources)
    problems = []
    for script in sorted(local - covered):
        if script in ALLOWED_ABSENCES:
            continue
        problems.append(f"{script} runs in the local gate and in no workflow")
    for script, reason in sorted(ALLOWED_ABSENCES.items()):
        if script in covered:
            problems.append(
                f"{script} is allowed to be absent from CI ({reason}) but a "
                "workflow now references it; remove the allowance"
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
            "secrets:",
            "    bash ./scripts/four.sh --history",
        ]
    )
    covered = {
        "ci.yml": (
            "run: ./scripts/one.py\nrun: ./scripts/two.sh\n"
            "run: ./scripts/three.rb --self-test\nrun: ./scripts/four.sh --history"
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
            "an interpreter in front of the path does not hide it",
            gate_scripts(just_ok)
            == {"scripts/one.py", "scripts/two.sh", "scripts/three.rb", "scripts/four.sh"},
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
            "\n`just pre-push` is advertised as the complete local gate. A command in it\n"
            "with no CI step is only as strong as the person who ran it. Add the step to\n"
            "`.github/workflows/`, or record a reviewed reason in ALLOWED_ABSENCES.",
            file=sys.stderr,
        )
        return 1
    print(
        f"every local gate command has a CI step "
        f"({len(gate_scripts(just_text))} script(s), {len(ALLOWED_ABSENCES)} reviewed absence)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
