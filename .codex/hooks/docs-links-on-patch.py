#!/usr/bin/env python3
"""Codex PostToolUse documentation-link guard.

When a successful ``apply_patch`` touches any Markdown file, validate relative
targets throughout that worktree. The configured Windows path
uses this Python-only implementation without requiring Git Bash; native hook
dispatch still depends on the Codex client, so Git and CI remain backstops.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

PATCH_PATH = re.compile(
    r"^\*\*\* (?:(?:Add|Update|Delete) File:|Move to:)\s*(.+?)\s*$"
)


def fail_open(detail: str) -> int:
    """Warn Codex through the supported hook channel, then allow the tool call."""
    print(
        json.dumps(
            {
                "systemMessage": (
                    "WARNING: documentation-link hook failed open: "
                    f"{detail}. The lint and CI checks remain the backstops."
                )
            }
        )
    )
    return 0


def repo_root(cwd: str) -> Path | None:
    try:
        done = subprocess.run(
            ["git", "-C", cwd, "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=5,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return Path(done.stdout.strip()).resolve() if done.returncode == 0 else None


def relative_path(root: Path, cwd: str, raw: str) -> str | None:
    try:
        # A Codex client on Windows may put backslashes in patch headers. Treat
        # them as separators even when this test or hook runs on POSIX.
        target = Path(raw.replace("\\", "/"))
        if not target.is_absolute():
            target = Path(cwd) / target
        return os.path.relpath(target.resolve(), root).replace(os.sep, "/")
    except (OSError, ValueError):
        return None


def first_markdown(command: str, root: Path, cwd: str) -> str | None:
    for line in command.splitlines():
        match = PATCH_PATH.match(line)
        if not match:
            continue
        raw = match.group(1)
        relative = relative_path(root, cwd, raw)
        folded = relative.casefold() if relative else ""
        if folded.endswith(".md"):
            return raw
    return None


def broken_doc_links(root: Path) -> list[tuple[str, str]]:
    """Run the canonical parser against tracked and newly written documents."""
    checker = Path(__file__).resolve().parents[2] / "scripts" / "check-doc-links.py"
    try:
        result = subprocess.run(
            [
                sys.executable,
                str(checker),
                "--root",
                str(root),
                "--working-tree",
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=25,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise RuntimeError(f"canonical link checker could not run: {exc}") from exc
    if result.returncode == 0:
        return []
    if result.returncode != 1:
        detail = (result.stderr or result.stdout).strip()
        raise RuntimeError(
            f"canonical link checker was inconclusive: {detail or result.returncode}"
        )

    broken: list[tuple[str, str]] = []
    for line in result.stdout.splitlines():
        if not line.startswith("BROKEN  "):
            continue
        source, separator, target = line.removeprefix("BROKEN  ").partition("  ->  ")
        if not separator:
            raise RuntimeError("canonical checker returned malformed output")
        broken.append((source, target))
    if not broken:
        raise RuntimeError("canonical checker failed without a finding")
    return broken


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except ValueError as exc:
        return fail_open(f"invalid JSON input ({exc})")

    if payload.get("tool_name") != "apply_patch":
        return 0
    tool_input = payload.get("tool_input")
    cwd = payload.get("cwd") or os.getcwd()
    if not isinstance(tool_input, dict) or not isinstance(cwd, str):
        return fail_open("unexpected tool_input or cwd shape")
    command = tool_input.get("command")
    if not isinstance(command, str):
        return fail_open("tool_input.command is missing or is not text")

    root = repo_root(cwd)
    if root is None:
        return fail_open("the active git worktree could not be resolved")
    if first_markdown(command, root, cwd) is None:
        return 0

    broken = broken_doc_links(root)
    if not broken:
        return 0
    for source, target in broken:
        print(f"BROKEN  {source}  ->  {target}", file=sys.stderr)
    print("documentation link check FAILED", file=sys.stderr)
    print(
        "A documentation link no longer resolves. "
        "just lint and CI run this too — fix it now.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001 - reporting must not brick Codex
        sys.exit(fail_open(f"internal error ({exc})"))
