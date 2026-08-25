#!/usr/bin/env python3
"""Keep the Node runtime contract anchored to the exact `.nvmrc` release.

The pin controls four independent consumers: the developer/CI runtime, the
root package engine, pnpm's dependency-engine resolver, and every setup-node
workflow step. A range or a newer host runtime is not equivalent to the pin:
it can resolve a materially different optional-dependency graph or compile
against APIs unavailable in the pinned release-build environment.

Usage:  ./scripts/check-node-version.py
        ./scripts/check-node-version.py --self-test
Exit:   0 contract is exact · 1 contract/runtime drift · 2 bad invocation
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent

VERSION = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")
SETUP_NODE = re.compile(
    r"^(?P<indent>\s*)-\s+uses:\s*actions/setup-node@\S+\s*(?:#.*)?$"
)
NODE_VERSION_FILE = re.compile(
    r"\bnode-version-file\s*:\s*(['\"]?)\.nvmrc\1(?=\s*[,}]|\s*(?:#.*)?$)",
    re.MULTILINE,
)
HARDCODED_NODE_VERSION = re.compile(r"\bnode-version(?!-file)\s*:")


class ContractError(ValueError):
    """The repository's Node contract is incomplete or inconsistent."""


def parse(version: str) -> tuple[int, int, int]:
    value = version.strip()
    matched = VERSION.fullmatch(value)
    if matched is None:
        raise ContractError(f"not an exact Node release: {value!r}")
    return (int(matched[1]), int(matched[2]), int(matched[3]))


def normalized(version: str) -> str:
    major, minor, patch = parse(version)
    return f"{major}.{minor}.{patch}"


def exact_runtime_refusal(pinned: str, running: str) -> str | None:
    want = normalized(pinned)
    got = normalized(running)
    if got != want:
        return f"Node {want} is required exactly (.nvmrc); found {running.strip()}"
    return None


def load_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise ContractError(f"cannot read {label} ({path}): {exc}") from exc
    except json.JSONDecodeError as exc:
        raise ContractError(f"invalid JSON in {label} ({path}): {exc}") from exc
    if not isinstance(value, dict):
        raise ContractError(f"{label} must contain a JSON object")
    return value


def workspace_scalars(path: Path) -> dict[str, str]:
    """Read the two required top-level pnpm settings without a YAML dependency."""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as exc:
        raise ContractError(f"cannot read {path}: {exc}") from exc

    wanted = {"nodeVersion", "engineStrict"}
    found: dict[str, str] = {}
    for number, line in enumerate(lines, 1):
        if not line or line[0].isspace() or line.lstrip().startswith("#"):
            continue
        matched = re.match(r"^([A-Za-z][A-Za-z0-9]*)\s*:\s*(.*?)\s*$", line)
        if matched is None or matched[1] not in wanted:
            continue
        key = matched[1]
        raw = re.sub(r"\s+#.*$", "", matched[2]).strip()
        if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in {"'", '"'}:
            raw = raw[1:-1]
        if not raw:
            raise ContractError(f"{path.name}:{number}: {key} needs a scalar value")
        if key in found:
            raise ContractError(f"{path.name}:{number}: duplicate {key}")
        found[key] = raw
    return found


def workspace_override(path: Path, dependency: str) -> str | None:
    """Read one immediate `overrides` entry from the repository's pnpm YAML."""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as exc:
        raise ContractError(f"cannot read {path}: {exc}") from exc

    in_overrides = False
    seen_overrides = False
    value: str | None = None
    for number, line in enumerate(lines, 1):
        if line == "overrides:":
            if seen_overrides:
                raise ContractError(
                    f"{path.name}:{number}: duplicate overrides section"
                )
            seen_overrides = True
            in_overrides = True
            continue
        if in_overrides and line and not line[0].isspace():
            in_overrides = False
        if not in_overrides or line.lstrip().startswith("#"):
            continue
        matched = re.match(r"^  ([^:]+)\s*:\s*(.*?)\s*$", line)
        if matched is None:
            continue
        key = matched[1].strip().strip("'\"")
        if key != dependency:
            continue
        raw = re.sub(r"\s+#.*$", "", matched[2]).strip()
        if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in {"'", '"'}:
            raw = raw[1:-1]
        if value is not None:
            raise ContractError(
                f"{path.name}:{number}: duplicate override for {dependency}"
            )
        value = raw
    return value


