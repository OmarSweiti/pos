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
     Files must use unique, strictly increasing
     ``<14-digit UTC timestamp>_<lower_snake>.sql`` versions. Every file in
     apps/server/migrations/ must also declare, in a comment, either

         Mirrors SQLite NNNN_name.sql        (what the register migration was)
         Server-only: <reason>              (nothing on the register corresponds)

     and every SQLite migration must be claimed by one of those declarations or
     be listed in REGISTER_LOCAL below.

  2. The ENGINE pass. Applies every Postgres migration, in filename order, to a
     scratch database on a real server. Ordinary files run in one transaction;
     only SQL beginning at byte zero with SQLx's exact ``-- no-transaction``
     marker opts out. This mirrors the application's SQLx 0.9 per-file execution
     boundary without needing sqlx-cli. It uses $DATABASE_URL when one is set
     (CI has a Postgres service), otherwise a throwaway Docker container, and
     otherwise says clearly that it did not run rather than reporting success.

Usage:  ./scripts/verify-pg-migrations.py [--verbose]
        ./scripts/verify-pg-migrations.py --mapping-only   # no database needed
        ./scripts/verify-pg-migrations.py --self-test      # prove the audit fires
Exit:   0 all clean (or the engine pass was skipped) · 1 a check failed
        · 2 could not run at all
