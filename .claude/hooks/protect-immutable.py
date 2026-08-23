#!/usr/bin/env python3
"""PreToolUse guard: refuse protected writes and reads of live env files.

Two rules, both from docs/implementation/01-conventions.md:

  1. §9 — a migration that is already committed is never edited. Not for a typo.
     Other databases have already applied it; the fix is the next migration.
  2. docs/plan/** are source documents — inputs to the implementation set, not
     working documents. Corrections belong in docs/implementation/.
  3. Any ``.env`` or ``.env.<suffix>`` file is secret-bearing input and may not
     be read. The one tracked ``.env.example`` template remains readable.

One script covers every write surface so a shell-matcher hook costs a single
interpreter start-up:

  Read / Grep / Glob                        live .env paths and patterns
  Edit / Write / MultiEdit / NotebookEdit  exact, on tool_input.file_path
  Bash / PowerShell / Monitor              the command arm, below

Both protected things are *directories*, not only the files in them, so the
shell arm refuses `rm -rf docs/plan` and `rm -rf crates/pos-db/migrations` as
well as a write to a named file inside either.

What the shell arm covers
-------------------------
Each command is split into segments, and each segment is tokenised with `shlex`
so quoting is handled by the shell's own rules rather than a regex. A token is
refused when it is the target of

  * an in-place or destructive verb  (sed -i, perl -i, rm, mv, git rm/mv,
    truncate, patch, shred, dd, unlink, rmdir, touch, PowerShell write cmdlets)
  * a redirect                       (>, >>, 2>, tee)
  * the destination of a copy        (cp, install, rsync, ln — the LAST path
                                      argument only, so reading *out of* a
                                      protected directory still works)
  * an explicit output option         (`git diff/show --output`, `cp -t`)
  * an interpreter invocation that names a protected path literally

`cd` is followed between segments, so `cd docs && rm -rf plan` resolves to the
same path as `rm -rf docs/plan`. A `cd` inside a subshell is assumed to persist,
which over-approximates; for an immutable file, erring toward refusal is the
safe direction.

Limitations
-----------
An interpreter invocation that literally names a protected path is refused,
even if the script intended only to read it. Arbitrary code can still construct
the same path dynamically, and a patch file can name a target the command line
does not. No command-string parser can prove the behavior of arbitrary code, so
the other layers remain mandatory:

  * `.claude/settings.json` denies writes under docs/plan at both the built-in
    Edit permission layer and the OS sandbox boundary on supported platforms.
  * `.githooks/pre-commit` refuses a staged change *or deletion* of a committed
    migration.
  * CI's `guards` job re-runs this file's own test suite on every change.

Internal parser errors fail open with a visible warning so a malformed command
cannot brick every edit in the repository. The exec launcher itself fails closed
when Python cannot start, while the permission and sandbox layers remain the
non-parser backstops.

Negative-tested by ./test-protect-immutable.sh — run it after any change here.
A guard nobody has seen fail is a guard nobody should trust.
"""

from __future__ import annotations

import fnmatch
import json
import os
import re
import shlex
import subprocess
import sys
from functools import lru_cache
from pathlib import Path

WRITE_TOOLS = frozenset({"Edit", "Write", "MultiEdit", "NotebookEdit"})
READ_TOOLS = frozenset({"Read", "Grep", "Glob"})
# Every tool that hands a command line to a shell. Monitor runs its `command`
# in the same shell Bash does, so it needs the same arm — a guard that covers
# one shell surface and not the next one to ship is a guard with a date on it.
SHELL_TOOLS = frozenset({"Bash", "PowerShell", "Monitor"})

MIGRATION = re.compile(r"(?:^|/)migrations/[^/]+\.sql$", re.IGNORECASE)
MIGRATION_DIR = re.compile(r"(?:^|/)migrations$", re.IGNORECASE)
PLANS = "docs/plan"

