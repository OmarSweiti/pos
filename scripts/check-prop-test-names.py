#!/usr/bin/env python3
"""Property tests are named `prop_<invariant>`; example tests are not.

`ref/domain-api.md` is normative for pos-domain, and it names every property test
with a `prop_` prefix — thirty-one of them. The phase plan names twenty-one more.
And microstep 1.1.5 verifies the suite with

    cargo nextest run -p pos-domain money::prop_

which is a *filter*. Two tests had been written without the prefix, so that command
matched nothing and reported success by running zero tests — the failure mode this
repository builds gates against, arriving through a test name.

Nothing could have caught it. `cargo nextest` has no opinion on naming, clippy has
no opinion on naming, and a reviewer reading `split_preserves_total` sees a
perfectly good test. Only the *relationship* between the name and the reference is
wrong, so only a check that knows the convention can see it.

Both directions are checked, because the convention is a pair:

  inside  `proptest! { … }`   every `fn` must start with `prop_`
  outside `proptest! { … }`   no `#[test]` fn may start with `prop_`

A `prop_`-named example test is the same drift travelling the other way: it makes
the filter above match something that is not a property test.

Usage:  ./scripts/check-prop-test-names.py
        ./scripts/check-prop-test-names.py --self-test   # prove the checks fire
Exit:   0 clean · 1 a violation · 2 could not run at all
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"

PROPTEST_OPEN = re.compile(r"\bproptest!\s*\{")
FN = re.compile(r"^\s*(?:pub\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
TEST_ATTR = re.compile(r"^\s*#\[test\]")


def scan(text: str) -> tuple[list[tuple[int, str]], list[tuple[int, str]]]:
    """(unprefixed inside a proptest! block, prop_-prefixed example tests)."""
    inside: list[tuple[int, str]] = []
    outside: list[tuple[int, str]] = []

    depth = 0          # brace depth once inside a proptest! block, 0 = not in one
    saw_test_attr = False

    for number, line in enumerate(text.splitlines(), start=1):
        if depth == 0 and PROPTEST_OPEN.search(line):
            # The macro's own opening brace counts as depth 1.
            depth = 1 + line.count("{") - line.count("}") - 1
            depth = max(depth, 1)
            saw_test_attr = False
            continue

        if depth > 0:
            if found := FN.match(line):
                name = found.group(1)
                if not name.startswith("prop_"):
                    inside.append((number, name))
            depth += line.count("{") - line.count("}")
            if depth <= 0:
                depth = 0
            continue

        # Outside any proptest! block: an example test must not borrow the prefix.
        if TEST_ATTR.match(line):
            saw_test_attr = True
            continue
        if found := FN.match(line):
            if saw_test_attr and found.group(1).startswith("prop_"):
                outside.append((number, found.group(1)))
            saw_test_attr = False


    return inside, outside


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def check(roots: list[Path]) -> list[str]:
    problems: list[str] = []
    for root in roots:
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*.rs")):
            if "target" in path.parts:
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            inside, outside = scan(text)
            for number, name in inside:
                problems.append(
                    f"{rel(path)}:{number}  `{name}` is inside a proptest! block and "
                    f"needs the prefix: prop_{name}"
                )
            for number, name in outside:
                problems.append(
                    f"{rel(path)}:{number}  `{name}` is an example test wearing the "
                    "property-test prefix. Name it <subject>_<behaviour>."
                )
    return problems


def self_test() -> int:
    import tempfile

    cases: list[tuple[str, str, bool]] = [
        ("a prefixed property test passes", """
proptest! {
    #[test]
    fn prop_split_preserves_total(x in 0i64..9) { }
}
""", False),
        ("an unprefixed property test fails", """
proptest! {
    #[test]
    fn split_preserves_total(x in 0i64..9) { }
}
""", True),
        ("the second one in a block is caught too", """
proptest! {
    #[test]
    fn prop_ok(x in 0i64..9) { }
    #[test]
    fn add_sub_roundtrip(a in 0i64..9) { }
}
""", True),
        ("an example test needs no prefix", """
#[test]
fn jod_exponent_is_three() { }
""", False),
        ("an example test may not borrow the prefix", """
#[test]
fn prop_looks_like_a_property_test() { }
""", True),
        ("a plain fn after the block is not a test", """
proptest! {
    #[test]
    fn prop_ok(x in 0i64..9) { }
}

fn helper() -> i64 { 0 }
""", False),
        ("nested braces do not end the block early", """
proptest! {
    #[test]
    fn prop_ok(x in 0i64..9) {
        if x > 0 { let _ = x; }
    }
    #[test]
    fn missing_prefix(y in 0i64..9) { }
}
""", True),
    ]

    passed = failed = 0
    for label, body, want_problem in cases:
        with tempfile.TemporaryDirectory() as tmp:
            crate = Path(tmp) / "src"
            crate.mkdir(parents=True)
            (crate / "lib.rs").write_text(body)
            got_problem = bool(check([Path(tmp)]))
        if got_problem == want_problem:
            print(f"  ok    {label}")
            passed += 1
        else:
            print(f"  FAIL  {label} (wanted problem={want_problem}, got {got_problem})")
            failed += 1

    print(f"\n{passed} passed, {failed} failed")
    return 1 if failed else 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    if not CRATES.is_dir():
        print(f"cannot find {CRATES}", file=sys.stderr)
        return 2

    problems = check([CRATES])
    if problems:
        print(f"{len(problems)} property-test naming problem(s):")
        for message in problems:
            print(f"  FAIL  {message}")
        print(
            "\nref/domain-api.md is normative and names these with a prop_ prefix; "
            "microstep 1.1.5 verifies them with the filter `money::prop_`, which "
            "silently matches nothing when a name drifts."
        )
        return 1
    print("property tests are named prop_<invariant> (ref/domain-api.md)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
