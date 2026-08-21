#!/usr/bin/env python3
"""Audit the Postgres mirror, and apply it to a real PostgreSQL server.

`crates/pos-db/migrations/` is validated by verify-schema.py against real SQLite.
Its Postgres counterpart had neither check: nothing executed it against Postgres,
and nothing said which SQLite migration each file mirrors. Two engines drifting
apart is a sync bug that only appears in the field.

Two passes, deliberately separable:

  1. The MAPPING audit. Needs no database, so it runs in `just lint` and on every
     CI job. sqlx names its files with a timestamp, so a Postgres migration cannot
     carry the same number as the SQLite one it mirrors — which is exactly why the
     mapping has to be written down instead of inferred from a filename. Every
     file in apps/server/migrations/ must declare, in a comment, either

         Mirrors SQLite NNNN_name.sql        (what the register migration was)
         Server-only                        (nothing on the register corresponds)

     and every SQLite migration must be claimed by one of those declarations or
     be listed in REGISTER_LOCAL below.

  2. The ENGINE pass. Applies every Postgres migration, in filename order, to a
     scratch database on a real server — the check that `sqlx migrate run` would
     have done, without needing sqlx-cli. It uses $DATABASE_URL when one is set
     (CI has a Postgres service), otherwise a throwaway Docker container, and
     otherwise says clearly that it did not run rather than reporting success.

Usage:  ./scripts/verify-pg-migrations.py [--verbose]
        ./scripts/verify-pg-migrations.py --mapping-only   # no database needed
        ./scripts/verify-pg-migrations.py --self-test      # prove the audit fires
Exit:   0 all clean (or the engine pass was skipped) · 1 a check failed
        · 2 could not run at all
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parent.parent
PG_DIR = ROOT / "apps" / "server" / "migrations"
SQLITE_DIR = ROOT / "crates" / "pos-db" / "migrations"

SCRATCH_DB = "pos_migration_check"
PG_IMAGE = "postgres:16-alpine"

MIRRORS = re.compile(r"\bmirrors\s+sqlite\s+(\d{4}_[a-z0-9_]+\.sql)", re.IGNORECASE)
SERVER_ONLY = re.compile(r"\bserver-only\b", re.IGNORECASE)

# Postgres migrations that predate this audit. They are committed, so they cannot
# be edited to carry the declaration the rule now requires (conventions §9) — the
# mapping is recorded here instead, once, with the reason.
GRANDFATHERED: dict[str, str] = {
    # Its header says "Server-side mirror of the catalog + the global change
    # sequence"; the register migration that shipped those tables is 0001.
    "20260819200319_init.sql": "0001_init.sql",
}

# SQLite migrations with deliberately no server counterpart. A register-local
# entity never syncs, so an empty mirror would be a file that lies.
REGISTER_LOCAL: dict[str, str] = {
    # (none yet — 0001 and 0002 both have mirrors)
}


def sqlite_migrations() -> list[Path]:
    return sorted(SQLITE_DIR.glob("*.sql"))


def pg_migrations() -> list[Path]:
    return sorted(PG_DIR.glob("*.sql"))


def rel(path: Path) -> str:
    """Repository-relative when it can be, absolute when it cannot — the
    self-test points the directories at a temporary tree outside ROOT."""
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


class Report:
    def __init__(self) -> None:
        self.failures: list[str] = []

    def fail(self, message: str) -> None:
        self.failures.append(message)


def audit_mapping(report: Report, verbose: bool = False) -> dict[str, str]:
    """Check every declaration both ways. Returns Postgres file → SQLite file."""
    claimed: dict[str, str] = {}

    for path in pg_migrations():
        text = path.read_text(encoding="utf-8", errors="replace")
        found = MIRRORS.search(text)
        if found:
            claimed[path.name] = found.group(1)
        elif SERVER_ONLY.search(text):
            claimed[path.name] = ""
        elif path.name in GRANDFATHERED:
            claimed[path.name] = GRANDFATHERED[path.name]
        else:
            report.fail(
                f"{path.name} declares no counterpart. Add a header comment saying "
                "either 'Mirrors SQLite NNNN_name.sql' or 'Server-only' — a "
                "timestamped filename cannot carry that mapping by itself."
            )
            continue
        if verbose:
            target = claimed[path.name] or "(server-only)"
            print(f"  {path.name}  ->  {target}")

    known = {p.name for p in sqlite_migrations()}
    for pg_name, sqlite_name in claimed.items():
        if sqlite_name and sqlite_name not in known:
            report.fail(
                f"{pg_name} claims to mirror {sqlite_name}, which does not exist in "
                f"{rel(SQLITE_DIR)}."
            )

    mirrored = {name for name in claimed.values() if name}
    for path in sqlite_migrations():
        if path.name in mirrored or path.name in REGISTER_LOCAL:
            continue
        report.fail(
            f"{path.name} has no Postgres counterpart. Mirror it in "
            f"{rel(PG_DIR)} with a 'Mirrors SQLite {path.name}' header, "
            "or add it to REGISTER_LOCAL in this script with the reason it never syncs."
        )

    return claimed


def run(argv: list[str], stdin: str | None = None, timeout: int = 120):
    return subprocess.run(
        argv, input=stdin, capture_output=True, text=True, timeout=timeout, check=False
    )


def scratch_url(base: str) -> str:
    parts = urlsplit(base)
    return urlunsplit(parts._replace(path=f"/{SCRATCH_DB}"))


def apply_with_psql(base_url: str, verbose: bool, report: Report) -> bool:
    """Apply every migration to a freshly created scratch database. True if run."""
    for statement in (
        f'DROP DATABASE IF EXISTS "{SCRATCH_DB}"',
        f'CREATE DATABASE "{SCRATCH_DB}"',
    ):
        done = run(["psql", base_url, "-v", "ON_ERROR_STOP=1", "-q", "-c", statement])
        if done.returncode != 0:
            report.fail(f"could not prepare the scratch database: {done.stderr.strip()}")
            return True

    target = scratch_url(base_url)
    for path in pg_migrations():
        done = run(
            ["psql", target, "-v", "ON_ERROR_STOP=1", "-q", "-f", str(path)],
            timeout=180,
        )
        if done.returncode != 0:
            report.fail(f"{path.name} does not apply to PostgreSQL:\n{done.stderr.strip()}")
            break
        if verbose:
            print(f"  applied {path.name}")

    run(["psql", base_url, "-q", "-c", f'DROP DATABASE IF EXISTS "{SCRATCH_DB}"'])
    return True


def apply_with_docker(verbose: bool, report: Report) -> bool:
    """Same pass against a throwaway container. True if it actually ran."""
    name = f"pos-pg-verify-{os.getpid()}"
    started = run([
        "docker", "run", "-d", "--rm", "--name", name,
        "-e", "POSTGRES_PASSWORD=verify", "-e", f"POSTGRES_DB={SCRATCH_DB}",
        PG_IMAGE,
    ])
    if started.returncode != 0:
        print("skipped the engine pass: Docker is installed but would not start a "
              f"container ({started.stderr.strip().splitlines()[-1:] or ['no output']})")
        return False

    try:
        for _ in range(60):
            ready = run(["docker", "exec", name, "pg_isready", "-U", "postgres",
                         "-d", SCRATCH_DB], timeout=20)
            if ready.returncode == 0:
                break
            time.sleep(1)
        else:
            report.fail(f"{PG_IMAGE} never became ready")
            return True

        for path in pg_migrations():
            done = run(
                ["docker", "exec", "-i", name, "psql", "-v", "ON_ERROR_STOP=1",
                 "-q", "-U", "postgres", "-d", SCRATCH_DB, "-f", "-"],
                stdin=path.read_text(encoding="utf-8"),
                timeout=180,
            )
            if done.returncode != 0:
                report.fail(f"{path.name} does not apply to PostgreSQL:\n{done.stderr.strip()}")
                break
            if verbose:
                print(f"  applied {path.name}")
    finally:
        run(["docker", "rm", "-f", name], timeout=60)
    return True


def engine_pass(verbose: bool, report: Report) -> bool:
    """Apply the mirror to a real server. False when no server was reachable."""
    url = os.environ.get("DATABASE_URL", "").strip()
    if url and shutil.which("psql"):
        return apply_with_psql(url, verbose, report)
    if url and not shutil.which("psql"):
        print("DATABASE_URL is set but psql is not installed; trying Docker instead")
    if shutil.which("docker"):
        return apply_with_docker(verbose, report)
    return False


def self_test() -> int:
    """Prove the mapping audit fails on the things it claims to catch."""
    import tempfile

    global PG_DIR, SQLITE_DIR  # noqa: PLW0603
    real_pg, real_sqlite = PG_DIR, SQLITE_DIR
    cases: list[tuple[str, dict[str, str], dict[str, str], bool]] = [
        ("a declared mirror passes",
         {"20260101000000_a.sql": "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, False),
        ("an undeclared Postgres migration fails",
         {"20260101000000_a.sql": "-- no declaration here\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
        ("a mirror of a migration that does not exist fails",
         {"20260101000000_a.sql": "-- Mirrors SQLite 0009_ghost.sql\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
        ("an unmirrored SQLite migration fails",
         {"20260101000000_a.sql": "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n", "0002_more.sql": "SELECT 1;\n"}, True),
        ("a server-only migration needs no counterpart",
         {"20260101000000_a.sql": "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n",
          "20260102000000_b.sql": "-- Server-only: reporting views.\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, False),
    ]

    failures = 0
    for label, pg_files, sqlite_files, want_failure in cases:
        with tempfile.TemporaryDirectory() as tmp:
            PG_DIR = Path(tmp) / "pg"
            SQLITE_DIR = Path(tmp) / "sqlite"
            PG_DIR.mkdir()
            SQLITE_DIR.mkdir()
            for name, body in pg_files.items():
                (PG_DIR / name).write_text(body)
            for name, body in sqlite_files.items():
                (SQLITE_DIR / name).write_text(body)
            report = Report()
            audit_mapping(report)
            got_failure = bool(report.failures)
            if got_failure == want_failure:
                print(f"  ok    {label}")
            else:
                print(f"  FAIL  {label} (wanted failure={want_failure}, got {got_failure})")
                failures += 1

    PG_DIR, SQLITE_DIR = real_pg, real_sqlite
    print(f"\nmapping audit self-test: {len(cases) - failures} passed, {failures} failed")
    return 1 if failures else 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    verbose = "--verbose" in sys.argv
    if not PG_DIR.is_dir():
        print(f"cannot find {PG_DIR}", file=sys.stderr)
        return 2
    if not pg_migrations():
        print(f"no migrations in {PG_DIR}", file=sys.stderr)
        return 2

    report = Report()
    audit_mapping(report, verbose)
    print(f"mapping: {len(pg_migrations())} Postgres migration(s) declared against "
          f"{len(sqlite_migrations())} SQLite migration(s)")

    if "--mapping-only" in sys.argv:
        ran = False
    else:
        ran = engine_pass(verbose, report)
        if ran:
            print(f"engine: {len(pg_migrations())} migration(s) applied to real PostgreSQL")
        else:
            print("engine: SKIPPED — no $DATABASE_URL and no Docker. The mirror was "
                  "audited but never executed; CI's rust job runs the engine pass.")

    if report.failures:
        print(f"\n{len(report.failures)} problem(s):")
        for message in report.failures:
            print(f"  FAIL  {message}")
        print("\nThe mapping policy is conventions §9 rule 4, as stated in "
              ".claude/rules/sql-migrations.md.")
        return 1
    print("Postgres mirror conforms")
    return 0


if __name__ == "__main__":
    sys.exit(main())
