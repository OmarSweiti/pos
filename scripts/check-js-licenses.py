#!/usr/bin/env python3
"""Fail closed on unreviewed JavaScript dependency licence metadata.

`pnpm licenses list --json` supplies the installed workspace graph. The
repository-owned policy records each accepted SPDX expression and its review
rationale; unknown, malformed, empty, or command-error inventories are fatal.

A missing pnpm store index file is reported with the remedy that actually
works, because pnpm's own suggestion for it does not — see
`pnpm_failure_detail`.

Usage: ./scripts/check-js-licenses.py [--self-test]
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
POLICY = ROOT / "js-license-policy.json"


class LicenseError(ValueError):
    """The dependency licence inventory cannot be accepted."""


def read_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise LicenseError(f"cannot read {label} ({path}): {exc}") from exc
    except json.JSONDecodeError as exc:
        raise LicenseError(f"invalid JSON in {label} ({path}): {exc}") from exc


def load_policy(path: Path) -> dict[str, str]:
    value = read_json(path, "JavaScript licence policy")
    if not isinstance(value, dict) or value.get("schemaVersion") != 1:
        raise LicenseError("JavaScript licence policy schemaVersion must be 1")
    allowed = value.get("allowedLicenses")
    if not isinstance(allowed, dict) or not allowed:
        raise LicenseError(
            "JavaScript licence policy needs a non-empty allowedLicenses object"
        )
    result: dict[str, str] = {}
    for expression, reason in allowed.items():
        if not isinstance(expression, str) or not expression.strip():
            raise LicenseError("allowed licence expressions must be non-empty strings")
        if not isinstance(reason, str) or not reason.strip():
            raise LicenseError(
                f"allowed licence {expression!r} needs a review rationale"
            )
        if expression != expression.strip():
            raise LicenseError(
                f"allowed licence {expression!r} has surrounding whitespace"
            )
        result[expression] = reason
    return result


def audit_inventory(inventory: Any, allowed: dict[str, str]) -> tuple[int, list[str]]:
    if not isinstance(inventory, dict):
        raise LicenseError("pnpm licence inventory must be a JSON object")
    if "error" in inventory:
        raise LicenseError(f"pnpm returned an error envelope: {inventory['error']!r}")
    if not inventory:
        raise LicenseError("pnpm reported zero dependency licences")

    unknown = sorted(set(inventory) - set(allowed))
    if unknown:
        raise LicenseError("unreviewed licence expression(s): " + ", ".join(unknown))

    packages = 0
    seen: set[tuple[str, str]] = set()
    used: list[str] = []
    for expression in sorted(inventory):
        entries = inventory[expression]
        if not isinstance(entries, list) or not entries:
            raise LicenseError(f"licence group {expression!r} must contain packages")
        used.append(expression)
        for index, entry in enumerate(entries):
            if not isinstance(entry, dict):
                raise LicenseError(f"{expression} entry {index} is not an object")
            name = entry.get("name")
            versions = entry.get("versions")
            declared = entry.get("license")
            if not isinstance(name, str) or not name.strip():
                raise LicenseError(f"{expression} entry {index} has no package name")
            if declared != expression:
                raise LicenseError(
                    f"{name}: declared licence {declared!r} disagrees with group {expression!r}"
                )
            if not isinstance(versions, list) or not versions:
                raise LicenseError(f"{name}: licence inventory has no versions")
            for version in versions:
                if not isinstance(version, str) or not version.strip():
                    raise LicenseError(f"{name}: invalid version in licence inventory")
                identity = (name, version)
                if identity in seen:
                    raise LicenseError(
                        f"duplicate package in licence inventory: {name}@{version}"
                    )
                seen.add(identity)
                packages += 1
    if packages == 0:
        raise LicenseError("pnpm reported zero dependency packages")
    return packages, used


# pnpm's own remedy for a missing store index file is "please consider running
# 'pnpm install'", and that advice does not work. Observed 2026-08-28 after two
# Dependabot bumps (vite 8.2.2, @vitejs/plugin-react 6.1.0) landed in the
# lockfile: `pnpm install`, `pnpm install --frozen-lockfile` and
# `pnpm install --force` all report "Already up to date" and leave the index
# file missing, because node_modules is complete — it is the *store* that is
# incomplete, and only `pnpm licenses list` reads that part of it.
#
# Relaying a remedy that cannot work costs the reader the whole debugging path,
# so this checker names the one that does. The package is reported per run
# because pnpm stops at the first missing entry: repairing one reveals the next.
MISSING_INDEX_CODE = "ERR_PNPM_MISSING_PACKAGE_INDEX_FILE"


def pnpm_failure_detail(stderr: str, stdout: str) -> str:
    """Describe a failed `pnpm licenses list`, correcting pnpm's bad advice."""
    detail = stderr.strip() or stdout.strip() or "no diagnostic"
    if MISSING_INDEX_CODE not in detail:
        return f"pnpm licenses list failed: {detail}"
    match = re.search(r"index file for (\S+)", detail)
    package = match.group(1) if match else "the package named above"
    return (
        f"the pnpm store has no index file for {package}, so the licence "
        "inventory cannot be read.\n"
        f"  Repair it with:  pnpm store add {package}\n"
        "  Then re-run this check. pnpm stops at the first missing entry, so "
        "repeat until it passes.\n"
        "  Ignore pnpm's own suggestion to run `pnpm install` — plain, "
        "--frozen-lockfile and --force all report\n"
        "  \"Already up to date\" and leave the store unrepaired. This happens "
        "when a dependency bump lands in\n"
        "  the lockfile without its store entry ever being fetched on this "
        "machine.\n"
        f"  pnpm said: {detail}"
    )


