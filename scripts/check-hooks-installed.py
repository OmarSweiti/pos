#!/usr/bin/env python3
"""Prove Git will actually run the committed hooks in THIS working tree.

`.githooks/test-hooks.sh` invokes each hook by absolute path, so it proves the
hook SCRIPTS still refuse and proves nothing about whether Git will ever call
them. Four states disable all three hooks while every other diagnostic stays
green:

  * a clone that never ran `just setup`, so `core.hooksPath` is unset;
  * `core.hooksPath` pointing somewhere else entirely;
  * the path is configured and correct, but `.githooks/` is absent from the
    working tree — `git sparse-checkout set crates/pos-domain` drops top-level
    directories, and a checkout of a revision predating the hooks does the same.
    Git resolves a RELATIVE `core.hooksPath` against the working tree, so the
    configuration can be right and still resolve to nothing;
  * `core.hooksPath` naming a directory that no longer exists. It is SHARED
    across linked worktrees, so an absolute path written from inside one
    survives that worktree's removal and points nowhere. Git skips a missing
    hooks directory as silently as it skips a missing hook;
  * a hook that lost its exec bit, which Git skips in total silence.

Enforcement is impossible — `--no-verify` and an un-run `just setup` are both
outside any check's reach. DETECTION is one command, which is why the honest
limit is "the hooks are bypassable", not "nothing can be done".

Usage:  ./scripts/check-hooks-installed.py
        ./scripts/check-hooks-installed.py --self-test
Exit:   0 Git will run the hooks · 1 the hooks are not wired · 2 bad invocation
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

HOOKS_DIRNAME = ".githooks"
REQUIRED_HOOKS = ("commit-msg", "pre-commit", "pre-push")
EXECUTABLE_MODE = "100755"


class ContractError(ValueError):
    """Git is not wired to run this repository's committed hooks."""


def resolved_hooks_dir(root: Path, configured: str) -> Path:
    """Resolve core.hooksPath the way Git does: relative to the working tree.

    Symlinks are resolved on both sides before comparison. `git rev-parse
    --show-toplevel` and Python's `__file__` can name the same directory through
    different links — /tmp against /private/tmp on macOS is the common one — and
    an unresolved comparison would refuse a correctly installed clone.
    `strict=False` matters: the sparse-checkout case is precisely the one where
    the directory does not exist yet still has to be named in the refusal."""
    candidate = Path(configured).expanduser()
    if not candidate.is_absolute():
        candidate = root / candidate
    return candidate.resolve()


def same_path(left: Path, right: Path) -> bool:
    """Compare two paths case-insensitively where the platform demands it."""
    return os.path.normcase(str(left)) == os.path.normcase(str(right))


def validate_hooks(root: Path, configured: str | None, modes: dict[str, str]) -> Path:
    """Pure validator. `configured` is the observed core.hooksPath, `modes` the
    Git index mode of each required hook — both passed in so the self-test can
    drive every refusal without a real repository or a real Git config."""
    if configured is None or not configured.strip():
        raise ContractError(
            "core.hooksPath is not set, so Git runs no hook in this clone"
        )

    expected = (root / HOOKS_DIRNAME).resolve()
    actual = resolved_hooks_dir(root, configured.strip())
    if not same_path(actual, expected):
        raise ContractError(
            f"core.hooksPath resolves to {actual}, not the committed {expected}"
        )

    if not actual.is_dir():
        raise ContractError(
            f"core.hooksPath is {configured.strip()!r} and resolves correctly, but "
            f"{actual} is not present in this working tree — a sparse checkout or a "
            "revision predating the hooks leaves the setting right and the hooks gone"
        )

    for name in REQUIRED_HOOKS:
        hook = actual / name
        if not hook.is_file():
            raise ContractError(f"{HOOKS_DIRNAME}/{name} is missing from the working tree")

        mode = modes.get(name)
        if mode is None:
            raise ContractError(
                f"{HOOKS_DIRNAME}/{name} is not tracked in this repository's index"
            )
        # The index mode is the portable authority: os.access(X_OK) answers True
        # for every existing file on Windows, so a filesystem probe is vacuously
        # green exactly where the exec bit is most easily lost.
        if mode != EXECUTABLE_MODE:
            raise ContractError(
                f"{HOOKS_DIRNAME}/{name} is committed {mode}, not {EXECUTABLE_MODE}; "
                "Git skips a non-executable hook without a word"
            )
        if os.name == "posix" and not os.access(hook, os.X_OK):
            raise ContractError(
                f"{HOOKS_DIRNAME}/{name} has lost its exec bit on disk; "
                "Git skips a non-executable hook without a word"
            )

    return actual


def configured_hooks_path(root: Path) -> str | None:
    """Read core.hooksPath. Absent is a refusal, not a crash; only an operational
    failure of Git itself raises."""
    try:
        done = subprocess.run(
            ["git", "config", "--get", "core.hooksPath"],
            check=False,
            cwd=root,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=15,
        )
    except FileNotFoundError:
        raise RuntimeError("git is not installed: https://git-scm.com/downloads") from None
    except (OSError, subprocess.SubprocessError) as exc:
        raise RuntimeError(f"could not run git config: {exc}") from exc
    if done.returncode == 1:
        return None
    if done.returncode != 0:
        raise RuntimeError(
            f"git config --get core.hooksPath failed (exit {done.returncode})"
        )
    return done.stdout.strip()


