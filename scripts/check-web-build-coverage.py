#!/usr/bin/env python3
"""Refuse a recursive web build that silently omits a workspace package.

pnpm itself resolves the workspace patterns. This checker consumes that same
package set and requires a non-empty `scripts.build` entry in every non-root
project before `pnpm -r build` runs.

Usage: ./scripts/check-web-build-coverage.py [--self-test]
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent


class CoverageError(ValueError):
    """The workspace inventory cannot prove complete build coverage."""


def load_manifest(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise CoverageError(f"cannot read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise CoverageError(f"invalid JSON in {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise CoverageError(f"{path} must contain a JSON object")
    return value


def within_root(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
    except ValueError:
        return False
    return True


def validate_inventory(records: Any, root: Path) -> list[tuple[str, Path]]:
    if not isinstance(records, list):
        raise CoverageError("pnpm workspace inventory must be a JSON array")

    canonical_root = root.resolve()
    projects: list[tuple[str, Path]] = []
    seen_names: set[str] = set()
    seen_paths: set[Path] = set()
    for index, record in enumerate(records):
        if not isinstance(record, dict):
            raise CoverageError(f"workspace record {index} is not an object")
        name = record.get("name")
        raw_path = record.get("path")
        if not isinstance(name, str) or not name.strip():
            raise CoverageError(f"workspace record {index} has no package name")
        if not isinstance(raw_path, str) or not raw_path:
            raise CoverageError(f"workspace record {index} has no package path")
        package_path = Path(raw_path).resolve()
        if package_path == canonical_root:
            continue
        if not within_root(package_path, canonical_root):
            raise CoverageError(
                f"workspace package {name!r} escapes the repository: {raw_path}"
            )
        if name in seen_names:
            raise CoverageError(f"duplicate workspace package name: {name!r}")
        if package_path in seen_paths:
            raise CoverageError(f"duplicate workspace package path: {package_path}")
        seen_names.add(name)
        seen_paths.add(package_path)

        manifest_path = package_path / "package.json"
        manifest = load_manifest(manifest_path)
        if manifest.get("name") != name:
            raise CoverageError(
                f"{manifest_path}: package name {manifest.get('name')!r} "
                f"does not match pnpm inventory {name!r}"
            )
        scripts = manifest.get("scripts")
        build = scripts.get("build") if isinstance(scripts, dict) else None
        if not isinstance(build, str) or not build.strip():
            raise CoverageError(
                f"{manifest_path}: every workspace package needs a non-empty scripts.build"
            )
        projects.append((name, package_path))

    if not projects:
        raise CoverageError("pnpm resolved zero non-root workspace packages")
    return sorted(projects)


def pnpm_inventory(root: Path) -> Any:
    try:
        done = subprocess.run(
            ["pnpm", "list", "-r", "--depth", "-1", "--json"],
            cwd=root,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=60,
        )
    except FileNotFoundError:
        raise CoverageError("pnpm is not installed") from None
    except (OSError, subprocess.SubprocessError) as exc:
        raise CoverageError(f"could not inventory the pnpm workspace: {exc}") from exc
    if done.returncode != 0:
        detail = done.stderr.strip() or done.stdout.strip() or "no diagnostic"
        raise CoverageError(f"pnpm workspace inventory failed: {detail}")
    try:
        return json.loads(done.stdout)
    except json.JSONDecodeError as exc:
        raise CoverageError(f"pnpm returned invalid JSON: {exc}") from exc


def fixture(root: Path, builds: dict[str, str | None]) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = [{"name": "root", "path": str(root)}]
    for name, build in builds.items():
        package_path = root / "packages" / name
        package_path.mkdir(parents=True)
        manifest: dict[str, Any] = {"name": name, "scripts": {}}
        if build is not None:
            manifest["scripts"]["build"] = build
        (package_path / "package.json").write_text(
            json.dumps(manifest), encoding="utf-8"
        )
        records.append({"name": name, "path": str(package_path)})
    return records


def self_test() -> int:
    failures = 0
    with tempfile.TemporaryDirectory(prefix="web-build-coverage-") as temporary:
        base = Path(temporary)
        good_root = base / "good"
        good_root.mkdir()
        good = fixture(good_root, {"app": "tsc -b", "library": "tsc --noEmit"})
        try:
            passed = len(validate_inventory(good, good_root)) == 2
        except CoverageError:
            passed = False
        print(f"  {'ok  ' if passed else 'FAIL'}  all workspace builds are accepted")
        failures += not passed

        cases: list[tuple[str, Any, Path]] = []

        missing_root = base / "missing"
        missing_root.mkdir()
        cases.append(
            (
                "a missing build script is refused",
                fixture(missing_root, {"app": None}),
                missing_root,
            )
        )

        empty_root = base / "empty"
        empty_root.mkdir()
        cases.append(
            (
                "an empty build script is refused",
                fixture(empty_root, {"app": "  "}),
                empty_root,
            )
        )

        zero_root = base / "zero"
        zero_root.mkdir()
        cases.append(
            (
                "zero resolved packages are refused",
                [{"name": "root", "path": str(zero_root)}],
                zero_root,
            )
        )

        escape_root = base / "escape"
        escape_root.mkdir()
        outside = base / "outside"
        outside.mkdir()
        (outside / "package.json").write_text(
            json.dumps({"name": "outside", "scripts": {"build": "tsc"}}),
            encoding="utf-8",
        )
        cases.append(
            (
                "a workspace path outside the repository is refused",
                [
                    {"name": "root", "path": str(escape_root)},
                    {"name": "outside", "path": str(outside)},
                ],
                escape_root,
            )
        )

        duplicate_root = base / "duplicate"
        duplicate_root.mkdir()
        duplicate = fixture(duplicate_root, {"app": "tsc"})
        duplicate.append(dict(duplicate[-1]))
        cases.append(
            ("a duplicate workspace record is refused", duplicate, duplicate_root)
        )

        malformed_root = base / "malformed"
        malformed_root.mkdir()
        cases.append(
            ("a malformed inventory is refused", {"packages": []}, malformed_root)
        )

        for label, records, root in cases:
            try:
                validate_inventory(records, root)
            except CoverageError:
                passed = True
            else:
                passed = False
            print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
            failures += not passed

    total = len(cases) + 1
    if failures:
        print(
            f"\ncheck-web-build-coverage self-test: {failures}/{total} case(s) FAILED"
        )
        return 1
    print(f"\ncheck-web-build-coverage self-test: {total} cases passed")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if sys.argv[1:]:
        print("usage: check-web-build-coverage.py [--self-test]", file=sys.stderr)
        return 2
    try:
        projects = validate_inventory(pnpm_inventory(ROOT), ROOT)
    except CoverageError as exc:
        print(f"check-web-build-coverage: REFUSED — {exc}", file=sys.stderr)
        return 1
    rendered = ", ".join(name for name, _path in projects)
    print(f"web build coverage: {len(projects)} workspace package(s): {rendered}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
