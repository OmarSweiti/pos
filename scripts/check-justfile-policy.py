#!/usr/bin/env python3
"""Prove user-supplied `just` arguments never become shell source."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent


def run(*arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["just", *arguments],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )


def interpolates_parameter(node: Any, names: set[str]) -> bool:
    if (
        isinstance(node, list)
        and len(node) == 2
        and node[0] == "variable"
        and node[1] in names
    ):
        return True
    if isinstance(node, list):
        return any(interpolates_parameter(item, names) for item in node)
    if isinstance(node, dict):
        return any(interpolates_parameter(item, names) for item in node.values())
    return False


def recipe_text(recipe: dict[str, Any]) -> str:
    pieces: list[str] = []

    def collect(node: Any) -> None:
        if isinstance(node, str):
            pieces.append(node)
        elif isinstance(node, list):
            for item in node:
                collect(item)
        elif isinstance(node, dict):
            for item in node.values():
                collect(item)

    collect(recipe.get("body", []))
    return "\n".join(pieces)


def main() -> int:
    dumped = run("--dump", "--dump-format", "json")
    if dumped.returncode != 0:
        print(f"justfile-policy: ERROR — {dumped.stderr.strip()}", file=sys.stderr)
        return 2
    try:
        recipes = json.loads(dumped.stdout)["recipes"]
        branch = recipes["branch"]
        pull_request = recipes["pr"]
        setup = recipes["setup"]
        setup_tools = recipes["setup-tools-check"]
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"justfile-policy: ERROR — malformed just dump ({error})", file=sys.stderr)
        return 2

    expected = {
        "branch": (branch, {"name"}),
        "pr": (pull_request, {"title", "body", "milestone"}),
    }
    failures: list[str] = []
    for recipe_name, (recipe, parameter_names) in expected.items():
        parameters = recipe.get("parameters", [])
        exported = {
            item.get("name")
            for item in parameters
            if isinstance(item, dict) and item.get("export") is True
        }
        if exported != parameter_names:
            failures.append(
                f"{recipe_name}: user parameters must all be exported environment values"
            )
        if interpolates_parameter(recipe.get("body", []), parameter_names):
            failures.append(
                f"{recipe_name}: a user parameter is interpolated into shell source"
            )
        body = recipe_text(recipe)
        for name in parameter_names:
            if f'"${name}"' not in body:
                failures.append(
                    f"{recipe_name}: ${name} must reach commands only as quoted shell data"
                )

    setup_body = recipe_text(setup)
    setup_sequence = (
        "just hooks",
        "just identity",
        "just setup-tools-check",
        "just gitleaks-check",
        "just policy-tools-check",
        "pnpm install --frozen-lockfile",
        "cargo fetch --locked",
    )
    positions = [setup_body.find(command) for command in setup_sequence]
    if any(position < 0 for position in positions) or positions != sorted(positions):
        failures.append(
            "setup: install fail-closed hooks first, then check tools before networked installs"
        )
    setup_tools_body = recipe_text(setup_tools)
    for prerequisite in ("python3", "node", "cargo nextest --version", "pnpm"):
        if prerequisite not in setup_tools_body:
            failures.append(f"setup-tools-check: missing prerequisite {prerequisite!r}")

    pr_body = recipe_text(pull_request)
    for forbidden in ("2>/dev/null", "head -1 || true", "Continuing without one"):
        if forbidden in pr_body:
            failures.append(
                f"pr: milestone resolution must not suppress or continue after {forbidden!r}"
            )
    for required in (
        "gh api --paginate --slurp",
        "len(matches) != 1",
        "milestone=$(resolve_milestone prefix",
        "milestone=$(resolve_milestone exact",
    ):
        if required not in pr_body:
            failures.append(f"pr: fail-closed milestone contract is missing {required!r}")
    lookup_position = pr_body.find("milestone=$(resolve_milestone")
    pre_push_position = pr_body.find("just pre-push")
    if lookup_position < 0 or pre_push_position < 0 or lookup_position > pre_push_position:
        failures.append("pr: milestone resolution must finish before any pre-push mutation")

    hostile_cases = (
        (
            "branch",
            ["branch", "probe; printf JUST_BRANCH_INJECTED"],
            "JUST_BRANCH_INJECTED",
        ),
        (
            "PR title",
            [
                "pr",
                "chore(repo): probe $(printf JUST_TITLE_INJECTED)   [—]",
                "body; printf JUST_BODY_INJECTED",
                "milestone`printf JUST_MILESTONE_INJECTED`",
            ],
            "JUST_",
        ),
    )
    for label, arguments, sentinel in hostile_cases:
        rendered = run("--dry-run", *arguments)
        if rendered.returncode != 0:
            failures.append(f"{label}: hostile-input dry run failed unexpectedly")
        elif sentinel in rendered.stdout:
            failures.append(f"{label}: hostile input was rendered into shell source")

    # A phase PR must stop before pre-push/push/PR creation when GitHub cannot
    # supply exactly one matching milestone. Mock only Git and GitHub; the real
    # just recipe and Python resolver execute end to end.
    with tempfile.TemporaryDirectory(prefix="pos-just-policy-") as temporary:
        temp = Path(temporary)
        calls = temp / "calls"
        fake_git = temp / "git"
        fake_gh = temp / "gh"
        fake_git.write_text(
            """#!/usr/bin/env bash
set -u
printf 'git %s\\n' "$*" >> "$POLICY_CALLS"
if [ "${1:-}" = branch ] && [ "${2:-}" = --show-current ]; then
  printf 'phase-1/group-1-tax\\n'
  exit 0
fi
exit 91
""",
            encoding="utf-8",
        )
        fake_gh.write_text(
            """#!/usr/bin/env bash
set -u
printf 'gh %s\\n' "$*" >> "$POLICY_CALLS"
if [ "${1:-}" = api ]; then
  case "$POLICY_SCENARIO" in
    api-failure) exit 71 ;;
    no-match) printf '[[]]\\n'; exit 0 ;;
    duplicate) printf '[[{"title":"Phase 1 — one"},{"title":"Phase 1 — two"}]]\\n'; exit 0 ;;
  esac
fi
exit 92
""",
            encoding="utf-8",
        )
        fake_git.chmod(0o755)
        fake_gh.chmod(0o755)
        for scenario in ("api-failure", "no-match", "duplicate"):
            calls.write_text("", encoding="utf-8")
            environment = os.environ.copy()
            environment.update(
                {
                    "PATH": f"{temp}{os.pathsep}{environment['PATH']}",
                    "POLICY_CALLS": str(calls),
                    "POLICY_SCENARIO": scenario,
                }
            )
            result = subprocess.run(
                ["just", "pr"],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            recorded = calls.read_text(encoding="utf-8")
            if result.returncode == 0:
                failures.append(f"pr: {scenario} milestone lookup unexpectedly succeeded")
            if any(
                mutation in recorded
                for mutation in ("git fetch", "git push", "gh pr create")
            ):
                failures.append(
                    f"pr: {scenario} milestone lookup reached a Git/GitHub mutation"
                )

    if failures:
        for failure in failures:
            print(f"justfile-policy: FAIL — {failure}", file=sys.stderr)
        return 1
    print("justfile-policy: branch and PR arguments remain quoted data, not shell source")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
