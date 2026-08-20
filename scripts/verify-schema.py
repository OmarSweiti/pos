#!/usr/bin/env python3
"""Execute every SQL block in ref/schema.md against real SQLite, then audit it.

The schema reference is 888 lines of DDL that nothing compiles. Prose SQL rots:
a column referenced in one migration and never created in another reads fine and
fails at runtime. This applies the whole set to an in-memory database, in order,
and then asserts the naming and type rules from 01-conventions.md §2 against the
*result* — PRAGMA table_info, not a regex over the text.

Checks, in order of how much money each one saves:

  1. Every block executes.  Syntax, unknown tables, unknown columns.
  2. No REAL / FLOAT / DOUBLE / NUMERIC / DECIMAL column, anywhere (I-1: no float
     touches money in SQL).
  3. Money columns end `_minor`, quantities `_milli`, rates `_ppm` (§2). Names are
     matched on `_`-separated components — "generated_at" is not a rate.
  4. `*_at` / `*_date` are TEXT, flags are INTEGER and spelled `is_*` / `has_*` (§2).
     Flag naming is reported as a note, not a failure: it is a judgment call.
  5. Every foreign key names a table and column some migration actually creates.
     This is structural (`PRAGMA foreign_key_list`), not `PRAGMA foreign_key_check`
     — that one is row-level, so on an empty database it can never fire. Plain
     `sqlite3 .read` does not catch a dangling FK target either.

Usage:  ./scripts/verify-schema.py [--verbose]
        ./scripts/verify-schema.py --self-test   # prove each check still fires
Exit:   0 all clean · 1 a check failed · 2 could not run at all
"""

from __future__ import annotations

import re
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCHEMA_DOC = ROOT / "docs" / "implementation" / "ref" / "schema.md"
BASE_MIGRATION = ROOT / "crates" / "pos-db" / "migrations" / "0001_init.sql"

FENCE = re.compile(r"^```sql\s*$")
FENCE_END = re.compile(r"^```\s*$")
HEADING = re.compile(r"^#{2,3}\s+(.*)$")

FLOATY = ("REAL", "FLOAT", "DOUBLE", "NUMERIC", "DECIMAL")

# Column names are matched on `_`-separated components, never as substrings:
# "generated_at" contains "rate" and is not a rate.
MONEY_STEMS = frozenset({
    "price", "amount", "total", "subtotal", "tax", "discount", "change",
    "cost", "balance", "tender", "paid", "due", "fee", "rounding",
})
# A leading verb makes the column a flag, not an amount: `allows_change` is not money.
BOOL_PREFIXES = frozenset({"is", "has", "allows", "can", "requires", "should", "must"})
# Participles that read as flags. Curated, so a counter like `version` never trips it.
BOOL_SUFFIXES = frozenset({
    "confirmed", "enabled", "disabled", "locked", "printed", "synced",
    "applied", "voided", "reversed", "settled",
})
UNIT_WORDS = frozenset({"minor", "milli", "ppm", "pct", "bp"})


class Report:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.notes: list[str] = []

    def fail(self, msg: str) -> None:
        self.failures.append(msg)

    def note(self, msg: str) -> None:
        self.notes.append(msg)


def blocks(doc: str) -> list[tuple[str, str]]:
    """(section-heading, sql) for each ```sql fence, in document order."""
    out: list[tuple[str, str]] = []
    heading = "(top of file)"
    lines = doc.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        if m := HEADING.match(line):
            heading = m.group(1).strip()
        elif FENCE.match(line):
            body: list[str] = []
            i += 1
            while i < len(lines) and not FENCE_END.match(lines[i]):
                body.append(lines[i])
                i += 1
            out.append((heading, "\n".join(body)))
        i += 1
    return out


