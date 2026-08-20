#!/usr/bin/env python3
"""PreToolUse guard: refuse writes to files this project treats as immutable.

Two rules, both from docs/implementation/01-conventions.md:

  1. §9 — a migration that is already committed is never edited. Not for a typo.
     Other databases have already applied it; the fix is the next migration.
  2. docs/plan/** are source documents — inputs to the implementation set, not
     working documents. Corrections belong in docs/implementation/.

One script covers every write surface so a Bash-matcher hook costs a single
interpreter start-up:

  Edit / Write / MultiEdit / NotebookEdit  exact, on tool_input.file_path
  Bash                                     best-effort: a protected path standing
                                           next to a write operator

The Bash arm is defence in depth, not a proof. A shell can reach any file, and a
sufficiently creative command line will get past it. Review and CI are the
backstop; this stops the accident, not the determined workaround.

Fails open. A bug in this guard must never block every edit in the repository —
so any internal error exits 0 with a warning rather than denying the call.

Negative-tested by ./test-protect-immutable.sh — run it after any change here.
A guard nobody has seen fail is a guard nobody should trust.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

WRITE_TOOLS = frozenset({"Edit", "Write", "MultiEdit", "NotebookEdit"})

MIGRATION = re.compile(r"(?:^|/)migrations/[^/]+\.sql$")
PLANS_PREFIX = "docs/plan/"

# A protected path standing immediately after a redirect, or piped into tee.
def _redirect_into(token: str) -> re.Pattern[str]:
    return re.compile(r"(?:>>?|\btee\b(?:\s+-a)?)\s*['\"]?" + re.escape(token))

# Commands that rewrite or remove a named file in place. Deliberately excludes
# `cp` and `ed`: both produce more false denials here than they prevent accidents.
IN_PLACE = re.compile(
    r"\bsed\s+-[a-zA-Z]*i"
    r"|\bperl\s+-[a-zA-Z]*i"
    r"|\bgit\s+(?:rm|mv)\b"
    r"|\b(?:rm|mv|truncate|patch|shred|dd)\b"
)

# Path-shaped tokens worth testing: any .sql file, and anything under docs/plan.
# Bare *.sql names matter because `cd .../migrations && sed -i '' 0001_init.sql`
# never mentions the directory in the same token.
CANDIDATE = re.compile(r"[A-Za-z0-9_./-]*(?:\.sql|docs/plan/[A-Za-z0-9_./-]+)")

# Cheap relevance test, so an unrelated Bash call never pays for a git subprocess.
RELEVANT = ("migrations", ".sql", "docs/plan")

# A compound command is many commands. Scanning it as one blob lets a write verb
# quoted in one place — a commit message, a comment — implicate a path named
# anywhere else in the same call, so each arm is matched per segment instead.
SEGMENT = re.compile(r"\n|;|&&|\|\||\||\$\(|`")


def repo_root(cwd: str) -> Path | None:
    try:
        done = subprocess.run(
            ["git", "-C", cwd, "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, timeout=5,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return Path(done.stdout.strip()) if done.returncode == 0 else None


def to_relative(root: Path, raw: str, cwd: str) -> str | None:
    try:
        target = Path(raw)
        if not target.is_absolute():
            target = Path(cwd) / target
        return os.path.relpath(os.path.normpath(target), root).replace(os.sep, "/")
    except (OSError, ValueError):
        return None


def is_committed(root: Path, relative: str) -> bool:
    """True when the path exists in HEAD — committed, not merely staged."""
    try:
        done = subprocess.run(
            ["git", "-C", str(root), "cat-file", "-e", f"HEAD:{relative}"],
            capture_output=True, timeout=5,
        )
    except (OSError, subprocess.SubprocessError):
        return False
    return done.returncode == 0


def committed_migration_basenames(root: Path) -> set[str]:
    """Basenames of every migration in HEAD, for tokens that carry no directory."""
    try:
        done = subprocess.run(
            ["git", "-C", str(root), "ls-tree", "-r", "--name-only", "HEAD"],
            capture_output=True, text=True, timeout=5,
        )
    except (OSError, subprocess.SubprocessError):
        return set()
    if done.returncode != 0:
        return set()
    return {
        line.rsplit("/", 1)[-1]
        for line in done.stdout.splitlines()
        if MIGRATION.search(line)
    }


def refusal(root: Path, relative: str) -> str | None:
    """The reason this path may not be written, or None if it may."""
    if relative.startswith(".."):
        return None  # outside the repository; not ours to police
    if relative.startswith(PLANS_PREFIX):
        return (
            f"BLOCKED: {relative} is a source document.\n"
            "docs/plan/** are inputs to the implementation set, never working documents "
            "(CLAUDE.md, 'The plan'). If the plan is wrong, record the correction in "
            "docs/implementation/ — that set is the plan of record."
        )
    if MIGRATION.search(relative) and is_committed(root, relative):
        return migration_refusal(relative)
    return None


def migration_refusal(display: str) -> str:
    return (
        f"BLOCKED: {display} is a committed migration.\n"
        "Migrations are forward-only and are never edited once committed — not for a "
        "typo, not 'it hasn't shipped yet' (01-conventions.md §9). Databases in the "
        "field have already applied this file.\n"
        "Write the next NNNN_short_name.sql instead, append it to MIGRATIONS in "
        "crates/pos-db/src/lib.rs, and mirror it in apps/server/migrations/."
    )


def check_file_write(root: Path, cwd: str, tool_input: dict[str, object]) -> str | None:
    raw = tool_input.get("file_path") or tool_input.get("notebook_path")
    if not isinstance(raw, str) or not raw:
        return None
    relative = to_relative(root, raw, cwd)
    return refusal(root, relative) if relative else None


def check_bash(root: Path, cwd: str, command: str) -> str | None:
    basenames: set[str] | None = None
    for segment in SEGMENT.split(command):
        in_place = IN_PLACE.search(segment) is not None
        for token in {m.group(0) for m in CANDIDATE.finditer(segment)}:
            if not (in_place or _redirect_into(token).search(segment)):
                continue  # this segment only reads the token
            relative = to_relative(root, token, cwd)
            reason = refusal(root, relative) if relative else None
            if reason is None and token.endswith(".sql") and "/" not in token:
                # A bare filename, reached after a `cd` into its directory.
                if basenames is None:
                    basenames = committed_migration_basenames(root)
                if token in basenames:
                    reason = migration_refusal(token)
            if reason:
                return f"{reason}\n(Detected in a shell command that writes to it.)"
    return None


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return 0  # not a payload we understand; fail open

    tool_name = payload.get("tool_name")
    tool_input = payload.get("tool_input")
    cwd = payload.get("cwd") or os.getcwd()
    if not isinstance(tool_input, dict) or not isinstance(cwd, str):
        return 0

    # Decide irrelevance without touching git: this hook is on the Bash matcher,
    # so it runs on every shell call and must cost almost nothing in the common case.
    if tool_name in WRITE_TOOLS:
        subject = tool_input.get("file_path") or tool_input.get("notebook_path")
    elif tool_name == "Bash":
        subject = tool_input.get("command")
    else:
        return 0
    if not isinstance(subject, str) or not any(hint in subject for hint in RELEVANT):
        return 0

    root = repo_root(cwd)
    if root is None:
        return 0  # not a git repository; "committed" has no meaning here

    if tool_name == "Bash":
        reason = check_bash(root, cwd, subject)
    else:
        reason = check_file_write(root, cwd, tool_input)

    if reason:
        print(reason, file=sys.stderr)
        return 2  # PreToolUse: deny, and show stderr to Claude
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001 — fail open, never brick the session
        print(f"protect-immutable.py: guard error, allowing: {exc}", file=sys.stderr)
        sys.exit(0)