"""

from __future__ import annotations

import json
import os
import re
import secrets
import shutil
import stat
import subprocess
import sys
import time
from collections import Counter
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parent.parent
PG_DIR = ROOT / "apps" / "server" / "migrations"
SQLITE_DIR = ROOT / "crates" / "pos-db" / "migrations"
COMPOSE_FILE = ROOT / "infra" / "docker-compose.yml"
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"

# The single Postgres pin. Dependabot only ever edits infra/docker-compose.yml,
# so a bump lands here and in .github/workflows/ci.yml by hand, in the same
# commit — audit_image_pin below is what makes a half-applied bump fail loudly.
PG_MAJOR = 18
PG_IMAGE = (
    "postgres:18-alpine@sha256:"
    "d3e1620b530c944afa6e887d22eb899824da68e19c52024bf98f5220c88a65b2"
)

MIRROR_DECLARATION = re.compile(
    r"^mirrors\s+sqlite\s+(\d{4}_[a-z0-9_]+\.sql)"
    r"(?:\s+\([^\r\n]*\)\.?)?$",
    re.IGNORECASE,
)
SERVER_ONLY_DECLARATION = re.compile(
    r"^server-only\s*[:\-—]\s*\S[^\r\n]*$", re.IGNORECASE
)
IMAGE_PIN = re.compile(rf"^postgres:{PG_MAJOR}-alpine@sha256:[0-9a-f]{{64}}$")
PG_MIGRATION_NAME = re.compile(
    r"^(?P<version>\d{14})_(?P<name>[a-z][a-z0-9]*(?:_[a-z0-9]+)*)\.sql$"
)
SQLX_NO_TRANSACTION_MARKER = "-- no-transaction"
YAML_BLOCK_KEY = re.compile(
    r"^(?P<indent> *)(?P<key>"
    r'"(?:[^"\\]|\\.)*"'
    r"|'(?:[^']|'')*'"
    r"|[A-Za-z0-9_.-]+)[ \t]*:(?:[ \t]*(?P<value>.*))?$"
)

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


def audit_migration_file_types(
    report: Report, paths: list[Path], engine: str
) -> bool:
    """Reject symlink/device indirection before any migration bytes are read."""
    clean = True
    for path in paths:
        try:
            if path.is_symlink():
                report.fail(
                    f"{rel(path)} is not a regular {engine} migration file; "
                    "symbolic links and other indirection are forbidden"
                )
                clean = False
                continue
            mode = path.lstat().st_mode
        except OSError as exc:
            report.fail(f"cannot inspect {engine} migration {rel(path)}: {exc}")
            clean = False
            continue
        if not stat.S_ISREG(mode):
            report.fail(
                f"{rel(path)} is not a regular {engine} migration file; "
                "symbolic links and other indirection are forbidden"
            )
            clean = False
    return clean


def audit_pg_filenames(report: Report, paths: list[Path] | None = None) -> None:
    """Match the migration set and ordering that sqlx derives from filenames."""
    versions: list[str] = []
    for path in pg_migrations() if paths is None else paths:
        match = PG_MIGRATION_NAME.fullmatch(path.name)
        if match is None:
            report.fail(
                f"{path.name} is not a sqlx migration named "
                "<14-digit UTC timestamp>_<lower_snake>.sql"
            )
            continue
        version = match.group("version")
        try:
            datetime.strptime(version, "%Y%m%d%H%M%S")
        except ValueError:
            report.fail(f"{path.name} starts with an invalid calendar timestamp")
            continue
        versions.append(version)

    duplicates = sorted(
        version for version, count in Counter(versions).items() if count > 1
    )
    if duplicates:
        report.fail(
            "Postgres migration versions must be unique; duplicate timestamp(s): "
            + ", ".join(duplicates)
        )
    if not duplicates and versions != sorted(versions):
        report.fail("Postgres migration versions must be strictly increasing")


def mapping_declarations(text: str) -> list[tuple[str, str]]:
    """Return canonical declarations from the leading ``--`` comment header.

    SQL strings, block comments, and comments after the first statement are not
    metadata. Keeping this parser intentionally narrow prevents executable SQL
    from impersonating the migration header that the audit is meant to review.
    The tuple is ``(kind, sqlite_name)``; server-only declarations have no name.
    """
    declarations: list[tuple[str, str]] = []
    for line in text.splitlines():
        stripped = line.lstrip()
        if not stripped:
            continue
        if not stripped.startswith("--"):
            break

        comment = stripped[2:].strip()
        mirror = MIRROR_DECLARATION.fullmatch(comment)
        if mirror:
            declarations.append(("mirror", mirror.group(1)))
        elif SERVER_ONLY_DECLARATION.fullmatch(comment):
            declarations.append(("server-only", ""))
    return declarations


def strip_yaml_comment(line: str) -> str:
    """Remove a YAML comment without treating ``#`` inside quotes as one.

    This is deliberately a small lexer, not a permissive YAML parser. The image
    policy accepts only direct block-mapping scalars at the governed service
    path; aliases, flow mappings, and block scalars fail closed.
    """
    quote: str | None = None
    escaped = False
    index = 0
    while index < len(line):
        char = line[index]
        if quote == '"':
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quote = None
        elif quote == "'":
            if char == "'":
                if index + 1 < len(line) and line[index + 1] == "'":
                    index += 1
                else:
                    quote = None
        elif char in {'"', "'"}:
            quote = char
        elif char == "#" and (index == 0 or line[index - 1].isspace()):
            return line[:index].rstrip()
        index += 1
    return line.rstrip()


def direct_yaml_scalar(raw: str) -> str | None:
    """Decode a direct one-line YAML scalar, rejecting indirect/complex forms."""
    raw = raw.strip()
    if not raw or raw[0] in "|>*&!{[":
        return None
    if raw.startswith('"'):
        try:
            value = json.loads(raw)
        except (json.JSONDecodeError, TypeError):
            return None
        return value if isinstance(value, str) else None
    if raw.startswith("'"):
        if len(raw) < 2 or not raw.endswith("'"):
            return None
        inner = raw[1:-1]
        # In a YAML single-quoted scalar, an embedded quote is written twice.
        if "'" in inner.replace("''", ""):
            return None
        return inner.replace("''", "'")
    return raw


def yaml_path_roles(
    key_path: tuple[str, ...], surface: str
) -> tuple[bool, bool, bool]:
    """Return ``(tracked, container, image)`` for a service-tree path."""
    if surface == "compose":
        root = key_path == ("services",)
        service = len(key_path) == 2 and key_path[0] == "services"
        image = (
            len(key_path) == 3
            and key_path[0] == "services"
            and key_path[2] == "image"
        )
        return root or service or image, root or service, image
    if surface == "ci":
        jobs = key_path == ("jobs",)
        job = len(key_path) == 2 and key_path[0] == "jobs"
        services = (
            len(key_path) == 3
            and key_path[0] == "jobs"
            and key_path[2] == "services"
        )
        service = (
            len(key_path) == 4
            and key_path[0] == "jobs"
            and key_path[2] == "services"
        )
        image = (
            len(key_path) == 5
            and key_path[0] == "jobs"
            and key_path[2] == "services"
            and key_path[4] == "image"
        )
        return (
            jobs or job or services or service or image,
            jobs or job or services or service,
            image,
        )
    raise ValueError(f"unknown PostgreSQL image surface: {surface}")


def sensitive_yaml_parent(key_path: tuple[str, ...], surface: str) -> bool:
    """Whether an unsupported direct child could hide a service/image key."""
    if surface == "compose":
        return key_path == ("services",) or (
            len(key_path) == 2 and key_path[0] == "services"
        )
    if surface == "ci":
        return (
            key_path == ("jobs",)
            or (len(key_path) == 2 and key_path[0] == "jobs")
            or (
                len(key_path) in {3, 4}
                and key_path[0] == "jobs"
                and key_path[2] == "services"
            )
        )
    raise ValueError(f"unknown PostgreSQL image surface: {surface}")


def is_postgres_repository_image(image: str) -> bool:
    """Recognize a Postgres repository regardless of registry or tag/digest."""
    repository_with_tag = image.strip().lower().split("@", maxsplit=1)[0]
    final_component = repository_with_tag.rsplit("/", maxsplit=1)[-1]
    repository = final_component.split(":", maxsplit=1)[0]
    return repository == "postgres"


def governed_image_scalars(
    text: str, surface: str
) -> tuple[list[str], list[str]]:
    """Read every direct service-image scalar on a governed YAML surface.

    Compose images live at ``services.<service>.image``; CI service images live
    at ``jobs.<job>.services.<service>.image``. Service names are immaterial:
    every Postgres repository image is counted. This strict block-map lexer
    rejects constructs that could hide or inject a service/image definition.
    """
    images: list[str] = []
    errors: list[str] = []
    parents: list[tuple[int, str]] = []
    seen_governed_paths: set[tuple[str, ...]] = set()

    for line_number, source_line in enumerate(text.splitlines(), start=1):
        line = strip_yaml_comment(source_line)
        if not line.strip():
            continue

        indent = len(line) - len(line.lstrip(" "))
        while parents and indent <= parents[-1][0]:
            parents.pop()
        parent_path = tuple(parent_key for _, parent_key in parents)

        match = YAML_BLOCK_KEY.fullmatch(line)
        if match is None:
            if sensitive_yaml_parent(parent_path, surface):
                errors.append(
                    f"line {line_number}: unsupported YAML construct directly "
                    f"under {'.'.join(parent_path)} could hide a service image"
                )
            continue

        indent = len(match.group("indent"))
        key = direct_yaml_scalar(match.group("key"))
        if key is None:
            errors.append(f"line {line_number}: YAML mapping key is not direct")
            continue
        raw_value = match.group("value") or ""
        key_path = parent_path + (key,)
        tracked, container, image = yaml_path_roles(key_path, surface)

        if tracked:
            if key_path in seen_governed_paths:
                errors.append(
                    f"line {line_number}: duplicate governed YAML key path "
                    f"{'.'.join(key_path)} is ambiguous"
                )
            seen_governed_paths.add(key_path)

            if container and raw_value.strip():
                errors.append(
                    f"line {line_number}: governed YAML path "
                    f"{'.'.join(key_path)} must use an explicit block mapping"
                )

        if image:
            scalar = direct_yaml_scalar(raw_value)
            if scalar is None:
                errors.append(
                    f"line {line_number}: service image must be a direct "
                    "one-line scalar, not an alias, block, or flow value"
                )
            elif "$" in scalar:
                errors.append(
                    f"line {line_number}: dynamic service image values are not "
                    "auditable; pin the literal image"
                )
            else:
                images.append(scalar)

        parents.append((indent, key))

    return images, errors


def audit_image_text(report: Report, text: str, surface: str, label: str) -> None:
    images, errors = governed_image_scalars(text, surface)
    for error in errors:
        report.fail(f"{label} {error}")
    postgres_images = [image for image in images if is_postgres_repository_image(image)]
    if postgres_images != [PG_IMAGE]:
        # Name the expected pin and every file that carries it. A Postgres bump
        # touches three files and Dependabot can only edit one, so the reader of
        # this failure needs the other two by name, not a symbol to go look up.
        found = ", ".join(postgres_images) if postgres_images else "none"
        report.fail(
            f"{label} must declare exactly one Postgres repository image across "
            "all services, and it must equal PG_IMAGE; comments, strings, "
            "aliases, dynamic values, and unrelated keys do not count\n"
            f"        expected: {PG_IMAGE}\n"
            f"        found:    {found}\n"
            "        all three must agree: infra/docker-compose.yml, "
            ".github/workflows/ci.yml, and PG_IMAGE in "
            "scripts/verify-pg-migrations.py"
        )


def audit_image_pin(report: Report) -> None:
    """Keep every PostgreSQL test surface on one immutable image manifest."""
    if IMAGE_PIN.fullmatch(PG_IMAGE) is None:
        report.fail(
            f"PG_IMAGE must pin postgres:{PG_MAJOR}-alpine to a full sha256 digest"
        )
    for path, surface in ((COMPOSE_FILE, "compose"), (CI_WORKFLOW, "ci")):
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as exc:
            report.fail(f"cannot read {rel(path)} to verify the Postgres image: {exc}")
            continue
        audit_image_text(report, text, surface, rel(path))


def substantive_policy_reason(value: object) -> bool:
    """Return whether a policy exception contains a reviewable explanation."""
    if not isinstance(value, str):
        return False
    normalized = " ".join(value.split())
    if normalized.casefold() in {
        "n/a",
        "na",
        "none",
        "placeholder",
        "reason here",
        "tbd",
        "todo",
        "unknown",
    }:
        return False
    words = [
        word
        for word in normalized.split()
        if any(character.isalnum() for character in word)
    ]
    return len(normalized) >= 8 and len(words) >= 2


def audit_register_local_policy(
    report: Report, known: set[str], mirrored: set[str]
) -> set[str]:
    """Validate and return the register-local exceptions allowed as coverage."""
    valid: set[str] = set()
    for sqlite_name, reason in REGISTER_LOCAL.items():
        canonical = isinstance(sqlite_name, str) and sqlite_name in known
        if not canonical:
            report.fail(
                "REGISTER_LOCAL keys must be exact filenames of existing SQLite "
                f"migrations in {rel(SQLITE_DIR)}; got {sqlite_name!r}."
            )

        reason_valid = substantive_policy_reason(reason)
        if not reason_valid:
            report.fail(
                f"REGISTER_LOCAL[{sqlite_name!r}] must have a nonblank, "
                "substantive reason explaining why the migration never syncs."
            )

        exclusive = canonical and sqlite_name not in mirrored
        if canonical and not exclusive:
            report.fail(
                f"{sqlite_name} is covered by both REGISTER_LOCAL and a Postgres "
                "mirror declaration. Choose exactly one coverage mechanism."
            )

        if canonical and reason_valid and exclusive:
            valid.add(sqlite_name)
    return valid


def audit_mapping(report: Report, verbose: bool = False) -> dict[str, str]:
    """Check every declaration both ways. Returns Postgres file → SQLite file."""
    claimed: dict[str, str] = {}
    pg_paths = pg_migrations()
    sqlite_paths = sqlite_migrations()
    audit_pg_filenames(report, pg_paths)
    types_clean = audit_migration_file_types(report, pg_paths, "Postgres")
    types_clean &= audit_migration_file_types(report, sqlite_paths, "SQLite")
    if not types_clean:
        return claimed

    for path in pg_paths:
        text = path.read_text(encoding="utf-8", errors="replace")
        declarations = mapping_declarations(text)
        if len(declarations) > 1:
            report.fail(
                f"{path.name} has multiple mapping declarations in its header. "
                "Keep exactly one 'Mirrors SQLite NNNN_name.sql' or "
                "'Server-only: <reason>' declaration."
            )
            continue
        if declarations:
            _kind, sqlite_name = declarations[0]
            claimed[path.name] = sqlite_name
        elif path.name in GRANDFATHERED:
            claimed[path.name] = GRANDFATHERED[path.name]
        else:
            report.fail(
                f"{path.name} declares no counterpart. Add a header comment saying "
                "either 'Mirrors SQLite NNNN_name.sql' or "
                "'Server-only: <reason>' — a "
                "timestamped filename cannot carry that mapping by itself."
            )
            continue
        if verbose:
            target = claimed[path.name] or "(server-only)"
            print(f"  {path.name}  ->  {target}")

    known = {p.name for p in sqlite_paths}
    for pg_name, sqlite_name in claimed.items():
        if sqlite_name and sqlite_name not in known:
            report.fail(
                f"{pg_name} claims to mirror {sqlite_name}, which does not exist in "
                f"{rel(SQLITE_DIR)}."
            )

    mirrored = {name for name in claimed.values() if name}
    register_local = audit_register_local_policy(report, known, mirrored)
    for path in sqlite_paths:
        if path.name in mirrored or path.name in register_local:
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


def scratch_database_name() -> str:
    """Return a collision-resistant PostgreSQL identifier for this one run."""
    return f"pos_migration_check_{os.getpid()}_{secrets.token_hex(6)}"


def scratch_url(base: str, database: str) -> str:
    parts = urlsplit(base)
    return urlunsplit(parts._replace(path=f"/{database}"))


def psql_transaction_options(sql: str) -> list[str]:
    """Return the psql option matching SQLx 0.9's per-file transaction mode.

    SQLx uses a case-sensitive ``starts_with`` check at byte zero. Keep this
    intentionally literal: leading whitespace, a BOM, or a differently-cased
    marker does not opt out in the runtime and must not opt out here either.
    """
    if sql.startswith(SQLX_NO_TRANSACTION_MARKER):
        return []
    return ["--single-transaction"]


def apply_with_psql(base_url: str, verbose: bool, report: Report) -> bool:
    """Apply every migration to a freshly created scratch database. True if run."""
    database = scratch_database_name()
    created = run(
        [
            "psql",
            base_url,
            "-v",
            "ON_ERROR_STOP=1",
            "-q",
            "-c",
            f'CREATE DATABASE "{database}"',
        ]
    )
    if created.returncode != 0:
        report.fail(f"could not create the scratch database: {created.stderr.strip()}")
        return True

    target = scratch_url(base_url, database)
    try:
        for path in pg_migrations():
            sql = path.read_text(encoding="utf-8")
            done = run(
                [
                    "psql",
                    target,
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-q",
                    *psql_transaction_options(sql),
                    "-f",
                    str(path),
                ],
                timeout=180,
            )
            if done.returncode != 0:
                report.fail(
                    f"{path.name} does not apply to PostgreSQL:\n{done.stderr.strip()}"
                )
                break
            if verbose:
                print(f"  applied {path.name}")
    finally:
        cleanup = run(
            [
                "psql",
                base_url,
                "-v",
                "ON_ERROR_STOP=1",
                "-q",
                "-c",
                f'DROP DATABASE IF EXISTS "{database}" WITH (FORCE)',
            ]
        )
        if cleanup.returncode != 0:
            report.fail(
                f"could not remove scratch database {database}: "
                f"{cleanup.stderr.strip()}"
            )
    return True


def remove_docker_container(
    name: str,
    report: Report,
    runner: Callable[..., subprocess.CompletedProcess[str]] | None = None,
) -> None:
    """Remove a scratch container and turn every cleanup failure into a finding.

    ``runner`` is an explicit test seam so the negative self-test never needs to
    create a real container.
    """
    execute = run if runner is None else runner
    try:
        cleanup = execute(["docker", "rm", "-f", name], timeout=60)
    except (OSError, subprocess.SubprocessError) as exc:
        report.fail(f"could not remove Docker scratch container {name}: {exc}")
        return
    if cleanup.returncode != 0:
        detail = (
            cleanup.stderr.strip()
            or cleanup.stdout.strip()
            or f"exit status {cleanup.returncode}"
        )
        report.fail(f"could not remove Docker scratch container {name}: {detail}")


def wait_for_docker_database(
    name: str,
    database: str,
    report: Report,
    *,
    runner: Callable[..., subprocess.CompletedProcess[str]] | None = None,
    sleeper: Callable[[float], None] = time.sleep,
    attempts: int = 60,
) -> bool:
    """Wait until the configured database accepts and answers a real query."""
    if attempts < 1:
        raise ValueError("Docker database readiness needs at least one attempt")

    execute = run if runner is None else runner
    command = [
        "docker",
        "exec",
        name,
        "psql",
        "-X",
        "-v",
        "ON_ERROR_STOP=1",
        "-qAt",
        "-U",
        "postgres",
        "-d",
        database,
        "-c",
        "SELECT 1",
    ]
    last_detail = "no diagnostic output"
    for attempt in range(1, attempts + 1):
        try:
            ready = execute(command, timeout=20)
        except (OSError, subprocess.SubprocessError) as exc:
            report.fail(
                f"could not run the Docker readiness query for database "
                f"{database}: {exc}"
            )
            return False

        if ready.returncode == 0 and ready.stdout.strip() == "1":
            return True
        if ready.returncode == 0:
            last_detail = (
                "readiness query returned unexpected output "
                f"{ready.stdout.strip()!r}"
            )
        else:
            last_detail = (
                ready.stderr.strip()
                or ready.stdout.strip()
                or f"exit status {ready.returncode}"
            )
        if attempt < attempts:
            sleeper(1)

    report.fail(
        f"Docker target database {database} did not accept SELECT 1 after "
        f"{attempts} attempts: {last_detail}"
    )
    return False


def apply_with_docker(verbose: bool, report: Report) -> bool:
    """Same pass against a throwaway container. True if it actually ran."""
    suffix = secrets.token_hex(6)
    database = f"pos_migration_check_{os.getpid()}_{suffix}"
    name = f"pos-pg-verify-{os.getpid()}-{suffix}"
    started = run([
        "docker", "run", "-d", "--rm", "--name", name,
        "-e", "POSTGRES_PASSWORD=verify", "-e", f"POSTGRES_DB={database}",
        PG_IMAGE,
    ])
    if started.returncode != 0:
        print("skipped the engine pass: Docker is installed but would not start a "
              f"container ({started.stderr.strip().splitlines()[-1:] or ['no output']})")
        return False

    try:
        if not wait_for_docker_database(name, database, report):
            return True

        for path in pg_migrations():
            sql = path.read_text(encoding="utf-8")
            done = run(
                [
                    "docker",
                    "exec",
                    "-i",
                    name,
                    "psql",
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-q",
                    "-U",
                    "postgres",
                    "-d",
                    database,
                    *psql_transaction_options(sql),
                    "-f",
                    "-",
                ],
                stdin=sql,
                timeout=180,
            )
            if done.returncode != 0:
                report.fail(f"{path.name} does not apply to PostgreSQL:\n{done.stderr.strip()}")
                break
            if verbose:
                print(f"  applied {path.name}")
    finally:
        remove_docker_container(name, report)
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
    real_register_local = REGISTER_LOCAL.copy()
    cases: list[tuple[str, dict[str, str], dict[str, str], bool]] = [
        ("a declared mirror passes",
         {"20260101000000_a.sql": "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, False),
        ("a malformed sqlx filename fails",
         {"oops.sql": "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
        ("duplicate sqlx migration versions fail",
         {"20260101000000_a.sql": "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n",
          "20260101000000_b.sql": "-- Server-only: split operation.\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
        ("an invalid calendar timestamp fails",
         {"20261301000000_a.sql": "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
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
        ("a no-transaction marker can precede the mapping declaration",
         {"20260101000000_a.sql":
          "-- no-transaction\n-- Server-only: creates a separate database.\n"
          "CREATE DATABASE verifier_fixture;\n"},
         {}, False),
        ("a server-only declaration requires a reason",
         {"20260101000000_a.sql": "-- Server-only\nSELECT 1;\n"},
         {}, True),
        ("an SQL string cannot impersonate a header declaration",
         {"20260101000000_a.sql": "SELECT 'Mirrors SQLite 0001_init.sql';\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
        ("a body comment cannot impersonate a header declaration",
         {"20260101000000_a.sql":
          "SELECT 1;\n-- Mirrors SQLite 0001_init.sql\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
        ("a block comment cannot impersonate a header declaration",
         {"20260101000000_a.sql":
          "/* Mirrors SQLite 0001_init.sql */\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
        ("ambiguous header declarations fail closed",
         {"20260101000000_a.sql":
          "-- Mirrors SQLite 0001_init.sql\n-- Server-only: contradictory.\nSELECT 1;\n"},
         {"0001_init.sql": "SELECT 1;\n"}, True),
    ]
    register_local_cases: list[
        tuple[
            str,
            dict[str, str],
            dict[str, str],
            dict[object, object],
            str | None,
        ]
    ] = [
        (
            "a canonical register-local exception with a reason passes",
            {},
            {"0001_init.sql": "SELECT 1;\n"},
            {
                "0001_init.sql": (
                    "This register-only table is never synchronized."
                )
            },
            None,
        ),
        (
            "a stale register-local migration fails",
            {
                "20260101000000_a.sql": (
                    "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"
                )
            },
            {"0001_init.sql": "SELECT 1;\n"},
            {"0009_ghost.sql": "This register-only table never synchronizes."},
            "keys must be exact filenames of existing SQLite migrations",
        ),
        (
            "a noncanonical register-local path fails",
            {
                "20260101000000_a.sql": (
                    "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"
                )
            },
            {"0001_init.sql": "SELECT 1;\n"},
            {"./0001_init.sql": "This register-only table never synchronizes."},
            "keys must be exact filenames of existing SQLite migrations",
        ),
        (
            "an empty register-local reason fails",
            {},
            {"0001_init.sql": "SELECT 1;\n"},
            {"0001_init.sql": "   "},
            "must have a nonblank, substantive reason",
        ),
        (
            "a placeholder register-local reason fails",
            {},
            {"0001_init.sql": "SELECT 1;\n"},
            {"0001_init.sql": "TODO"},
            "must have a nonblank, substantive reason",
        ),
        (
            "a non-string register-local reason fails",
            {},
            {"0001_init.sql": "SELECT 1;\n"},
            {"0001_init.sql": None},
            "must have a nonblank, substantive reason",
        ),
        (
            "duplicate mirror and register-local coverage fails",
            {
                "20260101000000_a.sql": (
                    "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"
                )
            },
            {"0001_init.sql": "SELECT 1;\n"},
            {"0001_init.sql": "This register-only table never synchronizes."},
            "covered by both REGISTER_LOCAL and a Postgres mirror declaration",
        ),
    ]

    failures = 0

    pin_cases = (
        ("a complete Postgres image digest passes", PG_IMAGE, False),
        (
            "a mutable Postgres tag is rejected",
            f"postgres:{PG_MAJOR}-alpine",
            True,
        ),
        (
            "a shortened image digest is rejected",
            f"postgres:{PG_MAJOR}-alpine@sha256:abc",
            True,
        ),
        # The pin is exact, not "any Postgres": a correctly-formed digest on a
        # different major is still an unreviewed bump.
        (
            "a complete digest on an unpinned major is rejected",
            "postgres:16-alpine@sha256:" + "c" * 64,
            True,
        ),
    )
    for label, image, want_failure in pin_cases:
        passed = (IMAGE_PIN.fullmatch(image) is None) == want_failure
        print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
        failures += not passed

    transaction_cases = (
        (
            "ordinary migrations run in SQLx's transaction boundary",
            "VACUUM;\n",
            ["--single-transaction"],
        ),
        (
            "the exact byte-zero SQLx marker opts out of the transaction",
            "-- no-transaction\nVACUUM;\n",
            [],
        ),
        (
            "a marker after a leading newline does not opt out",
            "\n-- no-transaction\nVACUUM;\n",
            ["--single-transaction"],
        ),
        (
            "the SQLx marker is case-sensitive",
            "-- NO-TRANSACTION\nVACUUM;\n",
            ["--single-transaction"],
        ),
    )
    for label, sql, expected in transaction_cases:
        passed = psql_transaction_options(sql) == expected
        print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
        failures += not passed

    compose_ok = (
        "services:\n"
        "  postgres:\n"
        f"    image: {PG_IMAGE}\n"
    )
    ci_ok = (
        "jobs:\n"
        "  rust:\n"
        "    services:\n"
        "      postgres:\n"
        f"        image: {PG_IMAGE}\n"
    )
    image_surface_cases = (
        ("the Compose PostgreSQL image scalar passes", compose_ok, "compose", False),
        ("the CI PostgreSQL image scalar passes", ci_ok, "ci", False),
        (
            "a quoted direct PostgreSQL image scalar passes",
            "services:\n  postgres:\n" f'    image: "{PG_IMAGE}"\n',
            "compose",
            False,
        ),
        (
            "an additional non-Postgres service image is allowed",
            compose_ok + "  redis:\n    image: redis:8-alpine\n",
            "compose",
            False,
        ),
        (
            "a mutable image plus the digest in a comment fails",
            "services:\n  postgres:\n"
            f"    image: postgres:latest # image: {PG_IMAGE}\n",
            "compose",
            True,
        ),
        (
            "a mutable image plus the digest in a string fails",
            "services:\n  postgres:\n    image: postgres:latest\n"
            f'metadata:\n  note: "image: {PG_IMAGE}"\n',
            "compose",
            True,
        ),
        (
            "an unrelated pinned image cannot mask the mutable service",
            "services:\n  postgres:\n    image: postgres:latest\n"
            f"  decoy:\n    image: {PG_IMAGE}\n",
            "compose",
            True,
        ),
        (
            "a mutable Postgres image under a different service fails",
            compose_ok + "  database:\n    image: postgres:latest\n",
            "compose",
            True,
        ),
        (
            "a registry-qualified Postgres image under another service fails",
            compose_ok
            + "  database:\n    image: docker.io/library/postgres:latest\n",
            "compose",
            True,
        ),
        (
            "quoted service and image keys cannot hide another Postgres image",
            compose_ok + '  "database":\n    "image": postgres:latest\n',
            "compose",
            True,
        ),
        (
            "a quoted flow-mapping duplicate service fails closed",
            compose_ok + '  "postgres": {image: postgres:latest}\n',
            "compose",
            True,
        ),
        (
            "duplicate PostgreSQL image keys fail closed",
            "services:\n  postgres:\n"
            f"    image: {PG_IMAGE}\n    image: {PG_IMAGE}\n",
            "compose",
            True,
        ),
        (
            "a dynamic additional service image fails closed",
            compose_ok + "  database:\n    image: ${DATABASE_IMAGE}\n",
            "compose",
            True,
        ),
        (
            "an overriding duplicate PostgreSQL service fails closed",
            "services:\n  postgres:\n"
            f"    image: {PG_IMAGE}\n"
            "  postgres:\n    build: ./untrusted-postgres\n",
            "compose",
            True,
        ),
        (
            "an image alias is rejected as ambiguous",
            f"x-pg-image: &pg {PG_IMAGE}\n"
            "services:\n  postgres:\n    image: *pg\n",
            "compose",
            True,
        ),
        (
            "a flow-mapping image is rejected as ambiguous",
            f"services: {{postgres: {{image: {PG_IMAGE}}}}}\n",
            "compose",
            True,
        ),
        (
            "multiple CI PostgreSQL service images fail closed",
            ci_ok
            + "  integration:\n    services:\n      postgres:\n"
            + f"        image: {PG_IMAGE}\n",
            "ci",
            True,
        ),
    )
    for label, text, surface, want_failure in image_surface_cases:
        report = Report()
        audit_image_text(report, text, surface, "fixture.yml")
        got_failure = bool(report.failures)
        passed = got_failure == want_failure
        print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
        failures += not passed

    names = {scratch_database_name() for _ in range(20)}
    safe_names = len(names) == 20 and all(
        re.fullmatch(r"[a-z][a-z0-9_]{0,62}", name) for name in names
    )
    if safe_names:
        print("  ok    scratch database names are unique, safe identifiers")
    else:
        print("  FAIL  scratch database names are unique, safe identifiers")
        failures += 1

    probe_name = scratch_database_name()
    rewritten = scratch_url(
        "postgresql://user:pass@localhost:5432/app?sslmode=disable", probe_name
    )
    url_ok = rewritten == (
        f"postgresql://user:pass@localhost:5432/{probe_name}?sslmode=disable"
    )
    if url_ok:
        print("  ok    scratch URL preserves connection options")
    else:
        print("  FAIL  scratch URL preserves connection options")
        failures += 1

    cleanup_report = Report()
    cleanup_calls: list[list[str]] = []

    def failing_cleanup_runner(argv, **_kwargs):
        cleanup_calls.append(argv)
        return subprocess.CompletedProcess(
            argv, returncode=1, stdout="", stderr="permission denied"
        )

    remove_docker_container(
        "pos-pg-verify-fixture", cleanup_report, runner=failing_cleanup_runner
    )
    cleanup_ok = cleanup_calls == [
        ["docker", "rm", "-f", "pos-pg-verify-fixture"]
    ] and any(
        "could not remove Docker scratch container pos-pg-verify-fixture"
        in message
        for message in cleanup_report.failures
    )
    print(
        f"  {'ok  ' if cleanup_ok else 'FAIL'}  "
        "a Docker cleanup failure becomes a verifier failure"
    )
    failures += not cleanup_ok

    transition_report = Report()
    transition_calls: list[list[str]] = []
    transition_sleeps: list[float] = []

    def database_creation_transition(argv, **_kwargs):
        transition_calls.append(argv)
        if len(transition_calls) == 1:
            return subprocess.CompletedProcess(
                argv,
                returncode=1,
                stdout="",
                stderr=(
                    'FATAL: database "pos_migration_check_fixture" does not exist'
                ),
            )
        return subprocess.CompletedProcess(
            argv, returncode=0, stdout="1\n", stderr=""
        )

    transition_ready = wait_for_docker_database(
        "pos-pg-verify-fixture",
        "pos_migration_check_fixture",
        transition_report,
        runner=database_creation_transition,
        sleeper=transition_sleeps.append,
        attempts=3,
    )
    transition_ok = (
        transition_ready
        and not transition_report.failures
        and len(transition_calls) == 2
        and transition_sleeps == [1]
        and all("pg_isready" not in call for call in transition_calls)
        and all(call[-2:] == ["-c", "SELECT 1"] for call in transition_calls)
        and all(
            call[call.index("-d") + 1] == "pos_migration_check_fixture"
            for call in transition_calls
        )
    )
    print(
        f"  {'ok  ' if transition_ok else 'FAIL'}  "
        "readiness waits through server-ready/database-not-created"
    )
    failures += not transition_ok

    timeout_report = Report()
    timeout_calls: list[list[str]] = []
    timeout_sleeps: list[float] = []

    def database_never_created(argv, **_kwargs):
        timeout_calls.append(argv)
        return subprocess.CompletedProcess(
            argv,
            returncode=1,
            stdout="",
            stderr='FATAL: database "pos_migration_check_fixture" does not exist',
        )

    timeout_ready = wait_for_docker_database(
        "pos-pg-verify-fixture",
        "pos_migration_check_fixture",
        timeout_report,
        runner=database_never_created,
        sleeper=timeout_sleeps.append,
        attempts=3,
    )
    timeout_ok = (
        not timeout_ready
        and len(timeout_calls) == 3
        and timeout_sleeps == [1, 1]
        and any(
            "did not accept SELECT 1 after 3 attempts" in message
            and "does not exist" in message
            for message in timeout_report.failures
        )
    )
    print(
        f"  {'ok  ' if timeout_ok else 'FAIL'}  "
        "database-readiness timeout fails with the last diagnostic"
    )
    failures += not timeout_ok

    try:
        for label, pg_files, sqlite_files, want_failure in cases:
            REGISTER_LOCAL.clear()
            with tempfile.TemporaryDirectory() as tmp:
                PG_DIR = Path(tmp) / "pg"
                SQLITE_DIR = Path(tmp) / "sqlite"
                PG_DIR.mkdir()
                SQLITE_DIR.mkdir()
                for name, body in pg_files.items():
                    (PG_DIR / name).write_text(body, encoding="utf-8")
                for name, body in sqlite_files.items():
                    (SQLITE_DIR / name).write_text(body, encoding="utf-8")
                report = Report()
                audit_mapping(report)
                got_failure = bool(report.failures)
                if got_failure == want_failure:
                    print(f"  ok    {label}")
                else:
                    print(
                        f"  FAIL  {label} "
                        f"(wanted failure={want_failure}, got {got_failure})"
                    )
                    failures += 1

        for (
            label,
            pg_files,
            sqlite_files,
            register_local,
            expected_failure,
        ) in register_local_cases:
            REGISTER_LOCAL.clear()
            REGISTER_LOCAL.update(register_local)
            with tempfile.TemporaryDirectory() as tmp:
                PG_DIR = Path(tmp) / "pg"
                SQLITE_DIR = Path(tmp) / "sqlite"
                PG_DIR.mkdir()
                SQLITE_DIR.mkdir()
                for name, body in pg_files.items():
                    (PG_DIR / name).write_text(body, encoding="utf-8")
                for name, body in sqlite_files.items():
                    (SQLITE_DIR / name).write_text(body, encoding="utf-8")
                report = Report()
                audit_mapping(report)
                passed = (
                    not report.failures
                    if expected_failure is None
                    else any(
                        expected_failure in message for message in report.failures
                    )
                )
                print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
                failures += not passed

        for engine in ("Postgres", "SQLite"):
            REGISTER_LOCAL.clear()
            with tempfile.TemporaryDirectory() as tmp:
                PG_DIR = Path(tmp) / "pg"
                SQLITE_DIR = Path(tmp) / "sqlite"
                PG_DIR.mkdir()
                SQLITE_DIR.mkdir()
                pg_path = PG_DIR / "20260101000000_a.sql"
                sqlite_path = SQLITE_DIR / "0001_init.sql"
                pg_body = "-- Mirrors SQLite 0001_init.sql\nSELECT 1;\n"
                sqlite_body = "SELECT 1;\n"
                target = Path(tmp) / f"mutable_{engine.casefold()}.sql"
                target.write_text(
                    pg_body if engine == "Postgres" else sqlite_body,
                    encoding="utf-8",
                )
                linked = pg_path if engine == "Postgres" else sqlite_path
                regular = sqlite_path if engine == "Postgres" else pg_path
                regular.write_text(
                    sqlite_body if engine == "Postgres" else pg_body,
                    encoding="utf-8",
                )
                try:
                    linked.symlink_to(target)
                    report = Report()
                    audit_mapping(report)
                except OSError:
                    class SyntheticSymlink:
                        def is_symlink(self) -> bool:
                            return True

                        def relative_to(self, _root: Path) -> Path:
                            raise ValueError

                        def __str__(self) -> str:
                            # Constructed and consumed inside this iteration, so
                            # the late binding B023 warns about cannot occur.
                            return linked.name  # noqa: B023

                    report = Report()
                    audit_migration_file_types(
                        report,
                        [SyntheticSymlink()],  # type: ignore[list-item]
                        engine,
                    )
                passed = any(
                    f"regular {engine} migration file" in message
                    and "symbolic links" in message
                    for message in report.failures
                )
                print(
                    f"  {'ok  ' if passed else 'FAIL'}  "
                    f"a {engine} migration symlink to mutable bytes fails"
                )
                failures += not passed
    finally:
        PG_DIR, SQLITE_DIR = real_pg, real_sqlite
        REGISTER_LOCAL.clear()
        REGISTER_LOCAL.update(real_register_local)

    total = (
        len(cases)
        + len(register_local_cases)
        + len(pin_cases)
        + len(transaction_cases)
        + len(image_surface_cases)
        + 7
    )
    print(f"\nmapping audit self-test: {total - failures} passed, {failures} failed")
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
    audit_image_pin(report)
    audit_mapping(report, verbose)
    print(f"mapping: {len(pg_migrations())} Postgres migration(s) declared against "
          f"{len(sqlite_migrations())} SQLite migration(s)")

    if "--mapping-only" in sys.argv:
        ran = False
    elif report.failures:
        ran = False
        print("engine: SKIPPED — mapping/image/filename audit failed")
    else:
        ran = engine_pass(verbose, report)
        if ran and report.failures:
            print("engine: FAILED — real PostgreSQL execution reported a problem")
        elif ran:
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
