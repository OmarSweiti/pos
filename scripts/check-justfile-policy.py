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
        encoding="utf-8",
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
        merge = recipes["merge"]
        setup = recipes["setup"]
        setup_tools = recipes["setup-tools-check"]
        db_up = recipes["db-up"]
        db_reset = recipes["db-reset"]
        build_web = recipes["build-web"]
        audit = recipes["audit"]
        guards = recipes["guards"]
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"justfile-policy: ERROR — malformed just dump ({error})", file=sys.stderr)
        return 2

    expected = {
        "branch": (branch, {"name"}),
        "pr": (pull_request, {"title", "body", "milestone"}),
        "merge": (merge, {"pr"}),
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
    for prerequisite in (
        "python3",
        "node",
        "cargo nextest --version",
        "pnpm",
        "ruff",
        "shellcheck",
    ):
        if prerequisite not in setup_tools_body:
            failures.append(f"setup-tools-check: missing prerequisite {prerequisite!r}")

    for recipe_name, recipe in (("db-up", db_up), ("db-reset", db_reset)):
        if "--wait --wait-timeout 120" not in recipe_text(recipe):
            failures.append(f"{recipe_name}: Docker readiness must have a finite timeout")

    build_body = recipe_text(build_web)
    for required in (
        "scripts/check-web-build-coverage.py",
        "pnpm -r build",
    ):
        if required not in build_body:
            failures.append(f"build-web: fail-closed build coverage is missing {required!r}")
    if "--if-present" in build_body:
        failures.append("build-web: --if-present makes missing build scripts pass")

    audit_body = recipe_text(audit)
    audit_dependencies = {
        item.get("recipe")
        for item in audit.get("dependencies", [])
        if isinstance(item, dict)
    }
    if "node-version-check" not in audit_dependencies:
        failures.append("audit: the JS supply-chain gate must enforce the exact Node runtime")
    for required in (
        "cargo deny check",
        "scripts/check-js-licenses.py",
        "pnpm audit --audit-level high",
    ):
        if required not in audit_body:
            failures.append(f"audit: supply-chain coverage is missing {required!r}")

    guards_body = recipe_text(guards)
    for required in (
        "scripts/check-web-build-coverage.py --self-test",
        "scripts/check-js-licenses.py --self-test",
        "scripts/check-justfile-policy.py",
    ):
        if required not in guards_body:
            failures.append(f"guards: policy self-test coverage is missing {required!r}")

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

    merge_body = recipe_text(merge)
    for required in (
        "gh auth status",
        "state,url,baseRefName,baseRefOid,headRefName,headRefOid,headRepository,isDraft,title,body",
        "title=${before[8]}",
        '"$base_ref" = development',
        "development|staging|main|hotfix/*",
        "scripts/validate-branch-flow.sh",
        'scripts/validate-change-title.sh --validate "$title"',
        'scripts/watch-pr-checks.sh "$pr_url"',
        '--match-head-commit "$head_oid" --squash --delete-branch',
        '--subject "$title (#$pr_number)"',
    ):
        if required not in merge_body:
            failures.append(f"merge: safe work-PR contract is missing {required!r}")
    if 'gh pr merge "$target"' in merge_body:
        failures.append("merge: the caller-supplied target must be canonicalized before merge")

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
        (
            "merge target",
            ["merge", "probe; printf JUST_MERGE_INJECTED"],
            "JUST_MERGE_INJECTED",
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
                encoding="utf-8",
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

    # Exercise the real merge recipe and readiness watcher with a fake GitHub
    # CLI. This proves route refusals occur before check evidence is collected,
    # both tips are re-read after the watcher, hostile inputs remain argv data,
    # and the only successful mutation carries GitHub's atomic head match and
    # the validated snapshot title.
    with tempfile.TemporaryDirectory(prefix="pos-merge-policy-") as temporary:
        temp = Path(temporary)
        calls = temp / "calls"
        view_count = temp / "view-count"
        fake_gh = temp / "gh"
        fake_gh.write_text(
            r'''#!/usr/bin/env bash
set -euo pipefail
{
  printf 'gh'
  for argument in "$@"; do printf ' <%s>' "$argument"; done
  printf '\n'
} >> "$POLICY_CALLS"

if [ "${1:-}" = auth ] && [ "${2:-}" = status ]; then
  exit 0
fi
if [ "${1:-}" = repo ] && [ "${2:-}" = view ]; then
  printf 'owner/pos\n'
  exit 0
fi
if [ "${1:-}" = pr ] && [ "${2:-}" = view ]; then
  json_fields=''
  previous=''
  for argument in "$@"; do
    if [ "$previous" = --json ]; then json_fields=$argument; break; fi
    previous=$argument
  done
  merge_view_count=0
  case "$json_fields" in
    *headRepository*)
      [ ! -f "$POLICY_VIEW_COUNT" ] || read -r merge_view_count < "$POLICY_VIEW_COUNT"
      merge_view_count=$((merge_view_count + 1))
      printf '%s\n' "$merge_view_count" > "$POLICY_VIEW_COUNT"
      ;;
  esac

  base=development
  head=fix/merge-policy
  state=OPEN
  draft=false
  title='fix(repo): merge policy   [—]'
  body=reviewed
  pr_url=https://github.com/owner/pos/pull/42
  base_oid=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  head_oid=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  case "$POLICY_SCENARIO" in
    promotion) base=staging; head=development ;;
    hotfix) base=main; head=hotfix/urgent-fix ;;
    closed) state=CLOSED ;;
    draft) draft=true ;;
    invalid-route) head=feature/not-in-the-repository-grammar ;;
    invalid-title) title='--self-test' ;;
    valid-title) title='fix(repo): merge $(printf PWNED) safely   [—]' ;;
    foreign) pr_url=https://github.com/other/pos/pull/42 ;;
    head-drift) [ "$merge_view_count" -lt 2 ] || head_oid=cccccccccccccccccccccccccccccccccccccccc ;;
    base-drift) [ "$merge_view_count" -lt 2 ] || base_oid=dddddddddddddddddddddddddddddddddddddddd ;;
    metadata-drift) [ "$merge_view_count" -lt 2 ] || title='fix(repo): changed after checks   [—]' ;;
  esac
  case "$json_fields" in
    *changedFiles*)
      printf '42\t%s\t%s\t%s\t%s\t1\t%s\t%s\n' \
        "$base" "$base_oid" "$head" "$head_oid" "$state" "$pr_url"
      printf '["fix(repo): merge policy   [—]",""]\n'
      ;;
    url)
      printf 'https://github.com/owner/pos/pull/42\n'
      ;;
    *)
      printf '{"state":"%s","url":"%s","baseRefName":"%s","baseRefOid":"%s","headRefName":"%s","headRefOid":"%s","headRepository":{"nameWithOwner":"owner/pos"},"isDraft":%s,"title":"%s","body":"%s"}\n' \
        "$state" "$pr_url" "$base" "$base_oid" "$head" "$head_oid" "$draft" "$title" "$body"
      ;;
  esac
  exit 0
fi
if [ "${1:-}" = pr ] && [ "${2:-}" = checks ]; then
  for argument in "$@"; do
    [ "$argument" != --watch ] || exit 0
  done
  printf 'rust\tci\tpull_request\tSUCCESS\thttps://github.com/owner/pos/actions/runs/1\n'
  printf 'guards\tci\tpull_request\tSUCCESS\thttps://github.com/owner/pos/actions/runs/2\n'
  printf 'web\tci\tpull_request\tSUCCESS\thttps://github.com/owner/pos/actions/runs/3\n'
  printf 'supply-chain\tci\tpull_request\tSUCCESS\thttps://github.com/owner/pos/actions/runs/4\n'
  printf 'protected-paths\tbranch-flow\tpull_request_target\tSUCCESS\thttps://github.com/owner/pos/actions/runs/5\n'
  printf 'topology\tbranch-flow\tpull_request_target\tSUCCESS\thttps://github.com/owner/pos/actions/runs/6\n'
  exit 0
fi
if [ "${1:-}" = api ]; then
  endpoint=''
  for argument in "$@"; do
    case "$argument" in repos/*) endpoint=$argument ;; esac
  done
  case "$endpoint" in
    repos/owner/pos/pulls/42/files*)
      printf '[[{"filename":"README.md","previous_filename":null}]]\n'
      ;;
    repos/owner/pos/actions/runs/*)
      run_id=${endpoint##*/}
      case "$run_id" in
        1|2|3|4) printf '.github/workflows/ci.yml\tpull_request\tci\n' ;;
        5|6) printf '.github/workflows/branch-flow.yml\tpull_request_target\tbranch-flow\n' ;;
        *) exit 93 ;;
      esac
      ;;
    *) exit 94 ;;
  esac
  exit 0
fi
if [ "${1:-}" = pr ] && [ "${2:-}" = merge ]; then
  exit 0
fi
exit 95
''',
            encoding="utf-8",
        )
        fake_gh.chmod(0o755)

        def run_merge(scenario: str, target: str = "42") -> subprocess.CompletedProcess[str]:
            calls.write_text("", encoding="utf-8")
            view_count.unlink(missing_ok=True)
            environment = os.environ.copy()
            environment.update(
                {
                    "PATH": f"{temp}{os.pathsep}{environment['PATH']}",
                    "POLICY_CALLS": str(calls),
                    "POLICY_SCENARIO": scenario,
                    "POLICY_VIEW_COUNT": str(view_count),
                }
            )
            return subprocess.run(
                ["just", "merge", target],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                encoding="utf-8",
                check=False,
            )

        for scenario in (
            "promotion",
            "hotfix",
            "closed",
            "draft",
            "invalid-route",
            "foreign",
        ):
            result = run_merge(scenario)
            recorded = calls.read_text(encoding="utf-8")
            if result.returncode == 0:
                failures.append(f"merge: {scenario} PR unexpectedly succeeded")
            if " <checks>" in recorded or " <merge>" in recorded:
                failures.append(
                    f"merge: {scenario} PR reached check collection or a merge mutation"
                )

        for scenario in ("head-drift", "base-drift", "metadata-drift"):
            result = run_merge(scenario)
            recorded = calls.read_text(encoding="utf-8")
            if result.returncode == 0:
                failures.append(f"merge: {scenario} after check collection was accepted")
            if " <merge>" in recorded:
                failures.append(f"merge: {scenario} reached the merge mutation")

        result = run_merge("invalid-title")
        recorded = calls.read_text(encoding="utf-8")
        if result.returncode == 0:
            failures.append("merge: an invalid PR title unexpectedly succeeded")
        if " <merge>" in recorded:
            failures.append("merge: an invalid PR title reached the merge mutation")
        if "got: --self-test" not in result.stderr:
            failures.append("merge: an invalid PR title was not named in the refusal")
        if "Edit the title on https://github.com/owner/pos/pull/42" not in result.stderr:
            failures.append("merge: an invalid PR title refusal gave no remediation")

        sentinel = temp / "shell-injection-ran"
        hostile_target = f"42; $(touch {sentinel})"
        result = run_merge("valid-title", hostile_target)
        recorded = calls.read_text(encoding="utf-8")
        if result.returncode != 0:
            failures.append(
                f"merge: valid work PR failed the fake-gh integration ({result.stderr.strip()})"
            )
        if sentinel.exists():
            failures.append("merge: hostile target became executable shell source")
        expected_merge = (
            "gh <pr> <merge> <https://github.com/owner/pos/pull/42> "
            "<--match-head-commit> <bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb> "
            "<--squash> <--delete-branch> <--subject> "
            "<fix(repo): merge $(printf PWNED) safely   [—] (#42)>"
        )
        merge_calls = [line for line in recorded.splitlines() if " <merge>" in line]
        if merge_calls != [expected_merge]:
            failures.append(
                "merge: successful work PR did not use the canonical URL, exact head, "
                "squash mode, branch deletion, and validated subject exactly once"
            )

    if failures:
        for failure in failures:
            print(f"justfile-policy: FAIL — {failure}", file=sys.stderr)
        return 1
    print(
        "justfile-policy: branch, PR, and merge inputs remain quoted data; "
        "merge routes, tips, and titles are fail-closed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