def setup_node_blocks(text: str, path: Path) -> list[str]:
    lines = text.splitlines()
    blocks: list[str] = []
    for index, line in enumerate(lines):
        if "actions/setup-node@" not in line:
            continue
        matched = SETUP_NODE.match(line)
        if matched is None:
            raise ContractError(
                f"{path}: setup-node must be a direct `- uses:` step so its pin can be audited"
            )
        indent = len(matched["indent"])
        block = [line]
        for following in lines[index + 1 :]:
            stripped = following.lstrip()
            following_indent = len(following) - len(stripped)
            if stripped and following_indent == indent and stripped.startswith("-"):
                break
            if stripped and following_indent < indent:
                break
            block.append(following)
        blocks.append("\n".join(block))
    return blocks


def validate_workflows(root: Path) -> list[str]:
    workflow_dir = root / ".github" / "workflows"
    paths = sorted((*workflow_dir.glob("*.yml"), *workflow_dir.glob("*.yaml")))
    setup_steps = 0
    checked: list[str] = []
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as exc:
            raise ContractError(f"cannot read workflow {path}: {exc}") from exc
        blocks = setup_node_blocks(text, path)
        for number, block in enumerate(blocks, 1):
            setup_steps += 1
            auditable = "\n".join(
                re.sub(r"\s+#.*$", "", line)
                for line in block.splitlines()
                if not line.lstrip().startswith("#")
            )
            if HARDCODED_NODE_VERSION.search(auditable):
                raise ContractError(
                    f"{path}: setup-node step {number} hardcodes node-version; use .nvmrc"
                )
            if len(NODE_VERSION_FILE.findall(auditable)) != 1:
                raise ContractError(
                    f"{path}: setup-node step {number} must read "
                    "node-version-file: .nvmrc exactly once"
                )
            checked.append(f"{path.relative_to(root)} setup-node step {number}")
    if setup_steps == 0:
        raise ContractError("no actions/setup-node workflow step was found")
    return checked


def validate_repository(root: Path, running: str) -> tuple[str, list[str]]:
    nvmrc = root / ".nvmrc"
    try:
        raw_pin = nvmrc.read_text(encoding="utf-8")
    except OSError as exc:
        raise ContractError(f"cannot read {nvmrc}: {exc}") from exc
    if raw_pin.count("\n") > 1 or not raw_pin.strip():
        raise ContractError(".nvmrc must contain exactly one non-empty line")
    pin = normalized(raw_pin)
    if raw_pin.strip() != pin:
        raise ContractError(f".nvmrc must contain the canonical exact release {pin!r}")

    package = load_json(root / "package.json", "root package.json")
    engines = package.get("engines")
    engine = engines.get("node") if isinstance(engines, dict) else None
    if engine != pin:
        raise ContractError(
            f"package.json engines.node must equal .nvmrc exactly ({pin!r}); found {engine!r}"
        )

    workspace_path = root / "pnpm-workspace.yaml"
    settings = workspace_scalars(workspace_path)
    if settings.get("nodeVersion") != pin:
        raise ContractError(
            "pnpm-workspace.yaml nodeVersion must equal .nvmrc exactly "
            f"({pin!r}); found {settings.get('nodeVersion')!r}"
        )
    if settings.get("engineStrict", "").lower() != "true":
        raise ContractError("pnpm-workspace.yaml engineStrict must be true")

    node_types = workspace_override(workspace_path, "@types/node")
    if node_types is None:
        raise ContractError("pnpm-workspace.yaml must pin an @types/node override")
    if node_types != normalized(node_types):
        raise ContractError(
            "the @types/node override must be a canonical exact release"
        )
    type_major, _type_minor, _type_patch = parse(node_types)
    runtime_major, _runtime_minor, _runtime_patch = parse(pin)
    if type_major != runtime_major:
        raise ContractError(
            f"@types/node {node_types} does not match the Node {runtime_major} runtime major"
        )
    for relative in ("apps/terminal/package.json", "apps/backoffice/package.json"):
        manifest = load_json(root / relative, relative)
        dev_dependencies = manifest.get("devDependencies")
        declared_types = (
            dev_dependencies.get("@types/node")
            if isinstance(dev_dependencies, dict)
            else None
        )
        if declared_types != node_types:
            raise ContractError(
                f"{relative} must pin @types/node exactly to the workspace override "
                f"({node_types!r}); found {declared_types!r}"
            )

    refusal = exact_runtime_refusal(pin, running)
    if refusal is not None:
        raise ContractError(refusal)
    workflows = validate_workflows(root)
    return pin, workflows