# Cheap relevance test, so an unrelated shell call never pays for a git
# subprocess. "plan" is here bare, not as "docs/plan", because `cd docs && rm
# -rf plan` never spells the full path in one string; the cost of the wider net
# is one `git rev-parse` on a command that happens to contain the word.
RELEVANT = ("migrations", ".sql", "docs/plan", "plan")

# A compound command is many commands. Scanning it as one blob lets a write verb
# quoted in one place — a commit message, a comment — implicate a path named
# anywhere else in the same call, so each arm is matched per segment instead.
SEGMENT = re.compile(r"\n|;|&&|\|\||\||\$\(|`")

# Verbs that rewrite or remove any path named in their arguments. The PowerShell
# names are here because the register ships on Windows: `just` already switches to
# powershell.exe there, so a contributor on Windows writes Remove-Item, and a guard
# that only knows POSIX verbs would wave it through.
IN_PLACE = re.compile(
    r"\bsed\s+-[a-zA-Z]*i"
    r"|\bperl\s+-[a-zA-Z]*i"
    r"|\bgit\s+(?:rm|mv|restore|checkout|clean|reset)\b"
    r"|\b(?:rm|mv|truncate|patch|shred|dd|unlink|rmdir|touch)\b"
    r"|\b(?:Remove-Item|Move-Item|Clear-Content|Rename-Item|Set-Item|"
    r"Remove-ItemProperty)\b"
    r"|\b(?:del|erase|ri|mi|ren|clc)\b",
    re.IGNORECASE,
)

# Verbs whose LAST path argument is a destination and whose earlier ones are
# sources. `cp docs/plan/x /tmp/` reads; `cp /tmp/x docs/plan/x` overwrites.
# Distinguishing the two by position is what lets `cp` be covered at all — as
# an all-arguments verb it produced more false denials than it prevented.
DEST_ONLY = frozenset({"cp", "install", "rsync", "ln", "copy-item"})

# Cmdlets that write every path they are handed, like `tee`.
WRITE_THROUGH = frozenset(
    {"tee", "out-file", "set-content", "add-content", "new-item"}
)

# Wrappers to look past when identifying the verb.
TRANSPARENT = frozenset({"sudo", "command", "env", "time", "nohup", "xargs"})

# Inline interpreters can conceal the actual write behind arbitrary source. If
# one literally names a protected path, refuse it conservatively. A dynamically
# assembled path remains outside what command-line inspection can prove.
INTERPRETER = re.compile(
    r"^(?:python(?:\d+(?:\.\d+)*)?|pypy(?:\d+)?|node|bun|deno|ruby|perl|php|"
    r"lua|pwsh|powershell(?:\.exe)?|bash|sh|zsh)$",
    re.IGNORECASE,
)
PROTECTED_LITERAL = re.compile(
    r"(?P<path>(?:[A-Za-z]:)?/?[A-Za-z0-9_./\\~@+-]*"
    r"(?:docs[\\/]+plan(?:[\\/]+[A-Za-z0-9_.@+-]+)*|"
    r"migrations[\\/]+[A-Za-z0-9_.@+-]+\.sql))",
    re.IGNORECASE,
)

REDIRECTS = frozenset({">", ">>", ">|", "1>", "1>>", "2>", "2>>", "&>", "&>>"})
REDIR_PREFIX = re.compile(r"^[12&]?>>?\|?")
# `dd of=path`, `git diff --output=path`: the path is the tail of an assignment.
ASSIGN_PREFIX = re.compile(r"^(?:--?[A-Za-z0-9_-]+=|[A-Za-z_][A-Za-z0-9_]*=)")
# Conservative path alphabet, with an optional Windows drive prefix. Rejects
# `2>&1`'s `&1`, `$VAR`, `:memory:`, globs — none of which name a resolvable path.
PATHISH = re.compile(r"^(?:[A-Za-z]:)?[A-Za-z0-9_./~@+][A-Za-z0-9_./~@+-]*$")
# A backslash before a path character is a Windows separator; a backslash before a
# space, quote, or `$` is a shell escape. Normalising only the former lets
# `crates\pos-db\migrations\0001_init.sql` survive shlex, which would otherwise
# eat every backslash as an escape and leave one unrecognisable word.
WIN_SEP = re.compile(r"\\(?=[A-Za-z0-9_.])")
# The flag repetition is bounded, not `*`: an unbounded nested quantifier over
# `\s+` backtracks exponentially on a hostile argument list, and no real `cd`
# carries four flags.
CD = re.compile(r"\bcd\s+(?:-{1,2}[A-Za-z0-9_-]+\s+){0,4}([^\s;|&)]+)")