def apply_all(conn: sqlite3.Connection, report: Report, verbose: bool) -> int:
    applied = 0
    base = BASE_MIGRATION.read_text(encoding="utf-8")
    try:
        conn.executescript(base)
        applied += 1
    except sqlite3.Error as exc:
        report.fail(f"{BASE_MIGRATION.relative_to(ROOT)} does not execute: {exc}")
        return applied

    for heading, sql in blocks(SCHEMA_DOC.read_text(encoding="utf-8")):
        if not sql.strip():
            continue
        try:
            conn.executescript(sql)
            applied += 1
            if verbose:
                print(f"  applied  {heading}")
        except sqlite3.Error as exc:
            report.fail(f"block '{heading}' does not execute against SQLite: {exc}")
    return applied


def audit_columns(conn: sqlite3.Connection, report: Report) -> tuple[int, int]:
    tables = [
        r[0] for r in conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table' "
            "AND name NOT LIKE 'sqlite_%' ORDER BY name"
        )
    ]
    columns = 0
    for table in tables:
        for _cid, name, decl_type, _notnull, _dflt, _pk in conn.execute(
            f'PRAGMA table_info("{table}")'
        ):
            columns += 1
            where = f"{table}.{name}"
            declared = (decl_type or "").upper()
            parts = name.lower().split("_")
            head, tail = parts[0], parts[-1]

            if any(f in declared for f in FLOATY):
                report.fail(f"{where} is {declared} — I-1 forbids float types in SQL")

            if head in BOOL_PREFIXES:
                # A flag. §2: INTEGER 0/1, and named is_* / has_*.
                if declared not in ("INTEGER", ""):
                    report.fail(f"{where} is {declared} — §2 wants INTEGER 0/1 for a flag")
                elif head not in ("is", "has"):
                    report.note(f"{where}: flag column — §2 spells these is_* / has_*")
            elif tail in MONEY_STEMS:
                report.fail(f"{where}: an amount must end _minor (I-1/§2)")
            elif tail in BOOL_SUFFIXES and declared == "INTEGER":
                report.note(f"{where}: reads as a flag — §2 spells these is_* / has_*")

            if "rate" in parts and tail not in UNIT_WORDS:
                report.fail(f"{where}: a rate must end _ppm (§2)")

            if ({"qty", "quantity"} & set(parts)) and tail != "milli":
                report.fail(f"{where}: a quantity must end _milli (I-3)")

            if tail in ("at", "date") and declared not in ("TEXT", ""):
                report.fail(f"{where} is {declared} — §2 wants ISO-8601 TEXT")

    return len(tables), columns


BAD_FIXTURE = """
CREATE TABLE parent (id BLOB PRIMARY KEY);
CREATE TABLE bad (
  id             BLOB PRIMARY KEY,
  unit_price     REAL    NOT NULL,          -- float, and no _minor
  qty            INTEGER NOT NULL,          -- no _milli
  vat_rate       INTEGER NOT NULL,          -- no _ppm
  completed_at   INTEGER NOT NULL,          -- not TEXT
  is_active      TEXT    NOT NULL,          -- flag, not INTEGER
  allows_refund  INTEGER NOT NULL DEFAULT 0,-- flag, wrong spelling
  orphan_id      BLOB    NOT NULL REFERENCES ghost(id)
);
CREATE TABLE fine (
  id            BLOB PRIMARY KEY,
  total_minor   INTEGER NOT NULL,
  qty_milli     INTEGER NOT NULL,
  rate_ppm      INTEGER NOT NULL,
  generated_at  TEXT    NOT NULL,           -- contains "rate"; must NOT be flagged
  version       INTEGER NOT NULL DEFAULT 0, -- counter; must NOT be flagged
  is_active     INTEGER NOT NULL DEFAULT 1,
  parent_id     BLOB    NOT NULL REFERENCES parent(id)
);
"""