def index_modes(root: Path) -> dict[str, str]:
    """The committed mode of each required hook, read from the Git index."""
    paths = [f"{HOOKS_DIRNAME}/{name}" for name in REQUIRED_HOOKS]
    try:
        done = subprocess.run(
            ["git", "ls-files", "-s", "--", *paths],
            check=False,
            cwd=root,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=15,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise RuntimeError(f"could not run git ls-files: {exc}") from exc
    if done.returncode != 0:
        raise RuntimeError(f"git ls-files failed (exit {done.returncode})")

    modes: dict[str, str] = {}
    for line in done.stdout.splitlines():
        # <mode> <oid> <stage>\t<path>
        meta, _, path = line.partition("\t")
        fields = meta.split()
        if len(fields) != 3 or not path:
            continue
        modes[Path(path).name] = fields[0]
    return modes


def write_fixture(root: Path) -> None:
    hooks = root / HOOKS_DIRNAME
    hooks.mkdir(parents=True)
    for name in REQUIRED_HOOKS:
        hook = hooks / name
        hook.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
        hook.chmod(0o755)


def self_test() -> int:
    good = dict.fromkeys(REQUIRED_HOOKS, EXECUTABLE_MODE)
    cases: list[tuple[str, Callable[[Path], tuple[str | None, dict[str, str]]]]] = []

    def case(
        label: str, mutate: Callable[[Path], tuple[str | None, dict[str, str]]]
    ) -> None:
        cases.append((label, mutate))

    case("an unset core.hooksPath is refused", lambda root: (None, dict(good)))
    case("an empty core.hooksPath is refused", lambda root: ("   ", dict(good)))
    case(
        "a core.hooksPath pointing elsewhere is refused",
        lambda root: (".git/hooks", dict(good)),
    )
    case(
        "core.hooksPath silenced with /dev/null is refused",
        lambda root: ("/dev/null", dict(good)),
    )

    case(
        "an absolute path left behind by a removed worktree is refused",
        lambda root: (str(root.parent / "gone-worktree" / HOOKS_DIRNAME), dict(good)),
    )

    def sparse_checkout(root: Path) -> tuple[str | None, dict[str, str]]:
        for name in REQUIRED_HOOKS:
            (root / HOOKS_DIRNAME / name).unlink()
        (root / HOOKS_DIRNAME).rmdir()
        return (HOOKS_DIRNAME, dict(good))

    case(
        "a correct setting whose directory is not checked out is refused",
        sparse_checkout,
    )

    def missing_hook(root: Path) -> tuple[str | None, dict[str, str]]:
        (root / HOOKS_DIRNAME / "pre-push").unlink()
        return (HOOKS_DIRNAME, dict(good))

    case("a missing hook file is refused", missing_hook)

    def untracked_hook(root: Path) -> tuple[str | None, dict[str, str]]:
        modes = dict(good)
        del modes["pre-commit"]
        return (HOOKS_DIRNAME, modes)

    case("a hook absent from the index is refused", untracked_hook)

    def non_executable(root: Path) -> tuple[str | None, dict[str, str]]:
        modes = dict(good)
        modes["commit-msg"] = "100644"
        return (HOOKS_DIRNAME, modes)

    case("a hook committed non-executable is refused", non_executable)

    def symlinked_hook(root: Path) -> tuple[str | None, dict[str, str]]:
        modes = dict(good)
        modes["pre-push"] = "120000"
        return (HOOKS_DIRNAME, modes)

    case("a hook committed as a symlink is refused", symlinked_hook)

    failures = 0
    with tempfile.TemporaryDirectory(prefix="hooks-installed-") as temporary:
        happy = Path(temporary) / "happy"
        write_fixture(happy)
        try:
            resolved = validate_hooks(happy, HOOKS_DIRNAME, dict(good))
            passed = resolved == (happy / HOOKS_DIRNAME).resolve()
        except ContractError:
            passed = False
        print(f"  {'ok  ' if passed else 'FAIL'}  an installed relative hooks path passes")
        failures += not passed

        absolute = Path(temporary) / "absolute"
        write_fixture(absolute)
        try:
            validate_hooks(absolute, str(absolute / HOOKS_DIRNAME), dict(good))
            passed = True
        except ContractError:
            passed = False
        print(f"  {'ok  ' if passed else 'FAIL'}  an installed absolute hooks path passes")
        failures += not passed

        for index, (label, mutate) in enumerate(cases):
            root = Path(temporary) / f"case-{index}"
            write_fixture(root)
            configured, modes = mutate(root)
            try:
                validate_hooks(root, configured, modes)
            except ContractError:
                passed = True
            else:
                passed = False
            print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
            failures += not passed

    total = len(cases) + 2
    if failures:
        print(f"\ncheck-hooks-installed self-test: {failures}/{total} case(s) FAILED")
        return 1
    print(f"\ncheck-hooks-installed self-test: {total} cases passed")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if sys.argv[1:]:
        print("usage: check-hooks-installed.py [--self-test]", file=sys.stderr)
        return 2

    try:
        configured = configured_hooks_path(ROOT)
        resolved = validate_hooks(ROOT, configured, index_modes(ROOT))
    except ContractError as exc:
        print(f"check-hooks-installed: REFUSED — {exc}", file=sys.stderr)
        print("  Install them:  just setup      (or just hooks)", file=sys.stderr)
        print(
            "  Local hooks stay bypassable with --no-verify; this only proves "
            "Git is wired to run them at all.",
            file=sys.stderr,
        )
        return 1
    except RuntimeError as exc:
        print(f"check-hooks-installed: ERROR — {exc}", file=sys.stderr)
        return 1

    print(f"git hooks: {resolved} — {', '.join(REQUIRED_HOOKS)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
