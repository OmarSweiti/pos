#!/usr/bin/env python3
"""Dependency-free contract test for repository-scoped Codex skills."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SKILLS_DIR = ROOT / ".agents/skills"
CLAUDE_SKILLS_DIR = ROOT / ".claude/skills"
NAME = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
PLACEHOLDER = re.compile(r"\b(?:TODO|FIXME|TBD)\b")
REQUIRED_CONTRACTS = {
    "add-migration": (
        "git ls-tree -r --name-only HEAD",
        "never edit, delete, rename, or",
        "append the file to `MIGRATIONS`",
        "exact,\n  ordered parity",
        "update `REGISTER_LOCAL`",
        "`-- Server-only: <reason>`",
        "uniquely named scratch database",
        "just lint",
        "just test",
        "just guards",
    ),
    "verify-schema": (
        "exact ordered parity",
        "duplicates, nonexistent entries, numbering gaps",
        "documentation/runtime\n   parity",
        "Mapping proves declaration coverage, not semantic equivalence",
        "uniquely named scratch database",
        "./scripts/verify-schema.py --self-test",
        "./scripts/verify-pg-migrations.py --self-test",
        "just guards",
    ),
}


def frontmatter(path: Path) -> tuple[dict[str, str], str]:
    source = path.read_text(encoding="utf-8")
    lines = source.splitlines()
    if not lines or lines[0] != "---":
        raise ValueError("must start with YAML frontmatter")
    try:
        closing = lines.index("---", 1)
    except ValueError as error:
        raise ValueError("frontmatter has no closing delimiter") from error

    metadata: dict[str, str] = {}
    for number, line in enumerate(lines[1:closing], start=2):
        if not line or line.lstrip().startswith("#"):
            continue
        if ":" not in line:
            raise ValueError(f"frontmatter line {number} is not key: value")
        key, value = (part.strip() for part in line.split(":", 1))
        if not key or not value:
            raise ValueError(f"frontmatter line {number} has an empty key or value")
        if key in metadata:
            raise ValueError(f"frontmatter key {key!r} is duplicated")
        metadata[key] = value
    return metadata, "\n".join(lines[closing + 1 :]).strip()


def validate(path: Path) -> list[str]:
    errors: list[str] = []
    relative = path.relative_to(ROOT)
    try:
        metadata, body = frontmatter(path)
    except (OSError, UnicodeError, ValueError) as error:
        return [f"{relative}: {error}"]

    if set(metadata) != {"name", "description"}:
        errors.append(f"{relative}: frontmatter must contain only name and description")
        return errors

    name = metadata["name"]
    description = metadata["description"]
    if not NAME.fullmatch(name) or len(name) > 64:
        errors.append(f"{relative}: name must be 1-64 lowercase letters, digits, or hyphens")
    if name != path.parent.name:
        errors.append(f"{relative}: name must match its skill directory")
    if not 1 <= len(description) <= 1024 or "<" in description or ">" in description:
        errors.append(f"{relative}: description must be 1-1024 characters without angle brackets")
    if not body or not body.startswith("# "):
        errors.append(f"{relative}: body must start with a level-one heading")
    if PLACEHOLDER.search(body):
        errors.append(f"{relative}: unresolved placeholder remains in the skill body")
    return errors


def normalized_body(name: str, body: str) -> str:
    """Remove only the intentional Claude/Codex sandbox wording difference."""
    starts = (
        "On supported hosts, the Claude sandbox",
        "Repository policy removes inherited `$DATABASE_URL`",
    )
    start = next((body.find(marker) for marker in starts if marker in body), -1)
    if start < 0:
        raise ValueError("client-specific database execution boundary is missing")

    if name == "add-migration":
        end_marker = "\n\nRun `just guards`"
    elif name == "verify-schema":
        end_marker = "\n6. If either verifier"
    else:
        raise ValueError(f"no client-boundary normalization is defined for {name}")
    end = body.find(end_marker, start)
    if end < 0:
        raise ValueError("client-specific database execution boundary is unterminated")
    return body[:start] + "<CLIENT-SPECIFIC DATABASE BOUNDARY>" + body[end:]


def missing_contracts(name: str, body: str) -> list[str]:
    return [phrase for phrase in REQUIRED_CONTRACTS.get(name, ()) if phrase not in body]


def cross_client_errors(codex_path: Path) -> list[str]:
    errors: list[str] = []
    name = codex_path.parent.name
    claude_path = CLAUDE_SKILLS_DIR / name / "SKILL.md"
    try:
        codex_metadata, codex_body = frontmatter(codex_path)
        claude_metadata, claude_body = frontmatter(claude_path)
    except (OSError, UnicodeError, ValueError) as error:
        return [f"{claude_path.relative_to(ROOT)}: {error}"]

    if codex_metadata != claude_metadata:
        errors.append(f"{name}: Claude and Codex skill metadata must match exactly")
    for client, body in (("Codex", codex_body), ("Claude", claude_body)):
        for phrase in missing_contracts(name, body):
            errors.append(
                f"{name}: {client} skill lost required safety contract {phrase!r}"
            )
    try:
        if normalized_body(name, codex_body) != normalized_body(name, claude_body):
            errors.append(
                f"{name}: Claude and Codex instructions drift outside the reviewed "
                "client-specific database boundary"
            )
    except ValueError as error:
        errors.append(f"{name}: {error}")
    return errors


def negative_fixture_errors(path: Path) -> list[str]:
    """Prove contract deletion and cross-client drift are actually detected."""
    name = path.parent.name
    try:
        _metadata, body = frontmatter(path)
    except (OSError, UnicodeError, ValueError) as error:
        return [f"{path.relative_to(ROOT)}: cannot build negative fixture ({error})"]
    required = REQUIRED_CONTRACTS.get(name, ())
    if not required:
        return [f"{name}: no required safety contracts are defined"]
    removed = body.replace(required[0], "", 1)
    errors: list[str] = []
    if not missing_contracts(name, removed):
        errors.append(f"{name}: contract-deletion negative fixture did not fail")
    if normalized_body(name, body) == normalized_body(name, body + "\npolicy drift"):
        errors.append(f"{name}: parity-drift negative fixture did not fail")
    return errors


def main() -> int:
    paths = sorted(SKILLS_DIR.glob("*/SKILL.md"))
    errors = [] if paths else [".agents/skills: no repository skills found"]
    for path in paths:
        errors.extend(validate(path))
        errors.extend(cross_client_errors(path))
        errors.extend(negative_fixture_errors(path))
    if errors:
        for error in errors:
            print(f"codex-skills: FAIL: {error}")
        return 1
    print(
        f"codex-skills: {len(paths)} metadata, safety, and Claude/Codex parity "
        "contracts passed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
