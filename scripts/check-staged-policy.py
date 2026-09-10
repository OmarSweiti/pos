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
    # stdin is DEVNULL, not inherited. Under pre-commit that is merely tidy;
    # under pre-push our stdin is Git's ref-update list, and a child that reads
    # it — `cat-file --batch-check` does — swallows every ref after the first,
    # leaving the hook to exit 0 having checked one of them.
    completed = subprocess.run(
        ["git", *args],
        check=False,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode not in accepted:
        detail = completed.stderr.decode("utf-8", "replace").strip()
        raise GitFailure(f"git {' '.join(args)} failed ({completed.returncode}): {detail}")
    return completed


def git_stdin(*args: str, payload: bytes) -> subprocess.CompletedProcess[bytes]:
    """As `git`, for the two plumbing calls that are fed on stdin. `input=`
    supplies its own pipe, so stdin is never inherited here either."""
    completed = subprocess.run(
        ["git", *args],
        check=False,
        input=payload,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode != 0:
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
        part in {"target", "dist", "node_modules", "__pycache__", ".pnpm-store"}
        for part in parts
    )
    if generated_tree or name.endswith((".pyc", ".pyo")):
        return "build artifacts and dependency trees do not belong in Git"
    return None


def content_refusals(
    label: str, path: str, mode: str, size: int | None, is_migration: bool
) -> list[str]:
    """The three rules that judge CONTENT rather than index state, so they hold
    for a staged index and for an already-pushed commit range alike.

    One implementation on purpose. These are the only refusals here with no
    server-side equivalent — `scripts/check-protected-paths.sh` inspects neither
    a filename class nor a blob size — so a second copy for the range caller is a
    second copy of the list that has to catch a `.env` on the one path Git leaves
    unhooked."""
    refusals: list[str] = []
    shown = display(path)

    reason = sensitive_reason(path)
    if reason is not None:
        refusals.append(f"{label}: REFUSED — {shown}: {reason}.")
    if is_migration and mode not in REGULAR_BLOB_MODES:
        refusals.append(
            f"{label}: REFUSED — {shown} carries Git mode {mode}; "
            "migrations must be regular files, never symlinks or submodules."
        )
    # A gitlink's size is unknowable here — its OID is a commit in a submodule's
    # object store, not ours — so it is reported as None and skips the cap rather
    # than failing the whole push closed on a legitimate submodule pointer.
    if size is not None and size > MAX_BLOB_BYTES:
        refusals.append(
            f"{label}: REFUSED — blob {shown} is {size / 1_000_000:.2f} MB; "
            "Git keeps it forever."
        )
    return refusals


def parse_raw_records(raw: bytes) -> list[tuple[str, str, str, str]]:
    """Parse `git diff-tree --raw -z -c` into (status, path, dst_mode, dst_oid).

    The combined (`-c`) grammar is the reason `parse_name_status` cannot be
    reused: it requires a single status character, and a merge emits one PER
    PARENT — an ordinary two-parent merge yields `AA`, which would raise and turn
    every merge push into a fail-closed refusal."""
    fields = raw.split(b"\0")
    if fields and fields[-1] == b"":
        fields.pop()

    records: list[tuple[str, str, str, str]] = []
    index = 0
    while index < len(fields):
        field = fields[index]
        # A field that does not open with ':' is a commit-id header. Discriminate
        # on the colon rather than on a 40-character length, so a SHA-256
        # repository's 64-hex ids still parse.
        if not field.startswith(b":"):
            index += 1
            continue

        parents = len(field) - len(field.lstrip(b":"))
        parts = field[parents:].split(b" ")
        expected = 2 * (parents + 1) + 1
        if parents < 1 or len(parts) != expected:
            raise GitFailure("git returned a malformed raw diff-tree record")
        if index + 1 >= len(fields):
            raise GitFailure("git returned a raw diff-tree record with no path")

        status = parts[-1].decode("ascii", "strict")
        if not status or any(character not in "AMT" for character in status):
            # With --no-renames neither R nor C can appear, and both carry a
            # score suffix and TWO paths — parsing them as one would desynchronize
            # the whole stream. Their appearance means the flags changed.
            raise GitFailure(f"git returned an unexpected raw status {status!r}")

        records.append(
            (
                status,
                os.fsdecode(fields[index + 1]),
                parts[parents].decode("ascii", "strict"),
                parts[2 * parents + 1].decode("ascii", "strict"),
            )
        )
        index += 2
    return records


def blob_sizes(object_ids: list[str]) -> dict[str, int]:
    """One `cat-file --batch-check` for the whole push, not one spawn per file."""
    if not object_ids:
        return {}
    payload = ("\n".join(object_ids) + "\n").encode("ascii")
    listing = git_stdin(
        "cat-file", "--batch-check=%(objectname) %(objecttype) %(objectsize)", payload=payload
    ).stdout

    sizes: dict[str, int] = {}
    for line in listing.decode("utf-8", "replace").splitlines():
        parts = line.split()
        # `<oid> missing` is exit 0 with no size. Fail closed rather than let a
        # blob we could not measure skip the cap.
        if len(parts) != 3 or parts[1] != "blob":
            raise GitFailure(f"git could not describe a pushed object: {line!r}")
        try:
            sizes[parts[0]] = int(parts[2])
        except ValueError as exc:
            raise GitFailure(f"git returned a non-numeric blob size: {line!r}") from exc
    return sizes


def check_commits(commits_file: str) -> int:
    """Apply the content rules to everything a set of commits introduces.

    Git runs `pre-commit` for an ordinary commit and routes around it entirely
    for a clean merge, a cherry-pick, a revert and `rebase --continue`. Those
    paths are exactly where new content is authored — conflict resolution most of
    all — so the content rules have to be applied again where every commit is
    visible at once, which is push time."""
    try:
        with open(commits_file, "rb") as handle:
            payload = handle.read()
    except OSError as exc:
        raise GitFailure(f"cannot read the pushed-commit list: {exc}") from exc
    if not payload.strip():
        return 0

    raw = git_stdin(
        "diff-tree",
        "--stdin",
        "-r",
        "-c",
        "--root",
        "--no-renames",
        "--raw",
        "-z",
        "--diff-filter=ACMT",
        payload=payload,
    ).stdout
    records = parse_raw_records(raw)
    if not records:
        return 0

    measurable = sorted(
        {oid for _s, _p, mode, oid in records if mode in REGULAR_BLOB_MODES or mode == "120000"}
    )
    sizes = blob_sizes(measurable)

    refused = False
    for _status, path, mode, object_id in records:
        policy_path = path.replace("\\", "/").casefold()
        is_migration = any(policy_path.startswith(prefix) for prefix in MIGRATION_PREFIXES)
        for message in content_refusals(
            "pre-push", path, mode, sizes.get(object_id), is_migration
        ):
            print(message)
            refused = True

    if refused:
        print(
            "\nNothing was pushed. These commits are already written, so the fix is a "
            "new commit removing the path — or an interactive rebase if it must not "
            "reach the remote at all."
        )
        return 1
    return 0


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


def check_index() -> int:
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

            mode, size = index_blob_metadata(path)
            for message in content_refusals("pre-commit", path, mode, size, is_migration):
                print(message)
                refused = True

        if refused:
            print("\nNothing was committed. Unstage the rejected path and fix the cause.")
            return 1
        return 0
    except (GitFailure, OSError, UnicodeError) as exc:
        print(f"pre-commit: ERROR — policy could not inspect the index: {exc}", file=sys.stderr)
        print("pre-commit: refusing closed; repair Git/index access and retry.", file=sys.stderr)
        return 2


def main() -> int:
    argv = sys.argv[1:]
    if argv[:1] == ["--commits-file"] and len(argv) == 2:
        try:
            return check_commits(argv[1])
        except (GitFailure, OSError, UnicodeError) as exc:
            print(
                f"pre-push: ERROR — policy could not inspect the pushed commits: {exc}",
                file=sys.stderr,
            )
            print("pre-push: refusing closed; repair Git access and retry.", file=sys.stderr)
            return 2
    if argv:
        print("usage: check-staged-policy.py [--commits-file PATH]", file=sys.stderr)
        return 2
    return check_index()


if __name__ == "__main__":
    raise SystemExit(main())