def running_node() -> str:
    try:
        done = subprocess.run(
            ["node", "--version"], capture_output=True, text=True, timeout=15
        )
    except FileNotFoundError:
        raise RuntimeError("Node.js is not installed: https://nodejs.org/") from None
    except (OSError, subprocess.SubprocessError) as exc:
        raise RuntimeError(f"could not run node --version: {exc}") from exc
    if done.returncode != 0:
        raise RuntimeError(f"node --version failed (exit {done.returncode})")
    if done.stderr.strip():
        raise RuntimeError(f"node --version wrote to stderr: {done.stderr.strip()}")
    return done.stdout.strip()


def write_fixture(root: Path) -> None:
    (root / ".github" / "workflows").mkdir(parents=True)
    (root / "apps" / "terminal").mkdir(parents=True)
    (root / "apps" / "backoffice").mkdir(parents=True)
    (root / ".nvmrc").write_text("24.19.0\n", encoding="utf-8")
    (root / "package.json").write_text(
        json.dumps({"engines": {"node": "24.19.0"}}), encoding="utf-8"
    )
    (root / "pnpm-workspace.yaml").write_text(
        "packages:\n  - packages/*\nnodeVersion: 24.19.0\nengineStrict: true\n"
        'overrides:\n  "@types/node": 24.13.3\n',
        encoding="utf-8",
    )
    for app in ("terminal", "backoffice"):
        (root / "apps" / app / "package.json").write_text(
            json.dumps({"devDependencies": {"@types/node": "24.13.3"}}),
            encoding="utf-8",
        )
    (root / ".github" / "workflows" / "ci.yml").write_text(
        "jobs:\n  web:\n    steps:\n"
        "      - uses: actions/setup-node@immutable\n"
        "        with: { node-version-file: .nvmrc, cache: pnpm }\n"
        "      - run: pnpm test\n",
        encoding="utf-8",
    )


