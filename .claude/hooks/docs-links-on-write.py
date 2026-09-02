#!/usr/bin/env python3
"""Report broken relative Markdown links after Claude changes documentation.

This is deliberately Python rather than a shell pipeline. Claude invokes it
through an exec-form launcher so the same hook body is available to the Bash and
PowerShell tool surfaces. It mirrors the root documentation checker but stays
dependency-free so a hook can run before workspace dependencies are installed.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

SHELL_TOOLS = frozenset({"Bash", "PowerShell", "Monitor"})
MUTATING_SHELL = re.compile(
    r"(?:^|\s)(?:[12&]?>>?|tee|touch|cp|mv|rm|--output(?:=|\s)|"
    r"sed\s+[^\n;|]*-[A-Za-z]*i|"
    r"perl\s+[^\n;|]*-[A-Za-z]*i|Set-Content|Add-Content|Out-File|"
    r"New-Item|Copy-Item|Move-Item|Remove-Item)(?:\s|$)",
    re.IGNORECASE,
)


class DocsHookOperationalError(RuntimeError):
    """A tooling or filesystem failure that prevents a reliable link check."""


def visible_warning(message: str) -> None:
    """Send fail-open diagnostics through the hook's visible stdout channel."""
    print(json.dumps({"systemMessage": message}))


def repo_root(cwd: str) -> Path:
    try:
        done = subprocess.run(
            ["git", "-C", cwd, "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=5,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise DocsHookOperationalError(f"git rev-parse could not run: {exc}") from exc
    root = done.stdout.strip()
    if done.returncode != 0 or not root:
        raise DocsHookOperationalError(
            f"git rev-parse could not resolve the repository (exit {done.returncode})"
        )
    return Path(root)


def changed_markdown(payload: dict[str, object]) -> bool:
    event = payload.get("hook_event_name")
    tool_name = payload.get("tool_name")
    tool_input = payload.get("tool_input")

    if event == "FileChanged":
        raw = payload.get("file_path")
    elif isinstance(tool_input, dict):
        raw = tool_input.get("file_path") or tool_input.get("notebook_path")
    else:
        raw = None

    if isinstance(raw, str):
        # Every Markdown file, not only docs/. The five root documents carry the
        # table that sends a reader into the documentation set; they were
        # outside this hook and outside the canonical checker alike.
        return raw.replace("\\", "/").casefold().endswith(".md")

    if tool_name in SHELL_TOOLS and isinstance(tool_input, dict):
        command = tool_input.get("command")
        if isinstance(command, str):
            normalized = command.replace("\\", "/")
            return (
                ".md" in normalized.casefold()
                and MUTATING_SHELL.search(command) is not None
            )
    return False


def broken_links(root: Path) -> list[tuple[Path, str]]:
    """Run the same parser as the canonical gate against the live worktree."""
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
        raise DocsHookOperationalError(f"canonical link checker could not run: {exc}") from exc
    if result.returncode == 0:
        return []
    if result.returncode != 1:
        detail = (result.stderr or result.stdout).strip()
        raise DocsHookOperationalError(
            f"canonical link checker was inconclusive: {detail or result.returncode}"
        )

    broken: list[tuple[Path, str]] = []
    for line in result.stdout.splitlines():
        if not line.startswith("BROKEN  "):
            continue
        source, separator, target = line.removeprefix("BROKEN  ").partition("  ->  ")
        if not separator:
            raise DocsHookOperationalError("canonical checker returned malformed output")
        broken.append((Path(source), target))
    if not broken:
        raise DocsHookOperationalError("canonical checker failed without a finding")
    return broken


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (ValueError, OSError) as exc:
        visible_warning(f"Documentation-link hook received malformed input; allowing: {exc}")
        return 0
    if not isinstance(payload, dict):
        visible_warning("Documentation-link hook payload is not an object; allowing.")
        return 0
    if not changed_markdown(payload):
        return 0

    cwd = payload.get("cwd")
    root = repo_root(cwd if isinstance(cwd, str) else str(Path.cwd()))

    findings = broken_links(root)
    if not findings:
        return 0
    for document, target in findings:
        print(f"BROKEN  {document.as_posix()}  ->  {target}", file=sys.stderr)
    print(
        "A documentation link no longer resolves. Fix it now; just lint and CI "
        "run the canonical checker too.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001
        visible_warning(f"Documentation-link hook error; allowing the tool call: {exc}")
        sys.exit(0)
