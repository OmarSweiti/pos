#!/usr/bin/env python3
"""The workspace lint contract is real for every member, not just declared once.

Invariant 1 says no float ever touches money. The *only* machine that enforces
that is `float_arithmetic = "forbid"` in the root `[workspace.lints.clippy]`
table:
both `cargo clippy` invocations in this repository (justfile and ci.yml) pass
`-D warnings` and no lint flags of their own, so the entire lint scope of every
gate comes from that one table.

A table is not enforcement. Cargo applies `[workspace.lints]` to a member only
when that member's own manifest opts in:

    [lints]
    workspace = true

Two lines. Omit them in a new crate and it silently loses every deny in this
file — the float ban included — while `cargo clippy -D warnings`, `just lint`,
`just pre-push` and CI all stay green, because nothing anywhere compares the
member list against the opt-in list. All seven members opt in today. The eighth
is the one this check exists for — and the seventh already proved the point:
`pos-test-support` arrived at microstep 1.1.0, and this check is what required
its two lines rather than leaving them to whoever noticed.

The complementary failure is quieter still: an entry demoted from `deny` to
`warn` keeps the lint listed, keeps it visibly "configured", and stops it
failing a build. So the required levels are asserted exactly, not merely
present.

`float_arithmetic` is required at `forbid`, not `deny`, and the difference is
the whole of I-1's enforcement. `deny` is lifted by `#![allow(...)]` in a crate
root -- two words, no diff anywhere near a money path, every gate still green.
`forbid` cannot be lifted: rustc answers E0453 and names the attribute. It also
reaches what a source scanner cannot -- an attribute a macro generated, or code
behind `include!` -- which is why the control is the level in this table rather
than a checker that reads Rust source. This file's job is to prove the level has
not been demoted.

This is deliberately a checker rather than a byte-pin in
check-branch-workflow-policy.rb. That file's exact-content boundary is right for
policy whose every byte is reviewed, but the root manifest also carries
`[workspace.dependencies]`, which Dependabot edits on a routine cadence. Pinning
it would make ordinary dependency work red while still not catching a member
that never opted in — the failure that actually loses the invariant.

Usage:  ./scripts/check-workspace-lints.py
        ./scripts/check-workspace-lints.py --self-test   # prove the checks fire
Exit:   0 clean · 1 a violation · 2 could not run at all
"""

from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Every lint whose absence or demotion would cost a class of bug that this
# repository has written down as an invariant or a standing rule. The comment
# beside each is why it is not merely a style preference.
REQUIRED_RUST = {
    # Nothing we write needs to opt out of the borrow checker.
    "unsafe_code": "forbid",
}
REQUIRED_CLIPPY = {
    # A panic in a register is a lost sale (conventions §1).
    "unwrap_used": "deny",
    "expect_used": "deny",
    "panic": "deny",
    # Invariant 1: no float ever touches money.
    "float_arithmetic": "forbid",
    # A committed dbg! prints whatever it was handed, which in this codebase is
    # routinely a value the security rules forbid logging.
    "dbg_macro": "deny",
}


def rel(path: Path) -> str:
    """Repository-relative display path, so failures are clickable."""
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def lint_level(value: object) -> str | None:
    """Read a lint's level from either TOML spelling.

    Cargo accepts a bare string (`unwrap_used = "deny"`) or a table carrying a
    priority (`unwrap_used = { level = "deny", priority = -1 }`). A checker that
    understood only the first would report a false violation the moment someone
    legitimately needed to order a lint group.
    """
    if isinstance(value, str):
        return value
    if isinstance(value, dict):
        level = value.get("level")
        return level if isinstance(level, str) else None
    return None


def check_workspace_table(root_manifest: dict) -> list[str]:
    """Every required lint is present in the root table, at its required level."""
    problems: list[str] = []
    lints = root_manifest.get("workspace", {}).get("lints", {})
    if not isinstance(lints, dict):
        return ["Cargo.toml has no [workspace.lints] table"]

    for tool, required in (("rust", REQUIRED_RUST), ("clippy", REQUIRED_CLIPPY)):
        table = lints.get(tool)
        if not isinstance(table, dict):
            problems.append(f"Cargo.toml has no [workspace.lints.{tool}] table")
            continue
        for lint, level in sorted(required.items()):
            if lint not in table:
                problems.append(
                    f"Cargo.toml [workspace.lints.{tool}] is missing "
                    f"{lint} = {level!r}"
                )
                continue
            found = lint_level(table[lint])
            if found != level:
                problems.append(
                    f"Cargo.toml [workspace.lints.{tool}] sets {lint} to "
                    f"{found!r}; this repository requires {level!r}"
                )
    return problems


