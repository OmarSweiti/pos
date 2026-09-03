#!/usr/bin/env python3
"""Keep pos-domain deterministic: no clock or random-ID capability.

The architectural rule is stronger than "the code does not call randomness
today". A UUID feature can quietly add a runtime RNG to the pure crate's normal
dependency graph, making the next accidental call easy. This check audits both
the resolved normal dependency features and the explicit source call sites.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

from rust_lexer import RustLexError, RustToken, rust_tokens

ROOT = Path(__file__).resolve().parent.parent
DOMAIN_SRC = ROOT / "crates" / "pos-domain" / "src"

FORBIDDEN_TREE = (
    re.compile(r'^\s*uuid feature "(?:v[1-8]|rng|fast-rng|js)"$'),
    re.compile(r"^\s*(?:getrandom|rand|rand_core|rand_chacha|fastrand) v\S+"),
)

CATEGORY_NAMES = {
    "wall": {"SystemTime", "Utc", "Local"},
    "instant": {"Instant"},
    "offset": {"OffsetDateTime"},
    "epoch": {"UNIX_EPOCH"},
    "uuid": {"Uuid"},
    "random": {"getrandom", "rand", "fastrand"},
}


def dependency_violations(tree: str) -> list[str]:
    failures: list[str] = []
    for line in tree.splitlines():
        plain = re.sub(r"^[\s│├└─]+", "", line)
        if any(pattern.search(plain) for pattern in FORBIDDEN_TREE):
            failures.append(plain)
    return failures


def aliases_by_category(tokens: list[RustToken]) -> dict[str, set[str]]:
    """Resolve ordinary ``use ... as ...`` and ``type X = ...`` aliases.

    The policy cares about capabilities rather than the local spelling chosen
    for them. This intentionally errs on the conservative side when a type alias
    expression mentions one of the known clock or ID types.
    """
    aliases = {category: set(names) for category, names in CATEGORY_NAMES.items()}

    changed = True
    while changed:
        changed = False
        for index in range(len(tokens) - 2):
            original, keyword, alias = tokens[index : index + 3]
            if (
                original.kind != "ident"
                or keyword.kind != "ident"
                or keyword.value != "as"
                or alias.kind != "ident"
            ):
                continue
            for names in aliases.values():
                if original.value in names and alias.value not in names:
                    names.add(alias.value)
                    changed = True

        index = 0
        while index < len(tokens):
            if tokens[index].kind != "ident" or tokens[index].value != "type":
                index += 1
                continue
            if index + 2 >= len(tokens) or tokens[index + 1].kind != "ident":
                index += 1
                continue
            alias = tokens[index + 1].value
            cursor = index + 2
            while cursor < len(tokens) and tokens[cursor].value not in {"=", ";"}:
                cursor += 1
            if cursor >= len(tokens) or tokens[cursor].value != "=":
                index += 1
                continue
            end = cursor + 1
            while end < len(tokens) and tokens[end].value != ";":
                end += 1
            rhs_names = {
                token.value for token in tokens[cursor + 1 : end] if token.kind == "ident"
            }
            for names in aliases.values():
                if rhs_names & names and alias not in names:
                    names.add(alias)
                    changed = True
            index = end + 1
    return aliases


def source_violations(path: Path, source: str) -> list[str]:
    try:
        tokens = rust_tokens(source)
    except RustLexError as error:
        return [f"{path.relative_to(ROOT)}: Rust lexical analysis failed: {error}"]

    aliases = aliases_by_category(tokens)
    found: set[tuple[int, str]] = set()

    def record(token: RustToken, label: str) -> None:
        found.add((token.offset, label))

    def associated_method(index: int) -> str | None:
        """Return `method` from `Type::method` or `<Type>::method`."""
        if (
            index + 3 < len(tokens)
            and tokens[index + 1].value == ":"
            and tokens[index + 2].value == ":"
            and tokens[index + 3].kind == "ident"
        ):
            return tokens[index + 3].value
        if (
            index > 0
            and index + 4 < len(tokens)
            and tokens[index - 1].value == "<"
            and tokens[index + 1].value == ">"
            and tokens[index + 2].value == ":"
            and tokens[index + 3].value == ":"
            and tokens[index + 4].kind == "ident"
        ):
            return tokens[index + 4].value
        return None

    for index, token in enumerate(tokens):
        if token.kind != "ident":
            continue

        # Associated methods and method items are both capabilities. Catching
        # the item matters: `let clock = SystemTime::now; clock()` otherwise
        # moves acquisition away from the type spelling.
        if (method := associated_method(index)) is not None:
            if token.value in aliases["uuid"] and re.fullmatch(
                r"(?:new|now)_v[1-8]", method
            ):
                record(token, "UUID generation")
            if token.value in aliases["random"]:
                record(token, "direct randomness")
            if token.value in aliases["wall"] and method == "now":
                record(token, "wall clock")
            if token.value in aliases["instant"] and method in {"now", "elapsed"}:
                record(token, "clock acquisition")
            if token.value in aliases["offset"] and method in {"now_utc", "now_local"}:
                record(token, "wall clock")

        # UNIX_EPOCH.elapsed() and an Instant value's `.elapsed()` both acquire
        # the current clock without spelling `now`. The broad method check is
        # deliberate in this pure crate: elapsed time must be computed from two
        # caller-supplied timestamps instead.
        if (
            index > 0
            and index + 1 < len(tokens)
            and token.value == "elapsed"
            and tokens[index - 1].value == "."
            and tokens[index + 1].value == "("
        ):
            record(token, "clock acquisition via elapsed()")

    failures: list[str] = []
    relative = path.relative_to(ROOT)
    for offset, label in sorted(found):
        line = source.count("\n", 0, offset) + 1
        failures.append(f"{relative}:{line}: {label}")
    return failures


def self_test() -> int:
    cases = (
        ("serde-only UUID is safe", 'uuid feature "serde"\n  uuid v1.24.1', False),
        ("UUIDv7 capability is rejected", 'uuid feature "v7"\n  uuid v1.24.1', True),
        ("runtime getrandom is rejected", "getrandom v0.4.2", True),
    )
    failed = 0
    for label, tree, want_failure in cases:
        passed = bool(dependency_violations(tree)) == want_failure
        print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
        failed += not passed

    source_cases = (
        ("injected UUID values are safe", "Uuid::from_u128(value)", False),
        ("generated UUIDs are rejected", "Uuid::now_v7()", True),
        ("wall-clock reads are rejected", "SystemTime::now()", True),
        ("an aliased wall clock is rejected",
         "use std::time::SystemTime as Clock; Clock::now()", True),
        ("an aliased monotonic clock is rejected",
         "use std::time::Instant as Tick; Tick::now()", True),
        ("a qualified wall clock is rejected", "<SystemTime>::now()", True),
        ("a qualified chrono clock is rejected", "<Utc>::now()", True),
        ("a clock method item is rejected",
         "let clock = SystemTime::now; clock()", True),
        ("UNIX_EPOCH elapsed reads are rejected",
         "SystemTime::UNIX_EPOCH.elapsed()", True),
        ("instance elapsed reads are rejected", "started.elapsed()", True),
        ("an aliased UUID generator is rejected",
         "use uuid::Uuid as Id; Id::new_v4()", True),
        ("an aliased random module is rejected",
         "use rand as entropy; entropy::random()", True),
        ("a raw-spelled wall clock is rejected",
         "std::time::r#SystemTime::now()", True),
        ("a raw-spelled UUID generator is rejected",
         "uuid::r#Uuid::now_v7()", True),
        ("a type alias to a raw-spelled clock is rejected",
         "type Clock = std::time::r#SystemTime; Clock::now()", True),
        ("policy words in comments are ignored",
         "// SystemTime::now()\nUuid::from_u128(value)", False),
        ("policy words in raw strings are ignored",
         'let note = r#"SystemTime::now()"#;', False),
        ("ambiguous Rust source fails closed", "/* unterminated", True),
    )
    probe = ROOT / "crates" / "pos-domain" / "src" / "probe.rs"
    for label, source, want_failure in source_cases:
        passed = bool(source_violations(probe, source)) == want_failure
        print(f"  {'ok  ' if passed else 'FAIL'}  {label}")
        failed += not passed

    passed = len(cases) + len(source_cases) - failed
    print(f"\ndomain-purity self-test: {passed} passed, {failed} failed")
    return 1 if failed else 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    command = [
        "cargo",
        "tree",
        "--locked",
        "-p",
        "pos-domain",
        "-e",
        "normal,features",
    ]
    result = subprocess.run(
        command,
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if result.returncode != 0:
        print(result.stderr.rstrip(), file=sys.stderr)
        print("could not inspect pos-domain's dependency graph", file=sys.stderr)
        return 2

    failures = [
        f"runtime dependency/feature: {item}"
        for item in dependency_violations(result.stdout)
    ]
    for path in sorted(DOMAIN_SRC.rglob("*.rs")):
        failures.extend(source_violations(path, path.read_text(encoding="utf-8")))

    if failures:
        print("pos-domain purity violation(s):", file=sys.stderr)
        for failure in failures:
            print(f"  FAIL  {failure}", file=sys.stderr)
        print(
            "Time and IDs are inputs; pos-domain must not acquire a clock or RNG.",
            file=sys.stderr,
        )
        return 1

    print("pos-domain has no runtime RNG capability or direct clock/random calls")
    return 0


if __name__ == "__main__":
    sys.exit(main())