def self_test() -> int:
    cases: list[tuple[str, Callable[[Path], str]]] = []

    def case(label: str, mutate: Callable[[Path], str]) -> None:
        cases.append((label, mutate))

    case("a newer patch runtime is refused", lambda root: "v24.19.1")
    case("a newer major runtime is refused", lambda root: "v26.4.0")

    def ranged_engine(root: Path) -> str:
        (root / "package.json").write_text(
            json.dumps({"engines": {"node": ">=24.19.0 <25"}}), encoding="utf-8"
        )
        return "v24.19.0"

    case("a ranged root engine is refused", ranged_engine)

    def wrong_resolver(root: Path) -> str:
        path = root / "pnpm-workspace.yaml"
        path.write_text(
            path.read_text(encoding="utf-8").replace("24.19.0", "24.20.0"),
            encoding="utf-8",
        )
        return "v24.19.0"

    case("pnpm resolver drift is refused", wrong_resolver)

    def loose_dependencies(root: Path) -> str:
        path = root / "pnpm-workspace.yaml"
        path.write_text(
            path.read_text(encoding="utf-8").replace(
                "engineStrict: true", "engineStrict: false"
            ),
            encoding="utf-8",
        )
        return "v24.19.0"

    case("non-strict dependency engines are refused", loose_dependencies)

    def wrong_types(root: Path) -> str:
        path = root / "apps" / "terminal" / "package.json"
        path.write_text(
            json.dumps({"devDependencies": {"@types/node": "26.2.0"}}),
            encoding="utf-8",
        )
        return "v24.19.0"

    case("Node typings off the pinned runtime are refused", wrong_types)

    def hardcoded_ci(root: Path) -> str:
        path = root / ".github" / "workflows" / "ci.yml"
        path.write_text(
            path.read_text(encoding="utf-8").replace(
                "node-version-file: .nvmrc", "node-version: 24.19.0"
            ),
            encoding="utf-8",
        )
        return "v24.19.0"

    case("a hardcoded workflow release is refused", hardcoded_ci)

    def unpinned_ci(root: Path) -> str:
        path = root / ".github" / "workflows" / "ci.yml"
        path.write_text(
            path.read_text(encoding="utf-8").replace(
                "with: { node-version-file: .nvmrc, cache: pnpm }",
                "with: { cache: pnpm }",
            ),
            encoding="utf-8",
        )
        return "v24.19.0"

    case("an unpinned setup-node step is refused", unpinned_ci)

    def comment_only_pin(root: Path) -> str:
        path = root / ".github" / "workflows" / "ci.yml"
        path.write_text(
            path.read_text(encoding="utf-8").replace(
                "with: { node-version-file: .nvmrc, cache: pnpm }",
                "with: { cache: pnpm } # node-version-file: .nvmrc",
            ),
            encoding="utf-8",
        )
        return "v24.19.0"

    case("a comment cannot impersonate a workflow pin", comment_only_pin)

    failures = 0
    with tempfile.TemporaryDirectory(prefix="node-contract-") as temporary:
        happy = Path(temporary) / "happy"
        write_fixture(happy)
        try:
            pin, workflows = validate_repository(happy, "v24.19.0")
            passed = pin == "24.19.0" and len(workflows) == 1
        except ContractError:
            passed = False
        print(f"  {'ok  ' if passed else 'FAIL'}  exact repository contract passes")
        failures += not passed

        for index, (label, mutate) in enumerate(cases):
            root = Path(temporary) / f"case-{index}"
            write_fixture(root)
            running = mutate(root)
            try:
                validate_repository(root, running)
            except ContractError:
                passed = True
            else:
                passed = False
            print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
            failures += not passed

    for label, value in (
        ("an LTS alias pin is refused", "lts/krypton"),
        ("a suffixed version is refused", "24.19.0-extra"),
    ):
        try:
            parse(value)
        except ContractError:
            passed = True
        else:
            passed = False
        print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
        failures += not passed

    total = len(cases) + 3
    if failures:
        print(f"\ncheck-node-version self-test: {failures}/{total} case(s) FAILED")
        return 1
    print(f"\ncheck-node-version self-test: {total} cases passed")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if sys.argv[1:]:
        print("usage: check-node-version.py [--self-test]", file=sys.stderr)
        return 2

    try:
        running = running_node()
        pin, workflows = validate_repository(ROOT, running)
    except (ContractError, RuntimeError) as exc:
        print(f"check-node-version: REFUSED — {exc}", file=sys.stderr)
        print(
            "\n  nvm and fnm read .nvmrc directly:  nvm use\n"
            "  Corepack then selects packageManager: corepack enable",
            file=sys.stderr,
        )
        return 1

    print(
        f"node {running} exactly matches .nvmrc ({pin}); "
        f"package, pnpm and {len(workflows)} workflow pin(s) agree"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