# Each entry: (substring that must appear in a failure, what it proves).
SELF_TEST_EXPECTED = [
    ("bad.unit_price is REAL", "float column type"),
    ("bad.unit_price: an amount must end _minor", "money without a unit"),
    ("bad.qty: a quantity must end _milli", "quantity without _milli"),
    ("bad.vat_rate: a rate must end _ppm", "rate without _ppm"),
    ("bad.completed_at is INTEGER", "timestamp not TEXT"),
    ("bad.is_active is TEXT", "flag not INTEGER"),
]
SELF_TEST_NOTES = [("bad.allows_refund", "flag spelled without is_/has_")]
# Names that must never be flagged at all.
SELF_TEST_CLEAN = ("fine.", "generated_at", "version")


def self_test() -> int:
    """A guard nobody has seen fail is a guard nobody should trust."""
    report = Report()
    conn = sqlite3.connect(":memory:")
    conn.executescript(BAD_FIXTURE)
    audit_columns(conn, report)

    ok = True
    for needle, what in SELF_TEST_EXPECTED:
        hit = any(needle in f for f in report.failures)
        print(f"  {'ok  ' if hit else 'FAIL'}  detects {what}")
        ok &= hit
    for needle, what in SELF_TEST_NOTES:
        hit = any(needle in n for n in report.notes)
        print(f"  {'ok  ' if hit else 'FAIL'}  notes {what}")
        ok &= hit

    false_positives = [
        m for m in report.failures + report.notes
        if m.startswith("fine.") or "generated_at" in m or m.startswith("fine.version")
    ]
    print(f"  {'ok  ' if not false_positives else 'FAIL'}  no false positives on the clean table")
    for fp in false_positives:
        print(f"        unexpected: {fp}")
    ok &= not false_positives

    # A reference to a table no migration creates: the check that `sqlite3 .read`
    # and `PRAGMA foreign_key_check` both miss on an empty database.
    fk_report = Report()
    audit_foreign_keys(conn, fk_report)
    caught = any("'ghost'" in f for f in fk_report.failures)
    print(f"  {'ok  ' if caught else 'FAIL'}  detects a foreign key to a table that is never created")
    ok &= caught

    print("\nself-test PASSED" if ok else "\nself-test FAILED")
    return 0 if ok else 1


def audit_foreign_keys(conn: sqlite3.Connection, report: Report) -> int:
    """Every REFERENCES target must be a table, and a column, that exists."""
    tables = {
        r[0] for r in conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
        )
    }
    checked = 0
    for table in sorted(tables):
        for row in conn.execute(f'PRAGMA foreign_key_list("{table}")'):
            checked += 1
            parent, to_col = row[2], row[4]
            if parent not in tables:
                report.fail(
                    f"{table}: foreign key references table '{parent}', "
                    "which no migration creates"
                )
                continue
            if to_col is not None:
                parent_cols = {
                    c[1] for c in conn.execute(f'PRAGMA table_info("{parent}")')
                }
                if to_col not in parent_cols:
                    report.fail(
                        f"{table}: foreign key references {parent}.{to_col}, "
                        "which that table does not have"
                    )
    return checked


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    verbose = "--verbose" in sys.argv
    for path in (SCHEMA_DOC, BASE_MIGRATION):
        if not path.is_file():
            print(f"cannot find {path}", file=sys.stderr)
            return 2

    report = Report()
    conn = sqlite3.connect(":memory:")
    conn.execute("PRAGMA foreign_keys=ON")

    applied = apply_all(conn, report, verbose)
    tables, columns = audit_columns(conn, report)

    fks = audit_foreign_keys(conn, report)

    indexes = conn.execute(
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%'"
    ).fetchone()[0]

    print(f"schema: {applied} SQL blocks applied, {tables} tables, {columns} columns, "
          f"{indexes} indexes, {fks} foreign keys")
    for note in report.notes:
        print(f"  note  {note}")
    if report.failures:
        print(f"\n{len(report.failures)} problem(s):")
        for f in report.failures:
            print(f"  FAIL  {f}")
        print("\nref/schema.md is the plan of record — fix the doc, or the migration that drifted from it.")
        return 1
    print("schema reference is executable and conforms to conventions §2")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
