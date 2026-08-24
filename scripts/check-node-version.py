#!/usr/bin/env python3
"""The running Node is the one .nvmrc pins.

Rust's version is pinned once, in rust-toolchain.toml, and CI reads that exact
file rather than restating the number. Node had the opposite arrangement: the
major appeared in .nvmrc, in package.json's `engines`, in two justfile
assertions and in five hardcoded `node-version:` workflow steps — and no
workflow used `node-version-file`, so the one file whose whole purpose is to
hold that number was the one thing CI never consulted.

`.nvmrc` is now the single pin. Workflows read it with `node-version-file:`,
and this check reads it here, so a runner and a developer's machine cannot
disagree about which Node built the bundle.

The major must match: staying on one LTS line is the point of pinning one. The
patch may be newer, never older, because the pin sits at or above the floor the
lockfile requires — jsdom 30 needs ^24.15.0.

Deliberately Python rather than a `node -e` one-liner in the justfile. The two
assertions this replaces were split across a `[unix]` and a `[windows]` arm, and
the Windows arm avoided a template literal in favour of string concatenation
because PowerShell mangles backticks when passing arguments to a native
executable. One Python file is one implementation for both platforms, and it can
be negative-tested, which a shell one-liner in a recipe cannot.

Usage:  ./scripts/check-node-version.py
        ./scripts/check-node-version.py --self-test   # prove the check fires
Exit:   0 the running Node satisfies the pin · 1 it does not · 2 could not run
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NVMRC = ROOT / ".nvmrc"

VERSION = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)")


def parse(version: str) -> tuple[int, int, int]:
    matched = VERSION.match(version.strip())
    if matched is None:
        raise ValueError(f"not a Node version: {version.strip()!r}")
    return (int(matched[1]), int(matched[2]), int(matched[3]))


def verdict(pinned: str, running: str) -> str | None:
    """Return the refusal message, or None when the running Node is acceptable."""
    want = parse(pinned)
    got = parse(running)
    if got[0] != want[0]:
        return (
            f"Node {want[0]}.x is required (.nvmrc pins {pinned.strip()}); "
            f"found {running.strip()}"
        )
    if got < want:
        return (
            f"Node >= {pinned.strip()} is required (.nvmrc); "
            f"found {running.strip()} — older than the pin, and the lockfile's "
            "floor sits at or below it"
        )
    return None


def running_node() -> str:
    try:
        done = subprocess.run(
            ["node", "--version"], capture_output=True, text=True, timeout=15
        )
    except FileNotFoundError:
        raise RuntimeError("Node.js is not installed: https://nodejs.org/") from None
    except (OSError, subprocess.SubprocessError) as exc:
        raise RuntimeError(f"could not run node --version: {exc}") from exc
    if done.returncode != 0:
        raise RuntimeError(f"node --version failed (exit {done.returncode})")
    return done.stdout


def self_test() -> int:
    cases = (
        ("the exact pinned version passes", "24.19.0", "v24.19.0", False),
        ("a newer patch on the pinned major passes", "24.19.0", "v24.20.1", False),
        ("a newer minor on the pinned major passes", "24.19.0", "v24.22.0", False),
        ("an older patch is refused", "24.19.0", "v24.18.9", True),
        ("an older minor is refused", "24.19.0", "v24.15.0", True),
        # The failure this repository actually had: a machine three majors past
        # the pin, with every gate reporting the wrong number or nothing at all.
        ("a newer major is refused", "24.19.0", "v26.4.0", True),
        ("an older major is refused", "24.19.0", "v22.22.2", True),
        ("the bare form without a leading v is read", "24.19.0", "24.19.0", False),
    )

    failures = 0
    for label, pinned, running, want_refusal in cases:
        refused = verdict(pinned, running) is not None
        passed = refused == want_refusal
        print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
        failures += not passed

    for label, value in (
        ("an unparseable pin is an error", "lts/jod"),
        ("an unparseable running version is an error", "vNaN"),
    ):
        try:
            verdict(value, "v24.19.0") if value == "lts/jod" else verdict("24.19.0", value)
        except ValueError:
            print(f"  ok    {label}")
        else:
            print(f"  FAIL  {label}")
            failures += 1

    if failures:
        print(f"\ncheck-node-version self-test: {failures} case(s) FAILED")
        return 1
    print(f"\ncheck-node-version self-test: {len(cases) + 2} cases passed")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    try:
        pinned = NVMRC.read_text(encoding="utf-8")
        message = verdict(pinned, running_node())
    except (OSError, RuntimeError, ValueError) as exc:
        print(f"check-node-version: could not run: {exc}", file=sys.stderr)
        return 2

    if message is not None:
        print(f"check-node-version: REFUSED — {message}", file=sys.stderr)
        print(
            "\n  nvm and fnm read .nvmrc directly:  nvm use\n"
            f"  Homebrew installs the line:        brew install node@{parse(pinned)[0]}\n"
            "  Or download it:                    https://nodejs.org/dist/",
            file=sys.stderr,
        )
        return 1

    print(f"node {running_node().strip()} satisfies the .nvmrc pin ({pinned.strip()})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
