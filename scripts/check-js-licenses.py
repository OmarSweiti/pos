#!/usr/bin/env python3
"""Fail closed on unreviewed JavaScript dependency licence metadata.

`pnpm licenses list --json` supplies the installed workspace graph. The
repository-owned policy records each accepted SPDX expression and its review
rationale; unknown, malformed, empty, or command-error inventories are fatal.

Usage: ./scripts/check-js-licenses.py [--self-test]
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
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


def pnpm_inventory(root: Path) -> Any:
    try:
        done = subprocess.run(
            ["pnpm", "licenses", "list", "--json"],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=120,
        )
    except FileNotFoundError:
        raise LicenseError("pnpm is not installed") from None
    except (OSError, subprocess.SubprocessError) as exc:
        raise LicenseError(f"could not run pnpm licenses list: {exc}") from exc
    if done.returncode != 0:
        detail = done.stderr.strip() or done.stdout.strip() or "no diagnostic"
        raise LicenseError(f"pnpm licenses list failed: {detail}")
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

    total = len(cases) + 2
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