def resolve_pnpm(which: Callable[[str], str | None] = shutil.which) -> str:
    """Resolve `pnpm` to a real executable path before running it.

    On Windows pnpm installs as `pnpm.CMD`, and `subprocess.run(["pnpm", ...])`
    does **not** search `PATHEXT` — so a perfectly installed pnpm raises
    `FileNotFoundError` and this checker reported "pnpm is not installed", which
    is both wrong and the least useful thing it could have said. `shutil.which`
    does search `PATHEXT`, and returns the path `subprocess` can actually spawn.

    Found by the first cross-platform CI run this repository ever performed, on
    2026-08-29. The job only runs on promotion pull requests, and there had never
    been one, so a checker that could not run on one of the three release
    platforms had been sitting in the workflow unexercised.
    """
    found = which("pnpm")
    if found is None:
        raise LicenseError(
            "pnpm was not found on PATH (searched PATHEXT too, so this is a real "
            "absence rather than the Windows .CMD resolution problem)"
        )
    return found


def pnpm_inventory(root: Path) -> Any:
    try:
        done = subprocess.run(
            [resolve_pnpm(), "licenses", "list", "--json"],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=120,
        )
    except FileNotFoundError:
        raise LicenseError("pnpm resolved to a path that could not be spawned") from None
    except (OSError, subprocess.SubprocessError) as exc:
        raise LicenseError(f"could not run pnpm licenses list: {exc}") from exc
    if done.returncode != 0:
        raise LicenseError(pnpm_failure_detail(done.stderr, done.stdout))
    if done.stderr.strip():
        raise LicenseError(f"pnpm licenses list wrote to stderr: {done.stderr.strip()}")
    try:
        return json.loads(done.stdout)
    except json.JSONDecodeError as exc:
        raise LicenseError(f"pnpm licenses list returned invalid JSON: {exc}") from exc


def entry(name: str, version: str, licence: str) -> dict[str, Any]:
    return {
        "name": name,
        "versions": [version],
        "license": licence,
        "paths": ["fixture"],
    }


