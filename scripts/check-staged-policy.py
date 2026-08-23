#!/usr/bin/env python3
"""Validate staged paths and blobs without consulting the working tree."""

from __future__ import annotations

import json
import os
import subprocess
import sys


MAX_BLOB_BYTES = 2_000_000
SAFE_ENV_TEMPLATE = "apps/server/.env.example"
REGULAR_BLOB_MODES = frozenset({"100644", "100755"})
MIGRATION_PREFIXES = (
    "crates/pos-db/migrations/",
    "apps/server/migrations/",
)


class GitFailure(RuntimeError):
    pass


def git(*args: str, accepted: tuple[int, ...] = (0,)) -> subprocess.CompletedProcess[bytes]:
    completed = subprocess.run(
        ["git", *args],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode not in accepted:
        detail = completed.stderr.decode("utf-8", "replace").strip()
        raise GitFailure(f"git {' '.join(args)} failed ({completed.returncode}): {detail}")
    return completed


def display(path: str) -> str:
    # Escape control characters so a hostile filename cannot forge another
    # diagnostic line in terminal or CI output.
    return json.dumps(path, ensure_ascii=False)


def parse_name_status(raw: bytes) -> list[tuple[str, str]]:
    fields = raw.split(b"\0")
    if fields and fields[-1] == b"":
        fields.pop()
    if len(fields) % 2:
        raise GitFailure("git returned a malformed NUL-delimited staged-path list")
    changes: list[tuple[str, str]] = []
    for offset in range(0, len(fields), 2):
        status = fields[offset].decode("ascii", "strict")
        path = os.fsdecode(fields[offset + 1])
        if len(status) != 1 or status not in "ACMDT":
            raise GitFailure(f"git returned an unexpected staged status {status!r}")
        changes.append((status, path))
    return changes


def sensitive_reason(path: str) -> str | None:
    normalized = path.replace("\\", "/")
    parts = [part.casefold() for part in normalized.split("/")]
    name = parts[-1]

    is_environment_file = name == ".env" or name.startswith(".env.")
    # Git's index paths use forward slashes. Keep the exception byte-for-byte
    # canonical so a parallel case variant or literal backslash path cannot
    # impersonate the one reviewed, tracked template.
    if is_environment_file and path != SAFE_ENV_TEMPLATE:
        return "environment files can contain credentials"
    if name.endswith((".db", ".sqlite", ".sqlite3", "-wal", "-shm", "-journal")):
        return "database files and SQLite sidecars are runtime data, not source"
    if name.endswith((".pem", ".key", ".p12", ".pfx", ".jks", ".keystore")) or name in {
        "id_rsa",
        "id_dsa",
        "id_ecdsa",
        "id_ed25519",
    }:
        return "private-key material must never enter Git"
    if name in {".npmrc", ".netrc", "_netrc", ".git-credentials", ".pypirc"}:
        return "this tool-managed file commonly stores credentials"
    if len(parts) >= 2 and parts[-2:] in (
        [".cargo", "credentials"],
        [".cargo", "credentials.toml"],
        [".docker", "config.json"],
    ):
        return "this tool-managed file commonly stores registry credentials"
    if normalized.casefold() == ".claude/settings.local.json":
        return "machine-local tool permissions do not belong in Git"
    if normalized.casefold().startswith("apps/terminal/src-tauri/gen/schemas/"):
        return "Tauri schemas are generated at build time"
    generated_tree = any(
        part in {"target", "dist", "node_modules", "__pycache__"} for part in parts
    )
    if generated_tree or name.endswith((".pyc", ".pyo")):
        return "build artifacts and dependency trees do not belong in Git"
    return None


def index_blob_metadata(path: str) -> tuple[str, int]:
    listing = git("ls-files", "--stage", "-z", "--", path).stdout
    records = [record for record in listing.split(b"\0") if record]
    if len(records) != 1 or b"\t" not in records[0]:
        raise GitFailure(f"staged blob metadata is ambiguous for {display(path)}")
    metadata, listed_path = records[0].split(b"\t", 1)
    fields = metadata.split()
    if len(fields) != 3 or fields[2] != b"0" or os.fsdecode(listed_path) != path:
        raise GitFailure(f"staged blob metadata is malformed for {display(path)}")
    mode = fields[0].decode("ascii", "strict")
    object_id = fields[1].decode("ascii", "strict")
    output = git("cat-file", "-s", object_id).stdout.strip()
    try:
        return mode, int(output)
    except ValueError as exc:
        raise GitFailure(
            f"git returned a non-numeric staged blob size for {display(path)}"
        ) from exc


def exists_in_head(path: str, head_exists: bool) -> bool:
    if not head_exists:
        return False
    # No `ls-tree | grep -q`: Git writes its complete result before we inspect
    # it, so pipefail/SIGPIPE timing cannot turn a committed migration into an
    # apparently new one.
    return bool(git("ls-tree", "-z", "--full-tree", "HEAD", "--", path).stdout)


def main() -> int:
    try:
        raw = git(
            "diff",
            "--cached",
            "--no-renames",
            "--name-status",
            "-z",
            "--diff-filter=ACMDT",
        ).stdout
        changes = parse_name_status(raw)
        if not changes:
            return 0

        head_status = git("rev-parse", "--verify", "--quiet", "HEAD^{commit}", accepted=(0, 1))
        head_exists = head_status.returncode == 0
        refused = False

        for status, path in changes:
            shown = display(path)
            normalized = path.replace("\\", "/")
            policy_path = normalized.casefold()

            if policy_path == "docs/plan" or policy_path.startswith("docs/plan/"):
                print(f"pre-commit: REFUSED — {shown} is read-only source-plan material.")
                refused = True

            is_migration = any(policy_path.startswith(prefix) for prefix in MIGRATION_PREFIXES)
            if is_migration:
                # Git paths are case-sensitive even when the developer's volume
                # is not. A parallel CRATES/... tree or an upper-cased SQL name
                # must not evade the exact-path HEAD lookup below.
                if path != policy_path:
                    print(
                        f"pre-commit: REFUSED — {shown} uses non-canonical migration "
                        "path casing or separators."
                    )
                    refused = True
                elif exists_in_head(path, head_exists):
                    action = "deletes" if status == "D" else "changes"
                    print(
                        f"pre-commit: REFUSED — {shown} {action} a committed migration; "
                        "add the next forward-only migration."
                    )
                    refused = True

            if status == "D":
                continue

            reason = sensitive_reason(path)
            if reason is not None:
                print(f"pre-commit: REFUSED — {shown}: {reason}.")
                refused = True

            mode, size = index_blob_metadata(path)
            if is_migration and mode not in REGULAR_BLOB_MODES:
                print(
                    f"pre-commit: REFUSED — {shown} is staged with Git mode {mode}; "
                    "migrations must be regular files, never symlinks or submodules."
                )
                refused = True
            if size > MAX_BLOB_BYTES:
                print(
                    f"pre-commit: REFUSED — staged blob {shown} is {size / 1_000_000:.2f} MB; "
                    "Git keeps it forever."
                )
                refused = True

        if refused:
            print("\nNothing was committed. Unstage the rejected path and fix the cause.")
            return 1
        return 0
    except (GitFailure, OSError, UnicodeError) as exc:
        print(f"pre-commit: ERROR — policy could not inspect the index: {exc}", file=sys.stderr)
        print("pre-commit: refusing closed; repair Git/index access and retry.", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
