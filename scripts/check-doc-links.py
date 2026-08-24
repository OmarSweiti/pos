#!/usr/bin/env python3
"""Fail when a maintained Markdown document links to a missing local path.

The parser is intentionally dependency-free because setup and agent hooks use
it before workspace dependencies exist. It covers inline links (including
titles, angle-bracket destinations, escaped characters and balanced
parentheses) plus reference definitions, while ignoring fenced and inline code.
It validates paths, not heading anchors.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from urllib.parse import unquote

ALLOWED_BROKEN = frozenset(
    {("docs/plan/phase-0-setup-guide.md", "../justfile")}
)
PRUNED_DIRECTORIES = frozenset(
    {
        ".git",
        ".pnpm-store",
        "__pycache__",
        "dist",
        "node_modules",
        "target",
        "worktrees",
    }
)
FENCE = re.compile(r"^[ ]{0,3}(`{3,}|~{3,})(.*)$")
REFERENCE_DEFINITION = re.compile(
    r"(?m)^[ \t]{0,3}\[([^\]\n]+)\]:[ \t]*(?:<([^>\n]+)>|((?:\\.|[^\s])+))"
)
REFERENCE_USE = re.compile(r"(?<!!)\[([^\]\n]+)\]\[([^\]\n]*)\]")
URI_SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
MARKDOWN_ESCAPE = re.compile(r"\\([!\"#$%&'()*+,\-./:;<=>?@[\\\]^_`{|}~ ])")


class OperationalError(RuntimeError):
    """The checker could not establish a reliable result."""


def _without_fenced_code(markdown: str) -> str:
    output: list[str] = []
    fence_char = ""
    fence_length = 0

    for line in markdown.splitlines(keepends=True):
        plain = line.rstrip("\r\n")
        if fence_char:
            closing = re.match(
                rf"^[ ]{{0,3}}{re.escape(fence_char)}{{{fence_length},}}[ \t]*$",
                plain,
            )
            if closing:
                fence_char = ""
                fence_length = 0
            output.append("\n" if line.endswith(("\n", "\r")) else "")
            continue

        opening = FENCE.match(plain)
        if opening:
            marker, info = opening.groups()
            # A backtick fence cannot have a backtick in its info string.
            if marker[0] == "~" or "`" not in info:
                fence_char = marker[0]
                fence_length = len(marker)
                output.append("\n" if line.endswith(("\n", "\r")) else "")
                continue
        output.append(line)

    return "".join(output)


def _without_inline_code(markdown: str) -> str:
    output: list[str] = []
    index = 0
    while index < len(markdown):
        if markdown[index] != "`":
            output.append(markdown[index])
            index += 1
            continue

        end_run = index
        while end_run < len(markdown) and markdown[end_run] == "`":
            end_run += 1
        delimiter = markdown[index:end_run]
        closing = markdown.find(delimiter, end_run)
        if closing < 0:
            output.append(delimiter)
            index = end_run
            continue

        hidden = markdown[index : closing + len(delimiter)]
        output.extend("\n" if character == "\n" else " " for character in hidden)
        index = closing + len(delimiter)
    return "".join(output)


def prose(markdown: str) -> str:
    """Return Markdown with regions that cannot contain links blanked out."""
    without_comments = re.sub(
        r"<!--[\s\S]*?-->",
        lambda match: re.sub(r"[^\n]", " ", match.group(0)),
        markdown,
    )
    return _without_inline_code(_without_fenced_code(without_comments))


def _not_escaped(text: str, index: int) -> bool:
    backslashes = 0
    cursor = index - 1
    while cursor >= 0 and text[cursor] == "\\":
        backslashes += 1
        cursor -= 1
    return backslashes % 2 == 0


def _inline_destinations(text: str) -> list[str]:
    targets: list[str] = []
    cursor = 0
    while True:
        close_label = text.find("](", cursor)
        if close_label < 0:
            break
        cursor = close_label + 2
        if not _not_escaped(text, close_label):
            continue

        while cursor < len(text) and text[cursor] in " \t\r\n":
            cursor += 1
        if cursor >= len(text):
            break

        if text[cursor] == "<":
            cursor += 1
            start = cursor
            while cursor < len(text):
                if text[cursor] == ">" and _not_escaped(text, cursor):
                    targets.append(text[start:cursor])
                    cursor += 1
                    break
                cursor += 1
            continue

        start = cursor
        depth = 0
        while cursor < len(text):
            character = text[cursor]
            if character == "\\" and cursor + 1 < len(text):
                cursor += 2
                continue
            if character == "(":
                depth += 1
            elif character == ")":
                if depth == 0:
                    break
                depth -= 1
            elif character.isspace() and depth == 0:
                break
            cursor += 1
        targets.append(text[start:cursor])
    return targets


def link_targets(markdown: str) -> set[str]:
    """Extract inline destinations and reference-definition destinations."""
    visible = prose(markdown)
    targets = set(_inline_destinations(visible))
    for match in REFERENCE_DEFINITION.finditer(visible):
        targets.add(match.group(2) or match.group(3))
    return {target for target in targets if target is not None}


def undefined_references(markdown: str) -> set[str]:
    """Return explicit full/collapsed reference labels with no definition."""
    visible = prose(markdown)

    def normalized(label: str) -> str:
        return re.sub(r"\s+", " ", label.strip()).casefold()

    definitions = {
        normalized(match.group(1)) for match in REFERENCE_DEFINITION.finditer(visible)
    }
    missing: set[str] = set()
    for match in REFERENCE_USE.finditer(visible):
        if not _not_escaped(visible, match.start()):
            continue
        label = match.group(2) or match.group(1)
        if normalized(label) not in definitions:
            missing.add(label)
    return missing


def _local_target(raw_target: str) -> str | None:
    target = MARKDOWN_ESCAPE.sub(r"\1", raw_target.strip())
    if not target or target.startswith(("#", "//")) or URI_SCHEME.match(target):
        return None
    path = target.split("#", 1)[0].split("?", 1)[0]
    return unquote(path) or None


def _candidate(root: Path, source: Path, target: str) -> Path | None:
    base = root if target.startswith("/") else source.parent
    candidate = Path(os.path.normpath(base / target.lstrip("/")))
    try:
        candidate.relative_to(root)
    except ValueError:
        return None
    return candidate


def _exists_with_exact_case(root: Path, candidate: Path) -> bool:
    """Do not let a case-insensitive workstation hide a Linux-only break."""
    try:
        relative = candidate.relative_to(root)
    except ValueError:
        return False

    current = root
    for part in relative.parts:
        if part in {"", "."}:
            continue
        try:
            names = os.listdir(current)
        except OSError:
            return False
        if part not in names:
            return False
        current /= part
    return current.exists()


def broken_links(
    root: Path,
    sources: list[Path],
    allowed: frozenset[tuple[str, str]] = ALLOWED_BROKEN,
) -> list[tuple[str, str]]:
    findings: list[tuple[str, str]] = []
    for source in sorted(sources):
        try:
            body = source.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as exc:
            raise OperationalError(f"could not read {source}: {exc}") from exc
        display = source.relative_to(root).as_posix()
        for label in sorted(undefined_references(body)):
            findings.append((display, f"undefined reference label [{label}]"))
        for raw_target in sorted(link_targets(body)):
            target = _local_target(raw_target)
            if target is None or (display, raw_target) in allowed:
                continue
            candidate = _candidate(root, source, target)
            if candidate is None or not _exists_with_exact_case(root, candidate):
                findings.append((display, raw_target))
    return findings


def tracked_markdown(root: Path) -> list[Path]:
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z"],
            check=False,
            capture_output=True,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise OperationalError(f"git ls-files could not run: {exc}") from exc
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise OperationalError(f"git ls-files failed: {detail or result.returncode}")
    paths = [entry for entry in result.stdout.split(b"\0") if entry]
    documents = [
        root / os.fsdecode(entry)
        for entry in paths
        if os.fsdecode(entry).casefold().endswith(".md")
    ]
    if not documents:
        raise OperationalError("git listed no tracked Markdown files")
    return documents


def working_tree_markdown(root: Path) -> list[Path]:
    documents: list[Path] = []
    for directory, subdirectories, filenames in os.walk(root):
        subdirectories[:] = [
            name for name in subdirectories if name not in PRUNED_DIRECTORIES
        ]
        for filename in filenames:
            if filename.casefold().endswith(".md"):
                documents.append(Path(directory) / filename)
    return documents


def _allowlist_is_current(root: Path) -> None:
    for source_name, target in ALLOWED_BROKEN:
        source = root / source_name
        if not source.is_file():
            raise OperationalError(f"allowlisted document no longer exists: {source_name}")
        local = _local_target(target)
        candidate = _candidate(root, source, local or "")
        if candidate is not None and _exists_with_exact_case(root, candidate):
            raise OperationalError(
                f"allowlisted link now resolves; remove exception: {source_name} -> {target}"
            )


def self_test() -> bool:
    passed = 0
    failed = 0
    with tempfile.TemporaryDirectory(prefix="pos-doc-links-") as raw_root:
        root = Path(raw_root)
        (root / "sub").mkdir()
        (root / "space name.md").write_text("ok\n", encoding="utf-8")
        (root / "real.md").write_text("ok\n", encoding="utf-8")
        (root / "justfile").write_text("ok\n", encoding="utf-8")

        cases = [
            ("existing Markdown", "[ok](real.md)", False),
            ("missing Markdown", "[bad](missing.md)", True),
            ("existing extensionless path", "[ok](justfile)", False),
            ("missing extensionless path", "[bad](Makefile)", True),
            ("existing path plus anchor", "[ok](real.md#heading)", False),
            ("missing path plus anchor", "[bad](missing.md#heading)", True),
            ("external URL", "[web](https://example.com/missing.md)", False),
            ("same-document anchor", "[heading](#heading)", False),
            ("inline title", '[ok](real.md "title")', False),
            ("angle destination with spaces", "[ok](<space name.md>)", False),
            ("missing angle destination", "[bad](<missing name.md>)", True),
            ("reference destination", "[ok][id]\n\n[id]: real.md", False),
            ("missing reference destination", "[bad][id]\n\n[id]: missing.md", True),
            ("undefined reference label", "[bad][missing-id]", True),
            ("fenced example", "```md\n[example](missing.md)\n```", False),
            ("inline-code example", "`[example](missing.md)`", False),
            ("HTML comment", "<!-- [example](missing.md) -->", False),
            ("balanced parentheses", "[ok](real(ignored).md)", True),
            ("glob is not a resolvable link", "[bad](crates/*/Cargo.toml)", True),
            ("wrong filename case", "[bad](REAL.md)", True),
            ("repository-root path", "[ok](/real.md)", False),
        ]

        for label, markdown, expected_broken in cases:
            source = root / "case.md"
            source.write_text(markdown + "\n", encoding="utf-8")
            actual = bool(broken_links(root, [source], frozenset()))
            if actual == expected_broken:
                print(f"  ok    {label}")
                passed += 1
            else:
                print(
                    f"  FAIL  {label} (wanted broken={expected_broken}, got {actual})"
                )
                failed += 1

        parent = root / "sub" / "parent.md"
        parent.write_text("[up](../real.md)\n", encoding="utf-8")
        if not broken_links(root, [parent], frozenset()):
            print("  ok    parent-relative destination")
            passed += 1
        else:
            print("  FAIL  parent-relative destination")
            failed += 1

    print(f"\ndoc-links self-test: {passed} passed, {failed} failed")
    return failed == 0


def repository_root(cwd: Path) -> Path:
    try:
        result = subprocess.run(
            ["git", "-C", str(cwd), "rev-parse", "--show-toplevel"],
            check=False,
            capture_output=True,
            text=True,
            timeout=5,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise OperationalError(f"repository discovery failed: {exc}") from exc
    resolved = result.stdout.strip()
    if result.returncode != 0 or not resolved:
        raise OperationalError("repository discovery failed")
    return Path(resolved).resolve()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path)
    parser.add_argument("--working-tree", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return 0 if self_test() else 1

    try:
        root = args.root.resolve() if args.root else repository_root(Path.cwd())
        sources = (
            working_tree_markdown(root) if args.working_tree else tracked_markdown(root)
        )
        if not args.working_tree:
            _allowlist_is_current(root)
        findings = broken_links(root, sources)
    except OperationalError as exc:
        print(f"documentation link check could not run: {exc}", file=sys.stderr)
        return 2

    if findings:
        for source, target in findings:
            print(f"BROKEN  {source}  ->  {target}")
        print("documentation link check FAILED")
        return 1
    print(f"documentation links OK ({len(sources)} Markdown files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