def self_test() -> int:
    failures = 0
    with tempfile.TemporaryDirectory(prefix="js-license-policy-") as temporary:
        policy_path = Path(temporary) / "policy.json"
        policy_path.write_text(
            json.dumps(
                {
                    "schemaVersion": 1,
                    "allowedLicenses": {
                        "MIT": "reviewed permissive licence",
                        "Apache-2.0": "reviewed permissive licence",
                    },
                }
            ),
            encoding="utf-8",
        )
        allowed = load_policy(policy_path)
        good = {
            "MIT": [entry("alpha", "1.0.0", "MIT")],
            "Apache-2.0": [entry("beta", "2.0.0", "Apache-2.0")],
        }
        try:
            count, used = audit_inventory(good, allowed)
            passed = count == 2 and used == ["Apache-2.0", "MIT"]
        except LicenseError:
            passed = False
        print(f"  {'ok  ' if passed else 'FAIL'}  reviewed licence inventory passes")
        failures += not passed

        cases: list[tuple[str, Any]] = [
            (
                "an unreviewed licence is refused",
                {"GPL-3.0-only": [entry("copyleft", "1.0.0", "GPL-3.0-only")]},
            ),
            ("an empty inventory is refused", {}),
            ("a command error envelope is refused", {"error": {"code": "BROKEN"}}),
            ("a malformed inventory is refused", [entry("alpha", "1.0.0", "MIT")]),
            (
                "a licence/group mismatch is refused",
                {"MIT": [entry("alpha", "1.0.0", "Apache-2.0")]},
            ),
            (
                "a duplicate package identity is refused",
                {
                    "MIT": [
                        entry("alpha", "1.0.0", "MIT"),
                        entry("alpha", "1.0.0", "MIT"),
                    ]
                },
            ),
        ]
        for label, inventory in cases:
            try:
                audit_inventory(inventory, allowed)
            except LicenseError:
                passed = True
            else:
                passed = False
            print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
            failures += not passed

        bad_policy = Path(temporary) / "bad-policy.json"
        bad_policy.write_text(
            json.dumps({"schemaVersion": 1, "allowedLicenses": {"MIT": ""}}),
            encoding="utf-8",
        )
        try:
            load_policy(bad_policy)
        except LicenseError:
            passed = True
        else:
            passed = False
        print(f"  {'ok  ' if passed else 'FAIL'}  a rationale-free policy is refused")
        failures += not passed

    # Windows resolution. The checker reported "pnpm is not installed" on a
    # runner where pnpm was installed and working, because pnpm is pnpm.CMD there
    # and subprocess does not search PATHEXT.
    resolution_cases = [
        (
            "a Windows .CMD shim resolves to its real path",
            resolve_pnpm(lambda _: r"C:\\npm\\prefix\\pnpm.CMD") == r"C:\\npm\\prefix\\pnpm.CMD",
        ),
        (
            "a POSIX pnpm resolves to its real path",
            resolve_pnpm(lambda _: "/usr/local/bin/pnpm") == "/usr/local/bin/pnpm",
        ),
    ]
    try:
        resolve_pnpm(lambda _: None)
    except LicenseError as exc:
        resolution_cases.append(
            (
                "a genuine absence says so, and says PATHEXT was searched",
                "PATHEXT" in str(exc),
            )
        )
    else:
        resolution_cases.append(("a genuine absence says so, and says PATHEXT was searched", False))
    for label, ok in resolution_cases:
        print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
        failures += not ok

    # The store-index remedy. A wrong remedy is worse than no remedy, so these
    # assert both halves: the working advice is present, and pnpm's own advice
    # is not repeated as if it were the fix.
    missing_index = (
        'ERROR  \u2009ERR_PNPM_MISSING_PACKAGE_INDEX_FILE  Failed to find package '
        'index file for vite@8.2.2 (at sha512-abc\tvite@8.2.2), please consider '
        "running 'pnpm install'"
    )
    described = pnpm_failure_detail(missing_index, "")
    index_cases = [
        (
            "a missing store index names the package",
            "vite@8.2.2" in described,
        ),
        (
            "a missing store index gives the remedy that works",
            "pnpm store add vite@8.2.2" in described,
        ),
        (
            "a missing store index contradicts pnpm's own advice",
            "Ignore pnpm's own suggestion" in described,
        ),
        (
            "a missing store index says to repeat until it passes",
            "repeat until it passes" in described,
        ),
        (
            "an unrelated pnpm failure is passed through unchanged",
            pnpm_failure_detail("EACCES: permission denied", "")
            == "pnpm licenses list failed: EACCES: permission denied",
        ),
        (
            "an empty diagnostic still produces a message",
            "no diagnostic" in pnpm_failure_detail("", ""),
        ),
    ]
    for label, ok in index_cases:
        print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
        failures += not ok

    total = len(cases) + len(index_cases) + len(resolution_cases) + 2
    if failures:
        print(f"\ncheck-js-licenses self-test: {failures}/{total} case(s) FAILED")
        return 1
    print(f"\ncheck-js-licenses self-test: {total} cases passed")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if sys.argv[1:]:
        print("usage: check-js-licenses.py [--self-test]", file=sys.stderr)
        return 2
    try:
        allowed = load_policy(POLICY)
        packages, used = audit_inventory(pnpm_inventory(ROOT), allowed)
    except LicenseError as exc:
        print(f"check-js-licenses: REFUSED — {exc}", file=sys.stderr)
        return 1
    print(
        f"JavaScript licences: {packages} package release(s), "
        f"{len(used)} reviewed expression(s): {', '.join(used)}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