def check_member_opt_in(name: str, manifest_path: str, manifest: dict) -> list[str]:
    """A member inherits the workspace lints only if it says so."""
    lints = manifest.get("lints")
    if not isinstance(lints, dict) or lints.get("workspace") is not True:
        return [
            f"{manifest_path} ({name}) does not inherit the workspace lints; "
            "every member needs\n"
            "            [lints]\n"
            "            workspace = true\n"
            "        without it the workspace denies — the float ban included — "
            "do not apply to this crate and every gate still passes"
        ]
    return []


def workspace_members() -> list[tuple[str, Path]]:
    """(name, manifest path) for each workspace member, from cargo itself.

    Derived rather than listed: a hardcoded member list in a checker is the same
    drift the checker exists to catch.
    """
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"cargo metadata failed: {result.stderr.strip()}")
    metadata = json.loads(result.stdout)
    return [
        (package["name"], Path(package["manifest_path"]))
        for package in metadata["packages"]
    ]


def read_manifest(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def audit() -> list[str]:
    problems = check_workspace_table(read_manifest(ROOT / "Cargo.toml"))
    members = workspace_members()
    if not members:
        return [*problems, "cargo reported no workspace members"]
    for name, manifest_path in sorted(members):
        problems += check_member_opt_in(
            name, rel(manifest_path), read_manifest(manifest_path)
        )
    return problems


def self_test() -> int:
    """Prove both checks refuse what they must, and accept what they must."""
    cases: list[tuple[str, list[str], bool]] = []

    opted_in = {"lints": {"workspace": True}}
    cases.append(
        ("a member that opts in passes", check_member_opt_in("c", "c/Cargo.toml", opted_in), False)
    )
    cases.append(
        (
            "a member with no [lints] table is refused",
            check_member_opt_in("c", "c/Cargo.toml", {}),
            True,
        ),
    )
    cases.append(
        (
            "a member that opts out explicitly is refused",
            check_member_opt_in("c", "c/Cargo.toml", {"lints": {"workspace": False}}),
            True,
        ),
    )
    cases.append(
        (
            "a member with its own lints but no inheritance is refused",
            check_member_opt_in("c", "c/Cargo.toml", {"lints": {"clippy": {}}}),
            True,
        ),
    )

    full = {
        "workspace": {
            "lints": {
                "rust": dict(REQUIRED_RUST),
                "clippy": dict(REQUIRED_CLIPPY),
            }
        }
    }
    cases.append(("the real required set passes", check_workspace_table(full), False))

    priority_form = json.loads(json.dumps(full))
    priority_form["workspace"]["lints"]["clippy"]["float_arithmetic"] = {
        "level": "forbid",
        "priority": -1,
    }
    cases.append(
        (
            "the table spelling with a priority passes",
            check_workspace_table(priority_form),
            False,
        ),
    )

    demoted = json.loads(json.dumps(full))
    demoted["workspace"]["lints"]["clippy"]["float_arithmetic"] = "warn"
    cases.append(
        (
            "demoting the float ban to warn is refused",
            check_workspace_table(demoted),
            True,
        ),
    )

    # The demotion this checker exists for after I-1's enforcement moved to
    # `forbid`. `deny` still fails a build and still reads as "configured", so
    # nothing downstream looks wrong -- but it restores the two-word crate-root
    # `#![allow]` escape that `forbid` answers with E0453.
    softened = json.loads(json.dumps(full))
    softened["workspace"]["lints"]["clippy"]["float_arithmetic"] = "deny"
    cases.append(
        (
            "softening the float ban from forbid to deny is refused",
            check_workspace_table(softened),
            True,
        ),
    )

    dropped = json.loads(json.dumps(full))
    del dropped["workspace"]["lints"]["clippy"]["unwrap_used"]
    cases.append(
        ("dropping a required clippy deny is refused", check_workspace_table(dropped), True)
    )

    no_rust = json.loads(json.dumps(full))
    del no_rust["workspace"]["lints"]["rust"]
    cases.append(
        ("dropping the rust lint table is refused", check_workspace_table(no_rust), True)
    )

    cases.append(
        ("a manifest with no lints at all is refused", check_workspace_table({}), True)
    )

    failures = 0
    for label, problems, want_problem in cases:
        passed = bool(problems) == want_problem
        print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
        failures += not passed

    if failures:
        print(f"\ncheck-workspace-lints self-test: {failures} case(s) FAILED")
        return 1
    print(f"\ncheck-workspace-lints self-test: {len(cases)} cases passed")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    try:
        problems = audit()
    except (OSError, RuntimeError, tomllib.TOMLDecodeError, json.JSONDecodeError) as exc:
        print(f"check-workspace-lints: could not run: {exc}", file=sys.stderr)
        return 2

    if problems:
        print(f"{len(problems)} problem(s):", file=sys.stderr)
        for problem in problems:
            print(f"  FAIL  {problem}", file=sys.stderr)
        print(
            "\nThe workspace lint contract is conventions §1 and invariant 1.",
            file=sys.stderr,
        )
        return 1

    print("workspace lints apply to every member")
    return 0


if __name__ == "__main__":
    sys.exit(main())