class GuardOperationalError(RuntimeError):
    """A tooling failure that prevents the guard from reaching a verdict."""


def visible_warning(message: str) -> None:
    """Send fail-open diagnostics through the hook's visible stdout channel."""
    print(json.dumps({"systemMessage": message}))


def repo_root(cwd: str) -> Path:
    try:
        done = subprocess.run(
            ["git", "-C", cwd, "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, timeout=5,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise GuardOperationalError(f"git rev-parse could not run: {exc}") from exc
    root = done.stdout.strip()
    if done.returncode != 0 or not root:
        raise GuardOperationalError(
            f"git rev-parse could not resolve the repository (exit {done.returncode})"
        )
    return Path(root)


@lru_cache(maxsize=4)
def head_paths(root: Path) -> tuple[str, ...]:
    """Every path in HEAD, or an operational error if Git cannot enumerate it."""
    try:
        done = subprocess.run(
            ["git", "-C", str(root), "ls-tree", "-r", "--name-only", "HEAD"],
            capture_output=True,
            text=True,
            timeout=5,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise GuardOperationalError(f"git ls-tree could not run: {exc}") from exc
    if done.returncode != 0:
        raise GuardOperationalError(
            f"git ls-tree could not inspect HEAD (exit {done.returncode})"
        )
    return tuple(line for line in done.stdout.splitlines() if line)


def to_relative(root: Path, raw: str, cwd: str) -> str | None:
    try:
        target = Path(raw)
        if not target.is_absolute():
            target = Path(cwd) / target
        # Git commonly canonicalizes `/var` to `/private/var` on macOS. Resolve
        # both sides so a symlinked working directory cannot make an in-repo
        # protected path appear to sit outside the repository.
        return os.path.relpath(
            target.resolve(strict=False),
            root.resolve(strict=False),
        ).replace(os.sep, "/")
    except (OSError, RuntimeError, ValueError) as exc:
        raise GuardOperationalError(f"could not resolve guarded path: {exc}") from exc


def is_committed(root: Path, relative: str) -> bool:
    """True when the path exists in HEAD — committed, not merely staged."""
    # macOS and Windows commonly use case-insensitive worktrees while Git's
    # object database remains case-sensitive, so compare the authoritative tree
    # with case folding rather than treating a `cat-file` miss as a verdict.
    wanted = relative.casefold()
    return any(line.casefold() == wanted for line in head_paths(root))


def holds_committed_migration(root: Path, relative: str) -> bool:
    """True when HEAD has a migration inside this directory."""
    prefix = relative.rstrip("/").casefold() + "/"
    return any(
        line.casefold().startswith(prefix) and MIGRATION.search(line)
        for line in head_paths(root)
    )


def committed_migration_basenames(root: Path) -> set[str]:
    """Basenames of every migration in HEAD, for tokens that carry no directory."""
    return {
        line.rsplit("/", 1)[-1].casefold()
        for line in head_paths(root)
        if MIGRATION.search(line)
    }


def refusal(root: Path, relative: str) -> str | None:
    """The reason this path may not be written, or None if it may."""
    if relative.startswith(".."):
        return None  # outside the repository; not ours to police
    folded = relative.casefold()
    if folded == PLANS or folded.startswith(PLANS + "/"):
        return plan_refusal(relative)
    if MIGRATION.search(relative) and is_committed(root, relative):
        return migration_refusal(relative)
    if MIGRATION_DIR.search(relative) and holds_committed_migration(root, relative):
        return (
            f"BLOCKED: {relative}/ holds committed migrations.\n"
            "Removing or moving the directory removes them, which is the same "
            "forbidden edit with a wider blast radius (01-conventions.md §9)."
        )
    return None


def plan_refusal(display: str) -> str:
    return (
        f"BLOCKED: {display} is a source document.\n"
        "docs/plan/** are inputs to the implementation set, never working documents "
        "(CLAUDE.md, 'The plan'). If the plan is wrong, record the correction in "
        "docs/implementation/ — that set is the plan of record."
    )


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


def check_sensitive_read(
    root: Path,
    cwd: str,
    tool_name: object,
    tool_input: dict[str, object],
) -> str | None:
    """Refuse live ``.env`` paths/patterns, including symlink aliases."""
    path_value = tool_input.get("file_path") if tool_name == "Read" else tool_input.get("path")
    pattern_value = (
        tool_input.get("glob") if tool_name == "Grep" else tool_input.get("pattern")
    )
    candidates: list[str] = []
    if isinstance(path_value, str) and path_value:
        candidates.append(path_value)
    if isinstance(pattern_value, str) and pattern_value:
        if isinstance(path_value, str) and path_value:
            candidates.append(str(Path(path_value) / pattern_value))
        else:
            candidates.append(pattern_value)

    safe_example = "apps/server/.env.example"
    glob_chars = frozenset("*?[]{}")

    def env_component(value: str) -> bool:
        folded = value.casefold()
        return folded == ".env" or folded.startswith(".env.")

    def brace_variants(value: str) -> list[str]:
        """Expand simple/nested brace alternatives for conservative matching."""
        opening = value.find("{")
        if opening < 0:
            return [value]
        depth = 0
        for index in range(opening, len(value)):
            if value[index] == "{":
                depth += 1
            elif value[index] == "}":
                depth -= 1
                if depth == 0:
                    body = value[opening + 1 : index]
                    choices: list[str] = []
                    start = 0
                    nested = 0
                    for cursor, character in enumerate(body):
                        if character == "{":
                            nested += 1
                        elif character == "}":
                            nested -= 1
                        elif character == "," and nested == 0:
                            choices.append(body[start:cursor])
                            start = cursor + 1
                    choices.append(body[start:])
                    expanded: list[str] = []
                    for choice in choices:
                        expanded.extend(
                            brace_variants(
                                value[:opening] + choice + value[index + 1 :]
                            )
                        )
                    return expanded
        # An unterminated brace is ambiguous. Treat it as capable rather than
        # letting malformed discovery syntax weaken the secret boundary.
        return ["*"]

    def glob_component_can_match_env(value: str) -> bool:
        # Probe the protected basename language, not just literal `.env` text.
        # This catches `[.]env*`, `.e?v*`, braces, and broad discovery globs.
        suffixes = {
            "",
            ".",
            ".prod",
            ".production",
            ".qa",
            ".preview",
            ".backup",
            ".secret",
            ".example",
            ".sample",
            ".template",
        }
        suffixes.update(
            f".{character * length}"
            for character in "a0_-"
            for length in range(1, 33)
        )
        probes = [".env" + suffix for suffix in suffixes]
        return any(
            fnmatch.fnmatchcase(probe, variant.casefold())
            for variant in brace_variants(value.casefold())
            for probe in probes
        )

    for raw in candidates:
        normalized = raw.replace("\\", "/")
        components = [part for part in normalized.split("/") if part not in {"", "."}]
        mentions_env = any(env_component(part) for part in components)
        has_glob = any(char in normalized for char in glob_chars)
        if has_glob and components:
            mentions_env = mentions_env or glob_component_can_match_env(components[-1])

        resolved_relative: str | None = None
        if not has_glob:
            target = Path(normalized)
            if not target.is_absolute():
                target = Path(cwd) / target
            try:
                resolved_relative = os.path.relpath(
                    target.resolve(strict=False), root.resolve(strict=False)
                ).replace(os.sep, "/")
            except (OSError, RuntimeError, ValueError) as exc:
                raise GuardOperationalError(
                    f"could not resolve sensitive read target: {exc}"
                ) from exc
            resolved_parts = [
                part for part in resolved_relative.split("/") if part not in {"", "."}
            ]
            mentions_env = mentions_env or any(
                env_component(part) for part in resolved_parts
            )

        if not mentions_env:
            continue
        if not has_glob and resolved_relative.casefold() == safe_example.casefold():
            continue
        return (
            f"BLOCKED: {raw} targets a live environment file or pattern.\n"
            "Claude may read only the tracked apps/server/.env.example template, "
            "not .env, arbitrary .env.<suffix> files, aliases, or discovery globs."
        )
    return None


def tokenise(segment: str) -> list[str]:
    """The segment's words, quoting resolved the way the shell would resolve it."""
    segment = WIN_SEP.sub("/", segment)
    try:
        return shlex.split(segment, comments=True)
    except ValueError:
        return segment.split()  # unbalanced quote; a rough split still beats nothing


def as_path(token: str) -> str | None:
    """The path a token names, or None if it does not name one."""
    token = ASSIGN_PREFIX.sub("", token)
    token = REDIR_PREFIX.sub("", token)
    # A Windows separator names the same file. shlex has already resolved
    # backslash escaping, so what survives here is a path, not an escape.
    token = token.replace("\\", "/")
    if not token or token.startswith("-"):
        return None
    return token if PATHISH.match(token) else None


def verb_of(tokens: list[str]) -> str:
    """The command being run, looking past sudo/env-style wrappers."""
    for token in tokens:
        if token.startswith("-") or "=" in token:
            continue
        name = os.path.basename(token).lower()
        if name in TRANSPARENT:
            continue
        return name
    return ""


def option_targets(tokens: list[str], names: frozenset[str]) -> set[str]:
    """Paths supplied to a named option as `--name path` or `--name=path`."""
    targets: set[str] = set()
    for index, token in enumerate(tokens):
        lowered = token.casefold()
        if lowered in names:
            if index + 1 < len(tokens) and (path := as_path(tokens[index + 1])):
                targets.add(path)
            continue
        for name in names:
            if lowered.startswith(name + "=") and (path := as_path(token)):
                targets.add(path)
    return targets


def write_targets(segment: str) -> set[str]:
    """Every token in this segment that the shell would write to."""
    tokens = tokenise(segment)
    if not tokens:
        return set()

    paths = [(i, p) for i, token in enumerate(tokens) if (p := as_path(token))]
    targets: set[str] = set()

    verb = verb_of(tokens)
    if IN_PLACE.search(segment):
        targets.update(path for _, path in paths)
    elif verb in DEST_ONLY:
        targets.update(
            option_targets(
                tokens,
                frozenset({"-t", "--target-directory", "-destination"}),
            )
        )
        # The destination is the LAST argument, so read that position directly.
        # Never fall back to an earlier path token: when a destination this parser
        # does not recognise sits in that slot, "the last thing that looked like a
        # path" is the *source* — and refusing a copy out of a protected directory
        # is precisely the false denial that kept `cp` uncovered to begin with.
        args = [token for token in tokens if not token.startswith("-")][1:]
        if not targets and args and (destination := as_path(args[-1])):
            targets.add(destination)

    if verb == "git" and any(
        token.casefold() in {"diff", "show", "log", "archive", "format-patch"}
        for token in tokens[1:]
    ):
        targets.update(option_targets(tokens, frozenset({"--output", "-o"})))

    if INTERPRETER.match(verb):
        targets.update(
            match.group("path").replace("\\", "/")
            for match in PROTECTED_LITERAL.finditer(segment)
        )

    # A redirect writes its target wherever it appears, including inside a
    # segment whose verb only reads.
    for i, token in enumerate(tokens):
        if token in REDIRECTS:
            if i + 1 < len(tokens) and (path := as_path(tokens[i + 1])):
                targets.add(path)
        elif REDIR_PREFIX.match(token) and (path := as_path(token)):
            targets.add(path)

    through = [
        i for i, t in enumerate(tokens)
        if os.path.basename(t).lower() in WRITE_THROUGH
    ]
    if through:
        targets.update(path for i, path in paths if i > through[0])

    return targets


def next_cwd(segment: str, cwd: str) -> str:
    """The directory later segments run in, after any `cd` in this one."""
    found = CD.search(segment)
    if not found:
        return cwd
    target = found.group(1).strip("'\"")
    if not target or target.startswith(("$", "-")):
        return cwd  # unresolvable; keep what we had
    return os.path.normpath(target if os.path.isabs(target) else os.path.join(cwd, target))


def check_shell(root: Path, cwd: str, command: str) -> str | None:
    """The reason this command line may not run, or None if it may."""
    cache: dict[str, set[str]] = {}

    def basenames() -> set[str]:
        if "committed" not in cache:
            cache["committed"] = committed_migration_basenames(root)
        return cache["committed"]

    def reason_for(token: str, here: str) -> str | None:
        relative = to_relative(root, token, here)
        reason = refusal(root, relative) if relative else None
        if reason or "/" in token or not token.casefold().endswith(".sql"):
            return reason
        # A bare filename whose directory was reached in a way we could not
        # resolve — a variable, a symlink, an earlier `pushd`.
        return migration_refusal(token) if token.casefold() in basenames() else None

    here = cwd
    for segment in SEGMENT.split(command):
        for token in write_targets(segment):
            if reason := reason_for(token, here):
                return f"{reason}\n(Detected in a shell command that writes to it.)"
        here = next_cwd(segment, here)
    return None


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (OSError, ValueError) as exc:  # json.JSONDecodeError is one
        visible_warning(f"Immutable-path guard received malformed input; allowing: {exc}")
        return 0
    if not isinstance(payload, dict):
        visible_warning("Immutable-path guard payload is not an object; allowing the tool call.")
        return 0

    tool_name = payload.get("tool_name")
    tool_input = payload.get("tool_input")
    cwd = payload.get("cwd") or os.getcwd()
    if not isinstance(tool_input, dict) or not isinstance(cwd, str):
        visible_warning("Immutable-path guard payload is incomplete; allowing the tool call.")
        return 0

    # Decide irrelevance without touching git: this hook is on the shell matcher,
    # so it runs on every shell call and must cost almost nothing in the common case.
    if tool_name in READ_TOOLS:
        subject = (
            tool_input.get("file_path")
            or tool_input.get("path")
            or tool_input.get("glob")
            or tool_input.get("pattern")
        )
    elif tool_name in WRITE_TOOLS:
        subject = tool_input.get("file_path") or tool_input.get("notebook_path")
    elif tool_name in SHELL_TOOLS:
        subject = tool_input.get("command")
    else:
        return 0
    if not isinstance(subject, str):
        visible_warning("Immutable-path guard payload has no path or command; allowing.")
        return 0
    if tool_name in READ_TOOLS:
        try:
            root = repo_root(cwd)
            reason = check_sensitive_read(root, cwd, tool_name, tool_input)
        except GuardOperationalError as exc:
            print(
                "BLOCKED: sensitive-read policy could not resolve the requested "
                f"target safely ({exc}).",
                file=sys.stderr,
            )
            return 2
        if reason:
            print(reason, file=sys.stderr)
            return 2
        return 0
    if not any(hint in subject.casefold() for hint in RELEVANT):
        return 0

    root = repo_root(cwd)

    if tool_name in SHELL_TOOLS:
        reason = check_shell(root, cwd, subject)
    else:
        reason = check_file_write(root, cwd, tool_input)

    if reason:
        print(reason, file=sys.stderr)
        return 2  # PreToolUse: deny, and show stderr to Claude
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    # Fail open: never brick the session over a bug in the guard.
    except Exception as exc:  # noqa: BLE001
        visible_warning(f"Immutable-path guard error; allowing the tool call: {exc}")
        sys.exit(0)
